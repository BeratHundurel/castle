use gpui::*;
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    input::Input,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    text::TextView,
    v_flex,
};

use super::MarkdownEditorView;
use super::types::*;

impl Focusable for MarkdownEditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl MarkdownEditorView {
    pub(crate) fn render_source(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("markdown-source")
            .size_full()
            .flex()
            .justify_center()
            .bg(cx.theme().background)
            .child(
                div()
                    .size_full()
                    .max_w(px(920.))
                    .min_w_0()
                    .px_5()
                    .py_4()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(cx.theme().mono_font_size)
                    .child(
                        Input::new(&self.editor)
                            .h_full()
                            .w_full()
                            .p_0()
                            .border_0()
                            .focus_bordered(false),
                    ),
            )
    }

    pub(crate) fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("markdown-preview")
            .size_full()
            .bg(cx.theme().background)
            .child(
                div().size_full().overflow_y_scrollbar().child(
                    div().w_full().flex().justify_center().child(
                        div().w_full().max_w(px(920.)).min_w_0().child(
                            TextView::new(&self.preview)
                                .code_block_actions(|code_block, _window, _cx| {
                                    Clipboard::new("copy-code").value(code_block.code().clone())
                                })
                                .p_6()
                                .scrollable(false)
                                .selectable(true),
                        ),
                    ),
                ),
            )
    }

    pub(crate) fn render_editor_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.is_loading {
            return div()
                .id("markdown-loading")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child("Loading note...")
                .into_any_element();
        }

        if let Some(error) = self.load_error.clone() {
            return div()
                .id("markdown-load-error")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .text_color(cx.theme().danger)
                .child(error)
                .into_any_element();
        }

        match self.mode {
            EditorMode::Split => div()
                .size_full()
                .child(
                    h_resizable("markdown-editor-split")
                        .child(resizable_panel().child(self.render_source(cx)))
                        .child(resizable_panel().child(self.render_preview(cx))),
                )
                .into_any_element(),
            EditorMode::Source => self.render_source(cx).into_any_element(),
            EditorMode::Preview => self.render_preview(cx).into_any_element(),
        }
    }

    pub(crate) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let path = self
            .current_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Not saved to a file yet".to_string());

        h_flex()
            .id("markdown-status-bar")
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
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(path)),
                    )
                    .child(self.render_save_state(cx)),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .flex_shrink_0()
                    .child(self.render_mode_switcher(cx))
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
                                format!("{} lines", self.stats.lines),
                            ))
                            .child(status_metric(
                                IconName::BookOpen,
                                format!("{} words", self.stats.words),
                            ))
                            .child(status_metric(
                                IconName::File,
                                format!("{} chars", self.stats.characters),
                            )),
                    ),
            )
    }

    fn render_mode_switcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.mode;

        h_flex()
            .id("markdown-mode-switcher")
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
                    .tooltip("Split")
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
        let (icon, color, label) = save_state_status(&self.save_state, cx);

        h_flex()
            .id("markdown-save-state")
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

impl Render for MarkdownEditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_background = cx.theme().background;
        let theme_border = cx.theme().border;
        let theme_input = cx.theme().input;

        v_flex()
            .id("markdown-editor-window")
            .key_context("MarkdownEditor")
            .track_focus(&self.focus_handle)
            .size_full()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .on_action(cx.listener(Self::on_action_save))
            .on_action(cx.listener(Self::on_action_save_as))
            .on_action(cx.listener(Self::on_action_toggle_mode))
            .on_action(cx.listener(Self::on_action_expand_emmet))
            .on_action(cx.listener(Self::on_action_emmet_submit_wrap))
            .on_action(cx.listener(Self::on_action_emmet_cancel_wrap))
            .on_action(cx.listener(Self::apply_format))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .w_full()
                    .child(self.render_editor_body(cx)),
            )
            .child(self.render_status_bar(cx))
            .children(self.show_emmet_input.then(|| {
                div()
                    .key_context("EmmetInput")
                    .absolute()
                    .top(px(60.))
                    .left(px(20.))
                    .w(px(300.))
                    .p_2()
                    .bg(theme_background)
                    .border_1()
                    .border_color(theme_border)
                    .rounded_md()
                    .shadow_sm()
                    .child(
                        Input::new(&self.emmet_input)
                            .w_full()
                            .bg(theme_input)
                            .px_2()
                            .py_1()
                            .rounded_sm(),
                    )
            }))
    }
}

fn save_state_status(
    save_state: &SaveState,
    cx: &mut Context<MarkdownEditorView>,
) -> (IconName, Hsla, SharedString) {
    match save_state {
        SaveState::Saved => (IconName::CircleCheck, cx.theme().success, "Saved".into()),
        SaveState::Dirty => (IconName::Asterisk, cx.theme().warning, "Unsaved".into()),
        SaveState::Saving => (IconName::Loader, cx.theme().info, "Saving".into()),
        SaveState::Missing => (
            IconName::TriangleAlert,
            cx.theme().warning,
            "File missing".into(),
        ),
        SaveState::Error(_) => (
            IconName::TriangleAlert,
            cx.theme().danger,
            "Save failed".into(),
        ),
    }
}

fn status_metric(icon: IconName, label: String) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_1()
        .child(Icon::new(icon).xsmall())
        .child(label)
}
