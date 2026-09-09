use crate::model::{WorkflowDefinition, WorkflowEdgeKind, WorkflowNodeKind};

pub fn to_mermaid(definition: &WorkflowDefinition) -> String {
    let mut output = String::from("flowchart LR\n");
    let mut nodes = definition.nodes.iter().collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    for node in nodes {
        let label = node_label(&node.kind);
        let node_id = mermaid_id(&node.id);
        let (open, close) = match &node.kind {
            WorkflowNodeKind::Trigger { .. } => ("((", "))"),
            WorkflowNodeKind::Condition { .. } => ("{", "}"),
            WorkflowNodeKind::Branch { .. } => ("{{", "}}"),
            WorkflowNodeKind::Action { .. } => ("[", "]"),
            WorkflowNodeKind::End { .. } => ("((", "))"),
        };
        output.push_str(&format!("    {node_id}{open}\"{label}\"{close}\n"));
    }

    let mut edges = definition.edges.iter().collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then_with(|| left.to.cmp(&right.to))
            .then_with(|| left.id.cmp(&right.id))
    });
    for edge in edges {
        output.push_str("    ");
        output.push_str(&mermaid_id(&edge.from));
        match &edge.kind {
            WorkflowEdgeKind::Default => output.push_str(" --> "),
            WorkflowEdgeKind::True => output.push_str(" -->|true| "),
            WorkflowEdgeKind::False => output.push_str(" -->|false| "),
            WorkflowEdgeKind::Case(case) => output.push_str(&format!(" -->|{}| ", escape(case))),
            WorkflowEdgeKind::Otherwise => output.push_str(" -->|otherwise| "),
        }
        output.push_str(&mermaid_id(&edge.to));
        output.push('\n');
    }
    output
}

fn node_label(kind: &WorkflowNodeKind) -> String {
    match kind {
        WorkflowNodeKind::Trigger { trigger } => format!("Trigger: {}", debug_label(trigger)),
        WorkflowNodeKind::Condition { condition } => format!("If: {}", debug_label(condition)),
        WorkflowNodeKind::Branch { cases } => {
            if cases.is_empty() {
                "Branch".to_string()
            } else {
                format!(
                    "Branch: {}",
                    cases
                        .iter()
                        .map(|case| escape(&case.label))
                        .collect::<Vec<_>>()
                        .join(" / ")
                )
            }
        }
        WorkflowNodeKind::Action { action } => format!("Action: {}", debug_label(action)),
        WorkflowNodeKind::End { label } => escape(label.as_deref().unwrap_or("End")),
    }
}

fn mermaid_id(id: &str) -> String {
    let mut result = String::from("node_");
    for character in id.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            result.push(character);
        } else {
            result.push('_');
        }
    }
    result
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\n', " ")
}

fn debug_label(value: &impl std::fmt::Debug) -> String {
    escape(&format!("{value:?}"))
        .replace("Workflow", "")
        .replace("ListWorkflowRole", "role")
        .replace("CardMovedTo", "moved to ")
        .replace("{ ", "")
        .replace(" }", "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ListWorkflowRole, WorkflowEdge, WorkflowNode, WorkflowNodeKind, WorkflowTrigger,
    };

    #[test]
    fn serializes_sorted_nodes_edges_and_escaped_labels() {
        let definition = WorkflowDefinition {
            nodes: vec![
                WorkflowNode {
                    id: "a node".to_string(),
                    kind: WorkflowNodeKind::End {
                        label: Some("Done \"safely\"".to_string()),
                    },
                    position: Default::default(),
                },
                WorkflowNode {
                    id: "start".to_string(),
                    kind: WorkflowNodeKind::Trigger {
                        trigger: WorkflowTrigger::CardMovedToRole {
                            role: ListWorkflowRole::Done,
                        },
                    },
                    position: Default::default(),
                },
            ],
            edges: vec![WorkflowEdge {
                id: "edge".to_string(),
                from: "start".to_string(),
                to: "a node".to_string(),
                kind: WorkflowEdgeKind::Case("Ready & safe".to_string()),
            }],
            ..Default::default()
        };
        let output = to_mermaid(&definition);
        assert!(output.contains("node_a_node((\"Done &quot;safely&quot;\"))"));
        assert!(output.contains("node_start -->|Ready &amp; safe| node_a_node"));
        assert!(
            output
                .find("node_a_node")
                .is_some_and(|end| end < output.find("node_start").unwrap_or(usize::MAX))
        );
    }
}
