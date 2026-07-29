use gpui::{ClipboardItem, Context, EntityInputHandler, Focusable as _, MouseDownEvent, Window};
use gpui_component::input::{Position, Redo, Rope, RopeExt as _, Search, Undo};
use std::ops::Range;

use super::DocumentEditorView;
use super::action::{VimKey, VimKeyAction};
use super::formatting::markdown_newline_prefix;
use super::types::EditorMode;
use crate::app_settings::AppSettings;

const MAX_COUNT: u32 = 999_999;
const VIM_CLIPBOARD_CHARACTERWISE: &str = "castle-vim-characterwise";
const VIM_CLIPBOARD_LINEWISE: &str = "castle-vim-linewise";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum VimMode {
    #[default]
    Normal,
    Insert,
    Visual,
    VisualLine,
}

impl VimMode {
    fn is_visual(self) -> bool {
        matches!(self, Self::Visual | Self::VisualLine)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VimOperator {
    Delete,
    Yank,
    Change,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VimTextObjectPrefix {
    Inner,
    Around,
}

#[derive(Clone, Debug)]
struct VimRegister {
    text: String,
    linewise: bool,
}

#[derive(Clone, Debug)]
pub(super) struct VimState {
    enabled: bool,
    mode: VimMode,
    count: Option<u32>,
    operator_count: Option<u32>,
    pending_operator: Option<VimOperator>,
    pending_g: bool,
    pending_text_object: Option<VimTextObjectPrefix>,
    visual_anchor: Option<usize>,
    visual_head: Option<usize>,
    preferred_column: Option<u32>,
    register: Option<VimRegister>,
}

impl VimState {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            mode: if enabled {
                VimMode::Normal
            } else {
                VimMode::Insert
            },
            count: None,
            operator_count: None,
            pending_operator: None,
            pending_g: false,
            pending_text_object: None,
            visual_anchor: None,
            visual_head: None,
            preferred_column: None,
            register: None,
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn mode(&self) -> VimMode {
        self.mode
    }

    pub(super) fn key_context(&self) -> &'static str {
        match (self.enabled, self.mode) {
            (true, VimMode::Normal) => "normal",
            (true, VimMode::Visual | VimMode::VisualLine) => "visual",
            _ => "insert",
        }
    }

    pub(super) fn command_text(&self) -> String {
        let mut command = String::new();
        if let Some(operator) = self.pending_operator {
            if let Some(count) = self.operator_count {
                command.push_str(&count.to_string());
            }
            command.push(match operator {
                VimOperator::Delete => 'd',
                VimOperator::Yank => 'y',
                VimOperator::Change => 'c',
            });
        }
        if let Some(count) = self.count {
            command.push_str(&count.to_string());
        }
        if self.pending_g {
            command.push('g');
        }
        if let Some(prefix) = self.pending_text_object {
            command.push(match prefix {
                VimTextObjectPrefix::Inner => 'i',
                VimTextObjectPrefix::Around => 'a',
            });
        }
        command
    }

    fn reset_command(&mut self) {
        self.count = None;
        self.operator_count = None;
        self.pending_operator = None;
        self.pending_g = false;
        self.pending_text_object = None;
    }

    fn take_count(&mut self) -> u32 {
        self.count.take().unwrap_or(1)
    }

    fn push_digit(&mut self, digit: u8) {
        let value = self
            .count
            .unwrap_or(0)
            .saturating_mul(10)
            .saturating_add(u32::from(digit))
            .min(MAX_COUNT);
        self.count = Some(value.max(1));
    }
}

#[derive(Clone, Copy)]
struct Motion {
    target: usize,
    inclusive: bool,
    linewise: bool,
}

impl DocumentEditorView {
    pub(super) fn vim_is_enabled(&self) -> bool {
        self.vim.enabled()
    }

    pub(super) fn vim_mode(&self) -> VimMode {
        self.vim.mode()
    }

    pub(super) fn vim_context(&self) -> String {
        format!("DocumentEditor vim_mode = {}", self.vim.key_context())
    }

    pub(super) fn vim_visual_range(&self, cx: &gpui::App) -> Option<Range<usize>> {
        if !self.vim.enabled || !self.vim.mode.is_visual() {
            return None;
        }
        let anchor = self.vim.visual_anchor?;
        let head = self.vim.visual_head?;
        let editor = self.editor.read(cx);
        if self.vim.mode == VimMode::VisualLine {
            let start_row = row_at(editor.text(), anchor).min(row_at(editor.text(), head));
            let end_row = row_at(editor.text(), anchor).max(row_at(editor.text(), head));
            Some(line_rows_range(editor.text(), start_row, end_row))
        } else {
            Some(inclusive_range(editor.text(), anchor, head))
        }
    }

    pub(super) fn finish_vim_visual_edit(
        &mut self,
        cursor: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_vim_cursor(cursor, window, cx);
        self.enter_vim_normal(window, cx);
    }

    pub(super) fn sync_vim_setting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = AppSettings::editor_vim_mode(cx);
        if self.vim.enabled == enabled {
            return;
        }

        self.vim.enabled = enabled;
        self.vim.mode = if enabled {
            VimMode::Normal
        } else {
            VimMode::Insert
        };
        self.vim.visual_anchor = None;
        self.vim.visual_head = None;
        self.vim.reset_command();
        self.vim_search_active = false;
        self.focus_source_mode(window, cx);
        cx.notify();
    }

    pub(super) fn reset_vim_command(&mut self) {
        self.vim.reset_command();
        self.vim.visual_anchor = None;
        self.vim.visual_head = None;
        self.vim_search_active = false;
        if self.vim.enabled {
            self.vim.mode = VimMode::Normal;
        }
    }

    pub(super) fn focus_source_mode(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vim.enabled && self.vim.mode != VimMode::Insert {
            self.focus_handle.focus(window, cx);
        } else {
            self.editor
                .update(cx, |editor, cx| editor.focus(window, cx));
        }
    }

    pub(super) fn on_action_vim_key(
        &mut self,
        action: &VimKeyAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.vim.enabled || self.mode != EditorMode::Source {
            return;
        }
        if self.vim.mode != VimMode::Insert {
            self.focus_handle.focus(window, cx);
        }

        let key = action.0;
        if key == VimKey::Escape {
            self.enter_vim_normal(window, cx);
            return;
        }

        if let VimKey::Digit(digit) = key
            && (digit != 0 || self.vim.count.is_some())
        {
            self.vim.push_digit(digit);
            cx.notify();
            return;
        }

        if self.vim.mode.is_visual() {
            self.handle_visual_key(key, window, cx);
        } else {
            self.handle_normal_key(key, window, cx);
        }
    }

    pub(super) fn on_vim_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.vim.enabled || self.vim.mode == VimMode::Insert || self.mode != EditorMode::Source
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
        self.vim.mode = VimMode::Normal;
        self.vim.visual_anchor = None;
        self.vim.visual_head = None;
        self.vim.reset_command();
        self.set_vim_cursor(offset, window, cx);
        cx.stop_propagation();
    }

