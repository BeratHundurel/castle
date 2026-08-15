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
use app_settings::AppSettings;

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
mod tests;
