use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use anyhow::Result;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    highlighter::Language,
    input::{CompletionProvider, Editor, EditorState, InputEvent, Rope, RopeExt as _, TabSize},
    v_flex,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Task, Window, div,
};
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    Range, TextEdit,
};
use serde_json::Value;

use crate::store::{AppSettings, StoredSettings};

const AUTO_SAVE_DELAY: Duration = Duration::from_millis(1_200);

gpui_kit::actions!(settings_document, [SaveSettingsDocument]);

#[derive(Clone, Debug)]
pub enum SettingsDocumentEvent {
    StateChanged,
    Applied,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsDocumentSaveState {
    Saved,
    Dirty,
    Saving,
    Error(SharedString),
}

pub struct SettingsDocumentView {
    path: PathBuf,
    focus_handle: FocusHandle,
    editor: Entity<EditorState>,
    save_state: SettingsDocumentSaveState,
    is_loading: bool,
    suppress_editor_events: bool,
    auto_save_epoch: u64,
    _load_task: Option<Task<()>>,
    auto_save_task: Option<Task<()>>,
}

impl EventEmitter<SettingsDocumentEvent> for SettingsDocumentView {}

impl SettingsDocumentView {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let path = AppSettings::settings_path(cx);
        let fallback = AppSettings::document_json(cx).unwrap_or_else(|_| "{\n  \n}".to_string());
        cx.new(|cx| Self::new(path, fallback, window, cx))
    }

