use gpui::SharedString;

#[derive(Clone, PartialEq, Eq)]
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