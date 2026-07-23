use std::collections::HashMap;

use entity::{
    board_label, board_label::Entity as BoardLabel, card, card::Entity as Card,
    entry::Entity as Entry, entry_attachment, entry_attachment::Entity as EntryAttachment,
    entry_checklist_item, entry_checklist_item::Entity as EntryChecklistItem, entry_label,
    entry_label::Entity as EntryLabel,
};
use gpui::{Context, SharedString};
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder};

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
        self.cards.clear();
        self.board_labels.clear();
        self.attachment_preview_paths.clear();
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
                .spawn(async move { load_board_data(db.as_ref(), board_id).await })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(DbErr::Custom(err.to_string())),
            };

            this.update(cx, |this, cx| {
                if this.board_id == Some(board_id) && this.load_generation == generation {
                    match result {
                        Ok((cards, board_labels)) => {
                            this.cards = cards;
                            this.board_labels = board_labels;
                            this.load_error = None;
                        }
                        Err(err) => {
                            let message = format!("Failed to load board {board_id}: {err}");
                            eprintln!("{message}");
                            this.cards.clear();
                            this.load_error = Some(SharedString::from(message));
                        }
                    }
                    cx.notify();
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
    let mut cards = Card::load()
        .filter(card::Column::BoardId.eq(board_id as i32))
        .filter(card::Column::DeletedAt.is_null())
        .order_by_asc(card::Column::Position)
        .order_by_asc(card::Column::Id)
        .with(Entry)
        .all(db)
        .await?
        .into_iter()
        .map(CardDTO::from)
        .collect::<Vec<_>>();

    let board_labels = BoardLabel::find()
        .filter(board_label::Column::BoardId.eq(board_id as i64))
        .order_by_asc(board_label::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(BoardLabelDTO::from)
        .collect::<Vec<_>>();

    let label_by_id = board_labels
        .iter()
        .cloned()
        .map(|label| (label.id as i64, label))
        .collect::<HashMap<_, _>>();
    let entry_ids = cards
        .iter()
        .flat_map(|card| card.entries.iter().map(|entry| entry.id as i64))
        .collect::<Vec<_>>();
    let associations = if entry_ids.is_empty() {
        vec![]
    } else {
        EntryLabel::find()
            .filter(entry_label::Column::EntryId.is_in(entry_ids.clone()))
            .order_by_asc(entry_label::Column::Id)
            .all(db)
            .await?
    };
    let mut labels_by_entry = HashMap::<i64, Vec<BoardLabelDTO>>::new();
    for association in associations {
        if let Some(label) = label_by_id.get(&association.board_label_id) {
            labels_by_entry
                .entry(association.entry_id)
                .or_default()
                .push(label.clone());
        }
    }

    let attachments = if entry_ids.is_empty() {
        vec![]
    } else {
        EntryAttachment::find()
            .filter(entry_attachment::Column::EntryId.is_in(entry_ids.clone()))
            .order_by_asc(entry_attachment::Column::Id)
            .all(db)
            .await?
    };
    let mut attachments_by_entry = HashMap::<i64, Vec<EntryAttachmentDTO>>::new();
    for attachment in attachments {
        attachments_by_entry
            .entry(attachment.entry_id)
            .or_default()
            .push(EntryAttachmentDTO::from(attachment));
    }

    let checklist_items = if entry_ids.is_empty() {
        vec![]
    } else {
        EntryChecklistItem::find()
            .filter(entry_checklist_item::Column::EntryId.is_in(entry_ids))
            .order_by_asc(entry_checklist_item::Column::Position)
            .order_by_asc(entry_checklist_item::Column::Id)
            .all(db)
            .await?
    };
    let mut checklist_items_by_entry = HashMap::<i64, Vec<ChecklistItemDTO>>::new();
    for item in checklist_items {
        checklist_items_by_entry
            .entry(item.entry_id)
            .or_default()
            .push(ChecklistItemDTO::from(item));
    }

    for card in &mut cards {
        for entry in &mut card.entries {
            entry.labels = labels_by_entry
                .remove(&(entry.id as i64))
                .unwrap_or_default();
            entry.checklist_items = checklist_items_by_entry
                .remove(&(entry.id as i64))
                .unwrap_or_default();
            entry.attachments = attachments_by_entry
                .remove(&(entry.id as i64))
                .unwrap_or_default();
        }
    }

    Ok((cards, board_labels))
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
