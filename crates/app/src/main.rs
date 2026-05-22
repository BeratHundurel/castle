#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod board_view;
mod markdown_editor_view;
mod sidebar_view;

use anyhow::Result;
use board_view::BoardView;
use dotenvy::dotenv;
use entity::{
    board, board::Entity as Board, note, note::Entity as Note, project::Entity as Project,
};
use gpui::{
    App, AppContext, Bounds, Context, Entity, FocusHandle, Focusable, Global, InteractiveElement,
    IntoElement, ParentElement, PathPromptOptions, Render, SharedString, Styled, Subscription,
    Window, WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::{
    ActiveTheme, IconName, Root, Sizable as _, Theme, ThemeRegistry, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
    tab::{Tab, TabBar},
    v_flex,
};
use markdown_editor_view::{DEFAULT_NOTE, MarkdownEditorView, SaveState, now_ts};
use migration::{Migrator, MigratorTrait};
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter,
};
use sidebar_view::{SidebarEvent, SidebarView};
use std::path::PathBuf;
use std::sync::Arc;
use std::{env, fs, path::Path};

struct OpenTab {
    id: u64,
    title: SharedString,
    kind: OpenTabKind,
}

enum OpenTabKind {
    Chooser,
    Board {
        board_id: u32,
        view: Entity<BoardView>,
    },
    Note {
        note_id: u32,
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

pub struct CastleApp {
    focus_handle: FocusHandle,
    sidebar: Entity<SidebarView>,
    title_input: Entity<InputState>,
    _title_subscription: Subscription,
    open_tabs: Vec<OpenTab>,
    active_tab_index: usize,
    next_tab_id: u64,
    projects: Vec<ProjectChoice>,
    boards: Vec<BoardChoice>,
    active_project_id: Option<u32>,
    suppress_title_event: bool,
}

impl CastleApp {
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

        let title_subscription =
            cx.subscribe(&title_input, |this, input, event: &InputEvent, cx| {
                if !matches!(event, InputEvent::Change) || this.suppress_title_event {
                    return;
                }

                let title = input.read(cx).text().to_string();
                this.rename_active_tab(title, cx);
            });

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
                    this.open_board_tab(*board_id, title.clone(), window, cx);
                }
                SidebarEvent::OpenNote {
                    note_id,
                    project_id,
                    title,
                } => {
                    this.active_project_id = *project_id;
                    this.open_note_tab(*note_id, title.clone(), window, cx);
                }
                SidebarEvent::ActivateProject { project_id } => {
                    this.active_project_id = Some(*project_id);
                    cx.notify();
                }
            },
        )
        .detach();

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            sidebar,
            title_input,
            _title_subscription: title_subscription,
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

    fn refresh_workspace(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let projects = Project::find().all(&*db).await?;
            let boards = Board::find().all(&*db).await?;

            let project_choices: Vec<ProjectChoice> = projects
                .iter()
                .map(|project| ProjectChoice {
                    id: project.id as u32,
                    name: SharedString::from(project.name.clone()),
                })
                .collect();

            let board_choices: Vec<BoardChoice> = boards
                .into_iter()
                .map(|board| {
                    let project_name = board.project_id.and_then(|project_id| {
                        projects
                            .iter()
                            .find(|project| project.id == project_id)
                            .map(|project| SharedString::from(project.name.clone()))
                    });

                    BoardChoice {
                        id: board.id as u32,
                        title: SharedString::from(board.title),
                        project_name,
                    }
                })
                .collect();

            this.update(cx, |this, cx| {
                this.projects = project_choices;
                this.boards = board_choices;
                cx.notify();
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.open_tabs.len();
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.open_tabs.push(OpenTab {
            id,
            title: "New tab".into(),
            kind: OpenTabKind::Chooser,
        });
        self.activate_tab(index, window, cx);
    }

    fn activate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.open_tabs.len() {
            return;
        }

        self.active_tab_index = index;
        self.sync_title_input(window, cx);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.open_tabs.len() {
            return;
        }

        self.open_tabs.remove(index);
        if self.open_tabs.is_empty() {
            self.open_tabs.push(OpenTab {
                id: self.next_tab_id,
                title: "New tab".into(),
                kind: OpenTabKind::Chooser,
            });
            self.next_tab_id = self.next_tab_id.saturating_add(1);
            self.active_tab_index = 0;
        } else if self.active_tab_index >= self.open_tabs.len() {
            self.active_tab_index = self.open_tabs.len().saturating_sub(1);
        } else if self.active_tab_index > index {
            self.active_tab_index -= 1;
        }

        self.sync_title_input(window, cx);
        cx.notify();
    }

    fn sync_title_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .open_tabs
            .get(self.active_tab_index)
            .map(|tab| tab.title.to_string())
            .unwrap_or_else(|| "New tab".to_string());

        self.suppress_title_event = true;
        self.title_input.update(cx, |input, cx| {
            input.set_value(title, window, cx);
        });
        self.suppress_title_event = false;
    }

    fn rename_active_tab(&mut self, title: String, cx: &mut Context<Self>) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }

        let Some(tab) = self.open_tabs.get_mut(self.active_tab_index) else {
            return;
        };

        tab.title = SharedString::from(title);
        match &tab.kind {
            OpenTabKind::Note { view, .. } => {
                view.update(cx, |note, cx| note.set_title(title.to_string(), cx));
                self.sidebar
                    .update(cx, |_, cx| SidebarView::list_projects(cx));
            }
            OpenTabKind::Board { board_id, .. } => {
                let db = cx.global::<DB>().conn.clone();
                let board_id = *board_id;
                let title = title.to_string();
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
                self.sidebar
                    .update(cx, |_, cx| SidebarView::list_projects(cx));
                self.refresh_workspace(cx);
            }
            OpenTabKind::Chooser => {}
        }

        cx.notify();
    }

    fn open_board_tab(
        &mut self,
        board_id: u32,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.open_tabs.iter().position(
            |tab| matches!(&tab.kind, OpenTabKind::Board { board_id: id, .. } if *id == board_id),
        ) {
            self.activate_tab(index, window, cx);
            return;
        }

        let view = BoardView::view(window, cx);
        view.update(cx, |board, cx| board.load_board(board_id, cx));
        self.replace_or_push_active(OpenTabKind::Board { board_id, view }, title, window, cx);
    }

    fn open_note_tab(
        &mut self,
        note_id: u32,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.open_tabs.iter().position(
            |tab| matches!(&tab.kind, OpenTabKind::Note { note_id: id, .. } if *id == note_id),
        ) {
            self.activate_tab(index, window, cx);
            return;
        }

        let view = MarkdownEditorView::view(note_id, window, cx);
        self.replace_or_push_active(OpenTabKind::Note { note_id, view }, title, window, cx);
    }

    fn replace_or_push_active(
        &mut self,
        kind: OpenTabKind,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.open_tabs.get_mut(self.active_tab_index)
            && matches!(tab.kind, OpenTabKind::Chooser)
        {
            tab.kind = kind;
            tab.title = title;
            self.sync_title_input(window, cx);
            cx.notify();
            return;
        }

        let index = self.open_tabs.len();
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.open_tabs.push(OpenTab { id, title, kind });
        self.activate_tab(index, window, cx);
    }

    fn create_note(
        &mut self,
        project_id: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let title = SharedString::from("Untitled note");
        let now = now_ts();
        let view = cx.entity();

        cx.spawn_in(window, async move |_, window| {
            let inserted = note::ActiveModel {
                title: Set(title.to_string()),
                project_id: Set(project_id.map(|id| id as i64)),
                file_path: Set(None),
                cached_content: Set(DEFAULT_NOTE.to_string()),
                file_missing_since: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&*db)
            .await
            .ok()?;

            window
                .update(|window, cx| {
                    view.update(cx, |this, cx| {
                        this.open_note_tab(
                            inserted.id as u32,
                            SharedString::from(inserted.title),
                            window,
                            cx,
                        );
                        this.sidebar
                            .update(cx, |_, cx| SidebarView::list_projects(cx));
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    fn open_note_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open note file".into()),
        });
        let db = cx.global::<DB>().conn.clone();
        let view = cx.entity();

        cx.spawn_in(window, async move |_, window| {
            let paths = paths.await.ok()?.ok()??;
            let path = paths.first()?.clone();
            let content = fs::read_to_string(&path).ok()?;
            let path_string = path.display().to_string();
            let title = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled note")
                .to_string();

            let existing = Note::find()
                .filter(note::Column::FilePath.eq(path_string.clone()))
                .one(&*db)
                .await
                .ok()
                .flatten();

            let note = if let Some(existing) = existing {
                note::ActiveModel {
                    id: Set(existing.id),
                    title: Set(existing.title),
                    file_path: Set(Some(path_string)),
                    cached_content: Set(content),
                    file_missing_since: Set(None),
                    updated_at: Set(now_ts()),
                    ..Default::default()
                }
                .update(&*db)
                .await
                .ok()?
            } else {
                let now = now_ts();
                note::ActiveModel {
                    title: Set(title),
                    project_id: Set(None),
                    file_path: Set(Some(path_string)),
                    cached_content: Set(content),
                    file_missing_since: Set(None),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .insert(&*db)
                .await
                .ok()?
            };

            window
                .update(|window, cx| {
                    view.update(cx, |this, cx| {
                        this.open_note_tab(
                            note.id as u32,
                            SharedString::from(note.title),
                            window,
                            cx,
                        );
                        this.sidebar
                            .update(cx, |_, cx| SidebarView::list_projects(cx));
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    fn create_board(
        &mut self,
        project_id: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let view = cx.entity();

        cx.spawn_in(window, async move |_, window| {
            let inserted = board::ActiveModel {
                title: Set("Board".to_string()),
                project_id: Set(project_id.map(|id| id as i64)),
                ..Default::default()
            }
            .insert(&*db)
            .await
            .ok()?;

            window
                .update(|window, cx| {
                    view.update(cx, |this, cx| {
                        this.open_board_tab(
                            inserted.id as u32,
                            SharedString::from(inserted.title),
                            window,
                            cx,
                        );
                        this.sidebar
                            .update(cx, |_, cx| SidebarView::list_projects(cx));
                        this.refresh_workspace(cx);
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        TitleBar::new().border_0().bg(theme.sidebar).child(
            h_flex()
                .id("title-bar-content")
                .size_full()
                .items_center()
                .child(
                    h_flex()
                        .id("active-title")
                        .w(px(248.))
                        .h_full()
                        .items_center()
                        .child(active_tab_icon(self.open_tabs.get(self.active_tab_index)))
                        .child(
                            Input::new(&self.title_input)
                                .border_0()
                                .bg(theme.sidebar)
                                .rounded_none()
                                .w_full(),
                        ),
                )
                .child(self.render_tabs(cx)),
        )
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_index = self
            .active_tab_index
            .min(self.open_tabs.len().saturating_sub(1));

        TabBar::new("open-tabs")
            .mx_5()
            .segmented()
            .menu(true)
            .bg(cx.theme().sidebar)
            .selected_index(active_index)
            .on_click(cx.listener(|this, index: &usize, window, cx| {
                this.activate_tab(*index, window, cx);
            }))
            .children(self.open_tabs.iter().enumerate().map(|(index, tab)| {
                Tab::new()
                    .px_2()
                    .label(tab_label(tab, cx))
                    .prefix(active_tab_icon(Some(tab)))
                    .suffix(
                        Button::new(("close-tab", tab.id as usize))
                            .icon(IconName::Close)
                            .ghost()
                            .xsmall()
                            .tooltip("Close tab")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_tab(index, window, cx);
                            })),
                    )
            }))
            .suffix(
                Button::new("new-tab")
                    .icon(IconName::Plus)
                    .ghost()
                    .xsmall()
                    .tooltip("New tab")
                    .on_click(cx.listener(|this, _, window, cx| this.new_tab(window, cx))),
            )
    }

    fn render_active_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(tab) = self.open_tabs.get(self.active_tab_index) else {
            return div().size_full().into_any_element();
        };

        match &tab.kind {
            OpenTabKind::Chooser => self.render_chooser(cx).into_any_element(),
            OpenTabKind::Board { view, .. } => view.clone().into_any_element(),
            OpenTabKind::Note { view, .. } => view.clone().into_any_element(),
        }
    }

    fn render_chooser(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_project = self
            .active_project_id
            .and_then(|id| self.projects.iter().find(|project| project.id == id));

        v_flex()
            .id("new-tab-chooser")
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .p_6()
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("New tab"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Choose what to open."),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("new-note-active")
                            .label(match active_project {
                                Some(project) => format!("New note in {}", project.name),
                                None => "New note".to_string(),
                            })
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_note(this.active_project_id, window, cx);
                            })),
                    )
                    .child(
                        Button::new("new-note-standalone")
                            .label("Standalone note")
                            .outline()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_note(None, window, cx);
                            })),
                    )
                    .child(
                        Button::new("open-note-file")
                            .label("Open note file")
                            .outline()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_note_file(window, cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("new-board-active")
                            .label(match active_project {
                                Some(project) => format!("New board in {}", project.name),
                                None => "New board".to_string(),
                            })
                            .outline()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_board(this.active_project_id, window, cx);
                            })),
                    )
                    .child(
                        Button::new("new-board-standalone")
                            .label("Standalone board")
                            .outline()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_board(None, window, cx);
                            })),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .w(px(420.))
                    .max_h(px(220.))
                    .overflow_y_scrollbar()
                    .children(self.boards.iter().map(|board| {
                        let board_id = board.id;
                        let title = board.title.clone();
                        let subtitle = board
                            .project_name
                            .clone()
                            .unwrap_or_else(|| "Standalone".into());

                        Button::new(("open-board", board_id as usize))
                            .label(format!("{} - {}", title, subtitle))
                            .ghost()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_board_tab(board_id, title.clone(), window, cx);
                            }))
                    })),
            )
    }
}

