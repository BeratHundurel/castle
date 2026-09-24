#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use anyhow::{Context as _, Result};
#[cfg(debug_assertions)]
use dotenvy::dotenv;
use gpui_kit::component::{Root, Theme, ThemeRegistry, TitleBar};
use gpui_kit::{App, AppContext, Bounds, SharedString, WindowBounds, WindowOptions, px, size};
use std::{borrow::Cow, fs, rc::Rc, sync::Arc};
use storage::{Store, StoreOptions};

use app::{app_paths::AppPaths, keymap, system_notifications, tray};
use runtime::AppRuntime;
use settings::AppSettings;
use shell::{AppShell, ShellIntegration};

const MAIN_WINDOW_MIN_WIDTH: f32 = 800.;
const MAIN_WINDOW_MIN_HEIGHT: f32 = 600.;

#[tokio::main]
async fn main() -> Result<()> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if let Some(argument) = arguments.first() {
        if argument == "--register-mcp" {
            app::mcp_registration::register_installed()?;
            return Ok(());
        }
        if argument == "--unregister-mcp" {
            app::mcp_registration::unregister()?;
            return Ok(());
        }
    }
    let start_in_tray = app::startup::starts_in_tray(arguments);

    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    #[cfg(debug_assertions)]
    let _ = dotenv();

    let paths = AppPaths::discover()?;
    fs::create_dir_all(&paths.data_dir)?;
    let db_path = paths.database_path()?;
    let is_fresh_database = !db_path.exists();
    if is_fresh_database {
        fs::File::create(&db_path)?;
    }

    let store = Store::connect(StoreOptions::new(paths.database_url)).await?;

    let first_run_workspace = if is_fresh_database {
        storage::workspace::onboarding::seed_fresh_workspace(&store, &paths.data_dir).await?
    } else {
        None
    };

    let mut settings = AppSettings::load(&paths.data_dir);
    let app_runtime = AppRuntime::new(store.clone(), paths.data_dir);

    if let Some(first_run_workspace) = first_run_workspace {
        settings.set_first_run_note(
            first_run_workspace.docs_note.id,
            first_run_workspace.docs_note.title,
        );
    }

    system_notifications::start(store, &db_path);

    app.run(move |cx| {
        gpui_kit::init(cx);
        load_bundled_fonts(cx);
        keymap::init(cx);

        init_http_client(cx);
        init_themes(cx);

        settings.apply_to_theme(cx);
        cx.set_global(settings.clone());
        cx.on_app_quit(AppSettings::flush).detach();
        if let Err(error) = app::startup::set_start_at_login(AppSettings::start_at_login(cx)) {
            eprintln!("Failed to synchronize the start-at-login setting: {error}");
        }
        cx.set_global(app_runtime);
        system_notifications::install_board_gateway(cx);

        let (main_window_visibility, main_window_visibility_receiver) =
            tokio::sync::watch::channel(!start_in_tray);

        let window_factory: tray::MainWindowFactory = Rc::new(move |bounds, cx| {
            create_main_window(main_window_visibility_receiver.clone(), bounds, cx)
        });

        let initial_window = open_initial_window(start_in_tray, || window_factory(None, cx))
            .expect("Failed to open Castle window");

        if let Err(_err) = tray::init(
            window_factory.clone(),
            initial_window,
            main_window_visibility.clone(),
            cx,
        ) && start_in_tray
        {
            main_window_visibility.send_replace(true);
            window_factory(None, cx)
                .expect("Failed to open Castle window after tray initialization failed");
        }
    });

    Ok(())
}

fn open_initial_window<T>(
    start_in_tray: bool,
    open: impl FnOnce() -> Result<T>,
) -> Result<Option<T>> {
    if start_in_tray {
        Ok(None)
    } else {
        open().map(Some)
    }
}

