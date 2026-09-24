use std::sync::{Arc, Mutex};

use super::{
    SaveWorkflowRequest, WorkflowListRecord, WorkflowPage, WorkflowRecord, WorkflowRoute,
    WorkflowRunHistoryEntry, WorkflowRunStatus, WorkflowService, WorkflowSnapshot, WorkflowTask,
    WorkflowWorkspace, WorkflowWorkspaceEvent, role_action_definition,
};
use crate::{ListWorkflowRole, WorkflowAction};
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, ScrollDelta, ScrollWheelEvent,
    Styled, TestAppContext, VisualTestContext, Window, div, point, px, size,
};

struct RetainedWorkflowPages {
    editor: Entity<WorkflowPage>,
    overview: Entity<WorkflowPage>,
}

impl Render for RetainedWorkflowPages {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .child(div().w(px(1200.)).h_full().child(self.editor.clone()))
            .child(div().w(px(600.)).h_full().child(self.overview.clone()))
    }
}

#[derive(Clone)]
struct TestWorkflowService {
    snapshot: Arc<Mutex<WorkflowSnapshot>>,
    fail_writes: bool,
}

impl TestWorkflowService {
    fn failing() -> Arc<Self> {
        Arc::new(Self {
            snapshot: Arc::new(Mutex::new(WorkflowSnapshot::default())),
            fail_writes: true,
        })
    }

    fn in_memory(snapshot: WorkflowSnapshot) -> Arc<Self> {
        Arc::new(Self {
            snapshot: Arc::new(Mutex::new(snapshot)),
            fail_writes: false,
        })
    }

    fn task<T: Send + 'static>(
        executor: gpui_kit::BackgroundExecutor,
        result: anyhow::Result<T>,
    ) -> WorkflowTask<T> {
        executor.spawn(async move { Ok(result) })
    }
}

impl WorkflowService for TestWorkflowService {
    fn load(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        _: u32,
    ) -> WorkflowTask<WorkflowSnapshot> {
        let result = self
            .snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| anyhow::anyhow!("workflow test service lock poisoned"));
        Self::task(executor, result)
    }

    fn save(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        request: SaveWorkflowRequest,
    ) -> WorkflowTask<WorkflowRecord> {
        if self.fail_writes {
            return Self::task(executor, Err(anyhow::anyhow!("service unavailable")));
        }
        let result = self
            .snapshot
            .lock()
            .map_err(|_| anyhow::anyhow!("workflow test service lock poisoned"))
            .map(|mut snapshot| {
                let id = request.workflow_id.unwrap_or_else(|| {
                    snapshot
                        .workflows
                        .iter()
                        .map(|workflow| workflow.id)
                        .max()
                        .unwrap_or(0)
                        + 1
                });
                let record = WorkflowRecord {
                    id,
                    board_id: request.board_id,
                    name: request.name,
                    enabled: request.enabled,
                    definition: request.definition,
                    created_at: 1,
                    updated_at: 1,
                };
                if let Some(existing) = snapshot.workflows.iter_mut().find(|item| item.id == id) {
                    *existing = record.clone();
                } else {
                    snapshot.workflows.push(record.clone());
                }
                record
            });
        Self::task(executor, result)
    }

    fn run_manual(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        _: u32,
        _: i64,
    ) -> WorkflowTask<usize> {
        Self::task(executor, Ok(0))
    }
}

