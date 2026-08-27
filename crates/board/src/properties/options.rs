use super::*;

impl BoardView {
    pub(crate) fn create_board_property_option(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(property_id) = self.properties.adding_property_option_id else {
            return;
        };
        let color = self
            .properties
            .data
            .definitions
            .iter()
            .find(|property| property.id == property_id)
            .map(|property| OPTION_COLORS[property.options.len() % OPTION_COLORS.len()].to_string())
            .unwrap_or_else(|| "blue".to_string());
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::create_property_option(&store, property_id, name, color)
                    .await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(option)) => {
                        if let Some(property) = this
                            .properties
                            .data
                            .definitions
                            .iter_mut()
                            .find(|property| property.id == property_id)
                        {
                            property.options.push(option);
                        }
                        this.properties.adding_property_option_id = None;
                        this.properties.update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.properties.update_error =
                            Some(format!("Could not create option: {error}").into());
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn start_property_option_rename(
        &mut self,
        option_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(option) = self
            .properties
            .data
            .definitions
            .iter()
            .flat_map(|property| property.options.iter())
            .find(|option| option.id == option_id)
        else {
            return;
        };
        self.properties.renaming_property_option_id = Some(option_id);
        self.properties
            .rename_property_option_input
            .update(cx, |input, cx| {
                input.set_value(option.name.clone(), window, cx);
                input.focus(window, cx);
            });
        cx.notify();
    }

    pub(crate) fn commit_property_option_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(option_id) = self.properties.renaming_property_option_id else {
            return;
        };
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::rename_property_option(&store, option_id, name).await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(option)) => {
                        if let Some(current) = this
                            .properties
                            .data
                            .definitions
                            .iter_mut()
                            .flat_map(|property| property.options.iter_mut())
                            .find(|current| current.id == option_id)
                        {
                            *current = option;
                        }
                        this.properties.renaming_property_option_id = None;
                        this.properties.update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.properties.update_error =
                            Some(format!("Could not rename option: {error}").into())
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn cycle_property_option_color(&mut self, option_id: i64, cx: &mut Context<Self>) {
        let current = self
            .properties
            .data
            .definitions
            .iter()
            .flat_map(|property| property.options.iter())
            .find(|option| option.id == option_id)
            .map(|option| option.color.as_str())
            .unwrap_or("blue");
        let index = OPTION_COLORS
            .iter()
            .position(|color| *color == current)
            .unwrap_or(0);
        let color = OPTION_COLORS[(index + 1) % OPTION_COLORS.len()].to_string();
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::update_property_option_color(&store, option_id, color)
                    .await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                if let Ok(Ok(option)) = result
                    && let Some(current) = this
                        .properties
                        .data
                        .definitions
                        .iter_mut()
                        .flat_map(|property| property.options.iter_mut())
                        .find(|current| current.id == option_id)
                {
                    *current = option;
                    this.emit_data_committed(cx, false);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn move_property_option(
        &mut self,
        property_id: i64,
        option_id: i64,
        offset: isize,
        cx: &mut Context<Self>,
    ) {
        let Some(property) = self
            .properties
            .data
            .definitions
            .iter_mut()
            .find(|property| property.id == property_id)
        else {
            return;
        };
        let Some(index) = property
            .options
            .iter()
            .position(|option| option.id == option_id)
        else {
            return;
        };
        let target = index.saturating_add_signed(offset);
        if target >= property.options.len() || target == index {
            return;
        }
        property.options.swap(index, target);
        for (position, option) in property.options.iter_mut().enumerate() {
            option.position = position as i32;
        }
        let ordered_ids = property
            .options
            .iter()
            .map(|option| option.id)
            .collect::<Vec<_>>();
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::reorder_property_options(
                    &store,
                    property_id,
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
                            Some(format!("Could not reorder options: {error}").into());
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

    pub(crate) fn confirm_delete_property_option(
        &mut self,
        option_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(option) = self
            .properties
            .data
            .definitions
            .iter()
            .flat_map(|property| property.options.iter())
            .find(|option| option.id == option_id)
        else {
            return;
        };
        let name = option.name.clone();
        let value_count = self
            .properties
            .values
            .values()
            .filter(|value| matches!(value, PropertyValue::Select(id) if *id == option_id))
            .count();
        let view_count = self
            .properties
            .saved_views
            .iter()
            .filter(|view| {
                view.config.filters.iter().any(|filter| {
                    matches!(
                        &filter.operand,
                        Some(FilterOperand::OptionIds(ids)) if ids.contains(&option_id)
                    )
                })
            })
            .count();
        let view = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, cx| {
            alert
                .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
                .title(format!("Delete option ‘{name}’"))
                .description(format!(
                    "This clears {value_count} card value(s) and updates {view_count} saved view(s)."
                ))
                .button_props(
                    DialogButtonProps::default()
                        .ok_variant(ButtonVariant::Danger)
                        .ok_text("Delete option")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .on_ok({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| this.delete_property_option(option_id, cx));
                        true
                    }
                })
        });
    }

    fn delete_property_option(&mut self, option_id: i64, cx: &mut Context<Self>) {
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::delete_property_option(&store, option_id).await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => {
                        for property in &mut this.properties.data.definitions {
                            property.options.retain(|option| option.id != option_id);
                        }
                        this.properties.values.retain(|_, value| {
                            !matches!(value, PropertyValue::Select(id) if *id == option_id)
                        });
                        this.properties.data.values.retain(|value| {
                            !matches!(value.value, PropertyValue::Select(id) if id == option_id)
                        });
                        remove_option_from_config(
                            &mut this.properties.active_view_config,
                            option_id,
                        );
                        this.filters = crate::filters::BoardFilters::from_config(
                            &this.properties.active_view_config,
                        );
                        for view in &mut this.properties.saved_views {
                            remove_option_from_config(&mut view.config, option_id);
                        }
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.properties.update_error =
                            Some(format!("Could not delete option: {error}").into())
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
