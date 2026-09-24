use std::{collections::HashMap, sync::Arc, time::Duration};

use board::{BoardDestination, BoardView, BoardViewEvent};
use calendar::{
    CalendarEntryRecord, CalendarListRecord, CalendarListRole, CalendarPage, CalendarRoute,
    CalendarService, CalendarSnapshot, CalendarTask, CalendarWorkspace, CalendarWorkspaceEvent,
    CreateCalendarRecurrence, EntryLifecycleState, SaveCalendarEntry,
};
use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate};
use gpui_kit::base::{
    NavMotion, NavOperation, NavStack, NavStackState,
    motion::{PresencePhase, Transition},
};
use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, FocusHandle, Focusable, Global, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, Styled, Window, actions, div, relative,
};
use runtime::AppRuntime;
use serde::Serialize;
use serde::de::DeserializeOwned;
use workflow::{
    SaveWorkflowRequest, WorkflowListEntry, WorkflowListRecord, WorkflowPage, WorkflowRecord,
    WorkflowRoute, WorkflowRunHistoryEntry, WorkflowRunStatus, WorkflowService, WorkflowSnapshot,
    WorkflowTask, WorkflowWorkspace, WorkflowWorkspaceEvent,
};

actions!(board_navigation, [NavigateBack]);
struct NavigationBindings;
impl Global for NavigationBindings {}

#[derive(Clone)]
struct StorageCalendarService {
    runtime: AppRuntime,
}

impl StorageCalendarService {
    fn new(runtime: AppRuntime) -> Self {
        Self { runtime }
    }
}

impl CalendarService for StorageCalendarService {
    fn load(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        board_id: u32,
        month: NaiveDate,
    ) -> CalendarTask<CalendarSnapshot> {
        let runtime = self.runtime.clone();
        let from = month.to_string();
        let through = (month.with_day(1).unwrap_or(month) + ChronoDuration::days(32))
            .with_day(1)
            .unwrap_or(month)
            - ChronoDuration::days(1);
        let through = through.to_string();
        let today = Local::now().date_naive().to_string();
        runtime.spawn_store(&executor, move |store| async move {
            let today =
                calendar::CalendarDate::parse(&today).map_err(|error| anyhow::anyhow!(error))?;
            let materialized =
                storage::calendar::materialize_scheduled_recurring_tasks(&store, today).await?;
            let workflow_report = storage::workflow::run_scheduled_workflows(
                &store,
                Some(i64::from(board_id)),
                "calendar_open",
                workflow::EventOrigin::Scheduler,
            )
            .await?;
            let workflow_changed_board = workflow_report
                .runs
                .iter()
                .flat_map(|run| &run.actions)
                .any(|action| {
                    !matches!(
                        action,
                        workflow::WorkflowAction::Notify { .. }
                            | workflow::WorkflowAction::AddHistory { .. }
                    )
                });
            let board_changed = materialized > 0 || workflow_changed_board;
            let entries = storage::calendar::load_entries(
                &store,
                Some(&from),
                Some(&through),
                Some(i64::from(board_id)),
            );
            let recurring = storage::calendar::list_recurring_tasks(&store, i64::from(board_id));
            let snapshot = storage::board::load_board_snapshot(&store, board_id).await?;
            let (entries, recurring) = tokio::try_join!(entries, recurring)?;
            Ok(CalendarSnapshot {
                entries: entries.into_iter().map(calendar_entry).collect(),
                recurring: recurring
                    .into_iter()
                    .map(transcode)
                    .collect::<anyhow::Result<Vec<_>>>()?,
                lists: snapshot.cards.into_iter().map(calendar_list).collect(),
                board_changed,
            })
        })
    }

