use entity::{board_label, card, entry, entry_checklist_item};
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
    pub(crate) position: i32,
    pub(crate) due_on: Option<SharedString>,
    pub(crate) labels: Vec<BoardLabelDTO>,
    pub(crate) checklist_items: Vec<ChecklistItemDTO>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChecklistItemDTO {
    pub(crate) id: u32,
    pub(crate) entry_id: u32,
    pub(crate) title: SharedString,
    pub(crate) checked: bool,
    pub(crate) position: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoardLabelDTO {
    pub(crate) id: u32,
    pub(crate) board_id: u32,
    pub(crate) name: SharedString,
    pub(crate) color: SharedString,
}

impl From<card::ModelEx> for CardDTO {
    fn from(c: card::ModelEx) -> Self {
        Self {
            id: c.id as u32,
            board_id: c.board_id as u32,
            title: SharedString::from(c.title),
            position: c.position,
            entries: {
                let mut entries: Vec<EntryDTO> =
                    c.entries.into_iter().map(EntryDTO::from).collect();
                entries.sort_by_key(|entry| (entry.position, entry.id));
                entries
            },
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
            position: e.position,
            due_on: e.due_on.map(SharedString::from),
            labels: vec![],
            checklist_items: vec![],
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
            position: e.position,
            due_on: e.due_on.map(SharedString::from),
            labels: vec![],
            checklist_items: vec![],
        }
    }
}

impl From<entry_checklist_item::Model> for ChecklistItemDTO {
    fn from(item: entry_checklist_item::Model) -> Self {
        Self {
            id: item.id as u32,
            entry_id: item.entry_id as u32,
            title: SharedString::from(item.title),
            checked: item.checked,
            position: item.position,
        }
    }
}

impl From<board_label::Model> for BoardLabelDTO {
    fn from(label: board_label::Model) -> Self {
        Self {
            id: label.id as u32,
            board_id: label.board_id as u32,
            name: SharedString::from(label.name),
            color: SharedString::from(label.color),
        }
    }
}
