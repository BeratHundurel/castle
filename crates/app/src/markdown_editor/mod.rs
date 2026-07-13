pub(crate) mod action;
mod emmet;
mod formatting;
mod handlers;
mod outline;
mod persistence;
mod render;
pub mod types;
mod util;

use anyhow::Result;
use entity::note;
use gpui::{
    App, AppContext, Bounds, Context, Entity, FocusHandle, Pixels, SharedString, Task, Window,
    point, px,
};
use gpui_component::{
    highlighter::Language,
    input::{InputEvent, InputState, TabSize},
    text::TextViewState,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use std::{ops::Range, path::PathBuf, time::Duration};

use crate::DB;
use crate::app_settings::AppSettings;
use outline::MarkdownOutline;
use types::*;

pub use types::DocumentStats;
pub(crate) use types::{DEFAULT_NOTE, SaveState};
pub(crate) use util::{now_ts, unique_note_path};

const AUTO_SAVE_IDLE_DELAY: Duration = Duration::from_millis(1_200);
const OUTLINE_SCROLL_LAYOUT_DELAY: Duration = Duration::from_millis(16);
const OUTLINE_SCROLL_ATTEMPTS: usize = 4;
const OUTLINE_SCROLL_TOP_INSET: Pixels = px(32.);
const OUTLINE_TRANSITION_DURATION: Duration = Duration::from_millis(160);

pub(crate) struct MarkdownEditorView {
    note_id: u32,
    title: SharedString,
    focus_handle: FocusHandle,
    editor: Entity<InputState>,
    preview: Entity<TextViewState>,
    mode: EditorMode,
    current_path: Option<PathBuf>,
    file_managed_by_app: bool,
    save_state: SaveState,
    load_error: Option<SharedString>,
    stats: DocumentStats,
    is_loading: bool,
    suppress_editor_events: bool,
    auto_save_epoch: u64,
    _auto_save_task: Option<Task<()>>,
    emmet_input: Entity<InputState>,
    show_emmet_input: bool,
    emmet_replacement_range: Option<Range<usize>>,
    source_bounds: Option<Bounds<Pixels>>,
    outline: MarkdownOutline,
    outline_visible: bool,
    outline_rendered: bool,
    outline_transition_epoch: usize,
    outline_selected: Option<usize>,
    outline_navigation_generation: u64,
    preview_scroll_handle: gpui::ScrollHandle,
    outline_focus_handle: FocusHandle,
    view_width: gpui::Pixels,
}

impl MarkdownEditorView {
    pub(crate) fn view(note_id: u32, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(note_id, window, cx))
    }

    fn new(note_id: u32, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let line_numbers = AppSettings::markdown_line_numbers(cx);
        let soft_wrap = AppSettings::markdown_soft_wrap(cx);
        let mode = EditorMode::from_str(AppSettings::markdown_editor_mode(cx).as_ref());
        let outline_visible = AppSettings::markdown_outline_visible(cx);

        let editor = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor(Language::Markdown)
                .line_number(line_numbers)
                .indent_guides(false)
                .tab_size(TabSize {
                    tab_size: 2,
                    ..Default::default()
                })
                .soft_wrap(soft_wrap)
                .searchable(true)
                .placeholder("Write Markdown...")
                .default_value("")
        });

        let preview = cx.new(|cx| {
            TextViewState::markdown("", cx)
                .scrollable(true)
                .selectable(true)
        });

        let emmet_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Enter Emmet abbreviation (e.g. details>summary)")
        });

        let focus_handle = cx.focus_handle();
        let outline_focus_handle = cx.focus_handle();

        cx.subscribe_in(
            &editor,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change if !this.suppress_editor_events => {
                    this.update_from_editor(cx);
                }
                InputEvent::PressEnter { .. } if !this.suppress_editor_events => {
                    this.continue_markdown_after_enter(window, cx);
                }
                _ => {}
            },
        )
        .detach();

        Self::load_note_async(note_id, window, cx).detach();

        Self {
            note_id,
            title: "Untitled note".into(),
            focus_handle,
            editor,
            preview,
            mode,
            current_path: None,
            file_managed_by_app: false,
            save_state: SaveState::Saved,
            load_error: None,
            stats: DocumentStats::from_text(""),
            is_loading: true,
            suppress_editor_events: false,
            auto_save_epoch: 0,
            _auto_save_task: None,
            emmet_input,
            show_emmet_input: false,
            emmet_replacement_range: None,
            source_bounds: None,
            outline: MarkdownOutline::default(),
            outline_visible,
            outline_rendered: outline_visible,
            outline_transition_epoch: 0,
            outline_selected: None,
            outline_navigation_generation: 0,
            preview_scroll_handle: gpui::ScrollHandle::new(),
            outline_focus_handle,
            view_width: gpui::px(0.),
        }
    }

    pub(crate) fn save_state(&self) -> SaveState {
        self.save_state.clone()
    }

    #[cfg(test)]
    pub(crate) fn loaded_content(&self, cx: &App) -> Option<String> {
        (!self.is_loading).then(|| self.editor.read(cx).value().to_string())
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

    fn set_mode(&mut self, mode: EditorMode, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = mode;
        self.focus_active_mode(window, cx);
        cx.notify();
    }

    fn toggle_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = match self.mode {
            EditorMode::Split => EditorMode::Source,
            EditorMode::Source => EditorMode::Preview,
            EditorMode::Preview => EditorMode::Split,
        };
        self.focus_active_mode(window, cx);
        cx.notify();
    }

    fn focus_active_mode(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.mode {
            EditorMode::Split | EditorMode::Source => {
                self.editor
                    .update(cx, |editor, cx| editor.focus(window, cx));
            }
            EditorMode::Preview => {
                self.focus_handle.focus(window, cx);
            }
        }
    }

    fn toggle_outline(&mut self, cx: &mut Context<Self>) {
        self.outline_visible = !self.outline_visible;
        self.outline_transition_epoch = self.outline_transition_epoch.saturating_add(1);
        let transition_epoch = self.outline_transition_epoch;

        if self.outline_visible {
            self.outline_rendered = true;
        } else {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(OUTLINE_TRANSITION_DURATION)
                    .await;
                this.update(cx, |this, cx| {
                    if this.outline_transition_epoch == transition_epoch && !this.outline_visible {
                        this.outline_rendered = false;
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }

        AppSettings::set_markdown_outline_visible(self.outline_visible, cx);
        cx.notify();
    }

    fn select_outline_item(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.outline.items.get(index).cloned() else {
            return;
        };
        self.outline_selected = Some(index);
        self.outline_navigation_generation = self.outline_navigation_generation.saturating_add(1);
        let navigation_generation = self.outline_navigation_generation;
        match self.mode {
            EditorMode::Source | EditorMode::Split => {
                self.editor.update(cx, |editor, cx| {
                    editor.set_cursor_position(
                        gpui_component::input::Position {
                            line: item.source_line as u32,
                            character: item.source_column as u32,
                        },
                        window,
                        cx,
                    );
                });
                self.align_source_heading_after_layout(navigation_generation, cx);
            }
            EditorMode::Preview => {
                self.preview_scroll_handle
                    .scroll_to_item(item.preview_section_index);
            }
        }
        cx.notify();
    }

    fn align_source_heading_after_layout(
        &self,
        navigation_generation: u64,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            for _ in 0..OUTLINE_SCROLL_ATTEMPTS {
                cx.background_executor()
                    .timer(OUTLINE_SCROLL_LAYOUT_DELAY)
                    .await;

                let aligned = this
                    .update(cx, |this, cx| {
                        if this.outline_navigation_generation != navigation_generation {
                            return true;
                        }

                        let Some(source_bounds) = this.source_bounds else {
                            return false;
                        };

                        this.editor.update(cx, |editor, cx| {
                            let cursor = editor.cursor();
                            let Some(cursor_bounds) = editor.range_to_bounds(&(cursor..cursor))
                            else {
                                return false;
                            };

                            let current = editor.scroll_offset();
                            let cursor_offset = cursor_bounds.origin.y
                                - source_bounds.origin.y
                                - OUTLINE_SCROLL_TOP_INSET;
                            editor
                                .set_scroll_offset(point(current.x, current.y - cursor_offset), cx);
                            true
                        })
                    })
                    .unwrap_or(true);

                if aligned {
                    return;
                }
            }
        })
        .detach();
    }
}
