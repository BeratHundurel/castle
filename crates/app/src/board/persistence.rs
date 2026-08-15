use gpui::{Context, SharedString};
use std::{future::Future, sync::Arc};

use crate::AppServices;

use super::{BoardView, BoardViewEvent, dto::*};

impl BoardView {
    pub(in crate::board) fn emit_data_committed(
        &self,
        cx: &mut Context<Self>,
        links_changed: bool,
    ) {
        if let Some(board_id) = self.data.board_id {
            cx.emit(BoardViewEvent::DataCommitted {
                board_id,
                links_changed,
            });
        }
    }

    pub(in crate::board) fn commit_board_mutation<F>(
        &mut self,
        cx: &mut Context<Self>,
        failure_context: &'static str,
        links_changed: bool,
        mutation: F,
    ) where
        F: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let Some(board_id) = self.data.board_id else {
            return;
        };
        let runtime = cx.global::<AppServices>().runtime();
        cx.spawn(async move |this, cx| {
            let result = runtime.spawn(mutation).await;
            this.update(cx, |this, cx| {
                if this.data.board_id != Some(board_id) {
                    return;
                }
                match result {
                    Ok(Ok(())) => {
                        this.mutation.mutation_error = None;
                        cx.emit(BoardViewEvent::DataCommitted {
                            board_id,
                            links_changed,
                        });
                    }
                    Ok(Err(error)) => {
                        this.mutation.mutation_error =
                            Some(format!("{failure_context}: {error}").into());
                        this.enrich_board_async(cx, board_id);
                    }
                    Err(error) => {
                        this.mutation.mutation_error =
                            Some(format!("{failure_context} task failed: {error}").into());
                        this.enrich_board_async(cx, board_id);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    #[cfg(test)]
    pub(crate) fn loaded_card_count(&self) -> usize {
        self.data.lists.len()
    }

    pub(crate) fn load_board(&mut self, board_id: u32, cx: &mut Context<Self>) {
        if self.data.board_id == Some(board_id) {
            return;
        }

        self.reload_board(board_id, cx);
    }

    pub(crate) fn reload_board(&mut self, board_id: u32, cx: &mut Context<Self>) {
        if self.data.board_id != Some(board_id) {
            self.mutation.mutation_error = None;
        }
        self.data.board_id = Some(board_id);
        self.mutation.load_error = None;
        self.entry_editing.adding_list = false;
        self.entry_editing.next_checklist_item_position = 0;
        self.enrich_board_async(cx, board_id);
    }

    pub(super) fn enrich_board_async(&mut self, cx: &mut Context<Self>, board_id: u32) {
        let generation = self.mutation.load_request.begin();
        self.mutation.loaded_generation = None;
        let local_mutation_generation = self.mutation.local_generation;
        let app_db = cx.global::<AppServices>();
        let store = app_db.store();
        let db = store.clone();
        let board_layout_persistence = cx.global::<super::BoardServices>().layout_persistence();
        let runtime = app_db.runtime();

        let task = cx.spawn(async move |this, cx| {
            let (cancel_on_drop, cancelled) = tokio::sync::oneshot::channel::<()>();
            let load = runtime.spawn(async move {
                tokio::select! {
                    biased;
                    _ = cancelled => None,
                    result = async move {
                    let _ = board_layout_persistence.wait_for_pending(board_id).await;
                    let board_data = async {
                        load_board_data(&store, board_id)
                            .await};
                    let properties = storage::board_properties::load_board_properties(
                        &db,
                        board_id as i64,
                    );
                    let views =
                        storage::board_properties::load_board_views(&db, board_id as i64);
                    let link_catalog =
                        storage::workspace_links::load_workspace_link_catalog(&db);
                    let ((cards, labels), properties, views, link_catalog) =
                        tokio::try_join!(board_data, properties, views, link_catalog)?;
                    let mut related_targets = vec![storage::workspace_links::WorkspaceItemRef {
                        kind: storage::workspace_links::WorkspaceItemKind::Board,
                        id: i64::from(board_id),
                    }];
                    related_targets.extend(cards.iter().map(|list| {
                        storage::workspace_links::WorkspaceItemRef {
                            kind: storage::workspace_links::WorkspaceItemKind::List,
                            id: i64::from(list.id),
                        }
                    }));
                    let related_notes = storage::workspace_links::load_related_notes_for_items(
                        &db,
                        &related_targets,
                    )
                    .await?;
                    Ok::<_, anyhow::Error>((
                        cards,
                        labels,
                        properties,
                        views,
                        link_catalog,
                        related_notes,
                    ))
                    } => Some(result),
                }
            });
            let result = match load.await {
                Ok(Some(result)) => result,
                Ok(None) => return,
                Err(error) => Err(anyhow::Error::from(error)),
            };
            drop(cancel_on_drop);

            this.update(cx, |this, cx| {
                if this.data.board_id == Some(board_id)
                    && this.mutation.load_request.generation() == generation
                {
                    this.mutation.loaded_generation = Some(generation);
                    if this.mutation.local_generation != local_mutation_generation {
                        cx.notify();
                        cx.emit(super::BoardViewEvent::LoadFinished(board_id));
                        return;
                    }
                    match result {
                        Ok((
                            cards,
                            board_labels,
                            board_properties,
                            saved_views,
                            link_catalog,
                            related_notes,
                        )) => {
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
                            this.data.lists = cards;
                            this.data.labels = board_labels;
                            this.properties.data = board_properties;
                            this.properties.values = property_values;
                            this.properties.saved_views = saved_views.views;
                            let workspace_link_catalog = Arc::new(link_catalog);
                            let project_id = workspace_link_catalog
                                .iter()
                                .find(|entry| {
                                    entry.item.kind
                                        == storage::workspace_links::WorkspaceItemKind::Board
                                        && entry.item.id == i64::from(board_id)
                                })
                                .and_then(|entry| entry.project_id);
                            this.related_notes
                                .completion_provider
                                .update_for_workspace_source(
                                    project_id,
                                    workspace_link_catalog.clone(),
                                );
                            this.related_notes.catalog = workspace_link_catalog;
                            this.related_notes.by_item = related_notes;
                            this.properties.active_view_id = active_view_id;
                            this.properties.active_view_config = active_view_config.clone();
                            this.filters =
                                super::filters::BoardFilters::from_config(&active_view_config);
                            this.properties.view_config_dirty = false;
                            this.properties.view_load_warnings = warnings;
                            this.entry_editing.attachment_preview_paths.clear();
                            this.mutation.load_error = None;
                        }
                        Err(err) => {
                            let message = format!("Failed to load board {board_id}: {err}");
                            eprintln!("{message}");
                            this.data.lists.clear();
                            this.properties.data = Default::default();
                            this.properties.values.clear();
                            this.properties.saved_views.clear();
                            this.related_notes.catalog = Arc::new(Vec::new());
                            this.related_notes.by_item.clear();
                            this.properties.active_view_id = None;
                            this.properties.active_view_config =
                                super::filters::default_view_config();
                            this.filters.clear();
                            this.properties.view_load_warnings.clear();
                            this.mutation.load_error = Some(SharedString::from(message));
                        }
                    }
                    cx.notify();
                    cx.emit(super::BoardViewEvent::LoadFinished(board_id));
                }
            })
            .ok();
        });
        self.mutation.load_request.set_task(task);
    }
}

pub(super) async fn load_board_data(
    store: impl Into<storage::Store>,
    board_id: u32,
) -> anyhow::Result<(Vec<BoardListDTO>, Vec<BoardLabelDTO>)> {
    let store = store.into();
    let db = store.clone();
    let snapshot = storage::board::load_board_snapshot(&db, board_id).await?;
    Ok((
        snapshot.cards.into_iter().map(BoardListDTO::from).collect(),
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
        ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectOptions, ConnectionTrait, Database,
        DbBackend, EntityTrait, QueryFilter, Statement,
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

                let request = storage::trash::MoveToTrash {
                    kind: storage::trash::TrashItemKind::Board,
                    id: board.id as u32,
                };
                storage::trash::move_to_trash(&db, request, 1).await?;
                storage::trash::restore_item(&db, storage::trash::RestoreTrashItem(request))
                    .await?;
                Ok::<_, anyhow::Error>((db, request))
            })
            .expect("board restore setup should succeed");

        let db = crate::AppServices::new(Arc::new(db), PathBuf::new());
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
            if view.read_with(cx, |board, _| !board.data.lists.is_empty()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        view.read_with(cx, |board, _| {
            assert_eq!(board.data.lists.len(), 1);
            assert_eq!(board.data.lists[0].entries.len(), 1);
        });
    }

    #[gpui::test]
    fn pending_reload_does_not_overwrite_a_local_card_move(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let (db, board_id, destination_id, entry_id) = runtime
            .block_on(async {
                let mut options = ConnectOptions::new("sqlite::memory:");
                options.max_connections(1).min_connections(1);
                let db = Database::connect(options).await?;
                Migrator::up(&db, None).await?;
                let board = board::ActiveModel {
                    title: Set("Move race".to_string()),
                    project_id: Set(None),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                let source = card::ActiveModel {
                    title: Set("Todo".to_string()),
                    board_id: Set(board.id),
                    position: Set(0),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                let destination = card::ActiveModel {
                    title: Set("Done".to_string()),
                    board_id: Set(board.id),
                    position: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                let entry = entry::ActiveModel {
                    title: Set("Move me".to_string()),
                    description: Set(String::new()),
                    card_id: Set(source.id),
                    position: Set(0),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>((
                    Arc::new(db),
                    board.id as u32,
                    destination.id as u32,
                    entry.id as u32,
                ))
            })
            .expect("board move race setup should succeed");

        let app_db = crate::AppServices::new(db.clone(), PathBuf::new());
        let board_services = crate::board::BoardServices::new(runtime.handle().clone());
        let position_persistence = board_services.layout_persistence();
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(app_db);
            cx.set_global(board_services);
            cx.open_window(Default::default(), |window, cx| {
                let view = super::BoardView::view(window, cx);
                view.update(cx, |board, cx| board.load_board(board_id, cx));
                view
            })
            .expect("board test window should open")
        });
        let view = window.root(cx).expect("board view should exist");
        for _ in 0..100 {
            cx.run_until_parked();
            if view.read_with(cx, |board, _| board.data.lists.len() == 2) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let held_connection = runtime
            .block_on(db.get_sqlite_connection_pool().acquire())
            .expect("test should reserve the only SQLite connection");
        view.update(cx, |board, cx| board.reload_board(board_id, cx));
        cx.run_until_parked();
        view.update(cx, |board, cx| {
            board.move_entry_to_list_end(entry_id, destination_id, cx)
        });
        assert!(view.read_with(cx, |board, _| {
            board.data.lists.iter().any(|list| {
                list.id == destination_id && list.entries.iter().any(|entry| entry.id == entry_id)
            })
        }));

        drop(held_connection);
        runtime
            .block_on(tokio::time::timeout(
                std::time::Duration::from_secs(1),
                position_persistence.wait_for_pending(board_id),
            ))
            .expect("the moved position should persist without a debounce delay")
            .expect("the moved position should be committed");
        cx.run_until_parked();

        assert!(view.read_with(cx, |board, _| {
            board.data.lists.iter().any(|list| {
                list.id == destination_id && list.entries.iter().any(|entry| entry.id == entry_id)
            })
        }));
        let stored = runtime
            .block_on(entry::Entity::find_by_id(i64::from(entry_id)).one(db.as_ref()))
            .expect("moved entry should remain queryable")
            .expect("moved entry should exist");
        assert_eq!(stored.card_id, i64::from(destination_id));
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

        let board_request = storage::trash::MoveToTrash {
            kind: storage::trash::TrashItemKind::Board,
            id: board.id as u32,
        };
        storage::trash::move_to_trash(&db, board_request, 10).await?;
        storage::trash::restore_item(&db, storage::trash::RestoreTrashItem(board_request)).await?;

        let (cards, _) = load_board_data(&db, board.id as u32).await?;
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title.as_ref(), "Todo");
        assert_eq!(cards[0].entries.len(), 1);
        assert_eq!(cards[0].entries[0].id, entry.id as u32);

        let project_request = storage::trash::MoveToTrash {
            kind: storage::trash::TrashItemKind::Project,
            id: project.id as u32,
        };
        storage::trash::move_to_trash(&db, project_request, 20).await?;
        storage::trash::restore_item(&db, storage::trash::RestoreTrashItem(project_request))
            .await?;

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

    #[gpui::test]
    fn rendered_card_drop_reorders_repeatedly_without_stalling(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let (db, board_id, first_entry_id, second_entry_id) = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                let board = board::ActiveModel {
                    title: Set("Rendered drag board".to_string()),
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
                let first = entry::ActiveModel {
                    title: Set("First".to_string()),
                    description: Set(String::new()),
                    card_id: Set(list.id),
                    position: Set(0),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                let second = entry::ActiveModel {
                    title: Set("Second".to_string()),
                    description: Set(String::new()),
                    card_id: Set(list.id),
                    position: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>((
                    Arc::new(db),
                    board.id as u32,
                    first.id as u32,
                    second.id as u32,
                ))
            })
            .expect("rendered drag setup should succeed");

        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(crate::AppServices::new(db, PathBuf::new()));
            cx.open_window(Default::default(), |window, cx| {
                let view = super::BoardView::view(window, cx);
                view.update(cx, |board, cx| board.load_board(board_id, cx));
                view
            })
            .expect("board drag test window should open")
        });
        let view = window.root(cx).expect("board view should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

        for _ in 0..100 {
            cx.run_until_parked();
            if view.read_with(&cx, |board, _| {
                board
                    .data
                    .lists
                    .first()
                    .is_some_and(|list| list.entries.len() == 2)
            }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cx.update(|window, cx| window.draw(cx).clear(cx));

        for iteration in 0..40 {
            let source = cx
                .debug_bounds("board-entry-1")
                .expect("first rendered card should have bounds")
                .center();
            let target = cx
                .debug_bounds("board-entry-2")
                .expect("second rendered card should have bounds")
                .center();

            cx.simulate_mouse_down(source, gpui::MouseButton::Left, gpui::Modifiers::default());
            cx.simulate_mouse_move(target, gpui::MouseButton::Left, gpui::Modifiers::default());
            cx.simulate_mouse_up(target, gpui::MouseButton::Left, gpui::Modifiers::default());

            assert_eq!(
                view.read_with(&cx, |board, _| {
                    board.data.lists[0]
                        .entries
                        .iter()
                        .map(|entry| entry.id)
                        .collect::<Vec<_>>()
                }),
                if iteration % 2 == 0 {
                    vec![second_entry_id, first_entry_id]
                } else {
                    vec![first_entry_id, second_entry_id]
                },
                "rendered drop {iteration} should reach the card reorder handler"
            );
            assert!(
                cx.update(|_, cx| !cx.has_active_drag()),
                "drop {iteration} must clear GPUI's active drag state"
            );
        }
    }
}
