use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result, bail};
use calendar::CalendarDate;
use entity::{
    board_label, board_label::Entity as BoardLabel, board_property,
    board_property::Entity as BoardProperty, card, card::Entity as Card, entry,
    entry::Entity as Entry, entry_checklist_item,
    entry_checklist_item::Entity as EntryChecklistItem, entry_label,
    entry_label::Entity as EntryLabel,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, Statement, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use workflow::{
    CompletionState, EventOrigin, MovePosition, WorkflowAction, WorkflowCondition, WorkflowContext,
    WorkflowDefinition, WorkflowEvent, WorkflowEventKind, WorkflowNodeKind, WorkflowPropertyValue,
};

use crate::{Store, board::ListWorkflowRole, time::unix_timestamp_seconds};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WorkflowRecord {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub enabled: bool,
    pub definition: WorkflowDefinition,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowDraft {
    pub id: Option<i64>,
    pub board_id: i64,
    pub name: String,
    pub enabled: bool,
    pub definition: WorkflowDefinition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    Running,
    Succeeded,
    Failed,
    Skipped,
}

impl WorkflowRunStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRunRecord {
    pub id: i64,
    pub workflow_id: i64,
    pub board_id: i64,
    pub entry_id: Option<i64>,
    pub trigger_kind: String,
    pub status: WorkflowRunStatus,
    pub actions: Vec<WorkflowAction>,
    pub error: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WorkflowExecutionReport {
    pub runs: Vec<WorkflowRunOutcome>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorkflowRunOutcome {
    pub workflow_id: i64,
    pub workflow_name: String,
    pub status: WorkflowRunStatus,
    pub actions: Vec<WorkflowAction>,
    pub error: Option<String>,
}

pub async fn capture_move_event(
    store: &Store,
    entry_id: i64,
    target_list_id: i64,
    origin: EventOrigin,
) -> Result<Option<(WorkflowEvent, WorkflowContext)>> {
    let entry = Entry::find_by_id(entry_id)
        .filter(entry::Column::DeletedAt.is_null())
        .filter(entry::Column::Archived.eq(false))
        .one(store)
        .await?
        .with_context(|| format!("active board entry {entry_id} was not found"))?;
    let source = Card::find_by_id(entry.card_id)
        .filter(card::Column::DeletedAt.is_null())
        .one(store)
        .await?
        .with_context(|| format!("active source list {} was not found", entry.card_id))?;
    let target = Card::find_by_id(target_list_id)
        .filter(card::Column::DeletedAt.is_null())
        .one(store)
        .await?
        .with_context(|| format!("active destination list {target_list_id} was not found"))?;
    if source.board_id != target.board_id {
        bail!("an entry can only move within its current board");
    }
    if source.id == target.id {
        return Ok(None);
    }
    let source_role = ListWorkflowRole::from_storage(&source.workflow_role);
    let target_role = ListWorkflowRole::from_storage(&target.workflow_role);
    let occurred_at = chrono::Utc::now().to_rfc3339();
    let event = WorkflowEvent {
        event_id: format!("move-{entry_id}-{}", unix_timestamp_seconds()),
        board_id: source.board_id,
        entry_id,
        occurred_at,
        origin,
        kind: WorkflowEventKind::CardMoved {
            from_list_id: Some(source.id),
            from_list_role: Some(source_role),
            to_list_id: target.id,
            to_list_role: target_role,
        },
    };
    let mut context = entry_context(store, &entry, &target).await?;
    context.list_id = target.id;
    context.list_role = target_role;
    Ok(Some((event, context)))
}

pub async fn run_persisted_move_event(
    store: &Store,
    entry_id: i64,
    source_list_id: i64,
    target_list_id: i64,
    origin: EventOrigin,
) -> Result<WorkflowExecutionReport> {
    if source_list_id == target_list_id {
        return Ok(WorkflowExecutionReport::default());
    }
    let entry = Entry::find_by_id(entry_id)
        .filter(entry::Column::DeletedAt.is_null())
        .filter(entry::Column::Archived.eq(false))
        .one(store)
        .await?
        .with_context(|| format!("active board entry {entry_id} was not found"))?;
    let source = Card::find_by_id(source_list_id)
        .filter(card::Column::DeletedAt.is_null())
        .one(store)
        .await?
        .with_context(|| format!("active source list {source_list_id} was not found"))?;
    let target = Card::find_by_id(target_list_id)
        .filter(card::Column::DeletedAt.is_null())
        .one(store)
        .await?
        .with_context(|| format!("active destination list {target_list_id} was not found"))?;
    if entry.card_id != target.id {
        bail!(
            "entry {entry_id} is in list {}, expected persisted destination list {target_list_id}",
            entry.card_id
        );
    }
    if source.board_id != target.board_id {
        bail!("an entry can only move within its current board");
    }
    let source_role = ListWorkflowRole::from_storage(&source.workflow_role);
    let target_role = ListWorkflowRole::from_storage(&target.workflow_role);
    let event = WorkflowEvent {
        event_id: format!("move-{entry_id}-{}", unix_timestamp_seconds()),
        board_id: target.board_id,
        entry_id,
        occurred_at: chrono::Utc::now().to_rfc3339(),
        origin,
        kind: WorkflowEventKind::CardMoved {
            from_list_id: Some(source.id),
            from_list_role: Some(source_role),
            to_list_id: target.id,
            to_list_role: target_role,
        },
    };
    let mut context = entry_context(store, &entry, &target).await?;
    context.list_id = target.id;
    context.list_role = target_role;
    run_event(store, event, context).await
}

pub async fn run_entry_event(
    store: &Store,
    entry_id: i64,
    origin: EventOrigin,
    kind: WorkflowEventKind,
) -> Result<WorkflowExecutionReport> {
    let (entry, list) = load_entry_and_list(store, entry_id).await?;
    let context = entry_context(store, &entry, &list).await?;
    let event = WorkflowEvent {
        event_id: format!("entry-{entry_id}-{}", unix_timestamp_seconds()),
        board_id: list.board_id,
        entry_id,
        occurred_at: chrono::Utc::now().to_rfc3339(),
        origin,
        kind,
    };
    run_event(store, event, context).await
}

pub async fn run_manual_workflow(
    store: &Store,
    board_id: i64,
    entry_id: i64,
    origin: EventOrigin,
) -> Result<WorkflowExecutionReport> {
    let (entry, list) = load_entry_and_list(store, entry_id).await?;
    if list.board_id != board_id {
        bail!("entry {entry_id} does not belong to board {board_id}");
    }
    let context = entry_context(store, &entry, &list).await?;
    let event = WorkflowEvent {
        event_id: format!("manual-{entry_id}-{}", unix_timestamp_seconds()),
        board_id,
        entry_id,
        occurred_at: chrono::Utc::now().to_rfc3339(),
        origin,
        kind: WorkflowEventKind::Manual,
    };
    run_event(store, event, context).await
}

pub async fn run_scheduled_workflows(
    store: &Store,
    board_id: Option<i64>,
    schedule_key: impl Into<String>,
    origin: EventOrigin,
) -> Result<WorkflowExecutionReport> {
    let rows = match board_id {
        Some(board_id) => {
            store
                .query_all_raw(Statement::from_sql_and_values(
                    sea_orm::DbBackend::Sqlite,
                    "SELECT e.id FROM entry e JOIN card c ON c.id = e.card_id WHERE c.board_id = ? AND e.deleted_at IS NULL AND e.archived = 0 AND c.deleted_at IS NULL ORDER BY e.id",
                    [board_id.into()],
                ))
                .await?
        }
        None => {
            store
                .query_all_raw(Statement::from_string(
                    sea_orm::DbBackend::Sqlite,
                    "SELECT e.id FROM entry e JOIN card c ON c.id = e.card_id JOIN board b ON b.id = c.board_id WHERE e.deleted_at IS NULL AND e.archived = 0 AND c.deleted_at IS NULL AND b.deleted_at IS NULL ORDER BY e.id",
                ))
                .await?
        }
    };
    let schedule_key = schedule_key.into();
    let mut report = WorkflowExecutionReport::default();
    for row in rows {
        let entry_id = row.try_get::<i64>("", "id")?;
        let Ok((entry, list)) = load_entry_and_list(store, entry_id).await else {
            continue;
        };
        let event = WorkflowEvent {
            event_id: format!(
                "scheduled-{entry_id}-{}-{schedule_key}",
                unix_timestamp_seconds()
            ),
            board_id: list.board_id,
            entry_id,
            occurred_at: chrono::Utc::now().to_rfc3339(),
            origin,
            kind: WorkflowEventKind::Scheduled {
                schedule_key: schedule_key.clone(),
            },
        };
        let context = entry_context(store, &entry, &list).await?;
        report
            .runs
            .extend(run_event(store, event, context).await?.runs);
    }
    Ok(report)
}

pub async fn run_created_event(
    store: &Store,
    entry_id: i64,
    origin: EventOrigin,
) -> Result<WorkflowExecutionReport> {
    let (entry, list) = load_entry_and_list(store, entry_id).await?;
    let context = entry_context(store, &entry, &list).await?;
    let event = WorkflowEvent {
        event_id: format!("created-{entry_id}-{}", unix_timestamp_seconds()),
        board_id: list.board_id,
        entry_id,
        occurred_at: chrono::Utc::now().to_rfc3339(),
        origin,
        kind: WorkflowEventKind::CardCreated {
            list_id: list.id,
            list_role: ListWorkflowRole::from_storage(&list.workflow_role),
        },
    };
    run_event(store, event, context).await
}

pub async fn run_checklist_changed(
    store: &Store,
    entry_id: i64,
    origin: EventOrigin,
) -> Result<WorkflowExecutionReport> {
    let (entry, list) = load_entry_and_list(store, entry_id).await?;
    let checked_count = EntryChecklistItem::find()
        .filter(entry_checklist_item::Column::EntryId.eq(entry_id))
        .filter(entry_checklist_item::Column::Checked.eq(true))
        .count(store)
        .await?;
    let total_count = EntryChecklistItem::find()
        .filter(entry_checklist_item::Column::EntryId.eq(entry_id))
        .count(store)
        .await?;
    let mut context = entry_context(store, &entry, &list).await?;
    context.checklist_checked =
        u32::try_from(checked_count).context("checklist checked count exceeded supported range")?;
    context.checklist_total =
        u32::try_from(total_count).context("checklist count exceeded supported range")?;
    let event = WorkflowEvent {
        event_id: format!("checklist-{entry_id}-{}", unix_timestamp_seconds()),
        board_id: list.board_id,
        entry_id,
        occurred_at: chrono::Utc::now().to_rfc3339(),
        origin,
        kind: WorkflowEventKind::ChecklistChanged {
            checked_count: context.checklist_checked,
            total_count: context.checklist_total,
        },
    };
    run_event(store, event, context).await
}

pub async fn run_label_changed(
    store: &Store,
    entry_id: i64,
    origin: EventOrigin,
    label: String,
    added: bool,
) -> Result<WorkflowExecutionReport> {
    let (entry, list) = load_entry_and_list(store, entry_id).await?;
    let context = entry_context(store, &entry, &list).await?;
    let event = WorkflowEvent {
        event_id: format!("label-{entry_id}-{}", unix_timestamp_seconds()),
        board_id: list.board_id,
        entry_id,
        occurred_at: chrono::Utc::now().to_rfc3339(),
        origin,
        kind: WorkflowEventKind::LabelChanged { label, added },
    };
    run_event(store, event, context).await
}

pub async fn run_property_changed(
    store: &Store,
    entry_id: i64,
    origin: EventOrigin,
    key: String,
) -> Result<WorkflowExecutionReport> {
    let (entry, list) = load_entry_and_list(store, entry_id).await?;
    let context = entry_context(store, &entry, &list).await?;
    let event = WorkflowEvent {
        event_id: format!("property-{entry_id}-{}", unix_timestamp_seconds()),
        board_id: list.board_id,
        entry_id,
        occurred_at: chrono::Utc::now().to_rfc3339(),
        origin,
        kind: WorkflowEventKind::PropertyChanged { key },
    };
    run_event(store, event, context).await
}

pub async fn run_due_date_changed(
    store: &Store,
    entry_id: i64,
    origin: EventOrigin,
) -> Result<WorkflowExecutionReport> {
    let (entry, list) = load_entry_and_list(store, entry_id).await?;
    let context = entry_context(store, &entry, &list).await?;
    let event = WorkflowEvent {
        event_id: format!("due-date-{entry_id}-{}", unix_timestamp_seconds()),
        board_id: list.board_id,
        entry_id,
        occurred_at: chrono::Utc::now().to_rfc3339(),
        origin,
        kind: WorkflowEventKind::DueDateChanged {
            status: context.due_date_status,
        },
    };
    run_event(store, event, context).await
}

async fn load_entry_and_list(store: &Store, entry_id: i64) -> Result<(entry::Model, card::Model)> {
    let entry = Entry::find_by_id(entry_id)
        .filter(entry::Column::DeletedAt.is_null())
        .filter(entry::Column::Archived.eq(false))
        .one(store)
        .await?
        .with_context(|| format!("active entry {entry_id} was not found for workflow event"))?;
    let list = Card::find_by_id(entry.card_id)
        .filter(card::Column::DeletedAt.is_null())
        .one(store)
        .await?
        .with_context(|| format!("list {} was not found for workflow event", entry.card_id))?;
    Ok((entry, list))
}

async fn entry_context(
    store: &Store,
    entry: &entry::Model,
    list: &card::Model,
) -> Result<WorkflowContext> {
    let checklist_items = EntryChecklistItem::find()
        .filter(entry_checklist_item::Column::EntryId.eq(entry.id))
        .all(store)
        .await?;
    let label_ids = EntryLabel::find()
        .filter(entry_label::Column::EntryId.eq(entry.id))
        .all(store)
        .await?
        .into_iter()
        .map(|label| label.board_label_id)
        .collect::<Vec<_>>();
    let labels = if label_ids.is_empty() {
        Vec::new()
    } else {
        BoardLabel::find()
            .filter(board_label::Column::Id.is_in(label_ids))
            .all(store)
            .await?
            .into_iter()
            .map(|label| label.name)
            .collect()
    };
    let board_properties =
        crate::board::properties::load_board_properties(store, list.board_id).await?;
    let property_names = board_properties
        .definitions
        .into_iter()
        .map(|property| (property.id, property.name))
        .collect::<BTreeMap<_, _>>();
    let mut properties = BTreeMap::new();
    for value in board_properties
        .values
        .into_iter()
        .filter(|value| value.entry_id == entry.id)
    {
        let Some(name) = property_names.get(&value.property_id) else {
            continue;
        };
        let Some(value) = workflow_property_value(value.value) else {
            continue;
        };
        properties.insert(name.clone(), value);
    }
    let checked_count = checklist_items.iter().filter(|item| item.checked).count();
    Ok(WorkflowContext {
        board_id: list.board_id,
        entry_id: entry.id,
        list_id: list.id,
        list_role: ListWorkflowRole::from_storage(&list.workflow_role),
        labels,
        checklist_checked: u32::try_from(checked_count)
            .context("checklist checked count exceeded supported range")?,
        checklist_total: u32::try_from(checklist_items.len())
            .context("checklist count exceeded supported range")?,
        completion_state: if entry.cancelled_at.is_some() {
            CompletionState::Cancelled
        } else if entry.completed_at.is_some() {
            CompletionState::Completed
        } else if entry.archived {
            CompletionState::Archived
        } else {
            CompletionState::Open
        },
        due_date_status: due_date_status(entry.due_on.as_deref()),
        properties,
    })
}

fn workflow_property_value(
    value: crate::board::properties::PropertyValue,
) -> Option<WorkflowPropertyValue> {
    match value {
        crate::board::properties::PropertyValue::Text(value)
        | crate::board::properties::PropertyValue::Url(value) => {
            Some(WorkflowPropertyValue::Text(value))
        }
        crate::board::properties::PropertyValue::Number(value)
            if value.is_finite()
                && value.fract() == 0.
                && value >= i64::MIN as f64
                && value <= i64::MAX as f64 =>
        {
            Some(WorkflowPropertyValue::Number(value as i64))
        }
        crate::board::properties::PropertyValue::Number(_) => None,
        crate::board::properties::PropertyValue::Checkbox(value) => {
            Some(WorkflowPropertyValue::Boolean(value))
        }
        crate::board::properties::PropertyValue::Date(value) => {
            Some(WorkflowPropertyValue::Date(value))
        }
        crate::board::properties::PropertyValue::Select(value) => {
            Some(WorkflowPropertyValue::Number(value))
        }
    }
}

pub async fn list_workflows<C>(store: &Store<C>, board_id: i64) -> Result<Vec<WorkflowRecord>>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    ensure_active_board(store, board_id).await?;
    let rows = store
        .query_all_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "SELECT id, board_id, name, enabled, definition_json, created_at, updated_at FROM workflow WHERE board_id = ? ORDER BY id",
            [board_id.into()],
        ))
        .await?;
    rows.into_iter().map(workflow_from_row).collect()
}

pub async fn get_workflow<C>(store: &Store<C>, workflow_id: i64) -> Result<WorkflowRecord>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    let row = store
        .query_one_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "SELECT id, board_id, name, enabled, definition_json, created_at, updated_at FROM workflow WHERE id = ?",
            [workflow_id.into()],
        ))
        .await?
        .with_context(|| format!("workflow {workflow_id} was not found"))?;
    workflow_from_row(row)
}

