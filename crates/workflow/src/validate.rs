use std::collections::{HashMap, HashSet, VecDeque};

use crate::{
    CURRENT_SCHEMA_VERSION, WorkflowAction, WorkflowCondition, WorkflowDefinition,
    WorkflowNodeKind, WorkflowTrigger,
};

pub fn validate(definition: &WorkflowDefinition) -> Result<(), String> {
    if definition.schema_version != 0 && definition.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "unsupported workflow schema version {}",
            definition.schema_version
        ));
    }

    let mut node_ids = HashSet::with_capacity(definition.nodes.len());
    for node in &definition.nodes {
        if node.id.trim().is_empty() {
            return Err("workflow node IDs must not be empty".to_string());
        }
        if !node_ids.insert(node.id.as_str()) {
            return Err(format!("workflow node ID {:?} is duplicated", node.id));
        }
        validate_node_kind(&node.kind)?;
    }

    let mut edge_ids = HashSet::with_capacity(definition.edges.len());
    for edge in &definition.edges {
        if edge.id.trim().is_empty() {
            return Err("workflow edge IDs must not be empty".to_string());
        }
        if !edge_ids.insert(edge.id.as_str()) {
            return Err(format!("workflow edge ID {:?} is duplicated", edge.id));
        }
        if !node_ids.contains(edge.from.as_str()) || !node_ids.contains(edge.to.as_str()) {
            return Err(format!(
                "workflow edge {:?} references a missing node",
                edge.id
            ));
        }
    }

    let trigger_ids = definition
        .nodes
        .iter()
        .filter_map(|node| {
            matches!(node.kind, WorkflowNodeKind::Trigger { .. }).then_some(node.id.as_str())
        })
        .collect::<Vec<_>>();
    if definition.enabled {
        if trigger_ids.len() != 1 {
            return Err(format!(
                "an enabled workflow must have exactly one trigger; found {}",
                trigger_ids.len()
            ));
        }
        if definition.edges.is_empty() {
            return Err("an enabled workflow must connect its trigger".to_string());
        }
        if !definition
            .nodes
            .iter()
            .any(|node| matches!(node.kind, WorkflowNodeKind::Action { .. }))
        {
            return Err("an enabled workflow must contain an action".to_string());
        }
    }

    if let Some(cycle) = find_cycle(definition) {
        return Err(format!("workflow graph contains a cycle at node {cycle:?}"));
    }

    if definition.enabled {
        let trigger_id = trigger_ids[0];
        if !reaches_terminal(definition, trigger_id) {
            return Err("the trigger does not reach an action or end node".to_string());
        }
    }

    Ok(())
}

fn validate_node_kind(kind: &WorkflowNodeKind) -> Result<(), String> {
    match kind {
        WorkflowNodeKind::Trigger { trigger } => match trigger {
            WorkflowTrigger::CardMovedToList { list_id }
            | WorkflowTrigger::CardMovedFromList { list_id }
                if *list_id <= 0 =>
            {
                Err("workflow list IDs must be positive".to_string())
            }
            WorkflowTrigger::LabelAdded { label } | WorkflowTrigger::LabelRemoved { label }
                if label.trim().is_empty() =>
            {
                Err("workflow label names must not be empty".to_string())
            }
            WorkflowTrigger::PropertyChanged { key } if key.trim().is_empty() => {
                Err("workflow property keys must not be empty".to_string())
            }
            WorkflowTrigger::Scheduled { schedule_key } if schedule_key.trim().is_empty() => {
                Err("workflow schedule keys must not be empty".to_string())
            }
            _ => Ok(()),
        },
        WorkflowNodeKind::Condition { condition } => validate_condition(condition),
        WorkflowNodeKind::Branch { cases } => {
            let mut case_ids = HashSet::with_capacity(cases.len());
            for case in cases {
                if case.id.trim().is_empty() {
                    return Err("workflow branch case IDs must not be empty".to_string());
                }
                if !case_ids.insert(case.id.as_str()) {
                    return Err(format!(
                        "workflow branch case ID {:?} is duplicated",
                        case.id
                    ));
                }
                validate_condition(&case.condition)?;
            }
            Ok(())
        }
        WorkflowNodeKind::Action { action } => validate_action(action),
        WorkflowNodeKind::End { .. } => Ok(()),
    }
}

fn validate_condition(condition: &WorkflowCondition) -> Result<(), String> {
    match condition {
        WorkflowCondition::All { conditions } | WorkflowCondition::Any { conditions } => {
            for condition in conditions {
                validate_condition(condition)?;
            }
        }
        WorkflowCondition::Not { condition } => validate_condition(condition)?,
        WorkflowCondition::ListIs { list_id } | WorkflowCondition::PreviousListIs { list_id }
            if *list_id <= 0 =>
        {
            return Err("workflow list IDs must be positive".to_string());
        }
        WorkflowCondition::LabelContains { label } if label.trim().is_empty() => {
            return Err("workflow label names must not be empty".to_string());
        }
        WorkflowCondition::PropertyEquals { key, .. } if key.trim().is_empty() => {
            return Err("workflow property keys must not be empty".to_string());
        }
        _ => {}
    }
    Ok(())
}

