use anyhow::{Context as _, Result};
use chrono::{Days, Local, NaiveDate};
use sea_orm::{DbBackend, Statement, Value};

pub const HOME_TASK_PAGE_SIZE: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceItemKind {
    Note,
    Board,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceHomeItem {
    pub kind: WorkspaceItemKind,
    pub id: u32,
    pub title: String,
    pub project_id: Option<u32>,
    pub project_name: Option<String>,
    pub is_pinned: bool,
    pub last_opened_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannerTask {
    pub entry_id: u32,
    pub board_id: u32,
    pub project_id: Option<u32>,
    pub title: String,
    pub board_title: String,
    pub list_title: String,
    pub due_on: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlannerTaskGroup {
    Today,
    Upcoming,
    Unscheduled,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceHomeState {
    pub today: Vec<PlannerTask>,
    pub today_total: usize,
    pub upcoming: Vec<PlannerTask>,
    pub upcoming_total: usize,
    pub unscheduled: Vec<PlannerTask>,
    pub unscheduled_total: usize,
    pub pinned: Vec<WorkspaceHomeItem>,
    pub recent: Vec<WorkspaceHomeItem>,
}

pub async fn load_home(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
) -> Result<WorkspaceHomeState> {
    load_home_on_date(db, Local::now().date_naive()).await
}

async fn load_home_on_date(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    date: NaiveDate,
) -> Result<WorkspaceHomeState> {
    let (today_total, upcoming_total, unscheduled_total) =
        load_planner_task_counts(db, date).await?;

    let today_entries =
        load_planner_task_page_on_date(db, date, PlannerTaskGroup::Today, 0, HOME_TASK_PAGE_SIZE)
            .await?;

    let upcoming_entries = load_planner_task_page_on_date(
        db,
        date,
        PlannerTaskGroup::Upcoming,
        0,
        HOME_TASK_PAGE_SIZE,
    )
    .await?;

    let unscheduled_entries = load_planner_task_page_on_date(
        db,
        date,
        PlannerTaskGroup::Unscheduled,
        0,
        HOME_TASK_PAGE_SIZE,
    )
    .await?;

    let items = load_home_items(db).await?;
    let pinned = items
        .iter()
        .filter(|item| item.is_pinned)
        .take(5)
        .cloned()
        .collect();

    let recent = items
        .into_iter()
        .filter(|item| !item.is_pinned && item.last_opened_at.is_some())
        .take(5)
        .collect();

    Ok(WorkspaceHomeState {
        today: today_entries,
        today_total,
        upcoming: upcoming_entries,
        upcoming_total,
        unscheduled: unscheduled_entries,
        unscheduled_total,
        pinned,
        recent,
    })
}

pub async fn load_planner_task_page(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    group: PlannerTaskGroup,
    offset: usize,
    limit: usize,
) -> Result<Vec<PlannerTask>> {
    load_planner_task_page_on_date(db, Local::now().date_naive(), group, offset, limit).await
}

async fn load_planner_task_counts(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    date: NaiveDate,
) -> Result<(usize, usize, usize)> {
    let today = date.format("%Y-%m-%d").to_string();
    let week_through = date
        .checked_add_days(Days::new(7))
        .unwrap_or(date)
        .format("%Y-%m-%d")
        .to_string();

    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT
                COUNT(CASE WHEN e.due_on <= ? THEN 1 END) AS today_total,
                COUNT(CASE WHEN e.due_on > ? AND e.due_on <= ? THEN 1 END) AS upcoming_total,
                COUNT(CASE WHEN e.due_on IS NULL THEN 1 END) AS unscheduled_total
            FROM entry e
            JOIN card c ON c.id = e.card_id AND c.deleted_at IS NULL
            JOIN board b ON b.id = c.board_id AND b.deleted_at IS NULL
            LEFT JOIN project p ON p.id = b.project_id
            WHERE e.deleted_at IS NULL
              AND e.archived = 0
              AND e.completed_at IS NULL
              AND e.cancelled_at IS NULL
              AND c.workflow_role = 'neutral'
              AND (b.project_id IS NULL OR p.deleted_at IS NULL)
            "#,
            [
                today.clone().into(),
                today.clone().into(),
                week_through.into(),
            ],
        ))
        .await?
        .context("planner task counts query returned no row")?;
    Ok((
        row.try_get::<i64>("", "today_total")?.try_into()?,
        row.try_get::<i64>("", "upcoming_total")?.try_into()?,
        row.try_get::<i64>("", "unscheduled_total")?.try_into()?,
    ))
}

async fn load_planner_task_page_on_date(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    date: NaiveDate,
    group: PlannerTaskGroup,
    offset: usize,
    limit: usize,
) -> Result<Vec<PlannerTask>> {
    let today = date.format("%Y-%m-%d").to_string();
    let week_through = date
        .checked_add_days(Days::new(7))
        .unwrap_or(date)
        .format("%Y-%m-%d")
        .to_string();

    let (date_filter, mut values): (&str, Vec<Value>) = match group {
        PlannerTaskGroup::Today => ("e.due_on <= ?", vec![today.into()]),
        PlannerTaskGroup::Upcoming => (
            "e.due_on > ? AND e.due_on <= ?",
            vec![today.into(), week_through.into()],
        ),
        PlannerTaskGroup::Unscheduled => ("e.due_on IS NULL", Vec::new()),
    };
    values.push(
        i64::try_from(limit)
            .context("planner page size is out of range")?
            .into(),
    );
    values.push(
        i64::try_from(offset)
            .context("planner page offset is out of range")?
            .into(),
    );
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!(
                r#"
                SELECT e.id AS entry_id, b.id AS board_id, b.project_id, e.title,
                       b.title AS board_title, c.title AS list_title, e.due_on
                FROM entry e
                JOIN card c ON c.id = e.card_id AND c.deleted_at IS NULL
                JOIN board b ON b.id = c.board_id AND b.deleted_at IS NULL
                LEFT JOIN project p ON p.id = b.project_id
                WHERE e.deleted_at IS NULL
                  AND e.archived = 0
                  AND e.completed_at IS NULL
                  AND e.cancelled_at IS NULL
                  AND c.workflow_role = 'neutral'
                  AND (b.project_id IS NULL OR p.deleted_at IS NULL)
                  AND ({date_filter})
                ORDER BY CASE WHEN e.due_on IS NULL THEN 1 ELSE 0 END,
                         e.due_on ASC, b.title ASC, c.position ASC, e.position ASC, e.id ASC
                LIMIT ? OFFSET ?
                "#
            ),
            values,
        ))
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(PlannerTask {
                entry_id: row.try_get::<i64>("", "entry_id")? as u32,
                board_id: row.try_get::<i64>("", "board_id")? as u32,
                project_id: row
                    .try_get::<Option<i64>>("", "project_id")?
                    .map(|id| id as u32),
                title: row.try_get("", "title")?,
                board_title: row.try_get("", "board_title")?,
                list_title: row.try_get("", "list_title")?,
                due_on: row.try_get("", "due_on")?,
            })
        })
        .collect()
}

