mod action;
mod content_item;
mod dto;
mod event;
mod handlers;
mod mutations;
mod render;
mod store;

use dto::*;
use gpui::*;
use gpui_component::{
    ActiveTheme, ThemeRegistry,
    input::{InputEvent, InputState},
    searchable_list::SearchableListDelegate,
    select::{SearchableVec, SelectEvent, SelectState},
};

pub(crate) use dto::ActiveItem;
pub(crate) use event::SidebarEvent;

use crate::app_settings::AppSettings;

pub(crate) struct SidebarView {
    pub(crate) active_project_id: Option<u32>,
    pub(crate) active_item: Option<ActiveItem>,
    focus_handle: FocusHandle,
    search_input: Entity<InputState>,
    theme_select: Entity<SelectState<SearchableVec<SharedString>>>,
    projects: Vec<ProjectDTO>,
    standalone_boards: Vec<BoardDTO>,
    standalone_notes: Vec<NoteDTO>,
    is_adding_project: bool,
    collapsed: bool,
    new_project_input: Entity<InputState>,
    rename_board_input: Entity<InputState>,
    rename_note_input: Entity<InputState>,
    rename_project_input: Entity<InputState>,
    renaming_board: Option<u32>,
    renaming_note: Option<u32>,
    renaming_project: Option<u32>,
}

impl SidebarView {
    pub(crate) fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));

        let new_project_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Project name..."));

        let rename_board_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Board name..."));

        let rename_note_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Note name..."));

        let rename_project_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Project name..."));

        let registry = ThemeRegistry::global(cx);
        let themes: Vec<SharedString> = registry
            .sorted_themes()
            .iter()
            .map(|theme| theme.name.clone())
            .collect();

        let current_theme = cx.theme().theme_name();
        let delegate = SearchableVec::new(themes);
        let selected_index = delegate
            .position(current_theme)
            .or_else(|| delegate.position(&SharedString::from("Alduin")));

        let theme_select =
            cx.new(|cx| SelectState::new(delegate, selected_index, window, cx).searchable(true));

        cx.subscribe(
            &theme_select,
            |_, _, event: &SelectEvent<SearchableVec<SharedString>>, cx| {
                let SelectEvent::Confirm(theme_name) = event;
                if let Some(theme_name) = theme_name {
                    AppSettings::set_theme_name(theme_name.clone(), cx);
                }
            },
        )
        .detach();

        cx.subscribe(
            &new_project_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).text().to_string();
                    let name = text.trim();
                    if !name.is_empty() {
                        this.add_project(cx, name.to_string());
                    }
                    this.is_adding_project = false;
                    cx.notify();
                }
                InputEvent::Blur => {
                    this.is_adding_project = false;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe(
            &rename_board_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).text().to_string();
                    let title = text.trim();
                    if let Some(board_id) = this.renaming_board
                        && !title.is_empty()
                    {
                        this.rename_board(cx, board_id, title.to_string());
                    }
                    this.renaming_board = None;
                    cx.notify();
                }
                InputEvent::Blur => {
                    this.renaming_board = None;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe(
            &rename_note_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).text().to_string();
                    let title = text.trim();
                    if let Some(note_id) = this.renaming_note
                        && !title.is_empty()
                    {
                        this.rename_note(cx, note_id, title.to_string());
                    }
                    this.renaming_note = None;
                    cx.notify();
                }
                InputEvent::Blur => {
                    this.renaming_note = None;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe(
            &rename_project_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).text().to_string();
                    let name = text.trim();
                    if let Some(project_id) = this.renaming_project
                        && !name.is_empty()
                    {
                        this.rename_project(cx, project_id, name.to_string());
                    }
                    this.renaming_project = None;
                    cx.notify();
                }
                InputEvent::Blur => {
                    this.renaming_project = None;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe(&search_input, |_, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        })
        .detach();

        Self {
            active_project_id: None,
            active_item: None,
            focus_handle: cx.focus_handle(),
            search_input,
            theme_select,
            projects: vec![],
            standalone_boards: vec![],
            standalone_notes: vec![],
            is_adding_project: false,
            collapsed: false,
            new_project_input,
            rename_board_input,
            rename_note_input,
            rename_project_input,
            renaming_board: None,
            renaming_note: None,
            renaming_project: None,
        }
    }

    fn find_note(&self, note_id: u32) -> Option<&NoteDTO> {
        self.projects
            .iter()
            .flat_map(|project| project.notes.iter())
            .chain(self.standalone_notes.iter())
            .find(|note| note.id == note_id)
    }

    fn find_board(&self, board_id: u32) -> Option<&BoardDTO> {
        self.projects
            .iter()
            .flat_map(|project| project.boards.iter())
            .chain(self.standalone_boards.iter())
            .find(|board| board.id == board_id)
    }

    fn find_project(&self, project_id: u32) -> Option<&ProjectDTO> {
        self.projects
            .iter()
            .find(|project| project.id == project_id)
    }

    pub(crate) fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub(crate) fn set_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.collapsed == collapsed {
            return;
        }

        self.collapsed = collapsed;
        cx.notify();
    }
}
