use std::collections::HashMap;

pub mod commands;
pub mod positions;
pub mod projection;
pub mod properties;
pub mod templates;

use entity::{
    board_label, board_label::Entity as BoardLabel, card, card::Entity as Card,
    entry::Entity as Entry, entry_attachment, entry_attachment::Entity as EntryAttachment,
    entry_checklist_item, entry_checklist_item::Entity as EntryChecklistItem, entry_label,
    entry_label::Entity as EntryLabel,
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, TransactionTrait,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardSnapshot {
    pub cards: Vec<BoardListRecord>,
    pub labels: Vec<LabelRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardListRecord {
    pub id: u32,
    pub title: String,
    pub board_id: u32,
    pub position: i32,
    pub entries: Vec<BoardCardRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardCardRecord {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub card_id: u32,
    pub position: i32,
    pub due_on: Option<String>,
    pub reminder_enabled: bool,
    pub labels: Vec<LabelRecord>,
    pub checklist_items: Vec<ChecklistItemRecord>,
    pub attachments: Vec<AttachmentRecord>,
    pub related_notes: Vec<crate::workspace::links::RelatedNote>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentRecord {
    pub id: u32,
    pub entry_id: u32,
    pub file_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChecklistItemRecord {
    pub id: u32,
    pub entry_id: u32,
    pub title: String,
    pub checked: bool,
    pub position: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelRecord {
    pub id: u32,
    pub board_id: u32,
    pub name: String,
    pub color: String,
}

pub async fn load_board_snapshot(
    db: &(impl ConnectionTrait + TransactionTrait),
    board_id: u32,
) -> Result<BoardSnapshot, DbErr> {
    let mut cards = Card::load()
        .filter(card::Column::BoardId.eq(board_id as i64))
        .filter(card::Column::DeletedAt.is_null())
        .order_by_asc(card::Column::Position)
        .order_by_asc(card::Column::Id)
        .with(Entry)
        .all(db)
        .await?
        .into_iter()
        .map(BoardListRecord::from)
        .collect::<Vec<_>>();

    let labels = BoardLabel::find()
        .filter(board_label::Column::BoardId.eq(board_id as i64))
        .order_by_asc(board_label::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(LabelRecord::from)
        .collect::<Vec<_>>();

    let label_by_id = labels
        .iter()
        .cloned()
        .map(|label| (label.id as i64, label))
        .collect::<HashMap<_, _>>();

    let entry_ids = cards
        .iter()
        .flat_map(|card| card.entries.iter().map(|entry| entry.id as i64))
        .collect::<Vec<_>>();

    let associations = if entry_ids.is_empty() {
        Vec::new()
    } else {
        EntryLabel::find()
            .filter(entry_label::Column::EntryId.is_in(entry_ids.clone()))
            .order_by_asc(entry_label::Column::Id)
            .all(db)
            .await?
    };
    let mut labels_by_entry = HashMap::<i64, Vec<LabelRecord>>::new();
    for association in associations {
        if let Some(label) = label_by_id.get(&association.board_label_id) {
            labels_by_entry
                .entry(association.entry_id)
                .or_default()
                .push(label.clone());
        }
    }

    let attachments = if entry_ids.is_empty() {
        Vec::new()
    } else {
        EntryAttachment::find()
            .filter(entry_attachment::Column::EntryId.is_in(entry_ids.clone()))
            .order_by_asc(entry_attachment::Column::Id)
            .all(db)
            .await?
    };
    let mut attachments_by_entry = HashMap::<i64, Vec<AttachmentRecord>>::new();
    for attachment in attachments {
        attachments_by_entry
            .entry(attachment.entry_id)
            .or_default()
            .push(AttachmentRecord::from(attachment));
    }

    let checklist_items = if entry_ids.is_empty() {
        Vec::new()
    } else {
        EntryChecklistItem::find()
            .filter(entry_checklist_item::Column::EntryId.is_in(entry_ids.clone()))
            .order_by_asc(entry_checklist_item::Column::Position)
            .order_by_asc(entry_checklist_item::Column::Id)
            .all(db)
            .await?
    };
    let mut checklist_items_by_entry = HashMap::<i64, Vec<ChecklistItemRecord>>::new();
    for item in checklist_items {
        checklist_items_by_entry
            .entry(item.entry_id)
            .or_default()
            .push(ChecklistItemRecord::from(item));
    }

    let mut related_notes_by_entry =
        crate::workspace::links::load_related_notes_for_entries(db, &entry_ids)
            .await
            .map_err(|error| DbErr::Custom(error.to_string()))?;

    for card in &mut cards {
        for entry in &mut card.entries {
            entry.labels = labels_by_entry
                .remove(&(entry.id as i64))
                .unwrap_or_default();
            entry.checklist_items = checklist_items_by_entry
                .remove(&(entry.id as i64))
                .unwrap_or_default();
            entry.attachments = attachments_by_entry
                .remove(&(entry.id as i64))
                .unwrap_or_default();
            entry.related_notes = related_notes_by_entry
                .remove(&(entry.id as i64))
                .unwrap_or_default();
        }
    }

    Ok(BoardSnapshot { cards, labels })
}

impl From<card::ModelEx> for BoardListRecord {
    fn from(card: card::ModelEx) -> Self {
        let mut entries = card
            .entries
            .into_iter()
            .filter(|entry| entry.deleted_at.is_none())
            .map(BoardCardRecord::from)
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| (entry.position, entry.id));

        Self {
            id: card.id as u32,
            title: card.title,
            board_id: card.board_id as u32,
            position: card.position,
            entries,
        }
    }
}

impl From<entity::entry::ModelEx> for BoardCardRecord {
    fn from(entry: entity::entry::ModelEx) -> Self {
        Self {
            id: entry.id as u32,
            title: entry.title,
            description: entry.description,
            card_id: entry.card_id as u32,
            position: entry.position,
            due_on: entry.due_on,
            reminder_enabled: entry.reminder_enabled,
            labels: Vec::new(),
            checklist_items: Vec::new(),
            attachments: Vec::new(),
            related_notes: Vec::new(),
        }
    }
}

impl From<entry_attachment::Model> for AttachmentRecord {
    fn from(attachment: entry_attachment::Model) -> Self {
        Self {
            id: attachment.id as u32,
            entry_id: attachment.entry_id as u32,
            file_name: attachment.file_name,
        }
    }
}

impl From<entry_checklist_item::Model> for ChecklistItemRecord {
    fn from(item: entry_checklist_item::Model) -> Self {
        Self {
            id: item.id as u32,
            entry_id: item.entry_id as u32,
            title: item.title,
            checked: item.checked,
            position: item.position,
        }
    }
}

impl From<board_label::Model> for LabelRecord {
    fn from(label: board_label::Model) -> Self {
        Self {
            id: label.id as u32,
            board_id: label.board_id as u32,
            name: label.name,
            color: label.color,
        }
    }
}
