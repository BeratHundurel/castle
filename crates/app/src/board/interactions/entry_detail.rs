use super::*;

impl BoardView {
    pub(in crate::board) fn update_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_editing.dialog.entry_id else {
            return;
        };

        let title = self.entry_editing.title_input.read(cx).value();
        let description = self.entry_editing.description_input.read(cx).value();
        let trimmed_title = title.trim();

        if trimmed_title.is_empty() {
            return;
        }

        let Some(entry) = self
            .data
            .lists
            .iter_mut()
            .flat_map(|card| card.entries.iter_mut())
            .find(|entry| entry.id == entry_id)
        else {
            return;
        };

        let workspace_links_changed =
            storage::workspace_links::workspace_relation_signature(entry.description.as_ref())
                != storage::workspace_links::workspace_relation_signature(description.as_ref());
        entry.title = SharedString::from(trimmed_title);
        entry.description = description.clone();
        self.entry_editing.dialog.editing = false;
        cx.notify();

        let db = cx.global::<AppServices>().store();
        let title = trimmed_title.to_string();
        let description = description.to_string();
        let runtime = cx.global::<AppServices>().runtime();

        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_commands::update_board_card(
                        &db,
                        entry_id,
                        title,
                        description,
                        crate::now_ts(),
                    )
                    .await
                })
                .await;
            this.update(cx, |this, cx| match result {
                Ok(Ok(())) => {
                    this.mutation.mutation_error = None;
                    if let Some(board_id) = this.data.board_id {
                        if workspace_links_changed {
                            this.enrich_board_async(cx, board_id);
                        }
                        cx.emit(BoardViewEvent::DataCommitted {
                            board_id,
                            links_changed: workspace_links_changed,
                        });
                    }
                }
                Ok(Err(error)) => {
                    this.mutation.mutation_error =
                        Some(format!("Could not save card: {error}").into());
                    if let Some(board_id) = this.data.board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                Err(error) => {
                    this.mutation.mutation_error =
                        Some(format!("Card save task failed: {error}").into());
                    if let Some(board_id) = this.data.board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::board) fn update_selected_entry_due_on(
        &mut self,
        due_on: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(entry_id) = self.entry_editing.dialog.entry_id else {
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

        entry.due_on = due_on.as_deref().map(SharedString::from);
        cx.notify();

        self.entry_editing.next_due_date_update_revision = self
            .entry_editing
            .next_due_date_update_revision
            .saturating_add(1);
        let revision = self.entry_editing.next_due_date_update_revision;
        let persisted_revisions = self.entry_editing.persisted_due_date_revisions.clone();
        let db = cx.global::<AppServices>().store();
        self.commit_board_mutation(cx, "Could not save due date", false, async move {
            let mut persisted_revisions = persisted_revisions.lock().await;
            if persisted_revisions
                .get(&entry_id)
                .is_some_and(|persisted_revision| *persisted_revision >= revision)
            {
                return Ok::<(), anyhow::Error>(());
            }
            storage::board_commands::set_board_card_due_on(&db, entry_id, due_on).await?;
            persisted_revisions.insert(entry_id, revision);
            crate::system_notifications::wake();
            Ok::<(), anyhow::Error>(())
        });
    }

    pub(in crate::board) fn set_selected_entry_reminder(
        &mut self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(entry_id) = self.entry_editing.dialog.entry_id else {
            return;
        };
        let Some(entry) = self
            .data
            .lists
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .find(|entry| entry.id == entry_id)
        else {
            return;
        };
        if entry.due_on.is_none() {
            return;
        }

        entry.reminder_enabled = enabled;
        cx.notify();

        let db = cx.global::<AppServices>().store();
        self.commit_board_mutation(cx, "Could not save reminder", false, async move {
            storage::board_commands::set_board_card_reminder(&db, entry_id, enabled).await?;
            crate::system_notifications::wake();
            Ok::<(), anyhow::Error>(())
        });
    }
}
