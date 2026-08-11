use std::{
    collections::{HashMap, HashSet},
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
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
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectOptions, ConnectionTrait,
    Database, DatabaseConnection, DatabaseTransaction, DbBackend, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Statement, TransactionSession,
    TransactionTrait, sea_query::Expr,
};

use migration::{Migrator, MigratorTrait};

use crate::agent_types::{
    AddChecklistItemInput, AttachmentDetail, BoardDetail, BoardPropertiesDetail,
    BoardPropertyDefinitionDetail, BoardPropertyKindInput, BoardPropertyOptionDetail,
    BoardPropertyValueDetail, BoardSummary, ChecklistItemDetail, ClearEntryPropertyInput,
    CreateBoardInput, CreateBoardLabelInput, CreateBoardPropertyInput,
    CreateBoardPropertyOptionInput, CreateEntryInput, CreateListInput, CreateNoteInput,
    CreateProjectInput, EntryDetail, EntryPropertyValueDetail, LabelDetail, ListDetail,
    MoveEntryInput, MoveNoteInput, NoteDetail, NoteLinkDetail, NoteLinksDetail, NoteSummary,
    NoteWorkspaceRelationInput, ProjectSummary, RelatedItemDetail, RenameBoardInput,
    RenameListInput, RenameProjectInput, SearchEntriesInput, SearchNotesInput, SetEntryLabelInput,
    SetEntryPropertyInput, SetEntryReminderInput, UpdateChecklistItemInput, UpdateEntryInput,
    UpdateNoteInput, WorkspaceItemKindInput, WorkspaceRelationsInput,
};

#[derive(Clone)]
pub struct Store<C = DatabaseConnection> {
    db: Arc<C>,
}

impl From<DatabaseConnection> for Store {
    fn from(db: DatabaseConnection) -> Self {
        Self::from_connection(db)
    }
}

impl From<Arc<DatabaseConnection>> for Store {
    fn from(db: Arc<DatabaseConnection>) -> Self {
        Self::from_shared_connection(db)
    }
}

#[derive(Clone, Debug)]
pub struct StoreOptions {
    database_url: String,
    min_connections: u32,
    max_connections: u32,
}

