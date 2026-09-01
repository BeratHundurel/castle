use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use gpui::{App, Global, SharedString, px};
use gpui_component::{Theme, ThemeRegistry, scroll::ScrollbarMode};
use serde::{Deserialize, Serialize};

use crate::persistence::{SettingsPersistence, SettingsWriteRequest};

const SETTINGS_FILE_NAME: &str = "settings.json";
const DEFAULT_THEME_NAME: &str = "Sick";
pub const DEFAULT_FONT_FAMILY: &str = "IBM Plex Sans";
const DEFAULT_FONT_SIZE: f64 = 16.0;
const DEFAULT_RADIUS: f64 = 6.0;
const DEFAULT_SHOW_SIDEBAR: bool = true;
const DEFAULT_SIDEBAR_WIDTH: f64 = 260.0;
const MIN_SIDEBAR_WIDTH: f64 = 200.0;
const MAX_SIDEBAR_WIDTH: f64 = 480.0;
const DEFAULT_SCROLLBAR_SHOW: &str = "scrolling";
pub const DEFAULT_EDITOR_FONT_FAMILY: &str = "IBM Plex Mono";
const DEFAULT_EDITOR_FONT_SIZE: f64 = 13.0;
const DEFAULT_MARKDOWN_PREVIEW_FONT_SIZE: f64 = 16.0;
const DEFAULT_MARKDOWN_EDITOR_MODE: &str = "source";
const DEFAULT_EDITOR_STATUS_LINE_VISIBLE: bool = true;
const DEFAULT_EDITOR_LINE_NUMBERS: bool = false;
const DEFAULT_EDITOR_SOFT_WRAP: bool = true;
const DEFAULT_EDITOR_VIM_MODE: bool = false;
const DEFAULT_DOCUMENT_OUTLINE_VISIBLE: bool = true;
const DEFAULT_CLOSE_TO_TRAY: bool = true;
pub const DEFAULT_TRAY_SHORTCUT: &str = "Ctrl+Alt+Space";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoredTab {
    Chooser,
    Trash,
    Board {
        board_id: u32,
        project_id: Option<u32>,
        title: String,
    },
    Note {
        note_id: u32,
        project_id: Option<u32>,
        title: String,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabSession {
    pub tabs: Vec<StoredTab>,
    pub active_tab_index: usize,
    pub active_project_id: Option<u32>,
}

#[derive(Clone)]
pub struct AppSettings {
    path: PathBuf,
    values: StoredSettings,
    persistence: SettingsPersistence,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StoredSettings {
    theme_name: String,
    font_family: String,
    font_size: f64,
    radius: f64,
    show_sidebar: bool,
    sidebar_width: f64,
    scrollbar_show: String,
    editor_font_family: String,
    #[serde(alias = "markdown_font_size")]
    editor_font_size: f64,
    markdown_preview_font_size: f64,
    markdown_editor_mode: String,
    #[serde(alias = "markdown_status_line_visible")]
    editor_status_line_visible: bool,
    #[serde(alias = "markdown_line_numbers")]
    editor_line_numbers: bool,
    #[serde(alias = "markdown_soft_wrap")]
    editor_soft_wrap: bool,
    editor_vim_mode: bool,
    #[serde(alias = "markdown_outline_visible")]
    document_outline_visible: bool,
    close_to_tray: bool,
    tray_shortcut: String,
    tab_session: TabSession,
}

impl Default for StoredSettings {
    fn default() -> Self {
        Self {
            theme_name: DEFAULT_THEME_NAME.to_string(),
            font_family: DEFAULT_FONT_FAMILY.to_string(),
            font_size: DEFAULT_FONT_SIZE,
            radius: DEFAULT_RADIUS,
            show_sidebar: DEFAULT_SHOW_SIDEBAR,
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            scrollbar_show: DEFAULT_SCROLLBAR_SHOW.to_string(),
            editor_font_family: DEFAULT_EDITOR_FONT_FAMILY.to_string(),
            editor_font_size: DEFAULT_EDITOR_FONT_SIZE,
            markdown_preview_font_size: DEFAULT_MARKDOWN_PREVIEW_FONT_SIZE,
            markdown_editor_mode: DEFAULT_MARKDOWN_EDITOR_MODE.to_string(),
            editor_status_line_visible: DEFAULT_EDITOR_STATUS_LINE_VISIBLE,
            editor_line_numbers: DEFAULT_EDITOR_LINE_NUMBERS,
            editor_soft_wrap: DEFAULT_EDITOR_SOFT_WRAP,
            editor_vim_mode: DEFAULT_EDITOR_VIM_MODE,
            document_outline_visible: DEFAULT_DOCUMENT_OUTLINE_VISIBLE,
            close_to_tray: DEFAULT_CLOSE_TO_TRAY,
            tray_shortcut: DEFAULT_TRAY_SHORTCUT.to_string(),
            tab_session: TabSession::default(),
        }
    }
}

impl Global for AppSettings {}

impl AppSettings {
    pub fn load(data_dir: impl AsRef<Path>) -> Self {
        let path = data_dir.as_ref().join(SETTINGS_FILE_NAME);
        let mut values = match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|err| {
                eprintln!("Failed to parse settings from {}: {err}", path.display());
                StoredSettings::default()
            }),
            Err(err) if err.kind() == ErrorKind::NotFound => StoredSettings::default(),
            Err(err) => {
                eprintln!("Failed to read settings from {}: {err}", path.display());
                StoredSettings::default()
            }
        };
        values.normalize();

        Self {
            path,
            values,
            persistence: SettingsPersistence::default(),
        }
    }

    pub fn apply_to_theme(&self, cx: &mut App) {
        apply_theme_name(&self.values.theme_name, cx);
        apply_font_family(&self.values.font_family, cx);
        apply_font_size(self.values.font_size, cx);
        apply_radius(self.values.radius, cx);
        apply_scrollbar_show(&self.values.scrollbar_show, cx);
        apply_editor_font_family(&self.values.editor_font_family, cx);
        apply_editor_font_size(self.values.editor_font_size, cx);
        Theme::sync_base(cx);
        cx.refresh_windows();
    }

    pub fn show_sidebar(cx: &App) -> bool {
        cx.global::<Self>().values.show_sidebar
    }

    pub fn set_show_sidebar(visible: bool, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.show_sidebar = visible;
        });
    }

    pub fn set_first_run_note(&mut self, note_id: u32, title: String) {
        self.values.tab_session = TabSession {
            tabs: vec![StoredTab::Note {
                note_id,
                project_id: None,
                title,
            }],
            active_tab_index: 0,
            active_project_id: None,
        };
        self.persist_sync();
    }

    pub fn sidebar_width(cx: &App) -> gpui::Pixels {
        px(cx.global::<Self>().values.sidebar_width as f32)
    }

    pub fn set_sidebar_width(width: gpui::Pixels, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.sidebar_width = width.as_f32() as f64;
        });
    }

    pub fn set_theme_name(theme_name: SharedString, cx: &mut App) {
        let (values, write) = {
            let settings = cx.global_mut::<Self>();
            settings.values.theme_name = theme_name.to_string();
            (settings.values.clone(), settings.prepare_write())
        };
        Self::schedule_write(write);

        apply_theme_name(&values.theme_name, cx);
        apply_font_family(&values.font_family, cx);
        apply_font_size(values.font_size, cx);
        apply_radius(values.radius, cx);
        apply_scrollbar_show(&values.scrollbar_show, cx);
        apply_editor_font_family(&values.editor_font_family, cx);
        apply_editor_font_size(values.editor_font_size, cx);
        Theme::sync_base(cx);
        cx.refresh_windows();
    }

    pub fn font_family(cx: &App) -> SharedString {
        cx.global::<Self>().values.font_family.as_str().into()
    }

    pub fn set_font_family(font_family: SharedString, cx: &mut App) {
        apply_font_family(font_family.as_ref(), cx);
        Self::update(cx, |settings| {
            settings.values.font_family = font_family.to_string();
        });
        cx.refresh_windows();
    }

    pub fn set_font_size(font_size: f64, cx: &mut App) {
        apply_font_size(font_size, cx);
        Self::update(cx, |settings| {
            settings.values.font_size = font_size;
        });
        cx.refresh_windows();
    }

    pub fn set_radius(radius: f64, cx: &mut App) {
        apply_radius(radius, cx);
        Self::update(cx, |settings| {
            settings.values.radius = radius;
        });
        cx.refresh_windows();
    }

    pub fn set_scrollbar_show(value: SharedString, cx: &mut App) {
        apply_scrollbar_show(value.as_ref(), cx);
        Self::update(cx, |settings| {
            settings.values.scrollbar_show = value.to_string();
        });
        cx.refresh_windows();
    }

    pub fn editor_font_family(cx: &App) -> SharedString {
        cx.global::<Self>()
            .values
            .editor_font_family
            .as_str()
            .into()
    }

    pub fn set_editor_font_family(font_family: SharedString, cx: &mut App) {
        apply_editor_font_family(font_family.as_ref(), cx);
        Self::update(cx, |settings| {
            settings.values.editor_font_family = font_family.to_string();
        });
        cx.refresh_windows();
    }

    pub fn set_editor_font_size(font_size: f64, cx: &mut App) {
        apply_editor_font_size(font_size, cx);
        Self::update(cx, |settings| {
            settings.values.editor_font_size = font_size;
        });
        cx.refresh_windows();
    }

    pub fn editor_font_size(cx: &App) -> f64 {
        cx.global::<Self>().values.editor_font_size
    }

    pub fn set_markdown_preview_font_size(font_size: f64, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.markdown_preview_font_size = font_size;
        });
        cx.refresh_windows();
    }

    pub fn markdown_preview_font_size(cx: &App) -> f64 {
        cx.global::<Self>().values.markdown_preview_font_size
    }

    pub fn markdown_editor_mode(cx: &App) -> SharedString {
        cx.global::<Self>()
            .values
            .markdown_editor_mode
            .as_str()
            .into()
    }

    pub fn editor_line_numbers(cx: &App) -> bool {
        cx.global::<Self>().values.editor_line_numbers
    }

    pub fn editor_status_line_visible(cx: &App) -> bool {
        cx.global::<Self>().values.editor_status_line_visible
    }

    pub fn editor_soft_wrap(cx: &App) -> bool {
        cx.global::<Self>().values.editor_soft_wrap
    }

    pub fn editor_vim_mode(cx: &App) -> bool {
        cx.global::<Self>().values.editor_vim_mode
    }

    pub fn set_markdown_editor_mode(value: SharedString, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.markdown_editor_mode = value.to_string();
        });
    }

    pub fn set_editor_status_line_visible(visible: bool, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.editor_status_line_visible = visible;
        });
        cx.refresh_windows();
    }

    pub fn set_editor_line_numbers(enabled: bool, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.editor_line_numbers = enabled;
        });
    }

    pub fn set_editor_soft_wrap(enabled: bool, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.editor_soft_wrap = enabled;
        });
    }

    pub fn set_editor_vim_mode(enabled: bool, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.editor_vim_mode = enabled;
        });
        cx.refresh_windows();
    }

    pub fn document_outline_visible(cx: &App) -> bool {
        cx.global::<Self>().values.document_outline_visible
    }

    pub fn set_document_outline_visible(enabled: bool, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.document_outline_visible = enabled;
        });
    }

    pub fn close_to_tray(cx: &App) -> bool {
        cx.global::<Self>().values.close_to_tray
    }

    pub fn set_close_to_tray(enabled: bool, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.close_to_tray = enabled;
        });
    }

    pub fn tray_shortcut(cx: &App) -> SharedString {
        cx.global::<Self>().values.tray_shortcut.as_str().into()
    }

    pub fn set_tray_shortcut(shortcut: SharedString, cx: &mut App) {
        let shortcut = shortcut.to_string();
        Self::update(cx, |settings| {
            settings.values.tray_shortcut = shortcut.clone();
        });
    }

    pub fn tab_session(cx: &App) -> TabSession {
        cx.global::<Self>().values.tab_session.clone()
    }

    pub fn set_tab_session(tab_session: TabSession, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.tab_session = tab_session;
        });
    }

    fn update(cx: &mut App, update: impl FnOnce(&mut Self)) {
        let write = {
            let settings = cx.global_mut::<Self>();
            update(settings);
            settings.prepare_write()
        };
        Self::schedule_write(write);
    }

    fn prepare_write(&self) -> SettingsWriteRequest {
        self.persistence.prepare(&self.path, &self.values)
    }

    fn schedule_write(write: SettingsWriteRequest) {
        SettingsPersistence::schedule(write);
    }

    pub fn flush(cx: &mut App) -> impl Future<Output = ()> + use<> {
        let write = cx.global::<Self>().prepare_write();
        async move {
            SettingsPersistence::write(write).await;
        }
    }

    fn persist_sync(&self) {
        SettingsPersistence::write_sync(&self.path, &self.values);
    }
}

