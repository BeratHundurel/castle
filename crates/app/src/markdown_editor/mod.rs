mod action;
mod emmet;
mod render;
mod types;
mod util;

use anyhow::Result;
use entity::{note, note::Entity as Note};
use gpui::{
    App, AppContext, Context, Entity, EntityInputHandler, FocusHandle, Focusable, KeyBinding,
    SharedString, Subscription, Task, Window,
};
use gpui_component::{
    highlighter::Language,
    input::{InputEvent, InputState, RopeExt, TabSize},
    text::TextViewState,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use std::{fs::read_to_string, time::Duration};
use std::{
    fs::{create_dir_all, write},
    path::PathBuf,
};

use crate::DB;
use action::*;
use emmet::parse_emmet_abbreviation;
use types::*;
use util::{suggested_file_name, unique_note_path};

pub(crate) use types::{DEFAULT_NOTE, SaveState};
pub(crate) use util::now_ts;

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

            match result {
                Ok(_) => {
                    this.update(cx, |this, cx| {
                        this.save_state = this.resolve_save_state(&content, cx);
                    })
                    .ok();
                }
                Err(err) => {
                    this.update(cx, |this, _cx| {
                        this.save_state = SaveState::Error(err.into());
                    })
                    .ok();
                }
            }
        }));
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
        self.save_state = SaveState::Saving;

        let content = self.editor.read(cx).value();
        let note_id = self.note_id;
        let db = cx.global::<DB>().conn.clone();

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

            match result {
                Ok(_) => {
                    this.update(cx, |this, cx| {
                        this.save_state = this.resolve_save_state(&content, cx);
                    })
                    .ok();
                }
                Err(err) => {
                    this.update(cx, |this, _cx| {
                        this.save_state = SaveState::Error(err.into());
                    })
                    .ok();
                }
            }

            this.update(cx, |this, cx| {
                this.save_state = this.resolve_save_state(&content, cx);
            })
            .ok();
        })
        .detach();
    }

    fn resolve_save_state(
        &self,
        saved_content: &SharedString,
        cx: &mut Context<Self>,
    ) -> SaveState {
        let current = self.editor.read(cx).value();
        if current == *saved_content {
            SaveState::Saved
        } else {
            SaveState::Dirty
        }
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

        let editor = self.editor.read(cx);
        let offset = editor.cursor();
        let text = editor.text().to_string();

        let prefix = &text[..offset];
        let mut start = offset;
        for (idx, ch) in prefix.char_indices().rev() {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '#' || ch == '>' {
                start = idx;
            } else {
                break;
            }
        }

        let (word, replacement_start_offset) = if start < offset {
            (text[start..offset].to_string(), Some(start))
        } else {
            (String::new(), None)
        };

        if !word.is_empty() {
            let replacement = parse_emmet_abbreviation(&word, "");
            self.editor.update(cx, |editor, cx| {
                if let Some(start) = replacement_start_offset {
                    let end = editor.cursor();
                    let rope = editor.text();
                    let start_utf16 = rope.offset_to_offset_utf16(start);
                    let end_utf16 = rope.offset_to_offset_utf16(end);

                    EntityInputHandler::replace_text_in_range(
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

                EntityInputHandler::replace_text_in_range(
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
}
