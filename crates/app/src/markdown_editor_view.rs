use anyhow::Result;
use entity::{note, note::Entity as Note};
use gpui::{
    Action, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, SharedString, Styled, Subscription, Task,
    Window, actions, div, px,
};
use gpui_component::{
    ActiveTheme as _, IconName, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    highlighter::Language,
    input::{Input, InputEvent, InputState, RopeExt, TabSize},
    resizable::{h_resizable, resizable_panel},
    text::{TextView, TextViewState},
    v_flex,
};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, EntityTrait};
use serde::Deserialize;
use std::{
    fs::{create_dir_all, read_to_string, write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::DB;

actions!(
    markdown_editor,
    [SaveMarkdownFile, SaveMarkdownFileAs, ToggleEditorMode,]
);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = markdown_editor, no_json)]
struct ApplyMarkdownFormat(MarkdownFormat);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = markdown_editor, no_json)]
struct ExpandEmmet;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = markdown_editor, no_json)]
struct EmmetSubmitWrap;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = markdown_editor, no_json)]
struct EmmetCancelWrap;

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
enum MarkdownFormat {
    HeadingOne,
    HeadingTwo,
    HeadingThree,
    Bold,
    Italic,
    InlineCode,
    Link,
    BulletList,
    OrderedList,
    Quote,
    CodeBlock,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorMode {
    Split,
    Source,
    Preview,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum SaveState {
    Saved,
    Dirty,
    Saving,
    Missing,
    Error(SharedString),
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct DocumentStats {
    lines: usize,
    words: usize,
    characters: usize,
}

pub(crate) struct MarkdownEditorView {
    note_id: u32,
    title: SharedString,
    focus_handle: FocusHandle,
    editor: Entity<InputState>,
    preview: Entity<TextViewState>,
    mode: EditorMode,
    current_path: Option<PathBuf>,
    last_file_saved: SharedString,
    save_state: SaveState,
    stats: DocumentStats,
    auto_save_epoch: u64,
    _auto_save_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    emmet_input: Entity<InputState>,
    show_emmet_input: bool,
    emmet_replacement_range: Option<std::ops::Range<usize>>,
}

pub(crate) const DEFAULT_NOTE: &str = r#"# Untitled note

Start writing Markdown here.
"#;

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-p", ExpandEmmet, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-p", ExpandEmmet, Some("MarkdownEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-s", SaveMarkdownFile, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-s", SaveMarkdownFile, Some("MarkdownEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-s", SaveMarkdownFileAs, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-s", SaveMarkdownFileAs, Some("MarkdownEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-e", ToggleEditorMode, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-e", ToggleEditorMode, Some("MarkdownEditor")),
        KeyBinding::new("enter", EmmetSubmitWrap, Some("EmmetInput")),
        KeyBinding::new("escape", EmmetCancelWrap, Some("EmmetInput")),
    ]);
}

impl MarkdownEditorView {
    pub(crate) fn view(note_id: u32, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(note_id, window, cx))
    }

    fn new(note_id: u32, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor(Language::Markdown)
                .line_number(true)
                .tab_size(TabSize {
                    tab_size: 4,
                    ..Default::default()
                })
                .soft_wrap(true)
                .searchable(true)
                .placeholder("Write Markdown...")
                .default_value(DEFAULT_NOTE)
        });

        let preview = cx.new(|cx| {
            TextViewState::markdown(DEFAULT_NOTE, cx)
                .scrollable(true)
                .selectable(true)
        });

