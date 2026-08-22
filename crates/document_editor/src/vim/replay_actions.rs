use super::*;

impl DocumentEditorView {
    pub(super) fn replace_vim_chars(
        &mut self,
        target: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (range, replacement, linewise) = if self.vim_state.state.mode.is_visual() {
            let Some(range) = self.vim_visual_range(cx) else {
                self.vim_state.state.reset_command();
                return;
            };
            let linewise = self.vim_state.state.mode == VimMode::VisualLine;
            let selected = self.editor.read(cx).text().slice(range.clone()).to_string();
            let replacement = replace_visual_text(&selected, target);
            (range, replacement, linewise)
        } else {
            let count = self.vim_state.state.take_count();
            let editor = self.editor.read(cx);
            let range = forward_char_range(editor.text(), editor.cursor(), count);
            if range.is_empty()
                || editor.text().slice(range.clone()).chars().count() != count as usize
            {
                self.vim_state.state.reset_command();
                cx.notify();
                return;
            }
            let replacement = if target == "\n" {
                line_break_for_row(editor.text(), row_at(editor.text(), editor.cursor()))
                    .to_string()
            } else {
                target.repeat(count as usize)
            };
            (range, replacement, false)
        };

        let replaced = self.editor.read(cx).text().slice(range.clone()).to_string();
        self.vim_state.state.register = Some(VimRegister {
            text: replaced.clone(),
            linewise,
        });
        let metadata = if linewise {
            VIM_CLIPBOARD_LINEWISE
        } else {
            VIM_CLIPBOARD_CHARACTERWISE
        };
        cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(
            replaced,
            metadata.to_string(),
        ));
        self.replace_vim_range(range.clone(), &replacement, window, cx);
        self.set_vim_cursor(range.start, window, cx);
        self.enter_vim_normal(window, cx);
    }

    pub(super) fn repeat_last_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count_override = self.vim_state.state.count.take();
        let Some(recipe) = self.vim_state.state.last_change.clone() else {
            self.vim_state.state.reset_command();
            cx.notify();
            return;
        };
        let count = count_override.unwrap_or(recipe.count).clamp(1, MAX_COUNT);
        let history_before = self.editor.read(cx).text().clone();
        let history_cursor = self.editor.read(cx).cursor();
        let live_editor = self.editor.clone();
        let scratch_value = history_before.to_string();
        let scratch_editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(Language::Plain)
                .default_value(scratch_value)
        });
        scratch_editor.update(cx, |editor, cx| {
            editor.set_cursor_position(
                history_before.offset_to_position(history_cursor),
                window,
                cx,
            );
        });
        self.editor = scratch_editor;
        self.vim_state.state.replaying = true;
        self.vim_state.state.reset_command();

        if replay_is_open_line(&recipe.steps) && recipe.visual.is_none() {
            for _ in 0..count {
                self.replay_vim_steps(&recipe.steps, window, cx);
                if self.vim_state.state.mode == VimMode::Insert {
                    if let Some(patch) = recipe.insert_patch.as_ref() {
                        self.apply_insert_patch(patch, 1, window, cx);
                    }
                    self.enter_vim_normal(window, cx);
                }
            }
        } else {
            if let Some(visual) = recipe.visual {
                self.prepare_visual_repeat(visual, window, cx);
            }
            let repeat_insert_text =
                recipe.visual.is_none() && replay_repeats_insert_text(&recipe.steps);
            if !repeat_insert_text {
                for digit in count.to_string().bytes() {
                    self.vim_state.state.push_digit(digit.saturating_sub(b'0'));
                }
            }
            self.replay_vim_steps(&recipe.steps, window, cx);
            if self.vim_state.state.mode == VimMode::Insert {
                if let Some(patch) = recipe.insert_patch.as_ref() {
                    let repetitions = if repeat_insert_text { count } else { 1 };
                    self.apply_insert_patch(patch, repetitions, window, cx);
                }
                self.enter_vim_normal(window, cx);
            }
        }
        let replayed_text = self.editor.read(cx).text().clone();
        let replayed_cursor = self.editor.read(cx).cursor();
        self.editor = live_editor;
        self.vim_state.state.replaying = false;
        self.vim_state.state.last_change = Some(recipe);
        self.discard_vim_change_candidate();
        if let Some((range, replacement)) =
            rope_replacement_between(&history_before, &replayed_text)
        {
            self.replace_vim_range(range, &replacement, window, cx);
        }
        self.set_vim_cursor(replayed_cursor, window, cx);
        self.enter_vim_normal(window, cx);
        self.push_vim_history(history_before, history_cursor, cx);
        cx.notify();
    }

    pub(super) fn replay_vim_steps(
        &mut self,
        steps: &[VimReplayStep],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for step in steps {
            match step {
                VimReplayStep::Key(VimKey::Digit(_)) => {}
                VimReplayStep::Key(key) => {
                    if self.vim_state.state.mode.is_visual() {
                        self.handle_visual_key(*key, window, cx);
                    } else {
                        self.handle_normal_key(*key, window, cx);
                    }
                }
                VimReplayStep::Literal(target) => {
                    if let Some(pending) = self.vim_state.state.pending_char.take() {
                        self.apply_pending_vim_char(pending, target.clone(), window, cx);
                    }
                }
            }
        }
    }

    pub(super) fn undo_vim_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.vim_state.state.undo_stack.pop() else {
            self.dispatch_input_action(Box::new(Undo), window, cx);
            return;
        };
        if entry.after != *self.editor.read(cx).text() {
            self.vim_state.state.redo_stack.clear();
            self.dispatch_input_action(Box::new(Undo), window, cx);
            return;
        }
        let current_len = self.editor.read(cx).text().len();
        self.vim_state.state.replaying = true;
        self.replace_vim_range(0..current_len, &entry.before.to_string(), window, cx);
        self.set_vim_cursor(entry.cursor_before, window, cx);
        self.enter_vim_normal(window, cx);
        self.vim_state.state.replaying = false;
        self.vim_state.state.redo_stack.push(entry);
    }

    pub(super) fn redo_vim_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.vim_state.state.redo_stack.pop() else {
            self.dispatch_input_action(Box::new(Redo), window, cx);
            return;
        };
        if entry.before != *self.editor.read(cx).text() {
            self.dispatch_input_action(Box::new(Redo), window, cx);
            return;
        }
        let current_len = self.editor.read(cx).text().len();
        self.vim_state.state.replaying = true;
        self.replace_vim_range(0..current_len, &entry.after.to_string(), window, cx);
        self.set_vim_cursor(entry.cursor_after, window, cx);
        self.enter_vim_normal(window, cx);
        self.vim_state.state.replaying = false;
        self.vim_state.state.undo_stack.push(entry);
    }

    pub(super) fn prepare_visual_repeat(
        &mut self,
        visual: VimVisualRepeat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let anchor = self.editor.read(cx).cursor();
        let head = {
            let editor = self.editor.read(cx);
            let rope = editor.text();
            if visual.linewise {
                let row = row_at(rope, anchor);
                let target_row = row
                    .saturating_add(visual.extent.saturating_sub(1))
                    .min(rope.lines_len().saturating_sub(1));
                rope.line_start_offset(target_row)
            } else {
                move_by_chars(rope, anchor, visual.extent.saturating_sub(1) as isize)
            }
        };
        self.vim_state.state.mode = if visual.linewise {
            VimMode::VisualLine
        } else {
            VimMode::Visual
        };
        self.vim_state.state.visual_anchor = Some(anchor);
        self.vim_state.state.visual_head = Some(head);
        self.set_vim_cursor(head, window, cx);
    }

    pub(super) fn apply_insert_patch(
        &mut self,
        patch: &VimInsertPatch,
        repetitions: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let anchor = self.editor.read(cx).cursor();
        let (start, end) = {
            let rope = self.editor.read(cx).text();
            (
                move_by_chars(rope, anchor, patch.start_delta),
                move_by_chars(rope, anchor, patch.end_delta),
            )
        };
        let replacement = patch.replacement.repeat(repetitions as usize);
        self.replace_vim_range(start..end, &replacement, window, cx);
        let extra_cursor = if repetitions > 1 && patch.cursor_delta >= 0 {
            patch
                .replacement
                .chars()
                .count()
                .saturating_mul(repetitions.saturating_sub(1) as usize) as isize
        } else {
            0
        };
        let cursor = move_by_chars(
            self.editor.read(cx).text(),
            start,
            patch.cursor_delta.saturating_add(extra_cursor),
        );
        self.set_input_cursor(cursor, window, cx);
    }
}