async fn load_home_items(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
) -> Result<Vec<WorkspaceHomeItem>> {
    let rows = db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            WITH items AS (
                SELECT 'note' AS kind, n.id, n.title, n.project_id, p.name AS project_name,
                       n.is_pinned, n.last_opened_at
                FROM note n
                LEFT JOIN project p ON p.id = n.project_id
                WHERE n.deleted_at IS NULL AND (n.project_id IS NULL OR p.deleted_at IS NULL)
                UNION ALL
                SELECT 'board' AS kind, b.id, b.title, b.project_id, p.name AS project_name,
                       b.is_pinned, b.last_opened_at
                FROM board b
                LEFT JOIN project p ON p.id = b.project_id
                WHERE b.deleted_at IS NULL AND (b.project_id IS NULL OR p.deleted_at IS NULL)
            ), pinned AS (
                SELECT * FROM items
                WHERE is_pinned = 1
                ORDER BY COALESCE(last_opened_at, 0) DESC, title ASC
                LIMIT 5
            ), recent AS (
                SELECT * FROM items
                WHERE is_pinned = 0 AND last_opened_at IS NOT NULL
                ORDER BY last_opened_at DESC, title ASC
                LIMIT 5
            )
            SELECT kind, id, title, project_id, project_name, is_pinned, last_opened_at
            FROM (
                SELECT * FROM pinned
                UNION ALL
                SELECT * FROM recent
            )
            ORDER BY is_pinned DESC, COALESCE(last_opened_at, 0) DESC, title ASC
            "#,
        ))
        .await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(WorkspaceHomeItem {
            kind: match row.try_get::<String>("", "kind")?.as_str() {
                "note" => WorkspaceItemKind::Note,
                _ => WorkspaceItemKind::Board,
            },
            id: row.try_get::<i64>("", "id")? as u32,
            title: row.try_get("", "title")?,
            project_id: row
                .try_get::<Option<i64>>("", "project_id")?
                .map(|id| id as u32),
            project_name: row.try_get("", "project_name")?,
            is_pinned: row.try_get("", "is_pinned")?,
            last_opened_at: row.try_get("", "last_opened_at")?,
        });
    }
    Ok(items)
}

