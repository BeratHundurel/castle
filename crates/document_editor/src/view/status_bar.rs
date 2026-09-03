use super::*;

impl DocumentEditorView {
    pub(crate) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let path = status_path(self.persistence.current_path.as_deref(), self.kind);
        let path_tooltip = SharedString::from(path.tooltip.clone());

        h_flex()
            .id("document-status-bar")
            .w_full()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .text_color(cx.theme().muted_foreground)
            .text_xs()
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::File).xsmall())
                    .children(
                        (self.vim_is_enabled() && self.mode.shows_source())
                            .then(|| self.render_vim_mode_indicator(cx)),
                    )
                    .child(
                        h_flex()
                            .id("document-file-location")
                            .min_w_0()
                            .overflow_hidden()
                            .gap_1()
                            .tooltip(move |window, cx| {
                                Tooltip::new(path_tooltip.clone()).build(window, cx)
                            })
                            .children(path.directory.map(|directory| {
                                h_flex()
                                    .min_w_0()
                                    .gap_1()
                                    .text_color(cx.theme().muted_foreground.opacity(0.72))
                                    .child(div().max_w(px(160.)).truncate().child(directory))
                                    .child(Icon::new(IconName::ChevronRight).xsmall())
                            }))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(cx.theme().foreground.opacity(0.78))
                                    .child(path.file_name),
                            ),
                    )
                    .child(self.render_save_state(cx)),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .flex_shrink_0()
                    .child(
                        div()
                            .px_2()
                            .h_5()
                            .flex()
                            .items_center()
                            .rounded_full()
                            .bg(cx.theme().accent.opacity(0.35))
                            .child(self.kind.label()),
                    )
                    .children(
                        (self.kind == DocumentKind::Markdown)
                            .then(|| self.render_mode_switcher(cx)),
                    )
                    .children(self.kind.supports_outline().then(|| {
                        Button::new("toggle-document-outline")
                            .icon(IconName::PanelRight)
                            .ghost()
                            .xsmall()
                            .selected(self.analysis.outline_visible)
                            .tooltip("Toggle outline (Ctrl+Shift+O)")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_outline(window, cx);
                            }))
                    }))
                    .child(
                        div()
                            .h_4()
                            .border_l_1()
                            .border_color(cx.theme().border.opacity(0.72)),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(status_metric(
                                IconName::PanelBottom,
                                format!("{} lines", self.analysis.stats.lines),
                            ))
                            .child(status_metric(
                                IconName::BookOpen,
                                format!("{} words", self.analysis.stats.words),
                            ))
                            .child(status_metric(
                                IconName::File,
                                format!("{} chars", self.analysis.stats.characters),
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_mode_switcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.mode;

        h_flex()
            .id("document-mode-switcher")
            .items_center()
            .gap_1()
            .child(
                Button::new("mode-source")
                    .icon(IconName::File)
                    .ghost()
                    .xsmall()
                    .selected(mode == EditorMode::Source)
                    .tooltip("Write")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_mode(EditorMode::Source, window, cx);
                    })),
            )
            .child(
                Button::new("mode-split")
                    .icon(IconName::PanelRight)
                    .ghost()
                    .xsmall()
                    .selected(mode == EditorMode::Split)
                    .tooltip("Side by side")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_mode(EditorMode::Split, window, cx);
                    })),
            )
            .child(
                Button::new("mode-preview")
                    .icon(IconName::Eye)
                    .ghost()
                    .xsmall()
                    .selected(mode == EditorMode::Preview)
                    .tooltip("Read")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_mode(EditorMode::Preview, window, cx);
                    })),
            )
    }

    fn render_save_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (icon, color, label) = save_state_status(&self.persistence.save_state, cx);

        h_flex()
            .id("document-save-state")
            .items_center()
            .gap_1()
            .px_2()
            .h_5()
            .rounded_full()
            .bg(color.opacity(0.1))
            .text_color(color)
            .flex_shrink_0()
            .child(Icon::new(icon).xsmall())
            .child(label)
    }
}
