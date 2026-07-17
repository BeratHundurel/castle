use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, ElementExt as _, Icon, IconName, Selectable as _, Sizable as _,
    animation::ease_in_out_cubic,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    input::{Input, RopeExt as _},
    scroll::ScrollableElement,
    text::{TextView, TextViewStyle},
    v_flex,
};
use std::ops::Range;

use super::types::*;
use super::{DocumentEditorView, DocumentKind};
use crate::DB;
use crate::app_settings::AppSettings;

#[derive(Clone)]
struct OutlineResizeDrag {
    editor_id: EntityId,
}

impl Render for OutlineResizeDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Focusable for DocumentEditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl DocumentEditorView {
    pub(crate) fn render_source(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let source_is_ready = self.source_bounds.is_some();
        let outline_in_layout = self.outline_rendered && self.view_width >= px(760.);
        let outline_width = outline_width_for_view(self.outline_width, self.view_width);
        let source_width = self.view_width
            - if outline_in_layout {
                outline_width
            } else {
                px(0.)
            };
        let navigation_highlight = self.render_outline_source_highlight(source_width, cx);
        let source_context = if self.kind == DocumentKind::Markdown {
            "MarkdownSource"
        } else {
            "DocumentSource"
        };
        let input = Input::new(&self.editor)
            .h_full()
            .w_full()
            .p_0()
            .border_0()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(cx.theme().mono_font_size)
            .focus_bordered(false);

        let input = if outline_in_layout && self.outline_transition_epoch > 0 {
            let (from_width, to_width) = if self.outline_visible {
                (self.view_width, self.view_width - outline_width)
            } else {
                (self.view_width - outline_width, self.view_width)
            };
            let from_padding = source_horizontal_padding(from_width);
            let to_padding = source_horizontal_padding(to_width);

            input
                .with_animation(
                    (
                        "document-source-padding-transition",
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
            .id("document-source")
            .key_context(source_context)
            .capture_action(cx.listener(Self::on_action_paste))
            .size_full()
            .relative()
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
            .children(navigation_highlight)
    }

    fn render_outline_source_highlight(
        &self,
        source_width: Pixels,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let highlight = self.outline_source_highlight?;
        let source_bounds = self.source_bounds?;
        let editor = self.editor.read(cx);
        let row = editor.text().offset_to_point(highlight.source_offset).row;
        let line_range = editor.text().line_start_offset(row)..editor.text().line_end_offset(row);
        let line_bounds = editor.range_to_bounds(&line_range)?;
        let top = line_bounds.top() - source_bounds.top();
        if line_bounds.bottom() <= source_bounds.top() || top >= source_bounds.size.height {
            return None;
        }

        let horizontal_padding = source_horizontal_padding(source_width);
        Some(
            div()
                .id((
                    "outline-source-navigation-highlight",
                    highlight.generation as usize,
                ))
                .absolute()
                .top(top)
                .left(horizontal_padding)
                .right(horizontal_padding)
                .h(line_bounds.size.height)
                .rounded(cx.theme().radius)
                .border_l_1()
                .border_color(cx.theme().primary.opacity(0.9))
                .bg(cx.theme().primary.opacity(0.14))
                .with_animation(
                    (
                        "outline-source-navigation-highlight-fade",
                        highlight.generation as usize,
                    ),
                    Animation::new(super::OUTLINE_SOURCE_HIGHLIGHT_DURATION)
                        .with_easing(ease_in_out_cubic),
                    |this, delta| {
                        let fade = ((delta - 0.2) / 0.8).clamp(0., 1.);
                        this.opacity(1. - fade)
                    },
                )
                .into_any_element(),
        )
    }

    pub(crate) fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let font_size = px(AppSettings::markdown_preview_font_size(cx) as f32);
        let sections = if self.outline.markdown_sections().is_empty() {
            vec![self.editor.read(cx).value().to_string()]
        } else {
            self.outline.markdown_sections().to_vec()
        };
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
        let rows = self.outline_rows.clone();
        let kind = self.kind;
        let empty = rows.is_empty();
        let json_has_error = self.outline.json_has_error();
        let empty_message = if self.kind == DocumentKind::Json {
            "Add JSON properties or array items to navigate this document."
        } else {
            "Add headings to navigate this note."
        };
        let outline_width = outline_width_for_view(self.outline_width, self.view_width);

        v_flex()
            .id("document-outline")
            .key_context("DocumentOutline")
            .track_focus(&self.outline_focus_handle)
            .relative()
            .w(outline_width)
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar.opacity(0.72))
            .on_action(cx.listener(Self::on_action_outline_previous))
            .on_action(cx.listener(Self::on_action_outline_next))
            .on_action(cx.listener(Self::on_action_outline_left))
            .on_action(cx.listener(Self::on_action_outline_right))
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
                        h_flex()
                            .gap_2()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Outline")
                            .children(json_has_error.then(|| {
                                Icon::new(IconName::TriangleAlert)
                                    .xsmall()
                                    .text_color(cx.theme().warning)
                            })),
                    )
                    .child(
                        Button::new("close-document-outline")
                            .icon(IconName::Close)
                            .ghost()
                            .xsmall()
                            .tooltip("Hide outline (Ctrl+Shift+O)")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_outline(window, cx);
                            })),
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
                            .child(div().text_sm().child(empty_message)),
                    )
                },
                |this| {
                    this.child(
                        uniform_list("document-outline-rows", rows.len(), {
                            cx.processor(move |_this, visible_range: Range<usize>, _window, cx| {
                                visible_range
                                    .filter_map(|index| {
                                        let row = rows.get(index)?.clone();
                                        let is_selected = selected == Some(index);
                                        let chevron = row.has_children.then(|| {
                                            let icon = if row.expanded {
                                                IconName::ChevronDown
                                            } else {
                                                IconName::ChevronRight
                                            };
                                            div()
                                                .id(("outline-chevron", index))
                                                .size_4()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation()
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.toggle_outline_node(index, cx);
                                                }))
                                                .child(Icon::new(icon).xsmall())
                                        });

                                        Some(
                                            h_flex()
                                                .id(("outline-item", index))
                                                .w_full()
                                                .h_7()
                                                .px_2()
                                                .pl(outline_row_left_padding(row.depth))
                                                .gap_1()
                                                .rounded(cx.theme().radius)
                                                .text_size(px(13.))
                                                .font_weight(
                                                    if kind == DocumentKind::Markdown
                                                        && row.depth == 0
                                                    {
                                                        FontWeight::SEMIBOLD
                                                    } else {
                                                        FontWeight::NORMAL
                                                    },
                                                )
                                                .text_color(if row.disabled {
                                                    cx.theme().muted_foreground.opacity(0.65)
                                                } else if is_selected {
                                                    cx.theme().primary
                                                } else {
                                                    cx.theme().muted_foreground
                                                })
                                                .bg(if is_selected {
                                                    cx.theme().accent.opacity(0.55)
                                                } else {
                                                    cx.theme().sidebar.opacity(0.)
                                                })
                                                .when(!row.disabled, |element| {
                                                    element
                                                        .hover(|this| {
                                                            this.bg(cx.theme().accent.opacity(0.38))
                                                        })
                                                        .on_click(cx.listener(
                                                            move |this, _, window, cx| {
                                                                this.outline_focus_handle
                                                                    .focus(window, cx);
                                                                this.select_outline_item(
                                                                    index, window, cx,
                                                                );
                                                            },
                                                        ))
                                                })
                                                .children(chevron)
                                                .when(
                                                    reserves_disclosure_space(
                                                        kind,
                                                        row.has_children,
                                                    ),
                                                    |element| element.child(div().w_4()),
                                                )
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .overflow_hidden()
                                                        .text_ellipsis()
                                                        .child(row.title),
                                                ),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            })
                        })
                        .flex_1()
                        .min_h_0()
                        .p_2()
                        .track_scroll(&self.outline_scroll_handle)
                        .with_sizing_behavior(ListSizingBehavior::Auto),
                    )
                },
            )
            .child(self.render_outline_resize_handle(cx))
            .vertical_scrollbar(&self.outline_scroll_handle)
    }

    fn render_outline_resize_handle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let editor_id = cx.entity_id();
        let drag = OutlineResizeDrag { editor_id };

        div()
            .id("document-outline-resize-handle")
            .group("document-outline-resize-handle")
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(-4.))
            .w(px(8.))
            .flex()
            .justify_center()
            .cursor_col_resize()
            .child(
                div()
                    .h_full()
                    .w(px(1.))
                    .bg(cx.theme().border)
                    .group_hover("document-outline-resize-handle", |this| {
                        this.bg(cx.theme().drag_border)
                    }),
            )
            .on_drag_move(cx.listener(
                move |this, event: &DragMoveEvent<OutlineResizeDrag>, _, cx| {
                    if event.drag(cx).editor_id != cx.entity_id() {
                        return;
                    }
                    this.resize_outline_from_pointer(event.event.position.x, cx);
                },
            ))
            .on_drag(drag, |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
    }

    fn resize_outline_from_pointer(&mut self, pointer_x: Pixels, cx: &mut Context<Self>) {
        let Some(view_bounds) = self.view_bounds else {
            return;
        };
        let requested_width = view_bounds.right() - pointer_x;
        let width = outline_width_for_view(requested_width, view_bounds.size.width);
        if self.outline_width != width {
            self.outline_width = width;
            cx.notify();
        }
    }

    fn render_outline_transition(&self, cx: &mut Context<Self>) -> AnyElement {
        let outline_width = outline_width_for_view(self.outline_width, self.view_width);
        let wrapper = div()
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .child(self.render_outline(cx));

        if self.outline_transition_epoch == 0 {
            return wrapper.w(outline_width).into_any_element();
        }

        let (from_width, to_width) = if self.outline_visible {
            (px(0.), outline_width)
        } else {
            (outline_width, px(0.))
        };

        wrapper
            .with_animation(
                ("document-outline-transition", self.outline_transition_epoch),
                Animation::new(super::OUTLINE_TRANSITION_DURATION).with_easing(ease_in_out_cubic),
                move |this, delta| this.w(from_width + (to_width - from_width) * delta),
            )
            .into_any_element()
    }

    pub(crate) fn render_editor_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.is_loading {
            return div()
                .id("document-loading")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child("Loading document...")
                .into_any_element();
        }

        if let Some(error) = self.load_error.clone() {
            return div()
                .id("document-load-error")
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
                            .selected(self.outline_visible)
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

impl Render for DocumentEditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_background = cx.theme().background;
        let theme_border = cx.theme().border;
        let theme_input = cx.theme().input;
        let status_line_visible = AppSettings::editor_status_line_visible(cx);

        v_flex()
            .id("document-editor-window")
            .key_context("DocumentEditor")
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
                                if this.view_bounds != Some(bounds) {
                                    this.view_width = bounds.size.width;
                                    this.view_bounds = Some(bounds);
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
            .children(status_line_visible.then(|| self.render_status_bar(cx)))
            .children(
                (self.kind == DocumentKind::Markdown && self.show_emmet_input).then(|| {
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
                }),
            )
    }
}

