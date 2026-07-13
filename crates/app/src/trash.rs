use std::path::PathBuf;

use anyhow::{Result, bail};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrashItemKind {
    Project,
    Note,
    Board,
    List,
    Entry,
}

impl TrashItemKind {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Note => "note",
            Self::Board => "board",
            Self::List => "list",
            Self::Entry => "entry",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Note => "Note",
            Self::Board => "Board",
            Self::List => "List",
            Self::Entry => "Card",
        }
    }

    fn table(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Note => "note",
            Self::Board => "board",
            Self::List => "card",
            Self::Entry => "entry",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrashItem {
    pub(crate) kind: TrashItemKind,
    pub(crate) id: u32,
    pub(crate) title: String,
    pub(crate) location: Option<String>,
    pub(crate) deleted_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MoveToTrash {
    pub(crate) kind: TrashItemKind,
    pub(crate) id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RestoreTrashItem(pub(crate) MoveToTrash);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PurgeTrashItem(pub(crate) MoveToTrash);

pub(crate) async fn load_trash(db: &DatabaseConnection) -> Result<Vec<TrashItem>> {
    let rows = db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            SELECT kind, id, title, location, deleted_at FROM (
                SELECT 'project' AS kind, p.id, p.name AS title, NULL AS location, p.deleted_at
                FROM project p WHERE p.deleted_at IS NOT NULL
                UNION ALL
                SELECT 'note', n.id, n.title, COALESCE(p.name, 'Standalone'), n.deleted_at
                FROM note n LEFT JOIN project p ON p.id = n.project_id
                WHERE n.deleted_at IS NOT NULL
                UNION ALL
                SELECT 'board', b.id, b.title, COALESCE(p.name, 'Standalone'), b.deleted_at
                FROM board b LEFT JOIN project p ON p.id = b.project_id
                WHERE b.deleted_at IS NOT NULL
                UNION ALL
                SELECT 'list', c.id, c.title, b.title, c.deleted_at
                FROM card c JOIN board b ON b.id = c.board_id
                WHERE c.deleted_at IS NOT NULL
                UNION ALL
                SELECT 'entry', e.id, e.title, b.title || ' / ' || c.title, e.deleted_at
                FROM entry e JOIN card c ON c.id = e.card_id JOIN board b ON b.id = c.board_id
                WHERE e.deleted_at IS NOT NULL
            )
            ORDER BY deleted_at DESC, title ASC
            "#,
        ))
        .await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let kind = match row.try_get::<String>("", "kind")?.as_str() {
            "project" => TrashItemKind::Project,
            "note" => TrashItemKind::Note,
            "board" => TrashItemKind::Board,
            "list" => TrashItemKind::List,
            _ => TrashItemKind::Entry,
        };
        items.push(TrashItem {
            kind,
            id: row.try_get::<i64>("", "id")? as u32,
            title: row.try_get("", "title")?,
            location: row.try_get("", "location")?,
            deleted_at: row.try_get("", "deleted_at")?,
        });
    }
    Ok(items)
}

pub(crate) async fn move_to_trash(
    db: &DatabaseConnection,
    item: MoveToTrash,
    deleted_at: i64,
) -> Result<()> {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        format!(
            "UPDATE {} SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL",
            item.kind.table()
        ),
        [deleted_at.into(), (item.id as i64).into()],
    ))
    .await?;
    crate::search::rebuild_search_index(db).await?;
    Ok(())
}

pub(crate) async fn restore_item(db: &DatabaseConnection, item: RestoreTrashItem) -> Result<()> {
    ensure_parent_available(db, item.0).await?;
    let sql = if item.0.kind == TrashItemKind::Project {
        "UPDATE project SET deleted_at = NULL, archived = 0 WHERE id = ? AND deleted_at IS NOT NULL"
            .to_string()
    } else {
        format!(
            "UPDATE {} SET deleted_at = NULL WHERE id = ? AND deleted_at IS NOT NULL",
            item.0.kind.table()
        )
    };
    let result = db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            [(item.0.id as i64).into()],
        ))
        .await?;
    if result.rows_affected() != 1 {
        bail!("This item is no longer in Trash");
    }
    crate::search::rebuild_search_index(db).await?;
    Ok(())
}

