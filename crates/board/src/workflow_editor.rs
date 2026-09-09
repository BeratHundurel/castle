use std::collections::HashMap;

use gpui_kit::component::{
    ActiveTheme, Disableable as _, Icon, IconName, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement as _,
    v_flex,
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use runtime::AppRuntime;
use storage::workspace::api::SaveWorkflowInput;
use workflow::{
    GraphPosition, MovePosition, WorkflowAction, WorkflowBranchCase, WorkflowCondition,
    WorkflowDefinition, WorkflowEdge, WorkflowEdgeKind, WorkflowNode, WorkflowNodeKind,
    WorkflowTrigger,
};

use super::BoardView;

pub(crate) struct WorkflowEditorState {
    pub(crate) open: bool,
    pub(crate) loading: bool,
    pub(crate) saving: bool,
    pub(crate) running: bool,
    pub(crate) error: Option<SharedString>,
    pub(crate) notice: Option<SharedString>,
    pub(crate) workflows: Vec<storage::workflow::WorkflowRecord>,
    pub(crate) active_id: Option<i64>,
    pub(crate) definition: WorkflowDefinition,
    pub(crate) selected_node: Option<String>,
    pub(crate) node_drag_start: Option<WorkflowNodeDragStart>,
    pub(crate) manual_entry_input: Entity<InputState>,
    pub(crate) name_input: Entity<InputState>,
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

impl WorkflowEditorState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<BoardView>) -> Self {
        Self {
            open: false,
            loading: false,
            saving: false,
            running: false,
            error: None,
            notice: None,
            workflows: Vec::new(),
            active_id: None,
            definition: default_definition(),
            selected_node: None,
            node_drag_start: None,
            manual_entry_input: cx.new(|cx| InputState::new(window, cx).placeholder("Entry ID")),
            name_input: cx.new(|cx| InputState::new(window, cx).placeholder("Workflow name")),
        }
    }
}

