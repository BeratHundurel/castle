#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use dotenvy::dotenv;
use entity::{board, entry, project};
use entity::{
    board::Entity as Board, card, card::Entity as Card, entry::Entity as Entry,
    project::Entity as Project,
};
use gpui::{ElementId, *};
use gpui_component::sidebar::SidebarItem;
use gpui_component::{Collapsible, TitleBar, WindowExt};
use migration::{Migrator, MigratorTrait};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, QueryFilter};
use std::rc::Rc;
use std::sync::Arc;
use std::{env, fs, path::Path};

use gpui::{
    App, AppContext, ClickEvent, Context, Entity, Focusable, MouseButton, ParentElement, Render,
    SharedString, Styled, Window, div, px, relative,
};

use gpui_component::{
    ActiveTheme, IconName, Root, Theme, ThemeRegistry,
    button::{Button, ButtonVariants},
    dialog::{
        DialogAction, DialogClose, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
    },
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    select::{SearchableVec, Select, SelectDelegate, SelectEvent, SelectState},
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex,
};
pub struct CastleApp {
    active_project_index: usize,
    active_board_index: Option<usize>,
    focus_handle: gpui::FocusHandle,
    search_input: Entity<InputState>,
    dialog_title_input: Entity<InputState>,
    dialog_description_input: Entity<InputState>,
    theme_select: Entity<SelectState<SearchableVec<SharedString>>>,
    projects: Vec<ProjectDTO>,
    is_adding_project: bool,
    is_adding_list: bool,
    new_project_input: Entity<InputState>,
    new_list_input: Entity<InputState>,
    new_board_input: Entity<InputState>,
    pending_card_id: Option<u32>,
    adding_board_to_project: Option<usize>,
}

