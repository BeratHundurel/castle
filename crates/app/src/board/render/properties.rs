use super::*;
use storage::board_properties::{
    FilterOperand, FilterOperator, PropertyDefinition, PropertyKey, PropertyKind, PropertyValue,
    SortDirection,
};

impl BoardView {
    pub(super) fn render_card_property_values(
        &self,
        entry: &EntryDTO,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = Vec::new();
        for key in &self.active_view_config.visible_properties {
            if key == &storage::board_properties::PropertyKey::RelatedNotes {
                if !entry.related_notes.is_empty() {
                    rows.push(
                        h_flex()
                            .gap_1()
                            .text_xs()
                            .text_color(cx.theme().primary_foreground.opacity(0.76))
                            .child(Icon::new(IconName::File).xsmall())
                            .child(entry.related_notes.len().to_string())
                            .into_any_element(),
                    );
                }
                continue;
            }
            let storage::board_properties::PropertyKey::Custom(property_id) = key else {
                continue;
            };
            let Some(property) = self
                .board_properties
                .definitions
                .iter()
                .find(|property| property.id == *property_id)
            else {
                continue;
            };
            let Some(value) = self
                .property_values
                .get(&(i64::from(entry.id), *property_id))
            else {
                continue;
            };
            rows.push(self.render_card_property_value(property, value, cx));
        }
        v_flex()
            .w_full()
            .min_w_0()
            .gap_1()
            .children(rows)
            .into_any_element()
    }

