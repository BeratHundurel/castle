use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use gpui::{App, Global, SharedString, px};
use gpui_component::{Theme, ThemeRegistry, scroll::ScrollbarShow};
use serde::{Deserialize, Serialize};

const SETTINGS_FILE_NAME: &str = "settings.json";
const DEFAULT_THEME_NAME: &str = "Sick";
pub(crate) const DEFAULT_FONT_FAMILY: &str = "IBM Plex Sans";
const DEFAULT_FONT_SIZE: f64 = 16.0;
const DEFAULT_RADIUS: f64 = 6.0;
const DEFAULT_SHOW_SIDEBAR: bool = true;
const DEFAULT_SCROLLBAR_SHOW: &str = "scrolling";
pub(crate) const DEFAULT_EDITOR_FONT_FAMILY: &str = "IBM Plex Mono";
const DEFAULT_MARKDOWN_FONT_SIZE: f64 = 13.0;
const DEFAULT_MARKDOWN_EDITOR_MODE: &str = "source";
const DEFAULT_MARKDOWN_LINE_NUMBERS: bool = false;
const DEFAULT_MARKDOWN_SOFT_WRAP: bool = true;

#[derive(Clone)]
pub struct AppSettings {
    path: PathBuf,
    values: StoredSettings,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct StoredSettings {
    theme_name: String,
    font_family: String,
    font_size: f64,
    radius: f64,
    show_sidebar: bool,
    scrollbar_show: String,
    editor_font_family: String,
    markdown_font_size: f64,
    markdown_editor_mode: String,
    markdown_line_numbers: bool,
    markdown_soft_wrap: bool,
}

impl Default for StoredSettings {
    fn default() -> Self {
        Self {
            theme_name: DEFAULT_THEME_NAME.to_string(),
            font_family: DEFAULT_FONT_FAMILY.to_string(),
            font_size: DEFAULT_FONT_SIZE,
            radius: DEFAULT_RADIUS,
            show_sidebar: DEFAULT_SHOW_SIDEBAR,
            scrollbar_show: DEFAULT_SCROLLBAR_SHOW.to_string(),
            editor_font_family: DEFAULT_EDITOR_FONT_FAMILY.to_string(),
            markdown_font_size: DEFAULT_MARKDOWN_FONT_SIZE,
            markdown_editor_mode: DEFAULT_MARKDOWN_EDITOR_MODE.to_string(),
            markdown_line_numbers: DEFAULT_MARKDOWN_LINE_NUMBERS,
            markdown_soft_wrap: DEFAULT_MARKDOWN_SOFT_WRAP,
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

        Self { path, values }
    }

    pub fn apply_to_theme(&self, cx: &mut App) {
        apply_theme_name(&self.values.theme_name, cx);
        apply_font_family(&self.values.font_family, cx);
        apply_font_size(self.values.font_size, cx);
        apply_radius(self.values.radius, cx);
        apply_scrollbar_show(&self.values.scrollbar_show, cx);
        apply_editor_font_family(&self.values.editor_font_family, cx);
        apply_markdown_font_size(self.values.markdown_font_size, cx);
        cx.refresh_windows();
    }

    pub(crate) fn show_sidebar(cx: &App) -> bool {
        cx.global::<Self>().values.show_sidebar
    }

    pub(crate) fn set_show_sidebar(visible: bool, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.show_sidebar = visible;
        });
    }

    pub(crate) fn set_theme_name(theme_name: SharedString, cx: &mut App) {
        let values = {
            let settings = cx.global_mut::<Self>();
            settings.values.theme_name = theme_name.to_string();
            settings.persist();
            settings.values.clone()
        };

        apply_theme_name(&values.theme_name, cx);
        apply_font_family(&values.font_family, cx);
        apply_font_size(values.font_size, cx);
        apply_radius(values.radius, cx);
        apply_scrollbar_show(&values.scrollbar_show, cx);
        apply_editor_font_family(&values.editor_font_family, cx);
        apply_markdown_font_size(values.markdown_font_size, cx);
        cx.refresh_windows();
    }

    pub(crate) fn font_family(cx: &App) -> SharedString {
        cx.global::<Self>().values.font_family.as_str().into()
    }

    pub(crate) fn set_font_family(font_family: SharedString, cx: &mut App) {
        apply_font_family(font_family.as_ref(), cx);
        Self::update(cx, |settings| {
            settings.values.font_family = font_family.to_string();
        });
        cx.refresh_windows();
    }

    pub(crate) fn set_font_size(font_size: f64, cx: &mut App) {
        apply_font_size(font_size, cx);
        Self::update(cx, |settings| {
            settings.values.font_size = font_size;
        });
        cx.refresh_windows();
    }

    pub(crate) fn set_radius(radius: f64, cx: &mut App) {
        apply_radius(radius, cx);
        Self::update(cx, |settings| {
            settings.values.radius = radius;
        });
        cx.refresh_windows();
    }