fn validate_action(action: &WorkflowAction) -> Result<(), String> {
    match action {
        WorkflowAction::MoveToList { list_id, .. } if *list_id <= 0 => {
            Err("workflow list IDs must be positive".to_string())
        }
        WorkflowAction::AddLabel { label } | WorkflowAction::RemoveLabel { label }
            if label.trim().is_empty() =>
        {
            Err("workflow label names must not be empty".to_string())
        }
        WorkflowAction::SetProperty { key, .. } | WorkflowAction::ClearProperty { key }
            if key.trim().is_empty() =>
        {
            Err("workflow property keys must not be empty".to_string())
        }
        WorkflowAction::Notify { message } | WorkflowAction::AddHistory { message }
            if message.trim().is_empty() =>
        {
            Err("workflow messages must not be empty".to_string())
        }
        WorkflowAction::SetDueDate { due_on }
            if due_on_or_start_on_has_invalid_shape(due_on.as_deref()) =>
        {
            Err("workflow dates must use YYYY-MM-DD format".to_string())
        }
        WorkflowAction::SetStartDate { start_on }
            if due_on_or_start_on_has_invalid_shape(start_on.as_deref()) =>
        {
            Err("workflow dates must use YYYY-MM-DD format".to_string())
        }
        _ => Ok(()),
    }
}

fn due_on_or_start_on_has_invalid_shape(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let bytes = value.as_bytes();
    bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn find_cycle(definition: &WorkflowDefinition) -> Option<String> {
    let mut outgoing = HashMap::<&str, Vec<&str>>::new();
    for edge in &definition.edges {
        outgoing
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for node in &definition.nodes {
        if visit_cycle(node.id.as_str(), &outgoing, &mut visiting, &mut visited) {
            return Some(node.id.clone());
        }
    }
    None
}

fn visit_cycle<'a>(
    node_id: &'a str,
    outgoing: &HashMap<&'a str, Vec<&'a str>>,
    visiting: &mut HashSet<&'a str>,
    visited: &mut HashSet<&'a str>,
) -> bool {
    if visiting.contains(node_id) {
        return true;
    }
    if !visited.insert(node_id) {
        return false;
    }
    visiting.insert(node_id);
    let cycle = outgoing
        .get(node_id)
        .into_iter()
        .flatten()
        .any(|next| visit_cycle(next, outgoing, visiting, visited));
    visiting.remove(node_id);
    cycle
}

fn reaches_terminal(definition: &WorkflowDefinition, trigger_id: &str) -> bool {
    let nodes = definition
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut outgoing = HashMap::<&str, Vec<&str>>::new();
    for edge in &definition.edges {
        outgoing
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    let mut queue = VecDeque::from([trigger_id]);
    let mut visited = HashSet::new();
    while let Some(node_id) = queue.pop_front() {
        if !visited.insert(node_id) {
            continue;
        }
        let Some(node) = nodes.get(node_id) else {
            continue;
        };
        if matches!(
            node.kind,
            WorkflowNodeKind::Action { .. } | WorkflowNodeKind::End { .. }
        ) {
            return true;
        }
        queue.extend(outgoing.get(node_id).into_iter().flatten().copied());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WorkflowAction, WorkflowEdge, WorkflowEdgeKind, WorkflowNode, WorkflowTrigger};

    fn definition(enabled: bool) -> WorkflowDefinition {
        WorkflowDefinition {
            enabled,
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
                        action: WorkflowAction::Archive,
                    },
                    position: Default::default(),
                },
            ],
            edges: vec![WorkflowEdge {
                id: "edge".to_string(),
                from: "trigger".to_string(),
                to: "action".to_string(),
                kind: WorkflowEdgeKind::Default,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn rejects_invalid_enabled_graphs() {
        let mut invalid = definition(true);
        invalid.edges[0].to = "missing".to_string();
        assert!(validate(&invalid).is_err());

        let mut invalid = definition(true);
        invalid.nodes.push(WorkflowNode {
            id: "second-trigger".to_string(),
            kind: WorkflowNodeKind::Trigger {
                trigger: WorkflowTrigger::Manual,
            },
            position: Default::default(),
        });
        assert!(validate(&invalid).is_err());
    }

    #[test]
    fn accepts_disabled_drafts_and_rejects_cycles() {
        let mut draft = definition(false);
        draft.edges[0].to = "missing".to_string();
        assert!(validate(&draft).is_err());

        let mut cyclic = definition(true);
        cyclic.edges.push(WorkflowEdge {
            id: "cycle".to_string(),
            from: "action".to_string(),
            to: "trigger".to_string(),
            kind: WorkflowEdgeKind::Default,
        });
        assert!(validate(&cyclic).is_err());
    }

    #[test]
    fn rejects_invalid_action_parameters() {
        let mut invalid = definition(true);
        invalid.nodes[1].kind = WorkflowNodeKind::Action {
            action: WorkflowAction::SetDueDate {
                due_on: Some("2026-9-1".to_string()),
            },
        };
        assert!(validate(&invalid).is_err());

        let mut invalid = definition(true);
        invalid.nodes[1].kind = WorkflowNodeKind::Action {
            action: WorkflowAction::Notify {
                message: "  ".to_string(),
            },
        };
        assert!(validate(&invalid).is_err());
    }
}
