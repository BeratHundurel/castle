use std::collections::HashMap;

use entity::{
    board_label, board_label::Entity as BoardLabel, card, card::Entity as Card,
    entry::Entity as Entry, entry_checklist_item,
    entry_checklist_item::Entity as EntryChecklistItem, entry_label,
    entry_label::Entity as EntryLabel,
};
use gpui::{Context, SharedString};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

use crate::DB;

use super::{BoardView, dto::*};

impl BoardView {
    pub(crate) fn load_board(&mut self, board_id: u32, cx: &mut Context<Self>) {
        if self.board_id == Some(board_id) {
            return;
        }

        self.board_id = Some(board_id);
        self.cards.clear();
        self.board_labels.clear();
        self.load_error = None;
        self.is_adding_list = false;
        self.next_checklist_item_position = 0;
        Self::enrich_board_async(cx, board_id);
    }

    pub(super) fn enrich_board_async(cx: &mut Context<Self>, board_id: u32) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| {
            let result = async {
                let mut cards = Card::load()
                    .filter(card::Column::BoardId.eq(board_id as i32))
                    .order_by_asc(card::Column::Position)
                    .order_by_asc(card::Column::Id)
                    .with(Entry)
                    .all(&*db)
                    .await?
                    .into_iter()
                    .map(CardDTO::from)
                    .collect::<Vec<_>>();

                let board_labels = BoardLabel::find()
                    .filter(board_label::Column::BoardId.eq(board_id as i64))
                    .order_by_asc(board_label::Column::Id)
                    .all(&*db)
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
                        .all(&*db)
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

                let checklist_items = if entry_ids.is_empty() {
                    vec![]
                } else {
                    EntryChecklistItem::find()
                        .filter(entry_checklist_item::Column::EntryId.is_in(entry_ids))
                        .order_by_asc(entry_checklist_item::Column::Position)
                        .order_by_asc(entry_checklist_item::Column::Id)
                        .all(&*db)
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
                    }
                }

                Ok::<_, sea_orm::DbErr>((cards, board_labels))
            }
            .await;

            this.update(cx, |this, cx| {
                if this.board_id == Some(board_id) {
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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use entity::{
        board, board::Entity as Board, board_label, board_label::Entity as BoardLabel, card,
        card::Entity as Card, entry, entry::Entity as Entry, entry_label,
        entry_label::Entity as EntryLabel,
    };
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{
        ActiveModelTrait, ActiveValue::Set, ColumnTrait, Database, EntityTrait, QueryFilter,
    };

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
}