    fn new(path: PathBuf, fallback: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let line_numbers = AppSettings::editor_line_numbers(cx);
        let soft_wrap = AppSettings::editor_soft_wrap(cx);
        let editor = cx.new(|cx| {
            let mut editor = EditorState::new(window, cx)
                .language(Language::Json)
                .line_number(line_numbers)
                .indent_guides(false)
                .tab_size(TabSize {
                    tab_size: 2,
                    ..Default::default()
                })
                .soft_wrap(soft_wrap)
                .searchable(true)
                .placeholder("Edit Castle settings...")
                .default_value("");
            editor.lsp_mut().completion_provider = Some(Rc::new(SettingsCompletionProvider));
            editor
        });
        let focus_handle = cx.focus_handle();
        let load_task = Self::load_async(path.clone(), fallback, window, cx);

        cx.subscribe_in(
            &editor,
            window,
            |this, editor, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) && !this.suppress_editor_events {
                    this.on_editor_change(editor.read(cx).value().to_string(), cx);
                }
            },
        )
        .detach();

        Self {
            path,
            focus_handle,
            editor,
            save_state: SettingsDocumentSaveState::Saved,
            is_loading: true,
            suppress_editor_events: false,
            auto_save_epoch: 0,
            _load_task: Some(load_task),
            auto_save_task: None,
        }
    }

    fn load_async(
        path: PathBuf,
        fallback: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { fs::read_to_string(&path) })
                .await;

            this.update_in(window, |this, window, cx| match result {
                Ok(content) => this.load_content(content, window, cx),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    this.load_content(fallback, window, cx)
                }
                Err(error) => this.fail_load(error.to_string(), cx),
            })
            .ok();
        })
    }

    fn load_content(&mut self, content: String, window: &mut Window, cx: &mut Context<Self>) {
        let (content, parse_error) = match serde_json::from_str::<StoredSettings>(&content) {
            Ok(_) => (document_content_without_tab_session(&content), None),
            Err(error) => (content, Some(format!("Invalid settings JSON: {error}"))),
        };
        self.suppress_editor_events = true;
        self.editor
            .update(cx, |editor, cx| editor.set_value(&content, window, cx));
        self.suppress_editor_events = false;
        self.is_loading = false;
        self.save_state = parse_error.map_or(SettingsDocumentSaveState::Saved, |error| {
            SettingsDocumentSaveState::Error(error.into())
        });
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
        self.emit_state_changed(cx);
    }

    fn fail_load(&mut self, message: String, cx: &mut Context<Self>) {
        self.is_loading = false;
        self.save_state = SettingsDocumentSaveState::Error(message.into());
        self.emit_state_changed(cx);
    }

    fn on_editor_change(&mut self, _content: String, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }

        self.save_state = SettingsDocumentSaveState::Dirty;
        self.emit_state_changed(cx);
        self.schedule_auto_save(cx);
    }

    fn schedule_auto_save(&mut self, cx: &mut Context<Self>) {
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        let epoch = self.auto_save_epoch;
        self.auto_save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTO_SAVE_DELAY).await;

            let content = this
                .update(cx, |this, _cx| {
                    (this.auto_save_epoch == epoch && !this.is_loading)
                        .then(|| this.editor.read(_cx).value().to_string())
                })
                .ok()
                .flatten();
            let Some(content) = content else {
                return;
            };

            this.update(cx, |this, cx| {
                if this.auto_save_epoch == epoch {
                    this.start_save(content, epoch, cx);
                }
            })
            .ok();
        }));
    }

    fn start_save(&mut self, content: String, epoch: u64, cx: &mut Context<Self>) {
        self.save_state = SettingsDocumentSaveState::Saving;
        self.emit_state_changed(cx);
        let bytes = content.into_bytes();

        cx.spawn(async move |this, cx| {
            let result = cx.update(|cx| AppSettings::apply_document_json(&bytes, cx));
            let result = result.map_err(|error| error.to_string());

            this.update(cx, |this, cx| {
                if this.auto_save_epoch != epoch {
                    return;
                }
                this.save_state = match result {
                    Ok(()) => SettingsDocumentSaveState::Saved,
                    Err(error) => SettingsDocumentSaveState::Error(error.into()),
                };
                if matches!(this.save_state, SettingsDocumentSaveState::Saved) {
                    cx.emit(SettingsDocumentEvent::Applied);
                }
                this.emit_state_changed(cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }

        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        let epoch = self.auto_save_epoch;
        let content = self.editor.read(cx).value().to_string();
        self.start_save(content, epoch, cx);
    }

    pub fn save_state(&self) -> SettingsDocumentSaveState {
        self.save_state.clone()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    #[doc(hidden)]
    pub fn loaded_content(&self, cx: &App) -> Option<String> {
        (!self.is_loading).then(|| self.editor.read(cx).value().to_string())
    }

    fn emit_state_changed(&self, cx: &mut Context<Self>) {
        cx.emit(SettingsDocumentEvent::StateChanged);
        cx.notify();
    }
}

fn document_content_without_tab_session(content: &str) -> String {
    let mut value = match serde_json::from_str::<Value>(content) {
        Ok(value) => value,
        Err(_) => return content.to_string(),
    };
    if let Some(object) = value.as_object_mut() {
        object.remove("tab_session");
    }
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| content.to_string())
}

impl Focusable for SettingsDocumentView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsDocumentView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = save_state_status(&self.save_state, cx);
        let directory = self
            .path
            .parent()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Castle")
            .to_string();

        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("settings.json")
            .to_string();

        v_flex()
            .id("settings-document")
            .key_context("SettingsDocument")
            .track_focus(&self.focus_handle)
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .on_action(cx.listener(|this, _: &SaveSettingsDocument, _, cx| this.save(cx)))
            .child(
                div()
                    .id("settings-document-editor")
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .bg(cx.theme().background)
                    .child(
                        Editor::new(&self.editor)
                            .h_full()
                            .w_full()
                            .p_0()
                            .border_0()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_size(cx.theme().mono_font_size),
                    ),
            )
            .child(
                h_flex()
                    .id("settings-document-status")
                    .debug_selector(|| "settings-document-status".to_owned())
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().secondary)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap_1()
                            .child(directory)
                            .child(Icon::new(IconName::ChevronRight).xsmall())
                            .child(div().truncate().child(file_name)),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .flex_shrink_0()
                                    .text_color(state.1)
                                    .child(Icon::new(state.0).xsmall())
                                    .child(state.2.clone()),
                            )
                            .child(
                                Button::new("settings-document-save")
                                    .icon(IconName::Check)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Save settings")
                                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                            ),
                    ),
            )
    }
}

fn save_state_status(
    state: &SettingsDocumentSaveState,
    cx: &mut Context<SettingsDocumentView>,
) -> (IconName, Hsla, SharedString) {
    match state {
        SettingsDocumentSaveState::Saved => (
            IconName::CircleCheck,
            cx.theme().success,
            "Settings saved".into(),
        ),
        SettingsDocumentSaveState::Dirty => (
            IconName::Asterisk,
            cx.theme().warning,
            "Unsaved settings changes".into(),
        ),
        SettingsDocumentSaveState::Saving => {
            (IconName::Loader, cx.theme().info, "Saving settings".into())
        }
        SettingsDocumentSaveState::Error(message) => {
            (IconName::TriangleAlert, cx.theme().danger, message.clone())
        }
    }
}

#[derive(Clone, Copy)]
enum SettingValueKind {
    Boolean,
    Number(&'static [&'static str]),
    Choice(&'static [&'static str]),
    Theme,
    Font,
    Text,
}

#[derive(Clone, Copy)]
struct SettingDefinition {
    name: &'static str,
    detail: &'static str,
    kind: SettingValueKind,
}

