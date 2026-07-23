pub(crate) mod action;
mod attachments;
mod emmet;
mod formatting;
mod handlers;
mod outline;
mod persistence;
mod render;
pub mod types;
mod util;

use gpui::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Pixels, SharedString,
    Task, UniformListScrollHandle, Window, point, px,
};
use gpui_component::{
    highlighter::Language,
    input::{InputEvent, InputState, RopeExt as _, TabSize},
};
use std::{
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::app_settings::AppSettings;
use outline::{DocumentOutline, JsonOutline, MarkdownOutline, OutlineRow};
use types::*;

pub use types::DocumentStats;
pub(crate) use types::{DEFAULT_NOTE, DocumentKind, SaveState};
pub(crate) use util::{now_ts, unique_note_path};

const AUTO_SAVE_IDLE_DELAY: Duration = Duration::from_millis(1_200);
const DOCUMENT_ANALYSIS_DELAY: Duration = Duration::from_millis(180);
const OUTLINE_SCROLL_LAYOUT_DELAY: Duration = Duration::from_millis(16);
const OUTLINE_SCROLL_ATTEMPTS: usize = 4;
const OUTLINE_SCROLL_TOP_INSET: Pixels = px(32.);
const OUTLINE_TRANSITION_DURATION: Duration = Duration::from_millis(160);
const OUTLINE_SOURCE_HIGHLIGHT_DURATION: Duration = Duration::from_millis(1_400);
const OUTLINE_DEFAULT_WIDTH: Pixels = px(224.);
const OUTLINE_MIN_WIDTH: Pixels = px(176.);
const OUTLINE_MAX_WIDTH: Pixels = px(480.);
const EDITOR_MIN_WIDTH_WITH_OUTLINE: Pixels = px(360.);
const OUTLINE_INDENT_STEP: Pixels = px(8.);

struct DocumentAnalysis {
    stats: DocumentStats,
    outline: DocumentOutline,
}

#[derive(Clone, Copy)]
struct OutlineSourceHighlight {
    generation: u64,
    source_offset: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DocumentEditorEvent {
    PathChanged,
    Saved(u32),
}

pub(crate) struct DocumentEditorView {
    note_id: u32,
    title: SharedString,
    focus_handle: FocusHandle,
    editor: Entity<InputState>,
    kind: DocumentKind,
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
    analysis_generation: u64,
    _analysis_task: Option<Task<()>>,
    emmet_input: Entity<InputState>,
    show_emmet_input: bool,
    emmet_replacement_range: Option<Range<usize>>,
    source_bounds: Option<Bounds<Pixels>>,
    outline: DocumentOutline,
    outline_rows: Arc<Vec<OutlineRow>>,
    outline_visible: bool,
    outline_rendered: bool,
    outline_transition_epoch: usize,
    outline_selected: Option<usize>,
    outline_navigation_generation: u64,
    outline_source_highlight: Option<OutlineSourceHighlight>,
    _outline_source_highlight_task: Option<Task<()>>,
    preview_scroll_handle: gpui::ScrollHandle,
    outline_scroll_handle: UniformListScrollHandle,
    outline_focus_handle: FocusHandle,
    view_width: gpui::Pixels,
    view_bounds: Option<Bounds<Pixels>>,
    outline_width: Pixels,
}

impl EventEmitter<DocumentEditorEvent> for DocumentEditorView {}

impl DocumentEditorView {
    pub(crate) fn view(note_id: u32, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(note_id, window, cx))
    }

    fn new(note_id: u32, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let line_numbers = AppSettings::editor_line_numbers(cx);
        let soft_wrap = AppSettings::editor_soft_wrap(cx);
        let outline_visible = AppSettings::document_outline_visible(cx);

        let editor = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor(Language::Plain)
                .line_number(line_numbers)
                .indent_guides(false)
                .tab_size(TabSize {
                    tab_size: 2,
                    ..Default::default()
                })
                .soft_wrap(soft_wrap)
                .searchable(true)
                .placeholder("Start typing...")
                .default_value("")
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
                InputEvent::PressEnter { .. }
                    if !this.suppress_editor_events && this.kind == DocumentKind::Markdown =>
                {
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
            kind: DocumentKind::PlainText,
            mode: EditorMode::Source,
            current_path: None,
            file_managed_by_app: false,
            save_state: SaveState::Saved,
            load_error: None,
            stats: DocumentStats::from_text(""),
            is_loading: true,
            suppress_editor_events: false,
            auto_save_epoch: 0,
            _auto_save_task: None,
            analysis_generation: 0,
            _analysis_task: None,
            emmet_input,
            show_emmet_input: false,
            emmet_replacement_range: None,
            source_bounds: None,
            outline: DocumentOutline::None,
            outline_rows: Arc::new(Vec::new()),
            outline_visible,
            outline_rendered: false,
            outline_transition_epoch: 0,
            outline_selected: None,
            outline_navigation_generation: 0,
            outline_source_highlight: None,
            _outline_source_highlight_task: None,
            preview_scroll_handle: gpui::ScrollHandle::new(),
            outline_scroll_handle: UniformListScrollHandle::default(),
            outline_focus_handle,
            view_width: gpui::px(0.),
            view_bounds: None,
            outline_width: OUTLINE_DEFAULT_WIDTH,
        }
    }

    pub(crate) fn save_state(&self) -> SaveState {
        self.save_state.clone()
    }

    pub(crate) fn reload_after_external_change(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.save_state != SaveState::Saved {
            return;
        }

        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        self.is_loading = true;
        Self::load_note_async(self.note_id, window, cx).detach();
        cx.notify();
    }

    pub(crate) fn kind(&self) -> DocumentKind {
        self.kind
    }

    #[cfg(test)]
    pub(crate) fn loaded_content(&self, cx: &App) -> Option<String> {
        (!self.is_loading).then(|| self.editor.read(cx).value().to_string())
    }

    #[cfg(test)]
    pub(crate) fn replace_content_for_test(
        &mut self,
        content: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.suppress_editor_events = true;
        self.editor
            .update(cx, |editor, cx| editor.set_value(content, window, cx));
        self.suppress_editor_events = false;
        self.update_from_editor(cx);
    }

    pub(crate) fn apply_title(&mut self, title: &str, cx: &mut Context<Self>) {
        let title = title.trim();
        if title.is_empty() || self.title.as_ref() == title {
            return;
        }

        self.title = SharedString::from(title);
        cx.notify();
    }

    fn set_mode(&mut self, mode: EditorMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.kind != DocumentKind::Markdown {
            return;
        }
        self.mode = mode;
        self.focus_active_mode(window, cx);
        cx.notify();
    }

    fn toggle_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.kind != DocumentKind::Markdown {
            return;
        }
        self.mode = match self.mode {
            EditorMode::Source => EditorMode::Preview,
            EditorMode::Preview => EditorMode::Source,
        };
        self.focus_active_mode(window, cx);
        cx.notify();
    }

    fn focus_active_mode(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.mode {
            EditorMode::Source => {
                self.editor
                    .update(cx, |editor, cx| editor.focus(window, cx));
            }
            EditorMode::Preview => {
                self.focus_handle.focus(window, cx);
            }
        }
    }

    fn apply_document_kind(&mut self, path: Option<&Path>, cx: &mut Context<Self>) -> DocumentKind {
        let kind = DocumentKind::from_path(path);
        self.kind = kind;
        self.mode = if kind == DocumentKind::Markdown {
            EditorMode::from_str(AppSettings::markdown_editor_mode(cx).as_ref())
        } else {
            EditorMode::Source
        };
        self.outline_rendered = kind.supports_outline() && self.outline_visible;
        self.editor
            .update(cx, |editor, cx| editor.set_highlighter(kind.language(), cx));
        kind
    }

    fn toggle_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.kind.supports_outline() {
            return;
        }
        self.outline_visible = !self.outline_visible;
        self.outline_transition_epoch = self.outline_transition_epoch.saturating_add(1);
        let transition_epoch = self.outline_transition_epoch;

        if self.outline_visible {
            self.outline_rendered = true;
            self.schedule_document_analysis(false, cx);
            self.outline_focus_handle.focus(window, cx);
        } else {
            self.focus_active_mode(window, cx);
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

        AppSettings::set_document_outline_visible(self.outline_visible, cx);
        cx.notify();
    }

    fn select_outline_item(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.outline_rows.get(index).cloned() else {
            return;
        };
        if item.disabled {
            return;
        }

        self.outline_selected = Some(index);
        self.outline_navigation_generation = self.outline_navigation_generation.saturating_add(1);
        let navigation_generation = self.outline_navigation_generation;
        match self.mode {
            EditorMode::Source => {
                self.editor.update(cx, |editor, cx| {
                    let position = editor.text().offset_to_position(item.source_offset);
                    editor.set_cursor_position(position, window, cx);
                });
                self.show_outline_source_highlight(item.source_offset, navigation_generation, cx);
                self.align_source_heading_after_layout(navigation_generation, cx);
            }
            EditorMode::Preview => {
                self.outline_source_highlight = None;
                self._outline_source_highlight_task = None;
                if let Some(section) = item.preview_section_index {
                    self.preview_scroll_handle.scroll_to_item(section);
                }
            }
        }
        cx.notify();
    }

    fn show_outline_source_highlight(
        &mut self,
        source_offset: usize,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        self.outline_source_highlight = Some(OutlineSourceHighlight {
            generation,
            source_offset,
        });
        self._outline_source_highlight_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(OUTLINE_SOURCE_HIGHLIGHT_DURATION)
                .await;
            this.update(cx, |this, cx| {
                if this
                    .outline_source_highlight
                    .is_some_and(|highlight| highlight.generation == generation)
                {
                    this.outline_source_highlight = None;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    fn toggle_outline_node(&mut self, row_index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.outline_rows.get(row_index) else {
            return;
        };
        let Some(node_index) = row.node_index else {
            return;
        };
        let changed = if row.expanded {
            self.outline.collapse(node_index)
        } else {
            self.outline.expand(node_index)
        };
        if changed {
            self.rebuild_outline_rows();
            self.outline_selected = self
                .outline_rows
                .iter()
                .position(|candidate| candidate.node_index == Some(node_index));
            cx.notify();
        }
    }

    fn rebuild_outline_rows(&mut self) {
        self.outline_rows = Arc::new(self.outline.rows());
        if self.outline_rows.is_empty() {
            self.outline_selected = None;
        } else if let Some(selected) = self.outline_selected {
            self.outline_selected = Some(selected.min(self.outline_rows.len().saturating_sub(1)));
        }
    }

    fn schedule_document_analysis(&mut self, delayed: bool, cx: &mut Context<Self>) {
        self.analysis_generation = self.analysis_generation.saturating_add(1);
        let generation = self.analysis_generation;
        let kind = self.kind;
        let analyze_json_outline = self.outline_visible;
        let background = cx.background_executor().clone();

        self._analysis_task = Some(cx.spawn(async move |this, cx| {
            if delayed {
                cx.background_executor()
                    .timer(DOCUMENT_ANALYSIS_DELAY)
                    .await;
            }

            let content = this
                .read_with(cx, |this, cx| {
                    analysis_is_current(this.analysis_generation, this.kind, generation, kind)
                        .then(|| this.editor.read(cx).value().to_string())
                })
                .ok()
                .flatten();
            let Some(content) = content else {
                return;
            };

            let analysis = background
                .spawn(async move { analyze_document(kind, content, analyze_json_outline) })
                .await;
            this.update(cx, |this, cx| {
                if !analysis_is_current(this.analysis_generation, this.kind, generation, kind) {
                    return;
                }

                let mut outline = analysis.outline;
                outline.preserve_json_expansion_from(&this.outline);
                this.stats = analysis.stats;
                this.outline = outline;
                this.rebuild_outline_rows();
                let cursor_line = this.editor.read(cx).cursor_position().line as usize;
                if this.kind == DocumentKind::Markdown {
                    this.outline_selected =
                        this.outline.active_markdown_index_for_line(cursor_line);
                }
                cx.notify();
            })
            .ok();
        }));
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
                    cx.background_executor()
                        .timer(OUTLINE_SCROLL_LAYOUT_DELAY)
                        .await;
                    this.update(cx, |this, cx| {
                        if this.outline_navigation_generation == navigation_generation {
                            cx.notify();
                        }
                    })
                    .ok();
                    return;
                }
            }
        })
        .detach();
    }
}

