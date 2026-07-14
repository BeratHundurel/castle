use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, ElementExt as _, Icon, IconName, Selectable as _, Sizable as _,
    animation::ease_in_out_cubic,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    input::Input,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    text::{TextView, TextViewStyle},
    v_flex,
};

use super::MarkdownEditorView;
use super::types::*;
use crate::DB;
use crate::app_settings::AppSettings;

impl Focusable for MarkdownEditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl MarkdownEditorView {
    pub(crate) fn render_source(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let source_is_ready = self.source_bounds.is_some();
        let outline_in_layout = self.outline_rendered && self.view_width >= px(760.);
        let source_width = self.view_width - if outline_in_layout { px(224.) } else { px(0.) };
        let input = Input::new(&self.editor)
            .h_full()
            .w_full()
            .p_0()
            .border_0()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(cx.theme().mono_font_size)
            .focus_bordered(false);

        let input = if self.mode == EditorMode::Split {
            input.px_5().into_any_element()
        } else if outline_in_layout && self.outline_transition_epoch > 0 {
            let (from_width, to_width) = if self.outline_visible {
                (self.view_width, self.view_width - px(224.))
            } else {
                (self.view_width - px(224.), self.view_width)
            };
            let from_padding = source_horizontal_padding(from_width);
            let to_padding = source_horizontal_padding(to_width);

            input
                .with_animation(
                    (
                        "markdown-source-padding-transition",
                        self.outline_transition_epoch,
                    ),
                    Animation::new(super::OUTLINE_TRANSITION_DURATION)
                        .with_easing(ease_in_out_cubic),
                    move |this, delta| this.px(from_padding + (to_padding - from_padding) * delta),
                )
                .into_any_element()
        } else {
            input
                .px(source_horizontal_padding(source_width))
                .into_any_element()
        };

        div()
            .id("markdown-source")
            .key_context("MarkdownSource")
            .capture_action(cx.listener(Self::on_action_paste))
            .size_full()
            .opacity(if source_is_ready { 1. } else { 0. })
            .bg(cx.theme().background)
            .on_prepaint(move |bounds, _, cx| {
                view.update(cx, |this, cx| {
                    let first_layout = this.source_bounds.is_none();
                    this.source_bounds = Some(bounds);
                    if first_layout {
                        cx.notify();
                    }
                });
            })
            .child(div().size_full().min_w_0().py_4().child(input))
    }

    pub(crate) fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let font_size = px(AppSettings::markdown_preview_font_size(cx) as f32);
        let sections = self.outline.sections.clone();
        let local_image_plugin = super::attachments::LocalImagePlugin::new(
            cx.global::<DB>().data_dir.clone(),
            self.current_path.as_deref(),
        );

        div()
            .id("markdown-preview")
            .size_full()
            .bg(cx.theme().background)
            .child(
                div()
                    .id("markdown-preview-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.preview_scroll_handle)
                    .child(
                        div().w_full().flex().justify_center().child(
                            v_flex().w_full().max_w(px(920.)).min_w_0().p_6().children(
                                sections
                                    .into_iter()
                                    .enumerate()
                                    .map(move |(index, section)| {
                                        TextView::markdown(
                                            ("markdown-preview-section", index),
                                            section,
                                        )
                                        .plugin(local_image_plugin.clone())
                                        .style(markdown_preview_style(font_size))
                                        .code_block_actions(|code_block, _window, _cx| {
                                            Clipboard::new("copy-code")
                                                .value(code_block.code().clone())
                                        })
                                        .text_size(font_size)
                                        .scrollable(false)
                                        .selectable(true)
                                    }),
                            ),
                        ),
                    ),
            )
    }

