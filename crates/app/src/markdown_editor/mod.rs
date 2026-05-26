mod action;
mod emmet;
mod formatting;
mod handlers;
mod persistence;
mod render;
pub mod types;
mod util;

use anyhow::Result;
use entity::note;
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, KeyBinding, SharedString, Task,
    Window,
};
use gpui_component::{
    highlighter::Language,
    input::{InputEvent, InputState, TabSize},
    text::TextViewState,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use std::{ops::Range, path::PathBuf, time::Duration};

use crate::DB;
use action::*;
use types::*;

pub use types::DocumentStats;
pub(crate) use types::{DEFAULT_NOTE, SaveState};
pub(crate) use util::now_ts;

const AUTO_SAVE_IDLE_DELAY: Duration = Duration::from_millis(1_200);

pub(crate) struct MarkdownEditorView {
    note_id: u32,
    title: SharedString,
    focus_handle: FocusHandle,
    editor: Entity<InputState>,
    preview: Entity<TextViewState>,
    mode: EditorMode,
    current_path: Option<PathBuf>,
    save_state: SaveState,
    stats: DocumentStats,
    auto_save_epoch: u64,
    _auto_save_task: Option<Task<()>>,
    emmet_input: Entity<InputState>,
    show_emmet_input: bool,
    emmet_replacement_range: Option<Range<usize>>,
}

pub fn init(cx: &mut App) {
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

        cx.subscribe(&editor, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.update_from_editor(cx);
            }
        })
        .detach();

        Self::load_note_async(note_id, window, cx);

        Self {
            note_id,
            title: "Untitled note".into(),
            focus_handle,
            editor,
            preview,
            mode: EditorMode::Split,
            current_path: None,
            save_state: SaveState::Saved,
            stats: DocumentStats::from_text(DEFAULT_NOTE),
            auto_save_epoch: 0,
            _auto_save_task: None,
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
        cx.notify();

        let now = now_ts();
        let note_id = self.note_id;
        let db = cx.global::<DB>().conn.clone();
        let title = title.to_string();

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
    }

    fn set_mode(&mut self, mode: EditorMode, cx: &mut Context<Self>) {
        self.mode = mode;
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
}