const NUM_FONT_SIZE: &[&str] = &["10", "12", "13", "14", "16", "18", "20", "22"];
const NUM_INTERFACE_FONT_SIZE: &[&str] = &["12", "14", "16", "18", "20"];
const NUM_RADIUS: &[&str] = &["0", "2", "4", "6", "8", "10", "12"];
const NUM_SIDEBAR_WIDTH: &[&str] = &["200", "260", "320", "400", "480"];
const SCROLLBAR_MODES: &[&str] = &["scrolling", "hover", "always"];
const EDITOR_MODES: &[&str] = &["source", "split", "preview"];

const SETTINGS_SCHEMA: &[SettingDefinition] = &[
    SettingDefinition {
        name: "theme_name",
        detail: "Color theme",
        kind: SettingValueKind::Theme,
    },
    SettingDefinition {
        name: "font_family",
        detail: "Interface font family",
        kind: SettingValueKind::Font,
    },
    SettingDefinition {
        name: "font_size",
        detail: "Interface font size",
        kind: SettingValueKind::Number(NUM_INTERFACE_FONT_SIZE),
    },
    SettingDefinition {
        name: "radius",
        detail: "Interface corner radius",
        kind: SettingValueKind::Number(NUM_RADIUS),
    },
    SettingDefinition {
        name: "show_sidebar",
        detail: "Show the sidebar",
        kind: SettingValueKind::Boolean,
    },
    SettingDefinition {
        name: "sidebar_width",
        detail: "Sidebar width",
        kind: SettingValueKind::Number(NUM_SIDEBAR_WIDTH),
    },
    SettingDefinition {
        name: "scrollbar_show",
        detail: "Scrollbar visibility",
        kind: SettingValueKind::Choice(SCROLLBAR_MODES),
    },
    SettingDefinition {
        name: "editor_font_family",
        detail: "Document editor font family",
        kind: SettingValueKind::Font,
    },
    SettingDefinition {
        name: "editor_font_size",
        detail: "Document editor font size",
        kind: SettingValueKind::Number(NUM_FONT_SIZE),
    },
    SettingDefinition {
        name: "markdown_preview_font_size",
        detail: "Markdown preview font size",
        kind: SettingValueKind::Number(NUM_FONT_SIZE),
    },
    SettingDefinition {
        name: "markdown_editor_mode",
        detail: "Default Markdown note view",
        kind: SettingValueKind::Choice(EDITOR_MODES),
    },
    SettingDefinition {
        name: "editor_status_line_visible",
        detail: "Show the editor status line",
        kind: SettingValueKind::Boolean,
    },
    SettingDefinition {
        name: "editor_line_numbers",
        detail: "Show editor line numbers",
        kind: SettingValueKind::Boolean,
    },
    SettingDefinition {
        name: "editor_soft_wrap",
        detail: "Wrap long editor lines",
        kind: SettingValueKind::Boolean,
    },
    SettingDefinition {
        name: "editor_vim_mode",
        detail: "Enable Vim editing mode",
        kind: SettingValueKind::Boolean,
    },
    SettingDefinition {
        name: "editor_focus_mode",
        detail: "Dim text outside the active paragraph",
        kind: SettingValueKind::Boolean,
    },
    SettingDefinition {
        name: "editor_typewriter_scrolling",
        detail: "Keep the active line centered",
        kind: SettingValueKind::Boolean,
    },
    SettingDefinition {
        name: "document_outline_visible",
        detail: "Show the document outline",
        kind: SettingValueKind::Boolean,
    },
    SettingDefinition {
        name: "close_to_tray",
        detail: "Close the app to the tray",
        kind: SettingValueKind::Boolean,
    },
    SettingDefinition {
        name: "tray_shortcut",
        detail: "Shortcut for showing Castle",
        kind: SettingValueKind::Text,
    },
    SettingDefinition {
        name: "quick_capture_shortcut",
        detail: "Shortcut for quick capture",
        kind: SettingValueKind::Text,
    },
];

#[derive(Default)]
pub struct SettingsCompletionProvider;

impl CompletionProvider for SettingsCompletionProvider {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _: CompletionContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<Result<CompletionResponse>> {
        let content = text.to_string();
        let catalog = CompletionCatalog::from_app(cx);
        let items = settings_completion_items(&content, offset, &catalog, text);
        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _: usize, _: &str, _: &mut App) -> bool {
        true
    }
}

struct CompletionCatalog {
    themes: Vec<String>,
    fonts: Vec<String>,
}

