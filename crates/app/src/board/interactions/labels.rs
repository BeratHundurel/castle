use super::*;

impl BoardView {
    pub(in crate::board) fn create_board_label(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(board_id) = self.board_id else {
            return;
        };

        let color = self.selected_label_color.to_string();
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();

        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    board_label::ActiveModel {
                        board_id: Set(board_id as i64),
                        name: Set(name),
                        color: Set(color),
                        ..Default::default()
                    }
                    .insert(&*db)
                    .await
                })
                .await;

            this.update(cx, |this, cx| match result {
                Ok(Ok(inserted)) if this.board_id == Some(board_id) => {
                    this.mutation_error = None;
                    this.board_labels.push(BoardLabelDTO::from(inserted));
                    cx.emit(BoardViewEvent::DataCommitted {
                        board_id,
                        links_changed: false,
                    });
                    cx.notify();
                }
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    this.mutation_error = Some(format!("Could not create label: {error}").into());
                    if this.board_id == Some(board_id) {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                Err(error) => {
                    this.mutation_error =
                        Some(format!("Label creation task failed: {error}").into());
                    if this.board_id == Some(board_id) {
                        this.enrich_board_async(cx, board_id);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::board) fn rename_board_label(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(label_id) = self.renaming_label_id else {
            return;
        };
        let Some(label) = self
            .board_labels
            .iter_mut()
            .find(|label| label.id == label_id)
        else {
            return;
        };

        label.name = SharedString::from(name.as_str());
        self.renaming_label_id = None;
        self.cards
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .for_each(|card| {
                if let Some(label) = card.labels.iter_mut().find(|label| label.id == label_id) {
                    label.name = SharedString::from(name.as_str());
                }
            });
        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        self.commit_board_mutation(cx, "Could not rename label", false, async move {
            board_label::ActiveModel {
                id: Set(label_id as i64),
                name: Set(name),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            Ok::<(), anyhow::Error>(())
        });
    }

    pub(in crate::board) fn set_entry_label_assignment(
        &mut self,
        entry_id: u32,
        label_id: u32,
        assigned: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(label) = self
            .board_labels
            .iter()
            .find(|label| label.id == label_id)
            .cloned()
        else {
            return;
        };
        let Some(entry) = self
            .cards
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

        let db = cx.global::<DB>().conn.clone();
        self.commit_board_mutation(cx, "Could not update card label", false, async move {
            if assigned {
                entry_label::ActiveModel {
                    entry_id: Set(entry_id as i64),
                    board_label_id: Set(label_id as i64),
                    ..Default::default()
                }
                .insert(&*db)
                .await?;
            } else {
                EntryLabel::delete_many()
                    .filter(entry_label::Column::EntryId.eq(entry_id as i64))
                    .filter(entry_label::Column::BoardLabelId.eq(label_id as i64))
                    .exec(&*db)
                    .await?;
            }

            Ok::<(), anyhow::Error>(())
        });
    }

    pub(in crate::board) fn delete_board_label(&mut self, label_id: u32, cx: &mut Context<Self>) {
        self.board_labels.retain(|label| label.id != label_id);
        self.filters.label_ids.remove(&label_id);
        self.cards
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .for_each(|card| card.labels.retain(|label| label.id != label_id));
        self.renaming_label_id = None;
        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        self.commit_board_mutation(cx, "Could not delete label", false, async move {
            BoardLabel::delete_by_id(label_id as i64).exec(&*db).await?;
            Ok::<(), anyhow::Error>(())
        });
    }
}
