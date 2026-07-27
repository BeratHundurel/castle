use super::*;

impl BoardView {
    pub(in crate::board) fn move_entry(
        &mut self,
        info: &DragInfo,
        target_card_id: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.board_id else {
            return;
        };

        if info.source_board_id != board_id
            || !self.cards.iter().any(|card| {
                card.id == info.source_card_id
                    && card.entries.iter().any(|entry| entry.id == info.entry_id)
            })
        {
            return;
        }

        self.move_entry_to_list_end(info.entry_id, target_card_id, cx);
    }

    pub(in crate::board) fn move_entry_to_list_end(
        &mut self,
        entry_id: u32,
        target_card_id: u32,
        cx: &mut Context<Self>,
    ) {
        if move_entry_to_list_end_in_memory(&mut self.cards, entry_id, target_card_id) {
            self.persist_entry_positions(cx);
        }
    }

    pub(in crate::board) fn move_entry_before(
        &mut self,
        info: &DragInfo,
        target_card_id: u32,
        target_entry_id: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.board_id else {
            return;
        };

        if info.source_board_id != board_id || info.entry_id == target_entry_id {
            return;
        }

        let source_index = self
            .cards
            .iter()
            .find(|card| card.id == info.source_card_id)
            .and_then(|card| {
                card.entries
                    .iter()
                    .position(|entry| entry.id == info.entry_id)
            });

        let target_index = self
            .cards
            .iter()
            .find(|card| card.id == target_card_id)
            .and_then(|card| {
                card.entries
                    .iter()
                    .position(|entry| entry.id == target_entry_id)
            });

        let moving_down_in_same_card = info.source_card_id == target_card_id
            && matches!(
                (source_index, target_index),
                (Some(source_index), Some(target_index)) if source_index < target_index
            );

        let moving_entry = self
            .cards
            .iter_mut()
            .find(|card| card.id == info.source_card_id)
            .and_then(|card| {
                let index = card
                    .entries
                    .iter()
                    .position(|entry| entry.id == info.entry_id)?;

                Some(card.entries.remove(index))
            });

        if let Some(mut dto) = moving_entry
            && let Some(target_card) = self.cards.iter_mut().find(|card| card.id == target_card_id)
        {
            let Some(mut target_index) = target_card
                .entries
                .iter()
                .position(|entry| entry.id == target_entry_id)
            else {
                return;
            };

            dto.card_id = target_card_id;
            if moving_down_in_same_card {
                target_index = target_index.saturating_add(1);
            }
            target_card.entries.insert(target_index, dto);
            self.persist_entry_positions(cx);
        }
    }

    pub(in crate::board) fn persist_entry_positions(&mut self, cx: &mut Context<Self>) {
        let positions = normalize_entry_positions(&mut self.cards);

        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let _task = tokio::runtime::Handle::current()
            .spawn(async move { persist_entry_positions_in_db(db.as_ref(), positions).await });
    }
}