    pub(crate) fn set_scrollbar_show(value: SharedString, cx: &mut App) {
        apply_scrollbar_show(value.as_ref(), cx);
        Self::update(cx, |settings| {
            settings.values.scrollbar_show = value.to_string();
        });
        cx.refresh_windows();
    }

    pub(crate) fn editor_font_family(cx: &App) -> SharedString {
        cx.global::<Self>()
            .values
            .editor_font_family
            .as_str()
            .into()
    }

    pub(crate) fn set_editor_font_family(font_family: SharedString, cx: &mut App) {
        apply_editor_font_family(font_family.as_ref(), cx);
        Self::update(cx, |settings| {
            settings.values.editor_font_family = font_family.to_string();
        });
        cx.refresh_windows();
    }

    pub(crate) fn set_markdown_font_size(font_size: f64, cx: &mut App) {
        apply_markdown_font_size(font_size, cx);
        Self::update(cx, |settings| {
            settings.values.markdown_font_size = font_size;
        });
        cx.refresh_windows();
    }

    pub(crate) fn markdown_font_size(cx: &App) -> f64 {
        cx.global::<Self>().values.markdown_font_size
    }

    pub(crate) fn markdown_editor_mode(cx: &App) -> SharedString {
        cx.global::<Self>()
            .values
            .markdown_editor_mode
            .as_str()
            .into()
    }

    pub(crate) fn markdown_line_numbers(cx: &App) -> bool {
        cx.global::<Self>().values.markdown_line_numbers
    }

    pub(crate) fn markdown_soft_wrap(cx: &App) -> bool {
        cx.global::<Self>().values.markdown_soft_wrap
    }

    pub(crate) fn set_markdown_editor_mode(value: SharedString, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.markdown_editor_mode = value.to_string();
        });
    }

    pub(crate) fn set_markdown_line_numbers(enabled: bool, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.markdown_line_numbers = enabled;
        });
    }

    pub(crate) fn set_markdown_soft_wrap(enabled: bool, cx: &mut App) {
        Self::update(cx, |settings| {
            settings.values.markdown_soft_wrap = enabled;
        });
    }

    fn update(cx: &mut App, update: impl FnOnce(&mut Self)) {
        let settings = cx.global_mut::<Self>();
        update(settings);
        settings.persist();
    }

    fn persist(&self) {
        if let Some(parent) = self.path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            eprintln!(
                "Failed to create settings directory {}: {err}",
                parent.display()
            );
            return;
        }

        match serde_json::to_string_pretty(&self.values) {
            Ok(contents) => {
                if let Err(err) = fs::write(&self.path, contents) {
                    eprintln!("Failed to write settings to {}: {err}", self.path.display());
                }
            }
            Err(err) => {
                eprintln!("Failed to serialize settings: {err}");
            }
        }
    }
}

impl StoredSettings {
    fn normalize(&mut self) {
        self.font_family = normalize_font_family(&self.font_family, DEFAULT_FONT_FAMILY);
        self.font_size = self.font_size.clamp(12.0, 20.0);
        self.radius = self.radius.clamp(0.0, 12.0);
        self.editor_font_family =
            normalize_font_family(&self.editor_font_family, DEFAULT_EDITOR_FONT_FAMILY);
        self.markdown_font_size = self.markdown_font_size.clamp(10.0, 22.0);

        if !matches!(
            self.scrollbar_show.as_str(),
            "scrolling" | "hover" | "always"
        ) {
            self.scrollbar_show = DEFAULT_SCROLLBAR_SHOW.to_string();
        }

        if !matches!(
            self.markdown_editor_mode.as_str(),
            "source" | "split" | "preview"
        ) {
            self.markdown_editor_mode = DEFAULT_MARKDOWN_EDITOR_MODE.to_string();
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
    Theme::global_mut(cx).scrollbar_show = scrollbar_show_from_key(value);
}

fn apply_editor_font_family(font_family: &str, cx: &mut App) {
    Theme::global_mut(cx).mono_font_family = SharedString::from(font_family);
}

fn apply_markdown_font_size(font_size: f64, cx: &mut App) {
    Theme::global_mut(cx).mono_font_size = px(font_size as f32);
}

pub(crate) fn scrollbar_show_key(show: ScrollbarShow) -> SharedString {
    match show {
        ScrollbarShow::Scrolling => "scrolling".into(),
        ScrollbarShow::Hover => "hover".into(),
        ScrollbarShow::Always => "always".into(),
    }
}

fn scrollbar_show_from_key(value: &str) -> ScrollbarShow {
    match value {
        "hover" => ScrollbarShow::Hover,
        "always" => ScrollbarShow::Always,
        _ => ScrollbarShow::Scrolling,
    }
}
