use gpui::{
    AppContext as _, ClipboardItem, Context, EntityInputHandler, Focusable as _, KeyDownEvent,
    MouseDownEvent, Window,
};
use gpui_component::{
    highlighter::Language,
    input::{EditorState, Position, Redo, Rope, RopeExt as _, Search, Undo},
};
use std::ops::Range;

use super::DocumentEditorView;
use super::action::{VimKey, VimKeyAction};
use super::formatting::markdown_newline_prefix;
use settings::AppSettings;

mod editing;
mod find;
mod history;
mod key_dispatch;
mod motions;
mod operators;
mod replay;
mod replay_actions;
mod text_objects;
mod view;

use find::*;
use motions::*;
use replay::*;
use text_objects::*;

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

#[cfg(test)]
mod tests;