#[gpui_kit::test]
fn manual_run_page_uses_wide_history_column_and_stacks_narrowly(cx: &mut TestAppContext) {
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service, window, cx));
            model.update(cx, |model, cx| {
                model.state.runs.push(WorkflowRunHistoryEntry {
                    id: 1,
                    workflow_id: 2,
                    workflow_name: "Complete on Done".into(),
                    entry_id: Some(3),
                    entry_title: Some("Publish release".into()),
                    trigger_kind: "manual".into(),
                    status: WorkflowRunStatus::Failed,
                    actions: vec![WorkflowAction::MarkComplete],
                    error: Some("The item could not be updated".into()),
                    started_at: 1_758_000_000,
                    finished_at: Some(1_758_000_001),
                });
                cx.notify();
            });
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Run, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1400.), px(800.)));
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }

    let controls = cx
        .debug_bounds("workflow-run-controls")
        .expect("run controls");
    let history = cx
        .debug_bounds("workflow-run-history")
        .expect("run history");
    assert!(history.origin.x >= controls.right());
    assert!(cx.debug_bounds("workflow-run-1").is_some());
    assert!(cx.debug_bounds("workflow-run-error-1").is_some());
    assert!(cx.debug_bounds("workflow-breadcrumb-editor").is_none());

    cx.simulate_resize(size(px(800.), px(800.)));
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }
    let controls = cx
        .debug_bounds("workflow-run-controls")
        .expect("run controls in narrow layout");
    let history = cx
        .debug_bounds("workflow-run-history")
        .expect("run history in narrow layout");
    assert!(history.origin.y >= controls.bottom());
}

#[gpui_kit::test]
fn retained_workflow_routes_do_not_share_responsive_measurement(cx: &mut TestAppContext) {
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service, window, cx));
            let editor = cx.new(|cx| WorkflowPage::new(model.clone(), WorkflowRoute::Editor, cx));
            let overview = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Overview, cx));
            let pages = cx.new(|_| RetainedWorkflowPages { editor, overview });
            cx.new(|cx| gpui_kit::component::Root::new(pages, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1800.), px(700.)));

    for _ in 0..2 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }

    assert!(
        cx.debug_bounds("workflow-builder-sidebar").is_some(),
        "a narrow retained route must not remove the builder from the wide editor route"
    );
}

#[gpui_kit::test]
fn workflow_refresh_keeps_overview_content_stable(cx: &mut TestAppContext) {
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let mut workspace = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service, window, cx));
            workspace = Some(model.clone());
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Overview, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let model = workspace.expect("workspace");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let before = cx
        .debug_bounds("workflow-starter-panel")
        .expect("starter panel before refresh");

    cx.update(|window, cx| {
        model.update(cx, |model, cx| {
            model.refresh(window, cx);
        });
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });

    assert_eq!(
        cx.debug_bounds("workflow-starter-panel"),
        Some(before),
        "a quick refresh must preserve the useful overview instead of flashing a loading row"
    );
}

#[gpui_kit::test]
fn workflow_editor_errors_do_not_leak_into_other_routes(cx: &mut TestAppContext) {
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service, window, cx));
            model.update(cx, |model, cx| {
                model.draft.error = Some("Add a trigger before saving".into());
                cx.notify();
            });
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Overview, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });

    assert!(
        cx.debug_bounds("workflow-editor-error").is_none(),
        "editor validation belongs to the editor rather than shifting every workflow route"
    );
}

#[gpui_kit::test]
fn workflow_overview_scrollbar_stays_aligned_with_its_viewport_at_bottom(cx: &mut TestAppContext) {
    let workflows = (1..=12)
        .map(|id| WorkflowRecord {
            id,
            board_id: 7,
            name: format!("Workflow {id}"),
            enabled: true,
            definition: role_action_definition(
                ListWorkflowRole::Done,
                WorkflowAction::MarkComplete,
                &format!("Workflow {id}"),
            ),
            created_at: id,
            updated_at: id,
        })
        .collect();
    let snapshot = WorkflowSnapshot {
        workflows,
        lists: Vec::new(),
        runs: Vec::new(),
    };
    let service = TestWorkflowService::in_memory(snapshot.clone());
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service, window, cx));
            model.update(cx, |model, cx| {
                model.state.workflows = snapshot.workflows;
                cx.notify();
            });
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Overview, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(760.), px(420.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });

    let viewport = cx
        .debug_bounds("workflow-overview-scroll")
        .expect("overview viewport");
    let before = cx.debug_bounds("scrollbar-overlay").expect("scrollbar");
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
        ..Default::default()
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let after = cx
        .debug_bounds("scrollbar-overlay")
        .expect("scrollbar after scrolling");

    assert_eq!(before, after, "the scrollbar overlay must remain fixed");
    assert_eq!(
        after, viewport,
        "the scrollbar should use the visible workflow viewport at the bottom"
    );
}

