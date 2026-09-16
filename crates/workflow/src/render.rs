use super::*;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};

impl WorkflowWorkspace {
    pub(super) fn render_workflow_templates(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        v_flex()
            .gap_2()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        Button::new("workflow-template-complete")
                            .debug_selector(|| "workflow-template-complete".into())
                            .label("Complete items when they reach Done")
                            .outline()
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
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("When an item reaches a Done list, mark it complete."),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        Button::new("workflow-template-cancel")
                            .label("Cancel items when they reach Cancelled")
                            .outline()
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
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Keep cancellation state in sync with the board."),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        Button::new("workflow-template-archive")
                            .label("Archive completed items")
                            .outline()
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
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Archive the item as soon as it is completed."),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        Button::new("workflow-template-recurring")
                            .label("Create the next recurring item")
                            .outline()
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
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        "After completion, create the following item in a recurring series.",
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_workflow_trigger_controls(
        &self,
        kind: &WorkflowNodeKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let weak = cx.entity().downgrade();
        Button::new("workflow-trigger-picker")
            .label(self.step_label(kind))
            .outline()
            .dropdown_menu(move |menu, _, _| {
                [
                    (
                        "Item moves to Done",
                        WorkflowNodeKind::Trigger {
                            trigger: WorkflowTrigger::CardMovedToRole {
                                role: workflow::ListWorkflowRole::Done,
                            },
                        },
                    ),
                    (
                        "Item moves to Cancelled",
                        WorkflowNodeKind::Trigger {
                            trigger: WorkflowTrigger::CardMovedToRole {
                                role: workflow::ListWorkflowRole::Cancelled,
                            },
                        },
                    ),
                    (
                        "Item is completed",
                        WorkflowNodeKind::Trigger {
                            trigger: WorkflowTrigger::CardCompleted,
                        },
                    ),
                    (
                        "Item is cancelled",
                        WorkflowNodeKind::Trigger {
                            trigger: WorkflowTrigger::CardCancelled,
                        },
                    ),
                    (
                        "Checklist is completed",
                        WorkflowNodeKind::Trigger {
                            trigger: WorkflowTrigger::ChecklistAllCompleted,
                        },
                    ),
                    (
                        "Run manually",
                        WorkflowNodeKind::Trigger {
                            trigger: WorkflowTrigger::Manual,
                        },
                    ),
                    (
                        "Calendar opens",
                        WorkflowNodeKind::Trigger {
                            trigger: WorkflowTrigger::Scheduled {
                                schedule_key: "calendar_open".to_string(),
                            },
                        },
                    ),
                ]
                .into_iter()
                .fold(menu, |menu, (label, kind)| {
                    let weak = weak.clone();
                    menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        weak.update(cx, |this, cx| {
                            this.configure_selected_workflow_node(kind.clone(), cx)
                        })
                        .ok();
                    }))
                })
            })
            .into_any_element()
    }

    pub(super) fn render_workflow_condition_controls(
        &self,
        kind: &WorkflowNodeKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let weak = cx.entity().downgrade();
        Button::new("workflow-condition-picker")
            .debug_selector(|| "workflow-condition-picker".into())
            .label(self.step_label(kind))
            .outline()
            .dropdown_menu(move |menu, _, _| {
                [
                    (
                        "Item is in a Done list",
                        WorkflowNodeKind::Condition {
                            condition: WorkflowCondition::ListRoleIs {
                                role: workflow::ListWorkflowRole::Done,
                            },
                        },
                    ),
                    (
                        "Item is in a Cancelled list",
                        WorkflowNodeKind::Condition {
                            condition: WorkflowCondition::ListRoleIs {
                                role: workflow::ListWorkflowRole::Cancelled,
                            },
                        },
                    ),
                    (
                        "Checklist is completed",
                        WorkflowNodeKind::Condition {
                            condition: WorkflowCondition::ChecklistIs {
                                state: workflow::ChecklistState::AllChecked,
                            },
                        },
                    ),
                    (
                        "Item is complete",
                        WorkflowNodeKind::Condition {
                            condition: WorkflowCondition::CompletionIs {
                                state: workflow::CompletionState::Completed,
                            },
                        },
                    ),
                    (
                        "Always continue",
                        WorkflowNodeKind::Condition {
                            condition: WorkflowCondition::Always,
                        },
                    ),
                ]
                .into_iter()
                .fold(menu, |menu, (label, kind)| {
                    let weak = weak.clone();
                    menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        weak.update(cx, |this, cx| {
                            this.configure_selected_workflow_node(kind.clone(), cx)
                        })
                        .ok();
                    }))
                })
            })
            .into_any_element()
    }

    pub(super) fn render_workflow_action_controls(
        &self,
        kind: &WorkflowNodeKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let weak = cx.entity().downgrade();
        Button::new("workflow-action-picker")
            .label(self.step_label(kind))
            .outline()
            .dropdown_menu(move |menu, _, _| {
                [
                    (
                        "Mark item complete",
                        WorkflowNodeKind::Action {
                            action: WorkflowAction::MarkComplete,
                        },
                    ),
                    (
                        "Cancel item",
                        WorkflowNodeKind::Action {
                            action: WorkflowAction::MarkCancelled,
                        },
                    ),
                    (
                        "Reopen item",
                        WorkflowNodeKind::Action {
                            action: WorkflowAction::Reopen,
                        },
                    ),
                    (
                        "Archive item",
                        WorkflowNodeKind::Action {
                            action: WorkflowAction::Archive,
                        },
                    ),
                    (
                        "Move item to trash",
                        WorkflowNodeKind::Action {
                            action: WorkflowAction::Trash,
                        },
                    ),
                    (
                        "Create next recurring item",
                        WorkflowNodeKind::Action {
                            action: WorkflowAction::CreateNextRecurringInstance,
                        },
                    ),
                ]
                .into_iter()
                .fold(menu, |menu, (label, kind)| {
                    let weak = weak.clone();
                    menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        weak.update(cx, |this, cx| {
                            this.configure_selected_workflow_node(kind.clone(), cx)
                        })
                        .ok();
                    }))
                })
            })
            .into_any_element()
    }

    pub(super) fn render_selected_node_controls(
        &self,
        selected_node: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let Some(kind) = selected_node.as_deref().and_then(|selected| {
            self.draft
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
            WorkflowNodeKind::Trigger { .. } => self.render_workflow_trigger_controls(&kind, cx),
            WorkflowNodeKind::Condition { .. } => {
                self.render_workflow_condition_controls(&kind, cx)
            }
            WorkflowNodeKind::Action { .. } => self.render_workflow_action_controls(&kind, cx),
            WorkflowNodeKind::Branch { .. } => div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Connect each named outcome below to decide where this branch continues.")
                .into_any_element(),
            WorkflowNodeKind::End { .. } => div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("End nodes stop this path.")
                .into_any_element(),
        };

        v_flex()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child(node_kind_label(&kind_for_list_parameters).to_ascii_uppercase()),
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
        let options = self.list_parameter_options(kind);
        if options.is_empty() {
            return div().into_any_element();
        }
        let weak = cx.entity().downgrade();
        h_flex()
            .child(
                Button::new("workflow-list-parameter-picker")
                    .debug_selector(|| "workflow-list-parameter-picker".into())
                    .label("Choose from board lists")
                    .text()
                    .small()
                    .dropdown_menu(move |menu, _, _| {
                        options.iter().fold(menu, |menu, (label, kind)| {
                            let weak = weak.clone();
                            let kind = kind.clone();
                            menu.item(PopupMenuItem::new(label.clone()).on_click(
                                move |_, _, cx| {
                                    weak.update(cx, |this, cx| {
                                        this.configure_selected_workflow_node(kind.clone(), cx)
                                    })
                                    .ok();
                                },
                            ))
                        })
                    }),
            )
            .into_any_element()
    }

    fn list_parameter_options(&self, kind: &WorkflowNodeKind) -> Vec<(String, WorkflowNodeKind)> {
        self.lists
            .iter()
            .flat_map(|list| {
                let list_id = i64::from(list.id);
                let title = list.title.clone();
                match kind {
                    WorkflowNodeKind::Trigger { .. } => vec![
                        (
                            format!("When an item moves to {title}"),
                            WorkflowNodeKind::Trigger {
                                trigger: WorkflowTrigger::CardMovedToList { list_id },
                            },
                        ),
                        (
                            format!("When an item leaves {title}"),
                            WorkflowNodeKind::Trigger {
                                trigger: WorkflowTrigger::CardMovedFromList { list_id },
                            },
                        ),
                    ],
                    WorkflowNodeKind::Condition { .. } => vec![(
                        format!("If item is in {title}"),
                        WorkflowNodeKind::Condition {
                            condition: WorkflowCondition::ListIs { list_id },
                        },
                    )],
                    WorkflowNodeKind::Action { .. } => vec![(
                        format!("Move item to {title}"),
                        WorkflowNodeKind::Action {
                            action: WorkflowAction::MoveToList {
                                list_id,
                                position: MovePosition::Bottom,
                            },
                        },
                    )],
                    WorkflowNodeKind::Branch { .. } | WorkflowNodeKind::End { .. } => Vec::new(),
                }
            })
            .collect()
    }

    pub(super) fn render_workflow_graph(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let definition = &self.draft.definition;
        if definition.nodes.is_empty() {
            return self.render_empty_workflow_canvas(cx);
        }
        let positions = definition
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.clone(), node_position(node, index)))
            .collect::<HashMap<_, _>>();
        const NODE_WIDTH: f32 = 264.;
        const NODE_HEIGHT: f32 = 96.;
        const EDGE_LABEL_GAP: f32 = 32.;
        const EDGE_LANE_OFFSET: f32 = 32.;

        let canvas_height = positions
            .values()
            .map(|(_, y)| y + 168.)
            .fold(self.canvas_height, f32::max);
        let canvas_width = positions
            .values()
            .map(|(x, _)| x + NODE_WIDTH + 120.)
            .fold(self.canvas_width, f32::max);
        let connectors = definition.edges.iter().filter_map(|edge| {
            let (from_x, from_y) = positions.get(&edge.from).copied()?;
            let (to_x, to_y) = positions.get(&edge.to).copied()?;
            let x1 = from_x + NODE_WIDTH / 2.;
            let x2 = to_x + NODE_WIDTH / 2.;
            let y1 = from_y + NODE_HEIGHT;
            let y2 = to_y;
            let line = |x: f32, y: f32, w: f32, h: f32| {
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(w.max(2.)))
                    .h(px(h.max(2.)))
                    .bg(theme.border)
            };
            let vertical_gap = y2 - y1;
            let (segments, label_x, label_y) = if vertical_gap >= EDGE_LABEL_GAP {
                let mid = (y1 + y2) / 2.;
                (
                    vec![
                        line(x1, y1, 2., mid - y1).into_any_element(),
                        line(x1.min(x2), mid, (x2 - x1).abs(), 2.).into_any_element(),
                        line(x2, mid, 2., y2 - mid).into_any_element(),
                    ],
                    x1 + 8.,
                    mid - 8.,
                )
            } else {
                let source_y = from_y + NODE_HEIGHT / 2.;
                let target_y = to_y + NODE_HEIGHT / 2.;
                let source_right = from_x + NODE_WIDTH;
                let target_right = to_x + NODE_WIDTH;
                let lane_x = source_right.max(target_right) + EDGE_LANE_OFFSET;
                (
                    vec![
                        line(source_right, source_y, lane_x - source_right, 2.).into_any_element(),
                        line(
                            lane_x,
                            source_y.min(target_y),
                            2.,
                            (target_y - source_y).abs(),
                        )
                        .into_any_element(),
                        line(target_right, target_y, lane_x - target_right, 2.).into_any_element(),
                    ],
                    lane_x + 8.,
                    (source_y + target_y) / 2. - 8.,
                )
            };
            Some(
                div()
                    .id(SharedString::from(format!("connector-{}", edge.id)))
                    .absolute()
                    .inset_0()
                    .children(segments)
                    .child(
                        div()
                            .debug_selector({
                                let edge_id = edge.id.clone();
                                move || format!("workflow-edge-label-{edge_id}")
                            })
                            .absolute()
                            .left(px(label_x))
                            .top(px(label_y))
                            .px_1()
                            .text_xs()
                            .bg(theme.background)
                            .text_color(theme.muted_foreground)
                            .child(connection_label(edge, &definition.nodes)),
                    )
                    .into_any_element(),
            )
        });
        let nodes = definition.nodes.iter().enumerate().map(|(index, node)| {
            let (x, y) = node_position(node, index);
            let node_id = node.id.clone();
            let selected = self.draft.selected_node.as_deref() == Some(node.id.as_str());
            let accent = match &node.kind {
                WorkflowNodeKind::Trigger { .. } => theme.primary,
                WorkflowNodeKind::Condition { .. } | WorkflowNodeKind::Branch { .. } => {
                    theme.warning
                }
                WorkflowNodeKind::Action { .. } => theme.success,
                WorkflowNodeKind::End { .. } => theme.muted_foreground,
            };
            let border = if selected {
                accent
            } else {
                theme.border.opacity(0.78)
            };
            let outgoing_count = definition
                .edges
                .iter()
                .filter(|edge| edge.from == node.id)
                .count();
            v_flex()
                .id(SharedString::from(format!("workflow-node-{}", node.id)))
                .debug_selector(|| format!("workflow-node-{}", node.id))
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(264.))
                .h(px(96.))
                .gap_1()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(border)
                .bg(if selected {
                    accent.opacity(0.08)
                } else {
                    theme.popover
                })
                .shadow_sm()
                .hover(|this| this.border_color(accent.opacity(0.88)))
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.draft.selected_node = Some(node_id.clone());
                        if this.route_narrow(WorkflowRoute::Editor) {
                            cx.emit(WorkflowWorkspaceEvent::Navigate(WorkflowRoute::Step));
                        }
                        this.draft.node_drag_start = None;
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
                            .draft
                            .definition
                            .nodes
                            .iter()
                            .position(|node| node.id == drag.node_id)
                        else {
                            return;
                        };

                        let current_position =
                            node_position(&this.draft.definition.nodes[node_index], node_index);
                        let start = match this.draft.node_drag_start.clone() {
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
                                this.draft.node_drag_start = Some(start);
                                return;
                            }
                        };

                        let delta = event.event.position.relative_to(&start.pointer);
                        let node = &mut this.draft.definition.nodes[node_index];
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
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(accent)
                        .child(Icon::new(node_icon(&node.kind)).xsmall())
                        .child(node_kind_label(&node.kind).to_ascii_uppercase()),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(self.step_label(&node.kind)),
                )
                .child(div().text_xs().text_color(theme.muted_foreground).child(
                    if outgoing_count == 0 {
                        "Choose the next step".to_string()
                    } else if outgoing_count == 1 {
                        "1 connected outcome".to_string()
                    } else {
                        format!("{outgoing_count} connected outcomes")
                    },
                ))
                .into_any_element()
        });

        div()
            .id("workflow-graph-canvas")
            .debug_selector(|| "workflow-graph-canvas".to_string())
            .relative()
            .w(px(canvas_width))
            .h(px(canvas_height))
            .min_h(px(400.))
            .children(connectors)
            .children(nodes)
            .into_any_element()
    }

    fn render_empty_workflow_canvas(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let width = self.canvas_width.max(640.);
        let height = self.canvas_height.max(420.);
        div()
            .id("workflow-graph-canvas")
            .debug_selector(|| "workflow-graph-canvas".to_string())
            .relative()
            .w(px(width))
            .h(px(height))
            .min_h(px(400.))
            .child(
                v_flex()
                    .id("workflow-empty-canvas")
                    .debug_selector(|| "workflow-empty-canvas".into())
                    .absolute()
                    .inset_0()
                    .items_center()
                    .justify_center()
                    .p_6()
                    .child(
                        v_flex()
                            .max_w(px(360.))
                            .gap_3()
                            .items_center()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Start with a trigger"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("A workflow needs to know what event should begin the automation. You can refine it in the builder after adding it."),
                            )
                            .child(
                                Button::new("workflow-empty-add-trigger")
                                    .debug_selector(|| "workflow-empty-add-trigger".into())
                                    .label("Add a trigger")
                                    .icon(IconName::Play)
                                    .primary()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.add_workflow_node(
                                            WorkflowNodeKind::Trigger {
                                                trigger: WorkflowTrigger::Manual,
                                            },
                                            cx,
                                        );
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}
