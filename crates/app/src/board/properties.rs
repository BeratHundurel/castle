use chrono::NaiveDate;
use gpui::{Context, Styled, Window};
use gpui_component::{
    ActiveTheme, Icon, IconName, WindowExt, button::ButtonVariant, calendar::Date,
    dialog::DialogButtonProps,
};
use storage::board_properties::{
    BoardViewConfig, FilterOperand, FilterOperator, PropertyKey, PropertyKind, PropertyValue,
    SortDirection, ViewFilter, ViewSort,
};

use crate::DB;

use super::BoardView;

const OPTION_COLORS: [&str; 6] = ["blue", "green", "amber", "red", "purple", "slate"];

impl BoardView {
    pub(super) fn start_editing_property_value(
        &mut self,
        entry_id: i64,
        property_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.property_values.get(&(entry_id, property_id));
        self.editing_property_id = Some(property_id);
        self.property_field_errors.remove(&(entry_id, property_id));
        if matches!(value, Some(PropertyValue::Date(_)))
            || self
                .board_properties
                .definitions
                .iter()
                .any(|property| property.id == property_id && property.kind == PropertyKind::Date)
        {
            let date = value.and_then(|value| match value {
                PropertyValue::Date(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d").ok(),
                _ => None,
            });
            self.property_date_picker.update(cx, |picker, cx| {
                picker.set_date(Date::Single(date), window, cx);
            });
        } else {
            let text = value.map(property_value_text).unwrap_or_default();
            self.property_value_input.update(cx, |input, cx| {
                input.set_value(text, window, cx);
                input.focus(window, cx);
            });
        }
        cx.notify();
    }

    pub(super) fn set_property_select_open(
        &mut self,
        property_id: i64,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selecting_property_id = open.then_some(property_id);
        self.property_select_search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            if open {
                input.focus(window, cx);
            }
        });
        cx.notify();
    }

    pub(super) fn commit_property_value(&mut self, value: String, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_dialog.entry_id.map(i64::from) else {
            return;
        };
        let Some(property_id) = self.editing_property_id else {
            return;
        };
        let Some(kind) = self
            .board_properties
            .definitions
            .iter()
            .find(|property| property.id == property_id)
            .map(|property| property.kind)
        else {
            return;
        };
        let value = value.trim();
        let parsed = if value.is_empty() {
            None
        } else {
            match kind {
                PropertyKind::Text => Some(PropertyValue::Text(value.to_string())),
                PropertyKind::Url => {
                    if !is_supported_url(value) {
                        self.set_property_field_error(
                            entry_id,
                            property_id,
                            "Use an http:// or https:// URL",
                            cx,
                        );
                        return;
                    }
                    Some(PropertyValue::Url(value.to_string()))
                }
                PropertyKind::Date => {
                    if NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err() {
                        self.set_property_field_error(
                            entry_id,
                            property_id,
                            "Use YYYY-MM-DD for this date",
                            cx,
                        );
                        return;
                    }
                    Some(PropertyValue::Date(value.to_string()))
                }
                PropertyKind::Number => match value.parse::<f64>() {
                    Ok(number) if number.is_finite() => Some(PropertyValue::Number(number)),
                    _ => {
                        self.set_property_field_error(
                            entry_id,
                            property_id,
                            "Enter a finite number",
                            cx,
                        );
                        return;
                    }
                },
                PropertyKind::Checkbox | PropertyKind::Select => return,
            }
        };
        self.editing_property_id = None;
        self.set_entry_property_value(entry_id, property_id, parsed, cx);
    }

    fn set_property_field_error(
        &mut self,
        entry_id: i64,
        property_id: i64,
        message: &str,
        cx: &mut Context<Self>,
    ) {
        self.property_field_errors
            .insert((entry_id, property_id), message.into());
        cx.notify();
    }

    pub(super) fn set_property_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.property_panel_open = open;
        if !open {
            self.property_form_open = false;
            self.adding_property_option_id = None;
            self.renaming_property_id = None;
            self.renaming_property_option_id = None;
        }
        cx.notify();
    }

    pub(super) fn set_fields_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.fields_panel_open = open;
        cx.notify();
    }

    pub(super) fn set_view_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.view_panel_open = open;
        if !open {
            self.new_view_form_open = false;
            self.renaming_view_id = None;
        }
        cx.notify();
    }

    pub(super) fn start_new_view_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_view_form_open = true;
        self.property_update_error = None;
        self.new_view_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn cancel_new_view_form(&mut self, cx: &mut Context<Self>) {
        self.new_view_form_open = false;
        self.property_update_error = None;
        cx.notify();
    }

    pub(super) fn set_sort_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.sort_panel_open = open;
        cx.notify();
    }

    pub(super) fn start_property_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.property_form_open = true;
        self.property_update_error = None;
        self.new_property_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn cancel_property_form(&mut self, cx: &mut Context<Self>) {
        self.property_form_open = false;
        self.property_update_error = None;
        cx.notify();
    }

    pub(super) fn select_new_property_kind(&mut self, kind: PropertyKind, cx: &mut Context<Self>) {
        self.new_property_kind = kind;
        cx.notify();
    }

    pub(super) fn start_adding_property_option(
        &mut self,
        property_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.adding_property_option_id = Some(property_id);
        self.new_property_option_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn create_board_property(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(board_id) = self.board_id else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            self.property_update_error = Some("Enter a property name".into());
            cx.notify();
            return;
        }
        let kind = self.new_property_kind;
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::create_property(
                        db.as_ref(),
                        i64::from(board_id),
                        name,
                        kind,
                    )
                    .await
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(property)) => {
                        this.board_properties.definitions.push(property);
                        this.property_form_open = false;
                        this.property_update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.property_update_error =
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

    pub(super) fn start_property_rename(
        &mut self,
        property_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(property) = self
            .board_properties
            .definitions
            .iter()
            .find(|property| property.id == property_id)
        else {
            return;
        };
        self.renaming_property_id = Some(property_id);
        self.rename_property_input.update(cx, |input, cx| {
            input.set_value(property.name.clone(), window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn commit_property_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(property_id) = self.renaming_property_id else {
            return;
        };
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::rename_property(db.as_ref(), property_id, name).await
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(property)) => {
                        if let Some(current) = this
                            .board_properties
                            .definitions
                            .iter_mut()
                            .find(|current| current.id == property_id)
                        {
                            *current = property;
                        }
                        this.renaming_property_id = None;
                        this.property_update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.property_update_error =
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

    pub(super) fn move_property(
        &mut self,
        property_id: i64,
        offset: isize,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .board_properties
            .definitions
            .iter()
            .position(|property| property.id == property_id)
        else {
            return;
        };
        let target = index.saturating_add_signed(offset);
        if target >= self.board_properties.definitions.len() || target == index {
            return;
        }
        self.board_properties.definitions.swap(index, target);
        for (position, property) in self.board_properties.definitions.iter_mut().enumerate() {
            property.position = position as i32;
        }
        let ordered_ids = self
            .board_properties
            .definitions
            .iter()
            .map(|property| property.id)
            .collect::<Vec<_>>();
        let Some(board_id) = self.board_id else {
            return;
        };
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::reorder_properties(
                        db.as_ref(),
                        i64::from(board_id),
                        &ordered_ids,
                    )
                    .await
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => this.emit_data_committed(cx, false),
                    Ok(Err(error)) => {
                        this.property_update_error =
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

    pub(super) fn confirm_delete_property(
        &mut self,
        property_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(property) = self
            .board_properties
            .definitions
            .iter()
            .find(|property| property.id == property_id)
        else {
            return;
        };
        let name = property.name.clone();
        let value_count = self
            .property_values
            .keys()
            .filter(|(_, candidate)| *candidate == property_id)
            .count();
        let view_count = self
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
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::delete_property(db.as_ref(), property_id).await
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => {
                        this.board_properties
                            .definitions
                            .retain(|property| property.id != property_id);
                        this.board_properties
                            .values
                            .retain(|value| value.property_id != property_id);
                        this.property_values
                            .retain(|(_, candidate), _| *candidate != property_id);
                        remove_property_from_config(&mut this.active_view_config, property_id);
                        this.filters =
                            super::filters::BoardFilters::from_config(&this.active_view_config);
                        for view in &mut this.saved_views {
                            remove_property_from_config(&mut view.config, property_id);
                        }
                        this.property_update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.property_update_error =
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

    pub(super) fn create_board_property_option(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(property_id) = self.adding_property_option_id else {
            return;
        };
        let color = self
            .board_properties
            .definitions
            .iter()
            .find(|property| property.id == property_id)
            .map(|property| OPTION_COLORS[property.options.len() % OPTION_COLORS.len()].to_string())
            .unwrap_or_else(|| "blue".to_string());
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::create_property_option(
                        db.as_ref(),
                        property_id,
                        name,
                        color,
                    )
                    .await
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(option)) => {
                        if let Some(property) = this
                            .board_properties
                            .definitions
                            .iter_mut()
                            .find(|property| property.id == property_id)
                        {
                            property.options.push(option);
                        }
                        this.adding_property_option_id = None;
                        this.property_update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.property_update_error =
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

    pub(super) fn start_property_option_rename(
        &mut self,
        option_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(option) = self
            .board_properties
            .definitions
            .iter()
            .flat_map(|property| property.options.iter())
            .find(|option| option.id == option_id)
        else {
            return;
        };
        self.renaming_property_option_id = Some(option_id);
        self.rename_property_option_input.update(cx, |input, cx| {
            input.set_value(option.name.clone(), window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn commit_property_option_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(option_id) = self.renaming_property_option_id else {
            return;
        };
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::rename_property_option(db.as_ref(), option_id, name)
                        .await
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(option)) => {
                        if let Some(current) = this
                            .board_properties
                            .definitions
                            .iter_mut()
                            .flat_map(|property| property.options.iter_mut())
                            .find(|current| current.id == option_id)
                        {
                            *current = option;
                        }
                        this.renaming_property_option_id = None;
                        this.property_update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.property_update_error =
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

    pub(super) fn cycle_property_option_color(&mut self, option_id: i64, cx: &mut Context<Self>) {
        let current = self
            .board_properties
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
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::update_property_option_color(
                        db.as_ref(),
                        option_id,
                        color,
                    )
                    .await
                })
                .await;
            this.update(cx, |this, cx| {
                if let Ok(Ok(option)) = result
                    && let Some(current) = this
                        .board_properties
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

    pub(super) fn move_property_option(
        &mut self,
        property_id: i64,
        option_id: i64,
        offset: isize,
        cx: &mut Context<Self>,
    ) {
        let Some(property) = self
            .board_properties
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
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::reorder_property_options(
                        db.as_ref(),
                        property_id,
                        &ordered_ids,
                    )
                    .await
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => this.emit_data_committed(cx, false),
                    Ok(Err(error)) => {
                        this.property_update_error =
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

    pub(super) fn confirm_delete_property_option(
        &mut self,
        option_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(option) = self
            .board_properties
            .definitions
            .iter()
            .flat_map(|property| property.options.iter())
            .find(|option| option.id == option_id)
        else {
            return;
        };
        let name = option.name.clone();
        let value_count = self
            .property_values
            .values()
            .filter(|value| matches!(value, PropertyValue::Select(id) if *id == option_id))
            .count();
        let view_count = self
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
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::delete_property_option(db.as_ref(), option_id).await
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => {
                        for property in &mut this.board_properties.definitions {
                            property.options.retain(|option| option.id != option_id);
                        }
                        this.property_values.retain(|_, value| {
                            !matches!(value, PropertyValue::Select(id) if *id == option_id)
                        });
                        this.board_properties.values.retain(|value| {
                            !matches!(value.value, PropertyValue::Select(id) if id == option_id)
                        });
                        remove_option_from_config(&mut this.active_view_config, option_id);
                        this.filters =
                            super::filters::BoardFilters::from_config(&this.active_view_config);
                        for view in &mut this.saved_views {
                            remove_option_from_config(&mut view.config, option_id);
                        }
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.property_update_error =
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

    pub(super) fn set_entry_property_value(
        &mut self,
        entry_id: i64,
        property_id: i64,
        value: Option<PropertyValue>,
        cx: &mut Context<Self>,
    ) {
        let key = (entry_id, property_id);
        let previous = self.property_values.get(&key).cloned();
        self.apply_local_property_value(entry_id, property_id, value.clone());
        self.property_field_errors.remove(&key);
        self.saving_property_values.insert(key);
        self.next_property_update_revision = self.next_property_update_revision.saturating_add(1);
        let revision = self.next_property_update_revision;
        self.property_update_revisions.insert(key, revision);
        let persisted_revisions = self.persisted_property_revisions.clone();
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();

        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    let mut persisted_revisions = persisted_revisions.lock().await;
                    if persisted_revisions
                        .get(&key)
                        .is_some_and(|persisted_revision| *persisted_revision >= revision)
                    {
                        return Ok::<(), anyhow::Error>(());
                    }
                    match value {
                        Some(value) => {
                            storage::board_properties::set_entry_property(
                                db.as_ref(),
                                entry_id,
                                property_id,
                                value,
                            )
                            .await?;
                        }
                        None => {
                            storage::board_properties::clear_entry_property(
                                db.as_ref(),
                                entry_id,
                                property_id,
                            )
                            .await?;
                        }
                    }
                    persisted_revisions.insert(key, revision);
                    Ok(())
                })
                .await;

            this.update(cx, |this, cx| {
                if this.property_update_revisions.get(&key) != Some(&revision) {
                    return;
                }
                this.saving_property_values.remove(&key);
                match result {
                    Ok(Ok(())) => {
                        this.property_field_errors.remove(&key);
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.apply_local_property_value(entry_id, property_id, previous);
                        this.property_field_errors.insert(
                            key,
                            format!("Save failed: {error}. Change the value to retry.").into(),
                        );
                    }
                    Err(error) => {
                        this.apply_local_property_value(entry_id, property_id, previous);
                        this.property_field_errors
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
        self.board_properties.values.retain(|existing| {
            existing.entry_id != entry_id || existing.property_id != property_id
        });
        match value {
            Some(value) => {
                self.property_values.insert(key, value.clone());
                self.board_properties
                    .values
                    .push(storage::board_properties::EntryProperty {
                        entry_id,
                        property_id,
                        value,
                    });
            }
            None => {
                self.property_values.remove(&key);
            }
        }
    }

    pub(super) fn toggle_visible_property(
        &mut self,
        property: PropertyKey,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self
            .active_view_config
            .visible_properties
            .iter()
            .position(|candidate| candidate == &property)
        {
            self.active_view_config.visible_properties.remove(index);
        } else if self.active_view_config.visible_properties.len() < 3 {
            self.active_view_config.visible_properties.push(property);
        } else {
            self.property_update_error = Some("A view can show up to three fields".into());
            cx.notify();
            return;
        }
        self.view_config_dirty = true;
        self.property_update_error = None;
        cx.notify();
    }

    pub(super) fn move_visible_property(
        &mut self,
        property: &PropertyKey,
        offset: isize,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .active_view_config
            .visible_properties
            .iter()
            .position(|candidate| candidate == property)
        else {
            return;
        };
        let target = index.saturating_add_signed(offset);
        if target < self.active_view_config.visible_properties.len() && target != index {
            self.active_view_config
                .visible_properties
                .swap(index, target);
            self.view_config_dirty = true;
            cx.notify();
        }
    }

    pub(super) fn toggle_compact_cards(&mut self, cx: &mut Context<Self>) {
        self.active_view_config.compact_cards = !self.active_view_config.compact_cards;
        self.view_config_dirty = true;
        cx.notify();
    }

    pub(super) fn set_sort(&mut self, property: PropertyKey, cx: &mut Context<Self>) {
        self.active_view_config.sort = match self.active_view_config.sort.as_ref() {
            Some(sort) if sort.property == property => Some(ViewSort {
                property,
                direction: match sort.direction {
                    SortDirection::Ascending => SortDirection::Descending,
                    SortDirection::Descending => SortDirection::Ascending,
                },
            }),
            _ => Some(ViewSort {
                property,
                direction: SortDirection::Ascending,
            }),
        };
        self.view_config_dirty = true;
        cx.notify();
    }

    pub(super) fn clear_sort(&mut self, cx: &mut Context<Self>) {
        self.active_view_config.sort = None;
        self.view_config_dirty = true;
        cx.notify();
    }

    pub(super) fn select_saved_view(&mut self, view_id: Option<i64>, cx: &mut Context<Self>) {
        let config = view_id
            .and_then(|view_id| self.saved_views.iter().find(|view| view.id == view_id))
            .map(|view| view.config.clone())
            .unwrap_or_else(super::filters::default_view_config);
        self.active_view_id = view_id;
        self.active_view_config = config.clone();
        self.filters = super::filters::BoardFilters::from_config(&config);
        self.view_config_dirty = false;
        self.view_panel_open = false;
        let Some(board_id) = self.board_id else {
            cx.notify();
            return;
        };
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::set_selected_board_view(
                        db.as_ref(),
                        i64::from(board_id),
                        view_id,
                    )
                    .await
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => this.property_update_error = None,
                    Ok(Err(error)) => {
                        this.property_update_error =
                            Some(format!("Could not remember selected view: {error}").into());
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

    pub(super) fn start_view_rename(
        &mut self,
        view_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.saved_views.iter().find(|view| view.id == view_id) else {
            return;
        };
        self.renaming_view_id = Some(view_id);
        self.rename_view_input.update(cx, |input, cx| {
            input.set_value(view.name.clone(), window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn commit_view_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(view_id) = self.renaming_view_id else {
            return;
        };
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::rename_board_view(db.as_ref(), view_id, name).await
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(view)) => {
                        if let Some(current) = this
                            .saved_views
                            .iter_mut()
                            .find(|current| current.id == view_id)
                        {
                            *current = view;
                        }
                        this.renaming_view_id = None;
                        this.property_update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.property_update_error =
                            Some(format!("Could not rename view: {error}").into());
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn create_saved_view(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(board_id) = self.board_id else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            self.property_update_error = Some("Enter a view name".into());
            cx.notify();
            return;
        }
        self.filters.sync_config(&mut self.active_view_config);
        let config = self.active_view_config.clone();
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    let view = storage::board_properties::create_board_view(
                        db.as_ref(),
                        i64::from(board_id),
                        name,
                        config,
                    )
                    .await?;
                    storage::board_properties::set_selected_board_view(
                        db.as_ref(),
                        i64::from(board_id),
                        Some(view.id),
                    )
                    .await?;
                    Ok::<_, anyhow::Error>(view)
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(view)) => {
                        this.active_view_id = Some(view.id);
                        this.active_view_config = view.config.clone();
                        this.saved_views.push(view);
                        this.view_config_dirty = false;
                        this.new_view_form_open = false;
                        this.property_update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.property_update_error =
                            Some(format!("Could not save view: {error}").into())
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn update_active_view(&mut self, cx: &mut Context<Self>) {
        let Some(view_id) = self.active_view_id else {
            return;
        };
        self.filters.sync_config(&mut self.active_view_config);
        let config = self.active_view_config.clone();
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::update_board_view(db.as_ref(), view_id, config).await
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(view)) => {
                        if let Some(current) = this
                            .saved_views
                            .iter_mut()
                            .find(|current| current.id == view_id)
                        {
                            *current = view.clone();
                        }
                        this.active_view_config = view.config;
                        this.view_config_dirty = false;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.property_update_error =
                            Some(format!("Could not update view: {error}").into())
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn set_default_view(&mut self, view_id: i64, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::set_default_board_view(db.as_ref(), view_id).await
                })
                .await;
            this.update(cx, |this, cx| {
                if let Ok(Ok(default_view)) = result {
                    for view in &mut this.saved_views {
                        view.is_default = view.id == default_view.id;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn delete_saved_view(&mut self, view_id: i64, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = cx.global::<DB>().runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board_properties::delete_board_view(db.as_ref(), view_id).await
                })
                .await;
            this.update(cx, |this, cx| {
                if matches!(result, Ok(Ok(()))) {
                    this.saved_views.retain(|view| view.id != view_id);
                    this.emit_data_committed(cx, false);
                    if this.active_view_id == Some(view_id) {
                        this.select_saved_view(None, cx);
                    } else {
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn start_custom_filter(
        &mut self,
        property_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing_filter_property_id = Some(property_id);
        let value = self
            .filters
            .custom
            .iter()
            .find(|filter| filter.property == PropertyKey::Custom(property_id))
            .and_then(|filter| filter.operand.as_ref())
            .map(filter_operand_text)
            .unwrap_or_default();
        self.filter_value_input.update(cx, |input, cx| {
            input.set_value(value, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn commit_custom_filter(&mut self, value: String, cx: &mut Context<Self>) {
        let Some(property_id) = self.editing_filter_property_id else {
            return;
        };
        let Some(kind) = self
            .board_properties
            .definitions
            .iter()
            .find(|property| property.id == property_id)
            .map(|property| property.kind)
        else {
            return;
        };
        let value = value.trim();
        if value.is_empty() {
            self.remove_custom_filter(property_id, cx);
            return;
        }
        let operand = match kind {
            PropertyKind::Text | PropertyKind::Url => FilterOperand::Text(value.to_string()),
            PropertyKind::Number => match value.parse::<f64>() {
                Ok(value) if value.is_finite() => FilterOperand::Number(value),
                _ => {
                    self.property_update_error = Some("Enter a finite filter number".into());
                    cx.notify();
                    return;
                }
            },
            PropertyKind::Date => {
                if NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err() {
                    self.property_update_error = Some("Use YYYY-MM-DD for the filter".into());
                    cx.notify();
                    return;
                }
                FilterOperand::Date(value.to_string())
            }
            PropertyKind::Checkbox | PropertyKind::Select => return,
        };
        let default_operator = match kind {
            PropertyKind::Text | PropertyKind::Url => FilterOperator::Contains,
            PropertyKind::Number => FilterOperator::Equals,
            PropertyKind::Date => FilterOperator::On,
            PropertyKind::Checkbox | PropertyKind::Select => FilterOperator::Equals,
        };
        if let Some(filter) = self
            .filters
            .custom
            .iter_mut()
            .find(|filter| filter.property == PropertyKey::Custom(property_id))
        {
            filter.operand = Some(operand);
        } else {
            self.filters.custom.push(ViewFilter {
                property: PropertyKey::Custom(property_id),
                operator: default_operator,
                operand: Some(operand),
            });
        }
        self.editing_filter_property_id = None;
        self.mark_filters_dirty(cx);
    }

    pub(super) fn cycle_custom_filter_operator(
        &mut self,
        property_id: i64,
        cx: &mut Context<Self>,
    ) {
        let Some(kind) = self
            .board_properties
            .definitions
            .iter()
            .find(|property| property.id == property_id)
            .map(|property| property.kind)
        else {
            return;
        };
        let operators: &[FilterOperator] = match kind {
            PropertyKind::Text | PropertyKind::Url => &[
                FilterOperator::Contains,
                FilterOperator::DoesNotContain,
                FilterOperator::Equals,
                FilterOperator::IsEmpty,
                FilterOperator::IsNotEmpty,
            ],
            PropertyKind::Number => &[
                FilterOperator::Equals,
                FilterOperator::GreaterThan,
                FilterOperator::LessThan,
                FilterOperator::IsEmpty,
                FilterOperator::IsNotEmpty,
            ],
            PropertyKind::Date => &[
                FilterOperator::Before,
                FilterOperator::On,
                FilterOperator::After,
                FilterOperator::IsEmpty,
                FilterOperator::IsNotEmpty,
            ],
            PropertyKind::Checkbox => &[
                FilterOperator::IsChecked,
                FilterOperator::IsUnchecked,
                FilterOperator::IsEmpty,
            ],
            PropertyKind::Select => &[
                FilterOperator::IsAnyOf,
                FilterOperator::IsNoneOf,
                FilterOperator::IsEmpty,
                FilterOperator::IsNotEmpty,
            ],
        };
        let filter = self
            .filters
            .custom
            .iter_mut()
            .find(|filter| filter.property == PropertyKey::Custom(property_id));
        if let Some(filter) = filter {
            let index = operators
                .iter()
                .position(|operator| *operator == filter.operator)
                .unwrap_or(0);
            filter.operator = operators[(index + 1) % operators.len()];
            if matches!(
                filter.operator,
                FilterOperator::IsEmpty
                    | FilterOperator::IsNotEmpty
                    | FilterOperator::IsChecked
                    | FilterOperator::IsUnchecked
            ) {
                filter.operand = None;
            } else if kind == PropertyKind::Select
                && matches!(
                    filter.operator,
                    FilterOperator::IsAnyOf | FilterOperator::IsNoneOf
                )
                && filter.operand.is_none()
            {
                filter.operand = Some(FilterOperand::OptionIds(Vec::new()));
            }
        } else {
            self.filters.custom.push(ViewFilter {
                property: PropertyKey::Custom(property_id),
                operator: operators[0],
                operand: None,
            });
        }
        self.mark_filters_dirty(cx);
    }

    pub(super) fn toggle_select_filter_option(
        &mut self,
        property_id: i64,
        option_id: i64,
        cx: &mut Context<Self>,
    ) {
        let index = self
            .filters
            .custom
            .iter()
            .position(|filter| filter.property == PropertyKey::Custom(property_id));
        if let Some(index) = index {
            let filter = &mut self.filters.custom[index];
            let ids = match &mut filter.operand {
                Some(FilterOperand::OptionIds(ids)) => ids,
                _ => {
                    filter.operand = Some(FilterOperand::OptionIds(Vec::new()));
                    let Some(FilterOperand::OptionIds(ids)) = filter.operand.as_mut() else {
                        return;
                    };
                    ids
                }
            };
            if let Some(index) = ids.iter().position(|id| *id == option_id) {
                ids.remove(index);
            } else {
                ids.push(option_id);
            }
            if ids.is_empty() {
                self.filters.custom.remove(index);
            }
        } else {
            self.filters.custom.push(ViewFilter {
                property: PropertyKey::Custom(property_id),
                operator: FilterOperator::IsAnyOf,
                operand: Some(FilterOperand::OptionIds(vec![option_id])),
            });
        }
        self.mark_filters_dirty(cx);
    }

    pub(super) fn set_checkbox_filter(
        &mut self,
        property_id: i64,
        checked: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        self.filters
            .custom
            .retain(|filter| filter.property != PropertyKey::Custom(property_id));
        if let Some(checked) = checked {
            self.filters.custom.push(ViewFilter {
                property: PropertyKey::Custom(property_id),
                operator: if checked {
                    FilterOperator::IsChecked
                } else {
                    FilterOperator::IsUnchecked
                },
                operand: None,
            });
        }
        self.mark_filters_dirty(cx);
    }

    pub(super) fn set_empty_checkbox_filter(&mut self, property_id: i64, cx: &mut Context<Self>) {
        self.filters
            .custom
            .retain(|filter| filter.property != PropertyKey::Custom(property_id));
        self.filters.custom.push(ViewFilter {
            property: PropertyKey::Custom(property_id),
            operator: FilterOperator::IsEmpty,
            operand: None,
        });
        self.mark_filters_dirty(cx);
    }

    pub(super) fn remove_custom_filter(&mut self, property_id: i64, cx: &mut Context<Self>) {
        self.filters
            .custom
            .retain(|filter| filter.property != PropertyKey::Custom(property_id));
        self.editing_filter_property_id = None;
        self.mark_filters_dirty(cx);
    }

    fn mark_filters_dirty(&mut self, cx: &mut Context<Self>) {
        self.filters.sync_config(&mut self.active_view_config);
        self.view_config_dirty = true;
        self.property_update_error = None;
        cx.notify();
    }

    fn set_property_task_error(&mut self, error: tokio::task::JoinError, _cx: &mut Context<Self>) {
        self.property_update_error = Some(format!("Property task failed: {error}").into());
    }
}

fn property_value_text(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Text(value) | PropertyValue::Date(value) | PropertyValue::Url(value) => {
            value.clone()
        }
        PropertyValue::Number(value) => value.to_string(),
        PropertyValue::Checkbox(_) | PropertyValue::Select(_) => String::new(),
    }
}

fn filter_operand_text(value: &FilterOperand) -> String {
    match value {
        FilterOperand::Text(value) | FilterOperand::Date(value) => value.clone(),
        FilterOperand::Number(value) => value.to_string(),
        FilterOperand::OptionIds(_)
        | FilterOperand::LabelIds(_)
        | FilterOperand::DueDatePresets(_) => String::new(),
    }
}

fn is_supported_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn config_references_property(config: &BoardViewConfig, property_id: i64) -> bool {
    let key = PropertyKey::Custom(property_id);
    config.visible_properties.contains(&key)
        || config.filters.iter().any(|filter| filter.property == key)
        || config
            .sort
            .as_ref()
            .is_some_and(|sort| sort.property == key)
}

fn remove_property_from_config(config: &mut BoardViewConfig, property_id: i64) {
    let key = PropertyKey::Custom(property_id);
    config
        .visible_properties
        .retain(|property| property != &key);
    config.filters.retain(|filter| filter.property != key);
    if config
        .sort
        .as_ref()
        .is_some_and(|sort| sort.property == key)
    {
        config.sort = None;
    }
}

fn remove_option_from_config(config: &mut BoardViewConfig, option_id: i64) {
    config.filters.retain_mut(|filter| {
        let Some(FilterOperand::OptionIds(ids)) = filter.operand.as_mut() else {
            return true;
        };
        ids.retain(|id| *id != option_id);
        !ids.is_empty()
    });
}
