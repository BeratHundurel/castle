use super::*;

impl DocumentEditorView {
    pub(crate) fn vim_is_enabled(&self) -> bool {
        self.vim_state.state.enabled()
    }

    pub(crate) fn vim_mode(&self) -> VimMode {
        self.vim_state.state.mode()
    }

    pub(crate) fn vim_context(&self) -> String {
        format!(
            "DocumentEditor vim_mode = {}",
            self.vim_state.state.key_context()
        )
    }

    pub(crate) fn vim_visual_range(&self, cx: &gpui::App) -> Option<Range<usize>> {
        if !self.vim_state.state.enabled || !self.vim_state.state.mode.is_visual() {
            return None;
        }
        let anchor = self.vim_state.state.visual_anchor?;
        let head = self.vim_state.state.visual_head?;
        let editor = self.editor.read(cx);
        if self.vim_state.state.mode == VimMode::VisualLine {
            let start_row = row_at(editor.text(), anchor).min(row_at(editor.text(), head));
            let end_row = row_at(editor.text(), anchor).max(row_at(editor.text(), head));
            Some(line_rows_range(editor.text(), start_row, end_row))
        } else {
            Some(inclusive_range(editor.text(), anchor, head))
        }
    }

    pub(crate) fn finish_vim_visual_edit(
        &mut self,
        cursor: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_vim_cursor(cursor, window, cx);
        self.enter_vim_normal(window, cx);
    }

    pub(crate) fn sync_vim_setting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = AppSettings::editor_vim_mode(cx);
        if self.vim_state.state.enabled == enabled {
            return;
        }

        self.vim_state.state.enabled = enabled;
        self.vim_state.state.mode = if enabled {
            VimMode::Normal
        } else {
            VimMode::Insert
        };
        self.vim_state.state.visual_anchor = None;
        self.vim_state.state.visual_head = None;
        self.vim_state.state.reset_command();
        self.vim_state.search_active = false;
        self.focus_source_mode(window, cx);
        cx.notify();
    }

    pub(crate) fn reset_vim_command(&mut self) {
        self.vim_state.state.reset_command();
        self.vim_state.state.visual_anchor = None;
        self.vim_state.state.visual_head = None;
        self.vim_state.search_active = false;
        if self.vim_state.state.enabled {
            self.vim_state.state.mode = VimMode::Normal;
        }
    }

    pub(crate) fn focus_source_mode(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vim_state.state.enabled && self.vim_state.state.mode != VimMode::Insert {
            self.focus_handle.focus(window, cx);
        } else {
            self.editor
                .update(cx, |editor, cx| editor.focus(window, cx));
        }
    }

    pub(crate) fn on_action_vim_key(
        &mut self,
        action: &VimKeyAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.vim_state.state.enabled || !self.mode.shows_source() {
            return;
        }
        if self.vim_state.state.mode != VimMode::Insert {
            self.focus_handle.focus(window, cx);
        }

        let key = action.0;
        if key == VimKey::Escape {
            self.discard_vim_change_candidate();
            self.enter_vim_normal(window, cx);
            return;
        }
        if let Some(pending) = self.vim_state.state.pending_char.take()
            && let Some(target) = vim_literal_for_key(key)
        {
            let before_mode = self.vim_state.state.mode;
            let before_text = self.editor.read(cx).text().clone();
            let before_cursor = self.editor.read(cx).cursor();
            if !self.vim_state.state.replaying {
                self.vim_state
                    .state
                    .change_candidate
                    .push(VimReplayStep::Literal(target.clone()));
            }
            self.apply_pending_vim_char(pending, target, window, cx);
            if !self.vim_state.state.replaying {
                self.finish_vim_action_recording(before_mode, before_text, before_cursor, cx);
            }
            return;
        }

        let before_mode = self.vim_state.state.mode;
        let before_text = self.editor.read(cx).text().clone();
        let before_cursor = self.editor.read(cx).cursor();
        let record_action = !self.vim_state.state.replaying
            && !matches!(
                key,
                VimKey::RepeatLastChange | VimKey::Undo | VimKey::Redo | VimKey::Search
            );
        if record_action {
            self.prepare_vim_change_candidate(key, cx);
        } else if key != VimKey::RepeatLastChange {
            self.discard_vim_change_candidate();
        }

        if let VimKey::Digit(digit) = key
            && (digit != 0 || self.vim_state.state.count.is_some())
        {
            self.vim_state.state.push_digit(digit);
            cx.notify();
            return;
        }

        if self.vim_state.state.mode.is_visual() {
            self.handle_visual_key(key, window, cx);
        } else {
            self.handle_normal_key(key, window, cx);
        }
        if record_action {
            self.finish_vim_action_recording(before_mode, before_text, before_cursor, cx);
        }
    }

    pub(crate) fn on_vim_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.vim_state.state.enabled
            || !self.mode.shows_source()
            || self.vim_state.state.mode == VimMode::Insert
            || self.vim_state.state.pending_char.is_none()
        {
            return;
        }

        if event.keystroke.key == "escape" {
            self.vim_state.state.reset_command();
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }

        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.function {
            self.vim_state.state.reset_command();
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }

        let target = match event.keystroke.key.as_str() {
            "enter" => Some("\n".to_string()),
            "tab" => Some("\t".to_string()),
            "space" => Some(" ".to_string()),
            _ => event
                .keystroke
                .key_char
                .clone()
                .or_else(|| Some(event.keystroke.key.clone())),
        };
        let Some(target) =
            target.filter(|target| !target.is_empty() && !target.chars().any(|ch| ch.is_control()))
        else {
            self.vim_state.state.reset_command();
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        };

        let Some(pending) = self.vim_state.state.pending_char.take() else {
            return;
        };
        let before_mode = self.vim_state.state.mode;
        let before_text = self.editor.read(cx).text().clone();
        let before_cursor = self.editor.read(cx).cursor();
        if !self.vim_state.state.replaying {
            self.vim_state
                .state
                .change_candidate
                .push(VimReplayStep::Literal(target.clone()));
        }
        self.apply_pending_vim_char(pending, target, window, cx);
        if !self.vim_state.state.replaying {
            self.finish_vim_action_recording(before_mode, before_text, before_cursor, cx);
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    pub(crate) fn on_vim_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.vim_state.state.enabled
            || self.vim_state.state.mode == VimMode::Insert
            || !self.mode.shows_source()
        {
            return;
        }
        let utf16_offset = self.editor.update(cx, |editor, cx| {
            EntityInputHandler::character_index_for_point(editor, event.position, window, cx)
        });
        let Some(utf16_offset) = utf16_offset else {
            return;
        };
        let offset = self
            .editor
            .read(cx)
            .text()
            .offset_utf16_to_offset(utf16_offset);
        self.vim_state.state.mode = VimMode::Normal;
        self.vim_state.state.visual_anchor = None;
        self.vim_state.state.visual_head = None;
        self.vim_state.state.reset_command();
        self.set_vim_cursor(offset, window, cx);
        cx.stop_propagation();
    }

    pub(crate) fn on_action_vim_insert_escape(
        &mut self,
        _: &gpui_component::input::Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.vim_state.state.enabled || !self.mode.shows_source() {
            cx.propagate();
            return;
        }
        if self.vim_state.state.mode == VimMode::Insert && !self.show_emmet_input {
            self.finish_vim_insert_capture(window, cx);
            self.enter_vim_normal(window, cx);
        } else {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
    }
}
