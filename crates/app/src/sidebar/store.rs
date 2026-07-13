use std::collections::HashMap;

use anyhow::Result;
use entity::{project, project::Entity as Project};
use gpui::{Context, SharedString};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, PaginatorTrait};

use crate::DB;
use crate::workspace_data::load_workspace_rows;

use super::{SidebarView, dto::*};

impl SidebarView {
    pub(crate) fn list_projects(&mut self, cx: &mut Context<Self>) {
        if self.projects_refreshing {
            self.projects_refresh_pending = true;
            return;
        }

        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        self.projects_refreshing = true;

        cx.spawn(async move |this, cx| {
            let rows = match runtime
                .spawn(async move { load_workspace_rows(db.as_ref()).await })
                .await
            {
                Ok(Ok(rows)) => rows,
                Ok(Err(err)) => {
                    eprintln!("Failed to load sidebar projects: {err}");
                    this.update(cx, |this, cx| {
                        this.projects_refreshing = false;
                        if std::mem::take(&mut this.projects_refresh_pending) {
                            this.list_projects(cx);
                        }
                    })
                    .ok();
                    return;
                }
                Err(err) => {
                    eprintln!("Failed to load sidebar projects: {err}");
                    this.update(cx, |this, cx| {
                        this.projects_refreshing = false;
                        if std::mem::take(&mut this.projects_refresh_pending) {
                            this.list_projects(cx);
                        }
                    })
                    .ok();
                    return;
                }
            };
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
                    is_pinned: board.is_pinned,
                    last_opened_at: board.last_opened_at,
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
                    is_pinned: note.is_pinned,
                    last_opened_at: note.last_opened_at,
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
                this.projects_refreshing = false;
                if let Some(first) = projects.first_mut() {
                    first.is_expanded = true;
                }

                this.projects = projects;
                this.standalone_boards = standalone_boards;
                this.standalone_notes = standalone_notes;
                if std::mem::take(&mut this.projects_refresh_pending) {
                    this.list_projects(cx);
                }
                cx.notify();
            })
            .ok();
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
