use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, ElementExt as _, Icon, IconName, Selectable as _,
    Sizable as _,
    animation::ease_in_out_cubic,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    input::{self, Input, InputState, RopeExt as _},
    scroll::ScrollableElement,
    text::{TextView, TextViewStyle},
    tooltip::Tooltip,
    v_flex,
};
use std::{collections::HashSet, ops::Range, path::Path};

use super::action::FormatDocument;
use super::types::*;
use super::vim::VimMode;
use super::{DocumentEditorView, DocumentInspectorTab, DocumentKind};
use crate::AppServices;
use crate::app_settings::AppSettings;

#[derive(Clone)]
struct OutlineResizeDrag {
    editor_id: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkdownPreviewVirtualization {
    Blocks,
    Sections,
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
        let source_is_ready = self.analysis.source_bounds.is_some();
        let outline_in_layout = self.analysis.outline_rendered && self.view_width >= px(760.);
        let outline_width = outline_width_for_view(self.outline_width, self.view_width);
        let source_width = self.view_width
            - if outline_in_layout {
                outline_width
            } else {
                px(0.)
            };
        let navigation_highlight = self.render_outline_source_highlight(source_width, cx);
        let vim_overlays = self.render_vim_overlays(cx);
        let source_context = if self.kind == DocumentKind::Markdown {
            "MarkdownSource"
        } else {
            "DocumentSource"
        };
        // The menu builder runs while InputState is mutably leased by its mouse handler.
        let has_selection = !self.editor.read(cx).selected_range().is_empty();
        let can_format = matches!(self.kind, DocumentKind::Markdown | DocumentKind::Json);
        let input = Input::new(&self.editor)
            .h_full()
            .w_full()
            .p_0()
            .border_0()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(cx.theme().mono_font_size)
            .focus_bordered(false)
            .context_menu(move |menu, _, cx| {
                let has_paste = cx.read_from_clipboard().is_some();

                menu.menu_with_disabled("Cut", !has_selection, Box::new(input::Cut))
                    .menu_with_disabled("Copy", !has_selection, Box::new(input::Copy))
                    .menu_with_disabled("Paste", !has_paste, Box::new(input::Paste))
                    .separator()
                    .menu("Select All", Box::new(input::SelectAll))
                    .separator()
                    .menu_with_disabled("Format Document", !can_format, Box::new(FormatDocument))
            });

        let input = if outline_in_layout && self.analysis.outline_transition_epoch > 0 {
            let (from_width, to_width) = if self.analysis.outline_visible {
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
                        self.analysis.outline_transition_epoch,
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
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_vim_mouse_down))
            .on_prepaint(move |bounds, _, cx| {
                view.update(cx, |this, cx| {
                    if this.analysis.source_bounds != Some(bounds) {
                        this.analysis.source_bounds = Some(bounds);
                        cx.notify();
                    }
                });
            })
            .child(div().size_full().min_w_0().py_4().child(input))
            .children(navigation_highlight)
            .children(vim_overlays)
    }

    fn render_vim_overlays(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if !self.vim_is_enabled() || self.vim_mode() == VimMode::Insert {
            return Vec::new();
        }
        let Some(source_bounds) = self.analysis.source_bounds else {
            return Vec::new();
        };

        let mut overlays = Vec::new();
        let visual_range = self.vim_visual_range(cx);
        let editor = self.editor.read(cx);
        if let Some(selection) = visual_range
            && let Some(visible_rows) = editor.visible_row_range()
        {
            for row in visible_rows {
                let line_start = editor.text().line_start_offset(row);
                let line_end = if row + 1 < editor.text().lines_len() {
                    editor.text().line_start_offset(row + 1)
                } else {
                    editor.text().len()
                };
                let start = selection.start.max(line_start);
                let end = selection.end.min(line_end);
                if start >= end {
                    continue;
                }
                for bounds in vim_selection_bounds(editor, start..end, source_bounds) {
                    overlays.push(
                        vim_overlay(
                            bounds,
                            source_bounds,
                            cx.theme().selection.opacity(0.55),
                            false,
                        )
                        .into_any_element(),
                    );
                }
            }
        }

        if let Some(bounds) = vim_cursor_bounds(editor, editor.cursor()) {
            overlays.push(
                vim_overlay(bounds, source_bounds, cx.theme().caret.opacity(0.58), true)
                    .into_any_element(),
            );
        }
        overlays
    }

