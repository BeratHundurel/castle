use super::*;

impl DocumentEditorView {
    pub(super) fn apply_direct_operator(
        &mut self,
        operator: VimOperator,
        motion_key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.vim_state.state.operator_count = self.vim_state.state.count.take();
        self.vim_state.state.pending_operator = Some(operator);
        _ = self.apply_operator_motion(operator, motion_key, window, cx);
    }

    pub(super) fn apply_operator_motion(
        &mut self,
        operator: VimOperator,
        key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let count = combined_operator_count(&mut self.vim_state.state);
        let motion = {
            let editor = self.editor.read(cx);
            motion_for_key(editor.text(), editor.cursor(), key, count, None)
        };
        let Some(motion) = motion else {
            return false;
        };
        let cursor = self.editor.read(cx).cursor();
        let range = if motion.linewise {
            linewise_motion_range(self.editor.read(cx).text(), cursor, motion.target)
        } else {
            operator_range(self.editor.read(cx).text(), cursor, motion)
        };
        self.apply_operator(operator, range, motion.linewise, window, cx);
        true
    }

    pub(super) fn apply_line_operator(
        &mut self,
        operator: VimOperator,
        count: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = {
            let editor = self.editor.read(cx);
            line_count_range(editor.text(), editor.cursor(), count)
        };
        self.apply_operator(operator, range, true, window, cx);
    }

    pub(super) fn apply_visual_operator(
        &mut self,
        operator: VimOperator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(range) = self.vim_visual_range(cx) else {
            return;
        };
        let linewise = self.vim_state.state.mode == VimMode::VisualLine;
        self.apply_operator(operator, range, linewise, window, cx);
    }

    pub(super) fn apply_visual_text_object(
        &mut self,
        prefix: VimTextObjectPrefix,
        key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.vim_state.state.take_count();
        let range = {
            let editor = self.editor.read(cx);
            text_object_range(editor.text(), editor.cursor(), count, prefix, key)
        };
        if range.is_empty() {
            self.vim_state.state.reset_command();
            cx.notify();
            return;
        }

        self.vim_state.state.mode = VimMode::Visual;
        self.vim_state.state.visual_anchor = Some(range.start);
        self.vim_state.state.visual_head =
            Some(previous_boundary(self.editor.read(cx).text(), range.end));
        self.vim_state.state.reset_command();
        if let Some(head) = self.vim_state.state.visual_head {
            self.set_vim_cursor(head, window, cx);
        }
        cx.notify();
    }

    pub(super) fn apply_operator(
        &mut self,
        operator: VimOperator,
        range: Range<usize>,
        linewise: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if range.is_empty() {
            self.vim_state.state.reset_command();
            return;
        }

        let text = self.editor.read(cx).text().slice(range.clone()).to_string();
        self.vim_state.state.register = Some(VimRegister {
            text: text.clone(),
            linewise,
        });
        let metadata = if linewise {
            VIM_CLIPBOARD_LINEWISE
        } else {
            VIM_CLIPBOARD_CHARACTERWISE
        };
        cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(
            text,
            metadata.to_string(),
        ));

        match operator {
            VimOperator::Yank => {
                self.set_vim_cursor(range.start, window, cx);
                self.enter_vim_normal(window, cx);
            }
            VimOperator::Delete => {
                self.replace_vim_range(range.clone(), "", window, cx);
                self.set_vim_cursor(range.start, window, cx);
                self.enter_vim_normal(window, cx);
            }
            VimOperator::Change => {
                let (replacement, cursor) = if linewise {
                    let register = self
                        .vim_state
                        .state
                        .register
                        .as_ref()
                        .map_or("", |r| &r.text);
                    let indent = leading_indent(register);
                    let replacement = if range.end < self.editor.read(cx).text().len() {
                        let line_break = if register.contains("\r\n") {
                            "\r\n"
                        } else {
                            "\n"
                        };
                        format!("{indent}{line_break}")
                    } else {
                        indent.clone()
                    };
                    let cursor = range.start + indent.len();
                    (replacement, cursor)
                } else {
                    (String::new(), range.start)
                };
                self.replace_vim_range(range.clone(), &replacement, window, cx);
                self.enter_vim_insert(cursor, window, cx);
            }
        }
        self.vim_state.state.reset_command();
    }
}
