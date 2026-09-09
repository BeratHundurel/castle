use anyhow::{Context as _, Result, bail};
use calendar::{CalendarDate, GenerationMode, RecurrenceRule, RecurrenceSeries};
use chrono::{Duration, NaiveDate};
use entity::{
    board, board::Entity as Board, card, card::Entity as Card, entry, entry::Entity as Entry,
    entry_checklist_item, entry_checklist_item::Entity as EntryChecklistItem, entry_label,
    entry_label::Entity as EntryLabel, entry_property_value,
    entry_property_value::Entity as EntryPropertyValue,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, Statement, TransactionSession, TransactionTrait,
};
use serde::{Deserialize, Serialize};

use crate::{Store, board::ListWorkflowRole, time::unix_timestamp_seconds};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CalendarEntryRecord {
    pub entry_id: i64,
    pub title: String,
    pub description: String,
    pub start_on: Option<String>,
    pub due_on: String,
    pub list_id: i64,
    pub list_title: String,
    pub board_id: i64,
    pub board_title: String,
    pub workflow_role: ListWorkflowRole,
    pub completed_at: Option<i64>,
    pub cancelled_at: Option<i64>,
    pub recurrence_series_id: Option<i64>,
    pub occurrence_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurringTaskRecord {
    pub id: i64,
    pub entry_id: i64,
    pub start_on: CalendarDate,
    pub rule: RecurrenceRule,
    pub next_on: CalendarDate,
    pub until_on: Option<CalendarDate>,
    pub occurrence_limit: Option<u32>,
    pub generation_mode: GenerationMode,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecurringTaskDraft {
    pub entry_id: i64,
    pub start_on: CalendarDate,
    pub rule: RecurrenceRule,
    pub until_on: Option<CalendarDate>,
    pub occurrence_limit: Option<u32>,
    pub generation_mode: GenerationMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct StoredRecurrence {
    start_on: CalendarDate,
    rule: RecurrenceRule,
    #[serde(default)]
    occurrence_limit: Option<u32>,
    #[serde(default)]
    generation_mode: GenerationMode,
}

pub async fn load_entries(
    db: &(impl ConnectionTrait + TransactionTrait),
    start_on: Option<&str>,
    end_on: Option<&str>,
    board_id: Option<i64>,
) -> Result<Vec<CalendarEntryRecord>> {
    let start_on = parse_date(start_on, "start_on")?;
    let end_on = parse_date(end_on, "end_on")?;
    if let (Some(start_on), Some(end_on)) = (start_on, end_on)
        && start_on > end_on
    {
        bail!("start_on must not be after end_on");
    }

    let mut entries_query = Entry::find()
        .filter(entry::Column::DeletedAt.is_null())
        .filter(entry::Column::Archived.eq(false))
        .filter(entry::Column::DueOn.is_not_null());
    if let Some(start_on) = start_on {
        entries_query = entries_query.filter(entry::Column::DueOn.gte(start_on.to_string()));
    }
    if let Some(end_on) = end_on {
        entries_query = entries_query.filter(entry::Column::DueOn.lte(end_on.to_string()));
    }
    let entries = entries_query
        .order_by_asc(entry::Column::DueOn)
        .order_by_asc(entry::Column::Position)
        .order_by_asc(entry::Column::Id)
        .all(db)
        .await?;

    let recurring = load_recurring_rows(db, board_id).await?;
    let recurring_entry_ids = recurring
        .iter()
        .filter(|recurring| recurring.enabled)
        .map(|recurring| recurring.entry_id)
        .collect::<Vec<_>>();
    let recurring_entries = if recurring_entry_ids.is_empty() {
        Vec::new()
    } else {
        Entry::find()
            .filter(entry::Column::Id.is_in(recurring_entry_ids))
            .filter(entry::Column::DeletedAt.is_null())
            .filter(entry::Column::Archived.eq(false))
            .all(db)
            .await?
    };

    let mut list_ids = entries
        .iter()
        .map(|entry| entry.card_id)
        .collect::<Vec<_>>();
    list_ids.extend(recurring_entries.iter().map(|entry| entry.card_id));
    list_ids.sort_unstable();
    list_ids.dedup();
    if list_ids.is_empty() {
        return Ok(Vec::new());
    }
    let lists = Card::find()
        .filter(card::Column::Id.is_in(list_ids))
        .filter(card::Column::DeletedAt.is_null())
        .all(db)
        .await?
        .into_iter()
        .filter(|list| board_id.is_none_or(|board_id| list.board_id == board_id))
        .map(|list| (list.id, list))
        .collect::<std::collections::HashMap<_, _>>();
    if lists.is_empty() {
        return Ok(Vec::new());
    }

    let board_ids = lists.values().map(|list| list.board_id).collect::<Vec<_>>();
    let boards = Board::find()
        .filter(board::Column::Id.is_in(board_ids))
        .filter(board::Column::DeletedAt.is_null())
        .all(db)
        .await?
        .into_iter()
        .map(|board| (board.id, board))
        .collect::<std::collections::HashMap<_, _>>();

    let mut records = entries
        .into_iter()
        .filter_map(|entry| {
            let list = lists.get(&entry.card_id)?;
            let board = boards.get(&list.board_id)?;
            let due_on = entry.due_on?;
            Some(CalendarEntryRecord {
                entry_id: entry.id,
                title: entry.title,
                description: entry.description,
                start_on: entry.start_on,
                due_on,
                list_id: list.id,
                list_title: list.title.clone(),
                board_id: board.id,
                board_title: board.title.clone(),
                workflow_role: ListWorkflowRole::from_storage(&list.workflow_role),
                completed_at: entry.completed_at,
                cancelled_at: entry.cancelled_at,
                recurrence_series_id: None,
                occurrence_key: None,
            })
        })
        .collect::<Vec<_>>();

    for recurring_task in recurring.iter().filter(|recurring| recurring.enabled) {
        let Some(template) = recurring_entries
            .iter()
            .find(|entry| entry.id == recurring_task.entry_id)
        else {
            continue;
        };
        let Some(list) = lists.get(&template.card_id) else {
            continue;
        };
        let Some(board) = boards.get(&list.board_id) else {
            continue;
        };
        let Some((projection_from, projection_through)) = recurrence_window(start_on, end_on)
        else {
            continue;
        };
        let mut series = RecurrenceSeries::new(
            recurring_task.id,
            template.id,
            board.id,
            list.id,
            recurring_task.start_on,
            recurring_task.rule.clone(),
        )?;
        series.end_on = recurring_task.until_on;
        series.occurrence_limit = recurring_task.occurrence_limit;
        series.generation_mode = recurring_task.generation_mode;
        for occurrence in series.occurrences_between(projection_from, projection_through, 100)? {
            let due_on = occurrence.occurrence_on.to_string();
            if let Some(record) = records.iter_mut().find(|record| {
                record.entry_id == template.id
                    && record.due_on == due_on
                    && record.recurrence_series_id.is_none()
            }) {
                record.recurrence_series_id = Some(occurrence.series_id);
                record.occurrence_key = Some(occurrence.occurrence_key);
                continue;
            }
            records.push(CalendarEntryRecord {
                entry_id: template.id,
                title: template.title.clone(),
                description: template.description.clone(),
                start_on: Some(due_on.clone()),
                due_on,
                list_id: list.id,
                list_title: list.title.clone(),
                board_id: board.id,
                board_title: board.title.clone(),
                workflow_role: ListWorkflowRole::from_storage(&list.workflow_role),
                completed_at: None,
                cancelled_at: None,
                recurrence_series_id: Some(occurrence.series_id),
                occurrence_key: Some(occurrence.occurrence_key),
            });
        }
    }

    records.sort_by(|left, right| {
        left.due_on
            .cmp(&right.due_on)
            .then_with(|| left.entry_id.cmp(&right.entry_id))
            .then_with(|| left.recurrence_series_id.cmp(&right.recurrence_series_id))
    });
    Ok(records)
}

async fn load_recurring_rows<C>(db: &C, board_id: Option<i64>) -> Result<Vec<RecurringTaskRecord>>
where
    C: ConnectionTrait + TransactionTrait,
{
    let (query, values) = match board_id {
        Some(board_id) => (
            "SELECT r.id, r.entry_id, r.rule_json, r.next_on, r.until_on, r.enabled FROM recurring_task r JOIN entry e ON e.id = r.entry_id JOIN card c ON c.id = e.card_id WHERE c.board_id = ? AND e.deleted_at IS NULL AND e.archived = 0 ORDER BY r.next_on, r.id",
            vec![board_id.into()],
        ),
        None => (
            "SELECT r.id, r.entry_id, r.rule_json, r.next_on, r.until_on, r.enabled FROM recurring_task r JOIN entry e ON e.id = r.entry_id JOIN card c ON c.id = e.card_id WHERE e.deleted_at IS NULL AND e.archived = 0 ORDER BY r.next_on, r.id",
            Vec::new(),
        ),
    };
    db.query_all_raw(Statement::from_sql_and_values(
        sea_orm::DbBackend::Sqlite,
        query,
        values,
    ))
    .await?
    .into_iter()
    .map(recurring_from_row)
    .collect()
}

fn recurrence_window(
    start_on: Option<NaiveDate>,
    end_on: Option<NaiveDate>,
) -> Option<(CalendarDate, CalendarDate)> {
    match (start_on, end_on) {
        (Some(start_on), Some(end_on)) => Some((start_on.into(), end_on.into())),
        (Some(start_on), None) => {
            let through = start_on.checked_add_signed(Duration::days(365))?;
            Some((start_on.into(), through.into()))
        }
        (None, Some(end_on)) => {
            let from = end_on.checked_sub_signed(Duration::days(365))?;
            Some((from.into(), end_on.into()))
        }
        (None, None) => None,
    }
}

pub async fn create_recurring_task<C>(
    store: &Store<C>,
    draft: RecurringTaskDraft,
) -> Result<RecurringTaskRecord>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    draft
        .rule
        .validate()
        .map_err(|error| anyhow::anyhow!(error))?;
    if draft.occurrence_limit == Some(0) {
        bail!("occurrence_limit must be positive");
    }
    let entry = Entry::find_by_id(draft.entry_id)
        .filter(entry::Column::DeletedAt.is_null())
        .filter(entry::Column::Archived.eq(false))
        .one(store)
        .await?
        .with_context(|| format!("active board entry {} was not found", draft.entry_id))?;
    let list = Card::find_by_id(entry.card_id)
        .filter(card::Column::DeletedAt.is_null())
        .one(store)
        .await?
        .with_context(|| format!("active list {} was not found", entry.card_id))?;
    let board = Board::find_by_id(list.board_id)
        .filter(board::Column::DeletedAt.is_null())
        .one(store)
        .await?
        .with_context(|| format!("active board {} was not found", list.board_id))?;
    let mut series = RecurrenceSeries::new(
        0,
        entry.id,
        board.id,
        list.id,
        draft.start_on,
        draft.rule.clone(),
    )?;
    series.end_on = draft.until_on;
    series.occurrence_limit = draft.occurrence_limit;
    series.generation_mode = draft.generation_mode;
    let first = series
        .first_occurrence()?
        .with_context(|| "recurrence has no initial occurrence")?;
    let first_on = first.occurrence_on.to_string();
    entry::ActiveModel {
        id: Set(entry.id),
        start_on: Set(Some(first_on.clone())),
        due_on: Set(Some(first_on.clone())),
        reminder_notified_for: Set(None),
        ..Default::default()
    }
    .update(store)
    .await?;
    let stored = serde_json::to_string(&StoredRecurrence {
        start_on: draft.start_on,
        rule: draft.rule,
        occurrence_limit: draft.occurrence_limit,
        generation_mode: draft.generation_mode,
    })?;
    let now = unix_timestamp_seconds();
    store
        .execute_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "INSERT INTO recurring_task (entry_id, rule_json, next_on, until_on, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, 1, ?, ?) ON CONFLICT(entry_id) DO UPDATE SET rule_json = excluded.rule_json, next_on = excluded.next_on, until_on = excluded.until_on, enabled = 1, updated_at = excluded.updated_at",
            [
                entry.id.into(),
                stored.into(),
                first_on.into(),
                draft.until_on.map(|date| date.to_string()).into(),
                now.into(),
                now.into(),
            ],
        ))
        .await?;
    get_recurring_task(store, entry.id).await
}

