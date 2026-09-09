use std::collections::{HashMap, HashSet, VecDeque};

use crate::model::{
    WorkflowAction, WorkflowCondition, WorkflowContext, WorkflowDefinition, WorkflowEdge,
    WorkflowEdgeKind, WorkflowEvent, WorkflowEventKind, WorkflowNodeKind, WorkflowTrigger,
};

pub fn evaluate(
    definition: &WorkflowDefinition,
    event: &WorkflowEvent,
    context: &WorkflowContext,
) -> Vec<WorkflowAction> {
    if !definition.enabled || definition.nodes.is_empty() || definition.edges.is_empty() {
        return Vec::new();
    }

    let nodes = definition
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();

    let outgoing = build_outgoing_edges(definition);
    let mut queue = definition
        .nodes
        .iter()
        .filter_map(|node| match &node.kind {
            WorkflowNodeKind::Trigger { trigger } if trigger_matches(trigger, event) => {
                Some(node.id.as_str())
            }
            _ => None,
        })
        .collect::<VecDeque<_>>();

    let mut visited = HashSet::new();
    let mut actions = Vec::new();

    while let Some(node_id) = queue.pop_front() {
        if !visited.insert(node_id) {
            continue;
        }
        let Some(node) = nodes.get(node_id) else {
            continue;
        };

        let follow_edges = match &node.kind {
            WorkflowNodeKind::Trigger { .. } | WorkflowNodeKind::Action { .. } => {
                if let WorkflowNodeKind::Action { action } = &node.kind {
                    actions.push(action.clone());
                }
                outgoing.get(node_id).cloned().unwrap_or_default()
            }
            WorkflowNodeKind::Condition { condition } => select_condition_edges(
                outgoing.get(node_id),
                condition_matches(condition, event, context),
            ),
            WorkflowNodeKind::Branch { cases } => {
                select_branch_edges(outgoing.get(node_id), cases, event, context)
            }
            WorkflowNodeKind::End { .. } => Vec::new(),
        };

        for edge in follow_edges {
            queue.push_back(edge.to.as_str());
        }
    }

    actions
}

fn build_outgoing_edges(definition: &WorkflowDefinition) -> HashMap<&str, Vec<&WorkflowEdge>> {
    let mut outgoing = HashMap::<&str, Vec<&WorkflowEdge>>::new();
    for edge in &definition.edges {
        outgoing.entry(edge.from.as_str()).or_default().push(edge);
    }
    for edges in outgoing.values_mut() {
        edges.sort_by(|left, right| {
            edge_kind_order(&left.kind)
                .cmp(&edge_kind_order(&right.kind))
                .then_with(|| left.to.cmp(&right.to))
                .then_with(|| left.id.cmp(&right.id))
        });
    }
    outgoing
}

fn select_condition_edges<'a>(
    edges: Option<&'a Vec<&'a WorkflowEdge>>,
    matched: bool,
) -> Vec<&'a WorkflowEdge> {
    let Some(edges) = edges else {
        return Vec::new();
    };
    let desired = if matched {
        WorkflowEdgeKind::True
    } else {
        WorkflowEdgeKind::False
    };

    let selected = edges
        .iter()
        .copied()
        .filter(|edge| edge.kind == desired)
        .collect::<Vec<_>>();

    if selected.is_empty() {
        edges
            .iter()
            .copied()
            .filter(|edge| matches!(edge.kind, WorkflowEdgeKind::Default))
            .collect()
    } else {
        selected
    }
}

fn select_branch_edges<'a>(
    edges: Option<&'a Vec<&'a WorkflowEdge>>,
    cases: &[crate::model::WorkflowBranchCase],
    event: &WorkflowEvent,
    context: &WorkflowContext,
) -> Vec<&'a WorkflowEdge> {
    let Some(edges) = edges else {
        return Vec::new();
    };

    let selected_case = cases
        .iter()
        .find(|case| condition_matches(&case.condition, event, context));

    let selected = match selected_case {
        Some(case) => edges
            .iter()
            .copied()
            .filter(|edge| matches!(&edge.kind, WorkflowEdgeKind::Case(id) if id == &case.id))
            .collect::<Vec<_>>(),
        None => edges
            .iter()
            .copied()
            .filter(|edge| matches!(edge.kind, WorkflowEdgeKind::Otherwise))
            .collect::<Vec<_>>(),
    };
    if selected.is_empty() {
        edges
            .iter()
            .copied()
            .filter(|edge| matches!(edge.kind, WorkflowEdgeKind::Default))
            .collect()
    } else {
        selected
    }
}

