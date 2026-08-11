use std::fs::{create_dir_all, read_to_string, remove_file, write};
use std::{collections::HashMap, path::Path};

use super::*;
use crate::workspace_data::load_workspace_rows;
use gpui_component::{
    WindowExt as _,
    dialog::{
        DialogAction, DialogClose, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
    },
    input::Input,
    notification::Notification,
};
use storage::workspace::ChangeRevision;

const EXTERNAL_CHANGE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(750);

impl AppShell {
    pub(crate) fn start_note_link_reindex(&mut self, cx: &mut Context<Self>) {
        let store = cx.global::<AppServices>().store();
        let db = store.connection();
        let runtime = cx.global::<AppServices>().runtime();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::workspace_links::repair_workspace_link_index_batch(db.as_ref(), 32)
                        .await
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
        let store = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();
        let (revision_sender, mut revision_receiver) = tokio::sync::watch::channel(None);

        let poller = runtime.spawn(watch_change_revisions(
            store,
            revision_sender,
            EXTERNAL_CHANGE_POLL_INTERVAL,
        ));
        drop(poller);

        self.external_change_task = Some(cx.spawn_in(window, async move |this, cx| {
            while revision_receiver.changed().await.is_ok() {
                let Some(revision) = *revision_receiver.borrow_and_update() else {
                    continue;
                };

                if this
                    .update_in(cx, |this, window, cx| {
                        let changed = this
                            .last_change_revision
                            .is_some_and(|previous| previous != revision.revision);
                        let board_changed = this
                            .last_board_revision
                            .is_some_and(|previous| previous != revision.board_revision);
                        let note_changed = this
                            .last_note_revision
                            .is_some_and(|previous| previous != revision.note_revision);
                        let link_changed = this
                            .last_link_revision
                            .is_some_and(|previous| previous != revision.link_revision);
                        this.last_change_revision = Some(revision.revision);
                        this.last_board_revision = Some(revision.board_revision);
                        this.last_note_revision = Some(revision.note_revision);
                        this.last_link_revision = Some(revision.link_revision);
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
            for view in self.note_views.values() {
                view.update(cx, |note, cx| note.reload_board_embeds(cx));
            }
        }

        if note_changed {
            let note_views = self.note_views.values().cloned().collect::<Vec<_>>();
            for view in note_views {
                view.update(cx, |note, cx| note.reload_after_external_change(window, cx));
            }
        }
        self.refresh_workspace(cx);
        self.load_home(cx);
        self.load_trash(cx);
    }

    pub(crate) fn refresh_workspace(&mut self, cx: &mut Context<Self>) {
        if self.workspace_refreshing {
            self.workspace_refresh_pending = true;
            return;
        }

        let db = cx.global::<AppServices>().store().connection();
        let runtime = cx.global::<AppServices>().runtime();
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

            let Ok(should_apply) = this.update(cx, |this, cx| {
                if std::mem::take(&mut this.workspace_refresh_pending) {
                    this.workspace_refreshing = false;
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

            let Ok(sidebar) = this.read_with(cx, |this, _| this.sidebar.clone()) else {
                return;
            };
            sidebar.update(cx, |sidebar, cx| {
                sidebar.apply_workspace_rows(&rows, cx);
            });

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
                for tab in &mut this.open_tabs {
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

                this.workspace_refreshing = false;
                this.projects = project_choices;
                this.boards = board_choices;
                this.notes = note_choices;
                this.rebuild_command_palette_workspace_commands();
                if tab_titles_changed {
                    this.persist_tab_session(cx);
                }
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
        let db = cx.global::<AppServices>().store().connection();
        let view = cx.entity().downgrade();
        let path = unique_note_path(cx.global::<AppServices>().data_dir().join("notes"), &title);
        let path_string = path.display().to_string();
        let background_executor = cx.background_executor().clone();
        let runtime = cx.global::<AppServices>().runtime();

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

            let inserted = runtime
                .spawn(async move {
                    storage::workspace::create_managed_note(
                        db.as_ref(),
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
        item: storage::workspace_links::WorkspaceItemRef,
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
                                .child("The note starts with a stable link back to this item."),
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
        item: storage::workspace_links::WorkspaceItemRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<AppServices>().store().connection();
        let view = cx.entity().downgrade();
        let path = unique_note_path(cx.global::<AppServices>().data_dir().join("notes"), &title);
        let path_string = path.display().to_string();
        let background_executor = cx.background_executor().clone();
        let runtime = cx.global::<AppServices>().runtime();
        let display_title = title.replace(['\r', '\n', '|'], " ");
        let source_link = storage::workspace_links::stable_workspace_link(item, &source_title);
        let content = format!(
            "# {display_title}\n\nRelated {}: {source_link}\n",
            item.kind.as_str(),
        );

        cx.spawn_in(window, async move |_, window| {
            let write_path = path.clone();
            let write_content = content.clone();
            if background_executor
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

            let db_for_insert = db.clone();
            let result = runtime
                .spawn(async move {
                    let inserted = storage::workspace::create_managed_linked_note(
                        db_for_insert.as_ref(),
                        project_id,
                        title,
                        path_string,
                        content,
                        item,
                    )
                    .await?;
                    let board_id = storage::workspace_links::load_workspace_link_catalog(
                        db_for_insert.as_ref(),
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
                background_executor
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
                                for tab in &this.open_tabs {
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

    pub(crate) fn open_text_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open text file".into()),
        });

        let background_executor = cx.background_executor().clone();
        let db = cx.global::<AppServices>().store().connection();
        let view = cx.entity().downgrade();
        let runtime = cx.global::<AppServices>().runtime();

        cx.spawn_in(window, async move |_, window| {
            let Some(paths) = paths.await.ok().and_then(Result::ok).flatten() else {
                return;
            };
            let Some(path) = paths.first().cloned() else {
                return;
            };
            let readable_path = path.clone();
            let content = match background_executor
                .spawn(async move { read_to_string(readable_path) })
                .await
            {
                Ok(content) => content,
                Err(err) => {
                    let message = format!("Could not open {} as UTF-8 text: {err}", path.display());
                    window
                        .update(|window, cx| {
                            window.push_notification(Notification::error(message), cx);
                        })
                        .ok();
                    return;
                }
            };

            let path_string = path.display().to_string();
            let title = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled document")
                .to_string();

            let persisted = runtime
                .spawn(async move {
                    storage::workspace::import_external_note(
                        db.as_ref(),
                        title,
                        path_string,
                        content,
                    )
                    .await
                })
                .await;

            let note = match persisted {
                Ok(Ok(note)) => note,
                Ok(Err(err)) => {
                    let message = format!("Could not add the text file to the workspace: {err}");
                    window
                        .update(|window, cx| {
                            window.push_notification(Notification::error(message), cx);
                        })
                        .ok();
                    return;
                }
                Err(err) => {
                    let message = format!("Could not finish opening the text file: {err}");
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
                            note.id,
                            None,
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
        let db = cx.global::<AppServices>().store().connection();
        let view = cx.entity().downgrade();
        let runtime = cx.global::<AppServices>().runtime();

        cx.spawn_in(window, async move |_, window| {
            let inserted = runtime
                .spawn(async move {
                    storage::workspace::create_board(db.as_ref(), project_id, title).await
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

async fn publish_change_revision(
    store: &storage::Store,
    sender: &tokio::sync::watch::Sender<Option<ChangeRevision>>,
    last_published: &mut Option<ChangeRevision>,
) -> anyhow::Result<bool> {
    let db = store.connection();
    let revision = storage::workspace::load_change_revision(db.as_ref()).await?;
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
    use std::{path::PathBuf, sync::Arc, time::Duration};

    #[tokio::test]
    async fn change_revision_updates_are_coalesced_before_reaching_gpui() -> anyhow::Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let (sender, mut receiver) = tokio::sync::watch::channel(None);
        let mut last_published = None;

        assert!(
            publish_change_revision(
                &storage::Store::from_connection(db.clone()),
                &sender,
                &mut last_published
            )
            .await?
        );
        assert!(receiver.has_changed()?);
        let initial = *receiver.borrow_and_update();

        assert!(
            publish_change_revision(
                &storage::Store::from_connection(db.clone()),
                &sender,
                &mut last_published
            )
            .await?
        );
        assert!(!receiver.has_changed()?);

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE castle_change_revision
             SET revision = revision + 1, board_revision = board_revision + 1
             WHERE id = 1",
        ))
        .await?;

        assert!(
            publish_change_revision(
                &storage::Store::from_connection(db.clone()),
                &sender,
                &mut last_published
            )
            .await?
        );
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
        assert!(
            !publish_change_revision(
                &storage::Store::from_connection(db.clone()),
                &sender,
                &mut last_published
            )
            .await?
        );
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
            storage::workspace_links::WorkspaceItemRef {
                kind: storage::workspace_links::WorkspaceItemKind::Board,
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
        let app_db = crate::AppServices::new(Arc::new(db), PathBuf::new());

        crate::workspace_data::reset_workspace_load_count();
        let mut shell = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(crate::app_settings::AppSettings::load(settings_dir));
            cx.set_global(app_db);
            cx.open_window(Default::default(), |window, cx| {
                let view = AppShell::view(window, cx);
                shell = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("workspace load-count window should open")
        });
        let shell = shell.expect("app shell should exist");
        let cx = gpui::VisualTestContext::from_window(window.into(), cx);

        for _ in 0..50 {
            cx.run_until_parked();
            if crate::workspace_data::workspace_load_count() >= 1
                && !shell.read_with(&cx, |shell, _| shell.workspace_refreshing)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(crate::workspace_data::workspace_load_count(), 1);
        shell.read_with(&cx, |shell, cx| {
            assert!(
                shell
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
        let app_db = crate::AppServices::new(Arc::new(db.clone()), PathBuf::new());

        let mut shell = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(crate::app_settings::AppSettings::load(settings_dir));
            cx.set_global(app_db);
            cx.open_window(Default::default(), |window, cx| {
                let view = AppShell::view(window, cx);
                shell = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("title-save window should open")
        });
        let shell = shell.expect("app shell should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

        for _ in 0..50 {
            cx.run_until_parked();
            if !shell.read_with(&cx, |shell, _| shell.workspace_refreshing) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        crate::workspace_data::reset_workspace_load_count();
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
                    .pending_workspace_title_saves
                    .get(&WorkspaceTitleTarget::Note(note_id))
                    .map(|pending| pending.generation)
            }),
            Some(3)
        );
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(300));

        for _ in 0..100 {
            cx.run_until_parked();
            if crate::workspace_data::workspace_load_count() == 1
                && !shell.read_with(&cx, |shell, _| shell.workspace_refreshing)
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
        assert_eq!(crate::workspace_data::workspace_load_count(), 1);
        shell.read_with(&cx, |shell, cx| {
            assert!(shell.notes.iter().any(|note| note.title == "Final title"));
            assert!(shell.sidebar.read(cx).contains_note_named("Final title"));
        });

        let flush = cx.update(|_, cx| {
            shell.update(cx, |shell, cx| {
                shell.rename_active_tab("Shutdown title".to_string(), cx);
                shell.flush_pending_workspace_title_saves(cx)
            })
        });
        runtime.block_on(flush);

        let saved = runtime
            .block_on(entity::note::Entity::find_by_id(note_id as i64).one(&db))
            .expect("shutdown title query should succeed")
            .expect("saved note should exist");
        assert_eq!(saved.title, "Shutdown title");
        assert!(shell.read_with(&cx, |shell, _| {
            shell.pending_workspace_title_saves.is_empty()
        }));
    }
}