impl StoredSettings {
    fn normalize(&mut self) {
        self.font_family = normalize_font_family(&self.font_family, DEFAULT_FONT_FAMILY);
        self.font_size = self.font_size.clamp(12.0, 20.0);
        self.radius = self.radius.clamp(0.0, 12.0);
        self.sidebar_width = self
            .sidebar_width
            .clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
        self.editor_font_family =
            normalize_font_family(&self.editor_font_family, DEFAULT_EDITOR_FONT_FAMILY);
        self.editor_font_size = self.editor_font_size.clamp(10.0, 22.0);
        self.markdown_preview_font_size = self.markdown_preview_font_size.clamp(10.0, 22.0);

        if !matches!(
            self.scrollbar_show.as_str(),
            "scrolling" | "hover" | "always"
        ) {
            self.scrollbar_show = DEFAULT_SCROLLBAR_SHOW.to_string();
        }

        if !matches!(self.markdown_editor_mode.as_str(), "source" | "preview") {
            self.markdown_editor_mode = DEFAULT_MARKDOWN_EDITOR_MODE.to_string();
        }

        if self
            .tray_shortcut
            .parse::<global_hotkey::hotkey::HotKey>()
            .is_err()
        {
            self.tray_shortcut = DEFAULT_TRAY_SHORTCUT.to_string();
        }

        if self.tab_session.tabs.is_empty() {
            self.tab_session.active_tab_index = 0;
        } else {
            self.tab_session.active_tab_index = self
                .tab_session
                .active_tab_index
                .min(self.tab_session.tabs.len() - 1);
        }
    }
}

