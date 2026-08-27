use std::{future::Future, pin::Pin, sync::Arc};

use crate::workspace::api::{
    AddChecklistItemInput, BoardPropertyDefinitionDetail, BoardPropertyOptionDetail, BoardSummary,
    ChecklistItemDetail, ClearEntryPropertyInput, CreateBoardInput, CreateBoardLabelInput,
    CreateBoardPropertyInput, CreateBoardPropertyOptionInput, CreateEntryInput, CreateListInput,
    CreateNoteInput, CreateProjectInput, EntryDetail, EntryPropertyValueDetail, LabelDetail,
    ListDetail, MoveEntryInput, MoveNoteInput, NoteDetail, NoteWorkspaceRelationInput,
    ProjectSummary, RelatedItemDetail, RenameBoardInput, RenameListInput, RenameProjectInput,
    SetEntryLabelInput, SetEntryPropertyInput, SetEntryReminderInput, UpdateChecklistItemInput,
    UpdateEntryInput, UpdateNoteInput,
};
use anyhow::Result;
use sea_orm::{ConnectionTrait, DatabaseTransaction, DbBackend, Statement, TransactionTrait};

use crate::store::Store;

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
