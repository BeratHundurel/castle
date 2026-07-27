use super::*;

impl BoardView {
    pub(super) fn render_entry_checklist(
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
}
