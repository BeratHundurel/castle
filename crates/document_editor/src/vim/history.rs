use super::*;

impl DocumentEditorView {
    pub(super) fn vim_command_in_progress(&self) -> bool {
        self.vim_state.state.count.is_some()
            || self.vim_state.state.pending_operator.is_some()
            || self.vim_state.state.pending_g
            || self.vim_state.state.pending_text_object.is_some()
            || self.vim_state.state.pending_char.is_some()
    }

    pub(super) fn prepare_vim_change_candidate(&mut self, key: VimKey, cx: &gpui::App) {
        if self.vim_state.state.mode.is_visual() {
            self.vim_state.state.change_candidate.clear();
            self.vim_state.state.candidate_visual = self.vim_visual_repeat(cx);
        } else if !self.vim_command_in_progress() {
            self.vim_state.state.change_candidate.clear();
            self.vim_state.state.candidate_visual = None;
        }
        self.vim_state
            .state
            .change_candidate
            .push(VimReplayStep::Key(key));
    }

    pub(super) fn finish_vim_action_recording(
        &mut self,
        before_mode: VimMode,
        before_text: Rope,
        before_cursor: usize,
        cx: &gpui::App,
    ) {
        let changed = before_text != *self.editor.read(cx).text();
        if self.vim_state.state.mode == VimMode::Insert && before_mode != VimMode::Insert {
            let (steps, count) = normalized_replay_steps(&self.vim_state.state.change_candidate);
            self.vim_state.state.insert_capture = Some(VimInsertCapture {
                before: self.editor.read(cx).text().clone(),
                anchor: self.editor.read(cx).cursor(),
                steps,
                count,
                visual: self.vim_state.state.candidate_visual,
                pre_edit_changed: changed,
                history_before: before_text,
                history_cursor: before_cursor,
            });
            self.vim_state.state.change_candidate.clear();
            self.vim_state.state.candidate_visual = None;
        } else if changed && self.vim_state.state.mode != VimMode::Insert {
            self.push_vim_history(before_text, before_cursor, cx);
            self.commit_vim_change(None);
        } else if self.vim_state.state.mode.is_visual() || !self.vim_command_in_progress() {
            self.discard_vim_change_candidate();
        }
    }

    pub(super) fn vim_visual_repeat(&self, cx: &gpui::App) -> Option<VimVisualRepeat> {
        let range = self.vim_visual_range(cx)?;
        if self.vim_state.state.mode == VimMode::VisualLine {
            let rope = self.editor.read(cx).text();
            let start = row_at(rope, range.start);
            let end_offset = previous_boundary(rope, range.end);
            let end = row_at(rope, end_offset);
            Some(VimVisualRepeat {
                linewise: true,
                extent: end.saturating_sub(start) + 1,
            })
        } else {
            Some(VimVisualRepeat {
                linewise: false,
                extent: self.editor.read(cx).text().slice(range).chars().count(),
            })
        }
    }

    pub(super) fn commit_vim_change(&mut self, insert_patch: Option<VimInsertPatch>) {
        let (steps, count) = normalized_replay_steps(&self.vim_state.state.change_candidate);
        if !steps.is_empty() {
            self.vim_state.state.last_change = Some(VimChangeRecipe {
                steps,
                count,
                insert_patch,
                visual: self.vim_state.state.candidate_visual,
            });
        }
        self.discard_vim_change_candidate();
    }

    pub(super) fn discard_vim_change_candidate(&mut self) {
        self.vim_state.state.change_candidate.clear();
        self.vim_state.state.candidate_visual = None;
    }

    pub(super) fn finish_vim_insert_capture(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(capture) = self.vim_state.state.insert_capture.take() else {
            return;
        };
        let after = self.editor.read(cx).text();
        let cursor = self.editor.read(cx).cursor();
        let insert_patch = insert_patch_between(&capture.before, after, capture.anchor, cursor);
        if insert_patch.is_none() && !capture.pre_edit_changed {
            return;
        }
        if capture.count > 1
            && replay_repeats_insert_text(&capture.steps)
            && let Some(patch) = insert_patch.as_ref()
            && patch.start_delta == 0
            && patch.end_delta == 0
        {
            let extra = patch
                .replacement
                .repeat(capture.count.saturating_sub(1) as usize);
            self.replace_vim_range(cursor..cursor, &extra, window, cx);
            self.set_input_cursor(cursor.saturating_add(extra.len()), window, cx);
        }
        self.vim_state.state.last_change = Some(VimChangeRecipe {
            steps: capture.steps,
            count: capture.count,
            insert_patch,
            visual: capture.visual,
        });
        self.push_vim_history(capture.history_before, capture.history_cursor, cx);
    }

    pub(super) fn push_vim_history(&mut self, before: Rope, cursor_before: usize, cx: &gpui::App) {
        let after = self.editor.read(cx).text().clone();
        if before == after {
            return;
        }
        if self.vim_state.state.undo_stack.len() >= 1_000 {
            self.vim_state.state.undo_stack.remove(0);
        }
        self.vim_state.state.undo_stack.push(VimHistoryEntry {
            before,
            after,
            cursor_before,
            cursor_after: self.editor.read(cx).cursor(),
        });
        self.vim_state.state.redo_stack.clear();
    }

    pub(crate) fn sync_vim_search_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.vim_state.state.enabled
            || self.vim_state.state.mode == VimMode::Insert
            || !self.mode.shows_source()
            || !self.editor.focus_handle(cx).is_focused(window)
        {
            return;
        }

        self.vim_state.search_active = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }
}
