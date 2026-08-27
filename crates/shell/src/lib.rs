mod action;
mod action_handlers;
mod board_integration;
mod home;
mod render;
mod tabs;
mod workspace;

pub(crate) use action::{CloseAllTabsAction, CloseOtherTabsAction, CloseTabAction};
pub use action::{CycleNextTab, CyclePrevTab, OpenSettingsAction, ToggleSidebarAction};
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    MouseButton, ParentElement, PathPromptOptions, Pixels, Render, SharedString, Styled, Task,
    Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme, IconName, Root, Sizable as _, TitleBar, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{
        Escape as InputEscape, InputEvent, InputState, MoveDown as InputMoveDown,
        MoveUp as InputMoveUp,
    },
    menu::ContextMenuExt as _,
    notification::Notification,
    tab::{Tab, TabBar},
    v_flex,
};
use std::{collections::HashMap, rc::Rc, sync::Arc};
use storage::workspace::WorkspaceTitleTarget;

use ::workspace::{SidebarEvent, SidebarView};
use board::{BoardTemplatePicker, BoardTemplatePickerEvent, BoardView, BoardViewEvent};
use command_palette::{CommandPaletteEvent, CommandPaletteView};
use document_editor::{
    DEFAULT_NOTE, DocumentEditorEvent, DocumentEditorView, DocumentKind, SaveState,
    unique_note_path,
};
use runtime::AppRuntime;
use settings::{
    AgentAccess, AppSettings, SettingsIntegration, SettingsView, ShortcutReference, StoredTab,
};
use storage::time::unix_timestamp_seconds as now_ts;
use storage::workspace::home::WorkspaceHomeState;
use storage::workspace::trash::{TrashItem, TrashItemKind};

const SIDEBAR_AUTO_COLLAPSE_WIDTH: f32 = 900.;

type UpdateTrayShortcut = Rc<dyn Fn(&str, &mut App)>;
type ShortcutProvider = Rc<dyn Fn(&App) -> Vec<ShortcutReference>>;

#[derive(Clone)]
pub struct ShellIntegration {
    update_tray_shortcut: UpdateTrayShortcut,
    shortcuts: ShortcutProvider,
    agent_access: Arc<dyn AgentAccess>,
}

impl ShellIntegration {
    pub fn new(
        update_tray_shortcut: impl Fn(&str, &mut App) + 'static,
        shortcuts: impl Fn(&App) -> Vec<ShortcutReference> + 'static,
        agent_access: Arc<dyn AgentAccess>,
    ) -> Self {
        Self {
            update_tray_shortcut: Rc::new(update_tray_shortcut),
            shortcuts: Rc::new(shortcuts),
            agent_access,
        }
    }
}

#[cfg(test)]
struct TestAgentAccess;

#[cfg(test)]
impl AgentAccess for TestAgentAccess {
    fn status(&self) -> Result<settings::AgentAccessAvailability, String> {
        Ok(settings::AgentAccessAvailability::ServerUnavailable)
    }

    fn set_enabled(&self, _enabled: bool) -> Result<settings::AgentAccessAvailability, String> {
        Ok(settings::AgentAccessAvailability::ServerUnavailable)
    }
}

#[cfg(test)]
fn test_shell_integration() -> ShellIntegration {
    ShellIntegration::new(|_, _| {}, |_| Vec::new(), Arc::new(TestAgentAccess))
}

struct OpenTab {
    id: u64,
    title: SharedString,
    kind: OpenTabKind,
}

enum OpenTabKind {
    Chooser,
    Trash,
    Board {
        board_id: u32,
        project_id: Option<u32>,
        view: Entity<BoardView>,
    },
    Note {
        note_id: u32,
        project_id: Option<u32>,
        view: Entity<DocumentEditorView>,
    },
}

struct PendingWorkspaceTitleSave {
    generation: u64,
    title: String,
}

struct PendingBoardOpen {
    board_id: u32,
    view: Entity<BoardView>,
    tab_id: u64,
    replaced_chooser_id: Option<u64>,
}

#[derive(Clone)]
pub(crate) struct ProjectChoice {
    pub(crate) id: u32,
    pub(crate) name: SharedString,
}

#[derive(Clone)]
pub(crate) struct BoardChoice {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) project_id: Option<u32>,
    pub(crate) project_name: Option<SharedString>,
}