#[gpui_kit::test]
fn workflow_canvas_scrollbar_stays_aligned_with_its_viewport_at_bottom(cx: &mut TestAppContext) {
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service, window, cx));
            model.update(cx, |model, cx| {
                model.apply_workflow_definition(
                    role_action_definition(
                        ListWorkflowRole::Done,
                        WorkflowAction::MarkComplete,
                        "Complete on Done",
                    ),
                    window,
                    cx,
                );
            });
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Editor, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(800.), px(400.)));
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }

    let viewport = cx
        .debug_bounds("workflow-canvas-scroll")
        .expect("canvas viewport");
    let before = cx.debug_bounds("scrollbar-overlay").expect("scrollbar");
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
        ..Default::default()
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let after = cx
        .debug_bounds("scrollbar-overlay")
        .expect("scrollbar after scrolling");

    assert_eq!(before, after, "the canvas scrollbar must remain fixed");
    assert_eq!(
        after, viewport,
        "the scrollbar should use the visible canvas viewport at the bottom"
    );
}

#[gpui_kit::test]
fn workflow_edge_labels_do_not_overlap_connected_cards(cx: &mut TestAppContext) {
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service, window, cx));
            model.update(cx, |model, cx| {
                model.apply_workflow_definition(
                    role_action_definition(
                        ListWorkflowRole::Done,
                        WorkflowAction::MarkComplete,
                        "Complete on Done",
                    ),
                    window,
                    cx,
                );
            });
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Editor, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.), px(768.)));
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }

    let source = cx
        .debug_bounds("workflow-node-trigger")
        .expect("trigger card");
    let target = cx
        .debug_bounds("workflow-node-condition")
        .expect("condition card");
    let label = cx
        .debug_bounds("workflow-edge-label-edge-1")
        .expect("edge label");
    let overlaps = |a: gpui_kit::Bounds<gpui_kit::Pixels>,
                    b: gpui_kit::Bounds<gpui_kit::Pixels>| {
        a.origin.x < b.origin.x + b.size.width
            && a.origin.x + a.size.width > b.origin.x
            && a.origin.y < b.origin.y + b.size.height
            && a.origin.y + a.size.height > b.origin.y
    };

    assert!(!overlaps(label, source), "edge label overlaps source card");
    assert!(!overlaps(label, target), "edge label overlaps target card");
}