fn normalize_font_family(font_family: &str, default: &str) -> String {
    let font_family = font_family.trim();
    match font_family {
        "" => default.to_string(),
        "IBM Flex Mono" if default == DEFAULT_FONT_FAMILY => DEFAULT_FONT_FAMILY.to_string(),
        "IBM Plex Mono" if default == DEFAULT_FONT_FAMILY => DEFAULT_FONT_FAMILY.to_string(),
        "IBM Flex Mono" => DEFAULT_EDITOR_FONT_FAMILY.to_string(),
        _ => font_family.to_string(),
    }
}

fn apply_theme_name(theme_name: &str, cx: &mut App) {
    let theme_name = SharedString::from(theme_name);
    if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
        Theme::global_mut(cx).apply_config(&theme_config);
    }
}

fn apply_font_family(font_family: &str, cx: &mut App) {
    Theme::global_mut(cx).font_family = SharedString::from(font_family);
}

fn apply_font_size(font_size: f64, cx: &mut App) {
    Theme::global_mut(cx).font_size = px(font_size as f32);
}

fn apply_radius(radius: f64, cx: &mut App) {
    let radius = px(radius as f32);
    let theme = Theme::global_mut(cx);
    theme.radius = radius;
    theme.radius_lg = if radius > px(0.) {
        radius + px(2.)
    } else {
        px(0.)
    };
}

