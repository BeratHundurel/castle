use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::Input,
    menu::DropdownMenu as _,
    scroll::ScrollableElement as _,
    v_flex,
};

use super::BoardView;
use super::action::*;
use super::drag::*;
use super::dto::EntryDTO;

impl Render for BoardView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let board = div()
            .id("board-view")
            .relative()
            .size_full()
            .overflow_hidden()
            .on_action(cx.listener(Self::on_delete_card_action))
            .on_action(cx.listener(Self::on_edit_card_action))
            .on_action(cx.listener(Self::on_delete_entry_action));

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

        if let Some(board_id) = board_id_for_render {
            for card in &self.cards {
                cards.push(self.render_card(card, board_id, cx).into_any_element());
            }
        }

        h_flex()
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
            .into_any_element()
    }

    fn render_card(
        &self,
        card: &super::dto::CardDTO,
        board_id: u32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let card_id = card.id;
        let card_drag_info = CardDragInfo::new(card_id, board_id, card.title.clone());
        let mut entries = Vec::new();

        for entry in &card.entries {
            entries.push(
                self.render_entry_card(entry, board_id, card_id, cx)
                    .into_any_element(),
            );
        }

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
            .drag_over::<DragInfo>(|this, _, _, cx| {
                this.border_2()
                    .border_color(cx.theme().accent_foreground)
                    .bg(cx.theme().drop_target)
                    .shadow_md()
            })
            .on_drop(cx.listener(move |this, info: &DragInfo, _, cx| {
                this.move_entry(info, card_id, cx);
            }))
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
                    .tooltip("Card actions")
                    .dropdown_menu_with_anchor(Anchor::LeftCenter, move |menu, _, cx| {
                        let muted = cx.theme().muted_foreground;

                        menu.menu_element(Box::new(EditCardAction(card_id)), move |_, _| {
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child("Edit")
                                .child(Icon::new(IconName::Replace).xsmall().text_color(muted))
                        })
                        .menu_element(
                            Box::new(DeleteCardAction(card_id)),
                            move |_, _| {
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .items_center()
                                    .justify_between()
                                    .child("Delete")
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
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entry_id = entry.id;
        let drag_info = DragInfo::new(entry.id, board_id, card_id, entry.title.clone());

        div()
            .id(entry.id as usize)
            .p_2()
            .bg(cx.theme().primary)
            .text_color(cx.theme().primary_foreground)
            .rounded(cx.theme().radius)
            .hover(|this| this.bg(cx.theme().primary_hover))
            .drag_over::<DragInfo>(|this, _, _, cx| {
                this.border_l_4()
                    .border_color(cx.theme().accent_foreground)
                    .bg(cx.theme().primary_hover)
                    .shadow_lg()
            })
            .cursor_move()
            .text_sm()
            .w_full()
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .whitespace_normal()
                    .child(entry.title.clone()),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_entry_dialog(entry_id, window, cx);
            }))
            .on_drag(drag_info, |info: &DragInfo, position, _, cx| {
                cx.new(|_| info.clone().position(position))
            })
            .on_drop(cx.listener(move |this, info: &DragInfo, _, cx| {
                this.move_entry_before(info, card_id, entry_id, cx);
            }))
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
                this.is_adding_list = true;
                this.new_list_input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                    input.focus(window, cx);
                });
                cx.notify();
            }))
            .child(IconName::Plus)
            .child("Add another list")
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
            .items_center()
            .justify_center()
            .p_5()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close_entry_dialog(cx)),
            )
            .child(
                v_flex()
                    .id("entry-detail-panel")
                    .w_full()
                    .max_w(px(760.))
                    .min_h(px(420.))
                    .max_h(px(720.))
                    .mr(px(320.))
                    .overflow_hidden()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
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
            .gap_3()
            .p_4()
            .border_b_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::LayoutDashboard).xsmall())
                            .child(match selected_entry {
                                Some((card_title, _)) => SharedString::from(card_title.to_string()),
                                None => SharedString::from("Entry details"),
                            }),
                    )
                    .when_else(
                        self.entry_dialog.editing,
                        |this| {
                            this.child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(relative(1.25))
                                    .child("Edit entry"),
                            )
                        },
                        |this| {
                            this.child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(relative(1.25))
                                    .child(match selected_entry {
                                        Some((_, entry)) => entry.title.clone(),
                                        None => SharedString::from("Entry not found"),
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
                        .tooltip("Entry actions")
                        .dropdown_menu_with_anchor(Anchor::LeftCenter, |menu, _, cx| {
                            let danger = cx.theme().danger;

                            menu.menu_element(Box::new(DeleteEntryAction), move |_, _| {
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .items_center()
                                    .justify_between()
                                    .text_color(danger)
                                    .child("Delete")
                                    .child(Icon::new(IconName::Delete).xsmall())
                            })
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

        v_flex()
            .flex_1()
            .gap_4()
            .p_4()
            .overflow_y_scrollbar()
            .when_else(
                self.entry_dialog.editing,
                |this| this.child(self.render_entry_edit_form(cx)),
                |this| {
                    this.child(
                        v_flex()
                            .gap_3()
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
                                div()
                                    .text_sm()
                                    .line_height(relative(1.5))
                                    .whitespace_normal()
                                    .text_color(match selected_entry {
                                        Some((_, entry))
                                            if !entry.description.trim().is_empty() =>
                                        {
                                            theme.popover_foreground
                                        }
                                        _ => theme.muted_foreground,
                                    })
                                    .child(match selected_entry {
                                        Some((_, entry))
                                            if !entry.description.trim().is_empty() =>
                                        {
                                            entry.description.clone()
                                        }
                                        Some(_) => SharedString::from("No description"),
                                        None => {
                                            SharedString::from("This entry is no longer available.")
                                        }
                                    }),
                            ),
                    )
                },
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
