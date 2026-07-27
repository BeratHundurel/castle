use super::*;

impl BoardView {
    pub(super) fn render_entry_description(
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
            .child(self.render_entry_attachments(selected_entry, cx))
    }
}
