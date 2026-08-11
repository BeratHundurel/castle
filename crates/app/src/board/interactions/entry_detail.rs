use super::*;

impl BoardView {
    pub(in crate::board) fn update_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_dialog.entry_id else {
            return;
        };

        let title = self.entry_title_input.read(cx).value();
        let description = self.entry_description_input.read(cx).value();
        let trimmed_title = title.trim();

        if trimmed_title.is_empty() {
            return;
        }

        let Some(entry) = self
            .cards
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
        self.entry_dialog.editing = false;
        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let title = trimmed_title.to_string();
        let description = description.to_string();
        let runtime = cx.global::<DB>().runtime.clone();

        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    let txn = db.begin().await?;
                    let model = entry::ActiveModel {
                        id: Set(entry_id as i64),
                        title: Set(title),
                        description: Set(description.clone()),
                        ..Default::default()
                    };

                    model.update(&txn).await?;
                    storage::workspace_links::index_entry_workspace_links_in_connection(
                        &txn,
                        entry_id as i64,
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
                    if let Some(board_id) = this.board_id {
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
                    this.mutation_error = Some(format!("Could not save card: {error}").into());
                    if let Some(board_id) = this.board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                Err(error) => {
                    this.mutation_error = Some(format!("Card save task failed: {error}").into());
                    if let Some(board_id) = this.board_id {
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
        let Some(entry_id) = self.entry_dialog.entry_id else {
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

        entry.due_on = due_on.as_deref().map(SharedString::from);
        cx.notify();

        self.next_due_date_update_revision = self.next_due_date_update_revision.saturating_add(1);
        let revision = self.next_due_date_update_revision;
        let persisted_revisions = self.persisted_due_date_revisions.clone();
        let db = cx.global::<DB>().conn.clone();
        self.commit_board_mutation(cx, "Could not save due date", false, async move {
            let mut persisted_revisions = persisted_revisions.lock().await;
            if persisted_revisions
                .get(&entry_id)
                .is_some_and(|persisted_revision| *persisted_revision >= revision)
            {
                return Ok::<(), anyhow::Error>(());
            }
            entry::ActiveModel {
                id: Set(entry_id as i64),
                due_on: Set(due_on),
                reminder_notified_for: Set(None),
                ..Default::default()
            }
            .update(&*db)
            .await?;
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
        let Some(entry_id) = self.entry_dialog.entry_id else {
            return;
        };
        let Some(entry) = self
            .cards
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

        let db = cx.global::<DB>().conn.clone();
        self.commit_board_mutation(cx, "Could not save reminder", false, async move {
            entry::ActiveModel {
                id: Set(entry_id as i64),
                reminder_enabled: Set(enabled),
                reminder_notified_for: Set(None),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            crate::system_notifications::wake();
            Ok::<(), anyhow::Error>(())
        });
    }
}
