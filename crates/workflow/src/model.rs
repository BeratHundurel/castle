use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListWorkflowRole {
    #[default]
    Neutral,
    Done,
    Cancelled,
}

impl ListWorkflowRole {
    pub const ALL: [Self; 3] = [Self::Neutral, Self::Done, Self::Cancelled];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Neutral => "Neutral",
            Self::Done => "Done",
            Self::Cancelled => "Cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "neutral" => Some(Self::Neutral),
            "done" => Some(Self::Done),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn from_storage(value: &str) -> Self {
        Self::parse(value).unwrap_or(Self::Neutral)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventOrigin {
    User,
    Agent,
    Automation,
    Scheduler,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DueDateStatus {
    #[default]
    NoDate,
    Overdue,
    Today,
    Upcoming,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecklistState {
    Empty,
    NoneChecked,
    PartiallyChecked,
    AllChecked,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionState {
    #[default]
    Open,
    Completed,
    Cancelled,
    Archived,
    Trashed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowPropertyValue {
    Text(String),
    Number(i64),
    Boolean(bool),
    Date(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowEventKind {
    CardCreated {
        list_id: i64,
        list_role: ListWorkflowRole,
    },
    CardMoved {
        from_list_id: Option<i64>,
        from_list_role: Option<ListWorkflowRole>,
        to_list_id: i64,
        to_list_role: ListWorkflowRole,
    },
    CardCompleted,
    CardCancelled,
    CardReopened,
    ChecklistChanged {
        checked_count: u32,
        total_count: u32,
    },
    LabelChanged {
        label: String,
        added: bool,
    },
    PropertyChanged {
        key: String,
    },
    DueDateChanged {
        status: DueDateStatus,
    },
    Scheduled {
        schedule_key: String,
    },
    Manual,
}

impl WorkflowEventKind {
    pub const fn name(&self) -> WorkflowEventName {
        match self {
            Self::CardCreated { .. } => WorkflowEventName::CardCreated,
            Self::CardMoved { .. } => WorkflowEventName::CardMoved,
            Self::CardCompleted => WorkflowEventName::CardCompleted,
            Self::CardCancelled => WorkflowEventName::CardCancelled,
            Self::CardReopened => WorkflowEventName::CardReopened,
            Self::ChecklistChanged { .. } => WorkflowEventName::ChecklistChanged,
            Self::LabelChanged { .. } => WorkflowEventName::LabelChanged,
            Self::PropertyChanged { .. } => WorkflowEventName::PropertyChanged,
            Self::DueDateChanged { .. } => WorkflowEventName::DueDateChanged,
            Self::Scheduled { .. } => WorkflowEventName::Scheduled,
            Self::Manual => WorkflowEventName::Manual,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEventName {
    CardCreated,
    CardMoved,
    CardCompleted,
    CardCancelled,
    CardReopened,
    ChecklistChanged,
    LabelChanged,
    PropertyChanged,
    DueDateChanged,
    Scheduled,
    Manual,
}

impl WorkflowEventName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CardCreated => "card_created",
            Self::CardMoved => "card_moved",
            Self::CardCompleted => "card_completed",
            Self::CardCancelled => "card_cancelled",
            Self::CardReopened => "card_reopened",
            Self::ChecklistChanged => "checklist_changed",
            Self::LabelChanged => "label_changed",
            Self::PropertyChanged => "property_changed",
            Self::DueDateChanged => "due_date_changed",
            Self::Scheduled => "scheduled",
            Self::Manual => "manual",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEvent {
    pub event_id: String,
    pub board_id: i64,
    pub entry_id: i64,
    pub occurred_at: String,
    pub origin: EventOrigin,
    pub kind: WorkflowEventKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTrigger {
    CardCreated,
    CardMovedToList { list_id: i64 },
    CardMovedFromList { list_id: i64 },
    CardMovedToRole { role: ListWorkflowRole },
    CardMovedFromRole { role: ListWorkflowRole },
    CardCompleted,
    CardCancelled,
    CardReopened,
    ChecklistAllCompleted,
    ChecklistItemChecked,
    LabelAdded { label: String },
    LabelRemoved { label: String },
    PropertyChanged { key: String },
    DueDateStatus { status: DueDateStatus },
    Scheduled { schedule_key: String },
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowCondition {
    All {
        conditions: Vec<WorkflowCondition>,
    },
    Any {
        conditions: Vec<WorkflowCondition>,
    },
    Not {
        condition: Box<WorkflowCondition>,
    },
    Always,
    ListIs {
        list_id: i64,
    },
    ListRoleIs {
        role: ListWorkflowRole,
    },
    PreviousListIs {
        list_id: i64,
    },
    PreviousListRoleIs {
        role: ListWorkflowRole,
    },
    LabelContains {
        label: String,
    },
    CompletionIs {
        state: CompletionState,
    },
    DueDateIs {
        status: DueDateStatus,
    },
    ChecklistIs {
        state: ChecklistState,
    },
    PropertyEquals {
        key: String,
        value: WorkflowPropertyValue,
    },
    EventOriginIs {
        origin: EventOrigin,
    },
    EventIs {
        name: WorkflowEventName,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovePosition {
    Top,
    #[default]
    Bottom,
    At(i32),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowAction {
    MoveToList {
        list_id: i64,
        #[serde(default)]
        position: MovePosition,
    },
    MarkComplete,
    MarkCancelled,
    Reopen,
    SetDueDate {
        due_on: Option<String>,
    },
    SetStartDate {
        start_on: Option<String>,
    },
    AddLabel {
        label: String,
    },
    RemoveLabel {
        label: String,
    },
    SetProperty {
        key: String,
        value: WorkflowPropertyValue,
    },
    ClearProperty {
        key: String,
    },
    Archive,
    Trash,
    CreateNextRecurringInstance,
    Notify {
        message: String,
    },
    AddHistory {
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowBranchCase {
    pub id: String,
    pub label: String,
    pub condition: WorkflowCondition,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub kind: WorkflowNodeKind,
    #[serde(default)]
    pub position: GraphPosition,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowNodeKind {
    Trigger { trigger: WorkflowTrigger },
    Condition { condition: WorkflowCondition },
    Branch { cases: Vec<WorkflowBranchCase> },
    Action { action: WorkflowAction },
    End { label: Option<String> },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEdgeKind {
    #[default]
    Default,
    True,
    False,
    Case(String),
    Otherwise,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub kind: WorkflowEdgeKind,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
}

impl WorkflowDefinition {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            name: name.into(),
            enabled: false,
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowContext {
    pub board_id: i64,
    pub entry_id: i64,
    pub list_id: i64,
    pub list_role: ListWorkflowRole,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub checklist_checked: u32,
    #[serde(default)]
    pub checklist_total: u32,
    #[serde(default)]
    pub completion_state: CompletionState,
    #[serde(default)]
    pub due_date_status: DueDateStatus,
    #[serde(default)]
    pub properties: BTreeMap<String, WorkflowPropertyValue>,
}

impl WorkflowContext {
    pub const fn checklist_state(&self) -> ChecklistState {
        if self.checklist_total == 0 {
            ChecklistState::Empty
        } else if self.checklist_checked == 0 {
            ChecklistState::NoneChecked
        } else if self.checklist_checked >= self.checklist_total {
            ChecklistState::AllChecked
        } else {
            ChecklistState::PartiallyChecked
        }
    }

    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|candidate| candidate == label)
    }
}

fn default_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

fn default_enabled() -> bool {
    true
}
