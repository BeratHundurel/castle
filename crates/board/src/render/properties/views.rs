use super::*;

impl BoardView {
    pub(crate) fn render_view_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_name = self
            .properties
            .active_view_id
            .and_then(|id| {
                self.properties
                    .saved_views
                    .iter()
                    .find(|view| view.id == id)
            })
            .map(|view| view.name.clone())
            .unwrap_or_else(|| "All cards".to_string());
        let views = self.properties.saved_views.clone();
        Popover::new("board-view-picker")
            .anchor(Anchor::TopLeft)
            .open(self.properties.view_panel_open)
            .on_open_change(cx.listener(|this, open, _, cx| {
                this.set_view_panel_open(*open, cx);
            }))
            .p_0()
            .w(px(320.))
            .trigger(
                Button::new("toggle-board-view-picker")
                    .icon(IconName::Eye)
                    .label(if self.properties.view_config_dirty {
                        format!("{active_name} · Modified")
                    } else {
                        active_name
                    })
                    .ghost()
                    .small()
                    .selected(self.properties.view_panel_open)
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
                                    .selected(self.properties.active_view_id.is_none())
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .gap_2()
                                            .child(
                                                Icon::new(
                                                    if self.properties.active_view_id.is_none() {
                                                        IconName::CircleCheck
                                                    } else {
                                                        IconName::Eye
                                                    },
                                                )
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
                                let renaming = self.properties.renaming_view_id == Some(view_id);
                                let selected = self.properties.active_view_id == Some(view_id);
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
                                                .child(
                                                    div().flex_1().child(
                                                        Input::new(
                                                            &self.properties.rename_view_input,
                                                        )
                                                        .small(),
                                                    ),
                                                )
                                                .child(
                                                    Button::new(SharedString::from(format!(
                                                        "cancel-rename-view-{view_id}"
                                                    )))
                                                    .icon(IconName::Close)
                                                    .ghost()
                                                    .xsmall()
                                                    .tooltip("Cancel rename")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.properties.renaming_view_id = None;
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
                        self.properties.active_view_id.is_some()
                            && self.properties.view_config_dirty,
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
                        self.properties.new_view_form_open,
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
                                            .child(div().flex_1().child(
                                                Input::new(&self.properties.new_view_input).small(),
                                            ))
                                            .child(
                                                Button::new("save-new-view")
                                                    .label("Save")
                                                    .primary()
                                                    .small()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        let name = this
                                                            .properties
                                                            .new_view_input
                                                            .read(cx)
                                                            .value()
                                                            .to_string();
                                                        this.create_saved_view(name, cx);
                                                    })),
                                            ),
                                    )
                                    .when_some(
                                        self.properties.update_error.clone(),
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
}