pub async fn upsert_workflow<C>(store: &Store<C>, draft: WorkflowDraft) -> Result<WorkflowRecord>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    let name = draft.name.trim();
    if name.is_empty() {
        bail!("workflow name must not be empty");
    }
    ensure_active_board(store, draft.board_id).await?;
    let mut definition = draft.definition;
    definition.name = name.to_string();
    definition.enabled = draft.enabled;
    workflow::validate(&definition)
        .map_err(|error| anyhow::anyhow!("invalid workflow: {error}"))?;
    validate_workflow_dates(&definition)?;
    validate_board_references(store, draft.board_id, &definition).await?;
    let definition_json = serde_json::to_string(&definition)?;
    let now = unix_timestamp_seconds();
    let id = match draft.id {
        Some(id) => {
            let result = store
                .execute_raw(Statement::from_sql_and_values(
                    sea_orm::DbBackend::Sqlite,
                    "UPDATE workflow SET name = ?, enabled = ?, definition_json = ?, updated_at = ? WHERE id = ? AND board_id = ?",
                    [
                        name.to_string().into(),
                        draft.enabled.into(),
                        definition_json.into(),
                        now.into(),
                        id.into(),
                        draft.board_id.into(),
                    ],
                ))
                .await?;
            if result.rows_affected() != 1 {
                bail!("workflow {id} was not found on board {}", draft.board_id);
            }
            id
        }
        None => {
            store
                .execute_raw(Statement::from_sql_and_values(
                    sea_orm::DbBackend::Sqlite,
                    "INSERT INTO workflow (board_id, name, enabled, definition_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
                    [
                        draft.board_id.into(),
                        name.to_string().into(),
                        draft.enabled.into(),
                        definition_json.into(),
                        now.into(),
                        now.into(),
                    ],
                ))
                .await?;
            store
                .query_one_raw(Statement::from_string(
                    sea_orm::DbBackend::Sqlite,
                    "SELECT last_insert_rowid() AS id",
                ))
                .await?
                .context("workflow insert did not return an ID")?
                .try_get("", "id")?
        }
    };
    get_workflow(store, id).await
}