pub async fn get_recurring_task<C>(store: &Store<C>, entry_id: i64) -> Result<RecurringTaskRecord>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    let row = store
        .query_one_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "SELECT id, entry_id, rule_json, next_on, until_on, enabled FROM recurring_task WHERE entry_id = ?",
            [entry_id.into()],
        ))
        .await?
        .with_context(|| format!("recurring task for entry {entry_id} was not found"))?;
    recurring_from_row(row)
}

async fn get_recurring_task_by_id<C>(
    store: &Store<C>,
    recurring_id: i64,
) -> Result<RecurringTaskRecord>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    let row = store
        .query_one_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "SELECT id, entry_id, rule_json, next_on, until_on, enabled FROM recurring_task WHERE id = ?",
            [recurring_id.into()],
        ))
        .await?
        .with_context(|| format!("recurring task {recurring_id} was not found"))?;
    recurring_from_row(row)
}

pub async fn list_recurring_tasks<C>(
    store: &Store<C>,
    board_id: i64,
) -> Result<Vec<RecurringTaskRecord>>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    let rows = store
        .query_all_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "SELECT r.id, r.entry_id, r.rule_json, r.next_on, r.until_on, r.enabled FROM recurring_task r JOIN entry e ON e.id = r.entry_id JOIN card c ON c.id = e.card_id WHERE c.board_id = ? AND e.deleted_at IS NULL AND e.archived = 0 ORDER BY r.next_on, r.id",
            [board_id.into()],
        ))
        .await?;
    rows.into_iter().map(recurring_from_row).collect()
}