impl Focusable for CastleApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CastleApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let theme = cx.theme().clone();

        v_flex()
            .id("app-container")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .id("main-container")
                    .size_full()
                    .overflow_hidden()
                    .rounded(theme.radius)
                    .child(self.sidebar.clone())
                    .child(
                        v_flex()
                            .id("content-container")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .min_w_0()
                                    .w_full()
                                    .overflow_hidden()
                                    .child(self.render_active_tab(cx)),
                            ),
                    )
                    .children(dialog_layer),
            )
    }
}

#[derive(Clone)]
pub(crate) struct DB {
    conn: Arc<DatabaseConnection>,
    data_dir: PathBuf,
}

impl Global for DB {}

#[tokio::main]
async fn main() -> Result<()> {
    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);
    dotenv()?;

    let database_url = env::var("DATABASE_URL")?;
    let db_path = PathBuf::from(database_url.trim_start_matches("sqlite:"));
    if !db_path.exists() {
        fs::File::create(&db_path)?;
    }

    let data_dir = db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let connection = Database::connect(&database_url).await?;
    Migrator::up(&connection, None).await?;

    let db = DB {
        conn: Arc::new(connection),
        data_dir,
    };

    app.run(move |cx| {
        gpui_component::init(cx);
        markdown_editor_view::init(cx);

        init_http_client(cx);
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
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("Failed to open window");
        })
        .detach();
    });

    Ok(())
}

