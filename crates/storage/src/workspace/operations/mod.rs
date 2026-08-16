use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use entity::{
    board, board::Entity as Board, board_label, board_label::Entity as BoardLabel, card,
    card::Entity as Card, entry, entry::Entity as Entry, entry_attachment,
    entry_attachment::Entity as EntryAttachment, entry_checklist_item,
    entry_checklist_item::Entity as EntryChecklistItem, entry_label,
    entry_label::Entity as EntryLabel, note, note::Entity as Note, project,
    project::Entity as Project,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait,
    ExprTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, TransactionSession,
    TransactionTrait, sea_query::Expr,
};

use workspace_api::{
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

impl<C> Store<C>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    pub async fn list_projects(&self) -> Result<Vec<ProjectSummary>> {
        let projects = Project::find()
            .filter(project::Column::Archived.eq(false))
            .filter(project::Column::DeletedAt.is_null())
            .order_by_asc(project::Column::Position)
            .order_by_asc(project::Column::Id)
            .all(self.db.as_ref())
            .await?;

        let board_counts = Board::find()
            .filter(board::Column::DeletedAt.is_null())
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .filter_map(|board| board.project_id)
            .fold(HashMap::<i64, u64>::new(), |mut counts, project_id| {
                *counts.entry(project_id).or_default() += 1;
                counts
            });

        Ok(projects
            .into_iter()
            .map(|project| ProjectSummary {
                id: project.id,
                name: project.name,
                position: project.position,
                board_count: board_counts.get(&project.id).copied().unwrap_or_default(),
            })
            .collect())
    }

    pub async fn list_boards(&self, project_id: Option<i64>) -> Result<Vec<BoardSummary>> {
        if let Some(project_id) = project_id {
            self.active_project(project_id).await?;
        }
        let mut query = Board::find().filter(board::Column::DeletedAt.is_null());
        if let Some(project_id) = project_id {
            query = query.filter(board::Column::ProjectId.eq(project_id));
        }
        let boards = query
            .order_by_asc(board::Column::Id)
            .all(self.db.as_ref())
            .await?;
        let projects = self.active_project_map().await?;

        Ok(boards
            .into_iter()
            .filter(|board| {
                board
                    .project_id
                    .is_none_or(|project_id| projects.contains_key(&project_id))
            })
            .map(|board| BoardSummary {
                id: board.id,
                title: board.title,
                project_id: board.project_id,
                project_name: board
                    .project_id
                    .and_then(|project_id| projects.get(&project_id).cloned()),
            })
            .collect())
    }

    pub async fn list_notes(
        &self,
        project_id: Option<i64>,
        limit: Option<u64>,
    ) -> Result<Vec<NoteSummary>> {
        if let Some(project_id) = project_id {
            self.active_project(project_id).await?;
        }

        let projects = self.active_project_map().await?;
        let mut query = Note::find().filter(note::Column::DeletedAt.is_null());

        if let Some(project_id) = project_id {
            query = query.filter(note::Column::ProjectId.eq(project_id));
        }

        let notes = query
            .order_by_desc(note::Column::IsPinned)
            .order_by_desc(note::Column::UpdatedAt)
            .order_by_asc(note::Column::Id)
            .limit(limit.unwrap_or(50).clamp(1, 100))
            .all(self.db.as_ref())
            .await?;

        Ok(notes
            .into_iter()
            .filter(|note| {
                note.project_id
                    .is_none_or(|project_id| projects.contains_key(&project_id))
            })
            .map(|note| note_summary(note, &projects))
            .collect())
    }

    pub async fn get_note(&self, note_id: i64) -> Result<NoteDetail> {
        let note = self.active_note(note_id).await?;
        self.note_detail(note).await
    }

    pub async fn get_note_links(&self, note_id: i64) -> Result<NoteLinksDetail> {
        let links = crate::note::links::load_note_links(self.db.as_ref(), note_id).await?;
        Ok(NoteLinksDetail {
            inbound: links.inbound.into_iter().map(note_link_detail).collect(),
            outbound: links.outbound.into_iter().map(note_link_detail).collect(),
            unresolved: links
                .unresolved
                .into_iter()
                .map(unresolved_link_detail)
                .collect(),
        })
    }

    pub async fn list_workspace_relations(
        &self,
        input: WorkspaceRelationsInput,
    ) -> Result<Vec<RelatedItemDetail>> {
        match (input.note_id, input.kind, input.item_id) {
            (Some(note_id), None, None) => {
                self.active_note(note_id).await?;
                self.related_items_for_note(note_id).await
            }
            (None, Some(kind), Some(item_id)) => {
                let relation = NoteWorkspaceRelationInput {
                    note_id: 0,
                    kind,
                    item_id,
                    board_id: input.board_id,
                    list_id: input.list_id,
                };
                let item = self.validate_relation_target(&relation).await?;
                self.related_items_for_workspace_item(item).await
            }
            _ => {
                bail!("provide either note_id, or kind and item_id with the required hierarchy IDs")
            }
        }
    }

    pub async fn link_note_to_workspace_item(
        &self,
        input: NoteWorkspaceRelationInput,
    ) -> Result<Vec<RelatedItemDetail>> {
        self.active_note(input.note_id).await?;
        let item = self.validate_relation_target(&input).await?;
        Ok(crate::workspace::links::set_manual_note_link(
            self.db.as_ref(),
            input.note_id,
            item,
            true,
            now_ts(),
        )
        .await?
        .related_notes
        .into_iter()
        .map(related_note_detail)
        .collect())
    }

    pub async fn unlink_note_from_workspace_item(
        &self,
        input: NoteWorkspaceRelationInput,
    ) -> Result<Vec<RelatedItemDetail>> {
        self.active_note(input.note_id).await?;
        let item = self.validate_relation_target(&input).await?;
        Ok(crate::workspace::links::set_manual_note_link(
            self.db.as_ref(),
            input.note_id,
            item,
            false,
            now_ts(),
        )
        .await?
        .related_notes
        .into_iter()
        .map(related_note_detail)
        .collect())
    }

    pub async fn search_notes(&self, input: SearchNotesInput) -> Result<Vec<NoteSummary>> {
        let query_text = input.query.trim();
        if query_text.is_empty() {
            bail!("query must not be empty");
        }
        if let Some(project_id) = input.project_id {
            self.active_project(project_id).await?;
        }
        let projects = self.active_project_map().await?;
        let mut query = Note::find()
            .filter(note::Column::DeletedAt.is_null())
            .filter(
                Condition::any()
                    .add(note::Column::Title.contains(query_text))
                    .add(note::Column::CachedContent.contains(query_text)),
            );

        if let Some(project_id) = input.project_id {
            query = query.filter(note::Column::ProjectId.eq(project_id));
        }

        let notes = query
            .order_by_desc(note::Column::UpdatedAt)
            .limit(input.limit.unwrap_or(25).clamp(1, 100))
            .all(self.db.as_ref())
            .await?;

        Ok(notes
            .into_iter()
            .filter(|note| {
                note.project_id
                    .is_none_or(|project_id| projects.contains_key(&project_id))
            })
            .map(|note| note_summary(note, &projects))
            .collect())
    }

    pub async fn create_note(&self, input: CreateNoteInput) -> Result<NoteDetail> {
        let title = required_text(input.title, "note title")?;
        let project_name = match input.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        let now = now_ts();
        let txn = self.db.as_ref().begin().await?;
        let note = note::ActiveModel {
            title: Set(title),
            project_id: Set(input.project_id),
            file_path: Set(None),
            file_managed_by_app: Set(false),
            cached_content: Set(input.content.clone()),
            file_missing_since: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
        crate::note::links::index_note_links_in_connection(
            &txn,
            note.id,
            &input.content,
            note.updated_at,
        )
        .await?;
        txn.commit().await?;
        let related_items = self.related_items_for_note(note.id).await?;
        Ok(NoteDetail {
            id: note.id,
            title: note.title,
            content: input.content,
            project_id: note.project_id,
            project_name,
            file_path: note.file_path,
            file_managed_by_app: note.file_managed_by_app,
            file_missing: false,
            is_pinned: note.is_pinned,
            created_at: note.created_at,
            updated_at: note.updated_at,
            related_items,
        })
    }

    pub async fn update_note(&self, input: UpdateNoteInput) -> Result<NoteDetail> {
        if input.title.is_none() && input.content.is_none() && input.is_pinned.is_none() {
            bail!("provide title, content, or is_pinned to update the note");
        }
        let mut note = self.active_note(input.note_id).await?;
        if let Some(expected) = input.expected_updated_at
            && expected != note.updated_at
        {
            bail!(
                "note {} changed since it was read; expected updated_at {}, current value is {}",
                note.id,
                expected,
                note.updated_at
            );
        }

        if let Some(content) = input.content.as_ref()
            && let Some(file_path) = note.file_path.as_ref()
        {
            let path = PathBuf::from(file_path);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(path, content).await?;
        }

        let new_title = input
            .title
            .map(|title| required_text(title, "note title"))
            .transpose()?;
        if let Some(new_title) = new_title.as_deref().filter(|title| *title != note.title) {
            let note_id = u32::try_from(note.id)
                .with_context(|| format!("note id {} cannot be renamed", note.id))?;
            crate::workspace::persist_workspace_title(
                self.db.as_ref(),
                crate::workspace::WorkspaceTitleTarget::Note(note_id),
                new_title.to_string(),
            )
            .await?;
            note = self.active_note(input.note_id).await?;
        }
        let content_for_index = input.content.clone();
        let current_updated_at = note.updated_at;
        let mut active: note::ActiveModel = note.into();
        if let Some(content) = input.content {
            active.cached_content = Set(content);
            active.file_missing_since = Set(None);
        }
        if let Some(is_pinned) = input.is_pinned {
            active.is_pinned = Set(is_pinned);
        }
        active.updated_at = Set(next_updated_at(current_updated_at));
        let txn = self.db.as_ref().begin().await?;
        let note = active.update(&txn).await?;
        if let Some(content) = content_for_index {
            crate::note::links::index_note_links_in_connection(
                &txn,
                note.id,
                &content,
                note.updated_at,
            )
            .await?;
        }
        txn.commit().await?;
        self.note_detail(note).await
    }

    pub async fn move_note(&self, input: MoveNoteInput) -> Result<NoteDetail> {
        if let Some(project_id) = input.project_id {
            self.active_project(project_id).await?;
        }
        let note = self.active_note(input.note_id).await?;
        let note = note::ActiveModel {
            id: Set(note.id),
            project_id: Set(input.project_id),
            updated_at: Set(next_updated_at(note.updated_at)),
            ..Default::default()
        }
        .update(self.db.as_ref())
        .await?;
        self.note_detail(note).await
    }

    pub async fn get_board(&self, board_id: i64) -> Result<BoardDetail> {
        let board = self.active_board(board_id).await?;
        let project_name = match board.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        let board_id = u32::try_from(board.id)
            .with_context(|| format!("board id {} is outside the supported range", board.id))?;
        let snapshot = crate::board::load_board_snapshot(self.db.as_ref(), board_id).await?;
        let board_item = crate::workspace::links::WorkspaceItemRef {
            kind: crate::workspace::links::WorkspaceItemKind::Board,
            id: board.id,
        };
        let mut relation_items = Vec::with_capacity(snapshot.cards.len() + 1);
        relation_items.push(board_item);
        relation_items.extend(snapshot.cards.iter().map(|list| {
            crate::workspace::links::WorkspaceItemRef {
                kind: crate::workspace::links::WorkspaceItemKind::List,
                id: i64::from(list.id),
            }
        }));
        let mut related_notes = crate::workspace::links::load_related_notes_for_items(
            self.db.as_ref(),
            &relation_items,
        )
        .await?;

        let details = snapshot
            .cards
            .into_iter()
            .map(|list| {
                let list_item = crate::workspace::links::WorkspaceItemRef {
                    kind: crate::workspace::links::WorkspaceItemKind::List,
                    id: i64::from(list.id),
                };
                ListDetail {
                    id: i64::from(list.id),
                    title: list.title.clone(),
                    position: list.position,
                    entries: list
                        .entries
                        .into_iter()
                        .map(|entry| {
                            entry_record_detail(entry, &list.title, &board, project_name.clone())
                        })
                        .collect(),
                    related_items: related_notes
                        .remove(&list_item)
                        .unwrap_or_default()
                        .into_iter()
                        .map(related_note_detail)
                        .collect(),
                }
            })
            .collect();

        Ok(BoardDetail {
            id: board.id,
            title: board.title,
            project_id: board.project_id,
            project_name,
            labels: snapshot
                .labels
                .into_iter()
                .map(label_record_detail)
                .collect(),
            lists: details,
            related_items: related_notes
                .remove(&board_item)
                .unwrap_or_default()
                .into_iter()
                .map(related_note_detail)
                .collect(),
        })
    }

    pub async fn get_entry(&self, entry_id: i64) -> Result<EntryDetail> {
        let entry = Entry::find_by_id(entry_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active board entry {entry_id} was not found"))?;

        self.entry_detail(entry).await
    }

    pub async fn search_entries(&self, input: SearchEntriesInput) -> Result<Vec<EntryDetail>> {
        let query = input.query.trim();
        if query.is_empty() {
            bail!("query must not be empty");
        }
        if let Some(project_id) = input.project_id {
            self.active_project(project_id).await?;
        }
        if let Some(board_id) = input.board_id {
            self.active_board(board_id).await?;
        }
        let limit = input.limit.unwrap_or(25).clamp(1, 100);
        let projects = self.active_project_map().await?;
        let board_ids = Board::find()
            .filter(board::Column::DeletedAt.is_null())
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .filter(|board| {
                board
                    .project_id
                    .is_none_or(|project_id| projects.contains_key(&project_id))
                    && input
                        .project_id
                        .is_none_or(|project_id| board.project_id == Some(project_id))
                    && input.board_id.is_none_or(|board_id| board.id == board_id)
            })
            .map(|board| board.id)
            .collect::<HashSet<_>>();
        if board_ids.is_empty() {
            return Ok(Vec::new());
        }
        let list_ids = Card::find()
            .filter(card::Column::DeletedAt.is_null())
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .filter(|list| board_ids.contains(&list.board_id))
            .map(|list| list.id)
            .collect::<Vec<_>>();
        if list_ids.is_empty() {
            return Ok(Vec::new());
        }
        let entries = Entry::find()
            .filter(entry::Column::DeletedAt.is_null())
            .filter(entry::Column::CardId.is_in(list_ids))
            .filter(
                Condition::any()
                    .add(entry::Column::Title.contains(query))
                    .add(entry::Column::Description.contains(query)),
            )
            .order_by_asc(entry::Column::Id)
            .limit(limit)
            .all(self.db.as_ref())
            .await?;

        let mut details = Vec::with_capacity(entries.len());
        for entry in entries {
            details.push(self.entry_detail(entry).await?);
        }
        Ok(details)
    }

    pub async fn create_project(&self, input: CreateProjectInput) -> Result<ProjectSummary> {
        let name = required_text(input.name, "project name")?;
        let project = crate::workspace::create_project(self, name).await?;
        Ok(ProjectSummary {
            id: i64::from(project.id),
            name: project.name,
            position: project.position,
            board_count: 0,
        })
    }

    pub async fn rename_project(&self, input: RenameProjectInput) -> Result<ProjectSummary> {
        self.active_project(input.project_id).await?;
        crate::workspace::rename_project(
            self,
            u32::try_from(input.project_id).context("project ID is out of range")?,
            required_text(input.name, "project name")?,
        )
        .await?;
        self.list_projects()
            .await?
            .into_iter()
            .find(|project| project.id == input.project_id)
            .with_context(|| format!("renamed project {} was not found", input.project_id))
    }

    pub async fn create_board(&self, input: CreateBoardInput) -> Result<BoardSummary> {
        let title = required_text(input.title, "board title")?;
        let project_name = match input.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        let board = crate::workspace::create_board(
            self,
            input
                .project_id
                .map(u32::try_from)
                .transpose()
                .context("project ID is out of range")?,
            title,
        )
        .await?;
        Ok(BoardSummary {
            id: i64::from(board.id),
            title: board.title,
            project_id: input.project_id,
            project_name,
        })
    }

    pub async fn rename_board(&self, input: RenameBoardInput) -> Result<BoardSummary> {
        let board = self.active_board(input.board_id).await?;
        let title = required_text(input.title, "board title")?;
        crate::workspace::persist_workspace_title(
            self,
            crate::workspace::WorkspaceTitleTarget::Board(
                u32::try_from(board.id).context("board ID is out of range")?,
            ),
            title.clone(),
        )
        .await?;
        let project_name = match board.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        Ok(BoardSummary {
            id: board.id,
            title,
            project_id: board.project_id,
            project_name,
        })
    }

    pub async fn create_list(&self, input: CreateListInput) -> Result<ListDetail> {
        let title = required_text(input.title, "list title")?;
        self.active_board(input.board_id).await?;
        let position = Card::find()
            .filter(card::Column::BoardId.eq(input.board_id))
            .filter(card::Column::DeletedAt.is_null())
            .count(self.db.as_ref())
            .await? as i32;
        let list = crate::board::commands::create_board_list(
            self,
            crate::board::commands::BoardListDraft {
                title,
                board_id: u32::try_from(input.board_id).context("board ID is out of range")?,
                position,
                cards: Vec::new(),
            },
        )
        .await?;
        Ok(ListDetail {
            id: i64::from(list.id),
            title: list.title,
            position: list.position,
            entries: Vec::new(),
            related_items: Vec::new(),
        })
    }

    pub async fn rename_list(&self, input: RenameListInput) -> Result<ListDetail> {
        let list = self.active_list(input.list_id).await?;
        crate::board::commands::rename_board_list(
            self,
            u32::try_from(list.id).context("list ID is out of range")?,
            required_text(input.title, "list title")?,
        )
        .await?;
        self.get_board(list.board_id)
            .await?
            .lists
            .into_iter()
            .find(|candidate| candidate.id == list.id)
            .with_context(|| format!("renamed list {} was not found", list.id))
    }

    pub async fn create_entry(&self, input: CreateEntryInput) -> Result<EntryDetail> {
        let title = required_text(input.title, "entry title")?;
        validate_due_on(input.due_on.as_deref())?;
        self.active_list(input.list_id).await?;
        let position = Entry::find()
            .filter(entry::Column::CardId.eq(input.list_id))
            .filter(entry::Column::DeletedAt.is_null())
            .count(self.db.as_ref())
            .await? as i32;
        let entry = crate::board::commands::create_board_card(
            self,
            crate::board::commands::BoardCardDraft {
                title,
                description: input.description,
                list_id: u32::try_from(input.list_id).context("list ID is out of range")?,
                position,
                due_on: input.due_on,
                label_ids: Vec::new(),
                checklist_items: Vec::new(),
            },
            now_ts(),
        )
        .await?;
        self.get_entry(i64::from(entry.id)).await
    }

    pub async fn update_entry(&self, input: UpdateEntryInput) -> Result<EntryDetail> {
        if input.clear_due_on && input.due_on.is_some() {
            bail!("due_on and clear_due_on cannot be used together");
        }
        validate_due_on(input.due_on.as_deref())?;
        let entry = Entry::find_by_id(input.entry_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active board entry {} was not found", input.entry_id))?;
        let title = match input.title {
            Some(title) => required_text(title, "entry title")?,
            None => entry.title.clone(),
        };
        let description = input
            .description
            .unwrap_or_else(|| entry.description.clone());
        crate::board::commands::update_board_card(
            self,
            u32::try_from(entry.id).context("entry ID is out of range")?,
            title,
            description,
            now_ts(),
        )
        .await?;
        if input.clear_due_on || input.due_on.is_some() {
            crate::board::commands::set_board_card_due_on(
                self,
                u32::try_from(entry.id).context("entry ID is out of range")?,
                if input.clear_due_on {
                    None
                } else {
                    input.due_on
                },
            )
            .await?;
        }
        self.get_entry(entry.id).await
    }

    pub async fn set_entry_reminder(&self, input: SetEntryReminderInput) -> Result<EntryDetail> {
        let entry = Entry::find_by_id(input.entry_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active board entry {} was not found", input.entry_id))?;
        self.active_list(entry.card_id).await?;
        if input.enabled && entry.due_on.is_none() {
            bail!("a board entry needs a due date before its reminder can be enabled");
        }
        crate::board::commands::set_board_card_reminder(
            self,
            u32::try_from(entry.id).context("entry ID is out of range")?,
            input.enabled,
        )
        .await?;
        self.get_entry(entry.id).await
    }

    pub async fn add_checklist_item(
        &self,
        input: AddChecklistItemInput,
    ) -> Result<ChecklistItemDetail> {
        self.get_entry(input.entry_id).await?;
        let position = EntryChecklistItem::find()
            .filter(entry_checklist_item::Column::EntryId.eq(input.entry_id))
            .count(self.db.as_ref())
            .await? as i32;
        let item = crate::board::commands::create_checklist_item(
            self,
            u32::try_from(input.entry_id).context("entry ID is out of range")?,
            required_text(input.title, "checklist item title")?,
            position,
        )
        .await?;
        Ok(ChecklistItemDetail {
            id: i64::from(item.id),
            title: item.title,
            checked: item.checked,
            position: item.position,
        })
    }

    pub async fn update_checklist_item(
        &self,
        input: UpdateChecklistItemInput,
    ) -> Result<ChecklistItemDetail> {
        if input.title.is_none() && input.checked.is_none() {
            bail!("provide title or checked to update the checklist item");
        }
        let item = EntryChecklistItem::find_by_id(input.item_id)
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("checklist item {} was not found", input.item_id))?;
        self.get_entry(item.entry_id).await?;
        let title = input
            .title
            .map(|title| required_text(title, "checklist item title"))
            .transpose()?;
        crate::board::commands::update_checklist_item(
            self,
            u32::try_from(item.id).context("checklist item ID is out of range")?,
            title,
            input.checked,
        )
        .await?;
        let item = EntryChecklistItem::find_by_id(item.id)
            .one(self.db.as_ref())
            .await?
            .context("updated checklist item was not found")?;
        Ok(ChecklistItemDetail {
            id: item.id,
            title: item.title,
            checked: item.checked,
            position: item.position,
        })
    }

    pub async fn create_board_label(&self, input: CreateBoardLabelInput) -> Result<LabelDetail> {
        self.active_board(input.board_id).await?;
        let label = crate::board::commands::create_label(
            self,
            u32::try_from(input.board_id).context("board ID is out of range")?,
            required_text(input.name, "label name")?,
            required_text(input.color, "label color")?,
        )
        .await?;
        Ok(label_record_detail(label))
    }

    pub async fn set_entry_label(&self, input: SetEntryLabelInput) -> Result<EntryDetail> {
        let entry = self.get_entry(input.entry_id).await?;
        let label = BoardLabel::find_by_id(input.label_id)
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("board label {} was not found", input.label_id))?;
        if label.board_id != entry.board_id {
            bail!(
                "label {} belongs to board {}, but entry {} belongs to board {}",
                label.id,
                label.board_id,
                entry.id,
                entry.board_id
            );
        }
        crate::board::commands::set_label_assignment(
            self,
            u32::try_from(input.entry_id).context("entry ID is out of range")?,
            u32::try_from(input.label_id).context("label ID is out of range")?,
            input.assigned,
        )
        .await?;
        self.get_entry(input.entry_id).await
    }

    pub async fn move_entry(&self, input: MoveEntryInput) -> Result<EntryDetail> {
        self.active_list(input.list_id).await?;
        let entry = Entry::find_by_id(input.entry_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active board entry {} was not found", input.entry_id))?;
        if entry.card_id == input.list_id {
            return self.entry_detail(entry).await;
        }

        let transaction = self.db.as_ref().begin().await?;
        Entry::update_many()
            .col_expr(
                entry::Column::Position,
                Expr::col(entry::Column::Position).sub(1),
            )
            .filter(entry::Column::CardId.eq(entry.card_id))
            .filter(entry::Column::Position.gt(entry.position))
            .exec(&transaction)
            .await?;
        let position = Entry::find()
            .filter(entry::Column::CardId.eq(input.list_id))
            .filter(entry::Column::DeletedAt.is_null())
            .count(&transaction)
            .await? as i32;
        let moved = entry::ActiveModel {
            id: Set(entry.id),
            card_id: Set(input.list_id),
            position: Set(position),
            ..Default::default()
        }
        .update(&transaction)
        .await?;
        transaction.commit().await?;
        self.entry_detail(moved).await
    }

    async fn validate_relation_target(
        &self,
        input: &NoteWorkspaceRelationInput,
    ) -> Result<crate::workspace::links::WorkspaceItemRef> {
        let kind = match input.kind {
            WorkspaceItemKindInput::Board => crate::workspace::links::WorkspaceItemKind::Board,
            WorkspaceItemKindInput::List => crate::workspace::links::WorkspaceItemKind::List,
            WorkspaceItemKindInput::Card => crate::workspace::links::WorkspaceItemKind::Card,
        };
        let catalog =
            crate::workspace::links::load_workspace_link_catalog(self.db.as_ref()).await?;
        let target = catalog
            .iter()
            .find(|entry| entry.item.kind == kind && entry.item.id == input.item_id)
            .with_context(|| format!("active {} {} was not found", kind.as_str(), input.item_id))?;
        match kind {
            crate::workspace::links::WorkspaceItemKind::Board => {
                if input
                    .board_id
                    .is_some_and(|board_id| board_id != input.item_id)
                    || input.list_id.is_some()
                {
                    bail!(
                        "board target hierarchy does not match item_id {}",
                        input.item_id
                    );
                }
            }
            crate::workspace::links::WorkspaceItemKind::List => {
                let board_id = input
                    .board_id
                    .context("board_id is required for a list target")?;
                if target.board_id != Some(board_id) || input.list_id.is_some() {
                    bail!(
                        "list {} does not belong to board {}",
                        input.item_id,
                        board_id
                    );
                }
            }
            crate::workspace::links::WorkspaceItemKind::Card => {
                let board_id = input
                    .board_id
                    .context("board_id is required for a card target")?;
                let list_id = input
                    .list_id
                    .context("list_id is required for a card target")?;
                if target.board_id != Some(board_id) || target.list_id != Some(list_id) {
                    bail!(
                        "card {} does not belong to board {} and list {}",
                        input.item_id,
                        board_id,
                        list_id
                    );
                }
            }
            crate::workspace::links::WorkspaceItemKind::Note => {
                bail!("note targets are not manual workspace relationships")
            }
        }
        Ok(target.item)
    }

    async fn related_items_for_note(&self, note_id: i64) -> Result<Vec<RelatedItemDetail>> {
        let links =
            crate::workspace::links::load_note_workspace_links(self.db.as_ref(), note_id).await?;
        let mut grouped = HashMap::<
            crate::workspace::links::WorkspaceItemRef,
            (crate::workspace::links::WorkspaceCatalogEntry, Vec<String>),
        >::new();
        for reference in links.references {
            let origin = workspace_origin_label(reference.origin);
            let row = grouped
                .entry(reference.item.item)
                .or_insert_with(|| (reference.item.clone(), Vec::new()));
            if !row.1.iter().any(|existing| existing == origin) {
                row.1.push(origin.to_string());
            }
        }
        let mut details = grouped
            .into_values()
            .map(|(entry, origins)| related_item_detail(entry, origins))
            .collect::<Vec<_>>();
        details.sort_by_key(|detail| (detail.kind.clone(), detail.breadcrumb.to_lowercase()));
        Ok(details)
    }

    async fn related_items_for_workspace_item(
        &self,
        item: crate::workspace::links::WorkspaceItemRef,
    ) -> Result<Vec<RelatedItemDetail>> {
        let related = crate::workspace::links::load_related_notes(self.db.as_ref(), item).await?;
        let catalog =
            crate::workspace::links::load_workspace_link_catalog(self.db.as_ref()).await?;
        Ok(related
            .into_iter()
            .filter_map(|note| {
                let entry = catalog.iter().find(|entry| {
                    entry.item.kind == crate::workspace::links::WorkspaceItemKind::Note
                        && entry.item.id == note.note_id
                })?;
                Some(related_item_detail(
                    entry.clone(),
                    note.origins
                        .into_iter()
                        .map(workspace_origin_label)
                        .map(str::to_string)
                        .collect(),
                ))
            })
            .collect())
    }

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

fn required_text(value: String, field: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(value.to_string())
}

fn validate_due_on(due_on: Option<&str>) -> Result<()> {
    if let Some(due_on) = due_on {
        NaiveDate::parse_from_str(due_on, "%Y-%m-%d")
            .with_context(|| format!("due_on must use YYYY-MM-DD, received {due_on:?}"))?;
    }
    Ok(())
}

fn note_summary(note: note::Model, projects: &HashMap<i64, String>) -> NoteSummary {
    NoteSummary {
        id: note.id,
        title: note.title,
        project_id: note.project_id,
        project_name: note
            .project_id
            .and_then(|project_id| projects.get(&project_id).cloned()),
        is_pinned: note.is_pinned,
        updated_at: note.updated_at,
    }
}

fn label_detail(label: board_label::Model) -> LabelDetail {
    LabelDetail {
        id: label.id,
        board_id: label.board_id,
        name: label.name,
        color: label.color,
    }
}

pub(crate) fn property_definition_detail(
    property: crate::board::properties::PropertyDefinition,
) -> BoardPropertyDefinitionDetail {
    BoardPropertyDefinitionDetail {
        id: property.id,
        board_id: property.board_id,
        name: property.name,
        kind: property.kind.as_str().to_string(),
        position: property.position,
        options: property
            .options
            .into_iter()
            .map(property_option_detail)
            .collect(),
    }
}

pub(crate) fn property_option_detail(
    option: crate::board::properties::PropertyOption,
) -> BoardPropertyOptionDetail {
    BoardPropertyOptionDetail {
        id: option.id,
        name: option.name,
        color: option.color,
        position: option.position,
    }
}

pub(crate) fn property_value_detail(
    value: crate::board::properties::PropertyValue,
) -> BoardPropertyValueDetail {
    match value {
        crate::board::properties::PropertyValue::Text(value) => {
            BoardPropertyValueDetail::Text(value)
        }
        crate::board::properties::PropertyValue::Number(value) => {
            BoardPropertyValueDetail::Number(value)
        }
        crate::board::properties::PropertyValue::Checkbox(value) => {
            BoardPropertyValueDetail::Checkbox(value)
        }
        crate::board::properties::PropertyValue::Date(value) => {
            BoardPropertyValueDetail::Date(value)
        }
        crate::board::properties::PropertyValue::Select(value) => {
            BoardPropertyValueDetail::Select(value)
        }
        crate::board::properties::PropertyValue::Url(value) => BoardPropertyValueDetail::Url(value),
    }
}

pub(crate) fn storage_property_value(
    value: BoardPropertyValueDetail,
) -> crate::board::properties::PropertyValue {
    match value {
        BoardPropertyValueDetail::Text(value) => {
            crate::board::properties::PropertyValue::Text(value)
        }
        BoardPropertyValueDetail::Number(value) => {
            crate::board::properties::PropertyValue::Number(value)
        }
        BoardPropertyValueDetail::Checkbox(value) => {
            crate::board::properties::PropertyValue::Checkbox(value)
        }
        BoardPropertyValueDetail::Date(value) => {
            crate::board::properties::PropertyValue::Date(value)
        }
        BoardPropertyValueDetail::Select(value) => {
            crate::board::properties::PropertyValue::Select(value)
        }
        BoardPropertyValueDetail::Url(value) => crate::board::properties::PropertyValue::Url(value),
    }
}

fn note_link_detail(link: crate::note::links::NoteLinkReference) -> NoteLinkDetail {
    NoteLinkDetail {
        source_note_id: link.source_note_id,
        source_title: link.source_title,
        source_project_name: link.source_project_name,
        target_note_id: link.target_note_id,
        target_title: link.target_title,
        target_project_name: link.target_project_name,
        target_kind: None,
        raw_target: link.raw_target,
        display_text: link.display_text,
        start_byte: link.start_byte,
        end_byte: link.end_byte,
        line_number: link.line_number,
    }
}

fn unresolved_link_detail(link: crate::note::links::UnresolvedLinkReference) -> NoteLinkDetail {
    NoteLinkDetail {
        source_note_id: link.source_note_id,
        source_title: link.source_title,
        source_project_name: link.source_project_name,
        target_note_id: None,
        target_title: None,
        target_project_name: None,
        target_kind: link.target_kind.map(|kind| kind.as_str().to_string()),
        raw_target: link.raw_target,
        display_text: link.display_text,
        start_byte: link.start_byte,
        end_byte: link.end_byte,
        line_number: link.line_number,
    }
}

fn workspace_origin_label(origin: crate::workspace::links::WorkspaceLinkOrigin) -> &'static str {
    match origin {
        crate::workspace::links::WorkspaceLinkOrigin::Manual => "manual",
        crate::workspace::links::WorkspaceLinkOrigin::Wikilink => "wikilink",
        crate::workspace::links::WorkspaceLinkOrigin::Embed => "embed",
    }
}

fn related_item_detail(
    entry: crate::workspace::links::WorkspaceCatalogEntry,
    origins: Vec<String>,
) -> RelatedItemDetail {
    RelatedItemDetail {
        kind: entry.item.kind.as_str().to_string(),
        id: entry.item.id,
        title: entry.title.clone(),
        breadcrumb: entry.breadcrumb(),
        stable_link: entry.stable_link(),
        origins,
    }
}

fn related_note_detail(note: crate::workspace::links::RelatedNote) -> RelatedItemDetail {
    let item = crate::workspace::links::WorkspaceItemRef {
        kind: crate::workspace::links::WorkspaceItemKind::Note,
        id: note.note_id,
    };
    RelatedItemDetail {
        kind: item.kind.as_str().to_string(),
        id: item.id,
        title: note.title.clone(),
        breadcrumb: note
            .project_name
            .as_ref()
            .map(|project| format!("{project} / {}", note.title))
            .unwrap_or_else(|| note.title.clone()),
        stable_link: crate::workspace::links::stable_workspace_link(item, &note.title),
        origins: note
            .origins
            .into_iter()
            .map(workspace_origin_label)
            .map(str::to_string)
            .collect(),
    }
}

fn label_record_detail(label: crate::board::LabelRecord) -> LabelDetail {
    LabelDetail {
        id: i64::from(label.id),
        board_id: i64::from(label.board_id),
        name: label.name,
        color: label.color,
    }
}

fn entry_record_detail(
    entry: crate::board::BoardCardRecord,
    list_title: &str,
    board: &board::Model,
    project_name: Option<String>,
) -> EntryDetail {
    EntryDetail {
        id: i64::from(entry.id),
        title: entry.title,
        description: entry.description,
        due_on: entry.due_on,
        reminder_enabled: entry.reminder_enabled,
        position: entry.position,
        list_id: i64::from(entry.card_id),
        list_title: list_title.to_string(),
        board_id: board.id,
        board_title: board.title.clone(),
        project_id: board.project_id,
        project_name,
        labels: entry.labels.into_iter().map(label_record_detail).collect(),
        checklist_items: entry
            .checklist_items
            .into_iter()
            .map(|item| ChecklistItemDetail {
                id: i64::from(item.id),
                title: item.title,
                checked: item.checked,
                position: item.position,
            })
            .collect(),
        attachments: entry
            .attachments
            .into_iter()
            .map(|attachment| AttachmentDetail {
                id: i64::from(attachment.id),
                file_name: attachment.file_name,
            })
            .collect(),
        related_items: entry
            .related_notes
            .into_iter()
            .map(related_note_detail)
            .collect(),
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