pub async fn delete_recurring_task<C>(store: &Store<C>, entry_id: i64) -> Result<()>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    store
        .execute_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "DELETE FROM recurring_task WHERE entry_id = ?",
            [entry_id.into()],
        ))
        .await?;
    Ok(())
}

pub async fn create_next_recurring_instance<C>(store: &Store<C>, entry_id: i64) -> Result<()>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    let Ok(recurring) = get_recurring_task(store, entry_id).await else {
        return Ok(());
    };
    if !recurring.enabled {
        return Ok(());
    }
    let entry = Entry::find_by_id(entry_id)
        .filter(entry::Column::DeletedAt.is_null())
        .one(store)
        .await?
        .with_context(|| format!("entry {entry_id} was not found while recurring"))?;
    let list = Card::find_by_id(entry.card_id)
        .filter(card::Column::DeletedAt.is_null())
        .one(store)
        .await?
        .with_context(|| format!("list {} was not found while recurring", entry.card_id))?;
    let mut series = RecurrenceSeries::new(
        recurring.id,
        entry.id,
        list.board_id,
        list.id,
        recurring.start_on,
        recurring.rule.clone(),
    )?;
    series.end_on = recurring.until_on;
    series.occurrence_limit = recurring.occurrence_limit;
    series.generation_mode = recurring.generation_mode;
    let Some(next) = series.next_occurrence_after(recurring.next_on)? else {
        store
            .execute_raw(Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                "UPDATE recurring_task SET enabled = 0, updated_at = ? WHERE id = ?",
                [unix_timestamp_seconds().into(), recurring.id.into()],
            ))
            .await?;
        return Ok(());
    };
    let position = Entry::find()
        .filter(entry::Column::CardId.eq(list.id))
        .filter(entry::Column::DeletedAt.is_null())
        .filter(entry::Column::Archived.eq(false))
        .count(store)
        .await? as i32;
    let labels = EntryLabel::find()
        .filter(entry_label::Column::EntryId.eq(entry.id))
        .all(store)
        .await?;
    let checklist_items = EntryChecklistItem::find()
        .filter(entry_checklist_item::Column::EntryId.eq(entry.id))
        .all(store)
        .await?;
    let property_values = EntryPropertyValue::find()
        .filter(entry_property_value::Column::EntryId.eq(entry.id))
        .all(store)
        .await?;
    let transaction = store.begin().await?;
    let next_entry = entry::ActiveModel {
        title: Set(entry.title),
        description: Set(entry.description.clone()),
        card_id: Set(list.id),
        position: Set(position),
        start_on: Set(Some(next.occurrence_on.to_string())),
        due_on: Set(Some(next.occurrence_on.to_string())),
        completed_at: Set(None),
        cancelled_at: Set(None),
        archived: Set(false),
        reminder_enabled: Set(entry.reminder_enabled),
        reminder_notified_for: Set(None),
        ..Default::default()
    }
    .insert(&transaction)
    .await?;
    for label in labels {
        entry_label::ActiveModel {
            entry_id: Set(next_entry.id),
            board_label_id: Set(label.board_label_id),
            ..Default::default()
        }
        .insert(&transaction)
        .await?;
    }
    for item in checklist_items {
        entry_checklist_item::ActiveModel {
            entry_id: Set(next_entry.id),
            title: Set(item.title),
            checked: Set(item.checked),
            position: Set(item.position),
            ..Default::default()
        }
        .insert(&transaction)
        .await?;
    }
    for value in property_values {
        entry_property_value::ActiveModel {
            entry_id: Set(next_entry.id),
            property_id: Set(value.property_id),
            text_value: Set(value.text_value),
            number_value: Set(value.number_value),
            boolean_value: Set(value.boolean_value),
            date_value: Set(value.date_value),
            option_id: Set(value.option_id),
        }
        .insert(&transaction)
        .await?;
    }
    crate::workspace::links::index_entry_workspace_links_in_connection(
        &transaction,
        next_entry.id,
        &next_entry.description,
        unix_timestamp_seconds(),
    )
    .await?;
    transaction.commit().await?;
    store
        .execute_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "UPDATE recurring_task SET entry_id = ?, next_on = ?, updated_at = ? WHERE id = ?",
            [
                next_entry.id.into(),
                next.occurrence_on.to_string().into(),
                unix_timestamp_seconds().into(),
                recurring.id.into(),
            ],
        ))
        .await?;
    Ok(())
}

