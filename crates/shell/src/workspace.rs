use std::cell::Cell;
use std::fs::{create_dir_all, remove_file, write};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
};

use super::*;
use gpui::relative;
use gpui_component::{
    WindowExt as _,
    dialog::{
        DialogAction, DialogClose, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
    },
    input::Input,
    notification::Notification,
};
use runtime::AppRuntime;
use storage::workspace::ChangeRevision;
use storage::workspace::load_workspace_rows;

const EXTERNAL_CHANGE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(750);

impl AppShell {
    pub(crate) fn start_note_link_reindex(&mut self, cx: &mut Context<Self>) {
        let app_runtime = cx.global::<AppRuntime>().clone();
        let db = app_runtime.store();
        cx.spawn(async move |this, cx| {
            let result = app_runtime
                .spawn_tokio(cx.background_executor(), async move {
                    storage::workspace::links::repair_workspace_link_index_batch(&db, 32).await
                })
                .await;
            match result {
                Ok(Ok(batch)) if batch.has_more => {
                    this.update(cx, |this, cx| this.start_note_link_reindex(cx))
                        .ok();
                }
                Ok(Ok(_)) => {}
                Ok(Err(error)) => eprintln!("Failed to index note links: {error}"),
                Err(error) => eprintln!("Failed to join note-link indexing task: {error}"),
            }
        })
        .detach();
    }

