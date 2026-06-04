mod action;
mod handler;
mod render;

pub(crate) use action::*;

use gpui::{AppContext, Entity, ScrollHandle, SharedString, Window};
use gpui_component::{IconName, input::InputState};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandPaletteMode {
    Commands,
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
        }
    }
}
