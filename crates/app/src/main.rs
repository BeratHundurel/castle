#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use dotenvy::dotenv;
use gpui::{App, AppContext, Bounds, SharedString, WindowBounds, WindowOptions, px, size};
use gpui_component::{Root, Theme, ThemeRegistry, TitleBar};
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::{env, fs, path::Path};

use app::{DB, app_settings::AppSettings, app_shell::AppShell, keymap, tray};

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

    let mut options = ConnectOptions::new(database_url);
    options.max_connections(4).min_connections(1);
    let connection = Database::connect(options).await?;
    Migrator::up(&connection, None).await?;

    let db = DB {
        conn: Arc::new(connection),
        data_dir,
    };
    let app_settings = AppSettings::load(&db.data_dir);

    app.run(move |cx| {
        gpui_component::init(cx);
        load_bundled_fonts(cx);
        keymap::init(cx);

        init_http_client(cx);
        init_themes(cx);

        app_settings.apply_to_theme(cx);
        cx.set_global(app_settings.clone());
        cx.set_global(db);

        let bounds = Bounds::centered(None, size(px(1200.), px(768.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitleBar::title_bar_options()),
                    ..Default::default()
                },
                |window, cx| {
                    let view = AppShell::view(window, cx);
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("Failed to open window");

        if let Err(err) = tray::init(window.into(), cx) {
            eprintln!("Failed to initialize tray mode: {err}");
        }
    });

    Ok(())
}

fn load_bundled_fonts(cx: &mut App) {
    let fonts = vec![
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-Italic.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-Medium.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-MediumItalic.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-SemiBold.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-SemiBoldItalic.ttf")
                .as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-Bold.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-BoldItalic.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-mono/IBMPlexMono-Regular.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-mono/IBMPlexMono-Italic.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-mono/IBMPlexMono-Bold.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-mono/IBMPlexMono-BoldItalic.ttf").as_slice(),
        ),
    ];

    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!("Failed to load bundled fonts: {err}");
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
        include_str!("../../../themes/ayu.json"),
        include_str!("../../../themes/catppuccin.json"),
        include_str!("../../../themes/everforest.json"),
        include_str!("../../../themes/gruvbox.json"),
        include_str!("../../../themes/harper.json"),
        include_str!("../../../themes/jellybeans.json"),
        include_str!("../../../themes/tokyonight.json"),
        include_str!("../../../themes/twilight.json"),
        include_str!("../../../themes/spaceduck.json"),
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
    let theme_name = SharedString::from("Sick");
    if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
        Theme::global_mut(cx).apply_config(&theme);
    }
}
