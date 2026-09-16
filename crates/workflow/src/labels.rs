use super::*;

pub(super) fn node_detail_label(kind: &WorkflowNodeKind) -> String {
    match kind {
        WorkflowNodeKind::Trigger { trigger } => trigger_label(trigger),
        WorkflowNodeKind::Condition { condition } => condition_label(condition),
        WorkflowNodeKind::Action { action } => action_label(action),
        WorkflowNodeKind::Branch { cases } => format!("Choose between {} outcomes", cases.len()),
        WorkflowNodeKind::End { label } => label
            .clone()
            .unwrap_or_else(|| "Finish this workflow".into()),
    }
}

fn trigger_label(trigger: &WorkflowTrigger) -> String {
    match trigger {
        WorkflowTrigger::CardCreated => "An item is created".into(),
        WorkflowTrigger::CardMovedToList { list_id } => format!("An item moves to list {list_id}"),
        WorkflowTrigger::CardMovedFromList { list_id } => format!("An item leaves list {list_id}"),
        WorkflowTrigger::CardMovedToRole { role } => format!("An item moves to {}", role.label()),
        WorkflowTrigger::CardMovedFromRole { role } => format!("An item leaves {}", role.label()),
        WorkflowTrigger::CardCompleted => "An item is completed".into(),
        WorkflowTrigger::CardCancelled => "An item is cancelled".into(),
        WorkflowTrigger::CardReopened => "An item is reopened".into(),
        WorkflowTrigger::ChecklistAllCompleted => "All checklist items are checked".into(),
        WorkflowTrigger::ChecklistItemChecked => "A checklist item is checked".into(),
        WorkflowTrigger::LabelAdded { label } => format!("Label “{label}” is added"),
        WorkflowTrigger::LabelRemoved { label } => format!("Label “{label}” is removed"),
        WorkflowTrigger::PropertyChanged { key } => format!("Property “{key}” changes"),
        WorkflowTrigger::DueDateStatus { status } => {
            format!("An item's due date is {}", due_label(*status))
        }
        WorkflowTrigger::Scheduled { schedule_key } => format!("Schedule “{schedule_key}” runs"),
        WorkflowTrigger::Manual => "Run manually".into(),
    }
}

fn condition_label(condition: &WorkflowCondition) -> String {
    match condition {
        WorkflowCondition::All { conditions } => {
            format!("All {} conditions match", conditions.len())
        }
        WorkflowCondition::Any { conditions } => {
            format!("Any of {} conditions match", conditions.len())
        }
        WorkflowCondition::Not { .. } => "The condition does not match".into(),
        WorkflowCondition::Always => "Always continue".into(),
        WorkflowCondition::ListIs { list_id } => format!("Item is in list {list_id}"),
        WorkflowCondition::PreviousListIs { list_id } => format!("Item was in list {list_id}"),
        WorkflowCondition::ListRoleIs { role } => format!("Item is in a {} list", role.label()),
        WorkflowCondition::PreviousListRoleIs { role } => {
            format!("Item was in a {} list", role.label())
        }
        WorkflowCondition::LabelContains { label } => format!("Item has label “{label}”"),
        WorkflowCondition::CompletionIs { state } => format!(
            "Item is {}",
            match state {
                CompletionState::Open => "open",
                CompletionState::Completed => "completed",
                CompletionState::Cancelled => "cancelled",
                CompletionState::Archived => "archived",
                CompletionState::Trashed => "in trash",
            }
        ),
        WorkflowCondition::DueDateIs { status } => format!("Due date is {}", due_label(*status)),
        WorkflowCondition::ChecklistIs { state } => match state {
            ChecklistState::Empty => "Checklist is empty",
            ChecklistState::NoneChecked => "No checklist items are checked",
            ChecklistState::PartiallyChecked => "Some checklist items are checked",
            ChecklistState::AllChecked => "All checklist items are checked",
        }
        .into(),
        WorkflowCondition::PropertyEquals { key, value } => {
            format!("Property “{key}” equals {}", property_label(value))
        }
        WorkflowCondition::EventOriginIs { origin } => format!(
            "Change comes from {}",
            match origin {
                EventOrigin::User => "a user",
                EventOrigin::Agent => "an agent",
                EventOrigin::Automation => "an automation",
                EventOrigin::Scheduler => "a schedule",
            }
        ),
        WorkflowCondition::EventIs { name } => {
            format!("Event is {}", name.as_str().replace('_', " "))
        }
    }
}