    fn render_outline_source_highlight(
        &self,
        source_width: Pixels,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let highlight = self.analysis.outline_source_highlight?;
        let source_bounds = self.analysis.source_bounds?;
        let editor = self.editor.read(cx);
        let row = editor.text().offset_to_point(highlight.source_offset).row;
        if !super::row_is_in_visible_layout(editor.visible_row_range(), row) {
            return None;
        }
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
        let font_size_value = AppSettings::markdown_preview_font_size(cx);
        let font_size = px(font_size_value as f32);

        let sections = if self.analysis.outline.markdown_sections().is_empty() {
            vec![self.editor.read(cx).value()]
        } else {
            self.analysis.outline.markdown_sections().to_vec()
        };
        let section_offsets = if self.analysis.outline.markdown_section_offsets().is_empty() {
            vec![0]
        } else {
            self.analysis.outline.markdown_section_offsets().to_vec()
        };
        let section_count = sections.len();
        let editor_entity = cx.entity();

        let virtualization = markdown_preview_virtualization(self.analysis.outline_rows.is_empty());
        let outline_in_layout = self.analysis.outline_rendered && self.view_width >= px(760.);
        let preview_width = self.view_width
            - if outline_in_layout {
                outline_width_for_view(self.outline_width, self.view_width)
            } else {
                px(0.)
            };

        let horizontal_padding = markdown_preview_horizontal_padding(preview_width);
        let mermaid_width =
            (preview_width.as_f32() - horizontal_padding.as_f32() * 2. - 32.).max(1.);
        let local_image_plugin = super::attachments::LocalImagePlugin::new(
            cx.global::<AppServices>().data_dir(),
            self.persistence.current_path.as_deref(),
        );
        let wikilink_plugin = super::links::WikiLinkPreviewPlugin::new(
            cx.entity(),
            self.inspector_links.project_id,
            self.inspector_links.note_catalog.clone(),
            self.inspector_links.note_links.clone(),
            self.inspector_links.workspace_catalog.clone(),
        );
        let board_embed_plugin =
            super::board_embeds::BoardViewEmbedPlugin::new(cx.entity(), self.embeds.states.clone());
        let preview_style = markdown_preview_style(font_size);

        if self.analysis.preview_list_state.item_count() != section_count {
            self.analysis.preview_list_state.reset(section_count);
        }
        if self
            .analysis
            .preview_font_size_bits
            .replace(font_size_value.to_bits())
            != font_size_value.to_bits()
        {
            self.analysis.preview_list_state.remeasure();
        }

        let content = match virtualization {
            MarkdownPreviewVirtualization::Blocks => TextView::markdown(
                "markdown-preview-blocks",
                sections.into_iter().next().unwrap_or_default(),
            )
            .plugin(local_image_plugin)
            .plugin(board_embed_plugin)
            .plugin(wikilink_plugin)
            .plugin(super::mermaid::MermaidPlugin::new(
                editor_entity.clone(),
                0,
                mermaid_width,
            ))
            .style(preview_style)
            .code_block_actions(|code_block, _window, _cx| {
                Clipboard::new("copy-code").value(code_block.code().clone())
            })
            .size_full()
            .px(horizontal_padding)
            .py_6()
            .text_size(font_size)
            .scrollable(true)
            .selectable(true)
            .into_any_element(),
            MarkdownPreviewVirtualization::Sections => {
                list(self.analysis.preview_list_state.clone(), {
                    move |index, _window, _cx| {
                        div()
                            .w_full()
                            .px(horizontal_padding)
                            .pt(markdown_preview_section_top_padding(index))
                            .when(index == 0, |this| this.pt_6())
                            .when(index + 1 == section_count, |this| this.pb_6())
                            .child(
                                TextView::markdown(
                                    ("markdown-preview-section", index),
                                    sections[index].clone(),
                                )
                                .plugin(local_image_plugin.clone())
                                .plugin(board_embed_plugin.clone())
                                .plugin(wikilink_plugin.clone())
                                .plugin(super::mermaid::MermaidPlugin::new(
                                    editor_entity.clone(),
                                    section_offsets.get(index).copied().unwrap_or_default(),
                                    mermaid_width,
                                ))
                                .style(preview_style.clone())
                                .code_block_actions(|code_block, _window, _cx| {
                                    Clipboard::new("copy-code").value(code_block.code().clone())
                                })
                                .text_size(font_size)
                                .scrollable(false)
                                .selectable(true),
                            )
                            .into_any_element()
                    }
                })
                .size_full()
                .into_any_element()
            }
        };

        div()
            .id("markdown-preview")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .child(content)
            .when(
                virtualization == MarkdownPreviewVirtualization::Sections,
                |this| this.vertical_scrollbar(&self.analysis.preview_list_state),
            )
    }