    fn render_card_property_value(
        &self,
        property: &PropertyDefinition,
        value: &PropertyValue,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().primary_foreground.opacity(0.68);
        match value {
            PropertyValue::Select(option_id) => {
                let option = property
                    .options
                    .iter()
                    .find(|option| option.id == *option_id);
                h_flex()
                    .min_w_0()
                    .when_some(option, |this, option| {
                        this.gap_1p5()
                            .child(
                                div()
                                    .size(px(6.))
                                    .rounded_full()
                                    .bg(self.label_marker_color(&option.color, cx)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(option.name.clone()),
                            )
                    })
                    .into_any_element()
            }
            PropertyValue::Checkbox(checked) => h_flex()
                .min_w_0()
                .gap_1p5()
                .text_xs()
                .text_color(muted)
                .child(
                    Icon::new(if *checked {
                        IconName::CircleCheck
                    } else {
                        IconName::CircleX
                    })
                    .xsmall(),
                )
                .child(div().truncate().child(property.name.clone()))
                .into_any_element(),
            PropertyValue::Date(value) => {
                let label = NaiveDate::parse_from_str(value, "%Y-%m-%d")
                    .map(|date| date.format("%b %-d, %Y").to_string())
                    .unwrap_or_else(|_| value.clone());
                self.render_card_date_pill(label, cx.theme().secondary, cx)
                    .into_any_element()
            }
            PropertyValue::Url(value) => {
                let url = value.clone();
                h_flex()
                    .id(SharedString::from(format!(
                        "card-property-url-{}",
                        property.id
                    )))
                    .min_w_0()
                    .gap_1p5()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(cx.theme().info)
                    .child(Icon::new(IconName::ExternalLink).xsmall())
                    .child(div().truncate().child(display_url(value)))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        cx.open_url(&url);
                    })
                    .into_any_element()
            }
            PropertyValue::Text(value) => property_card_text_row(&property.name, value, muted),
            PropertyValue::Number(value) => {
                property_card_text_row(&property.name, &value.to_string(), muted)
            }
        }
    }
    pub(super) fn render_property_manager(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let properties = self.board_properties.definitions.clone();
        let selected_kind = self.new_property_kind;

        Popover::new("board-property-manager")
            .anchor(Anchor::TopRight)
            .open(self.property_panel_open)
            .on_open_change(cx.listener(|this, open, _, cx| {
                this.set_property_panel_open(*open, cx);
            }))
            .p_0()
            .w(px(420.))
            .trigger(
                Button::new("manage-board-properties")
                    .icon(IconName::Settings)
                    .label("Properties")
                    .ghost()
                    .small()
                    .selected(self.property_panel_open)
                    .tooltip("Manage board properties"),
            )
            .child(
                v_flex()
                    .text_sm()
                    .child(
                        v_flex()
                            .gap_1()
                            .px_4()
                            .py_3()
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .child(
                                h_flex()
                                    .justify_between()
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Manage properties"))
                                    .when(!self.property_form_open, |this| {
                                        this.child(
                                            Button::new("start-property-form")
                                                .icon(IconName::Plus)
                                                .label("Add property")
                                                .primary()
                                                .small()
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.start_property_form(window, cx);
                                                })),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Add fields that fit this board. Lists keep their own meaning."),
                            ),
                    )
                    .when_some(self.property_update_error.clone(), |this, error| {
                        this.child(div().px_4().pt_3().text_xs().text_color(cx.theme().danger).child(error))
                    })
                    .when(!self.view_load_warnings.is_empty(), |this| {
                        this.child(
                            v_flex()
                                .gap_1()
                                .px_4()
                                .pt_3()
                                .children(self.view_load_warnings.iter().map(|warning| {
                                    div().text_xs().text_color(cx.theme().warning).child(warning.clone())
                                })),
                        )
                    })
                    .child(
                        v_flex()
                            .max_h(px(240.))
                            .overflow_y_scrollbar()
                            .when(properties.is_empty(), |this| {
                                this.child(
                                    div()
                                        .px_4()
                                        .py_5()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("No custom properties yet. Add one to show typed metadata on cards and use it in views."),
                                )
                            })
                            .children(properties.iter().map(|property| {
                                self.render_property_definition_row(property, cx)
                            })),
                    )
                    .when(self.property_form_open, |this| this.child(
                        v_flex()
                            .gap_2()
                            .p_4()
                            .border_t_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().muted_foreground)
                                    .child("New property"),
                            )
                            .child(
                                h_flex().gap_1().flex_wrap().children(
                                    [
                                        PropertyKind::Text,
                                        PropertyKind::Number,
                                        PropertyKind::Checkbox,
                                        PropertyKind::Date,
                                        PropertyKind::Select,
                                        PropertyKind::Url,
                                    ]
                                    .into_iter()
                                    .map(|kind| {
                                        Button::new(SharedString::from(format!(
                                            "new-property-kind-{}",
                                            kind.as_str()
                                        )))
                                        .label(property_kind_label(kind))
                                        .ghost()
                                        .xsmall()
                                        .selected(selected_kind == kind)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.select_new_property_kind(kind, cx);
                                        }))
                                    }),
                                ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(property_kind_description(selected_kind)),
                            )
                            .child(Input::new(&self.new_property_input).small())
                            .child(
                                h_flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        Button::new("cancel-property-form")
                                            .label("Cancel")
                                            .ghost()
                                            .small()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cancel_property_form(cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("create-property")
                                            .label("Create")
                                            .primary()
                                            .small()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let name = this.new_property_input.read(cx).value().to_string();
                                                this.create_board_property(name, cx);
                                            })),
                                    ),
                            ),
                    )),
            )
    }

    fn render_property_definition_row(
        &self,
        property: &PropertyDefinition,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let adding_option = self.adding_property_option_id == Some(property.id);
        let renaming = self.renaming_property_id == Some(property.id);
        let property_id = property.id;
        let position = self
            .board_properties
            .definitions
            .iter()
            .position(|candidate| candidate.id == property_id)
            .unwrap_or_default();
        let can_move_down = position + 1 < self.board_properties.definitions.len();
        v_flex()
            .gap_2()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.32))
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::Settings2)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(v_flex().min_w_0().flex_1().when_else(
                        renaming,
                        |this| this.child(Input::new(&self.rename_property_input).small()),
                        |this| {
                            this.child(div().truncate().child(property.name.clone()))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(property_kind_label(property.kind)),
                                )
                        },
                    ))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new(SharedString::from(format!(
                                    "property-up-{property_id}"
                                )))
                                .icon(IconName::ArrowUp)
                                .ghost()
                                .xsmall()
                                .disabled(position == 0)
                                .tooltip("Move up")
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.move_property(property_id, -1, cx);
                                    },
                                )),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "property-down-{property_id}"
                                )))
                                .icon(IconName::ArrowDown)
                                .ghost()
                                .xsmall()
                                .disabled(!can_move_down)
                                .tooltip("Move down")
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.move_property(property_id, 1, cx);
                                    },
                                )),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "rename-property-{property_id}"
                                )))
                                .icon(IconName::Replace)
                                .ghost()
                                .xsmall()
                                .tooltip("Rename property")
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.start_property_rename(property_id, window, cx);
                                    },
                                )),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "delete-property-{property_id}"
                                )))
                                .icon(IconName::Delete)
                                .ghost()
                                .xsmall()
                                .tooltip("Delete property")
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.confirm_delete_property(property_id, window, cx);
                                    },
                                )),
                            ),
                    ),
            )
            .when(property.kind == PropertyKind::Select, |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .pl_6()
                        .children(property.options.iter().enumerate().map(
                            |(option_index, option)| {
                                let option_id = option.id;
                                let renaming = self.renaming_property_option_id == Some(option_id);
                                let can_move_down = option_index + 1 < property.options.len();
                                h_flex()
                                    .min_h_7()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "option-color-{option_id}"
                                            )))
                                            .size_3()
                                            .rounded_full()
                                            .cursor_pointer()
                                            .bg(self.label_marker_color(&option.color, cx))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.cycle_property_option_color(option_id, cx);
                                            })),
                                    )
                                    .child(div().min_w_0().flex_1().when_else(
                                        renaming,
                                        |this| {
                                            this.child(
                                                Input::new(&self.rename_property_option_input)
                                                    .small(),
                                            )
                                        },
                                        |this| {
                                            this.child(
                                                div()
                                                    .truncate()
                                                    .text_sm()
                                                    .child(option.name.clone()),
                                            )
                                        },
                                    ))
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "option-up-{option_id}"
                                        )))
                                        .icon(IconName::ArrowUp)
                                        .ghost()
                                        .xsmall()
                                        .disabled(option_index == 0)
                                        .tooltip("Move option up")
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.move_property_option(
                                                    property_id,
                                                    option_id,
                                                    -1,
                                                    cx,
                                                );
                                            }),
                                        ),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "option-down-{option_id}"
                                        )))
                                        .icon(IconName::ArrowDown)
                                        .ghost()
                                        .xsmall()
                                        .disabled(!can_move_down)
                                        .tooltip("Move option down")
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.move_property_option(
                                                    property_id,
                                                    option_id,
                                                    1,
                                                    cx,
                                                );
                                            }),
                                        ),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "rename-option-{option_id}"
                                        )))
                                        .icon(IconName::Replace)
                                        .ghost()
                                        .xsmall()
                                        .tooltip("Rename option")
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.start_property_option_rename(
                                                    option_id, window, cx,
                                                );
                                            }),
                                        ),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "delete-option-{option_id}"
                                        )))
                                        .icon(IconName::Delete)
                                        .ghost()
                                        .xsmall()
                                        .tooltip("Delete option and clear its values")
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.confirm_delete_property_option(
                                                    option_id, window, cx,
                                                );
                                            }),
                                        ),
                                    )
                            },
                        ))
                        .when(!adding_option, |this| {
                            this.child(
                                Button::new(SharedString::from(format!(
                                    "add-property-option-{property_id}"
                                )))
                                .icon(IconName::Plus)
                                .label("Add option")
                                .ghost()
                                .xsmall()
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.start_adding_property_option(property_id, window, cx);
                                    },
                                )),
                            )
                        }),
                )
            })
            .when(adding_option, |this| {
                this.child(
                    h_flex()
                        .gap_2()
                        .pl_6()
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&self.new_property_option_input).small()),
                        )
                        .child(
                            Button::new(SharedString::from(format!("create-option-{property_id}")))
                                .label("Add")
                                .primary()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let name =
                                        this.new_property_option_input.read(cx).value().to_string();
                                    this.create_board_property_option(name, cx);
                                })),
                        ),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_entry_properties(
        &self,
        selected_entry: Option<(&str, &EntryDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entry_id = selected_entry.map(|(_, entry)| entry.id as i64);
        let definitions = &self.board_properties.definitions;

        v_flex()
            .gap_2()
            .when_some(self.property_update_error.clone(), |this, error| {
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
            entry_id.and_then(|entry_id| self.property_values.get(&(entry_id, property.id)));
        let value_element = self.render_property_value(entry_id, property, value, cx);
        let key = entry_id.map(|entry_id| (entry_id, property.id));
        let error = key
            .and_then(|key| self.property_field_errors.get(&key))
            .cloned();
        let saving = key.is_some_and(|key| self.saving_property_values.contains(&key));

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
                let query = if self.selecting_property_id == Some(property_id) {
                    self.property_select_search_input
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
                    .open(self.selecting_property_id == Some(property_id))
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
                            .child(Input::new(&self.property_select_search_input).small())
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
            PropertyKind::Date if self.editing_property_id == Some(property.id) => {
                DatePicker::new(&self.property_date_picker)
                    .w_full()
                    .cleanable(true)
                    .placeholder("Not set")
                    .number_of_months(1)
                    .into_any_element()
            }
            PropertyKind::Text | PropertyKind::Number | PropertyKind::Url
                if self.editing_property_id == Some(property.id) =>
            {
                Input::new(&self.property_value_input)
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

    pub(super) fn render_view_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_name = self
            .active_view_id
            .and_then(|id| self.saved_views.iter().find(|view| view.id == id))
            .map(|view| view.name.clone())
            .unwrap_or_else(|| "All cards".to_string());
        let views = self.saved_views.clone();
        Popover::new("board-view-picker")
            .anchor(Anchor::TopLeft)
            .open(self.view_panel_open)
            .on_open_change(cx.listener(|this, open, _, cx| {
                this.set_view_panel_open(*open, cx);
            }))
            .p_0()
            .w(px(320.))
            .trigger(
                Button::new("toggle-board-view-picker")
                    .icon(IconName::Eye)
                    .label(if self.view_config_dirty {
                        format!("{active_name} · Modified")
                    } else {
                        active_name
                    })
                    .ghost()
                    .small()
                    .selected(self.view_panel_open)
                    .dropdown_caret(true)
                    .tooltip("Switch or save board view"),
            )
            .child(
                v_flex()
                    .text_sm()
                    .child(
                        h_flex()
                            .min_h_10()
                            .px_3()
                            .font_weight(FontWeight::SEMIBOLD)
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .child("Views"),
                    )
                    .child(
                        v_flex()
                            .max_h(px(240.))
                            .overflow_y_scrollbar()
                            .p_1()
                            .child(
                                Button::new("select-all-cards-view")
                                    .ghost()
                                    .small()
                                    .w_full()
                                    .selected(self.active_view_id.is_none())
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .gap_2()
                                            .child(
                                                Icon::new(if self.active_view_id.is_none() {
                                                    IconName::CircleCheck
                                                } else {
                                                    IconName::Eye
                                                })
                                                .xsmall(),
                                            )
                                            .child(div().flex_1().child("All cards")),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.select_saved_view(None, cx);
                                    })),
                            )
                            .children(views.iter().map(|view| {
                                let view_id = view.id;
                                let is_default = view.is_default;
                                let renaming = self.renaming_view_id == Some(view_id);
                                let selected = self.active_view_id == Some(view_id);
                                let view_name = view.name.clone();
                                h_flex()
                                    .id(SharedString::from(format!("saved-view-row-{view_id}")))
                                    .w_full()
                                    .min_h_8()
                                    .gap_1()
                                    .rounded(cx.theme().radius)
                                    .when(selected, |this| this.bg(cx.theme().secondary))
                                    .when_else(
                                        renaming,
                                        |this| {
                                            this.p_1()
                                                .child(div().flex_1().child(
                                                    Input::new(&self.rename_view_input).small(),
                                                ))
                                                .child(
                                                    Button::new(SharedString::from(format!(
                                                        "cancel-rename-view-{view_id}"
                                                    )))
                                                    .icon(IconName::Close)
                                                    .ghost()
                                                    .xsmall()
                                                    .tooltip("Cancel rename")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.renaming_view_id = None;
                                                        cx.notify();
                                                    })),
                                                )
                                        },
                                        |this| {
                                            this.child(
                                                Button::new(SharedString::from(format!(
                                                    "select-view-{view_id}"
                                                )))
                                                .ghost()
                                                .small()
                                                .flex_1()
                                                .child(
                                                    h_flex()
                                                        .w_full()
                                                        .min_w_0()
                                                        .gap_2()
                                                        .child(
                                                            Icon::new(if selected {
                                                                IconName::CircleCheck
                                                            } else {
                                                                IconName::Eye
                                                            })
                                                            .xsmall(),
                                                        )
                                                        .child(
                                                            div()
                                                                .min_w_0()
                                                                .flex_1()
                                                                .truncate()
                                                                .child(view_name),
                                                        )
                                                        .when(is_default, |this| {
                                                            this.child(
                                                                div()
                                                                    .flex_shrink_0()
                                                                    .text_xs()
                                                                    .text_color(
                                                                        cx.theme().muted_foreground,
                                                                    )
                                                                    .child("Default"),
                                                            )
                                                        }),
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.select_saved_view(Some(view_id), cx);
                                                })),
                                            )
                                            .child(
                                                Button::new(SharedString::from(format!(
                                                    "view-actions-{view_id}"
                                                )))
                                                .icon(IconName::Ellipsis)
                                                .ghost()
                                                .compact()
                                                .tooltip("View actions")
                                                .dropdown_menu_with_anchor(
                                                    Anchor::TopRight,
                                                    move |menu, _, cx| {
                                                        let danger = cx.theme().danger;
                                                        menu.menu_with_icon(
                                                            "Rename",
                                                            IconName::Replace,
                                                            Box::new(RenameBoardViewAction(
                                                                view_id,
                                                            )),
                                                        )
                                                        .menu_with_disabled(
                                                            "Set as default",
                                                            Box::new(SetDefaultBoardViewAction(
                                                                view_id,
                                                            )),
                                                            is_default,
                                                        )
                                                        .separator()
                                                        .menu_element(
                                                            Box::new(DeleteBoardViewAction(
                                                                view_id,
                                                            )),
                                                            move |_, _| {
                                                                h_flex()
                                                                    .w_full()
                                                                    .justify_between()
                                                                    .text_color(danger)
                                                                    .child("Delete view")
                                                                    .child(
                                                                        Icon::new(IconName::Delete)
                                                                            .xsmall(),
                                                                    )
                                                            },
                                                        )
                                                    },
                                                ),
                                            )
                                        },
                                    )
                            })),
                    )
                    .when(
                        self.active_view_id.is_some() && self.view_config_dirty,
                        |this| {
                            this.child(
                                h_flex()
                                    .gap_2()
                                    .px_3()
                                    .py_2()
                                    .border_t_1()
                                    .border_color(cx.theme().border.opacity(0.72))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("Unsaved changes"),
                                    )
                                    .child(
                                        Button::new("update-active-view")
                                            .label("Update")
                                            .primary()
                                            .small()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.update_active_view(cx);
                                            })),
                                    ),
                            )
                        },
                    )
                    .when_else(
                        self.new_view_form_open,
                        |this| {
                            this.child(
                                v_flex()
                                    .gap_2()
                                    .p_3()
                                    .border_t_1()
                                    .border_color(cx.theme().border.opacity(0.72))
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("New view"),
                                            )
                                            .child(
                                                Button::new("cancel-new-view")
                                                    .icon(IconName::Close)
                                                    .ghost()
                                                    .xsmall()
                                                    .tooltip("Cancel")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.cancel_new_view_form(cx);
                                                    })),
                                            ),
                                    )
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(
                                                div().flex_1().child(
                                                    Input::new(&self.new_view_input).small(),
                                                ),
                                            )
                                            .child(
                                                Button::new("save-new-view")
                                                    .label("Save")
                                                    .primary()
                                                    .small()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        let name = this
                                                            .new_view_input
                                                            .read(cx)
                                                            .value()
                                                            .to_string();
                                                        this.create_saved_view(name, cx);
                                                    })),
                                            ),
                                    )
                                    .when_some(
                                        self.property_update_error.clone(),
                                        |this, error| {
                                            this.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().danger)
                                                    .child(error),
                                            )
                                        },
                                    ),
                            )
                        },
                        |this| {
                            this.child(
                                div()
                                    .p_1()
                                    .border_t_1()
                                    .border_color(cx.theme().border.opacity(0.72))
                                    .child(
                                        Button::new("start-new-view")
                                            .icon(IconName::Plus)
                                            .label("Save as new view")
                                            .ghost()
                                            .small()
                                            .w_full()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.start_new_view_form(window, cx);
                                            })),
                                    ),
                            )
                        },
                    ),
            )
    }

    pub(super) fn render_fields_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut fields = vec![
            PropertyKey::Labels,
            PropertyKey::DueDate,
            PropertyKey::RelatedNotes,
        ];
        fields.extend(
            self.board_properties
                .definitions
                .iter()
                .map(|property| PropertyKey::Custom(property.id)),
        );

        let selected = self.active_view_config.visible_properties.clone();

        Popover::new("board-fields-picker")
            .anchor(Anchor::TopRight)
            .open(self.fields_panel_open)
            .on_open_change(cx.listener(|this, open, _, cx| {
                this.set_fields_panel_open(*open, cx);
            }))
            .p_0()
            .w(px(320.))
            .trigger(
                Button::new("toggle-board-fields")
                    .icon(IconName::LayoutDashboard)
                    .label("Fields")
                    .ghost()
                    .small()
                    .selected(self.fields_panel_open)
                    .tooltip("Choose up to three fields shown on cards"),
            )
            .child(
                v_flex()
                    .text_sm()
                    .child(
                        v_flex()
                            .gap_1()
                            .px_4()
                            .py_3()
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child("Card fields"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Show up to three ordered fields on every card in this view."),
                            ),
                    )
                    .child(
                        v_flex().p_2().children(fields.into_iter().map(|field| {
                            let checked = selected.contains(&field);
                            let field_for_toggle = field.clone();
                            let field_for_up = field.clone();
                            let field_for_down = field.clone();
                            let index = selected.iter().position(|candidate| candidate == &field);
                            h_flex()
                                .min_h_8()
                                .gap_2()
                                .child(
                                    Checkbox::new(SharedString::from(format!("visible-field-{}", property_key_id(&field))))
                                        .checked(checked)
                                        .small()
                                        .label(self.property_key_label(&field))
                                        .flex_1()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.toggle_visible_property(field_for_toggle.clone(), cx);
                                        })),
                                )
                                .when_some(index, |this, index| {
                                    this.child(
                                        Button::new(SharedString::from(format!("field-up-{}", property_key_id(&field))))
                                            .icon(IconName::ArrowUp)
                                            .ghost()
                                            .xsmall()
                                            .disabled(index == 0)
                                            .tooltip("Move field up")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.move_visible_property(&field_for_up, -1, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!("field-down-{}", property_key_id(&field))))
                                            .icon(IconName::ArrowDown)
                                            .ghost()
                                            .xsmall()
                                            .disabled(index + 1 >= selected.len())
                                            .tooltip("Move field down")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.move_visible_property(&field_for_down, 1, cx);
                                            })),
                                    )
                                })
                        })),
                    )
                    .child(
                        div()
                            .p_2()
                            .border_t_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .child(
                                Checkbox::new("compact-board-cards")
                                    .checked(self.active_view_config.compact_cards)
                                    .small()
                                    .label("Compact cards")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_compact_cards(cx);
                                    })),
                            ),
                    ),
            )
    }

    pub(super) fn render_sort_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut fields = vec![
            PropertyKey::DueDate,
            PropertyKey::Labels,
            PropertyKey::RelatedNotes,
        ];
        fields.extend(
            self.board_properties
                .definitions
                .iter()
                .map(|property| PropertyKey::Custom(property.id)),
        );
        let active_sort = self.active_view_config.sort.clone();
        Popover::new("board-sort-picker")
            .anchor(Anchor::TopRight)
            .open(self.sort_panel_open)
            .on_open_change(cx.listener(|this, open, _, cx| {
                this.set_sort_panel_open(*open, cx);
            }))
            .p_0()
            .w(px(280.))
            .trigger(
                Button::new("toggle-board-sort")
                    .icon(IconName::SortAscending)
                    .label(if active_sort.is_some() {
                        "Sort · 1"
                    } else {
                        "Sort"
                    })
                    .ghost()
                    .small()
                    .selected(active_sort.is_some() || self.sort_panel_open)
                    .tooltip("Sort temporarily within each list"),
            )
            .child(
                v_flex()
                    .text_sm()
                    .child(
                        h_flex()
                            .px_4()
                            .py_3()
                            .justify_between()
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Sort within lists"),
                            )
                            .when(active_sort.is_some(), |this| {
                                this.child(
                                    Button::new("clear-board-sort")
                                        .label("Clear")
                                        .ghost()
                                        .xsmall()
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.clear_sort(cx)),
                                        ),
                                )
                            }),
                    )
                    .child(v_flex().justify_center().items_start().p_1().children(
                        fields.into_iter().map(|field| {
                            let selected_sort =
                                active_sort.as_ref().filter(|sort| sort.property == field);
                            let label = match selected_sort.map(|sort| sort.direction) {
                                Some(SortDirection::Ascending) => {
                                    format!("{} · Ascending", self.property_key_label(&field))
                                }
                                Some(SortDirection::Descending) => {
                                    format!("{} · Descending", self.property_key_label(&field))
                                }
                                None => self.property_key_label(&field),
                            };
                            Button::new(SharedString::from(format!(
                                "sort-field-{}",
                                property_key_id(&field)
                            )))
                            .label(label)
                            .ghost()
                            .small()
                            .selected(selected_sort.is_some())
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.set_sort(field.clone(), cx);
                                },
                            ))
                        }),
                    ))
                    .child(
                        div()
                            .px_4()
                            .py_2()
                            .border_t_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Manual order is preserved. Empty values stay last."),
                    ),
            )
    }

    pub(super) fn render_custom_filter_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let definitions = self.board_properties.definitions.clone();
        v_flex()
            .when(!definitions.is_empty(), |this| {
                this.child(
                    v_flex()
                        .gap_2()
                        .px_4()
                        .py_3()
                        .border_t_1()
                        .border_color(cx.theme().border.opacity(0.72))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(cx.theme().muted_foreground)
                                .child("Custom properties"),
                        )
                        .children(
                            definitions
                                .iter()
                                .map(|property| self.render_custom_filter_row(property, cx)),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_custom_filter_row(
        &self,
        property: &PropertyDefinition,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let property_id = property.id;
        let filter = self
            .filters
            .custom
            .iter()
            .find(|filter| filter.property == PropertyKey::Custom(property_id));
        match property.kind {
            PropertyKind::Checkbox => {
                let selected = filter.map(|filter| filter.operator);
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(property.name.clone()))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new(SharedString::from(format!(
                                    "filter-checkbox-true-{property_id}"
                                )))
                                .label("Checked")
                                .ghost()
                                .xsmall()
                                .selected(selected == Some(FilterOperator::IsChecked))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.set_checkbox_filter(property_id, Some(true), cx);
                                    },
                                )),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "filter-checkbox-false-{property_id}"
                                )))
                                .label("Unchecked")
                                .ghost()
                                .xsmall()
                                .selected(selected == Some(FilterOperator::IsUnchecked))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.set_checkbox_filter(property_id, Some(false), cx);
                                    },
                                )),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "filter-checkbox-empty-{property_id}"
                                )))
                                .label("Empty")
                                .ghost()
                                .xsmall()
                                .selected(selected == Some(FilterOperator::IsEmpty))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.set_empty_checkbox_filter(property_id, cx);
                                    },
                                )),
                            )
                            .when(selected.is_some(), |this| {
                                this.child(
                                    Button::new(SharedString::from(format!(
                                        "filter-checkbox-clear-{property_id}"
                                    )))
                                    .label("Clear")
                                    .ghost()
                                    .xsmall()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_checkbox_filter(property_id, None, cx);
                                    })),
                                )
                            }),
                    )
                    .into_any_element()
            }
            PropertyKind::Select => {
                let selected_ids = filter
                    .and_then(|filter| filter.operand.as_ref())
                    .and_then(|operand| match operand {
                        FilterOperand::OptionIds(ids) => Some(ids.as_slice()),
                        _ => None,
                    })
                    .unwrap_or_default();
                let operandless = filter.is_some_and(|filter| {
                    matches!(
                        filter.operator,
                        FilterOperator::IsEmpty | FilterOperator::IsNotEmpty
                    )
                });
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .child(property.name.clone())
                            .when(filter.is_some(), |this| {
                                this.child(
                                    Button::new(SharedString::from(format!(
                                        "cycle-filter-{property_id}"
                                    )))
                                    .label(filter_operator_label(
                                        filter
                                            .map(|filter| filter.operator)
                                            .unwrap_or(FilterOperator::IsAnyOf),
                                    ))
                                    .ghost()
                                    .xsmall()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.cycle_custom_filter_operator(property_id, cx);
                                    })),
                                )
                            }),
                    )
                    .when(!operandless, |this| {
                        this.children(property.options.iter().map(|option| {
                            let option_id = option.id;
                            Checkbox::new(SharedString::from(format!(
                                "filter-option-{property_id}-{option_id}"
                            )))
                            .checked(selected_ids.contains(&option_id))
                            .small()
                            .label(option.name.clone())
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.toggle_select_filter_option(property_id, option_id, cx);
                                },
                            ))
                        }))
                    })
                    .into_any_element()
            }
            _ => {
                let editing = self.editing_filter_property_id == Some(property_id);
                let operator =
                    filter
                        .map(|filter| filter.operator)
                        .unwrap_or(match property.kind {
                            PropertyKind::Date => FilterOperator::On,
                            PropertyKind::Number => FilterOperator::Equals,
                            _ => FilterOperator::Contains,
                        });
                let operandless = matches!(
                    operator,
                    FilterOperator::IsEmpty | FilterOperator::IsNotEmpty
                );
                let label = filter
                    .and_then(|filter| filter.operand.as_ref())
                    .map(filter_operand_label)
                    .unwrap_or_else(|| "Add value".to_string());
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(property.name.clone()))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new(SharedString::from(format!(
                                    "filter-operator-{property_id}"
                                )))
                                .label(filter_operator_label(operator))
                                .ghost()
                                .xsmall()
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.cycle_custom_filter_operator(property_id, cx);
                                    },
                                )),
                            )
                            .when(!operandless, |this| {
                                this.when_else(
                                    editing,
                                    |this| {
                                        this.child(
                                            div().flex_1().child(
                                                Input::new(&self.filter_value_input).small(),
                                            ),
                                        )
                                    },
                                    |this| {
                                        this.child(
                                            Button::new(SharedString::from(format!(
                                                "filter-value-{property_id}"
                                            )))
                                            .label(label)
                                            .outline()
                                            .xsmall()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.start_custom_filter(property_id, window, cx);
                                            })),
                                        )
                                    },
                                )
                            })
                            .when(filter.is_some(), |this| {
                                this.child(
                                    Button::new(SharedString::from(format!(
                                        "remove-filter-{property_id}"
                                    )))
                                    .icon(IconName::Close)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Remove filter")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_custom_filter(property_id, cx);
                                    })),
                                )
                            }),
                    )
                    .into_any_element()
            }
        }
    }

    pub(super) fn property_key_label(&self, key: &PropertyKey) -> String {
        match key {
            PropertyKey::DueDate => "Due date".to_string(),
            PropertyKey::Labels => "Labels".to_string(),
            PropertyKey::RelatedNotes => "Related notes".to_string(),
            PropertyKey::Custom(property_id) => self
                .board_properties
                .definitions
                .iter()
                .find(|property| property.id == *property_id)
                .map(|property| property.name.clone())
                .unwrap_or_else(|| "Unavailable property".to_string()),
        }
    }
}

