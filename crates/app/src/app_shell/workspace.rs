use std::fs::{create_dir_all, read_to_string, write};
use std::{collections::HashMap, sync::Arc};

use super::*;
use crate::workspace_data::load_workspace_rows;
use gpui_component::{WindowExt as _, notification::Notification};
use sea_orm::{ConnectionTrait, DbBackend, DbErr, Statement};

const EXTERNAL_CHANGE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(750);

impl AppShell {
    pub(crate) fn start_external_change_watcher(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        let (revision_sender, mut revision_receiver) = tokio::sync::watch::channel(None);

        let poller = runtime.spawn(watch_change_revisions(
            db,
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
                        this.last_change_revision = Some(revision.revision);
                        this.last_board_revision = Some(revision.board_revision);
                        this.last_note_revision = Some(revision.note_revision);
                        if changed {
                            this.refresh_after_external_change(
                                board_changed,
                                note_changed,
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
        let runtime = tokio::runtime::Handle::current();

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
                    note::ActiveModel {
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
                            inserted.id as u32,
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

    pub(crate) fn open_text_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open text file".into()),
        });

        let background_executor = cx.background_executor().clone();
        let db = cx.global::<DB>().conn.clone();
        let view = cx.entity().downgrade();
        let runtime = tokio::runtime::Handle::current();

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
                    let existing = Note::find()
                        .filter(note::Column::FilePath.eq(path_string.clone()))
                        .one(&*db)
                        .await?;

                    if let Some(existing) = existing {
                        note::ActiveModel {
                            id: Set(existing.id),
                            file_path: Set(Some(path_string)),
                            cached_content: Set(content),
                            file_missing_since: Set(None),
                            updated_at: Set(now_ts()),
                            ..Default::default()
                        }
                        .update(&*db)
                        .await?;

                        Ok::<_, sea_orm::DbErr>((existing.id as u32, existing.title))
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
                        .await?;

                        Ok((inserted.id as u32, inserted.title))
                    }
                })
                .await;

            let (note_id, note_title) = match persisted {
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
                            note_id,
                            None,
                            SharedString::from(note_title),
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
        let runtime = tokio::runtime::Handle::current();

        cx.spawn_in(window, async move |_, window| {
            let inserted = runtime
                .spawn(async move {
                    board::ActiveModel {
                        title: Set(title),
                        project_id: Set(project_id.map(|id| id as i64)),
                        ..Default::default()
                    }
                    .insert(&*db)
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
                        this.open_board_tab(
                            inserted.id as u32,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChangeRevision {
    revision: i64,
    board_revision: i64,
    note_revision: i64,
}

async fn watch_change_revisions(
    db: Arc<sea_orm::DatabaseConnection>,
    sender: tokio::sync::watch::Sender<Option<ChangeRevision>>,
    interval: std::time::Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_published = None;

    loop {
        ticker.tick().await;
        match publish_change_revision(db.as_ref(), &sender, &mut last_published).await {
            Ok(true) => {}
            Ok(false) => break,
            Err(err) => eprintln!("Failed to check for external Castle changes: {err}"),
        }
    }
}

async fn publish_change_revision(
    db: &sea_orm::DatabaseConnection,
    sender: &tokio::sync::watch::Sender<Option<ChangeRevision>>,
    last_published: &mut Option<ChangeRevision>,
) -> Result<bool, DbErr> {
    let revision = load_change_revision(db).await?;
    if *last_published == Some(revision) {
        return Ok(true);
    }
    if sender.send(Some(revision)).is_err() {
        return Ok(false);
    }
    *last_published = Some(revision);
    Ok(true)
}

async fn load_change_revision(db: &sea_orm::DatabaseConnection) -> Result<ChangeRevision, DbErr> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait, Database, EntityTrait};
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
        let app_db = crate::DB {
            conn: Arc::new(db),
            data_dir: PathBuf::new(),
        };

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
        let app_db = crate::DB {
            conn: Arc::new(db.clone()),
            data_dir: PathBuf::new(),
        };

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
