use anyhow::Result;
use entity::{board, board::Entity as Board, project, project::Entity as Project};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, IconName, Side, Sizable, Theme, ThemeRegistry, h_flex,
    input::{Input, InputEvent, InputState},
    select::{SearchableVec, Select, SelectDelegate, SelectEvent, SelectState},
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex,
};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, EntityTrait};
use serde::Deserialize;

use crate::DB;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
struct DeleteBoardAction(u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
struct EditBoardAction(u32);

#[derive(Clone)]
pub(crate) enum SidebarEvent {
    BoardSelected { board_id: u32 },
}

struct ProjectDTO {
    id: u32,
    name: SharedString,
    is_expanded: bool,
    boards: Vec<BoardDTO>,
}

struct BoardDTO {
    id: u32,
    title: SharedString,
}

pub(crate) struct SidebarView {
    active_project_index: usize,
    active_board_index: Option<usize>,
    focus_handle: FocusHandle,
    search_input: Entity<InputState>,
    theme_select: Entity<SelectState<SearchableVec<SharedString>>>,
    projects: Vec<ProjectDTO>,
    is_adding_project: bool,
    new_project_input: Entity<InputState>,
    new_board_input: Entity<InputState>,
    rename_board_input: Entity<InputState>,
    adding_board_to_project: Option<usize>,
    renaming_board: Option<(usize, usize)>,
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
                    if let Some(project_index) = this.adding_board_to_project
                        && !name.is_empty()
                    {
                        this.add_board(cx, project_index, name.to_string());
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
                    if let Some((project_index, board_index)) = this.renaming_board
                        && !title.is_empty()
                    {
                        this.rename_board(cx, project_index, board_index, title.to_string());
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

        Self {
            active_project_index: 0,
            active_board_index: Some(0),
            focus_handle: cx.focus_handle(),
            search_input,
            theme_select,
            projects: vec![],
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
            let results = Project::load().with(Board).all(&*db).await?;

            let mut projects: Vec<ProjectDTO> = results
                .into_iter()
                .map(|p| ProjectDTO {
                    id: p.id as u32,
                    name: SharedString::from(p.name),
                    is_expanded: false,
                    boards: p
                        .boards
                        .into_iter()
                        .map(|b| BoardDTO {
                            id: b.id as u32,
                            title: SharedString::from(b.title),
                        })
                        .collect(),
                })
                .collect();

            this.update(cx, |this, cx| {
                if let Some(first) = projects.first_mut() {
                    first.is_expanded = true;
                }

                this.projects = projects;

                if let Some(first_board) = this.projects.first().and_then(|p| p.boards.first()) {
                    this.active_project_index = 0;
                    this.active_board_index = Some(0);
                    cx.emit(SidebarEvent::BoardSelected {
                        board_id: first_board.id,
                    });
                }

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
            let project_active_model = project::ActiveModel {
                name: Set(name),
                ..Default::default()
            };

            let project_entity = project_active_model.insert(&*db).await?;
            let project = ProjectDTO {
                id: project_entity.id as u32,
                name: SharedString::from(project_entity.name),
                is_expanded: true,
                boards: vec![],
            };

            this.update(cx, |this, cx| {
                this.projects.push(project);
                cx.notify();
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn add_board(&mut self, cx: &mut Context<Self>, project_index: usize, title: String) {
        let Some(project_id) = self.projects.get(project_index).map(|project| project.id) else {
            return;
        };

        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let board_active_model = board::ActiveModel {
                title: Set(title),
                project_id: Set(project_id as i64),
                ..Default::default()
            };

            let board_entity = board_active_model.insert(&*db).await?;
            let board = BoardDTO {
                id: board_entity.id as u32,
                title: SharedString::from(board_entity.title),
            };

            this.update(cx, |this, cx| {
                if let Some(project) = this.projects.get_mut(project_index) {
                    let board_index = project.boards.len();
                    project.boards.push(board);
                    this.active_project_index = project_index;
                    this.active_board_index = Some(board_index);
                    if let Some(board) = this
                        .projects
                        .get(project_index)
                        .and_then(|p| p.boards.get(board_index))
                    {
                        cx.emit(SidebarEvent::BoardSelected { board_id: board.id });
                    }
                    cx.notify();
                }
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn delete_board(&mut self, cx: &mut Context<Self>, board_id: u32) {
        let Some((project_index, board_index)) = self.find_board(board_id) else {
            return;
        };

        let db = cx.global::<DB>().conn.clone();

        if let Some(project) = self.projects.get_mut(project_index) {
            project.boards.remove(board_index);
        }

        self.renaming_board = None;
        self.adding_board_to_project = None;

        if self.active_project_index == project_index
            && self.active_board_index == Some(board_index)
        {
            self.active_board_index = self
                .projects
                .get(project_index)
                .and_then(|project| project.boards.first())
                .map(|_| 0);
        } else if self.active_project_index == project_index
            && let Some(active_board_index) = self.active_board_index
            && active_board_index > board_index
        {
            self.active_board_index = Some(active_board_index - 1);
        }

        if let Some(board) = self
            .active_board_index
            .and_then(|board_index| self.projects.get(project_index)?.boards.get(board_index))
        {
            cx.emit(SidebarEvent::BoardSelected { board_id: board.id });
        }

        cx.notify();

        cx.spawn(async move |_, _| -> Result<()> {
            Board::delete_by_id(board_id as i64).exec(&*db).await?;
            Ok(())
        })
        .detach();
    }

    fn rename_board(
        &mut self,
        cx: &mut Context<Self>,
        project_index: usize,
        board_index: usize,
        title: String,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let Some(board_id) = self
            .projects
            .get(project_index)
            .and_then(|project| project.boards.get(board_index))
            .map(|board| board.id)
        else {
            return;
        };

        if let Some(board) = self
            .projects
            .get_mut(project_index)
            .and_then(|project| project.boards.get_mut(board_index))
        {
            board.title = SharedString::from(title.clone());
        }

        cx.notify();

        cx.spawn(async move |_, _| -> Result<()> {
            let model = board::ActiveModel {
                id: Set(board_id as i64),
                title: Set(title),
                ..Default::default()
            };
            model.update(&*db).await?;
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
        let Some((project_index, board_index)) = self.find_board(action.0) else {
            return;
        };

        let Some(title) = self
            .projects
            .get(project_index)
            .and_then(|project| project.boards.get(board_index))
            .map(|board| board.title.to_string())
        else {
            return;
        };

        self.renaming_board = Some((project_index, board_index));
        self.rename_board_input.update(cx, |input, cx| {
            input.set_value(title, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
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

    fn find_board(&self, board_id: u32) -> Option<(usize, usize)> {
        self.projects
            .iter()
            .enumerate()
            .find_map(|(project_index, project)| {
                project
                    .boards
                    .iter()
                    .position(|board| board.id == board_id)
                    .map(|board_index| (project_index, board_index))
            })
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
        let active_project_index = self.active_project_index;
        let active_board_index = self.active_board_index;
        let adding_board_to_project = self.adding_board_to_project;
        let renaming_board = self.renaming_board;

        let project_menu_items: Vec<SidebarMenuItem> = self
            .projects
            .iter()
            .enumerate()
            .map(|(p_idx, p)| {
                let first_board_id = p.boards.first().map(|board| board.id);
                SidebarMenuItem::new(p.name.clone())
                    .icon(IconName::FolderOpen)
                    .active(active_project_index == p_idx && active_board_index.is_none())
                    .default_open(p.is_expanded)
                    .click_to_toggle(true)
                    .children({
                        let mut boards: Vec<SidebarMenuItem> =
                            Vec::with_capacity(p.boards.len() + 1);

                        boards.extend(p.boards.iter().enumerate().map(|(b_idx, b)| {
                            let board_id = b.id;
                            SidebarMenuItem::new(b.title.clone())
                                .when(renaming_board == Some((p_idx, b_idx)), |this| {
                                    let input = self.rename_board_input.clone();
                                    this.suffix(move |_window, cx| {
                                        Input::new(&input)
                                            .small()
                                            .bg(cx.theme().sidebar.opacity(0.))
                                            .border_0()
                                            .rounded(cx.theme().radius)
                                            .text_xs()
                                            .w_full()
                                    })
                                })
                                .active(
                                    active_project_index == p_idx
                                        && active_board_index == Some(b_idx),
                                )
                                .context_menu({
                                    move |menu, _, _| {
                                        menu.menu_with_icon(
                                            "Edit",
                                            IconName::Replace,
                                            Box::new(EditBoardAction(board_id)),
                                        )
                                        .check_side(Side::Right)
                                        .menu_with_icon(
                                            "Delete",
                                            IconName::Delete,
                                            Box::new(DeleteBoardAction(board_id)),
                                        )
                                    }
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.active_project_index = p_idx;
                                    this.active_board_index = Some(b_idx);
                                    this.focus_handle.focus(window, cx);
                                    cx.emit(SidebarEvent::BoardSelected { board_id });
                                    cx.notify();
                                }))
                        }));

                        if adding_board_to_project == Some(p_idx) {
                            boards.push(SidebarMenuItem::new("").disable(true).suffix({
                                let input = self.new_board_input.clone();
                                move |_window, cx| {
                                    Input::new(&input)
                                        .small()
                                        .bg(cx.theme().sidebar)
                                        .border_0()
                                        .rounded(cx.theme().radius)
                                        .text_xs()
                                        .w_full()
                                }
                            }));
                        } else {
                            boards.push(
                                SidebarMenuItem::new("Add board")
                                    .icon(IconName::Plus)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.adding_board_to_project = Some(p_idx);
                                        this.new_board_input.update(cx, |input, cx| {
                                            input.focus(window, cx);
                                        });
                                        cx.notify();
                                    })),
                            );
                        }

                        boards
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.active_project_index = p_idx;
                        this.active_board_index = None;
                        if let Some(project) = this.projects.get_mut(p_idx) {
                            project.is_expanded = !project.is_expanded;
                        }
                        if let Some(board_id) = first_board_id {
                            cx.emit(SidebarEvent::BoardSelected { board_id });
                        }
                        this.focus_handle.focus(window, cx);
                        cx.notify();
                    }))
            })
            .collect();

        div()
            .h_full()
            .flex_shrink_0()
            .on_action(cx.listener(Self::on_delete_board_action))
            .on_action(cx.listener(Self::on_edit_board_action))
            .child(
                Sidebar::new("sidebar-story")
                    .w(px(260.))
                    .flex_shrink_0()
                    .collapsible(false)
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
