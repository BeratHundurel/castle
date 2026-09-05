use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use entity::{
    board, board::Entity as Board, board_label, board_label::Entity as BoardLabel, card,
    card::Entity as Card, entry, entry::Entity as Entry, entry_attachment,
    entry_attachment::Entity as EntryAttachment, entry_checklist_item,
    entry_checklist_item::Entity as EntryChecklistItem, entry_label,
    entry_label::Entity as EntryLabel, note, note::Entity as Note, project,
    project::Entity as Project, workspace_link, workspace_link::Entity as WorkspaceLink,
};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, Condition, ConnectionTrait, EntityTrait, ExprTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionSession, TransactionTrait,
    sea_query::{Expr, Query, SelectStatement},
};

use crate::workspace::api::{
    AddChecklistItemInput, AttachmentDetail, BoardDetail, BoardPropertyDefinitionDetail,
    BoardPropertyOptionDetail, BoardPropertyValueDetail, BoardSummary, ChecklistItemDetail,
    CreateBoardInput, CreateBoardLabelInput, CreateEntryInput, CreateListInput, CreateNoteInput,
    CreateProjectInput, EntryDetail, LabelDetail, ListDetail, MoveEntryInput, MoveNoteInput,
    NoteDetail, NoteLinkDetail, NoteLinksDetail, NoteSummary, NoteWorkspaceRelationInput,
    ProjectSummary, RelatedItemDetail, RenameBoardInput, RenameListInput, RenameProjectInput,
    SearchEntriesInput, SearchNotesInput, SetEntryLabelInput, SetEntryReminderInput,
    UpdateChecklistItemInput, UpdateEntryInput, UpdateNoteInput, WorkspaceItemKindInput,
    WorkspaceRelationsInput,
};

use crate::store::Store;

mod boards;
mod entries;
mod mapping;
mod notes;
mod projects;
mod properties;
mod relations;
mod validation;

use mapping::*;
use validation::{required_text, validate_due_on};

impl<C> Store<C>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    async fn active_note(&self, note_id: i64) -> Result<note::Model> {
        let note = Note::find_by_id(note_id)
            .filter(note::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active note {note_id} was not found"))?;
        if let Some(project_id) = note.project_id {
            self.active_project(project_id).await?;
        }
        Ok(note)
    }

    async fn note_detail(&self, note: note::Model) -> Result<NoteDetail> {
        let project_name = match note.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        let (content, file_missing) = match note.file_path.as_ref() {
            Some(path) => match tokio::fs::read_to_string(path).await {
                Ok(content) => (content, false),
                Err(_) => (note.cached_content.clone(), true),
            },
            None => (note.cached_content.clone(), false),
        };
        let related_items = self.related_items_for_note(note.id).await?;
        Ok(NoteDetail {
            id: note.id,
            title: note.title,
            content,
            project_id: note.project_id,
            project_name,
            file_path: note.file_path,
            file_managed_by_app: note.file_managed_by_app,
            file_missing: file_missing || note.file_missing_since.is_some(),
            is_pinned: note.is_pinned,
            created_at: note.created_at,
            updated_at: note.updated_at,
            related_items,
        })
    }

    async fn active_project(&self, project_id: i64) -> Result<project::Model> {
        Project::find_by_id(project_id)
            .filter(project::Column::Archived.eq(false))
            .filter(project::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active project {project_id} was not found"))
    }

    async fn active_project_map(&self) -> Result<HashMap<i64, String>> {
        Ok(Project::find()
            .filter(project::Column::Archived.eq(false))
            .filter(project::Column::DeletedAt.is_null())
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .map(|project| (project.id, project.name))
            .collect())
    }

    async fn active_board(&self, board_id: i64) -> Result<board::Model> {
        let board = Board::find_by_id(board_id)
            .filter(board::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active board {board_id} was not found"))?;
        if let Some(project_id) = board.project_id {
            self.active_project(project_id).await?;
        }
        Ok(board)
    }

    async fn active_list(&self, list_id: i64) -> Result<card::Model> {
        let list = Card::find_by_id(list_id)
            .filter(card::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active list {list_id} was not found"))?;
        self.active_board(list.board_id).await?;
        Ok(list)
    }

    async fn entry_detail(&self, entry: entry::Model) -> Result<EntryDetail> {
        let list = self.active_list(entry.card_id).await?;
        let board = self.active_board(list.board_id).await?;
        let project_name = match board.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        self.entry_detail_with_context(entry, &list, &board, project_name)
            .await
    }

    async fn entry_detail_with_context(
        &self,
        entry: entry::Model,
        list: &card::Model,
        board: &board::Model,
        project_name: Option<String>,
    ) -> Result<EntryDetail> {
        let checklist_items = EntryChecklistItem::find()
            .filter(entry_checklist_item::Column::EntryId.eq(entry.id))
            .order_by_asc(entry_checklist_item::Column::Position)
            .order_by_asc(entry_checklist_item::Column::Id)
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .map(|item| ChecklistItemDetail {
                id: item.id,
                title: item.title,
                checked: item.checked,
                position: item.position,
            })
            .collect();
        let label_ids = EntryLabel::find()
            .filter(entry_label::Column::EntryId.eq(entry.id))
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .map(|association| association.board_label_id)
            .collect::<Vec<_>>();
        let labels = if label_ids.is_empty() {
            Vec::new()
        } else {
            BoardLabel::find()
                .filter(board_label::Column::Id.is_in(label_ids))
                .order_by_asc(board_label::Column::Id)
                .all(self.db.as_ref())
                .await?
                .into_iter()
                .map(label_detail)
                .collect()
        };
        let attachments = EntryAttachment::find()
            .filter(entry_attachment::Column::EntryId.eq(entry.id))
            .order_by_asc(entry_attachment::Column::Id)
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .map(|attachment| AttachmentDetail {
                id: attachment.id,
                file_name: attachment.file_name,
            })
            .collect();
        let related_items = self
            .related_items_for_workspace_item(crate::workspace::links::WorkspaceItemRef {
                kind: crate::workspace::links::WorkspaceItemKind::Card,
                id: entry.id,
            })
            .await?;
        Ok(EntryDetail {
            id: entry.id,
            title: entry.title,
            description: entry.description,
            due_on: entry.due_on,
            reminder_enabled: entry.reminder_enabled,
            position: entry.position,
            list_id: list.id,
            list_title: list.title.clone(),
            board_id: board.id,
            board_title: board.title.clone(),
            project_id: board.project_id,
            project_name,
            labels,
            checklist_items,
            attachments,
            related_items,
        })
    }
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn next_updated_at(current: i64) -> i64 {
    std::cmp::max(now_ts(), current.saturating_add(1))
}

#[cfg(test)]
mod tests;