    fn render_outline(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.inspector_links.tab == DocumentInspectorTab::Links {
            return self.render_links_inspector(cx).into_any_element();
        }
        let selected = self.analysis.outline_selected;
        let rows = self.analysis.outline_rows.clone();
        let kind = self.kind;
        let empty = rows.is_empty();
        let can_expand_all = self.analysis.outline.can_expand_all();
        let can_collapse_all = self.analysis.outline.can_collapse_all();
        let empty_message = if self.kind == DocumentKind::Json {
            "Add JSON properties or array items to navigate this document."
        } else {
            "Add headings to navigate this note."
        };
        let outline_width = outline_width_for_view(self.outline_width, self.view_width);

        v_flex()
            .id("document-outline")
            .key_context("DocumentOutline")
            .track_focus(&self.analysis.outline_focus_handle)
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
                    .child(self.render_inspector_tabs(cx))
                    .child(
                        h_flex()
                            .gap_1()
                            .children((kind == DocumentKind::Json).then(|| {
                                h_flex()
                                    .gap_0p5()
                                    .child(
                                        Button::new("expand-all-json-outline")
                                            .icon(IconName::ChevronDown)
                                            .ghost()
                                            .xsmall()
                                            .disabled(!can_expand_all)
                                            .tooltip("Expand all JSON nodes")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_all_outline_nodes_expanded(true, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("collapse-all-json-outline")
                                            .icon(IconName::ChevronRight)
                                            .ghost()
                                            .xsmall()
                                            .disabled(!can_collapse_all)
                                            .tooltip("Collapse all JSON nodes")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_all_outline_nodes_expanded(false, cx);
                                            })),
                                    )
                            }))
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
                                                                this.analysis
                                                                    .outline_focus_handle
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
                        .track_scroll(&self.analysis.outline_scroll_handle)
                        .with_sizing_behavior(ListSizingBehavior::Auto),
                    )
                },
            )
            .child(self.render_outline_resize_handle(cx))
            .vertical_scrollbar(&self.analysis.outline_scroll_handle)
            .into_any_element()
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

        if self.analysis.outline_transition_epoch == 0 {
            return wrapper.w(outline_width).into_any_element();
        }

        let (from_width, to_width) = if self.analysis.outline_visible {
            (px(0.), outline_width)
        } else {
            (outline_width, px(0.))
        };

        wrapper
            .with_animation(
                (
                    "document-outline-transition",
                    self.analysis.outline_transition_epoch,
                ),
                Animation::new(super::OUTLINE_TRANSITION_DURATION).with_easing(ease_in_out_cubic),
                move |this, delta| this.w(from_width + (to_width - from_width) * delta),
            )
            .into_any_element()
    }

    pub(crate) fn render_editor_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.persistence.is_loading {
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

        if let Some(error) = self.persistence.load_error.clone() {
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
                        (self.vim_is_enabled() && self.mode == EditorMode::Source)
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

    fn render_inspector_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_1()
            .child(
                Button::new("document-inspector-outline")
                    .label("Outline")
                    .ghost()
                    .small()
                    .selected(self.inspector_links.tab == DocumentInspectorTab::Outline)
                    .on_click(cx.listener(|this, _, _, cx| this.show_outline_inspector(cx))),
            )
            .children(
                (self.kind == DocumentKind::Json && self.analysis.outline.json_has_error()).then(
                    || {
                        Icon::new(IconName::TriangleAlert)
                            .xsmall()
                            .text_color(cx.theme().warning)
                    },
                ),
            )
            .children((self.kind == DocumentKind::Markdown).then(|| {
                Button::new("document-inspector-links")
                    .label("Links")
                    .ghost()
                    .small()
                    .selected(self.inspector_links.tab == DocumentInspectorTab::Links)
                    .on_click(cx.listener(|this, _, _, cx| this.show_links_inspector(cx)))
            }))
    }

    fn render_links_inspector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let outline_width = outline_width_for_view(self.outline_width, self.view_width);
        let inbound = self.inspector_links.note_links.inbound.clone();
        let outbound = self.inspector_links.note_links.outbound.clone();
        let inbound_rows = if inbound.is_empty() {
            vec![link_empty_state("No notes link here yet", cx).into_any_element()]
        } else {
            inbound
                .iter()
                .enumerate()
                .map(|(index, link)| {
                    self.render_note_link_row("inbound-note-link", index, link, true, cx)
                })
                .collect()
        };
        let outbound_rows = if outbound.is_empty() {
            vec![link_empty_state("This note has no links", cx).into_any_element()]
        } else {
            outbound
                .iter()
                .enumerate()
                .map(
                    |(index, link)| match (link.target_note_id, link.target_title.as_deref()) {
                        (Some(_), Some(_)) => {
                            self.render_note_link_row("outbound-note-link", index, link, false, cx)
                        }
                        _ => h_flex()
                            .id(("unresolved-note-link", index))
                            .min_h_9()
                            .px_3()
                            .gap_2()
                            .text_sm()
                            .text_color(cx.theme().warning)
                            .child(Icon::new(IconName::TriangleAlert).xsmall())
                            .child(div().min_w_0().truncate().child(link.raw_target.clone()))
                            .into_any_element(),
                    },
                )
                .collect()
        };
        let board_rows =
            self.render_workspace_link_rows(storage::workspace_links::WorkspaceItemKind::Board, cx);
        let list_rows =
            self.render_workspace_link_rows(storage::workspace_links::WorkspaceItemKind::List, cx);
        let card_rows =
            self.render_workspace_link_rows(storage::workspace_links::WorkspaceItemKind::Card, cx);

        v_flex()
            .id("document-links")
            .relative()
            .w(outline_width)
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar.opacity(0.72))
            .child(
                h_flex()
                    .h_10()
                    .px_3()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.7))
                    .child(self.render_inspector_tabs(cx))
                    .child(
                        Button::new("close-document-links")
                            .icon(IconName::Close)
                            .ghost()
                            .xsmall()
                            .tooltip("Hide inspector (Ctrl+Shift+O)")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_outline(window, cx);
                            })),
                    ),
            )
            .when(self.inspector_links.loading, |this| {
                this.child(
                    div()
                        .p_4()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading links…"),
                )
            })
            .when_some(self.inspector_links.error.clone(), |this, error| {
                this.child(
                    v_flex()
                        .p_4()
                        .gap_2()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child("Could not load note links")
                        .child(div().text_xs().child(error)),
                )
            })
            .when(
                !self.inspector_links.loading && self.inspector_links.error.is_none(),
                |this| {
                    this.child(
                        v_flex()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .child(link_section_title("Links to this note", cx))
                            .children(inbound_rows)
                            .child(link_section_title("Links from this note", cx))
                            .children(outbound_rows)
                            .child(link_section_title("Board references", cx))
                            .when(
                                board_rows.is_empty()
                                    && list_rows.is_empty()
                                    && card_rows.is_empty(),
                                |this| {
                                    this.child(link_empty_state(
                                        "This note has no board references",
                                        cx,
                                    ))
                                },
                            )
                            .when(!board_rows.is_empty(), |this| {
                                this.child(link_group_title("Boards", cx))
                                    .children(board_rows)
                            })
                            .when(!list_rows.is_empty(), |this| {
                                this.child(link_group_title("Lists", cx))
                                    .children(list_rows)
                            })
                            .when(!card_rows.is_empty(), |this| {
                                this.child(link_group_title("Cards", cx))
                                    .children(card_rows)
                            }),
                    )
                },
            )
    }

    fn render_workspace_link_rows(
        &self,
        kind: storage::workspace_links::WorkspaceItemKind,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut seen = HashSet::new();
        self.inspector_links
            .workspace_links
            .references
            .iter()
            .filter(|reference| reference.item.item.kind == kind)
            .filter(|reference| seen.insert(reference.item.item))
            .filter_map(|reference| {
                let target = super::links::workspace_navigation_target(&reference.item)?;
                let item_id = reference.item.item.id;
                let label = reference.item.breadcrumb();
                let origin = match reference.origin {
                    storage::workspace_links::WorkspaceLinkOrigin::Manual => "Linked",
                    storage::workspace_links::WorkspaceLinkOrigin::Wikilink => "Markdown",
                    storage::workspace_links::WorkspaceLinkOrigin::Embed => "Embed",
                };
                Some(
                    h_flex()
                        .id(("workspace-link-reference", item_id as u64))
                        .min_h_9()
                        .px_3()
                        .py_1()
                        .cursor_pointer()
                        .hover(|this| this.bg(cx.theme().accent.opacity(0.38)))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(super::DocumentEditorEvent::OpenWorkspaceTarget(target));
                        }))
                        .child(
                            v_flex()
                                .min_w_0()
                                .flex_1()
                                .child(div().text_sm().truncate().child(label))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground.opacity(0.72))
                                        .child(origin),
                                ),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }

    fn render_note_link_row(
        &self,
        id: &'static str,
        index: usize,
        link: &storage::note_links::NoteLinkReference,
        inbound: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (note_id, title, project_name, source_offset) = if inbound {
            (
                link.source_note_id as u32,
                link.source_title.as_str(),
                link.source_project_name.as_deref(),
                Some(link.start_byte),
            )
        } else {
            (
                link.target_note_id.unwrap_or_default() as u32,
                link.target_title.as_deref().unwrap_or(&link.raw_target),
                link.target_project_name.as_deref(),
                None,
            )
        };

        h_flex()
            .id((id, index))
            .min_h_10()
            .px_3()
            .py_1()
            .gap_2()
            .cursor_pointer()
            .hover(|this| this.bg(cx.theme().accent.opacity(0.38)))
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(super::DocumentEditorEvent::OpenNote {
                    note_id,
                    source_offset,
                });
            }))
            .child(Icon::new(IconName::File).xsmall())
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().text_sm().truncate().child(title.to_string()))
                    .children(project_name.map(|project| {
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(project.to_string())
                    })),
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

