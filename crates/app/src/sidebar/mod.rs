mod action;
mod dto;
mod event;
mod render;

use anyhow::Result;
use entity::{
    board, board::Entity as Board, note, note::Entity as Note, project, project::Entity as Project,
};
use gpui::*;
use gpui_component::{
    ActiveTheme, Theme, ThemeRegistry,
    input::{InputEvent, InputState},
    searchable_list::SearchableListDelegate,
    select::{SearchableVec, SelectEvent, SelectState},
};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};

use crate::DB;
use action::*;
use dto::*;

pub(crate) use dto::ActiveItem;
pub(crate) use event::SidebarEvent;

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
    new_project_input: Entity<InputState>,
    new_board_input: Entity<InputState>,
    rename_board_input: Entity<InputState>,
    rename_note_input: Entity<InputState>,
    adding_board_to_project: Option<Option<u32>>,
    renaming_board: Option<u32>,
    renaming_note: Option<u32>,
}

impl SidebarView {
    pub(crate) fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));

        let new_project_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Project name..."));

        let new_board_input = cx.new(|cx| InputState::new(window, cx).placeholder("Board name..."));

        let rename_board_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Board name..."));

        let rename_note_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Note name..."));

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
                if let Some(theme_name) = theme_name
                    && let Some(theme_config) =
                        ThemeRegistry::global(cx).themes().get(theme_name).cloned()
                {
                    Theme::global_mut(cx).apply_config(&theme_config);
                    cx.refresh_windows();
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
            &new_board_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).text().to_string();
                    let name = text.trim();
                    if let Some(project_id) = this.adding_board_to_project
                        && !name.is_empty()
                    {
                        this.add_board(cx, project_id, name.to_string());
                    }
                    this.adding_board_to_project = None;
                    cx.notify();
                }
                InputEvent::Blur => {
                    this.adding_board_to_project = None;
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
            new_project_input,
            new_board_input,
            rename_board_input,
            rename_note_input,
            adding_board_to_project: None,
            renaming_board: None,
            renaming_note: None,
        }
    }

    pub(crate) fn list_projects(cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let results = Project::load().with(Board).with(Note).all(&*db).await?;
            let standalone_boards = Board::find()
                .filter(board::Column::ProjectId.is_null())
                .all(&*db)
                .await?;

            let standalone_notes = Note::find()
                .filter(note::Column::ProjectId.is_null())
                .all(&*db)
                .await?;

            let mut projects: Vec<ProjectDTO> = results.into_iter().map(ProjectDTO::from).collect();

            let standalone_boards: Vec<BoardDTO> =
                standalone_boards.into_iter().map(BoardDTO::from).collect();

            let standalone_notes: Vec<NoteDTO> =
                standalone_notes.into_iter().map(NoteDTO::from).collect();

            this.update(cx, |this, cx| {
                if let Some(first) = projects.first_mut() {
                    first.is_expanded = true;
                }

                this.projects = projects;
                this.standalone_boards = standalone_boards;
                this.standalone_notes = standalone_notes;
                cx.notify();
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn add_project(&mut self, cx: &mut Context<Self>, name: String) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let project_entity = project::ActiveModel {
                name: Set(name),
                ..Default::default()
            }
            .insert(&*db)
            .await?;

            this.update(cx, |this, cx| {
                this.projects.push(ProjectDTO {
                    id: project_entity.id as u32,
                    name: SharedString::from(project_entity.name),
                    is_expanded: true,
                    boards: vec![],
                    notes: vec![],
                });
                cx.notify();
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn add_board(&mut self, cx: &mut Context<Self>, project_id: Option<u32>, title: String) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let inserted = board::ActiveModel {
                title: Set(title),
                project_id: Set(project_id.map(|id| id as i64)),
                ..Default::default()
            }
            .insert(&*db)
            .await?;

            this.update(cx, |this, cx| {
                let board = BoardDTO {
                    id: inserted.id as u32,
                    title: SharedString::from(inserted.title),
                    project_id,
                };
                if let Some(project_id) = project_id
                    && let Some(project) = this.projects.iter_mut().find(|p| p.id == project_id)
                {
                    project.boards.push(board.clone());
                } else {
                    this.standalone_boards.push(board.clone());
                }
                this.select_board(board.id, project_id, board.title, cx);
                cx.notify();
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn select_board(
        &mut self,
        board_id: u32,
        project_id: Option<u32>,
        title: SharedString,
        cx: &mut Context<Self>,
    ) {
        self.active_project_id = project_id;
        self.active_item = Some(ActiveItem::Board(board_id));
        cx.emit(SidebarEvent::OpenBoard {
            board_id,
            project_id,
            title,
        });
    }

    fn select_note(
        &mut self,
        note_id: u32,
        project_id: Option<u32>,
        title: SharedString,
        cx: &mut Context<Self>,
    ) {
        self.active_project_id = project_id;
        self.active_item = Some(ActiveItem::Note(note_id));
        cx.emit(SidebarEvent::OpenNote {
            note_id,
            project_id,
            title,
        });
    }

    fn delete_board(&mut self, cx: &mut Context<Self>, board_id: u32) {
        self.standalone_boards.retain(|board| board.id != board_id);
        for project in &mut self.projects {
            project.boards.retain(|board| board.id != board_id);
        }
        self.renaming_board = None;
        self.adding_board_to_project = None;
        cx.notify();
        cx.emit(SidebarEvent::BoardDeleted { board_id });

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |_, _| -> Result<()> {
            Board::delete_by_id(board_id as i64).exec(&*db).await?;
            Ok(())
        })
        .detach();
    }

    fn rename_board(&mut self, cx: &mut Context<Self>, board_id: u32, title: String) {
        for board in self
            .projects
            .iter_mut()
            .flat_map(|project| project.boards.iter_mut())
            .chain(self.standalone_boards.iter_mut())
        {
            if board.id == board_id {
                board.title = SharedString::from(title.as_str());
                break;
            }
        }

        cx.notify();
        cx.emit(SidebarEvent::BoardRenamed {
            board_id,
            title: SharedString::from(title.as_str()),
        });

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |_, _| -> Result<()> {
            board::ActiveModel {
                id: Set(board_id as i64),
                title: Set(title),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            Ok(())
        })
        .detach();
    }

    fn delete_note(&mut self, cx: &mut Context<Self>, note_id: u32) {
        self.standalone_notes.retain(|note| note.id != note_id);
        for project in &mut self.projects {
            project.notes.retain(|note| note.id != note_id);
        }
        self.renaming_note = None;
        cx.notify();
        cx.emit(SidebarEvent::NoteDeleted { note_id });

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |_, _| -> Result<()> {
            Note::delete_by_id(note_id as i64).exec(&*db).await?;
            Ok(())
        })
        .detach();
    }

    fn rename_note(&mut self, cx: &mut Context<Self>, note_id: u32, title: String) {
        let shared_title = SharedString::from(title.as_str());

        if let Some(note) = self
            .projects
            .iter_mut()
            .flat_map(|project| project.notes.iter_mut())
            .chain(self.standalone_notes.iter_mut())
            .find(|note| note.id == note_id)
        {
            note.title = shared_title.clone();
        }
        cx.notify();

        cx.emit(SidebarEvent::NoteRenamed {
            note_id,
            title: shared_title,
        });

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |_, _| -> Result<()> {
            note::ActiveModel {
                id: Set(note_id as i64),
                title: Set(title),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            Ok(())
        })
        .detach();
    }

    fn start_renaming_board(
        &mut self,
        action: &EditBoardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(title) = self
            .find_board(action.0)
            .map(|board| board.title.to_string())
        else {
            return;
        };

        self.renaming_board = Some(action.0);
        self.rename_board_input.update(cx, |input, cx| {
            input.set_value(title, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn start_renaming_note(
        &mut self,
        action: &EditNoteAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(title) = self.find_note(action.0).map(|note| note.title.to_string()) else {
            return;
        };

        self.renaming_note = Some(action.0);
        self.rename_note_input.update(cx, |input, cx| {
            input.set_value(title, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn move_board(&mut self, cx: &mut Context<Self>, board_id: u32, project_id: Option<u32>) {
        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |this, cx| -> Result<()> {
            board::ActiveModel {
                id: Set(board_id as i64),
                project_id: Set(project_id.map(|id| id as i64)),
                ..Default::default()
            }
            .update(&*db)
            .await?;

            this.update(cx, |_, cx| Self::list_projects(cx)).ok();
            Ok(())
        })
        .detach();
    }

    fn move_note(&mut self, cx: &mut Context<Self>, note_id: u32, project_id: Option<u32>) {
        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |this, cx| -> Result<()> {
            note::ActiveModel {
                id: Set(note_id as i64),
                project_id: Set(project_id.map(|id| id as i64)),
                ..Default::default()
            }
            .update(&*db)
            .await?;

            this.update(cx, |_, cx| Self::list_projects(cx)).ok();
            Ok(())
        })
        .detach();
    }

    fn on_delete_board_action(
        &mut self,
        action: &DeleteBoardAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_board(cx, action.0);
    }

    fn on_edit_board_action(
        &mut self,
        action: &EditBoardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_renaming_board(action, window, cx);
    }

    fn on_move_board_action(
        &mut self,
        action: &MoveBoardAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_board(cx, action.board_id, action.project_id);
    }

    fn on_move_note_action(
        &mut self,
        action: &MoveNoteAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_note(cx, action.note_id, action.project_id);
    }

    fn on_delete_note_action(
        &mut self,
        action: &DeleteNoteAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_note(cx, action.0);
    }

    fn on_edit_note_action(
        &mut self,
        action: &EditNoteAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_renaming_note(action, window, cx);
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
}