async fn validate_board_references<C>(
    store: &Store<C>,
    board_id: i64,
    definition: &WorkflowDefinition,
) -> Result<()>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    let mut references = WorkflowReferences::default();
    for node in &definition.nodes {
        collect_workflow_references(&node.kind, &mut references);
    }

    for list_id in references.list_ids {
        let exists = Card::find_by_id(list_id)
            .filter(card::Column::BoardId.eq(board_id))
            .filter(card::Column::DeletedAt.is_null())
            .one(store)
            .await?
            .is_some();
        if !exists {
            bail!("workflow references list {list_id} outside board {board_id}");
        }
    }

    for label in references.label_names {
        let exists = BoardLabel::find()
            .filter(board_label::Column::BoardId.eq(board_id))
            .filter(board_label::Column::Name.eq(label.as_str()))
            .one(store)
            .await?
            .is_some();
        if !exists {
            bail!("workflow references label {label:?} outside board {board_id}");
        }
    }

    for key in references.property_keys {
        let exists = BoardProperty::find()
            .filter(board_property::Column::BoardId.eq(board_id))
            .filter(board_property::Column::Name.eq(key.as_str()))
            .filter(board_property::Column::DeletedAt.is_null())
            .one(store)
            .await?
            .is_some();
        if !exists {
            bail!("workflow references property {key:?} outside board {board_id}");
        }
    }
    Ok(())
}

