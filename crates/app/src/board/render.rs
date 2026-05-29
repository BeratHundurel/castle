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

impl Render for BoardView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        let Some(board_id_for_render) = self.board_id else {
            return h_flex()
                .id("scrollable-container")
                .size_full()
                .overflow_x_scrollbar()
                .gap_4()
                .p_4()
                .items_start();
        };

        h_flex()
            .id("scrollable-container")
            .on_action(cx.listener(Self::on_delete_card_action))
            .on_action(cx.listener(Self::on_edit_card_action))
            .size_full()
            .overflow_x_scrollbar()
            .gap_4()
            .p_4()
            .items_start()
            .children({
                self.cards.iter().map(|card| {
                    let card_id = card.id;
                    let board_id = board_id_for_render;
                    let card_drag_info = CardDragInfo::new(card_id, board_id, card.title.clone());

                    v_flex()
                        .id(card.id as usize)
                        .w_80()
                        .min_w_auto()
                        .max_h_3_4()
                        .h_auto()
                        .gap_2()
                        .p_2()
                        .bg(cx.theme().secondary)
                        .text_color(cx.theme().secondary_foreground)
                        .rounded(cx.theme().radius)
                        .drag_over::<DragInfo>(|this, _, _, cx| {
                            this.border_1().border_color(cx.theme().primary)
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
                        .child(
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
                                        .dropdown_menu_with_anchor(Anchor::LeftCenter, {
                                            move |menu, _, cx| {
                                                let muted = cx.theme().muted_foreground;
                                                menu.menu_element(
                                                    Box::new(EditCardAction(card_id)),
                                                    move |_window, _cx| {
                                                        h_flex()
                                                            .w_full()
                                                            .gap_2()
                                                            .items_center()
                                                            .justify_between()
                                                            .child("Edit")
                                                            .child(
                                                                Icon::new(IconName::Replace)
                                                                    .xsmall()
                                                                    .text_color(muted),
                                                            )
                                                    },
                                                )
                                                .menu_element(
                                                    Box::new(DeleteCardAction(card_id)),
                                                    move |_window, _cx| {
                                                        h_flex()
                                                            .w_full()
                                                            .gap_2()
                                                            .items_center()
                                                            .justify_between()
                                                            .child("Delete")
                                                            .child(
                                                                Icon::new(IconName::Delete)
                                                                    .xsmall()
                                                                    .text_color(muted),
                                                            )
                                                    },
                                                )
                                            }
                                        }),
                                ),
                        )
                        .children(card.entries.iter().map(|entry| {
                            let drag_info =
                                DragInfo::new(entry.id, board_id, card_id, entry.title.clone());

                            div()
                                .id(entry.id as usize)
                                .p_2()
                                .bg(cx.theme().primary)
                                .text_color(cx.theme().primary_foreground)
                                .rounded(cx.theme().radius)
                                .hover(|this| this.bg(cx.theme().primary_hover))
                                .cursor_move()
                                .text_sm()
                                .w_full()
                                .child(entry.title.clone())
                                .on_drag(drag_info, |info: &DragInfo, position, _, cx| {
                                    cx.new(|_| info.clone().position(position))
                                })
                        }))
                        .child(
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
                                .child("Add a card"),
                        )
                })
            })
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
                        .into_any_element()
                }
            })
    }
}
