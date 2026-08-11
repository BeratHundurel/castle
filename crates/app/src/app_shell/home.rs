use chrono::{Local, TimeZone as _};
use gpui::StatefulInteractiveElement as _;
use gpui_component::{
    Icon, Selectable as _, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    input::Input,
    scroll::ScrollableElement as _,
};

use super::*;
use crate::home::{TodayEntry, WorkspaceHomeItem, WorkspaceItemKind};
use crate::trash::{MoveToTrash, PurgeTrashItem, PurgedArtifacts, RestoreTrashItem};

mod loading;
mod render;
mod trash;

fn remove_purged_artifacts(artifacts: PurgedArtifacts, attachments_dir: std::path::PathBuf) {
    for path in artifacts.managed_files {
        let _ = std::fs::remove_file(path);
    }
    for note_id in artifacts.attachment_note_ids {
        let _ = std::fs::remove_dir_all(attachments_dir.join(note_id.to_string()));
    }
    for entry_id in artifacts.attachment_entry_ids {
        let _ = std::fs::remove_dir_all(attachments_dir.join("entries").join(entry_id.to_string()));
    }
}

fn section_title(
    title: &'static str,
    subtitle: &'static str,
    cx: &mut Context<AppShell>,
) -> impl IntoElement {
    h_flex()
        .items_end()
        .justify_between()
        .child(
            div()
                .text_lg()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(subtitle),
        )
}

fn empty_state(
    icon: IconName,
    title: &'static str,
    body: &'static str,
    cx: &mut Context<AppShell>,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .items_center()
        .gap_2()
        .p_6()
        .rounded(cx.theme().radius)
        .bg(cx.theme().secondary.opacity(0.28))
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(icon).small())
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .child(title),
        )
        .child(div().text_xs().child(body))
}

