use super::*;

impl BoardView {
    pub(super) fn render_entry_attachments(
        &self,
        selected_entry: Option<(&str, &EntryDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let attachments = selected_entry
            .map(|(_, entry)| entry.attachments.clone())
            .unwrap_or_default();
        let can_attach = selected_entry
            .map(|(_, entry)| entry.id <= i32::MAX as u32)
            .unwrap_or(false);
        v_flex()
            .gap_3()
            .pt_3()
            .border_t_1()
            .border_color(cx.theme().border.opacity(0.48))
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
                            .child(Icon::new(IconName::Folder).xsmall())
                            .child(format!("Attachments · {}", attachments.len())),
                    )
                    .child(
                        Button::new("add-card-images")
                            .icon(IconName::Plus)
                            .label("Attach")
                            .ghost()
                            .small()
                            .disabled(!can_attach)
                            .tooltip(if can_attach {
                                "Choose one or more image files"
                            } else {
                                "Wait for this new card to finish saving"
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_image_attachments(window, cx);
                            })),
                    ),
            )
            .when_else(
                attachments.is_empty(),
                |this| {
                    this.child(
                        div()
                            .min_h(px(52.))
                            .flex()
                            .items_center()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Add screenshots or visual references to this card."),
                    )
                },
                |this| {
                    this.child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .items_start()
                            .gap_3()
                            .flex_wrap()
                            .children(attachments.into_iter().map(|attachment| {
                                let attachment_id = attachment.id;
                                let preview_path =
                                    self.attachment_preview_paths.get(&attachment_id).cloned();
                                v_flex()
                                    .w(px(252.))
                                    .overflow_hidden()
                                    .rounded(cx.theme().radius)
                                    .border_1()
                                    .border_color(cx.theme().border.opacity(0.6))
                                    .bg(cx.theme().background)
                                    .child(
                                        div()
                                            .relative()
                                            .w_full()
                                            .h(px(150.))
                                            .overflow_hidden()
                                            .when_some(preview_path, |this, path| {
                                                this.child(
                                                    img(path)
                                                        .size_full()
                                                        .object_fit(ObjectFit::Cover),
                                                )
                                            })
                                            .child(
                                                Button::new((
                                                    "delete-card-image",
                                                    attachment_id as usize,
                                                ))
                                                .icon(IconName::Delete)
                                                .ghost()
                                                .xsmall()
                                                .absolute()
                                                .top_2()
                                                .right_2()
                                                .bg(cx.theme().popover.opacity(0.9))
                                                .tooltip("Remove attachment")
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.delete_image_attachment(
                                                            attachment_id,
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                )),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .px_2()
                                            .py_1p5()
                                            .truncate()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(attachment.file_name),
                                    )
                            })),
                    )
                },
            )
    }
}