#[derive(Default)]
struct WorkflowReferences {
    list_ids: BTreeSet<i64>,
    label_names: BTreeSet<String>,
    property_keys: BTreeSet<String>,
}

fn collect_workflow_references(kind: &WorkflowNodeKind, references: &mut WorkflowReferences) {
    match kind {
        WorkflowNodeKind::Trigger { trigger } => match trigger {
            workflow::WorkflowTrigger::CardMovedToList { list_id }
            | workflow::WorkflowTrigger::CardMovedFromList { list_id } => {
                references.list_ids.insert(*list_id);
            }
            workflow::WorkflowTrigger::LabelAdded { label }
            | workflow::WorkflowTrigger::LabelRemoved { label } => {
                references.label_names.insert(label.clone());
            }
            workflow::WorkflowTrigger::PropertyChanged { key } => {
                references.property_keys.insert(key.clone());
            }
            _ => {}
        },
        WorkflowNodeKind::Condition { condition } => {
            collect_condition_references(condition, references);
        }
        WorkflowNodeKind::Branch { cases } => {
            for case in cases {
                collect_condition_references(&case.condition, references);
            }
        }
        WorkflowNodeKind::Action { action } => match action {
            WorkflowAction::MoveToList { list_id, .. } => {
                references.list_ids.insert(*list_id);
            }
            WorkflowAction::AddLabel { label } | WorkflowAction::RemoveLabel { label } => {
                references.label_names.insert(label.clone());
            }
            WorkflowAction::SetProperty { key, .. } | WorkflowAction::ClearProperty { key } => {
                references.property_keys.insert(key.clone());
            }
            _ => {}
        },
        WorkflowNodeKind::End { .. } => {}
    }
}

fn collect_condition_references(
    condition: &WorkflowCondition,
    references: &mut WorkflowReferences,
) {
    match condition {
        WorkflowCondition::All { conditions } | WorkflowCondition::Any { conditions } => {
            for condition in conditions {
                collect_condition_references(condition, references);
            }
        }
        WorkflowCondition::Not { condition } => collect_condition_references(condition, references),
        WorkflowCondition::ListIs { list_id } | WorkflowCondition::PreviousListIs { list_id } => {
            references.list_ids.insert(*list_id);
        }
        WorkflowCondition::LabelContains { label } => {
            references.label_names.insert(label.clone());
        }
        WorkflowCondition::PropertyEquals { key, .. } => {
            references.property_keys.insert(key.clone());
        }
        WorkflowCondition::Always
        | WorkflowCondition::ListRoleIs { .. }
        | WorkflowCondition::PreviousListRoleIs { .. }
        | WorkflowCondition::CompletionIs { .. }
        | WorkflowCondition::DueDateIs { .. }
        | WorkflowCondition::ChecklistIs { .. }
        | WorkflowCondition::EventOriginIs { .. }
        | WorkflowCondition::EventIs { .. } => {}
    }
}

fn validate_workflow_dates(definition: &WorkflowDefinition) -> Result<()> {
    for node in &definition.nodes {
        let WorkflowNodeKind::Action { action } = &node.kind else {
            continue;
        };
        let date = match action {
            WorkflowAction::SetDueDate { due_on } => due_on.as_deref(),
            WorkflowAction::SetStartDate { start_on } => start_on.as_deref(),
            _ => None,
        };
        if let Some(date) = date {
            CalendarDate::parse(date)
                .map_err(|error| anyhow::anyhow!("invalid workflow date {date:?}: {error}"))?;
        }
    }
    Ok(())
}

pub async fn delete_workflow<C>(store: &Store<C>, board_id: i64, workflow_id: i64) -> Result<()>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    let result = store
        .execute_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "DELETE FROM workflow WHERE id = ? AND board_id = ?",
            [workflow_id.into(), board_id.into()],
        ))
        .await?;
    if result.rows_affected() != 1 {
        bail!("workflow {workflow_id} was not found on board {board_id}");
    }
    Ok(())
}

