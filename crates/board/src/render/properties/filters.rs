use super::*;

impl BoardView {
    pub(crate) fn render_custom_filter_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let definitions = self.properties.data.definitions.clone();
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
                let editing = self.properties.editing_filter_property_id == Some(property_id);
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
                                        this.child(div().flex_1().child(
                                            Input::new(&self.properties.filter_value_input).small(),
                                        ))
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
}
