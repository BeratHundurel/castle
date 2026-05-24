use gpui::*;
use gpui_component::{
    ActiveTheme as _, IconName, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    input::Input,
    resizable::{h_resizable, resizable_panel},
    text::TextView,
    v_flex,
};

use super::MarkdownEditorView;
use super::action::*;
use super::types::*;

impl Focusable for MarkdownEditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
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
            .child(self.render_toolbar(cx))
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

impl MarkdownEditorView {
    pub(crate) fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.mode;
        let save_state = self.save_state.clone();

        h_flex()
            .id("markdown-toolbar")
            .w_full()
            .gap_2()
            .items_center()
            .justify_between()
            .py_2()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("save-note")
                            .icon(IconName::Check)
                            .ghost()
                            .small()
                            .tooltip("Save (Ctrl+S)")
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    )
                    .child(
                        Button::new("save-note-as")
                            .label("Save as")
                            .ghost()
                            .small()
                            .tooltip("Save as (Ctrl+Shift+S)")
                            .on_click(cx.listener(|this, _, window, cx| this.save_as(window, cx))),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(format_button("h1", "H1", MarkdownFormat::HeadingOne))
                    .child(format_button("h2", "H2", MarkdownFormat::HeadingTwo))
                    .child(format_button("h3", "H3", MarkdownFormat::HeadingThree))
                    .child(format_button("bold", "B", MarkdownFormat::Bold))
                    .child(format_button("italic", "I", MarkdownFormat::Italic))
                    .child(format_button("code", "Code", MarkdownFormat::InlineCode))
                    .child(format_button("link", "Link", MarkdownFormat::Link))
                    .child(format_button(
                        "bullet",
                        "- List",
                        MarkdownFormat::BulletList,
                    ))
                    .child(format_button(
                        "ordered",
                        "1. List",
                        MarkdownFormat::OrderedList,
                    ))
                    .child(format_button("quote", "Quote", MarkdownFormat::Quote))
                    .child(format_button(
                        "code-block",
                        "Block",
                        MarkdownFormat::CodeBlock,
                    )),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("mode-split")
                            .label("Split")
                            .ghost()
                            .small()
                            .selected(mode == EditorMode::Split)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_mode(EditorMode::Split, cx);
                            })),
                    )
                    .child(
                        Button::new("mode-source")
                            .label("Source")
                            .ghost()
                            .small()
                            .selected(mode == EditorMode::Source)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_mode(EditorMode::Source, cx);
                            })),
                    )
                    .child(
                        Button::new("mode-preview")
                            .label("Preview")
                            .ghost()
                            .small()
                            .selected(mode == EditorMode::Preview)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_mode(EditorMode::Preview, cx);
                            })),
                    )
                    .child(status_badge(save_state, cx)),
            )
    }

    pub(crate) fn render_source(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("markdown-source")
            .size_full()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(cx.theme().mono_font_size)
            .bg(cx.theme().background)
            .child(
                Input::new(&self.editor)
                    .h_full()
                    .p_0()
                    .border_0()
                    .focus_bordered(false),
            )
    }

    pub(crate) fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("markdown-preview")
            .size_full()
            .bg(cx.theme().background)
            .child(
                TextView::new(&self.preview)
                    .code_block_actions(|code_block, _window, _cx| {
                        Clipboard::new("copy-code").value(code_block.code().clone())
                    })
                    .p_5()
                    .scrollable(true)
                    .selectable(true),
            )
    }

    pub(crate) fn render_editor_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
            .gap_3()
            .px_3()
            .py_1p5()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .text_color(cx.theme().muted_foreground)
            .text_xs()
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(path)),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(format!("{} lines", self.stats.lines))
                    .child(format!("{} words", self.stats.words))
                    .child(format!("{} chars", self.stats.characters))
                    .child("Ctrl+E toggles mode"),
            )
    }
}

fn format_button(id: &'static str, label: &'static str, format: MarkdownFormat) -> Button {
    Button::new(format!("format-{id}"))
        .label(label)
        .ghost()
        .small()
        .on_click(move |_, window, cx| {
            window.dispatch_action(Box::new(ApplyMarkdownFormat(format)), cx);
        })
}

fn status_badge(save_state: SaveState, cx: &mut Context<MarkdownEditorView>) -> impl IntoElement {
    match save_state {
        SaveState::Saved => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().success)
            .child(IconName::CircleCheck)
            .child("Saved")
            .into_any_element(),
        SaveState::Dirty => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().warning)
            .child(IconName::Asterisk)
            .child("Unsaved")
            .into_any_element(),
        SaveState::Saving => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().info)
            .child(IconName::Loader)
            .child("Saving")
            .into_any_element(),
        SaveState::Missing => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().warning)
            .child(IconName::TriangleAlert)
            .child("File missing")
            .into_any_element(),
        SaveState::Error(err) => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().danger)
            .child(IconName::TriangleAlert)
            .child(err)
            .into_any_element(),
    }
}
