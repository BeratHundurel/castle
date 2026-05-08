#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod board_view;
mod sidebar_view;

use anyhow::Result;
use board_view::BoardView;
use dotenvy::dotenv;
use gpui::{
    App, AppContext, Bounds, Context, Entity, FocusHandle, Focusable, Global, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, Styled, Window, WindowBounds, WindowOptions,
    div, px, size,
};
use gpui_component::{ActiveTheme, Root, Theme, ThemeRegistry, TitleBar, h_flex, v_flex};
use migration::{Migrator, MigratorTrait};
use sea_orm::{Database, DatabaseConnection};
use sidebar_view::{SidebarEvent, SidebarView};
use std::sync::Arc;
use std::{env, fs, path::Path};

pub struct CastleApp {
    focus_handle: FocusHandle,
    sidebar: Entity<SidebarView>,
    board: Entity<BoardView>,
}

impl CastleApp {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sidebar = SidebarView::view(window, cx);
        let board = BoardView::view(window, cx);

        cx.subscribe_in(
            &sidebar,
            window,
            |this, _, event: &SidebarEvent, window, cx| match event {
                SidebarEvent::BoardSelected { board_id } => {
                    this.board
                        .update(cx, |board, cx| board.load_board(*board_id, cx));

                    this.focus_handle.focus(window, cx);
                }
            },
        )
        .detach();

        sidebar.update(cx, |_, cx| SidebarView::list_projects(cx));

        Self {
            focus_handle: cx.focus_handle(),
            sidebar,
            board,
        }
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
            .child(TitleBar::new().bg(theme.sidebar))
            .child(
                h_flex()
                    .id("main-container")
                    .size_full()
                    .overflow_hidden()
                    .rounded(theme.radius)
                    .child(self.sidebar.clone())
                    .child(
                        div()
                            .id("board-container")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            .child(self.board.clone()),
                    )
                    .children(dialog_layer),
            )
    }
}

#[derive(Clone)]
pub(crate) struct DB {
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