impl BoardView {
    pub(crate) fn open_workflow_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(board_id) = self.data.board_id else {
            return;
        };
        self.workflow_editor.open = true;
        self.workflow_editor.loading = true;
        self.workflow_editor.running = false;
        self.workflow_editor.error = None;
        self.workflow_editor.notice = None;
        self.workflow_editor.active_id = None;
        self.workflow_editor.definition = default_definition();
        self.workflow_editor.selected_node = None;
        self.workflow_editor.node_drag_start = None;
        self.defer_workflow_name_input("New workflow", window, cx);
        self.load_workflows(board_id, window, cx);
        cx.notify();
    }

    pub(crate) fn close_workflow_editor(&mut self, cx: &mut Context<Self>) {
        self.workflow_editor.open = false;
        self.workflow_editor.running = false;
        self.workflow_editor.error = None;
        cx.notify();
    }

    fn load_workflows(&mut self, board_id: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(app_runtime) = cx.try_global::<AppRuntime>().cloned() else {
            self.workflow_editor.loading = false;
            self.workflow_editor.error = Some("Workflow services are unavailable.".into());
            cx.notify();
            return;
        };
        let task = app_runtime.spawn_store(cx.background_executor(), move |store| async move {
            storage::workflow::list_workflows(&store, i64::from(board_id)).await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.data.board_id != Some(board_id) || !this.workflow_editor.open {
                    return;
                }
                this.workflow_editor.loading = false;
                match result {
                    Ok(Ok(workflows)) => {
                        let (workflows, invalid_count) = filter_valid_workflows(workflows);
                        this.workflow_editor.workflows = workflows;
                        if let Some(workflow) = this.workflow_editor.workflows.first().cloned() {
                            this.select_workflow_record(workflow, window, cx);
                        } else {
                            this.workflow_editor.active_id = None;
                            this.workflow_editor.definition = default_definition();
                            this.defer_workflow_name_input("New workflow", window, cx);
                        }
                        this.workflow_editor.notice = (invalid_count > 0).then(|| {
                            format!(
                                "Skipped {invalid_count} invalid saved workflow{}.",
                                if invalid_count == 1 { "" } else { "s" }
                            )
                            .into()
                        });
                    }
                    Ok(Err(error)) => {
                        this.workflow_editor.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.workflow_editor.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn select_workflow_record(
        &mut self,
        workflow: storage::workflow::WorkflowRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workflow_editor.active_id = Some(workflow.id);
        self.workflow_editor.definition = workflow.definition;
        self.workflow_editor.selected_node = None;
        self.workflow_editor.node_drag_start = None;
        self.workflow_editor.notice = None;
        self.defer_workflow_name_input(workflow.name, window, cx);
    }

    pub(crate) fn select_workflow(
        &mut self,
        workflow_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workflow) = self
            .workflow_editor
            .workflows
            .iter()
            .find(|workflow| workflow.id == workflow_id)
            .cloned()
        else {
            return;
        };
        self.select_workflow_record(workflow, window, cx);
        cx.notify();
    }

    pub(crate) fn new_workflow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workflow_editor.active_id = None;
        self.workflow_editor.definition = default_definition();
        self.workflow_editor.selected_node = None;
        self.workflow_editor.node_drag_start = None;
        self.workflow_editor.notice = None;
        self.defer_workflow_name_input("New workflow", window, cx);
        cx.notify();
    }

    fn defer_workflow_name_input(
        &self,
        value: impl Into<SharedString>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.workflow_editor.name_input.clone();
        let value = value.into();
        cx.defer_in(window, move |_, window, cx| {
            input.update(cx, |input, cx| {
                input.set_value(value, window, cx);
            });
        });
    }

    pub(crate) fn toggle_workflow_enabled(&mut self, cx: &mut Context<Self>) {
        self.workflow_editor.definition.enabled = !self.workflow_editor.definition.enabled;
        cx.notify();
    }

    pub(crate) fn add_workflow_node(&mut self, kind: WorkflowNodeKind, cx: &mut Context<Self>) {
        let index = self.workflow_editor.definition.nodes.len();
        let node_id = next_node_id(&self.workflow_editor.definition.nodes);
        let previous_id = self
            .workflow_editor
            .definition
            .nodes
            .last()
            .map(|node| node.id.clone());
        self.workflow_editor.definition.nodes.push(WorkflowNode {
            id: node_id.clone(),
            kind,
            position: GraphPosition {
                x: 180.,
                y: 24. + index as f32 * 112.,
            },
        });
        if let Some(previous_id) = previous_id {
            let edge_id = next_edge_id(&self.workflow_editor.definition.edges);
            self.workflow_editor.definition.edges.push(WorkflowEdge {
                id: edge_id,
                from: previous_id,
                to: node_id.clone(),
                kind: WorkflowEdgeKind::Default,
            });
        }
        self.workflow_editor.selected_node = Some(node_id);
        cx.notify();
    }

    pub(crate) fn remove_selected_workflow_node(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = self.workflow_editor.selected_node.take() else {
            return;
        };
        self.workflow_editor
            .definition
            .nodes
            .retain(|node| node.id != selected);
        self.workflow_editor
            .definition
            .edges
            .retain(|edge| edge.from != selected && edge.to != selected);
        cx.notify();
    }

    pub(crate) fn connect_selected_workflow_node(&mut self, cx: &mut Context<Self>) {
        self.connect_selected_workflow_node_with_kind(WorkflowEdgeKind::Default, cx);
    }

    pub(crate) fn connect_selected_workflow_node_with_kind(
        &mut self,
        kind: WorkflowEdgeKind,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = self.workflow_editor.selected_node.clone() else {
            return;
        };
        let Some(index) = self
            .workflow_editor
            .definition
            .nodes
            .iter()
            .position(|node| node.id == selected)
        else {
            return;
        };
        let Some(next) = self.workflow_editor.definition.nodes.get(index + 1) else {
            return;
        };
        self.connect_selected_workflow_node_to(next.id.clone(), kind, cx);
    }

    pub(crate) fn connect_selected_workflow_node_to(
        &mut self,
        target_id: String,
        kind: WorkflowEdgeKind,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = self.workflow_editor.selected_node.clone() else {
            return;
        };
        if selected == target_id
            || !self
                .workflow_editor
                .definition
                .nodes
                .iter()
                .any(|node| node.id == target_id)
        {
            return;
        }
        if self
            .workflow_editor
            .definition
            .edges
            .iter()
            .any(|edge| edge.from == selected && edge.to == target_id && edge.kind == kind)
        {
            return;
        }
        self.workflow_editor.definition.edges.push(WorkflowEdge {
            id: next_edge_id(&self.workflow_editor.definition.edges),
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
        let Some(selected) = self.workflow_editor.selected_node.as_deref() else {
            return;
        };
        let Some(node) = self
            .workflow_editor
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
        definition: WorkflowDefinition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workflow_editor.active_id = None;
        self.workflow_editor.definition = definition;
        self.workflow_editor.selected_node = None;
        self.workflow_editor.node_drag_start = None;
        let name = self.workflow_editor.definition.name.clone();
        self.defer_workflow_name_input(name, window, cx);
        cx.notify();
    }

    pub(crate) fn save_workflow(&mut self, cx: &mut Context<Self>) {
        let Some(board_id) = self.data.board_id else {
            return;
        };
        let name = self
            .workflow_editor
            .name_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        if name.is_empty() {
            self.workflow_editor.error = Some("Workflow name must not be empty".into());
            cx.notify();
            return;
        }
        let Ok(definition) = serde_json::to_value(&self.workflow_editor.definition) else {
            self.workflow_editor.error = Some("Workflow graph could not be serialized".into());
            cx.notify();
            return;
        };
        let input = SaveWorkflowInput {
            workflow_id: self.workflow_editor.active_id,
            board_id: i64::from(board_id),
            name,
            enabled: self.workflow_editor.definition.enabled,
            definition,
        };
        let Some(app_runtime) = cx.try_global::<AppRuntime>().cloned() else {
            self.workflow_editor.error = Some("Workflow services are unavailable.".into());
            cx.notify();
            return;
        };
        self.workflow_editor.saving = true;
        self.workflow_editor.error = None;
        let task = app_runtime.spawn_store(cx.background_executor(), move |store| async move {
            store
                .mutations(storage::MutationOrigin::LocalApp)
                .save_workflow(input)
                .await
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.data.board_id != Some(board_id) || !this.workflow_editor.open {
                    return;
                }
                this.workflow_editor.saving = false;
                match result {
                    Ok(Ok(workflow)) => {
                        this.workflow_editor.active_id = Some(workflow.id);
                        this.workflow_editor.definition = workflow.definition.clone();
                        if let Some(existing) = this
                            .workflow_editor
                            .workflows
                            .iter_mut()
                            .find(|candidate| candidate.id == workflow.id)
                        {
                            *existing = workflow.clone();
                        } else {
                            this.workflow_editor.workflows.push(workflow.clone());
                        }
                        this.defer_workflow_name_input(workflow.name, window, cx);
                    }
                    Ok(Err(error)) => {
                        this.workflow_editor.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.workflow_editor.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn run_manual_workflows_from_editor(&mut self, cx: &mut Context<Self>) {
        let Some(board_id) = self.data.board_id else {
            return;
        };
        let entry_text = self
            .workflow_editor
            .manual_entry_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let Ok(entry_id) = entry_text.parse::<i64>() else {
            self.workflow_editor.error = Some("Entry ID must be a number".into());
            self.workflow_editor.notice = None;
            cx.notify();
            return;
        };
        self.workflow_editor.running = true;
        self.workflow_editor.error = None;
        self.workflow_editor.notice = None;
        let Some(app_runtime) = cx.try_global::<AppRuntime>().cloned() else {
            self.workflow_editor.running = false;
            self.workflow_editor.error = Some("Workflow services are unavailable.".into());
            cx.notify();
            return;
        };
        let task = app_runtime.spawn_store(cx.background_executor(), move |store| async move {
            storage::workflow::run_manual_workflow(
                &store,
                i64::from(board_id),
                entry_id,
                workflow::EventOrigin::User,
            )
            .await
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                if this.data.board_id != Some(board_id) || !this.workflow_editor.open {
                    return;
                }
                this.workflow_editor.running = false;
                match result {
                    Ok(Ok(report)) => {
                        this.workflow_editor.notice =
                            Some(format!("Ran {} workflow run(s)", report.runs.len()).into());
                        this.reload_board(board_id, cx);
                    }
                    Ok(Err(error)) => {
                        this.workflow_editor.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.workflow_editor.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn render_workflow_editor_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        if !self.workflow_editor.open {
            return div().into_any_element();
        }
        let theme = cx.theme().clone();
        let active_id = self.workflow_editor.active_id;
        let selected_node = self.workflow_editor.selected_node.clone();
        let graph = self.render_workflow_graph(cx);
        let mermaid = workflow::to_mermaid(&self.workflow_editor.definition);

        div()
            .id("workflow-editor-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay.opacity(0.78))
            .child(
                v_flex()
                    .id("workflow-editor-panel")
                    .debug_selector(|| "workflow-editor-panel".to_string())
                    .w(px(1120.))
                    .max_w(gpui_kit::relative(0.96))
                    .h(px(720.))
                    .max_h(gpui_kit::relative(0.94))
                    .overflow_hidden()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border.opacity(0.78))
                    .bg(theme.popover)
                    .text_color(theme.popover_foreground)
                    .shadow_lg()
                    .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation()
                    })
                    .child(
                        h_flex()
                            .min_h_12()
                            .px_4()
                            .gap_3()
                            .items_center()
                            .border_b_1()
                            .border_color(theme.border.opacity(0.72))
                            .child(
                                Icon::new(IconName::Settings2)
                                    .small()
                                    .text_color(theme.primary),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Workflow studio"),
                            )
                            .child(
                                Input::new(&self.workflow_editor.name_input)
                                    .w(px(230.))
                                    .h_8()
                                    .bg(theme.background)
                                    .px_2()
                                    .rounded_sm(),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new("workflow-enabled")
                                    .label(if self.workflow_editor.definition.enabled {
                                        "Enabled"
                                    } else {
                                        "Disabled"
                                    })
                                    .small()
                                    .selected(self.workflow_editor.definition.enabled)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_workflow_enabled(cx);
                                    })),
                            )
                            .child(
                                Button::new("workflow-save")
                                    .icon(IconName::Check)
                                    .label(if self.workflow_editor.saving {
                                        "Saving..."
                                    } else {
                                        "Save"
                                    })
                                    .primary()
                                    .small()
                                    .disabled(self.workflow_editor.saving)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.save_workflow(cx);
                                    })),
                            )
                            .child(
                                Button::new("workflow-close")
                                    .icon(IconName::Close)
                                    .ghost()
                                    .small()
                                    .tooltip("Close workflow studio")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_workflow_editor(cx);
                                    })),
                            ),
                    )
                    .when_some(self.workflow_editor.error.clone(), |this, error| {
                        this.child(
                            div()
                                .px_4()
                                .py_2()
                                .text_sm()
                                .text_color(theme.danger)
                                .bg(theme.danger.opacity(0.08))
                                .child(error),
                        )
                    })
                    .when_some(self.workflow_editor.notice.clone(), |this, notice| {
                        this.child(
                            div()
                                .px_4()
                                .py_2()
                                .text_sm()
                                .text_color(theme.success)
                                .bg(theme.success.opacity(0.08))
                                .child(notice),
                        )
                    })
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(self.render_workflow_list(active_id, cx))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .min_h_0()
                                    .h_full()
                                    .p_4()
                                    .overflow_scrollbar()
                                    .child(graph),
                            )
                            .child(self.render_workflow_tools(selected_node, mermaid, cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_workflow_list(&self, active_id: Option<i64>, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        v_flex()
            .w(px(190.))
            .h_full()
            .flex_shrink_0()
            .gap_2()
            .p_3()
            .border_r_1()
            .border_color(theme.border.opacity(0.72))
            .bg(theme.background.opacity(0.4))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.muted_foreground)
                            .child("Workflows"),
                    )
                    .child(
                        Button::new("workflow-new")
                            .icon(IconName::Plus)
                            .ghost()
                            .compact()
                            .tooltip("New workflow")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.new_workflow(window, cx);
                            })),
                    ),
            )
            .when(self.workflow_editor.loading, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Loading workflows..."),
                )
            })
            .children(self.workflow_editor.workflows.iter().map(|workflow| {
                let workflow_id = workflow.id;
                Button::new(SharedString::from(format!("workflow-{workflow_id}")))
                    .w_full()
                    .justify_start()
                    .label(workflow.name.clone())
                    .small()
                    .selected(active_id == Some(workflow_id))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_workflow(workflow_id, window, cx);
                    }))
                    .into_any_element()
            }))
            .into_any_element()
    }

    fn render_workflow_manual_run(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        v_flex()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child("Run manual workflows"),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Input::new(&self.workflow_editor.manual_entry_input)
                            .w(px(142.))
                            .h_8()
                            .bg(theme.background)
                            .px_2()
                            .rounded_sm(),
                    )
                    .child(
                        Button::new("workflow-run-manual")
                            .label(if self.workflow_editor.running {
                                "Running..."
                            } else {
                                "Run"
                            })
                            .small()
                            .primary()
                            .disabled(self.workflow_editor.running)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.run_manual_workflows_from_editor(cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn render_workflow_node_palette(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .gap_1()
            .flex_wrap()
            .child(
                Button::new("workflow-add-trigger")
                    .label("Trigger")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.add_workflow_node(
                            WorkflowNodeKind::Trigger {
                                trigger: WorkflowTrigger::Manual,
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-add-condition")
                    .label("Condition")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.add_workflow_node(
                            WorkflowNodeKind::Condition {
                                condition: WorkflowCondition::Always,
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-add-action")
                    .label("Action")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.add_workflow_node(
                            WorkflowNodeKind::Action {
                                action: WorkflowAction::Notify {
                                    message: "Review this card".to_string(),
                                },
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-add-branch")
                    .label("Branch")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.add_workflow_node(
                            WorkflowNodeKind::Branch {
                                cases: vec![
                                    WorkflowBranchCase {
                                        id: "case-1".to_string(),
                                        label: "Done list".to_string(),
                                        condition: WorkflowCondition::ListRoleIs {
                                            role: workflow::ListWorkflowRole::Done,
                                        },
                                    },
                                    WorkflowBranchCase {
                                        id: "case-2".to_string(),
                                        label: "Cancelled list".to_string(),
                                        condition: WorkflowCondition::ListRoleIs {
                                            role: workflow::ListWorkflowRole::Cancelled,
                                        },
                                    },
                                ],
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-add-end")
                    .label("End")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.add_workflow_node(
                            WorkflowNodeKind::End {
                                label: Some("Complete".to_string()),
                            },
                            cx,
                        );
                    })),
            )
            .into_any_element()
    }

    fn render_workflow_templates(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .gap_1()
            .child(
                Button::new("workflow-template-complete")
                    .label("Complete on Done")
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.apply_workflow_definition(
                            role_action_definition(
                                workflow::ListWorkflowRole::Done,
                                WorkflowAction::MarkComplete,
                                "Complete on Done",
                            ),
                            window,
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-template-cancel")
                    .label("Cancel on Cancelled")
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.apply_workflow_definition(
                            role_action_definition(
                                workflow::ListWorkflowRole::Cancelled,
                                WorkflowAction::MarkCancelled,
                                "Cancel on Cancelled",
                            ),
                            window,
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-template-archive")
                    .label("Archive on completion")
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.apply_workflow_definition(
                            event_action_definition(
                                WorkflowTrigger::CardCompleted,
                                WorkflowAction::Archive,
                                "Archive on completion",
                            ),
                            window,
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-template-recurring")
                    .label("Create next recurrence")
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.apply_workflow_definition(
                            event_action_definition(
                                WorkflowTrigger::CardCompleted,
                                WorkflowAction::CreateNextRecurringInstance,
                                "Create next recurrence",
                            ),
                            window,
                            cx,
                        );
                    })),
            )
            .into_any_element()
    }

    fn render_workflow_connection_actions(
        &self,
        selected_node: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .gap_1()
            .child(
                Button::new("workflow-connect-default")
                    .label("Connect →")
                    .small()
                    .outline()
                    .disabled(selected_node.is_none())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.connect_selected_workflow_node(cx);
                    })),
            )
            .child(
                Button::new("workflow-connect-true")
                    .label("True")
                    .small()
                    .outline()
                    .disabled(selected_node.is_none())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.connect_selected_workflow_node_with_kind(WorkflowEdgeKind::True, cx);
                    })),
            )
            .child(
                Button::new("workflow-connect-false")
                    .label("False")
                    .small()
                    .outline()
                    .disabled(selected_node.is_none())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.connect_selected_workflow_node_with_kind(WorkflowEdgeKind::False, cx);
                    })),
            )
            .child(
                Button::new("workflow-remove-node")
                    .icon(IconName::Delete)
                    .ghost()
                    .small()
                    .disabled(selected_node.is_none())
                    .tooltip("Remove selected node")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.remove_selected_workflow_node(cx);
                    })),
            )
            .into_any_element()
    }

    fn render_workflow_mermaid_preview(
        &self,
        mermaid: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        v_flex()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child("Mermaid preview"),
            )
            .child(
                div()
                    .w_full()
                    .p_2()
                    .rounded_sm()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border.opacity(0.72))
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(SharedString::from(mermaid)),
            )
            .into_any_element()
    }

    fn render_workflow_tools(
        &self,
        selected_node: Option<String>,
        mermaid: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        v_flex()
            .w(px(260.))
            .h_full()
            .flex_shrink_0()
            .gap_3()
            .p_3()
            .border_l_1()
            .border_color(theme.border.opacity(0.72))
            .overflow_y_scrollbar()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child("Add to canvas"),
            )
            .child(self.render_workflow_manual_run(cx))
            .child(self.render_workflow_node_palette(cx))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child("Templates"),
            )
            .child(self.render_workflow_templates(cx))
            .child(self.render_selected_node_controls(selected_node.clone(), cx))
            .child(self.render_workflow_connection_actions(selected_node.clone(), cx))
            .child(self.render_connection_targets(selected_node.as_deref(), cx))
            .child(self.render_workflow_mermaid_preview(mermaid, cx))
            .into_any_element()
    }

    fn render_connection_targets(
        &self,
        selected_node: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let Some(selected_node) = selected_node else {
            return div().into_any_element();
        };
        let targets = self
            .workflow_editor
            .definition
            .nodes
            .iter()
            .filter(|node| node.id != selected_node)
            .map(|node| {
                let target_id = node.id.clone();
                Button::new(SharedString::from(format!(
                    "workflow-connect-to-{target_id}"
                )))
                .label(format!("→ {} · {}", node_kind_label(&node.kind), node.id))
                .small()
                .outline()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.connect_selected_workflow_node_to(
                        target_id.clone(),
                        WorkflowEdgeKind::Default,
                        cx,
                    );
                }))
                .into_any_element()
            })
            .collect::<Vec<_>>();
        v_flex()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child("Connect selected to"),
            )
            .when(targets.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("No other nodes"),
                )
            })
            .children(targets)
            .into_any_element()
    }

    fn render_workflow_trigger_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .gap_1()
            .flex_wrap()
            .child(
                Button::new("workflow-trigger-done")
                    .label("Moved → Done")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Trigger {
                                trigger: WorkflowTrigger::CardMovedToRole {
                                    role: workflow::ListWorkflowRole::Done,
                                },
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-trigger-cancelled")
                    .label("Moved → Cancelled")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Trigger {
                                trigger: WorkflowTrigger::CardMovedToRole {
                                    role: workflow::ListWorkflowRole::Cancelled,
                                },
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-trigger-completed")
                    .label("Completed")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Trigger {
                                trigger: WorkflowTrigger::CardCompleted,
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-trigger-cancelled-event")
                    .label("Cancelled")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Trigger {
                                trigger: WorkflowTrigger::CardCancelled,
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-trigger-checklist")
                    .label("Checklist done")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Trigger {
                                trigger: WorkflowTrigger::ChecklistAllCompleted,
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-trigger-manual")
                    .label("Manual")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Trigger {
                                trigger: WorkflowTrigger::Manual,
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-trigger-calendar-open")
                    .label("Calendar open")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Trigger {
                                trigger: WorkflowTrigger::Scheduled {
                                    schedule_key: "calendar_open".to_string(),
                                },
                            },
                            cx,
                        );
                    })),
            )
            .into_any_element()
    }

    fn render_workflow_condition_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .gap_1()
            .flex_wrap()
            .child(
                Button::new("workflow-condition-done")
                    .label("List is Done")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Condition {
                                condition: WorkflowCondition::ListRoleIs {
                                    role: workflow::ListWorkflowRole::Done,
                                },
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-condition-cancelled")
                    .label("List is Cancelled")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Condition {
                                condition: WorkflowCondition::ListRoleIs {
                                    role: workflow::ListWorkflowRole::Cancelled,
                                },
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-condition-checklist")
                    .label("Checklist done")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Condition {
                                condition: WorkflowCondition::ChecklistIs {
                                    state: workflow::ChecklistState::AllChecked,
                                },
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-condition-complete")
                    .label("Is complete")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Condition {
                                condition: WorkflowCondition::CompletionIs {
                                    state: workflow::CompletionState::Completed,
                                },
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-condition-always")
                    .label("Always")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Condition {
                                condition: WorkflowCondition::Always,
                            },
                            cx,
                        );
                    })),
            )
            .into_any_element()
    }

    fn render_workflow_action_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .gap_1()
            .flex_wrap()
            .child(
                Button::new("workflow-action-complete")
                    .label("Complete")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Action {
                                action: WorkflowAction::MarkComplete,
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-action-cancel")
                    .label("Cancel")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Action {
                                action: WorkflowAction::MarkCancelled,
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-action-reopen")
                    .label("Reopen")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Action {
                                action: WorkflowAction::Reopen,
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-action-archive")
                    .label("Archive")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Action {
                                action: WorkflowAction::Archive,
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-action-trash")
                    .label("Trash")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Action {
                                action: WorkflowAction::Trash,
                            },
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("workflow-action-next-recurrence")
                    .label("Next recurrence")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.configure_selected_workflow_node(
                            WorkflowNodeKind::Action {
                                action: WorkflowAction::CreateNextRecurringInstance,
                            },
                            cx,
                        );
                    })),
            )
            .into_any_element()
    }

    fn render_selected_node_controls(
        &self,
        selected_node: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let Some(kind) = selected_node.as_deref().and_then(|selected| {
            self.workflow_editor
                .definition
                .nodes
                .iter()
                .find(|node| node.id == selected)
                .map(|node| node.kind.clone())
        }) else {
            return div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Select a node to configure it.")
                .into_any_element();
        };

        let kind_for_list_parameters = kind.clone();
        let controls = match kind {
            WorkflowNodeKind::Trigger { .. } => self.render_workflow_trigger_controls(cx),
            WorkflowNodeKind::Condition { .. } => self.render_workflow_condition_controls(cx),
            WorkflowNodeKind::Action { .. } => self.render_workflow_action_controls(cx),
            WorkflowNodeKind::Branch { .. } => div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Branch cases can be edited through the workflow JSON API.")
                .into_any_element(),
            WorkflowNodeKind::End { .. } => div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("End nodes stop this path.")
                .into_any_element(),
        };

        v_flex()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child("Selected node"),
            )
            .child(controls)
            .child(self.render_list_parameter_controls(&kind_for_list_parameters, cx))
            .into_any_element()
    }

    fn render_list_parameter_controls(
        &self,
        kind: &WorkflowNodeKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let mut buttons = Vec::new();
        for list in &self.data.lists {
            let list_id = i64::from(list.id);
            let title = list.title.clone();
            match kind {
                WorkflowNodeKind::Trigger { .. } => {
                    let to_title = title.clone();
                    buttons.push(
                        Button::new(SharedString::from(format!(
                            "workflow-trigger-to-list-{list_id}"
                        )))
                        .label(format!("To {to_title}"))
                        .small()
                        .outline()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.configure_selected_workflow_node(
                                WorkflowNodeKind::Trigger {
                                    trigger: WorkflowTrigger::CardMovedToList { list_id },
                                },
                                cx,
                            );
                        }))
                        .into_any_element(),
                    );
                    buttons.push(
                        Button::new(SharedString::from(format!(
                            "workflow-trigger-from-list-{list_id}"
                        )))
                        .label(format!("From {title}"))
                        .small()
                        .outline()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.configure_selected_workflow_node(
                                WorkflowNodeKind::Trigger {
                                    trigger: WorkflowTrigger::CardMovedFromList { list_id },
                                },
                                cx,
                            );
                        }))
                        .into_any_element(),
                    );
                }
                WorkflowNodeKind::Condition { .. } => {
                    buttons.push(
                        Button::new(SharedString::from(format!(
                            "workflow-condition-list-{list_id}"
                        )))
                        .label(format!("List is {title}"))
                        .small()
                        .outline()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.configure_selected_workflow_node(
                                WorkflowNodeKind::Condition {
                                    condition: WorkflowCondition::ListIs { list_id },
                                },
                                cx,
                            );
                        }))
                        .into_any_element(),
                    );
                }
                WorkflowNodeKind::Action { .. } => {
                    buttons.push(
                        Button::new(SharedString::from(format!(
                            "workflow-action-move-to-list-{list_id}"
                        )))
                        .label(format!("Move to {title}"))
                        .small()
                        .outline()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.configure_selected_workflow_node(
                                WorkflowNodeKind::Action {
                                    action: WorkflowAction::MoveToList {
                                        list_id,
                                        position: MovePosition::Bottom,
                                    },
                                },
                                cx,
                            );
                        }))
                        .into_any_element(),
                    );
                }
                WorkflowNodeKind::Branch { .. } | WorkflowNodeKind::End { .. } => {}
            }
        }
        if buttons.is_empty() {
            return div().into_any_element();
        }
        v_flex()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child("Board lists"),
            )
            .child(h_flex().gap_1().flex_wrap().children(buttons))
            .into_any_element()
    }

    fn render_workflow_graph(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let definition = &self.workflow_editor.definition;
        let positions = definition
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.clone(), node_position(node, index)))
            .collect::<HashMap<_, _>>();
        let canvas_height = (definition.nodes.len().max(3) as f32 * 112.) + 50.;
        let connectors = definition.edges.iter().filter_map(|edge| {
            let (from_x, from_y) = positions.get(&edge.from).copied()?;
            let (_, to_y) = positions.get(&edge.to).copied()?;
            let height = (to_y - from_y - 68.).max(18.);
            Some(
                div()
                    .id(SharedString::from(format!("connector-{}", edge.id)))
                    .absolute()
                    .left(px(from_x + 119.))
                    .top(px(from_y + 68.))
                    .w(px(2.))
                    .h(px(height))
                    .bg(theme.border.opacity(0.9))
                    .child(
                        div()
                            .absolute()
                            .left(px(-4.))
                            .bottom(px(-8.))
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(if edge.kind == WorkflowEdgeKind::Default {
                                "↓"
                            } else {
                                edge_kind_label(&edge.kind)
                            }),
                    )
                    .into_any_element(),
            )
        });
        let nodes = definition.nodes.iter().enumerate().map(|(index, node)| {
            let (x, y) = node_position(node, index);
            let node_id = node.id.clone();
            let selected = self.workflow_editor.selected_node.as_deref() == Some(node.id.as_str());
            let border = if selected {
                theme.primary
            } else {
                theme.border.opacity(0.78)
            };
            v_flex()
                .id(SharedString::from(format!("workflow-node-{}", node.id)))
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(240.))
                .h(px(72.))
                .gap_1()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(border)
                .bg(theme.popover)
                .shadow_sm()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.workflow_editor.selected_node = Some(node_id.clone());
                        this.workflow_editor.node_drag_start = None;
                        cx.notify();
                    }),
                )
                .on_drag(
                    WorkflowNodeDrag {
                        node_id: node.id.clone(),
                    },
                    |drag, _, _, cx| cx.new(|_| drag.clone()),
                )
                .on_drag_move(cx.listener(
                    |this, event: &gpui_kit::DragMoveEvent<WorkflowNodeDrag>, _, cx| {
                        let drag = event.drag(cx).clone();
                        let Some(node_index) = this
                            .workflow_editor
                            .definition
                            .nodes
                            .iter()
                            .position(|node| node.id == drag.node_id)
                        else {
                            return;
                        };

                        let current_position = node_position(
                            &this.workflow_editor.definition.nodes[node_index],
                            node_index,
                        );
                        let start = match this.workflow_editor.node_drag_start.clone() {
                            Some(start) if start.node_id == drag.node_id => start,
                            _ => {
                                let start = WorkflowNodeDragStart {
                                    node_id: drag.node_id,
                                    pointer: event.event.position,
                                    position: GraphPosition {
                                        x: current_position.0,
                                        y: current_position.1,
                                    },
                                };
                                this.workflow_editor.node_drag_start = Some(start);
                                return;
                            }
                        };

                        let delta = event.event.position.relative_to(&start.pointer);
                        let node = &mut this.workflow_editor.definition.nodes[node_index];
                        node.position = GraphPosition {
                            x: (start.position.x + f32::from(delta.x)).max(16.),
                            y: (start.position.y + f32::from(delta.y)).max(16.),
                        };
                        cx.notify();
                    },
                ))
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(Icon::new(node_icon(&node.kind)).xsmall())
                        .child(node_kind_label(&node.kind)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(node_detail_label(&node.kind)),
                )
                .into_any_element()
        });

        div()
            .id("workflow-graph-canvas")
            .debug_selector(|| "workflow-graph-canvas".to_string())
            .relative()
            .w(px(620.))
            .h(px(canvas_height))
            .min_h(px(320.))
            .rounded_md()
            .border_1()
            .border_color(theme.border.opacity(0.72))
            .bg(theme.background.opacity(0.56))
            .children(connectors)
            .children(nodes)
            .into_any_element()
    }
}

