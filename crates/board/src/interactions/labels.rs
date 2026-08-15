use super::*;

impl BoardView {
    pub(crate) fn create_board_label(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(board_id) = self.data.board_id else {
            return;
        };

        let color = self.entry_editing.selected_label_color.to_string();
        let db = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();

        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_commands::create_label(&db, board_id, name, color).await
                })
                .await;

            this.update(cx, |this, cx| match result {
                Ok(Ok(inserted)) if this.data.board_id == Some(board_id) => {
                    this.mutation.mutation_error = None;
                    this.data.labels.push(BoardLabelDTO::from(inserted));
                    cx.emit(BoardViewEvent::DataCommitted {
                        board_id,
                        links_changed: false,
                    });
                    cx.notify();
                }
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    this.mutation.mutation_error =
                        Some(format!("Could not create label: {error}").into());
                    if this.data.board_id == Some(board_id) {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                Err(error) => {
                    this.mutation.mutation_error =
                        Some(format!("Label creation task failed: {error}").into());
                    if this.data.board_id == Some(board_id) {
                        this.enrich_board_async(cx, board_id);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn rename_board_label(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(label_id) = self.entry_editing.renaming_label_id else {
            return;
        };
        let Some(label) = self
            .data
            .labels
            .iter_mut()
            .find(|label| label.id == label_id)
        else {
            return;
        };

        label.name = SharedString::from(name.as_str());
        self.entry_editing.renaming_label_id = None;
        self.data
            .lists
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .for_each(|card| {
                if let Some(label) = card.labels.iter_mut().find(|label| label.id == label_id) {
                    label.name = SharedString::from(name.as_str());
                }
            });
        cx.notify();

        let db = cx.global::<AppServices>().store();
        self.commit_board_mutation(cx, "Could not rename label", false, async move {
            storage::board_commands::rename_label(&db, label_id, name).await
        });
    }

    pub(crate) fn set_entry_label_assignment(
        &mut self,
        entry_id: u32,
        label_id: u32,
        assigned: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(label) = self
            .data
            .labels
            .iter()
            .find(|label| label.id == label_id)
            .cloned()
        else {
            return;
        };
        let Some(entry) = self
            .data
            .lists
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .find(|card| card.id == entry_id)
        else {
            return;
        };

        if assigned {
            if entry
                .labels
                .iter()
                .any(|entry_label| entry_label.id == label_id)
            {
                return;
            }
            entry.labels.push(label);
        } else {
            entry
                .labels
                .retain(|entry_label| entry_label.id != label_id);
        }
        cx.notify();

        let db = cx.global::<AppServices>().store();
        self.commit_board_mutation(cx, "Could not update card label", false, async move {
            storage::board_commands::set_label_assignment(&db, entry_id, label_id, assigned).await
        });
    }

    pub(crate) fn delete_board_label(&mut self, label_id: u32, cx: &mut Context<Self>) {
        self.data.labels.retain(|label| label.id != label_id);
        self.filters.label_ids.remove(&label_id);
        self.data
            .lists
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .for_each(|card| card.labels.retain(|label| label.id != label_id));
        self.entry_editing.renaming_label_id = None;
        cx.notify();

        let db = cx.global::<AppServices>().store();
        self.commit_board_mutation(cx, "Could not delete label", false, async move {
            storage::board_commands::delete_label(&db, label_id).await
        });
    }
}
