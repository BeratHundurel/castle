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
                .child(self.render_filter_toolbar(cx))
                .child(scrollable)
                .into_any_element()
        } else {
            scrollable
        }
    }

    pub(super) fn render_filter_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_count = self.filters.count();

        h_flex()
            .id("board-filter-toolbar")
            .min_h_9()
            .px_4()
            .gap_2()
            .justify_end()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.72))
            .bg(cx.theme().background)
            .when(self.filters.is_active(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Dragging is paused while filtering"),
                )
            })
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
                            .label(if active_count == 0 {
                                "Filter".to_string()
                            } else {
                                format!("Filter · {active_count}")
                            })
                            .outline()
                            .small()
                            .selected(self.filters.is_active())
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
    }

    pub(super) fn render_filter_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let labels = self.board_labels.clone();
        let board_view = cx.entity();

        v_flex()
            .id("board-filter-panel")
            .w_full()
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
