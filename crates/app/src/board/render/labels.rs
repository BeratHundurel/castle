use super::*;

impl BoardView {
    pub(super) fn render_entry_labels(
        &self,
        selected_entry: Option<(&str, &BoardCardDTO)>,
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
                .border_color(cx.theme().border.opacity(0.4))
                .bg(cx.theme().secondary.opacity(0.1))
                .child(header)
                .child(self.render_label_manager(entry_id, cx));
        }

        v_flex()
            .min_h(px(132.))
            .p_3()
            .gap_3()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border.opacity(0.4))
            .bg(cx.theme().secondary.opacity(0.1))
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
                                    .child("No labels yet"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .line_height(relative(1.35))
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Add labels to scan this card faster."),
                            ),
                    )
                },
            )
    }

    pub(super) fn render_label_manager(
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
                let color = self.label_marker_color(label.color.as_ref(), cx);
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
                            .prefix(div().size_2p5().rounded(px(3.)).bg(
                                self.label_marker_color(self.selected_label_color.as_ref(), cx),
                            ))
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
                                let color = self.label_marker_color(key, cx);
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
}