        let emmet_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Enter Emmet abbreviation (e.g. details>summary)")
        });

        let focus_handle = cx.focus_handle();
        let editor_focus = editor.focus_handle(cx);
        window.defer(cx, move |window, cx| {
            editor_focus.focus(window, cx);
        });

        let _subscriptions = vec![
            cx.subscribe(&editor, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value();
                    this.update_from_editor(value, cx);
                }
            }),
        ];

        Self::load_note_async(note_id, window, cx);

        Self {
            note_id,
            title: "Untitled note".into(),
            focus_handle,
            editor,
            preview,
            mode: EditorMode::Split,
            current_path: None,
            last_file_saved: DEFAULT_NOTE.into(),
            save_state: SaveState::Saved,
            stats: DocumentStats::from_text(DEFAULT_NOTE),
            auto_save_epoch: 0,
            _auto_save_task: None,
            _subscriptions,
            emmet_input,
            show_emmet_input: false,
            emmet_replacement_range: None,
        }
    }

    pub(crate) fn save_state(&self) -> SaveState {
        self.save_state.clone()
    }

    pub(crate) fn set_title(&mut self, title: String, cx: &mut Context<Self>) {
        let title = title.trim();
        if title.is_empty() || self.title.as_ref() == title {
            return;
        }

        self.title = SharedString::from(title);
        let db = cx.global::<DB>().conn.clone();
        let note_id = self.note_id;
        let title = title.to_string();
        let now = now_ts();

        cx.spawn(async move |_, _| -> Result<()> {
            note::ActiveModel {
                id: Set(note_id as i64),
                title: Set(title),
                updated_at: Set(now),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            Ok(())
        })
        .detach();

        cx.notify();
    }

    fn load_note_async(note_id: u32, window: &mut Window, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();
        let view = cx.entity();

        cx.spawn_in(window, async move |_, window| {
            let model = Note::find_by_id(note_id as i64).one(&*db).await.ok()??;
            let path = model.file_path.as_ref().map(PathBuf::from);
            let (content, missing) = match path.as_ref() {
                Some(path) => match read_to_string(path) {
                    Ok(content) => (content, false),
                    Err(_) => (model.cached_content.clone(), true),
                },
                None => (model.cached_content.clone(), false),
            };

            if missing && model.file_missing_since.is_none() {
                let _ = note::ActiveModel {
                    id: Set(note_id as i64),
                    file_missing_since: Set(Some(now_ts())),
                    ..Default::default()
                }
                .update(&*db)
                .await;
            }

            window
                .update(|window, cx| {
                    view.update(cx, |this, cx| {
                        this.load_model(model, content, missing, window, cx);
                    })
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    fn load_model(
        &mut self,
        model: note::Model,
        content: String,
        missing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        self.title = model.title.into();
        self.current_path = model.file_path.map(PathBuf::from);
        self.last_file_saved = content.clone().into();
        self.save_state = if missing {
            SaveState::Missing
        } else {
            SaveState::Saved
        };
        self.stats = DocumentStats::from_text(&content);

        self.editor.update(cx, |editor, cx| {
            editor.set_highlighter(Language::Markdown, cx);
            editor.set_value(content.clone(), window, cx);
            editor.focus(window, cx);
        });
        self.preview.update(cx, |preview, cx| {
            preview.set_text(&content, cx);
        });
        cx.notify();
    }

    fn update_from_editor(&mut self, value: SharedString, cx: &mut Context<Self>) {
        self.preview.update(cx, |preview, cx| {
            preview.set_text(value.as_ref(), cx);
        });
        self.stats = DocumentStats::from_text(value.as_ref());
        self.save_state = match self.save_state {
            SaveState::Missing => SaveState::Missing,
            _ if self.current_path.is_some() && value == self.last_file_saved => SaveState::Saved,
            _ => SaveState::Dirty,
        };
        self.schedule_auto_save(cx);
        cx.notify();
    }

    fn schedule_auto_save(&mut self, cx: &mut Context<Self>) {
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        let epoch = self.auto_save_epoch;

        self._auto_save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(900))
                .await;

            let save_request = this
                .update(cx, |this, cx| {
                    if this.auto_save_epoch != epoch {
                        return None;
                    }

                    let note_id = this.note_id;
                    let path = this.current_path.clone();
                    let is_missing = matches!(this.save_state, SaveState::Missing);
                    let content = this.editor.read(cx).value();

                    if path.is_some() && !is_missing {
                        this.save_state = SaveState::Saving;
                        cx.notify();
                    }

                    Some((note_id, path, is_missing, content))
                })
                .ok()
                .flatten();

            let Some((note_id, path, is_missing, content)) = save_request else {
                return;
            };

            let db = this
                .read_with(cx, |_, cx| cx.global::<DB>().conn.clone())
                .ok();
            let Some(db) = db else {
                return;
            };

            let mut write_result = Ok(());
            if let Some(path) = path.as_ref()
                && !is_missing
            {
                if let Some(parent) = path.parent() {
                    write_result = create_dir_all(parent).map_err(|err| err.to_string());
                }
                if write_result.is_ok() {
                    write_result = write(path, content.to_string()).map_err(|err| err.to_string());
                }
            }

            let cache_result = note::ActiveModel {
                id: Set(note_id as i64),
                cached_content: Set(content.to_string()),
                updated_at: Set(now_ts()),
                ..Default::default()
            }
            .update(&*db)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string());

            let result = write_result.and(cache_result);

            this.update(cx, |this, cx| {
                if this.auto_save_epoch == epoch {
                    this.finish_auto_save(content, result, cx);
                }
            })
            .ok();
        }));
    }

    fn finish_auto_save(
        &mut self,
        content: SharedString,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(())
                if self.current_path.is_some()
                    && !matches!(self.save_state, SaveState::Missing) =>
            {
                self.last_file_saved = content.clone();
                let current_content = self.editor.read(cx).value();
                self.save_state = if current_content == content {
                    SaveState::Saved
                } else {
                    SaveState::Dirty
                };
            }
            Ok(()) => {}
            Err(err) => {
                self.save_state = SaveState::Error(err.into());
            }
        }
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let path = self.current_path.clone().unwrap_or_else(|| {
            unique_note_path(
                cx.global::<DB>().data_dir.join("notes"),
                self.title.as_ref(),
            )
        });
        self.save_to_path(path, cx);
    }

    fn save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let start_dir = self
            .current_path
            .as_ref()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or_else(|| cx.global::<DB>().data_dir.join("notes"));

        let file_name = suggested_file_name(self.title.as_ref());
        let receiver = cx.prompt_for_new_path(&start_dir, Some(&file_name));
        let view = cx.entity();

        cx.spawn_in(window, async move |_, window| {
            let path = receiver.await.ok().into_iter().flatten().flatten().next()?;
            window
                .update(|_, cx| {
                    view.update(cx, |this, cx| {
                        this.save_to_path(path, cx);
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    fn save_to_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        let content = self.editor.read(cx).value();
        let note_id = self.note_id;
        let db = cx.global::<DB>().conn.clone();
        self.save_state = SaveState::Saving;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = (|| {
                if let Some(parent) = path.parent() {
                    create_dir_all(parent).map_err(|err| err.to_string())?;
                }
                write(&path, content.to_string()).map_err(|err| err.to_string())?;
                Ok(())
            })();

            let result = match result {
                Ok(()) => note::ActiveModel {
                    id: Set(note_id as i64),
                    file_path: Set(Some(path.display().to_string())),
                    cached_content: Set(content.to_string()),
                    file_missing_since: Set(None),
                    updated_at: Set(now_ts()),
                    ..Default::default()
                }
                .update(&*db)
                .await
                .map(|_| ())
                .map_err(|err| err.to_string()),
                Err(err) => Err(err),
            };

            this.update(cx, |this, cx| {
                this.finish_save(path, content, result, cx);
            })
            .ok();
        })
        .detach();
    }

    fn finish_save(
        &mut self,
        path: PathBuf,
        content: SharedString,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => {
                self.current_path = Some(path);
                self.last_file_saved = content.clone();
                let current_content = self.editor.read(cx).value();
                self.save_state = if current_content == content {
                    SaveState::Saved
                } else {
                    SaveState::Dirty
                };
            }
            Err(err) => {
                self.save_state = SaveState::Error(err.into());
            }
        }
        cx.notify();
    }

    fn toggle_mode(&mut self, cx: &mut Context<Self>) {
        self.mode = match self.mode {
            EditorMode::Split => EditorMode::Source,
            EditorMode::Source => EditorMode::Preview,
            EditorMode::Preview => EditorMode::Split,
        };
        cx.notify();
    }

    fn apply_format(
        &mut self,
        action: &ApplyMarkdownFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self.editor.read(cx).selected_value().to_string();
        let replacement = match action.0 {
            MarkdownFormat::HeadingOne => Self::prefix_block(&selected, "# ", "Heading"),
            MarkdownFormat::HeadingTwo => Self::prefix_block(&selected, "## ", "Heading"),
            MarkdownFormat::HeadingThree => Self::prefix_block(&selected, "### ", "Heading"),
            MarkdownFormat::Bold => Self::wrap_inline(&selected, "**", "**", "bold text"),
            MarkdownFormat::Italic => Self::wrap_inline(&selected, "*", "*", "italic text"),
            MarkdownFormat::InlineCode => Self::wrap_inline(&selected, "`", "`", "code"),
            MarkdownFormat::Link => Self::wrap_inline(&selected, "[", "](https://)", "link text"),
            MarkdownFormat::BulletList => Self::prefix_lines(&selected, "- ", "List item"),
            MarkdownFormat::OrderedList => Self::numbered_lines(&selected),
            MarkdownFormat::Quote => Self::prefix_lines(&selected, "> ", "Quote"),
            MarkdownFormat::CodeBlock => Self::code_block(&selected),
        };

        self.editor.update(cx, |editor, cx| {
            editor.replace(replacement, window, cx);
            editor.focus(window, cx);
        });
    }

    fn wrap_inline(selected: &str, prefix: &str, suffix: &str, placeholder: &str) -> String {
        let body = if selected.is_empty() {
            placeholder
        } else {
            selected
        };
        format!("{prefix}{body}{suffix}")
    }

    fn prefix_block(selected: &str, prefix: &str, placeholder: &str) -> String {
        let body = selected.trim_start_matches('#').trim_start();
        let body = if body.is_empty() { placeholder } else { body };
        format!("{prefix}{body}")
    }

    fn prefix_lines(selected: &str, prefix: &str, placeholder: &str) -> String {
        if selected.is_empty() {
            return format!("{prefix}{placeholder}");
        }

        selected
            .lines()
            .map(|line| format!("{prefix}{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn numbered_lines(selected: &str) -> String {
        if selected.is_empty() {
            return "1. List item".to_string();
        }

        selected
            .lines()
            .enumerate()
            .map(|(index, line)| format!("{}. {}", index + 1, line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn code_block(selected: &str) -> String {
        let body = if selected.is_empty() {
            "code"
        } else {
            selected
        };
        format!("```\n{body}\n```")
    }

    fn on_action_save(&mut self, _: &SaveMarkdownFile, _: &mut Window, cx: &mut Context<Self>) {
        self.save(cx);
    }

    fn on_action_save_as(
        &mut self,
        _: &SaveMarkdownFileAs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_as(window, cx);
    }

    fn on_action_toggle_mode(
        &mut self,
        _: &ToggleEditorMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_mode(cx);
    }

    fn on_action_expand_emmet(
        &mut self,
        _: &ExpandEmmet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self.editor.read(cx).selected_value().to_string();
        let editor_has_selection = !selected.is_empty();

        if editor_has_selection {
            self.show_emmet_input = true;
            let range = self.editor.read(cx).selected_range();
            self.emmet_replacement_range = Some(range);

            self.emmet_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
            cx.notify();
            return;
        }

        let mut replacement_start_offset = None;
        let mut word = String::new();
        let editor = self.editor.read(cx);
        let offset = editor.cursor();
        let text = editor.text().to_string();
        let mut start = offset;
        let bytes = text.as_bytes();
        while start > 0 {
            let c = bytes[start - 1];
            if c.is_ascii_alphanumeric() || c == b'.' || c == b'#' || c == b'>' {
                start -= 1;
            } else {
                break;
            }
        }
        if start < offset {
            word = text[start..offset].to_string();
            replacement_start_offset = Some(start);
        }

        if !word.is_empty() {
            let replacement = parse_emmet_abbreviation(&word, "");
            self.editor.update(cx, |editor, cx| {
                if let Some(start) = replacement_start_offset {
                    let end = editor.cursor();
                    let rope = editor.text();
                    let start_utf16 = rope.offset_to_offset_utf16(start);
                    let end_utf16 = rope.offset_to_offset_utf16(end);

                    gpui::EntityInputHandler::replace_text_in_range(
                        editor,
                        Some(start_utf16..end_utf16),
                        &replacement,
                        window,
                        cx,
                    );
                }
                editor.focus(window, cx);
            });
        } else {
            self.show_emmet_input = true;
            let range = editor.selected_range();
            self.emmet_replacement_range = Some(range);
            self.emmet_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
            cx.notify();
        }
    }

    fn on_action_emmet_submit_wrap(
        &mut self,
        _: &EmmetSubmitWrap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.show_emmet_input {
            return;
        }

        let abbreviation = self.emmet_input.read(cx).value();

        if let Some(range) = self.emmet_replacement_range.clone() {
            self.editor.update(cx, |editor, cx| {
                let rope = editor.text();
                let content = rope.slice(range.clone()).to_string();
                let replacement = parse_emmet_abbreviation(&abbreviation, &content);
                let start_utf16 = rope.offset_to_offset_utf16(range.start);
                let end_utf16 = rope.offset_to_offset_utf16(range.end);

                gpui::EntityInputHandler::replace_text_in_range(
                    editor,
                    Some(start_utf16..end_utf16),
                    &replacement,
                    window,
                    cx,
                );
                editor.focus(window, cx);
            });
        }

        self.show_emmet_input = false;
        self.emmet_replacement_range = None;
        cx.notify();
    }

    fn on_action_emmet_cancel_wrap(
        &mut self,
        _: &EmmetCancelWrap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_emmet_input {
            self.show_emmet_input = false;
            self.emmet_replacement_range = None;
            self.editor
                .update(cx, |editor, cx| editor.focus(window, cx));
            cx.notify();
        }
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.mode;
        let save_state = self.save_state.clone();

        h_flex()
            .id("markdown-toolbar")
            .w_full()
            .gap_2()
            .items_center()
            .justify_between()
            .py_2()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("save-note")
                            .icon(IconName::Check)
                            .ghost()
                            .small()
                            .tooltip("Save (Ctrl+S)")
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    )
                    .child(
                        Button::new("save-note-as")
                            .label("Save as")
                            .ghost()
                            .small()
                            .tooltip("Save as (Ctrl+Shift+S)")
                            .on_click(cx.listener(|this, _, window, cx| this.save_as(window, cx))),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(format_button("h1", "H1", MarkdownFormat::HeadingOne))
                    .child(format_button("h2", "H2", MarkdownFormat::HeadingTwo))
                    .child(format_button("h3", "H3", MarkdownFormat::HeadingThree))
                    .child(format_button("bold", "B", MarkdownFormat::Bold))
                    .child(format_button("italic", "I", MarkdownFormat::Italic))
                    .child(format_button("code", "Code", MarkdownFormat::InlineCode))
                    .child(format_button("link", "Link", MarkdownFormat::Link))
                    .child(format_button(
                        "bullet",
                        "- List",
                        MarkdownFormat::BulletList,
                    ))
                    .child(format_button(
                        "ordered",
                        "1. List",
                        MarkdownFormat::OrderedList,
                    ))
                    .child(format_button("quote", "Quote", MarkdownFormat::Quote))
                    .child(format_button(
                        "code-block",
                        "Block",
                        MarkdownFormat::CodeBlock,
                    )),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("mode-split")
                            .label("Split")
                            .ghost()
                            .small()
                            .selected(mode == EditorMode::Split)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.mode = EditorMode::Split;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("mode-source")
                            .label("Source")
                            .ghost()
                            .small()
                            .selected(mode == EditorMode::Source)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.mode = EditorMode::Source;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("mode-preview")
                            .label("Preview")
                            .ghost()
                            .small()
                            .selected(mode == EditorMode::Preview)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.mode = EditorMode::Preview;
                                cx.notify();
                            })),
                    )
                    .child(status_badge(save_state, cx)),
            )
    }

    fn render_source(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("markdown-source")
            .size_full()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(cx.theme().mono_font_size)
            .bg(cx.theme().background)
            .child(
                Input::new(&self.editor)
                    .h_full()
                    .p_0()
                    .border_0()
                    .focus_bordered(false),
            )
    }

    fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("markdown-preview")
            .size_full()
            .bg(cx.theme().background)
            .child(
                TextView::new(&self.preview)
                    .code_block_actions(|code_block, _window, _cx| {
                        Clipboard::new("copy-code").value(code_block.code().clone())
                    })
                    .p_5()
                    .scrollable(true)
                    .selectable(true),
            )
    }

    fn render_editor_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match self.mode {
            EditorMode::Split => div()
                .size_full()
                .child(
                    h_resizable("markdown-editor-split")
                        .child(resizable_panel().child(self.render_source(cx)))
                        .child(resizable_panel().child(self.render_preview(cx))),
                )
                .into_any_element(),
            EditorMode::Source => self.render_source(cx).into_any_element(),
            EditorMode::Preview => self.render_preview(cx).into_any_element(),
        }
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let path = self
            .current_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Not saved to a file yet".to_string());

        h_flex()
            .id("markdown-status-bar")
            .w_full()
            .items_center()
            .justify_between()
            .gap_3()
            .px_3()
            .py_1p5()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .text_color(cx.theme().muted_foreground)
            .text_xs()
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(path)),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(format!("{} lines", self.stats.lines))
                    .child(format!("{} words", self.stats.words))
                    .child(format!("{} chars", self.stats.characters))
                    .child("Ctrl+E toggles mode"),
            )
    }
}

