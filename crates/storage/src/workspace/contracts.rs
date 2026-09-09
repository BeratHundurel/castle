use crate::board::ListWorkflowRole;

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateProjectInput {
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Human-readable project name")
    )]
    pub name: String,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateBoardInput {
    #[cfg_attr(feature = "schema", schemars(description = "Board title"))]
    pub title: String,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Parent project ID; omit for a standalone board")
    )]
    pub project_id: Option<i64>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateListInput {
    pub board_id: i64,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Name of the list within the board")
    )]
    pub title: String,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateEntryInput {
    #[cfg_attr(
        feature = "schema",
        schemars(description = "ID of the list that will contain the entry")
    )]
    pub list_id: i64,
    pub title: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub description: String,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Optional due date in YYYY-MM-DD format")
    )]
    pub due_on: Option<String>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectBoardsInput {
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Filter by project ID; omit to include every active board")
    )]
    pub project_id: Option<i64>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BoardInput {
    pub board_id: i64,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EntryInput {
    pub entry_id: i64,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SearchEntriesInput {
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "Case-insensitive text matched against board entry titles and descriptions"
        )
    )]
    pub query: String,
    pub project_id: Option<i64>,
    pub board_id: Option<i64>,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Maximum results, from 1 to 100; defaults to 25")
    )]
    pub limit: Option<u64>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UpdateEntryInput {
    pub entry_id: i64,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Replacement title; omit to keep the current title")
    )]
    pub title: Option<String>,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Replacement description; omit to keep the current description")
    )]
    pub description: Option<String>,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Replacement due date in YYYY-MM-DD format")
    )]
    pub due_on: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Set true to remove the entry's due date")
    )]
    pub clear_due_on: bool,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SetEntryScheduleInput {
    pub entry_id: i64,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Replacement start date in YYYY-MM-DD format")
    )]
    pub start_on: Option<String>,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Replacement due date in YYYY-MM-DD format")
    )]
    pub due_on: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Set true to remove the entry's start date")
    )]
    pub clear_start_on: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Set true to remove the entry's due date")
    )]
    pub clear_due_on: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum EntryLifecycleState {
    Open,
    Completed,
    Cancelled,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SetEntryLifecycleInput {
    pub entry_id: i64,
    pub state: EntryLifecycleState,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MoveEntryInput {
    pub entry_id: i64,
    #[cfg_attr(feature = "schema", schemars(description = "Destination list ID"))]
    pub list_id: i64,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectNotesInput {
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Filter by project ID; omit to include every active note")
    )]
    pub project_id: Option<i64>,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Maximum results, from 1 to 100; defaults to 50")
    )]
    pub limit: Option<u64>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NoteInput {
    pub note_id: i64,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum WorkspaceItemKindInput {
    Board,
    List,
    Card,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NoteWorkspaceRelationInput {
    pub note_id: i64,
    pub kind: WorkspaceItemKindInput,
    pub item_id: i64,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Required parent board ID for list and card targets")
    )]
    pub board_id: Option<i64>,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Required parent list ID for card targets")
    )]
    pub list_id: Option<i64>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorkspaceRelationsInput {
    pub note_id: Option<i64>,
    pub kind: Option<WorkspaceItemKindInput>,
    pub item_id: Option<i64>,
    pub board_id: Option<i64>,
    pub list_id: Option<i64>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SearchNotesInput {
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Case-insensitive text matched against note titles and content")
    )]
    pub query: String,
    pub project_id: Option<i64>,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Maximum results, from 1 to 100; defaults to 25")
    )]
    pub limit: Option<u64>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateNoteInput {
    pub title: String,
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Initial Markdown or plain-text content")
    )]
    pub content: String,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Parent project ID; omit for a standalone note")
    )]
    pub project_id: Option<i64>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UpdateNoteInput {
    pub note_id: i64,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Replacement title; omit to keep the current title")
    )]
    pub title: Option<String>,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Replacement content; omit to keep the current content")
    )]
    pub content: Option<String>,
    pub is_pinned: Option<bool>,
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "Reject the update if the note changed since this updated_at value"
        )
    )]
    pub expected_updated_at: Option<i64>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MoveNoteInput {
    pub note_id: i64,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Destination project ID; omit to make the note standalone")
    )]
    pub project_id: Option<i64>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RenameProjectInput {
    pub project_id: i64,
    pub name: String,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RenameBoardInput {
    pub board_id: i64,
    pub title: String,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RenameListInput {
    pub list_id: i64,
    pub title: String,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SetListWorkflowRoleInput {
    pub list_id: i64,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Workflow role: neutral, done, or cancelled")
    )]
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub workflow_role: ListWorkflowRole,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SaveWorkflowInput {
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Existing workflow ID; omit to create a workflow")
    )]
    pub workflow_id: Option<i64>,
    pub board_id: i64,
    #[cfg_attr(feature = "schema", schemars(description = "Workflow display name"))]
    pub name: String,
    pub enabled: bool,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Versioned workflow graph JSON returned by list_board_workflows")
    )]
    pub definition: serde_json::Value,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorkflowInput {
    pub board_id: i64,
    pub workflow_id: i64,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RunWorkflowInput {
    pub board_id: i64,
    pub entry_id: i64,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorkflowRunsInput {
    pub board_id: i64,
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Maximum runs to return, from 1 to 100; defaults to 25")
    )]
    pub limit: Option<u64>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CalendarEntriesInput {
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Optional board ID; omit to include every active board")
    )]
    pub board_id: Option<i64>,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Inclusive range start in YYYY-MM-DD format")
    )]
    pub start_on: Option<String>,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Inclusive range end in YYYY-MM-DD format")
    )]
    pub end_on: Option<String>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RecurringTasksInput {
    pub board_id: i64,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateRecurringTaskInput {
    pub entry_id: i64,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "First occurrence date in YYYY-MM-DD format")
    )]
    pub start_on: String,
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "Rule such as daily, every 2 weeks on mon, wed, or every month on the 1st"
        )
    )]
    pub rule: String,
    pub until_on: Option<String>,
    pub occurrence_limit: Option<u32>,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "on_completion or on_schedule; defaults to on_completion")
    )]
    pub generation_mode: Option<String>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RecurringTaskInput {
    pub entry_id: i64,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SetEntryReminderInput {
    pub entry_id: i64,
    pub enabled: bool,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AddChecklistItemInput {
    pub entry_id: i64,
    pub title: String,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UpdateChecklistItemInput {
    pub item_id: i64,
    pub title: Option<String>,
    pub checked: Option<bool>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateBoardLabelInput {
    pub board_id: i64,
    pub name: String,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Castle label color name, for example blue, green, red, or yellow")
    )]
    pub color: String,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SetEntryLabelInput {
    pub entry_id: i64,
    pub label_id: i64,
    pub assigned: bool,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum BoardPropertyKindInput {
    Text,
    Number,
    Checkbox,
    Date,
    Select,
    Url,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateBoardPropertyInput {
    pub board_id: i64,
    pub name: String,
    pub kind: BoardPropertyKindInput,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateBoardPropertyOptionInput {
    pub property_id: i64,
    pub name: String,
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Presentation color name; Castle does not infer meaning from it")
    )]
    pub color: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", content = "value", rename_all = "snake_case")
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum BoardPropertyValueDetail {
    Text(String),
    Number(f64),
    Checkbox(bool),
    Date(String),
    Select(i64),
    Url(String),
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SetEntryPropertyInput {
    pub entry_id: i64,
    pub property_id: i64,
    pub value: BoardPropertyValueDetail,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClearEntryPropertyInput {
    pub entry_id: i64,
    pub property_id: i64,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectSummary {
    pub id: i64,
    pub name: String,
    pub position: i32,
    pub board_count: u64,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BoardSummary {
    pub id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ListDetail {
    pub id: i64,
    pub title: String,
    pub position: i32,
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub workflow_role: ListWorkflowRole,
    pub entries: Vec<EntryDetail>,
    pub related_items: Vec<RelatedItemDetail>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BoardDetail {
    pub id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub labels: Vec<LabelDetail>,
    pub lists: Vec<ListDetail>,
    pub related_items: Vec<RelatedItemDetail>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EntryDetail {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub start_on: Option<String>,
    pub due_on: Option<String>,
    pub completed_at: Option<i64>,
    pub cancelled_at: Option<i64>,
    pub archived: bool,
    pub reminder_enabled: bool,
    pub position: i32,
    pub list_id: i64,
    pub list_title: String,
    pub board_id: i64,
    pub board_title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub labels: Vec<LabelDetail>,
    pub checklist_items: Vec<ChecklistItemDetail>,
    pub attachments: Vec<AttachmentDetail>,
    pub related_items: Vec<RelatedItemDetail>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NoteSummary {
    pub id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub is_pinned: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NoteDetail {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub file_path: Option<String>,
    pub file_managed_by_app: bool,
    pub file_missing: bool,
    pub is_pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub related_items: Vec<RelatedItemDetail>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RelatedItemDetail {
    pub kind: String,
    pub id: i64,
    pub title: String,
    pub breadcrumb: String,
    pub stable_link: String,
    pub origins: Vec<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NoteLinksDetail {
    pub inbound: Vec<NoteLinkDetail>,
    pub outbound: Vec<NoteLinkDetail>,
    pub unresolved: Vec<NoteLinkDetail>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NoteLinkDetail {
    pub source_note_id: i64,
    pub source_title: String,
    pub source_project_name: Option<String>,
    pub target_note_id: Option<i64>,
    pub target_title: Option<String>,
    pub target_project_name: Option<String>,
    pub target_kind: Option<String>,
    pub raw_target: String,
    pub display_text: Option<String>,
    pub start_byte: usize,
    pub end_byte: usize,
    pub line_number: usize,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ChecklistItemDetail {
    pub id: i64,
    pub title: String,
    pub checked: bool,
    pub position: i32,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LabelDetail {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AttachmentDetail {
    pub id: i64,
    pub file_name: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BoardPropertyOptionDetail {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub position: i32,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BoardPropertyDefinitionDetail {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub kind: String,
    pub position: i32,
    pub options: Vec<BoardPropertyOptionDetail>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EntryPropertyValueDetail {
    pub entry_id: i64,
    pub property_id: i64,
    pub value: BoardPropertyValueDetail,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BoardPropertiesDetail {
    pub definitions: Vec<BoardPropertyDefinitionDetail>,
    pub values: Vec<EntryPropertyValueDetail>,
}