#[gpui_kit::test]
fn workflow_editor_breadcrumb_and_properties_follow_the_draft(cx: &mut TestAppContext) {
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let mut workspace = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service, window, cx));
            model.update(cx, |model, cx| {
                model.apply_workflow_definition(
                    role_action_definition(
                        ListWorkflowRole::Done,
                        WorkflowAction::MarkComplete,
                        "Complete on Done",
                    ),
                    window,
                    cx,
                );
            });
            workspace = Some(model.clone());
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Editor, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let model = workspace.expect("workspace");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.), px(768.)));
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }

    let header = cx.debug_bounds("workflow-header").expect("header");
    let name = cx
        .debug_bounds("workflow-breadcrumb-current")
        .expect("current workflow breadcrumb");
    assert!(cx.debug_bounds("workflow-breadcrumb-board").is_some());
    assert!(cx.debug_bounds("workflow-breadcrumb-overview").is_some());
    assert!(name.origin.y < header.origin.y + header.size.height);
    let sidebar = cx
        .debug_bounds("workflow-builder-sidebar")
        .expect("workflow sidebar");
    let properties = cx
        .debug_bounds("workflow-properties")
        .expect("workflow properties");
    let add_step = cx.debug_bounds("workflow-add-step").expect("add step");
    let canvas = cx.debug_bounds("workflow-canvas-scroll").expect("canvas");
    assert!(add_step.origin.x >= sidebar.origin.x);
    assert!(add_step.right() <= sidebar.right());
    assert!(add_step.origin.y >= properties.bottom());
    assert!(properties.size.height <= px(160.));
    assert_eq!(canvas.origin.y, header.bottom());
    let name_field = cx.debug_bounds("workflow-name-field").expect("name field");
    let enable_switch = cx.debug_bounds("workflow-enabled").expect("enable switch");
    assert!(name_field.origin.y >= enable_switch.bottom());
    let initially_enabled = model.read_with(&cx, |model, _| model.draft.definition.enabled);
    cx.simulate_click(enable_switch.center(), Default::default());
    assert_ne!(
        model.read_with(&cx, |model, _| model.draft.definition.enabled),
        initially_enabled
    );
    for selector in [
        "workflow-name-field",
        "workflow-enabled",
        "workflow-discard",
    ] {
        let bounds = cx.debug_bounds(selector).expect(selector);
        assert!(
            bounds.origin.y >= header.origin.y + header.size.height,
            "{selector} belongs with workflow properties outside the header"
        );
    }

    cx.update(|window, cx| {
        model.update(cx, |model, cx| {
            model
                .draft
                .name_input
                .update(cx, |input, cx| input.set_value("X", window, cx));
        });
        window.draw(cx).clear(cx);
    });
    let renamed = cx
        .debug_bounds("workflow-breadcrumb-current")
        .expect("renamed workflow breadcrumb");
    assert!(renamed.size.width < name.size.width);
    assert!(model.read_with(&cx, |model, cx| model.draft.dirty(cx)));

    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let recorded = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&model, move |_, event, _| {
            recorded.borrow_mut().push(event.clone());
        })
        .detach();
    });
    let overview = cx
        .debug_bounds("workflow-breadcrumb-overview")
        .expect("workflows breadcrumb");
    cx.simulate_click(overview.center(), Default::default());
    assert!(events.borrow().iter().any(|event| matches!(
        event,
        WorkflowWorkspaceEvent::Navigate(WorkflowRoute::Overview)
    )));
}

#[gpui_kit::test]
fn narrow_workflow_editor_opens_workflow_details(cx: &mut TestAppContext) {
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let mut workspace = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service, window, cx));
            workspace = Some(model.clone());
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Editor, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let model = workspace.expect("workspace");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(800.), px(600.)));
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }

    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let recorded = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&model, move |_, event, _| {
            recorded.borrow_mut().push(event.clone());
        })
        .detach();
    });
    let details = cx
        .debug_bounds("workflow-details")
        .expect("workflow details entry");
    cx.simulate_click(details.center(), Default::default());
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, WorkflowWorkspaceEvent::Navigate(WorkflowRoute::Step)))
    );

    cx.simulate_resize(size(px(480.), px(600.)));
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }
    let header = cx.debug_bounds("workflow-header").expect("header");
    let save = cx.debug_bounds("workflow-save").expect("save button");
    let current = cx
        .debug_bounds("workflow-breadcrumb-current")
        .expect("workflow name");
    assert!(save.origin.x + save.size.width <= header.origin.x + header.size.width);
    assert!(current.size.width > px(0.));
}

#[gpui_kit::test]
fn workflow_details_route_keeps_name_and_editor_breadcrumb(cx: &mut TestAppContext) {
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service, window, cx));
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Step, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });

    assert!(cx.debug_bounds("workflow-breadcrumb-overview").is_some());
    assert!(cx.debug_bounds("workflow-breadcrumb-editor").is_some());
    assert!(cx.debug_bounds("workflow-breadcrumb-current").is_some());
    assert!(cx.debug_bounds("workflow-name-field").is_some());
    assert!(cx.debug_bounds("workflow-add-step").is_some());
}