fn trigger_matches(trigger: &WorkflowTrigger, event: &WorkflowEvent) -> bool {
    match (trigger, &event.kind) {
        (WorkflowTrigger::CardCreated, WorkflowEventKind::CardCreated { .. }) => true,
        (
            WorkflowTrigger::CardMovedToList { list_id },
            WorkflowEventKind::CardMoved { to_list_id, .. },
        ) => list_id == to_list_id,
        (
            WorkflowTrigger::CardMovedFromList { list_id },
            WorkflowEventKind::CardMoved { from_list_id, .. },
        ) => from_list_id == &Some(*list_id),
        (
            WorkflowTrigger::CardMovedToRole { role },
            WorkflowEventKind::CardMoved { to_list_role, .. },
        ) => role == to_list_role,
        (
            WorkflowTrigger::CardMovedFromRole { role },
            WorkflowEventKind::CardMoved { from_list_role, .. },
        ) => from_list_role == &Some(*role),
        (WorkflowTrigger::CardCompleted, WorkflowEventKind::CardCompleted) => true,
        (WorkflowTrigger::CardCancelled, WorkflowEventKind::CardCancelled) => true,
        (WorkflowTrigger::CardReopened, WorkflowEventKind::CardReopened) => true,
        (
            WorkflowTrigger::ChecklistAllCompleted,
            WorkflowEventKind::ChecklistChanged {
                checked_count,
                total_count,
            },
        ) => *total_count > 0 && checked_count >= total_count,
        (WorkflowTrigger::ChecklistItemChecked, WorkflowEventKind::ChecklistChanged { .. }) => true,
        (
            WorkflowTrigger::LabelAdded { label },
            WorkflowEventKind::LabelChanged {
                label: changed,
                added,
            },
        ) => *added && label == changed,
        (
            WorkflowTrigger::LabelRemoved { label },
            WorkflowEventKind::LabelChanged {
                label: changed,
                added,
            },
        ) => !added && label == changed,
        (
            WorkflowTrigger::PropertyChanged { key },
            WorkflowEventKind::PropertyChanged { key: changed },
        ) => key == changed,
        (
            WorkflowTrigger::DueDateStatus { status },
            WorkflowEventKind::DueDateChanged { status: changed },
        ) => status == changed,
        (
            WorkflowTrigger::Scheduled { schedule_key },
            WorkflowEventKind::Scheduled {
                schedule_key: changed,
            },
        ) => schedule_key == changed,
        (WorkflowTrigger::Manual, WorkflowEventKind::Manual) => true,
        _ => false,
    }
}

fn condition_matches(
    condition: &WorkflowCondition,
    event: &WorkflowEvent,
    context: &WorkflowContext,
) -> bool {
    match condition {
        WorkflowCondition::All { conditions } => conditions
            .iter()
            .all(|condition| condition_matches(condition, event, context)),
        WorkflowCondition::Any { conditions } => conditions
            .iter()
            .any(|condition| condition_matches(condition, event, context)),
        WorkflowCondition::Not { condition } => !condition_matches(condition, event, context),
        WorkflowCondition::Always => true,
        WorkflowCondition::ListIs { list_id } => context.list_id == *list_id,
        WorkflowCondition::ListRoleIs { role } => context.list_role == *role,
        WorkflowCondition::PreviousListIs { list_id } => previous_list_id(event) == Some(*list_id),
        WorkflowCondition::PreviousListRoleIs { role } => previous_list_role(event) == Some(*role),
        WorkflowCondition::LabelContains { label } => context.has_label(label),
        WorkflowCondition::CompletionIs { state } => context.completion_state == *state,
        WorkflowCondition::DueDateIs { status } => context.due_date_status == *status,
        WorkflowCondition::ChecklistIs { state } => context.checklist_state() == *state,
        WorkflowCondition::PropertyEquals { key, value } => {
            context.properties.get(key) == Some(value)
        }
        WorkflowCondition::EventOriginIs { origin } => event.origin == *origin,
        WorkflowCondition::EventIs { name } => event.kind.name() == *name,
    }
}

