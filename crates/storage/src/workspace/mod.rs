use anyhow::{Context as _, Result};
use entity::{
    board, board::Entity as Board, note, note::Entity as Note, project, project::Entity as Project,
};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, Condition, ConnectionTrait, DbBackend, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Statement, TransactionSession, TransactionTrait,
    sea_query::{Query, SelectStatement},
};

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub mod contracts;
pub mod file_import;
pub mod folder_import;
pub mod home;
pub mod links;
pub mod onboarding;
mod operations;
pub mod search;
pub mod trash;

pub use contracts as api;

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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceTitleUpdate {
    pub file_path: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangeRevision {
    pub revision: i64,
    pub board_revision: i64,
    pub note_revision: i64,
    pub link_revision: i64,
}

pub async fn create_project(
    db: &(impl ConnectionTrait + TransactionTrait),
    name: String,
) -> Result<ProjectRow, DbErr> {
    let position = Project::find().count(db).await? as i32;
    let project = project::ActiveModel {
        name: Set(name),
        archived: Set(false),
        position: Set(position),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(ProjectRow {
        id: project.id as u32,
        name: project.name,
        position: project.position,
    })
}

pub async fn rename_project(
    db: &(impl ConnectionTrait + TransactionTrait),
    project_id: u32,
    name: String,
) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    let current = Project::find_by_id(i64::from(project_id))
        .one(&txn)
        .await?
        .ok_or_else(|| DbErr::RecordNotFound("project".to_string()))?;
    if current.name != name {
        crate::workspace::links::record_reference_alias(
            &txn,
            crate::workspace::links::WorkspaceAliasTarget::Project(i64::from(project_id)),
            &current.name,
            now_ts(),
        )
        .await
        .map_err(|error| DbErr::Custom(error.to_string()))?;
    }
    project::ActiveModel {
        id: Set(i64::from(project_id)),
        name: Set(name),
        ..Default::default()
    }
    .update(&txn)
    .await?;
    txn.commit().await?;
    Ok(())
}

pub async fn move_board_to_project(
    db: &(impl ConnectionTrait + TransactionTrait),
    board_id: u32,
    project_id: Option<u32>,
) -> Result<(), DbErr> {
    board::ActiveModel {
        id: Set(i64::from(board_id)),
        project_id: Set(project_id.map(i64::from)),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(())
}

pub async fn move_note_to_project(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: u32,
    project_id: Option<u32>,
) -> Result<(), DbErr> {
    note::ActiveModel {
        id: Set(i64::from(note_id)),
        project_id: Set(project_id.map(i64::from)),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(())
}

pub async fn reorder_projects(
    db: &(impl ConnectionTrait + TransactionTrait),
    positions: Vec<(u32, i32)>,
) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    for (project_id, position) in positions {
        project::ActiveModel {
            id: Set(i64::from(project_id)),
            position: Set(position),
            ..Default::default()
        }
        .update(&txn)
        .await?;
    }
    txn.commit().await
}

fn visible_project_ids_query() -> SelectStatement {
    Query::select()
        .column(project::Column::Id)
        .from(Project)
        .and_where(project::Column::Archived.eq(false))
        .and_where(project::Column::DeletedAt.is_null())
        .to_owned()
}

pub async fn load_workspace_rows(
    db: &(impl ConnectionTrait + TransactionTrait),
) -> Result<WorkspaceRows> {
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
    db: &(impl ConnectionTrait + TransactionTrait),
    project_id: Option<u32>,
    title: String,
    file_path: String,
    content: String,
) -> Result<WorkspaceItem, DbErr> {
    let txn = db.begin().await?;
    let note = insert_managed_note(&txn, project_id, title, file_path, content.clone()).await?;
    crate::note::links::index_note_links_in_connection(&txn, note.id, &content, note.updated_at)
        .await
        .map_err(|error| DbErr::Custom(error.to_string()))?;
    txn.commit().await?;

    Ok(WorkspaceItem {
        id: note.id as u32,
        title: note.title,
    })
}

pub async fn create_managed_linked_note(
    db: &(impl ConnectionTrait + TransactionTrait),
    project_id: Option<u32>,
    title: String,
    file_path: String,
    content: String,
    item: crate::workspace::links::WorkspaceItemRef,
) -> Result<WorkspaceItem, DbErr> {
    let txn = db.begin().await?;
    let note = insert_managed_note(&txn, project_id, title, file_path, content.clone()).await?;
    crate::note::links::index_note_links_in_connection(&txn, note.id, &content, note.updated_at)
        .await
        .map_err(|error| DbErr::Custom(error.to_string()))?;
    crate::workspace::links::set_manual_note_link_in_connection(
        &txn,
        note.id,
        item,
        true,
        note.updated_at,
    )
    .await
    .map_err(|error| DbErr::Custom(error.to_string()))?;
    txn.commit().await?;

    Ok(WorkspaceItem {
        id: note.id as u32,
        title: note.title,
    })
}

async fn insert_managed_note(
    db: &impl ConnectionTrait,
    project_id: Option<u32>,
    title: String,
    file_path: String,
    content: String,
) -> Result<note::Model, DbErr> {
    let now = now_ts();
    note::ActiveModel {
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
    .await
}

pub async fn import_external_note(
    db: &(impl ConnectionTrait + TransactionTrait),
    title: String,
    file_path: String,
    content: String,
) -> Result<WorkspaceItem, DbErr> {
    let indexed_content = content.clone();
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
    crate::note::links::index_note_links(db, note.id, &indexed_content, note.updated_at)
        .await
        .map_err(|error| DbErr::Custom(error.to_string()))?;

    Ok(WorkspaceItem {
        id: note.id as u32,
        title: note.title,
    })
}

pub async fn create_board(
    db: &(impl ConnectionTrait + TransactionTrait),
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
    db: &(impl ConnectionTrait + TransactionTrait),
    target: WorkspaceTitleTarget,
    title: String,
) -> Result<WorkspaceTitleUpdate> {
    match target {
        WorkspaceTitleTarget::Board(board_id) => {
            let transaction = db.begin().await?;
            let current = Board::find_by_id(board_id as i64)
                .one(&transaction)
                .await?
                .ok_or_else(|| anyhow::anyhow!("board {board_id} was not found"))?;
            if current.title != title {
                crate::workspace::links::record_reference_alias(
                    &transaction,
                    crate::workspace::links::WorkspaceAliasTarget::Item(
                        crate::workspace::links::WorkspaceItemRef {
                            kind: crate::workspace::links::WorkspaceItemKind::Board,
                            id: board_id as i64,
                        },
                    ),
                    &current.title,
                    now_ts(),
                )
                .await
                .map_err(|error| DbErr::Custom(error.to_string()))?;
            }
            board::ActiveModel {
                id: Set(board_id as i64),
                title: Set(title),
                ..Default::default()
            }
            .update(&transaction)
            .await?;
            transaction.commit().await?;

            Ok(WorkspaceTitleUpdate::default())
        }
        WorkspaceTitleTarget::Note(note_id) => {
            let current = Note::find_by_id(note_id as i64)
                .one(db)
                .await?
                .ok_or_else(|| anyhow::anyhow!("note {note_id} was not found"))?;
            let now = now_ts();
            let old_path = current
                .file_managed_by_app
                .then_some(current.file_path.as_deref())
                .flatten()
                .map(PathBuf::from);
            let new_path = match old_path.as_deref() {
                Some(path) => Some(unique_renamed_note_path(path, &title).await?),
                None => current.file_path.as_deref().map(PathBuf::from),
            };
            let path_changed = old_path
                .as_deref()
                .zip(new_path.as_deref())
                .is_some_and(|(old_path, new_path)| !same_path(old_path, new_path));
            let file_moved = if path_changed && let Some(old_path) = old_path.as_deref() {
                tokio::fs::try_exists(old_path).await?
            } else {
                false
            };
            let transaction = db.begin().await?;

            if file_moved
                && let (Some(old_path), Some(new_path)) = (old_path.as_deref(), new_path.as_deref())
            {
                tokio::fs::rename(old_path, new_path)
                    .await
                    .with_context(|| {
                        format!(
                            "failed to rename managed note file from {} to {}",
                            old_path.display(),
                            new_path.display()
                        )
                    })?;
            }

            let update_result = async {
                if current.title != title {
                    crate::note::links::record_note_alias(
                        &transaction,
                        current.id,
                        &current.title,
                        now,
                    )
                    .await?;
                }
                note::ActiveModel {
                    id: Set(note_id as i64),
                    title: Set(title),
                    file_path: Set(new_path
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned())),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .update(&transaction)
                .await?;
                transaction.commit().await?;
                Ok::<(), anyhow::Error>(())
            }
            .await;

            if let Err(err) = update_result {
                if file_moved
                    && let (Some(old_path), Some(new_path)) =
                        (old_path.as_deref(), new_path.as_deref())
                    && let Err(rollback_err) = tokio::fs::rename(new_path, old_path).await
                {
                    return Err(err).context(format!(
                        "also failed to restore {} after the database update failed: {rollback_err}",
                        old_path.display()
                    ));
                }
                return Err(err);
            }

            Ok(WorkspaceTitleUpdate {
                file_path: new_path.map(|path| path.to_string_lossy().into_owned()),
            })
        }
    }
}

pub fn suggested_note_file_name(title: &str, extension: &str) -> String {
    let stem = if title.trim().is_empty() {
        "untitled"
    } else {
        title.trim()
    };
    let extension = extension.trim_start_matches('.');
    let mut file_name = String::with_capacity(stem.len() + extension.len() + 1);

    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            file_name.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            file_name.push('-');
        }
    }

    if file_name.is_empty() {
        file_name.push_str("untitled");
    }
    if !extension.is_empty() {
        file_name.push('.');
        file_name.push_str(extension);
    }
    file_name
}

async fn unique_renamed_note_path(current_path: &Path, title: &str) -> Result<PathBuf> {
    let parent = current_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("managed note path has no parent directory"))?;
    let extension = current_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("md");
    let file_name = suggested_note_file_name(title, extension);
    let candidate = parent.join(&file_name);
    if same_path(&candidate, current_path) || !tokio::fs::try_exists(&candidate).await? {
        return Ok(candidate);
    }

    let stem = Path::new(&file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("untitled");
    for index in 2.. {
        let candidate = parent.join(format!("{stem}-{index}.{extension}"));
        if same_path(&candidate, current_path) || !tokio::fs::try_exists(&candidate).await? {
            return Ok(candidate);
        }
    }

    unreachable!()
}

fn same_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

pub async fn load_change_revision(
    db: &(impl ConnectionTrait + TransactionTrait),
) -> Result<ChangeRevision, DbErr> {
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT revision, board_revision, note_revision, link_revision
             FROM castle_change_revision WHERE id = 1",
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("Castle change revision row is missing".to_string()))?;

    Ok(ChangeRevision {
        revision: row.try_get("", "revision")?,
        board_revision: row.try_get("", "board_revision")?,
        note_revision: row.try_get("", "note_revision")?,
        link_revision: row.try_get("", "link_revision")?,
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
    async fn renaming_managed_note_renames_its_file_and_avoids_collisions() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let directory = tempfile::tempdir()?;
        let original_path = directory.path().join("untitled-note-3.md");
        tokio::fs::write(&original_path, "# Original").await?;
        let note = create_managed_note(
            &db,
            None,
            "Untitled note".to_string(),
            original_path.to_string_lossy().into_owned(),
            "# Original".to_string(),
        )
        .await?;

        let update = persist_workspace_title(
            &db,
            WorkspaceTitleTarget::Note(note.id),
            "Something".to_string(),
        )
        .await?;
        let renamed_path = directory.path().join("something.md");
        let renamed_path_string = renamed_path.to_string_lossy().into_owned();
        assert_eq!(
            update.file_path.as_deref(),
            Some(renamed_path_string.as_str())
        );
        assert!(!original_path.exists());
        assert_eq!(
            tokio::fs::read_to_string(&renamed_path).await?,
            "# Original"
        );

        tokio::fs::write(directory.path().join("roadmap.md"), "Existing").await?;
        let update = persist_workspace_title(
            &db,
            WorkspaceTitleTarget::Note(note.id),
            "Roadmap".to_string(),
        )
        .await?;
        let collision_path = directory.path().join("roadmap-2.md");
        let collision_path_string = collision_path.to_string_lossy().into_owned();
        assert_eq!(
            update.file_path.as_deref(),
            Some(collision_path_string.as_str())
        );
        assert!(!renamed_path.exists());
        assert_eq!(
            tokio::fs::read_to_string(&collision_path).await?,
            "# Original"
        );

        let saved = Note::find_by_id(note.id as i64)
            .one(&db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("renamed note was not found"))?;
        assert_eq!(saved.title, "Roadmap");
        assert_eq!(
            saved.file_path.as_deref(),
            Some(collision_path_string.as_str())
        );
        Ok(())
    }

    #[tokio::test]
    async fn renaming_external_note_does_not_rename_its_file() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let directory = tempfile::tempdir()?;
        let external_path = directory.path().join("external-name.md");
        tokio::fs::write(&external_path, "External").await?;
        let note = import_external_note(
            &db,
            "External".to_string(),
            external_path.to_string_lossy().into_owned(),
            "External".to_string(),
        )
        .await?;

        let update = persist_workspace_title(
            &db,
            WorkspaceTitleTarget::Note(note.id),
            "Renamed externally".to_string(),
        )
        .await?;
        let external_path_string = external_path.to_string_lossy().into_owned();
        assert_eq!(
            update.file_path.as_deref(),
            Some(external_path_string.as_str())
        );
        assert!(external_path.exists());
        assert!(!directory.path().join("renamed-externally.md").exists());
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
