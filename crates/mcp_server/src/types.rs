use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct ToolResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ToolResponse<T> {
    pub(crate) fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub(crate) fn error(error: impl ToString) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error.to_string()),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct EmptyInput {}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateProjectInput {
    #[schemars(description = "Human-readable project name")]
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateBoardInput {
    #[schemars(description = "Board title")]
    pub title: String,
    #[schemars(description = "Parent project ID; omit for a standalone board")]
    pub project_id: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateListInput {
    pub board_id: i64,
    #[schemars(description = "Kanban list name, for example Todo, Doing, or Done")]
    pub title: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateTodoInput {
    #[schemars(description = "ID of the list that will contain the todo")]
    pub list_id: i64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[schemars(description = "Optional due date in YYYY-MM-DD format")]
    pub due_on: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ProjectBoardsInput {
    #[schemars(description = "Filter by project ID; omit to include every active board")]
    pub project_id: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct BoardInput {
    pub board_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct TodoInput {
    pub todo_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SearchTodosInput {
    #[schemars(description = "Case-insensitive text matched against todo titles and descriptions")]
    pub query: String,
    pub project_id: Option<i64>,
    pub board_id: Option<i64>,
    #[schemars(description = "Maximum results, from 1 to 100; defaults to 25")]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateTodoInput {
    pub todo_id: i64,
    #[schemars(description = "Replacement title; omit to keep the current title")]
    pub title: Option<String>,
    #[schemars(description = "Replacement description; omit to keep the current description")]
    pub description: Option<String>,
    #[schemars(description = "Replacement due date in YYYY-MM-DD format")]
    pub due_on: Option<String>,
    #[serde(default)]
    #[schemars(description = "Set true to remove the todo's due date")]
    pub clear_due_on: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct MoveTodoInput {
    pub todo_id: i64,
    #[schemars(description = "Destination list ID, commonly a Doing or Done list")]
    pub list_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ProjectNotesInput {
    #[schemars(description = "Filter by project ID; omit to include every active note")]
    pub project_id: Option<i64>,
    #[schemars(description = "Maximum results, from 1 to 100; defaults to 50")]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct NoteInput {
    pub note_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SearchNotesInput {
    #[schemars(description = "Case-insensitive text matched against note titles and content")]
    pub query: String,
    pub project_id: Option<i64>,
    #[schemars(description = "Maximum results, from 1 to 100; defaults to 25")]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateNoteInput {
    pub title: String,
    #[serde(default)]
    #[schemars(description = "Initial Markdown or plain-text content")]
    pub content: String,
    #[schemars(description = "Parent project ID; omit for a standalone note")]
    pub project_id: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateNoteInput {
    pub note_id: i64,
    #[schemars(description = "Replacement title; omit to keep the current title")]
    pub title: Option<String>,
    #[schemars(description = "Replacement content; omit to keep the current content")]
    pub content: Option<String>,
    pub is_pinned: Option<bool>,
    #[schemars(description = "Reject the update if the note changed since this updated_at value")]
    pub expected_updated_at: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct MoveNoteInput {
    pub note_id: i64,
    #[schemars(description = "Destination project ID; omit to make the note standalone")]
    pub project_id: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RenameProjectInput {
    pub project_id: i64,
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RenameBoardInput {
    pub board_id: i64,
    pub title: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RenameListInput {
    pub list_id: i64,
    pub title: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SetTodoReminderInput {
    pub todo_id: i64,
    pub enabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AddChecklistItemInput {
    pub todo_id: i64,
    pub title: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateChecklistItemInput {
    pub item_id: i64,
    pub title: Option<String>,
    pub checked: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateBoardLabelInput {
    pub board_id: i64,
    pub name: String,
    #[schemars(description = "Castle label color name, for example blue, green, red, or yellow")]
    pub color: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SetTodoLabelInput {
    pub todo_id: i64,
    pub label_id: i64,
    pub assigned: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ProjectSummary {
    pub id: i64,
    pub name: String,
    pub position: i32,
    pub board_count: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct BoardSummary {
    pub id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ListDetail {
    pub id: i64,
    pub title: String,
    pub position: i32,
    pub todos: Vec<TodoDetail>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct BoardDetail {
    pub id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub labels: Vec<LabelDetail>,
    pub lists: Vec<ListDetail>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct TodoDetail {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub due_on: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct NoteSummary {
    pub id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub is_pinned: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct NoteDetail {
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
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ChecklistItemDetail {
    pub id: i64,
    pub title: String,
    pub checked: bool,
    pub position: i32,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct LabelDetail {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct AttachmentDetail {
    pub id: i64,
    pub file_name: String,
}
