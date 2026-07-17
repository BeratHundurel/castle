use gpui::{Action, Context, Entity, SharedString};
use gpui_component::{IconName, input::InputState};

use super::{SidebarView, action::*, dto::*};
use crate::document_editor::DocumentKind;

#[derive(Clone)]
pub(super) enum SidebarContentItem {
    Board {
        id: u32,
        title: SharedString,
        project_id: Option<u32>,
        is_pinned: bool,
    },
    Note {
        id: u32,
        title: SharedString,
        project_id: Option<u32>,
        kind: DocumentKind,
        is_pinned: bool,
    },
}

impl From<&BoardDTO> for SidebarContentItem {
    fn from(board: &BoardDTO) -> Self {
        Self::Board {
            id: board.id,
            title: board.title.clone(),
            project_id: board.project_id,
            is_pinned: board.is_pinned,
        }
    }
}

impl From<&NoteDTO> for SidebarContentItem {
    fn from(note: &NoteDTO) -> Self {
        Self::Note {
            id: note.id,
            title: note.title.clone(),
            project_id: note.project_id,
            kind: note.kind,
            is_pinned: note.is_pinned,
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

    pub(super) fn can_move_to(&self, project_id: Option<u32>) -> bool {
        self.project_id() != project_id
    }

    pub(super) fn icon(&self) -> IconName {
        match self {
            Self::Board { .. } => IconName::LayoutDashboard,
            Self::Note { kind, .. } => match kind {
                DocumentKind::Markdown => IconName::BookOpen,
                DocumentKind::Json => IconName::SquareTerminal,
                DocumentKind::PlainText => IconName::File,
            },
        }
    }

    pub(super) fn kind_label(&self) -> &'static str {
        match self {
            Self::Board { .. } => "Board",
            Self::Note { .. } => "Note",
        }
    }

    pub(super) fn move_to(
        &self,
        sidebar: &mut SidebarView,
        project_id: Option<u32>,
        cx: &mut Context<SidebarView>,
    ) {
        match self {
            Self::Board { id, .. } => sidebar.move_board(cx, *id, project_id),
            Self::Note { id, .. } => sidebar.move_note(cx, *id, project_id),
        }
    }

    pub(super) fn is_pinned(&self) -> bool {
        match self {
            Self::Board { is_pinned, .. } | Self::Note { is_pinned, .. } => *is_pinned,
        }
    }

    pub(super) fn pin_action(&self) -> Box<dyn Action> {
        match self {
            Self::Board { id, is_pinned, .. } => Box::new(ToggleBoardPinnedAction {
                board_id: *id,
                pinned: !*is_pinned,
            }),
            Self::Note { id, is_pinned, .. } => Box::new(ToggleNotePinnedAction {
                note_id: *id,
                pinned: !*is_pinned,
            }),
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
                ..
            } => sidebar.select_board(*id, *project_id, title.clone(), cx),
            Self::Note {
                id,
                title,
                project_id,
                ..
            } => sidebar.select_note(*id, *project_id, title.clone(), cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SidebarContentItem;
    use crate::document_editor::DocumentKind;
    use gpui_component::{IconName, IconNamed as _};

    #[test]
    fn move_targets_exclude_the_current_location() {
        let standalone_note = SidebarContentItem::Note {
            id: 1,
            title: "Standalone note".into(),
            project_id: None,
            kind: DocumentKind::Markdown,
            is_pinned: false,
        };
        let project_board = SidebarContentItem::Board {
            id: 2,
            title: "Project board".into(),
            project_id: Some(10),
            is_pinned: false,
        };

        assert!(!standalone_note.can_move_to(None));
        assert!(standalone_note.can_move_to(Some(10)));
        assert!(project_board.can_move_to(None));
        assert!(!project_board.can_move_to(Some(10)));
        assert!(project_board.can_move_to(Some(11)));
    }

    #[test]
    fn note_icons_reflect_the_document_kind() {
        let note = |kind| SidebarContentItem::Note {
            id: 1,
            title: "Document".into(),
            project_id: None,
            kind,
            is_pinned: false,
        };

        assert_eq!(
            note(DocumentKind::Markdown).icon().path(),
            IconName::BookOpen.path()
        );
        assert_eq!(
            note(DocumentKind::Json).icon().path(),
            IconName::SquareTerminal.path()
        );
        assert_eq!(
            note(DocumentKind::PlainText).icon().path(),
            IconName::File.path()
        );
    }
}
