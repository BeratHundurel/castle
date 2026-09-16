#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use anyhow::Result;
#[cfg(debug_assertions)]
use dotenvy::dotenv;
use gpui_kit::component::{Root, Theme, ThemeRegistry, TitleBar};
use gpui_kit::{
    App, AppContext, Bounds, Pixels, SharedString, WindowBounds, WindowOptions, px, size,
};
use std::{borrow::Cow, cell::RefCell, fs, rc::Rc, sync::Arc};
use storage::{Store, StoreOptions};

use app::{app_paths::AppPaths, keymap, system_notifications, tray};
use quick_capture::NoteCreatedHandler;
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
        if let Err(error) = app::startup::set_start_at_login(AppSettings::start_at_login(cx)) {
            eprintln!("Failed to synchronize the start-at-login setting: {error}");
        }
        cx.set_global(app_runtime);
        system_notifications::install_board_gateway(cx);

        let (main_window_visibility, main_window_visibility_receiver) =
            tokio::sync::watch::channel(!start_in_tray);
        let note_created_handler = Rc::new(RefCell::new(None));
        let note_created_handler_for_window = note_created_handler.clone();
        let bounds = Bounds::centered(None, size(px(1200.), px(768.)), cx);
        let window = cx
            .open_window(main_window_options(bounds, start_in_tray), |window, cx| {
                let note_created_handler_for_shell = note_created_handler_for_window.clone();
                let integration = ShellIntegration::new(
                    app::tray::update_shortcut,
                    app::tray::update_quick_capture_shortcut,
                    |enabled| {
                        app::startup::set_start_at_login(enabled).map_err(|error| error.to_string())
                    },
                    |cx| app::keymap::shortcuts(cx).to_vec(),
                    main_window_visibility_receiver,
                    Arc::new(app::mcp_registration::McpAgentAccess),
                );
                let view = AppShell::view(window, integration, cx);
                let shell_for_capture = view.downgrade();
                let note_created: NoteCreatedHandler = Rc::new(move |cx| {
                    if let Some(shell) = shell_for_capture.upgrade() {
                        shell.update(cx, |shell, cx| {
                            shell.refresh_after_quick_capture(cx);
                        });
                    }
                });
                note_created_handler_for_shell
                    .borrow_mut()
                    .replace(note_created);
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("Failed to open window");

        if let Some(note_created) = note_created_handler.borrow_mut().take() {
            if let Err(err) = tray::init(window.into(), note_created, main_window_visibility, cx) {
                eprintln!("Failed to initialize tray mode: {err}");
            }
        } else {
            eprintln!("Failed to initialize quick capture callback");
        }
    });

    Ok(())
}

fn main_window_options(bounds: Bounds<Pixels>, start_in_tray: bool) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitleBar::title_bar_options()),
        focus: !start_in_tray,
        show: !start_in_tray,
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
    use std::collections::HashMap;

    use gpui_kit::{Bounds, px, size};
    use serde::Deserialize;

    use super::main_window_options;

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
    fn startup_window_is_created_hidden_and_unfocused() {
        let options = main_window_options(Bounds::default(), true);

        assert!(!options.show);
        assert!(!options.focus);
    }

    #[test]
    fn main_window_has_a_usable_minimum_size() {
        let options = main_window_options(Bounds::default(), false);

        assert_eq!(options.window_min_size, Some(size(px(800.), px(600.))));
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
