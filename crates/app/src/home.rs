use anyhow::Result;
use chrono::Local;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceItemKind {
    Note,
    Board,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceHomeItem {
    pub(crate) kind: WorkspaceItemKind,
    pub(crate) id: u32,
    pub(crate) title: String,
    pub(crate) project_id: Option<u32>,
    pub(crate) project_name: Option<String>,
    pub(crate) is_pinned: bool,
    pub(crate) last_opened_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TodayEntry {
    pub(crate) entry_id: u32,
    pub(crate) board_id: u32,
    pub(crate) project_id: Option<u32>,
    pub(crate) title: String,
    pub(crate) board_title: String,
    pub(crate) list_title: String,
    pub(crate) due_on: String,
    pub(crate) labels: Vec<String>,
    pub(crate) checklist_checked: u32,
    pub(crate) checklist_total: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceHomeState {
    pub(crate) today: Vec<TodayEntry>,
    pub(crate) pinned: Vec<WorkspaceHomeItem>,
    pub(crate) recent: Vec<WorkspaceHomeItem>,
}

pub(crate) async fn load_home(db: &DatabaseConnection) -> Result<WorkspaceHomeState> {
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT e.id AS entry_id, b.id AS board_id, b.project_id, e.title,
                   b.title AS board_title, c.title AS list_title, e.due_on,
                   COALESCE(GROUP_CONCAT(DISTINCT bl.name), '') AS labels,
                   COUNT(DISTINCT ci.id) AS checklist_total,
                   COUNT(DISTINCT CASE WHEN ci.checked = 1 THEN ci.id END) AS checklist_checked
            FROM entry e
            JOIN card c ON c.id = e.card_id AND c.deleted_at IS NULL
            JOIN board b ON b.id = c.board_id AND b.deleted_at IS NULL
            LEFT JOIN project p ON p.id = b.project_id
            LEFT JOIN entry_label el ON el.entry_id = e.id
            LEFT JOIN board_label bl ON bl.id = el.board_label_id
            LEFT JOIN entry_checklist_item ci ON ci.entry_id = e.id
            WHERE e.deleted_at IS NULL
              AND e.due_on IS NOT NULL
              AND e.due_on <= ?
              AND (b.project_id IS NULL OR p.deleted_at IS NULL)
            GROUP BY e.id, b.id, b.project_id, e.title, b.title, c.title, e.due_on
            ORDER BY e.due_on ASC, b.title ASC, c.position ASC, e.position ASC, e.id ASC
            "#,
            [today.into()],
        ))
        .await?;

    let mut due_entries = Vec::with_capacity(rows.len());
    for row in rows {
        let labels: String = row.try_get("", "labels")?;
        due_entries.push(TodayEntry {
            entry_id: row.try_get::<i64>("", "entry_id")? as u32,
            board_id: row.try_get::<i64>("", "board_id")? as u32,
            project_id: row
                .try_get::<Option<i64>>("", "project_id")?
                .map(|id| id as u32),
            title: row.try_get("", "title")?,
            board_title: row.try_get("", "board_title")?,
            list_title: row.try_get("", "list_title")?,
            due_on: row.try_get("", "due_on")?,
            labels: labels
                .split(',')
                .filter(|label| !label.is_empty())
                .map(str::to_string)
                .collect(),
            checklist_checked: row.try_get::<i64>("", "checklist_checked")? as u32,
            checklist_total: row.try_get::<i64>("", "checklist_total")? as u32,
        });
    }

    let items = load_home_items(db).await?;
    let pinned = items
        .iter()
        .filter(|item| item.is_pinned)
        .cloned()
        .collect();
    let recent = items
        .into_iter()
        .filter(|item| !item.is_pinned && item.last_opened_at.is_some())
        .take(8)
        .collect();

    Ok(WorkspaceHomeState {
        today: due_entries,
        pinned,
        recent,
    })
}

async fn load_home_items(db: &DatabaseConnection) -> Result<Vec<WorkspaceHomeItem>> {
    let rows = db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            SELECT kind, id, title, project_id, project_name, is_pinned, last_opened_at
            FROM (
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
            )
            WHERE is_pinned = 1 OR last_opened_at IS NOT NULL
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

pub(crate) async fn mark_opened(
    db: &DatabaseConnection,
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

pub(crate) async fn set_pinned(
    db: &DatabaseConnection,
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
        entry::ActiveModel {
            title: Set("Ship Home".to_string()),
            description: Set(String::new()),
            card_id: Set(list.id),
            position: Set(0),
            due_on: Set(Some(Local::now().date_naive().to_string())),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let home = load_home(&db).await?;
        assert_eq!(home.today.len(), 1);
        assert_eq!(home.today[0].title, "Ship Home");
        assert_eq!(home.pinned.len(), 1);
        assert_eq!(home.pinned[0].id, pinned_note.id as u32);
        assert_eq!(home.recent.len(), 1);
        assert_eq!(home.recent[0].id, board.id as u32);
        Ok(())
    }
}
