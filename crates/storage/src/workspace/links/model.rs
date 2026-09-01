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
    pub fn path_segments(&self) -> Vec<String> {
        let mut path = Vec::new();
        if let Some(project_name) = self.project_name.as_ref() {
            path.push(project_name.clone());
        }
        match self.item.kind {
            WorkspaceItemKind::Note | WorkspaceItemKind::Board => path.push(self.title.clone()),
            WorkspaceItemKind::List => {
                path.push(
                    self.board_title
                        .clone()
                        .unwrap_or_else(|| "Unavailable board".to_string()),
                );
                path.push(self.title.clone());
            }
            WorkspaceItemKind::Card => {
                path.push(
                    self.board_title
                        .clone()
                        .unwrap_or_else(|| "Unavailable board".to_string()),
                );
                path.push(
                    self.list_title
                        .clone()
                        .unwrap_or_else(|| "Unavailable list".to_string()),
                );
                path.push(self.title.clone());
            }
        }
        path
    }

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

    pub fn readable_link(&self) -> String {
        let path = self
            .path_segments()
            .into_iter()
            .map(|segment| super::escape_segment(&segment))
            .collect::<Vec<_>>()
            .join(" / ");
        format!("[[{}:{}]]", self.item.kind.as_str(), path)
    }

    pub fn stable_link(&self) -> String {
        self.readable_link()
    }
}

pub fn stable_workspace_link(item: WorkspaceItemRef, title: &str) -> String {
    let display = crate::workspace::links::escape_segment(title.trim());
    format!("[[{}:{display}]]", item.kind.as_str())
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
