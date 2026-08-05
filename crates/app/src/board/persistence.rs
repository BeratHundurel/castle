use gpui::{Context, SharedString};
use sea_orm::{DatabaseConnection, DbErr};

use crate::DB;

use super::{BoardView, dto::*};

impl BoardView {
    #[cfg(test)]
    pub(crate) fn loaded_card_count(&self) -> usize {
        self.cards.len()
    }

    pub(crate) fn load_board(&mut self, board_id: u32, cx: &mut Context<Self>) {
        if self.board_id == Some(board_id) {
            return;
        }

        self.reload_board(board_id, cx);
    }

    pub(crate) fn reload_board(&mut self, board_id: u32, cx: &mut Context<Self>) {
        self.board_id = Some(board_id);
        self.load_error = None;
        self.is_adding_list = false;
        self.next_checklist_item_position = 0;
        self.enrich_board_async(cx, board_id);
    }

    pub(super) fn enrich_board_async(&mut self, cx: &mut Context<Self>, board_id: u32) {
        self.load_generation = self.load_generation.saturating_add(1);
        let generation = self.load_generation;
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();

        cx.spawn(async move |this, cx| {
            let result = match runtime
                .spawn(async move {
                    let board_data = async {
                        load_board_data(db.as_ref(), board_id)
                            .await
                            .map_err(anyhow::Error::from)
                    };
                    let properties = storage::board_properties::load_board_properties(
                        db.as_ref(),
                        board_id as i64,
                    );
                    let views =
                        storage::board_properties::load_board_views(db.as_ref(), board_id as i64);
                    let ((cards, labels), properties, views) =
                        tokio::try_join!(board_data, properties, views)?;
                    Ok::<_, anyhow::Error>((cards, labels, properties, views))
                })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(anyhow::Error::from(err)),
            };

            this.update(cx, |this, cx| {
                if this.board_id == Some(board_id) && this.load_generation == generation {
                    match result {
                        Ok((cards, board_labels, board_properties, saved_views)) => {
                            let property_values = board_properties
                                .values
                                .iter()
                                .map(|value| {
                                    ((value.entry_id, value.property_id), value.value.clone())
                                })
                                .collect();
                            let active_view = saved_views.selected_view_id.and_then(|view_id| {
                                saved_views.views.iter().find(|view| view.id == view_id)
                            });
                            let active_view_id = active_view.map(|view| view.id);
                            let active_view_config = active_view
                                .map(|view| view.config.clone())
                                .unwrap_or_else(super::filters::default_view_config);
                            let mut warnings = board_properties
                                .warnings
                                .iter()
                                .cloned()
                                .map(SharedString::from)
                                .collect::<Vec<_>>();
                            warnings.extend(
                                saved_views.warnings.iter().cloned().map(SharedString::from),
                            );
                            this.cards = cards;
                            this.board_labels = board_labels;
                            this.board_properties = board_properties;
                            this.property_values = property_values;
                            this.saved_views = saved_views.views;
                            this.active_view_id = active_view_id;
                            this.active_view_config = active_view_config.clone();
                            this.filters =
                                super::filters::BoardFilters::from_config(&active_view_config);
                            this.view_config_dirty = false;
                            this.view_load_warnings = warnings;
                            this.attachment_preview_paths.clear();
                            this.load_error = None;
                        }
                        Err(err) => {
                            let message = format!("Failed to load board {board_id}: {err}");
                            eprintln!("{message}");
                            this.cards.clear();
                            this.board_properties = Default::default();
                            this.property_values.clear();
                            this.saved_views.clear();
                            this.active_view_id = None;
                            this.active_view_config = super::filters::default_view_config();
                            this.filters.clear();
                            this.view_load_warnings.clear();
                            this.load_error = Some(SharedString::from(message));
                        }
                    }
                    cx.notify();
                    cx.emit(super::BoardViewEvent::LoadFinished(board_id));
                }
            })
            .ok();
        })
        .detach();
    }
}