impl CompletionCatalog {
    fn from_app(cx: &App) -> Self {
        let themes = gpui_kit::component::ThemeRegistry::global(cx)
            .sorted_themes()
            .iter()
            .map(|theme| theme.name.to_string())
            .collect();
        let fonts = cx.text_system().all_font_names();
        Self { themes, fonts }
    }
}

fn settings_completion_items(
    text: &str,
    offset: usize,
    catalog: &CompletionCatalog,
    rope: &Rope,
) -> Vec<CompletionItem> {
    let offset = offset.min(text.len());
    if !text.is_char_boundary(offset) {
        return Vec::new();
    }
    let member_start = current_member_start(text, offset);
    let member = &text[member_start..offset];
    let trimmed_start = member.len() - member.trim_start().len();
    let trimmed = &member[trimmed_start..];

    if let Some(colon) = find_member_colon(trimmed) {
        let key = trimmed[..colon].trim().trim_matches('"').to_string();
        return value_completions(
            &key,
            &text[member_start + trimmed_start + colon + 1..offset],
            member_start + trimmed_start + colon + 1,
            catalog,
            rope,
        );
    }

    let (query_start, query, quoted) = if let Some(rest) = trimmed.strip_prefix('"') {
        if rest.contains('"') {
            return Vec::new();
        }
        (member_start + trimmed_start + 1, rest.to_string(), true)
    } else if trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        (member_start + trimmed_start, trimmed.to_string(), false)
    } else {
        return Vec::new();
    };

    let used_keys = root_keys(text);
    SETTINGS_SCHEMA
        .iter()
        .filter(|definition| !used_keys.contains(definition.name))
        .filter(|definition| matches_query(definition.name, &query))
        .map(|definition| CompletionItem {
            label: definition.name.to_string(),
            detail: Some(definition.detail.to_string()),
            kind: Some(CompletionItemKind::PROPERTY),
            filter_text: Some(query.clone()),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: Range::new(
                    rope.offset_to_position(query_start),
                    rope.offset_to_position(offset),
                ),
                new_text: if quoted {
                    format!("{}\": ", definition.name)
                } else {
                    format!("\"{}\": ", definition.name)
                },
            })),
            ..Default::default()
        })
        .collect()
}

fn value_completions(
    key: &str,
    value_prefix: &str,
    value_start: usize,
    catalog: &CompletionCatalog,
    rope: &Rope,
) -> Vec<CompletionItem> {
    let Some(definition) = SETTINGS_SCHEMA
        .iter()
        .find(|definition| definition.name == key)
    else {
        return Vec::new();
    };
    let leading = value_prefix.len() - value_prefix.trim_start().len();
    let value_prefix = &value_prefix[leading..];
    let quoted = value_prefix.starts_with('"');
    let query_start = value_start + leading + usize::from(quoted);
    let query = value_prefix
        .strip_prefix('"')
        .unwrap_or(value_prefix)
        .to_string();
    if query.contains('"') {
        return Vec::new();
    }

    let values = match definition.kind {
        SettingValueKind::Boolean => vec!["true".to_string(), "false".to_string()],
        SettingValueKind::Number(values) | SettingValueKind::Choice(values) => {
            values.iter().map(|value| (*value).to_string()).collect()
        }
        SettingValueKind::Theme => catalog.themes.clone(),
        SettingValueKind::Font => catalog.fonts.clone(),
        SettingValueKind::Text => Vec::new(),
    };

    values
        .into_iter()
        .filter(|value| matches_query(value, &query))
        .map(|value| CompletionItem {
            label: value.clone(),
            detail: Some(definition.detail.to_string()),
            kind: Some(match definition.kind {
                SettingValueKind::Boolean => CompletionItemKind::VALUE,
                SettingValueKind::Number(_) => CompletionItemKind::VALUE,
                SettingValueKind::Choice(_) | SettingValueKind::Theme | SettingValueKind::Font => {
                    CompletionItemKind::ENUM_MEMBER
                }
                SettingValueKind::Text => CompletionItemKind::TEXT,
            }),
            filter_text: Some(query.clone()),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: Range::new(
                    rope.offset_to_position(query_start),
                    rope.offset_to_position(value_start + value_prefix.len()),
                ),
                new_text: if quoted {
                    format!("{value}\"")
                } else if matches!(
                    definition.kind,
                    SettingValueKind::Choice(_) | SettingValueKind::Theme | SettingValueKind::Font
                ) {
                    format!("\"{value}\"")
                } else {
                    value
                },
            })),
            ..Default::default()
        })
        .collect()
}

