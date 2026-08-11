use super::*;

impl BoardView {
    pub(super) fn render_scrollable_board(
        &self,
        board_id_for_render: Option<u32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let mut cards = Vec::new();

        if let Some(error) = self.load_error.clone() {
            return div()
                .id("board-load-error")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .text_color(theme.danger)
                .child(error)
                .into_any_element();
        }

        if board_id_for_render.is_some() && self.cards.is_empty() && !self.is_adding_list {
            return self.render_empty_board(cx).into_any_element();
        }

        if let Some(board_id) = board_id_for_render {
            for card in &self.cards {
                cards.push(self.render_card(card, board_id, cx).into_any_element());
            }
        }

        let scrollable = h_flex()
            .id("scrollable-container")
            .size_full()
            .track_scroll(&self.board_scroll_handle)
            .overflow_x_scrollbar()
            .gap_4()
            .p_4()
            .items_start()
            .children(cards)
            .child({
                if self.is_adding_list {
                    Input::new(&self.new_list_input)
                        .w_80()
                        .h_10()
                        .rounded_none()
                        .focus_bordered(false)
                        .border_0()
                        .border_b_1()
                        .border_color(theme.foreground)
                        .into_any_element()
                } else {
                    self.render_add_list_button(cx).into_any_element()
                }
            })
            .into_any_element();

        if board_id_for_render.is_some() {
            v_flex()
                .size_full()
                .overflow_hidden()
                .when_some(self.mutation_error.clone(), |this, error| {
                    this.child(
                        div()
                            .px_4()
                            .py_2()
                            .border_b_1()
                            .border_color(theme.danger.opacity(0.35))
                            .bg(theme.danger.opacity(0.08))
                            .text_sm()
                            .text_color(theme.danger)
                            .child(error),
                    )
                })
                .child(self.render_filter_toolbar(cx))
                .child(scrollable)
                .into_any_element()
        } else {
            scrollable
        }
    }