    pub(super) fn on_action_vim_insert_escape(
        &mut self,
        _: &gpui_component::input::Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.vim.enabled || self.mode != EditorMode::Source {
            cx.propagate();
            return;
        }
        if self.vim.mode == VimMode::Insert && !self.show_emmet_input {
            self.enter_vim_normal(window, cx);
        } else {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
    }

    pub(super) fn sync_vim_search_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.vim_search_active
            || !self.vim.enabled
            || self.vim.mode == VimMode::Insert
            || self.mode != EditorMode::Source
            || !self.editor.focus_handle(cx).is_focused(window)
        {
            return;
        }

        self.vim_search_active = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn handle_normal_key(&mut self, key: VimKey, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(operator) = self.vim.pending_operator {
            if self.handle_pending_operator(operator, key, window, cx) {
                return;
            }
            self.vim.reset_command();
        }
        if self.vim.pending_g && key != VimKey::Go {
            self.vim.reset_command();
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
            VimKey::Go => {
                if self.vim.pending_g {
                    self.vim.pending_g = false;
                    self.apply_motion_key(VimKey::Go, window, cx);
                } else {
                    self.vim.pending_g = true;
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
                self.vim.mode = VimMode::Visual;
                let cursor = self.editor.read(cx).cursor();
                self.vim.visual_anchor = Some(cursor);
                self.vim.visual_head = Some(cursor);
                self.vim.reset_command();
                self.focus_handle.focus(window, cx);
                cx.notify();
            }
            VimKey::VisualLine => {
                self.vim.mode = VimMode::VisualLine;
                let cursor = self.editor.read(cx).cursor();
                self.vim.visual_anchor = Some(cursor);
                self.vim.visual_head = Some(cursor);
                self.vim.reset_command();
                self.focus_handle.focus(window, cx);
                cx.notify();
            }
            VimKey::DeleteChar => self.delete_vim_char(window, cx),
            VimKey::DeletePreviousChar => self.delete_vim_previous_char(window, cx),
            VimKey::SubstituteChar => self.substitute_vim_char(window, cx),
            VimKey::SubstituteLine => {
                let count = self.vim.take_count();
                self.apply_line_operator(VimOperator::Change, count, window, cx);
            }
            VimKey::YankLine => {
                let count = self.vim.take_count();
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
            VimKey::Undo => self.dispatch_input_action(Box::new(Undo), window, cx),
            VimKey::Redo => self.dispatch_input_action(Box::new(Redo), window, cx),
            VimKey::Search => self.dispatch_search(window, cx),
            _ => {
                self.vim.reset_command();
                cx.notify();
            }
        }
    }

    fn handle_visual_key(&mut self, key: VimKey, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(prefix) = self.vim.pending_text_object.take() {
            if is_text_object_key(key) {
                self.apply_visual_text_object(prefix, key, window, cx);
            } else {
                self.vim.reset_command();
                cx.notify();
            }
            return;
        }
        if self.vim.pending_g && key != VimKey::Go {
            self.vim.reset_command();
            cx.notify();
            return;
        }
        match key {
            VimKey::Digit(0) if self.vim.count.is_none() => {
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
            VimKey::Go => {
                if self.vim.pending_g {
                    self.vim.pending_g = false;
                    self.apply_motion_key(VimKey::Go, window, cx);
                } else {
                    self.vim.pending_g = true;
                    cx.notify();
                }
            }
            VimKey::Insert => {
                self.vim.pending_text_object = Some(VimTextObjectPrefix::Inner);
                cx.notify();
            }
            VimKey::Append => {
                self.vim.pending_text_object = Some(VimTextObjectPrefix::Around);
                cx.notify();
            }
            VimKey::Visual => {
                if self.vim.mode == VimMode::Visual {
                    self.enter_vim_normal(window, cx);
                } else {
                    self.vim.mode = VimMode::Visual;
                    self.vim.reset_command();
                    cx.notify();
                }
            }
            VimKey::VisualLine => {
                if self.vim.mode == VimMode::VisualLine {
                    self.enter_vim_normal(window, cx);
                } else {
                    self.vim.mode = VimMode::VisualLine;
                    self.vim.reset_command();
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
            VimKey::PasteAfter | VimKey::PasteBefore => self.paste_vim(true, window, cx),
            VimKey::Undo => self.dispatch_input_action(Box::new(Undo), window, cx),
            VimKey::Redo => self.dispatch_input_action(Box::new(Redo), window, cx),
            VimKey::Search => self.dispatch_search(window, cx),
            _ => {
                self.vim.reset_command();
                cx.notify();
            }
        }
    }

    fn begin_operator(&mut self, operator: VimOperator, cx: &mut Context<Self>) {
        self.vim.operator_count = self.vim.count.take();
        self.vim.pending_operator = Some(operator);
        self.vim.pending_g = false;
        cx.notify();
    }

    fn handle_pending_operator(
        &mut self,
        operator: VimOperator,
        key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(prefix) = self.vim.pending_text_object.take() {
            if is_text_object_key(key) {
                let count = combined_operator_count(&mut self.vim);
                let range = {
                    let editor = self.editor.read(cx);
                    text_object_range(editor.text(), editor.cursor(), count, prefix, key)
                };
                self.apply_operator(operator, range, false, window, cx);
            } else {
                self.vim.reset_command();
                cx.notify();
            }
            return true;
        }
        if let Some(prefix) = match key {
            VimKey::Insert => Some(VimTextObjectPrefix::Inner),
            VimKey::Append => Some(VimTextObjectPrefix::Around),
            _ => None,
        } {
            self.vim.pending_text_object = Some(prefix);
            self.vim.pending_g = false;
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
            let count = combined_operator_count(&mut self.vim);
            self.apply_line_operator(operator, count, window, cx);
            return true;
        }

        if key == VimKey::Go && !self.vim.pending_g {
            self.vim.pending_g = true;
            cx.notify();
            return true;
        }
        if key == VimKey::Go && self.vim.pending_g {
            self.vim.pending_g = false;
            return self.apply_operator_motion(operator, VimKey::Go, window, cx);
        }
        if self.vim.pending_g {
            self.vim.reset_command();
            cx.notify();
            return true;
        }

        self.apply_operator_motion(operator, key, window, cx)
    }

    fn apply_direct_operator(
        &mut self,
        operator: VimOperator,
        motion_key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.vim.operator_count = self.vim.count.take();
        self.vim.pending_operator = Some(operator);
        _ = self.apply_operator_motion(operator, motion_key, window, cx);
    }

    fn apply_operator_motion(
        &mut self,
        operator: VimOperator,
        key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let count = combined_operator_count(&mut self.vim);
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

    fn apply_line_operator(
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

    fn apply_visual_operator(
        &mut self,
        operator: VimOperator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(range) = self.vim_visual_range(cx) else {
            return;
        };
        let linewise = self.vim.mode == VimMode::VisualLine;
        self.apply_operator(operator, range, linewise, window, cx);
    }

    fn apply_visual_text_object(
        &mut self,
        prefix: VimTextObjectPrefix,
        key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.vim.take_count();
        let range = {
            let editor = self.editor.read(cx);
            text_object_range(editor.text(), editor.cursor(), count, prefix, key)
        };
        if range.is_empty() {
            self.vim.reset_command();
            cx.notify();
            return;
        }

        self.vim.mode = VimMode::Visual;
        self.vim.visual_anchor = Some(range.start);
        self.vim.visual_head = Some(previous_boundary(self.editor.read(cx).text(), range.end));
        self.vim.reset_command();
        if let Some(head) = self.vim.visual_head {
            self.set_vim_cursor(head, window, cx);
        }
        cx.notify();
    }

    fn apply_operator(
        &mut self,
        operator: VimOperator,
        range: Range<usize>,
        linewise: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if range.is_empty() {
            self.vim.reset_command();
            return;
        }

        let text = self.editor.read(cx).text().slice(range.clone()).to_string();
        self.vim.register = Some(VimRegister {
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
                    let register = self.vim.register.as_ref().map_or("", |r| &r.text);
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
        self.vim.reset_command();
    }

    fn apply_motion_key(&mut self, key: VimKey, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.vim.take_count();
        let preferred = self.vim.preferred_column;
        let motion = {
            let editor = self.editor.read(cx);
            motion_for_key(editor.text(), editor.cursor(), key, count, preferred)
        };
        let Some(motion) = motion else {
            self.vim.reset_command();
            return;
        };

        if matches!(key, VimKey::Up | VimKey::Down) {
            if self.vim.preferred_column.is_none() {
                self.vim.preferred_column = Some(
                    self.editor
                        .read(cx)
                        .text()
                        .offset_to_position(self.editor.read(cx).cursor())
                        .character,
                );
            }
        } else {
            self.vim.preferred_column = None;
        }
        self.vim.pending_g = false;
        self.set_vim_cursor(motion.target, window, cx);
        cx.notify();
    }

    fn delete_vim_char(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.vim.take_count();
        let range = {
            let editor = self.editor.read(cx);
            forward_char_range(editor.text(), editor.cursor(), count)
        };
        self.apply_operator(VimOperator::Delete, range, false, window, cx);
    }

    fn delete_vim_previous_char(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.vim.take_count();
        let range = {
            let editor = self.editor.read(cx);
            backward_char_range(editor.text(), editor.cursor(), count)
        };
        self.apply_operator(VimOperator::Delete, range, false, window, cx);
    }

    fn substitute_vim_char(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.vim.take_count();
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

    fn join_vim_lines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.vim.take_count().max(2);
        let edit = {
            let editor = self.editor.read(cx);
            join_line_edit(editor.text(), editor.cursor(), count)
        };
        let Some((range, replacement)) = edit else {
            self.vim.reset_command();
            cx.notify();
            return;
        };
        let cursor = range.start;
        self.replace_vim_range(range, &replacement, window, cx);
        self.set_vim_cursor(cursor, window, cx);
        self.enter_vim_normal(window, cx);
    }

    fn paste_vim(&mut self, before: bool, window: &mut Window, cx: &mut Context<Self>) {
        let clipboard_register = cx.read_from_clipboard().and_then(|item| {
            let linewise = item
                .metadata()
                .is_some_and(|metadata| metadata.as_str() == VIM_CLIPBOARD_LINEWISE);
            item.text().map(|text| VimRegister { text, linewise })
        });
        let register = clipboard_register
            .or_else(|| self.vim.register.clone())
            .unwrap_or_else(|| VimRegister {
                text: String::new(),
                linewise: false,
            });
        if register.text.is_empty() {
            return;
        }
        let count = self.vim.take_count() as usize;
        let replacement = if register.linewise {
            let mut text = register.text;
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.repeat(count)
        } else {
            register.text.repeat(count)
        };

        if self.vim.mode.is_visual() {
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

    fn open_vim_line(&mut self, above: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (offset, prefix, insertion) = {
            let editor = self.editor.read(cx);
            let rope = editor.text();
            let row = row_at(rope, editor.cursor());
            let line = rope.slice_line(row).to_string();
            let prefix = if above {
                leading_indent(&line)
            } else {
                markdown_newline_prefix(&line)
            };
            if above {
                (
                    rope.line_start_offset(row),
                    prefix.clone(),
                    format!("{prefix}\n"),
                )
            } else {
                let end = line_content_end(rope, row);
                (end, prefix.clone(), format!("\n{prefix}"))
            }
        };
        self.replace_vim_range(offset..offset, &insertion, window, cx);
        let cursor = if above {
            offset + prefix.len()
        } else {
            offset + 1 + prefix.len()
        };
        self.enter_vim_insert(cursor, window, cx);
    }

    fn enter_vim_insert_at_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.editor.read(cx).cursor();
        self.enter_vim_insert(cursor, window, cx);
    }

    fn enter_vim_insert(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.vim.mode = VimMode::Insert;
        self.vim.visual_anchor = None;
        self.vim.visual_head = None;
        self.vim.reset_command();
        self.set_input_cursor(offset, window, cx);
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
        cx.notify();
    }

    fn enter_vim_normal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_insert = self.vim.mode == VimMode::Insert;
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
        self.vim.mode = VimMode::Normal;
        self.vim.visual_anchor = None;
        self.vim.visual_head = None;
        self.vim.preferred_column = None;
        self.vim.reset_command();
        self.set_vim_cursor(target, window, cx);
        cx.notify();
    }

    fn set_vim_cursor(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        let offset = {
            let editor = self.editor.read(cx);
            clamp_normal_offset(editor.text(), offset)
        };
        if self.vim.mode.is_visual() {
            self.vim.visual_head = Some(offset);
        }
        self.set_input_cursor(offset, window, cx);
        self.focus_handle.focus(window, cx);
    }

    fn set_input_cursor(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            let offset = offset.min(editor.text().len());
            let position = editor.text().offset_to_position(offset);
            editor.set_cursor_position(position, window, cx);
        });
    }

    fn replace_vim_range(
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

    fn dispatch_input_action(
        &mut self,
        action: Box<dyn gpui::Action>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
        window.dispatch_action(action, cx);
        self.focus_handle.focus(window, cx);
        self.vim.reset_command();
        cx.notify();
    }

    fn dispatch_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.vim_search_active = false;
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
        window.dispatch_action(Box::new(Search), cx);
        self.vim_search_active = true;
        self.vim.reset_command();
        cx.notify();
    }
}

fn combined_operator_count(vim: &mut VimState) -> u32 {
    let operator = vim.operator_count.take().unwrap_or(1);
    let motion = vim.take_count();
    vim.pending_operator = None;
    operator.saturating_mul(motion).min(MAX_COUNT)
}

fn motion_for_key(
    rope: &Rope,
    cursor: usize,
    key: VimKey,
    count: u32,
    preferred_column: Option<u32>,
) -> Option<Motion> {
    let target = match key {
        VimKey::Left => {
            let line_start = rope.line_start_offset(row_at(rope, cursor));
            repeat_motion(cursor, count, |offset| {
                previous_boundary(rope, offset).max(line_start)
            })
        }
        VimKey::Right => repeat_motion(cursor, count, |offset| {
            next_boundary(rope, offset).min(normal_line_end(rope, row_at(rope, cursor)))
        }),
        VimKey::Down | VimKey::Up => {
            let row = row_at(rope, cursor);
            let delta = if key == VimKey::Down {
                i64::from(count)
            } else {
                -i64::from(count)
            };
            let target_row =
                (row as i64 + delta).clamp(0, rope.lines_len().saturating_sub(1) as i64) as usize;
            let column =
                preferred_column.unwrap_or_else(|| rope.offset_to_position(cursor).character);
            let target = rope.position_to_offset(&Position::new(target_row as u32, column));
            target.min(normal_line_end(rope, target_row))
        }
        VimKey::WordForward => repeat_motion(cursor, count, |offset| next_word_start(rope, offset)),
        VimKey::WordBackward => {
            repeat_motion(cursor, count, |offset| previous_word_start(rope, offset))
        }
        VimKey::WordEnd => repeat_motion(cursor, count, |offset| word_end(rope, offset)),
        VimKey::BigWordForward => {
            repeat_motion(cursor, count, |offset| next_big_word_start(rope, offset))
        }
        VimKey::BigWordBackward => repeat_motion(cursor, count, |offset| {
            previous_big_word_start(rope, offset)
        }),
        VimKey::BigWordEnd => repeat_motion(cursor, count, |offset| big_word_end(rope, offset)),
        VimKey::LineStart => rope.line_start_offset(row_at(rope, cursor)),
        VimKey::FirstNonBlank => first_non_blank(rope, cursor),
        VimKey::LineEnd => {
            let row = row_at(rope, cursor)
                .saturating_add(count as usize)
                .saturating_sub(1)
                .min(rope.lines_len().saturating_sub(1));
            normal_line_end(rope, row)
        }
        VimKey::Go => {
            let row = if count > 1 {
                (count as usize - 1).min(rope.lines_len().saturating_sub(1))
            } else {
                0
            };
            rope.line_start_offset(row)
        }
        VimKey::DocumentEnd => {
            let row = if count > 1 {
                (count as usize - 1).min(rope.lines_len().saturating_sub(1))
            } else {
                rope.lines_len().saturating_sub(1)
            };
            rope.line_start_offset(row)
        }
        _ => return None,
    };
    Some(Motion {
        target,
        inclusive: matches!(key, VimKey::WordEnd | VimKey::BigWordEnd | VimKey::LineEnd),
        linewise: matches!(
            key,
            VimKey::Down | VimKey::Up | VimKey::Go | VimKey::DocumentEnd
        ),
    })
}

fn repeat_motion(mut offset: usize, count: u32, mut motion: impl FnMut(usize) -> usize) -> usize {
    for _ in 0..count {
        let next = motion(offset);
        if next == offset {
            break;
        }
        offset = next;
    }
    offset
}

fn operator_range(rope: &Rope, cursor: usize, motion: Motion) -> Range<usize> {
    if motion.target >= cursor {
        let end = if motion.inclusive {
            next_boundary(rope, motion.target)
        } else {
            motion.target
        };
        cursor..end
    } else {
        motion.target..cursor
    }
}

fn linewise_motion_range(rope: &Rope, cursor: usize, target: usize) -> Range<usize> {
    let start_row = row_at(rope, cursor).min(row_at(rope, target));
    let end_row = row_at(rope, cursor).max(row_at(rope, target));
    line_rows_range(rope, start_row, end_row)
}

fn line_count_range(rope: &Rope, cursor: usize, count: u32) -> Range<usize> {
    let start_row = row_at(rope, cursor);
    let end_row = start_row
        .saturating_add(count as usize)
        .saturating_sub(1)
        .min(rope.lines_len().saturating_sub(1));
    line_rows_range(rope, start_row, end_row)
}

fn forward_char_range(rope: &Rope, cursor: usize, count: u32) -> Range<usize> {
    let start = cursor;
    let line_end = line_content_end(rope, row_at(rope, start));
    let end = repeat_motion(start, count, |offset| {
        next_boundary(rope, offset).min(line_end)
    });
    start..end
}

fn backward_char_range(rope: &Rope, cursor: usize, count: u32) -> Range<usize> {
    let line_start = rope.line_start_offset(row_at(rope, cursor));
    let start = repeat_motion(cursor, count, |offset| {
        previous_boundary(rope, offset).max(line_start)
    });
    start..cursor
}

fn join_line_edit(rope: &Rope, cursor: usize, count: u32) -> Option<(Range<usize>, String)> {
    let start_row = row_at(rope, cursor);
    if start_row + 1 >= rope.lines_len() {
        return None;
    }
    let end_row = start_row
        .saturating_add(count.max(2) as usize)
        .saturating_sub(1)
        .min(rope.lines_len().saturating_sub(1));

    let current_start = rope.line_start_offset(start_row);
    let mut range_start = line_content_end(rope, start_row);
    while range_start > current_start
        && rope
            .char_at(previous_boundary(rope, range_start))
            .is_some_and(|ch| matches!(ch, ' ' | '\t'))
    {
        range_start = previous_boundary(rope, range_start);
    }

    let range_end = line_content_end(rope, end_row);
    let mut joined = String::new();
    for row in start_row + 1..=end_row {
        let line = rope
            .slice(rope.line_start_offset(row)..line_content_end(rope, row))
            .to_string();
        let line = line.trim_matches([' ', '\t']);
        if line.is_empty() {
            continue;
        }
        if !joined.is_empty() {
            joined.push(' ');
        }
        joined.push_str(line);
    }
    if range_start > current_start && !joined.is_empty() {
        joined.insert(0, ' ');
    }
    Some((range_start..range_end, joined))
}

fn line_rows_range(rope: &Rope, start_row: usize, end_row: usize) -> Range<usize> {
    let start = rope.line_start_offset(start_row);
    let end = if end_row + 1 < rope.lines_len() {
        rope.line_start_offset(end_row + 1)
    } else {
        rope.len()
    };
    start..end
}

fn inclusive_range(rope: &Rope, anchor: usize, head: usize) -> Range<usize> {
    let start = anchor.min(head);
    let end = next_boundary(rope, anchor.max(head));
    start..end
}

fn row_at(rope: &Rope, offset: usize) -> usize {
    rope.offset_to_point(offset.min(rope.len())).row
}

fn line_content_end(rope: &Rope, row: usize) -> usize {
    let start = rope.line_start_offset(row);
    let mut end = rope.line_end_offset(row).min(rope.len());
    if end > start && rope.char_at(previous_boundary(rope, end)) == Some('\r') {
        end = previous_boundary(rope, end);
    }
    end
}

fn normal_line_end(rope: &Rope, row: usize) -> usize {
    let start = rope.line_start_offset(row);
    let end = line_content_end(rope, row);
    if end > start {
        previous_boundary(rope, end)
    } else {
        start
    }
}

fn first_non_blank(rope: &Rope, cursor: usize) -> usize {
    let row = row_at(rope, cursor);
    let start = rope.line_start_offset(row);
    let end = line_content_end(rope, row);
    let mut offset = start;
    while offset < end {
        match rope.char_at(offset) {
            Some(' ' | '\t') => offset = next_boundary(rope, offset),
            _ => break,
        }
    }
    if offset == end { start } else { offset }
}

fn clamp_normal_offset(rope: &Rope, offset: usize) -> usize {
    let offset = offset.min(rope.len());
    let row = row_at(rope, offset);
    let start = rope.line_start_offset(row);
    let end = line_content_end(rope, row);
    if end > start {
        offset.min(previous_boundary(rope, end))
    } else {
        start
    }
}

fn next_boundary(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len() {
        return rope.len();
    }
    rope.char_at(offset)
        .map_or(rope.len(), |ch| (offset + ch.len_utf8()).min(rope.len()))
}

fn previous_boundary(rope: &Rope, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    rope.floor_char_boundary(offset.min(rope.len()).saturating_sub(1))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Space,
    Word,
    Punctuation,
}

fn char_class(ch: char) -> CharClass {
    if ch.is_whitespace() {
        CharClass::Space
    } else if ch.is_alphanumeric() || ch == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

fn next_word_start(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len() {
        return rope.len();
    }
    let mut cursor = offset;
    let class = rope.char_at(cursor).map(char_class);
    while cursor < rope.len() && rope.char_at(cursor).map(char_class) == class {
        cursor = next_boundary(rope, cursor);
    }
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = next_boundary(rope, cursor);
    }
    cursor
}

fn previous_word_start(rope: &Rope, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let mut cursor = previous_boundary(rope, offset);
    while cursor > 0 && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = previous_boundary(rope, cursor);
    }
    let class = rope.char_at(cursor).map(char_class);
    while cursor > 0 {
        let previous = previous_boundary(rope, cursor);
        if rope.char_at(previous).map(char_class) != class {
            break;
        }
        cursor = previous;
    }
    cursor
}

fn word_end(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len() {
        return rope.len();
    }
    let mut cursor = offset;
    if let Some(class) = rope.char_at(cursor).map(char_class) {
        let next = next_boundary(rope, cursor);
        if next < rope.len() && rope.char_at(next).map(char_class) != Some(class) {
            cursor = next;
        }
    }
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = next_boundary(rope, cursor);
    }
    let class = rope.char_at(cursor).map(char_class);
    let mut end = cursor;
    while cursor < rope.len() && rope.char_at(cursor).map(char_class) == class {
        end = cursor;
        cursor = next_boundary(rope, cursor);
    }
    end
}

fn next_big_word_start(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len() {
        return rope.len();
    }
    let mut cursor = offset;
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(|ch| !ch.is_whitespace()) {
        cursor = next_boundary(rope, cursor);
    }
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = next_boundary(rope, cursor);
    }
    cursor
}

fn previous_big_word_start(rope: &Rope, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let mut cursor = previous_boundary(rope, offset);
    while cursor > 0 && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = previous_boundary(rope, cursor);
    }
    while cursor > 0 {
        let previous = previous_boundary(rope, cursor);
        if rope.char_at(previous).is_some_and(char::is_whitespace) {
            break;
        }
        cursor = previous;
    }
    cursor
}

fn big_word_end(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len() {
        return rope.len();
    }
    let mut cursor = offset;
    let next = next_boundary(rope, cursor);
    if next < rope.len()
        && rope.char_at(cursor).is_some_and(|ch| !ch.is_whitespace())
        && rope.char_at(next).is_some_and(char::is_whitespace)
    {
        cursor = next;
    }
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = next_boundary(rope, cursor);
    }
    let mut end = cursor;
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(|ch| !ch.is_whitespace()) {
        end = cursor;
        cursor = next_boundary(rope, cursor);
    }
    end
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextObjectClass {
    HorizontalSpace,
    LineBreak,
    Word,
    Punctuation,
}

fn text_object_class(ch: char) -> TextObjectClass {
    if matches!(ch, '\r' | '\n') {
        TextObjectClass::LineBreak
    } else if ch.is_whitespace() {
        TextObjectClass::HorizontalSpace
    } else if ch.is_alphanumeric() || ch == '_' {
        TextObjectClass::Word
    } else {
        TextObjectClass::Punctuation
    }
}

fn text_object_run(rope: &Rope, offset: usize) -> Range<usize> {
    if rope.len() == 0 {
        return 0..0;
    }
    let offset = if offset >= rope.len() {
        previous_boundary(rope, rope.len())
    } else {
        rope.floor_char_boundary(offset)
    };
    let Some(class) = rope.char_at(offset).map(text_object_class) else {
        return offset..offset;
    };
    if class == TextObjectClass::LineBreak {
        let ch = rope.char_at(offset);
        let start = if ch == Some('\n') && offset > 0 {
            let previous = previous_boundary(rope, offset);
            if rope.char_at(previous) == Some('\r') {
                previous
            } else {
                offset
            }
        } else {
            offset
        };
        let mut end = next_boundary(rope, offset);
        if ch == Some('\r') && rope.char_at(end) == Some('\n') {
            end = next_boundary(rope, end);
        }
        return start..end;
    }

    let mut start = offset;
    while start > 0 {
        let previous = previous_boundary(rope, start);
        if rope.char_at(previous).map(text_object_class) != Some(class) {
            break;
        }
        start = previous;
    }

    let mut end = next_boundary(rope, offset);
    while end < rope.len() && rope.char_at(end).map(text_object_class) == Some(class) {
        end = next_boundary(rope, end);
    }
    start..end
}

fn next_non_space_run(rope: &Rope, offset: usize) -> Option<Range<usize>> {
    let mut cursor = offset;
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(|ch| ch.is_whitespace()) {
        cursor = next_boundary(rope, cursor);
    }
    (cursor < rope.len()).then(|| text_object_run(rope, cursor))
}

fn previous_non_space_run(rope: &Rope, offset: usize) -> Option<Range<usize>> {
    if offset == 0 {
        return None;
    }
    let mut cursor = previous_boundary(rope, offset);
    while cursor > 0 && rope.char_at(cursor).is_some_and(|ch| ch.is_whitespace()) {
        cursor = previous_boundary(rope, cursor);
    }
    if rope.char_at(cursor).is_some_and(|ch| ch.is_whitespace()) {
        None
    } else {
        Some(text_object_run(rope, cursor))
    }
}

fn extend_through_word_runs(
    rope: &Rope,
    mut range: Range<usize>,
    additional_runs: u32,
) -> Range<usize> {
    for _ in 0..additional_runs {
        let Some(next) = next_non_space_run(rope, range.end) else {
            break;
        };
        range.end = next.end;
    }
    range
}

fn word_text_object_range(
    rope: &Rope,
    cursor: usize,
    count: u32,
    prefix: VimTextObjectPrefix,
) -> Range<usize> {
    let run = text_object_run(rope, cursor);
    if run.is_empty() {
        return run;
    }
    let class = rope.char_at(run.start).map(text_object_class);
    let count = count.max(1);

    if prefix == VimTextObjectPrefix::Inner {
        return extend_through_word_runs(rope, run, count.saturating_sub(1));
    }

    if matches!(
        class,
        Some(TextObjectClass::HorizontalSpace | TextObjectClass::LineBreak)
    ) {
        if let Some(next) = next_non_space_run(rope, run.end) {
            return extend_through_word_runs(rope, run.start..next.end, count.saturating_sub(1));
        }
        if let Some(previous) = previous_non_space_run(rope, run.start) {
            return previous.start..run.end;
        }
        return run;
    }

    let mut range = extend_through_word_runs(rope, run, count.saturating_sub(1));
    let mut trailing = range.end;
    while trailing < rope.len()
        && rope
            .char_at(trailing)
            .is_some_and(|ch| ch.is_whitespace() && !matches!(ch, '\r' | '\n'))
    {
        trailing = next_boundary(rope, trailing);
    }
    if trailing > range.end {
        range.end = trailing;
        return range;
    }

    let mut leading = range.start;
    while leading > 0 {
        let previous = previous_boundary(rope, leading);
        let Some(ch) = rope.char_at(previous) else {
            break;
        };
        if !ch.is_whitespace() || matches!(ch, '\r' | '\n') {
            break;
        }
        leading = previous;
    }
    range.start = leading;
    range
}

fn is_text_object_key(key: VimKey) -> bool {
    matches!(
        key,
        VimKey::WordForward
            | VimKey::DoubleQuote
            | VimKey::SingleQuote
            | VimKey::Backtick
            | VimKey::Parenthesis
            | VimKey::Bracket
            | VimKey::Brace
    )
}

fn text_object_range(
    rope: &Rope,
    cursor: usize,
    count: u32,
    prefix: VimTextObjectPrefix,
    key: VimKey,
) -> Range<usize> {
    match key {
        VimKey::WordForward => word_text_object_range(rope, cursor, count, prefix),
        VimKey::DoubleQuote => quote_text_object_range(rope, cursor, prefix, '"'),
        VimKey::SingleQuote => quote_text_object_range(rope, cursor, prefix, '\''),
        VimKey::Backtick => quote_text_object_range(rope, cursor, prefix, '`'),
        VimKey::Parenthesis => pair_text_object_range(rope, cursor, prefix, '(', ')'),
        VimKey::Bracket => pair_text_object_range(rope, cursor, prefix, '[', ']'),
        VimKey::Brace => pair_text_object_range(rope, cursor, prefix, '{', '}'),
        _ => cursor..cursor,
    }
}

fn quote_text_object_range(
    rope: &Rope,
    cursor: usize,
    prefix: VimTextObjectPrefix,
    quote: char,
) -> Range<usize> {
    let row = row_at(rope, cursor);
    let line_start = rope.line_start_offset(row);
    let line_end = line_content_end(rope, row);
    let mut opening = None;
    let mut offset = line_start;
    while offset < line_end {
        if rope.char_at(offset) == Some(quote) && !is_escaped(rope, offset, line_start) {
            if let Some(start) = opening.take() {
                if cursor >= start && cursor <= offset {
                    return if prefix == VimTextObjectPrefix::Inner {
                        next_boundary(rope, start)..offset
                    } else {
                        start..next_boundary(rope, offset)
                    };
                }
            } else {
                opening = Some(offset);
            }
        }
        offset = next_boundary(rope, offset);
    }
    cursor..cursor
}

fn is_escaped(rope: &Rope, offset: usize, line_start: usize) -> bool {
    let mut slash_count = 0;
    let mut scan = offset;
    while scan > line_start {
        scan = previous_boundary(rope, scan);
        if rope.char_at(scan) != Some('\\') {
            break;
        }
        slash_count += 1;
    }
    slash_count % 2 == 1
}

fn pair_text_object_range(
    rope: &Rope,
    cursor: usize,
    prefix: VimTextObjectPrefix,
    open: char,
    close: char,
) -> Range<usize> {
    if rope.len() == 0 {
        return 0..0;
    }
    let mut offset = if cursor >= rope.len() {
        previous_boundary(rope, rope.len())
    } else {
        rope.floor_char_boundary(cursor)
    };
    let mut depth = 0_u32;
    let opening = loop {
        match rope.char_at(offset) {
            Some(ch) if ch == close => depth = depth.saturating_add(1),
            Some(ch) if ch == open => {
                if depth == 0 {
                    break Some(offset);
                }
                depth -= 1;
                if depth == 0 {
                    break Some(offset);
                }
            }
            _ => {}
        }
        if offset == 0 {
            break None;
        }
        offset = previous_boundary(rope, offset);
    };
    let Some(opening) = opening else {
        return cursor..cursor;
    };

    depth = 0;
    offset = next_boundary(rope, opening);
    let closing = loop {
        if offset >= rope.len() {
            break None;
        }
        match rope.char_at(offset) {
            Some(ch) if ch == open => depth = depth.saturating_add(1),
            Some(ch) if ch == close => {
                if depth == 0 {
                    break Some(offset);
                }
                depth -= 1;
            }
            _ => {}
        }
        offset = next_boundary(rope, offset);
    };
    let Some(closing) = closing else {
        return cursor..cursor;
    };

    if prefix == VimTextObjectPrefix::Inner {
        next_boundary(rope, opening)..closing
    } else {
        opening..next_boundary(rope, closing)
    }
}

fn leading_indent(text: &str) -> String {
    text.chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DB, app_settings::AppSettings, test_alloc};
    use entity::note;
    use gpui::AppContext as _;
    use gpui_component::input::InputEvent;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};
    use std::{cell::Cell, path::PathBuf, rc::Rc, sync::Arc};

    #[test]
    fn word_motions_distinguish_words_whitespace_and_punctuation() {
        let rope = Rope::from("one  two...three");
        assert_eq!(next_word_start(&rope, 0), 5);
        assert_eq!(next_word_start(&rope, 5), 8);
        assert_eq!(next_word_start(&rope, 8), 11);
        assert_eq!(previous_word_start(&rope, 16), 11);
        assert_eq!(word_end(&rope, 5), 7);
    }

    #[test]
    fn big_word_motions_treat_punctuation_as_part_of_a_word() {
        let text = "one.two\t中-文  last";
        let rope = Rope::from(text);
        let middle = text.find("中-文").expect("middle WORD should be present");
        let last = text.find("last").expect("last WORD should be present");

        assert_eq!(next_big_word_start(&rope, 0), middle);
        assert_eq!(next_big_word_start(&rope, middle), last);
        assert_eq!(previous_big_word_start(&rope, last), middle);
        assert_eq!(previous_big_word_start(&rope, middle), 0);
        assert_eq!(big_word_end(&rope, 0), "one.two".len() - 1);
        assert_eq!(
            big_word_end(&rope, "one.two".len() - 1),
            middle + "中-".len()
        );
        assert_eq!(big_word_end(&rope, last), text.len() - 1);
    }

    #[test]
    fn character_ranges_are_unicode_safe_and_stop_at_line_boundaries() {
        let rope = Rope::from("a中b\nxy");
        assert_eq!(forward_char_range(&rope, 1, 2), 1..5);
        assert_eq!(forward_char_range(&rope, 4, 99), 4..5);
        assert_eq!(backward_char_range(&rope, 4, 1), 1..4);
        assert_eq!(backward_char_range(&rope, 4, 99), 0..4);
        assert_eq!(backward_char_range(&rope, 6, 2), 6..6);
    }

    #[test]
    fn line_join_edits_trim_indent_and_handle_crlf_blank_and_final_lines() {
        let crlf = Rope::from("one  \r\n\t two \r\n中");
        assert_eq!(
            join_line_edit(&crlf, 0, 2),
            Some((3..13, " two".to_string()))
        );
        assert_eq!(
            join_line_edit(&crlf, 0, 3),
            Some((3..18, " two 中".to_string()))
        );

        let blank = Rope::from("one\n\n  two\n");
        assert_eq!(
            join_line_edit(&blank, 0, 3),
            Some((3..10, " two".to_string()))
        );
        assert_eq!(join_line_edit(&blank, blank.len(), 2), None);

        let empty_first = Rope::from("  \n next");
        assert_eq!(
            join_line_edit(&empty_first, 0, 2),
            Some((0..8, "next".to_string()))
        );
    }

    #[test]
    fn inner_word_selects_only_the_run_under_the_cursor() {
        let text = "Further testing showed naïve 中文... results";
        let rope = Rope::from(text);
        let testing = text.find("testing").expect("testing should be present");
        let naive = text.find("naïve").expect("naïve should be present");
        let chinese = text.find("中文").expect("Chinese word should be present");
        let punctuation = text.find("...").expect("punctuation should be present");

        let cases = [
            (0, 0.."Further".len()),
            (3, 0.."Further".len()),
            ("Further".len(), "Further".len()..testing),
            (testing + 2, testing..testing + "testing".len()),
            (naive + 2, naive..naive + "naïve".len()),
            (chinese + 3, chinese..chinese + "中文".len()),
            (punctuation + 1, punctuation..punctuation + 3),
        ];

        for (cursor, expected) in cases {
            assert_eq!(
                word_text_object_range(&rope, cursor, 1, VimTextObjectPrefix::Inner),
                expected,
                "unexpected inner-word range at byte offset {cursor}"
            );
        }
    }

    #[test]
    fn inner_word_counts_include_intervening_space_without_the_following_word() {
        let rope = Rope::from("Further testing showed");
        assert_eq!(
            word_text_object_range(&rope, 0, 2, VimTextObjectPrefix::Inner),
            0.."Further testing".len()
        );
        assert_eq!(
            word_text_object_range(&rope, 7, 1, VimTextObjectPrefix::Inner),
            7..8
        );
    }

    #[test]
    fn around_word_prefers_trailing_space_then_falls_back_to_leading_space() {
        let text = "Further testing";
        let rope = Rope::from(text);
        let testing = text.find("testing").expect("testing should be present");
        assert_eq!(
            word_text_object_range(&rope, 2, 1, VimTextObjectPrefix::Around),
            0..testing
        );
        assert_eq!(
            word_text_object_range(&rope, testing, 1, VimTextObjectPrefix::Around),
            "Further".len()..text.len()
        );
        assert_eq!(
            word_text_object_range(&rope, "Further".len(), 1, VimTextObjectPrefix::Around),
            "Further".len()..text.len()
        );
    }

    #[test]
    fn word_text_objects_do_not_merge_line_breaks_with_horizontal_space() {
        let rope = Rope::from("one  \r\n\n\ttwo");
        assert_eq!(
            word_text_object_range(&rope, 3, 1, VimTextObjectPrefix::Inner),
            3..5
        );
        assert_eq!(
            word_text_object_range(&rope, 5, 1, VimTextObjectPrefix::Inner),
            5..7
        );
        assert_eq!(
            word_text_object_range(&rope, 7, 1, VimTextObjectPrefix::Inner),
            7..8
        );
        assert_eq!(
            word_text_object_range(&rope, 8, 1, VimTextObjectPrefix::Inner),
            8..9
        );
    }

    #[test]
    fn quote_text_objects_handle_around_inner_unicode_and_escapes() {
        let text = "say \"Further \\\"naïve\\\" 中\" now";
        let rope = Rope::from(text);
        let cursor = text
            .find("naïve")
            .expect("quoted Unicode text should exist");
        let opening = text.find('"').expect("opening quote should exist");
        let closing = text.rfind('"').expect("closing quote should exist");

        assert_eq!(
            quote_text_object_range(&rope, cursor, VimTextObjectPrefix::Inner, '"'),
            opening + 1..closing
        );
        assert_eq!(
            quote_text_object_range(&rope, cursor, VimTextObjectPrefix::Around, '"'),
            opening..closing + 1
        );

        let cases = [("'one'", '\''), ("`two`", '`')];
        for (source, delimiter) in cases {
            let rope = Rope::from(source);
            assert_eq!(
                quote_text_object_range(&rope, 2, VimTextObjectPrefix::Inner, delimiter),
                1..source.len() - 1
            );
        }
    }

    #[test]
    fn quote_text_objects_stay_on_the_current_line_and_require_a_pair() {
        let rope = Rope::from("\"open\nclose\"");
        assert_eq!(
            quote_text_object_range(&rope, 2, VimTextObjectPrefix::Inner, '"'),
            2..2
        );
        let unmatched = Rope::from("before \"after");
        assert_eq!(
            quote_text_object_range(&unmatched, 9, VimTextObjectPrefix::Around, '"'),
            9..9
        );
    }

    #[test]
    fn pair_text_objects_choose_the_innermost_nested_pair() {
        let text = "call(outer[中 + inner(x)]) tail";
        let rope = Rope::from(text);
        let cursor = text.find('x').expect("nested value should exist");
        let inner_open = text.find("(x)").expect("inner pair should exist");
        let bracket_open = text.find('[').expect("bracket should exist");
        let bracket_close = text.find(']').expect("closing bracket should exist");

        assert_eq!(
            pair_text_object_range(&rope, cursor, VimTextObjectPrefix::Inner, '(', ')'),
            inner_open + 1..inner_open + 2
        );
        assert_eq!(
            pair_text_object_range(&rope, cursor, VimTextObjectPrefix::Around, '(', ')'),
            inner_open..inner_open + 3
        );
        assert_eq!(
            pair_text_object_range(&rope, cursor, VimTextObjectPrefix::Inner, '[', ']'),
            bracket_open + 1..bracket_close
        );

        let unmatched = Rope::from("one {two");
        assert_eq!(
            pair_text_object_range(&unmatched, 6, VimTextObjectPrefix::Inner, '{', '}'),
            6..6
        );
    }

    #[test]
    fn linewise_visual_ranges_include_complete_crlf_and_final_lines() {
        let rope = Rope::from("one\r\ntwo\nthree");
        assert_eq!(line_rows_range(&rope, 0, 1), 0..9);
        assert_eq!(line_rows_range(&rope, 1, 2), 5..14);
    }

    #[test]
    fn line_ranges_include_newlines_and_handle_final_lines() {
        let rope = Rope::from("one\r\ntwo\nthree");
        assert_eq!(line_count_range(&rope, 0, 2), 0..9);
        assert_eq!(line_count_range(&rope, 9, 2), 9..14);
        assert_eq!(normal_line_end(&rope, 0), 2);
    }

    #[test]
    fn motions_cover_lines_tabs_unicode_and_document_edges() {
        let rope = Rope::from("one two\n\t中 x\nlast");
        let cases = [
            (0, VimKey::WordForward, 1, 4),
            (6, VimKey::WordBackward, 1, 4),
            (0, VimKey::WordEnd, 1, 2),
            (0, VimKey::BigWordForward, 2, 9),
            (13, VimKey::BigWordBackward, 1, 9),
            (8, VimKey::BigWordEnd, 1, 9),
            (8, VimKey::Left, 1, 8),
            (6, VimKey::Right, 1, 6),
            (8, VimKey::FirstNonBlank, 1, 9),
            (9, VimKey::LineEnd, 1, 13),
            (0, VimKey::Down, 1, 8),
            (13, VimKey::Go, 1, 0),
            (13, VimKey::Go, 2, 8),
            (0, VimKey::DocumentEnd, 1, 15),
            (0, VimKey::DocumentEnd, 2, 8),
        ];

        for (cursor, key, count, expected) in cases {
            assert_eq!(
                motion_for_key(&rope, cursor, key, count, None).map(|motion| motion.target),
                Some(expected),
                "unexpected target for {key:?} from {cursor} with count {count}"
            );
        }
    }

    #[test]
    fn normal_cursor_clamping_handles_empty_and_whitespace_only_lines() {
        let empty = Rope::from("");
        assert_eq!(clamp_normal_offset(&empty, 8), 0);

        let rope = Rope::from("  \n\n中\n");
        assert_eq!(first_non_blank(&rope, 1), 0);
        assert_eq!(clamp_normal_offset(&rope, 2), 1);
        assert_eq!(clamp_normal_offset(&rope, 3), 3);
        assert_eq!(clamp_normal_offset(&rope, 7), 4);
        assert_eq!(clamp_normal_offset(&rope, 8), 8);
    }

    #[test]
    fn operator_ranges_distinguish_characterwise_and_linewise_motions() {
        let rope = Rope::from("one two\nthree\nfour");
        let right = motion_for_key(&rope, 4, VimKey::Right, 1, None)
            .map(|motion| operator_range(&rope, 4, motion));
        let word_end = motion_for_key(&rope, 4, VimKey::WordEnd, 1, None)
            .map(|motion| operator_range(&rope, 4, motion));
        let big_word_end = motion_for_key(&rope, 0, VimKey::BigWordEnd, 1, None)
            .map(|motion| operator_range(&rope, 0, motion));
        let down = motion_for_key(&rope, 4, VimKey::Down, 1, None)
            .map(|motion| linewise_motion_range(&rope, 4, motion.target));

        assert_eq!(right, Some(4..5));
        assert_eq!(word_end, Some(4..7));
        assert_eq!(big_word_end, Some(0..3));
        assert_eq!(down, Some(0..14));
    }

    #[test]
    fn inclusive_visual_ranges_respect_multibyte_characters() {
        let rope = Rope::from("a中b");
        assert_eq!(inclusive_range(&rope, 1, 1), 1..4);
        assert_eq!(inclusive_range(&rope, 4, 1), 1..5);
    }

    #[test]
    fn operator_counts_multiply_and_saturate() {
        let mut vim = VimState::new(true);
        vim.operator_count = Some(2);
        vim.count = Some(3);
        assert_eq!(combined_operator_count(&mut vim), 6);

        vim.operator_count = Some(MAX_COUNT);
        vim.count = Some(MAX_COUNT);
        assert_eq!(combined_operator_count(&mut vim), MAX_COUNT);

        let mut digits = VimState::new(true);
        for _ in 0..12 {
            digits.push_digit(9);
        }
        assert_eq!(digits.count, Some(MAX_COUNT));
    }

    #[test]
    fn command_text_preserves_operator_and_motion_count_order() {
        let mut vim = VimState::new(true);
        vim.operator_count = Some(2);
        vim.pending_operator = Some(VimOperator::Delete);
        vim.count = Some(3);
        vim.pending_g = true;
        assert_eq!(vim.command_text(), "2d3g");

        vim.pending_g = false;
        vim.pending_text_object = Some(VimTextObjectPrefix::Inner);
        assert_eq!(vim.command_text(), "2d3i");
    }

    #[test]
    fn command_reset_clears_invalid_sequence_state() {
        let mut vim = VimState::new(true);
        vim.count = Some(24);
        vim.operator_count = Some(3);
        vim.pending_operator = Some(VimOperator::Change);
        vim.pending_g = true;
        vim.pending_text_object = Some(VimTextObjectPrefix::Around);

        vim.reset_command();

        assert_eq!(vim.count, None);
        assert_eq!(vim.operator_count, None);
        assert_eq!(vim.pending_operator, None);
        assert!(!vim.pending_g);
        assert_eq!(vim.pending_text_object, None);
    }

    #[test]
    fn local_motion_on_a_large_rope_does_not_materialize_the_document() {
        let rope = Rope::from(format!("{}target word", "line\n".repeat(500_000)));
        let start = rope.len() - "target word".len();
        let allocation = test_alloc::start_measurement();
        for _ in 0..128 {
            std::hint::black_box(next_word_start(&rope, start));
            std::hint::black_box(previous_word_start(&rope, rope.len()));
        }
        let allocation = allocation.finish();

        assert!(
            allocation.allocated_bytes < rope.len() / 4,
            "local motions allocated {} bytes for a {} byte rope",
            allocation.allocated_bytes,
            rope.len()
        );
        assert_eq!(next_word_start(&rope, start), start + "target ".len());
        assert_eq!(
            previous_word_start(&rope, rope.len()),
            start + "target ".len()
        );
    }

    #[gpui::test]
    fn modal_focus_edits_history_clipboard_search_and_live_settings(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let (db, note_id) = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                let note = note::ActiveModel {
                    title: Set("Vim test".into()),
                    cached_content: Set("alpha beta".into()),
                    created_at: Set(1),
                    updated_at: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>((db, note.id as u32))
            })
            .expect("Vim test database should initialize");
        let settings_dir = std::env::temp_dir().join(format!(
            "castle-vim-mode-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after the Unix epoch")
                .as_nanos()
        ));
        let mut editor_view = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(AppSettings::load(settings_dir));
            AppSettings::set_editor_vim_mode(true, cx);
            AppSettings::set_editor_status_line_visible(false, cx);
            crate::keymap::init(cx);
            cx.set_global(DB {
                conn: Arc::new(db),
                data_dir: PathBuf::new(),
            });
            cx.open_window(Default::default(), |window, cx| {
                let view = DocumentEditorView::view(note_id, window, cx);
                editor_view = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("Vim test window should open")
        });
        let view = editor_view.expect("document editor should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

        for _ in 0..100 {
            cx.run_until_parked();
            if view.read_with(&cx, |editor, _| !editor.is_loading) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let changes = Rc::new(Cell::new(0));
        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("alpha beta", window, cx);
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
                let input = editor.editor.clone();
                let changes = changes.clone();
                cx.subscribe(&input, move |_, _, event: &InputEvent, _| {
                    if matches!(event, InputEvent::Change) {
                        changes.set(changes.get() + 1);
                    }
                })
                .detach();
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });

        assert!(cx.debug_bounds("vim-mode-overlay").is_some());
        assert!(cx.update(|window, cx| view.read(cx).focus_handle.is_focused(window)));
        assert!(!cx.update(|window, cx| view.read(cx).editor.focus_handle(cx).is_focused(window)));

        cx.simulate_input("q");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "alpha beta"
        );
        assert_eq!(changes.get(), 0);