#[gpui_kit::test]
fn workflow_mermaid_preview_renders_a_diagram(cx: &mut TestAppContext) {
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service, window, cx));
            model.update(cx, |model, cx| {
                model.apply_workflow_definition(
                    role_action_definition(
                        ListWorkflowRole::Done,
                        WorkflowAction::MarkComplete,
                        "Complete on Done",
                    ),
                    window,
                    cx,
                );
            });
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Mermaid, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1000.), px(700.)));
    for _ in 0..500 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        if cx.debug_bounds("workflow-mermaid-diagram").is_some() {
            break;
        }
    }

    assert!(
        cx.debug_bounds("workflow-mermaid-diagram").is_some(),
        "Mermaid preview should render the workflow as a diagram rather than source text"
    );
}

#[gpui_kit::test]
fn workflow_canvas_selection_and_drafts(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let _guard = runtime.enter();
    let service = TestWorkflowService::failing();
    let mut workspace = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service.clone(), window, cx));
            model.update(cx, |model, cx| {
                model.lists.push(WorkflowListRecord {
                    id: 1,
                    title: "Done".into(),
                    entries: Vec::new(),
                });
                model.apply_workflow_definition(
                    role_action_definition(
                        ListWorkflowRole::Done,
                        WorkflowAction::MarkComplete,
                        "Complete on Done",
                    ),
                    window,
                    cx,
                )
            });
            workspace = Some(model.clone());
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Editor, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let model = workspace.expect("workspace");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.), px(768.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("workflow-builder-sidebar").is_some(),
        "the step palette should remain available before a node is selected"
    );
    let node = cx
        .debug_bounds("workflow-node-condition")
        .expect("condition");
    cx.simulate_click(node.center(), Default::default());
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let inspector = cx
        .debug_bounds("workflow-inspector-scroll")
        .expect("step inspector");
    let condition = cx
        .debug_bounds("workflow-condition-picker")
        .expect("condition setting");
    let outcome = cx
        .debug_bounds("workflow-outcome-picker")
        .expect("connection setting");
    let remove = cx
        .debug_bounds("workflow-remove-step")
        .expect("remove step action");
    assert!(condition.origin.x >= inspector.origin.x);
    assert!(condition.right() <= inspector.right());
    assert_eq!(condition.size.height, outcome.size.height);
    assert!(remove.origin.y > outcome.bottom());
    assert!(cx.debug_bounds("workflow-list-parameter-picker").is_some());
    assert!(cx.debug_bounds("workflow-step-options").is_none());
    let editor_viewport_before_error = cx
        .debug_bounds("workflow-canvas-scroll")
        .expect("editor viewport before save error");
    cx.update(|window, cx| {
        model.update(cx, |model, cx| {
            let key = model.draft.key;
            model
                .draft
                .name_input
                .update(cx, |input, cx| input.set_value("My draft", window, cx));
            let graph = model.draft.definition.clone();
            model.new_workflow(window, cx);
            model.resume_draft(key, cx);
            assert_eq!(model.draft.name_input.read(cx).value().as_ref(), "My draft");
            assert_eq!(model.draft.definition, graph);
            assert!(model.draft.dirty(cx));
            model.save_workflow(cx);
        })
    });
    cx.run_until_parked();
    assert!(model.read_with(&cx, |model, _| model.draft.error.is_some()));
    assert!(model.read_with(&cx, |model, cx| model.draft.dirty(cx)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    assert!(
        cx.debug_bounds("workflow-editor-error").is_some(),
        "save validation should remain visible without adding a layout row"
    );
    assert_eq!(
        cx.debug_bounds("workflow-canvas-scroll"),
        Some(editor_viewport_before_error),
        "save feedback must not shift the editor canvas"
    );
    cx.simulate_resize(size(px(1200.), px(400.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    let before = cx.debug_bounds("workflow-node-condition").expect("node");
    let viewport = cx.debug_bounds("workflow-canvas-scroll").expect("canvas");
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-100.))),
        ..Default::default()
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    assert!(
        cx.debug_bounds("workflow-node-condition")
            .expect("node")
            .top()
            < before.top(),
        "Canvas content must move when scrolled"
    );
    cx.update(|_, cx| {
        model.update(cx, |model, _| {
            model.draft.canvas_scroll.set_offset(point(px(0.), px(0.)))
        })
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let recorded = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&model, move |_, event, _| {
            recorded.borrow_mut().push(event.clone())
        })
        .detach()
    });
    let node = cx.debug_bounds("workflow-node-trigger").expect("trigger");
    cx.simulate_click(node.center(), Default::default());
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, WorkflowWorkspaceEvent::Navigate(WorkflowRoute::Step)))
    );
}