pub(crate) async fn purge_item(
    db: &DatabaseConnection,
    item: PurgeTrashItem,
) -> Result<Vec<PathBuf>> {
    let mut managed_files = Vec::new();
    if item.0.kind == TrashItemKind::Note {
        let row = db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT file_path, file_managed_by_app FROM note WHERE id = ? AND deleted_at IS NOT NULL",
                [(item.0.id as i64).into()],
            ))
            .await?;
        if let Some(path) = row.and_then(|row| {
            let managed = row
                .try_get::<bool>("", "file_managed_by_app")
                .unwrap_or(false);
            managed
                .then(|| {
                    row.try_get::<Option<String>>("", "file_path")
                        .ok()
                        .flatten()
                })
                .flatten()
                .map(PathBuf::from)
        }) {
            managed_files.push(path);
        }
    } else if item.0.kind == TrashItemKind::Project {
        let rows = db
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT file_path FROM note WHERE project_id = ? AND file_managed_by_app = 1 AND file_path IS NOT NULL",
                [(item.0.id as i64).into()],
            ))
            .await?;
        for row in rows {
            managed_files.push(PathBuf::from(row.try_get::<String>("", "file_path")?));
        }
        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "DELETE FROM note WHERE project_id = ?",
            [(item.0.id as i64).into()],
        ))
        .await?;
        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "DELETE FROM board WHERE project_id = ?",
            [(item.0.id as i64).into()],
        ))
        .await?;
    }

    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        format!(
            "DELETE FROM {} WHERE id = ? AND deleted_at IS NOT NULL",
            item.0.kind.table()
        ),
        [(item.0.id as i64).into()],
    ))
    .await?;
    crate::search::rebuild_search_index(db).await?;
    Ok(managed_files)
}

pub(crate) async fn purge_all(db: &DatabaseConnection) -> Result<Vec<PathBuf>> {
    let rows = db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            SELECT n.file_path
            FROM note n
            LEFT JOIN project p ON p.id = n.project_id
            WHERE n.file_managed_by_app = 1
              AND n.file_path IS NOT NULL
              AND (n.deleted_at IS NOT NULL OR p.deleted_at IS NOT NULL)
            "#,
        ))
        .await?;
    let mut managed_files = Vec::with_capacity(rows.len());
    for row in rows {
        managed_files.push(PathBuf::from(row.try_get::<String>("", "file_path")?));
    }

    for sql in [
        "DELETE FROM entry WHERE deleted_at IS NOT NULL",
        "DELETE FROM card WHERE deleted_at IS NOT NULL",
        "DELETE FROM note WHERE deleted_at IS NOT NULL",
        "DELETE FROM board WHERE deleted_at IS NOT NULL",
        "DELETE FROM note WHERE project_id IN (SELECT id FROM project WHERE deleted_at IS NOT NULL)",
        "DELETE FROM board WHERE project_id IN (SELECT id FROM project WHERE deleted_at IS NOT NULL)",
        "DELETE FROM project WHERE deleted_at IS NOT NULL",
    ] {
        db.execute_raw(Statement::from_string(DbBackend::Sqlite, sql))
            .await?;
    }
    crate::search::rebuild_search_index(db).await?;
    Ok(managed_files)
}