#[derive(Clone)]
pub(crate) struct NoteChoice {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) project_id: Option<u32>,
    pub(crate) project_name: Option<SharedString>,
}

struct TabsState {
    open_tabs: Vec<OpenTab>,
    note_views: HashMap<u32, Entity<DocumentEditorView>>,
    active_tab_index: usize,
    next_tab_id: u64,
}

#[derive(Clone)]
enum LoadPhase {
    Initial,
    Loading {
        had_content: bool,
    },
    Ready,
    Failed {
        message: SharedString,
        had_content: bool,
    },
}

impl LoadPhase {
    fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    fn has_content(&self) -> bool {
        matches!(
            self,
            Self::Ready
                | Self::Loading { had_content: true }
                | Self::Failed {
                    had_content: true,
                    ..
                }
        )
    }

    fn error(&self) -> Option<SharedString> {
        match self {
            Self::Failed { message, .. } => Some(message.clone()),
            _ => None,
        }
    }
}

pub(crate) struct WorkspaceState {
    pub(crate) projects: Vec<ProjectChoice>,
    pub(crate) boards: Vec<BoardChoice>,
    pub(crate) notes: Vec<NoteChoice>,
    pub(crate) active_project_id: Option<u32>,
    refreshing: bool,
    refresh_pending: bool,
    pending_title_saves: HashMap<WorkspaceTitleTarget, PendingWorkspaceTitleSave>,
    pending_board_open: Option<PendingBoardOpen>,
    title_save_lock: Arc<tokio::sync::Mutex<()>>,
}

struct HomeState {
    data: WorkspaceHomeState,
    phase: LoadPhase,
    refresh_pending: bool,
}

struct TrashState {
    items: Vec<TrashItem>,
    phase: LoadPhase,
    refresh_pending: bool,
    search_input: Entity<InputState>,
    query: String,
    kind_filter: Option<TrashItemKind>,
}

struct ExternalChangeState {
    task: Option<Task<()>>,
    revision: Option<i64>,
    board_revision: Option<i64>,
    note_revision: Option<i64>,
    link_revision: Option<i64>,
}

pub struct AppShell {
    pub(crate) focus_handle: FocusHandle,
    sidebar: Entity<SidebarView>,
    title_input: Entity<InputState>,
    command_palette: Entity<CommandPaletteView>,
    tabs: TabsState,
    pub(crate) workspace: WorkspaceState,
    suppress_title_event: bool,
    settings_view: Entity<SettingsView>,
    board_template_picker: Entity<BoardTemplatePicker>,
    window_is_narrow: bool,
    home: HomeState,
    trash: TrashState,
    external_changes: ExternalChangeState,
    last_card_destination: Option<i64>,
    record_opened_task: Option<Task<()>>,
}

