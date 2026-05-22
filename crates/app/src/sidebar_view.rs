use anyhow::Result;
use entity::{
    board, board::Entity as Board, note, note::Entity as Note, project, project::Entity as Project,
};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, Theme, ThemeRegistry, h_flex,
    input::{Input, InputEvent, InputState},
    select::{SearchableVec, Select, SelectDelegate, SelectEvent, SelectState},
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex,
};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;

use crate::DB;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
struct DeleteBoardAction(u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
struct EditBoardAction(u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
struct MoveBoardAction {
    board_id: u32,
    project_id: Option<u32>,
}

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
struct MoveNoteAction {
    note_id: u32,
    project_id: Option<u32>,
}

#[derive(Clone)]
pub(crate) enum SidebarEvent {
    OpenBoard {
        board_id: u32,
        project_id: Option<u32>,
        title: SharedString,
    },
    OpenNote {
        note_id: u32,
        project_id: Option<u32>,
        title: SharedString,
    },
    ActivateProject {
        project_id: u32,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveItem {
    Board(u32),
    Note(u32),
}

struct ProjectDTO {
    id: u32,
    name: SharedString,
    is_expanded: bool,
    boards: Vec<BoardDTO>,
    notes: Vec<NoteDTO>,
}

#[derive(Clone)]
struct BoardDTO {
    id: u32,
    title: SharedString,
    project_id: Option<u32>,
}

#[derive(Clone)]
struct NoteDTO {
    id: u32,
    title: SharedString,
    project_id: Option<u32>,
}

pub(crate) struct SidebarView {
    active_project_id: Option<u32>,
    active_item: Option<ActiveItem>,
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
    adding_board_to_project: Option<Option<u32>>,
    renaming_board: Option<u32>,
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
            adding_board_to_project: None,
            renaming_board: None,
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

            let mut projects: Vec<ProjectDTO> = results
                .into_iter()
                .map(|project| ProjectDTO {
                    id: project.id as u32,
                    name: SharedString::from(project.name),
                    is_expanded: false,
                    boards: project
                        .boards
                        .into_iter()
                        .map(|board| BoardDTO {
                            id: board.id as u32,
                            title: SharedString::from(board.title),
                            project_id: board.project_id.map(|id| id as u32),
                        })
                        .collect(),
                    notes: project
                        .notes
                        .into_iter()
                        .map(|note| NoteDTO {
                            id: note.id as u32,
                            title: SharedString::from(note.title),
                            project_id: note.project_id.map(|id| id as u32),
                        })
                        .collect(),
                })
                .collect();

            let standalone_boards = standalone_boards
                .into_iter()
                .map(|board| BoardDTO {
                    id: board.id as u32,
                    title: SharedString::from(board.title),
                    project_id: None,
                })
                .collect();

            let standalone_notes = standalone_notes
                .into_iter()
                .map(|note| NoteDTO {
                    id: note.id as u32,
                    title: SharedString::from(note.title),
                    project_id: None,
                })
                .collect();

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
                board.title = SharedString::from(title.clone());
                break;
            }
        }

        cx.notify();

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

    fn find_board(&self, board_id: u32) -> Option<&BoardDTO> {
        self.projects
            .iter()
            .flat_map(|project| project.boards.iter())
            .chain(self.standalone_boards.iter())
            .find(|board| board.id == board_id)
    }

    fn render_board_item(
        &self,
        board: &BoardDTO,
        cx: &mut Context<Self>,
        search_matches: bool,
    ) -> SidebarMenuItem {
        let board_id = board.id;
        let project_id = board.project_id;
        let title = board.title.clone();
        let is_active = self.active_item == Some(ActiveItem::Board(board_id));
        let is_renaming = self.renaming_board == Some(board_id);
        let projects = self
            .projects
            .iter()
            .map(|project| (project.id, project.name.clone()))
            .collect::<Vec<_>>();

        SidebarMenuItem::new(title.clone())
            .icon(IconName::LayoutDashboard)
            .when(!search_matches, |this| this.disable(true))
            .when(is_renaming, |this| {
                let input = self.rename_board_input.clone();
                this.suffix(move |_window, cx| {
                    Input::new(&input)
                        .small()
                        .bg(cx.theme().sidebar.opacity(0.))
                        .rounded_none()
                        .focus_bordered(false)
                        .border_0()
                        .text_xs()
                        .w_full()
                })
            })
            .active(is_active)
            .context_menu(move |mut menu, _, cx| {
                let muted = cx.theme().muted_foreground;
                menu = menu
                    .menu_element(Box::new(EditBoardAction(board_id)), move |_window, _cx| {
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .justify_between()
                            .child("Edit")
                            .child(Icon::new(IconName::Replace).xsmall().text_color(muted))
                    })
                    .menu_element(
                        Box::new(MoveBoardAction {
                            board_id,
                            project_id: None,
                        }),
                        move |_window, _cx| {
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child("Move to Standalone")
                                .child(Icon::new(IconName::Folder).xsmall().text_color(muted))
                        },
                    );

                for (target_project_id, name) in projects.clone() {
                    if Some(target_project_id) == project_id {
                        continue;
                    }
                    menu = menu.menu_element(
                        Box::new(MoveBoardAction {
                            board_id,
                            project_id: Some(target_project_id),
                        }),
                        move |_window, _cx| {
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child(format!("Move to {}", name))
                                .child(Icon::new(IconName::FolderOpen).xsmall().text_color(muted))
                        },
                    );
                }

                menu.menu_element(
                    Box::new(DeleteBoardAction(board_id)),
                    move |_window, _cx| {
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .justify_between()
                            .child("Delete")
                            .child(Icon::new(IconName::Delete).xsmall().text_color(muted))
                    },
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_board(board_id, project_id, title.clone(), cx);
                this.focus_handle.focus(window, cx);
                cx.notify();
            }))
    }

    fn render_note_item(
        &self,
        note: &NoteDTO,
        cx: &mut Context<Self>,
        search_matches: bool,
    ) -> SidebarMenuItem {
        let note_id = note.id;
        let project_id = note.project_id;
        let title = note.title.clone();
        let is_active = self.active_item == Some(ActiveItem::Note(note_id));
        let projects = self
            .projects
            .iter()
            .map(|project| (project.id, project.name.clone()))
            .collect::<Vec<_>>();

        SidebarMenuItem::new(title.clone())
            .icon(IconName::BookOpen)
            .when(!search_matches, |this| this.disable(true))
            .active(is_active)
            .context_menu(move |mut menu, _, cx| {
                let muted = cx.theme().muted_foreground;
                menu = menu.menu_element(
                    Box::new(MoveNoteAction {
                        note_id,
                        project_id: None,
                    }),
                    move |_window, _cx| {
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .justify_between()
                            .child("Move to Standalone")
                            .child(Icon::new(IconName::Folder).xsmall().text_color(muted))
                    },
                );

                for (target_project_id, name) in projects.clone() {
                    if Some(target_project_id) == project_id {
                        continue;
                    }
                    menu = menu.menu_element(
                        Box::new(MoveNoteAction {
                            note_id,
                            project_id: Some(target_project_id),
                        }),
                        move |_window, _cx| {
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child(format!("Move to {}", name))
                                .child(Icon::new(IconName::FolderOpen).xsmall().text_color(muted))
                        },
                    );
                }

                menu
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_note(note_id, project_id, title.clone(), cx);
                this.focus_handle.focus(window, cx);
                cx.notify();
            }))
    }
}

impl EventEmitter<SidebarEvent> for SidebarView {}

impl Focusable for SidebarView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SidebarView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let adding_board_to_project = self.adding_board_to_project;
        let search_query = self.search_input.read(cx).text().to_string();
        let search_lower = search_query.to_lowercase();

        let project_menu_items: Vec<SidebarMenuItem> = self
            .projects
            .iter()
            .filter_map(|project| {
                let project_matches =
                    search_lower.is_empty() || project.name.to_lowercase().contains(&search_lower);
                let filtered_boards = project
                    .boards
                    .iter()
                    .filter(|board| {
                        project_matches || board.title.to_lowercase().contains(&search_lower)
                    })
                    .collect::<Vec<_>>();
                let filtered_notes = project
                    .notes
                    .iter()
                    .filter(|note| {
                        project_matches || note.title.to_lowercase().contains(&search_lower)
                    })
                    .collect::<Vec<_>>();

                if !project_matches && filtered_boards.is_empty() && filtered_notes.is_empty() {
                    return None;
                }

                let project_id = project.id;
                let mut children: Vec<SidebarMenuItem> = Vec::new();
                children.extend(
                    filtered_notes
                        .into_iter()
                        .map(|note| self.render_note_item(note, cx, true)),
                );
                children.extend(
                    filtered_boards
                        .into_iter()
                        .map(|board| self.render_board_item(board, cx, true)),
                );

                if adding_board_to_project == Some(Some(project_id)) {
                    let input = self.new_board_input.clone();
                    children.push(SidebarMenuItem::new("").disable(true).suffix(
                        move |_window, cx| {
                            Input::new(&input)
                                .small()
                                .bg(cx.theme().sidebar)
                                .rounded_none()
                                .focus_bordered(false)
                                .border_0()
                                .border_b_1()
                                .border_color(cx.theme().foreground)
                                .text_xs()
                                .w_full()
                        },
                    ));
                } else {
                    children.push(
                        SidebarMenuItem::new("Add board")
                            .icon(IconName::Plus)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.adding_board_to_project = Some(Some(project_id));
                                this.new_board_input.update(cx, |input, cx| {
                                    input.focus(window, cx);
                                });
                                cx.notify();
                            })),
                    );
                }

                Some(
                    SidebarMenuItem::new(project.name.clone())
                        .icon(IconName::FolderOpen)
                        .active(self.active_project_id == Some(project_id))
                        .default_open(project.is_expanded || !search_lower.is_empty())
                        .click_to_toggle(true)
                        .children(children)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.active_project_id = Some(project_id);
                            cx.emit(SidebarEvent::ActivateProject { project_id });
                            this.focus_handle.focus(window, cx);
                            cx.notify();
                        })),
                )
            })
            .collect();

        let standalone_matches = search_lower.is_empty() || "standalone".contains(&search_lower);
        let standalone_boards = self
            .standalone_boards
            .iter()
            .filter(|board| {
                standalone_matches || board.title.to_lowercase().contains(&search_lower)
            })
            .collect::<Vec<_>>();

        let standalone_notes = self
            .standalone_notes
            .iter()
            .filter(|note| standalone_matches || note.title.to_lowercase().contains(&search_lower))
            .collect::<Vec<_>>();

        let mut standalone_items: Vec<SidebarMenuItem> = standalone_notes
            .into_iter()
            .map(|note| self.render_note_item(note, cx, true))
            .collect();

        standalone_items.extend(
            standalone_boards
                .into_iter()
                .map(|board| self.render_board_item(board, cx, true)),
        );

        div()
            .h_full()
            .flex_shrink_0()
            .on_action(cx.listener(Self::on_delete_board_action))
            .on_action(cx.listener(Self::on_edit_board_action))
            .on_action(cx.listener(Self::on_move_board_action))
            .on_action(cx.listener(Self::on_move_note_action))
            .child(
                Sidebar::new("sidebar")
                    .w(px(260.))
                    .collapsible(false)
                    .border_0()
                    .gap_0()
                    .header(
                        v_flex()
                            .id("header")
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(
                                SidebarHeader::new()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(theme.radius)
                                            .bg(theme.primary)
                                            .text_color(theme.primary_foreground)
                                            .size_8()
                                            .flex_shrink_0()
                                            .child(IconName::GalleryVerticalEnd),
                                    )
                                    .child(
                                        v_flex()
                                            .id("header-title")
                                            .gap_0()
                                            .text_sm()
                                            .flex_1()
                                            .line_height(relative(1.25))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child("Castle")
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.sidebar_foreground)
                                            .child(
                                                div()
                                                    .child("Your private note taking app")
                                                    .text_color(theme.muted_foreground)
                                                    .text_xs(),
                                            ),
                                    ),
                            )
                            .child(
                                Input::new(&self.search_input)
                                    .cleanable(true)
                                    .prefix(IconName::Search),
                            )
                            .child({
                                if self.is_adding_project {
                                    Input::new(&self.new_project_input)
                                        .w_full()
                                        .rounded_none()
                                        .focus_bordered(false)
                                        .border_0()
                                        .border_b_1()
                                        .border_color(theme.foreground)
                                        .into_any_element()
                                } else {
                                    div()
                                        .id("add-project-btn-container")
                                        .flex()
                                        .w_full()
                                        .justify_center()
                                        .items_center()
                                        .h_8()
                                        .rounded(theme.radius)
                                        .bg(theme.accent_foreground.opacity(0.15))
                                        .hover(|this| {
                                            this.bg(theme.accent_foreground.opacity(0.20))
                                        })
                                        .border_1()
                                        .border_color(theme.accent_foreground.opacity(0.30))
                                        .cursor_pointer()
                                        .child(
                                            h_flex()
                                                .id("add-project-btn")
                                                .w_full()
                                                .justify_center()
                                                .items_center()
                                                .gap_1()
                                                .text_sm()
                                                .text_color(theme.accent_foreground)
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(IconName::Plus)
                                                .child("Add Project"),
                                        )
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.is_adding_project = true;
                                            this.new_project_input.update(cx, |input, cx| {
                                                input.focus(window, cx);
                                            });
                                            cx.notify();
                                        }))
                                        .into_any_element()
                                }
                            }),
                    )
                    .child(
                        SidebarGroup::new("Projects")
                            .child(SidebarMenu::new().children(project_menu_items)),
                    )
                    .when(!standalone_items.is_empty(), |this| {
                        this.child(
                            SidebarGroup::new("Standalone")
                                .child(SidebarMenu::new().children(standalone_items)),
                        )
                    })
                    .footer(
                        SidebarFooter::new().child(
                            h_flex()
                                .id("theme-select-footer")
                                .gap_2()
                                .items_center()
                                .child(IconName::Palette)
                                .child(
                                    Select::new(&self.theme_select)
                                        .placeholder("Theme")
                                        .w_full()
                                        .menu_max_h(rems(14.)),
                                )
                                .w_full(),
                        ),
                    ),
            )
    }
}