fn inline_retry(
    error: SharedString,
    retry: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<AppShell>,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .justify_between()
        .gap_3()
        .p_3()
        .rounded(cx.theme().radius)
        .bg(cx.theme().danger.opacity(0.08))
        .text_sm()
        .text_color(cx.theme().danger)
        .child(error)
        .child(
            Button::new("retry-workspace-view")
                .label("Retry")
                .outline()
                .small()
                .on_click(retry),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity::{board, card, entry, note};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectOptions, Database, EntityTrait};
    use std::{path::PathBuf, sync::Arc, time::Duration};

    #[test]
    fn purged_artifact_cleanup_keeps_active_note_attachments() -> anyhow::Result<()> {
        let test_dir = std::env::temp_dir().join(format!(
            "castle-purged-artifacts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let attachments_dir = test_dir.join("attachments");
        let purged_note_dir = attachments_dir.join("41");
        let active_note_dir = attachments_dir.join("42");
        let purged_entry_dir = attachments_dir.join("entries").join("51");
        let active_entry_dir = attachments_dir.join("entries").join("52");
        std::fs::create_dir_all(&purged_note_dir)?;
        std::fs::create_dir_all(&active_note_dir)?;
        std::fs::create_dir_all(&purged_entry_dir)?;
        std::fs::create_dir_all(&active_entry_dir)?;
        std::fs::write(purged_note_dir.join("image.png"), b"purged")?;
        std::fs::write(active_note_dir.join("image.png"), b"active")?;
        std::fs::write(purged_entry_dir.join("image.png"), b"purged")?;
        std::fs::write(active_entry_dir.join("image.png"), b"active")?;
        let managed_file = test_dir.join("note.md");
        std::fs::write(&managed_file, b"note")?;

        remove_purged_artifacts(
            PurgedArtifacts {
                managed_files: vec![managed_file.clone()],
                attachment_note_ids: vec![41],
                attachment_entry_ids: vec![51],
            },
            attachments_dir,
        );

        assert!(!managed_file.exists());
        assert!(!purged_note_dir.exists());
        assert!(!purged_entry_dir.exists());
        assert!(active_note_dir.join("image.png").exists());
        assert!(active_entry_dir.join("image.png").exists());
        std::fs::remove_dir_all(test_dir)?;
        Ok(())
    }

    #[gpui::test]
    fn rapid_tab_churn_keeps_database_and_views_responsive(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();

        let database_path = std::env::temp_dir().join(format!(
            "castle-tab-churn-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after the Unix epoch")
                .as_nanos()
        ));
        std::fs::File::create(&database_path).expect("test database file should be created");
        let database_url = format!("sqlite:{}", database_path.display()).replace('\\', "/");

        let (db, note_id, board_id) = runtime
            .block_on(async {
                let mut options = ConnectOptions::new(database_url);
                options.max_connections(1).min_connections(1);
                let db = Database::connect(options).await?;
                Migrator::up(&db, None).await?;
                let note = note::ActiveModel {
                    title: Set("Restored note".to_string()),
                    project_id: Set(None),
                    file_path: Set(None),
                    file_managed_by_app: Set(false),
                    cached_content: Set("# Restored content".to_string()),
                    file_missing_since: Set(None),
                    created_at: Set(1),
                    updated_at: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                let board = board::ActiveModel {
                    title: Set("Restored board".to_string()),
                    project_id: Set(None),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                let list = card::ActiveModel {
                    title: Set("Todo".to_string()),
                    board_id: Set(board.id),
                    position: Set(0),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                entry::ActiveModel {
                    title: Set("Restored card".to_string()),
                    description: Set(String::new()),
                    card_id: Set(list.id),
                    position: Set(0),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>((db, note.id as u32, board.id as u32))
            })
            .expect("tab churn test setup should succeed");

        let settings_dir =
            std::env::temp_dir().join(format!("castle-restore-test-{}", std::process::id()));
        let db = Arc::new(db);
        let held_connection = runtime
            .block_on(db.get_sqlite_connection_pool().acquire())
            .expect("test should reserve the SQLite connection");
        let app_db = crate::AppServices::new(db.clone(), PathBuf::new());
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
            .expect("restore test window should open")
        });
        let shell = shell.expect("app shell should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_note_tab(note_id, None, "Restored note".into(), window, cx);
                shell.open_board_tab(board_id, None, "Restored board".into(), window, cx);
            });
        });
        cx.run_until_parked();
        shell.read_with(&cx, |shell, _| {
            assert!(matches!(
                shell.open_tabs[shell.active_tab_index].kind,
                OpenTabKind::Note { .. }
            ));
            assert_eq!(
                shell
                    .pending_board_open
                    .as_ref()
                    .map(|pending| pending.board_id),
                Some(board_id),
                "the current surface must remain active until the first board snapshot is ready"
            );
        });
        let (pending_note_view, pending_board_view) = shell.read_with(&cx, |shell, _| {
            let note = shell
                .open_tabs
                .iter()
                .find_map(|tab| match &tab.kind {
                    OpenTabKind::Note { view, .. } => Some(view.clone()),
                    _ => None,
                })
                .expect("note tab should have a view");
            let board = shell
                .open_tabs
                .iter()
                .find_map(|tab| match &tab.kind {
                    OpenTabKind::Board { view, .. } => Some(view.clone()),
                    _ => None,
                })
                .expect("board tab should have a view");
            (note, board)
        });
        for _ in 0..100 {
            cx.update(|window, cx| {
                pending_note_view
                    .update(cx, |note, cx| note.reload_after_external_change(window, cx));
                pending_board_view.update(cx, |board, cx| board.reload_board(board_id, cx));
            });
            cx.run_until_parked();
        }
        let closed_note = pending_note_view.downgrade();
        let closed_board = pending_board_view.downgrade();
        drop(pending_note_view);
        drop(pending_board_view);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.close_all_tabs(window, cx);
            });
        });
        cx.run_until_parked();
        assert!(
            closed_board.upgrade().is_none(),
            "closing a board tab must release its view and attachment state"
        );
        assert!(
            closed_note.upgrade().is_none(),
            "closing a saved note tab must release its editor state"
        );

        for _ in 0..100 {
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| {
                    shell.open_note_tab(note_id, None, "Restored note".into(), window, cx);
                });
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| {
                    shell.close_all_tabs(window, cx);
                    shell.open_board_tab(board_id, None, "Restored board".into(), window, cx);
                });
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| shell.close_all_tabs(window, cx));
            });
        }
        cx.run_until_parked();

        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_note_tab(note_id, None, "Restored note".into(), window, cx);
                shell.open_board_tab(board_id, None, "Restored board".into(), window, cx);
            });
        });
        cx.run_until_parked();
        drop(held_connection);

        let (note_view, board_view) = shell.read_with(&cx, |shell, _| {
            let note_view = shell.open_tabs.iter().find_map(|tab| match &tab.kind {
                OpenTabKind::Note { view, .. } => Some(view.clone()),
                _ => None,
            });
            let board_view = shell.open_tabs.iter().find_map(|tab| match &tab.kind {
                OpenTabKind::Board { view, .. } => Some(view.clone()),
                _ => None,
            });
            (
                note_view.expect("restored note tab should exist"),
                board_view.expect("restored board tab should exist"),
            )
        });

        for _ in 0..10_000 {
            cx.run_until_parked();
            let note_loaded = note_view
                .read_with(&cx, |note, cx| note.loaded_content(cx))
                .is_some();
            let board_loaded = board_view.read_with(&cx, |board, _| board.loaded_card_count() == 1);
            if note_loaded && board_loaded {
                break;
            }
            std::thread::yield_now();
        }

        assert_eq!(
            note_view.read_with(&cx, |note, cx| note.loaded_content(cx)),
            Some("# Restored content".to_string())
        );
        assert_eq!(
            board_view.read_with(&cx, |board, _| board.loaded_card_count()),
            1
        );

        runtime
            .block_on(tokio::time::timeout(Duration::from_secs(1), async {
                entity::project::ActiveModel {
                    name: Set("Created after restore".to_string()),
                    archived: Set(false),
                    position: Set(1),
                    ..Default::default()
                }
                .insert(db.as_ref())
                .await?;
                card::ActiveModel {
                    title: Set("Added after restore".to_string()),
                    board_id: Set(board_id as i64),
                    position: Set(1),
                    ..Default::default()
                }
                .insert(db.as_ref())
                .await?;
                Ok::<_, sea_orm::DbErr>(())
            }))
            .expect("database should remain responsive after tab churn")
            .expect("post-churn writes should succeed");

        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.refresh_workspace(cx);
                if let Some(index) = shell.open_tabs.iter().position(
                    |tab| matches!(tab.kind, OpenTabKind::Board { board_id: id, .. } if id == board_id),
                ) {
                    shell.close_tab(index, window, cx);
                }
                shell.open_board_tab(board_id, None, "Restored board".into(), window, cx);
            });
        });

        for _ in 0..100 {
            cx.run_until_parked();
            let sidebar_has_project = shell.read_with(&cx, |shell, cx| {
                shell
                    .sidebar
                    .read(cx)
                    .contains_project_named("Created after restore")
            });
            let reopened_board_has_lists = shell.read_with(&cx, |shell, cx| {
                shell.open_tabs.iter().any(|tab| match &tab.kind {
                    OpenTabKind::Board { view, .. } => view.read(cx).loaded_card_count() == 2,
                    _ => false,
                })
            });
            if sidebar_has_project && reopened_board_has_lists {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(shell.read_with(&cx, |shell, cx| {
            shell
                .sidebar
                .read(cx)
                .contains_project_named("Created after restore")
        }));
        assert!(shell.read_with(&cx, |shell, cx| {
            shell.open_tabs.iter().any(|tab| match &tab.kind {
                OpenTabKind::Board { view, .. } => view.read(cx).loaded_card_count() == 2,
                _ => false,
            })
        }));

        cx.update(|window, cx| {
            note_view.update(cx, |note, cx| {
                note.replace_content_for_test("# Saved after close", window, cx);
            });
        });
        assert_eq!(
            note_view.read_with(&cx, |note, cx| note.loaded_content(cx)),
            Some("# Saved after close".to_string())
        );
        assert_eq!(
            note_view.read_with(&cx, |note, _| note.save_state()),
            SaveState::Dirty
        );
        assert_eq!(
            shell.read_with(&cx, |shell, _| {
                shell.note_views.get(&note_id).map(Entity::entity_id)
            }),
            Some(note_view.entity_id())
        );
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let note_index = shell
                    .open_tabs
                    .iter()
                    .position(|tab| matches!(tab.kind, OpenTabKind::Note { .. }))
                    .expect("note tab should still be open");
                shell.close_tab(note_index, window, cx);
            });
        });
        let closed_dirty_note = note_view.downgrade();
        assert!(shell.read_with(&cx, |shell, _| shell.note_views.contains_key(&note_id)));
        drop(note_view);
        assert!(
            closed_dirty_note.upgrade().is_some(),
            "a dirty closed editor must stay alive until autosave finishes"
        );

        cx.executor().advance_clock(Duration::from_millis(1_300));
        for _ in 0..150 {
            cx.run_until_parked();
            let persisted = runtime
                .block_on(note::Entity::find_by_id(note_id as i64).one(db.as_ref()))
                .ok()
                .flatten()
                .is_some_and(|note| note.cached_content == "# Saved after close\n");
            if persisted && closed_dirty_note.upgrade().is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        let saved_content = runtime
            .block_on(note::Entity::find_by_id(note_id as i64).one(db.as_ref()))
            .expect("saved note query should succeed")
            .expect("saved note should still exist")
            .cached_content;
        assert_eq!(saved_content, "# Saved after close\n");
        assert!(
            closed_dirty_note.upgrade().is_none(),
            "a closed editor must be released after autosave succeeds"
        );
    }
}
