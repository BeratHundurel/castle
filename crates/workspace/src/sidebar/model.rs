use gpui_kit::SharedString;

use crate::DocumentKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveItem {
    Board(u32),
    Note(u32),
}

pub(crate) struct ProjectNode {
    pub(crate) id: u32,
    pub(crate) name: SharedString,
    pub(crate) position: i32,
    pub(crate) is_expanded: bool,
    pub(crate) boards: Vec<BoardItem>,
    pub(crate) notes: Vec<NoteItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoardItem {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) project_id: Option<u32>,
    pub(crate) is_pinned: bool,
    pub(crate) last_opened_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteItem {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) project_id: Option<u32>,
    pub(crate) kind: DocumentKind,
    pub(crate) is_pinned: bool,
    pub(crate) last_opened_at: Option<i64>,
}