fn current_member_start(text: &str, offset: usize) -> usize {
    let prefix = &text[..offset.min(text.len())];
    prefix
        .char_indices()
        .rev()
        .find(|(_, character)| matches!(character, '{' | ',' | '\n'))
        .map(|(index, character)| index + character.len_utf8())
        .unwrap_or(0)
}

fn find_member_colon(member: &str) -> Option<usize> {
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in member.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && in_string {
            escaped = true;
            continue;
        }
        if character == '"' {
            in_string = !in_string;
        } else if character == ':' && !in_string {
            return Some(index);
        }
    }
    None
}

fn root_keys(text: &str) -> HashSet<String> {
    let mut keys = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .map(|object| object.keys().cloned().collect::<HashSet<_>>())
        .unwrap_or_default();

    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else {
            continue;
        };
        if rest[end + 1..].trim_start().starts_with(':') {
            keys.insert(rest[..end].to_string());
        }
    }
    keys
}

fn matches_query(label: &str, query: &str) -> bool {
    query.is_empty()
        || label
            .to_ascii_lowercase()
            .starts_with(&query.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> CompletionCatalog {
        CompletionCatalog {
            themes: vec!["Sick".to_string(), "Ayu Dark".to_string()],
            fonts: vec!["IBM Plex Sans".to_string(), "IBM Plex Mono".to_string()],
        }
    }

    #[test]
    fn key_completion_replaces_only_the_typed_property_prefix() {
        let text = "{\n  \"theme\n}";
        let rope = Rope::from(text);
        let items = settings_completion_items(text, text.len() - 2, &catalog(), &rope);
        let theme = items
            .iter()
            .find(|item| item.label == "theme_name")
            .expect("theme setting should be suggested");

        let Some(CompletionTextEdit::Edit(edit)) = &theme.text_edit else {
            panic!("settings completion should use a text edit");
        };
        assert_eq!(edit.new_text, "theme_name\": ");
        assert_eq!(edit.range.start.character, 3);
    }

    #[test]
    fn value_completion_suggests_boolean_and_enum_values() {
        let boolean_text = "{\n  \"show_sidebar\": fa";
        let boolean_rope = Rope::from(boolean_text);
        let booleans =
            settings_completion_items(boolean_text, boolean_text.len(), &catalog(), &boolean_rope);
        assert!(booleans.iter().any(|item| item.label == "false"));

        let mode_text = "{\n  \"markdown_editor_mode\": \"sp";
        let mode_rope = Rope::from(mode_text);
        let modes = settings_completion_items(mode_text, mode_text.len(), &catalog(), &mode_rope);
        let split = modes
            .iter()
            .find(|item| item.label == "split")
            .expect("split mode should be suggested");
        let Some(CompletionTextEdit::Edit(edit)) = &split.text_edit else {
            panic!("settings completion should use a text edit");
        };
        assert_eq!(edit.new_text, "split\"");
    }

    #[test]
    fn used_keys_are_not_suggested_again() {
        let text = "{\n  \"theme_name\": \"Sick\",\n  \"\n}";
        let rope = Rope::from(text);
        let items = settings_completion_items(text, text.len() - 2, &catalog(), &rope);
        assert!(!items.iter().any(|item| item.label == "theme_name"));
        assert!(items.iter().any(|item| item.label == "font_family"));
    }

    #[test]
    fn document_content_hides_the_persisted_tab_session() {
        let content = serde_json::to_string(&serde_json::json!({
            "theme_name": "Sick",
            "tab_session": {
                "tabs": [],
                "active_tab_index": 0,
                "active_project_id": null
            }
        }))
        .expect("settings fixture should serialize");

        let document = document_content_without_tab_session(&content);
        assert!(document.contains("theme_name"));
        assert!(!document.contains("tab_session"));
    }

    #[gpui_kit::test]
    fn settings_document_status_bar_stays_on_one_row(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        let directory = tempfile::tempdir().expect("isolated settings directory");
        let mut view = None;
        let window = cx.update(|cx| {
            cx.set_global(AppSettings::load(directory.path()));
            cx.open_window(Default::default(), |window, cx| {
                let settings = SettingsDocumentView::view(window, cx);
                view = Some(settings.clone());
                cx.new(|cx| gpui_kit::component::Root::new(settings, window, cx))
            })
            .expect("settings test window should open")
        });
        let _view = view.expect("settings view should exist");
        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(gpui_kit::size(gpui_kit::px(1_200.), gpui_kit::px(800.)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });

        let status_bar = cx
            .debug_bounds("settings-document-status")
            .expect("settings status bar should render");
        assert!(
            status_bar.size.height <= gpui_kit::px(40.),
            "settings status bar should remain a single row: {status_bar:?}"
        );
    }
}
