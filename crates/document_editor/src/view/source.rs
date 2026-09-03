use super::*;

impl DocumentEditorView {
    pub(crate) fn render_source(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (outline_in_layout, _) = editor_layout_signature(
            self.view_width,
            self.analysis.outline_rendered,
            self.outline_width,
        );
        let source_width = self.view_width
            - if outline_in_layout {
                outline_width_for_view(self.outline_width, self.view_width)
            } else {
                px(0.)
            };

        self.render_source_with_width(source_width, outline_in_layout, cx)
    }

    pub(crate) fn render_source_with_width(
        &self,
        source_width: Pixels,
        outline_in_layout: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
        let source_is_ready = self.analysis.source_bounds.is_some();
        let outline_width = outline_width_for_view(self.outline_width, self.view_width);
        let navigation_highlight = self.render_outline_source_highlight(source_width, cx);
        let vim_overlays = self.render_vim_overlays(cx);
        let source_layout_mode = self.mode;
        let source_context = if self.kind == DocumentKind::Markdown {
            "MarkdownSource"
        } else {
            "DocumentSource"
        };
        // The menu builder runs while the editor state is mutably leased by its mouse handler.
        let has_selection = !self.editor.read(cx).selected_range().is_empty();
        let can_format = matches!(self.kind, DocumentKind::Markdown | DocumentKind::Json);
        let input = Editor::new(&self.editor)
            .h_full()
            .w_full()
            .p_0()
            .border_0()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(cx.theme().mono_font_size)
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
                    Animation::new(crate::OUTLINE_TRANSITION_DURATION)
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
                    let mode_changed = this.analysis.source_bounds_mode != Some(source_layout_mode);
                    if this.analysis.source_bounds != Some(bounds) || mode_changed {
                        this.analysis.source_bounds = Some(bounds);
                        this.analysis.source_bounds_mode = Some(source_layout_mode);
                        if mode_changed {
                            cx.notify();
                        }
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
        if !crate::row_is_in_visible_layout(editor.visible_row_range(), row) {
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
                    Animation::new(crate::OUTLINE_SOURCE_HIGHLIGHT_DURATION)
                        .with_easing(ease_in_out_cubic),
                    |this, delta| {
                        let fade = ((delta - 0.2) / 0.8).clamp(0., 1.);
                        this.opacity(1. - fade)
                    },
                )
                .into_any_element(),
        )
    }
}
