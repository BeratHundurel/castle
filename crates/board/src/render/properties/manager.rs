use super::*;

impl BoardView {
    pub(crate) fn render_property_manager(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let properties = self.properties.data.definitions.clone();
        let selected_kind = self.properties.new_property_kind;

        Popover::new("board-property-manager")
            .anchor(Anchor::TopRight)
            .open(self.properties.property_panel_open)
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
                    .selected(self.properties.property_panel_open)
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
                                    .when(!self.properties.property_form_open, |this| {
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
                    .when_some(self.properties.update_error.clone(), |this, error| {
                        this.child(div().px_4().pt_3().text_xs().text_color(cx.theme().danger).child(error))
                    })
                    .when(!self.properties.view_load_warnings.is_empty(), |this| {
                        this.child(
                            v_flex()
                                .gap_1()
                                .px_4()
                                .pt_3()
                                .children(self.properties.view_load_warnings.iter().map(|warning| {
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
                    .when(self.properties.property_form_open, |this| this.child(
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
                            .child(Input::new(&self.properties.new_property_input).small())
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
                                                let name = this.properties.new_property_input.read(cx).value().to_string();
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
        let adding_option = self.properties.adding_property_option_id == Some(property.id);
        let renaming = self.properties.renaming_property_id == Some(property.id);
        let property_id = property.id;
        let position = self
            .properties
            .data
            .definitions
            .iter()
            .position(|candidate| candidate.id == property_id)
            .unwrap_or_default();
        let can_move_down = position + 1 < self.properties.data.definitions.len();
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
                        |this| {
                            this.child(Input::new(&self.properties.rename_property_input).small())
                        },
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
                                let renaming =
                                    self.properties.renaming_property_option_id == Some(option_id);
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
                                                Input::new(
                                                    &self.properties.rename_property_option_input,
                                                )
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
                            div().flex_1().child(
                                Input::new(&self.properties.new_property_option_input).small(),
                            ),
                        )
                        .child(
                            Button::new(SharedString::from(format!("create-option-{property_id}")))
                                .label("Add")
                                .primary()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let name = this
                                        .properties
                                        .new_property_option_input
                                        .read(cx)
                                        .value()
                                        .to_string();
                                    this.create_board_property_option(name, cx);
                                })),
                        ),
                )
            })
            .into_any_element()
    }
}
