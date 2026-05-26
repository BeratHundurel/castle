use anyhow::Result;
use entity::{
    board, board::Entity as Board, note, note::Entity as Note, project, project::Entity as Project,
};
use gpui::{Context, SharedString};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

use crate::DB;

use super::{SidebarView, dto::*};

impl SidebarView {
    pub(crate) fn list_projects(cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let results = Project::load().with(Board).with(Note).all(&*db).await?;
            let standalone_boards = Board::find()
                .filter(board::Column::ProjectId.is_null())
                .all(&*db)
                .await?;

            let standalone_notes = Note::find()
                .filter(note::Column::ProjectId.is_null())
                .all(&*db)
                .await?;

            let mut projects: Vec<ProjectDTO> = results.into_iter().map(ProjectDTO::from).collect();

            let standalone_boards: Vec<BoardDTO> =
                standalone_boards.into_iter().map(BoardDTO::from).collect();

            let standalone_notes: Vec<NoteDTO> =
                standalone_notes.into_iter().map(NoteDTO::from).collect();

            this.update(cx, |this, cx| {
                if let Some(first) = projects.first_mut() {
                    first.is_expanded = true;
                }

                this.projects = projects;
                this.standalone_boards = standalone_boards;
                this.standalone_notes = standalone_notes;
                cx.notify();
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    pub(super) fn add_project(&mut self, cx: &mut Context<Self>, name: String) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let project_entity = project::ActiveModel {
                name: Set(name),
                ..Default::default()
            }
            .insert(&*db)
            .await?;

            this.update(cx, |this, cx| {
                this.projects.push(ProjectDTO {
                    id: project_entity.id as u32,
                    name: SharedString::from(project_entity.name),
                    is_expanded: true,
                    boards: vec![],
                    notes: vec![],
                });
                cx.notify();
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    pub(super) fn add_board(
        &mut self,
        cx: &mut Context<Self>,
        project_id: Option<u32>,
        title: String,
    ) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let inserted = board::ActiveModel {
                title: Set(title),
                project_id: Set(project_id.map(|id| id as i64)),
                ..Default::default()
            }
            .insert(&*db)
            .await?;

            this.update(cx, |this, cx| {
                let board = BoardDTO {
                    id: inserted.id as u32,
                    title: SharedString::from(inserted.title),
                    project_id,
                };
                if let Some(project_id) = project_id
                    && let Some(project) = this.projects.iter_mut().find(|p| p.id == project_id)
                {
                    project.boards.push(board.clone());
                } else {
                    this.standalone_boards.push(board.clone());
                }
                this.select_board(board.id, project_id, board.title, cx);
                cx.notify();
            })
            .ok();

            Ok(())
        })
        .detach();
    }
}
