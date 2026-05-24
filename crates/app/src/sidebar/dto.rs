use gpui::SharedString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveItem {
    Board(u32),
    Note(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectDTO {
    pub(crate) id: u32,
    pub(crate) name: SharedString,
    pub(crate) is_expanded: bool,
    pub(crate) boards: Vec<BoardDTO>,
    pub(crate) notes: Vec<NoteDTO>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoardDTO {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) project_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteDTO {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) project_id: Option<u32>,
}
