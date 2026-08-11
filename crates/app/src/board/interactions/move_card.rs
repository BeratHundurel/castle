use super::*;

impl BoardView {
    pub(in crate::board) fn delete_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_editing.dialog.entry_id else {
            return;
        };

        for card in &mut self.data.lists {
            card.entries.retain(|entry| entry.id != entry_id);
        }

        self.entry_editing.open = false;
        self.entry_editing.dialog.open = false;
        self.entry_editing.dialog.entry_id = None;
        self.entry_editing.dialog.editing = false;
        cx.notify();

        let db = cx.global::<AppServices>().store().connection();
        self.commit_board_mutation(cx, "Could not delete card", true, async move {
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
        self.persist_board_layout(cx);
    }

    pub(in crate::board) fn move_card(
        &mut self,
        info: &CardDragInfo,
        target_card_id: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.data.board_id else {
            return;
        };

        if info.source_board_id != board_id || info.card_id == target_card_id {
            return;
        }

        let Some(from_index) = self
            .data
            .lists
            .iter()
            .position(|card| card.id == info.card_id)
        else {
            return;
        };
        let Some(to_index) = self
            .data
            .lists
            .iter()
            .position(|card| card.id == target_card_id)
        else {
            return;
        };

        let moved_card = self.data.lists.remove(from_index);
        self.data.lists.insert(to_index, moved_card);
        self.persist_card_positions(cx);
    }

    pub(in crate::board) fn move_card_to_end(
        &mut self,
        info: &CardDragInfo,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.data.board_id else {
            return;
        };

        if info.source_board_id != board_id {
            return;
        }

        let Some(from_index) = self
            .data
            .lists
            .iter()
            .position(|card| card.id == info.card_id)
        else {
            return;
        };

        if from_index + 1 == self.data.lists.len() {
            return;
        }

        let moved_card = self.data.lists.remove(from_index);
        self.data.lists.push(moved_card);
        self.persist_card_positions(cx);
    }

    pub(in crate::board) fn delete_card(&mut self, cx: &mut Context<Self>, card_id: u32) {
        self.data.lists.retain(|card| card.id != card_id);
        cx.notify();

        let db = cx.global::<AppServices>().store().connection();
        self.commit_board_mutation(cx, "Could not delete list", true, async move {
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