fn action_label(action: &WorkflowAction) -> String {
    match action {
        WorkflowAction::MoveToList { list_id, .. } => format!("Move item to list {list_id}"),
        WorkflowAction::MarkComplete => "Mark item complete".into(),
        WorkflowAction::MarkCancelled => "Cancel item".into(),
        WorkflowAction::Reopen => "Reopen item".into(),
        WorkflowAction::SetDueDate { due_on } => due_on
            .as_ref()
            .map(|date| format!("Set due date to {date}"))
            .unwrap_or_else(|| "Remove due date".into()),
        WorkflowAction::SetStartDate { start_on } => start_on
            .as_ref()
            .map(|date| format!("Set start date to {date}"))
            .unwrap_or_else(|| "Remove start date".into()),
        WorkflowAction::AddLabel { label } => format!("Add label “{label}”"),
        WorkflowAction::RemoveLabel { label } => format!("Remove label “{label}”"),
        WorkflowAction::SetProperty { key, value } => {
            format!("Set “{key}” to {}", property_label(value))
        }
        WorkflowAction::ClearProperty { key } => format!("Clear property “{key}”"),
        WorkflowAction::Archive => "Archive item".into(),
        WorkflowAction::Trash => "Move item to trash".into(),
        WorkflowAction::CreateNextRecurringInstance => "Create the next recurring item".into(),
        WorkflowAction::Notify { message } => format!("Notify: {message}"),
        WorkflowAction::AddHistory { message } => format!("Record: {message}"),
    }
}

fn due_label(status: DueDateStatus) -> &'static str {
    match status {
        DueDateStatus::NoDate => "not set",
        DueDateStatus::Overdue => "overdue",
        DueDateStatus::Today => "today",
        DueDateStatus::Upcoming => "upcoming",
    }
}
fn property_label(value: &WorkflowPropertyValue) -> String {
    match value {
        WorkflowPropertyValue::Text(value) | WorkflowPropertyValue::Date(value) => value.clone(),
        WorkflowPropertyValue::Number(value) => value.to_string(),
        WorkflowPropertyValue::Boolean(value) => value.to_string(),
    }
}

pub(super) fn connection_label(edge: &WorkflowEdge, nodes: &[WorkflowNode]) -> String {
    match &edge.kind {
        WorkflowEdgeKind::Default => "Next".into(),
        WorkflowEdgeKind::True => "Yes".into(),
        WorkflowEdgeKind::False => "No".into(),
        WorkflowEdgeKind::Otherwise => "Otherwise".into(),
        WorkflowEdgeKind::Case(id) => nodes
            .iter()
            .find(|node| node.id == edge.from)
            .and_then(|node| match &node.kind {
                WorkflowNodeKind::Branch { cases } => cases
                    .iter()
                    .find(|case| case.id == *id)
                    .map(|case| case.label.clone()),
                _ => None,
            })
            .unwrap_or_else(|| id.clone()),
    }
}

impl WorkflowWorkspace {
    pub(super) fn step_label(&self, kind: &WorkflowNodeKind) -> String {
        let list_name = |id: i64| {
            self.lists
                .iter()
                .find(|list| i64::from(list.id) == id)
                .map(|list| list.title.clone())
                .unwrap_or_else(|| format!("Missing list ({id})"))
        };
        match kind {
            WorkflowNodeKind::Trigger {
                trigger: WorkflowTrigger::CardMovedToList { list_id },
            } => format!("An item moves to {}", list_name(*list_id)),
            WorkflowNodeKind::Trigger {
                trigger: WorkflowTrigger::CardMovedFromList { list_id },
            } => format!("An item leaves {}", list_name(*list_id)),
            WorkflowNodeKind::Condition {
                condition: WorkflowCondition::ListIs { list_id },
            } => format!("Item is in {}", list_name(*list_id)),
            WorkflowNodeKind::Condition {
                condition: WorkflowCondition::PreviousListIs { list_id },
            } => format!("Item was in {}", list_name(*list_id)),
            WorkflowNodeKind::Action {
                action: WorkflowAction::MoveToList { list_id, .. },
            } => format!("Move item to {}", list_name(*list_id)),
            _ => node_detail_label(kind),
        }
    }
}