    fn save_entry(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        request: SaveCalendarEntry,
    ) -> CalendarTask<()> {
        let runtime = self.runtime.clone();
        runtime.spawn_store(&executor, move |store| async move {
            let due_on = request.due_on.clone();
            store
                .mutations(storage::MutationOrigin::LocalApp)
                .update_entry(storage::workspace::api::UpdateEntryInput {
                    entry_id: request.entry_id,
                    title: Some(request.title),
                    description: None,
                    due_on,
                    clear_due_on: request.due_on.is_none(),
                })
                .await?;
            store
                .mutations(storage::MutationOrigin::LocalApp)
                .set_entry_lifecycle(storage::workspace::api::SetEntryLifecycleInput {
                    entry_id: request.entry_id,
                    state: match request.status {
                        EntryLifecycleState::Open => {
                            storage::workspace::api::EntryLifecycleState::Open
                        }
                        EntryLifecycleState::Completed => {
                            storage::workspace::api::EntryLifecycleState::Completed
                        }
                        EntryLifecycleState::Cancelled => {
                            storage::workspace::api::EntryLifecycleState::Cancelled
                        }
                    },
                })
                .await?;
            Ok(())
        })
    }

    fn create_recurring_task(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        request: CreateCalendarRecurrence,
    ) -> CalendarTask<()> {
        let runtime = self.runtime.clone();
        runtime.spawn_store(&executor, move |store| async move {
            store
                .mutations(storage::MutationOrigin::LocalApp)
                .create_recurring_task(storage::workspace::api::CreateRecurringTaskInput {
                    entry_id: request.entry_id,
                    start_on: request.start_on,
                    rule: request.rule,
                    until_on: request.until_on,
                    occurrence_limit: None,
                    generation_mode: Some(
                        match request.generation_mode {
                            calendar::GenerationMode::OnSchedule => "on_schedule",
                            calendar::GenerationMode::OnCompletion => "on_completion",
                        }
                        .to_string(),
                    ),
                })
                .await?;
            Ok(())
        })
    }

    fn delete_recurring_task(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        entry_id: i64,
    ) -> CalendarTask<()> {
        let runtime = self.runtime.clone();
        runtime.spawn_store(&executor, move |store| async move {
            store
                .mutations(storage::MutationOrigin::LocalApp)
                .delete_recurring_task(storage::workspace::api::RecurringTaskInput { entry_id })
                .await
        })
    }

