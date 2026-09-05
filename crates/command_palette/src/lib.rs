#![recursion_limit = "256"]

mod action;
mod commands;
mod controller;
mod search;
mod view;

pub use action::{
    CloseCommandPaletteAction, CommandPaletteAction, OpenWorkspaceSearchAction,
    SelectNextCommandPaletteItem, SelectPrevCommandPaletteItem, SwitchThemeAction,
};

use std::{cell::Cell, collections::HashMap};

use gpui_kit::component::{
    IconName,
    input::{InputEvent, InputState},
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, ScrollHandle, SharedString, Task, Window,
};
use storage::workspace::{WorkspaceRows, search::SearchResult};

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandPaletteMode {
    Commands,
    Search,
    Themes,
}

#[derive(Clone)]
struct PaletteCommand {
    label: SharedString,
    subtitle: SharedString,
    icon: IconName,
    kind: PaletteCommandKind,
}

struct SearchablePaletteCommand {
    command: PaletteCommand,
    search_text: String,
}

#[derive(Clone)]
enum PaletteCommandKind {
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
    ImportWorkspace,
    ExportWorkspace,
    NewTab,
    CloseAllTabs,
    OpenSettings,
    SwitchTheme,
    SearchWorkspace,
    CreateCardFromSelection,
    InsertBoardView,
}

#[derive(Clone)]
pub enum CommandPaletteEvent {
    Closed,
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
    ImportWorkspace,
    ExportWorkspace,
    NewTab,
    CloseAllTabs,
    OpenSettings,
    CreateCardFromSelection,
    InsertBoardView,
    OpenSearchResult(SearchResult),
}

pub struct CommandPaletteView {
    input: Entity<InputState>,
    open: bool,
    mode: CommandPaletteMode,
    query: String,
    selected_index: usize,
    scroll_handle: ScrollHandle,
    search_preview_scroll_handle: ScrollHandle,
    search_preview_scroll_pending: Cell<bool>,
    suppress_input_event: bool,
    workspace_commands: Vec<SearchablePaletteCommand>,
    project_names: HashMap<u32, SharedString>,
    active_project_id: Option<u32>,
    search_results: Vec<SearchResult>,
    search_loading: bool,
    search_error: Option<SharedString>,
    search_generation: i64,
    search_debounce_task: Option<Task<()>>,
}

impl CommandPaletteView {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Type a command"));
        cx.subscribe_in(
            &input,
            window,
            |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    if this.suppress_input_event {
                        return;
                    }

                    this.query = input.read(cx).text().to_string();
                    this.selected_index = 0;
                    this.search_preview_scroll_pending.set(true);
                    this.scroll_handle.scroll_to_item(0);
                    if this.mode == CommandPaletteMode::Search {
                        this.run_workspace_search(cx);
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.execute_selected(window, cx),
                _ => {}
            },
        )
        .detach();

        Self {
            input,
            open: false,
            mode: CommandPaletteMode::Commands,
            query: String::new(),
            selected_index: 0,
            scroll_handle: ScrollHandle::new(),
            search_preview_scroll_handle: ScrollHandle::new(),
            search_preview_scroll_pending: Cell::new(true),
            suppress_input_event: false,
            workspace_commands: Vec::new(),
            project_names: HashMap::new(),
            active_project_id: None,
            search_results: Vec::new(),
            search_loading: false,
            search_error: None,
            search_generation: 0,
            search_debounce_task: None,
        }
    }

    pub fn apply_workspace_rows(&mut self, rows: &WorkspaceRows, cx: &mut Context<Self>) {
        self.project_names = rows
            .projects
            .iter()
            .map(|project| (project.id, SharedString::from(project.name.as_str())))
            .collect();

        let note_commands = rows.notes.iter().map(|note| {
            self.searchable_workspace_command(note.id, note.project_id, &note.title, true)
        });
        let board_commands = rows.boards.iter().map(|board| {
            self.searchable_workspace_command(board.id, board.project_id, &board.title, false)
        });
        self.workspace_commands = note_commands.chain(board_commands).collect();
        cx.notify();
    }

    pub fn set_active_project(&mut self, project_id: Option<u32>) {
        self.active_project_id = project_id;
    }

    pub fn rename_note(&mut self, note_id: u32, title: SharedString, cx: &mut Context<Self>) {
        self.rename_workspace_item(note_id, title, true);
        cx.notify();
    }

    pub fn rename_board(&mut self, board_id: u32, title: SharedString, cx: &mut Context<Self>) {
        self.rename_workspace_item(board_id, title, false);
        cx.notify();
    }

    pub fn rename_project(&mut self, project_id: u32, name: SharedString, cx: &mut Context<Self>) {
        self.project_names.insert(project_id, name.clone());
        for entry in &mut self.workspace_commands {
            let matches_project = match &entry.command.kind {
                PaletteCommandKind::OpenNote {
                    project_id: item_project_id,
                    ..
                }
                | PaletteCommandKind::OpenBoard {
                    project_id: item_project_id,
                    ..
                } => *item_project_id == Some(project_id),
                _ => false,
            };
            if matches_project {
                let item_kind = if matches!(entry.command.kind, PaletteCommandKind::OpenNote { .. })
                {
                    "Note"
                } else {
                    "Board"
                };
                entry.command.subtitle = SharedString::from(format!("{item_kind} - {name}"));
                entry.search_text = commands::search_text(&entry.command);
            }
        }
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    fn searchable_workspace_command(
        &self,
        id: u32,
        project_id: Option<u32>,
        title: &str,
        is_note: bool,
    ) -> SearchablePaletteCommand {
        let project_name = project_id
            .and_then(|id| self.project_names.get(&id).cloned())
            .unwrap_or_else(|| "Standalone".into());
        let item_kind = if is_note { "Note" } else { "Board" };
        let command = PaletteCommand {
            label: SharedString::from(format!("Go to: {title}")),
            subtitle: SharedString::from(format!("{item_kind} - {project_name}")),
            icon: if is_note {
                IconName::BookOpen
            } else {
                IconName::LayoutDashboard
            },
            kind: if is_note {
                PaletteCommandKind::OpenNote {
                    note_id: id,
                    project_id,
                    title: SharedString::from(title),
                }
            } else {
                PaletteCommandKind::OpenBoard {
                    board_id: id,
                    project_id,
                    title: SharedString::from(title),
                }
            },
        };
        commands::searchable_command(command)
    }

    fn rename_workspace_item(&mut self, item_id: u32, title: SharedString, is_note: bool) {
        for entry in &mut self.workspace_commands {
            let matches_item = match &entry.command.kind {
                PaletteCommandKind::OpenNote { note_id, .. } => is_note && *note_id == item_id,
                PaletteCommandKind::OpenBoard { board_id, .. } => !is_note && *board_id == item_id,
                _ => false,
            };
            if !matches_item {
                continue;
            }

            entry.command.label = SharedString::from(format!("Go to: {title}"));
            match &mut entry.command.kind {
                PaletteCommandKind::OpenNote {
                    title: command_title,
                    ..
                }
                | PaletteCommandKind::OpenBoard {
                    title: command_title,
                    ..
                } => command_title.clone_from(&title),
                _ => {}
            }
            entry.search_text = commands::search_text(&entry.command);
            break;
        }
    }
}

impl EventEmitter<CommandPaletteEvent> for CommandPaletteView {}