pub async fn materialize_scheduled_recurring_tasks<C>(
    store: &Store<C>,
    through: CalendarDate,
) -> Result<usize>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    let rows = store
        .query_all_raw(Statement::from_string(
            sea_orm::DbBackend::Sqlite,
            "SELECT r.id, r.entry_id, r.rule_json, r.next_on, r.until_on, r.enabled FROM recurring_task r JOIN entry e ON e.id = r.entry_id WHERE r.enabled = 1 AND e.deleted_at IS NULL AND e.archived = 0",
        ))
        .await?;
    let mut materialized = 0;
    for recurring in rows.into_iter().map(recurring_from_row) {
        let mut recurring = recurring?;
        for _ in 0..100 {
            if recurring.generation_mode != GenerationMode::OnSchedule
                || !recurring.enabled
                || recurring.next_on >= through
            {
                break;
            }
            create_next_recurring_instance(store, recurring.entry_id).await?;
            materialized += 1;
            recurring = get_recurring_task_by_id(store, recurring.id).await?;
        }
    }
    Ok(materialized)
}

fn recurring_from_row(row: sea_orm::QueryResult) -> Result<RecurringTaskRecord> {
    let stored: StoredRecurrence = serde_json::from_str(&row.try_get::<String>("", "rule_json")?)?;
    Ok(RecurringTaskRecord {
        id: row.try_get("", "id")?,
        entry_id: row.try_get("", "entry_id")?,
        start_on: stored.start_on,
        rule: stored.rule,
        next_on: CalendarDate::parse(&row.try_get::<String>("", "next_on")?)?,
        until_on: row
            .try_get::<Option<String>>("", "until_on")?
            .map(|value| CalendarDate::parse(&value))
            .transpose()?,
        occurrence_limit: stored.occurrence_limit,
        generation_mode: stored.generation_mode,
        enabled: row.try_get("", "enabled")?,
    })
}

