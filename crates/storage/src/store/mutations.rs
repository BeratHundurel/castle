use std::{future::Future, pin::Pin, sync::Arc};

use crate::workspace::api::{
    AddChecklistItemInput, BoardPropertyDefinitionDetail, BoardPropertyOptionDetail, BoardSummary,
    ChecklistItemDetail, ClearEntryPropertyInput, CreateBoardInput, CreateBoardLabelInput,
    CreateBoardPropertyInput, CreateBoardPropertyOptionInput, CreateEntryInput, CreateListInput,
    CreateNoteInput, CreateProjectInput, CreateRecurringTaskInput, EntryDetail,
    EntryLifecycleState, EntryPropertyValueDetail, LabelDetail, ListDetail, MoveEntryInput,
    MoveNoteInput, NoteDetail, NoteWorkspaceRelationInput, ProjectSummary, RecurringTaskInput,
    RelatedItemDetail, RenameBoardInput, RenameListInput, RenameProjectInput, RunWorkflowInput,
    SaveWorkflowInput, SetEntryLabelInput, SetEntryLifecycleInput, SetEntryPropertyInput,
    SetEntryReminderInput, SetEntryScheduleInput, SetListWorkflowRoleInput,
    UpdateChecklistItemInput, UpdateEntryInput, UpdateNoteInput, WorkflowInput,
};
use anyhow::{Context as _, Result, bail};
use calendar::{CalendarDate, GenerationMode, RecurrenceRule};
use entity::{
    board_label::Entity as BoardLabel, board_property::Entity as BoardProperty,
    entry_checklist_item::Entity as ChecklistItem,
};
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait, Statement, TransactionTrait,
};

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
        let entry_id = input.entry_id;
        let property_key = BoardProperty::find_by_id(input.property_id)
            .one(&self.store)
            .await?
            .with_context(|| format!("board property {} was not found", input.property_id))?
            .name;
        let detail = self
            .execute(ChangeDomain::Board, move |store| {
                Box::pin(store.set_entry_property(input))
            })
            .await?;
        if let Err(error) = crate::workflow::run_property_changed(
            &self.store,
            entry_id,
            self.workflow_origin(),
            property_key,
        )
        .await
        {
            eprintln!("Failed to run board workflow after changing entry property: {error}");
        }
        Ok(detail)
    }

    pub async fn clear_entry_property(&self, input: ClearEntryPropertyInput) -> Result<()> {
        let entry_id = input.entry_id;
        let property_key = BoardProperty::find_by_id(input.property_id)
            .one(&self.store)
            .await?
            .with_context(|| format!("board property {} was not found", input.property_id))?
            .name;
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.clear_entry_property(input))
        })
        .await?;
        if let Err(error) = crate::workflow::run_property_changed(
            &self.store,
            entry_id,
            self.workflow_origin(),
            property_key,
        )
        .await
        {
            eprintln!("Failed to run board workflow after clearing entry property: {error}");
        }
        Ok(())
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

    pub async fn set_list_workflow_role(
        &self,
        input: SetListWorkflowRoleInput,
    ) -> Result<ListDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.set_list_workflow_role(input))
        })
        .await
    }

    pub async fn create_entry(&self, input: CreateEntryInput) -> Result<EntryDetail> {
        let detail = self
            .execute(ChangeDomain::Link, move |store| {
                Box::pin(store.create_entry(input))
            })
            .await?;
        if let Err(error) =
            crate::workflow::run_created_event(&self.store, detail.id, self.workflow_origin()).await
        {
            eprintln!("Failed to run board workflow after creating entry: {error}");
        }
        Ok(detail)
    }

    pub async fn update_entry(&self, input: UpdateEntryInput) -> Result<EntryDetail> {
        let entry_id = input.entry_id;
        let should_run_due_workflow = input.due_on.is_some() || input.clear_due_on;
        let detail = self
            .execute(ChangeDomain::Link, move |store| {
                Box::pin(store.update_entry(input))
            })
            .await?;
        if should_run_due_workflow
            && let Err(error) =
                crate::workflow::run_due_date_changed(&self.store, entry_id, self.workflow_origin())
                    .await
        {
            eprintln!("Failed to run board workflow after updating entry dates: {error}");
        }
        Ok(detail)
    }

    pub async fn set_entry_schedule(&self, input: SetEntryScheduleInput) -> Result<EntryDetail> {
        let entry_id = input.entry_id;
        let detail = self
            .execute(ChangeDomain::Board, move |store| {
                Box::pin(store.set_entry_schedule(input))
            })
            .await?;
        if let Err(error) =
            crate::workflow::run_due_date_changed(&self.store, entry_id, self.workflow_origin())
                .await
        {
            eprintln!("Failed to run board workflow after changing entry dates: {error}");
        }
        Ok(detail)
    }

    pub async fn set_entry_lifecycle(&self, input: SetEntryLifecycleInput) -> Result<EntryDetail> {
        let entry_id = input.entry_id;
        let state = input.state;
        let detail = self
            .execute(ChangeDomain::Board, move |store| {
                Box::pin(store.set_entry_lifecycle(input))
            })
            .await?;
        if state == EntryLifecycleState::Completed
            && let Ok(recurring) = crate::calendar::get_recurring_task(&self.store, entry_id).await
            && recurring.generation_mode == GenerationMode::OnCompletion
            && let Err(error) =
                crate::calendar::create_next_recurring_instance(&self.store, entry_id).await
        {
            eprintln!("Failed to create next recurring instance: {error}");
        }
        let kind = match state {
            EntryLifecycleState::Open => workflow::WorkflowEventKind::CardReopened,
            EntryLifecycleState::Completed => workflow::WorkflowEventKind::CardCompleted,
            EntryLifecycleState::Cancelled => workflow::WorkflowEventKind::CardCancelled,
        };
        if let Err(error) = crate::workflow::run_entry_event(
            &self.store,
            entry_id,
            match self.origin {
                MutationOrigin::LocalApp => workflow::EventOrigin::User,
                MutationOrigin::ExternalAgent => workflow::EventOrigin::Agent,
            },
            kind,
        )
        .await
        {
            eprintln!("Failed to run board workflow after changing entry lifecycle: {error}");
        }
        Ok(detail)
    }

    pub async fn save_workflow(
        &self,
        input: SaveWorkflowInput,
    ) -> Result<crate::workflow::WorkflowRecord> {
        let definition = serde_json::from_value(input.definition)?;
        let draft = crate::workflow::WorkflowDraft {
            id: input.workflow_id,
            board_id: input.board_id,
            name: input.name,
            enabled: input.enabled,
            definition,
        };
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(crate::workflow::upsert_workflow(store, draft))
        })
        .await
    }

    pub async fn delete_workflow(&self, input: WorkflowInput) -> Result<()> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(crate::workflow::delete_workflow(
                store,
                input.board_id,
                input.workflow_id,
            ))
        })
        .await
    }

    pub async fn run_workflow(
        &self,
        input: RunWorkflowInput,
    ) -> Result<crate::workflow::WorkflowExecutionReport> {
        let report = crate::workflow::run_manual_workflow(
            &self.store,
            input.board_id,
            input.entry_id,
            self.workflow_origin(),
        )
        .await?;
        if self.origin == MutationOrigin::ExternalAgent {
            record_change_in_connection(self.store.db.as_ref(), ChangeDomain::Board).await?;
        }
        Ok(report)
    }

    pub async fn create_recurring_task(
        &self,
        input: CreateRecurringTaskInput,
    ) -> Result<crate::calendar::RecurringTaskRecord> {
        let start_on =
            CalendarDate::parse(&input.start_on).map_err(|error| anyhow::anyhow!(error))?;
        let rule = RecurrenceRule::parse(&input.rule).map_err(|error| anyhow::anyhow!(error))?;
        let until_on = input
            .until_on
            .as_deref()
            .map(CalendarDate::parse)
            .transpose()
            .map_err(|error| anyhow::anyhow!(error))?;
        let generation_mode = match input.generation_mode.as_deref() {
            None | Some("on_completion") => GenerationMode::OnCompletion,
            Some("on_schedule") => GenerationMode::OnSchedule,
            Some(value) => bail!("unknown generation_mode {value:?}"),
        };
        let draft = crate::calendar::RecurringTaskDraft {
            entry_id: input.entry_id,
            start_on,
            rule,
            until_on,
            occurrence_limit: input.occurrence_limit,
            generation_mode,
        };
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(crate::calendar::create_recurring_task(store, draft))
        })
        .await
    }

    pub async fn delete_recurring_task(&self, input: RecurringTaskInput) -> Result<()> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(crate::calendar::delete_recurring_task(
                store,
                input.entry_id,
            ))
        })
        .await
    }

    pub async fn move_entry(&self, input: MoveEntryInput) -> Result<EntryDetail> {
        let move_event = crate::workflow::capture_move_event(
            &self.store,
            input.entry_id,
            input.list_id,
            match self.origin {
                MutationOrigin::LocalApp => workflow::EventOrigin::User,
                MutationOrigin::ExternalAgent => workflow::EventOrigin::Agent,
            },
        )
        .await?;
        let detail = self
            .execute(ChangeDomain::Board, move |store| {
                Box::pin(store.move_entry(input))
            })
            .await?;
        if let Some((event, context)) = move_event
            && let Err(error) = crate::workflow::run_event(&self.store, event, context).await
        {
            eprintln!("Failed to run board workflows after moving entry: {error}");
        }
        Ok(detail)
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
        let should_run_workflow = input.checked.is_some();
        let entry_id = if should_run_workflow {
            Some(
                ChecklistItem::find_by_id(input.item_id)
                    .one(&self.store)
                    .await?
                    .with_context(|| format!("checklist item {} was not found", input.item_id))?
                    .entry_id,
            )
        } else {
            None
        };
        let detail = self
            .execute(ChangeDomain::Board, move |store| {
                Box::pin(store.update_checklist_item(input))
            })
            .await?;
        if let Some(entry_id) = entry_id
            && let Err(error) = crate::workflow::run_checklist_changed(
                &self.store,
                entry_id,
                match self.origin {
                    MutationOrigin::LocalApp => workflow::EventOrigin::User,
                    MutationOrigin::ExternalAgent => workflow::EventOrigin::Agent,
                },
            )
            .await
        {
            eprintln!("Failed to run board workflow after checklist update: {error}");
        }
        Ok(detail)
    }

    pub async fn create_board_label(&self, input: CreateBoardLabelInput) -> Result<LabelDetail> {
        self.execute(ChangeDomain::Board, move |store| {
            Box::pin(store.create_board_label(input))
        })
        .await
    }

    pub async fn set_entry_label(&self, input: SetEntryLabelInput) -> Result<EntryDetail> {
        let entry_id = input.entry_id;
        let assigned = input.assigned;
        let label_name = BoardLabel::find_by_id(input.label_id)
            .one(&self.store)
            .await?
            .with_context(|| format!("board label {} was not found", input.label_id))?
            .name;
        let detail = self
            .execute(ChangeDomain::Board, move |store| {
                Box::pin(store.set_entry_label(input))
            })
            .await?;
        if let Err(error) = crate::workflow::run_label_changed(
            &self.store,
            entry_id,
            self.workflow_origin(),
            label_name,
            assigned,
        )
        .await
        {
            eprintln!("Failed to run board workflow after changing entry label: {error}");
        }
        Ok(detail)
    }
}

impl Mutations {
    fn workflow_origin(&self) -> workflow::EventOrigin {
        match self.origin {
            MutationOrigin::LocalApp => workflow::EventOrigin::User,
            MutationOrigin::ExternalAgent => workflow::EventOrigin::Agent,
        }
    }
}