impl StoreOptions {
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            min_connections: 1,
            max_connections: 4,
        }
    }

    pub fn connection_pool(mut self, min_connections: u32, max_connections: u32) -> Self {
        self.min_connections = min_connections;
        self.max_connections = std::cmp::max(max_connections, min_connections);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MutationOrigin {
    LocalApp,
    ExternalAgent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChangeDomain {
    Workspace,
    Board,
    Note,
    Link,
}

async fn record_change_in_connection(
    db: &impl ConnectionTrait,
    domain: ChangeDomain,
) -> Result<()> {
    let assignments = match domain {
        ChangeDomain::Workspace => "revision = revision + 1",
        ChangeDomain::Board => "revision = revision + 1, board_revision = board_revision + 1",
        ChangeDomain::Note => "revision = revision + 1, note_revision = note_revision + 1",
        ChangeDomain::Link => {
            "revision = revision + 1, board_revision = board_revision + 1, note_revision = note_revision + 1, link_revision = link_revision + 1"
        }
    };
    db.execute_raw(Statement::from_string(
        DbBackend::Sqlite,
        format!("UPDATE castle_change_revision SET {assignments} WHERE id = 1"),
    ))
    .await?;
    Ok(())
}

impl Store {
    pub async fn connect(options: StoreOptions) -> Result<Self> {
        let mut connect_options = ConnectOptions::new(options.database_url);
        connect_options
            .min_connections(options.min_connections)
            .max_connections(options.max_connections);
        let db = Database::connect(connect_options).await?;
        Migrator::up(&db, None).await?;
        Ok(Self { db: Arc::new(db) })
    }

    #[cfg(test)]
    pub(crate) fn new(db: DatabaseConnection) -> Self {
        Self { db: Arc::new(db) }
    }

    pub fn from_connection(db: DatabaseConnection) -> Self {
        Self { db: Arc::new(db) }
    }

    pub fn from_shared_connection(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub fn connection(&self) -> Arc<DatabaseConnection> {
        self.db.clone()
    }

    pub fn mutations(&self, origin: MutationOrigin) -> Mutations {
        Mutations {
            store: self.clone(),
            origin,
        }
    }
}

#[derive(Clone)]
pub struct Mutations {
    store: Store,
    origin: MutationOrigin,
}

type MutationFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

impl Mutations {
    async fn execute<T, F>(&self, domain: ChangeDomain, operation: F) -> Result<T>
    where
        T: Send,
        F: for<'a> FnOnce(&'a Store<DatabaseTransaction>) -> MutationFuture<'a, T>,
    {
        let transaction = Arc::new(self.store.db.as_ref().begin().await?);
        let transactional_store = Store {
            db: transaction.clone(),
        };
        let result = operation(&transactional_store).await?;
        if self.origin == MutationOrigin::ExternalAgent {
            record_change_in_connection(transactional_store.db.as_ref(), domain).await?;
        }
        drop(transactional_store);
        let transaction = Arc::try_unwrap(transaction)
            .map_err(|_| anyhow::anyhow!("storage transaction remained shared after mutation"))?;
        transaction.commit().await?;
        Ok(result)
    }

    pub async fn create_board_property(
        &self,
        input: CreateBoardPropertyInput,
    ) -> Result<BoardPropertyDefinitionDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.create_board_property(input))
        })
        .await
    }

    pub async fn create_board_property_option(
        &self,
        input: CreateBoardPropertyOptionInput,
    ) -> Result<BoardPropertyOptionDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.create_board_property_option(input))
        })
        .await
    }

    pub async fn set_entry_property(
        &self,
        input: SetEntryPropertyInput,
    ) -> Result<EntryPropertyValueDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.set_entry_property(input))
        })
        .await
    }

    pub async fn clear_entry_property(&self, input: ClearEntryPropertyInput) -> Result<()> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.clear_entry_property(input))
        })
        .await
    }

    pub async fn link_note_to_workspace_item(
        &self,
        input: NoteWorkspaceRelationInput,
    ) -> Result<Vec<RelatedItemDetail>> {
        self.execute(ChangeDomain::Link, move |store| {
            Box::pin(store.link_note_to_workspace_item(input))
        })
        .await
    }

    pub async fn unlink_note_from_workspace_item(
        &self,
        input: NoteWorkspaceRelationInput,
    ) -> Result<Vec<RelatedItemDetail>> {
        self.execute(ChangeDomain::Link, move |store| {
            Box::pin(store.unlink_note_from_workspace_item(input))
        })
        .await
    }

    pub async fn create_note(&self, input: CreateNoteInput) -> Result<NoteDetail> {
        self.execute(ChangeDomain::Link, move |store| {
            Box::pin(store.create_note(input))
        })
        .await
    }

    pub async fn update_note(&self, input: UpdateNoteInput) -> Result<NoteDetail> {
        self.execute(ChangeDomain::Link, move |store| {
            Box::pin(store.update_note(input))
        })
        .await
    }

    pub async fn move_note(&self, input: MoveNoteInput) -> Result<NoteDetail> {
        self.execute(ChangeDomain::Note, move |store| {
            Box::pin(store.move_note(input))
        })
        .await
    }

    pub async fn create_project(&self, input: CreateProjectInput) -> Result<ProjectSummary> {
        self.execute(ChangeDomain::Workspace, move |store| {
            Box::pin(store.create_project(input))
        })
        .await
    }

    pub async fn rename_project(&self, input: RenameProjectInput) -> Result<ProjectSummary> {
        self.execute(ChangeDomain::Workspace, move |store| {
            Box::pin(store.rename_project(input))
        })
        .await
    }

    pub async fn create_board(&self, input: CreateBoardInput) -> Result<BoardSummary> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.create_board(input))
        })
        .await
    }

    pub async fn rename_board(&self, input: RenameBoardInput) -> Result<BoardSummary> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.rename_board(input))
        })
        .await
    }

    pub async fn create_list(&self, input: CreateListInput) -> Result<ListDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.create_list(input))
        })
        .await
    }

    pub async fn rename_list(&self, input: RenameListInput) -> Result<ListDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.rename_list(input))
        })
        .await
    }

    pub async fn create_entry(&self, input: CreateEntryInput) -> Result<EntryDetail> {
        self.execute(ChangeDomain::Link, move |store| {
            Box::pin(store.create_entry(input))
        })
        .await
    }

    pub async fn update_entry(&self, input: UpdateEntryInput) -> Result<EntryDetail> {
        self.execute(ChangeDomain::Link, move |store| {
            Box::pin(store.update_entry(input))
        })
        .await
    }

    pub async fn move_entry(&self, input: MoveEntryInput) -> Result<EntryDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.move_entry(input))
        })
        .await
    }

    pub async fn set_entry_reminder(&self, input: SetEntryReminderInput) -> Result<EntryDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.set_entry_reminder(input))
        })
        .await
    }

    pub async fn add_checklist_item(
        &self,
        input: AddChecklistItemInput,
    ) -> Result<ChecklistItemDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.add_checklist_item(input))
        })
        .await
    }

    pub async fn update_checklist_item(
        &self,
        input: UpdateChecklistItemInput,
    ) -> Result<ChecklistItemDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.update_checklist_item(input))
        })
        .await
    }

    pub async fn create_board_label(&self, input: CreateBoardLabelInput) -> Result<LabelDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.create_board_label(input))
        })
        .await
    }

    pub async fn set_entry_label(&self, input: SetEntryLabelInput) -> Result<EntryDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.set_entry_label(input))
        })
        .await
    }
}

