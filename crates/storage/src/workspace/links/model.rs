use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceItemKind {
    Note,
    Board,
    List,
    Card,
}

impl WorkspaceItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Board => "board",
            Self::List => "list",
            Self::Card => "card",
        }
    }

    fn prefixed_target(self, id: i64) -> String {
        format!("{}:{id}", self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct WorkspaceItemRef {
    pub kind: WorkspaceItemKind,
    pub id: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceLinkOrigin {
    Manual,
    Wikilink,
    Embed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceCatalogEntry {
    pub item: WorkspaceItemRef,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub board_id: Option<i64>,
    pub board_title: Option<String>,
    pub list_id: Option<i64>,
    pub list_title: Option<String>,
}

impl WorkspaceCatalogEntry {
    pub fn breadcrumb(&self) -> String {
        match self.item.kind {
            WorkspaceItemKind::Note => self
                .project_name
                .as_ref()
                .map(|project| format!("{project} / {}", self.title))
                .unwrap_or_else(|| self.title.clone()),
            WorkspaceItemKind::Board => self.title.clone(),
            WorkspaceItemKind::List => format!(
                "{} / {}",
                self.board_title.as_deref().unwrap_or("Unavailable board"),
                self.title
            ),
            WorkspaceItemKind::Card => format!(
                "{} / {} / {}",
                self.board_title.as_deref().unwrap_or("Unavailable board"),
                self.list_title.as_deref().unwrap_or("Unavailable list"),
                self.title
            ),
        }
    }

    pub fn stable_link(&self) -> String {
        stable_workspace_link(self.item, &self.title)
    }
}

pub fn stable_workspace_link(item: WorkspaceItemRef, title: &str) -> String {
    let display = title.replace(['\r', '\n', '|', '[', ']'], " ");
    format!("[[{}|{display}]]", item.kind.prefixed_target(item.id))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelatedNote {
    pub note_id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub origins: Vec<WorkspaceLinkOrigin>,
}

impl RelatedNote {
    pub fn manually_linked(&self) -> bool {
        self.origins.contains(&WorkspaceLinkOrigin::Manual)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceLinkReference {
    pub item: WorkspaceCatalogEntry,
    pub origin: WorkspaceLinkOrigin,
    pub source_offset: Option<usize>,
    pub line_number: Option<usize>,
    pub inbound: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NoteWorkspaceLinks {
    pub references: Vec<WorkspaceLinkReference>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreatedLinkedCard {
    pub entry_id: i64,
    pub board_id: i64,
    pub list_id: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManualLinkUpdate {
    pub related_notes: Vec<RelatedNote>,
    pub changed: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceLinkRepairBatch {
    pub indexed_notes: usize,
    pub indexed_workspace_notes: usize,
    pub indexed_entries: usize,
    pub has_more: bool,
}
