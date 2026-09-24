use crate as workflow;
use std::{collections::HashMap, sync::Arc};
mod evaluate;
mod mermaid;
mod model;
mod validate;

pub use evaluate::evaluate;
pub use mermaid::to_mermaid;
pub use model::*;
pub use validate::validate;

use gpui_kit::component::{
    ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement as _,
    switch::Switch,
    v_flex,
};
use gpui_kit::component::{
    searchable_list::{SearchableListItem, SearchableVec},
    select::{Select, SelectState},
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_kit::{App, EventEmitter, FocusHandle, Focusable, ScrollHandle};
mod labels;
mod render;
use labels::connection_label;
mod pages;
mod preview;
pub use pages::WorkflowPage;
use preview::WorkflowMermaidPreview;

pub type WorkflowTask<T> = gpui_kit::Task<Result<anyhow::Result<T>, tokio::task::JoinError>>;

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowRecord {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub enabled: bool,
    pub definition: WorkflowDefinition,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowListEntry {
    pub id: u32,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowListRecord {
    pub id: u32,
    pub title: String,
    pub entries: Vec<WorkflowListEntry>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkflowSnapshot {
    pub workflows: Vec<WorkflowRecord>,
    pub lists: Vec<WorkflowListRecord>,
    pub runs: Vec<WorkflowRunHistoryEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowRunStatus {
    Running,
    Succeeded,
    Failed,
    Skipped,
}

impl WorkflowRunStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Skipped => "Skipped",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowRunHistoryEntry {
    pub id: i64,
    pub workflow_id: i64,
    pub workflow_name: String,
    pub entry_id: Option<i64>,
    pub entry_title: Option<String>,
    pub trigger_kind: String,
    pub status: WorkflowRunStatus,
    pub actions: Vec<WorkflowAction>,
    pub error: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct SaveWorkflowRequest {
    pub workflow_id: Option<i64>,
    pub board_id: i64,
    pub name: String,
    pub enabled: bool,
    pub definition: WorkflowDefinition,
}

pub trait WorkflowService: Send + Sync {
    fn load(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        board_id: u32,
    ) -> WorkflowTask<WorkflowSnapshot>;
    fn save(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        request: SaveWorkflowRequest,
    ) -> WorkflowTask<WorkflowRecord>;
    fn run_manual(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        board_id: u32,
        entry_id: i64,
    ) -> WorkflowTask<usize>;
}

struct WorkflowEditorState {
    running: bool,
    overview_notice: Option<SharedString>,
    run_notice: Option<SharedString>,
    new_workflow_open: bool,
    workflows: Vec<WorkflowRecord>,
    runs: Vec<WorkflowRunHistoryEntry>,
    manual_entry_select: Entity<SelectState<SearchableVec<EntryOption>>>,
}

#[derive(Clone)]
pub(crate) struct WorkflowNodeDragStart {
    node_id: String,
    pointer: gpui_kit::Point<gpui_kit::Pixels>,
    position: GraphPosition,
}

#[derive(Clone)]
struct WorkflowNodeDrag {
    node_id: String,
}

impl gpui_kit::Render for WorkflowNodeDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .px_2()
            .py_1()
            .rounded_sm()
            .border_1()
            .border_color(theme.drag_border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .text_xs()
            .shadow_sm()
            .child(self.node_id.clone())
    }
}

impl WorkflowWorkspace {
    pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let board_id = self.board_id;
        self.load_revision += 1;
        let revision = self.load_revision;
        let task = self
            .service
            .load(cx.background_executor().clone(), board_id);
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.load_revision != revision {
                    return;
                }
                let mut presentation_changed = false;
                match result {
                    Ok(Ok(snapshot)) => {
                        let (records, invalid) = filter_valid_workflows(snapshot.workflows);
                        let notice = (invalid > 0)
                            .then(|| format!("Skipped {invalid} invalid saved workflows").into());
                        if this.state.workflows != records {
                            this.state.workflows = records;
                            presentation_changed = true;
                        }
                        if this.state.runs != snapshot.runs {
                            this.state.runs = snapshot.runs;
                            presentation_changed = true;
                        }
                        if this.lists != snapshot.lists {
                            let options: Vec<_> = snapshot
                                .lists
                                .iter()
                                .flat_map(|list| {
                                    list.entries.iter().map(|entry| EntryOption {
                                        id: i64::from(entry.id),
                                        title: entry.title.clone().into(),
                                        list: list.title.clone().into(),
                                    })
                                })
                                .collect();
                            this.lists = snapshot.lists;
                            this.state.manual_entry_select.update(cx, |picker, cx| {
                                picker.set_items(SearchableVec::new(options), window, cx)
                            });
                            presentation_changed = true;
                        }
                        if this.state.overview_notice != notice {
                            this.state.overview_notice = notice;
                            presentation_changed = true;
                        }
                    }
                    Ok(Err(error)) => {
                        let notice = Some(error.to_string().into());
                        if this.state.overview_notice != notice {
                            this.state.overview_notice = notice;
                            presentation_changed = true;
                        }
                    }
                    Err(error) => {
                        let notice = Some(error.to_string().into());
                        if this.state.overview_notice != notice {
                            this.state.overview_notice = notice;
                            presentation_changed = true;
                        }
                    }
                }
                if presentation_changed {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn select_workflow(
        &mut self,
        workflow_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.draft.active_id != Some(workflow_id) {
            let key = self
                .drafts
                .iter()
                .find(|(_, draft)| draft.active_id == Some(workflow_id))
                .map(|(key, _)| *key);
            let next = if let Some(key) = key {
                self.drafts.remove(&key)
            } else {
                self.state
                    .workflows
                    .iter()
                    .find(|record| record.id == workflow_id)
                    .cloned()
                    .map(|record| {
                        self.next_draft += 1;
                        WorkflowDraft::new(
                            self.next_draft,
                            Some(record.id),
                            record.definition,
                            window,
                            cx,
                        )
                    })
            };
            if let Some(next) = next {
                let previous = std::mem::replace(&mut self.draft, next);
                self.drafts.insert(previous.key, previous);
            }
        }
        cx.emit(WorkflowWorkspaceEvent::Navigate(WorkflowRoute::Editor));
        cx.notify();
    }

    pub fn new_workflow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_workflow_definition(WorkflowDefinition::new("New workflow"), window, cx);
    }

    fn open_new_workflow(&mut self, cx: &mut Context<Self>) {
        self.state.new_workflow_open = true;
        cx.notify();
    }

    fn close_new_workflow(&mut self, cx: &mut Context<Self>) {
        self.state.new_workflow_open = false;
        cx.notify();
    }

    pub(crate) fn toggle_workflow_enabled(&mut self, cx: &mut Context<Self>) {
        self.draft.definition.enabled = !self.draft.definition.enabled;
        cx.notify();
    }

    pub(crate) fn add_workflow_node(&mut self, kind: WorkflowNodeKind, cx: &mut Context<Self>) {
        let index = self.draft.definition.nodes.len();
        let node_id = next_node_id(&self.draft.definition.nodes);
        self.draft.definition.nodes.push(WorkflowNode {
            id: node_id.clone(),
            kind,
            position: GraphPosition {
                x: 180.,
                y: 24. + index as f32 * 112.,
            },
        });
        self.draft.selected_node = Some(node_id);
        if self.route_narrow(WorkflowRoute::Editor) {
            cx.emit(WorkflowWorkspaceEvent::Navigate(WorkflowRoute::Step));
        }
        cx.notify();
    }

    pub(crate) fn remove_selected_workflow_node(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = self.draft.selected_node.take() else {
            return;
        };
        self.draft
            .definition
            .nodes
            .retain(|node| node.id != selected);
        self.draft
            .definition
            .edges
            .retain(|edge| edge.from != selected && edge.to != selected);
        cx.notify();
    }

    pub(crate) fn connect_selected_workflow_node_to(
        &mut self,
        target_id: String,
        kind: WorkflowEdgeKind,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = self.draft.selected_node.clone() else {
            return;
        };
        if selected == target_id
            || !self
                .draft
                .definition
                .nodes
                .iter()
                .any(|node| node.id == target_id)
        {
            return;
        }
        if self
            .draft
            .definition
            .edges
            .iter()
            .any(|edge| edge.from == selected && edge.to == target_id && edge.kind == kind)
        {
            return;
        }
        self.draft.definition.edges.push(WorkflowEdge {
            id: next_edge_id(&self.draft.definition.edges),
            from: selected,
            to: target_id,
            kind,
        });
        cx.notify();
    }

    pub(crate) fn configure_selected_workflow_node(
        &mut self,
        kind: WorkflowNodeKind,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = self.draft.selected_node.as_deref() else {
            return;
        };
        let Some(node) = self
            .draft
            .definition
            .nodes
            .iter_mut()
            .find(|node| node.id == selected)
        else {
            return;
        };
        node.kind = kind;
        cx.notify();
    }

    pub(crate) fn apply_workflow_definition(
        &mut self,
        mut definition: WorkflowDefinition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        definition.enabled = false;
        self.next_draft += 1;
        let draft = WorkflowDraft::new(self.next_draft, None, definition, window, cx);
        let previous = std::mem::replace(&mut self.draft, draft);
        self.drafts.insert(previous.key, previous);
        self.state.new_workflow_open = false;
        cx.emit(WorkflowWorkspaceEvent::Navigate(WorkflowRoute::Editor));
        cx.notify();
    }

    pub(crate) fn save_workflow(&mut self, cx: &mut Context<Self>) {
        if self.draft.saving {
            return;
        }
        let name = self.draft.name_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.draft.error = Some("Workflow name must not be empty".into());
            cx.notify();
            return;
        }
        let mut saved = self.draft.definition.clone();
        saved.name = name.clone();
        if let Err(error) = workflow::validate(&saved) {
            self.draft.error = Some(error.into());
            cx.notify();
            return;
        }
        let key = self.draft.key;
        self.draft.saving = true;
        self.draft.error = None;
        let task = self.service.save(
            cx.background_executor().clone(),
            SaveWorkflowRequest {
                workflow_id: self.draft.active_id,
                board_id: i64::from(self.board_id),
                name,
                enabled: saved.enabled,
                definition: saved.clone(),
            },
        );
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                let draft = if this.draft.key == key {
                    Some(&mut this.draft)
                } else {
                    this.drafts.get_mut(&key)
                };
                if let Some(draft) = draft {
                    draft.saving = false;
                    match result {
                        Ok(Ok(record)) => {
                            draft.active_id = Some(record.id);
                            draft.baseline = Some(saved);
                            if let Some(existing) = this
                                .state
                                .workflows
                                .iter_mut()
                                .find(|item| item.id == record.id)
                            {
                                *existing = record;
                            } else {
                                this.state.workflows.push(record);
                            }
                            cx.emit(WorkflowWorkspaceEvent::Committed(this.board_id));
                        }
                        Ok(Err(error)) => draft.error = Some(error.to_string().into()),
                        Err(error) => draft.error = Some(error.to_string().into()),
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn run_manual_workflows_from_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.running {
            return;
        }
        let board_id = self.board_id;
        let Some(entry_id) = self
            .state
            .manual_entry_select
            .read(cx)
            .selected_value()
            .copied()
        else {
            self.state.run_notice = Some("Choose an item to run saved manual workflows".into());
            cx.notify();
            return;
        };
        self.state.running = true;
        self.state.run_notice = None;
        let task = self
            .service
            .run_manual(cx.background_executor().clone(), board_id, entry_id);
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.board_id != board_id {
                    return;
                }
                this.state.running = false;
                match result {
                    Ok(Ok(run_count)) => {
                        let workflow_label = if run_count == 1 {
                            "workflow"
                        } else {
                            "workflows"
                        };
                        this.state.run_notice =
                            Some(format!("Ran {run_count} {workflow_label}").into());
                        cx.emit(WorkflowWorkspaceEvent::Committed(board_id));
                    }
                    Ok(Err(error)) => {
                        this.state.run_notice = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.state.run_notice = Some(error.to_string().into());
                    }
                }
                this.refresh(window, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn filter_valid_workflows(workflows: Vec<WorkflowRecord>) -> (Vec<WorkflowRecord>, usize) {
    let mut invalid_count = 0;
    let valid = workflows
        .into_iter()
        .filter(|workflow| {
            if validate(&workflow.definition).is_ok() {
                true
            } else {
                invalid_count += 1;
                false
            }
        })
        .collect();
    (valid, invalid_count)
}

fn role_action_definition(
    role: workflow::ListWorkflowRole,
    action: WorkflowAction,
    name: &str,
) -> WorkflowDefinition {
    WorkflowDefinition {
        name: name.to_string(),
        nodes: vec![
            WorkflowNode {
                id: "trigger".to_string(),
                kind: WorkflowNodeKind::Trigger {
                    trigger: WorkflowTrigger::CardMovedToRole { role },
                },
                position: GraphPosition { x: 180., y: 24. },
            },
            WorkflowNode {
                id: "condition".to_string(),
                kind: WorkflowNodeKind::Condition {
                    condition: WorkflowCondition::ListRoleIs { role },
                },
                position: GraphPosition { x: 180., y: 136. },
            },
            WorkflowNode {
                id: "action".to_string(),
                kind: WorkflowNodeKind::Action { action },
                position: GraphPosition { x: 180., y: 248. },
            },
            WorkflowNode {
                id: "end".to_string(),
                kind: WorkflowNodeKind::End {
                    label: Some(role.label().to_string()),
                },
                position: GraphPosition { x: 180., y: 360. },
            },
        ],
        edges: vec![
            WorkflowEdge {
                id: "edge-1".to_string(),
                from: "trigger".to_string(),
                to: "condition".to_string(),
                kind: WorkflowEdgeKind::Default,
            },
            WorkflowEdge {
                id: "edge-2".to_string(),
                from: "condition".to_string(),
                to: "action".to_string(),
                kind: WorkflowEdgeKind::True,
            },
            WorkflowEdge {
                id: "edge-3".to_string(),
                from: "action".to_string(),
                to: "end".to_string(),
                kind: WorkflowEdgeKind::Default,
            },
        ],
        ..WorkflowDefinition::new(name)
    }
}

fn event_action_definition(
    trigger: WorkflowTrigger,
    action: WorkflowAction,
    name: &str,
) -> WorkflowDefinition {
    WorkflowDefinition {
        name: name.to_string(),
        nodes: vec![
            WorkflowNode {
                id: "trigger".to_string(),
                kind: WorkflowNodeKind::Trigger { trigger },
                position: GraphPosition { x: 180., y: 24. },
            },
            WorkflowNode {
                id: "action".to_string(),
                kind: WorkflowNodeKind::Action { action },
                position: GraphPosition { x: 180., y: 136. },
            },
            WorkflowNode {
                id: "end".to_string(),
                kind: WorkflowNodeKind::End {
                    label: Some("Done".to_string()),
                },
                position: GraphPosition { x: 180., y: 248. },
            },
        ],
        edges: vec![
            WorkflowEdge {
                id: "edge-1".to_string(),
                from: "trigger".to_string(),
                to: "action".to_string(),
                kind: WorkflowEdgeKind::Default,
            },
            WorkflowEdge {
                id: "edge-2".to_string(),
                from: "action".to_string(),
                to: "end".to_string(),
                kind: WorkflowEdgeKind::Default,
            },
        ],
        ..WorkflowDefinition::new(name)
    }
}

fn node_position(node: &WorkflowNode, index: usize) -> (f32, f32) {
    if !node.position.x.is_finite() || !node.position.y.is_finite() {
        return (180., 24. + index as f32 * 112.);
    }
    if node.position.x == 0. && node.position.y == 0. {
        (180., 24. + index as f32 * 112.)
    } else {
        (
            node.position.x.clamp(16., 10_000.),
            node.position.y.clamp(16., 100_000.),
        )
    }
}

fn next_node_id(nodes: &[WorkflowNode]) -> String {
    let mut index = nodes.len().saturating_add(1);
    loop {
        let candidate = format!("node-{index}");
        if nodes.iter().all(|node| node.id != candidate) {
            return candidate;
        }
        index = index.saturating_add(1);
    }
}

fn next_edge_id(edges: &[WorkflowEdge]) -> String {
    let mut index = edges.len().saturating_add(1);
    loop {
        let candidate = format!("edge-{index}");
        if edges.iter().all(|edge| edge.id != candidate) {
            return candidate;
        }
        index = index.saturating_add(1);
    }
}

fn node_icon(kind: &WorkflowNodeKind) -> IconName {
    match kind {
        WorkflowNodeKind::Trigger { .. } => IconName::Play,
        WorkflowNodeKind::Condition { .. } | WorkflowNodeKind::Branch { .. } => IconName::Settings2,
        WorkflowNodeKind::Action { .. } => IconName::Settings2,
        WorkflowNodeKind::End { .. } => IconName::CircleCheck,
    }
}

fn node_kind_label(kind: &WorkflowNodeKind) -> &'static str {
    match kind {
        WorkflowNodeKind::Trigger { .. } => "When",
        WorkflowNodeKind::Condition { .. } => "If",
        WorkflowNodeKind::Branch { .. } => "Branch",
        WorkflowNodeKind::Action { .. } => "Then",
        WorkflowNodeKind::End { .. } => "End",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkflowRoute {
    Overview,
    Editor,
    Step,
    Mermaid,
    Run,
}
#[derive(Clone, Debug)]
pub enum WorkflowWorkspaceEvent {
    Navigate(WorkflowRoute),
    Back,
    Board,
    Committed(u32),
}

pub struct WorkflowWorkspace {
    board_id: u32,
    service: Arc<dyn WorkflowService>,
    lists: Vec<WorkflowListRecord>,
    state: WorkflowEditorState,
    draft: WorkflowDraft,
    drafts: HashMap<u64, WorkflowDraft>,
    next_draft: u64,
    load_revision: u64,
    route_narrow: [bool; 5],
    canvas_width: f32,
    canvas_height: f32,
    mermaid_preview: WorkflowMermaidPreview,
}
impl EventEmitter<WorkflowWorkspaceEvent> for WorkflowWorkspace {}

struct WorkflowDraft {
    key: u64,
    active_id: Option<i64>,
    definition: WorkflowDefinition,
    baseline: Option<WorkflowDefinition>,
    selected_node: Option<String>,
    node_drag_start: Option<WorkflowNodeDragStart>,
    name_input: Entity<InputState>,
    saving: bool,
    error: Option<SharedString>,
    canvas_scroll: ScrollHandle,
}
impl WorkflowDraft {
    fn new(
        key: u64,
        active_id: Option<i64>,
        definition: WorkflowDefinition,
        window: &mut Window,
        cx: &mut Context<WorkflowWorkspace>,
    ) -> Self {
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(definition.name.clone())
                .placeholder("Workflow name")
        });
        cx.observe(&name_input, |_, _, cx| cx.notify()).detach();
        Self {
            key,
            active_id,
            baseline: active_id.map(|_| definition.clone()),
            definition,
            selected_node: None,
            node_drag_start: None,
            name_input,
            saving: false,
            error: None,
            canvas_scroll: ScrollHandle::default(),
        }
    }
    fn dirty(&self, cx: &App) -> bool {
        let mut current = self.definition.clone();
        current.name = self.name_input.read(cx).value().to_string();
        self.baseline.as_ref() != Some(&current)
    }
}
impl WorkflowWorkspace {
    pub fn new(
        board_id: u32,
        service: Arc<dyn WorkflowService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft =
            WorkflowDraft::new(0, None, WorkflowDefinition::new("New workflow"), window, cx);
        Self {
            board_id,
            service,
            lists: Vec::new(),
            state: WorkflowEditorState {
                running: false,
                overview_notice: None,
                run_notice: None,
                new_workflow_open: false,
                workflows: Vec::new(),
                runs: Vec::new(),
                manual_entry_select: cx.new(|cx| {
                    SelectState::new(SearchableVec::new(Vec::new()), None, window, cx)
                        .searchable(true)
                }),
            },
            draft,
            drafts: HashMap::new(),
            next_draft: 0,
            load_revision: 0,
            route_narrow: [false; 5],
            canvas_width: 640.,
            canvas_height: 480.,
            mermaid_preview: WorkflowMermaidPreview::default(),
        }
    }
    fn route_narrow(&self, route: WorkflowRoute) -> bool {
        self.route_narrow[match route {
            WorkflowRoute::Overview => 0,
            WorkflowRoute::Editor => 1,
            WorkflowRoute::Step => 2,
            WorkflowRoute::Mermaid => 3,
            WorkflowRoute::Run => 4,
        }]
    }
    fn set_route_narrow(&mut self, route: WorkflowRoute, narrow: bool) {
        self.route_narrow[match route {
            WorkflowRoute::Overview => 0,
            WorkflowRoute::Editor => 1,
            WorkflowRoute::Step => 2,
            WorkflowRoute::Mermaid => 3,
            WorkflowRoute::Run => 4,
        }] = narrow;
    }
    fn resume_draft(&mut self, key: u64, cx: &mut Context<Self>) {
        if key != self.draft.key
            && let Some(next) = self.drafts.remove(&key)
        {
            let old = std::mem::replace(&mut self.draft, next);
            self.drafts.insert(old.key, old);
        }
        cx.emit(WorkflowWorkspaceEvent::Navigate(WorkflowRoute::Editor));
        cx.notify();
    }
    fn discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.draft.saving {
            return;
        }
        self.draft.definition = self.draft.baseline.clone().unwrap_or_else(|| {
            let mut definition = WorkflowDefinition::new("New workflow");
            definition.enabled = false;
            definition
        });
        self.draft.name_input.update(cx, |input, cx| {
            input.set_value(self.draft.definition.name.clone(), window, cx)
        });
        self.draft.selected_node = None;
        self.draft.error = None;
        cx.notify();
    }
}

#[derive(Clone)]
struct EntryOption {
    id: i64,
    title: SharedString,
    list: SharedString,
}
impl SearchableListItem for EntryOption {
    type Value = i64;
    fn title(&self) -> SharedString {
        self.title.clone()
    }
    fn matches(&self, query: &str) -> bool {
        format!("{} {}", self.title, self.list)
            .to_lowercase()
            .contains(&query.to_lowercase())
    }
    fn render(&self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        v_flex().child(self.title.clone()).child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(self.list.clone()),
        )
    }
    fn value(&self) -> &i64 {
        &self.id
    }
}

#[cfg(test)]
mod tests;