pub async fn mark_opened(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    kind: WorkspaceItemKind,
    id: u32,
    opened_at: i64,
) -> Result<()> {
    let table = match kind {
        WorkspaceItemKind::Note => "note",
        WorkspaceItemKind::Board => "board",
    };
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        format!("UPDATE {table} SET last_opened_at = ? WHERE id = ? AND deleted_at IS NULL"),
        [opened_at.into(), (id as i64).into()],
    ))
    .await?;
    Ok(())
}

pub async fn set_pinned(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    kind: WorkspaceItemKind,
    id: u32,
    pinned: bool,
) -> Result<()> {
    let table = match kind {
        WorkspaceItemKind::Note => "note",
        WorkspaceItemKind::Board => "board",
    };
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        format!("UPDATE {table} SET is_pinned = ? WHERE id = ? AND deleted_at IS NULL"),
        [pinned.into(), (id as i64).into()],
    ))
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity::{board, card, entry, note, project};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};

    #[tokio::test]
    async fn home_orders_due_work_and_separates_pinned_from_recent() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let today = NaiveDate::from_ymd_opt(2026, 9, 24).expect("valid test date");

        let project = project::ActiveModel {
            name: Set("Castle".to_string()),
            archived: Set(false),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let pinned_note = note::ActiveModel {
            title: Set("Pinned note".to_string()),
            project_id: Set(Some(project.id)),
            file_path: Set(None),
            file_managed_by_app: Set(false),
            cached_content: Set(String::new()),
            file_missing_since: Set(None),
            created_at: Set(1),
            updated_at: Set(1),
            is_pinned: Set(true),
            last_opened_at: Set(Some(5)),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let board = board::ActiveModel {
            title: Set("Roadmap".to_string()),
            project_id: Set(Some(project.id)),
            is_pinned: Set(false),
            last_opened_at: Set(Some(10)),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let list = card::ActiveModel {
            title: Set("Doing".to_string()),
            board_id: Set(board.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        for (position, (title, due_on)) in [
            ("Overdue", "2026-09-23"),
            ("Ship Home", "2026-09-24"),
            ("Future", "2026-09-25"),
        ]
        .into_iter()
        .enumerate()
        {
            entry::ActiveModel {
                title: Set(title.to_string()),
                description: Set(String::new()),
                card_id: Set(list.id),
                position: Set(position as i32),
                due_on: Set(Some(due_on.to_string())),
                ..Default::default()
            }
            .insert(&db)
            .await?;
        }

        let home = load_home_on_date(&db, today).await?;
        assert_eq!(home.today.len(), 2);
        assert_eq!(home.today[0].title, "Overdue");
        assert_eq!(home.today[1].title, "Ship Home");
        assert_eq!(home.pinned.len(), 1);
        assert_eq!(home.pinned[0].id, pinned_note.id as u32);
        assert_eq!(home.recent.len(), 1);
        assert_eq!(home.recent[0].id, board.id as u32);
        Ok(())
    }
}
