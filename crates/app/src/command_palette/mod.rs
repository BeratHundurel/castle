mod action;
mod commands;
mod handler;
mod render;
mod search_preview;
mod search_render;

pub(crate) use action::*;

use gpui::{AppContext, Entity, ScrollHandle, SharedString, Task, Window};
use gpui_component::{IconName, input::InputState};

use crate::search::SearchResult;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandPaletteMode {
    Commands,
    Search,
    Themes,
}

#[derive(Clone)]
pub(crate) struct PaletteCommand {
    pub(crate) label: SharedString,
    pub(crate) subtitle: SharedString,
    pub(crate) icon: IconName,
    pub(crate) kind: PaletteCommandKind,
}

pub(crate) struct SearchablePaletteCommand {
    pub(crate) command: PaletteCommand,
    pub(crate) search_text: String,
}

#[derive(Clone)]
pub(crate) enum PaletteCommandKind {
    OpenNote {
        note_id: u32,
        project_id: Option<u32>,
        title: SharedString,
    },
    OpenBoard {
        board_id: u32,
        project_id: Option<u32>,
        title: SharedString,
    },
    NewNote {
        project_id: Option<u32>,
        title: String,
    },
    NewBoard {
        project_id: Option<u32>,
        title: String,
    },
    OpenFile,
    NewTab,
    CloseAllTabs,
    SwitchTheme,
    SearchWorkspace,
}

pub(crate) struct CommandPalette {
    pub(crate) input: Entity<InputState>,
    pub(crate) open: bool,
    pub(crate) mode: CommandPaletteMode,
    pub(crate) query: String,
    pub(crate) selected_index: usize,
    pub(crate) scroll_handle: ScrollHandle,
    pub(crate) suppress_input_event: bool,
    pub(crate) workspace_commands: Vec<SearchablePaletteCommand>,
    pub(crate) search_results: Vec<SearchResult>,
    pub(crate) search_loading: bool,
    pub(crate) search_error: Option<SharedString>,
    pub(crate) search_generation: i64,
    pub(crate) search_debounce_task: Option<Task<()>>,
}

impl CommandPalette {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut gpui::Context<crate::app_shell::AppShell>,
    ) -> Self {
        Self {
            input: cx.new(|cx| InputState::new(window, cx).placeholder("Type a command")),
            open: false,
            mode: CommandPaletteMode::Commands,
            query: String::new(),
            selected_index: 0,
            scroll_handle: ScrollHandle::new(),
            suppress_input_event: false,
            workspace_commands: Vec::new(),
            search_results: Vec::new(),
            search_loading: false,
            search_error: None,
            search_generation: 0,
            search_debounce_task: None,
        }
    }
}