pub(super) async fn load_board_data(
    db: &DatabaseConnection,
    board_id: u32,
) -> Result<(Vec<CardDTO>, Vec<BoardLabelDTO>), DbErr> {
    let snapshot = storage::board::load_board_snapshot(db, board_id).await?;
    Ok((
        snapshot.cards.into_iter().map(CardDTO::from).collect(),
        snapshot
            .labels
            .into_iter()
            .map(BoardLabelDTO::from)
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::load_board_data;
    use anyhow::Result;
    use entity::{
        board, board::Entity as Board, board_label, board_label::Entity as BoardLabel, card,
        card::Entity as Card, entry, entry::Entity as Entry, entry_attachment,
        entry_checklist_item, entry_label, entry_label::Entity as EntryLabel,
    };
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{
        ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, Database, DbBackend,
        EntityTrait, QueryFilter, Statement,
    };
    use std::{path::PathBuf, sync::Arc, time::Instant};

    #[tokio::test]
    #[ignore = "performance proof; run explicitly with one test thread"]
    async fn large_board_load_latency_benchmark() -> Result<()> {
        const LISTS: usize = 10;
        const ENTRIES_PER_LIST: usize = 50;
        const MEASUREMENTS: usize = 20;

        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = board::ActiveModel {
            title: Set("Large board".to_string()),
            project_id: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let label = board_label::ActiveModel {
            board_id: Set(board.id),
            name: Set("Measured".to_string()),
            color: Set("blue".to_string()),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let mut entry_id = 1_i64;
        for list_index in 0..LISTS {
            let list = card::ActiveModel {
                id: Set(list_index as i64 + 1),
                title: Set(format!("List {list_index}")),
                board_id: Set(board.id),
                position: Set(list_index as i32),
                ..Default::default()
            }
            .insert(&db)
            .await?;

            for entry_index in 0..ENTRIES_PER_LIST {
                entry::ActiveModel {
                    id: Set(entry_id),
                    title: Set(format!("Entry {list_index}-{entry_index}")),
                    description: Set("A measured card description".to_string()),
                    card_id: Set(list.id),
                    position: Set(entry_index as i32),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                entry_label::ActiveModel {
                    id: Set(entry_id),
                    entry_id: Set(entry_id),
                    board_label_id: Set(label.id),
                }
                .insert(&db)
                .await?;
                entry_attachment::ActiveModel {
                    id: Set(entry_id),
                    entry_id: Set(entry_id),
                    file_name: Set(format!("attachment-{entry_id}.png")),
                }
                .insert(&db)
                .await?;
                for checklist_index in 0..2_i64 {
                    entry_checklist_item::ActiveModel {
                        id: Set((entry_id - 1) * 2 + checklist_index + 1),
                        entry_id: Set(entry_id),
                        title: Set(format!("Check {checklist_index}")),
                        checked: Set(checklist_index == 0),
                        position: Set(checklist_index as i32),
                    }
                    .insert(&db)
                    .await?;
                }
                entry_id += 1;
            }
        }

        for _ in 0..3 {
            load_board_data(&db, board.id as u32).await?;
        }

        let mut elapsed_micros = Vec::with_capacity(MEASUREMENTS);
        for _ in 0..MEASUREMENTS {
            let started = Instant::now();
            let (cards, labels) = load_board_data(&db, board.id as u32).await?;
            elapsed_micros.push(started.elapsed().as_micros());
            assert_eq!(cards.len(), LISTS);
            assert_eq!(
                cards.iter().map(|card| card.entries.len()).sum::<usize>(),
                LISTS * ENTRIES_PER_LIST
            );
            assert_eq!(labels.len(), 1);
        }
        elapsed_micros.sort_unstable();
        let median = elapsed_micros[MEASUREMENTS / 2];
        let p95 = elapsed_micros[MEASUREMENTS * 95 / 100];
        println!(
            "lists={LISTS} entries={} labels={} attachments={} checklist_items={} median_load_micros={median} p95_load_micros={p95}",
            LISTS * ENTRIES_PER_LIST,
            LISTS * ENTRIES_PER_LIST,
            LISTS * ENTRIES_PER_LIST,
            LISTS * ENTRIES_PER_LIST * 2
        );

        Ok(())
    }

    #[gpui::test]
    fn restored_board_populates_gpui_view_without_restart(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let (db, request) = runtime
            .block_on(async {
                let db = Database::connect(
                    "sqlite:file:castle_board_view_integration?mode=memory&cache=shared",
                )
                .await?;
                Migrator::up(&db, None).await?;

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
                    title: Set("Visible after restore".to_string()),
                    description: Set(String::new()),
                    card_id: Set(list.id),
                    position: Set(0),
                    ..Default::default()
                }
                .insert(&db)
                .await?;

                let request = crate::trash::MoveToTrash {
                    kind: crate::trash::TrashItemKind::Board,
                    id: board.id as u32,
                };
                crate::trash::move_to_trash(&db, request, 1).await?;
                crate::trash::restore_item(&db, crate::trash::RestoreTrashItem(request)).await?;
                Ok::<_, anyhow::Error>((db, request))
            })
            .expect("board restore setup should succeed");

        let db = crate::DB {
            conn: Arc::new(db),
            data_dir: PathBuf::new(),
        };
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(db);
            cx.open_window(Default::default(), |window, cx| {
                let view = super::BoardView::view(window, cx);
                view.update(cx, |board, cx| board.load_board(request.id, cx));
                view
            })
            .expect("board test window should open")
        });
        let view = window.root(cx).expect("board view should exist");

        for _ in 0..100 {
            cx.run_until_parked();
            if view.read_with(cx, |board, _| !board.cards.is_empty()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        view.read_with(cx, |board, _| {
            assert_eq!(board.cards.len(), 1);
            assert_eq!(board.cards[0].entries.len(), 1);
        });
    }

    #[tokio::test]
    async fn restored_board_and_project_keep_lists_and_entries_loadable() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let project = entity::project::ActiveModel {
            name: Set("Castle".to_string()),
            archived: Set(false),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let board = board::ActiveModel {
            title: Set("Kanban".to_string()),
            project_id: Set(Some(project.id)),
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
        let entry = entry::ActiveModel {
            title: Set("Keep me".to_string()),
            description: Set("Board content".to_string()),
            card_id: Set(list.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let board_request = crate::trash::MoveToTrash {
            kind: crate::trash::TrashItemKind::Board,
            id: board.id as u32,
        };
        crate::trash::move_to_trash(&db, board_request, 10).await?;
        crate::trash::restore_item(&db, crate::trash::RestoreTrashItem(board_request)).await?;

        let (cards, _) = load_board_data(&db, board.id as u32).await?;
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title.as_ref(), "Todo");
        assert_eq!(cards[0].entries.len(), 1);
        assert_eq!(cards[0].entries[0].id, entry.id as u32);

        let project_request = crate::trash::MoveToTrash {
            kind: crate::trash::TrashItemKind::Project,
            id: project.id as u32,
        };
        crate::trash::move_to_trash(&db, project_request, 20).await?;
        crate::trash::restore_item(&db, crate::trash::RestoreTrashItem(project_request)).await?;

        let (cards, _) = load_board_data(&db, board.id as u32).await?;
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].entries.len(), 1);
        assert_eq!(cards[0].entries[0].title.as_ref(), "Keep me");
        Ok(())
    }

    #[tokio::test]
    async fn board_labels_are_isolated_and_remove_card_assignments_on_delete() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let first_board = board::ActiveModel {
            title: Set("First".to_string()),
            project_id: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let second_board = board::ActiveModel {
            title: Set("Second".to_string()),
            project_id: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let list = card::ActiveModel {
            title: Set("Inbox".to_string()),
            board_id: Set(first_board.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let card = entry::ActiveModel {
            title: Set("Task".to_string()),
            description: Set(String::new()),
            card_id: Set(list.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let first_label = board_label::ActiveModel {
            board_id: Set(first_board.id),
            name: Set("Work".to_string()),
            color: Set("blue".to_string()),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let second_label = board_label::ActiveModel {
            board_id: Set(second_board.id),
            name: Set("Personal".to_string()),
            color: Set("green".to_string()),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        entry_label::ActiveModel {
            entry_id: Set(card.id),
            board_label_id: Set(first_label.id),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let first_board_labels = BoardLabel::find()
            .filter(board_label::Column::BoardId.eq(first_board.id))
            .all(&db)
            .await?;
        assert_eq!(first_board_labels, vec![first_label.clone()]);
        assert_ne!(first_board_labels, vec![second_label]);

        BoardLabel::delete_by_id(first_label.id).exec(&db).await?;
        assert!(EntryLabel::find().all(&db).await?.is_empty());
        assert!(Board::find_by_id(first_board.id).one(&db).await?.is_some());
        assert!(Card::find_by_id(list.id).one(&db).await?.is_some());
        let persisted_card = Entry::find_by_id(card.id).one(&db).await?;
        assert_eq!(
            persisted_card
                .as_ref()
                .and_then(|card| card.due_on.as_deref()),
            None
        );

        entry::ActiveModel {
            id: Set(card.id),
            due_on: Set(Some("2026-07-10".to_string())),
            ..Default::default()
        }
        .update(&db)
        .await?;
        let persisted_card = Entry::find_by_id(card.id).one(&db).await?;
        assert_eq!(
            persisted_card
                .as_ref()
                .and_then(|card| card.due_on.as_deref()),
            Some("2026-07-10")
        );

        Ok(())
    }

    #[tokio::test]
    async fn card_images_and_reminders_reload_with_the_board() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = board::ActiveModel {
            title: Set("Launch".to_string()),
            project_id: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let list = card::ActiveModel {
            title: Set("Ready".to_string()),
            board_id: Set(board.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let entry = entry::ActiveModel {
            title: Set("Ship Castle".to_string()),
            description: Set(String::new()),
            card_id: Set(list.id),
            position: Set(0),
            due_on: Set(Some("2026-07-23".to_string())),
            reminder_enabled: Set(true),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        entry_attachment::ActiveModel {
            entry_id: Set(entry.id),
            file_name: Set("release.png".to_string()),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let (cards, _) = load_board_data(&db, board.id as u32).await?;
        let loaded = &cards[0].entries[0];
        assert!(loaded.reminder_enabled);
        assert_eq!(loaded.attachments.len(), 1);
        assert_eq!(loaded.attachments[0].file_name.as_ref(), "release.png");

        let revision = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT revision FROM castle_change_revision WHERE id = 1",
            ))
            .await?
            .ok_or_else(|| anyhow::anyhow!("change revision row is missing"))?;
        assert_eq!(revision.try_get::<i64>("", "revision")?, 0);
        Ok(())
    }
}
