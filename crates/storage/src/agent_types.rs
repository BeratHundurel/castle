#[derive(Debug)]
pub struct CreateProjectInput {
    pub name: String,
}

#[derive(Debug)]
pub struct CreateBoardInput {
    pub title: String,
    pub project_id: Option<i64>,
}

#[derive(Debug)]
pub struct CreateListInput {
    pub board_id: i64,
    pub title: String,
}

#[derive(Debug)]
pub struct CreateEntryInput {
    pub list_id: i64,
    pub title: String,
    pub description: String,
    pub due_on: Option<String>,
}

#[derive(Debug)]
pub struct ProjectBoardsInput {
    pub project_id: Option<i64>,
}

#[derive(Debug)]
pub struct BoardInput {
    pub board_id: i64,
}

#[derive(Debug)]
pub struct EntryInput {
    pub entry_id: i64,
}

#[derive(Debug)]
pub struct SearchEntriesInput {
    pub query: String,
    pub project_id: Option<i64>,
    pub board_id: Option<i64>,
    pub limit: Option<u64>,
}

#[derive(Debug)]
pub struct UpdateEntryInput {
    pub entry_id: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub due_on: Option<String>,
    pub clear_due_on: bool,
}

#[derive(Debug)]
pub struct MoveEntryInput {
    pub entry_id: i64,
    pub list_id: i64,
}

#[derive(Debug)]
pub struct ProjectNotesInput {
    pub project_id: Option<i64>,
    pub limit: Option<u64>,
}

#[derive(Debug)]
pub struct NoteInput {
    pub note_id: i64,
}

#[derive(Debug, Clone, Copy)]
pub enum WorkspaceItemKindInput {
    Board,
    List,
    Card,
}

#[derive(Debug)]
pub struct NoteWorkspaceRelationInput {
    pub note_id: i64,
    pub kind: WorkspaceItemKindInput,
    pub item_id: i64,
    pub board_id: Option<i64>,
    pub list_id: Option<i64>,
}

#[derive(Debug)]
pub struct WorkspaceRelationsInput {
    pub note_id: Option<i64>,
    pub kind: Option<WorkspaceItemKindInput>,
    pub item_id: Option<i64>,
    pub board_id: Option<i64>,
    pub list_id: Option<i64>,
}

#[derive(Debug)]
pub struct SearchNotesInput {
    pub query: String,
    pub project_id: Option<i64>,
    pub limit: Option<u64>,
}

#[derive(Debug)]
pub struct CreateNoteInput {
    pub title: String,
    pub content: String,
    pub project_id: Option<i64>,
}

#[derive(Debug)]
pub struct UpdateNoteInput {
    pub note_id: i64,
    pub title: Option<String>,
    pub content: Option<String>,
    pub is_pinned: Option<bool>,
    pub expected_updated_at: Option<i64>,
}

#[derive(Debug)]
pub struct MoveNoteInput {
    pub note_id: i64,
    pub project_id: Option<i64>,
}

#[derive(Debug)]
pub struct RenameProjectInput {
    pub project_id: i64,
    pub name: String,
}

#[derive(Debug)]
pub struct RenameBoardInput {
    pub board_id: i64,
    pub title: String,
}

#[derive(Debug)]
pub struct RenameListInput {
    pub list_id: i64,
    pub title: String,
}

#[derive(Debug)]
pub struct SetEntryReminderInput {
    pub entry_id: i64,
    pub enabled: bool,
}

#[derive(Debug)]
pub struct AddChecklistItemInput {
    pub entry_id: i64,
    pub title: String,
}

#[derive(Debug)]
pub struct UpdateChecklistItemInput {
    pub item_id: i64,
    pub title: Option<String>,
    pub checked: Option<bool>,
}

#[derive(Debug)]
pub struct CreateBoardLabelInput {
    pub board_id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Debug)]
pub struct SetEntryLabelInput {
    pub entry_id: i64,
    pub label_id: i64,
    pub assigned: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum BoardPropertyKindInput {
    Text,
    Number,
    Checkbox,
    Date,
    Select,
    Url,
}

#[derive(Debug)]
pub struct CreateBoardPropertyInput {
    pub board_id: i64,
    pub name: String,
    pub kind: BoardPropertyKindInput,
}

#[derive(Debug)]
pub struct CreateBoardPropertyOptionInput {
    pub property_id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone)]
pub enum BoardPropertyValueDetail {
    Text(String),
    Number(f64),
    Checkbox(bool),
    Date(String),
    Select(i64),
    Url(String),
}

#[derive(Debug)]
pub struct SetEntryPropertyInput {
    pub entry_id: i64,
    pub property_id: i64,
    pub value: BoardPropertyValueDetail,
}

#[derive(Debug)]
pub struct ClearEntryPropertyInput {
    pub entry_id: i64,
    pub property_id: i64,
}

#[derive(Debug, Clone)]
pub struct ProjectSummary {
    pub id: i64,
    pub name: String,
    pub position: i32,
    pub board_count: u64,
}

#[derive(Debug, Clone)]
pub struct BoardSummary {
    pub id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ListDetail {
    pub id: i64,
    pub title: String,
    pub position: i32,
    pub entries: Vec<EntryDetail>,
    pub related_items: Vec<RelatedItemDetail>,
}

#[derive(Debug, Clone)]
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
pub struct EntryDetail {
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

#[derive(Debug, Clone)]
pub struct NoteSummary {
    pub id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub is_pinned: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
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
pub struct RelatedItemDetail {
    pub kind: String,
    pub id: i64,
    pub title: String,
    pub breadcrumb: String,
    pub stable_link: String,
    pub origins: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NoteLinksDetail {
    pub inbound: Vec<NoteLinkDetail>,
    pub outbound: Vec<NoteLinkDetail>,
    pub unresolved: Vec<NoteLinkDetail>,
}

#[derive(Debug, Clone)]
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
pub struct ChecklistItemDetail {
    pub id: i64,
    pub title: String,
    pub checked: bool,
    pub position: i32,
}

#[derive(Debug, Clone)]
pub struct LabelDetail {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone)]
pub struct AttachmentDetail {
    pub id: i64,
    pub file_name: String,
}

#[derive(Debug, Clone)]
pub struct BoardPropertyOptionDetail {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub position: i32,
}

#[derive(Debug, Clone)]
pub struct BoardPropertyDefinitionDetail {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub kind: String,
    pub position: i32,
    pub options: Vec<BoardPropertyOptionDetail>,
}

#[derive(Debug, Clone)]
pub struct EntryPropertyValueDetail {
    pub entry_id: i64,
    pub property_id: i64,
    pub value: BoardPropertyValueDetail,
}

#[derive(Debug, Clone)]
pub struct BoardPropertiesDetail {
    pub definitions: Vec<BoardPropertyDefinitionDetail>,
    pub values: Vec<EntryPropertyValueDetail>,
}
