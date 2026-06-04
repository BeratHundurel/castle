use std::collections::HashMap;

use anyhow::Result;
use entity::{project, project::Entity as Project};
use gpui::{Context, SharedString};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, PaginatorTrait};

use crate::DB;
use crate::workspace_data::load_workspace_rows;

use super::{SidebarView, dto::*};

impl SidebarView {
    pub(crate) fn list_projects(cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let rows = load_workspace_rows(db.as_ref()).await?;
            let mut projects: Vec<ProjectDTO> = rows
                .projects
                .into_iter()
                .map(|project| ProjectDTO {
                    id: project.id,
                    name: SharedString::from(project.name),
                    position: project.position,
                    is_expanded: false,
                    boards: vec![],
                    notes: vec![],
                })
                .collect();

            for (index, project) in projects.iter_mut().enumerate() {
                if project.position == 0 {
                    project.position = index as i32;
                }
            }

            let project_indexes: HashMap<u32, usize> = projects
                .iter()
                .enumerate()
                .map(|(index, project)| (project.id, index))
                .collect();

            let mut standalone_boards = Vec::new();
            for board in rows.boards {
                let dto = BoardDTO {
                    id: board.id,
                    title: SharedString::from(board.title),
                    project_id: board.project_id,
                };

                if let Some(project_index) = dto
                    .project_id
                    .and_then(|id| project_indexes.get(&id).copied())
                {
                    projects[project_index].boards.push(dto);
                } else if dto.project_id.is_none() {
                    standalone_boards.push(dto);
                }
            }

            let mut standalone_notes = Vec::new();
            for note in rows.notes {
                let dto = NoteDTO {
                    id: note.id,
                    title: SharedString::from(note.title),
                    project_id: note.project_id,
                };

                if let Some(project_index) = dto
                    .project_id
                    .and_then(|id| project_indexes.get(&id).copied())
                {
                    projects[project_index].notes.push(dto);
                } else if dto.project_id.is_none() {
                    standalone_notes.push(dto);
                }
            }

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
            let position = Project::find().count(&*db).await? as i32;
            let project_entity = project::ActiveModel {
                name: Set(name),
                archived: Set(false),
                position: Set(position),
                ..Default::default()
            }
            .insert(&*db)
            .await?;

            this.update(cx, |this, cx| {
                this.projects.push(ProjectDTO {
                    id: project_entity.id as u32,
                    name: SharedString::from(project_entity.name),
                    position: project_entity.position,
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
}