    fn render_outline(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.outline_selected;
        let items = self.outline.items.clone();
        let empty = items.is_empty();

        v_flex()
            .id("markdown-outline")
            .key_context("MarkdownOutline")
            .track_focus(&self.outline_focus_handle)
            .w(px(224.))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar.opacity(0.72))
            .on_action(cx.listener(Self::on_action_outline_previous))
            .on_action(cx.listener(Self::on_action_outline_next))
            .on_action(cx.listener(Self::on_action_outline_open))
            .on_action(cx.listener(Self::on_action_outline_close))
            .child(
                h_flex()
                    .h_10()
                    .px_3()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.7))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Outline"),
                    )
                    .child(
                        Button::new("close-markdown-outline")
                            .icon(IconName::Close)
                            .ghost()
                            .xsmall()
                            .tooltip("Hide outline (Ctrl+Shift+O)")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_outline(cx))),
                    ),
            )
            .when_else(
                empty,
                |this| {
                    this.child(
                        v_flex()
                            .p_4()
                            .gap_2()
                            .text_color(cx.theme().muted_foreground)
                            .child(Icon::new(IconName::PanelRight).small())
                            .child(div().text_sm().child("Add headings to navigate this note.")),
                    )
                },
                |this| {
                    this.child(
                        v_flex()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .p_2()
                            .children(items.into_iter().enumerate().map(|(index, item)| {
                                let is_selected = selected == Some(index);
                                div()
                                    .id(("outline-item", index))
                                    .w_full()
                                    .px_2()
                                    .py_1()
                                    .pl(px(8. + f32::from(item.level.saturating_sub(1)) * 12.))
                                    .rounded(cx.theme().radius)
                                    .text_size(px(13.))
                                    .font_weight(if item.level == 1 {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .text_color(if is_selected {
                                        cx.theme().primary
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .bg(if is_selected {
                                        cx.theme().accent.opacity(0.55)
                                    } else {
                                        cx.theme().sidebar.opacity(0.)
                                    })
                                    .hover(|this| this.bg(cx.theme().accent.opacity(0.38)))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_outline_item(index, window, cx);
                                    }))
                                    .child(item.title)
                            })),
                    )
                },
            )
    }

    fn render_outline_transition(&self, cx: &mut Context<Self>) -> AnyElement {
        let wrapper = div()
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .child(self.render_outline(cx));

        if self.outline_transition_epoch == 0 {
            return wrapper.w(px(224.)).into_any_element();
        }

        let (from_width, to_width) = if self.outline_visible {
            (px(0.), px(224.))
        } else {
            (px(224.), px(0.))
        };

        wrapper
            .with_animation(
                ("markdown-outline-transition", self.outline_transition_epoch),
                Animation::new(super::OUTLINE_TRANSITION_DURATION).with_easing(ease_in_out_cubic),
                move |this, delta| this.w(from_width + (to_width - from_width) * delta),
            )
            .into_any_element()
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
                        Button::new("toggle-markdown-outline")
                            .icon(IconName::PanelRight)
                            .ghost()
                            .xsmall()
                            .selected(self.outline_visible)
                            .tooltip("Toggle outline (Ctrl+Shift+O)")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_outline(cx))),
                    )
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
            .on_action(cx.listener(Self::on_action_toggle_outline))
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
                    .on_prepaint({
                        let view = cx.entity();
                        move |bounds, _, cx| {
                            view.update(cx, |this, cx| {
                                if this.view_width != bounds.size.width {
                                    this.view_width = bounds.size.width;
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .child(
                        h_flex()
                            .size_full()
                            .min_w_0()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .child(self.render_editor_body(cx)),
                            )
                            .children(
                                (self.outline_rendered && self.view_width >= px(760.))
                                    .then(|| self.render_outline_transition(cx)),
                            ),
                    ),
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

fn source_horizontal_padding(source_width: Pixels) -> Pixels {
    const EDITOR_MAX_WIDTH: f32 = 920.;
    const EDITOR_GUTTER: f32 = 20.;

    px(((source_width.as_f32() - EDITOR_MAX_WIDTH) / 2. + EDITOR_GUTTER).max(EDITOR_GUTTER))
}

fn markdown_preview_style(font_size: Pixels) -> TextViewStyle {
    TextViewStyle {
        heading_base_font_size: font_size,
        code_block: StyleRefinement::default().text_size(font_size),
        ..Default::default()
    }
}
