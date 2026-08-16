use anyhow::Result;
use entity::{note, note::Entity as Note};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, EntityTrait, TransactionSession,
    TransactionTrait,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentRecord {
    pub id: u32,
    pub title: String,
    pub project_id: Option<i64>,
    pub file_path: Option<String>,
    pub file_managed_by_app: bool,
    pub cached_content: String,
    pub file_missing_since: Option<i64>,
}

pub async fn load_document(
    db: &impl ConnectionTrait,
    note_id: u32,
) -> Result<Option<DocumentRecord>> {
    Ok(Note::find_by_id(i64::from(note_id))
        .one(db)
        .await?
        .map(DocumentRecord::from))
}

pub async fn persist_document_content(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: u32,
    content: String,
    clear_missing: bool,
) -> Result<()> {
    persist_document(db, note_id, content, clear_missing, None).await
}

pub async fn persist_document_to_path(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: u32,
    file_path: String,
    file_managed_by_app: bool,
    content: String,
) -> Result<()> {
    persist_document(
        db,
        note_id,
        content,
        true,
        Some((file_path, file_managed_by_app)),
    )
    .await
}

async fn persist_document(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: u32,
    content: String,
    clear_missing: bool,
    location: Option<(String, bool)>,
) -> Result<()> {
    let current = Note::find_by_id(i64::from(note_id)).one(db).await?;
    let updated_at = current
        .as_ref()
        .map(|note| note.updated_at.saturating_add(1).max(now_ts()))
        .unwrap_or_else(now_ts);
    let txn = db.begin().await?;
    let mut note = note::ActiveModel {
        id: Set(i64::from(note_id)),
        cached_content: Set(content.clone()),
        updated_at: Set(updated_at),
        ..Default::default()
    };
    if clear_missing {
        note.file_missing_since = Set(None);
    }
    if let Some((file_path, file_managed_by_app)) = location {
        note.file_path = Set(Some(file_path));
        note.file_managed_by_app = Set(file_managed_by_app);
    }
    note.update(&txn).await?;
    crate::note::links::index_note_links_in_connection(
        &txn,
        i64::from(note_id),
        &content,
        updated_at,
    )
    .await?;
    txn.commit().await?;
    Ok(())
}

pub async fn mark_document_missing(db: &impl ConnectionTrait, note_id: u32) -> Result<()> {
    note::ActiveModel {
        id: Set(i64::from(note_id)),
        file_missing_since: Set(Some(now_ts())),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(())
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

impl From<note::Model> for DocumentRecord {
    fn from(note: note::Model) -> Self {
        Self {
            id: note.id as u32,
            title: note.title,
            project_id: note.project_id,
            file_path: note.file_path,
            file_managed_by_app: note.file_managed_by_app,
            cached_content: note.cached_content,
            file_missing_since: note.file_missing_since,
        }
    }
}
