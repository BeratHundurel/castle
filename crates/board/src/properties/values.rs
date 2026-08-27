use super::*;

impl BoardView {
    pub(crate) fn set_entry_property_value(
        &mut self,
        entry_id: i64,
        property_id: i64,
        value: Option<PropertyValue>,
        cx: &mut Context<Self>,
    ) {
        let key = (entry_id, property_id);
        let previous = self.properties.values.get(&key).cloned();
        self.apply_local_property_value(entry_id, property_id, value.clone());
        self.properties.field_errors.remove(&key);
        self.properties.saving_values.insert(key);
        self.properties.next_update_revision =
            self.properties.next_update_revision.saturating_add(1);
        let revision = self.properties.next_update_revision;
        self.properties.update_revisions.insert(key, revision);
        let persisted_revisions = self.properties.persisted_revisions.clone();
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                let mut persisted_revisions = persisted_revisions.lock().await;
                if persisted_revisions
                    .get(&key)
                    .is_some_and(|persisted_revision| *persisted_revision >= revision)
                {
                    return Ok::<(), anyhow::Error>(());
                }
                match value {
                    Some(value) => {
                        storage::board::properties::set_entry_property(
                            &store,
                            entry_id,
                            property_id,
                            value,
                        )
                        .await?;
                    }
                    None => {
                        storage::board::properties::clear_entry_property(
                            &store,
                            entry_id,
                            property_id,
                        )
                        .await?;
                    }
                }
                persisted_revisions.insert(key, revision);
                Ok(())
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;

            this.update(cx, |this, cx| {
                if this.properties.update_revisions.get(&key) != Some(&revision) {
                    return;
                }
                this.properties.saving_values.remove(&key);
                match result {
                    Ok(Ok(())) => {
                        this.properties.field_errors.remove(&key);
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.apply_local_property_value(entry_id, property_id, previous);
                        this.properties.field_errors.insert(
                            key,
                            format!("Save failed: {error}. Change the value to retry.").into(),
                        );
                    }
                    Err(error) => {
                        this.apply_local_property_value(entry_id, property_id, previous);
                        this.properties
                            .field_errors
                            .insert(key, format!("Property task failed: {error}").into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn apply_local_property_value(
        &mut self,
        entry_id: i64,
        property_id: i64,
        value: Option<PropertyValue>,
    ) {
        let key = (entry_id, property_id);
        self.properties.data.values.retain(|existing| {
            existing.entry_id != entry_id || existing.property_id != property_id
        });
        match value {
            Some(value) => {
                self.properties.values.insert(key, value.clone());
                self.properties
                    .data
                    .values
                    .push(storage::board::properties::EntryProperty {
                        entry_id,
                        property_id,
                        value,
                    });
            }
            None => {
                self.properties.values.remove(&key);
            }
        }
    }
}