impl<C> Store<C>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    pub async fn board_properties(&self, board_id: i64) -> Result<BoardPropertiesDetail> {
        let properties =
            crate::board_properties::load_board_properties(self.db.as_ref(), board_id).await?;
        Ok(BoardPropertiesDetail {
            definitions: properties
                .definitions
                .into_iter()
                .map(property_definition_detail)
                .collect(),
            values: properties
                .values
                .into_iter()
                .map(|value| EntryPropertyValueDetail {
                    entry_id: value.entry_id,
                    property_id: value.property_id,
                    value: property_value_detail(value.value),
                })
                .collect(),
        })
    }

    pub async fn create_board_property(
        &self,
        input: CreateBoardPropertyInput,
    ) -> Result<BoardPropertyDefinitionDetail> {
        let kind = match input.kind {
            BoardPropertyKindInput::Text => crate::board_properties::PropertyKind::Text,
            BoardPropertyKindInput::Number => crate::board_properties::PropertyKind::Number,
            BoardPropertyKindInput::Checkbox => crate::board_properties::PropertyKind::Checkbox,
            BoardPropertyKindInput::Date => crate::board_properties::PropertyKind::Date,
            BoardPropertyKindInput::Select => crate::board_properties::PropertyKind::Select,
            BoardPropertyKindInput::Url => crate::board_properties::PropertyKind::Url,
        };
        crate::board_properties::create_property(self.db.as_ref(), input.board_id, input.name, kind)
            .await
            .map(property_definition_detail)
    }

    pub async fn create_board_property_option(
        &self,
        input: CreateBoardPropertyOptionInput,
    ) -> Result<BoardPropertyOptionDetail> {
        crate::board_properties::create_property_option(
            self.db.as_ref(),
            input.property_id,
            input.name,
            input.color,
        )
        .await
        .map(property_option_detail)
    }

    pub async fn set_entry_property(
        &self,
        input: SetEntryPropertyInput,
    ) -> Result<EntryPropertyValueDetail> {
        let value = storage_property_value(input.value);
        crate::board_properties::set_entry_property(
            self.db.as_ref(),
            input.entry_id,
            input.property_id,
            value,
        )
        .await
        .map(|value| EntryPropertyValueDetail {
            entry_id: value.entry_id,
            property_id: value.property_id,
            value: property_value_detail(value.value),
        })
    }

    pub async fn clear_entry_property(&self, input: ClearEntryPropertyInput) -> Result<()> {
        crate::board_properties::clear_entry_property(
            self.db.as_ref(),
            input.entry_id,
            input.property_id,
        )
        .await
    }

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
        let links = crate::note_links::load_note_links(self.db.as_ref(), note_id).await?;
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
        Ok(crate::workspace_links::set_manual_note_link(
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
        Ok(crate::workspace_links::set_manual_note_link(
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
        crate::note_links::index_note_links_in_connection(
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
            crate::note_links::index_note_links_in_connection(
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
        let board_item = crate::workspace_links::WorkspaceItemRef {
            kind: crate::workspace_links::WorkspaceItemKind::Board,
            id: board.id,
        };
        let mut relation_items = Vec::with_capacity(snapshot.cards.len() + 1);
        relation_items.push(board_item);
        relation_items.extend(snapshot.cards.iter().map(|list| {
            crate::workspace_links::WorkspaceItemRef {
                kind: crate::workspace_links::WorkspaceItemKind::List,
                id: i64::from(list.id),
            }
        }));
        let mut related_notes =
            crate::workspace_links::load_related_notes_for_items(self.db.as_ref(), &relation_items)
                .await?;

        let details = snapshot
            .cards
            .into_iter()
            .map(|list| {
                let list_item = crate::workspace_links::WorkspaceItemRef {
                    kind: crate::workspace_links::WorkspaceItemKind::List,
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
        let position = Project::find().count(self.db.as_ref()).await? as i32;
        let project = project::ActiveModel {
            name: Set(name),
            archived: Set(false),
            position: Set(position),
            ..Default::default()
        }
        .insert(self.db.as_ref())
        .await?;
        Ok(ProjectSummary {
            id: project.id,
            name: project.name,
            position: project.position,
            board_count: 0,
        })
    }

    pub async fn rename_project(&self, input: RenameProjectInput) -> Result<ProjectSummary> {
        let project = self.active_project(input.project_id).await?;
        project::ActiveModel {
            id: Set(project.id),
            name: Set(required_text(input.name, "project name")?),
            ..Default::default()
        }
        .update(self.db.as_ref())
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
        let board = board::ActiveModel {
            title: Set(title),
            project_id: Set(input.project_id),
            ..Default::default()
        }
        .insert(self.db.as_ref())
        .await?;
        Ok(BoardSummary {
            id: board.id,
            title: board.title,
            project_id: board.project_id,
            project_name,
        })
    }

    pub async fn rename_board(&self, input: RenameBoardInput) -> Result<BoardSummary> {
        let board = self.active_board(input.board_id).await?;
        let board = board::ActiveModel {
            id: Set(board.id),
            title: Set(required_text(input.title, "board title")?),
            ..Default::default()
        }
        .update(self.db.as_ref())
        .await?;
        let project_name = match board.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        Ok(BoardSummary {
            id: board.id,
            title: board.title,
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
        let list = card::ActiveModel {
            title: Set(title),
            board_id: Set(input.board_id),
            position: Set(position),
            ..Default::default()
        }
        .insert(self.db.as_ref())
        .await?;
        Ok(ListDetail {
            id: list.id,
            title: list.title,
            position: list.position,
            entries: Vec::new(),
            related_items: Vec::new(),
        })
    }

    pub async fn rename_list(&self, input: RenameListInput) -> Result<ListDetail> {
        let list = self.active_list(input.list_id).await?;
        let list = card::ActiveModel {
            id: Set(list.id),
            title: Set(required_text(input.title, "list title")?),
            ..Default::default()
        }
        .update(self.db.as_ref())
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
        let description = input.description;
        let txn = self.db.as_ref().begin().await?;
        let entry = entry::ActiveModel {
            title: Set(title),
            description: Set(description.clone()),
            card_id: Set(input.list_id),
            position: Set(position),
            due_on: Set(input.due_on),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
        crate::workspace_links::index_entry_workspace_links_in_connection(
            &txn,
            entry.id,
            &description,
            now_ts(),
        )
        .await?;
        txn.commit().await?;
        self.entry_detail(entry).await
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
        let mut active: entry::ActiveModel = entry.into();
        if let Some(title) = input.title {
            active.title = Set(required_text(title, "entry title")?);
        }
        if let Some(description) = input.description {
            active.description = Set(description);
        }
        if input.clear_due_on {
            active.due_on = Set(None);
        } else if let Some(due_on) = input.due_on {
            active.due_on = Set(Some(due_on));
        }
        let txn = self.db.as_ref().begin().await?;
        let entry = active.update(&txn).await?;
        crate::workspace_links::index_entry_workspace_links_in_connection(
            &txn,
            entry.id,
            &entry.description,
            now_ts(),
        )
        .await?;
        txn.commit().await?;
        self.entry_detail(entry).await
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
        let entry = entry::ActiveModel {
            id: Set(entry.id),
            reminder_enabled: Set(input.enabled),
            reminder_notified_for: Set(None),
            ..Default::default()
        }
        .update(self.db.as_ref())
        .await?;
        self.entry_detail(entry).await
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
        let item = entry_checklist_item::ActiveModel {
            entry_id: Set(input.entry_id),
            title: Set(required_text(input.title, "checklist item title")?),
            checked: Set(false),
            position: Set(position),
            ..Default::default()
        }
        .insert(self.db.as_ref())
        .await?;
        Ok(ChecklistItemDetail {
            id: item.id,
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
        let mut active: entry_checklist_item::ActiveModel = item.into();
        if let Some(title) = input.title {
            active.title = Set(required_text(title, "checklist item title")?);
        }
        if let Some(checked) = input.checked {
            active.checked = Set(checked);
        }
        let item = active.update(self.db.as_ref()).await?;
        Ok(ChecklistItemDetail {
            id: item.id,
            title: item.title,
            checked: item.checked,
            position: item.position,
        })
    }

    pub async fn create_board_label(&self, input: CreateBoardLabelInput) -> Result<LabelDetail> {
        self.active_board(input.board_id).await?;
        let label = board_label::ActiveModel {
            board_id: Set(input.board_id),
            name: Set(required_text(input.name, "label name")?),
            color: Set(required_text(input.color, "label color")?),
            ..Default::default()
        }
        .insert(self.db.as_ref())
        .await?;
        Ok(label_detail(label))
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
        let existing = EntryLabel::find()
            .filter(entry_label::Column::EntryId.eq(input.entry_id))
            .filter(entry_label::Column::BoardLabelId.eq(input.label_id))
            .one(self.db.as_ref())
            .await?;
        match (input.assigned, existing) {
            (true, None) => {
                entry_label::ActiveModel {
                    entry_id: Set(input.entry_id),
                    board_label_id: Set(input.label_id),
                    ..Default::default()
                }
                .insert(self.db.as_ref())
                .await?;
            }
            (false, Some(association)) => {
                EntryLabel::delete_by_id(association.id)
                    .exec(self.db.as_ref())
                    .await?;
            }
            _ => {}
        }
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
    ) -> Result<crate::workspace_links::WorkspaceItemRef> {
        let kind = match input.kind {
            WorkspaceItemKindInput::Board => crate::workspace_links::WorkspaceItemKind::Board,
            WorkspaceItemKindInput::List => crate::workspace_links::WorkspaceItemKind::List,
            WorkspaceItemKindInput::Card => crate::workspace_links::WorkspaceItemKind::Card,
        };
        let catalog = crate::workspace_links::load_workspace_link_catalog(self.db.as_ref()).await?;
        let target = catalog
            .iter()
            .find(|entry| entry.item.kind == kind && entry.item.id == input.item_id)
            .with_context(|| format!("active {} {} was not found", kind.as_str(), input.item_id))?;
        match kind {
            crate::workspace_links::WorkspaceItemKind::Board => {
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
            crate::workspace_links::WorkspaceItemKind::List => {
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
            crate::workspace_links::WorkspaceItemKind::Card => {
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
            crate::workspace_links::WorkspaceItemKind::Note => {
                bail!("note targets are not manual workspace relationships")
            }
        }
        Ok(target.item)
    }

    async fn related_items_for_note(&self, note_id: i64) -> Result<Vec<RelatedItemDetail>> {
        let links =
            crate::workspace_links::load_note_workspace_links(self.db.as_ref(), note_id).await?;
        let mut grouped = HashMap::<
            crate::workspace_links::WorkspaceItemRef,
            (crate::workspace_links::WorkspaceCatalogEntry, Vec<String>),
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
        item: crate::workspace_links::WorkspaceItemRef,
    ) -> Result<Vec<RelatedItemDetail>> {
        let related = crate::workspace_links::load_related_notes(self.db.as_ref(), item).await?;
        let catalog = crate::workspace_links::load_workspace_link_catalog(self.db.as_ref()).await?;
        Ok(related
            .into_iter()
            .filter_map(|note| {
                let entry = catalog.iter().find(|entry| {
                    entry.item.kind == crate::workspace_links::WorkspaceItemKind::Note
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
            .related_items_for_workspace_item(crate::workspace_links::WorkspaceItemRef {
                kind: crate::workspace_links::WorkspaceItemKind::Card,
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

fn property_definition_detail(
    property: crate::board_properties::PropertyDefinition,
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

fn property_option_detail(
    option: crate::board_properties::PropertyOption,
) -> BoardPropertyOptionDetail {
    BoardPropertyOptionDetail {
        id: option.id,
        name: option.name,
        color: option.color,
        position: option.position,
    }
}

fn property_value_detail(
    value: crate::board_properties::PropertyValue,
) -> BoardPropertyValueDetail {
    match value {
        crate::board_properties::PropertyValue::Text(value) => {
            BoardPropertyValueDetail::Text(value)
        }
        crate::board_properties::PropertyValue::Number(value) => {
            BoardPropertyValueDetail::Number(value)
        }
        crate::board_properties::PropertyValue::Checkbox(value) => {
            BoardPropertyValueDetail::Checkbox(value)
        }
        crate::board_properties::PropertyValue::Date(value) => {
            BoardPropertyValueDetail::Date(value)
        }
        crate::board_properties::PropertyValue::Select(value) => {
            BoardPropertyValueDetail::Select(value)
        }
        crate::board_properties::PropertyValue::Url(value) => BoardPropertyValueDetail::Url(value),
    }
}

fn storage_property_value(
    value: BoardPropertyValueDetail,
) -> crate::board_properties::PropertyValue {
    match value {
        BoardPropertyValueDetail::Text(value) => {
            crate::board_properties::PropertyValue::Text(value)
        }
        BoardPropertyValueDetail::Number(value) => {
            crate::board_properties::PropertyValue::Number(value)
        }
        BoardPropertyValueDetail::Checkbox(value) => {
            crate::board_properties::PropertyValue::Checkbox(value)
        }
        BoardPropertyValueDetail::Date(value) => {
            crate::board_properties::PropertyValue::Date(value)
        }
        BoardPropertyValueDetail::Select(value) => {
            crate::board_properties::PropertyValue::Select(value)
        }
        BoardPropertyValueDetail::Url(value) => crate::board_properties::PropertyValue::Url(value),
    }
}

fn note_link_detail(link: crate::note_links::NoteLinkReference) -> NoteLinkDetail {
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

fn unresolved_link_detail(link: crate::note_links::UnresolvedLinkReference) -> NoteLinkDetail {
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

fn workspace_origin_label(origin: crate::workspace_links::WorkspaceLinkOrigin) -> &'static str {
    match origin {
        crate::workspace_links::WorkspaceLinkOrigin::Manual => "manual",
        crate::workspace_links::WorkspaceLinkOrigin::Wikilink => "wikilink",
        crate::workspace_links::WorkspaceLinkOrigin::Embed => "embed",
    }
}

fn related_item_detail(
    entry: crate::workspace_links::WorkspaceCatalogEntry,
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

fn related_note_detail(note: crate::workspace_links::RelatedNote) -> RelatedItemDetail {
    let item = crate::workspace_links::WorkspaceItemRef {
        kind: crate::workspace_links::WorkspaceItemKind::Note,
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
        stable_link: crate::workspace_links::stable_workspace_link(item, &note.title),
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
    entry: crate::board::EntryRecord,
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
mod tests {
    use super::*;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    async fn store() -> Result<Store> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        Ok(Store::new(db))
    }

    #[tokio::test]
    async fn creates_and_moves_a_complete_board_hierarchy() -> Result<()> {
        let store = store().await?;
        let project = store
            .create_project(CreateProjectInput {
                name: "Agent work".to_string(),
            })
            .await?;
        let board = store
            .create_board(CreateBoardInput {
                title: "Delivery".to_string(),
                project_id: Some(project.id),
            })
            .await?;
        let first_list = store
            .create_list(CreateListInput {
                board_id: board.id,
                title: "Ideas".to_string(),
            })
            .await?;
        let second_list = store
            .create_list(CreateListInput {
                board_id: board.id,
                title: "Selected".to_string(),
            })
            .await?;
        let entry = store
            .create_entry(CreateEntryInput {
                list_id: first_list.id,
                title: "Write MCP tests".to_string(),
                description: "Cover the full hierarchy".to_string(),
                due_on: Some("2026-07-24".to_string()),
            })
            .await?;
        let reminder = store
            .set_entry_reminder(SetEntryReminderInput {
                entry_id: entry.id,
                enabled: true,
            })
            .await?;
        assert!(reminder.reminder_enabled);
        let checklist_item = store
            .add_checklist_item(AddChecklistItemInput {
                entry_id: entry.id,
                title: "Run the suite".to_string(),
            })
            .await?;
        store
            .update_checklist_item(UpdateChecklistItemInput {
                item_id: checklist_item.id,
                title: None,
                checked: Some(true),
            })
            .await?;
        let label = store
            .create_board_label(CreateBoardLabelInput {
                board_id: board.id,
                name: "Agent".to_string(),
                color: "blue".to_string(),
            })
            .await?;
        store
            .set_entry_label(SetEntryLabelInput {
                entry_id: entry.id,
                label_id: label.id,
                assigned: true,
            })
            .await?;
        let note = store
            .create_note(CreateNoteInput {
                title: "Delivery context".to_string(),
                content: String::new(),
                project_id: Some(project.id),
            })
            .await?;
        store
            .link_note_to_workspace_item(NoteWorkspaceRelationInput {
                note_id: note.id,
                kind: WorkspaceItemKindInput::Card,
                item_id: entry.id,
                board_id: Some(board.id),
                list_id: Some(first_list.id),
            })
            .await?;
        entry_attachment::ActiveModel {
            entry_id: Set(entry.id),
            file_name: Set("context.png".to_string()),
            ..Default::default()
        }
        .insert(store.db.as_ref())
        .await?;
        let property = store
            .create_board_property(CreateBoardPropertyInput {
                board_id: board.id,
                name: "Estimate".to_string(),
                kind: BoardPropertyKindInput::Number,
            })
            .await?;
        store
            .set_entry_property(SetEntryPropertyInput {
                entry_id: entry.id,
                property_id: property.id,
                value: BoardPropertyValueDetail::Number(3.5),
            })
            .await?;
        let properties = store.board_properties(board.id).await?;
        assert_eq!(properties.definitions[0].name, "Estimate");
        assert!(matches!(
            properties.values[0].value,
            BoardPropertyValueDetail::Number(value) if value == 3.5
        ));

        let matches = store
            .search_entries(SearchEntriesInput {
                query: "MCP".to_string(),
                project_id: Some(project.id),
                board_id: None,
                limit: None,
            })
            .await?;
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, entry.id);
        assert_eq!(matches[0].checklist_items.len(), 1);
        assert!(matches[0].checklist_items[0].checked);
        assert_eq!(matches[0].labels[0].name, "Agent");

        let moved = store
            .move_entry(MoveEntryInput {
                entry_id: entry.id,
                list_id: second_list.id,
            })
            .await?;
        assert_eq!(moved.list_title, "Selected");
        let board_detail = store.get_board(board.id).await?;
        let moved_entry = &board_detail.lists[1].entries[0];
        assert_eq!(moved_entry.labels[0].name, "Agent");
        assert!(moved_entry.checklist_items[0].checked);
        assert_eq!(moved_entry.attachments[0].file_name, "context.png");
        assert_eq!(moved_entry.related_items[0].id, note.id);
        assert_eq!(
            moved_entry.related_items[0].breadcrumb,
            "Agent work / Delivery context"
        );
        Ok(())
    }

    #[tokio::test]
    async fn creates_searches_updates_and_moves_notes() -> Result<()> {
        let store = store().await?;
        let project = store
            .create_project(CreateProjectInput {
                name: "Research".to_string(),
            })
            .await?;
        let created = store
            .create_note(CreateNoteInput {
                title: "MCP ideas".to_string(),
                content: "# Ideas\n\nAdd note tools.".to_string(),
                project_id: Some(project.id),
            })
            .await?;

        let matches = store
            .search_notes(SearchNotesInput {
                query: "note tools".to_string(),
                project_id: Some(project.id),
                limit: None,
            })
            .await?;
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, created.id);

        let updated = store
            .update_note(UpdateNoteInput {
                note_id: created.id,
                title: Some("MCP roadmap".to_string()),
                content: Some("# Roadmap\n\nNotes are supported.".to_string()),
                is_pinned: Some(true),
                expected_updated_at: Some(created.updated_at),
            })
            .await?;
        assert_eq!(updated.title, "MCP roadmap");
        assert!(updated.is_pinned);
        assert!(updated.updated_at > created.updated_at);

        let standalone = store
            .move_note(MoveNoteInput {
                note_id: created.id,
                project_id: None,
            })
            .await?;
        assert_eq!(standalone.project_id, None);
        assert_eq!(standalone.content, "# Roadmap\n\nNotes are supported.");

        let missing = store
            .create_note(CreateNoteInput {
                title: "Missing target".to_string(),
                content: "See [[card:999|Unavailable card]]".to_string(),
                project_id: None,
            })
            .await?;
        let links = store.get_note_links(missing.id).await?;
        assert_eq!(links.unresolved.len(), 1);
        assert_eq!(links.unresolved[0].target_kind.as_deref(), Some("card"));
        assert_eq!(links.unresolved[0].start_byte, 4);
        assert_eq!(links.unresolved[0].end_byte, 33);
        Ok(())
    }

    #[tokio::test]
    async fn workspace_relations_validate_hierarchy_and_reindex_card_descriptions() -> Result<()> {
        let store = store().await?;
        let board = store
            .create_board(CreateBoardInput {
                title: "Roadmap".to_string(),
                project_id: None,
            })
            .await?;
        let list = store
            .create_list(CreateListInput {
                board_id: board.id,
                title: "Current".to_string(),
            })
            .await?;
        let card = store
            .create_entry(CreateEntryInput {
                list_id: list.id,
                title: "Research API".to_string(),
                description: String::new(),
                due_on: None,
            })
            .await?;
        let note = store
            .create_note(CreateNoteInput {
                title: "API research".to_string(),
                content: String::new(),
                project_id: None,
            })
            .await?;
        let relation = NoteWorkspaceRelationInput {
            note_id: note.id,
            kind: WorkspaceItemKindInput::Card,
            item_id: card.id,
            board_id: Some(board.id),
            list_id: Some(list.id),
        };
        let related = store
            .link_note_to_workspace_item(NoteWorkspaceRelationInput { ..relation })
            .await?;
        assert_eq!(related.len(), 1);
        assert!(related[0].origins.iter().any(|origin| origin == "manual"));

        let invalid = store
            .link_note_to_workspace_item(NoteWorkspaceRelationInput {
                board_id: Some(board.id + 1),
                ..relation
            })
            .await;
        assert!(invalid.is_err());

        store
            .update_entry(UpdateEntryInput {
                entry_id: card.id,
                title: None,
                description: Some(format!("See [[note:{}|API research]]", note.id)),
                due_on: None,
                clear_due_on: false,
            })
            .await?;
        let related = store
            .unlink_note_from_workspace_item(NoteWorkspaceRelationInput { ..relation })
            .await?;
        assert_eq!(related.len(), 1);
        assert!(related[0].origins.iter().any(|origin| origin == "wikilink"));
        Ok(())
    }

    #[tokio::test]
    async fn local_mutations_do_not_bump_and_external_mutations_bump_the_owned_domain() -> Result<()>
    {
        let store = store().await?;
        let project = store
            .mutations(MutationOrigin::LocalApp)
            .create_project(CreateProjectInput {
                name: "Revision".to_string(),
            })
            .await?;
        let note = store
            .mutations(MutationOrigin::LocalApp)
            .create_note(CreateNoteInput {
                title: "Watcher regression".to_string(),
                content: String::new(),
                project_id: Some(project.id),
            })
            .await?;
        store
            .db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE note SET last_opened_at = ? WHERE id = ?",
                [123_i64.into(), note.id.into()],
            ))
            .await?;

        let row = change_revision_row(&store).await?;
        assert_eq!(row.try_get::<i64>("", "revision")?, 0);
        assert_eq!(row.try_get::<i64>("", "board_revision")?, 0);
        assert_eq!(row.try_get::<i64>("", "note_revision")?, 0);

        store
            .mutations(MutationOrigin::ExternalAgent)
            .move_note(MoveNoteInput {
                note_id: note.id,
                project_id: None,
            })
            .await?;
        let row = change_revision_row(&store).await?;
        assert_eq!(row.try_get::<i64>("", "revision")?, 1);
        assert_eq!(row.try_get::<i64>("", "board_revision")?, 0);
        assert_eq!(row.try_get::<i64>("", "note_revision")?, 1);
        Ok(())
    }

    #[tokio::test]
    async fn external_commands_encode_their_revision_domains_once() -> Result<()> {
        let store = store().await?;
        let mutations = store.mutations(MutationOrigin::ExternalAgent);
        let project = mutations
            .create_project(CreateProjectInput {
                name: "Domains".to_string(),
            })
            .await?;
        assert_revisions(&store, (1, 0, 0, 0)).await?;

        let board = mutations
            .create_board(CreateBoardInput {
                title: "Board".to_string(),
                project_id: Some(project.id),
            })
            .await?;
        assert_revisions(&store, (2, 1, 0, 0)).await?;

        let list = mutations
            .create_list(CreateListInput {
                board_id: board.id,
                title: "List".to_string(),
            })
            .await?;
        assert_revisions(&store, (3, 2, 0, 0)).await?;

        mutations
            .create_entry(CreateEntryInput {
                list_id: list.id,
                title: "Linked domain".to_string(),
                description: String::new(),
                due_on: None,
            })
            .await?;
        assert_revisions(&store, (4, 3, 1, 1)).await?;
        Ok(())
    }

    #[tokio::test]
    async fn failed_revision_bump_rolls_back_the_data_mutation() -> Result<()> {
        let store = store().await?;
        store
            .db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "CREATE TRIGGER fail_revision_bump BEFORE UPDATE ON castle_change_revision BEGIN SELECT RAISE(ABORT, 'forced revision failure'); END",
            ))
            .await?;

        let result = store
            .mutations(MutationOrigin::ExternalAgent)
            .create_project(CreateProjectInput {
                name: "Must roll back".to_string(),
            })
            .await;

        assert!(result.is_err());
        assert_eq!(Project::find().count(store.db.as_ref()).await?, 0);
        let row = change_revision_row(&store).await?;
        assert_eq!(row.try_get::<i64>("", "revision")?, 0);
        Ok(())
    }

    async fn change_revision_row(store: &Store) -> Result<sea_orm::QueryResult> {
        let row = store
            .db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT revision, board_revision, note_revision, link_revision FROM castle_change_revision WHERE id = 1",
            ))
            .await?
            .context("revision row was not found")?;
        Ok(row)
    }

    async fn assert_revisions(store: &Store, expected: (i64, i64, i64, i64)) -> Result<()> {
        let row = change_revision_row(store).await?;
        assert_eq!(row.try_get::<i64>("", "revision")?, expected.0);
        assert_eq!(row.try_get::<i64>("", "board_revision")?, expected.1);
        assert_eq!(row.try_get::<i64>("", "note_revision")?, expected.2);
        assert_eq!(row.try_get::<i64>("", "link_revision")?, expected.3);
        Ok(())
    }
}
