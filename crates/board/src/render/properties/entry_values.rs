use super::*;

impl BoardView {
    pub(crate) fn render_entry_properties(
        &self,
        selected_entry: Option<(&str, &BoardCardDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entry_id = selected_entry.map(|(_, entry)| entry.id as i64);
        let definitions = &self.properties.data.definitions;

        v_flex()
            .gap_2()
            .when_some(self.properties.update_error.clone(), |this, error| {
                this.child(div().text_xs().text_color(cx.theme().danger).child(error))
            })
            .when(!definitions.is_empty(), |this| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().muted_foreground)
                        .child(Icon::new(IconName::Settings2).xsmall())
                        .child("Properties"),
                )
                .child(
                    v_flex()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().border.opacity(0.48))
                        .children(definitions.iter().map(|property| {
                            self.render_entry_property_row(entry_id, property, cx)
                        })),
                )
            })
    }

    fn render_entry_property_row(
        &self,
        entry_id: Option<i64>,
        property: &PropertyDefinition,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value =
            entry_id.and_then(|entry_id| self.properties.values.get(&(entry_id, property.id)));
        let value_element = self.render_property_value(entry_id, property, value, cx);
        let key = entry_id.map(|entry_id| (entry_id, property.id));
        let error = key
            .and_then(|key| self.properties.field_errors.get(&key))
            .cloned();
        let saving = key.is_some_and(|key| self.properties.saving_values.contains(&key));

        v_flex()
            .gap_1()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.32))
            .child(
                h_flex()
                    .min_h(px(32.))
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .w(px(132.))
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(property.name.clone()),
                    )
                    .child(value_element)
                    .when(saving, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Saving…"),
                        )
                    }),
            )
            .when_some(error, |this, error| {
                this.child(
                    div()
                        .pl(px(144.))
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .into_any_element()
    }

    fn render_property_value(
        &self,
        entry_id: Option<i64>,
        property: &PropertyDefinition,
        value: Option<&PropertyValue>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry_id) = entry_id else {
            return div().flex_1().child("Empty").into_any_element();
        };
        match property.kind {
            PropertyKind::Checkbox => {
                let property_id = property.id;
                let selected = match value {
                    Some(PropertyValue::Checkbox(value)) => Some(*value),
                    _ => None,
                };
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(
                        Button::new(SharedString::from(format!("property-true-{property_id}")))
                            .label("Checked")
                            .ghost()
                            .xsmall()
                            .selected(selected == Some(true))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.set_entry_property_value(
                                    entry_id,
                                    property_id,
                                    Some(PropertyValue::Checkbox(true)),
                                    cx,
                                );
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("property-false-{property_id}")))
                            .label("Unchecked")
                            .ghost()
                            .xsmall()
                            .selected(selected == Some(false))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.set_entry_property_value(
                                    entry_id,
                                    property_id,
                                    Some(PropertyValue::Checkbox(false)),
                                    cx,
                                );
                            })),
                    )
                    .when(selected.is_some(), |this| {
                        this.child(
                            Button::new(SharedString::from(format!(
                                "property-clear-{property_id}"
                            )))
                            .icon(IconName::Close)
                            .ghost()
                            .xsmall()
                            .tooltip("Clear value")
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.set_entry_property_value(entry_id, property_id, None, cx);
                                },
                            )),
                        )
                    })
                    .into_any_element()
            }
            PropertyKind::Select if !property.options.is_empty() => {
                let property_id = property.id;
                let selected = match value {
                    Some(PropertyValue::Select(option_id)) => Some(*option_id),
                    _ => None,
                };
                let selected_label = selected
                    .and_then(|selected| {
                        property.options.iter().find(|option| option.id == selected)
                    })
                    .map(|option| option.name.clone())
                    .unwrap_or_else(|| "Not set".to_string());
                let query = if self.properties.selecting_property_id == Some(property_id) {
                    self.properties
                        .property_select_search_input
                        .read(cx)
                        .value()
                        .to_lowercase()
                } else {
                    String::new()
                };
                let options = property
                    .options
                    .iter()
                    .filter(|option| {
                        query.is_empty() || option.name.to_lowercase().contains(&query)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                Popover::new(SharedString::from(format!("property-select-{property_id}")))
                    .anchor(Anchor::TopLeft)
                    .open(self.properties.selecting_property_id == Some(property_id))
                    .on_open_change(cx.listener(move |this, open, window, cx| {
                        this.set_property_select_open(property_id, *open, window, cx);
                    }))
                    .w(px(260.))
                    .trigger(
                        Button::new(SharedString::from(format!(
                            "property-select-trigger-{property_id}"
                        )))
                        .label(selected_label)
                        .outline()
                        .small()
                        .dropdown_caret(true),
                    )
                    .child(
                        v_flex()
                            .max_h(px(280.))
                            .overflow_y_scrollbar()
                            .p_1()
                            .child(
                                Input::new(&self.properties.property_select_search_input).small(),
                            )
                            .when(options.is_empty(), |this| {
                                this.child(
                                    div()
                                        .px_3()
                                        .py_2()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("No matching options"),
                                )
                            })
                            .children(options.iter().map(|option| {
                                let option_id = option.id;
                                let option_name = option.name.clone();
                                Button::new(SharedString::from(format!(
                                    "select-option-{property_id}-{option_id}"
                                )))
                                .label(option_name)
                                .ghost()
                                .small()
                                .selected(selected == Some(option_id))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.set_entry_property_value(
                                            entry_id,
                                            property_id,
                                            Some(PropertyValue::Select(option_id)),
                                            cx,
                                        );
                                    },
                                ))
                            }))
                            .when(selected.is_some(), |this| {
                                this.child(
                                    Button::new(SharedString::from(format!(
                                        "clear-select-{property_id}"
                                    )))
                                    .label("Clear")
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_entry_property_value(
                                            entry_id,
                                            property_id,
                                            None,
                                            cx,
                                        );
                                    })),
                                )
                            }),
                    )
                    .into_any_element()
            }
            PropertyKind::Select => div()
                .flex_1()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Add options in Manage properties")
                .into_any_element(),
            PropertyKind::Date if self.properties.editing_property_id == Some(property.id) => {
                DatePicker::new(&self.properties.property_date_picker)
                    .w_full()
                    .cleanable(true)
                    .placeholder("Not set")
                    .number_of_months(1)
                    .into_any_element()
            }
            PropertyKind::Text | PropertyKind::Number | PropertyKind::Url
                if self.properties.editing_property_id == Some(property.id) =>
            {
                Input::new(&self.properties.property_value_input)
                    .small()
                    .flex_1()
                    .into_any_element()
            }
            PropertyKind::Url => {
                let property_id = property.id;
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .when_some(
                        value.and_then(|value| match value {
                            PropertyValue::Url(value) => Some(value.clone()),
                            _ => None,
                        }),
                        |this, url| {
                            let open_url = url.clone();
                            this.child(
                                Button::new(SharedString::from(format!(
                                    "open-property-url-{property_id}"
                                )))
                                .icon(IconName::ExternalLink)
                                .label(display_url(&url))
                                .ghost()
                                .small()
                                .on_click(move |_, _, cx| {
                                    cx.open_url(&open_url);
                                }),
                            )
                        },
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "edit-property-url-{property_id}"
                        )))
                        .label(if value.is_some() { "Edit" } else { "Add URL" })
                        .ghost()
                        .xsmall()
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.start_editing_property_value(
                                    entry_id,
                                    property_id,
                                    window,
                                    cx,
                                );
                            },
                        )),
                    )
                    .into_any_element()
            }
            _ => div()
                .id(SharedString::from(format!(
                    "edit-entry-property-{}-{}",
                    entry_id, property.id
                )))
                .min_w_0()
                .flex_1()
                .cursor_pointer()
                .rounded_sm()
                .px_1()
                .hover(|this| this.bg(cx.theme().accent.opacity(0.32)))
                .text_sm()
                .text_color(if value.is_some() {
                    cx.theme().foreground
                } else {
                    cx.theme().muted_foreground
                })
                .child(property_value_label(property, value))
                .on_click(cx.listener({
                    let property_id = property.id;
                    move |this, _, window, cx| {
                        this.start_editing_property_value(entry_id, property_id, window, cx);
                    }
                }))
                .into_any_element(),
        }
    }
}