fn active_tab_icon(tab: Option<&OpenTab>) -> IconName {
    match tab.map(|tab| &tab.kind) {
        Some(OpenTabKind::Note { .. }) => IconName::BookOpen,
        Some(OpenTabKind::Board { .. }) => IconName::LayoutDashboard,
        _ => IconName::Plus,
    }
}

fn tab_label(tab: &OpenTab, cx: &mut Context<CastleApp>) -> SharedString {
    match &tab.kind {
        OpenTabKind::Note { view, .. } => {
            let state = view.read(cx).save_state();
            if matches!(
                state,
                SaveState::Dirty | SaveState::Missing | SaveState::Error(_)
            ) {
                SharedString::from(format!("* {}", tab.title))
            } else {
                tab.title.clone()
            }
        }
        _ => tab.title.clone(),
    }
}

fn init_http_client(cx: &mut App) {
    match reqwest_client::ReqwestClient::user_agent("castle") {
        Ok(client) => cx.set_http_client(Arc::new(client)),
        Err(err) => eprintln!("Failed to initialize HTTP client: {err}"),
    }
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
        include_str!("../../../themes/sick.json"),
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
    let theme_name = SharedString::from("Gruvbox Dark");
    if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
        Theme::global_mut(cx).apply_config(&theme);
    }
}
