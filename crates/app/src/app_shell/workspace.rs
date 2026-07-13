use std::collections::HashMap;
use std::fs::{create_dir_all, read_to_string, write};

use super::*;
use crate::workspace_data::load_workspace_rows;

impl AppShell {
    pub(crate) fn refresh_workspace(&mut self, cx: &mut Context<Self>) {
        if self.workspace_refreshing {
            self.workspace_refresh_pending = true;
            return;
        }

        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        self.workspace_refreshing = true;

        cx.spawn(async move |this, cx| {
            let rows = match runtime
                .spawn(async move { load_workspace_rows(db.as_ref()).await })
                .await
            {
                Ok(Ok(rows)) => rows,
                Ok(Err(err)) => {
                    eprintln!("Failed to refresh workspace: {err}");
                    this.update(cx, |this, cx| {
                        this.workspace_refreshing = false;
                        if std::mem::take(&mut this.workspace_refresh_pending) {
                            this.refresh_workspace(cx);
                        }
                    })
                    .ok();
                    return;
                }
                Err(err) => {
                    eprintln!("Failed to refresh workspace: {err}");
                    this.update(cx, |this, cx| {
                        this.workspace_refreshing = false;
                        if std::mem::take(&mut this.workspace_refresh_pending) {
                            this.refresh_workspace(cx);
                        }
                    })
                    .ok();
                    return;
                }
            };

            let project_choices: Vec<ProjectChoice> = rows
                .projects
                .iter()
                .map(|project| ProjectChoice {
                    id: project.id,
                    name: SharedString::from(project.name.clone()),
                })
                .collect();

            let project_names: HashMap<u32, SharedString> = project_choices
                .iter()
                .map(|project| (project.id, project.name.clone()))
                .collect();

            let board_choices: Vec<BoardChoice> = rows
                .boards
                .into_iter()
                .map(|board| BoardChoice {
                    id: board.id,
                    title: SharedString::from(board.title),
                    project_id: board.project_id,
                    project_name: board
                        .project_id
                        .and_then(|project_id| project_names.get(&project_id).cloned()),
                })
                .collect();

            let note_choices: Vec<NoteChoice> = rows
                .notes
                .into_iter()
                .map(|note| NoteChoice {
                    id: note.id,
                    title: SharedString::from(note.title),
                    project_id: note.project_id,
                    project_name: note
                        .project_id
                        .and_then(|project_id| project_names.get(&project_id).cloned()),
                })
                .collect();

            this.update(cx, |this, cx| {
                this.workspace_refreshing = false;
                this.projects = project_choices;
                this.boards = board_choices;
                this.notes = note_choices;
                this.rebuild_command_palette_workspace_commands();
                if std::mem::take(&mut this.workspace_refresh_pending) {
                    this.refresh_workspace(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn create_note(
        &mut self,
        project_id: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.create_note_with_title(project_id, "Untitled note".to_string(), window, cx);
    }

    pub(crate) fn create_note_with_title(
        &mut self,
        project_id: Option<u32>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let now = now_ts();
        let view = cx.entity().downgrade();
        let path = unique_note_path(cx.global::<DB>().data_dir.join("notes"), &title);
        let path_string = path.display().to_string();
        let background_executor = cx.background_executor().clone();

        cx.spawn_in(window, async move |_, window| {
            let write_path = path.clone();
            background_executor
                .spawn(async move {
                    if let Some(parent) = write_path.parent() {
                        create_dir_all(parent)?;
                    }
                    write(write_path, DEFAULT_NOTE)
                })
                .await
                .ok()?;

            let inserted = note::ActiveModel {
                title: Set(title),
                project_id: Set(project_id.map(|id| id as i64)),
                file_path: Set(Some(path_string)),
                file_managed_by_app: Set(true),
                cached_content: Set(DEFAULT_NOTE.to_string()),
                file_missing_since: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&*db)
            .await
            .ok()?;

            window
                .update(|window, cx| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };

                    view.update(cx, |this, cx| {
                        this.open_note_tab(
                            inserted.id as u32,
                            project_id,
                            SharedString::from(inserted.title),
                            window,
                            cx,
                        );
                        this.sidebar
                            .update(cx, |sidebar, cx| sidebar.list_projects(cx));
                        this.refresh_workspace(cx);
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    pub(crate) fn open_note_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open note file".into()),
        });

        let background_executor = cx.background_executor().clone();
        let db = cx.global::<DB>().conn.clone();
        let view = cx.entity().downgrade();

        cx.spawn_in(window, async move |_, window| {
            let paths = paths.await.ok()?.ok()??;
            let path = paths.first()?.clone();
            let readable_path = path.clone();
            let content = background_executor
                .spawn(async move { read_to_string(readable_path) })
                .await
                .ok()?;

            let path_string = path.display().to_string();
            let title = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled note")
                .to_string();

            let existing = Note::find()
                .filter(note::Column::FilePath.eq(path_string.clone()))
                .one(&*db)
                .await
                .ok()
                .flatten();

            let (note_id, note_title) = if let Some(existing) = existing {
                note::ActiveModel {
                    id: Set(existing.id),
                    file_path: Set(Some(path_string)),
                    cached_content: Set(content),
                    file_missing_since: Set(None),
                    updated_at: Set(now_ts()),
                    ..Default::default()
                }
                .update(&*db)
                .await
                .ok()?;

                (existing.id as u32, existing.title)
            } else {
                let now = now_ts();
                let inserted = note::ActiveModel {
                    title: Set(title),
                    project_id: Set(None),
                    file_path: Set(Some(path_string)),
                    file_managed_by_app: Set(false),
                    cached_content: Set(content),
                    file_missing_since: Set(None),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .insert(&*db)
                .await
                .ok()?;

                (inserted.id as u32, inserted.title)
            };

            window
                .update(|window, cx| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };

                    view.update(cx, |this, cx| {
                        this.open_note_tab(
                            note_id,
                            None,
                            SharedString::from(note_title),
                            window,
                            cx,
                        );
                        this.sidebar
                            .update(cx, |sidebar, cx| sidebar.list_projects(cx));
                        this.refresh_workspace(cx);
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    pub(super) fn create_board(
        &mut self,
        project_id: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.create_board_with_title(project_id, "Board".to_string(), window, cx);
    }

    pub(crate) fn create_board_with_title(
        &mut self,
        project_id: Option<u32>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let view = cx.entity().downgrade();

        cx.spawn_in(window, async move |_, window| {
            let inserted = board::ActiveModel {
                title: Set(title),
                project_id: Set(project_id.map(|id| id as i64)),
                ..Default::default()
            }
            .insert(&*db)
            .await
            .ok()?;

            window
                .update(|window, cx| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };

                    view.update(cx, |this, cx| {
                        this.open_board_tab(
                            inserted.id as u32,
                            project_id,
                            SharedString::from(inserted.title),
                            window,
                            cx,
                        );
                        this.sidebar
                            .update(cx, |sidebar, cx| sidebar.list_projects(cx));
                        this.refresh_workspace(cx);
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }
}
