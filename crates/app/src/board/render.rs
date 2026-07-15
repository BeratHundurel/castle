use chrono::{Local, NaiveDate};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    checkbox::Checkbox,
    date_picker::DatePicker,
    h_flex,
    input::Input,
    menu::DropdownMenu as _,
    popover::Popover,
    scroll::ScrollableElement as _,
    v_flex,
};

use super::BoardView;
use super::action::*;
use super::drag::*;
use super::dto::EntryDTO;
use super::due_date::{DueDateStatus, due_date_status};
use super::filters::{DueDateFilter, matches_filters};

impl Render for BoardView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let board = div()
            .id("board-view")
            .relative()
            .size_full()
            .overflow_hidden()
            .on_action(cx.listener(Self::on_delete_card_action))
            .on_action(cx.listener(Self::on_edit_card_action))
            .on_action(cx.listener(Self::on_duplicate_card_action))
            .on_action(cx.listener(Self::on_delete_entry_action))
            .on_action(cx.listener(Self::on_duplicate_entry_action));

        let Some(board_id_for_render) = self.board_id else {
            return board.child(self.render_scrollable_board(None, cx));
        };

        board
            .child(self.render_scrollable_board(Some(board_id_for_render), cx))
            .when(self.is_entry_open && self.entry_dialog.open, |this| {
                this.child(self.render_entry_detail_overlay(cx))
            })
    }
}