fn default_definition() -> WorkflowDefinition {
    role_action_definition(
        workflow::ListWorkflowRole::Done,
        WorkflowAction::MarkComplete,
        "New workflow",
    )
}

fn filter_valid_workflows(
    workflows: Vec<storage::workflow::WorkflowRecord>,
) -> (Vec<storage::workflow::WorkflowRecord>, usize) {
    let mut invalid_count = 0;
    let valid = workflows
        .into_iter()
        .filter(|workflow| {
            if workflow::validate(&workflow.definition).is_ok() {
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
        WorkflowNodeKind::Trigger { .. } => "Trigger",
        WorkflowNodeKind::Condition { .. } => "Condition",
        WorkflowNodeKind::Branch { .. } => "Branch",
        WorkflowNodeKind::Action { .. } => "Action",
        WorkflowNodeKind::End { .. } => "End",
    }
}

fn node_detail_label(kind: &WorkflowNodeKind) -> String {
    match kind {
        WorkflowNodeKind::Trigger { trigger } => format!("{:?}", trigger),
        WorkflowNodeKind::Condition { condition } => format!("{:?}", condition),
        WorkflowNodeKind::Branch { cases } => format!("{} cases", cases.len()),
        WorkflowNodeKind::Action { action } => format!("{:?}", action),
        WorkflowNodeKind::End { label } => label.clone().unwrap_or_else(|| "Stop".to_string()),
    }
}

fn edge_kind_label(kind: &WorkflowEdgeKind) -> &'static str {
    match kind {
        WorkflowEdgeKind::Default => "↓",
        WorkflowEdgeKind::True => "true",
        WorkflowEdgeKind::False => "false",
        WorkflowEdgeKind::Case(_) => "case",
        WorkflowEdgeKind::Otherwise => "otherwise",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{TestAppContext, VisualTestContext, size};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::Database;
    use std::{path::PathBuf, sync::Arc};

    struct WorkflowShellHarness {
        board: Entity<BoardView>,
    }

    impl gpui_kit::Render for WorkflowShellHarness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            v_flex()
                .id("workflow-test-app-container")
                .size_full()
                .overflow_hidden()
                .child(div().h(px(32.)).flex_shrink_0())
                .child(
                    h_flex()
                        .id("workflow-test-main-container")
                        .relative()
                        .size_full()
                        .overflow_hidden()
                        .child(
                            v_flex()
                                .id("workflow-test-content-container")
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .min_h_0()
                                        .w_full()
                                        .overflow_hidden()
                                        .child(self.board.clone()),
                                ),
                        ),
                )
        }
    }

    #[gpui_kit::test]
    fn opening_workflow_editor_does_not_panic(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("workflow test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let (database, board_id) = runtime
            .block_on(async {
                let database = Database::connect("sqlite::memory:").await?;
                Migrator::up(&database, None).await?;
                let board =
                    storage::workspace::create_board(&database, None, "Workflow board".to_string())
                        .await?;
                Ok::<_, anyhow::Error>((database, board.id))
            })
            .expect("workflow test database should initialize");

        cx.update(|cx| {
            cx.set_global(gpui_kit::component::Theme::default());
            gpui_kit::init(cx);
            cx.set_global(AppRuntime::new(Arc::new(database), PathBuf::new()));
        });
        let mut board_view = None;
        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                let view = BoardView::view(window, cx);
                board_view = Some(view.clone());
                view.update(cx, |board, cx| {
                    board.data.board_id = Some(board_id);
                    board.data.lists = vec![crate::model::BoardListState {
                        id: 1,
                        title: "Planning".into(),
                        board_id,
                        position: 0,
                        workflow_role: storage::board::ListWorkflowRole::Neutral,
                        entries: Vec::new(),
                    }];
                    cx.notify();
                });
                let shell = cx.new(|_| WorkflowShellHarness {
                    board: view.clone(),
                });
                cx.new(|cx| gpui_kit::component::Root::new(shell, window, cx))
            })
            .expect("workflow test window should open")
        });
        let view = board_view.expect("workflow test view should exist");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let workflows_button = cx
            .debug_bounds("open-board-workflows")
            .expect("workflows button should be rendered");
        cx.simulate_click(workflows_button.center(), gpui_kit::Modifiers::default());
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        for _ in 0..100 {
            cx.run_until_parked();
            if view.read_with(&cx, |board, _| !board.workflow_editor.loading) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        assert!(view.read_with(&cx, |board, _| {
            board.workflow_editor.open
                && board.workflow_editor.workflows.is_empty()
                && board.workflow_editor.active_id.is_none()
        }));
    }

    #[gpui_kit::test]
    fn opening_workflow_editor_draws_persisted_graph_at_narrow_geometry(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("workflow test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let (database, board_id) = runtime
            .block_on(async {
                let database = Database::connect("sqlite::memory:").await?;
                Migrator::up(&database, None).await?;
                let board =
                    storage::workspace::create_board(&database, None, "Workflow board".to_string())
                        .await?;
                let store = storage::Store::from(database.clone());
                let mut definition = default_definition();
                definition.name = "Persisted large graph".to_string();
                let mut previous_id = definition.nodes.last().map(|node| node.id.clone());
                for index in 0..64 {
                    let node_id = format!("persisted-{index}");
                    definition.nodes.push(WorkflowNode {
                        id: node_id.clone(),
                        kind: WorkflowNodeKind::Action {
                            action: WorkflowAction::Notify {
                                message: "Persisted graph node".to_string(),
                            },
                        },
                        position: GraphPosition {
                            x: 180.,
                            y: 24. + (definition.nodes.len() as f32 * 112.),
                        },
                    });
                    if let Some(from) = previous_id.replace(node_id.clone()) {
                        definition.edges.push(WorkflowEdge {
                            id: format!("persisted-edge-{index}"),
                            from,
                            to: node_id,
                            kind: WorkflowEdgeKind::Default,
                        });
                    }
                }
                storage::workflow::upsert_workflow(
                    &store,
                    storage::workflow::WorkflowDraft {
                        id: None,
                        board_id: i64::from(board.id),
                        name: definition.name.clone(),
                        enabled: false,
                        definition,
                    },
                )
                .await?;
                Ok::<_, anyhow::Error>((database, board.id))
            })
            .expect("workflow test database should initialize");

        cx.update(|cx| {
            cx.set_global(gpui_kit::component::Theme::default());
            gpui_kit::init(cx);
            cx.set_global(AppRuntime::new(Arc::new(database), PathBuf::new()));
        });
        let mut board_view = None;
        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                let view = BoardView::view(window, cx);
                board_view = Some(view.clone());
                view.update(cx, |board, cx| {
                    board.data.board_id = Some(board_id);
                    board.data.lists = vec![crate::model::BoardListState {
                        id: 1,
                        title: "Planning".into(),
                        board_id,
                        position: 0,
                        workflow_role: storage::board::ListWorkflowRole::Neutral,
                        entries: Vec::new(),
                    }];
                    cx.notify();
                });
                let shell = cx.new(|_| WorkflowShellHarness {
                    board: view.clone(),
                });
                cx.new(|cx| gpui_kit::component::Root::new(shell, window, cx))
            })
            .expect("workflow test window should open")
        });
        let view = board_view.expect("workflow test view should exist");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let workflows_button = cx
            .debug_bounds("open-board-workflows")
            .expect("workflows button should be rendered");
        cx.simulate_click(workflows_button.center(), gpui_kit::Modifiers::default());
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        for _ in 0..100 {
            cx.run_until_parked();
            if view.read_with(&cx, |board, _| !board.workflow_editor.loading) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        assert!(view.read_with(&cx, |board, _| {
            board.workflow_editor.open
                && !board.workflow_editor.loading
                && board.workflow_editor.workflows.len() == 1
                && board.workflow_editor.active_id.is_some()
                && board.workflow_editor.definition.nodes.len() == 68
        }));
        assert!(cx.debug_bounds("workflow-editor-panel").is_some());
        assert!(cx.debug_bounds("workflow-graph-canvas").is_some());
    }

    #[gpui_kit::test]
    fn opening_workflow_editor_without_runtime_surfaces_an_error(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("workflow test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.update(|cx| {
            cx.set_global(gpui_kit::component::Theme::default());
            gpui_kit::init(cx);
        });
        let mut board_view = None;
        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                let view = BoardView::view(window, cx);
                board_view = Some(view.clone());
                view.update(cx, |board, cx| {
                    board.data.board_id = Some(1);
                    board.data.lists = vec![crate::model::BoardListState {
                        id: 1,
                        title: "Planning".into(),
                        board_id: 1,
                        position: 0,
                        workflow_role: storage::board::ListWorkflowRole::Neutral,
                        entries: Vec::new(),
                    }];
                    cx.notify();
                });
                cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
            })
            .expect("workflow runtime test window should open")
        });
        let view = board_view.expect("workflow runtime test view should exist");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(1_200.), px(768.)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let workflows_button = cx
            .debug_bounds("open-board-workflows")
            .expect("workflows button should be rendered");
        cx.simulate_click(workflows_button.center(), gpui_kit::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        assert!(view.read_with(&cx, |board, _| {
            board.workflow_editor.open
                && !board.workflow_editor.loading
                && board.workflow_editor.error.is_some()
        }));
    }
}
