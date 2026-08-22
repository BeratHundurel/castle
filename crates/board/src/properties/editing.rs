use super::*;

impl BoardView {
    pub(crate) fn start_editing_property_value(
        &mut self,
        entry_id: i64,
        property_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.properties.values.get(&(entry_id, property_id));
        self.properties.editing_property_id = Some(property_id);
        self.properties
            .field_errors
            .remove(&(entry_id, property_id));
        if matches!(value, Some(PropertyValue::Date(_)))
            || self
                .properties
                .data
                .definitions
                .iter()
                .any(|property| property.id == property_id && property.kind == PropertyKind::Date)
        {
            let date = value.and_then(|value| match value {
                PropertyValue::Date(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d").ok(),
                _ => None,
            });
            self.properties
                .property_date_picker
                .update(cx, |picker, cx| {
                    picker.set_date(Date::Single(date), window, cx);
                });
        } else {
            let text = value.map(property_value_text).unwrap_or_default();
            self.properties
                .property_value_input
                .update(cx, |input, cx| {
                    input.set_value(text, window, cx);
                    input.focus(window, cx);
                });
        }
        cx.notify();
    }

    pub(crate) fn set_property_select_open(
        &mut self,
        property_id: i64,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.properties.selecting_property_id = open.then_some(property_id);
        self.properties
            .property_select_search_input
            .update(cx, |input, cx| {
                input.set_value("", window, cx);
                if open {
                    input.focus(window, cx);
                }
            });
        cx.notify();
    }

    pub(crate) fn commit_property_value(&mut self, value: String, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_editing.dialog.entry_id.map(i64::from) else {
            return;
        };
        let Some(property_id) = self.properties.editing_property_id else {
            return;
        };
        let Some(kind) = self
            .properties
            .data
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
        self.properties.editing_property_id = None;
        self.set_entry_property_value(entry_id, property_id, parsed, cx);
    }

    fn set_property_field_error(
        &mut self,
        entry_id: i64,
        property_id: i64,
        message: &str,
        cx: &mut Context<Self>,
    ) {
        self.properties
            .field_errors
            .insert((entry_id, property_id), message.into());
        cx.notify();
    }

    pub(crate) fn set_property_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.properties.property_panel_open = open;
        if !open {
            self.properties.property_form_open = false;
            self.properties.adding_property_option_id = None;
            self.properties.renaming_property_id = None;
            self.properties.renaming_property_option_id = None;
        }
        cx.notify();
    }

    pub(crate) fn set_fields_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.properties.fields_panel_open = open;
        cx.notify();
    }

    pub(crate) fn set_view_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.properties.view_panel_open = open;
        if !open {
            self.properties.new_view_form_open = false;
            self.properties.renaming_view_id = None;
        }
        cx.notify();
    }

    pub(crate) fn start_new_view_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.properties.new_view_form_open = true;
        self.properties.update_error = None;
        self.properties.new_view_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn cancel_new_view_form(&mut self, cx: &mut Context<Self>) {
        self.properties.new_view_form_open = false;
        self.properties.update_error = None;
        cx.notify();
    }

    pub(crate) fn set_sort_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.properties.sort_panel_open = open;
        cx.notify();
    }

    pub(crate) fn start_property_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.properties.property_form_open = true;
        self.properties.update_error = None;
        self.properties.new_property_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn cancel_property_form(&mut self, cx: &mut Context<Self>) {
        self.properties.property_form_open = false;
        self.properties.update_error = None;
        cx.notify();
    }

    pub(crate) fn select_new_property_kind(&mut self, kind: PropertyKind, cx: &mut Context<Self>) {
        self.properties.new_property_kind = kind;
        cx.notify();
    }

    pub(crate) fn start_adding_property_option(
        &mut self,
        property_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.properties.adding_property_option_id = Some(property_id);
        self.properties
            .new_property_option_input
            .update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
        cx.notify();
    }
}
