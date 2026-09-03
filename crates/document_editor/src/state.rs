use super::*;

pub(super) struct DocumentAnalysis {
    pub(super) stats: DocumentStats,
    pub(super) outline: DocumentOutline,
    pub(super) mermaids: Vec<mermaid::MermaidDescriptor>,
    pub(super) preview_sections: Arc<Vec<SharedString>>,
}

#[derive(Clone, Copy)]
pub(super) struct OutlineSourceHighlight {
    pub(super) generation: u64,
    pub(super) source_offset: usize,
}

#[derive(Clone, Debug)]
pub enum DocumentEditorEvent {
    PathChanged,
    Saved(u32),
    WorkspaceLinksChanged,
    OpenNote {
        note_id: u32,
        source_offset: Option<usize>,
    },
    OpenWorkspaceTarget(workspace::WorkspaceNavigationTarget),
    CreateCardFromSelection {
        note_id: u32,
        title: String,
    },
    InsertBoardView {
        note_id: u32,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum DocumentInspectorTab {
    #[default]
    Outline,
    Links,
}

pub(super) struct PersistenceState {
    pub(super) current_path: Option<PathBuf>,
    pub(super) file_managed_by_app: bool,
    pub(super) save_state: SaveState,
    pub(super) load_error: Option<SharedString>,
    pub(super) is_loading: bool,
    pub(super) suppress_editor_events: bool,
    pub(super) auto_save_epoch: u64,
    pub(super) load_task: Option<Task<()>>,
    pub(super) auto_save_task: Option<Task<()>>,
    pub(super) format_task: Option<Task<()>>,
}

pub(super) struct AnalysisState {
    pub(super) stats: DocumentStats,
    pub(super) request: workspace::RequestTracker,
    pub(super) source_bounds: Option<Bounds<Pixels>>,
    pub(super) outline: DocumentOutline,
    pub(super) outline_rows: Arc<Vec<OutlineRow>>,
    pub(super) outline_visible: bool,
    pub(super) outline_rendered: bool,
    pub(super) outline_transition_epoch: usize,
    pub(super) outline_selected: Option<usize>,
    pub(super) outline_navigation_generation: u64,
    pub(super) outline_source_highlight: Option<OutlineSourceHighlight>,
    pub(super) outline_source_highlight_task: Option<Task<()>>,
    pub(super) source_bounds_mode: Option<EditorMode>,
    pub(super) preview_bounds: Option<Bounds<Pixels>>,
    pub(super) preview_bounds_mode: Option<EditorMode>,
    pub(super) preview_sections: Arc<Vec<SharedString>>,
    pub(super) preview_list_state: gpui::ListState,
    pub(super) preview_font_size_bits: Cell<u64>,
    pub(super) outline_scroll_handle: UniformListScrollHandle,
    pub(super) outline_focus_handle: FocusHandle,
}

pub(super) struct InspectorLinksState {
    pub(super) tab: DocumentInspectorTab,
    pub(super) note_links: Arc<storage::note::links::NoteLinkSet>,
    pub(super) note_catalog: Arc<Vec<storage::note::links::NoteLinkCatalogEntry>>,
    pub(super) workspace_links: Arc<storage::workspace::links::NoteWorkspaceLinks>,
    pub(super) workspace_catalog: Arc<storage::workspace::links::WorkspaceReferenceCatalog>,
    pub(super) relation_signature: Vec<String>,
    pub(super) project_id: Option<i64>,
    pub(super) completion_provider: links::WorkspaceReferenceCompletionProvider,
    pub(super) loading: bool,
    pub(super) error: Option<SharedString>,
    pub(super) request: workspace::RequestTracker,
}

pub(super) struct EmbedStateGroup {
    pub(super) states:
        Arc<std::collections::HashMap<board_embeds::EmbedKey, board_embeds::EmbedState>>,
    pub(super) request: workspace::RequestTracker,
    pub(super) loading_keys: std::collections::HashSet<board_embeds::EmbedKey>,
    pub(super) refresh_task: Option<Task<()>>,
    pub(super) refresh_epoch: u64,
}

pub(super) struct VimSessionState {
    pub(super) state: VimState,
    pub(super) search_active: bool,
}
