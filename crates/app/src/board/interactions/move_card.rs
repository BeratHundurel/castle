use super::*;

impl BoardView {
    pub(in crate::board) fn delete_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_dialog.entry_id else {
            return;
        };

        for card in &mut self.cards {
            card.entries.retain(|entry| entry.id != entry_id);
        }

        self.is_entry_open = false;
        self.entry_dialog.open = false;
        self.entry_dialog.entry_id = None;
        self.entry_dialog.editing = false;
        cx.notify();

        let db = cx.global::<DB>().conn.clone();

        let _task = tokio::runtime::Handle::current().spawn(async move {
            crate::trash::move_to_trash(
                db.as_ref(),
                crate::trash::MoveToTrash {
                    kind: crate::trash::TrashItemKind::Entry,
                    id: entry_id,
                },
                crate::document_editor::now_ts(),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        });
    }

    pub(in crate::board) fn persist_card_positions(&mut self, cx: &mut Context<Self>) {
        let positions: Vec<(u32, i32)> = self
            .cards
            .iter_mut()
            .enumerate()
            .map(|(index, card)| {
                card.position = index as i32;
                (card.id, card.position)
            })
            .collect();

        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let _task = tokio::runtime::Handle::current().spawn(async move {
            for (card_id, position) in positions {
                let model = card::ActiveModel {
                    id: Set(card_id as i64),
                    position: Set(position),
                    ..Default::default()
                };
                model.update(&*db).await?;
            }

            Ok::<(), sea_orm::DbErr>(())
        });
    }

    pub(in crate::board) fn move_card(
        &mut self,
        info: &CardDragInfo,
        target_card_id: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.board_id else {
            return;
        };

        if info.source_board_id != board_id || info.card_id == target_card_id {
            return;
        }

        let Some(from_index) = self.cards.iter().position(|card| card.id == info.card_id) else {
            return;
        };
        let Some(to_index) = self.cards.iter().position(|card| card.id == target_card_id) else {
            return;
        };

        let moved_card = self.cards.remove(from_index);
        self.cards.insert(to_index, moved_card);
        self.persist_card_positions(cx);
    }

    pub(in crate::board) fn move_card_to_end(
        &mut self,
        info: &CardDragInfo,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.board_id else {
            return;
        };

        if info.source_board_id != board_id {
            return;
        }

        let Some(from_index) = self.cards.iter().position(|card| card.id == info.card_id) else {
            return;
        };

        if from_index + 1 == self.cards.len() {
            return;
        }

        let moved_card = self.cards.remove(from_index);
        self.cards.push(moved_card);
        self.persist_card_positions(cx);
    }

    pub(in crate::board) fn delete_card(&mut self, cx: &mut Context<Self>, card_id: u32) {
        self.cards.retain(|card| card.id != card_id);
        cx.notify();

        let db = cx.global::<DB>().conn.clone();

        let _task = tokio::runtime::Handle::current().spawn(async move {
            crate::trash::move_to_trash(
                db.as_ref(),
                crate::trash::MoveToTrash {
                    kind: crate::trash::TrashItemKind::List,
                    id: card_id,
                },
                crate::document_editor::now_ts(),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        });
    }
}
