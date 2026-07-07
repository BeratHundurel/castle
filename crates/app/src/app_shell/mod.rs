mod action;
mod handler;
mod render;
mod tabs;
mod workspace;

pub(crate) use action::*;
use anyhow::Result;
use entity::{board, note, note::Entity as Note};
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    MouseButton, ParentElement, PathPromptOptions, Render, SharedString, Styled, Window, div,
    prelude::FluentBuilder as _, px,
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

use crate::DB;
use crate::board::BoardView;
use crate::command_palette::{CommandPalette, CommandPaletteMode};
use crate::markdown_editor::{
    DEFAULT_NOTE, MarkdownEditorView, SaveState, now_ts, unique_note_path,
};
use crate::sidebar::{SidebarEvent, SidebarView};

struct OpenTab {
    id: u64,
    title: SharedString,
    kind: OpenTabKind,
}

enum OpenTabKind {
    Chooser,
    Board {
        board_id: u32,
        project_id: Option<u32>,
        view: Entity<BoardView>,
    },
    Note {
        note_id: u32,
        project_id: Option<u32>,
        view: Entity<MarkdownEditorView>,
    },
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
    active_tab_index: usize,
    next_tab_id: u64,
    pub(crate) projects: Vec<ProjectChoice>,
    pub(crate) boards: Vec<BoardChoice>,
    pub(crate) notes: Vec<NoteChoice>,
    pub(crate) active_project_id: Option<u32>,
    suppress_title_event: bool,
}

impl AppShell {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sidebar = SidebarView::view(window, cx);
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("New tab")
                .default_value("New tab")
        });

        let command_palette = CommandPalette::new(window, cx);

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

        cx.subscribe_in(
            &sidebar,
            window,
            |this, _, event: &SidebarEvent, window, cx| match event {
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
                                note.set_title(title.to_string(), cx);
                            });
                            break;
                        }
                    }
                    if renamed_active {
                        this.sync_title_input(window, cx);
                    }
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
                SidebarEvent::ProjectDeleted { project_id }
                | SidebarEvent::ProjectArchived { project_id } => {
                    if this.active_project_id == Some(*project_id) {
                        this.active_project_id = None;
                    }
                    this.refresh_workspace(cx);
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
            open_tabs: vec![OpenTab {
                id: 1,
                title: "New tab".into(),
                kind: OpenTabKind::Chooser,
            }],
            active_tab_index: 0,
            next_tab_id: 2,
            projects: vec![],
            boards: vec![],
            notes: vec![],
            active_project_id: None,
            suppress_title_event: false,
        };

        this.refresh_workspace(cx);
        this.sidebar
            .update(cx, |_, cx| SidebarView::list_projects(cx));
        this
    }
}