fn previous_list_id(event: &WorkflowEvent) -> Option<i64> {
    match event.kind {
        WorkflowEventKind::CardMoved { from_list_id, .. } => from_list_id,
        _ => None,
    }
}

fn previous_list_role(event: &WorkflowEvent) -> Option<crate::model::ListWorkflowRole> {
    match event.kind {
        WorkflowEventKind::CardMoved { from_list_role, .. } => from_list_role,
        _ => None,
    }
}

fn edge_kind_order(kind: &WorkflowEdgeKind) -> u8 {
    match kind {
        WorkflowEdgeKind::Default => 0,
        WorkflowEdgeKind::True => 1,
        WorkflowEdgeKind::False => 2,
        WorkflowEdgeKind::Case(_) => 3,
        WorkflowEdgeKind::Otherwise => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        EventOrigin, ListWorkflowRole, MovePosition, WorkflowEdge, WorkflowNode, WorkflowNodeKind,
    };

    fn event(kind: WorkflowEventKind) -> WorkflowEvent {
        WorkflowEvent {
            event_id: "event-1".to_string(),
            board_id: 1,
            entry_id: 2,
            occurred_at: "2026-09-09T12:00:00Z".to_string(),
            origin: EventOrigin::User,
            kind,
        }
    }

    fn context() -> WorkflowContext {
        WorkflowContext {
            board_id: 1,
            entry_id: 2,
            list_id: 8,
            list_role: ListWorkflowRole::Done,
            ..Default::default()
        }
    }

    #[test]
    fn evaluates_true_condition_and_preserves_action_order() {
        let definition = WorkflowDefinition {
            enabled: true,
            nodes: vec![
                WorkflowNode {
                    id: "trigger".to_string(),
                    kind: WorkflowNodeKind::Trigger {
                        trigger: WorkflowTrigger::CardMovedToRole {
                            role: ListWorkflowRole::Done,
                        },
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "condition".to_string(),
                    kind: WorkflowNodeKind::Condition {
                        condition: WorkflowCondition::All {
                            conditions: vec![
                                WorkflowCondition::ListRoleIs {
                                    role: ListWorkflowRole::Done,
                                },
                                WorkflowCondition::EventOriginIs {
                                    origin: EventOrigin::User,
                                },
                            ],
                        },
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "first".to_string(),
                    kind: WorkflowNodeKind::Action {
                        action: WorkflowAction::MarkComplete,
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "second".to_string(),
                    kind: WorkflowNodeKind::Action {
                        action: WorkflowAction::MoveToList {
                            list_id: 13,
                            position: MovePosition::Bottom,
                        },
                    },
                    position: Default::default(),
                },
            ],
            edges: vec![
                WorkflowEdge {
                    id: "e1".to_string(),
                    from: "trigger".to_string(),
                    to: "condition".to_string(),
                    kind: WorkflowEdgeKind::Default,
                },
                WorkflowEdge {
                    id: "e2".to_string(),
                    from: "condition".to_string(),
                    to: "first".to_string(),
                    kind: WorkflowEdgeKind::True,
                },
                WorkflowEdge {
                    id: "e3".to_string(),
                    from: "first".to_string(),
                    to: "second".to_string(),
                    kind: WorkflowEdgeKind::Default,
                },
            ],
            ..Default::default()
        };

        assert_eq!(
            evaluate(
                &definition,
                &event(WorkflowEventKind::CardMoved {
                    from_list_id: Some(3),
                    from_list_role: Some(ListWorkflowRole::Neutral),
                    to_list_id: 8,
                    to_list_role: ListWorkflowRole::Done,
                }),
                &context(),
            ),
            vec![
                WorkflowAction::MarkComplete,
                WorkflowAction::MoveToList {
                    list_id: 13,
                    position: MovePosition::Bottom,
                }
            ]
        );
    }

    #[test]
    fn false_condition_does_not_fall_through_to_true_branch() {
        let definition = WorkflowDefinition {
            enabled: true,
            nodes: vec![
                WorkflowNode {
                    id: "trigger".to_string(),
                    kind: WorkflowNodeKind::Trigger {
                        trigger: WorkflowTrigger::Manual,
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "condition".to_string(),
                    kind: WorkflowNodeKind::Condition {
                        condition: WorkflowCondition::ListRoleIs {
                            role: ListWorkflowRole::Done,
                        },
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "archive".to_string(),
                    kind: WorkflowNodeKind::Action {
                        action: WorkflowAction::Archive,
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "trash".to_string(),
                    kind: WorkflowNodeKind::Action {
                        action: WorkflowAction::Trash,
                    },
                    position: Default::default(),
                },
            ],
            edges: vec![
                WorkflowEdge {
                    id: "e1".to_string(),
                    from: "trigger".to_string(),
                    to: "condition".to_string(),
                    kind: WorkflowEdgeKind::Default,
                },
                WorkflowEdge {
                    id: "e2".to_string(),
                    from: "condition".to_string(),
                    to: "archive".to_string(),
                    kind: WorkflowEdgeKind::True,
                },
                WorkflowEdge {
                    id: "e3".to_string(),
                    from: "condition".to_string(),
                    to: "trash".to_string(),
                    kind: WorkflowEdgeKind::False,
                },
            ],
            ..Default::default()
        };
        let mut context = context();
        context.list_role = ListWorkflowRole::Neutral;
        assert_eq!(
            evaluate(&definition, &event(WorkflowEventKind::Manual), &context),
            vec![WorkflowAction::Trash]
        );
    }

    #[test]
    fn role_conditions_distinguish_current_and_previous_list_roles() {
        let definition = WorkflowDefinition {
            enabled: true,
            nodes: vec![
                WorkflowNode {
                    id: "trigger".to_string(),
                    kind: WorkflowNodeKind::Trigger {
                        trigger: WorkflowTrigger::CardMovedToRole {
                            role: ListWorkflowRole::Done,
                        },
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "condition".to_string(),
                    kind: WorkflowNodeKind::Condition {
                        condition: WorkflowCondition::All {
                            conditions: vec![
                                WorkflowCondition::ListRoleIs {
                                    role: ListWorkflowRole::Done,
                                },
                                WorkflowCondition::PreviousListRoleIs {
                                    role: ListWorkflowRole::Neutral,
                                },
                            ],
                        },
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "match".to_string(),
                    kind: WorkflowNodeKind::Action {
                        action: WorkflowAction::MarkComplete,
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "mismatch".to_string(),
                    kind: WorkflowNodeKind::Action {
                        action: WorkflowAction::Trash,
                    },
                    position: Default::default(),
                },
            ],
            edges: vec![
                WorkflowEdge {
                    id: "trigger-condition".to_string(),
                    from: "trigger".to_string(),
                    to: "condition".to_string(),
                    kind: WorkflowEdgeKind::Default,
                },
                WorkflowEdge {
                    id: "condition-match".to_string(),
                    from: "condition".to_string(),
                    to: "match".to_string(),
                    kind: WorkflowEdgeKind::True,
                },
                WorkflowEdge {
                    id: "condition-mismatch".to_string(),
                    from: "condition".to_string(),
                    to: "mismatch".to_string(),
                    kind: WorkflowEdgeKind::False,
                },
            ],
            ..Default::default()
        };

        assert_eq!(
            evaluate(
                &definition,
                &event(WorkflowEventKind::CardMoved {
                    from_list_id: Some(3),
                    from_list_role: Some(ListWorkflowRole::Neutral),
                    to_list_id: 8,
                    to_list_role: ListWorkflowRole::Done,
                }),
                &context(),
            ),
            vec![WorkflowAction::MarkComplete]
        );
        assert_eq!(
            evaluate(
                &definition,
                &event(WorkflowEventKind::CardMoved {
                    from_list_id: Some(3),
                    from_list_role: Some(ListWorkflowRole::Done),
                    to_list_id: 8,
                    to_list_role: ListWorkflowRole::Done,
                }),
                &context(),
            ),
            vec![WorkflowAction::Trash]
        );
    }

    #[test]
    fn fan_out_action_order_is_stable_when_nodes_and_edges_are_shuffled() {
        let definition = WorkflowDefinition {
            enabled: true,
            nodes: vec![
                WorkflowNode {
                    id: "z-action".to_string(),
                    kind: WorkflowNodeKind::Action {
                        action: WorkflowAction::SetStartDate {
                            start_on: Some("2026-09-10".to_string()),
                        },
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "trigger".to_string(),
                    kind: WorkflowNodeKind::Trigger {
                        trigger: WorkflowTrigger::Manual,
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "a-action".to_string(),
                    kind: WorkflowNodeKind::Action {
                        action: WorkflowAction::SetDueDate {
                            due_on: Some("2026-09-11".to_string()),
                        },
                    },
                    position: Default::default(),
                },
            ],
            edges: vec![
                WorkflowEdge {
                    id: "to-z".to_string(),
                    from: "trigger".to_string(),
                    to: "z-action".to_string(),
                    kind: WorkflowEdgeKind::Default,
                },
                WorkflowEdge {
                    id: "to-a".to_string(),
                    from: "trigger".to_string(),
                    to: "a-action".to_string(),
                    kind: WorkflowEdgeKind::Default,
                },
            ],
            ..Default::default()
        };

        assert_eq!(
            evaluate(&definition, &event(WorkflowEventKind::Manual), &context()),
            vec![
                WorkflowAction::SetDueDate {
                    due_on: Some("2026-09-11".to_string()),
                },
                WorkflowAction::SetStartDate {
                    start_on: Some("2026-09-10".to_string()),
                },
            ]
        );
    }

    #[test]
    fn checklist_all_completed_accepts_consistent_overcounted_input() {
        let definition = WorkflowDefinition {
            enabled: true,
            nodes: vec![
                WorkflowNode {
                    id: "trigger".to_string(),
                    kind: WorkflowNodeKind::Trigger {
                        trigger: WorkflowTrigger::ChecklistAllCompleted,
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "complete".to_string(),
                    kind: WorkflowNodeKind::Action {
                        action: WorkflowAction::MarkComplete,
                    },
                    position: Default::default(),
                },
            ],
            edges: vec![WorkflowEdge {
                id: "trigger-complete".to_string(),
                from: "trigger".to_string(),
                to: "complete".to_string(),
                kind: WorkflowEdgeKind::Default,
            }],
            ..Default::default()
        };

        assert_eq!(
            evaluate(
                &definition,
                &event(WorkflowEventKind::ChecklistChanged {
                    checked_count: 4,
                    total_count: 3,
                }),
                &context(),
            ),
            vec![WorkflowAction::MarkComplete]
        );
    }

    #[test]
    fn cyclic_graph_executes_each_node_once() {
        let definition = WorkflowDefinition {
            enabled: true,
            nodes: vec![
                WorkflowNode {
                    id: "trigger".to_string(),
                    kind: WorkflowNodeKind::Trigger {
                        trigger: WorkflowTrigger::Manual,
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "action".to_string(),
                    kind: WorkflowNodeKind::Action {
                        action: WorkflowAction::AddHistory {
                            message: "once".to_string(),
                        },
                    },
                    position: Default::default(),
                },
            ],
            edges: vec![
                WorkflowEdge {
                    id: "e1".to_string(),
                    from: "trigger".to_string(),
                    to: "action".to_string(),
                    kind: WorkflowEdgeKind::Default,
                },
                WorkflowEdge {
                    id: "e2".to_string(),
                    from: "action".to_string(),
                    to: "action".to_string(),
                    kind: WorkflowEdgeKind::Default,
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            evaluate(&definition, &event(WorkflowEventKind::Manual), &context()).len(),
            1
        );
    }
}
