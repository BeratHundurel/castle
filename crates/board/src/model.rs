use gpui::SharedString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoardListState {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) board_id: u32,
    pub(crate) position: i32,
    pub(crate) entries: Vec<BoardCardState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoardCardState {
    pub(crate) id: u32,
    pub(crate) title: SharedString,
    pub(crate) description: SharedString,
    pub(crate) card_id: u32,
    pub(crate) position: i32,
    pub(crate) due_on: Option<SharedString>,
    pub(crate) reminder_enabled: bool,
    pub(crate) labels: Vec<BoardLabel>,
    pub(crate) checklist_items: Vec<ChecklistItem>,
    pub(crate) attachments: Vec<EntryAttachment>,
    pub(crate) related_notes: Vec<storage::workspace::links::RelatedNote>,
}

impl storage::board::projection::BoardViewEntry for BoardCardState {
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
pub(crate) struct EntryAttachment {
    pub(crate) id: u32,
    pub(crate) entry_id: u32,
    pub(crate) file_name: SharedString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChecklistItem {
    pub(crate) id: u32,
    pub(crate) entry_id: u32,
    pub(crate) title: SharedString,
    pub(crate) checked: bool,
    pub(crate) position: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoardLabel {
    pub(crate) id: u32,
    pub(crate) board_id: u32,
    pub(crate) name: SharedString,
    pub(crate) color: SharedString,
}

impl From<storage::board::BoardListRecord> for BoardListState {
    fn from(card: storage::board::BoardListRecord) -> Self {
        Self {
            id: card.id,
            title: card.title.into(),
            board_id: card.board_id,
            position: card.position,
            entries: card.entries.into_iter().map(BoardCardState::from).collect(),
        }
    }
}

impl From<storage::board::BoardCardRecord> for BoardCardState {
    fn from(entry: storage::board::BoardCardRecord) -> Self {
        Self {
            id: entry.id,
            title: entry.title.into(),
            description: entry.description.into(),
            card_id: entry.card_id,
            position: entry.position,
            due_on: entry.due_on.map(SharedString::from),
            reminder_enabled: entry.reminder_enabled,
            labels: entry.labels.into_iter().map(BoardLabel::from).collect(),
            checklist_items: entry
                .checklist_items
                .into_iter()
                .map(ChecklistItem::from)
                .collect(),
            attachments: entry
                .attachments
                .into_iter()
                .map(EntryAttachment::from)
                .collect(),
            related_notes: entry.related_notes,
        }
    }
}

impl From<storage::board::AttachmentRecord> for EntryAttachment {
    fn from(attachment: storage::board::AttachmentRecord) -> Self {
        Self {
            id: attachment.id,
            entry_id: attachment.entry_id,
            file_name: attachment.file_name.into(),
        }
    }
}

impl From<storage::board::ChecklistItemRecord> for ChecklistItem {
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

impl From<storage::board::LabelRecord> for BoardLabel {
    fn from(label: storage::board::LabelRecord) -> Self {
        Self {
            id: label.id,
            board_id: label.board_id,
            name: label.name.into(),
            color: label.color.into(),
        }
    }
}