fn create_main_window(
    main_window_visibility: tokio::sync::watch::Receiver<bool>,
    saved_bounds: Option<WindowBounds>,
    cx: &mut App,
) -> Result<tray::MainWindow> {
    let bounds = saved_bounds
        .map(window_bounds_for_tray_restore)
        .unwrap_or_else(|| {
            WindowBounds::Windowed(Bounds::centered(None, size(px(1200.), px(768.)), cx))
        });
    
    let mut shell = None;
    let window = cx.open_window(main_window_options(bounds), |window, cx| {
        let integration = ShellIntegration::new(
            app::tray::update_shortcut,
            app::tray::update_quick_capture_shortcut,
            |enabled| app::startup::set_start_at_login(enabled).map_err(|error| error.to_string()),
            |cx| app::keymap::shortcuts(cx).to_vec(),
            main_window_visibility,
            Arc::new(app::mcp_registration::McpAgentAccess),
        );
        let view = AppShell::view(window, integration, cx);
        shell = Some(view.downgrade());
        cx.new(|cx| Root::new(view, window, cx))
    })?;
    let shell = shell.context("Castle window did not construct its shell")?;
    Ok(tray::MainWindow::new(window.into(), shell))
}

fn window_bounds_for_tray_restore(saved_bounds: WindowBounds) -> WindowBounds {
    WindowBounds::Windowed(saved_bounds.get_bounds())
}