impl Focusable for MarkdownEditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MarkdownEditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_background = cx.theme().background;
        let theme_border = cx.theme().border;
        let theme_input = cx.theme().input;

        v_flex()
            .id("markdown-editor-window")
            .key_context("MarkdownEditor")
            .track_focus(&self.focus_handle)
            .size_full()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .on_action(cx.listener(Self::on_action_save))
            .on_action(cx.listener(Self::on_action_save_as))
            .on_action(cx.listener(Self::on_action_toggle_mode))
            .on_action(cx.listener(Self::on_action_expand_emmet))
            .on_action(cx.listener(Self::on_action_emmet_submit_wrap))
            .on_action(cx.listener(Self::on_action_emmet_cancel_wrap))
            .on_action(cx.listener(Self::apply_format))
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .w_full()
                    .child(self.render_editor_body(cx)),
            )
            .child(self.render_status_bar(cx))
            .children(if self.show_emmet_input {
                Some(
                    div()
                        .key_context("EmmetInput")
                        .absolute()
                        .top(px(60.))
                        .left(px(20.))
                        .w(px(300.))
                        .p_2()
                        .bg(theme_background)
                        .border_1()
                        .border_color(theme_border)
                        .rounded_md()
                        .shadow_sm()
                        .child(
                            Input::new(&self.emmet_input)
                                .w_full()
                                .bg(theme_input)
                                .px_2()
                                .py_1()
                                .rounded_sm(),
                        ),
                )
            } else {
                None
            })
    }
}

