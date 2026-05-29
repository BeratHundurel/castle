mod action;
mod handler;
mod render;
mod tabs;
mod workspace;

pub(crate) use action::*;
use anyhow::Result;
use entity::{
    board, board::Entity as Board, note, note::Entity as Note, project::Entity as Project,
};
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    MouseButton, ParentElement, PathPromptOptions, Render, SharedString, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme, IconName, Root, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::ContextMenuExt as _,
    scroll::ScrollableElement as _,
    sidebar::SidebarToggleButton,
    tab::{Tab, TabBar},
    v_flex,
};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};
use std::fs::read_to_string;

use crate::DB;
use crate::board::BoardView;
use crate::markdown_editor::{DEFAULT_NOTE, MarkdownEditorView, SaveState, now_ts};
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
struct ProjectChoice {
    id: u32,
    name: SharedString,
}

#[derive(Clone)]
struct BoardChoice {
    id: u32,
    title: SharedString,
    project_name: Option<SharedString>,
}

pub struct AppShell {
    focus_handle: FocusHandle,
    sidebar: Entity<SidebarView>,
    title_input: Entity<InputState>,
    open_tabs: Vec<OpenTab>,
    active_tab_index: usize,
    next_tab_id: u64,
    projects: Vec<ProjectChoice>,
    boards: Vec<BoardChoice>,
    active_project_id: Option<u32>,
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

        cx.subscribe(&title_input, |this, input, event: &InputEvent, cx| {
            if !matches!(event, InputEvent::Change) || this.suppress_title_event {
                return;
            }

            let title = input.read(cx).text().to_string();
            this.rename_active_tab(title, cx);
        })
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
                    this.active_project_id = Some(*project_id);
                    cx.notify();
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
            },
        )
        .detach();

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            sidebar,
            title_input,
            open_tabs: vec![OpenTab {
                id: 1,
                title: "New tab".into(),
                kind: OpenTabKind::Chooser,
            }],
            active_tab_index: 0,
            next_tab_id: 2,
            projects: vec![],
            boards: vec![],
            active_project_id: None,
            suppress_title_event: false,
        };

        this.refresh_workspace(cx);
        this.sidebar
            .update(cx, |_, cx| SidebarView::list_projects(cx));
        this
    }
}
