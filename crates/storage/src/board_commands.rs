use anyhow::Result;
use entity::{
    board_label, board_label::Entity as BoardLabel, card, card::Entity as BoardList, entry,
    entry::Entity as BoardCard, entry_attachment, entry_attachment::Entity as EntryAttachment,
    entry_checklist_item, entry_checklist_item::Entity as ChecklistItem, entry_label,
    entry_label::Entity as EntryLabel,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait,
    QueryFilter, TransactionSession, TransactionTrait, sea_query::Expr,
};

use crate::board::{
    AttachmentRecord, BoardCardRecord, BoardListRecord, ChecklistItemRecord, LabelRecord,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardCardDraft {
    pub title: String,
    pub description: String,
    pub list_id: u32,
    pub position: i32,
    pub due_on: Option<String>,
    pub label_ids: Vec<u32>,
    pub checklist_items: Vec<ChecklistItemDraft>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChecklistItemDraft {
    pub title: String,
    pub checked: bool,
    pub position: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardListDraft {
    pub title: String,
    pub board_id: u32,
    pub position: i32,
    pub cards: Vec<BoardCardDraft>,
}

pub async fn create_board_card(
    db: &(impl ConnectionTrait + TransactionTrait),
    draft: BoardCardDraft,
    indexed_at: i64,
) -> Result<BoardCardRecord> {
    let txn = db.begin().await?;
    let card = insert_card(&txn, draft, indexed_at).await?;
    txn.commit().await?;
    Ok(card)
}

pub async fn duplicate_board_card(
    db: &(impl ConnectionTrait + TransactionTrait),
    mut draft: BoardCardDraft,
    indexed_at: i64,
) -> Result<()> {
    let txn = db.begin().await?;
    BoardCard::update_many()
        .col_expr(
            entry::Column::Position,
            Expr::col(entry::Column::Position).add(1),
        )
        .filter(entry::Column::CardId.eq(i64::from(draft.list_id)))
        .filter(entry::Column::Position.gte(draft.position + 1))
        .exec(&txn)
        .await?;
    draft.title = format!("Copy of {}", draft.title);
    draft.position += 1;
    insert_card(&txn, draft, indexed_at).await?;
    txn.commit().await?;
    Ok(())
}

async fn insert_card(
    db: &impl ConnectionTrait,
    draft: BoardCardDraft,
    indexed_at: i64,
) -> Result<BoardCardRecord> {
    let card = entry::ActiveModel {
        title: Set(draft.title),
        description: Set(draft.description.clone()),
        card_id: Set(i64::from(draft.list_id)),
        position: Set(draft.position),
        due_on: Set(draft.due_on),
        ..Default::default()
    }
    .insert(db)
    .await?;
    for label_id in draft.label_ids {
        entry_label::ActiveModel {
            entry_id: Set(card.id),
            board_label_id: Set(i64::from(label_id)),
            ..Default::default()
        }
        .insert(db)
        .await?;
    }
    let mut checklist_items = Vec::with_capacity(draft.checklist_items.len());
    for item in draft.checklist_items {
        let item = entry_checklist_item::ActiveModel {
            entry_id: Set(card.id),
            title: Set(item.title),
            checked: Set(item.checked),
            position: Set(item.position),
            ..Default::default()
        }
        .insert(db)
        .await?;
        checklist_items.push(ChecklistItemRecord::from(item));
    }
    crate::workspace_links::index_entry_workspace_links_in_connection(
        db,
        card.id,
        &draft.description,
        indexed_at,
    )
    .await?;
    Ok(BoardCardRecord {
        id: card.id as u32,
        title: card.title,
        description: card.description,
        card_id: card.card_id as u32,
        position: card.position,
        due_on: card.due_on,
        reminder_enabled: card.reminder_enabled,
        labels: Vec::new(),
        checklist_items,
        attachments: Vec::new(),
        related_notes: Vec::new(),
    })
}

pub async fn create_board_list(
    db: &impl ConnectionTrait,
    draft: BoardListDraft,
) -> Result<BoardListRecord> {
    let list = card::ActiveModel {
        title: Set(draft.title),
        board_id: Set(i64::from(draft.board_id)),
        position: Set(draft.position),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(BoardListRecord {
        id: list.id as u32,
        title: list.title,
        board_id: list.board_id as u32,
        position: list.position,
        entries: Vec::new(),
    })
}

pub async fn duplicate_board_list(
    db: &(impl ConnectionTrait + TransactionTrait),
    source: BoardListDraft,
    indexed_at: i64,
) -> Result<()> {
    let txn = db.begin().await?;
    BoardList::update_many()
        .col_expr(
            card::Column::Position,
            Expr::col(card::Column::Position).add(1),
        )
        .filter(card::Column::BoardId.eq(i64::from(source.board_id)))
        .filter(card::Column::Position.gte(source.position + 1))
        .exec(&txn)
        .await?;
    let list = card::ActiveModel {
        title: Set(format!("Copy of {}", source.title)),
        board_id: Set(i64::from(source.board_id)),
        position: Set(source.position + 1),
        ..Default::default()
    }
    .insert(&txn)
    .await?;
    for mut card in source.cards {
        card.list_id = list.id as u32;
        insert_card(&txn, card, indexed_at).await?;
    }
    txn.commit().await?;
    Ok(())
}

pub async fn rename_board_list(
    db: &impl ConnectionTrait,
    list_id: u32,
    title: String,
) -> Result<()> {
    card::ActiveModel {
        id: Set(i64::from(list_id)),
        title: Set(title),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(())
}

pub async fn update_board_card(
    db: &(impl ConnectionTrait + TransactionTrait),
    card_id: u32,
    title: String,
    description: String,
    indexed_at: i64,
) -> Result<()> {
    let txn = db.begin().await?;
    entry::ActiveModel {
        id: Set(i64::from(card_id)),
        title: Set(title),
        description: Set(description.clone()),
        ..Default::default()
    }
    .update(&txn)
    .await?;
    crate::workspace_links::index_entry_workspace_links_in_connection(
        &txn,
        i64::from(card_id),
        &description,
        indexed_at,
    )
    .await?;
    txn.commit().await?;
    Ok(())
}

pub async fn set_board_card_due_on(
    db: &impl ConnectionTrait,
    card_id: u32,
    due_on: Option<String>,
) -> Result<()> {
    entry::ActiveModel {
        id: Set(i64::from(card_id)),
        due_on: Set(due_on),
        reminder_notified_for: Set(None),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(())
}

pub async fn set_board_card_reminder(
    db: &impl ConnectionTrait,
    card_id: u32,
    enabled: bool,
) -> Result<()> {
    entry::ActiveModel {
        id: Set(i64::from(card_id)),
        reminder_enabled: Set(enabled),
        reminder_notified_for: Set(None),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(())
}

pub async fn create_label(
    db: &impl ConnectionTrait,
    board_id: u32,
    name: String,
    color: String,
) -> Result<LabelRecord> {
    Ok(LabelRecord::from(
        board_label::ActiveModel {
            board_id: Set(i64::from(board_id)),
            name: Set(name),
            color: Set(color),
            ..Default::default()
        }
        .insert(db)
        .await?,
    ))
}

pub async fn rename_label(db: &impl ConnectionTrait, label_id: u32, name: String) -> Result<()> {
    board_label::ActiveModel {
        id: Set(i64::from(label_id)),
        name: Set(name),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(())
}

pub async fn set_label_assignment(
    db: &impl ConnectionTrait,
    card_id: u32,
    label_id: u32,
    assigned: bool,
) -> Result<()> {
    let existing = EntryLabel::find()
        .filter(entry_label::Column::EntryId.eq(i64::from(card_id)))
        .filter(entry_label::Column::BoardLabelId.eq(i64::from(label_id)))
        .one(db)
        .await?;
    match (assigned, existing) {
        (true, None) => {
            entry_label::ActiveModel {
                entry_id: Set(i64::from(card_id)),
                board_label_id: Set(i64::from(label_id)),
                ..Default::default()
            }
            .insert(db)
            .await?;
        }
        (false, Some(assignment)) => {
            EntryLabel::delete_by_id(assignment.id).exec(db).await?;
        }
        _ => {}
    }
    Ok(())
}

pub async fn delete_label(db: &impl ConnectionTrait, label_id: u32) -> Result<()> {
    BoardLabel::delete_by_id(i64::from(label_id))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn create_checklist_item(
    db: &impl ConnectionTrait,
    card_id: u32,
    title: String,
    position: i32,
) -> Result<ChecklistItemRecord> {
    Ok(ChecklistItemRecord::from(
        entry_checklist_item::ActiveModel {
            entry_id: Set(i64::from(card_id)),
            title: Set(title),
            checked: Set(false),
            position: Set(position),
            ..Default::default()
        }
        .insert(db)
        .await?,
    ))
}

pub async fn update_checklist_item(
    db: &impl ConnectionTrait,
    item_id: u32,
    title: Option<String>,
    checked: Option<bool>,
) -> Result<()> {
    let mut item = entry_checklist_item::ActiveModel {
        id: Set(i64::from(item_id)),
        ..Default::default()
    };
    if let Some(title) = title {
        item.title = Set(title);
    }
    if let Some(checked) = checked {
        item.checked = Set(checked);
    }
    item.update(db).await?;
    Ok(())
}

pub async fn delete_checklist_item(db: &impl ConnectionTrait, item_id: u32) -> Result<()> {
    ChecklistItem::delete_by_id(i64::from(item_id))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn reorder_checklist_items(
    db: &(impl ConnectionTrait + TransactionTrait),
    positions: Vec<(u32, i32)>,
) -> Result<()> {
    let txn = db.begin().await?;
    for (item_id, position) in positions {
        entry_checklist_item::ActiveModel {
            id: Set(i64::from(item_id)),
            position: Set(position),
            ..Default::default()
        }
        .update(&txn)
        .await?;
    }
    txn.commit().await?;
    Ok(())
}

pub async fn create_attachments(
    db: &(impl ConnectionTrait + TransactionTrait),
    card_id: u32,
    file_names: Vec<String>,
) -> Result<Vec<AttachmentRecord>> {
    let txn = db.begin().await?;
    let mut attachments = Vec::with_capacity(file_names.len());
    for file_name in file_names {
        let attachment = entry_attachment::ActiveModel {
            entry_id: Set(i64::from(card_id)),
            file_name: Set(file_name),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
        attachments.push(AttachmentRecord::from(attachment));
    }
    txn.commit().await?;
    Ok(attachments)
}

pub async fn delete_attachment(db: &impl ConnectionTrait, attachment_id: u32) -> Result<()> {
    EntryAttachment::delete_by_id(i64::from(attachment_id))
        .exec(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::Database;

    #[tokio::test]
    async fn board_commands_preserve_order_and_nested_data_across_duplication() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = crate::workspace::create_board(&db, None, "Delivery".to_string()).await?;
        let list = create_board_list(
            &db,
            BoardListDraft {
                title: "Selected".to_string(),
                board_id: board.id,
                position: 0,
                cards: Vec::new(),
            },
        )
        .await?;
        let label = create_label(&db, board.id, "Ready".to_string(), "blue".to_string()).await?;
        let draft = BoardCardDraft {
            title: "Ship release".to_string(),
            description: "Keep the checklist".to_string(),
            list_id: list.id,
            position: 0,
            due_on: Some("2026-08-12".to_string()),
            label_ids: vec![label.id],
            checklist_items: vec![ChecklistItemDraft {
                title: "Verify assets".to_string(),
                checked: true,
                position: 0,
            }],
        };
        let card = create_board_card(&db, draft.clone(), 1).await?;
        set_board_card_reminder(&db, card.id, true).await?;
        let attachments = create_attachments(
            &db,
            card.id,
            vec!["release.png".to_string(), "notes.txt".to_string()],
        )
        .await?;
        duplicate_board_card(&db, draft.clone(), 2).await?;

        let snapshot = crate::board::load_board_snapshot(&db, board.id).await?;
        assert_eq!(snapshot.cards.len(), 1);
        assert_eq!(snapshot.cards[0].entries.len(), 2);
        assert_eq!(snapshot.cards[0].entries[0].title, "Ship release");
        assert_eq!(snapshot.cards[0].entries[0].position, 0);
        assert!(snapshot.cards[0].entries[0].reminder_enabled);
        assert_eq!(snapshot.cards[0].entries[0].attachments.len(), 2);
        assert_eq!(snapshot.cards[0].entries[1].title, "Copy of Ship release");
        assert_eq!(snapshot.cards[0].entries[1].position, 1);
        assert_eq!(snapshot.cards[0].entries[1].labels[0].name, "Ready");
        assert!(snapshot.cards[0].entries[1].checklist_items[0].checked);

        delete_attachment(&db, attachments[0].id).await?;
        rename_board_list(&db, list.id, "Approved".to_string()).await?;
        duplicate_board_list(
            &db,
            BoardListDraft {
                title: "Approved".to_string(),
                board_id: board.id,
                position: 0,
                cards: vec![draft],
            },
            3,
        )
        .await?;

        let snapshot = crate::board::load_board_snapshot(&db, board.id).await?;
        assert_eq!(snapshot.cards.len(), 2);
        assert_eq!(snapshot.cards[0].title, "Approved");
        assert_eq!(snapshot.cards[0].position, 0);
        assert_eq!(snapshot.cards[0].entries[0].attachments.len(), 1);
        assert_eq!(snapshot.cards[1].title, "Copy of Approved");
        assert_eq!(snapshot.cards[1].position, 1);
        assert_eq!(snapshot.cards[1].entries[0].title, "Ship release");
        Ok(())
    }
}
