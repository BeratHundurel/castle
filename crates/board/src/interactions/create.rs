use super::*;

impl BoardView {
    pub(crate) fn duplicate_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_editing.dialog.entry_id else {
            return;
        };
        let Some(source) = self
            .data
            .lists
            .iter()
            .flat_map(|list| list.entries.iter())
            .find(|entry| entry.id == entry_id)
            .cloned()
        else {
            return;
        };
        self.duplicate_entry(source, cx);
    }

    pub(crate) fn duplicate_entry(&mut self, source: BoardCardDTO, cx: &mut Context<Self>) {
        let board_id = self.data.board_id;
        let task = cx
            .global::<AppServices>()
            .spawn_store(move |store| async move {
                storage::board::commands::duplicate_board_card(
                    &store,
                    board_card_draft(source),
                    app_services::now_ts(),
                )
                .await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(Ok(())) => {
                    this.mutation.mutation_error = None;
                    if let Some(board_id) = board_id {
                        this.enrich_board_async(cx, board_id);
                        cx.emit(BoardViewEvent::DataCommitted {
                            board_id,
                            links_changed: true,
                        });
                    }
                }
                Ok(Err(error)) => {
                    this.mutation.mutation_error =
                        Some(format!("Could not duplicate card: {error}").into());
                    if let Some(board_id) = board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                Err(error) => {
                    this.mutation.mutation_error =
                        Some(format!("Card duplication task failed: {error}").into());
                    if let Some(board_id) = board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn duplicate_card(&mut self, card_id: u32, cx: &mut Context<Self>) {
        let Some(source) = self
            .data
            .lists
            .iter()
            .find(|card| card.id == card_id)
            .cloned()
        else {
            return;
        };
        let board_id = self.data.board_id;
        let task = cx
            .global::<AppServices>()
            .spawn_store(move |store| async move {
                storage::board::commands::duplicate_board_list(
                    &store,
                    board_list_draft(source),
                    app_services::now_ts(),
                )
                .await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(Ok(())) => {
                    this.mutation.mutation_error = None;
                    if let Some(board_id) = board_id {
                        this.enrich_board_async(cx, board_id);
                        cx.emit(BoardViewEvent::DataCommitted {
                            board_id,
                            links_changed: true,
                        });
                    }
                }
                Ok(Err(error)) => {
                    this.mutation.mutation_error =
                        Some(format!("Could not duplicate list: {error}").into());
                    if let Some(board_id) = board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                Err(error) => {
                    this.mutation.mutation_error =
                        Some(format!("List duplication task failed: {error}").into());
                    if let Some(board_id) = board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
            })
            .ok();
        })
        .detach();
    }
    pub(crate) fn entry_values(
        &self,
        entry_id: u32,
    ) -> Option<(SharedString, SharedString, Option<SharedString>)> {
        self.data
            .lists
            .iter()
            .flat_map(|card| card.entries.iter())
            .find(|entry| entry.id == entry_id)
            .map(|entry| {
                (
                    entry.title.clone(),
                    entry.description.clone(),
                    entry.due_on.clone(),
                )
            })
    }

    pub(crate) fn next_card_id(&mut self) -> u32 {
        self.entry_editing.next_temporary_list_id =
            self.entry_editing.next_temporary_list_id.saturating_add(1);
        u32::MAX.saturating_sub(self.entry_editing.next_temporary_list_id)
    }

    pub(crate) fn next_entry_id(&mut self) -> u32 {
        self.entry_editing.next_temporary_card_id =
            self.entry_editing.next_temporary_card_id.saturating_add(1);
        u32::MAX.saturating_sub(self.entry_editing.next_temporary_card_id)
    }

    pub(crate) fn add_entry(&mut self, cx: &mut Context<Self>, entry: BoardCardDTO, temp_id: u32) {
        let card_id = entry.card_id;

        if let Some(card) = self
            .data
            .lists
            .iter_mut()
            .find(|card| card.id == entry.card_id)
        {
            card.entries.push(entry.clone());
            cx.notify();
        }

        let task = cx
            .global::<AppServices>()
            .spawn_store(move |store| async move {
                storage::board::commands::create_board_card(
                    &store,
                    board_card_draft(entry),
                    app_services::now_ts(),
                )
                .await
            });

        cx.spawn(async move |this, cx| {
            let result = task.await;

            this.update(cx, |this, cx| match result {
                Ok(Ok(inserted)) => {
                    this.mutation.mutation_error = None;
                    let real_id = inserted.id;
                    if let Some(entry) = this
                        .data
                        .lists
                        .iter_mut()
                        .find(|card| card.id == card_id)
                        .and_then(|card| card.entries.iter_mut().find(|entry| entry.id == temp_id))
                    {
                        entry.id = real_id;
                    }
                    if this.entry_editing.dialog.entry_id == Some(temp_id) {
                        this.entry_editing.dialog.entry_id = Some(real_id);
                    }
                    if let Some(board_id) = this.data.board_id {
                        cx.emit(BoardViewEvent::DataCommitted {
                            board_id,
                            links_changed: true,
                        });
                    }
                }
                Ok(Err(error)) => {
                    this.mutation.mutation_error =
                        Some(format!("Could not create card: {error}").into());
                    if let Some(board_id) = this.data.board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                Err(error) => {
                    this.mutation.mutation_error =
                        Some(format!("Card creation task failed: {error}").into());
                    if let Some(board_id) = this.data.board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn add_card(&mut self, cx: &mut Context<Self>, card: BoardListDTO, temp_id: u32) {
        let board_id = card.board_id;

        self.data.lists.push(card.clone());
        cx.notify();

        let task = cx
            .global::<AppServices>()
            .spawn_store(move |store| async move {
                storage::board::commands::create_board_list(&store, board_list_draft(card)).await
            });

        cx.spawn(async move |this, cx| {
            let result = task.await;

            this.update(cx, |this, cx| match result {
                Ok(Ok(inserted)) => {
                    this.mutation.mutation_error = None;
                    let real_id = inserted.id;
                    if this.data.board_id == Some(board_id)
                        && let Some(card) =
                            this.data.lists.iter_mut().find(|card| card.id == temp_id)
                    {
                        card.id = real_id;
                    }
                    cx.emit(BoardViewEvent::DataCommitted {
                        board_id,
                        links_changed: false,
                    });
                }
                Ok(Err(error)) => {
                    this.mutation.mutation_error =
                        Some(format!("Could not create list: {error}").into());
                    this.enrich_board_async(cx, board_id);
                }
                Err(error) => {
                    this.mutation.mutation_error =
                        Some(format!("List creation task failed: {error}").into());
                    this.enrich_board_async(cx, board_id);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn rename_card(&mut self, cx: &mut Context<Self>, new_title: &str) {
        let Some(card_id) = self.entry_editing.renaming_list_id else {
            return;
        };

        let title = new_title.to_string();
        let Some(card) = self.data.lists.iter_mut().find(|card| card.id == card_id) else {
            return;
        };

        card.title = SharedString::from(new_title);
        self.entry_editing.renaming_list_id = None;
        cx.notify();

        self.commit_board_mutation(
            cx,
            "Could not rename list",
            false,
            move |store| async move {
                storage::board::commands::rename_board_list(&store, card_id, title).await
            },
        );
    }

    pub(crate) fn show_add_entry_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let board_view = cx.entity();
        let dialog_title_input = self.entry_editing.dialog_title_input.clone();
        let dialog_description_input = self.entry_editing.dialog_description_input.clone();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .on_ok({
                    let board_view = board_view.clone();
                    move |_, window, cx| {
                        board_view.update(cx, |this, cx| {
                            let Some(card_id) = this.entry_editing.pending_list_id else {
                                return;
                            };

                            let entry_id = this.next_entry_id();
                            let entry = BoardCardDTO {
                                id: entry_id,
                                title: this.entry_editing.dialog_title_input.read(cx).value(),
                                description: this
                                    .entry_editing
                                    .dialog_description_input
                                    .read(cx)
                                    .value(),
                                card_id,
                                position: this
                                    .data
                                    .lists
                                    .iter()
                                    .find(|card| card.id == card_id)
                                    .map(|card| card.entries.len() as i32)
                                    .unwrap_or_default(),
                                due_on: None,
                                reminder_enabled: false,
                                labels: vec![],
                                checklist_items: vec![],
                                attachments: vec![],
                                related_notes: vec![],
                            };

                            this.entry_editing
                                .dialog_title_input
                                .update(cx, |input, cx| {
                                    input.set_value("", window, cx);
                                });
                            this.entry_editing
                                .dialog_description_input
                                .update(cx, |input, cx| {
                                    input.set_value("", window, cx);
                                });

                            this.entry_editing.pending_list_id = None;
                            this.add_entry(cx, entry, entry_id);
                        });

                        true
                    }
                })
                .child(
                    DialogHeader::new()
                        .mb_2()
                        .child(DialogTitle::new().child("Add a card"))
                        .child(
                            DialogDescription::new()
                                .child("Add a title and an optional description."),
                        ),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .mb_3()
                        .child(Input::new(&dialog_title_input))
                        .child(Editor::new(&dialog_description_input).h(gpui::rems(6.))),
                )
                .child(
                    DialogFooter::new()
                        .justify_between()
                        .child(DialogClose::new().child(
                            Button::new("cancel").label("Cancel").outline().on_click({
                                move |_, window, cx| {
                                    window.close_dialog(cx);
                                }
                            }),
                        ))
                        .child(
                            DialogAction::new()
                                .child(Button::new("confirm").primary().label("Add card")),
                        ),
                )
        });
    }
}

fn board_card_draft(card: BoardCardDTO) -> storage::board::commands::BoardCardDraft {
    storage::board::commands::BoardCardDraft {
        title: card.title.to_string(),
        description: card.description.to_string(),
        list_id: card.card_id,
        position: card.position,
        due_on: card.due_on.map(|value| value.to_string()),
        label_ids: card.labels.into_iter().map(|label| label.id).collect(),
        checklist_items: card
            .checklist_items
            .into_iter()
            .map(|item| storage::board::commands::ChecklistItemDraft {
                title: item.title.to_string(),
                checked: item.checked,
                position: item.position,
            })
            .collect(),
    }
}

fn board_list_draft(list: BoardListDTO) -> storage::board::commands::BoardListDraft {
    storage::board::commands::BoardListDraft {
        title: list.title.to_string(),
        board_id: list.board_id,
        position: list.position,
        cards: list.entries.into_iter().map(board_card_draft).collect(),
    }
}