fn parse_date(value: Option<&str>, field: &str) -> Result<Option<NaiveDate>> {
    value
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .with_context(|| format!("{field} must use YYYY-MM-DD"))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::commands::{
        BoardCardDraft, BoardListDraft, create_board_card, create_board_list,
    };
    use migration::{Migrator, MigratorTrait};
    use sea_orm::Database;
    use workflow::ListWorkflowRole;

    #[tokio::test]
    async fn loads_due_entries_in_an_iso_date_range() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = crate::workspace::create_board(&db, None, "Calendar".to_string()).await?;
        let list = create_board_list(
            &db,
            BoardListDraft {
                title: "Planning".to_string(),
                board_id: board.id,
                position: 0,
                workflow_role: ListWorkflowRole::Neutral,
                cards: Vec::new(),
            },
        )
        .await?;
        create_board_card(
            &db,
            BoardCardDraft {
                title: "In range".to_string(),
                description: "".to_string(),
                list_id: list.id,
                position: 0,
                due_on: Some("2026-09-12".to_string()),
                label_ids: Vec::new(),
                checklist_items: Vec::new(),
            },
            1,
        )
        .await?;
        create_board_card(
            &db,
            BoardCardDraft {
                title: "Outside range".to_string(),
                description: "".to_string(),
                list_id: list.id,
                position: 1,
                due_on: Some("2026-10-12".to_string()),
                label_ids: Vec::new(),
                checklist_items: Vec::new(),
            },
            1,
        )
        .await?;

        let records = load_entries(&db, Some("2026-09-01"), Some("2026-09-30"), None).await?;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].title, "In range");
        assert_eq!(records[0].workflow_role, ListWorkflowRole::Neutral);
        assert!(
            load_entries(&db, Some("2026-10-01"), Some("2026-09-30"), None)
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn projects_recurring_occurrences_and_upserts_the_series() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = crate::workspace::create_board(&db, None, "Calendar".to_string()).await?;
        let list = create_board_list(
            &db,
            BoardListDraft {
                title: "Planning".to_string(),
                board_id: board.id,
                position: 0,
                workflow_role: ListWorkflowRole::Neutral,
                cards: Vec::new(),
            },
        )
        .await?;
        let entry = create_board_card(
            &db,
            BoardCardDraft {
                title: "Recurring review".to_string(),
                description: "Review the queue".to_string(),
                list_id: list.id,
                position: 0,
                due_on: None,
                label_ids: Vec::new(),
                checklist_items: Vec::new(),
            },
            1,
        )
        .await?;
        let store = Store::from(db);
        let first = CalendarDate::parse("2026-09-10")?;
        let recurring = create_recurring_task(
            &store,
            RecurringTaskDraft {
                entry_id: i64::from(entry.id),
                start_on: first,
                rule: RecurrenceRule::daily(1).map_err(|error| anyhow::anyhow!(error))?,
                until_on: Some(CalendarDate::parse("2026-09-12")?),
                occurrence_limit: None,
                generation_mode: GenerationMode::OnCompletion,
            },
        )
        .await?;
        let updated = create_recurring_task(
            &store,
            RecurringTaskDraft {
                entry_id: i64::from(entry.id),
                start_on: first,
                rule: RecurrenceRule::daily(2).map_err(|error| anyhow::anyhow!(error))?,
                until_on: Some(CalendarDate::parse("2026-09-12")?),
                occurrence_limit: None,
                generation_mode: GenerationMode::OnCompletion,
            },
        )
        .await?;
        assert_eq!(updated.id, recurring.id);
        assert_eq!(updated.rule.interval, 2);

        let records = load_entries(
            &store,
            Some("2026-09-10"),
            Some("2026-09-12"),
            Some(i64::from(board.id)),
        )
        .await?;
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].due_on, "2026-09-10");
        assert_eq!(records[0].recurrence_series_id, Some(recurring.id));
        assert_eq!(records[1].due_on, "2026-09-12");
        assert_eq!(records[1].occurrence_key.as_deref(), Some("2026-09-12"));
        Ok(())
    }

    #[tokio::test]
    async fn completion_recurrence_creates_one_next_entry_and_advances_the_series() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = crate::workspace::create_board(&db, None, "Calendar".to_string()).await?;
        let list = create_board_list(
            &db,
            BoardListDraft {
                title: "Habits".to_string(),
                board_id: board.id,
                position: 0,
                workflow_role: ListWorkflowRole::Neutral,
                cards: Vec::new(),
            },
        )
        .await?;
        let entry = create_board_card(
            &db,
            BoardCardDraft {
                title: "Daily review".to_string(),
                description: "Review the queue".to_string(),
                list_id: list.id,
                position: 0,
                due_on: None,
                label_ids: Vec::new(),
                checklist_items: Vec::new(),
            },
            1,
        )
        .await?;
        let store = Store::from(db);
        create_recurring_task(
            &store,
            RecurringTaskDraft {
                entry_id: i64::from(entry.id),
                start_on: CalendarDate::parse("2026-09-10")?,
                rule: RecurrenceRule::daily(1).map_err(|error| anyhow::anyhow!(error))?,
                until_on: Some(CalendarDate::parse("2026-09-12")?),
                occurrence_limit: None,
                generation_mode: GenerationMode::OnCompletion,
            },
        )
        .await?;

        create_next_recurring_instance(&store, i64::from(entry.id)).await?;
        let recurring = list_recurring_tasks(&store, i64::from(board.id))
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("recurring task was not found after generation"))?;
        assert_ne!(recurring.entry_id, i64::from(entry.id));
        assert_eq!(recurring.next_on, CalendarDate::parse("2026-09-11")?);
        assert_eq!(recurring.start_on, CalendarDate::parse("2026-09-10")?);
        let generated = Entry::find_by_id(recurring.entry_id)
            .one(&store)
            .await?
            .ok_or_else(|| anyhow::anyhow!("generated recurring entry was not found"))?;
        assert_eq!(generated.title, "Daily review");
        assert_eq!(generated.due_on.as_deref(), Some("2026-09-11"));

        create_next_recurring_instance(&store, i64::from(entry.id)).await?;
        assert_eq!(Entry::find().count(&store).await?, 2);
        Ok(())
    }
}
