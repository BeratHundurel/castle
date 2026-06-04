use entity::{board, note, project};
use gpui::SharedString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveItem {
    Board(u32),
    Note(u32),
}

pub(crate) struct ProjectDTO {
    pub(crate) id: u32,
    pub(crate) name: SharedString,
    pub(crate) position: i32,
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

impl From<project::ModelEx> for ProjectDTO {
    fn from(project: project::ModelEx) -> Self {
        Self {
            id: project.id as u32,
            name: SharedString::from(project.name),
            position: project.position,
            is_expanded: false,
            boards: project.boards.into_iter().map(BoardDTO::from).collect(),
            notes: project.notes.into_iter().map(NoteDTO::from).collect(),
        }
    }
}

impl From<board::Model> for BoardDTO {
    fn from(board: board::Model) -> Self {
        Self {
            id: board.id as u32,
            title: SharedString::from(board.title),
            project_id: board.project_id.map(|id| id as u32),
        }
    }
}

impl From<board::ModelEx> for BoardDTO {
    fn from(board: board::ModelEx) -> Self {
        Self {
            id: board.id as u32,
            title: SharedString::from(board.title),
            project_id: board.project_id.map(|id| id as u32),
        }
    }
}

impl From<note::Model> for NoteDTO {
    fn from(note: note::Model) -> Self {
        Self {
            id: note.id as u32,
            title: SharedString::from(note.title),
            project_id: note.project_id.map(|id| id as u32),
        }
    }
}

impl From<note::ModelEx> for NoteDTO {
    fn from(note: note::ModelEx) -> Self {
        Self {
            id: note.id as u32,
            title: SharedString::from(note.title),
            project_id: note.project_id.map(|id| id as u32),
        }
    }
}
