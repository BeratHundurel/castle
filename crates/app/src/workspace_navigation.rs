#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceNavigationTarget {
    Note {
        note_id: u32,
        source_offset: Option<usize>,
    },
    Board {
        board_id: u32,
        list_id: Option<u32>,
        card_id: Option<u32>,
    },
}

impl WorkspaceNavigationTarget {
    pub(crate) fn board(board_id: u32) -> Self {
        Self::Board {
            board_id,
            list_id: None,
            card_id: None,
        }
    }

    pub(crate) fn list(board_id: u32, list_id: u32) -> Self {
        Self::Board {
            board_id,
            list_id: Some(list_id),
            card_id: None,
        }
    }

    pub(crate) fn card(board_id: u32, card_id: u32) -> Self {
        Self::Board {
            board_id,
            list_id: None,
            card_id: Some(card_id),
        }
    }
}