impl BoardView {
    fn render_scrollable_board(
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

    fn render_filter_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
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

    fn render_filter_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
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

    fn render_card(
        &self,
        card: &super::dto::CardDTO,
        board_id: u32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let card_id = card.id;
        let card_drag_info =
            CardDragInfo::new(card_id, board_id, card.title.clone(), card.entries.len());
        let cards_are_filterable = self.filters.is_active();
        let mut entries = Vec::new();

        for entry in card
            .entries
            .iter()
            .filter(|entry| self.entry_matches_filters(entry))
        {
            entries.push(
                self.render_entry_card(entry, board_id, card_id, !cards_are_filterable, cx)
                    .into_any_element(),
            );
        }

        let has_matching_cards = !entries.is_empty();

        v_flex()
            .id(card.id as usize)
            .w_80()
            .min_w_auto()
            .max_h_3_4()
            .h_auto()
            .gap_2()
            .p_2()
            .bg(theme.secondary)
            .text_color(theme.secondary_foreground)
            .rounded(theme.radius)
            .when(!cards_are_filterable, |this| {
                this.drag_over::<DragInfo>(|this, _, _, cx| {
                    this.border_2()
                        .border_color(cx.theme().accent_foreground)
                        .bg(cx.theme().drop_target)
                        .shadow_md()
                })
                .on_drop(cx.listener(move |this, info: &DragInfo, _, cx| {
                    this.move_entry(info, card_id, cx);
                }))
            })
            .drag_over::<CardDragInfo>(|this, _, _, cx| {
                this.border_1()
                    .border_color(cx.theme().primary)
                    .bg(cx.theme().secondary_hover)
                    .shadow_lg()
            })
            .on_drop(cx.listener(move |this, info: &CardDragInfo, _, cx| {
                this.move_card(info, card_id, cx);
            }))
            .child(self.render_card_header(card, card_drag_info, cx))
            .children(entries)
            .when(cards_are_filterable && !has_matching_cards, |this| {
                this.child(
                    div()
                        .px_1()
                        .py_2()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("No matching cards"),
                )
            })
            .child(self.render_add_entry_button(card_id, cx))
    }

    fn render_card_header(
        &self,
        card: &super::dto::CardDTO,
        card_drag_info: CardDragInfo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let card_id = card.id;

        h_flex()
            .id("card-list-title")
            .p_1()
            .justify_between()
            .font_weight(FontWeight::MEDIUM)
            .cursor_move()
            .hover(|this| this.text_color(theme.foreground))
            .on_drag(card_drag_info, |info: &CardDragInfo, position, _, cx| {
                cx.new(|_| info.clone().position(position))
            })
            .when_else(
                self.renaming_card_id == Some(card_id),
                |this| {
                    this.child(
                        Input::new(&self.rename_card_input)
                            .bg(theme.secondary)
                            .focus_bordered(false)
                            .rounded_none()
                            .border_0()
                            .border_b_1()
                            .border_color(theme.foreground),
                    )
                },
                |this| this.child(card.title.clone()),
            )
            .child(
                Button::new(("card-menu", card_id as usize))
                    .icon(IconName::Ellipsis)
                    .ghost()
                    .compact()
                    .tooltip("List actions")
                    .dropdown_menu_with_anchor(Anchor::LeftCenter, move |menu, _, cx| {
                        let muted = cx.theme().muted_foreground;

                        menu.menu_element(Box::new(EditCardAction(card_id)), move |_, _| {
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child("Rename list")
                                .child(Icon::new(IconName::Replace).xsmall().text_color(muted))
                        })
                        .menu_element(Box::new(DuplicateCardAction(card_id)), move |_, _| {
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child("Duplicate list")
                                .child(Icon::new(IconName::Copy).xsmall().text_color(muted))
                        })
                        .menu_element(
                            Box::new(DeleteCardAction(card_id)),
                            move |_, _| {
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .items_center()
                                    .justify_between()
                                    .child("Delete list")
                                    .child(Icon::new(IconName::Delete).xsmall().text_color(muted))
                            },
                        )
                    }),
            )
    }

    fn render_entry_card(
        &self,
        entry: &EntryDTO,
        board_id: u32,
        card_id: u32,
        drag_enabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entry_id = entry.id;
        let drag_info = DragInfo::new(entry.id, board_id, card_id, entry.title.clone());

        div()
            .id(entry.id as usize)
            .px_3()
            .py_2p5()
            .bg(cx.theme().primary)
            .text_color(cx.theme().primary_foreground)
            .rounded(cx.theme().radius)
            .hover(|this| this.bg(cx.theme().primary_hover))
            .when(drag_enabled, |this| {
                this.drag_over::<DragInfo>(|this, _, _, cx| {
                    this.border_l_4()
                        .border_color(cx.theme().accent_foreground)
                        .bg(cx.theme().primary_hover)
                        .shadow_lg()
                })
                .cursor_move()
            })
            .text_sm()
            .w_full()
            .child(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1p5()
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .whitespace_normal()
                            .line_height(relative(1.3))
                            .font_weight(FontWeight::MEDIUM)
                            .child(entry.title.clone()),
                    )
                    .when(!entry.labels.is_empty(), |this| {
                        this.child(self.render_card_label_chips(entry, cx))
                    })
                    .when(
                        entry.due_on.is_some() || !entry.checklist_items.is_empty(),
                        |this| this.child(self.render_card_metadata(entry, cx)),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_entry_dialog(entry_id, window, cx);
            }))
            .when(drag_enabled, |this| {
                this.on_drag(drag_info, |info: &DragInfo, position, _, cx| {
                    cx.new(|_| info.clone().position(position))
                })
                .on_drop(cx.listener(move |this, info: &DragInfo, _, cx| {
                    this.move_entry_before(info, card_id, entry_id, cx);
                }))
            })
    }

    fn render_add_entry_button(&self, card_id: u32, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id(("add-item", card_id as usize))
            .w_full()
            .rounded(cx.theme().radius)
            .gap_2()
            .p_1()
            .text_color(cx.theme().secondary_foreground)
            .text_sm()
            .hover(|this| {
                this.bg(cx.theme().secondary_hover)
                    .text_color(cx.theme().accent_foreground)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.pending_card_id = Some(card_id);
                    this.show_add_entry_dialog(window, cx);
                }),
            )
            .font_weight(FontWeight::MEDIUM)
            .child(IconName::Plus)
            .child("Add a card")
    }

    fn render_add_list_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        h_flex()
            .id("add-list-button")
            .gap_2()
            .w_80()
            .p_2()
            .bg(theme.info.opacity(0.12))
            .text_color(theme.info)
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .border_1()
            .border_color(theme.info.opacity(0.24))
            .rounded(theme.radius)
            .hover(|this| this.bg(theme.info.opacity(0.18)))
            .drag_over::<CardDragInfo>(|this, _, _, cx| {
                this.bg(cx.theme().secondary_hover)
                    .border_color(cx.theme().primary)
                    .text_color(cx.theme().primary)
            })
            .on_drop(cx.listener(|this, info: &CardDragInfo, _, cx| {
                this.move_card_to_end(info, cx);
            }))
            .on_click(cx.listener(|this, _, window, cx| {
                this.start_adding_list(window, cx);
            }))
            .child(IconName::Plus)
            .child("Add another list")
    }

    fn render_empty_board(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        v_flex()
            .id("empty-board")
            .size_full()
            .items_center()
            .justify_center()
            .p_6()
            .pb(px(120.))
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(420.))
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .size_12()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(theme.radius_lg)
                            .bg(theme.info.opacity(0.12))
                            .text_color(theme.info)
                            .child(Icon::new(IconName::LayoutDashboard).large()),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Start with a list"),
                            )
                            .child(
                                div()
                                    .max_w(px(340.))
                                    .text_center()
                                    .text_sm()
                                    .line_height(relative(1.45))
                                    .text_color(theme.muted_foreground)
                                    .child("Lists organize cards into the stages of your work."),
                            ),
                    )
                    .child(
                        Button::new("add-first-list")
                            .icon(IconName::Plus)
                            .label("Add your first list")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_adding_list(window, cx);
                            })),
                    ),
            )
    }

    fn selected_entry(&self) -> Option<(&str, &EntryDTO)> {
        let entry_id = self.entry_dialog.entry_id?;

        self.cards.iter().find_map(|card| {
            card.entries
                .iter()
                .find(|entry| entry.id == entry_id)
                .map(|entry| (card.title.as_ref(), entry))
        })
    }

    fn entry_matches_filters(&self, entry: &EntryDTO) -> bool {
        matches_filters(
            entry.labels.iter().map(|label| label.id),
            entry.due_on.as_deref(),
            &self.filters,
            Local::now().date_naive(),
        )
    }

    fn label_color(&self, color: &str, cx: &Context<Self>) -> Hsla {
        match color {
            "green" => cx.theme().success,
            "amber" => cx.theme().warning,
            "red" => cx.theme().danger,
            "purple" => cx.theme().accent_foreground,
            "slate" => cx.theme().muted_foreground,
            _ => cx.theme().info,
        }
    }

    fn render_label_chip(
        &self,
        label: &super::dto::BoardLabelDTO,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let color = self.label_color(label.color.as_ref(), cx);

        h_flex()
            .flex_shrink_0()
            .min_w_0()
            .max_w(px(128.))
            .gap_1()
            .items_center()
            .rounded_full()
            .px_1p5()
            .py_0p5()
            .bg(color.opacity(0.18))
            .text_color(color)
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .child(div().size(px(6.)).rounded_full().bg(color))
            .child(div().truncate().child(label.name.clone()))
    }

    fn render_card_label_chips(&self, entry: &EntryDTO, cx: &Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .h_5()
            .min_w_0()
            .gap_1p5()
            .items_center()
            .flex_wrap()
            .overflow_hidden()
            .children(
                entry
                    .labels
                    .iter()
                    .map(|label| self.render_label_chip(label, cx)),
            )
    }

    fn render_card_metadata(&self, entry: &EntryDTO, cx: &Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .min_w_0()
            .h_5()
            .gap_3()
            .items_center()
            .when_some(entry.due_on.as_ref(), |this, due_on| {
                this.child(self.render_card_due_date(due_on, cx))
            })
            .when(!entry.checklist_items.is_empty(), |this| {
                this.child(self.render_card_checklist_progress(entry, cx))
            })
    }

    fn render_card_due_date(&self, due_on: &SharedString, cx: &Context<Self>) -> impl IntoElement {
        let status = due_date_status(due_on.as_ref(), Local::now().date_naive());
        let color = match status {
            DueDateStatus::Overdue => cx.theme().danger,
            DueDateStatus::Today => cx.theme().warning,
            DueDateStatus::Future | DueDateStatus::Invalid => {
                cx.theme().primary_foreground.opacity(0.76)
            }
        };
        let label = NaiveDate::parse_from_str(due_on.as_ref(), "%Y-%m-%d")
            .map(|date| date.format("%b %-d").to_string())
            .unwrap_or_else(|_| due_on.to_string());

        h_flex()
            .gap_1()
            .items_center()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(color)
            .child(Icon::new(IconName::Calendar).xsmall())
            .child(label)
    }

    fn render_card_checklist_progress(
        &self,
        entry: &EntryDTO,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let completed = entry
            .checklist_items
            .iter()
            .filter(|item| item.checked)
            .count();
        let is_complete = completed == entry.checklist_items.len();

        h_flex()
            .gap_1()
            .items_center()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(if is_complete {
                cx.theme().success
            } else {
                cx.theme().primary_foreground.opacity(0.76)
            })
            .child(Icon::new(IconName::CircleCheck).xsmall())
            .child(format!("{completed}/{}", entry.checklist_items.len()))
    }

    fn render_entry_detail_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let selected_entry = self.selected_entry();

        div()
            .id("entry-detail-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_stretch()
            .justify_end()
            .bg(theme.overlay.opacity(0.72))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close_entry_dialog(cx)),
            )
            .child(
                v_flex()
                    .id("entry-detail-panel")
                    .w(px(640.))
                    .min_w(px(420.))
                    .max_w(relative(0.94))
                    .h_full()
                    .overflow_hidden()
                    .rounded_none()
                    .border_l_1()
                    .border_color(theme.border.opacity(0.78))
                    .bg(theme.popover)
                    .text_color(theme.popover_foreground)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(self.render_entry_detail_header(selected_entry, cx))
                    .child(self.render_entry_detail_body(selected_entry, cx))
                    .when(self.entry_dialog.editing, |this| {
                        this.child(self.render_entry_detail_footer(cx))
                    }),
            )
    }

    fn render_entry_detail_header(
        &self,
        selected_entry: Option<(&str, &EntryDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();

        h_flex()
            .items_start()
            .gap_4()
            .px_5()
            .py_4()
            .border_b_1()
            .border_color(theme.border.opacity(0.74))
            .bg(theme.popover)
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::LayoutDashboard).xsmall())
                            .child(match selected_entry {
                                Some((card_title, _)) => SharedString::from(card_title.to_string()),
                                None => SharedString::from("Card details"),
                            }),
                    )
                    .when_else(
                        self.entry_dialog.editing,
                        |this| {
                            this.child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(relative(1.15))
                                    .child("Edit card"),
                            )
                        },
                        |this| {
                            this.child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(relative(1.15))
                                    .whitespace_normal()
                                    .child(match selected_entry {
                                        Some((_, entry)) => entry.title.clone(),
                                        None => SharedString::from("Card not found"),
                                    }),
                            )
                        },
                    ),
            )
            .child(self.render_entry_header_actions(cx))
    }

    fn render_entry_header_actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .items_center()
            .gap_1()
            .when(!self.entry_dialog.editing, |this| {
                this.child(
                    Button::new("edit-entry")
                        .icon(IconName::Replace)
                        .ghost()
                        .compact()
                        .tooltip("Edit")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.start_editing_entry(window, cx);
                        })),
                )
                .child(
                    Button::new("entry-actions")
                        .icon(IconName::Ellipsis)
                        .ghost()
                        .compact()
                        .tooltip("Card actions")
                        .dropdown_menu_with_anchor(Anchor::LeftCenter, |menu, _, cx| {
                            let danger = cx.theme().danger;

                            menu.menu_element(Box::new(DuplicateEntryAction), move |_, _| {
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .items_center()
                                    .justify_between()
                                    .child("Duplicate card")
                                    .child(Icon::new(IconName::Copy).xsmall())
                            })
                            .menu_element(
                                Box::new(DeleteEntryAction),
                                move |_, _| {
                                    h_flex()
                                        .w_full()
                                        .gap_2()
                                        .items_center()
                                        .justify_between()
                                        .text_color(danger)
                                        .child("Delete")
                                        .child(Icon::new(IconName::Delete).xsmall())
                                },
                            )
                        }),
                )
            })
            .child(
                Button::new("close-entry-detail")
                    .icon(IconName::Close)
                    .ghost()
                    .xsmall()
                    .tooltip("Close")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_entry_dialog(cx);
                    })),
            )
    }

    fn render_entry_detail_body(
        &self,
        selected_entry: Option<(&str, &EntryDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();

        if self.entry_dialog.editing {
            return v_flex()
                .flex_1()
                .gap_4()
                .p_5()
                .overflow_y_scrollbar()
                .child(self.render_entry_edit_form(cx));
        }

        v_flex()
            .flex_1()
            .gap_4()
            .p_5()
            .bg(theme.popover)
            .overflow_y_scrollbar()
            .child(
                v_flex()
                    .gap_4()
                    .child(self.render_entry_description(selected_entry, cx))
                    .child(
                        h_flex()
                            .items_start()
                            .gap_4()
                            .flex_wrap()
                            .child(
                                div()
                                    .min_w(px(260.))
                                    .flex_1()
                                    .child(self.render_entry_labels(selected_entry, cx)),
                            )
                            .child(
                                div()
                                    .min_w(px(260.))
                                    .flex_1()
                                    .child(self.render_entry_due_date(selected_entry, cx)),
                            ),
                    )
                    .child(self.render_entry_checklist(selected_entry, cx)),
            )
    }

    fn render_entry_due_date(
        &self,
        selected_entry: Option<(&str, &EntryDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let due_on = selected_entry
            .and_then(|(_, entry)| entry.due_on.as_deref())
            .filter(|due_on| !due_on.trim().is_empty());
        let (status_label, status_color) = match due_on {
            Some(due_on) => match due_date_status(due_on, Local::now().date_naive()) {
                DueDateStatus::Overdue => ("Overdue", cx.theme().danger),
                DueDateStatus::Today => ("Today", cx.theme().primary),
                DueDateStatus::Future => ("Scheduled", cx.theme().success),
                DueDateStatus::Invalid => ("Invalid", cx.theme().warning),
            },
            None => ("Unscheduled", cx.theme().muted_foreground),
        };

        v_flex()
            .gap_3()
            .min_h(px(132.))
            .p_3()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border.opacity(0.48))
            .bg(cx.theme().secondary.opacity(0.16))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().muted_foreground)
                            .child(Icon::new(IconName::Calendar).xsmall())
                            .child("Due date"),
                    )
                    .child(
                        div()
                            .rounded(px(3.))
                            .px_1p5()
                            .py(px(2.))
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .bg(status_color.opacity(0.14))
                            .text_color(status_color)
                            .child(status_label),
                    ),
            )
            .child(
                DatePicker::new(&self.due_date_picker)
                    .w_full()
                    .cleanable(true)
                    .placeholder("No due date")
                    .number_of_months(1),
            )
    }

    fn render_entry_checklist(
        &self,
        selected_entry: Option<(&str, &EntryDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let items = selected_entry
            .map(|(_, entry)| entry.checklist_items.clone())
            .unwrap_or_default();
        let board_view = cx.entity();
        let completed = items.iter().filter(|item| item.checked).count();
        let total = items.len();
        let progress = if total == 0 {
            0.0
        } else {
            completed as f32 / total as f32
        };

        v_flex()
            .gap_3()
            .p_3()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border.opacity(0.48))
            .bg(cx.theme().secondary.opacity(0.16))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().muted_foreground)
                            .child(Icon::new(IconName::CircleCheck).xsmall())
                            .child("Checklist"),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{completed}/{total}")),
                            )
                            .child(
                                Button::new("focus-checklist-input")
                                    .icon(IconName::Plus)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Add checklist item")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.focus_checklist_input(window, cx);
                                    })),
                            ),
                    ),
            )
            .when(total > 0, |this| {
                this.child(
                    div()
                        .h(px(5.))
                        .w_full()
                        .rounded_full()
                        .overflow_hidden()
                        .bg(cx.theme().secondary)
                        .child(div().h_full().w(relative(progress)).rounded_full().bg(
                            if completed == total {
                                cx.theme().success
                            } else {
                                cx.theme().primary
                            },
                        )),
                )
            })
            .when(total == 0, |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .min_h(px(52.))
                        .justify_center()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().popover_foreground)
                                .child("No checklist items"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .line_height(relative(1.35))
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    "Use the field below to turn this card into trackable steps.",
                                ),
                        ),
                )
            })
            .children(items.iter().enumerate().map(|(index, item)| {
                let item_id = item.id;
                let board_view = board_view.clone();

                h_flex()
                    .id(("checklist-item", item_id as usize))
                    .min_w_0()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded(cx.theme().radius * 0.5)
                    .overflow_hidden()
                    .bg(cx.theme().secondary.opacity(0.22))
                    .hover(|this| this.bg(cx.theme().secondary_hover))
                    .when(item.checked, |this| this.opacity(0.62))
                    .child(div().flex_1().min_w_0().overflow_hidden().when_else(
                        self.renaming_checklist_item_id == Some(item_id),
                        |this| {
                            this.child(
                                Input::new(&self.rename_checklist_item_input)
                                    .w_full()
                                    .min_w_0()
                                    .xsmall()
                                    .bg(cx.theme().input_background()),
                            )
                        },
                        |this| {
                            this.child(
                                Checkbox::new(("checklist-item-toggle", item_id as usize))
                                    .w_full()
                                    .min_w_0()
                                    .xsmall()
                                    .checked(item.checked)
                                    .tooltip(item.title.clone())
                                    .child(
                                        div()
                                            .w_full()
                                            .min_w_0()
                                            .truncate()
                                            .child(item.title.clone()),
                                    )
                                    .on_click(move |checked, _, cx| {
                                        board_view.update(cx, |this, cx| {
                                            this.set_checklist_item_checked(item_id, *checked, cx);
                                        });
                                    }),
                            )
                        },
                    ))
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_0p5()
                            .child(
                                Button::new(("rename-checklist-item", item_id as usize))
                                    .icon(IconName::Replace)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Rename checklist item")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.start_renaming_checklist_item(item_id, window, cx)
                                    })),
                            )
                            .when(total > 1, |this| {
                                this.child(
                                    Button::new(("move-checklist-item-up", item_id as usize))
                                        .icon(IconName::ArrowUp)
                                        .ghost()
                                        .xsmall()
                                        .disabled(index == 0)
                                        .tooltip("Move up")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.move_checklist_item(item_id, -1, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("move-checklist-item-down", item_id as usize))
                                        .icon(IconName::ArrowDown)
                                        .ghost()
                                        .xsmall()
                                        .disabled(index + 1 == total)
                                        .tooltip("Move down")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.move_checklist_item(item_id, 1, cx);
                                        })),
                                )
                            })
                            .child(
                                Button::new(("delete-checklist-item", item_id as usize))
                                    .icon(IconName::Delete)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Delete checklist item")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_checklist_item(item_id, cx);
                                    })),
                            ),
                    )
            }))
            .child(
                Input::new(&self.new_checklist_item_input)
                    .w_full()
                    .h_9()
                    .bg(cx.theme().input_background()),
            )
    }

    fn render_entry_description(
        &self,
        selected_entry: Option<(&str, &EntryDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let has_description =
            matches!(selected_entry, Some((_, entry)) if !entry.description.trim().is_empty());
        let description = match selected_entry {
            Some((_, entry)) if has_description => entry.description.clone(),
            Some(_) => SharedString::from(
                "Add context, acceptance criteria, or links so this card is clear later.",
            ),
            None => SharedString::from("This card is no longer available."),
        };

        v_flex()
            .gap_3()
            .p_3()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border.opacity(0.48))
            .bg(theme.secondary.opacity(0.16))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::BookOpen).xsmall())
                            .child("Description"),
                    )
                    .child(
                        Button::new("edit-entry-description")
                            .icon(IconName::Replace)
                            .label("Edit")
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_editing_entry(window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .min_h(px(72.))
                    .w_full()
                    .text_sm()
                    .line_height(relative(1.5))
                    .whitespace_normal()
                    .text_color(if has_description {
                        theme.popover_foreground
                    } else {
                        theme.muted_foreground
                    })
                    .child(description),
            )
    }

    fn render_entry_labels(
        &self,
        selected_entry: Option<(&str, &EntryDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entry_id = selected_entry.map(|(_, entry)| entry.id);
        let assigned_label_count = selected_entry
            .map(|(_, entry)| entry.labels.len())
            .unwrap_or_default();
        let header = h_flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().muted_foreground)
                    .child(Icon::new(IconName::Palette).xsmall())
                    .child("Labels"),
            )
            .when_else(
                self.entry_dialog.managing_labels,
                |this| {
                    this.child(
                        Button::new("done-managing-labels")
                            .label("Done")
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stop_managing_labels(cx);
                            })),
                    )
                },
                |this| {
                    this.child(
                        Button::new("manage-card-labels")
                            .icon(IconName::Palette)
                            .label("Manage")
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_managing_labels(window, cx);
                            })),
                    )
                },
            );

        if self.entry_dialog.managing_labels {
            return v_flex()
                .min_h(px(132.))
                .p_3()
                .gap_3()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border.opacity(0.48))
                .bg(cx.theme().secondary.opacity(0.16))
                .child(header)
                .child(self.render_label_manager(entry_id, cx));
        }

        v_flex()
            .min_h(px(132.))
            .p_3()
            .gap_3()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border.opacity(0.48))
            .bg(cx.theme().secondary.opacity(0.16))
            .child(header)
            .when_else(
                assigned_label_count > 0,
                |this| {
                    this.child(
                        h_flex().gap_2().flex_wrap().children(
                            selected_entry
                                .into_iter()
                                .flat_map(|(_, entry)| entry.labels.iter())
                                .map(|label| self.render_label_chip(label, cx)),
                        ),
                    )
                },
                |this| {
                    this.child(
                        v_flex()
                            .gap_1()
                            .min_h(px(52.))
                            .justify_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(cx.theme().popover_foreground)
                                    .child("No labels assigned"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .line_height(relative(1.35))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        "Add labels to make this card easier to scan on the board.",
                                    ),
                            ),
                    )
                },
            )
    }

    fn render_label_manager(
        &self,
        entry_id: Option<u32>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let labels = self.board_labels.clone();

        v_flex()
            .gap_3()
            .child(v_flex().gap_2().children(labels.iter().map(|label| {
                let label_id = label.id;
                let assigned = entry_id
                    .and_then(|entry_id| {
                        self.cards
                            .iter()
                            .flat_map(|list| list.entries.iter())
                            .find(|entry| entry.id == entry_id)
                            .map(|entry| {
                                entry
                                    .labels
                                    .iter()
                                    .any(|entry_label| entry_label.id == label_id)
                            })
                    })
                    .unwrap_or(false);
                let color = self.label_color(label.color.as_ref(), cx);
                let board_view = cx.entity();

                h_flex()
                    .id(("board-label", label_id as usize))
                    .min_w_0()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded(cx.theme().radius * 0.5)
                    .overflow_hidden()
                    .bg(cx.theme().secondary.opacity(0.2))
                    .hover(|this| this.bg(cx.theme().secondary_hover))
                    .child(div().size_2p5().flex_shrink_0().rounded(px(3.)).bg(color))
                    .child(div().flex_1().min_w_0().overflow_hidden().when_else(
                        self.renaming_label_id == Some(label_id),
                        |this| {
                            this.child(
                                Input::new(&self.rename_label_input)
                                    .w_full()
                                    .min_w_0()
                                    .xsmall()
                                    .bg(cx.theme().input_background()),
                            )
                        },
                        |this| {
                            this.child(
                                Checkbox::new((
                                    "toggle-card-label",
                                    ((entry_id.unwrap_or_default() as u64) << 32) | label_id as u64,
                                ))
                                .w_full()
                                .min_w_0()
                                .xsmall()
                                .checked(assigned)
                                .tooltip(label.name.clone())
                                .child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .truncate()
                                        .child(label.name.clone()),
                                )
                                .on_click(
                                    move |assigned, _, cx| {
                                        if let Some(entry_id) = entry_id {
                                            board_view.update(cx, |this, cx| {
                                                this.set_entry_label_assignment(
                                                    entry_id, label_id, *assigned, cx,
                                                );
                                            });
                                        }
                                    },
                                ),
                            )
                        },
                    ))
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_0p5()
                            .child(
                                Button::new(("rename-board-label", label_id as usize))
                                    .icon(IconName::Replace)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Rename label")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.start_renaming_board_label(label_id, window, cx);
                                    })),
                            )
                            .child(
                                Button::new(("delete-board-label", label_id as usize))
                                    .icon(IconName::Delete)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Delete label")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_board_label(label_id, cx);
                                    })),
                            ),
                    )
            })))
            .when(labels.is_empty(), |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .min_h(px(52.))
                        .justify_center()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().popover_foreground)
                                .child("No board labels"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .line_height(relative(1.35))
                                .text_color(cx.theme().muted_foreground)
                                .child("Create one below, then assign it to this card."),
                        ),
                )
            })
            .child(
                v_flex()
                    .gap_2()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border.opacity(0.48))
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().muted_foreground)
                            .child("Create label")
                            .child("Press Enter to save"),
                    )
                    .child(
                        Input::new(&self.new_label_input)
                            .w_full()
                            .small()
                            .prefix(
                                div()
                                    .size_2p5()
                                    .rounded(px(3.))
                                    .bg(self.label_color(self.selected_label_color.as_ref(), cx)),
                            )
                            .bg(cx.theme().input_background()),
                    )
                    .child(
                        h_flex().gap_1p5().flex_wrap().children(
                            [
                                ("blue", "Blue"),
                                ("green", "Green"),
                                ("amber", "Amber"),
                                ("red", "Red"),
                                ("purple", "Purple"),
                                ("slate", "Slate"),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(index, (key, label))| {
                                let color = self.label_color(key, cx);
                                let selected = self.selected_label_color.as_ref() == key;

                                Button::new(("label-color", index))
                                    .tooltip(label)
                                    .custom(
                                        ButtonCustomVariant::new(cx)
                                            .color(color.opacity(if selected {
                                                0.32
                                            } else {
                                                0.18
                                            }))
                                            .foreground(color)
                                            .hover(color.opacity(0.28))
                                            .active(color.opacity(0.36)),
                                    )
                                    .outline()
                                    .xsmall()
                                    .size_6()
                                    .selected(selected)
                                    .when_else(
                                        selected,
                                        |this| this.icon(IconName::Check),
                                        |this| this.child(div().size_3().rounded(px(3.)).bg(color)),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.select_label_color(key, cx);
                                    }))
                            }),
                        ),
                    ),
            )
    }

    fn render_entry_edit_form(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::Replace).xsmall())
                            .child("Title"),
                    )
                    .child(
                        Input::new(&self.entry_title_input)
                            .w_full()
                            .bg(theme.secondary)
                            .border_1()
                            .border_color(theme.border),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::BookOpen).xsmall())
                            .child("Description"),
                    )
                    .child(
                        Input::new(&self.entry_description_input)
                            .w_full()
                            .min_h(px(180.))
                            .bg(theme.secondary)
                            .border_1()
                            .border_color(theme.border),
                    ),
            )
    }

    fn render_entry_detail_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        h_flex()
            .items_center()
            .justify_end()
            .gap_2()
            .p_4()
            .border_t_1()
            .border_color(theme.border)
            .child(
                Button::new("cancel-entry-edit")
                    .icon(IconName::Close)
                    .label("Cancel")
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.cancel_editing_entry(cx);
                    })),
            )
            .child(
                Button::new("save-entry")
                    .icon(IconName::Check)
                    .label("Save")
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.update_selected_entry(cx);
                    })),
            )
    }
}
