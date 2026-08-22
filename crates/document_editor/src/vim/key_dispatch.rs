use super::*;

impl DocumentEditorView {
    pub(super) fn handle_normal_key(
        &mut self,
        key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(operator) = self.vim_state.state.pending_operator {
            if self.handle_pending_operator(operator, key, window, cx) {
                return;
            }
            self.vim_state.state.reset_command();
            cx.notify();
            return;
        }
        if self.vim_state.state.pending_g && key != VimKey::Go {
            self.vim_state.state.reset_command();
            cx.notify();
            return;
        }

        match key {
            VimKey::Digit(0) => self.apply_motion_key(VimKey::LineStart, window, cx),
            VimKey::Left
            | VimKey::Down
            | VimKey::Up
            | VimKey::Right
            | VimKey::WordForward
            | VimKey::WordBackward
            | VimKey::WordEnd
            | VimKey::BigWordForward
            | VimKey::BigWordBackward
            | VimKey::BigWordEnd
            | VimKey::FirstNonBlank
            | VimKey::LineEnd
            | VimKey::DocumentEnd => self.apply_motion_key(key, window, cx),
            VimKey::FindForward => self.begin_find(VimFindKind::Forward, cx),
            VimKey::FindBackward => self.begin_find(VimFindKind::Backward, cx),
            VimKey::TillForward => self.begin_find(VimFindKind::TillForward, cx),
            VimKey::TillBackward => self.begin_find(VimFindKind::TillBackward, cx),
            VimKey::RepeatFind => self.repeat_find(false, window, cx),
            VimKey::RepeatFindReverse => self.repeat_find(true, window, cx),
            VimKey::Go => {
                if self.vim_state.state.pending_g {
                    self.vim_state.state.pending_g = false;
                    self.apply_motion_key(VimKey::Go, window, cx);
                } else {
                    self.vim_state.state.pending_g = true;
                    cx.notify();
                }
            }
            VimKey::Insert => self.enter_vim_insert_at_cursor(window, cx),
            VimKey::Append => {
                let target = {
                    let editor = self.editor.read(cx);
                    next_boundary(editor.text(), editor.cursor())
                };
                self.enter_vim_insert(target, window, cx);
            }
            VimKey::InsertLineStart => {
                let target = {
                    let editor = self.editor.read(cx);
                    first_non_blank(editor.text(), editor.cursor())
                };
                self.enter_vim_insert(target, window, cx);
            }
            VimKey::AppendLineEnd => {
                let target = {
                    let editor = self.editor.read(cx);
                    line_content_end(editor.text(), row_at(editor.text(), editor.cursor()))
                };
                self.enter_vim_insert(target, window, cx);
            }
            VimKey::OpenBelow => self.open_vim_line(false, window, cx),
            VimKey::OpenAbove => self.open_vim_line(true, window, cx),
            VimKey::Visual => {
                self.vim_state.state.mode = VimMode::Visual;
                let cursor = self.editor.read(cx).cursor();
                self.vim_state.state.visual_anchor = Some(cursor);
                self.vim_state.state.visual_head = Some(cursor);
                self.vim_state.state.reset_command();
                self.focus_handle.focus(window, cx);
                cx.notify();
            }
            VimKey::VisualLine => {
                self.vim_state.state.mode = VimMode::VisualLine;
                let cursor = self.editor.read(cx).cursor();
                self.vim_state.state.visual_anchor = Some(cursor);
                self.vim_state.state.visual_head = Some(cursor);
                self.vim_state.state.reset_command();
                self.focus_handle.focus(window, cx);
                cx.notify();
            }
            VimKey::DeleteChar => self.delete_vim_char(window, cx),
            VimKey::DeletePreviousChar => self.delete_vim_previous_char(window, cx),
            VimKey::SubstituteChar => self.substitute_vim_char(window, cx),
            VimKey::ReplaceChar => {
                self.vim_state.state.pending_char = Some(VimPendingChar::Replace);
                cx.notify();
            }
            VimKey::SubstituteLine => {
                let count = self.vim_state.state.take_count();
                self.apply_line_operator(VimOperator::Change, count, window, cx);
            }
            VimKey::YankLine => {
                let count = self.vim_state.state.take_count();
                self.apply_line_operator(VimOperator::Yank, count, window, cx);
            }
            VimKey::JoinLines => self.join_vim_lines(window, cx),
            VimKey::Delete => self.begin_operator(VimOperator::Delete, cx),
            VimKey::Yank => self.begin_operator(VimOperator::Yank, cx),
            VimKey::Change => self.begin_operator(VimOperator::Change, cx),
            VimKey::DeleteToLineEnd => {
                self.apply_direct_operator(VimOperator::Delete, VimKey::LineEnd, window, cx)
            }
            VimKey::ChangeToLineEnd => {
                self.apply_direct_operator(VimOperator::Change, VimKey::LineEnd, window, cx)
            }
            VimKey::PasteAfter => self.paste_vim(false, window, cx),
            VimKey::PasteBefore => self.paste_vim(true, window, cx),
            VimKey::Undo => self.undo_vim_change(window, cx),
            VimKey::Redo => self.redo_vim_change(window, cx),
            VimKey::RepeatLastChange => self.repeat_last_change(window, cx),
            VimKey::Search => self.dispatch_search(window, cx),
            _ => {
                self.vim_state.state.reset_command();
                cx.notify();
            }
        }
    }

