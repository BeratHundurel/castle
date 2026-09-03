mod action;
mod action_handlers;
mod content_item;
pub(crate) mod drag;
mod event;
mod model;
mod mutations;
mod render;
mod workspace_rows;

use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use model::*;

use settings::AppSettings;

const SIDEBAR_MIN_WIDTH: Pixels = px(200.);
const SIDEBAR_MAX_WIDTH: Pixels = px(480.);

pub use event::SidebarEvent;
pub use model::ActiveItem;

pub struct SidebarView {
    active_project_id: Option<u32>,
    active_item: Option<ActiveItem>,
    focus_handle: FocusHandle,
    search_input: Entity<InputState>,
    projects: Vec<ProjectNode>,
    standalone_boards: Vec<BoardItem>,
    standalone_notes: Vec<NoteItem>,
    is_adding_project: bool,
    collapsed: bool,
    width: Pixels,
    bounds: Option<Bounds<Pixels>>,
    new_project_input: Entity<InputState>,
    rename_board_input: Entity<InputState>,
    rename_note_input: Entity<InputState>,
    rename_project_input: Entity<InputState>,
    renaming_board: Option<u32>,
    renaming_note: Option<u32>,
    renaming_project: Option<u32>,
}

impl SidebarView {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
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
            projects: vec![],
            standalone_boards: vec![],
            standalone_notes: vec![],
            is_adding_project: false,
            collapsed: false,
            width: AppSettings::sidebar_width(cx),
            bounds: None,
            new_project_input,
            rename_board_input,
            rename_note_input,
            rename_project_input,
            renaming_board: None,
            renaming_note: None,
            renaming_project: None,
        }
    }

    fn find_note(&self, note_id: u32) -> Option<&NoteItem> {
        self.projects
            .iter()
            .flat_map(|project| project.notes.iter())
            .chain(self.standalone_notes.iter())
            .find(|note| note.id == note_id)
    }

    fn find_board(&self, board_id: u32) -> Option<&BoardItem> {
        self.projects
            .iter()
            .flat_map(|project| project.boards.iter())
            .chain(self.standalone_boards.iter())
            .find(|board| board.id == board_id)
    }

    fn find_project(&self, project_id: u32) -> Option<&ProjectNode> {
        self.projects
            .iter()
            .find(|project| project.id == project_id)
    }

    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn width(&self) -> Pixels {
        self.width
    }

    pub fn set_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let width = width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
        if self.width == width {
            return;
        }
        self.width = width;
        cx.notify();
    }

    pub fn set_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.collapsed == collapsed {
            return;
        }

        self.collapsed = collapsed;
        cx.notify();
    }

    pub fn set_active_board(&mut self, board_id: u32, project_id: Option<u32>) {
        self.active_project_id = project_id;
        self.active_item = Some(ActiveItem::Board(board_id));
    }

    pub fn set_active_note(&mut self, note_id: u32, project_id: Option<u32>) {
        self.active_project_id = project_id;
        self.active_item = Some(ActiveItem::Note(note_id));
    }

    pub fn clear_active_item(&mut self) {
        self.active_item = None;
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn contains_project_named(&self, name: &str) -> bool {
        self.projects.iter().any(|project| project.name == name)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn contains_note_named(&self, title: &str) -> bool {
        self.projects
            .iter()
            .flat_map(|project| project.notes.iter())
            .chain(self.standalone_notes.iter())
            .any(|note| note.title == title)
    }
}