pub async fn list_workflow_runs<C>(
    store: &Store<C>,
    board_id: i64,
    limit: u64,
) -> Result<Vec<WorkflowRunRecord>>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    ensure_active_board(store, board_id).await?;
    let limit = limit.clamp(1, 100);
    let rows = store
        .query_all_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "SELECT id, workflow_id, board_id, entry_id, trigger_kind, status, actions_json, error, started_at, finished_at FROM workflow_run WHERE board_id = ? ORDER BY started_at DESC, id DESC LIMIT ?",
            [board_id.into(), (limit as i64).into()],
        ))
        .await?;
    rows.into_iter().map(workflow_run_from_row).collect()
}

pub async fn run_event(
    store: &Store,
    event: WorkflowEvent,
    context: WorkflowContext,
) -> Result<WorkflowExecutionReport> {
    let workflows = list_workflows(store, event.board_id).await?;
    let mut report = WorkflowExecutionReport::default();
    for workflow in workflows.into_iter().filter(|workflow| workflow.enabled) {
        let actions = workflow::evaluate(&workflow.definition, &event, &context);
        if actions.is_empty() {
            record_run(
                store,
                WorkflowRunDraft {
                    workflow_id: workflow.id,
                    board_id: event.board_id,
                    entry_id: Some(event.entry_id),
                    trigger_kind: event.kind.name().as_str(),
                    status: WorkflowRunStatus::Skipped,
                    actions: &actions,
                    error: None,
                    started_at: unix_timestamp_seconds(),
                    finished_at: Some(unix_timestamp_seconds()),
                },
            )
            .await?;
            report.runs.push(WorkflowRunOutcome {
                workflow_id: workflow.id,
                workflow_name: workflow.name,
                status: WorkflowRunStatus::Skipped,
                actions,
                error: None,
            });
            continue;
        }

        let started_at = unix_timestamp_seconds();
        let mut error = None;
        for action in &actions {
            if let Err(action_error) = apply_action(store, &event, &context, action).await {
                error = Some(action_error.to_string());
                break;
            }
        }
        let status = if error.is_some() {
            WorkflowRunStatus::Failed
        } else {
            WorkflowRunStatus::Succeeded
        };
        record_run(
            store,
            WorkflowRunDraft {
                workflow_id: workflow.id,
                board_id: event.board_id,
                entry_id: Some(event.entry_id),
                trigger_kind: event.kind.name().as_str(),
                status,
                actions: &actions,
                error: error.as_deref(),
                started_at,
                finished_at: Some(unix_timestamp_seconds()),
            },
        )
        .await?;
        report.runs.push(WorkflowRunOutcome {
            workflow_id: workflow.id,
            workflow_name: workflow.name,
            status,
            actions,
            error,
        });
    }
    Ok(report)
}

async fn apply_action(
    store: &Store,
    event: &WorkflowEvent,
    context: &WorkflowContext,
    action: &WorkflowAction,
) -> Result<()> {
    match action {
        WorkflowAction::MoveToList { list_id, position } => {
            move_entry(store, event.entry_id, *list_id, *position).await
        }
        WorkflowAction::MarkComplete => {
            store
                .execute_raw(Statement::from_sql_and_values(
                    sea_orm::DbBackend::Sqlite,
                    "UPDATE entry SET completed_at = ?, cancelled_at = NULL WHERE id = ? AND deleted_at IS NULL",
                    [unix_timestamp_seconds().into(), event.entry_id.into()],
                ))
                .await?;
            if let Ok(recurring) = crate::calendar::get_recurring_task(store, event.entry_id).await
                && recurring.generation_mode == calendar::GenerationMode::OnCompletion
            {
                crate::calendar::create_next_recurring_instance(store, event.entry_id).await?;
            }
            Ok(())
        }
        WorkflowAction::MarkCancelled => {
            store
                .execute_raw(Statement::from_sql_and_values(
                    sea_orm::DbBackend::Sqlite,
                    "UPDATE entry SET completed_at = NULL, cancelled_at = ? WHERE id = ? AND deleted_at IS NULL",
                    [unix_timestamp_seconds().into(), event.entry_id.into()],
                ))
                .await?;
            Ok(())
        }
        WorkflowAction::Reopen => {
            store
                .execute_raw(Statement::from_sql_and_values(
                    sea_orm::DbBackend::Sqlite,
                    "UPDATE entry SET completed_at = NULL, cancelled_at = NULL WHERE id = ? AND deleted_at IS NULL",
                    [event.entry_id.into()],
                ))
                .await?;
            Ok(())
        }
        WorkflowAction::SetDueDate { due_on } => {
            store
                .execute_raw(Statement::from_sql_and_values(
                    sea_orm::DbBackend::Sqlite,
                    "UPDATE entry SET due_on = ?, reminder_notified_for = NULL WHERE id = ? AND deleted_at IS NULL",
                    [due_on.clone().into(), event.entry_id.into()],
                ))
                .await?;
            Ok(())
        }
        WorkflowAction::SetStartDate { start_on } => {
            store
                .execute_raw(Statement::from_sql_and_values(
                    sea_orm::DbBackend::Sqlite,
                    "UPDATE entry SET start_on = ? WHERE id = ? AND deleted_at IS NULL",
                    [start_on.clone().into(), event.entry_id.into()],
                ))
                .await?;
            Ok(())
        }
        WorkflowAction::Archive => {
            store
                .execute_raw(Statement::from_sql_and_values(
                    sea_orm::DbBackend::Sqlite,
                    "UPDATE entry SET archived = 1 WHERE id = ? AND deleted_at IS NULL",
                    [event.entry_id.into()],
                ))
                .await?;
            crate::workspace::search::remove_entry_from_index(
                store,
                u32::try_from(event.entry_id).context("entry ID is out of range")?,
            )
            .await?;
            Ok(())
        }
        WorkflowAction::Trash => {
            crate::workspace::trash::move_to_trash(
                store,
                crate::workspace::trash::MoveToTrash {
                    kind: crate::workspace::trash::TrashItemKind::Entry,
                    id: u32::try_from(event.entry_id).context("entry ID is out of range")?,
                },
                unix_timestamp_seconds(),
            )
            .await
        }
        WorkflowAction::CreateNextRecurringInstance => {
            crate::calendar::create_next_recurring_instance(store, event.entry_id).await
        }
        WorkflowAction::AddLabel { label } | WorkflowAction::RemoveLabel { label } => {
            let board_label = BoardLabel::find()
                .filter(board_label::Column::BoardId.eq(event.board_id))
                .filter(board_label::Column::Name.eq(label))
                .one(store)
                .await?
                .with_context(|| {
                    format!(
                        "workflow label {label:?} was not found on board {}",
                        event.board_id
                    )
                })?;
            let assigned = matches!(action, WorkflowAction::AddLabel { .. });
            crate::board::commands::set_label_assignment(
                store,
                u32::try_from(event.entry_id).context("entry ID is out of range")?,
                u32::try_from(board_label.id).context("label ID is out of range")?,
                assigned,
            )
            .await
        }
        WorkflowAction::SetProperty { key, value } => {
            let property = BoardProperty::find()
                .filter(board_property::Column::BoardId.eq(event.board_id))
                .filter(board_property::Column::Name.eq(key))
                .filter(board_property::Column::DeletedAt.is_null())
                .one(store)
                .await?
                .with_context(|| {
                    format!(
                        "workflow property {key:?} was not found on board {}",
                        event.board_id
                    )
                })?;
            let kind = crate::board::properties::PropertyKind::parse(&property.kind)?;
            let value = storage_property_value(kind, value)?;
            crate::board::properties::set_entry_property(store, event.entry_id, property.id, value)
                .await
                .map(|_| ())
        }
        WorkflowAction::ClearProperty { key } => {
            let property = BoardProperty::find()
                .filter(board_property::Column::BoardId.eq(event.board_id))
                .filter(board_property::Column::Name.eq(key))
                .filter(board_property::Column::DeletedAt.is_null())
                .one(store)
                .await?
                .with_context(|| {
                    format!(
                        "workflow property {key:?} was not found on board {}",
                        event.board_id
                    )
                })?;
            crate::board::properties::clear_entry_property(store, event.entry_id, property.id).await
        }
        WorkflowAction::Notify { .. } | WorkflowAction::AddHistory { .. } => {
            let _ = context;
            Ok(())
        }
    }
}