    pub(super) fn handle_visual_key(
        &mut self,
        key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(prefix) = self.vim_state.state.pending_text_object.take() {
            if is_text_object_key(key) {
                self.apply_visual_text_object(prefix, key, window, cx);
            } else {
                self.vim_state.state.reset_command();
                cx.notify();
            }
            return;
        }
        if self.vim_state.state.pending_g && key != VimKey::Go {
            self.vim_state.state.reset_command();
            cx.notify();
            return;
        }
        match key {
            VimKey::Digit(0) if self.vim_state.state.count.is_none() => {
                self.apply_motion_key(VimKey::LineStart, window, cx)
            }
            VimKey::Left
            | VimKey::Down
            | VimKey::Up
            | VimKey::Right
            | VimKey::WordForward
            | VimKey::WordBackward
            | VimKey::WordEnd
            | VimKey::BigWordForward
            | VimKey::BigWordBackward
            | VimKey::BigWordEnd
            | VimKey::FirstNonBlank
            | VimKey::LineEnd
            | VimKey::DocumentEnd => self.apply_motion_key(key, window, cx),
            VimKey::FindForward => self.begin_find(VimFindKind::Forward, cx),
            VimKey::FindBackward => self.begin_find(VimFindKind::Backward, cx),
            VimKey::TillForward => self.begin_find(VimFindKind::TillForward, cx),
            VimKey::TillBackward => self.begin_find(VimFindKind::TillBackward, cx),
            VimKey::RepeatFind => self.repeat_find(false, window, cx),
            VimKey::RepeatFindReverse => self.repeat_find(true, window, cx),
            VimKey::Go => {
                if self.vim_state.state.pending_g {
                    self.vim_state.state.pending_g = false;
                    self.apply_motion_key(VimKey::Go, window, cx);
                } else {
                    self.vim_state.state.pending_g = true;
                    cx.notify();
                }
            }
            VimKey::Insert => {
                self.vim_state.state.pending_text_object = Some(VimTextObjectPrefix::Inner);
                cx.notify();
            }
            VimKey::Append => {
                self.vim_state.state.pending_text_object = Some(VimTextObjectPrefix::Around);
                cx.notify();
            }
            VimKey::Visual => {
                if self.vim_state.state.mode == VimMode::Visual {
                    self.enter_vim_normal(window, cx);
                } else {
                    self.vim_state.state.mode = VimMode::Visual;
                    self.vim_state.state.reset_command();
                    cx.notify();
                }
            }
            VimKey::VisualLine => {
                if self.vim_state.state.mode == VimMode::VisualLine {
                    self.enter_vim_normal(window, cx);
                } else {
                    self.vim_state.state.mode = VimMode::VisualLine;
                    self.vim_state.state.reset_command();
                    cx.notify();
                }
            }
            VimKey::Delete | VimKey::DeleteChar | VimKey::DeletePreviousChar => {
                self.apply_visual_operator(VimOperator::Delete, window, cx)
            }
            VimKey::Yank | VimKey::YankLine => {
                self.apply_visual_operator(VimOperator::Yank, window, cx)
            }
            VimKey::Change | VimKey::SubstituteChar | VimKey::SubstituteLine => {
                self.apply_visual_operator(VimOperator::Change, window, cx)
            }
            VimKey::ReplaceChar => {
                self.vim_state.state.pending_char = Some(VimPendingChar::Replace);
                cx.notify();
            }
            VimKey::PasteAfter | VimKey::PasteBefore => self.paste_vim(true, window, cx),
            VimKey::Undo => self.undo_vim_change(window, cx),
            VimKey::Redo => self.redo_vim_change(window, cx),
            VimKey::RepeatLastChange => {
                self.enter_vim_normal(window, cx);
                self.repeat_last_change(window, cx);
            }
            VimKey::Search => self.dispatch_search(window, cx),
            _ => {
                self.vim_state.state.reset_command();
                cx.notify();
            }
        }
    }

    pub(super) fn begin_operator(&mut self, operator: VimOperator, cx: &mut Context<Self>) {
        self.vim_state.state.operator_count = self.vim_state.state.count.take();
        self.vim_state.state.pending_operator = Some(operator);
        self.vim_state.state.pending_g = false;
        cx.notify();
    }

