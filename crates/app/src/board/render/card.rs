use super::*;

fn accepts_entry_card_drop(value: &dyn std::any::Any) -> bool {
    value.is::<DragInfo>()
        || value
            .downcast_ref::<SidebarDragInfo>()
            .and_then(SidebarDragInfo::note_id)
            .is_some()
}

fn accepts_list_header_drop(value: &dyn std::any::Any) -> bool {
    value.is::<CardDragInfo>()
        || value
            .downcast_ref::<SidebarDragInfo>()
            .and_then(SidebarDragInfo::note_id)
            .is_some()
}

impl BoardView {
    pub(super) fn render_card(
        &self,
        card: &crate::board::dto::BoardListDTO,
        board_id: u32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let card_id = card.id;
        let card_drag_info =
            CardDragInfo::new(card_id, board_id, card.title.clone(), card.entries.len());
        let cards_are_filterable =
            self.filters.is_active() || self.active_view_config.sort.is_some();
        let mut entries = Vec::new();
        let mut matching_entries = card
            .entries
            .iter()
            .filter(|entry| self.entry_matches_filters(entry))
            .collect::<Vec<_>>();
        if self.active_view_config.sort.is_some() {
            matching_entries
                .sort_by(|left, right| self.compare_entries_for_active_sort(left, right));
        }

        for entry in matching_entries {
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
            .when(self.revealed_list_id == Some(card_id), |this| {
                this.border_2().border_color(theme.primary).shadow_md()
            })
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

    pub(super) fn render_card_header(
        &self,
        card: &crate::board::dto::BoardListDTO,
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
            .can_drop(|value, _, _| accepts_list_header_drop(value))
            .drag_over::<SidebarDragInfo>(|this, _, _, cx| {
                this.rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().primary)
                    .bg(cx.theme().drop_target)
            })
            .on_drop(cx.listener(move |this, info: &SidebarDragInfo, _, cx| {
                if let Some(note_id) = info.note_id() {
                    this.link_note_to_item(
                        storage::workspace_links::WorkspaceItemRef {
                            kind: storage::workspace_links::WorkspaceItemKind::List,
                            id: i64::from(card_id),
                        },
                        note_id,
                        cx,
                    );
                }
            }))
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
                h_flex()
                    .gap_0p5()
                    .child(self.render_related_notes_popover(
                        storage::workspace_links::WorkspaceItemRef {
                            kind: storage::workspace_links::WorkspaceItemKind::List,
                            id: i64::from(card_id),
                        },
                        SharedString::from(format!("list-{card_id}")),
                        cx,
                    ))
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
                                        .child(
                                            Icon::new(IconName::Replace).xsmall().text_color(muted),
                                        )
                                })
                                .menu_element(
                                    Box::new(DuplicateCardAction(card_id)),
                                    move |_, _| {
                                        h_flex()
                                            .w_full()
                                            .gap_2()
                                            .items_center()
                                            .justify_between()
                                            .child("Duplicate list")
                                            .child(
                                                Icon::new(IconName::Copy)
                                                    .xsmall()
                                                    .text_color(muted),
                                            )
                                    },
                                )
                                .menu_element(
                                    Box::new(CopyListInternalLinkAction(card_id)),
                                    move |_, _| {
                                        h_flex()
                                            .w_full()
                                            .gap_2()
                                            .items_center()
                                            .justify_between()
                                            .child("Copy internal link")
                                            .child(
                                                Icon::new(IconName::Copy)
                                                    .xsmall()
                                                    .text_color(muted),
                                            )
                                    },
                                )
                                .menu_element(
                                    Box::new(DeleteCardAction(card_id)),
                                    move |_, _| {
                                        h_flex()
                                            .w_full()
                                            .gap_2()
                                            .items_center()
                                            .justify_between()
                                            .child("Delete list")
                                            .child(
                                                Icon::new(IconName::Delete)
                                                    .xsmall()
                                                    .text_color(muted),
                                            )
                                    },
                                )
                            }),
                    ),
            )
    }

    pub(super) fn render_entry_card(
        &self,
        entry: &BoardCardDTO,
        board_id: u32,
        card_id: u32,
        drag_enabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entry_id = entry.id;
        let drag_info = DragInfo::new(entry.id, board_id, card_id, entry.title.clone());
        let show_labels = self
            .active_view_config
            .visible_properties
            .contains(&storage::board_properties::PropertyKey::Labels);
        let show_due_date = self
            .active_view_config
            .visible_properties
            .contains(&storage::board_properties::PropertyKey::DueDate);
        let compact = self.active_view_config.compact_cards;

        div()
            .id(entry.id as usize)
            .debug_selector(move || format!("board-entry-{entry_id}"))
            .can_drop(|value, _, _| accepts_entry_card_drop(value))
            .drag_over::<SidebarDragInfo>(|this, _, _, cx| {
                this.border_2()
                    .border_color(cx.theme().primary)
                    .bg(cx.theme().drop_target)
            })
            .on_drop(cx.listener(move |this, info: &SidebarDragInfo, _, cx| {
                if let Some(note_id) = info.note_id() {
                    this.link_note_to_item(
                        storage::workspace_links::WorkspaceItemRef {
                            kind: storage::workspace_links::WorkspaceItemKind::Card,
                            id: i64::from(entry_id),
                        },
                        note_id,
                        cx,
                    );
                }
            }))
            .when_else(
                compact,
                |this| this.px_2().py_1p5(),
                |this| this.px_3().py_2p5(),
            )
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
                    .gap(if compact { px(4.) } else { px(6.) })
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .whitespace_normal()
                            .line_height(relative(1.3))
                            .font_weight(FontWeight::NORMAL)
                            .child(entry.title.clone()),
                    )
                    .when(show_labels && !entry.labels.is_empty(), |this| {
                        this.child(self.render_card_label_chips(entry, cx))
                    })
                    .child(self.render_card_property_values(entry, cx))
                    .when(
                        (show_due_date && entry.due_on.is_some())
                            || !entry.checklist_items.is_empty()
                            || !entry.attachments.is_empty(),
                        |this| this.child(self.render_card_metadata(entry, show_due_date, cx)),
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

    pub(super) fn render_add_entry_button(
        &self,
        card_id: u32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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

    pub(super) fn render_add_list_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
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

    pub(super) fn render_empty_board(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
}
