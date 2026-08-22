use super::*;

impl DocumentEditorView {
    pub(super) fn apply_motion_key(
        &mut self,
        key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.vim_state.state.count.take();
        let preferred = self.vim_state.state.preferred_column;
        let motion = {
            let editor = self.editor.read(cx);
            motion_for_key(editor.text(), editor.cursor(), key, count, preferred)
        };
        let Some(motion) = motion else {
            self.vim_state.state.reset_command();
            return;
        };

        if matches!(key, VimKey::Up | VimKey::Down) {
            if self.vim_state.state.preferred_column.is_none() {
                self.vim_state.state.preferred_column = Some(
                    self.editor
                        .read(cx)
                        .text()
                        .offset_to_position(self.editor.read(cx).cursor())
                        .character,
                );
            }
        } else {
            self.vim_state.state.preferred_column = None;
        }
        self.vim_state.state.pending_g = false;
        self.set_vim_cursor(motion.target, window, cx);
        cx.notify();
    }

    pub(super) fn delete_vim_char(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.vim_state.state.take_count();
        let range = {
            let editor = self.editor.read(cx);
            forward_char_range(editor.text(), editor.cursor(), count)
        };
        self.apply_operator(VimOperator::Delete, range, false, window, cx);
    }

    pub(super) fn delete_vim_previous_char(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.vim_state.state.take_count();
        let range = {
            let editor = self.editor.read(cx);
            backward_char_range(editor.text(), editor.cursor(), count)
        };
        self.apply_operator(VimOperator::Delete, range, false, window, cx);
    }

    pub(super) fn substitute_vim_char(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.vim_state.state.take_count();
        let range = {
            let editor = self.editor.read(cx);
            forward_char_range(editor.text(), editor.cursor(), count)
        };
        if range.is_empty() {
            self.enter_vim_insert_at_cursor(window, cx);
        } else {
            self.apply_operator(VimOperator::Change, range, false, window, cx);
        }
    }

