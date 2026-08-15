use super::*;

impl BoardView {
    pub(crate) fn create_checklist_item(&mut self, title: String, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_editing.dialog.entry_id else {
            return;
        };
        let Some(next_position) = self
            .data
            .lists
            .iter()
            .flat_map(|list| list.entries.iter())
            .find(|card| card.id == entry_id)
            .map(|entry| {
                entry
                    .checklist_items
                    .iter()
                    .map(|item| item.position)
                    .max()
                    .unwrap_or(-1)
                    .saturating_add(1)
            })
        else {
            return;
        };

        let position = std::cmp::max(
            self.entry_editing.next_checklist_item_position,
            next_position,
        );
        self.entry_editing.next_checklist_item_position = position.saturating_add(1);

        let db = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_commands::create_checklist_item(&db, entry_id, title, position)
                        .await
                })
                .await;
            this.update(cx, |this, cx| match result {
                Ok(Ok(inserted)) => {
                    let Some(entry) = this
                        .data
                        .lists
                        .iter_mut()
                        .flat_map(|list| list.entries.iter_mut())
                        .find(|card| card.id == entry_id)
                    else {
                        return;
                    };
                    entry.checklist_items.push(ChecklistItemDTO::from(inserted));
                    this.mutation.mutation_error = None;
                    this.emit_data_committed(cx, false);
                    cx.notify();
                }
                Ok(Err(error)) => {
                    this.mutation.mutation_error =
                        Some(format!("Could not create checklist item: {error}").into());
                    if let Some(board_id) = this.data.board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                Err(error) => {
                    this.mutation.mutation_error =
                        Some(format!("Checklist creation task failed: {error}").into());
                    if let Some(board_id) = this.data.board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn set_checklist_item_checked(
        &mut self,
        item_id: u32,
        checked: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self
            .data
            .lists
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .flat_map(|card| card.checklist_items.iter_mut())
            .find(|item| item.id == item_id)
        else {
            return;
        };
        item.checked = checked;
        cx.notify();

        let db = cx.global::<AppServices>().store();
        self.commit_board_mutation(cx, "Could not update checklist item", false, async move {
            storage::board_commands::update_checklist_item(&db, item_id, None, Some(checked)).await
        });
    }

    pub(crate) fn delete_checklist_item(&mut self, item_id: u32, cx: &mut Context<Self>) {
        for card in self
            .data
            .lists
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
        {
            card.checklist_items.retain(|item| item.id != item_id);
        }
        cx.notify();

        let db = cx.global::<AppServices>().store();
        self.commit_board_mutation(cx, "Could not delete checklist item", false, async move {
            storage::board_commands::delete_checklist_item(&db, item_id).await
        });
    }

    pub(crate) fn move_checklist_item(
        &mut self,
        item_id: u32,
        direction: isize,
        cx: &mut Context<Self>,
    ) {
        let Some(items) = self
            .data
            .lists
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .find_map(|card| {
                card.checklist_items
                    .iter()
                    .any(|item| item.id == item_id)
                    .then_some(&mut card.checklist_items)
            })
        else {
            return;
        };
        let Some(index) = items.iter().position(|item| item.id == item_id) else {
            return;
        };
        let Some(target) = index.checked_add_signed(direction) else {
            return;
        };
        if target >= items.len() {
            return;
        }
        items.swap(index, target);
        let positions = items
            .iter_mut()
            .enumerate()
            .map(|(position, item)| {
                item.position = position as i32;
                (item.id, item.position)
            })
            .collect::<Vec<_>>();
        cx.notify();

        let db = cx.global::<AppServices>().store();
        self.commit_board_mutation(cx, "Could not reorder checklist", false, async move {
            storage::board_commands::reorder_checklist_items(&db, positions).await
        });
    }

    pub(crate) fn rename_checklist_item(&mut self, title: String, cx: &mut Context<Self>) {
        let Some(item_id) = self.entry_editing.renaming_checklist_item_id else {
            return;
        };
        let Some(item) = self
            .data
            .lists
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .flat_map(|card| card.checklist_items.iter_mut())
            .find(|item| item.id == item_id)
        else {
            return;
        };
        item.title = SharedString::from(title.as_str());
        self.entry_editing.renaming_checklist_item_id = None;
        cx.notify();
        let db = cx.global::<AppServices>().store();
        self.commit_board_mutation(cx, "Could not rename checklist item", false, async move {
            storage::board_commands::update_checklist_item(&db, item_id, Some(title), None).await
        });
    }
}
