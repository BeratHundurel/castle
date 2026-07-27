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

        entry.title = SharedString::from(trimmed_title);
        entry.description = description.clone();
        self.entry_dialog.editing = false;
        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let title = trimmed_title.to_string();
        let description = description.to_string();

        let _task = tokio::runtime::Handle::current().spawn(async move {
            let model = entry::ActiveModel {
                id: Set(entry_id as i64),
                title: Set(title),
                description: Set(description),
                ..Default::default()
            };

            model.update(&*db).await?;
            Ok::<(), sea_orm::DbErr>(())
        });
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
        let _task = tokio::runtime::Handle::current().spawn(async move {
            let mut persisted_revisions = persisted_revisions.lock().await;
            if persisted_revisions
                .get(&entry_id)
                .is_some_and(|persisted_revision| *persisted_revision >= revision)
            {
                return Ok::<(), sea_orm::DbErr>(());
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
            Ok::<(), sea_orm::DbErr>(())
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
        tokio::runtime::Handle::current().spawn(async move {
            entry::ActiveModel {
                id: Set(entry_id as i64),
                reminder_enabled: Set(enabled),
                reminder_notified_for: Set(None),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            crate::system_notifications::wake();
            Ok::<(), sea_orm::DbErr>(())
        });
    }
}