    pub(super) fn join_vim_lines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.vim_state.state.take_count().max(2);
        let edit = {
            let editor = self.editor.read(cx);
            join_line_edit(editor.text(), editor.cursor(), count)
        };
        let Some((range, replacement)) = edit else {
            self.vim_state.state.reset_command();
            cx.notify();
            return;
        };
        let cursor = range.start;
        self.replace_vim_range(range, &replacement, window, cx);
        self.set_vim_cursor(cursor, window, cx);
        self.enter_vim_normal(window, cx);
    }

    pub(super) fn paste_vim(&mut self, before: bool, window: &mut Window, cx: &mut Context<Self>) {
        let clipboard_register = cx.read_from_clipboard().and_then(|item| {
            let linewise = item
                .metadata()
                .is_some_and(|metadata| metadata.as_str() == VIM_CLIPBOARD_LINEWISE);
            item.text().map(|text| VimRegister { text, linewise })
        });
        let register = clipboard_register
            .or_else(|| self.vim_state.state.register.clone())
            .unwrap_or_else(|| VimRegister {
                text: String::new(),
                linewise: false,
            });
        if register.text.is_empty() {
            return;
        }
        let count = self.vim_state.state.take_count() as usize;
        let replacement = if register.linewise {
            let mut text = register.text;
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.repeat(count)
        } else {
            register.text.repeat(count)
        };

        if self.vim_state.state.mode.is_visual() {
            let Some(range) = self.vim_visual_range(cx) else {
                return;
            };
            self.replace_vim_range(range.clone(), &replacement, window, cx);
            self.set_vim_cursor(range.start, window, cx);
            self.enter_vim_normal(window, cx);
            return;
        }

        let (offset, cursor_after, replacement) = {
            let editor = self.editor.read(cx);
            if register.linewise {
                let row = row_at(editor.text(), editor.cursor());
                if before {
                    let offset = editor.text().line_start_offset(row);
                    (offset, offset, replacement)
                } else if row + 1 < editor.text().lines_len() {
                    let offset = editor.text().line_start_offset(row + 1);
                    (offset, offset, replacement)
                } else {
                    let offset = editor.text().len();
                    let needs_newline = offset > 0
                        && editor
                            .text()
                            .char_at(previous_boundary(editor.text(), offset))
                            != Some('\n');
                    let replacement = if needs_newline {
                        format!("\n{}", replacement.trim_end_matches('\n'))
                    } else {
                        replacement
                    };
                    (offset, offset + usize::from(needs_newline), replacement)
                }
            } else {
                let offset = if before {
                    editor.cursor()
                } else {
                    next_boundary(editor.text(), editor.cursor())
                };
                let cursor_delta = replacement
                    .char_indices()
                    .last()
                    .map_or(0, |(index, _)| index);
                (offset, offset + cursor_delta, replacement)
            }
        };
        self.replace_vim_range(offset..offset, &replacement, window, cx);
        self.set_vim_cursor(cursor_after, window, cx);
        self.enter_vim_normal(window, cx);
    }

    pub(super) fn open_vim_line(
        &mut self,
        above: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (offset, prefix, insertion, line_break_len) = {
            let editor = self.editor.read(cx);
            let rope = editor.text();
            let row = row_at(rope, editor.cursor());
            let line = rope.slice_line(row).to_string();
            let line_break = line_break_for_row(rope, row);
            let prefix = if above {
                leading_indent(&line)
            } else {
                markdown_newline_prefix(&line)
            };
            if above {
                (
                    rope.line_start_offset(row),
                    prefix.clone(),
                    format!("{prefix}{line_break}"),
                    line_break.len(),
                )
            } else {
                let end = line_content_end(rope, row);
                (
                    end,
                    prefix.clone(),
                    format!("{line_break}{prefix}"),
                    line_break.len(),
                )
            }
        };
        self.replace_vim_range(offset..offset, &insertion, window, cx);
        let cursor = if above {
            offset + prefix.len()
        } else {
            offset + line_break_len + prefix.len()
        };
        self.enter_vim_insert(cursor, window, cx);
    }

    pub(super) fn enter_vim_insert_at_cursor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cursor = self.editor.read(cx).cursor();
        self.enter_vim_insert(cursor, window, cx);
    }

    pub(super) fn enter_vim_insert(
        &mut self,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.vim_state.state.mode = VimMode::Insert;
        self.vim_state.state.visual_anchor = None;
        self.vim_state.state.visual_head = None;
        self.vim_state.state.reset_command();
        self.set_input_cursor(offset, window, cx);
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
        cx.notify();
    }

    pub(super) fn enter_vim_normal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_insert = self.vim_state.state.mode == VimMode::Insert;
        let target = {
            let editor = self.editor.read(cx);
            let cursor = editor.cursor();
            let row = row_at(editor.text(), cursor);
            let start = editor.text().line_start_offset(row);
            let end = line_content_end(editor.text(), row);
            if was_insert && cursor > start {
                previous_boundary(editor.text(), cursor.min(end))
            } else {
                cursor.min(normal_line_end(editor.text(), row))
            }
        };
        self.vim_state.state.mode = VimMode::Normal;
        self.vim_state.state.visual_anchor = None;
        self.vim_state.state.visual_head = None;
        self.vim_state.state.preferred_column = None;
        self.vim_state.state.reset_command();
        self.set_vim_cursor(target, window, cx);
        cx.notify();
    }

    pub(super) fn set_vim_cursor(
        &mut self,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = {
            let editor = self.editor.read(cx);
            clamp_normal_offset(editor.text(), offset)
        };
        if self.vim_state.state.mode.is_visual() {
            self.vim_state.state.visual_head = Some(offset);
        }
        self.set_input_cursor(offset, window, cx);
        self.focus_handle.focus(window, cx);
    }

    pub(super) fn set_input_cursor(
        &mut self,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            let offset = offset.min(editor.text().len());
            let position = editor.text().offset_to_position(offset);
            editor.set_cursor_position(position, window, cx);
        });
    }

    pub(super) fn replace_vim_range(
        &mut self,
        range: Range<usize>,
        replacement: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            let start = editor.text().offset_to_offset_utf16(range.start);
            let end = editor.text().offset_to_offset_utf16(range.end);
            EntityInputHandler::replace_text_in_range(
                editor,
                Some(start..end),
                replacement,
                window,
                cx,
            );
        });
    }

    pub(super) fn dispatch_input_action(
        &mut self,
        action: Box<dyn gpui::Action>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
        window.dispatch_action(action, cx);
        self.focus_handle.focus(window, cx);
        self.vim_state.state.reset_command();
        cx.notify();
    }

    pub(super) fn dispatch_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.vim_state.search_active = false;
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
        window.dispatch_action(Box::new(Search), cx);
        self.vim_state.search_active = true;
        self.vim_state.state.reset_command();
        cx.notify();
    }
}