    pub(super) fn render_filter_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("board-filter-toolbar")
            .min_h_10()
            .px_3()
            .gap_1()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.72))
            .bg(cx.theme().background)
            .can_drop(|value, _, _| {
                value
                    .downcast_ref::<SidebarDragInfo>()
                    .and_then(SidebarDragInfo::note_id)
                    .is_some()
            })
            .drag_over::<SidebarDragInfo>(|this, _, _, cx| {
                this.border_1()
                    .border_color(cx.theme().primary)
                    .bg(cx.theme().drop_target)
            })
            .on_drop(cx.listener(|this, info: &SidebarDragInfo, _, cx| {
                if let (Some(board_id), Some(note_id)) = (this.board_id, info.note_id()) {
                    this.link_note_to_item(
                        storage::workspace_links::WorkspaceItemRef {
                            kind: storage::workspace_links::WorkspaceItemKind::Board,
                            id: i64::from(board_id),
                        },
                        note_id,
                        cx,
                    );
                }
            }))
            .child(self.render_view_picker(cx))
            .child(
                div()
                    .mx_1()
                    .h_5()
                    .w(px(1.))
                    .bg(cx.theme().border.opacity(0.72)),
            )
            .when(!self.filters.due_dates.is_empty(), |this| {
                this.child(
                    Button::new("active-due-date-filter")
                        .icon(IconName::Close)
                        .label("Due date")
                        .ghost()
                        .xsmall()
                        .tooltip("Remove due date filter")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_due_date_filters(cx);
                        })),
                )
            })
            .when(!self.filters.label_ids.is_empty(), |this| {
                this.child(
                    Button::new("active-label-filter")
                        .icon(IconName::Close)
                        .label("Labels")
                        .ghost()
                        .xsmall()
                        .tooltip("Remove label filter")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_label_filters(cx);
                        })),
                )
            })
            .when_some(self.filters.related_notes, |this, _| {
                this.child(
                    Button::new("active-related-notes-filter")
                        .icon(IconName::Close)
                        .label("Related notes")
                        .ghost()
                        .xsmall()
                        .tooltip("Remove related notes filter")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_related_notes_filter(None, cx);
                        })),
                )
            })
            .children(self.filters.custom.iter().filter_map(|filter| {
                let storage::board_properties::PropertyKey::Custom(property_id) = &filter.property
                else {
                    return None;
                };
                let property_id = *property_id;
                Some(
                    Button::new(SharedString::from(format!(
                        "active-custom-filter-{property_id}"
                    )))
                    .icon(IconName::Close)
                    .label(self.property_key_label(&filter.property))
                    .ghost()
                    .xsmall()
                    .tooltip("Remove property filter")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.remove_custom_filter(property_id, cx);
                    })),
                )
            }))
            .when_some(self.board_id, |this, board_id| {
                this.child(self.render_related_notes_popover(
                    storage::workspace_links::WorkspaceItemRef {
                        kind: storage::workspace_links::WorkspaceItemKind::Board,
                        id: i64::from(board_id),
                    },
                    "board".into(),
                    cx,
                ))
            })
            .child(div().flex_1())
            .when(
                self.filters.is_active() || self.active_view_config.sort.is_some(),
                |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Dragging is paused in this view"),
                    )
                },
            )
            .child(
                Popover::new("board-filter-popover")
                    .anchor(Anchor::TopRight)
                    .open(self.filter_panel_open)
                    .on_open_change(cx.listener(|this, open, _, cx| {
                        this.set_filter_panel_open(*open, cx);
                    }))
                    .p_0()
                    .w_80()
                    .trigger(
                        Button::new("toggle-board-filters")
                            .icon(IconName::Settings2)
                            .label("Filter")
                            .ghost()
                            .small()
                            .selected(self.filters.is_active() || self.filter_panel_open)
                            .tooltip("Filter cards"),
                    )
                    .child(self.render_filter_panel(cx)),
            )
            .when(self.filters.is_active(), |this| {
                this.child(
                    Button::new("clear-board-filters")
                        .label("Clear")
                        .ghost()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_filters(cx);
                        })),
                )
            })
            .child(self.render_sort_picker(cx))
            .child(self.render_fields_picker(cx))
            .child(
                Button::new("copy-board-internal-link")
                    .icon(IconName::Copy)
                    .ghost()
                    .small()
                    .tooltip("Copy board internal link")
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(CopyBoardInternalLinkAction), cx);
                    }),
            )
            .child(
                Button::new("save-board-template")
                    .icon(IconName::Copy)
                    .label("Template")
                    .ghost()
                    .small()
                    .tooltip("Save this board as a reusable template")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_save_template_dialog(window, cx);
                    })),
            )
            .child(
                div()
                    .mx_1()
                    .h_5()
                    .w(px(1.))
                    .bg(cx.theme().border.opacity(0.72)),
            )
            .child(self.render_property_manager(cx))
    }

    pub(super) fn render_filter_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let labels = self.board_labels.clone();
        let board_view = cx.entity();

        v_flex()
            .id("board-filter-panel")
            .w_full()
            .max_h(px(560.))
            .overflow_y_scrollbar()
            .text_sm()
            .child(
                h_flex()
                    .min_h_12()
                    .px_4()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.72))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Filter cards"),
                    )
                    .when(self.filters.is_active(), |this| {
                        this.child(
                            Button::new("clear-board-filters-popover")
                                .label("Clear all")
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.clear_filters(cx);
                                })),
                        )
                    }),
            )
            .child(
                v_flex()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().muted_foreground)
                            .child("Due date"),
                    )
                    .children(
                        [
                            (DueDateFilter::Overdue, "Overdue"),
                            (DueDateFilter::Today, "Today"),
                            (DueDateFilter::NextSevenDays, "Next 7 days"),
                            (DueDateFilter::NoDueDate, "No due date"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (filter, label))| {
                            let selected = self.filters.due_dates.contains(&filter);
                            let board_view = board_view.clone();

                            Checkbox::new(("filter-due-date", index))
                                .checked(selected)
                                .small()
                                .w_full()
                                .py_1()
                                .label(label)
                                .on_click(move |selected, _, cx| {
                                    board_view.update(cx, |this, cx| {
                                        this.set_due_date_filter(filter, *selected, cx);
                                    });
                                })
                        }),
                    ),
            )
            .child(self.render_custom_filter_controls(cx))
            .child(
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
                            .child("Related notes"),
                    )
                    .child(
                        Checkbox::new("filter-related-notes-present")
                            .checked(self.filters.related_notes == Some(true))
                            .small()
                            .w_full()
                            .label("Is not empty")
                            .on_click({
                                let board_view = board_view.clone();
                                move |selected, _, cx| {
                                    board_view.update(cx, |this, cx| {
                                        this.set_related_notes_filter(selected.then_some(true), cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        Checkbox::new("filter-related-notes-empty")
                            .checked(self.filters.related_notes == Some(false))
                            .small()
                            .w_full()
                            .label("Is empty")
                            .on_click({
                                let board_view = board_view.clone();
                                move |selected, _, cx| {
                                    board_view.update(cx, |this, cx| {
                                        this.set_related_notes_filter(
                                            selected.then_some(false),
                                            cx,
                                        );
                                    });
                                }
                            }),
                    ),
            )
            .child(
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
                            .child("Labels"),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .max_h_40()
                            .overflow_y_scrollbar()
                            .when(labels.is_empty(), |this| {
                                this.child(
                                    div()
                                        .py_1()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("No labels on this board"),
                                )
                            })
                            .children(labels.iter().map(|label| {
                                let label_id = label.id;
                                let selected = self.filters.label_ids.contains(&label_id);
                                let board_view = board_view.clone();

                                Checkbox::new(("filter-label", label_id as usize))
                                    .checked(selected)
                                    .small()
                                    .w_full()
                                    .py_1()
                                    .label(label.name.clone())
                                    .on_click(move |selected, _, cx| {
                                        board_view.update(cx, |this, cx| {
                                            this.set_label_filter(label_id, *selected, cx);
                                        });
                                    })
                            })),
                    ),
            )
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_t_1()
                    .border_color(cx.theme().border.opacity(0.72))
                    .bg(cx.theme().secondary.opacity(0.35))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Cards match at least one option in each selected section"),
            )
    }
}