    pub(crate) fn start_external_change_watcher(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let store = cx.global::<AppRuntime>().store();
        let (revision_sender, mut revision_receiver) = tokio::sync::watch::channel(None);

        cx.global::<AppRuntime>()
            .spawn_tokio_detached(watch_change_revisions(
                store,
                revision_sender,
                EXTERNAL_CHANGE_POLL_INTERVAL,
            ));

        self.external_changes.task = Some(cx.spawn_in(window, async move |this, cx| {
            while revision_receiver.changed().await.is_ok() {
                let Some(revision) = *revision_receiver.borrow_and_update() else {
                    continue;
                };

                if this
                    .update_in(cx, |this, window, cx| {
                        let changed = this
                            .external_changes
                            .revision
                            .is_some_and(|previous| previous != revision.revision);
                        let board_changed = this
                            .external_changes
                            .board_revision
                            .is_some_and(|previous| previous != revision.board_revision);
                        let note_changed = this
                            .external_changes
                            .note_revision
                            .is_some_and(|previous| previous != revision.note_revision);
                        let link_changed = this
                            .external_changes
                            .link_revision
                            .is_some_and(|previous| previous != revision.link_revision);
                        this.external_changes.revision = Some(revision.revision);
                        this.external_changes.board_revision = Some(revision.board_revision);
                        this.external_changes.note_revision = Some(revision.note_revision);
                        this.external_changes.link_revision = Some(revision.link_revision);
                        if changed {
                            this.refresh_after_external_change(
                                board_changed || link_changed,
                                note_changed || link_changed,
                                window,
                                cx,
                            );
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn refresh_after_external_change(
        &mut self,
        board_changed: bool,
        note_changed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if board_changed {
            let board_views = self
                .tabs
                .open_tabs
                .iter()
                .filter_map(|tab| match &tab.kind {
                    OpenTabKind::Board { board_id, view, .. } => Some((*board_id, view.clone())),
                    _ => None,
                })
                .collect::<Vec<_>>();

            for (board_id, view) in board_views {
                view.update(cx, |board, cx| board.reload_board(board_id, cx));
            }
            for view in self.tabs.note_views.values() {
                view.update(cx, |note, cx| note.reload_board_embeds(cx));
            }
        }

        if note_changed {
            let note_views = self.tabs.note_views.values().cloned().collect::<Vec<_>>();
            for view in note_views {
                view.update(cx, |note, cx| note.reload_after_external_change(window, cx));
            }
        }
        self.refresh_workspace(cx);
        self.load_home(cx);
        self.load_trash(cx);
    }

    pub(crate) fn refresh_workspace(&mut self, cx: &mut Context<Self>) {
        if self.workspace.refreshing {
            self.workspace.refresh_pending = true;
            return;
        }

        let app_runtime = cx.global::<AppRuntime>().clone();
        let db = app_runtime.store();
        self.workspace.refreshing = true;

        cx.spawn(async move |this, cx| {
            let rows = match app_runtime
                .spawn_tokio(cx.background_executor(), async move {
                    load_workspace_rows(&db).await
                })
                .await
            {
                Ok(Ok(rows)) => rows,
                Ok(Err(err)) => {
                    eprintln!("Failed to refresh workspace: {err}");
                    this.update(cx, |this, cx| {
                        this.workspace.refreshing = false;
                        if std::mem::take(&mut this.workspace.refresh_pending) {
                            this.refresh_workspace(cx);
                        }
                    })
                    .ok();
                    return;
                }
                Err(err) => {
                    eprintln!("Failed to refresh workspace: {err}");
                    this.update(cx, |this, cx| {
                        this.workspace.refreshing = false;
                        if std::mem::take(&mut this.workspace.refresh_pending) {
                            this.refresh_workspace(cx);
                        }
                    })
                    .ok();
                    return;
                }
            };

            let Ok(should_apply) = this.update(cx, |this, cx| {
                if std::mem::take(&mut this.workspace.refresh_pending) {
                    this.workspace.refreshing = false;
                    this.refresh_workspace(cx);
                    false
                } else {
                    true
                }
            }) else {
                return;
            };
            if !should_apply {
                return;
            }

            let Ok((sidebar, command_palette)) = this.read_with(cx, |this, _| {
                (this.sidebar.clone(), this.command_palette.clone())
            }) else {
                return;
            };

            sidebar.update(cx, |sidebar, cx| {
                sidebar.apply_workspace_rows(&rows, cx);
            });
            command_palette.update(cx, |palette, cx| palette.apply_workspace_rows(&rows, cx));

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
                let note_titles: HashMap<u32, SharedString> = note_choices
                    .iter()
                    .map(|note| (note.id, note.title.clone()))
                    .collect();
                let mut tab_titles_changed = false;
                for tab in &mut this.tabs.open_tabs {
                    let OpenTabKind::Note { note_id, .. } = &tab.kind else {
                        continue;
                    };
                    let Some(title) = note_titles.get(note_id) else {
                        continue;
                    };
                    if tab.title != *title {
                        tab.title.clone_from(title);
                        tab_titles_changed = true;
                    }
                }

                this.workspace.refreshing = false;
                this.workspace.projects = project_choices;
                this.workspace.boards = board_choices;
                this.workspace.notes = note_choices;
                if tab_titles_changed {
                    this.persist_tab_session(cx);
                }
                if std::mem::take(&mut this.workspace.refresh_pending) {
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
        let app_runtime = cx.global::<AppRuntime>().clone();
        let view = cx.entity().downgrade();
        let path = unique_note_path(cx.global::<AppRuntime>().data_dir().join("notes"), &title);
        let path_string = path.display().to_string();

        cx.spawn_in(window, async move |_, window| {
            let write_path = path.clone();
            window
                .background_executor()
                .spawn(async move {
                    if let Some(parent) = write_path.parent() {
                        create_dir_all(parent)?;
                    }
                    write(write_path, DEFAULT_NOTE)
                })
                .await
                .ok()?;

            let inserted = app_runtime
                .spawn_store(window.background_executor(), move |store| async move {
                    storage::workspace::create_managed_note(
                        &store,
                        project_id,
                        title,
                        path_string,
                        DEFAULT_NOTE.to_string(),
                    )
                    .await
                })
                .await
                .ok()?
                .ok()?;

            window
                .update(|window, cx| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };

                    view.update(cx, |this, cx| {
                        this.open_note_tab(
                            inserted.id,
                            project_id,
                            SharedString::from(inserted.title),
                            window,
                            cx,
                        );
                        this.refresh_workspace(cx);
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    pub(crate) fn create_linked_note(
        &mut self,
        project_id: Option<u32>,
        title: String,
        item: storage::workspace::links::WorkspaceItemRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let source_title = title.clone();
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Note title")
                .default_value(title)
        });
        let dialog_input = title_input.clone();
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .w(px(520.))
                .on_ok({
                    let app = app.clone();
                    let input = dialog_input.clone();
                    let source_title = source_title.clone();
                    move |_, window, cx| {
                        let title = input.read(cx).value().trim().to_string();
                        if title.is_empty() {
                            window
                                .push_notification(Notification::error("Enter a note title."), cx);
                            return false;
                        }
                        app.update(cx, |this, cx| {
                            this.create_linked_note_with_title(
                                project_id,
                                title,
                                source_title.clone(),
                                item,
                                window,
                                cx,
                            );
                        });
                        true
                    }
                })
                .child(
                    DialogHeader::new()
                        .child(DialogTitle::new().child("Create linked note"))
                        .child(
                            DialogDescription::new()
                                .child("The note starts with a readable link back to this item."),
                        ),
                )
                .child(v_flex().py_3().child(Input::new(&dialog_input)))
                .child(
                    DialogFooter::new()
                        .child(
                            DialogClose::new().child(
                                Button::new("cancel-create-linked-note")
                                    .label("Cancel")
                                    .outline(),
                            ),
                        )
                        .child(
                            DialogAction::new().child(
                                Button::new("confirm-create-linked-note")
                                    .label("Create note")
                                    .primary(),
                            ),
                        ),
                )
        });
        title_input.update(cx, |input, cx| input.focus(window, cx));
    }

    fn create_linked_note_with_title(
        &mut self,
        project_id: Option<u32>,
        title: String,
        source_title: String,
        item: storage::workspace::links::WorkspaceItemRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app_runtime = cx.global::<AppRuntime>().clone();
        let view = cx.entity().downgrade();
        let path = unique_note_path(cx.global::<AppRuntime>().data_dir().join("notes"), &title);
        let path_string = path.display().to_string();
        let display_title = title.replace(['\r', '\n', '|'], " ");
        let fallback_link = storage::workspace::links::stable_workspace_link(item, &source_title);

        cx.spawn_in(window, async move |_, window| {
            let source_link = match app_runtime
                .spawn_store(window.background_executor(), move |store| async move {
                    storage::workspace::links::load_workspace_reference_catalog(&store).await
                })
                .await
            {
                Ok(Ok(catalog)) => catalog
                    .format_item_link(item, None)
                    .unwrap_or_else(|| fallback_link.clone()),
                _ => fallback_link,
            };
            let content = format!(
                "# {display_title}\n\nRelated {}: {source_link}\n",
                item.kind.as_str(),
            );
            let write_path = path.clone();
            let write_content = content.clone();
            if window
                .background_executor()
                .spawn(async move {
                    if let Some(parent) = write_path.parent() {
                        create_dir_all(parent)?;
                    }
                    write(write_path, write_content)
                })
                .await
                .is_err()
            {
                window
                    .update(|window, cx| {
                        window.push_notification(
                            Notification::error("Could not create the linked note file"),
                            cx,
                        );
                    })
                    .ok();
                return None;
            }

            let result = app_runtime
                .spawn_store(window.background_executor(), move |store| async move {
                    let inserted = storage::workspace::create_managed_linked_note(
                        &store,
                        project_id,
                        title,
                        path_string,
                        content,
                        item,
                    )
                    .await?;
                    let board_id = storage::workspace::links::load_workspace_link_catalog(
                        &store,
                    )
                    .await?
                    .into_iter()
                    .find(|entry| entry.item == item)
                    .and_then(|entry| entry.board_id)
                    .and_then(|id| u32::try_from(id).ok());
                    Ok::<_, anyhow::Error>((inserted, board_id))
                })
                .await;

            let cleanup_error = if matches!(&result, Ok(Err(_)) | Err(_)) {
                let cleanup_path = path.clone();
                window
                    .background_executor()
                    .spawn(async move { remove_linked_note_file(&cleanup_path) })
                    .await
                    .err()
                    .map(|error| error.to_string())
            } else {
                None
            };

            window
                .update(|window, cx| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };
                    view.update(cx, |this, cx| match result {
                        Ok(Ok((inserted, board_id))) => {
                            if let Some(board_id) = board_id {
                                for tab in &this.tabs.open_tabs {
                                    if let OpenTabKind::Board {
                                        board_id: open_board_id,
                                        view,
                                        ..
                                    } = &tab.kind
                                        && *open_board_id == board_id
                                    {
                                        view.update(cx, |board, cx| {
                                            board.reload_board(board_id, cx)
                                        });
                                    }
                                }
                            }
                            this.open_note_tab(
                                inserted.id,
                                project_id,
                                SharedString::from(inserted.title),
                                window,
                                cx,
                            );
                            this.refresh_workspace(cx);
                        }
                        Ok(Err(error)) => window.push_notification(
                            Notification::error(match cleanup_error.as_deref() {
                                Some(cleanup_error) => format!(
                                    "Could not create linked note: {error}. The file at {} could not be removed: {cleanup_error}",
                                    path.display()
                                ),
                                None => format!("Could not create linked note: {error}"),
                            }),
                            cx,
                        ),
                        Err(error) => window.push_notification(
                            Notification::error(match cleanup_error.as_deref() {
                                Some(cleanup_error) => format!(
                                    "Linked note task failed: {error}. The file at {} could not be removed: {cleanup_error}",
                                    path.display()
                                ),
                                None => format!("Linked note task failed: {error}"),
                            }),
                            cx,
                        ),
                    });
                })
                .ok()?;
            Some(())
        })
        .detach();
    }

    pub(crate) fn import_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import file".into()),
        });

        let app_runtime = cx.global::<AppRuntime>().clone();
        let view = cx.entity().downgrade();

        cx.spawn_in(window, async move |_, window| {
            let Some(paths) = paths.await.ok().and_then(Result::ok).flatten() else {
                return;
            };
            let Some(path) = paths.first().cloned() else {
                return;
            };
            let display_path = path.clone();
            let file = match window
                .background_executor()
                .spawn(async move { storage::workspace::file_import::scan_file(&path) })
                .await
            {
                Ok(file) => file,
                Err(err) => {
                    let message = format!("Could not import {}: {err}", display_path.display());
                    window
                        .update(|window, cx| {
                            window.push_notification(Notification::error(message), cx);
                        })
                        .ok();
                    return;
                }
            };

            let persisted = app_runtime
                .spawn_store(window.background_executor(), move |store| async move {
                    storage::workspace::file_import::import_file(&store, file).await
                })
                .await;

            let note = match persisted {
                Ok(Ok(note)) => note,
                Ok(Err(err)) => {
                    let message = format!("Could not import the file: {err}");
                    window
                        .update(|window, cx| {
                            window.push_notification(Notification::error(message), cx);
                        })
                        .ok();
                    return;
                }
                Err(err) => {
                    let message = format!("Could not finish importing the file: {err}");
                    window
                        .update(|window, cx| {
                            window.push_notification(Notification::error(message), cx);
                        })
                        .ok();
                    return;
                }
            };

            window
                .update(|window, cx| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };

                    view.update(cx, |this, cx| {
                        this.open_note_tab(
                            note.note_id,
                            note.project_id,
                            SharedString::from(note.title),
                            window,
                            cx,
                        );
                        this.refresh_workspace(cx);
                    });
                })
                .ok();
        })
        .detach();
    }

    pub(crate) fn export_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace_archive_busy {
            return;
        }
        let settings_json = match AppSettings::export_json(cx) {
            Ok(settings_json) => settings_json,
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Could not prepare workspace export: {error}")),
                    cx,
                );
                return;
            }
        };
        let data_dir = cx.global::<AppRuntime>().data_dir_handle();
        let receiver =
            cx.prompt_for_new_path(data_dir.as_path(), Some("castle-workspace.castle.zip"));
        let view = cx.entity().downgrade();
        let app_runtime = cx.global::<AppRuntime>().clone();
        self.workspace_archive_busy = true;
        cx.notify();

        cx.spawn_in(window, async move |_, window| {
            let Some(destination) = receiver.await.ok().and_then(Result::ok).flatten() else {
                let view = view.clone();
                window
                    .update(|_, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.workspace_archive_busy = false;
                            cx.notify();
                        });
                    })
                    .ok();
                return;
            };
            let display_destination = destination.display().to_string();
            let result = app_runtime
                .spawn_store(window.background_executor(), move |store| async move {
                    storage::workspace::archive::export_workspace(
                        &store,
                        data_dir.as_path(),
                        &settings_json,
                        &destination,
                    )
                    .await
                })
                .await;

            window
                .update(|window, cx| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };
                    view.update(cx, |this, cx| {
                        this.workspace_archive_busy = false;
                        let notification = match result {
                            Ok(Ok(summary)) => {
                                let attachment_note = if summary.missing_attachments == 0 {
                                    String::new()
                                } else {
                                    format!(
                                        " {} attachment files were missing.",
                                        summary.missing_attachments
                                    )
                                };
                                Notification::success(format!(
                                    "Exported {} notes, {} boards, and {} projects to {}.{}",
                                    summary.counts.notes,
                                    summary.counts.boards,
                                    summary.counts.projects,
                                    display_destination,
                                    attachment_note
                                ))
                            }
                            Ok(Err(error)) => Notification::error(format!(
                                "Could not export the workspace: {error}"
                            )),
                            Err(error) => Notification::error(format!(
                                "Could not finish exporting the workspace: {error}"
                            )),
                        };
                        window.push_notification(notification, cx);
                        cx.notify();
                    })
                })
                .ok();
        })
        .detach();
    }

    pub(crate) fn import_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace_archive_busy || window.has_active_dialog(cx) {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import workspace archive".into()),
        });
        let view = cx.entity().downgrade();

        cx.spawn_in(window, async move |_, window| {
            let Some(paths) = paths.await.ok().and_then(Result::ok).flatten() else {
                return;
            };
            let Some(archive_path) = paths.first().cloned() else {
                return;
            };
            window
                .update(|window, cx| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };
                    view.update(cx, |this, cx| {
                        this.open_workspace_import_dialog(archive_path, window, cx);
                    });
                })
                .ok();
        })
        .detach();
    }

    fn open_workspace_import_dialog(
        &mut self,
        archive_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_archive_busy || window.has_active_dialog(cx) {
            return;
        }
        let archive_name = archive_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("workspace archive")
            .to_string();
        let app = cx.entity();
        let merge_app = app.clone();
        let merge_path = archive_path.clone();
        let replace_app = app;
        let replace_path = archive_path;
        let import_submitted = Rc::new(Cell::new(false));

        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .w(px(560.))
                .child(
                    DialogHeader::new()
                        .child(DialogTitle::new().child("Import workspace"))
                        .child(DialogDescription::new().child(format!(
                            "Choose how to add {archive_name} to this Castle installation."
                        ))),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .py_3()
                        .child(
                            div()
                                .text_sm()
                                .child("Merge")
                                .child(
                                    div()
                                        .text_xs()
                                        .child("Keep the current workspace and add the archive contents."),
                                ),
                        )
                        .child(
                            div()
                                .text_sm()
                                .child("Replace workspace")
                                .child(
                                    div()
                                        .text_xs()
                                        .child("Clear current workspace data, including starter content, then restore the archive."),
                                ),
                        ),
                )
                .child(
                    DialogFooter::new()
                        .justify_between()
                        .child(
                            div().flex_none().child(
                                DialogClose::new().child(
                                    Button::new("cancel-import-workspace")
                                        .child(footer_action_label("Cancel"))
                                        .outline(),
                                ),
                            ),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new("merge-import-workspace")
                                        .child(footer_action_label("Merge"))
                                        .primary()
                                        .on_click({
                                            let merge_app = merge_app.clone();
                                            let merge_path = merge_path.clone();
                                            let import_submitted = import_submitted.clone();
                                            move |_, window, cx| {
                                                if !claim_workspace_import_submission(
                                                    &import_submitted,
                                                ) {
                                                    return;
                                                }
                                                window.close_dialog(cx);
                                                merge_app.update(cx, |this, cx| {
                                                    this.start_workspace_import(
                                                        merge_path.clone(),
                                                        storage::workspace::archive::ImportMode::Merge,
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("replace-import-workspace")
                                        .child(footer_action_label("Replace workspace"))
                                        .danger()
                                        .on_click({
                                            let replace_app = replace_app.clone();
                                            let replace_path = replace_path.clone();
                                            let import_submitted = import_submitted.clone();
                                            move |_, window, cx| {
                                                if !claim_workspace_import_submission(
                                                    &import_submitted,
                                                ) {
                                                    return;
                                                }
                                                window.close_dialog(cx);
                                                replace_app.update(cx, |this, cx| {
                                                    this.start_workspace_import(
                                                        replace_path.clone(),
                                                        storage::workspace::archive::ImportMode::Replace,
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            }
                                        }),
                                ),
                        ),
                )
        });
    }

    fn start_workspace_import(
        &mut self,
        archive_path: PathBuf,
        mode: storage::workspace::archive::ImportMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_archive_busy {
            return;
        }
        self.workspace_archive_busy = true;
        cx.notify();
        let display_path = archive_path.display().to_string();
        let data_dir = cx.global::<AppRuntime>().data_dir_handle();
        let app_runtime = cx.global::<AppRuntime>().clone();
        let view = cx.entity().downgrade();

        cx.spawn_in(window, async move |_, window| {
            let result = app_runtime
                .spawn_store(window.background_executor(), move |store| async move {
                    storage::workspace::archive::import_workspace(
                        &store,
                        data_dir.as_path(),
                        &archive_path,
                        mode,
                    )
                    .await
                })
                .await;

            window
                .update(|window, cx| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };
                    view.update(cx, |this, cx| {
                        this.workspace_archive_busy = false;
                        let notification = match result {
                            Ok(Ok(summary)) => {
                                let settings_error =
                                    AppSettings::import_json(&summary.settings_json, cx).err();
                                if settings_error.is_none() {
                                    let tray_shortcut = AppSettings::tray_shortcut(cx);
                                    (this.update_tray_shortcut)(tray_shortcut.as_ref(), cx);
                                    let quick_capture_shortcut =
                                        AppSettings::quick_capture_shortcut(cx);
                                    (this.update_quick_capture_shortcut)(
                                        quick_capture_shortcut.as_ref(),
                                        cx,
                                    );
                                }
                                if mode == storage::workspace::archive::ImportMode::Replace {
                                    this.workspace.active_project_id = None;
                                    this.close_all_tabs(window, cx);
                                }
                                let show_sidebar = AppSettings::show_sidebar(cx);
                                this.sidebar.update(cx, |sidebar, cx| {
                                    sidebar.set_width(AppSettings::sidebar_width(cx), cx);
                                });
                                this.set_sidebar_visible(show_sidebar, cx);
                                this.refresh_workspace(cx);
                                this.load_home(cx);
                                this.load_trash(cx);

                                let mut message = format!(
                                    "Imported {} notes, {} boards, and {} projects from {}.",
                                    summary.counts.notes,
                                    summary.counts.boards,
                                    summary.counts.projects,
                                    display_path
                                );
                                if mode == storage::workspace::archive::ImportMode::Merge {
                                    message.push_str(" Existing workspace data was kept.");
                                } else {
                                    message.push_str(" The current workspace was replaced.");
                                }
                                if !summary.warnings.is_empty() {
                                    message.push_str(&format!(
                                        " {} attachment warning(s).",
                                        summary.warnings.len()
                                    ));
                                }
                                if let Some(error) = settings_error {
                                    message.push_str(&format!(
                                        " Workspace settings could not be applied: {error}."
                                    ));
                                }
                                Notification::success(message)
                            }
                            Ok(Err(error)) => Notification::error(format!(
                                "Could not import the workspace: {error}"
                            )),
                            Err(error) => Notification::error(format!(
                                "Could not finish importing the workspace: {error}"
                            )),
                        };
                        window.push_notification(notification, cx);
                        cx.notify();
                    });
                })
                .ok();
        })
        .detach();
    }

    pub(super) fn create_board(
        &mut self,
        project_id: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_board_template_picker(project_id, window, cx);
    }

    pub(crate) fn create_board_with_title(
        &mut self,
        project_id: Option<u32>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app_runtime = cx.global::<AppRuntime>().clone();
        let view = cx.entity().downgrade();

        cx.spawn_in(window, async move |_, window| {
            let inserted = app_runtime
                .spawn_store(window.background_executor(), move |store| async move {
                    storage::workspace::create_board(&store, project_id, title).await
                })
                .await
                .ok()?
                .ok()?;

            window
                .update(|window, cx| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };

                    view.update(cx, |this, cx| {
                        this.open_board_tab(
                            inserted.id,
                            project_id,
                            SharedString::from(inserted.title),
                            window,
                            cx,
                        );
                        this.refresh_workspace(cx);
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }
}

async fn watch_change_revisions(
    store: storage::Store,
    sender: tokio::sync::watch::Sender<Option<ChangeRevision>>,
    interval: std::time::Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_published = None;

    loop {
        ticker.tick().await;
        match publish_change_revision(&store, &sender, &mut last_published).await {
            Ok(true) => {}
            Ok(false) => break,
            Err(err) => eprintln!("Failed to check for external Castle changes: {err}"),
        }
    }
}

fn remove_linked_note_file(path: &Path) -> std::io::Result<()> {
    remove_file(path)
}

fn claim_workspace_import_submission(submitted: &Cell<bool>) -> bool {
    !submitted.replace(true)
}

fn footer_action_label(label: impl Into<SharedString>) -> impl IntoElement {
    // Button::label renders inside a truncating single-line wrapper whose
    // tight leading clips descenders, so static footer labels use a child
    // with room to breathe instead.
    let label: SharedString = label.into();
    div()
        .flex_none()
        .whitespace_nowrap()
        .line_height(relative(1.2))
        .child(label)
}

async fn publish_change_revision(
    store: impl Into<storage::Store>,
    sender: &tokio::sync::watch::Sender<Option<ChangeRevision>>,
    last_published: &mut Option<ChangeRevision>,
) -> anyhow::Result<bool> {
    let store = store.into();
    let revision = storage::workspace::load_change_revision(&store).await?;
    if *last_published == Some(revision) {
        return Ok(true);
    }
    if sender.send(Some(revision)).is_err() {
        return Ok(false);
    }
    *last_published = Some(revision);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{
        ActiveModelTrait, ActiveValue::Set, ConnectionTrait, Database, DbBackend, EntityTrait,
        PaginatorTrait, Statement,
    };

    #[test]
    fn workspace_import_confirmation_accepts_only_one_submission() {
        let submitted = Cell::new(false);

        assert!(claim_workspace_import_submission(&submitted));
        assert!(!claim_workspace_import_submission(&submitted));
    }
    use std::{path::PathBuf, sync::Arc, time::Duration};

    #[tokio::test]
    async fn change_revision_updates_are_coalesced_before_reaching_gpui() -> anyhow::Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let (sender, mut receiver) = tokio::sync::watch::channel(None);
        let mut last_published = None;

        assert!(publish_change_revision(&db, &sender, &mut last_published).await?);
        assert!(receiver.has_changed()?);
        let initial = *receiver.borrow_and_update();

        assert!(publish_change_revision(&db, &sender, &mut last_published).await?);
        assert!(!receiver.has_changed()?);

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE castle_change_revision
             SET revision = revision + 1, board_revision = board_revision + 1
             WHERE id = 1",
        ))
        .await?;

        assert!(publish_change_revision(&db, &sender, &mut last_published).await?);
        assert!(receiver.has_changed()?);
        let changed = *receiver.borrow_and_update();
        assert_eq!(
            changed.map(|revision| revision.revision),
            initial.map(|revision| revision.revision + 1)
        );
        assert_eq!(
            changed.map(|revision| revision.board_revision),
            initial.map(|revision| revision.board_revision + 1)
        );

        drop(receiver);
        last_published = None;
        assert!(!publish_change_revision(&db, &sender, &mut last_published).await?);
        Ok(())
    }

    #[tokio::test]
    async fn linked_note_file_is_removed_after_transaction_failure() -> anyhow::Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("linked-note.md");
        std::fs::write(&path, "# Linked note")?;

        let result = storage::workspace::create_managed_linked_note(
            &db,
            None,
            "Linked note".to_string(),
            path.display().to_string(),
            "# Linked note".to_string(),
            storage::workspace::links::WorkspaceItemRef {
                kind: storage::workspace::links::WorkspaceItemKind::Board,
                id: 999,
            },
        )
        .await;
        assert!(result.is_err());
        remove_linked_note_file(&path)?;

        assert!(!path.exists());
        assert_eq!(entity::note::Entity::find().count(&db).await?, 0);
        Ok(())
    }

    #[gpui::test]
    #[ignore = "performance proof; run explicitly with one test thread"]
    fn startup_workspace_load_count(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();

        let db = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                entity::project::ActiveModel {
                    name: Set("Shared snapshot".to_string()),
                    archived: Set(false),
                    position: Set(0),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>(db)
            })
            .expect("workspace load-count database should initialize");
        let settings_dir = std::env::temp_dir().join(format!(
            "castle-workspace-load-count-{}",
            std::process::id()
        ));
        let app_db = runtime::AppRuntime::new(Arc::new(db), PathBuf::new());

        storage::workspace::reset_workspace_load_count();
        let mut shell = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(settings::AppSettings::load(settings_dir));
            cx.set_global(app_db);
            cx.open_window(Default::default(), |window, cx| {
                let view = AppShell::view(window, test_shell_integration(), cx);
                shell = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("workspace load-count window should open")
        });
        let shell = shell.expect("app shell should exist");
        let cx = gpui::VisualTestContext::from_window(window.into(), cx);

        for _ in 0..50 {
            cx.run_until_parked();
            if storage::workspace::workspace_load_count() >= 1
                && !shell.read_with(&cx, |shell, _| shell.workspace.refreshing)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(storage::workspace::workspace_load_count(), 1);
        shell.read_with(&cx, |shell, cx| {
            assert!(
                shell
                    .workspace
                    .projects
                    .iter()
                    .any(|project| project.name == "Shared snapshot")
            );
            assert!(
                shell
                    .sidebar
                    .read(cx)
                    .contains_project_named("Shared snapshot")
            );
        });
    }

    #[gpui::test]
    #[ignore = "performance proof; run explicitly with one test thread"]
    fn rapid_title_edits_save_latest_value_with_one_workspace_load(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();

        let (db, note_id) = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                let note = entity::note::ActiveModel {
                    title: Set("Original".to_string()),
                    project_id: Set(None),
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
                Ok::<_, anyhow::Error>((db, note.id as u32))
            })
            .expect("title-save database should initialize");
        let settings_dir =
            std::env::temp_dir().join(format!("castle-title-save-{}", std::process::id()));
        let app_db = runtime::AppRuntime::new(Arc::new(db.clone()), PathBuf::new());

        let mut shell = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(settings::AppSettings::load(settings_dir));
            cx.set_global(app_db);
            cx.open_window(Default::default(), |window, cx| {
                let view = AppShell::view(window, test_shell_integration(), cx);
                shell = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("title-save window should open")
        });
        let shell = shell.expect("app shell should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

        for _ in 0..50 {
            cx.run_until_parked();
            if !shell.read_with(&cx, |shell, _| shell.workspace.refreshing) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        storage::workspace::reset_workspace_load_count();
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_note_tab(note_id, None, "Original".into(), window, cx);
                shell.rename_active_tab("First".to_string(), cx);
                shell.rename_active_tab("Second".to_string(), cx);
                shell.rename_active_tab("Final title".to_string(), cx);
            });
        });
        assert_eq!(
            shell.read_with(&cx, |shell, _| {
                shell
                    .workspace
                    .pending_title_saves
                    .get(&WorkspaceTitleTarget::Note(note_id))
                    .map(|pending| pending.generation)
            }),
            Some(3)
        );
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(300));

        for _ in 0..100 {
            cx.run_until_parked();
            if storage::workspace::workspace_load_count() == 1
                && !shell.read_with(&cx, |shell, _| shell.workspace.refreshing)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let saved = runtime
            .block_on(entity::note::Entity::find_by_id(note_id as i64).one(&db))
            .expect("saved title query should succeed")
            .expect("saved note should exist");
        assert_eq!(saved.title, "Final title");
        assert_eq!(storage::workspace::workspace_load_count(), 1);
        shell.read_with(&cx, |shell, cx| {
            assert!(
                shell
                    .workspace
                    .notes
                    .iter()
                    .any(|note| note.title == "Final title")
            );
            assert!(shell.sidebar.read(cx).contains_note_named("Final title"));
        });

        let flush = cx.update(|_, cx| {
            shell.update(cx, |shell, cx| {
                shell.rename_active_tab("Shutdown title".to_string(), cx);
                shell.flush_pending_workspace_title_saves(cx)
            })
        });
        cx.foreground_executor().block_on(flush);

        let saved = runtime
            .block_on(entity::note::Entity::find_by_id(note_id as i64).one(&db))
            .expect("shutdown title query should succeed")
            .expect("saved note should exist");
        assert_eq!(saved.title, "Shutdown title");
        assert!(shell.read_with(&cx, |shell, _| {
            shell.workspace.pending_title_saves.is_empty()
        }));
    }
}
