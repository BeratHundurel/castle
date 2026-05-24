use entity::{card, entry};
use gpui::SharedString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CardDTO {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) board_id: u32,
    pub(crate) position: i32,
    pub(crate) entries: Vec<EntryDTO>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryDTO {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) description: SharedString,
    pub(crate) card_id: u32,
}

impl From<card::ModelEx> for CardDTO {
    fn from(c: card::ModelEx) -> Self {
        Self {
            id: c.id as u32,
            board_id: c.board_id as u32,
            title: SharedString::from(c.title),
            position: c.position,
            entries: c.entries.into_iter().map(EntryDTO::from).collect(),
        }
    }
}

impl From<entry::Model> for EntryDTO {
    fn from(e: entry::Model) -> Self {
        Self {
            id: e.id as u32,
            title: SharedString::from(e.title),
            description: SharedString::from(e.description),
            card_id: e.card_id as u32,
        }
    }
}

impl From<entry::ModelEx> for EntryDTO {
    fn from(e: entry::ModelEx) -> Self {
        Self {
            id: e.id as u32,
            title: SharedString::from(e.title),
            description: SharedString::from(e.description),
            card_id: e.card_id as u32,
        }
    }
}