impl Render for DocumentEditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.mermaid.release_retired_images_after_frame(window);
        self.sync_vim_setting(window, cx);
        self.sync_vim_search_focus(window, cx);
        let theme_background = cx.theme().background;
        let theme_border = cx.theme().border;
        let theme_input = cx.theme().input;
        let status_line_visible = AppSettings::editor_status_line_visible(cx);
        let vim_context = self.vim_context();

        v_flex()
            .id("document-editor-window")
            .key_context(vim_context.as_str())
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .on_action(cx.listener(Self::on_action_save))
            .on_action(cx.listener(Self::on_action_save_as))
            .on_action(cx.listener(Self::on_action_format_document))
            .on_action(cx.listener(Self::on_action_toggle_mode))
            .on_action(cx.listener(Self::on_action_create_card_from_selection))
            .on_action(cx.listener(Self::on_action_insert_board_view))
            .on_action(cx.listener(Self::on_action_toggle_outline))
            .on_action(cx.listener(Self::on_action_expand_emmet))
            .on_action(cx.listener(Self::on_action_emmet_submit_wrap))
            .on_action(cx.listener(Self::on_action_emmet_cancel_wrap))
            .on_action(cx.listener(Self::apply_format))
            .on_action(cx.listener(Self::on_action_vim_key))
            .capture_key_down(cx.listener(Self::on_vim_key_down))
            .capture_action(cx.listener(Self::on_action_vim_insert_escape))
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
                                (self.analysis.outline_rendered && self.view_width >= px(760.))
                                    .then(|| self.render_outline_transition(cx)),
                            ),
                    ),
            )
            .children(status_line_visible.then(|| self.render_status_bar(cx)))
            .children(
                (!status_line_visible && self.vim_is_enabled() && self.mode == EditorMode::Source)
                    .then(|| {
                        div()
                            .id("vim-mode-overlay")
                            .debug_selector(|| "vim-mode-overlay".to_string())
                            .absolute()
                            .bottom(px(8.))
                            .left(px(8.))
                            .child(self.render_vim_mode_indicator(cx))
                    }),
            )
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