fn property_kind_label(kind: PropertyKind) -> &'static str {
    match kind {
        PropertyKind::Text => "Text",
        PropertyKind::Number => "Number",
        PropertyKind::Checkbox => "Checkbox",
        PropertyKind::Date => "Date",
        PropertyKind::Select => "Select",
        PropertyKind::Url => "URL",
    }
}

fn property_kind_description(kind: PropertyKind) -> &'static str {
    match kind {
        PropertyKind::Text => "Short text for names, references, or notes.",
        PropertyKind::Number => "A sortable numeric value such as effort or cost.",
        PropertyKind::Checkbox => "An optional true or false flag.",
        PropertyKind::Date => "A calendar date independent from Castle's due date.",
        PropertyKind::Select => "One choice from a board-specific list of colored options.",
        PropertyKind::Url => "A validated web address that can be opened from Castle.",
    }
}

fn property_value_label(
    property: &PropertyDefinition,
    value: Option<&PropertyValue>,
) -> SharedString {
    match value {
        Some(PropertyValue::Text(value))
        | Some(PropertyValue::Date(value))
        | Some(PropertyValue::Url(value)) => value.clone().into(),
        Some(PropertyValue::Number(value)) => value.to_string().into(),
        Some(PropertyValue::Checkbox(value)) => if *value { "Checked" } else { "Unchecked" }.into(),
        Some(PropertyValue::Select(option_id)) => property
            .options
            .iter()
            .find(|option| option.id == *option_id)
            .map(|option| SharedString::from(option.name.clone()))
            .unwrap_or_else(|| SharedString::from("Unavailable option")),
        None => match property.kind {
            PropertyKind::Checkbox => SharedString::from("Unchecked"),
            _ => SharedString::from("Empty"),
        },
    }
}