#[gpui_kit::test]
fn empty_workflow_editor_guides_the_first_step(cx: &mut TestAppContext) {
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service.clone(), window, cx));
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Editor, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.), px(768.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });

    assert!(
        cx.debug_bounds("workflow-empty-canvas").is_some(),
        "an empty editor should describe the first useful action instead of showing a blank canvas"
    );
    assert!(
        cx.debug_bounds("workflow-builder-sidebar").is_some(),
        "the builder palette should remain available before a step is selected"
    );
    assert_eq!(
        cx.debug_bounds("workflow-empty-add-trigger")
            .expect("empty canvas action")
            .size
            .height,
        px(32.),
        "the canvas call to action should use the default control size"
    );
    assert_eq!(
        cx.debug_bounds("workflow-add-step")
            .expect("sidebar add step action")
            .size
            .height,
        px(24.),
        "the sidebar step action should use the compact control size"
    );
}

#[gpui_kit::test]
fn save_completion_keeps_newer_edits_and_inactive_drafts(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let service = TestWorkflowService::in_memory(WorkflowSnapshot::default());
    let service_state = service.clone();
    let mut workspace = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| WorkflowWorkspace::new(7, service.clone(), window, cx));
            workspace = Some(model.clone());
            let page = cx.new(|cx| WorkflowPage::new(model, WorkflowRoute::Overview, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let model = workspace.expect("workspace");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let key = cx.update(|window, cx| {
        model.update(cx, |model, cx| {
            model.apply_workflow_definition(
                role_action_definition(
                    ListWorkflowRole::Done,
                    WorkflowAction::MarkComplete,
                    "Saved revision",
                ),
                window,
                cx,
            );
            let key = model.draft.key;
            model.save_workflow(cx);
            model.draft.name_input.update(cx, |input, cx| {
                input.set_value("Newer unsaved revision", window, cx)
            });
            model.new_workflow(window, cx);
            key
        })
    });
    for _ in 0..500 {
        cx.run_until_parked();
        if model.read_with(&cx, |model, _| {
            model.drafts.get(&key).is_some_and(|draft| !draft.saving)
        }) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    model.read_with(&cx, |model, cx| {
        let saved = model.drafts.get(&key).expect("retained draft");
        assert!(!saved.saving);
        assert!(saved.error.is_none());
        assert!(saved.active_id.is_some());
        assert_eq!(
            saved.name_input.read(cx).value().as_ref(),
            "Newer unsaved revision"
        );
        assert!(saved.dirty(cx));
        assert_eq!(model.state.workflows[0].name, "Saved revision");
    });
    let snapshot = service_state.snapshot.lock().expect("service state");
    assert_eq!(snapshot.workflows[0].name, "Saved revision");
}

#[cfg(target_os = "windows")]
#[test]
fn workflow_pages_fit_windows_stack() {
    let output = std::process::Command::new(std::env::current_exe().expect("executable"))
        .args([
            "--exact",
            "tests::workflow_canvas_selection_and_drafts",
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
