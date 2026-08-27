use super::*;

impl BoardView {
    pub(crate) fn move_entry(
        &mut self,
        info: &DragInfo,
        target_card_id: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.data.board_id else {
            return;
        };

        if info.source_board_id != board_id
            || !self.data.lists.iter().any(|card| {
                card.id == info.source_card_id
                    && card.entries.iter().any(|entry| entry.id == info.entry_id)
            })
        {
            return;
        }

        self.move_entry_to_list_end(info.entry_id, target_card_id, cx);
    }

    pub(crate) fn move_entry_to_list_end(
        &mut self,
        entry_id: u32,
        target_card_id: u32,
        cx: &mut Context<Self>,
    ) {
        if move_entry_to_list_end_in_memory(&mut self.data.lists, entry_id, target_card_id) {
            self.persist_board_layout(cx);
        }
    }

    pub(crate) fn move_entry_before(
        &mut self,
        info: &DragInfo,
        target_card_id: u32,
        target_entry_id: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.data.board_id else {
            return;
        };

        if info.source_board_id != board_id || info.entry_id == target_entry_id {
            return;
        }

        let source_index = self
            .data
            .lists
            .iter()
            .find(|card| card.id == info.source_card_id)
            .and_then(|card| {
                card.entries
                    .iter()
                    .position(|entry| entry.id == info.entry_id)
            });

        let target_index = self
            .data
            .lists
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
            .data
            .lists
            .iter_mut()
            .find(|card| card.id == info.source_card_id)
            .and_then(|card| {
                let index = card
                    .entries
                    .iter()
                    .position(|entry| entry.id == info.entry_id)?;

                Some(card.entries.remove(index))
            });

        if let Some(mut entry) = moving_entry
            && let Some(target_card) = self
                .data
                .lists
                .iter_mut()
                .find(|card| card.id == target_card_id)
        {
            let Some(mut target_index) = target_card
                .entries
                .iter()
                .position(|entry| entry.id == target_entry_id)
            else {
                return;
            };

            entry.card_id = target_card_id;
            if moving_down_in_same_card {
                target_index = target_index.saturating_add(1);
            }
            target_card.entries.insert(target_index, entry);
            self.persist_board_layout(cx);
        }
    }

    pub(crate) fn persist_board_layout(&mut self, cx: &mut Context<Self>) {
        let entries = normalize_entry_positions(&mut self.data.lists);
        let lists = self
            .data
            .lists
            .iter_mut()
            .enumerate()
            .map(|(position, list)| {
                list.position = position as i32;
                (list.id, list.position)
            })
            .collect();
        self.mutation.local_generation = self.mutation.local_generation.saturating_add(1);
        let mutation_generation = self.mutation.local_generation;

        cx.notify();

        let Some(board_id) = self.data.board_id else {
            return;
        };
        let db = cx.global::<AppRuntime>();
        let persistence = cx.global::<crate::BoardServices>().layout_persistence();
        let runtime = db.tokio_handle();
        let revision = match persistence.submit(
            board_id,
            db.store(),
            storage::board::positions::BoardLayoutSnapshot { lists, entries },
        ) {
            Ok(revision) => revision,
            Err(error) => {
                self.mutation.mutation_error =
                    Some(format!("Could not queue board layout: {error}").into());
                self.enrich_board_async(cx, board_id);
                return;
            }
        };
        self.mutation.layout_commit_task = Some(cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move { persistence.wait_for_revision(board_id, revision).await })
                .await;
            this.update(cx, |this, cx| {
                if this.data.board_id != Some(board_id)
                    || this.mutation.local_generation != mutation_generation
                {
                    return;
                }
                match result {
                    Ok(Ok(())) => {
                        this.mutation.mutation_error = None;
                        cx.emit(super::super::BoardViewEvent::DataCommitted {
                            board_id,
                            links_changed: false,
                        });
                    }
                    Ok(Err(error)) => {
                        this.mutation.mutation_error =
                            Some(format!("Could not save board layout: {error}").into());
                        this.enrich_board_async(cx, board_id);
                    }
                    Err(error) => {
                        this.mutation.mutation_error =
                            Some(format!("Board layout task failed: {error}").into());
                        this.enrich_board_async(cx, board_id);
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }
}
