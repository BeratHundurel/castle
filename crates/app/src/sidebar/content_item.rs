use gpui::{Action, Context, Entity, SharedString};
use gpui_component::{IconName, input::InputState};

use super::{SidebarView, action::*, dto::*};

#[derive(Clone)]
pub(super) enum SidebarContentItem {
    Board {
        id: u32,
        title: SharedString,
        project_id: Option<u32>,
    },
    Note {
        id: u32,
        title: SharedString,
        project_id: Option<u32>,
    },
}

impl From<&BoardDTO> for SidebarContentItem {
    fn from(board: &BoardDTO) -> Self {
        Self::Board {
            id: board.id,
            title: board.title.clone(),
            project_id: board.project_id,
        }
    }
}

impl From<&NoteDTO> for SidebarContentItem {
    fn from(note: &NoteDTO) -> Self {
        Self::Note {
            id: note.id,
            title: note.title.clone(),
            project_id: note.project_id,
        }
    }
}

impl SidebarContentItem {
    pub(super) fn title(&self) -> SharedString {
        match self {
            Self::Board { title, .. } | Self::Note { title, .. } => title.clone(),
        }
    }

    pub(super) fn project_id(&self) -> Option<u32> {
        match self {
            Self::Board { project_id, .. } | Self::Note { project_id, .. } => *project_id,
        }
    }

    pub(super) fn icon(&self) -> IconName {
        match self {
            Self::Board { .. } => IconName::LayoutDashboard,
            Self::Note { .. } => IconName::BookOpen,
        }
    }

    pub(super) fn active_item(&self) -> ActiveItem {
        match self {
            Self::Board { id, .. } => ActiveItem::Board(*id),
            Self::Note { id, .. } => ActiveItem::Note(*id),
        }
    }

    pub(super) fn is_renaming(&self, sidebar: &SidebarView) -> bool {
        match self {
            Self::Board { id, .. } => sidebar.renaming_board == Some(*id),
            Self::Note { id, .. } => sidebar.renaming_note == Some(*id),
        }
    }

    pub(super) fn rename_input(&self, sidebar: &SidebarView) -> Entity<InputState> {
        match self {
            Self::Board { .. } => sidebar.rename_board_input.clone(),
            Self::Note { .. } => sidebar.rename_note_input.clone(),
        }
    }

    pub(super) fn edit_action(&self) -> Box<dyn Action> {
        match self {
            Self::Board { id, .. } => Box::new(EditBoardAction(*id)),
            Self::Note { id, .. } => Box::new(EditNoteAction(*id)),
        }
    }

    pub(super) fn move_action(&self, project_id: Option<u32>) -> Box<dyn Action> {
        match self {
            Self::Board { id, .. } => Box::new(MoveBoardAction {
                board_id: *id,
                project_id,
            }),
            Self::Note { id, .. } => Box::new(MoveNoteAction {
                note_id: *id,
                project_id,
            }),
        }
    }

    pub(super) fn delete_action(&self) -> Box<dyn Action> {
        match self {
            Self::Board { id, .. } => Box::new(DeleteBoardAction(*id)),
            Self::Note { id, .. } => Box::new(DeleteNoteAction(*id)),
        }
    }

    pub(super) fn select(&self, sidebar: &mut SidebarView, cx: &mut Context<SidebarView>) {
        match self {
            Self::Board {
                id,
                title,
                project_id,
            } => sidebar.select_board(*id, *project_id, title.clone(), cx),
            Self::Note {
                id,
                title,
                project_id,
            } => sidebar.select_note(*id, *project_id, title.clone(), cx),
        }
    }
}
