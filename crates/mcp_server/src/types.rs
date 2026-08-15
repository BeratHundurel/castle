use schemars::JsonSchema;
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
    #[schemars(description = "Name of the list within the board")]
    pub title: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateEntryInput {
    #[schemars(description = "ID of the list that will contain the entry")]
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
pub(crate) struct EntryInput {
    pub entry_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SearchEntriesInput {
    #[schemars(
        description = "Case-insensitive text matched against board entry titles and descriptions"
    )]
    pub query: String,
    pub project_id: Option<i64>,
    pub board_id: Option<i64>,
    #[schemars(description = "Maximum results, from 1 to 100; defaults to 25")]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateEntryInput {
    pub entry_id: i64,
    #[schemars(description = "Replacement title; omit to keep the current title")]
    pub title: Option<String>,
    #[schemars(description = "Replacement description; omit to keep the current description")]
    pub description: Option<String>,
    #[schemars(description = "Replacement due date in YYYY-MM-DD format")]
    pub due_on: Option<String>,
    #[serde(default)]
    #[schemars(description = "Set true to remove the entry's due date")]
    pub clear_due_on: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct MoveEntryInput {
    pub entry_id: i64,
    #[schemars(description = "Destination list ID")]
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

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceItemKindInput {
    Board,
    List,
    Card,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct NoteWorkspaceRelationInput {
    pub note_id: i64,
    pub kind: WorkspaceItemKindInput,
    pub item_id: i64,
    #[schemars(description = "Required parent board ID for list and card targets")]
    pub board_id: Option<i64>,
    #[schemars(description = "Required parent list ID for card targets")]
    pub list_id: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct WorkspaceRelationsInput {
    pub note_id: Option<i64>,
    pub kind: Option<WorkspaceItemKindInput>,
    pub item_id: Option<i64>,
    pub board_id: Option<i64>,
    pub list_id: Option<i64>,
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
pub(crate) struct SetEntryReminderInput {
    pub entry_id: i64,
    pub enabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AddChecklistItemInput {
    pub entry_id: i64,
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
pub(crate) struct SetEntryLabelInput {
    pub entry_id: i64,
    pub label_id: i64,
    pub assigned: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BoardPropertyKindInput {
    Text,
    Number,
    Checkbox,
    Date,
    Select,
    Url,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateBoardPropertyInput {
    pub board_id: i64,
    pub name: String,
    pub kind: BoardPropertyKindInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateBoardPropertyOptionInput {
    pub property_id: i64,
    pub name: String,
    #[schemars(description = "Presentation color name; Castle does not infer meaning from it")]
    pub color: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(crate) enum BoardPropertyValueDetail {
    Text(String),
    Number(f64),
    Checkbox(bool),
    Date(String),
    Select(i64),
    Url(String),
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SetEntryPropertyInput {
    pub entry_id: i64,
    pub property_id: i64,
    pub value: BoardPropertyValueDetail,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ClearEntryPropertyInput {
    pub entry_id: i64,
    pub property_id: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct ProjectSummary {
    pub id: i64,
    pub name: String,
    pub position: i32,
    pub board_count: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct BoardSummary {
    pub id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct ListDetail {
    pub id: i64,
    pub title: String,
    pub position: i32,
    pub entries: Vec<EntryDetail>,
    pub related_items: Vec<RelatedItemDetail>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct BoardDetail {
    pub id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub labels: Vec<LabelDetail>,
    pub lists: Vec<ListDetail>,
    pub related_items: Vec<RelatedItemDetail>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct EntryDetail {
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
    pub related_items: Vec<RelatedItemDetail>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct NoteSummary {
    pub id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub is_pinned: bool,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
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
    pub related_items: Vec<RelatedItemDetail>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct RelatedItemDetail {
    pub kind: String,
    pub id: i64,
    pub title: String,
    pub breadcrumb: String,
    pub stable_link: String,
    pub origins: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct NoteLinksDetail {
    pub inbound: Vec<NoteLinkDetail>,
    pub outbound: Vec<NoteLinkDetail>,
    pub unresolved: Vec<NoteLinkDetail>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct NoteLinkDetail {
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

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct ChecklistItemDetail {
    pub id: i64,
    pub title: String,
    pub checked: bool,
    pub position: i32,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct LabelDetail {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct AttachmentDetail {
    pub id: i64,
    pub file_name: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct BoardPropertyOptionDetail {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub position: i32,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct BoardPropertyDefinitionDetail {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub kind: String,
    pub position: i32,
    pub options: Vec<BoardPropertyOptionDetail>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct EntryPropertyValueDetail {
    pub entry_id: i64,
    pub property_id: i64,
    pub value: BoardPropertyValueDetail,
}

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct BoardPropertiesDetail {
    pub definitions: Vec<BoardPropertyDefinitionDetail>,
    pub values: Vec<EntryPropertyValueDetail>,
}

macro_rules! into_storage {
    ($type:ident { $($field:ident),+ $(,)? }) => {
        impl From<$type> for storage::agent_types::$type {
            fn from(input: $type) -> Self {
                Self {
                    $($field: input.$field),+
                }
            }
        }
    };
}

into_storage!(CreateProjectInput { name });
into_storage!(CreateBoardInput { title, project_id });
into_storage!(CreateListInput { board_id, title });
into_storage!(CreateEntryInput {
    list_id,
    title,
    description,
    due_on
});
into_storage!(SearchEntriesInput {
    query,
    project_id,
    board_id,
    limit
});
into_storage!(UpdateEntryInput {
    entry_id,
    title,
    description,
    due_on,
    clear_due_on
});
into_storage!(MoveEntryInput { entry_id, list_id });
into_storage!(SearchNotesInput {
    query,
    project_id,
    limit
});
into_storage!(CreateNoteInput {
    title,
    content,
    project_id
});
into_storage!(UpdateNoteInput {
    note_id,
    title,
    content,
    is_pinned,
    expected_updated_at
});
into_storage!(MoveNoteInput {
    note_id,
    project_id
});
into_storage!(RenameProjectInput { project_id, name });
into_storage!(RenameBoardInput { board_id, title });
into_storage!(RenameListInput { list_id, title });
into_storage!(SetEntryReminderInput { entry_id, enabled });
into_storage!(AddChecklistItemInput { entry_id, title });
into_storage!(UpdateChecklistItemInput {
    item_id,
    title,
    checked
});
into_storage!(CreateBoardLabelInput {
    board_id,
    name,
    color
});
into_storage!(SetEntryLabelInput {
    entry_id,
    label_id,
    assigned
});
into_storage!(CreateBoardPropertyOptionInput {
    property_id,
    name,
    color
});
into_storage!(ClearEntryPropertyInput {
    entry_id,
    property_id
});

impl From<WorkspaceItemKindInput> for storage::agent_types::WorkspaceItemKindInput {
    fn from(kind: WorkspaceItemKindInput) -> Self {
        match kind {
            WorkspaceItemKindInput::Board => Self::Board,
            WorkspaceItemKindInput::List => Self::List,
            WorkspaceItemKindInput::Card => Self::Card,
        }
    }
}

impl From<NoteWorkspaceRelationInput> for storage::agent_types::NoteWorkspaceRelationInput {
    fn from(input: NoteWorkspaceRelationInput) -> Self {
        Self {
            note_id: input.note_id,
            kind: input.kind.into(),
            item_id: input.item_id,
            board_id: input.board_id,
            list_id: input.list_id,
        }
    }
}

impl From<WorkspaceRelationsInput> for storage::agent_types::WorkspaceRelationsInput {
    fn from(input: WorkspaceRelationsInput) -> Self {
        Self {
            note_id: input.note_id,
            kind: input.kind.map(Into::into),
            item_id: input.item_id,
            board_id: input.board_id,
            list_id: input.list_id,
        }
    }
}

impl From<BoardPropertyKindInput> for storage::agent_types::BoardPropertyKindInput {
    fn from(kind: BoardPropertyKindInput) -> Self {
        match kind {
            BoardPropertyKindInput::Text => Self::Text,
            BoardPropertyKindInput::Number => Self::Number,
            BoardPropertyKindInput::Checkbox => Self::Checkbox,
            BoardPropertyKindInput::Date => Self::Date,
            BoardPropertyKindInput::Select => Self::Select,
            BoardPropertyKindInput::Url => Self::Url,
        }
    }
}

impl From<CreateBoardPropertyInput> for storage::agent_types::CreateBoardPropertyInput {
    fn from(input: CreateBoardPropertyInput) -> Self {
        Self {
            board_id: input.board_id,
            name: input.name,
            kind: input.kind.into(),
        }
    }
}

impl From<BoardPropertyValueDetail> for storage::agent_types::BoardPropertyValueDetail {
    fn from(value: BoardPropertyValueDetail) -> Self {
        match value {
            BoardPropertyValueDetail::Text(value) => Self::Text(value),
            BoardPropertyValueDetail::Number(value) => Self::Number(value),
            BoardPropertyValueDetail::Checkbox(value) => Self::Checkbox(value),
            BoardPropertyValueDetail::Date(value) => Self::Date(value),
            BoardPropertyValueDetail::Select(value) => Self::Select(value),
            BoardPropertyValueDetail::Url(value) => Self::Url(value),
        }
    }
}

impl From<SetEntryPropertyInput> for storage::agent_types::SetEntryPropertyInput {
    fn from(input: SetEntryPropertyInput) -> Self {
        Self {
            entry_id: input.entry_id,
            property_id: input.property_id,
            value: input.value.into(),
        }
    }
}

macro_rules! from_storage {
    ($type:ident { $($field:ident),+ $(,)? }) => {
        impl From<storage::agent_types::$type> for $type {
            fn from(value: storage::agent_types::$type) -> Self {
                Self {
                    $($field: value.$field),+
                }
            }
        }
    };
}

from_storage!(ProjectSummary {
    id,
    name,
    position,
    board_count
});
from_storage!(BoardSummary {
    id,
    title,
    project_id,
    project_name
});
from_storage!(NoteSummary {
    id,
    title,
    project_id,
    project_name,
    is_pinned,
    updated_at
});
from_storage!(RelatedItemDetail {
    kind,
    id,
    title,
    breadcrumb,
    stable_link,
    origins
});
from_storage!(NoteLinkDetail {
    source_note_id,
    source_title,
    source_project_name,
    target_note_id,
    target_title,
    target_project_name,
    target_kind,
    raw_target,
    display_text,
    start_byte,
    end_byte,
    line_number,
});
from_storage!(ChecklistItemDetail {
    id,
    title,
    checked,
    position
});
from_storage!(LabelDetail {
    id,
    board_id,
    name,
    color
});
from_storage!(AttachmentDetail { id, file_name });
from_storage!(BoardPropertyOptionDetail {
    id,
    name,
    color,
    position
});

impl From<storage::agent_types::ListDetail> for ListDetail {
    fn from(value: storage::agent_types::ListDetail) -> Self {
        Self {
            id: value.id,
            title: value.title,
            position: value.position,
            entries: value.entries.into_iter().map(Into::into).collect(),
            related_items: value.related_items.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<storage::agent_types::BoardDetail> for BoardDetail {
    fn from(value: storage::agent_types::BoardDetail) -> Self {
        Self {
            id: value.id,
            title: value.title,
            project_id: value.project_id,
            project_name: value.project_name,
            labels: value.labels.into_iter().map(Into::into).collect(),
            lists: value.lists.into_iter().map(Into::into).collect(),
            related_items: value.related_items.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<storage::agent_types::EntryDetail> for EntryDetail {
    fn from(value: storage::agent_types::EntryDetail) -> Self {
        Self {
            id: value.id,
            title: value.title,
            description: value.description,
            due_on: value.due_on,
            reminder_enabled: value.reminder_enabled,
            position: value.position,
            list_id: value.list_id,
            list_title: value.list_title,
            board_id: value.board_id,
            board_title: value.board_title,
            project_id: value.project_id,
            project_name: value.project_name,
            labels: value.labels.into_iter().map(Into::into).collect(),
            checklist_items: value.checklist_items.into_iter().map(Into::into).collect(),
            attachments: value.attachments.into_iter().map(Into::into).collect(),
            related_items: value.related_items.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<storage::agent_types::NoteDetail> for NoteDetail {
    fn from(value: storage::agent_types::NoteDetail) -> Self {
        Self {
            id: value.id,
            title: value.title,
            content: value.content,
            project_id: value.project_id,
            project_name: value.project_name,
            file_path: value.file_path,
            file_managed_by_app: value.file_managed_by_app,
            file_missing: value.file_missing,
            is_pinned: value.is_pinned,
            created_at: value.created_at,
            updated_at: value.updated_at,
            related_items: value.related_items.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<storage::agent_types::NoteLinksDetail> for NoteLinksDetail {
    fn from(value: storage::agent_types::NoteLinksDetail) -> Self {
        Self {
            inbound: value.inbound.into_iter().map(Into::into).collect(),
            outbound: value.outbound.into_iter().map(Into::into).collect(),
            unresolved: value.unresolved.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<storage::agent_types::BoardPropertyValueDetail> for BoardPropertyValueDetail {
    fn from(value: storage::agent_types::BoardPropertyValueDetail) -> Self {
        match value {
            storage::agent_types::BoardPropertyValueDetail::Text(value) => Self::Text(value),
            storage::agent_types::BoardPropertyValueDetail::Number(value) => Self::Number(value),
            storage::agent_types::BoardPropertyValueDetail::Checkbox(value) => {
                Self::Checkbox(value)
            }
            storage::agent_types::BoardPropertyValueDetail::Date(value) => Self::Date(value),
            storage::agent_types::BoardPropertyValueDetail::Select(value) => Self::Select(value),
            storage::agent_types::BoardPropertyValueDetail::Url(value) => Self::Url(value),
        }
    }
}

impl From<storage::agent_types::BoardPropertyDefinitionDetail> for BoardPropertyDefinitionDetail {
    fn from(value: storage::agent_types::BoardPropertyDefinitionDetail) -> Self {
        Self {
            id: value.id,
            board_id: value.board_id,
            name: value.name,
            kind: value.kind,
            position: value.position,
            options: value.options.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<storage::agent_types::EntryPropertyValueDetail> for EntryPropertyValueDetail {
    fn from(value: storage::agent_types::EntryPropertyValueDetail) -> Self {
        Self {
            entry_id: value.entry_id,
            property_id: value.property_id,
            value: value.value.into(),
        }
    }
}

impl From<storage::agent_types::BoardPropertiesDetail> for BoardPropertiesDetail {
    fn from(value: storage::agent_types::BoardPropertiesDetail) -> Self {
        Self {
            definitions: value.definitions.into_iter().map(Into::into).collect(),
            values: value.values.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_wire_defaults_are_applied_before_storage_conversion() {
        let input: CreateEntryInput = serde_json::from_value(serde_json::json!({
            "list_id": 7,
            "title": "Ship"
        }))
        .expect("MCP create entry input should deserialize");
        let input: storage::agent_types::CreateEntryInput = input.into();

        assert_eq!(input.list_id, 7);
        assert_eq!(input.title, "Ship");
        assert_eq!(input.description, "");
        assert_eq!(input.due_on, None);
    }

    #[test]
    fn tagged_property_values_preserve_the_mcp_json_contract() {
        let value = BoardPropertyValueDetail::Select(42);
        assert_eq!(
            serde_json::to_value(value).expect("property value should serialize"),
            serde_json::json!({ "kind": "select", "value": 42 })
        );
    }

    #[test]
    fn storage_results_are_projected_into_the_wire_envelope() {
        let detail = storage::agent_types::ProjectSummary {
            id: 9,
            name: "Delivery".to_string(),
            position: 2,
            board_count: 3,
        };
        let response = ToolResponse::success(ProjectSummary::from(detail));

        assert_eq!(
            serde_json::to_value(response).expect("tool response should serialize"),
            serde_json::json!({
                "success": true,
                "data": {
                    "id": 9,
                    "name": "Delivery",
                    "position": 2,
                    "board_count": 3
                },
                "error": null
            })
        );
    }
}
