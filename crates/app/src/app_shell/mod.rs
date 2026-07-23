mod action;
mod handler;
mod home;
mod render;
mod settings;
mod tabs;
mod workspace;

pub(crate) use action::*;
use entity::{board, note, note::Entity as Note};
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    MouseButton, ParentElement, PathPromptOptions, Pixels, Render, SharedString, Styled, Task,
    Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme, IconName, Root, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{
        Escape as InputEscape, InputEvent, InputState, MoveDown as InputMoveDown,
        MoveUp as InputMoveUp,
    },
    menu::ContextMenuExt as _,
    sidebar::SidebarToggleButton,
    tab::{Tab, TabBar},
    v_flex,
};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};
use std::{collections::HashMap, sync::Arc};

use crate::DB;
use crate::app_settings::{AppSettings, StoredTab};
use crate::board::BoardView;
use crate::command_palette::{CommandPalette, CommandPaletteMode};
use crate::document_editor::{
    DEFAULT_NOTE, DocumentEditorEvent, DocumentEditorView, DocumentKind, SaveState, now_ts,
    unique_note_path,
};
use crate::home::WorkspaceHomeState;
use crate::sidebar::{SidebarEvent, SidebarView};
use crate::trash::{TrashItem, TrashItemKind};

const SIDEBAR_AUTO_COLLAPSE_WIDTH: f32 = 900.;

struct OpenTab {
    id: u64,
    title: SharedString,
    kind: OpenTabKind,
}

enum OpenTabKind {
    Chooser,
    Trash,
    Board {
        board_id: u32,
        project_id: Option<u32>,
        view: Entity<BoardView>,
    },
    Note {
        note_id: u32,
        project_id: Option<u32>,
        view: Entity<DocumentEditorView>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum WorkspaceTitleTarget {
    Board(u32),
    Note(u32),
}

struct PendingWorkspaceTitleSave {
    generation: u64,
    title: String,
}

#[derive(Clone)]
pub(crate) struct ProjectChoice {
    pub(crate) id: u32,
    pub(crate) name: SharedString,
}

#[derive(Clone)]
pub(crate) struct BoardChoice {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) project_id: Option<u32>,
    pub(crate) project_name: Option<SharedString>,
}

#[derive(Clone)]
pub(crate) struct NoteChoice {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) project_id: Option<u32>,
    pub(crate) project_name: Option<SharedString>,
}

pub struct AppShell {
    pub(crate) focus_handle: FocusHandle,
    sidebar: Entity<SidebarView>,
    title_input: Entity<InputState>,
    pub(crate) command_palette: CommandPalette,
    open_tabs: Vec<OpenTab>,
    note_views: HashMap<u32, Entity<DocumentEditorView>>,
    active_tab_index: usize,
    next_tab_id: u64,
    pub(crate) projects: Vec<ProjectChoice>,
    pub(crate) boards: Vec<BoardChoice>,
    pub(crate) notes: Vec<NoteChoice>,
    pub(crate) active_project_id: Option<u32>,
    suppress_title_event: bool,
    settings_dialog_open: bool,
    window_is_narrow: bool,
    home_state: WorkspaceHomeState,
    home_loaded: bool,
    home_refreshing: bool,
    home_refresh_pending: bool,
    home_error: Option<SharedString>,
    trash_items: Vec<TrashItem>,
    trash_loaded: bool,
    trash_refreshing: bool,
    trash_refresh_pending: bool,
    trash_error: Option<SharedString>,
    trash_search_input: Entity<InputState>,
    trash_query: String,
    trash_kind_filter: Option<TrashItemKind>,
    workspace_refreshing: bool,
    workspace_refresh_pending: bool,
    pending_workspace_title_saves: HashMap<WorkspaceTitleTarget, PendingWorkspaceTitleSave>,
    workspace_title_save_lock: Arc<tokio::sync::Mutex<()>>,
    record_opened_generation: u64,
    tab_session_save_generation: u64,
    external_change_task: Option<Task<()>>,
    last_change_revision: Option<i64>,
    last_board_revision: Option<i64>,
    last_note_revision: Option<i64>,
}