impl DocumentEditorView {
    fn render_vim_mode_indicator(&self, cx: &mut Context<Self>) -> AnyElement {
        let (label, color) = match self.vim_mode() {
            VimMode::Normal => ("NORMAL", cx.theme().accent_foreground),
            VimMode::Insert => ("INSERT", cx.theme().success),
            VimMode::Visual => ("VISUAL", cx.theme().warning),
            VimMode::VisualLine => ("VISUAL LINE", cx.theme().warning),
        };
        let command = self.vim_state.state.command_text();
        h_flex()
            .id("vim-mode-indicator")
            .h_5()
            .px_2()
            .gap_1()
            .items_center()
            .rounded_sm()
            .bg(color.opacity(0.14))
            .text_color(color)
            .text_xs()
            .child(label)
            .children(
                (!command.is_empty())
                    .then(|| div().text_color(cx.theme().muted_foreground).child(command)),
            )
            .into_any_element()
    }
}

fn vim_overlay(
    bounds: Bounds<Pixels>,
    source_bounds: Bounds<Pixels>,
    color: Hsla,
    cursor: bool,
) -> impl IntoElement {
    let left = bounds.origin.x - source_bounds.origin.x;
    let top = bounds.origin.y - source_bounds.origin.y;
    let width = if cursor {
        bounds.size.width.max(px(7.))
    } else {
        bounds.size.width.max(px(2.))
    };
    div()
        .absolute()
        .left(left)
        .top(top)
        .w(width)
        .h(bounds.size.height)
        .bg(color)
}