fn analyze_document(
    kind: DocumentKind,
    content: String,
    analyze_json_outline: bool,
) -> DocumentAnalysis {
    let stats = DocumentStats::from_text(&content);
    let outline = match kind {
        DocumentKind::Markdown => DocumentOutline::Markdown(MarkdownOutline::parse(&content)),
        DocumentKind::Json if analyze_json_outline => {
            DocumentOutline::Json(JsonOutline::parse(&content))
        }
        DocumentKind::Json | DocumentKind::PlainText => DocumentOutline::None,
    };

    DocumentAnalysis { stats, outline }
}

fn analysis_is_current(
    current_generation: u64,
    current_kind: DocumentKind,
    analysis_generation: u64,
    analysis_kind: DocumentKind,
) -> bool {
    current_generation == analysis_generation && current_kind == analysis_kind
}

#[cfg(test)]
mod tests {
    use super::{
        DocumentEditorView, DocumentKind, DocumentOutline, analysis_is_current, analyze_document,
    };
    use crate::{DB, test_alloc};
    use entity::note;
    use gpui::AppContext as _;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};
    use std::{path::PathBuf, sync::Arc};

    #[gpui::test]
    fn delayed_analysis_does_not_copy_content_before_debounce(cx: &mut gpui::TestAppContext) {
        const CONTENT_BYTES: usize = 512 * 1024;
        const RESCHEDULES: usize = 8;

        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let (db, note_id) = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                let note = note::ActiveModel {
                    title: Set("Large analysis note".to_string()),
                    project_id: Set(None),
                    file_path: Set(None),
                    file_managed_by_app: Set(false),
                    cached_content: Set("x".repeat(CONTENT_BYTES)),
                    file_missing_since: Set(None),
                    created_at: Set(1),
                    updated_at: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>((db, note.id as u32))
            })
            .expect("analysis test database should initialize");
        let settings_dir = std::env::temp_dir().join(format!(
            "castle-analysis-allocation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after the Unix epoch")
                .as_nanos()
        ));
        let mut editor_view = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(crate::app_settings::AppSettings::load(settings_dir));
            cx.set_global(DB {
                conn: Arc::new(db),
                data_dir: PathBuf::new(),
            });
            cx.open_window(Default::default(), |window, cx| {
                let view = DocumentEditorView::view(note_id, window, cx);
                editor_view = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("analysis test window should open")
        });
        let view = editor_view.expect("document editor should exist");
        let _window = window;

        for _ in 0..100 {
            cx.run_until_parked();
            if view
                .read_with(cx, |editor, cx| editor.loaded_content(cx))
                .is_some()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let legacy_allocation = test_alloc::start_measurement();
        let legacy_started = std::time::Instant::now();
        for _ in 0..RESCHEDULES {
            std::hint::black_box(
                view.read_with(cx, |editor, cx| editor.editor.read(cx).value().to_string()),
            );
        }
        let legacy_elapsed = legacy_started.elapsed();
        let legacy_allocation = legacy_allocation.finish();

        let allocation = test_alloc::start_measurement();
        let optimized_started = std::time::Instant::now();
        for _ in 0..RESCHEDULES {
            view.update(cx, |editor, cx| {
                editor.schedule_document_analysis(true, cx);
            });
        }
        let optimized_elapsed = optimized_started.elapsed();
        let allocation = allocation.finish();

        assert!(
            allocation.allocated_bytes < legacy_allocation.allocated_bytes / 100,
            "delayed analysis allocated {} bytes versus {} bytes for eager snapshots",
            allocation.allocated_bytes,
            legacy_allocation.allocated_bytes
        );
        println!(
            "document_bytes={CONTENT_BYTES} reschedules={RESCHEDULES} eager_snapshot_micros={} eager_snapshot_allocated_bytes={} delayed_schedule_micros={} delayed_schedule_peak_heap_growth_bytes={} delayed_schedule_retained_heap_growth_bytes={} delayed_schedule_allocated_bytes={}",
            legacy_elapsed.as_micros(),
            legacy_allocation.allocated_bytes,
            optimized_elapsed.as_micros(),
            allocation.peak_growth_bytes,
            allocation.retained_growth_bytes,
            allocation.allocated_bytes
        );
    }

    #[test]
    fn large_plain_text_analysis_only_computes_statistics() {
        let content = "plain text without parser work\n".repeat(100_000);
        let analysis = analyze_document(DocumentKind::PlainText, content, true);

        assert!(matches!(analysis.outline, DocumentOutline::None));
        assert_eq!(analysis.stats.lines, 100_000);
    }

    #[test]
    fn hidden_json_outline_skips_json_parsing() {
        let analysis = analyze_document(DocumentKind::Json, "{ malformed".to_string(), false);
        assert!(matches!(analysis.outline, DocumentOutline::None));
    }

    #[test]
    fn stale_or_reclassified_analysis_is_rejected() {
        assert!(analysis_is_current(
            4,
            DocumentKind::Json,
            4,
            DocumentKind::Json
        ));
        assert!(!analysis_is_current(
            5,
            DocumentKind::Json,
            4,
            DocumentKind::Json
        ));
        assert!(!analysis_is_current(
            4,
            DocumentKind::PlainText,
            4,
            DocumentKind::Json
        ));
    }
}