    fn reschedule_entry(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        entry_id: i64,
        due_on: String,
    ) -> CalendarTask<()> {
        let runtime = self.runtime.clone();
        runtime.spawn_store(&executor, move |store| async move {
            store
                .mutations(storage::MutationOrigin::LocalApp)
                .set_entry_schedule(storage::workspace::api::SetEntryScheduleInput {
                    entry_id,
                    start_on: None,
                    due_on: Some(due_on),
                    clear_start_on: false,
                    clear_due_on: false,
                })
                .await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct StorageWorkflowService {
    runtime: AppRuntime,
}

impl StorageWorkflowService {
    fn new(runtime: AppRuntime) -> Self {
        Self { runtime }
    }
}

impl WorkflowService for StorageWorkflowService {
    fn load(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        board_id: u32,
    ) -> WorkflowTask<WorkflowSnapshot> {
        let runtime = self.runtime.clone();
        runtime.spawn_store(&executor, move |store| async move {
            let records = storage::workflow::list_workflows(&store, i64::from(board_id)).await?;
            let board = storage::board::load_board_snapshot(&store, board_id).await?;
            let workflows = records
                .into_iter()
                .map(workflow_record)
                .collect::<anyhow::Result<Vec<_>>>()?;
            let lists = board
                .cards
                .into_iter()
                .map(workflow_list)
                .collect::<Vec<_>>();
            let workflow_names = workflows
                .iter()
                .map(|record| (record.id, record.name.clone()))
                .collect::<HashMap<_, _>>();
            let entry_titles = lists
                .iter()
                .flat_map(|list| {
                    list.entries
                        .iter()
                        .map(|entry| (i64::from(entry.id), entry.title.clone()))
                })
                .collect::<HashMap<_, _>>();
            let runs = storage::workflow::list_workflow_runs(&store, i64::from(board_id), 20)
                .await?
                .into_iter()
                .map(|run| WorkflowRunHistoryEntry {
                    id: run.id,
                    workflow_id: run.workflow_id,
                    workflow_name: workflow_names
                        .get(&run.workflow_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Workflow {}", run.workflow_id)),
                    entry_title: run
                        .entry_id
                        .and_then(|entry_id| entry_titles.get(&entry_id).cloned()),
                    entry_id: run.entry_id,
                    trigger_kind: run.trigger_kind,
                    status: workflow_run_status(run.status),
                    actions: run.actions,
                    error: run.error,
                    started_at: run.started_at,
                    finished_at: run.finished_at,
                })
                .collect();
            Ok(WorkflowSnapshot {
                workflows,
                lists,
                runs,
            })
        })
    }

    fn save(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        request: SaveWorkflowRequest,
    ) -> WorkflowTask<WorkflowRecord> {
        let runtime = self.runtime.clone();
        runtime.spawn_store(&executor, move |store| async move {
            let definition = serde_json::to_value(&request.definition)?;
            let record = store
                .mutations(storage::MutationOrigin::LocalApp)
                .save_workflow(storage::workspace::api::SaveWorkflowInput {
                    workflow_id: request.workflow_id,
                    board_id: request.board_id,
                    name: request.name,
                    enabled: request.enabled,
                    definition,
                })
                .await?;
            workflow_record(record)
        })
    }

    fn run_manual(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        board_id: u32,
        entry_id: i64,
    ) -> WorkflowTask<usize> {
        let runtime = self.runtime.clone();
        runtime.spawn_store(&executor, move |store| async move {
            let report = storage::workflow::run_manual_workflow(
                &store,
                i64::from(board_id),
                entry_id,
                workflow::EventOrigin::User,
            )
            .await?;
            Ok(report.runs.len())
        })
    }
}

fn transcode<T, U>(value: U) -> anyhow::Result<T>
where
    T: DeserializeOwned,
    U: Serialize,
{
    Ok(serde_json::from_value(serde_json::to_value(value)?)?)
}

fn calendar_role(role: storage::board::ListWorkflowRole) -> CalendarListRole {
    match role {
        storage::board::ListWorkflowRole::Neutral => CalendarListRole::Neutral,
        storage::board::ListWorkflowRole::Done => CalendarListRole::Done,
        storage::board::ListWorkflowRole::Cancelled => CalendarListRole::Cancelled,
    }
}

fn calendar_entry(entry: storage::calendar::CalendarEntryRecord) -> CalendarEntryRecord {
    CalendarEntryRecord {
        entry_id: entry.entry_id,
        title: entry.title,
        description: entry.description,
        start_on: entry.start_on,
        due_on: entry.due_on,
        list_id: entry.list_id,
        list_title: entry.list_title,
        board_id: entry.board_id,
        board_title: entry.board_title,
        workflow_role: calendar_role(entry.workflow_role),
        completed_at: entry.completed_at,
        cancelled_at: entry.cancelled_at,
        recurrence_series_id: entry.recurrence_series_id,
        occurrence_key: entry.occurrence_key,
    }
}

fn calendar_list(list: storage::board::BoardListRecord) -> CalendarListRecord {
    CalendarListRecord {
        id: list.id,
        title: list.title,
        entries: list
            .entries
            .into_iter()
            .map(|entry| calendar::CalendarListEntry {
                id: entry.id,
                title: entry.title,
                due_on: entry.due_on,
            })
            .collect(),
    }
}

fn workflow_record(record: storage::workflow::WorkflowRecord) -> anyhow::Result<WorkflowRecord> {
    Ok(WorkflowRecord {
        id: record.id,
        board_id: record.board_id,
        name: record.name,
        enabled: record.enabled,
        definition: transcode(record.definition)?,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

fn workflow_list(list: storage::board::BoardListRecord) -> WorkflowListRecord {
    WorkflowListRecord {
        id: list.id,
        title: list.title,
        entries: list
            .entries
            .into_iter()
            .map(|entry| WorkflowListEntry {
                id: entry.id,
                title: entry.title,
            })
            .collect(),
    }
}

fn workflow_run_status(status: storage::workflow::WorkflowRunStatus) -> WorkflowRunStatus {
    match status {
        storage::workflow::WorkflowRunStatus::Running => WorkflowRunStatus::Running,
        storage::workflow::WorkflowRunStatus::Succeeded => WorkflowRunStatus::Succeeded,
        storage::workflow::WorkflowRunStatus::Failed => WorkflowRunStatus::Failed,
        storage::workflow::WorkflowRunStatus::Skipped => WorkflowRunStatus::Skipped,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Destination {
    Board,
    Calendar(CalendarRoute),
    Workflow(WorkflowRoute),
}

impl Destination {
    fn parent(self) -> Option<Self> {
        match self {
            Self::Board => None,
            Self::Calendar(CalendarRoute::Month) | Self::Workflow(WorkflowRoute::Overview) => {
                Some(Self::Board)
            }
            Self::Calendar(CalendarRoute::Item | CalendarRoute::Recurring) => {
                Some(Self::Calendar(CalendarRoute::Month))
            }
            Self::Workflow(WorkflowRoute::Editor) => Some(Self::Workflow(WorkflowRoute::Overview)),
            Self::Workflow(_) => Some(Self::Workflow(WorkflowRoute::Editor)),
        }
    }
}

struct RetainedPage {
    view: AnyView,
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
}

pub(super) struct BoardNavigation {
    board_id: u32,
    board: Entity<BoardView>,
    stack: Entity<NavStackState>,
    pages: HashMap<Destination, RetainedPage>,
    calendar: Option<Entity<CalendarWorkspace>>,
    workflow: Option<Entity<WorkflowWorkspace>>,
    calendar_service: Arc<dyn CalendarService>,
    workflow_service: Arc<dyn WorkflowService>,
}

impl BoardNavigation {
    pub(super) fn new(
        board_id: u32,
        board: Entity<BoardView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if !cx.has_global::<NavigationBindings>() {
            cx.set_global(NavigationBindings);
            cx.bind_keys([KeyBinding::new(
                "alt-left",
                NavigateBack,
                Some("BoardNavigation"),
            )]);
        }
        let stack = cx.new(|cx| {
            let mut stack = NavStackState::new();
            stack.push(board.clone(), NavMotion::Immediate, cx);
            stack
        });
        cx.observe(&stack, |_, _, cx| cx.notify()).detach();
        cx.subscribe_in(&board, window, |this, _, event, window, cx| {
            if let BoardViewEvent::Navigate(destination) = event {
                let destination = match destination {
                    BoardDestination::Calendar => Destination::Calendar(CalendarRoute::Month),
                    BoardDestination::Workflows => Destination::Workflow(WorkflowRoute::Overview),
                };
                this.navigate(destination, NavMotion::Animated, window, cx);
                this.refresh_destination(destination, window, cx);
            }
        })
        .detach();
        let focus = board.read(cx).focus_handle(cx);
        let runtime = cx.global::<AppRuntime>().clone();
        let calendar_service: Arc<dyn CalendarService> =
            Arc::new(StorageCalendarService::new(runtime.clone()));
        let workflow_service: Arc<dyn WorkflowService> =
            Arc::new(StorageWorkflowService::new(runtime));
        let pages = HashMap::from([(
            Destination::Board,
            RetainedPage {
                view: board.clone().into(),
                focus,
                return_focus: None,
            },
        )]);
        Self {
            board_id,
            board,
            stack,
            pages,
            calendar: None,
            workflow: None,
            calendar_service,
            workflow_service,
        }
    }

    fn current(&self, cx: &App) -> Destination {
        let current = self.stack.read(cx).current().map(|view| view.entity_id());
        self.pages
            .iter()
            .find(|(_, page)| Some(page.view.entity_id()) == current)
            .map(|(destination, _)| *destination)
            .unwrap_or(Destination::Board)
    }

    pub(super) fn is_board(&self, cx: &App) -> bool {
        self.current(cx) == Destination::Board
    }

    pub(super) fn focus_current(&self, window: &mut Window, cx: &mut App) {
        if let Some(page) = self.pages.get(&self.current(cx)) {
            page.focus.focus(window, cx);
        }
    }

    pub(super) fn return_to_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Destination::Board, NavMotion::Immediate, window, cx);
    }

    fn refresh_destination(
        &self,
        destination: Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match destination {
            Destination::Calendar(_) => {
                if let Some(model) = &self.calendar {
                    model.update(cx, |model, cx| model.refresh(window, cx));
                }
            }
            Destination::Workflow(_) => {
                if let Some(model) = &self.workflow {
                    model.update(cx, |model, cx| model.refresh(window, cx));
                }
            }
            _ => {}
        }
    }

    fn ensure_page(
        &mut self,
        destination: Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pages.contains_key(&destination) {
            return;
        }
        let (view, focus) = match destination {
            Destination::Calendar(route) => {
                let model = if let Some(model) = &self.calendar {
                    model.clone()
                } else {
                    let service = self.calendar_service.clone();
                    let model =
                        cx.new(|cx| CalendarWorkspace::new(self.board_id, service, window, cx));
                    cx.subscribe_in(&model, window, |this, _, event, window, cx| match event {
                        CalendarWorkspaceEvent::Navigate(route) => this.navigate(
                            Destination::Calendar(*route),
                            NavMotion::Animated,
                            window,
                            cx,
                        ),
                        CalendarWorkspaceEvent::Back => this.back(window, cx),
                        CalendarWorkspaceEvent::Board => {
                            this.navigate(Destination::Board, NavMotion::Animated, window, cx)
                        }
                        CalendarWorkspaceEvent::Committed(board_id) => {
                            this.committed(*board_id, cx)
                        }
                    })
                    .detach();
                    self.calendar = Some(model.clone());
                    model
                };
                let page = cx.new(|cx| CalendarPage::new(model, route, cx));
                let focus = page.read(cx).focus_handle(cx);
                (page.into(), focus)
            }
            Destination::Workflow(route) => {
                let model = if let Some(model) = &self.workflow {
                    model.clone()
                } else {
                    let service = self.workflow_service.clone();
                    let model =
                        cx.new(|cx| WorkflowWorkspace::new(self.board_id, service, window, cx));
                    cx.subscribe_in(&model, window, |this, _, event, window, cx| match event {
                        WorkflowWorkspaceEvent::Navigate(route) => this.navigate(
                            Destination::Workflow(*route),
                            NavMotion::Animated,
                            window,
                            cx,
                        ),
                        WorkflowWorkspaceEvent::Back => this.back(window, cx),
                        WorkflowWorkspaceEvent::Board => {
                            this.navigate(Destination::Board, NavMotion::Animated, window, cx)
                        }
                        WorkflowWorkspaceEvent::Committed(board_id) => {
                            this.committed(*board_id, cx);
                            if let Some(calendar) = &this.calendar {
                                calendar.update(cx, |model, cx| model.refresh(window, cx));
                            }
                        }
                    })
                    .detach();
                    self.workflow = Some(model.clone());
                    model
                };
                let page = cx.new(|cx| WorkflowPage::new(model, route, cx));
                let focus = page.read(cx).focus_handle(cx);
                (page.into(), focus)
            }
            Destination::Board => return,
        };
        self.pages.insert(
            destination,
            RetainedPage {
                view,
                focus,
                return_focus: None,
            },
        );
    }

    fn committed(&self, board_id: u32, cx: &mut Context<Self>) {
        if board_id != self.board_id {
            return;
        }
        self.board.update(cx, |board, cx| {
            board.reload_board(board_id, cx);
            cx.emit(BoardViewEvent::DataCommitted {
                board_id,
                links_changed: false,
            });
        });
    }

    fn navigate(
        &mut self,
        destination: Destination,
        motion: NavMotion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.current(cx) == destination {
            return;
        }
        self.ensure_page(destination, window, cx);
        let Some(page) = self.pages.get(&destination) else {
            return;
        };
        let id = page.view.entity_id();
        let existing = self
            .stack
            .read(cx)
            .views()
            .position(|view| view.entity_id() == id);
        if let Some(index) = existing {
            while self.stack.read(cx).depth() > index + 1 {
                let last = self.stack.read(cx).depth() == index + 2;
                self.stack.update(cx, |stack, cx| {
                    stack.pop(if last { motion } else { NavMotion::Immediate }, cx);
                });
            }
        } else {
            if let Some(parent) = destination.parent() {
                self.navigate(parent, NavMotion::Immediate, window, cx);
            }
            let focused = window.focused(cx);
            if let Some(parent) = destination
                .parent()
                .and_then(|parent| self.pages.get_mut(&parent))
            {
                parent.return_focus = focused;
            }
            let Some(page) = self.pages.get(&destination) else {
                return;
            };
            self.stack
                .update(cx, |stack, cx| stack.push(page.view.clone(), motion, cx));
        }
        if let Some(page) = self.pages.get(&destination) {
            page.return_focus
                .as_ref()
                .unwrap_or(&page.focus)
                .focus(window, cx);
        }
        cx.notify();
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(parent) = self.current(cx).parent() {
            self.navigate(parent, NavMotion::Animated, window, cx);
        }
    }
}

impl Render for BoardNavigation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .overflow_hidden()
            .key_context("BoardNavigation")
            .on_action(cx.listener(|this, _: &NavigateBack, window, cx| {
                this.back(window, cx);
                cx.stop_propagation();
            }))
            .child(
                NavStack::new(&self.stack)
                    .size_full()
                    .overflow_hidden()
                    .transition(Transition::new(Duration::from_millis(180)))
                    .item(|page, _, _| {
                        let offset = match (page.phase(), page.operation()) {
                            (
                                PresencePhase::Entering,
                                Some(NavOperation::Push | NavOperation::Replace),
                            ) => 1. - page.progress(),
                            (PresencePhase::Exiting, Some(NavOperation::Pop)) => page.progress(),
                            _ => 0.,
                        };
                        page.w_full().left(relative(offset)).into_any_element()
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{BoardNavigation, Destination, NavigateBack};
    use board::BoardView;
    use calendar::CalendarRoute;
    use gpui_kit::base::NavMotion;
    use gpui_kit::{
        AppContext, Context, Entity, Focusable, IntoElement, ParentElement, Render, Styled,
        TestAppContext, VisualTestContext, Window, div, px, size,
    };
    use workflow::WorkflowRoute;

    struct ShellFrame(Entity<BoardNavigation>);
    impl Render for ShellFrame {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .child(div().w(px(220.)).h_full())
                .child(div().flex_1().min_w_0().h_full().child(self.0.clone()))
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }

    #[gpui_kit::test]
    fn board_page_navigation_retains_root_and_uses_real_toolbar(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let _guard = runtime.enter();
        let store = runtime
            .block_on(storage::Store::connect(
                storage::StoreOptions::new("sqlite::memory:").connection_pool(1, 1),
            ))
            .expect("store");
        let board = runtime
            .block_on(
                store
                    .mutations(storage::MutationOrigin::LocalApp)
                    .create_board(storage::workspace::api::CreateBoardInput {
                        title: "Navigation test".into(),
                        project_id: None,
                    }),
            )
            .expect("board");
        let mut navigation = None;
        let window = cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(runtime::AppRuntime::new(store, std::env::temp_dir()));
            cx.open_window(Default::default(), |window, cx| {
                let board_view = BoardView::view(window, cx);
                board_view.update(cx, |view, cx| view.load_board(board.id as u32, cx));
                let host =
                    cx.new(|cx| BoardNavigation::new(board.id as u32, board_view, window, cx));
                navigation = Some(host.clone());
                let frame = cx.new(|_| ShellFrame(host));
                cx.new(|cx| gpui_kit::component::Root::new(frame, window, cx))
            })
            .expect("window")
        });
        let host = navigation.expect("host");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(1440.), px(900.)));
        for _ in 0..500 {
            draw(&mut cx);
            if cx.debug_bounds("open-board-workflows").is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let root = host.read_with(&cx, |host, _| host.board.entity_id());
        cx.update(|window, cx| {
            host.read(cx)
                .board
                .read(cx)
                .focus_handle(cx)
                .focus(window, cx)
        });
        let original_focus = cx.update(|window, cx| window.focused(cx));
        let button = cx
            .debug_bounds("open-board-workflows")
            .expect("workflows button on empty board");
        cx.simulate_click(button.center(), Default::default());
        draw(&mut cx);
        assert_eq!(
            host.read_with(&cx, |host, cx| host.current(cx)),
            Destination::Workflow(WorkflowRoute::Overview)
        );
        assert!(!host.read_with(&cx, |host, cx| host.is_board(cx)));
        let model = host.read_with(&cx, |host, _| host.workflow.clone().expect("workflow"));
        cx.update(|window, cx| model.update(cx, |model, cx| model.new_workflow(window, cx)));
        draw(&mut cx);
        assert_eq!(
            host.read_with(&cx, |host, cx| host.current(cx)),
            Destination::Workflow(WorkflowRoute::Editor)
        );
        let depth = host.read_with(&cx, |host, cx| host.stack.read(cx).depth());
        cx.update(|window, cx| {
            host.update(cx, |host, cx| {
                host.navigate(
                    Destination::Workflow(WorkflowRoute::Editor),
                    NavMotion::Animated,
                    window,
                    cx,
                )
            })
        });
        assert_eq!(
            host.read_with(&cx, |host, cx| host.stack.read(cx).depth()),
            depth
        );
        cx.dispatch_action(NavigateBack);
        draw(&mut cx);
        assert_eq!(
            host.read_with(&cx, |host, cx| host.current(cx)),
            Destination::Workflow(WorkflowRoute::Overview)
        );
        cx.dispatch_action(NavigateBack);
        draw(&mut cx);
        assert!(host.read_with(&cx, |host, cx| host.is_board(cx)));
        assert_eq!(cx.update(|window, cx| window.focused(cx)), original_focus);
        cx.update(|window, cx| {
            host.update(cx, |host, cx| {
                host.navigate(
                    Destination::Calendar(CalendarRoute::Month),
                    NavMotion::Immediate,
                    window,
                    cx,
                )
            })
        });
        draw(&mut cx);
        let calendar_page = host.read_with(&cx, |host, _| {
            host.pages[&Destination::Calendar(CalendarRoute::Month)]
                .view
                .entity_id()
        });
        assert!(
            cx.debug_bounds("calendar-up-next").is_some(),
            "wide calendar should start with its Upcoming rail"
        );
        let calendar_width = cx
            .debug_bounds("calendar-page-Month")
            .expect("settled month page")
            .size
            .width;
        for route in [CalendarRoute::Recurring, CalendarRoute::Month] {
            cx.update(|window, cx| {
                host.update(cx, |host, cx| {
                    host.navigate(
                        Destination::Calendar(route),
                        NavMotion::Animated,
                        window,
                        cx,
                    )
                })
            });
            draw(&mut cx);
            if route == CalendarRoute::Recurring {
                assert_eq!(
                    cx.debug_bounds("calendar-page-Recurring")
                        .expect("entering recurring page")
                        .size
                        .width,
                    calendar_width,
                    "navigation motion must not change the responsive layout width"
                );
            } else {
                assert!(
                    cx.debug_bounds("calendar-up-next").is_some(),
                    "returning from Recurring must include Upcoming in the first calendar frame"
                );
            }
        }
        cx.update(|window, cx| host.update(cx, |host, cx| host.return_to_board(window, cx)));
        draw(&mut cx);
        cx.update(|window, cx| {
            host.update(cx, |host, cx| {
                host.navigate(
                    Destination::Calendar(CalendarRoute::Month),
                    NavMotion::Animated,
                    window,
                    cx,
                )
            })
        });
        draw(&mut cx);
        let other = cx.update(|window, cx| {
            let board = BoardView::view(window, cx);
            cx.new(|cx| BoardNavigation::new(999, board, window, cx))
        });
        cx.update(|window, cx| {
            other.update(cx, |host, cx| {
                host.navigate(
                    Destination::Workflow(WorkflowRoute::Overview),
                    NavMotion::Immediate,
                    window,
                    cx,
                )
            })
        });
        assert_eq!(
            host.read_with(&cx, |host, cx| host.current(cx)),
            Destination::Calendar(CalendarRoute::Month)
        );
        assert_eq!(
            other.read_with(&cx, |host, cx| host.current(cx)),
            Destination::Workflow(WorkflowRoute::Overview)
        );
        assert_ne!(
            host.read_with(&cx, |host, _| host.board.entity_id()),
            other.read_with(&cx, |host, _| host.board.entity_id())
        );
        host.read_with(&cx, |host, _| {
            assert_eq!(host.board.entity_id(), root);
            assert_eq!(
                host.pages[&Destination::Calendar(CalendarRoute::Month)]
                    .view
                    .entity_id(),
                calendar_page
            );
        });
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn navigation_transitions_fit_windows_stack() {
        let output = std::process::Command::new(std::env::current_exe().expect("executable"))
            .args([
                "--exact",
                "board_navigation::tests::board_page_navigation_retains_root_and_uses_real_toolbar",
                "--nocapture",
            ])
            .env("RUST_MIN_STACK", "1048576")
            .output()
            .expect("launch");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
