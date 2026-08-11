use super::*;

impl BoardView {
    pub(in crate::board) fn duplicate_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_dialog.entry_id else {
            return;
        };
        let Some(source) = self
            .cards
            .iter()
            .flat_map(|list| list.entries.iter())
            .find(|entry| entry.id == entry_id)
            .cloned()
        else {
            return;
        };
        self.duplicate_entry(source, cx);
    }

    pub(in crate::board) fn duplicate_entry(&mut self, source: EntryDTO, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();
        let board_id = self.board_id;
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    let txn = db.begin().await?;
                    let description = source.description.to_string();
                    Entry::update_many()
                        .col_expr(
                            entry::Column::Position,
                            Expr::col(entry::Column::Position).add(1),
                        )
                        .filter(entry::Column::CardId.eq(source.card_id as i64))
                        .filter(entry::Column::Position.gte(source.position + 1))
                        .exec(&txn)
                        .await?;
                    let inserted = entry::ActiveModel {
                        title: Set(format!("Copy of {}", source.title)),
                        description: Set(description.clone()),
                        card_id: Set(source.card_id as i64),
                        position: Set(source.position + 1),
                        due_on: Set(source.due_on.map(|value| value.to_string())),
                        ..Default::default()
                    }
                    .insert(&txn)
                    .await?;
                    for label in source.labels {
                        entry_label::ActiveModel {
                            entry_id: Set(inserted.id),
                            board_label_id: Set(label.id as i64),
                            ..Default::default()
                        }
                        .insert(&txn)
                        .await?;
                    }
                    for item in source.checklist_items {
                        entry_checklist_item::ActiveModel {
                            entry_id: Set(inserted.id),
                            title: Set(item.title.to_string()),
                            checked: Set(item.checked),
                            position: Set(item.position),
                            ..Default::default()
                        }
                        .insert(&txn)
                        .await?;
                    }
                    storage::workspace_links::index_entry_workspace_links_in_connection(
                        &txn,
                        inserted.id,
                        &description,
                        crate::document_editor::now_ts(),
                    )
                    .await?;
                    txn.commit().await?;
                    Ok::<(), anyhow::Error>(())
                })
                .await;
            this.update(cx, |this, cx| match result {
                Ok(Ok(())) => {
                    this.mutation_error = None;
                    if let Some(board_id) = board_id {
                        this.enrich_board_async(cx, board_id);
                        cx.emit(BoardViewEvent::DataCommitted {
                            board_id,
                            links_changed: true,
                        });
                    }
                }
                Ok(Err(error)) => {
                    this.mutation_error = Some(format!("Could not duplicate card: {error}").into());
                    if let Some(board_id) = board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                Err(error) => {
                    this.mutation_error =
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

    pub(in crate::board) fn duplicate_card(&mut self, card_id: u32, cx: &mut Context<Self>) {
        let Some(source) = self.cards.iter().find(|card| card.id == card_id).cloned() else {
            return;
        };
        let db = cx.global::<DB>().conn.clone();
        let board_id = self.board_id;
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    let txn = db.begin().await?;
                    let mut descriptions = Vec::new();
                    Card::update_many()
                        .col_expr(
                            card::Column::Position,
                            Expr::col(card::Column::Position).add(1),
                        )
                        .filter(card::Column::BoardId.eq(source.board_id as i64))
                        .filter(card::Column::Position.gte(source.position + 1))
                        .exec(&txn)
                        .await?;
                    let inserted_list = card::ActiveModel {
                        title: Set(format!("Copy of {}", source.title)),
                        board_id: Set(source.board_id as i64),
                        position: Set(source.position + 1),
                        ..Default::default()
                    }
                    .insert(&txn)
                    .await?;
                    for entry in source.entries {
                        let description = entry.description.to_string();
                        let inserted = entry::ActiveModel {
                            title: Set(entry.title.to_string()),
                            description: Set(description.clone()),
                            card_id: Set(inserted_list.id),
                            position: Set(entry.position),
                            due_on: Set(entry.due_on.map(|value| value.to_string())),
                            ..Default::default()
                        }
                        .insert(&txn)
                        .await?;
                        descriptions.push((inserted.id, description));
                        for label in entry.labels {
                            entry_label::ActiveModel {
                                entry_id: Set(inserted.id),
                                board_label_id: Set(label.id as i64),
                                ..Default::default()
                            }
                            .insert(&txn)
                            .await?;
                        }
                        for item in entry.checklist_items {
                            entry_checklist_item::ActiveModel {
                                entry_id: Set(inserted.id),
                                title: Set(item.title.to_string()),
                                checked: Set(item.checked),
                                position: Set(item.position),
                                ..Default::default()
                            }
                            .insert(&txn)
                            .await?;
                        }
                    }
                    for (entry_id, description) in descriptions {
                        storage::workspace_links::index_entry_workspace_links_in_connection(
                            &txn,
                            entry_id,
                            &description,
                            crate::document_editor::now_ts(),
                        )
                        .await?;
                    }
                    txn.commit().await?;
                    Ok::<(), anyhow::Error>(())
                })
                .await;
            this.update(cx, |this, cx| match result {
                Ok(Ok(())) => {
                    this.mutation_error = None;
                    if let Some(board_id) = board_id {
                        this.enrich_board_async(cx, board_id);
                        cx.emit(BoardViewEvent::DataCommitted {
                            board_id,
                            links_changed: true,
                        });
                    }
                }
                Ok(Err(error)) => {
                    this.mutation_error = Some(format!("Could not duplicate list: {error}").into());
                    if let Some(board_id) = board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                Err(error) => {
                    this.mutation_error =
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
    pub(in crate::board) fn entry_values(
        &self,
        entry_id: u32,
    ) -> Option<(SharedString, SharedString, Option<SharedString>)> {
        self.cards
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

    pub(in crate::board) fn next_card_id(&mut self) -> u32 {
        self.next_temporary_card_id = self.next_temporary_card_id.saturating_add(1);
        u32::MAX.saturating_sub(self.next_temporary_card_id)
    }

    pub(in crate::board) fn next_entry_id(&mut self) -> u32 {
        self.next_temporary_entry_id = self.next_temporary_entry_id.saturating_add(1);
        u32::MAX.saturating_sub(self.next_temporary_entry_id)
    }

    pub(in crate::board) fn add_entry(
        &mut self,
        cx: &mut Context<Self>,
        entry: EntryDTO,
        temp_id: u32,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        let card_id = entry.card_id;

        if let Some(card) = self.cards.iter_mut().find(|card| card.id == entry.card_id) {
            card.entries.push(entry.clone());
            cx.notify();
        }

        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    let description = entry.description.to_string();
                    let txn = db.begin().await?;
                    let inserted = entry::ActiveModel {
                        title: Set(entry.title.to_string()),
                        description: Set(description.clone()),
                        card_id: Set(entry.card_id as i64),
                        position: Set(entry.position),
                        due_on: Set(None),
                        ..Default::default()
                    }
                    .insert(&txn)
                    .await?;
                    storage::workspace_links::index_entry_workspace_links_in_connection(
                        &txn,
                        inserted.id,
                        &description,
                        crate::document_editor::now_ts(),
                    )
                    .await?;
                    txn.commit().await?;
                    Ok::<_, anyhow::Error>(inserted)
                })
                .await;

            this.update(cx, |this, cx| match result {
                Ok(Ok(inserted)) => {
                    this.mutation_error = None;
                    let real_id = inserted.id as u32;
                    if let Some(entry) = this
                        .cards
                        .iter_mut()
                        .find(|card| card.id == card_id)
                        .and_then(|card| card.entries.iter_mut().find(|entry| entry.id == temp_id))
                    {
                        entry.id = real_id;
                    }
                    if this.entry_dialog.entry_id == Some(temp_id) {
                        this.entry_dialog.entry_id = Some(real_id);
                    }
                    if let Some(board_id) = this.board_id {
                        cx.emit(BoardViewEvent::DataCommitted {
                            board_id,
                            links_changed: true,
                        });
                    }
                }
                Ok(Err(error)) => {
                    this.mutation_error = Some(format!("Could not create card: {error}").into());
                    if let Some(board_id) = this.board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                Err(error) => {
                    this.mutation_error =
                        Some(format!("Card creation task failed: {error}").into());
                    if let Some(board_id) = this.board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::board) fn add_card(
        &mut self,
        cx: &mut Context<Self>,
        card: CardDTO,
        temp_id: u32,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        let board_id = card.board_id;

        self.cards.push(card.clone());
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    card::ActiveModel {
                        title: Set(card.title.to_string()),
                        board_id: Set(card.board_id as i64),
                        position: Set(card.position),
                        ..Default::default()
                    }
                    .insert(&*db)
                    .await
                })
                .await;

            this.update(cx, |this, cx| match result {
                Ok(Ok(inserted)) => {
                    this.mutation_error = None;
                    let real_id = inserted.id as u32;
                    if this.board_id == Some(board_id)
                        && let Some(card) = this.cards.iter_mut().find(|card| card.id == temp_id)
                    {
                        card.id = real_id;
                    }
                    cx.emit(BoardViewEvent::DataCommitted {
                        board_id,
                        links_changed: false,
                    });
                }
                Ok(Err(error)) => {
                    this.mutation_error = Some(format!("Could not create list: {error}").into());
                    this.enrich_board_async(cx, board_id);
                }
                Err(error) => {
                    this.mutation_error =
                        Some(format!("List creation task failed: {error}").into());
                    this.enrich_board_async(cx, board_id);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::board) fn rename_card(&mut self, cx: &mut Context<Self>, new_title: &str) {
        let Some(card_id) = self.renaming_card_id else {
            return;
        };

        let title = new_title.to_string();
        let db = cx.global::<DB>().conn.clone();

        let Some(card) = self.cards.iter_mut().find(|card| card.id == card_id) else {
            return;
        };

        card.title = SharedString::from(new_title);
        self.renaming_card_id = None;
        cx.notify();

        self.commit_board_mutation(cx, "Could not rename list", false, async move {
            let model = card::ActiveModel {
                id: Set(card_id as i64),
                title: Set(title),
                ..Default::default()
            };
            model.update(&*db).await?;
            Ok::<(), anyhow::Error>(())
        });
    }

    pub(in crate::board) fn show_add_entry_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let board_view = cx.entity();
        let dialog_title_input = self.dialog_title_input.clone();
        let dialog_description_input = self.dialog_description_input.clone();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .on_ok({
                    let board_view = board_view.clone();
                    move |_, window, cx| {
                        board_view.update(cx, |this, cx| {
                            let Some(card_id) = this.pending_card_id else {
                                return;
                            };

                            let entry_id = this.next_entry_id();
                            let entry = EntryDTO {
                                id: entry_id,
                                title: this.dialog_title_input.read(cx).value(),
                                description: this.dialog_description_input.read(cx).value(),
                                card_id,
                                position: this
                                    .cards
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

                            this.dialog_title_input.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                            });
                            this.dialog_description_input.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                            });

                            this.pending_card_id = None;
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
                        .child(Input::new(&dialog_description_input)),
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
