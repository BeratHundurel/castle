use super::*;

impl BoardView {
    pub(crate) fn create_board_property(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(board_id) = self.data.board_id else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            self.properties.update_error = Some("Enter a property name".into());
            cx.notify();
            return;
        }
        let kind = self.properties.new_property_kind;
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::create_property(&store, i64::from(board_id), name, kind)
                    .await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(property)) => {
                        this.properties.data.definitions.push(property);
                        this.properties.property_form_open = false;
                        this.properties.update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.properties.update_error =
                            Some(format!("Could not create property: {error}").into());
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn start_property_rename(
        &mut self,
        property_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(property) = self
            .properties
            .data
            .definitions
            .iter()
            .find(|property| property.id == property_id)
        else {
            return;
        };
        self.properties.renaming_property_id = Some(property_id);
        self.properties
            .rename_property_input
            .update(cx, |input, cx| {
                input.set_value(property.name.clone(), window, cx);
                input.focus(window, cx);
            });
        cx.notify();
    }

    pub(crate) fn commit_property_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(property_id) = self.properties.renaming_property_id else {
            return;
        };
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::rename_property(&store, property_id, name).await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(property)) => {
                        if let Some(current) = this
                            .properties
                            .data
                            .definitions
                            .iter_mut()
                            .find(|current| current.id == property_id)
                        {
                            *current = property;
                        }
                        this.properties.renaming_property_id = None;
                        this.properties.update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.properties.update_error =
                            Some(format!("Could not rename: {error}").into())
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn move_property(
        &mut self,
        property_id: i64,
        offset: isize,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .properties
            .data
            .definitions
            .iter()
            .position(|property| property.id == property_id)
        else {
            return;
        };
        let target = index.saturating_add_signed(offset);
        if target >= self.properties.data.definitions.len() || target == index {
            return;
        }
        self.properties.data.definitions.swap(index, target);
        for (position, property) in self.properties.data.definitions.iter_mut().enumerate() {
            property.position = position as i32;
        }
        let ordered_ids = self
            .properties
            .data
            .definitions
            .iter()
            .map(|property| property.id)
            .collect::<Vec<_>>();
        let Some(board_id) = self.data.board_id else {
            return;
        };
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::reorder_properties(
                    &store,
                    i64::from(board_id),
                    &ordered_ids,
                )
                .await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => this.emit_data_committed(cx, false),
                    Ok(Err(error)) => {
                        this.properties.update_error =
                            Some(format!("Could not reorder properties: {error}").into());
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn confirm_delete_property(
        &mut self,
        property_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(property) = self
            .properties
            .data
            .definitions
            .iter()
            .find(|property| property.id == property_id)
        else {
            return;
        };
        let name = property.name.clone();
        let value_count = self
            .properties
            .values
            .keys()
            .filter(|(_, candidate)| *candidate == property_id)
            .count();
        let view_count = self
            .properties
            .saved_views
            .iter()
            .filter(|view| config_references_property(&view.config, property_id))
            .count();
        let view = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, cx| {
            alert
                .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
                .title(format!("Delete ‘{name}’"))
                .description(format!(
                    "This removes {value_count} card value(s) and updates {view_count} saved view(s)."
                ))
                .button_props(
                    DialogButtonProps::default()
                        .ok_variant(ButtonVariant::Danger)
                        .ok_text("Delete property")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .on_ok({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| this.delete_property(property_id, cx));
                        true
                    }
                })
        });
    }

    fn delete_property(&mut self, property_id: i64, cx: &mut Context<Self>) {
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::delete_property(&store, property_id).await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => {
                        this.properties
                            .data
                            .definitions
                            .retain(|property| property.id != property_id);
                        this.properties
                            .data
                            .values
                            .retain(|value| value.property_id != property_id);
                        this.properties
                            .values
                            .retain(|(_, candidate), _| *candidate != property_id);
                        remove_property_from_config(
                            &mut this.properties.active_view_config,
                            property_id,
                        );
                        this.filters = crate::filters::BoardFilters::from_config(
                            &this.properties.active_view_config,
                        );
                        for view in &mut this.properties.saved_views {
                            remove_property_from_config(&mut view.config, property_id);
                        }
                        this.properties.update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.properties.update_error =
                            Some(format!("Could not delete property: {error}").into())
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