impl CastleApp {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));
        let dialog_title_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Give your title"));

        let dialog_description_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Give your description")
                .multi_line(true)
                .auto_grow(3, 24)
                .soft_wrap(true)
                .searchable(true)
        });

        let new_project_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Project name..."));

        let new_list_input = cx.new(|cx| InputState::new(window, cx).placeholder("List name..."));
        let new_board_input = cx.new(|cx| InputState::new(window, cx).placeholder("Board name..."));

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
                        Self::add_project(
                            cx,
                            ProjectDTO {
                                id: 0,
                                name: name.to_string(),
                                is_expanded: true,
                                boards: vec![],
                            },
                        );
                    }
                    this.is_adding_project = false;
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
            &new_list_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).text().to_string();
                    let name = text.trim();
                    if !name.is_empty() {
                        let project_index = this.active_project_index;
                        let board_index = this.active_board_index.unwrap_or(0);

                        let board_id = this
                            .projects
                            .get(project_index)
                            .and_then(|p| p.boards.get(board_index))
                            .map(|b| b.id);

                        if let Some(board_id) = board_id {
                            this.add_card(
                                cx,
                                CardDTO {
                                    id: 0,
                                    title: name.to_string(),
                                    board_id,
                                    drop_on: None,
                                    entries: vec![],
                                },
                                project_index,
                                board_index,
                                board_id,
                            );
                        }
                    }
                    this.is_adding_list = false;
                }
                InputEvent::Blur => {
                    this.is_adding_list = false;
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
                }
                InputEvent::Blur => {
                    this.adding_board_to_project = None;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        let app = Self {
            active_project_index: 0,
            active_board_index: Some(0),
            focus_handle: cx.focus_handle(),
            search_input,
            dialog_title_input,
            dialog_description_input,
            theme_select,
            projects: vec![],
            is_adding_project: false,
            is_adding_list: false,
            new_project_input,
            new_list_input,
            new_board_input,
            pending_card_id: None,
            adding_board_to_project: None,
        };

        Self::list_projects(cx);
        app
    }

    fn list_projects(cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let results = Project::load().with(Board).all(&*db).await?;

            let mut projects: Vec<ProjectDTO> = results
                .into_iter()
                .map(|p| ProjectDTO {
                    id: p.id as u32,
                    name: p.name,
                    is_expanded: false,
                    boards: p
                        .boards
                        .into_iter()
                        .map(|b| BoardDTO {
                            id: b.id as u32,
                            project_id: b.project_id as u32,
                            title: b.title,
                            cards: vec![],
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
                    let board_id = first_board.id;
                    Self::enrich_board_async(cx, 0, 0, board_id);
                } else {
                    cx.notify();
                }
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn enrich_board_async(
        cx: &mut Context<Self>,
        project_index: usize,
        board_index: usize,
        board_id: u32,
    ) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let result = Card::load()
                .filter(card::Column::BoardId.eq(board_id as i32))
                .with(Entry)
                .all(&*db)
                .await?;

            let cards: Vec<CardDTO> = result
                .into_iter()
                .map(|c| CardDTO {
                    id: c.id as u32,
                    board_id: c.board_id as u32,
                    title: c.title,
                    drop_on: None,
                    entries: c
                        .entries
                        .into_iter()
                        .map(|e| EntryDTO {
                            id: e.id as u32,
                            title: e.title,
                            description: e.description,
                            card_id: e.card_id as u32,
                        })
                        .collect(),
                })
                .collect();

            this.update(cx, |this, cx| {
                if let Some(board) = this
                    .projects
                    .get_mut(project_index)
                    .and_then(|p| p.boards.get_mut(board_index))
                {
                    board.cards = cards;
                    cx.notify();
                };
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn add_project(cx: &mut Context<Self>, project: ProjectDTO) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let project_active_model = project::ActiveModel {
                name: Set(project.name),
                ..Default::default()
            };

            let project_entity = project_active_model.insert(&*db).await?;
            let project = ProjectDTO {
                id: project_entity.id as u32,
                name: project_entity.name,
                is_expanded: true,
                boards: vec![],
            };

            this.update(cx, |this, cx| {
                this.projects.push(project);
                cx.notify()
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn add_entry(
        &mut self,
        cx: &mut Context<Self>,
        entry: EntryDTO,
        project_index: usize,
        board_index: usize,
        card_id: u32,
    ) {
        let db = cx.global::<DB>().conn.clone();

        if let Some(board) = self
            .projects
            .get_mut(project_index)
            .and_then(|p| p.boards.get_mut(board_index))
            && let Some(card) = board.cards.iter_mut().find(|c| c.id == card_id)
        {
            card.entries.push(entry.clone());
            cx.notify();
        };

        cx.spawn(async move |this, cx| -> Result<()> {
            let entry = entry.clone();
            let model = entry::ActiveModel {
                title: Set(entry.title),
                description: Set(entry.description),
                card_id: Set(entry.card_id as i64),
                ..Default::default()
            };
            let inserted = model.insert(&*db).await?;
            let real_id = inserted.id as u32;

            this.update(cx, |this, _cx| {
                if let Some(card) = this
                    .projects
                    .get_mut(project_index)
                    .and_then(|p| p.boards.get_mut(board_index))
                    .and_then(|b| b.cards.iter_mut().find(|c| c.id == card_id))
                    && let Some(entry) = card.entries.last_mut()
                {
                    entry.id = real_id;
                };
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn add_card(
        &mut self,
        cx: &mut Context<Self>,
        card: CardDTO,
        project_index: usize,
        board_index: usize,
        board_id: u32,
    ) {
        let db = cx.global::<DB>().conn.clone();

        if let Some(board) = self
            .projects
            .get_mut(project_index)
            .and_then(|p| p.boards.get_mut(board_index))
        {
            board.cards.push(card.clone());
            cx.notify();
        };

        cx.spawn(async move |this, cx| -> Result<()> {
            let model = card::ActiveModel {
                title: Set(card.title),
                board_id: Set(board_id as i64),
                ..Default::default()
            };
            let inserted = model.insert(&*db).await?;
            let real_id = inserted.id as u32;

            this.update(cx, |this, _cx| {
                if let Some(board) = this
                    .projects
                    .get_mut(project_index)
                    .and_then(|p| p.boards.get_mut(board_index))
                    && let Some(last_card) = board.cards.last_mut()
                {
                    last_card.id = real_id;
                };
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn add_board(&mut self, cx: &mut Context<Self>, project_index: usize, title: String) {
        let db = cx.global::<DB>().conn.clone();

        if let Some(project) = self.projects.get(project_index) {
            let project_id = project.id;

            cx.spawn(async move |this, cx| -> Result<()> {
                let board_active_model = board::ActiveModel {
                    title: Set(title),
                    project_id: Set(project_id as i64),
                    ..Default::default()
                };

                let board_entity = board_active_model.insert(&*db).await?;
                let board = BoardDTO {
                    id: board_entity.id as u32,
                    title: board_entity.title,
                    project_id: board_entity.project_id as u32,
                    cards: vec![],
                };

                this.update(cx, |this, cx| {
                    if let Some(project) = this.projects.get_mut(project_index) {
                        let board_index = project.boards.len();
                        project.boards.push(board);
                        this.active_project_index = project_index;
                        this.active_board_index = Some(board_index);
                        cx.notify();
                    }
                })
                .ok();

                Ok(())
            })
            .detach();
        }
    }

    fn show_add_entry_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog_title_input = self.dialog_title_input.clone();
        let dialog_description_input = self.dialog_description_input.clone();
        let active_project_index = self.active_project_index;
        let active_board_index = self.active_board_index.unwrap_or(0);
        let card_id = self.pending_card_id.unwrap_or(0);

        let confirm_handler = Rc::new(cx.listener(move |this, _, _, cx| {
            let title = this.dialog_title_input.read(cx).text().to_string();
            let description = this.dialog_description_input.read(cx).text().to_string();
            let entry = EntryDTO {
                id: 0,
                title,
                description,
                card_id,
            };
            this.pending_card_id = None;
            this.add_entry(cx, entry, active_project_index, active_board_index, card_id);
        }));

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let confirm_handler = confirm_handler.clone();
            dialog
                .on_ok(move |e, window, cx| {
                    (confirm_handler)(e, window, cx);
                    true
                })
                .child(
                    DialogHeader::new()
                        .mb_2()
                        .child(DialogTitle::new().child("Add a new entry"))
                        .child(DialogDescription::new().child("Enter the information needed")),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .mb_3()
                        .child(Input::new(&dialog_title_input))
                        .child(Input::new(&dialog_description_input)),
                )
                .child(
                    DialogFooter::new()
                        .justify_between()
                        .child(DialogClose::new().child(
                            Button::new("cancel").label("Cancel").outline().on_click({
                                move |_, window, cx| {
                                    window.close_dialog(cx);
                                }
                            }),
                        ))
                        .child(
                            DialogAction::new()
                                .child(Button::new("confirm").primary().label("Confirm")),
                        ),
                )
        });
    }
}

#[derive(Clone, PartialEq, Eq)]
struct DragInfo {
    entry_id: u32,
    source_board_id: u32,
    source_card_id: u32,
    position: Point<Pixels>,
    title: Arc<str>,
}

impl DragInfo {
    fn new(entry_id: u32, source_board_id: u32, source_card_id: u32, title: Arc<str>) -> Self {
        Self {
            entry_id,
            source_board_id,
            source_card_id,
            position: Point::default(),
            title,
        }
    }

    fn position(mut self, pos: Point<Pixels>) -> Self {
        self.position = pos;
        self
    }
}

impl Render for DragInfo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let size = gpui::size(px(200.), px(40.));

        div()
            .pl(self.position.x - size.width.half())
            .pt(self.position.y - size.height.half())
            .child(
                div()
                    .flex()
                    .justify_start()
                    .items_center()
                    .w(size.width)
                    .h(size.height)
                    .p_2()
                    .bg(cx.theme().primary.opacity(0.7))
                    .text_color(cx.theme().primary_foreground)
                    .rounded(cx.theme().radius)
                    .text_sm()
                    .shadow_md()
                    .child(self.title.clone().to_string()),
            )
    }
}

struct ProjectDTO {
    id: u32,
    name: String,
    is_expanded: bool,
    boards: Vec<BoardDTO>,
}

#[derive(Clone, PartialEq, Eq)]
struct BoardDTO {
    id: u32,
    title: String,
    project_id: u32,
    cards: Vec<CardDTO>,
}

#[derive(Clone, PartialEq, Eq)]
struct CardDTO {
    id: u32,
    title: String,
    board_id: u32,
    drop_on: Option<DragInfo>,
    entries: Vec<EntryDTO>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryDTO {
    id: u32,
    title: String,
    description: String,
    card_id: u32,
}

impl ProjectDTO {
    pub fn handler(
        &self,
        index: usize,
    ) -> impl Fn(&mut CastleApp, &ClickEvent, &mut Window, &mut Context<CastleApp>) + 'static {
        move |app, _, window, cx| {
            app.active_project_index = index;
            app.active_board_index = None;
            if let Some(p) = app.projects.get_mut(index) {
                p.is_expanded = !p.is_expanded;
            }
            app.focus_handle.focus(window, cx);
        }
    }
}

impl BoardDTO {
    pub fn handler(
        &self,
        project_index: usize,
        board_index: usize,
        board_id: u32,
    ) -> impl Fn(&mut CastleApp, &ClickEvent, &mut Window, &mut Context<CastleApp>) + 'static {
        move |app, _, window, cx| {
            app.active_project_index = project_index;
            app.active_board_index = Some(board_index);
            app.focus_handle.focus(window, cx);
            CastleApp::enrich_board_async(cx, project_index, board_index, board_id);
        }
    }
}

impl Focusable for CastleApp {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CastleApp {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let theme = cx.theme().clone();
        v_flex()
            .id("app-container")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_scroll()
            .child(TitleBar::new().bg(theme.sidebar))
            .child(
                h_flex()
                    .id("main-container")
                    .size_full()
                    .overflow_scroll()
                    .rounded(theme.radius)
                    .child(
                        Sidebar::new("sidebar-story")
                            .w(px(260.))
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
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.is_adding_project = true;
                                                    cx.notify();
                                                }))
                                                .into_any_element()
                                        }
                                    }),
                            )
                            .child(SidebarGroup::new("Projects").child(
                                SidebarMenu::new().children(self.projects.iter().enumerate().map(
                                    |(p_idx, p)| {
                                        SidebarMenuItem::new(p.name.clone())
                                            .icon(IconName::FolderOpen)
                                            .active(
                                                self.active_project_index == p_idx
                                                    && self.active_board_index.is_none(),
                                            )
                                            .default_open(p.is_expanded)
                                            .click_to_toggle(true)
                                            .children({
                                                p.boards
                                                    .iter()
                                                    .enumerate()
                                                    .map(|(b_idx, b)| {
                                                        SidebarMenuItem::new(b.title.clone())
                                                            .active(
                                                                self.active_project_index == p_idx
                                                                    && self.active_board_index
                                                                        == Some(b_idx),
                                                            )
                                                            .on_click(cx.listener(
                                                                b.handler(p_idx, b_idx, b.id),
                                                            ))
                                                    })
                                                    .collect::<Vec<_>>()
                                            })
                                            .on_click(cx.listener(p.handler(p_idx)))
                                    },
                                )),
                            ))
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
                    .child({
                        let active_board =
                            self.projects.get(self.active_project_index).and_then(|p| {
                                if let Some(b_idx) = self.active_board_index {
                                    p.boards.get(b_idx)
                                } else {
                                    p.boards.first()
                                }
                            });

                        let board_id_for_render = active_board.map(|b| b.id).unwrap_or(0);
                        let cards = active_board.map(|b| b.cards.as_slice()).unwrap_or(&[]);

                        h_flex()
                            .id("scrollable-container")
                            .size_full()
                            .overflow_x_scrollbar()
                            .gap_4()
                            .p_4()
                            .items_start()
                            .children({
                                cards
                                    .iter()
                                    .map(|card| {
                                        let card_id = card.id;
                                        let board_id = board_id_for_render;

                                        v_flex()
                                            .id(card.id as usize)
                                            .w_80()
                                            .min_w_auto()
                                            .max_h_3_4()
                                            .h_auto()
                                            .gap_2()
                                            .p_2()
                                            .bg(cx.theme().secondary)
                                            .text_color(cx.theme().secondary_foreground)
                                            .rounded(cx.theme().radius)
                                            .drag_over::<DragInfo>(|this, _, _, cx| {
                                                this.border_1().border_color(cx.theme().primary)
                                            })
                                            .on_drop(cx.listener(
                                                move |this, info: &DragInfo, _, _| {
                                                    if info.source_board_id == board_id
                                                        && info.source_card_id == card_id
                                                    {
                                                        return;
                                                    }

                                                    let active_project = this
                                                        .projects
                                                        .get_mut(this.active_project_index);

                                                    if let Some(project) = active_project {
                                                        let mut moving_entry: Option<EntryDTO> =
                                                            None;

                                                        if let Some(b) = project
                                                            .boards
                                                            .iter_mut()
                                                            .find(|b| b.id == info.source_board_id)
                                                            && let Some(source_card) =
                                                                b.cards.iter_mut().find(|c| {
                                                                    c.id == info.source_card_id
                                                                })
                                                            && let Some(index) = source_card
                                                                .entries
                                                                .iter()
                                                                .position(|entry| {
                                                                    entry.id == info.entry_id
                                                                })
                                                        {
                                                            moving_entry = Some(
                                                                source_card.entries.remove(index),
                                                            );
                                                        }

                                                        if let Some(entry) = moving_entry
                                                            && let Some(b) = project
                                                                .boards
                                                                .iter_mut()
                                                                .find(|b| b.id == board_id)
                                                            && let Some(c) = b
                                                                .cards
                                                                .iter_mut()
                                                                .find(|c| c.id == card_id)
                                                        {
                                                            c.entries.push(entry);
                                                            c.drop_on = Some(info.clone());
                                                        }
                                                    }
                                                },
                                            ))
                                            .child(
                                                div()
                                                    .p_1()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(card.title.clone()),
                                            )
                                            .children(card.entries.iter().map(|entry| {
                                                let drag_info = DragInfo::new(
                                                    entry.id,
                                                    board_id,
                                                    card_id,
                                                    entry.title.clone().into(),
                                                );

                                                div()
                                                    .id(entry.id as usize)
                                                    .p_2()
                                                    .bg(cx.theme().primary)
                                                    .text_color(cx.theme().primary_foreground)
                                                    .rounded(cx.theme().radius)
                                                    .hover(|this| {
                                                        this.bg(cx.theme().primary_hover)
                                                            .cursor(CursorStyle::PointingHand)
                                                            .border_1()
                                                            .border_color(
                                                                cx.theme().primary_foreground,
                                                            )
                                                    })
                                                    .cursor_move()
                                                    .text_sm()
                                                    .w_full()
                                                    .child(entry.title.clone())
                                                    .on_drag(
                                                        drag_info,
                                                        |info: &DragInfo, position, _, cx| {
                                                            cx.new(|_| {
                                                                info.clone().position(position)
                                                            })
                                                        },
                                                    )
                                            }))
                                            .child(
                                                h_flex()
                                                    .id(("add-item", card_id as usize))
                                                    .w_full()
                                                    .rounded(cx.theme().radius)
                                                    .gap_2()
                                                    .p_1()
                                                    .text_color(cx.theme().secondary_foreground)
                                                    .text_sm()
                                                    .hover(|this| {
                                                        this.bg(cx.theme().secondary_hover)
                                                            .text_color(
                                                                cx.theme().accent_foreground,
                                                            )
                                                            .cursor(CursorStyle::PointingHand)
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(move |this, _, window, cx| {
                                                            this.pending_card_id = Some(card_id);
                                                            this.show_add_entry_dialog(window, cx);
                                                        }),
                                                    )
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(IconName::Plus)
                                                    .child("Add a card"),
                                            )
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .child({
                                if self.is_adding_list {
                                    Input::new(&self.new_list_input).w_80().into_any_element()
                                } else {
                                    h_flex()
                                        .id("add-list-button")
                                        .gap_2()
                                        .w_80()
                                        .p_2()
                                        .bg(theme.info.opacity(0.12))
                                        .text_color(theme.info)
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .border_1()
                                        .border_color(theme.info.opacity(0.24))
                                        .rounded(theme.radius)
                                        .cursor_pointer()
                                        .hover(|this| this.bg(theme.info.opacity(0.18)))
                                        .on_click(cx.listener(|this, _, _window, cx| {
                                            this.is_adding_list = true;
                                            cx.notify();
                                        }))
                                        .child(IconName::Plus)
                                        .child("Add another list")
                                        .into_any_element()
                                }
                            })
                    })
                    .children(dialog_layer),
            )
    }
}

#[derive(Clone)]
struct DB {
    conn: Arc<DatabaseConnection>,
}

impl Global for DB {}

#[tokio::main]
async fn main() -> Result<()> {
    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);
    dotenv()?;

    let database_url = env::var("DATABASE_URL")?;
    let db_path = database_url.trim_start_matches("sqlite:");
    if !Path::new(db_path).exists() {
        fs::File::create(db_path)?;
    }

    let connection = Database::connect(&database_url).await?;
    Migrator::up(&connection, None).await?;

    let db = DB {
        conn: Arc::new(connection),
    };

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);

        init_themes(cx);

        cx.set_global(db);

        let bounds = Bounds::centered(None, size(px(1200.), px(768.)), cx);
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitleBar::title_bar_options()),
                    ..Default::default()
                },
                |window, cx| {
                    let view = CastleApp::view(window, cx);
                    // This first level on the window, should be a Root.
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("Failed to open window");
        })
        .detach();
    });

    Ok(())
}

fn init_themes(cx: &mut App) {
    let theme_contents = [
        include_str!("../../../themes/alduin.json"),
        include_str!("../../../themes/ayu.json"),
        include_str!("../../../themes/catppuccin.json"),
        include_str!("../../../themes/everforest.json"),
        include_str!("../../../themes/gruvbox.json"),
        include_str!("../../../themes/harper.json"),
        include_str!("../../../themes/jellybeans.json"),
        include_str!("../../../themes/molokai.json"),
        include_str!("../../../themes/tokyonight.json"),
        include_str!("../../../themes/twilight.json"),
    ];

    for content in theme_contents {
        if let Err(err) = ThemeRegistry::global_mut(cx).load_themes_from_str(content) {
            eprintln!("Failed to load embedded theme: {}", err);
        }
    }

    apply_default_theme(cx);
    cx.refresh_windows();
}

fn apply_default_theme(cx: &mut App) {
    let theme_name = SharedString::from("Alduin");
    if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
        Theme::global_mut(cx).apply_config(&theme);
    }
}