impl AppShell {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn observe_document_editor(view: &Entity<DocumentEditorView>, cx: &mut Context<Self>) {
        cx.subscribe(
            view,
            |this, _, event: &DocumentEditorEvent, cx| match event {
                DocumentEditorEvent::PathChanged => this.refresh_workspace(cx),
                DocumentEditorEvent::Saved(note_id) => {
                    if !this.open_tabs.iter().any(|tab| {
                        matches!(
                            &tab.kind,
                            OpenTabKind::Note {
                                note_id: open_note_id,
                                ..
                            } if *open_note_id == *note_id
                        )
                    }) {
                        this.note_views.remove(note_id);
                    }
                }
            },
        )
        .detach();
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tab_session = AppSettings::tab_session(cx);
        let sidebar = SidebarView::view(window, cx);
        let mut open_tabs = Vec::with_capacity(tab_session.tabs.len().max(1));
        let mut note_views = HashMap::new();
        let mut next_tab_id = 1_u64;
        for stored_tab in tab_session.tabs {
            let (title, kind) = match stored_tab {
                StoredTab::Chooser => (SharedString::from("Home"), OpenTabKind::Chooser),
                StoredTab::Trash => (SharedString::from("Trash"), OpenTabKind::Trash),
                StoredTab::Board {
                    board_id,
                    project_id,
                    title,
                } => {
                    let view = BoardView::view(window, cx);
                    view.update(cx, |board, cx| board.load_board(board_id, cx));
                    (
                        SharedString::from(title),
                        OpenTabKind::Board {
                            board_id,
                            project_id,
                            view,
                        },
                    )
                }
                StoredTab::Note {
                    note_id,
                    project_id,
                    title,
                } => {
                    let view = DocumentEditorView::view(note_id, window, cx);
                    Self::observe_document_editor(&view, cx);
                    note_views.insert(note_id, view.clone());
                    (
                        SharedString::from(title),
                        OpenTabKind::Note {
                            note_id,
                            project_id,
                            view,
                        },
                    )
                }
            };
            open_tabs.push(OpenTab {
                id: next_tab_id,
                title,
                kind,
            });
            next_tab_id = next_tab_id.saturating_add(1);
        }
        if open_tabs.is_empty() {
            open_tabs.push(OpenTab {
                id: next_tab_id,
                title: "Home".into(),
                kind: OpenTabKind::Chooser,
            });
            next_tab_id = next_tab_id.saturating_add(1);
        }
        let active_tab_index = tab_session.active_tab_index.min(open_tabs.len() - 1);
        let active_title = open_tabs[active_tab_index].title.to_string();
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Home")
                .default_value(active_title)
        });

        let command_palette = CommandPalette::new(window, cx);
        let trash_search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search Trash..."));

        cx.subscribe(&title_input, |this, input, event: &InputEvent, cx| {
            if !matches!(event, InputEvent::Change) || this.suppress_title_event {
                return;
            }

            let title = input.read(cx).text().to_string();
            this.rename_active_tab(title, cx);
        })
        .detach();

        cx.subscribe_in(
            &command_palette.input,
            window,
            |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    if this.command_palette.suppress_input_event {
                        return;
                    }

                    this.command_palette.query = input.read(cx).text().to_string();
                    this.command_palette.selected_index = 0;
                    this.command_palette.scroll_handle.scroll_to_item(0);

                    if this.command_palette.mode == CommandPaletteMode::Search {
                        this.run_workspace_search(cx);
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.execute_first_command_palette_match(window, cx);
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe(
            &trash_search_input,
            |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.trash_query = input.read(cx).text().to_string();
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &sidebar,
            window,
            |this, _, event: &SidebarEvent, window, cx| match event {
                SidebarEvent::OpenHome => this.open_home(window, cx),
                SidebarEvent::OpenTrash => this.open_trash(window, cx),
                SidebarEvent::OpenThemeSwitcher => this.open_theme_switcher(window, cx),
                SidebarEvent::WidthChanged => cx.notify(),
                SidebarEvent::WorkspaceChanged => {
                    this.load_home(cx);
                    this.load_trash(cx);
                    this.refresh_workspace(cx);
                }
                SidebarEvent::OpenBoard {
                    board_id,
                    project_id,
                    title,
                } => {
                    this.active_project_id = *project_id;
                    this.open_board_tab(*board_id, *project_id, title.clone(), window, cx);
                }
                SidebarEvent::OpenNote {
                    note_id,
                    project_id,
                    title,
                } => {
                    this.active_project_id = *project_id;
                    this.open_note_tab(*note_id, *project_id, title.clone(), window, cx);
                }
                SidebarEvent::ActivateProject { project_id } => {
                    this.activate_project(*project_id, window, cx);
                }
                SidebarEvent::BoardRenamed { board_id, title } => {
                    let mut renamed_active = false;
                    for (i, tab) in this.open_tabs.iter_mut().enumerate() {
                        if let OpenTabKind::Board { board_id: id, .. } = &tab.kind
                            && *id == *board_id
                        {
                            tab.title = title.clone();
                            renamed_active = i == this.active_tab_index;
                            break;
                        }
                    }
                    if renamed_active {
                        this.sync_title_input(window, cx);
                    }
                    if let Some(board) = this.boards.iter_mut().find(|board| board.id == *board_id) {
                        board.title = title.clone();
                    }
                    this.rebuild_command_palette_workspace_commands();
                    this.persist_tab_session(cx);
                    cx.notify();
                }
                SidebarEvent::NoteRenamed { note_id, title } => {
                    let mut renamed_active = false;
                    for (i, tab) in this.open_tabs.iter_mut().enumerate() {
                        if let OpenTabKind::Note { note_id: id, view, .. } = &tab.kind
                            && *id == *note_id
                        {
                            tab.title = title.clone();
                            renamed_active = i == this.active_tab_index;
                            let view = view.clone();
                            view.update(cx, |note, cx| {
                                note.apply_title(title, cx);
                            });
                            break;
                        }
                    }
                    if renamed_active {
                        this.sync_title_input(window, cx);
                    }
                    if let Some(note) = this.notes.iter_mut().find(|note| note.id == *note_id) {
                        note.title = title.clone();
                    }
                    this.rebuild_command_palette_workspace_commands();
                    this.persist_tab_session(cx);
                    cx.notify();
                }
                SidebarEvent::BoardDeleted { board_id } => {
                    if let Some(index) = this.open_tabs.iter().position(
                        |tab| matches!(&tab.kind, OpenTabKind::Board { board_id: id, .. } if *id == *board_id),
                    ) {
                        this.close_tab(index, window, cx);
                    }
                }
                SidebarEvent::NoteDeleted { note_id } => {
                    if let Some(index) = this
                        .open_tabs
                        .iter()
                        .position(|tab| matches!(&tab.kind, OpenTabKind::Note { note_id: id, .. } if *id == *note_id))
                    {
                        this.close_tab(index, window, cx);
                    }
                }
                SidebarEvent::ProjectRenamed { project_id, name } => {
                    for project in &mut this.projects {
                        if project.id == *project_id {
                            project.name = name.clone();
                        }
                    }

                    for board in &mut this.boards {
                        if board.project_id == Some(*project_id) {
                            board.project_name = Some(name.clone());
                        }
                    }

                    for note in &mut this.notes {
                        if note.project_id == Some(*project_id) {
                            note.project_name = Some(name.clone());
                        }
                    }

                    this.rebuild_command_palette_workspace_commands();
                    cx.notify();
                }
                SidebarEvent::ProjectDeleted { project_id } => {
                    this.close_project_tabs(*project_id, window, cx);
                    if this.active_project_id == Some(*project_id) {
                        this.active_project_id = None;
                    }
                    this.persist_tab_session(cx);
                }
                SidebarEvent::ProjectsReordered => {
                    this.refresh_workspace(cx);
                }
            },
        )
        .detach();

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            sidebar,
            title_input,
            command_palette,
            open_tabs,
            note_views,
            active_tab_index,
            next_tab_id,
            projects: vec![],
            boards: vec![],
            notes: vec![],
            active_project_id: tab_session.active_project_id,
            suppress_title_event: false,
            settings_dialog_open: false,
            window_is_narrow: false,
            home_state: WorkspaceHomeState::default(),
            home_loaded: false,
            home_refreshing: false,
            home_refresh_pending: false,
            home_error: None,
            trash_items: Vec::new(),
            trash_loaded: false,
            trash_refreshing: false,
            trash_refresh_pending: false,
            trash_error: None,
            trash_search_input,
            trash_query: String::new(),
            trash_kind_filter: None,
            workspace_refreshing: false,
            workspace_refresh_pending: false,
            pending_workspace_title_saves: HashMap::new(),
            workspace_title_save_lock: Arc::new(tokio::sync::Mutex::new(())),
            record_opened_generation: 0,
            tab_session_save_generation: 0,
            external_change_task: None,
            last_change_revision: None,
            last_board_revision: None,
            last_note_revision: None,
        };

        let show_sidebar = AppSettings::show_sidebar(cx);
        this.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_collapsed(!show_sidebar, cx);
        });
        this.sync_sidebar_with_window_width(window.bounds().size.width, cx);
        cx.observe_window_bounds(window, |this, window, cx| {
            this.sync_sidebar_with_window_width(window.bounds().size.width, cx);
        })
        .detach();
        cx.on_app_quit(|this, cx| this.flush_pending_workspace_title_saves(cx))
            .detach();
        this.start_external_change_watcher(window, cx);
        this.refresh_workspace(cx);
        this.sync_sidebar_active(cx);
        this.load_home(cx);
        this.load_trash(cx);
        this
    }
}