impl DocumentStats {
    fn from_text(text: &str) -> Self {
        Self {
            lines: text.lines().count().max(1),
            words: text.split_whitespace().count(),
            characters: text.chars().count(),
        }
    }
}

fn parse_emmet_abbreviation(abbreviation: &str, content: &str) -> String {
    let parts = abbreviation.split('>');
    let mut prefix = String::new();
    let mut suffix = String::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        let mut tag = "div";
        let mut id = "";
        let mut classes = Vec::new();

        let mut current = part;
        if let Some(pos) = current.find(['.', '#']) {
            if pos > 0 {
                tag = &current[..pos];
            }
            current = &current[pos..];
        } else {
            tag = current;
            current = "";
        }

        while !current.is_empty() {
            let is_class = current.starts_with('.');
            let is_id = current.starts_with('#');
            current = &current[1..];
            let next_pos = current.find(['.', '#']).unwrap_or(current.len());
            let name = &current[..next_pos];

            if is_class && !name.is_empty() {
                classes.push(name);
            } else if is_id && !name.is_empty() {
                id = name;
            }
            current = &current[next_pos..];
        }

        let mut attrs = String::new();
        if !id.is_empty() {
            attrs.push_str(&format!(" id=\"{id}\""));
        }
        if !classes.is_empty() {
            attrs.push_str(&format!(" class=\"{}\"", classes.join(" ")));
        }

        prefix.push_str(&format!("<{tag}{attrs}>"));
        suffix = format!("</{tag}>") + &suffix;
    }

    format!("{prefix}{content}{suffix}")
}