pub(super) fn vim_selection_bounds(
    editor: &InputState,
    range: Range<usize>,
    source_bounds: Bounds<Pixels>,
) -> Vec<Bounds<Pixels>> {
    if range.end.saturating_sub(range.start) <= 2
        && editor
            .text()
            .slice(range.clone())
            .chars()
            .all(|ch| matches!(ch, '\r' | '\n'))
    {
        return vim_cursor_bounds(editor, range.start).into_iter().collect();
    }
    let (Some(start), Some(end)) = (
        editor.range_to_bounds(&(range.start..range.start)),
        editor.range_to_bounds(&(range.end..range.end)),
    ) else {
        return editor.range_to_bounds(&range).into_iter().collect();
    };
    let line_height = editor.line_height().unwrap_or(start.size.height);
    let visible_top = source_bounds.top();
    let visible_bottom = source_bounds.bottom();
    let is_visible =
        |origin_y: Pixels| origin_y < visible_bottom && origin_y + line_height > visible_top;
    if end.origin.y == start.origin.y {
        if !is_visible(start.origin.y) {
            return Vec::new();
        }
        return vec![Bounds::new(
            start.origin,
            size((end.origin.x - start.origin.x).max(px(2.)), line_height),
        )];
    }

    let padding = source_horizontal_padding(source_bounds.size.width);
    let content_left = source_bounds.origin.x + padding;
    let content_right = source_bounds.origin.x + source_bounds.size.width - padding;
    let mut bounds = Vec::new();
    if is_visible(start.origin.y) {
        bounds.push(Bounds::new(
            start.origin,
            size((content_right - start.origin.x).max(px(2.)), line_height),
        ));
    }
    let mut y = start.origin.y + line_height;
    if y < visible_top {
        let hidden_rows = ((visible_top - y) / line_height).floor();
        y += line_height * hidden_rows;
        while y + line_height <= visible_top {
            y += line_height;
        }
    }
    let full_row_end = end.origin.y.min(visible_bottom);
    while y + px(0.5) < full_row_end {
        bounds.push(Bounds::new(
            point(content_left, y),
            size((content_right - content_left).max(px(2.)), line_height),
        ));
        y += line_height;
    }
    if let Some(tail_width) = vim_selection_tail_width(content_left, end.origin.x)
        && is_visible(end.origin.y)
    {
        bounds.push(Bounds::new(
            point(content_left, end.origin.y),
            size(tail_width, line_height),
        ));
    }
    bounds
}