fn property_card_text_row(name: &str, value: &str, muted: Hsla) -> AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .text_xs()
        .child(
            div()
                .flex_shrink_0()
                .text_color(muted)
                .child(name.to_string()),
        )
        .child(div().min_w_0().truncate().child(value.to_string()))
        .into_any_element()
}

fn display_url(value: &str) -> String {
    value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .unwrap_or(value)
        .trim_end_matches('/')
        .to_string()
}

fn property_key_id(key: &PropertyKey) -> String {
    match key {
        PropertyKey::DueDate => "due-date".to_string(),
        PropertyKey::Labels => "labels".to_string(),
        PropertyKey::RelatedNotes => "related-notes".to_string(),
        PropertyKey::Custom(id) => format!("custom-{id}"),
    }
}

fn filter_operator_label(operator: FilterOperator) -> &'static str {
    match operator {
        FilterOperator::Contains => "Contains",
        FilterOperator::DoesNotContain => "Does not contain",
        FilterOperator::Equals => "Equals",
        FilterOperator::GreaterThan => "Greater than",
        FilterOperator::LessThan => "Less than",
        FilterOperator::Before => "Before",
        FilterOperator::On => "On",
        FilterOperator::After => "After",
        FilterOperator::IsAnyOf => "Is any of",
        FilterOperator::IsNoneOf => "Is none of",
        FilterOperator::IsEmpty => "Is empty",
        FilterOperator::IsNotEmpty => "Is not empty",
        FilterOperator::IsChecked => "Checked",
        FilterOperator::IsUnchecked => "Unchecked",
    }
}

fn filter_operand_label(operand: &FilterOperand) -> String {
    match operand {
        FilterOperand::Text(value) | FilterOperand::Date(value) => value.clone(),
        FilterOperand::Number(value) => value.to_string(),
        FilterOperand::OptionIds(values) => format!("{} option(s)", values.len()),
        FilterOperand::LabelIds(values) => format!("{} label(s)", values.len()),
        FilterOperand::DueDatePresets(values) => format!("{} date range(s)", values.len()),
    }
}