fn format_button(id: &'static str, label: &'static str, format: MarkdownFormat) -> Button {
    Button::new(format!("format-{id}"))
        .label(label)
        .ghost()
        .small()
        .on_click(move |_, window, cx| {
            window.dispatch_action(Box::new(ApplyMarkdownFormat(format)), cx);
        })
}

fn status_badge(save_state: SaveState, cx: &mut Context<MarkdownEditorView>) -> impl IntoElement {
    match save_state {
        SaveState::Saved => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().success)
            .child(IconName::CircleCheck)
            .child("Saved")
            .into_any_element(),
        SaveState::Dirty => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().warning)
            .child(IconName::Asterisk)
            .child("Unsaved")
            .into_any_element(),
        SaveState::Saving => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().info)
            .child(IconName::Loader)
            .child("Saving")
            .into_any_element(),
        SaveState::Missing => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().warning)
            .child(IconName::TriangleAlert)
            .child("File missing")
            .into_any_element(),
        SaveState::Error(err) => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().danger)
            .child(IconName::TriangleAlert)
            .child(err)
            .into_any_element(),
    }
}

fn suggested_file_name(title: &str) -> String {
    let stem = if title.trim().is_empty() {
        "untitled"
    } else {
        title.trim()
    };
    let mut file_name = String::with_capacity(stem.len() + 3);

    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            file_name.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            file_name.push('-');
        }
    }

    if file_name.is_empty() {
        file_name.push_str("untitled");
    }
    if !file_name.ends_with(".md") {
        file_name.push_str(".md");
    }
    file_name
}

fn unique_note_path(dir: PathBuf, title: &str) -> PathBuf {
    let file_name = suggested_file_name(title);
    let candidate = dir.join(&file_name);
    if !candidate.exists() {
        return candidate;
    }

    let stem = Path::new(&file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("untitled");
    let extension = Path::new(&file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("md");

    for index in 2.. {
        let candidate = dir.join(format!("{stem}-{index}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    dir.join(file_name)
}

pub(crate) fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}
