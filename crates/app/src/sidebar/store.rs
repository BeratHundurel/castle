use std::collections::HashMap;

use entity::{project, project::Entity as Project};
use gpui::{Context, PathPromptOptions, SharedString, Window};
use gpui_component::{WindowExt as _, notification::Notification};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, PaginatorTrait};

use crate::DB;
use crate::document_editor::DocumentKind;
use std::path::Path;

use super::{SidebarView, dto::*};

impl SidebarView {
    pub(crate) fn apply_workspace_rows(
        &mut self,
        rows: &crate::workspace_data::WorkspaceRows,
        cx: &mut Context<Self>,
    ) {
        let mut projects: Vec<ProjectDTO> = rows
            .projects
            .iter()
            .map(|project| ProjectDTO {
                id: project.id,
                name: SharedString::from(project.name.as_str()),
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
        for board in &rows.boards {
            let dto = BoardDTO {
                id: board.id,
                title: SharedString::from(board.title.as_str()),
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
        for note in &rows.notes {
            let dto = NoteDTO {
                id: note.id,
                title: SharedString::from(note.title.as_str()),
                project_id: note.project_id,
                kind: DocumentKind::from_path(note.file_path.as_deref().map(Path::new)),
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

        if let Some(first) = projects.first_mut() {
            first.is_expanded = true;
        }

        self.projects = projects;
        self.standalone_boards = standalone_boards;
        self.standalone_notes = standalone_notes;
        cx.notify();
    }

    pub(crate) fn request_workspace_refresh(&mut self, cx: &mut Context<Self>) {
        cx.emit(super::SidebarEvent::WorkspaceChanged);
    }

    pub(super) fn add_project(&mut self, cx: &mut Context<Self>, name: String) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();

        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    let position = Project::find().count(&*db).await? as i32;
                    project::ActiveModel {
                        name: Set(name),
                        archived: Set(false),
                        position: Set(position),
                        ..Default::default()
                    }
                    .insert(&*db)
                    .await
                })
                .await;

            this.update(cx, |this, cx| match result {
                Ok(Ok(project)) => {
                    this.projects.push(ProjectDTO {
                        id: project.id as u32,
                        name: SharedString::from(project.name),
                        position: project.position,
                        is_expanded: true,
                        boards: vec![],
                        notes: vec![],
                    });
                    this.request_workspace_refresh(cx);
                    cx.notify();
                }
                Ok(Err(err)) => eprintln!("Failed to add project: {err}"),
                Err(err) => eprintln!("Failed to join project creation task: {err}"),
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn add_folder_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add folder as project".into()),
        });
        let background_executor = cx.background_executor().clone();
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();

        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = paths.await.ok().and_then(Result::ok).flatten() else {
                return;
            };
            let Some(path) = paths.first().cloned() else {
                return;
            };

            let scan = background_executor
                .spawn(async move { crate::folder_import::scan_folder(&path) })
                .await;
            let scan = match scan {
                Ok(scan) => scan,
                Err(err) => {
                    this.update_in(cx, |_, window, cx| {
                        window.push_notification(
                            Notification::error(format!("Could not scan the folder: {err}")),
                            cx,
                        );
                    })
                    .ok();
                    return;
                }
            };

            let result = runtime
                .spawn(async move { crate::folder_import::import_folder(db.as_ref(), scan).await })
                .await;

            this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(result)) => {
                    this.request_workspace_refresh(cx);
                    let action = if result.created_project {
                        "Added"
                    } else {
                        "Refreshed"
                    };
                    let mut message = format!(
                        "{action} {}: {} new, {} refreshed",
                        result.project_name, result.inserted, result.updated
                    );
                    if result.skipped > 0 {
                        message.push_str(&format!(", {} skipped", result.skipped));
                    }
                    window.push_notification(Notification::success(message), cx);
                }
                Ok(Err(err)) => window.push_notification(
                    Notification::error(format!("Could not add the folder project: {err}")),
                    cx,
                ),
                Err(err) => window.push_notification(
                    Notification::error(format!("Folder import task failed: {err}")),
                    cx,
                ),
            })
            .ok();
        })
        .detach();
    }
}