        cx.simulate_keystrokes("l y w");
        assert_eq!(changes.get(), 0);
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("lpha ".to_string())
        );

        cx.simulate_keystrokes("x");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "apha beta"
        );
        assert_eq!(changes.get(), 1);

        cx.simulate_keystrokes("u");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "alpha beta"
        );
        assert_eq!(changes.get(), 2);

        cx.simulate_keystrokes("ctrl-r");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "apha beta"
        );
        cx.simulate_keystrokes("u");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "alpha beta"
        );

        cx.simulate_keystrokes("i");
        assert!(cx.update(|window, cx| view.read(cx).editor.focus_handle(cx).is_focused(window)));
        cx.simulate_input("Z");
        cx.simulate_keystrokes("escape");
        assert_eq!(
            view.read_with(&cx, |editor, _| editor.vim_mode()),
            VimMode::Normal
        );
        assert!(cx.update(|window, cx| view.read(cx).focus_handle.is_focused(window)));
        let after_insert =
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value().to_string());
        cx.simulate_input("q");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            after_insert
        );

        cx.simulate_keystrokes("ctrl-f");
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let search_focus = cx.update(|window, cx| {
            let editor = view.read(cx);
            (
                editor.focus_handle.is_focused(window),
                editor.editor.focus_handle(cx).is_focused(window),
                editor.vim_search_active,
            )
        });
        assert!(
            search_focus.0,
            "unexpected search-close focus state: {search_focus:?}"
        );

        cx.update(|window, cx| {
            AppSettings::set_editor_vim_mode(false, cx);
            view.update(cx, |editor, cx| editor.sync_vim_setting(window, cx));
        });
        assert!(!view.read_with(&cx, |editor, _| editor.vim_is_enabled()));
        assert!(cx.update(|window, cx| view.read(cx).editor.focus_handle(cx).is_focused(window)));
        cx.simulate_input("!");
        assert_ne!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            after_insert
        );

        cx.update(|window, cx| {
            AppSettings::set_editor_vim_mode(true, cx);
            view.update(cx, |editor, cx| editor.sync_vim_setting(window, cx));
        });
        assert_eq!(
            view.read_with(&cx, |editor, _| editor.vim_mode()),
            VimMode::Normal
        );
        assert!(cx.update(|window, cx| view.read(cx).focus_handle.is_focused(window)));

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.kind = super::super::DocumentKind::Markdown;
                editor.mode = EditorMode::Source;
                editor.replace_content_for_test("- item", window, cx);
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        cx.simulate_keystrokes("shift-a enter");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "- item\n- "
        );
        assert_eq!(
            view.read_with(&cx, |editor, _| editor.vim_mode()),
            VimMode::Insert
        );
        cx.simulate_keystrokes("escape v l");
        assert_eq!(
            view.read_with(&cx, |editor, _| editor.vim_mode()),
            VimMode::Visual
        );
        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.set_mode(EditorMode::Preview, window, cx);
            });
        });
        assert_eq!(
            view.read_with(&cx, |editor, _| editor.vim_mode()),
            VimMode::Normal
        );
        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.set_mode(EditorMode::Source, window, cx);
            });
        });
        assert!(cx.update(|window, cx| view.read(cx).focus_handle.is_focused(window)));

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("Further testing showed", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 0), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
            let _ = window.draw(cx);
        });
        cx.simulate_keystrokes("v");
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.simulate_keystrokes("i w");
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        view.read_with(&cx, |editor, cx| {
            let range = editor
                .vim_visual_range(cx)
                .expect("viw should leave a Visual selection");
            assert_eq!(range, 0.."Further".len());
            let input = editor.editor.read(cx);
            let source_bounds = editor
                .source_bounds
                .expect("source bounds should be available after drawing");
            let selection = super::super::render::vim_selection_bounds(input, range, source_bounds);
            let cursor = super::super::render::vim_cursor_bounds(input, input.cursor())
                .expect("Visual cursor should have bounds");
            assert_eq!(selection.len(), 1);
            assert!(selection[0].size.width > cursor.size.width * 4.);
            assert_eq!(selection[0].size.height, cursor.size.height);
        });
        cx.simulate_keystrokes("y");
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("Further".to_string())
        );
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "Further testing showed"
        );

        cx.simulate_keystrokes("w d i w");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "Further  showed"
        );

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("one.two  three", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 0), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        let before_motion = changes.get();
        cx.simulate_keystrokes("shift-w shift-e 2 shift-b");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).cursor()),
            0
        );
        assert_eq!(changes.get(), before_motion);

        let before_delete_word = changes.get();
        cx.simulate_keystrokes("d shift-w");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "three"
        );
        assert_eq!(changes.get(), before_delete_word + 1);

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("a中b", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 2), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        let before_delete_previous = changes.get();
        cx.simulate_keystrokes("shift-x");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "ab"
        );
        assert_eq!(changes.get(), before_delete_previous + 1);

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("a中bc", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 0), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        let before_substitute = changes.get();
        cx.simulate_keystrokes("2 s");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "bc"
        );
        assert_eq!(changes.get(), before_substitute + 1);
        assert_eq!(
            view.read_with(&cx, |editor, _| editor.vim_mode()),
            VimMode::Insert
        );
        cx.simulate_input("Z");
        cx.simulate_keystrokes("escape");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "Zbc"
        );

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("  one\r\nnext", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 3), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        let before_substitute_line = changes.get();
        cx.simulate_keystrokes("shift-s");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "  \r\nnext"
        );
        assert_eq!(changes.get(), before_substitute_line + 1);
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).cursor()),
            2
        );
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "  X\r\nnext"
        );

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("one\ntwo", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 0), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        let before_yank_line = changes.get();
        cx.simulate_keystrokes("shift-y");
        assert_eq!(changes.get(), before_yank_line);
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("one\n".to_string())
        );

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("one  \r\n\t two \r\n中", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 0), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        let before_join = changes.get();
        cx.simulate_keystrokes("3 shift-j");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "one two 中"
        );
        assert_eq!(changes.get(), before_join + 1);

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test(
                    "zero\none \"Further testing\" tail\nthree",
                    window,
                    cx,
                );
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(1, 8), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        let before_quote_selection = changes.get();
        cx.simulate_keystrokes("v i shift-'->\"");
        assert_eq!(
            view.read_with(&cx, |editor, _| editor.vim_mode()),
            VimMode::Visual
        );
        view.read_with(&cx, |editor, cx| {
            let range = editor
                .vim_visual_range(cx)
                .expect("vi double quote should select its contents");
            assert_eq!(
                editor.editor.read(cx).text().slice(range).to_string(),
                "Further testing"
            );
        });
        assert_eq!(changes.get(), before_quote_selection);
        cx.simulate_keystrokes("y");
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("Further testing".to_string())
        );

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("(\"I will testign some braces\")", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 10), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        let before_parenthesis_selection = changes.get();
        cx.simulate_keystrokes("v i shift-9->(");
        view.read_with(&cx, |editor, cx| {
            let range = editor
                .vim_visual_range(cx)
                .expect("vi parenthesis should select its contents");
            assert_eq!(
                editor.editor.read(cx).text().slice(range).to_string(),
                "\"I will testign some braces\""
            );
        });
        assert_eq!(changes.get(), before_parenthesis_selection);
        cx.simulate_keystrokes("escape");

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("  alpha", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 4), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        let before_symbol_motions = changes.get();
        cx.simulate_keystrokes("shift-6->^ shift-4->$");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).cursor()),
            6
        );
        assert_eq!(changes.get(), before_symbol_motions);

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("say \"naïve 中\" now", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 7), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        let before_delete_quote = changes.get();
        cx.simulate_keystrokes("d i shift-'->\"");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "say \"\" now"
        );
        assert_eq!(changes.get(), before_delete_quote + 1);
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("naïve 中".to_string())
        );

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test("one\r\n two\r\nthree", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 1), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        let before_visual_line = changes.get();
        cx.simulate_keystrokes("shift-v j");
        assert_eq!(
            view.read_with(&cx, |editor, _| editor.vim_mode()),
            VimMode::VisualLine
        );
        view.read_with(&cx, |editor, cx| {
            let range = editor
                .vim_visual_range(cx)
                .expect("Vj should select two complete lines");
            assert_eq!(
                editor.editor.read(cx).text().slice(range).to_string(),
                "one\r\n two\r\n"
            );
        });
        assert_eq!(changes.get(), before_visual_line);
        cx.simulate_keystrokes("d");
        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
            "three"
        );
        assert_eq!(changes.get(), before_visual_line + 1);
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("one\r\n two\r\n".to_string())
        );
    }
}
