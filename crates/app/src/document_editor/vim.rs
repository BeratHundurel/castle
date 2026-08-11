use gpui::{
    AppContext as _, ClipboardItem, Context, EntityInputHandler, Focusable as _, KeyDownEvent,
    MouseDownEvent, Window,
};
use gpui_component::{
    highlighter::Language,
    input::{InputState, Position, Redo, Rope, RopeExt as _, Search, Undo},
};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VimFindKind {
    Forward,
    Backward,
    TillForward,
    TillBackward,
}

impl VimFindKind {
    fn reverse(self) -> Self {
        match self {
            Self::Forward => Self::Backward,
            Self::Backward => Self::Forward,
            Self::TillForward => Self::TillBackward,
            Self::TillBackward => Self::TillForward,
        }
    }

    fn command(self) -> char {
        match self {
            Self::Forward => 'f',
            Self::Backward => 'F',
            Self::TillForward => 't',
            Self::TillBackward => 'T',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VimPendingChar {
    Find(VimFindKind),
    Replace,
}

#[derive(Clone, Debug)]
struct VimLastFind {
    kind: VimFindKind,
    target: String,
}

#[derive(Clone, Debug)]
enum VimReplayStep {
    Key(VimKey),
    Literal(String),
}

#[derive(Clone, Copy, Debug)]
struct VimVisualRepeat {
    linewise: bool,
    extent: usize,
}

#[derive(Clone, Debug)]
struct VimInsertPatch {
    start_delta: isize,
    end_delta: isize,
    replacement: String,
    cursor_delta: isize,
}

#[derive(Clone, Debug)]
struct VimChangeRecipe {
    steps: Vec<VimReplayStep>,
    count: u32,
    insert_patch: Option<VimInsertPatch>,
    visual: Option<VimVisualRepeat>,
}

#[derive(Clone, Debug)]
struct VimHistoryEntry {
    before: Rope,
    after: Rope,
    cursor_before: usize,
    cursor_after: usize,
}

#[derive(Clone, Debug)]
struct VimInsertCapture {
    before: Rope,
    anchor: usize,
    steps: Vec<VimReplayStep>,
    count: u32,
    visual: Option<VimVisualRepeat>,
    pre_edit_changed: bool,
    history_before: Rope,
    history_cursor: usize,
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
    pending_char: Option<VimPendingChar>,
    last_find: Option<VimLastFind>,
    visual_anchor: Option<usize>,
    visual_head: Option<usize>,
    preferred_column: Option<u32>,
    register: Option<VimRegister>,
    change_candidate: Vec<VimReplayStep>,
    candidate_visual: Option<VimVisualRepeat>,
    insert_capture: Option<VimInsertCapture>,
    last_change: Option<VimChangeRecipe>,
    replaying: bool,
    undo_stack: Vec<VimHistoryEntry>,
    redo_stack: Vec<VimHistoryEntry>,
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
            pending_char: None,
            last_find: None,
            visual_anchor: None,
            visual_head: None,
            preferred_column: None,
            register: None,
            change_candidate: Vec::new(),
            candidate_visual: None,
            insert_capture: None,
            last_change: None,
            replaying: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
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
        if let Some(pending) = self.pending_char {
            command.push(match pending {
                VimPendingChar::Find(kind) => kind.command(),
                VimPendingChar::Replace => 'r',
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
        self.pending_char = None;
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
        self.vim_state.state.enabled()
    }

    pub(super) fn vim_mode(&self) -> VimMode {
        self.vim_state.state.mode()
    }

    pub(super) fn vim_context(&self) -> String {
        format!(
            "DocumentEditor vim_mode = {}",
            self.vim_state.state.key_context()
        )
    }

    pub(super) fn vim_visual_range(&self, cx: &gpui::App) -> Option<Range<usize>> {
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

    pub(super) fn reset_vim_command(&mut self) {
        self.vim_state.state.reset_command();
        self.vim_state.state.visual_anchor = None;
        self.vim_state.state.visual_head = None;
        self.vim_state.search_active = false;
        if self.vim_state.state.enabled {
            self.vim_state.state.mode = VimMode::Normal;
        }
    }

    pub(super) fn focus_source_mode(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vim_state.state.enabled && self.vim_state.state.mode != VimMode::Insert {
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
        if !self.vim_state.state.enabled || self.mode != EditorMode::Source {
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

    pub(super) fn on_vim_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.vim_state.state.enabled
            || self.mode != EditorMode::Source
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

    pub(super) fn on_vim_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.vim_state.state.enabled
            || self.vim_state.state.mode == VimMode::Insert
            || self.mode != EditorMode::Source
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

    pub(super) fn on_action_vim_insert_escape(
        &mut self,
        _: &gpui_component::input::Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.vim_state.state.enabled || self.mode != EditorMode::Source {
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

    fn vim_command_in_progress(&self) -> bool {
        self.vim_state.state.count.is_some()
            || self.vim_state.state.pending_operator.is_some()
            || self.vim_state.state.pending_g
            || self.vim_state.state.pending_text_object.is_some()
            || self.vim_state.state.pending_char.is_some()
    }

    fn prepare_vim_change_candidate(&mut self, key: VimKey, cx: &gpui::App) {
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

    fn finish_vim_action_recording(
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

    fn vim_visual_repeat(&self, cx: &gpui::App) -> Option<VimVisualRepeat> {
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

    fn commit_vim_change(&mut self, insert_patch: Option<VimInsertPatch>) {
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

    fn discard_vim_change_candidate(&mut self) {
        self.vim_state.state.change_candidate.clear();
        self.vim_state.state.candidate_visual = None;
    }

    fn finish_vim_insert_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn push_vim_history(&mut self, before: Rope, cursor_before: usize, cx: &gpui::App) {
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

    pub(super) fn sync_vim_search_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.vim_state.search_active
            || !self.vim_state.state.enabled
            || self.vim_state.state.mode == VimMode::Insert
            || self.mode != EditorMode::Source
            || !self.editor.focus_handle(cx).is_focused(window)
        {
            return;
        }

        self.vim_state.search_active = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn handle_normal_key(&mut self, key: VimKey, window: &mut Window, cx: &mut Context<Self>) {
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

    fn handle_visual_key(&mut self, key: VimKey, window: &mut Window, cx: &mut Context<Self>) {
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

    fn begin_operator(&mut self, operator: VimOperator, cx: &mut Context<Self>) {
        self.vim_state.state.operator_count = self.vim_state.state.count.take();
        self.vim_state.state.pending_operator = Some(operator);
        self.vim_state.state.pending_g = false;
        cx.notify();
    }

    fn handle_pending_operator(
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

    fn begin_find(&mut self, kind: VimFindKind, cx: &mut Context<Self>) {
        self.vim_state.state.pending_char = Some(VimPendingChar::Find(kind));
        cx.notify();
    }

    fn apply_pending_vim_char(
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

    fn apply_find(
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

    fn repeat_find(&mut self, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
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

    fn apply_operator_repeated_find(
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

    fn replace_vim_chars(&mut self, target: &str, window: &mut Window, cx: &mut Context<Self>) {
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

    fn repeat_last_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            InputState::new(window, cx)
                .code_editor(Language::Plain)
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

    fn replay_vim_steps(
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

    fn undo_vim_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn redo_vim_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn prepare_visual_repeat(
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

    fn apply_insert_patch(
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

    fn apply_direct_operator(
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

    fn apply_operator_motion(
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
        let linewise = self.vim_state.state.mode == VimMode::VisualLine;
        self.apply_operator(operator, range, linewise, window, cx);
    }

    fn apply_visual_text_object(
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

    fn apply_operator(
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

    fn apply_motion_key(&mut self, key: VimKey, window: &mut Window, cx: &mut Context<Self>) {
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

    fn delete_vim_char(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.vim_state.state.take_count();
        let range = {
            let editor = self.editor.read(cx);
            forward_char_range(editor.text(), editor.cursor(), count)
        };
        self.apply_operator(VimOperator::Delete, range, false, window, cx);
    }

    fn delete_vim_previous_char(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.vim_state.state.take_count();
        let range = {
            let editor = self.editor.read(cx);
            backward_char_range(editor.text(), editor.cursor(), count)
        };
        self.apply_operator(VimOperator::Delete, range, false, window, cx);
    }

    fn substitute_vim_char(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn join_vim_lines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn paste_vim(&mut self, before: bool, window: &mut Window, cx: &mut Context<Self>) {
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

    fn open_vim_line(&mut self, above: bool, window: &mut Window, cx: &mut Context<Self>) {
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

    fn enter_vim_insert_at_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.editor.read(cx).cursor();
        self.enter_vim_insert(cursor, window, cx);
    }

    fn enter_vim_insert(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.vim_state.state.mode = VimMode::Insert;
        self.vim_state.state.visual_anchor = None;
        self.vim_state.state.visual_head = None;
        self.vim_state.state.reset_command();
        self.set_input_cursor(offset, window, cx);
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
        cx.notify();
    }

    fn enter_vim_normal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn set_vim_cursor(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
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
        self.vim_state.state.reset_command();
        cx.notify();
    }

    fn dispatch_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.vim_state.search_active = false;
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
        window.dispatch_action(Box::new(Search), cx);
        self.vim_state.search_active = true;
        self.vim_state.state.reset_command();
        cx.notify();
    }
}

fn vim_find_kind_for_key(key: VimKey) -> Option<VimFindKind> {
    match key {
        VimKey::FindForward => Some(VimFindKind::Forward),
        VimKey::FindBackward => Some(VimFindKind::Backward),
        VimKey::TillForward => Some(VimFindKind::TillForward),
        VimKey::TillBackward => Some(VimFindKind::TillBackward),
        _ => None,
    }
}

fn vim_literal_for_key(key: VimKey) -> Option<String> {
    let literal = match key {
        VimKey::Digit(digit) => {
            return char::from_digit(u32::from(digit), 10).map(|ch| ch.to_string());
        }
        VimKey::Left => "h",
        VimKey::Down => "j",
        VimKey::Up => "k",
        VimKey::Right => "l",
        VimKey::WordForward => "w",
        VimKey::WordBackward => "b",
        VimKey::WordEnd => "e",
        VimKey::BigWordForward => "W",
        VimKey::BigWordBackward => "B",
        VimKey::BigWordEnd => "E",
        VimKey::FindForward => "f",
        VimKey::FindBackward => "F",
        VimKey::TillForward => "t",
        VimKey::TillBackward => "T",
        VimKey::RepeatFind => ";",
        VimKey::RepeatFindReverse => ",",
        VimKey::LiteralEnter => "\n",
        VimKey::LiteralTab => "\t",
        VimKey::LiteralSpace => " ",
        VimKey::FirstNonBlank => "^",
        VimKey::LineEnd => "$",
        VimKey::Go => "g",
        VimKey::DocumentEnd => "G",
        VimKey::Insert => "i",
        VimKey::Append => "a",
        VimKey::InsertLineStart => "I",
        VimKey::AppendLineEnd => "A",
        VimKey::OpenBelow => "o",
        VimKey::OpenAbove => "O",
        VimKey::Visual => "v",
        VimKey::VisualLine => "V",
        VimKey::DoubleQuote => "\"",
        VimKey::SingleQuote => "'",
        VimKey::Backtick => "`",
        VimKey::Parenthesis => "(",
        VimKey::ParenthesisClose => ")",
        VimKey::Bracket => "[",
        VimKey::BracketClose => "]",
        VimKey::Brace => "{",
        VimKey::BraceClose => "}",
        VimKey::DeleteChar => "x",
        VimKey::DeletePreviousChar => "X",
        VimKey::SubstituteChar => "s",
        VimKey::SubstituteLine => "S",
        VimKey::ReplaceChar => "r",
        VimKey::YankLine => "Y",
        VimKey::JoinLines => "J",
        VimKey::Delete => "d",
        VimKey::Yank => "y",
        VimKey::Change => "c",
        VimKey::DeleteToLineEnd => "D",
        VimKey::ChangeToLineEnd => "C",
        VimKey::PasteAfter => "p",
        VimKey::PasteBefore => "P",
        VimKey::Undo => "u",
        VimKey::RepeatLastChange => ".",
        VimKey::LineStart | VimKey::Redo | VimKey::Search | VimKey::Escape => return None,
    };
    Some(literal.to_string())
}

fn target_matches(rope: &Rope, offset: usize, line_end: usize, target: &str) -> bool {
    let end = offset.saturating_add(target.len());
    end <= line_end
        && rope.is_char_boundary(offset)
        && rope.is_char_boundary(end)
        && rope.slice(offset..end) == target
}

fn find_forward_occurrence(
    rope: &Rope,
    mut offset: usize,
    line_end: usize,
    target: &str,
) -> Option<usize> {
    while offset < line_end {
        if target_matches(rope, offset, line_end, target) {
            return Some(offset);
        }
        offset = next_boundary(rope, offset);
    }
    None
}

fn find_backward_occurrence(
    rope: &Rope,
    line_start: usize,
    before: usize,
    target: &str,
) -> Option<usize> {
    let mut offset = line_start;
    let mut found = None;
    while offset < before {
        if target_matches(rope, offset, before, target) {
            found = Some(offset);
        }
        offset = next_boundary(rope, offset);
    }
    found
}

fn find_char_motion(
    rope: &Rope,
    cursor: usize,
    kind: VimFindKind,
    target: &str,
    count: u32,
    repeating: bool,
) -> Option<Motion> {
    if target.is_empty() || target.contains(['\r', '\n']) {
        return None;
    }
    let row = row_at(rope, cursor);
    let line_start = rope.line_start_offset(row);
    let line_end = line_content_end(rope, row);
    let mut occurrence = None;

    match kind {
        VimFindKind::Forward | VimFindKind::TillForward => {
            let mut search_start = next_boundary(rope, cursor).min(line_end);
            for index in 0..count.max(1) {
                let mut found = find_forward_occurrence(rope, search_start, line_end, target)?;
                if repeating
                    && index == 0
                    && kind == VimFindKind::TillForward
                    && found == search_start
                {
                    search_start = found.saturating_add(target.len()).min(line_end);
                    found = find_forward_occurrence(rope, search_start, line_end, target)?;
                }
                occurrence = Some(found);
                search_start = found.saturating_add(target.len()).min(line_end);
            }
        }
        VimFindKind::Backward | VimFindKind::TillBackward => {
            let mut before = cursor;
            for index in 0..count.max(1) {
                let mut found = find_backward_occurrence(rope, line_start, before, target)?;
                if repeating
                    && index == 0
                    && kind == VimFindKind::TillBackward
                    && found.saturating_add(target.len()) == before
                {
                    before = found;
                    found = find_backward_occurrence(rope, line_start, before, target)?;
                }
                occurrence = Some(found);
                before = found;
            }
        }
    }

    let occurrence = occurrence?;
    let target_offset = match kind {
        VimFindKind::Forward | VimFindKind::Backward => occurrence,
        VimFindKind::TillForward => previous_boundary(rope, occurrence),
        VimFindKind::TillBackward => occurrence.saturating_add(target.len()),
    };
    Some(Motion {
        target: target_offset,
        inclusive: matches!(kind, VimFindKind::Forward | VimFindKind::TillForward),
        linewise: false,
    })
}

fn replace_visual_text(selected: &str, target: &str) -> String {
    if target == "\n" {
        return target.to_string();
    }
    let mut replacement = String::with_capacity(selected.len().max(target.len()));
    for ch in selected.chars() {
        if matches!(ch, '\r' | '\n') {
            replacement.push(ch);
        } else {
            replacement.push_str(target);
        }
    }
    replacement
}

fn normalized_replay_steps(steps: &[VimReplayStep]) -> (Vec<VimReplayStep>, u32) {
    let mut normalized = Vec::with_capacity(steps.len());
    let mut combined_count = 1_u32;
    let mut index = 0;
    while index < steps.len() {
        let Some(VimReplayStep::Key(VimKey::Digit(first))) = steps.get(index) else {
            normalized.push(steps[index].clone());
            index += 1;
            continue;
        };
        if *first == 0 {
            normalized.push(steps[index].clone());
            index += 1;
            continue;
        }
        let mut group = 0_u32;
        while let Some(VimReplayStep::Key(VimKey::Digit(digit))) = steps.get(index) {
            group = group
                .saturating_mul(10)
                .saturating_add(u32::from(*digit))
                .min(MAX_COUNT);
            index += 1;
        }
        combined_count = combined_count.saturating_mul(group).min(MAX_COUNT);
    }
    (normalized, combined_count)
}

fn replay_is_open_line(steps: &[VimReplayStep]) -> bool {
    matches!(
        steps.first(),
        Some(VimReplayStep::Key(VimKey::OpenBelow | VimKey::OpenAbove))
    )
}

fn replay_repeats_insert_text(steps: &[VimReplayStep]) -> bool {
    matches!(
        steps.first(),
        Some(VimReplayStep::Key(
            VimKey::Insert | VimKey::Append | VimKey::InsertLineStart | VimKey::AppendLineEnd
        ))
    )
}

fn signed_char_distance(rope: &Rope, from: usize, to: usize) -> isize {
    if to >= from {
        rope.slice(from..to).chars().count() as isize
    } else {
        -(rope.slice(to..from).chars().count() as isize)
    }
}

fn move_by_chars(rope: &Rope, mut offset: usize, delta: isize) -> usize {
    if delta >= 0 {
        for _ in 0..delta as usize {
            let next = next_boundary(rope, offset);
            if next == offset {
                break;
            }
            offset = next;
        }
    } else {
        for _ in 0..delta.unsigned_abs() {
            let previous = previous_boundary(rope, offset);
            if previous == offset {
                break;
            }
            offset = previous;
        }
    }
    offset
}

fn insert_patch_between(
    before: &Rope,
    after: &Rope,
    anchor: usize,
    cursor: usize,
) -> Option<VimInsertPatch> {
    if before == after {
        return None;
    }

    let mut before_start = 0;
    let mut after_start = 0;
    let mut before_chars = before.chars();
    let mut after_chars = after.chars();
    while before_start < anchor {
        match (before_chars.next(), after_chars.next()) {
            (Some(left), Some(right)) if left == right => {
                before_start += left.len_utf8();
                after_start += right.len_utf8();
            }
            _ => break,
        }
    }

    let mut before_end = before.len();
    let mut after_end = after.len();
    while before_end > before_start && after_end > after_start {
        let before_previous = previous_boundary(before, before_end);
        let after_previous = previous_boundary(after, after_end);
        if before.char_at(before_previous) != after.char_at(after_previous) {
            break;
        }
        before_end = before_previous;
        after_end = after_previous;
    }

    Some(VimInsertPatch {
        start_delta: signed_char_distance(before, anchor, before_start),
        end_delta: signed_char_distance(before, anchor, before_end),
        replacement: after.slice(after_start..after_end).to_string(),
        cursor_delta: signed_char_distance(after, after_start, cursor),
    })
}

fn rope_replacement_between(before: &Rope, after: &Rope) -> Option<(Range<usize>, String)> {
    if before == after {
        return None;
    }

    let mut before_start = 0;
    let mut after_start = 0;
    let mut before_chars = before.chars();
    let mut after_chars = after.chars();
    loop {
        match (before_chars.next(), after_chars.next()) {
            (Some(left), Some(right)) if left == right => {
                before_start += left.len_utf8();
                after_start += right.len_utf8();
            }
            _ => break,
        }
    }

    let mut before_end = before.len();
    let mut after_end = after.len();
    while before_end > before_start && after_end > after_start {
        let before_previous = previous_boundary(before, before_end);
        let after_previous = previous_boundary(after, after_end);
        if before.char_at(before_previous) != after.char_at(after_previous) {
            break;
        }
        before_end = before_previous;
        after_end = after_previous;
    }

    Some((
        before_start..before_end,
        after.slice(after_start..after_end).to_string(),
    ))
}

fn combined_operator_count(vim: &mut VimState) -> Option<u32> {
    let operator = vim.operator_count.take();
    let motion = vim.count.take();
    vim.pending_operator = None;
    match (operator, motion) {
        (None, None) => None,
        (Some(count), None) | (None, Some(count)) => Some(count),
        (Some(operator), Some(motion)) => Some(operator.saturating_mul(motion).min(MAX_COUNT)),
    }
}

fn motion_for_key(
    rope: &Rope,
    cursor: usize,
    key: VimKey,
    count: Option<u32>,
    preferred_column: Option<u32>,
) -> Option<Motion> {
    let count_value = count.unwrap_or(1);
    let target = match key {
        VimKey::Left => {
            let line_start = rope.line_start_offset(row_at(rope, cursor));
            repeat_motion(cursor, count_value, |offset| {
                previous_boundary(rope, offset).max(line_start)
            })
        }
        VimKey::Right => repeat_motion(cursor, count_value, |offset| {
            next_boundary(rope, offset).min(normal_line_end(rope, row_at(rope, cursor)))
        }),
        VimKey::Down | VimKey::Up => {
            let row = row_at(rope, cursor);
            let delta = if key == VimKey::Down {
                i64::from(count_value)
            } else {
                -i64::from(count_value)
            };
            let target_row =
                (row as i64 + delta).clamp(0, rope.lines_len().saturating_sub(1) as i64) as usize;
            let column =
                preferred_column.unwrap_or_else(|| rope.offset_to_position(cursor).character);
            let target = rope.position_to_offset(&Position::new(target_row as u32, column));
            target.min(normal_line_end(rope, target_row))
        }
        VimKey::WordForward => {
            repeat_motion(cursor, count_value, |offset| next_word_start(rope, offset))
        }
        VimKey::WordBackward => repeat_motion(cursor, count_value, |offset| {
            previous_word_start(rope, offset)
        }),
        VimKey::WordEnd => repeat_motion(cursor, count_value, |offset| word_end(rope, offset)),
        VimKey::BigWordForward => repeat_motion(cursor, count_value, |offset| {
            next_big_word_start(rope, offset)
        }),
        VimKey::BigWordBackward => repeat_motion(cursor, count_value, |offset| {
            previous_big_word_start(rope, offset)
        }),
        VimKey::BigWordEnd => {
            repeat_motion(cursor, count_value, |offset| big_word_end(rope, offset))
        }
        VimKey::LineStart => rope.line_start_offset(row_at(rope, cursor)),
        VimKey::FirstNonBlank => first_non_blank(rope, cursor),
        VimKey::LineEnd => {
            let row = row_at(rope, cursor)
                .saturating_add(count_value as usize)
                .saturating_sub(1)
                .min(rope.lines_len().saturating_sub(1));
            normal_line_end(rope, row)
        }
        VimKey::Go => {
            let row = count
                .map(|count| (count as usize - 1).min(rope.lines_len().saturating_sub(1)))
                .unwrap_or(0);
            rope.line_start_offset(row)
        }
        VimKey::DocumentEnd => {
            let row = count
                .map(|count| (count as usize - 1).min(rope.lines_len().saturating_sub(1)))
                .unwrap_or_else(|| rope.lines_len().saturating_sub(1));
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

fn line_break_after_row(rope: &Rope, row: usize) -> Option<&'static str> {
    if row + 1 >= rope.lines_len() {
        return None;
    }
    let offset = line_content_end(rope, row);
    match rope.char_at(offset) {
        Some('\r') if rope.char_at(next_boundary(rope, offset)) == Some('\n') => Some("\r\n"),
        Some('\n') => Some("\n"),
        _ => None,
    }
}

fn line_break_for_row(rope: &Rope, row: usize) -> &'static str {
    if let Some(line_break) = line_break_after_row(rope, row) {
        return line_break;
    }
    for distance in 1..rope.lines_len() {
        if let Some(line_break) = row
            .checked_sub(distance)
            .and_then(|row| line_break_after_row(rope, row))
        {
            return line_break;
        }
        if let Some(line_break) = row
            .checked_add(distance)
            .filter(|row| *row < rope.lines_len())
            .and_then(|row| line_break_after_row(rope, row))
        {
            return line_break;
        }
    }
    "\n"
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
            | VimKey::ParenthesisClose
            | VimKey::Bracket
            | VimKey::BracketClose
            | VimKey::Brace
            | VimKey::BraceClose
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
        VimKey::Parenthesis | VimKey::ParenthesisClose => {
            pair_text_object_range(rope, cursor, prefix, '(', ')')
        }
        VimKey::Bracket | VimKey::BracketClose => {
            pair_text_object_range(rope, cursor, prefix, '[', ']')
        }
        VimKey::Brace | VimKey::BraceClose => {
            pair_text_object_range(rope, cursor, prefix, '{', '}')
        }
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
    use crate::{AppServices, app_settings::AppSettings, test_alloc};
    use entity::note;
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
    fn line_break_inference_prefers_the_current_then_nearest_line() {
        let mixed = Rope::from("one\r\ntwo\nthree");
        assert_eq!(line_break_for_row(&mixed, 0), "\r\n");
        assert_eq!(line_break_for_row(&mixed, 1), "\n");
        assert_eq!(line_break_for_row(&mixed, 2), "\n");

        assert_eq!(line_break_for_row(&Rope::from("one"), 0), "\n");
    }

    #[test]
    fn motions_cover_lines_tabs_unicode_and_document_edges() {
        let rope = Rope::from("one two\n\t中 x\nlast");
        let cases = [
            (0, VimKey::WordForward, Some(1), 4),
            (6, VimKey::WordBackward, Some(1), 4),
            (0, VimKey::WordEnd, Some(1), 2),
            (0, VimKey::BigWordForward, Some(2), 9),
            (13, VimKey::BigWordBackward, Some(1), 9),
            (8, VimKey::BigWordEnd, Some(1), 9),
            (8, VimKey::Left, Some(1), 8),
            (6, VimKey::Right, Some(1), 6),
            (8, VimKey::FirstNonBlank, Some(1), 9),
            (9, VimKey::LineEnd, Some(1), 13),
            (0, VimKey::Down, Some(1), 8),
            (13, VimKey::Go, None, 0),
            (13, VimKey::Go, Some(2), 8),
            (0, VimKey::DocumentEnd, None, 15),
            (15, VimKey::DocumentEnd, Some(1), 0),
            (0, VimKey::DocumentEnd, Some(2), 8),
        ];

        for (cursor, key, count, expected) in cases {
            assert_eq!(
                motion_for_key(&rope, cursor, key, count, None).map(|motion| motion.target),
                Some(expected),
                "unexpected target for {key:?} from {cursor} with count {count:?}"
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
        let right = motion_for_key(&rope, 4, VimKey::Right, Some(1), None)
            .map(|motion| operator_range(&rope, 4, motion));
        let word_end = motion_for_key(&rope, 4, VimKey::WordEnd, Some(1), None)
            .map(|motion| operator_range(&rope, 4, motion));
        let big_word_end = motion_for_key(&rope, 0, VimKey::BigWordEnd, Some(1), None)
            .map(|motion| operator_range(&rope, 0, motion));
        let down = motion_for_key(&rope, 4, VimKey::Down, Some(1), None)
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
        assert_eq!(combined_operator_count(&mut vim), Some(6));

        vim.operator_count = Some(MAX_COUNT);
        vim.count = Some(MAX_COUNT);
        assert_eq!(combined_operator_count(&mut vim), Some(MAX_COUNT));

        let mut no_count = VimState::new(true);
        no_count.pending_operator = Some(VimOperator::Delete);
        assert_eq!(combined_operator_count(&mut no_count), None);

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
    fn character_find_motions_cover_directions_counts_unicode_and_line_edges() {
        let cases = [
            ("a-b-a-b", 0, VimFindKind::Forward, "b", 1, Some(2)),
            ("a-b-a-b", 0, VimFindKind::Forward, "b", 2, Some(6)),
            ("a-b-a-b", 0, VimFindKind::TillForward, "b", 1, Some(1)),
            ("a-b-a-b", 6, VimFindKind::Backward, "a", 1, Some(4)),
            ("a-b-a-b", 6, VimFindKind::TillBackward, "a", 1, Some(5)),
            ("a中b中", 0, VimFindKind::Forward, "中", 2, Some(5)),
            ("x\r\nyx", 0, VimFindKind::Forward, "x", 1, None),
            ("\t a\t", 0, VimFindKind::Forward, "\t", 1, Some(3)),
            ("", 0, VimFindKind::Forward, "x", 1, None),
        ];
        for (text, cursor, kind, target, count, expected) in cases {
            let rope = Rope::from(text);
            assert_eq!(
                find_char_motion(&rope, cursor, kind, target, count, false)
                    .map(|motion| motion.target),
                expected,
                "unexpected {kind:?} result for {text:?}"
            );
        }
    }

    #[test]
    fn repeated_till_motions_skip_the_previous_adjacent_target() {
        let rope = Rope::from("a,x,x");
        let first = find_char_motion(&rope, 0, VimFindKind::TillForward, "x", 1, false)
            .expect("first till should find x");
        assert_eq!(first.target, 1);
        let repeated =
            find_char_motion(&rope, first.target, VimFindKind::TillForward, "x", 1, true)
                .expect("repeat should skip the x already used by t");
        assert_eq!(repeated.target, 3);

        let backward = find_char_motion(&rope, 4, VimFindKind::TillBackward, "x", 1, true)
            .expect("reverse till repeat should find the previous x");
        assert_eq!(backward.target, 3);
    }

    #[test]
    fn find_motions_preserve_operator_inclusivity() {
        let rope = Rope::from("abXcdXef");
        let forward = find_char_motion(&rope, 0, VimFindKind::Forward, "X", 1, false)
            .expect("f should find X");
        let till = find_char_motion(&rope, 0, VimFindKind::TillForward, "X", 1, false)
            .expect("t should find X");
        let backward = find_char_motion(&rope, 7, VimFindKind::Backward, "X", 1, false)
            .expect("F should find X");
        let till_backward = find_char_motion(&rope, 7, VimFindKind::TillBackward, "X", 1, false)
            .expect("T should find X");

        assert_eq!(operator_range(&rope, 0, forward), 0..3);
        assert_eq!(operator_range(&rope, 0, till), 0..2);
        assert_eq!(operator_range(&rope, 7, backward), 5..7);
        assert_eq!(operator_range(&rope, 7, till_backward), 6..7);
    }

    #[test]
    fn visual_replacement_preserves_line_breaks_and_repeats_unicode_targets() {
        assert_eq!(replace_visual_text("ab\r\n中", "λ"), "λλ\r\nλ");
        assert_eq!(replace_visual_text("abc", "\n"), "\n");
        assert_eq!(replace_visual_text("", "x"), "");
    }

    #[test]
    fn repeat_recipes_combine_counts_without_losing_zero_motions() {
        let (steps, count) = normalized_replay_steps(&[
            VimReplayStep::Key(VimKey::Digit(2)),
            VimReplayStep::Key(VimKey::Delete),
            VimReplayStep::Key(VimKey::Digit(3)),
            VimReplayStep::Key(VimKey::WordForward),
        ]);
        assert_eq!(count, 6);
        assert!(matches!(
            steps.as_slice(),
            [
                VimReplayStep::Key(VimKey::Delete),
                VimReplayStep::Key(VimKey::WordForward)
            ]
        ));

        let (steps, count) = normalized_replay_steps(&[
            VimReplayStep::Key(VimKey::Delete),
            VimReplayStep::Key(VimKey::Digit(0)),
        ]);
        assert_eq!(count, 1);
        assert!(matches!(
            steps.last(),
            Some(VimReplayStep::Key(VimKey::Digit(0)))
        ));
    }

    #[test]
    fn insert_patch_tracks_unicode_edits_relative_to_the_insert_anchor() {
        let before = Rope::from("a中c");
        let after = Rope::from("aλ中c");
        let patch = insert_patch_between(&before, &after, 1, "aλ".len())
            .expect("insert should produce a patch");
        assert_eq!(patch.start_delta, 0);
        assert_eq!(patch.end_delta, 0);
        assert_eq!(patch.replacement, "λ");
        assert_eq!(patch.cursor_delta, 1);
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

    #[test]
    fn find_and_repeat_planning_on_a_large_rope_stay_local() {
        let prefix = "line\n".repeat(500_000);
        let line_start = prefix.len();
        let rope = Rope::from(format!("{prefix}alpha,target,target"));
        let mut edited = rope.clone();
        let insert_at = line_start + "alpha".len();
        edited.insert(insert_at, "λ");
        let steps = [
            VimReplayStep::Key(VimKey::Digit(2)),
            VimReplayStep::Key(VimKey::Delete),
            VimReplayStep::Key(VimKey::Digit(3)),
            VimReplayStep::Key(VimKey::FindForward),
            VimReplayStep::Literal(",".to_string()),
        ];

        let allocation = test_alloc::start_measurement();
        for _ in 0..128 {
            std::hint::black_box(find_char_motion(
                &rope,
                line_start,
                VimFindKind::Forward,
                ",",
                2,
                false,
            ));
        }
        let patch = insert_patch_between(&rope, &edited, insert_at, insert_at + "λ".len())
            .expect("local insertion should produce a repeat patch");
        let (normalized, count) = normalized_replay_steps(&steps);
        let allocation = allocation.finish();

        assert!(
            allocation.allocated_bytes < rope.len() / 4,
            "find and repeat planning allocated {} bytes for a {} byte rope",
            allocation.allocated_bytes,
            rope.len()
        );
        assert_eq!(patch.replacement, "λ");
        assert_eq!(count, 6);
        assert_eq!(normalized.len(), 3);
    }

    fn with_vim_editor(
        cx: &mut gpui::TestAppContext,
        initial_content: &str,
        test: impl FnOnce(gpui::Entity<DocumentEditorView>, &mut gpui::VisualTestContext),
    ) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let (db, note_id) = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                let note = note::ActiveModel {
                    title: Set("Vim test".into()),
                    cached_content: Set(initial_content.into()),
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
            "castle-vim-mode-focused-{}-{}",
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
            cx.set_global(AppServices::new(Arc::new(db), PathBuf::new()));
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
            if view.read_with(&cx, |editor, _| !editor.persistence.is_loading) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            view.read_with(&cx, |editor, _| !editor.persistence.is_loading),
            "Vim test editor should finish loading"
        );
        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test(initial_content, window, cx);
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
            let _ = window.draw(cx);
        });

        test(view, &mut cx);
    }

    fn set_vim_test_content(
        view: &gpui::Entity<DocumentEditorView>,
        content: &str,
        position: Position,
        cx: &mut gpui::VisualTestContext,
    ) {
        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test(content, window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(position, window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
    }

    fn vim_test_value(
        view: &gpui::Entity<DocumentEditorView>,
        cx: &gpui::VisualTestContext,
    ) -> String {
        view.read_with(cx, |editor, cx| editor.editor.read(cx).value().to_string())
    }

    #[gpui::test]
    fn counted_linewise_yank_and_paste_execute_through_the_keymap(cx: &mut gpui::TestAppContext) {
        with_vim_editor(cx, "one two\nthree four\nfive", |view, cx| {
            cx.simulate_keystrokes("2 y y");

            assert_eq!(vim_test_value(&view, cx), "one two\nthree four\nfive");
            assert_eq!(
                cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
                Some("one two\nthree four\n".to_string())
            );

            cx.simulate_keystrokes("shift-g p");

            assert_eq!(
                vim_test_value(&view, cx),
                "one two\nthree four\nfive\none two\nthree four"
            );
            assert_eq!(
                view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
                "one two\nthree four\nfive\n".len()
            );
        });
    }

    #[gpui::test]
    fn insert_line_start_open_line_and_direct_changes_round_trip(cx: &mut gpui::TestAppContext) {
        with_vim_editor(cx, "  alpha\nbeta", |view, cx| {
            cx.simulate_keystrokes("shift-i");
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape");
            assert_eq!(vim_test_value(&view, cx), "  Xalpha\nbeta");

            set_vim_test_content(&view, "alpha beta\ngamma", Position::new(0, 0), cx);
            cx.simulate_keystrokes("shift-d");
            assert_eq!(vim_test_value(&view, cx), "\ngamma");
            assert_eq!(
                cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
                Some("alpha beta".to_string())
            );

            set_vim_test_content(&view, "alpha beta\ngamma", Position::new(0, 0), cx);
            cx.simulate_keystrokes("shift-c");
            assert_eq!(
                view.read_with(cx, |editor, _| editor.vim_mode()),
                VimMode::Insert
            );
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape");
            assert_eq!(vim_test_value(&view, cx), "X\ngamma");

            set_vim_test_content(&view, "alpha\ngamma", Position::new(0, 0), cx);
            cx.simulate_keystrokes("o");
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape");
            assert_eq!(vim_test_value(&view, cx), "alpha\nX\ngamma");
        });
    }

    #[gpui::test]
    fn visual_paste_replaces_the_inclusive_selection(cx: &mut gpui::TestAppContext) {
        with_vim_editor(cx, "abcd", |view, cx| {
            cx.update(|_, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(
                    "X".to_string(),
                    VIM_CLIPBOARD_CHARACTERWISE.to_string(),
                ));
            });

            cx.simulate_keystrokes("v l p");

            assert_eq!(vim_test_value(&view, cx), "Xcd");
            assert_eq!(
                view.read_with(cx, |editor, _| editor.vim_mode()),
                VimMode::Normal
            );
            assert_eq!(
                view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
                0
            );
        });
    }

    #[gpui::test]
    fn explicit_line_counts_distinguish_g_from_bare_g_in_motions_and_operators(
        cx: &mut gpui::TestAppContext,
    ) {
        with_vim_editor(cx, "first\nmiddle\nlast", |view, cx| {
            cx.simulate_keystrokes("shift-g");
            assert_eq!(
                view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
                "first\nmiddle\n".len()
            );

            cx.simulate_keystrokes("1 shift-g");

            assert_eq!(
                view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
                0
            );

            cx.simulate_keystrokes("2 shift-g");
            assert_eq!(
                view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
                "first\n".len()
            );

            set_vim_test_content(&view, "first\nmiddle\nlast", Position::new(1, 0), cx);
            cx.simulate_keystrokes("d 1 shift-g");
            assert_eq!(vim_test_value(&view, cx), "last");
            assert_eq!(
                cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
                Some("first\nmiddle\n".to_string())
            );
        });
    }

    #[gpui::test]
    fn invalid_operator_sequences_consume_the_key_and_clear_counts(cx: &mut gpui::TestAppContext) {
        with_vim_editor(cx, "abc", |view, cx| {
            cx.update(|_, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(
                    "X".to_string(),
                    VIM_CLIPBOARD_CHARACTERWISE.to_string(),
                ));
            });

            cx.simulate_keystrokes("d p");

            assert_eq!(vim_test_value(&view, cx), "abc");
            assert_eq!(
                view.read_with(cx, |editor, _| editor.vim_state.state.command_text()),
                ""
            );
            cx.simulate_keystrokes("x");
            assert_eq!(vim_test_value(&view, cx), "bc");

            set_vim_test_content(&view, "abc", Position::new(0, 0), cx);
            cx.simulate_keystrokes("2 d p x");
            assert_eq!(vim_test_value(&view, cx), "bc");

            set_vim_test_content(&view, "abc", Position::new(0, 0), cx);
            cx.simulate_keystrokes("d x");
            assert_eq!(vim_test_value(&view, cx), "abc");
            cx.simulate_keystrokes("x");
            assert_eq!(vim_test_value(&view, cx), "bc");
        });
    }

    #[gpui::test]
    fn open_line_preserves_crlf_above_below_and_at_the_final_line(cx: &mut gpui::TestAppContext) {
        with_vim_editor(cx, "one\r\ntwo", |view, cx| {
            cx.simulate_keystrokes("o");
            assert_eq!(vim_test_value(&view, cx), "one\r\n\r\ntwo");
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape");
            assert_eq!(vim_test_value(&view, cx), "one\r\nX\r\ntwo");

            set_vim_test_content(&view, "one\r\ntwo", Position::new(1, 0), cx);
            cx.simulate_keystrokes("shift-o");
            assert_eq!(vim_test_value(&view, cx), "one\r\n\r\ntwo");
            cx.simulate_input("Y");
            cx.simulate_keystrokes("escape");
            assert_eq!(vim_test_value(&view, cx), "one\r\nY\r\ntwo");

            set_vim_test_content(&view, "one\r\ntwo", Position::new(1, 0), cx);
            cx.simulate_keystrokes("o");
            assert_eq!(vim_test_value(&view, cx), "one\r\ntwo\r\n");
            cx.simulate_input("Z");
            cx.simulate_keystrokes("escape");
            assert_eq!(vim_test_value(&view, cx), "one\r\ntwo\r\nZ");

            set_vim_test_content(&view, "one\r\ntwo", Position::new(0, 0), cx);
            cx.simulate_keystrokes("shift-o");
            assert_eq!(vim_test_value(&view, cx), "\r\none\r\ntwo");
            cx.simulate_input("A");
            cx.simulate_keystrokes("escape");
            assert_eq!(vim_test_value(&view, cx), "A\r\none\r\ntwo");
        });
    }

    #[gpui::test]
    fn character_find_repeats_reverses_and_composes_with_operators(cx: &mut gpui::TestAppContext) {
        with_vim_editor(cx, "", |view, cx| {
            set_vim_test_content(&view, "a-b-a-b tail", Position::new(0, 0), cx);
            cx.simulate_keystrokes("f b");
            assert_eq!(
                view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
                2
            );
            cx.simulate_keystrokes(";");
            assert_eq!(
                view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
                6
            );
            cx.simulate_keystrokes(",");
            assert_eq!(
                view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
                2
            );

            cx.simulate_keystrokes("f z");
            cx.simulate_keystrokes(";");
            assert_eq!(
                view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
                6,
                "a failed find must preserve the previous successful find"
            );

            set_vim_test_content(&view, "abXcdXef", Position::new(0, 0), cx);
            cx.simulate_keystrokes("d t shift-x->X");
            assert_eq!(vim_test_value(&view, cx), "XcdXef");
            set_vim_test_content(&view, "abXcdXef", Position::new(0, 0), cx);
            cx.simulate_keystrokes("d 2 f shift-x->X");
            assert_eq!(vim_test_value(&view, cx), "ef");

            set_vim_test_content(&view, "(a)b", Position::new(0, 0), cx);
            cx.simulate_keystrokes("f shift-0->)");
            assert_eq!(
                view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
                2
            );
        });
    }

    #[gpui::test]
    fn replace_character_handles_counts_unicode_crlf_visual_ranges_and_failure(
        cx: &mut gpui::TestAppContext,
    ) {
        with_vim_editor(cx, "", |view, cx| {
            set_vim_test_content(&view, "a中bc", Position::new(0, 0), cx);
            cx.simulate_keystrokes("2 r x");
            assert_eq!(vim_test_value(&view, cx), "xxbc");
            assert_eq!(
                cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
                Some("a中".to_string())
            );

            set_vim_test_content(&view, "abc", Position::new(0, 1), cx);
            cx.simulate_keystrokes("r λ");
            assert_eq!(vim_test_value(&view, cx), "aλc");

            set_vim_test_content(&view, "ab\r\ncd", Position::new(0, 1), cx);
            cx.simulate_keystrokes("r enter");
            assert_eq!(vim_test_value(&view, cx), "a\r\n\r\ncd");

            set_vim_test_content(&view, "abcd\r\nef", Position::new(0, 0), cx);
            cx.simulate_keystrokes("v 2 l r z");
            assert_eq!(vim_test_value(&view, cx), "zzzd\r\nef");

            set_vim_test_content(&view, "ab", Position::new(0, 1), cx);
            cx.simulate_keystrokes("2 r x");
            assert_eq!(vim_test_value(&view, cx), "ab", "r must fail atomically");
        });
    }

    #[gpui::test]
    fn dot_repeats_normal_operator_find_and_visual_changes(cx: &mut gpui::TestAppContext) {
        with_vim_editor(cx, "", |view, cx| {
            set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
            cx.simulate_keystrokes("x");
            cx.simulate_keystrokes("w .");
            assert_eq!(vim_test_value(&view, cx), "ne wo");
            cx.simulate_keystrokes("u");
            assert_eq!(vim_test_value(&view, cx), "ne two");

            set_vim_test_content(&view, "one\ntwo\nthree\n", Position::new(0, 0), cx);
            cx.simulate_keystrokes("d d .");
            assert_eq!(vim_test_value(&view, cx), "three\n");

            set_vim_test_content(&view, "aXbXcX", Position::new(0, 0), cx);
            cx.simulate_keystrokes("d f shift-x->X .");
            assert_eq!(vim_test_value(&view, cx), "cX");

            set_vim_test_content(&view, "abcdef", Position::new(0, 0), cx);
            cx.simulate_keystrokes("v l d l .");
            assert_eq!(vim_test_value(&view, cx), "cf");
        });
    }

    #[gpui::test]
    fn dot_replays_insert_change_open_line_unicode_and_replacement(cx: &mut gpui::TestAppContext) {
        with_vim_editor(cx, "", |view, cx| {
            set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
            cx.simulate_keystrokes("3 i");
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape");
            assert_eq!(vim_test_value(&view, cx), "XXXone two");

            set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
            cx.simulate_keystrokes("i");
            cx.simulate_input("λ");
            cx.simulate_keystrokes("escape w .");
            assert_eq!(vim_test_value(&view, cx), "λone λtwo");

            set_vim_test_content(&view, "one two three", Position::new(0, 0), cx);
            cx.simulate_keystrokes("c w");
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape w .");
            assert_eq!(vim_test_value(&view, cx), "Xtwo X");

            set_vim_test_content(&view, "one\r\ntwo", Position::new(0, 0), cx);
            cx.simulate_keystrokes("o");
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape shift-g .");
            assert_eq!(vim_test_value(&view, cx), "one\r\nX\r\ntwo\r\nX");

            set_vim_test_content(&view, "abcd", Position::new(0, 0), cx);
            cx.simulate_keystrokes("r x l .");
            assert_eq!(vim_test_value(&view, cx), "xxcd");
        });
    }

    #[gpui::test]
    fn dot_count_overrides_the_original_count_and_failed_commands_do_not_replace_it(
        cx: &mut gpui::TestAppContext,
    ) {
        with_vim_editor(cx, "", |view, cx| {
            set_vim_test_content(&view, "abcdefghij", Position::new(0, 0), cx);
            cx.simulate_keystrokes("2 x l 3 .");
            assert_eq!(vim_test_value(&view, cx), "cghij");

            set_vim_test_content(&view, "abcdef", Position::new(0, 0), cx);
            cx.simulate_keystrokes("x d p .");
            assert_eq!(vim_test_value(&view, cx), "cdef");
        });
    }

    #[gpui::test]
    fn dot_repeats_text_objects_line_changes_paste_and_join(cx: &mut gpui::TestAppContext) {
        with_vim_editor(cx, "", |view, cx| {
            set_vim_test_content(&view, "say \"one\" then \"two\"", Position::new(0, 6), cx);
            cx.simulate_keystrokes("d i shift-'->\" w w .");
            assert_eq!(vim_test_value(&view, cx), "say \"\" then \"\"");

            set_vim_test_content(&view, "one tail\nnext tail", Position::new(0, 4), cx);
            cx.simulate_keystrokes("shift-c");
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape j 0 w .");
            assert_eq!(vim_test_value(&view, cx), "one X\nnext X");

            set_vim_test_content(&view, "a\n  b\nc\n  d", Position::new(0, 0), cx);
            cx.simulate_keystrokes("shift-j j .");
            assert_eq!(vim_test_value(&view, cx), "a b\nc d");

            set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
            cx.simulate_keystrokes("y w shift-4->$ p .");
            assert_eq!(vim_test_value(&view, cx), "one twoone one ");
        });
    }

    #[gpui::test]
    fn dot_captures_insert_backspace_markdown_continuation_and_insert_counts(
        cx: &mut gpui::TestAppContext,
    ) {
        with_vim_editor(cx, "", |view, cx| {
            set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
            cx.simulate_keystrokes("i");
            cx.simulate_input("abc");
            cx.simulate_keystrokes("backspace escape w .");
            assert_eq!(vim_test_value(&view, cx), "abone abtwo");

            set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
            cx.simulate_keystrokes("i");
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape w 3 .");
            assert_eq!(vim_test_value(&view, cx), "Xone XXXtwo");

            cx.update(|window, cx| {
                view.update(cx, |editor, cx| {
                    editor.kind = super::super::DocumentKind::Markdown;
                    editor.replace_content_for_test("- one\n- two", window, cx);
                    editor.editor.update(cx, |input, cx| {
                        input.set_cursor_position(Position::new(0, 5), window, cx);
                    });
                    editor.reset_vim_command();
                    editor.focus_source_mode(window, cx);
                });
            });
            cx.simulate_keystrokes("shift-a enter");
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape shift-g .");
            assert_eq!(vim_test_value(&view, cx), "- one\n- X\n- two\n- X");

            set_vim_test_content(&view, "one", Position::new(0, 0), cx);
            cx.simulate_keystrokes("o");
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape 2 .");
            assert_eq!(vim_test_value(&view, cx), "one\nX\nX\nX");
        });
    }

    #[gpui::test]
    fn visual_line_dot_is_one_modal_undo_step_and_redoes_as_one_step(
        cx: &mut gpui::TestAppContext,
    ) {
        with_vim_editor(cx, "one\ntwo\nthree\nfour\n", |view, cx| {
            cx.simulate_keystrokes("shift-v j d .");
            assert_eq!(vim_test_value(&view, cx), "");
            cx.simulate_keystrokes("u");
            assert_eq!(vim_test_value(&view, cx), "three\nfour\n");
            cx.simulate_keystrokes("ctrl-r");
            assert_eq!(vim_test_value(&view, cx), "");
        });
    }

    #[gpui::test]
    fn compound_dot_replay_emits_one_editor_change(cx: &mut gpui::TestAppContext) {
        with_vim_editor(cx, "one two", |view, cx| {
            let changes = Rc::new(Cell::new(0));
            cx.update(|_, cx| {
                let input = view.read(cx).editor.clone();
                let changes = changes.clone();
                cx.subscribe(&input, move |_, event: &InputEvent, _| {
                    if matches!(event, InputEvent::Change) {
                        changes.set(changes.get() + 1);
                    }
                })
                .detach();
            });

            cx.simulate_keystrokes("c i w");
            cx.simulate_input("X");
            cx.simulate_keystrokes("escape");
            changes.set(0);

            cx.simulate_keystrokes("w .");
            assert_eq!(vim_test_value(&view, cx), "X X");
            assert_eq!(changes.get(), 1);
        });
    }

    #[gpui::test]
    fn cancelled_and_failed_character_arguments_preserve_the_last_change(
        cx: &mut gpui::TestAppContext,
    ) {
        with_vim_editor(cx, "abcdef", |view, cx| {
            cx.simulate_keystrokes("x f escape l .");
            assert_eq!(vim_test_value(&view, cx), "bdef");

            set_vim_test_content(&view, "abc", Position::new(0, 0), cx);
            cx.simulate_keystrokes("x 9 r z .");
            assert_eq!(vim_test_value(&view, cx), "c");
        });
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
            cx.set_global(AppServices::new(Arc::new(db), PathBuf::new()));
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
            if view.read_with(&cx, |editor, _| !editor.persistence.is_loading) {
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
                editor.vim_state.search_active,
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
                .analysis
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