fn storage_property_value(
    kind: crate::board::properties::PropertyKind,
    value: &WorkflowPropertyValue,
) -> Result<crate::board::properties::PropertyValue> {
    match (kind, value) {
        (crate::board::properties::PropertyKind::Text, WorkflowPropertyValue::Text(value)) => {
            Ok(crate::board::properties::PropertyValue::Text(value.clone()))
        }
        (crate::board::properties::PropertyKind::Url, WorkflowPropertyValue::Text(value)) => {
            Ok(crate::board::properties::PropertyValue::Url(value.clone()))
        }
        (crate::board::properties::PropertyKind::Number, WorkflowPropertyValue::Number(value)) => {
            Ok(crate::board::properties::PropertyValue::Number(
                *value as f64,
            ))
        }
        (
            crate::board::properties::PropertyKind::Checkbox,
            WorkflowPropertyValue::Boolean(value),
        ) => Ok(crate::board::properties::PropertyValue::Checkbox(*value)),
        (crate::board::properties::PropertyKind::Date, WorkflowPropertyValue::Date(value)) => {
            Ok(crate::board::properties::PropertyValue::Date(value.clone()))
        }
        (crate::board::properties::PropertyKind::Select, WorkflowPropertyValue::Number(value)) => {
            Ok(crate::board::properties::PropertyValue::Select(*value))
        }
        _ => bail!("workflow value does not match {} property", kind.as_str()),
    }
}

async fn move_entry(
    store: &Store,
    entry_id: i64,
    list_id: i64,
    position: MovePosition,
) -> Result<()> {
    let entry = Entry::find_by_id(entry_id)
        .filter(entry::Column::DeletedAt.is_null())
        .filter(entry::Column::Archived.eq(false))
        .one(store)
        .await?
        .with_context(|| format!("active board entry {entry_id} was not found"))?;
    let target = Card::find_by_id(list_id)
        .filter(card::Column::DeletedAt.is_null())
        .one(store)
        .await?
        .with_context(|| format!("active destination list {list_id} was not found"))?;
    let source = Card::find_by_id(entry.card_id)
        .filter(card::Column::DeletedAt.is_null())
        .one(store)
        .await?
        .with_context(|| format!("active source list {} was not found", entry.card_id))?;
    if source.board_id != target.board_id {
        bail!("workflow cannot move an entry between boards");
    }
    if entry.card_id == list_id {
        return Ok(());
    }

    let transaction = store.begin().await?;
    Entry::update_many()
        .col_expr(
            entry::Column::Position,
            sea_orm::sea_query::Expr::col(entry::Column::Position).sub(1),
        )
        .filter(entry::Column::CardId.eq(entry.card_id))
        .filter(entry::Column::Position.gt(entry.position))
        .filter(entry::Column::Archived.eq(false))
        .exec(&transaction)
        .await?;
    let count = Entry::find()
        .filter(entry::Column::CardId.eq(list_id))
        .filter(entry::Column::DeletedAt.is_null())
        .filter(entry::Column::Archived.eq(false))
        .count(&transaction)
        .await? as i32;
    let destination_position = match position {
        MovePosition::Top => 0,
        MovePosition::Bottom => count,
        MovePosition::At(value) => value.clamp(0, count),
    };
    if destination_position < count {
        Entry::update_many()
            .col_expr(
                entry::Column::Position,
                sea_orm::sea_query::Expr::col(entry::Column::Position).add(1),
            )
            .filter(entry::Column::CardId.eq(list_id))
            .filter(entry::Column::Position.gte(destination_position))
            .filter(entry::Column::Archived.eq(false))
            .exec(&transaction)
            .await?;
    }
    entry::ActiveModel {
        id: Set(entry.id),
        card_id: Set(list_id),
        position: Set(destination_position),
        ..Default::default()
    }
    .update(&transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn ensure_active_board<C>(store: &Store<C>, board_id: i64) -> Result<()>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    let exists = store
        .query_one_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "SELECT id FROM board WHERE id = ? AND deleted_at IS NULL",
            [board_id.into()],
        ))
        .await?
        .is_some();
    if !exists {
        bail!("active board {board_id} was not found");
    }
    Ok(())
}

fn workflow_from_row(row: sea_orm::QueryResult) -> Result<WorkflowRecord> {
    let mut definition: WorkflowDefinition =
        serde_json::from_str(&row.try_get::<String>("", "definition_json")?)?;
    let name = row.try_get::<String>("", "name")?;
    let enabled = row.try_get::<bool>("", "enabled")?;
    definition.name = name.clone();
    definition.enabled = enabled;
    Ok(WorkflowRecord {
        id: row.try_get("", "id")?,
        board_id: row.try_get("", "board_id")?,
        name,
        enabled,
        definition,
        created_at: row.try_get("", "created_at")?,
        updated_at: row.try_get("", "updated_at")?,
    })
}