async fn ensure_parent_available(db: &DatabaseConnection, item: MoveToTrash) -> Result<()> {
    let sql = match item.kind {
        TrashItemKind::Project => return Ok(()),
        TrashItemKind::Note => {
            "SELECT p.deleted_at FROM note n LEFT JOIN project p ON p.id = n.project_id WHERE n.id = ?"
        }
        TrashItemKind::Board => {
            "SELECT p.deleted_at FROM board b LEFT JOIN project p ON p.id = b.project_id WHERE b.id = ?"
        }
        TrashItemKind::List => {
            "SELECT COALESCE(b.deleted_at, p.deleted_at) AS deleted_at FROM card c JOIN board b ON b.id = c.board_id LEFT JOIN project p ON p.id = b.project_id WHERE c.id = ?"
        }
        TrashItemKind::Entry => {
            "SELECT COALESCE(c.deleted_at, b.deleted_at, p.deleted_at) AS deleted_at FROM entry e JOIN card c ON c.id = e.card_id JOIN board b ON b.id = c.board_id LEFT JOIN project p ON p.id = b.project_id WHERE e.id = ?"
        }
    };
    if let Some(row) = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            [(item.id as i64).into()],
        ))
        .await?
        && row.try_get::<Option<i64>>("", "deleted_at")?.is_some()
    {
        bail!("Restore the parent item first");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity::{board, card, entry, note, project};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{
        ActiveModelTrait, ActiveValue::Set, ConnectOptions, ConnectionTrait, Database, EntityTrait,
    };

    #[tokio::test]
    async fn restoring_note_preserves_file_and_cached_content() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let inserted = note::ActiveModel {
            title: Set("Feature plan".to_string()),
            project_id: Set(None),
            file_path: Set(Some("C:\\notes\\features.md".to_string())),
            file_managed_by_app: Set(false),
            cached_content: Set("# Features\n\nRestorable content".to_string()),
            file_missing_since: Set(None),
            created_at: Set(10),
            updated_at: Set(20),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let request = MoveToTrash {
            kind: TrashItemKind::Note,
            id: inserted.id as u32,
        };

        move_to_trash(&db, request, 30).await?;
        restore_item(&db, RestoreTrashItem(request)).await?;

        let restored = note::Entity::find_by_id(inserted.id)
            .one(&db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("restored note is missing"))?;
        assert_eq!(restored.deleted_at, None);
        assert_eq!(restored.file_path, inserted.file_path);
        assert_eq!(restored.file_managed_by_app, inserted.file_managed_by_app);
        assert_eq!(restored.cached_content, inserted.cached_content);
        assert!(restore_item(&db, RestoreTrashItem(request)).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn archived_projects_migrate_to_trash_and_notes_restore() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, Some(4)).await?;
        db.execute_unprepared(
            "INSERT INTO project (name, archived, position) VALUES ('Archived', 1, 0)",
        )
        .await?;
        let project_id = 1_i64;
        Migrator::up(&db, None).await?;

        let items = load_trash(&db).await?;
        assert!(
            items.iter().any(|item| {
                item.kind == TrashItemKind::Project && item.id == project_id as u32
            })
        );

        restore_item(
            &db,
            RestoreTrashItem(MoveToTrash {
                kind: TrashItemKind::Project,
                id: project_id as u32,
            }),
        )
        .await?;

        let note = note::ActiveModel {
            title: Set("Recover me".to_string()),
            project_id: Set(Some(project_id)),
            file_path: Set(None),
            file_managed_by_app: Set(false),
            cached_content: Set(String::new()),
            file_missing_since: Set(None),
            created_at: Set(1),
            updated_at: Set(1),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let request = MoveToTrash {
            kind: TrashItemKind::Note,
            id: note.id as u32,
        };
        move_to_trash(&db, request, 42).await?;
        assert_eq!(load_trash(&db).await?.len(), 1);
        restore_item(&db, RestoreTrashItem(request)).await?;
        assert!(load_trash(&db).await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn nested_items_require_their_project_to_be_restored_first() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let project = project::ActiveModel {
            name: Set("Trashed project".to_string()),
            archived: Set(false),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let board = board::ActiveModel {
            title: Set("Board".to_string()),
            project_id: Set(Some(project.id)),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let trashed_list = card::ActiveModel {
            title: Set("List".to_string()),
            board_id: Set(board.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let active_list = card::ActiveModel {
            title: Set("Active list".to_string()),
            board_id: Set(board.id),
            position: Set(1),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let trashed_entry = entry::ActiveModel {
            title: Set("Card".to_string()),
            description: Set(String::new()),
            card_id: Set(active_list.id),
            position: Set(0),
            due_on: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let list_request = MoveToTrash {
            kind: TrashItemKind::List,
            id: trashed_list.id as u32,
        };
        let entry_request = MoveToTrash {
            kind: TrashItemKind::Entry,
            id: trashed_entry.id as u32,
        };
        move_to_trash(&db, list_request, 1).await?;
        move_to_trash(&db, entry_request, 2).await?;
        move_to_trash(
            &db,
            MoveToTrash {
                kind: TrashItemKind::Project,
                id: project.id as u32,
            },
            3,
        )
        .await?;

        assert!(
            restore_item(&db, RestoreTrashItem(list_request))
                .await
                .is_err()
        );
        assert!(
            restore_item(&db, RestoreTrashItem(entry_request))
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn repeated_note_trash_cycles_release_pool_connections() -> Result<()> {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.max_connections(2).min_connections(1);
        let db = Database::connect(options).await?;
        Migrator::up(&db, None).await?;
        let inserted = note::ActiveModel {
            title: Set("Repeated restore".to_string()),
            project_id: Set(None),
            file_path: Set(None),
            file_managed_by_app: Set(false),
            cached_content: Set("# Content".to_string()),
            file_missing_since: Set(None),
            created_at: Set(1),
            updated_at: Set(1),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let request = MoveToTrash {
            kind: TrashItemKind::Note,
            id: inserted.id as u32,
        };

        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            for cycle in 0..20 {
                move_to_trash(&db, request, cycle).await?;
                assert_eq!(load_trash(&db).await?.len(), 1);
                restore_item(&db, RestoreTrashItem(request)).await?;
                assert!(load_trash(&db).await?.is_empty());
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;

        Ok(())
    }
}