    pub(super) fn handle_pending_operator(
        &mut self,
        operator: VimOperator,
        key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(prefix) = self.vim_state.state.pending_text_object.take() {
            if is_text_object_key(key) {
                let count = combined_operator_count(&mut self.vim_state.state).unwrap_or(1);
                let range = {
                    let editor = self.editor.read(cx);
                    text_object_range(editor.text(), editor.cursor(), count, prefix, key)
                };
                self.apply_operator(operator, range, false, window, cx);
            } else {
                self.vim_state.state.reset_command();
                cx.notify();
            }
            return true;
        }
        if let Some(prefix) = match key {
            VimKey::Insert => Some(VimTextObjectPrefix::Inner),
            VimKey::Append => Some(VimTextObjectPrefix::Around),
            _ => None,
        } {
            self.vim_state.state.pending_text_object = Some(prefix);
            self.vim_state.state.pending_g = false;
            cx.notify();
            return true;
        }
        let key = if key == VimKey::Digit(0) {
            VimKey::LineStart
        } else {
            key
        };
        let repeated = matches!(
            (operator, key),
            (VimOperator::Delete, VimKey::Delete)
                | (VimOperator::Yank, VimKey::Yank)
                | (VimOperator::Change, VimKey::Change)
        );
        if repeated {
            let count = combined_operator_count(&mut self.vim_state.state).unwrap_or(1);
            self.apply_line_operator(operator, count, window, cx);
            return true;
        }

        if key == VimKey::Go && !self.vim_state.state.pending_g {
            self.vim_state.state.pending_g = true;
            cx.notify();
            return true;
        }
        if key == VimKey::Go && self.vim_state.state.pending_g {
            self.vim_state.state.pending_g = false;
            return self.apply_operator_motion(operator, VimKey::Go, window, cx);
        }
        if self.vim_state.state.pending_g {
            self.vim_state.state.reset_command();
            cx.notify();
            return true;
        }

        if let Some(kind) = vim_find_kind_for_key(key) {
            self.vim_state.state.pending_char = Some(VimPendingChar::Find(kind));
            cx.notify();
            return true;
        }
        if matches!(key, VimKey::RepeatFind | VimKey::RepeatFindReverse) {
            return self.apply_operator_repeated_find(
                operator,
                key == VimKey::RepeatFindReverse,
                window,
                cx,
            );
        }

        self.apply_operator_motion(operator, key, window, cx)
    }

    pub(super) fn begin_find(&mut self, kind: VimFindKind, cx: &mut Context<Self>) {
        self.vim_state.state.pending_char = Some(VimPendingChar::Find(kind));
        cx.notify();
    }

    pub(super) fn apply_pending_vim_char(
        &mut self,
        pending: VimPendingChar,
        target: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match pending {
            VimPendingChar::Find(kind) => self.apply_find(kind, target, false, window, cx),
            VimPendingChar::Replace => self.replace_vim_chars(&target, window, cx),
        }
    }

    pub(super) fn apply_find(
        &mut self,
        kind: VimFindKind,
        target: String,
        repeating: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let operator = self.vim_state.state.pending_operator;
        let count = if operator.is_some() {
            combined_operator_count(&mut self.vim_state.state)
        } else {
            self.vim_state.state.count.take()
        };
        let motion = {
            let editor = self.editor.read(cx);
            find_char_motion(
                editor.text(),
                editor.cursor(),
                kind,
                &target,
                count.unwrap_or(1),
                repeating,
            )
        };
        let Some(motion) = motion else {
            self.vim_state.state.reset_command();
            cx.notify();
            return;
        };

        if !repeating {
            self.vim_state.state.last_find = Some(VimLastFind {
                kind,
                target: target.clone(),
            });
        }
        if let Some(operator) = operator {
            let range = {
                let editor = self.editor.read(cx);
                operator_range(editor.text(), editor.cursor(), motion)
            };
            self.apply_operator(operator, range, false, window, cx);
        } else {
            self.set_vim_cursor(motion.target, window, cx);
            self.vim_state.state.reset_command();
            cx.notify();
        }
    }

    pub(super) fn repeat_find(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(last) = self.vim_state.state.last_find.clone() else {
            self.vim_state.state.reset_command();
            cx.notify();
            return;
        };
        let kind = if reverse {
            last.kind.reverse()
        } else {
            last.kind
        };
        self.apply_find(kind, last.target, true, window, cx);
    }

    pub(super) fn apply_operator_repeated_find(
        &mut self,
        operator: VimOperator,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(last) = self.vim_state.state.last_find.clone() else {
            self.vim_state.state.reset_command();
            cx.notify();
            return true;
        };
        let kind = if reverse {
            last.kind.reverse()
        } else {
            last.kind
        };
        self.vim_state.state.pending_operator = Some(operator);
        self.apply_find(kind, last.target, true, window, cx);
        true
    }
}
