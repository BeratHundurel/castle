use super::*;

impl BoardView {
    pub(super) fn render_entry_edit_form(&self, cx: &mut Context<Self>) -> impl IntoElement {
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

    pub(super) fn render_entry_detail_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        h_flex()
            .flex_shrink_0()
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