fn workflow_run_from_row(row: sea_orm::QueryResult) -> Result<WorkflowRunRecord> {
    let status = match row.try_get::<String>("", "status")?.as_str() {
        "running" => WorkflowRunStatus::Running,
        "succeeded" => WorkflowRunStatus::Succeeded,
        "failed" => WorkflowRunStatus::Failed,
        "skipped" => WorkflowRunStatus::Skipped,
        value => bail!("unknown workflow run status {value}"),
    };
    Ok(WorkflowRunRecord {
        id: row.try_get("", "id")?,
        workflow_id: row.try_get("", "workflow_id")?,
        board_id: row.try_get("", "board_id")?,
        entry_id: row.try_get("", "entry_id")?,
        trigger_kind: row.try_get("", "trigger_kind")?,
        status,
        actions: serde_json::from_str(&row.try_get::<String>("", "actions_json")?)?,
        error: row.try_get("", "error")?,
        started_at: row.try_get("", "started_at")?,
        finished_at: row.try_get("", "finished_at")?,
    })
}

fn due_date_status(value: Option<&str>) -> workflow::DueDateStatus {
    let Some(value) = value else {
        return workflow::DueDateStatus::NoDate;
    };
    let Ok(due_on) = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") else {
        return workflow::DueDateStatus::NoDate;
    };
    let today = chrono::Local::now().date_naive();
    if due_on < today {
        workflow::DueDateStatus::Overdue
    } else if due_on == today {
        workflow::DueDateStatus::Today
    } else {
        workflow::DueDateStatus::Upcoming
    }
}

struct WorkflowRunDraft<'a> {
    workflow_id: i64,
    board_id: i64,
    entry_id: Option<i64>,
    trigger_kind: &'a str,
    status: WorkflowRunStatus,
    actions: &'a [WorkflowAction],
    error: Option<&'a str>,
    started_at: i64,
    finished_at: Option<i64>,
}

async fn record_run(store: &Store, draft: WorkflowRunDraft<'_>) -> Result<()> {
    let actions_json = serde_json::to_string(draft.actions)?;
    store
        .execute_raw(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "INSERT INTO workflow_run (workflow_id, board_id, entry_id, trigger_kind, status, actions_json, error, started_at, finished_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                draft.workflow_id.into(),
                draft.board_id.into(),
                draft.entry_id.into(),
                draft.trigger_kind.to_string().into(),
                draft.status.as_str().into(),
                actions_json.into(),
                draft.error.map(str::to_string).into(),
                draft.started_at.into(),
                draft.finished_at.into(),
            ],
        ))
        .await?;
    Ok(())
}

