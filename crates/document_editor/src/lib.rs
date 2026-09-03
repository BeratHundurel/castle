pub mod action;
mod action_handlers;
mod attachments;
mod board_embeds;
mod document_state;
mod emmet;
mod file_paths;
mod formatting;
pub mod links;
mod mermaid;
mod outline;
mod persistence;
mod smart_editing;
mod state;
mod view;
mod vim;

use gpui::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, HighlightStyle, Pixels,
    SharedString, Subscription, Task, UniformListScrollHandle, Window, point, px,
};
use gpui_component::{
    Theme,
    highlighter::Language,
    input::{EditorState, InputEvent, InputState, Rope, RopeExt as _, TabSize, TextDecoration},
};
use std::{
    cell::Cell,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use document_state::*;
use outline::{DocumentOutline, JsonOutline, MarkdownOutline, OutlineRow};
use settings::AppSettings;
pub use state::*;
use vim::VimState;

pub use document_state::{DEFAULT_NOTE, DocumentStats, SaveState};
pub use file_paths::unique_note_path;
pub use workspace::DocumentKind;

const AUTO_SAVE_IDLE_DELAY: Duration = Duration::from_millis(1_200);
const DOCUMENT_ANALYSIS_DELAY: Duration = Duration::from_millis(180);
const VIEW_LAYOUT_REFRESH_DELAY: Duration = Duration::from_millis(100);
const OUTLINE_SCROLL_LAYOUT_DELAY: Duration = Duration::from_millis(16);
const OUTLINE_SCROLL_ATTEMPTS: usize = 4;
const OUTLINE_TRANSITION_DURATION: Duration = Duration::from_millis(160);
const OUTLINE_SOURCE_HIGHLIGHT_DURATION: Duration = Duration::from_millis(1_400);
const OUTLINE_DEFAULT_WIDTH: Pixels = px(224.);
const OUTLINE_MIN_WIDTH: Pixels = px(176.);
const OUTLINE_MAX_WIDTH: Pixels = px(480.);
const EDITOR_MIN_WIDTH_WITH_OUTLINE: Pixels = px(360.);
const OUTLINE_INDENT_STEP: Pixels = px(8.);
const TYPEWRITER_SCROLL_MARGIN_LINES: usize = usize::MAX;
const FOCUS_MODE_FADE: f32 = 0.72;

pub struct DocumentEditorView {
    note_id: u32,
    title: SharedString,
    focus_handle: FocusHandle,
    editor: Entity<EditorState>,
    kind: DocumentKind,
    mode: EditorMode,
    vim_state: VimSessionState,
    writing: WritingExperienceState,
    zen: ZenModeState,
    persistence: PersistenceState,
    analysis: AnalysisState,
    emmet_input: Entity<InputState>,
    show_emmet_input: bool,
    emmet_replacement_range: Option<Range<usize>>,
    inspector_links: InspectorLinksState,
    embeds: EmbedStateGroup,
    mermaid: mermaid::MermaidState,
    _theme_subscription: Subscription,
    _settings_subscription: Subscription,
    pending_navigation_offset: Option<usize>,
    view_width: gpui::Pixels,
    view_bounds: Option<Bounds<Pixels>>,
    view_layout_refresh_task: Option<Task<()>>,
    view_layout_refresh_epoch: u64,
    outline_width: Pixels,
}

impl EventEmitter<DocumentEditorEvent> for DocumentEditorView {}

impl DocumentEditorView {
    pub fn view(note_id: u32, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(note_id, window, cx))
    }

    fn new(note_id: u32, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let line_numbers = AppSettings::editor_line_numbers(cx);
        let soft_wrap = AppSettings::editor_soft_wrap(cx);
        let outline_visible = AppSettings::document_outline_visible(cx);
        let preview_font_size_bits = AppSettings::markdown_preview_font_size(cx).to_bits();
        let vim_enabled = AppSettings::editor_vim_mode(cx);
        let focus_mode = AppSettings::editor_focus_mode(cx);
        let typewriter_scrolling = AppSettings::editor_typewriter_scrolling(cx);
        let wikilink_completion_provider =
            links::WorkspaceReferenceCompletionProvider::new(note_id as i64);
        let input_completion_provider = std::rc::Rc::new(wikilink_completion_provider.clone());

        let editor = cx.new(|cx| {
            let mut editor = EditorState::new(window, cx)
                .language(Language::Plain)
                .scroll_beyond_last_line(if typewriter_scrolling { None } else { Some(1) })
                .cursor_surrounding_lines(
                    typewriter_scrolling.then_some(TYPEWRITER_SCROLL_MARGIN_LINES),
                )
                .line_number(line_numbers)
                .indent_guides(false)
                .tab_size(TabSize {
                    tab_size: 2,
                    ..Default::default()
                })
                .soft_wrap(soft_wrap)
                .searchable(true)
                .placeholder("Start typing...")
                .default_value("");
            editor.lsp_mut().completion_provider = Some(input_completion_provider);
            editor
        });

        let emmet_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Enter Emmet abbreviation (e.g. details>summary)")
        });

        let focus_handle = cx.focus_handle();
        let outline_focus_handle = cx.focus_handle();
        let focus_decorations = editor.update(cx, |editor, cx| {
            editor.create_decorations_collection(Vec::new(), cx)
        });
        let theme_subscription = cx.observe_global::<Theme>(|this, cx| {
            if this.kind == DocumentKind::Markdown && this.mode.shows_preview() {
                this.activate_mermaids(cx);
            }
        });
        let settings_subscription = cx
            .observe_global_in::<AppSettings>(window, |this, window, cx| {
                this.sync_writing_preferences(window, cx)
            });
        cx.observe(&editor, |this, _, cx| {
            this.refresh_focus_decorations(cx);
        })
        .detach();
        cx.on_release(|this, cx| this.mermaid.clear(cx)).detach();
        cx.subscribe_in(
            &editor,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    if !this.persistence.suppress_editor_events {
                        this.update_from_editor(cx);
                    }
                }
                InputEvent::PressEnter { .. }
                    if !this.persistence.suppress_editor_events
                        && this.kind == DocumentKind::Markdown =>
                {
                    this.continue_markdown_after_enter(window, cx);
                }
                _ => {}
            },
        )
        .detach();

        let load_task = Self::load_note_async(note_id, window, cx);
        let note_links_task = Self::load_note_links_async(note_id, 1, cx);

        Self {
            note_id,
            title: "Untitled note".into(),
            focus_handle,
            editor,
            kind: DocumentKind::PlainText,
            mode: EditorMode::Source,
            vim_state: VimSessionState {
                state: VimState::new(vim_enabled),
                search_active: false,
            },
            writing: WritingExperienceState {
                focus_mode,
                typewriter_scrolling,
                focused_range: None,
                focus_decorations,
            },
            zen: ZenModeState {
                enabled: false,
                show_status_bar: false,
                show_outline: false,
            },
            persistence: PersistenceState {
                current_path: None,
                file_managed_by_app: false,
                save_state: SaveState::Saved,
                load_error: None,
                is_loading: true,
                suppress_editor_events: false,
                auto_save_epoch: 0,
                load_task: Some(load_task),
                auto_save_task: None,
                format_task: None,
            },
            analysis: AnalysisState {
                stats: DocumentStats::from_text(""),
                request: workspace::RequestTracker::default(),
                source_bounds: None,
                outline: DocumentOutline::None,
                outline_rows: Arc::new(Vec::new()),
                outline_visible,
                outline_rendered: false,
                outline_transition_epoch: 0,
                outline_selected: None,
                outline_navigation_generation: 0,
                outline_source_highlight: None,
                outline_source_highlight_task: None,
                source_bounds_mode: None,
                preview_bounds: None,
                preview_bounds_mode: None,
                preview_sections: Arc::new(Vec::new()),
                preview_list_state: gpui::ListState::new(
                    0,
                    gpui::ListAlignment::Top,
                    gpui::px(2_048.),
                )
                .measure_all(),
                preview_font_size_bits: Cell::new(preview_font_size_bits),
                outline_scroll_handle: UniformListScrollHandle::default(),
                outline_focus_handle,
            },
            emmet_input,
            show_emmet_input: false,
            emmet_replacement_range: None,
            inspector_links: InspectorLinksState {
                tab: DocumentInspectorTab::Outline,
                note_links: Arc::new(storage::note::links::NoteLinkSet::default()),
                note_catalog: Arc::new(Vec::new()),
                workspace_links: Arc::new(storage::workspace::links::NoteWorkspaceLinks::default()),
                workspace_catalog: Arc::new(Default::default()),
                relation_signature: Vec::new(),
                project_id: None,
                completion_provider: wikilink_completion_provider,
                loading: true,
                error: None,
                request: workspace::RequestTracker::with_task(1, note_links_task),
            },
            embeds: EmbedStateGroup {
                states: Arc::new(std::collections::HashMap::new()),
                request: workspace::RequestTracker::default(),
                loading_keys: std::collections::HashSet::new(),
                refresh_task: None,
                refresh_epoch: 0,
            },
            mermaid: mermaid::MermaidState::default(),
            _theme_subscription: theme_subscription,
            _settings_subscription: settings_subscription,
            pending_navigation_offset: None,
            view_width: gpui::px(0.),
            view_bounds: None,
            view_layout_refresh_task: None,
            view_layout_refresh_epoch: 0,
            outline_width: OUTLINE_DEFAULT_WIDTH,
        }
    }

    pub fn save_state(&self) -> SaveState {
        self.persistence.save_state.clone()
    }

    pub fn reload_after_external_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.persistence.save_state != SaveState::Saved {
            return;
        }

        self.persistence.auto_save_epoch = self.persistence.auto_save_epoch.saturating_add(1);
        self.persistence.is_loading = true;
        self.persistence.load_task = Some(Self::load_note_async(self.note_id, window, cx));
        self.refresh_note_links(cx);
        cx.notify();
    }

    pub fn kind(&self) -> DocumentKind {
        self.kind
    }

    pub fn insert_text_at_selection(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.editor.read(cx).selected_range();
        self.editor.update(cx, |editor, cx| {
            let rope = editor.text();
            let start = rope.offset_to_offset_utf16(range.start);
            let end = rope.offset_to_offset_utf16(range.end);
            gpui::EntityInputHandler::replace_text_in_range(
                editor,
                Some(start..end),
                text,
                window,
                cx,
            );
            editor.focus(window, cx);
        });
    }

    pub(crate) fn select_source_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            let end = range.end.min(editor.text().len());
            let start = range.start.min(end);
            editor.set_selected_range(start..end, cx);
            editor.focus(window, cx);
        });
    }

    pub(crate) fn replace_source_range(
        &mut self,
        range: Range<usize>,
        replacement: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_source_range(range, window, cx);
        self.insert_text_at_selection(replacement, window, cx);
    }

    #[doc(hidden)]
    pub fn loaded_content(&self, cx: &App) -> Option<String> {
        (!self.persistence.is_loading).then(|| self.editor.read(cx).value().to_string())
    }

    #[doc(hidden)]
    pub fn replace_content_for_test(
        &mut self,
        content: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.persistence.suppress_editor_events = true;
        self.editor
            .update(cx, |editor, cx| editor.set_value(content, window, cx));
        self.persistence.suppress_editor_events = false;
        self.update_from_editor(cx);
    }

    pub fn apply_title(&mut self, title: &str, cx: &mut Context<Self>) {
        let title = title.trim();
        if title.is_empty() || self.title.as_ref() == title {
            return;
        }

        self.title = SharedString::from(title);
        cx.notify();
    }

    pub fn apply_file_path(&mut self, file_path: Option<String>, cx: &mut Context<Self>) {
        let file_path = file_path.map(PathBuf::from);
        if self.persistence.current_path == file_path {
            return;
        }

        self.persistence.current_path = file_path;
        let current_path = self.persistence.current_path.clone();
        self.apply_document_kind(current_path.as_deref(), cx);
        cx.emit(DocumentEditorEvent::PathChanged);
        cx.notify();
    }

    pub fn navigate_to_offset(
        &mut self,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.persistence.is_loading {
            self.pending_navigation_offset = Some(offset);
            return;
        }
        self.editor.update(cx, |editor, cx| {
            let offset = offset.min(editor.text().len());
            let position = editor.text().offset_to_position(offset);
            editor.set_cursor_position(position, window, cx);
        });
    }

    fn set_mode(&mut self, mode: EditorMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.kind != DocumentKind::Markdown {
            return;
        }
        self.mode = mode;
        if mode.shows_preview() {
            self.reset_vim_command();
            self.activate_mermaids(cx);
        } else {
            self.deactivate_mermaids(cx);
        }
        self.focus_active_mode(window, cx);
        cx.notify();
    }

    fn toggle_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.kind != DocumentKind::Markdown {
            return;
        }
        self.mode = match self.mode {
            EditorMode::Source => EditorMode::Preview,
            EditorMode::Split => EditorMode::Source,
            EditorMode::Preview => EditorMode::Split,
        };
        if self.mode.shows_preview() {
            self.reset_vim_command();
            self.activate_mermaids(cx);
        } else {
            self.deactivate_mermaids(cx);
        }
        self.focus_active_mode(window, cx);
        cx.notify();
    }

    fn focus_active_mode(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.mode {
            EditorMode::Source | EditorMode::Split => self.focus_source_mode(window, cx),
            EditorMode::Preview => {
                self.focus_handle.focus(window, cx);
            }
        }
    }

    fn sync_writing_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_focus_mode(AppSettings::editor_focus_mode(cx), cx);
        self.apply_typewriter_scrolling(AppSettings::editor_typewriter_scrolling(cx), window, cx);
    }

    fn apply_focus_mode(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.writing.focus_mode == enabled {
            return;
        }
        self.writing.focus_mode = enabled;
        self.writing.focused_range = None;
        if enabled {
            self.refresh_focus_decorations(cx);
        } else {
            self.writing.focus_decorations.clear(cx);
        }
        cx.notify();
    }

    fn toggle_focus_mode(&mut self, cx: &mut Context<Self>) {
        let enabled = !self.writing.focus_mode;
        self.apply_focus_mode(enabled, cx);
        AppSettings::set_editor_focus_mode(enabled, cx);
    }

    pub fn is_zen_mode(&self) -> bool {
        self.zen.enabled
    }

    pub(crate) fn effective_outline_rendered(&self) -> bool {
        self.zen.outline_wanted(self.analysis.outline_rendered)
            && (!self.zen.enabled || self.kind.supports_outline())
    }

    pub fn exit_zen_mode(&mut self, cx: &mut Context<Self>) {
        if !self.zen.enabled {
            return;
        }
        self.set_zen_mode(false, cx);
    }

    fn set_zen_mode(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.zen.enabled == enabled {
            return;
        }
        self.zen.enabled = enabled;
        self.zen.show_status_bar = false;
        self.zen.show_outline = false;
        cx.notify();
    }

    fn toggle_zen_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = !self.zen.enabled;
        self.set_zen_mode(enabled, cx);
        if enabled {
            self.focus_active_mode(window, cx);
        }
    }

    fn toggle_zen_status_bar(&mut self, cx: &mut Context<Self>) {
        if !self.zen.enabled {
            return;
        }
        self.zen.show_status_bar = !self.zen.show_status_bar;
        cx.notify();
    }

    fn toggle_zen_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.zen.enabled {
            return;
        }
        if !self.kind.supports_outline() {
            return;
        }
        self.zen.show_outline = !self.zen.show_outline;
        if self.zen.show_outline {
            self.schedule_document_analysis(false, cx);
            self.analysis.outline_focus_handle.focus(window, cx);
        } else {
            self.focus_active_mode(window, cx);
        }
        cx.notify();
    }

    fn refresh_focus_decorations(&mut self, cx: &mut Context<Self>) {
        if !self.writing.focus_mode {
            if self.writing.focused_range.take().is_some() {
                self.writing.focus_decorations.clear(cx);
            }
            return;
        }

        let selection = self
            .vim_visual_range(cx)
            .unwrap_or_else(|| self.editor.read(cx).selected_range());
        let (focused_range, text_len) = {
            let editor = self.editor.read(cx);
            (
                focused_paragraph_range(editor.text(), selection),
                editor.text().len(),
            )
        };
        if self.writing.focused_range.as_ref() == Some(&focused_range) {
            return;
        }

        let style = HighlightStyle {
            fade_out: Some(FOCUS_MODE_FADE),
            ..Default::default()
        };
        let mut decorations = Vec::with_capacity(2);
        if focused_range.start > 0 {
            decorations.push(TextDecoration::new(0..focused_range.start, style));
        }
        if focused_range.end < text_len {
            decorations.push(TextDecoration::new(focused_range.end..text_len, style));
        }
        self.writing.focused_range = Some(focused_range);
        self.writing.focus_decorations.set(decorations, cx);
    }

    fn apply_typewriter_scrolling(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.writing.typewriter_scrolling == enabled {
            return;
        }
        self.writing.typewriter_scrolling = enabled;
        self.editor.update(cx, |editor, cx| {
            editor.set_scroll_beyond_last_line(if enabled { None } else { Some(1) }, window, cx);
            editor.set_cursor_surrounding_lines(
                enabled.then_some(TYPEWRITER_SCROLL_MARGIN_LINES),
                window,
                cx,
            );
        });
        cx.notify();
    }

    fn toggle_typewriter_scrolling(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = !self.writing.typewriter_scrolling;
        self.apply_typewriter_scrolling(enabled, window, cx);
        AppSettings::set_editor_typewriter_scrolling(enabled, cx);
    }

    fn apply_document_kind(&mut self, path: Option<&Path>, cx: &mut Context<Self>) -> DocumentKind {
        let Some(kind) = changed_document_kind(self.kind, path) else {
            return self.kind;
        };

        self.kind = kind;
        self.mode = if kind == DocumentKind::Markdown {
            EditorMode::from_key(AppSettings::markdown_editor_mode(cx).as_ref())
        } else {
            EditorMode::Source
        };
        self.analysis.outline_rendered = kind.supports_outline() && self.analysis.outline_visible;
        self.editor.update(cx, |editor, cx| {
            editor.set_highlighter(document_language(kind), cx);
            editor.refresh(cx);
        });
        kind
    }

    fn toggle_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.kind.supports_outline() {
            return;
        }
        if self.zen.enabled {
            self.toggle_zen_outline(window, cx);
            return;
        }
        self.analysis.outline_visible = !self.analysis.outline_visible;
        self.analysis.outline_transition_epoch =
            self.analysis.outline_transition_epoch.saturating_add(1);
        let transition_epoch = self.analysis.outline_transition_epoch;
        let outline_visible = self.analysis.outline_visible;

        if outline_visible {
            self.analysis.outline_rendered = true;
            self.schedule_document_analysis(false, cx);
            self.analysis.outline_focus_handle.focus(window, cx);
        } else {
            self.focus_active_mode(window, cx);
        }

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(OUTLINE_TRANSITION_DURATION)
                .await;
            this.update(cx, |this, cx| {
                if this.analysis.outline_transition_epoch != transition_epoch
                    || this.analysis.outline_visible != outline_visible
                {
                    return;
                }
                if !outline_visible {
                    this.analysis.outline_rendered = false;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();

        AppSettings::set_document_outline_visible(self.analysis.outline_visible, cx);
        cx.notify();
    }

    fn schedule_view_layout_refresh(&mut self, cx: &mut Context<Self>) {
        self.view_layout_refresh_epoch = self.view_layout_refresh_epoch.saturating_add(1);
        let epoch = self.view_layout_refresh_epoch;
        self.view_layout_refresh_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(VIEW_LAYOUT_REFRESH_DELAY)
                .await;
            this.update(cx, |this, cx| {
                if this.view_layout_refresh_epoch != epoch {
                    return;
                }
                this.view_layout_refresh_task = None;
                cx.notify();
            })
            .ok();
        }));
    }

    fn select_outline_item(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.analysis.outline_rows.get(index).cloned() else {
            return;
        };
        if item.disabled {
            return;
        }

        self.analysis.outline_selected = Some(index);
        self.analysis.outline_navigation_generation = self
            .analysis
            .outline_navigation_generation
            .saturating_add(1);
        let navigation_generation = self.analysis.outline_navigation_generation;
        match self.mode {
            EditorMode::Source | EditorMode::Split => {
                let source_bounds = self.analysis.source_bounds;
                let centered_at_document_start = self.editor.update(cx, |editor, cx| {
                    let position = editor.text().offset_to_position(item.source_offset);
                    let centers_at_document_start = source_bounds.is_some_and(|source_bounds| {
                        source_row_centers_at_document_start(
                            editor.line_height(),
                            source_bounds.size.height,
                            position.line as usize,
                        )
                    });
                    editor.set_cursor_position(position, window, cx);
                    if centers_at_document_start {
                        let current = editor.scroll_offset();
                        editor.set_scroll_offset(point(current.x, px(0.)), cx);
                    }
                    centers_at_document_start
                });
                self.show_outline_source_highlight(item.source_offset, navigation_generation, cx);
                if !centered_at_document_start {
                    self.align_source_heading_after_layout(navigation_generation, cx);
                }
                if self.mode == EditorMode::Split
                    && let Some(section) = item.preview_section_index
                {
                    self.analysis
                        .preview_list_state
                        .scroll_to(gpui::ListOffset {
                            item_ix: section,
                            offset_in_item: gpui::px(0.),
                        });
                }
            }
            EditorMode::Preview => {
                self.analysis.outline_source_highlight = None;
                self.analysis.outline_source_highlight_task = None;
                if let Some(section) = item.preview_section_index {
                    self.analysis
                        .preview_list_state
                        .scroll_to(gpui::ListOffset {
                            item_ix: section,
                            offset_in_item: gpui::px(0.),
                        });
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
        self.analysis.outline_source_highlight = Some(OutlineSourceHighlight {
            generation,
            source_offset,
        });
        self.analysis.outline_source_highlight_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(OUTLINE_SOURCE_HIGHLIGHT_DURATION)
                .await;
            this.update(cx, |this, cx| {
                if this
                    .analysis
                    .outline_source_highlight
                    .is_some_and(|highlight| highlight.generation == generation)
                {
                    this.analysis.outline_source_highlight = None;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    fn toggle_outline_node(&mut self, row_index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.analysis.outline_rows.get(row_index) else {
            return;
        };
        let Some(node_index) = row.node_index else {
            return;
        };
        let changed = if row.expanded {
            self.analysis.outline.collapse(node_index)
        } else {
            self.analysis.outline.expand(node_index)
        };
        if changed {
            self.rebuild_outline_rows();
            self.analysis.outline_selected = self
                .analysis
                .outline_rows
                .iter()
                .position(|candidate| candidate.node_index == Some(node_index));
            cx.notify();
        }
    }

    fn set_all_outline_nodes_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        let selected_node = self
            .analysis
            .outline_selected
            .and_then(|index| self.analysis.outline_rows.get(index))
            .and_then(|row| row.node_index);
        let changed = if expanded {
            self.analysis.outline.expand_all()
        } else {
            self.analysis.outline.collapse_all()
        };
        if !changed {
            return;
        }

        let selected_node = selected_node.and_then(|node_index| {
            expanded
                .then_some(node_index)
                .or_else(|| self.analysis.outline.root_node_index(node_index))
        });
        self.rebuild_outline_rows();
        self.analysis.outline_selected = selected_node.and_then(|node_index| {
            self.analysis
                .outline_rows
                .iter()
                .position(|row| row.node_index == Some(node_index))
        });
        if let Some(index) = self.analysis.outline_selected {
            self.analysis
                .outline_scroll_handle
                .scroll_to_item(index, gpui::ScrollStrategy::Top);
        }
        cx.notify();
    }

    fn rebuild_outline_rows(&mut self) {
        self.analysis.outline_rows = Arc::new(self.analysis.outline.rows());
        if self.analysis.outline_rows.is_empty() {
            self.analysis.outline_selected = None;
        } else if let Some(selected) = self.analysis.outline_selected {
            self.analysis.outline_selected =
                Some(selected.min(self.analysis.outline_rows.len().saturating_sub(1)));
        }
    }

    fn schedule_document_analysis(&mut self, delayed: bool, cx: &mut Context<Self>) {
        let generation = self.analysis.request.begin();
        let kind = self.kind;
        let analyze_json_outline = self.analysis.outline_visible;
        let background = cx.background_executor().clone();

        let task = cx.spawn(async move |this, cx| {
            if delayed {
                cx.background_executor()
                    .timer(DOCUMENT_ANALYSIS_DELAY)
                    .await;
            }

            let content = this
                .read_with(cx, |this, cx| {
                    analysis_is_current(
                        this.analysis.request.generation(),
                        this.kind,
                        generation,
                        kind,
                    )
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
                if !analysis_is_current(
                    this.analysis.request.generation(),
                    this.kind,
                    generation,
                    kind,
                ) {
                    return;
                }

                let mut outline = analysis.outline;
                outline.preserve_json_expansion_from(&this.analysis.outline);
                this.analysis.stats = analysis.stats;
                this.analysis.outline = outline;
                this.analysis.preview_sections = analysis.preview_sections;
                this.mermaid.set_analyzed(analysis.mermaids);
                this.rebuild_outline_rows();
                this.analysis.preview_list_state.remeasure();
                let cursor_line = this.editor.read(cx).cursor_position().line as usize;
                if this.kind == DocumentKind::Markdown {
                    this.analysis.outline_selected = this
                        .analysis
                        .outline
                        .active_markdown_index_for_line(cursor_line);
                    if this.mode.shows_preview() {
                        this.activate_mermaids(cx);
                    }
                }
                cx.notify();
            })
            .ok();
        });
        self.analysis.request.set_task(task);
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
                        if this.analysis.outline_navigation_generation != navigation_generation {
                            return true;
                        }

                        let Some(source_bounds) = this.analysis.source_bounds else {
                            return false;
                        };

                        this.editor.update(cx, |editor, cx| {
                            let cursor = editor.cursor();
                            let cursor_row = editor.text().offset_to_point(cursor).row;
                            if !row_is_in_visible_layout(editor.visible_row_range(), cursor_row) {
                                return false;
                            }
                            let Some(cursor_bounds) = editor.range_to_bounds(&(cursor..cursor))
                            else {
                                return false;
                            };

                            let current = editor.scroll_offset();
                            let cursor_offset = cursor_bounds.origin.y
                                - source_bounds.origin.y
                                - source_bounds.size.height / 2.;
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
                        if this.analysis.outline_navigation_generation == navigation_generation {
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

fn changed_document_kind(current: DocumentKind, path: Option<&Path>) -> Option<DocumentKind> {
    let kind = DocumentKind::from_path(path);
    (kind != current).then_some(kind)
}

fn document_language(kind: DocumentKind) -> Language {
    match kind {
        DocumentKind::Markdown => Language::Markdown,
        DocumentKind::Json => Language::Json,
        DocumentKind::PlainText => Language::Plain,
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
    let mermaids = if kind == DocumentKind::Markdown {
        mermaid::parse_mermaid_blocks(&content)
    } else {
        Vec::new()
    };
    let preview_sections = match &outline {
        DocumentOutline::Markdown(_) => {
            let sections = if outline.markdown_sections().is_empty() {
                vec![SharedString::from(content.as_str())]
            } else {
                outline.markdown_sections().to_vec()
            };
            Arc::new(view::prepare_markdown_preview_sections(&content, sections))
        }
        DocumentOutline::None | DocumentOutline::Json(_) => Arc::new(Vec::new()),
    };

    DocumentAnalysis {
        stats,
        outline,
        mermaids,
        preview_sections,
    }
}

fn analysis_is_current(
    current_generation: u64,
    current_kind: DocumentKind,
    analysis_generation: u64,
    analysis_kind: DocumentKind,
) -> bool {
    current_generation == analysis_generation && current_kind == analysis_kind
}

fn row_is_in_visible_layout(visible_rows: Option<Range<usize>>, row: usize) -> bool {
    visible_rows.is_some_and(|visible_rows| visible_rows.contains(&row))
}

fn source_row_centers_at_document_start(
    line_height: Option<Pixels>,
    viewport_height: Pixels,
    row: usize,
) -> bool {
    line_height.is_some_and(|line_height| {
        line_height * row.saturating_add(1) as f32 <= viewport_height / 2.
    })
}

fn focused_paragraph_range(text: &Rope, selection: Range<usize>) -> Range<usize> {
    if text.len() == 0 {
        return 0..0;
    }

    let start_offset = selection.start.min(text.len());
    let end_offset = if selection.is_empty() {
        start_offset
    } else {
        selection.end.min(text.len()).saturating_sub(1)
    };
    let mut start_row = text.offset_to_point(start_offset).row;
    let mut end_row = text.offset_to_point(end_offset).row;
    let line_is_blank = |row| text.slice_line(row).chars().all(char::is_whitespace);

    if !line_is_blank(start_row) {
        while start_row > 0 && !line_is_blank(start_row - 1) {
            start_row -= 1;
        }
    }
    if !line_is_blank(end_row) {
        while end_row + 1 < text.lines_len() && !line_is_blank(end_row + 1) {
            end_row += 1;
        }
    }

    let end = if end_row + 1 < text.lines_len() {
        text.line_start_offset(end_row + 1)
    } else {
        text.len()
    };
    text.line_start_offset(start_row)..end
}

#[cfg(test)]
mod tests {
    use super::{
        DocumentEditorView, DocumentKind, DocumentOutline, JsonOutline,
        OUTLINE_SCROLL_LAYOUT_DELAY, analysis_is_current, analyze_document, changed_document_kind,
        document_language, focused_paragraph_range, row_is_in_visible_layout,
        source_row_centers_at_document_start,
    };
    use entity::note;
    use gpui::AppContext as _;
    use gpui_component::highlighter::Language;
    use migration::{Migrator, MigratorTrait};
    use runtime::AppRuntime;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};
    use settings::AppSettings;
    use std::{path::PathBuf, sync::Arc, time::Duration};
    use test_support as test_alloc;

    #[gpui::test]
    fn json_autosave_preserves_unformatted_content(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();

        let directory = tempfile::tempdir().expect("test directory should be created");
        let document_path = directory.path().join("autosave.json");
        std::fs::write(&document_path, "{}\n").expect("test document should be created");
        let db = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                Ok::<_, anyhow::Error>(db)
            })
            .expect("autosave test database should initialize");
        let note_id = runtime
            .block_on(async {
                Ok::<_, anyhow::Error>(
                    note::ActiveModel {
                        title: Set("Autosave JSON".to_string()),
                        project_id: Set(None),
                        file_path: Set(Some(document_path.display().to_string())),
                        file_managed_by_app: Set(false),
                        cached_content: Set("{}\n".to_string()),
                        file_missing_since: Set(None),
                        created_at: Set(1),
                        updated_at: Set(1),
                        ..Default::default()
                    }
                    .insert(&db)
                    .await?
                    .id as u32,
                )
            })
            .expect("autosave test note should be created");
        let db = Arc::new(db);
        let mut editor_view = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(AppSettings::load(directory.path()));
            cx.set_global(AppRuntime::new(db.clone(), directory.path().to_path_buf()));
            cx.open_window(Default::default(), |window, cx| {
                let view = DocumentEditorView::view(note_id, window, cx);
                editor_view = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("autosave test window should open")
        });
        let view = editor_view.expect("document editor should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

        for _ in 0..100 {
            cx.run_until_parked();
            if view.read_with(&cx, |editor, _| !editor.persistence.is_loading) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.replace_content_for_test(r#"{"alpha":1}"#, window, cx);
            });
        });
        cx.executor().advance_clock(Duration::from_millis(1_300));
        for _ in 0..100 {
            cx.run_until_parked();
            if std::fs::read_to_string(&document_path)
                .is_ok_and(|content| content == r#"{"alpha":1}"#)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(
            view.read_with(&cx, |editor, cx| editor.loaded_content(cx)),
            Some(r#"{"alpha":1}"#.to_string())
        );
        assert_eq!(
            std::fs::read_to_string(&document_path).expect("autosaved document should be readable"),
            r#"{"alpha":1}"#
        );
    }

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
            cx.set_global(AppSettings::load(settings_dir));
            cx.set_global(AppRuntime::new(Arc::new(db), PathBuf::new()));
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
    fn markdown_analysis_keeps_a_preview_block_without_headings() {
        let analysis = analyze_document(DocumentKind::Markdown, "A paragraph".to_string(), false);

        assert_eq!(analysis.preview_sections.len(), 1);
        assert_eq!(analysis.preview_sections[0], "A paragraph");
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

    #[test]
    fn outline_navigation_waits_for_the_destination_row_layout() {
        assert!(!row_is_in_visible_layout(Some(20..40), 5));
        assert!(row_is_in_visible_layout(Some(20..40), 20));
        assert!(row_is_in_visible_layout(Some(20..40), 39));
        assert!(!row_is_in_visible_layout(Some(20..40), 40));
        assert!(!row_is_in_visible_layout(None, 20));
    }

    #[test]
    fn source_rows_in_the_top_half_viewport_clamp_to_document_start() {
        assert!(source_row_centers_at_document_start(
            Some(gpui::px(20.)),
            gpui::px(400.),
            8
        ));
        assert!(source_row_centers_at_document_start(
            Some(gpui::px(20.)),
            gpui::px(400.),
            9
        ));
        assert!(!source_row_centers_at_document_start(
            Some(gpui::px(20.)),
            gpui::px(400.),
            10
        ));
        assert!(!source_row_centers_at_document_start(
            None,
            gpui::px(400.),
            0
        ));
    }

    #[test]
    fn focus_mode_tracks_the_paragraph_containing_the_cursor() {
        let text = gpui_component::input::Rope::from(
            "First paragraph\ncontinues here\n\nSecond paragraph with café\ncontinues too\n",
        );
        let cursor = "First paragraph\ncontinues here\n\nSecond paragraph with ca".len();

        assert_eq!(
            focused_paragraph_range(&text, cursor..cursor),
            "First paragraph\ncontinues here\n\n".len()..text.len()
        );
    }

    #[test]
    fn focus_mode_keeps_every_paragraph_touched_by_a_selection() {
        let source = "One\n\nTwo\ncontinued\n\nThree";
        let text = gpui_component::input::Rope::from(source);
        let selection = source.find("ne").expect("selection start should exist")
            ..source
                .find("continued")
                .expect("selection end should exist")
                + "continued".len();

        assert_eq!(
            focused_paragraph_range(&text, selection),
            0.."One\n\nTwo\ncontinued\n".len()
        );
    }

    #[test]
    fn focus_mode_on_a_blank_line_only_keeps_that_separator() {
        let source = "One\n\nTwo";
        let text = gpui_component::input::Rope::from(source);
        let cursor = source.find("\n\n").expect("blank separator should exist") + 1;

        assert_eq!(focused_paragraph_range(&text, cursor..cursor), 4..5);
    }

    #[test]
    fn markdown_path_rename_preserves_the_active_highlighter() {
        assert_eq!(
            changed_document_kind(
                DocumentKind::Markdown,
                Some(std::path::Path::new("renamed-note.md"))
            ),
            None
        );
        assert_eq!(
            changed_document_kind(
                DocumentKind::Markdown,
                Some(std::path::Path::new("renamed-note.json"))
            ),
            Some(DocumentKind::Json)
        );
    }

    #[test]
    fn document_kinds_select_the_expected_highlighter() {
        assert_eq!(
            document_language(DocumentKind::Markdown),
            Language::Markdown
        );
        assert_eq!(document_language(DocumentKind::Json), Language::Json);
        assert_eq!(document_language(DocumentKind::PlainText), Language::Plain);
    }

    #[gpui::test]
    fn outline_navigation_to_document_start_has_no_intermediate_scroll_frame(
        cx: &mut gpui::TestAppContext,
    ) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let content = include_str!("../../../themes/sick.json");
        let (db, note_id) = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                let note = note::ActiveModel {
                    title: Set("JSON outline navigation".to_string()),
                    project_id: Set(None),
                    file_path: Set(None),
                    file_managed_by_app: Set(false),
                    cached_content: Set(content.to_string()),
                    file_missing_since: Set(None),
                    created_at: Set(1),
                    updated_at: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>((db, note.id as u32))
            })
            .expect("outline test database should initialize");
        let settings_dir =
            std::env::temp_dir().join(format!("castle-outline-navigation-{}", std::process::id()));
        let mut editor_view = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(AppSettings::load(settings_dir));
            cx.set_global(AppRuntime::new(Arc::new(db), PathBuf::new()));
            cx.open_window(Default::default(), |window, cx| {
                let view = DocumentEditorView::view(note_id, window, cx);
                editor_view = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("outline test window should open")
        });
        let view = editor_view.expect("document editor should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

        for _ in 0..100 {
            cx.run_until_parked();
            if view.read_with(&cx, |editor, _| !editor.persistence.is_loading) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.kind = DocumentKind::Json;
                editor.analysis.outline = DocumentOutline::Json(JsonOutline::parse(content));
                editor.analysis.outline.expand_all();
                editor.rebuild_outline_rows();
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(
                        gpui_component::input::Position::new(350, 0),
                        window,
                        cx,
                    );
                });
            });
        });
        cx.run_until_parked();

        let target_index = view
            .read_with(&cx, |editor, _| {
                editor
                    .analysis
                    .outline_rows
                    .iter()
                    .position(|row| row.title.starts_with("colors  ·"))
            })
            .expect("the colors node should be present");
        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.select_outline_item(target_index, window, cx);
            });
        });
        cx.run_until_parked();
        let first_frame_offset =
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).scroll_offset().y);

        cx.executor().advance_clock(OUTLINE_SCROLL_LAYOUT_DELAY);
        cx.run_until_parked();
        let centered_offset =
            view.read_with(&cx, |editor, cx| editor.editor.read(cx).scroll_offset().y);

        assert_eq!(
            first_frame_offset, centered_offset,
            "outline navigation must not paint an intermediate scroll position"
        );
    }

    #[gpui::test]
    fn markdown_plain_text_round_trip_restores_markdown_kind(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();

        let directory = tempfile::tempdir().expect("test directory should be created");
        let markdown_path = directory.path().join("note.md").display().to_string();
        let plain_path = directory.path().join("note.txt").display().to_string();
        let (db, note_id) = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                let note = note::ActiveModel {
                    title: Set("Kind round trip".to_string()),
                    project_id: Set(None),
                    file_path: Set(Some(markdown_path.clone())),
                    file_managed_by_app: Set(true),
                    cached_content: Set("# Title\n".to_string()),
                    file_missing_since: Set(None),
                    created_at: Set(1),
                    updated_at: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>((db, note.id as u32))
            })
            .expect("kind round trip database should initialize");
        let mut editor_view = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(AppSettings::load(directory.path()));
            cx.set_global(AppRuntime::new(Arc::new(db), PathBuf::new()));
            cx.open_window(Default::default(), |window, cx| {
                let view = DocumentEditorView::view(note_id, window, cx);
                editor_view = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("kind round trip window should open")
        });
        let view = editor_view.expect("document editor should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

        for _ in 0..100 {
            cx.run_until_parked();
            if view.read_with(&cx, |editor, _| !editor.persistence.is_loading) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(
            view.read_with(&cx, |editor, _| editor.kind()),
            DocumentKind::Markdown
        );

        cx.update(|_, cx| {
            view.update(cx, |editor, cx| {
                editor.apply_file_path(Some(plain_path.clone()), cx);
            });
        });
        assert_eq!(
            view.read_with(&cx, |editor, _| editor.kind()),
            DocumentKind::PlainText
        );

        cx.update(|_, cx| {
            view.update(cx, |editor, cx| {
                let markdown_path = directory.path().join("note.md").display().to_string();
                editor.apply_file_path(Some(markdown_path), cx);
            });
        });
        assert_eq!(
            view.read_with(&cx, |editor, _| editor.kind()),
            DocumentKind::Markdown
        );
    }
}
