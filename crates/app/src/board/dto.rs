use entity::{board_label, card, entry, entry_attachment, entry_checklist_item};
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
    pub(crate) reminder_enabled: bool,
    pub(crate) labels: Vec<BoardLabelDTO>,
    pub(crate) checklist_items: Vec<ChecklistItemDTO>,
    pub(crate) attachments: Vec<EntryAttachmentDTO>,
    pub(crate) related_notes: Vec<storage::workspace_links::RelatedNote>,
}

impl storage::board_projection::BoardViewEntry for EntryDTO {
    fn view_id(&self) -> i64 {
        i64::from(self.id)
    }

    fn view_position(&self) -> i32 {
        self.position
    }

    fn view_due_on(&self) -> Option<&str> {
        self.due_on.as_deref()
    }

    fn view_has_labels(&self) -> bool {
        !self.labels.is_empty()
    }

    fn view_has_any_label(&self, label_ids: &[i64]) -> bool {
        self.labels
            .iter()
            .any(|label| label_ids.contains(&i64::from(label.id)))
    }

    fn view_has_no_labels(&self, label_ids: &[i64]) -> bool {
        self.labels
            .iter()
            .all(|label| !label_ids.contains(&i64::from(label.id)))
    }

    fn view_label_sort_key(&self) -> String {
        self.labels
            .iter()
            .map(|label| label.name.to_lowercase())
            .collect::<Vec<_>>()
            .join("\0")
    }

    fn view_related_note_count(&self) -> usize {
        self.related_notes.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryAttachmentDTO {
    pub(crate) id: u32,
    pub(crate) entry_id: u32,
    pub(crate) file_name: SharedString,
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
                let mut entries: Vec<EntryDTO> = c
                    .entries
                    .into_iter()
                    .filter(|entry| entry.deleted_at.is_none())
                    .map(EntryDTO::from)
                    .collect();
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
            reminder_enabled: e.reminder_enabled,
            labels: vec![],
            checklist_items: vec![],
            attachments: vec![],
            related_notes: vec![],
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
            reminder_enabled: e.reminder_enabled,
            labels: vec![],
            checklist_items: vec![],
            attachments: vec![],
            related_notes: vec![],
        }
    }
}

impl From<entry_attachment::Model> for EntryAttachmentDTO {
    fn from(attachment: entry_attachment::Model) -> Self {
        Self {
            id: attachment.id as u32,
            entry_id: attachment.entry_id as u32,
            file_name: SharedString::from(attachment.file_name),
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

impl From<storage::board::CardRecord> for CardDTO {
    fn from(card: storage::board::CardRecord) -> Self {
        Self {
            id: card.id,
            title: card.title.into(),
            board_id: card.board_id,
            position: card.position,
            entries: card.entries.into_iter().map(EntryDTO::from).collect(),
        }
    }
}

impl From<storage::board::EntryRecord> for EntryDTO {
    fn from(entry: storage::board::EntryRecord) -> Self {
        Self {
            id: entry.id,
            title: entry.title.into(),
            description: entry.description.into(),
            card_id: entry.card_id,
            position: entry.position,
            due_on: entry.due_on.map(SharedString::from),
            reminder_enabled: entry.reminder_enabled,
            labels: entry.labels.into_iter().map(BoardLabelDTO::from).collect(),
            checklist_items: entry
                .checklist_items
                .into_iter()
                .map(ChecklistItemDTO::from)
                .collect(),
            attachments: entry
                .attachments
                .into_iter()
                .map(EntryAttachmentDTO::from)
                .collect(),
            related_notes: entry.related_notes,
        }
    }
}

impl From<storage::board::AttachmentRecord> for EntryAttachmentDTO {
    fn from(attachment: storage::board::AttachmentRecord) -> Self {
        Self {
            id: attachment.id,
            entry_id: attachment.entry_id,
            file_name: attachment.file_name.into(),
        }
    }
}

impl From<storage::board::ChecklistItemRecord> for ChecklistItemDTO {
    fn from(item: storage::board::ChecklistItemRecord) -> Self {
        Self {
            id: item.id,
            entry_id: item.entry_id,
            title: item.title.into(),
            checked: item.checked,
            position: item.position,
        }
    }
}

impl From<storage::board::LabelRecord> for BoardLabelDTO {
    fn from(label: storage::board::LabelRecord) -> Self {
        Self {
            id: label.id,
            board_id: label.board_id,
            name: label.name.into(),
            color: label.color.into(),
        }
    }
}