pub fn context_for_move(
    board_id: i64,
    entry_id: i64,
    list_id: i64,
    list_role: ListWorkflowRole,
    _previous_list_id: Option<i64>,
    _previous_list_role: Option<ListWorkflowRole>,
) -> WorkflowContext {
    WorkflowContext {
        board_id,
        entry_id,
        list_id,
        list_role,
        completion_state: CompletionState::Open,
        ..WorkflowContext::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::Database;
    use workflow::{WorkflowEdge, WorkflowNode, WorkflowNodeKind, WorkflowTrigger};

    fn definition() -> WorkflowDefinition {
        WorkflowDefinition {
            enabled: true,
            nodes: vec![
                WorkflowNode {
                    id: "trigger".to_string(),
                    kind: WorkflowNodeKind::Trigger {
                        trigger: WorkflowTrigger::CardMovedToRole {
                            role: workflow::ListWorkflowRole::Done,
                        },
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "archive".to_string(),
                    kind: WorkflowNodeKind::Action {
                        action: WorkflowAction::Archive,
                    },
                    position: Default::default(),
                },
            ],
            edges: vec![WorkflowEdge {
                id: "edge".to_string(),
                from: "trigger".to_string(),
                to: "archive".to_string(),
                kind: workflow::WorkflowEdgeKind::Default,
            }],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn workflow_round_trip_evaluates_and_records_archive_action() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = crate::workspace::create_board(&db, None, "Workflow".to_string()).await?;
        let list = crate::board::commands::create_board_list(
            &db,
            crate::board::commands::BoardListDraft {
                title: "Done".to_string(),
                board_id: board.id,
                position: 0,
                workflow_role: ListWorkflowRole::Done,
                cards: Vec::new(),
            },
        )
        .await?;
        let entry = crate::board::commands::create_board_card(
            &db,
            crate::board::commands::BoardCardDraft {
                title: "Ship".to_string(),
                description: String::new(),
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
        let saved = upsert_workflow(
            &store,
            WorkflowDraft {
                id: None,
                board_id: i64::from(board.id),
                name: "Archive done".to_string(),
                enabled: true,
                definition: definition(),
            },
        )
        .await?;
        assert_eq!(list_workflows(&store, i64::from(board.id)).await?.len(), 1);

        let event = WorkflowEvent {
            event_id: "move-1".to_string(),
            board_id: i64::from(board.id),
            entry_id: i64::from(entry.id),
            occurred_at: "2026-09-09T00:00:00Z".to_string(),
            origin: EventOrigin::User,
            kind: WorkflowEventKind::CardMoved {
                from_list_id: None,
                from_list_role: None,
                to_list_id: i64::from(list.id),
                to_list_role: workflow::ListWorkflowRole::Done,
            },
        };
        let report = run_event(
            &store,
            event,
            WorkflowContext {
                board_id: i64::from(board.id),
                entry_id: i64::from(entry.id),
                list_id: i64::from(list.id),
                list_role: workflow::ListWorkflowRole::Done,
                ..Default::default()
            },
        )
        .await?;
        assert_eq!(report.runs[0].workflow_id, saved.id);
        assert_eq!(report.runs[0].status, WorkflowRunStatus::Succeeded);
        assert!(
            entity::entry::Entity::find_by_id(i64::from(entry.id))
                .one(&store)
                .await?
                .is_some_and(|entry| entry.archived)
        );
        assert_eq!(
            list_workflow_runs(&store, i64::from(board.id), 10)
                .await?
                .len(),
            1
        );
        Ok(())
    }

    #[tokio::test]
    async fn workflow_rejects_list_references_from_another_board() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let first_board = crate::workspace::create_board(&db, None, "First".to_string()).await?;
        let second_board = crate::workspace::create_board(&db, None, "Second".to_string()).await?;
        let foreign_list = crate::board::commands::create_board_list(
            &db,
            crate::board::commands::BoardListDraft {
                title: "Foreign".to_string(),
                board_id: second_board.id,
                position: 0,
                workflow_role: ListWorkflowRole::Neutral,
                cards: Vec::new(),
            },
        )
        .await?;
        let mut invalid = definition();
        invalid.nodes[0].kind = WorkflowNodeKind::Trigger {
            trigger: WorkflowTrigger::CardMovedToList {
                list_id: i64::from(foreign_list.id),
            },
        };

        let store = Store::from(db);
        let error = upsert_workflow(
            &store,
            WorkflowDraft {
                id: None,
                board_id: i64::from(first_board.id),
                name: "Invalid cross-board workflow".to_string(),
                enabled: true,
                definition: invalid,
            },
        )
        .await
        .expect_err("a workflow must not target a list from another board");
        assert!(error.to_string().contains("outside board"));
        Ok(())
    }

    #[tokio::test]
    async fn lifecycle_event_dispatches_saved_workflow() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = crate::workspace::create_board(&db, None, "Lifecycle".to_string()).await?;
        let list = crate::board::commands::create_board_list(
            &db,
            crate::board::commands::BoardListDraft {
                title: "Work".to_string(),
                board_id: board.id,
                position: 0,
                workflow_role: ListWorkflowRole::Neutral,
                cards: Vec::new(),
            },
        )
        .await?;
        let entry = crate::board::commands::create_board_card(
            &db,
            crate::board::commands::BoardCardDraft {
                title: "Close me".to_string(),
                description: String::new(),
                list_id: list.id,
                position: 0,
                due_on: None,
                label_ids: Vec::new(),
                checklist_items: Vec::new(),
            },
            1,
        )
        .await?;
        let mut lifecycle_definition = definition();
        lifecycle_definition.nodes[0].kind = WorkflowNodeKind::Trigger {
            trigger: WorkflowTrigger::CardCompleted,
        };
        let store = Store::from(db);
        upsert_workflow(
            &store,
            WorkflowDraft {
                id: None,
                board_id: i64::from(board.id),
                name: "Archive completed".to_string(),
                enabled: true,
                definition: lifecycle_definition,
            },
        )
        .await?;

        let report = run_entry_event(
            &store,
            i64::from(entry.id),
            EventOrigin::User,
            WorkflowEventKind::CardCompleted,
        )
        .await?;
        assert_eq!(report.runs[0].status, WorkflowRunStatus::Succeeded);
        assert!(
            entity::entry::Entity::find_by_id(i64::from(entry.id))
                .one(&store)
                .await?
                .is_some_and(|entry| entry.archived)
        );
        Ok(())
    }

    #[tokio::test]
    async fn manual_workflow_execution_is_board_scoped() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = crate::workspace::create_board(&db, None, "Manual".to_string()).await?;
        let list = crate::board::commands::create_board_list(
            &db,
            crate::board::commands::BoardListDraft {
                title: "Work".to_string(),
                board_id: board.id,
                position: 0,
                workflow_role: ListWorkflowRole::Neutral,
                cards: Vec::new(),
            },
        )
        .await?;
        let entry = crate::board::commands::create_board_card(
            &db,
            crate::board::commands::BoardCardDraft {
                title: "Run me".to_string(),
                description: String::new(),
                list_id: list.id,
                position: 0,
                due_on: None,
                label_ids: Vec::new(),
                checklist_items: Vec::new(),
            },
            1,
        )
        .await?;
        let mut manual_definition = definition();
        manual_definition.nodes[0].kind = WorkflowNodeKind::Trigger {
            trigger: WorkflowTrigger::Manual,
        };
        let store = Store::from(db);
        upsert_workflow(
            &store,
            WorkflowDraft {
                id: None,
                board_id: i64::from(board.id),
                name: "Manual archive".to_string(),
                enabled: true,
                definition: manual_definition,
            },
        )
        .await?;

        let report = run_manual_workflow(
            &store,
            i64::from(board.id),
            i64::from(entry.id),
            EventOrigin::User,
        )
        .await?;
        assert_eq!(report.runs.len(), 1);
        assert_eq!(report.runs[0].status, WorkflowRunStatus::Succeeded);
        assert!(
            entity::entry::Entity::find_by_id(i64::from(entry.id))
                .one(&store)
                .await?
                .is_some_and(|entry| entry.archived)
        );
        Ok(())
    }

    #[tokio::test]
    async fn scheduled_workflows_run_for_active_entries() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = crate::workspace::create_board(&db, None, "Scheduled".to_string()).await?;
        let list = crate::board::commands::create_board_list(
            &db,
            crate::board::commands::BoardListDraft {
                title: "Work".to_string(),
                board_id: board.id,
                position: 0,
                workflow_role: ListWorkflowRole::Neutral,
                cards: Vec::new(),
            },
        )
        .await?;
        let entry = crate::board::commands::create_board_card(
            &db,
            crate::board::commands::BoardCardDraft {
                title: "Schedule me".to_string(),
                description: String::new(),
                list_id: list.id,
                position: 0,
                due_on: None,
                label_ids: Vec::new(),
                checklist_items: Vec::new(),
            },
            1,
        )
        .await?;
        let mut scheduled_definition = definition();
        scheduled_definition.nodes[0].kind = WorkflowNodeKind::Trigger {
            trigger: WorkflowTrigger::Scheduled {
                schedule_key: "calendar_open".to_string(),
            },
        };
        let store = Store::from(db);
        upsert_workflow(
            &store,
            WorkflowDraft {
                id: None,
                board_id: i64::from(board.id),
                name: "Scheduled archive".to_string(),
                enabled: true,
                definition: scheduled_definition,
            },
        )
        .await?;

        let report = run_scheduled_workflows(
            &store,
            Some(i64::from(board.id)),
            "calendar_open",
            EventOrigin::Scheduler,
        )
        .await?;
        assert_eq!(report.runs.len(), 1);
        assert_eq!(report.runs[0].status, WorkflowRunStatus::Succeeded);
        assert!(
            entity::entry::Entity::find_by_id(i64::from(entry.id))
                .one(&store)
                .await?
                .is_some_and(|entry| entry.archived)
        );
        Ok(())
    }
}