fn vim_selection_tail_width(content_left: Pixels, end_x: Pixels) -> Option<Pixels> {
    let width = end_x - content_left;
    (width > px(0.5)).then_some(width)
}

pub(super) fn vim_cursor_bounds(editor: &InputState, cursor: usize) -> Option<Bounds<Pixels>> {
    let caret = editor.range_to_bounds(&(cursor..cursor))?;
    let line_height = editor.line_height().unwrap_or(caret.size.height);
    let character = match editor.text().char_at(cursor) {
        Some('\r' | '\n') | None => caret,
        Some(ch) => editor
            .range_to_bounds(&(cursor..cursor + ch.len_utf8()))
            .unwrap_or(caret),
    };
    Some(normalize_vim_cursor_bounds(character, caret, line_height))
}

fn normalize_vim_cursor_bounds(
    character: Bounds<Pixels>,
    caret: Bounds<Pixels>,
    line_height: Pixels,
) -> Bounds<Pixels> {
    let bounds = if character.size.height > line_height + px(0.5) {
        caret
    } else {
        character
    };
    Bounds::new(
        bounds.origin,
        size(bounds.size.width.max(px(7.)), line_height),
    )
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

#[derive(Debug, PartialEq, Eq)]
struct StatusPath {
    directory: Option<String>,
    file_name: String,
    tooltip: String,
}

fn status_path(path: Option<&Path>, kind: DocumentKind) -> StatusPath {
    let Some(path) = path else {
        return StatusPath {
            directory: None,
            file_name: "Not saved yet".to_string(),
            tooltip: "This note has not been saved to a file".to_string(),
        };
    };

    let tooltip = readable_full_path(path);
    let trimmed = tooltip.trim_end_matches(['/', '\\']);
    let (parent, file_name) = trimmed
        .rfind(['/', '\\'])
        .map(|separator| (&trimmed[..separator], &trimmed[separator + 1..]))
        .unwrap_or(("", trimmed));
    let directory = parent
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|directory| !directory.is_empty() && !directory.ends_with(':'))
        .map(str::to_string);
    let file_name = status_file_name(file_name, kind);

    StatusPath {
        directory,
        file_name,
        tooltip,
    }
}

fn readable_full_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if let Some(path) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{path}")
    } else {
        path.strip_prefix(r"\\?\")
            .unwrap_or(path.as_ref())
            .to_string()
    }
}