impl AppShell {
    pub fn view(window: &mut Window, integration: ShellIntegration, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, integration, cx))
    }

    fn observe_document_editor(
        view: &Entity<DocumentEditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            view,
            window,
            |this, _, event: &DocumentEditorEvent, window, cx| match event {
                DocumentEditorEvent::PathChanged => this.refresh_workspace(cx),
                DocumentEditorEvent::Saved(note_id) => {
                    if !this.tabs.open_tabs.iter().any(|tab| {
                        matches!(
                            &tab.kind,
                            OpenTabKind::Note {
                                note_id: open_note_id,
                                ..
                            } if *open_note_id == *note_id
                        )
                    }) {
                        this.tabs.note_views.remove(note_id);
                    }
                }
                DocumentEditorEvent::WorkspaceLinksChanged => {
                    let boards = this
                        .tabs
                        .open_tabs
                        .iter()
                        .filter_map(|tab| match &tab.kind {
                            OpenTabKind::Board { board_id, view, .. } => {
                                Some((*board_id, view.clone()))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    for (board_id, view) in boards {
                        view.update(cx, |board, cx| board.reload_board(board_id, cx));
                    }
                }
                DocumentEditorEvent::OpenNote {
                    note_id,
                    source_offset,
                } => {
                    if let Some(note) = this.workspace.notes.iter().find(|note| note.id == *note_id)
                    {
                        let project_id = note.project_id;
                        let title = note.title.clone();
                        this.open_note_tab(*note_id, project_id, title, window, cx);
                        if let Some(offset) = source_offset
                            && let Some(view) = this.tabs.note_views.get(note_id)
                        {
                            view.update(cx, |editor, cx| {
                                editor.navigate_to_offset(*offset, window, cx)
                            });
                        }
                    }
                }
                DocumentEditorEvent::OpenWorkspaceTarget(target) => {
                    this.open_workspace_target(*target, window, cx);
                }
                DocumentEditorEvent::CreateCardFromSelection { note_id, title } => {
                    this.open_create_card_from_selection_picker(
                        *note_id,
                        title.clone(),
                        window,
                        cx,
                    );
                }
                DocumentEditorEvent::InsertBoardView { note_id } => {
                    this.open_insert_board_view_picker(*note_id, window, cx);
                }
            },
        )
        .detach();
    }

    fn observe_board_view(view: &Entity<BoardView>, window: &mut Window, cx: &mut Context<Self>) {
        cx.subscribe_in(
            view,
            window,
            |this, loaded_view, event: &BoardViewEvent, window, cx| match event {
                BoardViewEvent::LoadFinished(board_id) => {
                    loaded_view.update(cx, |board, cx| {
                        board.apply_pending_reveal(window, cx);
                    });
                    let is_current =
                        this.workspace
                            .pending_board_open
                            .as_ref()
                            .is_some_and(|pending| {
                                pending.board_id == *board_id
                                    && pending.view.entity_id() == loaded_view.entity_id()
                            });
                    if is_current {
                        this.finish_pending_board_open(window, cx);
                    }
                }
                BoardViewEvent::OpenNote(note_id) => {
                    this.open_workspace_target(
                        ::workspace::WorkspaceNavigationTarget::Note {
                            note_id: *note_id,
                            source_offset: None,
                        },
                        window,
                        cx,
                    );
                }
                BoardViewEvent::OpenWorkspaceTarget(target) => {
                    this.open_workspace_target(*target, window, cx);
                }
                BoardViewEvent::NavigationUnavailable(message) => {
                    window.push_notification(Notification::warning(message.clone()), cx);
                }
                BoardViewEvent::DataCommitted {
                    board_id,
                    links_changed,
                } => {
                    for view in this.tabs.note_views.values() {
                        view.update(cx, |note, cx| {
                            note.refresh_board_embeds_for(i64::from(*board_id), cx)
                        });
                    }
                    if *links_changed {
                        for view in this.tabs.note_views.values() {
                            view.update(cx, |note, cx| note.refresh_note_links(cx));
                        }
                        for tab in &this.tabs.open_tabs {
                            let OpenTabKind::Board {
                                board_id: open_board_id,
                                view,
                                ..
                            } = &tab.kind
                            else {
                                continue;
                            };
                            if view.entity_id() != loaded_view.entity_id() {
                                view.update(cx, |board, cx| board.reload_board(*open_board_id, cx));
                            }
                        }
                    }
                }
                BoardViewEvent::CreateLinkedNote {
                    item,
                    project_id,
                    title,
                } => {
                    this.create_linked_note(*project_id, title.clone(), *item, window, cx);
                }
            },
        )
        .detach();
    }

    fn observe_command_palette(
        view: &Entity<CommandPaletteView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            view,
            window,
            |this, _, event: &CommandPaletteEvent, window, cx| match event {
                CommandPaletteEvent::Closed => this.focus_handle.focus(window, cx),
                CommandPaletteEvent::OpenNote {
                    note_id,
                    project_id,
                    title,
                } => this.open_note_tab(*note_id, *project_id, title.clone(), window, cx),
                CommandPaletteEvent::OpenBoard {
                    board_id,
                    project_id,
                    title,
                } => {
                    this.open_board_tab(*board_id, *project_id, title.clone(), window, cx);
                }
                CommandPaletteEvent::NewNote { project_id, title } => {
                    this.create_note_with_title(*project_id, title.clone(), window, cx)
                }
                CommandPaletteEvent::NewBoard { project_id, title } => {
                    this.create_board_with_title(*project_id, title.clone(), window, cx)
                }
                CommandPaletteEvent::OpenFile => this.import_file(window, cx),
                CommandPaletteEvent::NewTab => this.new_tab(window, cx),
                CommandPaletteEvent::CloseAllTabs => this.close_all_tabs(window, cx),
                CommandPaletteEvent::OpenSettings => this.open_settings(window, cx),
                CommandPaletteEvent::CreateCardFromSelection => {
                    if let Some(note_view) = this.active_note_view() {
                        note_view.update(cx, |editor, cx| editor.create_card_from_selection(cx));
                    } else {
                        window.push_notification(Notification::warning("Open a note first."), cx);
                    }
                }
                CommandPaletteEvent::InsertBoardView => {
                    if let Some(note_view) = this.active_note_view() {
                        note_view.update(cx, |editor, cx| editor.request_insert_board_view(cx));
                    } else {
                        window.push_notification(Notification::warning("Open a note first."), cx);
                    }
                }
                CommandPaletteEvent::OpenSearchResult(result) => {
                    let target = match result.kind {
                        storage::workspace::search::SearchResultKind::Note => {
                            ::workspace::WorkspaceNavigationTarget::Note {
                                note_id: result.open_id,
                                source_offset: None,
                            }
                        }
                        storage::workspace::search::SearchResultKind::Board => {
                            ::workspace::WorkspaceNavigationTarget::board(result.open_id)
                        }
                        storage::workspace::search::SearchResultKind::Card => {
                            ::workspace::WorkspaceNavigationTarget::list(
                                result.open_id,
                                result.item_id,
                            )
                        }
                        storage::workspace::search::SearchResultKind::Entry => {
                            ::workspace::WorkspaceNavigationTarget::card(
                                result.open_id,
                                result.item_id,
                            )
                        }
                    };
                    this.open_workspace_target(target, window, cx);
                }
            },
        )
        .detach();
    }

    fn new(window: &mut Window, integration: ShellIntegration, cx: &mut Context<Self>) -> Self {
        let tab_session = AppSettings::tab_session(cx);
        let sidebar = SidebarView::view(window, cx);
        let mut open_tabs = Vec::with_capacity(tab_session.tabs.len().max(1));
        let mut note_views = HashMap::new();
        let mut next_tab_id = 1_u64;
        for stored_tab in tab_session.tabs {
            let (title, kind) = match stored_tab {
                StoredTab::Chooser => (SharedString::from("Home"), OpenTabKind::Chooser),
                StoredTab::Trash => (SharedString::from("Trash"), OpenTabKind::Trash),
                StoredTab::Board {
                    board_id,
                    project_id,
                    title,
                } => {
                    let view = BoardView::view(window, cx);
                    Self::observe_board_view(&view, window, cx);
                    view.update(cx, |board, cx| board.load_board(board_id, cx));
                    (
                        SharedString::from(title),
                        OpenTabKind::Board {
                            board_id,
                            project_id,
                            view,
                        },
                    )
                }
                StoredTab::Note {
                    note_id,
                    project_id,
                    title,
                } => {
                    let view = DocumentEditorView::view(note_id, window, cx);
                    Self::observe_document_editor(&view, window, cx);
                    note_views.insert(note_id, view.clone());
                    (
                        SharedString::from(title),
                        OpenTabKind::Note {
                            note_id,
                            project_id,
                            view,
                        },
                    )
                }
            };
            open_tabs.push(OpenTab {
                id: next_tab_id,
                title,
                kind,
            });
            next_tab_id = next_tab_id.saturating_add(1);
        }
        if open_tabs.is_empty() {
            open_tabs.push(OpenTab {
                id: next_tab_id,
                title: "Home".into(),
                kind: OpenTabKind::Chooser,
            });
            next_tab_id = next_tab_id.saturating_add(1);
        }
        let active_tab_index = tab_session.active_tab_index.min(open_tabs.len() - 1);
        let active_title = open_tabs[active_tab_index].title.to_string();
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Home")
                .default_value(active_title)
        });

        let command_palette = CommandPaletteView::view(window, cx);
        Self::observe_command_palette(&command_palette, window, cx);
        let trash_search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search Trash..."));

        cx.subscribe(&title_input, |this, input, event: &InputEvent, cx| {
            if !matches!(event, InputEvent::Change) || this.suppress_title_event {
                return;
            }

            let title = input.read(cx).text().to_string();
            this.rename_active_tab(title, cx);
        })
        .detach();

        cx.subscribe(
            &trash_search_input,
            |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.trash.query = input.read(cx).text().to_string();
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &sidebar,
            window,
            |this, _, event: &SidebarEvent, window, cx| match event {
                SidebarEvent::OpenHome => this.open_home(window, cx),
                SidebarEvent::OpenTrash => this.open_trash(window, cx),
                SidebarEvent::OpenThemeSwitcher => this.open_theme_switcher(window, cx),
                SidebarEvent::ImportFile => this.import_file(window, cx),
                SidebarEvent::WidthChanged => cx.notify(),
                SidebarEvent::WorkspaceChanged => {
                    this.load_home(cx);
                    this.load_trash(cx);
                    this.refresh_workspace(cx);
                }
                SidebarEvent::OpenBoard {
                    board_id,
                    project_id,
                    title,
                } => {
                    this.workspace.active_project_id = *project_id;
                    this.open_board_tab(*board_id, *project_id, title.clone(), window, cx);
                }
                SidebarEvent::OpenNote {
                    note_id,
                    project_id,
                    title,
                } => {
                    this.workspace.active_project_id = *project_id;
                    this.open_note_tab(*note_id, *project_id, title.clone(), window, cx);
                }
                SidebarEvent::ActivateProject { project_id } => {
                    this.activate_project(*project_id, window, cx);
                }
                SidebarEvent::BoardRenamed { board_id, title } => {
                    let mut renamed_active = false;
                    for (i, tab) in this.tabs.open_tabs.iter_mut().enumerate() {
                        if let OpenTabKind::Board { board_id: id, .. } = &tab.kind
                            && *id == *board_id
                        {
                            tab.title = title.clone();
                            renamed_active = i == this.tabs.active_tab_index;
                            break;
                        }
                    }
                    if renamed_active {
                        this.sync_title_input(window, cx);
                    }
                    if let Some(board) = this.workspace.boards.iter_mut().find(|board| board.id == *board_id) {
                        board.title = title.clone();
                    }
                    this.command_palette.update(cx, |palette, cx| {
                        palette.rename_board(*board_id, title.clone(), cx)
                    });
                    this.persist_tab_session(cx);
                    cx.notify();
                }
                SidebarEvent::NoteRenamed { note_id, title } => {
                    let mut renamed_active = false;
                    for (i, tab) in this.tabs.open_tabs.iter_mut().enumerate() {
                        if let OpenTabKind::Note { note_id: id, view, .. } = &tab.kind
                            && *id == *note_id
                        {
                            tab.title = title.clone();
                            renamed_active = i == this.tabs.active_tab_index;
                            let view = view.clone();
                            view.update(cx, |note, cx| {
                                note.apply_title(title, cx);
                            });
                            break;
                        }
                    }
                    if renamed_active {
                        this.sync_title_input(window, cx);
                    }
                    if let Some(note) = this.workspace.notes.iter_mut().find(|note| note.id == *note_id) {
                        note.title = title.clone();
                    }
                    this.command_palette.update(cx, |palette, cx| {
                        palette.rename_note(*note_id, title.clone(), cx)
                    });
                    this.persist_tab_session(cx);
                    cx.notify();
                }
                SidebarEvent::NotePathChanged { note_id, file_path } => {
                    if let Some(view) = this.tabs.open_tabs.iter().find_map(|tab| match &tab.kind {
                        OpenTabKind::Note {
                            note_id: open_note_id,
                            view,
                            ..
                        } if open_note_id == note_id => Some(view.clone()),
                        _ => None,
                    }) {
                        view.update(cx, |note, cx| {
                            note.apply_file_path(file_path.clone(), cx);
                        });
                    }
                }
                SidebarEvent::BoardDeleted { board_id } => {
                    if let Some(index) = this.tabs.open_tabs.iter().position(
                        |tab| matches!(&tab.kind, OpenTabKind::Board { board_id: id, .. } if *id == *board_id),
                    ) {
                        this.close_tab(index, window, cx);
                    }
                }
                SidebarEvent::NoteDeleted { note_id } => {
                    if let Some(index) = this
                        .tabs.open_tabs
                        .iter()
                        .position(|tab| matches!(&tab.kind, OpenTabKind::Note { note_id: id, .. } if *id == *note_id))
                    {
                        this.close_tab(index, window, cx);
                    }
                }
                SidebarEvent::ProjectRenamed { project_id, name } => {
                    for project in &mut this.workspace.projects {
                        if project.id == *project_id {
                            project.name = name.clone();
                        }
                    }

                    for board in &mut this.workspace.boards {
                        if board.project_id == Some(*project_id) {
                            board.project_name = Some(name.clone());
                        }
                    }

                    for note in &mut this.workspace.notes {
                        if note.project_id == Some(*project_id) {
                            note.project_name = Some(name.clone());
                        }
                    }

                    this.command_palette.update(cx, |palette, cx| {
                        palette.rename_project(*project_id, name.clone(), cx)
                    });
                    cx.notify();
                }
                SidebarEvent::ProjectDeleted { project_id } => {
                    this.close_project_tabs(*project_id, window, cx);
                    if this.workspace.active_project_id == Some(*project_id) {
                        this.workspace.active_project_id = None;
                    }
                    this.persist_tab_session(cx);
                }
                SidebarEvent::ProjectsReordered => {
                    this.refresh_workspace(cx);
                }
            },
        )
        .detach();

        let sidebar_for_visibility = sidebar.clone();
        let shell_for_sidebar = cx.entity().downgrade();
        let update_tray_shortcut = integration.update_tray_shortcut.clone();
        let shortcuts = integration.shortcuts.clone();
        let settings_view = cx.new(|_| {
            SettingsView::new(SettingsIntegration::new(
                move |cx| !sidebar_for_visibility.read(cx).is_collapsed(),
                move |visible, cx| {
                    if let Some(shell) = shell_for_sidebar.upgrade() {
                        shell.update(cx, |shell, cx| {
                            shell.set_sidebar_visible(visible, cx);
                        });
                    }
                },
                move |shortcut, cx| update_tray_shortcut(shortcut, cx),
                move |cx| shortcuts(cx),
                integration.agent_access,
            ))
        });
        let board_template_picker = BoardTemplatePicker::view(cx);
        cx.subscribe_in(
            &board_template_picker,
            window,
            |this, _, event: &BoardTemplatePickerEvent, window, cx| match event {
                BoardTemplatePickerEvent::BoardCreated {
                    board_id,
                    project_id,
                    title,
                } => {
                    this.open_board_tab(*board_id, *project_id, title.clone(), window, cx);
                    this.refresh_workspace(cx);
                }
            },
        )
        .detach();

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            sidebar,
            title_input,
            command_palette,
            tabs: TabsState {
                open_tabs,
                note_views,
                active_tab_index,
                next_tab_id,
            },
            workspace: WorkspaceState {
                projects: vec![],
                boards: vec![],
                notes: vec![],
                active_project_id: tab_session.active_project_id,
                refreshing: false,
                refresh_pending: false,
                pending_title_saves: HashMap::new(),
                pending_board_open: None,
                title_save_lock: Arc::new(tokio::sync::Mutex::new(())),
            },
            suppress_title_event: false,
            settings_view: settings_view.clone(),
            board_template_picker,
            window_is_narrow: false,
            home: HomeState {
                data: WorkspaceHomeState::default(),
                phase: LoadPhase::Initial,
                refresh_pending: false,
            },
            trash: TrashState {
                items: Vec::new(),
                phase: LoadPhase::Initial,
                refresh_pending: false,
                search_input: trash_search_input,
                query: String::new(),
                kind_filter: None,
            },
            external_changes: ExternalChangeState {
                task: None,
                revision: None,
                board_revision: None,
                note_revision: None,
                link_revision: None,
            },
            last_card_destination: None,
            record_opened_task: None,
        };

        let show_sidebar = AppSettings::show_sidebar(cx);
        this.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_collapsed(!show_sidebar, cx);
        });
        this.sync_sidebar_with_window_width(window.bounds().size.width, cx);
        cx.observe_window_bounds(window, |this, window, cx| {
            this.sync_sidebar_with_window_width(window.bounds().size.width, cx);
        })
        .detach();
        cx.on_app_quit(|this, cx| {
            let title_flush = this.flush_pending_workspace_title_saves(cx);
            let settings_flush = AppSettings::flush(cx);
            async move {
                tokio::join!(title_flush, settings_flush);
            }
        })
        .detach();
        this.start_external_change_watcher(window, cx);
        this.start_note_link_reindex(cx);
        settings_view.update(cx, |settings, cx| settings.refresh_agent_access(cx));
        this.refresh_workspace(cx);
        this.sync_sidebar_active(cx);
        this.load_home(cx);
        this.load_trash(cx);
        this
    }
}