fn save_state_status(
    save_state: &SaveState,
    cx: &mut Context<DocumentEditorView>,
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

fn outline_width_for_view(requested_width: Pixels, view_width: Pixels) -> Pixels {
    let available_width = (view_width - super::EDITOR_MIN_WIDTH_WITH_OUTLINE)
        .max(super::OUTLINE_MIN_WIDTH)
        .min(super::OUTLINE_MAX_WIDTH);
    requested_width.clamp(super::OUTLINE_MIN_WIDTH, available_width)
}

fn outline_row_left_padding(depth: usize) -> Pixels {
    px(8.) + super::OUTLINE_INDENT_STEP * depth as f32
}

fn markdown_preview_style(font_size: Pixels) -> TextViewStyle {
    TextViewStyle {
        heading_base_font_size: font_size,
        code_block: StyleRefinement::default().text_size(font_size),
        ..Default::default()
    }
}

fn reserves_disclosure_space(kind: DocumentKind, has_children: bool) -> bool {
    kind == DocumentKind::Json && !has_children
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::{
        DocumentKind, outline_row_left_padding, outline_width_for_view, reserves_disclosure_space,
    };

    #[test]
    fn markdown_rows_do_not_reserve_json_disclosure_space() {
        assert!(!reserves_disclosure_space(DocumentKind::Markdown, false));
        assert!(reserves_disclosure_space(DocumentKind::Json, false));
        assert!(!reserves_disclosure_space(DocumentKind::Json, true));
    }

    #[test]
    fn outline_width_respects_panel_and_editor_constraints() {
        assert_eq!(outline_width_for_view(px(120.), px(1_200.)), px(176.));
        assert_eq!(outline_width_for_view(px(900.), px(1_200.)), px(480.));
        assert_eq!(outline_width_for_view(px(480.), px(760.)), px(400.));
    }

    #[test]
    fn nested_outline_rows_use_compact_indentation() {
        assert_eq!(outline_row_left_padding(0), px(8.));
        assert_eq!(outline_row_left_padding(3), px(32.));
    }
}