fn status_file_name(file_name: &str, kind: DocumentKind) -> String {
    if kind == DocumentKind::Markdown
        && let Some((stem, extension)) = file_name.rsplit_once('.')
        && matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown")
    {
        return stem.to_string();
    }

    file_name.to_string()
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

fn markdown_preview_horizontal_padding(preview_width: Pixels) -> Pixels {
    const PREVIEW_MAX_WIDTH: f32 = 920.;
    const PREVIEW_GUTTER: f32 = 24.;

    px(((preview_width.as_f32() - PREVIEW_MAX_WIDTH) / 2. + PREVIEW_GUTTER).max(PREVIEW_GUTTER))
}

fn markdown_preview_virtualization(outline_is_empty: bool) -> MarkdownPreviewVirtualization {
    if outline_is_empty {
        MarkdownPreviewVirtualization::Blocks
    } else {
        MarkdownPreviewVirtualization::Sections
    }
}

fn markdown_preview_block_gap() -> Rems {
    rems(1.)
}

fn markdown_preview_section_top_padding(index: usize) -> Rems {
    if index == 0 {
        rems(0.)
    } else {
        markdown_preview_block_gap()
    }
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

fn link_section_title(label: &str, cx: &App) -> impl IntoElement {
    h_flex()
        .h_9()
        .px_3()
        .mt_1()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(label.to_string())
}

fn link_group_title(label: &str, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .pt_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground.opacity(0.8))
        .child(label.to_string())
}

fn link_empty_state(label: &str, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .pb_3()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(label.to_string())
}

fn reserves_disclosure_space(kind: DocumentKind, has_children: bool) -> bool {
    kind == DocumentKind::Json && !has_children
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, px, rems, size};

    use super::{
        DocumentKind, MarkdownPreviewVirtualization, markdown_preview_block_gap,
        markdown_preview_horizontal_padding, markdown_preview_section_top_padding,
        markdown_preview_virtualization, normalize_vim_cursor_bounds, outline_row_left_padding,
        outline_width_for_view, reserves_disclosure_space, status_path, vim_selection_tail_width,
    };
    use std::path::Path;

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

    #[test]
    fn markdown_preview_keeps_reading_width_and_minimum_gutter() {
        assert_eq!(markdown_preview_horizontal_padding(px(800.)), px(24.));
        assert_eq!(markdown_preview_horizontal_padding(px(1_200.)), px(164.));
    }

    #[test]
    fn markdown_preview_preserves_outline_navigation_with_section_virtualization() {
        assert_eq!(
            markdown_preview_virtualization(true),
            MarkdownPreviewVirtualization::Blocks
        );
        assert_eq!(
            markdown_preview_virtualization(false),
            MarkdownPreviewVirtualization::Sections
        );
    }

    #[test]
    fn markdown_preview_restores_block_spacing_between_virtualized_sections() {
        assert_eq!(markdown_preview_section_top_padding(0), rems(0.));
        assert_eq!(
            markdown_preview_section_top_padding(1),
            markdown_preview_block_gap()
        );
    }

    #[test]
    fn status_path_replaces_storage_prefix_with_a_compact_breadcrumb() {
        let path = status_path(
            Some(Path::new(
                r"\\?\C:\Users\Berat\Documents\Obsidian Vault\Cover Letter\Cover Letter Variant.md",
            )),
            DocumentKind::Markdown,
        );

        assert_eq!(path.directory.as_deref(), Some("Cover Letter"));
        assert_eq!(path.file_name, "Cover Letter Variant");
        assert_eq!(
            path.tooltip,
            r"C:\Users\Berat\Documents\Obsidian Vault\Cover Letter\Cover Letter Variant.md"
        );
    }

    #[test]
    fn status_path_preserves_meaningful_non_markdown_extensions() {
        let path = status_path(
            Some(Path::new("workspace/settings.json")),
            DocumentKind::Json,
        );

        assert_eq!(path.directory.as_deref(), Some("workspace"));
        assert_eq!(path.file_name, "settings.json");
        assert_eq!(path.tooltip, "workspace/settings.json");
    }

    #[test]
    fn status_path_has_a_clear_label_before_the_first_save() {
        let path = status_path(None, DocumentKind::Markdown);

        assert_eq!(path.directory, None);
        assert_eq!(path.file_name, "Not saved yet");
        assert_eq!(path.tooltip, "This note has not been saved to a file");
    }

    #[test]
    fn vim_cursor_uses_single_row_caret_geometry_for_cross_row_ranges() {
        let line_height = px(18.);
        let caret = Bounds::new(point(px(10.), px(20.)), size(px(0.), line_height));
        let cross_row = Bounds::new(point(px(10.), px(20.)), size(px(80.), px(36.)));
        let normalized = normalize_vim_cursor_bounds(cross_row, caret, line_height);

        assert_eq!(normalized.origin, caret.origin);
        assert_eq!(normalized.size, size(px(7.), line_height));
    }

    #[test]
    fn vim_selection_does_not_paint_a_sliver_at_an_exclusive_row_boundary() {
        assert_eq!(vim_selection_tail_width(px(40.), px(40.)), None);
        assert_eq!(vim_selection_tail_width(px(40.), px(40.4)), None);
        assert_eq!(vim_selection_tail_width(px(40.), px(52.)), Some(px(12.)));
    }
}