fn apply_scrollbar_show(value: &str, cx: &mut App) {
    Theme::global_mut(cx).scrollbar_mode = scrollbar_show_from_key(value);
}

fn apply_editor_font_family(font_family: &str, cx: &mut App) {
    Theme::global_mut(cx).mono_font_family = SharedString::from(font_family);
}

fn apply_editor_font_size(font_size: f64, cx: &mut App) {
    Theme::global_mut(cx).mono_font_size = px(font_size as f32);
}

pub fn scrollbar_show_key(show: ScrollbarMode) -> SharedString {
    match show {
        ScrollbarMode::Scrolling => "scrolling".into(),
        ScrollbarMode::Hover => "hover".into(),
        ScrollbarMode::Always => "always".into(),
    }
}

fn scrollbar_show_from_key(value: &str) -> ScrollbarMode {
    match value {
        "hover" => ScrollbarMode::Hover,
        "always" => ScrollbarMode::Always,
        _ => ScrollbarMode::Scrolling,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn applying_settings_updates_resizable_handle_colors(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            ThemeRegistry::global_mut(cx)
                .load_themes_from_str(include_str!("../../../themes/gruvbox.json"))
                .expect("Gruvbox theme should load");

            let directory = tempfile::tempdir().expect("settings directory should be created");
            let mut settings = AppSettings::load(directory.path());
            settings.values.theme_name = "Gruvbox Dark".to_string();

            settings.apply_to_theme(cx);

            assert_eq!(
                gpui_base::Theme::global(cx).resizable.handle.expect("handle should be set"),
                Theme::global(cx).border
            );
            assert_eq!(
                gpui_base::Theme::global(cx).resizable.active_handle.expect("active handle should be set"),
                Theme::global(cx).drag_border
            );
        });
    }

    #[test]
    fn settings_without_a_tab_session_remain_compatible() {
        let settings: StoredSettings = serde_json::from_str(r#"{"theme_name":"Sick"}"#)
            .expect("legacy settings should deserialize");

        assert_eq!(settings.tab_session, TabSession::default());
        assert_eq!(
            settings.markdown_preview_font_size,
            DEFAULT_MARKDOWN_PREVIEW_FONT_SIZE
        );
        assert_eq!(settings.sidebar_width, DEFAULT_SIDEBAR_WIDTH);
        assert!(settings.editor_status_line_visible);
        assert!(!settings.editor_vim_mode);
        assert!(settings.close_to_tray);
        assert_eq!(settings.tray_shortcut, DEFAULT_TRAY_SHORTCUT);
    }

    #[test]
    fn removed_format_on_auto_save_setting_is_ignored() {
        let settings: StoredSettings = serde_json::from_str(r#"{"format_on_auto_save":true}"#)
            .expect("legacy settings should still deserialize");
        let serialized = serde_json::to_value(settings).expect("settings should serialize");

        assert!(serialized.get("format_on_auto_save").is_none());
    }

    #[test]
    fn legacy_markdown_editor_settings_deserialize_into_document_settings() {
        let settings: StoredSettings = serde_json::from_str(
            r#"{
                "markdown_font_size": 17.0,
                "markdown_status_line_visible": false,
                "markdown_line_numbers": true,
                "markdown_soft_wrap": false,
                "markdown_outline_visible": false
            }"#,
        )
        .expect("legacy editor settings should deserialize");

        assert_eq!(settings.editor_font_size, 17.0);
        assert!(!settings.editor_status_line_visible);
        assert!(settings.editor_line_numbers);
        assert!(!settings.editor_soft_wrap);
        assert!(!settings.document_outline_visible);

        let serialized = serde_json::to_string(&settings).expect("settings should serialize");
        assert!(serialized.contains("\"editor_font_size\""));
        assert!(!serialized.contains("\"markdown_font_size\""));
    }

    #[test]
    fn tab_session_round_trips_and_normalizes_the_active_index() {
        let mut settings = StoredSettings {
            tab_session: TabSession {
                tabs: vec![
                    StoredTab::Board {
                        board_id: 12,
                        project_id: Some(3),
                        title: "Roadmap".to_string(),
                    },
                    StoredTab::Note {
                        note_id: 27,
                        project_id: None,
                        title: "Scratchpad".to_string(),
                    },
                ],
                active_tab_index: 9,
                active_project_id: Some(3),
            },
            ..StoredSettings::default()
        };

        settings.normalize();
        let serialized = serde_json::to_string(&settings).expect("settings should serialize");
        let restored: StoredSettings =
            serde_json::from_str(&serialized).expect("settings should deserialize");

        assert_eq!(restored.tab_session.active_tab_index, 1);
        assert_eq!(restored.tab_session, settings.tab_session);
    }

    #[test]
    fn first_run_note_replaces_stale_tabs_and_persists() {
        let directory = tempfile::tempdir().expect("settings directory should be created");
        let mut settings = AppSettings::load(directory.path());
        settings.values.tab_session = TabSession {
            tabs: vec![StoredTab::Board {
                board_id: 99,
                project_id: Some(7),
                title: "Old board".to_string(),
            }],
            active_tab_index: 0,
            active_project_id: Some(7),
        };

        settings.set_first_run_note(42, "docs.md".to_string());

        let restored = AppSettings::load(directory.path());
        assert_eq!(
            restored.values.tab_session,
            TabSession {
                tabs: vec![StoredTab::Note {
                    note_id: 42,
                    project_id: None,
                    title: "docs.md".to_string(),
                }],
                active_tab_index: 0,
                active_project_id: None,
            }
        );
    }

    #[gpui::test]
    fn settings_writes_leave_foreground_free_and_latest_snapshot_wins(
        cx: &mut gpui::TestAppContext,
    ) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let directory = tempfile::tempdir().expect("settings directory should be created");
        let settings = AppSettings::load(directory.path());
        let settings_path = directory.path().join(SETTINGS_FILE_NAME);
        let write_gate = settings.persistence.write_gate.clone();
        let held_write = runtime.block_on(write_gate.lock_owned());

        cx.update(|cx| cx.set_global(settings));
        for active_tab_index in 0..40 {
            cx.update(|cx| {
                AppSettings::set_tab_session(
                    TabSession {
                        tabs: vec![StoredTab::Note {
                            note_id: 42,
                            project_id: None,
                            title: format!("Note {active_tab_index}"),
                        }],
                        active_tab_index,
                        active_project_id: None,
                    },
                    cx,
                );
            });
        }

        assert!(
            !settings_path.exists(),
            "a blocked settings writer must not block or write on GPUI's foreground executor"
        );
        drop(held_write);
        let flush = cx.update(AppSettings::flush);
        runtime.block_on(flush);

        let restored = AppSettings::load(directory.path());
        assert_eq!(
            restored.values.tab_session.tabs,
            vec![StoredTab::Note {
                note_id: 42,
                project_id: None,
                title: "Note 39".to_string(),
            }]
        );
    }

    #[test]
    fn invalid_tray_shortcut_is_reset_to_default() {
        let mut settings = StoredSettings {
            tray_shortcut: "Ctrl+not-a-key".to_string(),
            ..StoredSettings::default()
        };

        settings.normalize();

        assert_eq!(settings.tray_shortcut, DEFAULT_TRAY_SHORTCUT);
    }

    #[test]
    fn removed_split_editor_mode_is_reset_to_source() {
        let mut settings = StoredSettings {
            markdown_editor_mode: "split".to_string(),
            ..StoredSettings::default()
        };

        settings.normalize();

        assert_eq!(settings.markdown_editor_mode, DEFAULT_MARKDOWN_EDITOR_MODE);
    }

    #[test]
    fn sidebar_width_is_normalized_to_resizable_bounds() {
        let mut settings = StoredSettings {
            sidebar_width: 900.0,
            ..StoredSettings::default()
        };

        settings.normalize();
        assert_eq!(settings.sidebar_width, MAX_SIDEBAR_WIDTH);

        settings.sidebar_width = 100.0;
        settings.normalize();
        assert_eq!(settings.sidebar_width, MIN_SIDEBAR_WIDTH);
    }
}