fn main_window_options(bounds: WindowBounds) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(bounds),
        titlebar: Some(TitleBar::title_bar_options()),
        focus: true,
        show: true,
        window_min_size: Some(size(px(MAIN_WINDOW_MIN_WIDTH), px(MAIN_WINDOW_MIN_HEIGHT))),
        ..Default::default()
    }
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
        include_str!("../../../themes/flexoki.json"),
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use gpui_kit::{Bounds, TestAppContext, WindowBounds, point, px, size};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::Database;
    use serde::Deserialize;

    use super::{
        create_main_window, main_window_options, open_initial_window,
        window_bounds_for_tray_restore,
    };

    #[derive(Deserialize)]
    struct ThemeSet {
        themes: Vec<ThemeConfig>,
    }

    #[derive(Deserialize)]
    struct ThemeConfig {
        name: String,
        highlight: HighlightConfig,
    }

    #[derive(Deserialize)]
    struct HighlightConfig {
        syntax: serde_json::Value,
    }

    #[test]
    fn tray_startup_does_not_create_a_main_window() {
        let mut creations = 0;
        let window = open_initial_window(true, || {
            creations += 1;
            Ok::<_, anyhow::Error>(())
        })
        .expect("startup should succeed");

        assert!(window.is_none());
        assert_eq!(creations, 0);
        let window = open_initial_window(false, || {
            creations += 1;
            Ok::<_, anyhow::Error>(())
        })
        .expect("normal startup should succeed");
        assert_eq!(window, Some(()));
        assert_eq!(creations, 1);
    }

    #[test]
    fn main_window_has_a_usable_minimum_size() {
        let bounds = WindowBounds::Windowed(Bounds::default());
        let options = main_window_options(bounds);

        assert_eq!(options.window_bounds, Some(bounds));
        assert_eq!(options.window_min_size, Some(size(px(800.), px(600.))));
        assert!(options.show);
        assert!(options.focus);
    }

    #[test]
    fn tray_restore_uses_saved_geometry_without_reapplying_window_mode() {
        let saved_bounds = Bounds {
            origin: point(px(92.), px(144.)),
            size: size(px(1024.), px(700.)),
        };
        for saved_state in [
            WindowBounds::Windowed(saved_bounds),
            WindowBounds::Maximized(saved_bounds),
            WindowBounds::Fullscreen(saved_bounds),
        ] {
            assert_eq!(
                window_bounds_for_tray_restore(saved_state),
                WindowBounds::Windowed(saved_bounds)
            );
        }
    }

    #[gpui_kit::test]
    fn tray_restore_releases_the_shell_and_keeps_saved_maximized_geometry_windowed(
        cx: &mut TestAppContext,
    ) {
        let tokio = tokio::runtime::Runtime::new().expect("Tokio runtime");
        let _runtime_guard = tokio.enter();
        cx.executor().allow_parking();
        let db = tokio.block_on(async {
            let db = Database::connect("sqlite::memory:")
                .await
                .expect("test database");
            Migrator::up(&db, None).await.expect("database migrations");
            db
        });
        let settings_dir = tempfile::tempdir().expect("settings directory");
        let (visibility_sender, visibility) = tokio::sync::watch::channel(true);
        let first = cx.update(|cx| {
            cx.set_global(gpui_kit::component::Theme::default());
            gpui_kit::init(cx);
            cx.set_global(settings::AppSettings::load(settings_dir.path()));
            cx.set_global(runtime::AppRuntime::new(db, PathBuf::new()));
            create_main_window(visibility.clone(), None, cx).expect("first window")
        });
        let original_shell = first.shell();
        let original_id = original_shell
            .upgrade()
            .expect("first shell should exist")
            .entity_id();
        cx.update(|cx| {
            first
                .handle()
                .update(cx, |_, window, _| window.remove_window())
                .expect("first window should close");
        });
        drop(first);
        cx.run_until_parked();
        assert!(
            original_shell.upgrade().is_none(),
            "closed shell must be released"
        );

        let saved_bounds = Bounds {
            origin: point(px(92.), px(144.)),
            size: size(px(1024.), px(700.)),
        };
        let second = cx.update(|cx| {
            create_main_window(visibility, Some(WindowBounds::Maximized(saved_bounds)), cx)
                .expect("restored window")
        });
        assert_ne!(
            second
                .shell()
                .upgrade()
                .expect("restored shell")
                .entity_id(),
            original_id
        );
        let restored_bounds = cx.update(|cx| {
            second
                .handle()
                .update(cx, |_, window, _| window.window_bounds())
                .expect("restored window should exist")
        });
        assert_eq!(restored_bounds, WindowBounds::Windowed(saved_bounds));
        assert!(*visibility_sender.borrow());
    }

    #[test]
    fn syntax_palettes_are_not_copied_across_theme_families() {
        let theme_files = [
            ("ayu", include_str!("../../../themes/ayu.json")),
            (
                "catppuccin",
                include_str!("../../../themes/catppuccin.json"),
            ),
            (
                "everforest",
                include_str!("../../../themes/everforest.json"),
            ),
            ("flexoki", include_str!("../../../themes/flexoki.json")),
            ("gruvbox", include_str!("../../../themes/gruvbox.json")),
            ("harper", include_str!("../../../themes/harper.json")),
            (
                "jellybeans",
                include_str!("../../../themes/jellybeans.json"),
            ),
            ("molokai", include_str!("../../../themes/molokai.json")),
            (
                "tokyonight",
                include_str!("../../../themes/tokyonight.json"),
            ),
            ("twilight", include_str!("../../../themes/twilight.json")),
            ("spaceduck", include_str!("../../../themes/spaceduck.json")),
            ("sick", include_str!("../../../themes/sick.json")),
        ];
        let mut palettes = HashMap::<String, (&str, String)>::new();

        for (family, contents) in theme_files {
            let theme_set: ThemeSet = serde_json::from_str(contents)
                .unwrap_or_else(|err| panic!("failed to parse {family} theme: {err}"));

            for theme in theme_set.themes {
                let palette = serde_json::to_string(&theme.highlight.syntax)
                    .unwrap_or_else(|err| panic!("failed to serialize {}: {err}", theme.name));

                if let Some((other_family, other_theme)) =
                    palettes.insert(palette, (family, theme.name.clone()))
                {
                    assert_eq!(
                        family, other_family,
                        "{} and {} unexpectedly share a syntax palette",
                        theme.name, other_theme
                    );
                }
            }
        }
    }
}
