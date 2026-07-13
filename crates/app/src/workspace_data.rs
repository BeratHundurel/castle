use anyhow::Result;
use entity::{
    board, board::Entity as Board, note, note::Entity as Note, project, project::Entity as Project,
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectRow {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) position: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoardRow {
    pub(crate) id: u32,
    pub(crate) title: String,
    pub(crate) project_id: Option<u32>,
    pub(crate) is_pinned: bool,
    pub(crate) last_opened_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoteRow {
    pub(crate) id: u32,
    pub(crate) title: String,
    pub(crate) project_id: Option<u32>,
    pub(crate) is_pinned: bool,
    pub(crate) last_opened_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceRows {
    pub(crate) projects: Vec<ProjectRow>,
    pub(crate) boards: Vec<BoardRow>,
    pub(crate) notes: Vec<NoteRow>,
}

pub(crate) async fn load_workspace_rows(db: &DatabaseConnection) -> Result<WorkspaceRows> {
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

    let active_project_ids = projects
        .iter()
        .map(|project| project.id)
        .collect::<HashSet<_>>();

    let boards = Board::find()
        .filter(board::Column::DeletedAt.is_null())
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
        .filter(|(_, _, project_id, _, _)| {
            project_id.is_none_or(|id| active_project_ids.contains(&(id as u32)))
        })
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
        .order_by_asc(note::Column::Id)
        .select_only()
        .column(note::Column::Id)
        .column(note::Column::Title)
        .column(note::Column::ProjectId)
        .column(note::Column::IsPinned)
        .column(note::Column::LastOpenedAt)
        .into_tuple::<(i64, String, Option<i64>, bool, Option<i64>)>()
        .all(db)
        .await?
        .into_iter()
        .filter(|(_, _, project_id, _, _)| {
            project_id.is_none_or(|id| active_project_ids.contains(&(id as u32)))
        })
        .map(
            |(id, title, project_id, is_pinned, last_opened_at)| NoteRow {
                id: id as u32,
                title,
                project_id: project_id.map(|id| id as u32),
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

#[cfg(test)]
mod tests {
    use super::*;
    use entity::{note, project};
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
}
