use anyhow::Result;
use entity::{
    board, board::Entity as Board, note, note::Entity as Note, project, project::Entity as Project,
};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DbBackend, DbErr, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Statement,
    sea_query::{Query, SelectStatement},
};

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static WORKSPACE_LOAD_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectRow {
    pub id: u32,
    pub name: String,
    pub position: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardRow {
    pub id: u32,
    pub title: String,
    pub project_id: Option<u32>,
    pub is_pinned: bool,
    pub last_opened_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteRow {
    pub id: u32,
    pub title: String,
    pub project_id: Option<u32>,
    pub file_path: Option<String>,
    pub is_pinned: bool,
    pub last_opened_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRows {
    pub projects: Vec<ProjectRow>,
    pub boards: Vec<BoardRow>,
    pub notes: Vec<NoteRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceItem {
    pub id: u32,
    pub title: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkspaceTitleTarget {
    Board(u32),
    Note(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangeRevision {
    pub revision: i64,
    pub board_revision: i64,
    pub note_revision: i64,
}

fn visible_project_ids_query() -> SelectStatement {
    Query::select()
        .column(project::Column::Id)
        .from(Project)
        .and_where(project::Column::Archived.eq(false))
        .and_where(project::Column::DeletedAt.is_null())
        .to_owned()
}

pub async fn load_workspace_rows(db: &DatabaseConnection) -> Result<WorkspaceRows> {
    WORKSPACE_LOAD_COUNT.fetch_add(1, Ordering::Relaxed);

    let projects: Vec<ProjectRow> = Project::find()
        .filter(project::Column::Archived.eq(false))
        .filter(project::Column::DeletedAt.is_null())
        .order_by_asc(project::Column::Position)
        .order_by_asc(project::Column::Id)
        .select_only()
        .column(project::Column::Id)
        .column(project::Column::Name)
        .column(project::Column::Position)
        .into_tuple::<(i64, String, i32)>()
        .all(db)
        .await?
        .into_iter()
        .map(|(id, name, position)| ProjectRow {
            id: id as u32,
            name,
            position,
        })
        .collect();

    let boards = Board::find()
        .filter(board::Column::DeletedAt.is_null())
        .filter(
            Condition::any()
                .add(board::Column::ProjectId.is_null())
                .add(board::Column::ProjectId.in_subquery(visible_project_ids_query())),
        )
        .order_by_asc(board::Column::Id)
        .select_only()
        .column(board::Column::Id)
        .column(board::Column::Title)
        .column(board::Column::ProjectId)
        .column(board::Column::IsPinned)
        .column(board::Column::LastOpenedAt)
        .into_tuple::<(i64, String, Option<i64>, bool, Option<i64>)>()
        .all(db)
        .await?
        .into_iter()
        .map(
            |(id, title, project_id, is_pinned, last_opened_at)| BoardRow {
                id: id as u32,
                title,
                project_id: project_id.map(|id| id as u32),
                is_pinned,
                last_opened_at,
            },
        )
        .collect();

    let notes = Note::find()
        .filter(note::Column::DeletedAt.is_null())
        .filter(
            Condition::any()
                .add(note::Column::ProjectId.is_null())
                .add(note::Column::ProjectId.in_subquery(visible_project_ids_query())),
        )
        .order_by_asc(note::Column::Id)
        .select_only()
        .column(note::Column::Id)
        .column(note::Column::Title)
        .column(note::Column::ProjectId)
        .column(note::Column::FilePath)
        .column(note::Column::IsPinned)
        .column(note::Column::LastOpenedAt)
        .into_tuple::<(i64, String, Option<i64>, Option<String>, bool, Option<i64>)>()
        .all(db)
        .await?
        .into_iter()
        .map(
            |(id, title, project_id, file_path, is_pinned, last_opened_at)| NoteRow {
                id: id as u32,
                title,
                project_id: project_id.map(|id| id as u32),
                file_path,
                is_pinned,
                last_opened_at,
            },
        )
        .collect();

    Ok(WorkspaceRows {
        projects,
        boards,
        notes,
    })
}

pub async fn create_managed_note(
    db: &DatabaseConnection,
    project_id: Option<u32>,
    title: String,
    file_path: String,
    content: String,
) -> Result<WorkspaceItem, DbErr> {
    let now = now_ts();
    let note = note::ActiveModel {
        title: Set(title),
        project_id: Set(project_id.map(i64::from)),
        file_path: Set(Some(file_path)),
        file_managed_by_app: Set(true),
        cached_content: Set(content),
        file_missing_since: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(WorkspaceItem {
        id: note.id as u32,
        title: note.title,
    })
}

pub async fn import_external_note(
    db: &DatabaseConnection,
    title: String,
    file_path: String,
    content: String,
) -> Result<WorkspaceItem, DbErr> {
    let existing = Note::find()
        .filter(note::Column::FilePath.eq(file_path.clone()))
        .one(db)
        .await?;

    let note = if let Some(existing) = existing {
        note::ActiveModel {
            id: Set(existing.id),
            file_path: Set(Some(file_path)),
            cached_content: Set(content),
            file_missing_since: Set(None),
            updated_at: Set(now_ts()),
            ..Default::default()
        }
        .update(db)
        .await?
    } else {
        let now = now_ts();
        note::ActiveModel {
            title: Set(title),
            project_id: Set(None),
            file_path: Set(Some(file_path)),
            file_managed_by_app: Set(false),
            cached_content: Set(content),
            file_missing_since: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await?
    };

    Ok(WorkspaceItem {
        id: note.id as u32,
        title: note.title,
    })
}

pub async fn create_board(
    db: &DatabaseConnection,
    project_id: Option<u32>,
    title: String,
) -> Result<WorkspaceItem, DbErr> {
    let board = board::ActiveModel {
        title: Set(title),
        project_id: Set(project_id.map(i64::from)),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(WorkspaceItem {
        id: board.id as u32,
        title: board.title,
    })
}

pub async fn persist_workspace_title(
    db: &DatabaseConnection,
    target: WorkspaceTitleTarget,
    title: String,
) -> Result<()> {
    match target {
        WorkspaceTitleTarget::Board(board_id) => {
            board::ActiveModel {
                id: Set(board_id as i64),
                title: Set(title),
                ..Default::default()
            }
            .update(db)
            .await?;
        }
        WorkspaceTitleTarget::Note(note_id) => {
            let current = Note::find_by_id(note_id as i64)
                .one(db)
                .await?
                .ok_or_else(|| anyhow::anyhow!("note {note_id} was not found"))?;
            let now = now_ts();
            if current.title != title {
                crate::note_links::record_note_alias(db, current.id, &current.title, now).await?;
            }
            note::ActiveModel {
                id: Set(note_id as i64),
                title: Set(title),
                updated_at: Set(now),
                ..Default::default()
            }
            .update(db)
            .await?;
        }
    }

    Ok(())
}

pub async fn load_change_revision(db: &DatabaseConnection) -> Result<ChangeRevision, DbErr> {
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT revision, board_revision, note_revision
             FROM castle_change_revision WHERE id = 1",
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("Castle change revision row is missing".to_string()))?;

    Ok(ChangeRevision {
        revision: row.try_get("", "revision")?,
        board_revision: row.try_get("", "board_revision")?,
        note_revision: row.try_get("", "note_revision")?,
    })
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

pub fn reset_workspace_load_count() {
    WORKSPACE_LOAD_COUNT.store(0, Ordering::Relaxed);
}

pub fn workspace_load_count() -> usize {
    WORKSPACE_LOAD_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_alloc;
    use entity::{board, note, note_alias, project};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database, EntityTrait};

    #[tokio::test]
    async fn projected_workspace_rows_avoid_materializing_note_bodies() -> Result<()> {
        const NOTE_COUNT: usize = 8;
        const BODY_BYTES_PER_NOTE: usize = 1024 * 1024;

        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let project = project::ActiveModel {
            name: Set("Memory proof".to_string()),
            archived: Set(false),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let large_body = "x".repeat(BODY_BYTES_PER_NOTE);
        for index in 0..NOTE_COUNT {
            note::ActiveModel {
                title: Set(format!("Large note {index}")),
                project_id: Set(Some(project.id)),
                file_path: Set(None),
                file_managed_by_app: Set(false),
                cached_content: Set(large_body.clone()),
                file_missing_since: Set(None),
                created_at: Set(index as i64),
                updated_at: Set(index as i64),
                ..Default::default()
            }
            .insert(&db)
            .await?;
        }

        let full_notes = Note::find().all(&db).await?;
        let full_body_bytes = full_notes
            .iter()
            .map(|note| note.cached_content.len())
            .sum::<usize>();

        let projected_rows = load_workspace_rows(&db).await?;
        let projected_title_bytes = projected_rows
            .notes
            .iter()
            .map(|note| note.title.len())
            .sum::<usize>();

        assert_eq!(projected_rows.notes.len(), NOTE_COUNT);
        assert_eq!(full_notes.len(), NOTE_COUNT);
        assert_eq!(full_body_bytes, NOTE_COUNT * BODY_BYTES_PER_NOTE);
        assert_eq!(projected_rows.notes[0].project_id, Some(project.id as u32));
        assert!(
            projected_title_bytes < full_body_bytes / 1000,
            "projected loader materialized too much note payload: projected title bytes={projected_title_bytes}, legacy body bytes={full_body_bytes}",
        );

        println!(
            "legacy_note_body_bytes={full_body_bytes} projected_note_body_bytes=0 projected_title_bytes={projected_title_bytes}",
        );

        Ok(())
    }

    #[tokio::test]
    async fn workspace_rows_keep_standalone_items_and_exclude_inactive_projects() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let active_project = project::ActiveModel {
            name: Set("Active".to_string()),
            archived: Set(false),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let archived_project = project::ActiveModel {
            name: Set("Archived".to_string()),
            archived: Set(true),
            position: Set(1),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let deleted_project = project::ActiveModel {
            name: Set("Deleted".to_string()),
            archived: Set(false),
            position: Set(2),
            deleted_at: Set(Some(1)),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        for (id, title, project_id) in [
            (1, "Active board", Some(active_project.id)),
            (2, "Archived board", Some(archived_project.id)),
            (3, "Deleted board", Some(deleted_project.id)),
            (4, "Standalone board", None),
        ] {
            board::ActiveModel {
                id: Set(id),
                title: Set(title.to_string()),
                project_id: Set(project_id),
                ..Default::default()
            }
            .insert(&db)
            .await?;
        }

        for (id, title, project_id, file_path) in [
            (
                1,
                "Active note",
                Some(active_project.id),
                Some("active.json"),
            ),
            (
                2,
                "Archived note",
                Some(archived_project.id),
                Some("archived.md"),
            ),
            (
                3,
                "Deleted note",
                Some(deleted_project.id),
                Some("deleted.md"),
            ),
            (4, "Standalone note", None, Some("scratch.txt")),
        ] {
            note::ActiveModel {
                id: Set(id),
                title: Set(title.to_string()),
                project_id: Set(project_id),
                file_path: Set(file_path.map(str::to_string)),
                file_managed_by_app: Set(false),
                cached_content: Set(String::new()),
                file_missing_since: Set(None),
                created_at: Set(id),
                updated_at: Set(id),
                ..Default::default()
            }
            .insert(&db)
            .await?;
        }

        let rows = load_workspace_rows(&db).await?;

        assert_eq!(
            rows.projects
                .iter()
                .map(|project| project.id)
                .collect::<Vec<_>>(),
            vec![active_project.id as u32]
        );
        assert_eq!(
            rows.boards.iter().map(|board| board.id).collect::<Vec<_>>(),
            vec![1, 4]
        );
        assert_eq!(
            rows.notes.iter().map(|note| note.id).collect::<Vec<_>>(),
            vec![1, 4]
        );
        assert_eq!(rows.notes[0].file_path.as_deref(), Some("active.json"));
        assert_eq!(rows.notes[1].file_path.as_deref(), Some("scratch.txt"));

        Ok(())
    }

    #[tokio::test]
    async fn workspace_mutations_keep_persistence_details_out_of_callers() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let board = create_board(&db, None, "Roadmap".to_string()).await?;
        let note = create_managed_note(
            &db,
            None,
            "Design".to_string(),
            "notes/design.md".to_string(),
            "# Design".to_string(),
        )
        .await?;
        let imported = import_external_note(
            &db,
            "External".to_string(),
            "C:\\notes\\external.md".to_string(),
            "First".to_string(),
        )
        .await?;
        let refreshed = import_external_note(
            &db,
            "Ignored replacement title".to_string(),
            "C:\\notes\\external.md".to_string(),
            "Second".to_string(),
        )
        .await?;

        assert_eq!(board.title, "Roadmap");
        assert_eq!(imported.id, refreshed.id);
        assert_eq!(refreshed.title, "External");
        persist_workspace_title(
            &db,
            WorkspaceTitleTarget::Note(note.id),
            "Architecture".to_string(),
        )
        .await?;

        let renamed = note::Entity::find_by_id(note.id as i64)
            .one(&db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("renamed note was not found"))?;
        assert_eq!(renamed.title, "Architecture");
        let aliases = note_alias::Entity::find().all(&db).await?;
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].alias, "Design");

        let revision = load_change_revision(&db).await?;
        assert_eq!(revision.revision, 0);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "performance proof; run explicitly with one test thread"]
    async fn inactive_project_children_heap_benchmark() -> Result<()> {
        const EXCLUDED_NOTE_COUNT: usize = 64;
        const TITLE_BYTES: usize = 1024 * 1024;

        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let active_project = project::ActiveModel {
            name: Set("Active".to_string()),
            archived: Set(false),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let archived_project = project::ActiveModel {
            name: Set("Archived".to_string()),
            archived: Set(true),
            position: Set(1),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        note::ActiveModel {
            id: Set(1),
            title: Set("Visible note".to_string()),
            project_id: Set(Some(active_project.id)),
            file_path: Set(None),
            file_managed_by_app: Set(false),
            cached_content: Set(String::new()),
            file_missing_since: Set(None),
            created_at: Set(0),
            updated_at: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let title = "x".repeat(TITLE_BYTES);
        for index in 0..EXCLUDED_NOTE_COUNT {
            note::ActiveModel {
                id: Set(index as i64 + 2),
                title: Set(title.clone()),
                project_id: Set(Some(archived_project.id)),
                file_path: Set(None),
                file_managed_by_app: Set(false),
                cached_content: Set(String::new()),
                file_missing_since: Set(None),
                created_at: Set(index as i64 + 1),
                updated_at: Set(index as i64 + 1),
                ..Default::default()
            }
            .insert(&db)
            .await?;
        }

        drop(title);
        let allocation = test_alloc::start_measurement();
        let rows = load_workspace_rows(&db).await?;
        let allocation = allocation.finish();

        assert_eq!(rows.projects.len(), 1);
        assert_eq!(rows.notes.len(), 1);
        assert_eq!(rows.notes[0].title, "Visible note");
        println!(
            "excluded_note_title_bytes={} peak_heap_growth_bytes={} total_allocated_bytes={}",
            EXCLUDED_NOTE_COUNT * TITLE_BYTES,
            allocation.peak_growth_bytes,
            allocation.allocated_bytes,
        );

        Ok(())
    }
}
