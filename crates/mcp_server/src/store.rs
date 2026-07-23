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
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait,
    DatabaseConnection, DbBackend, EntityTrait, ExprTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Statement, TransactionTrait, sea_query::Expr,
};

use crate::types::{
    AddChecklistItemInput, AttachmentDetail, BoardDetail, BoardSummary, ChecklistItemDetail,
    CreateBoardInput, CreateBoardLabelInput, CreateListInput, CreateNoteInput, CreateProjectInput,
    CreateTodoInput, LabelDetail, ListDetail, MoveNoteInput, MoveTodoInput, NoteDetail,
    NoteSummary, ProjectSummary, RenameBoardInput, RenameListInput, RenameProjectInput,
    SearchNotesInput, SearchTodosInput, SetTodoLabelInput, SetTodoReminderInput, TodoDetail,
    UpdateChecklistItemInput, UpdateNoteInput, UpdateTodoInput,
};

#[derive(Clone)]
pub(crate) struct CastleStore {
    db: DatabaseConnection,
}

#[derive(Clone, Copy)]
pub(crate) enum ChangeDomain {
    Workspace,
    Board,
    Note,
}

impl CastleStore {
    pub(crate) fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub(crate) async fn record_external_change(&self, domain: ChangeDomain) -> Result<()> {
        let assignments = match domain {
            ChangeDomain::Workspace => "revision = revision + 1",
            ChangeDomain::Board => "revision = revision + 1, board_revision = board_revision + 1",
            ChangeDomain::Note => "revision = revision + 1, note_revision = note_revision + 1",
        };
        self.db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                format!("UPDATE castle_change_revision SET {assignments} WHERE id = 1"),
            ))
            .await?;
        Ok(())
    }

    pub(crate) async fn list_projects(&self) -> Result<Vec<ProjectSummary>> {
        let projects = Project::find()
            .filter(project::Column::Archived.eq(false))
            .filter(project::Column::DeletedAt.is_null())
            .order_by_asc(project::Column::Position)
            .order_by_asc(project::Column::Id)
            .all(&self.db)
            .await?;

        let board_counts = Board::find()
            .filter(board::Column::DeletedAt.is_null())
            .all(&self.db)
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

    pub(crate) async fn list_boards(&self, project_id: Option<i64>) -> Result<Vec<BoardSummary>> {
        if let Some(project_id) = project_id {
            self.active_project(project_id).await?;
        }
        let mut query = Board::find().filter(board::Column::DeletedAt.is_null());
        if let Some(project_id) = project_id {
            query = query.filter(board::Column::ProjectId.eq(project_id));
        }
        let boards = query.order_by_asc(board::Column::Id).all(&self.db).await?;
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

    pub(crate) async fn list_notes(
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
            .all(&self.db)
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

    pub(crate) async fn get_note(&self, note_id: i64) -> Result<NoteDetail> {
        let note = self.active_note(note_id).await?;
        self.note_detail(note).await
    }

    pub(crate) async fn search_notes(&self, input: SearchNotesInput) -> Result<Vec<NoteSummary>> {
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
            .all(&self.db)
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

    pub(crate) async fn create_note(&self, input: CreateNoteInput) -> Result<NoteDetail> {
        let title = required_text(input.title, "note title")?;
        let project_name = match input.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        let now = now_ts();
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
        .insert(&self.db)
        .await?;
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
        })
    }

    pub(crate) async fn update_note(&self, input: UpdateNoteInput) -> Result<NoteDetail> {
        if input.title.is_none() && input.content.is_none() && input.is_pinned.is_none() {
            bail!("provide title, content, or is_pinned to update the note");
        }
        let note = self.active_note(input.note_id).await?;
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

        let current_updated_at = note.updated_at;
        let mut active: note::ActiveModel = note.into();
        if let Some(title) = input.title {
            active.title = Set(required_text(title, "note title")?);
        }
        if let Some(content) = input.content {
            active.cached_content = Set(content);
            active.file_missing_since = Set(None);
        }
        if let Some(is_pinned) = input.is_pinned {
            active.is_pinned = Set(is_pinned);
        }
        active.updated_at = Set(next_updated_at(current_updated_at));
        let note = active.update(&self.db).await?;
        self.note_detail(note).await
    }

    pub(crate) async fn move_note(&self, input: MoveNoteInput) -> Result<NoteDetail> {
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
        .update(&self.db)
        .await?;
        self.note_detail(note).await
    }

    pub(crate) async fn get_board(&self, board_id: i64) -> Result<BoardDetail> {
        let board = self.active_board(board_id).await?;
        let project_name = match board.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        let labels = BoardLabel::find()
            .filter(board_label::Column::BoardId.eq(board.id))
            .order_by_asc(board_label::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(label_detail)
            .collect();

        let lists = Card::find()
            .filter(card::Column::BoardId.eq(board.id))
            .filter(card::Column::DeletedAt.is_null())
            .order_by_asc(card::Column::Position)
            .order_by_asc(card::Column::Id)
            .all(&self.db)
            .await?;

        let mut details = Vec::with_capacity(lists.len());
        for list in lists {
            let todo_models = Entry::find()
                .filter(entry::Column::CardId.eq(list.id))
                .filter(entry::Column::DeletedAt.is_null())
                .order_by_asc(entry::Column::Position)
                .order_by_asc(entry::Column::Id)
                .all(&self.db)
                .await?;

            let mut todos = Vec::with_capacity(todo_models.len());
            for todo in todo_models {
                todos.push(
                    self.todo_detail_with_context(todo, &list, &board, project_name.clone())
                        .await?,
                );
            }
            details.push(ListDetail {
                id: list.id,
                title: list.title,
                position: list.position,
                todos,
            });
        }

        Ok(BoardDetail {
            id: board.id,
            title: board.title,
            project_id: board.project_id,
            project_name,
            labels,
            lists: details,
        })
    }

    pub(crate) async fn get_todo(&self, todo_id: i64) -> Result<TodoDetail> {
        let todo = Entry::find_by_id(todo_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(&self.db)
            .await?
            .with_context(|| format!("active todo {todo_id} was not found"))?;

        self.todo_detail(todo).await
    }

    pub(crate) async fn search_todos(&self, input: SearchTodosInput) -> Result<Vec<TodoDetail>> {
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
            .all(&self.db)
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
            .all(&self.db)
            .await?
            .into_iter()
            .filter(|list| board_ids.contains(&list.board_id))
            .map(|list| list.id)
            .collect::<Vec<_>>();
        if list_ids.is_empty() {
            return Ok(Vec::new());
        }
        let todos = Entry::find()
            .filter(entry::Column::DeletedAt.is_null())
            .filter(entry::Column::CardId.is_in(list_ids))
            .filter(
                Condition::any()
                    .add(entry::Column::Title.contains(query))
                    .add(entry::Column::Description.contains(query)),
            )
            .order_by_asc(entry::Column::Id)
            .limit(limit)
            .all(&self.db)
            .await?;

        let mut details = Vec::with_capacity(todos.len());
        for todo in todos {
            details.push(self.todo_detail(todo).await?);
        }
        Ok(details)
    }

    pub(crate) async fn create_project(&self, input: CreateProjectInput) -> Result<ProjectSummary> {
        let name = required_text(input.name, "project name")?;
        let position = Project::find().count(&self.db).await? as i32;
        let project = project::ActiveModel {
            name: Set(name),
            archived: Set(false),
            position: Set(position),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        Ok(ProjectSummary {
            id: project.id,
            name: project.name,
            position: project.position,
            board_count: 0,
        })
    }

    pub(crate) async fn rename_project(&self, input: RenameProjectInput) -> Result<ProjectSummary> {
        let project = self.active_project(input.project_id).await?;
        project::ActiveModel {
            id: Set(project.id),
            name: Set(required_text(input.name, "project name")?),
            ..Default::default()
        }
        .update(&self.db)
        .await?;
        self.list_projects()
            .await?
            .into_iter()
            .find(|project| project.id == input.project_id)
            .with_context(|| format!("renamed project {} was not found", input.project_id))
    }

    pub(crate) async fn create_board(&self, input: CreateBoardInput) -> Result<BoardSummary> {
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
        .insert(&self.db)
        .await?;
        Ok(BoardSummary {
            id: board.id,
            title: board.title,
            project_id: board.project_id,
            project_name,
        })
    }

    pub(crate) async fn rename_board(&self, input: RenameBoardInput) -> Result<BoardSummary> {
        let board = self.active_board(input.board_id).await?;
        let board = board::ActiveModel {
            id: Set(board.id),
            title: Set(required_text(input.title, "board title")?),
            ..Default::default()
        }
        .update(&self.db)
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

    pub(crate) async fn create_list(&self, input: CreateListInput) -> Result<ListDetail> {
        let title = required_text(input.title, "list title")?;
        self.active_board(input.board_id).await?;
        let position = Card::find()
            .filter(card::Column::BoardId.eq(input.board_id))
            .filter(card::Column::DeletedAt.is_null())
            .count(&self.db)
            .await? as i32;
        let list = card::ActiveModel {
            title: Set(title),
            board_id: Set(input.board_id),
            position: Set(position),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        Ok(ListDetail {
            id: list.id,
            title: list.title,
            position: list.position,
            todos: Vec::new(),
        })
    }

    pub(crate) async fn rename_list(&self, input: RenameListInput) -> Result<ListDetail> {
        let list = self.active_list(input.list_id).await?;
        let list = card::ActiveModel {
            id: Set(list.id),
            title: Set(required_text(input.title, "list title")?),
            ..Default::default()
        }
        .update(&self.db)
        .await?;
        self.get_board(list.board_id)
            .await?
            .lists
            .into_iter()
            .find(|candidate| candidate.id == list.id)
            .with_context(|| format!("renamed list {} was not found", list.id))
    }

    pub(crate) async fn create_todo(&self, input: CreateTodoInput) -> Result<TodoDetail> {
        let title = required_text(input.title, "todo title")?;
        validate_due_on(input.due_on.as_deref())?;
        self.active_list(input.list_id).await?;
        let position = Entry::find()
            .filter(entry::Column::CardId.eq(input.list_id))
            .filter(entry::Column::DeletedAt.is_null())
            .count(&self.db)
            .await? as i32;
        let todo = entry::ActiveModel {
            title: Set(title),
            description: Set(input.description),
            card_id: Set(input.list_id),
            position: Set(position),
            due_on: Set(input.due_on),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        self.todo_detail(todo).await
    }

    pub(crate) async fn update_todo(&self, input: UpdateTodoInput) -> Result<TodoDetail> {
        if input.clear_due_on && input.due_on.is_some() {
            bail!("due_on and clear_due_on cannot be used together");
        }
        validate_due_on(input.due_on.as_deref())?;
        let todo = Entry::find_by_id(input.todo_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(&self.db)
            .await?
            .with_context(|| format!("active todo {} was not found", input.todo_id))?;
        let mut active: entry::ActiveModel = todo.into();
        if let Some(title) = input.title {
            active.title = Set(required_text(title, "todo title")?);
        }
        if let Some(description) = input.description {
            active.description = Set(description);
        }
        if input.clear_due_on {
            active.due_on = Set(None);
        } else if let Some(due_on) = input.due_on {
            active.due_on = Set(Some(due_on));
        }
        let todo = active.update(&self.db).await?;
        self.todo_detail(todo).await
    }

    pub(crate) async fn set_todo_reminder(
        &self,
        input: SetTodoReminderInput,
    ) -> Result<TodoDetail> {
        let todo = Entry::find_by_id(input.todo_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(&self.db)
            .await?
            .with_context(|| format!("active todo {} was not found", input.todo_id))?;
        self.active_list(todo.card_id).await?;
        if input.enabled && todo.due_on.is_none() {
            bail!("a todo needs a due date before its reminder can be enabled");
        }
        let todo = entry::ActiveModel {
            id: Set(todo.id),
            reminder_enabled: Set(input.enabled),
            reminder_notified_for: Set(None),
            ..Default::default()
        }
        .update(&self.db)
        .await?;
        self.todo_detail(todo).await
    }

    pub(crate) async fn add_checklist_item(
        &self,
        input: AddChecklistItemInput,
    ) -> Result<ChecklistItemDetail> {
        self.get_todo(input.todo_id).await?;
        let position = EntryChecklistItem::find()
            .filter(entry_checklist_item::Column::EntryId.eq(input.todo_id))
            .count(&self.db)
            .await? as i32;
        let item = entry_checklist_item::ActiveModel {
            entry_id: Set(input.todo_id),
            title: Set(required_text(input.title, "checklist item title")?),
            checked: Set(false),
            position: Set(position),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        Ok(ChecklistItemDetail {
            id: item.id,
            title: item.title,
            checked: item.checked,
            position: item.position,
        })
    }

    pub(crate) async fn update_checklist_item(
        &self,
        input: UpdateChecklistItemInput,
    ) -> Result<ChecklistItemDetail> {
        if input.title.is_none() && input.checked.is_none() {
            bail!("provide title or checked to update the checklist item");
        }
        let item = EntryChecklistItem::find_by_id(input.item_id)
            .one(&self.db)
            .await?
            .with_context(|| format!("checklist item {} was not found", input.item_id))?;
        self.get_todo(item.entry_id).await?;
        let mut active: entry_checklist_item::ActiveModel = item.into();
        if let Some(title) = input.title {
            active.title = Set(required_text(title, "checklist item title")?);
        }
        if let Some(checked) = input.checked {
            active.checked = Set(checked);
        }
        let item = active.update(&self.db).await?;
        Ok(ChecklistItemDetail {
            id: item.id,
            title: item.title,
            checked: item.checked,
            position: item.position,
        })
    }

    pub(crate) async fn create_board_label(
        &self,
        input: CreateBoardLabelInput,
    ) -> Result<LabelDetail> {
        self.active_board(input.board_id).await?;
        let label = board_label::ActiveModel {
            board_id: Set(input.board_id),
            name: Set(required_text(input.name, "label name")?),
            color: Set(required_text(input.color, "label color")?),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        Ok(label_detail(label))
    }

    pub(crate) async fn set_todo_label(&self, input: SetTodoLabelInput) -> Result<TodoDetail> {
        let todo = self.get_todo(input.todo_id).await?;
        let label = BoardLabel::find_by_id(input.label_id)
            .one(&self.db)
            .await?
            .with_context(|| format!("board label {} was not found", input.label_id))?;
        if label.board_id != todo.board_id {
            bail!(
                "label {} belongs to board {}, but todo {} belongs to board {}",
                label.id,
                label.board_id,
                todo.id,
                todo.board_id
            );
        }
        let existing = EntryLabel::find()
            .filter(entry_label::Column::EntryId.eq(input.todo_id))
            .filter(entry_label::Column::BoardLabelId.eq(input.label_id))
            .one(&self.db)
            .await?;
        match (input.assigned, existing) {
            (true, None) => {
                entry_label::ActiveModel {
                    entry_id: Set(input.todo_id),
                    board_label_id: Set(input.label_id),
                    ..Default::default()
                }
                .insert(&self.db)
                .await?;
            }
            (false, Some(association)) => {
                EntryLabel::delete_by_id(association.id)
                    .exec(&self.db)
                    .await?;
            }
            _ => {}
        }
        self.get_todo(input.todo_id).await
    }

    pub(crate) async fn move_todo(&self, input: MoveTodoInput) -> Result<TodoDetail> {
        self.active_list(input.list_id).await?;
        let todo = Entry::find_by_id(input.todo_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(&self.db)
            .await?
            .with_context(|| format!("active todo {} was not found", input.todo_id))?;
        if todo.card_id == input.list_id {
            return self.todo_detail(todo).await;
        }

        let transaction = self.db.begin().await?;
        Entry::update_many()
            .col_expr(
                entry::Column::Position,
                Expr::col(entry::Column::Position).sub(1),
            )
            .filter(entry::Column::CardId.eq(todo.card_id))
            .filter(entry::Column::Position.gt(todo.position))
            .exec(&transaction)
            .await?;
        let position = Entry::find()
            .filter(entry::Column::CardId.eq(input.list_id))
            .filter(entry::Column::DeletedAt.is_null())
            .count(&transaction)
            .await? as i32;
        let moved = entry::ActiveModel {
            id: Set(todo.id),
            card_id: Set(input.list_id),
            position: Set(position),
            ..Default::default()
        }
        .update(&transaction)
        .await?;
        transaction.commit().await?;
        self.todo_detail(moved).await
    }

    async fn active_note(&self, note_id: i64) -> Result<note::Model> {
        let note = Note::find_by_id(note_id)
            .filter(note::Column::DeletedAt.is_null())
            .one(&self.db)
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
        })
    }

    async fn active_project(&self, project_id: i64) -> Result<project::Model> {
        Project::find_by_id(project_id)
            .filter(project::Column::Archived.eq(false))
            .filter(project::Column::DeletedAt.is_null())
            .one(&self.db)
            .await?
            .with_context(|| format!("active project {project_id} was not found"))
    }

    async fn active_project_map(&self) -> Result<HashMap<i64, String>> {
        Ok(Project::find()
            .filter(project::Column::Archived.eq(false))
            .filter(project::Column::DeletedAt.is_null())
            .all(&self.db)
            .await?
            .into_iter()
            .map(|project| (project.id, project.name))
            .collect())
    }

    async fn active_board(&self, board_id: i64) -> Result<board::Model> {
        let board = Board::find_by_id(board_id)
            .filter(board::Column::DeletedAt.is_null())
            .one(&self.db)
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
            .one(&self.db)
            .await?
            .with_context(|| format!("active list {list_id} was not found"))?;
        self.active_board(list.board_id).await?;
        Ok(list)
    }

    async fn todo_detail(&self, todo: entry::Model) -> Result<TodoDetail> {
        let list = self.active_list(todo.card_id).await?;
        let board = self.active_board(list.board_id).await?;
        let project_name = match board.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        self.todo_detail_with_context(todo, &list, &board, project_name)
            .await
    }

    async fn todo_detail_with_context(
        &self,
        todo: entry::Model,
        list: &card::Model,
        board: &board::Model,
        project_name: Option<String>,
    ) -> Result<TodoDetail> {
        let checklist_items = EntryChecklistItem::find()
            .filter(entry_checklist_item::Column::EntryId.eq(todo.id))
            .order_by_asc(entry_checklist_item::Column::Position)
            .order_by_asc(entry_checklist_item::Column::Id)
            .all(&self.db)
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
            .filter(entry_label::Column::EntryId.eq(todo.id))
            .all(&self.db)
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
                .all(&self.db)
                .await?
                .into_iter()
                .map(label_detail)
                .collect()
        };
        let attachments = EntryAttachment::find()
            .filter(entry_attachment::Column::EntryId.eq(todo.id))
            .order_by_asc(entry_attachment::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|attachment| AttachmentDetail {
                id: attachment.id,
                file_name: attachment.file_name,
            })
            .collect();
        Ok(TodoDetail {
            id: todo.id,
            title: todo.title,
            description: todo.description,
            due_on: todo.due_on,
            reminder_enabled: todo.reminder_enabled,
            position: todo.position,
            list_id: list.id,
            list_title: list.title.clone(),
            board_id: board.id,
            board_title: board.title.clone(),
            project_id: board.project_id,
            project_name,
            labels,
            checklist_items,
            attachments,
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

    async fn store() -> Result<CastleStore> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        Ok(CastleStore::new(db))
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
        let todo_list = store
            .create_list(CreateListInput {
                board_id: board.id,
                title: "Todo".to_string(),
            })
            .await?;
        let done_list = store
            .create_list(CreateListInput {
                board_id: board.id,
                title: "Done".to_string(),
            })
            .await?;
        let todo = store
            .create_todo(CreateTodoInput {
                list_id: todo_list.id,
                title: "Write MCP tests".to_string(),
                description: "Cover the full hierarchy".to_string(),
                due_on: Some("2026-07-24".to_string()),
            })
            .await?;
        let reminder = store
            .set_todo_reminder(SetTodoReminderInput {
                todo_id: todo.id,
                enabled: true,
            })
            .await?;
        assert!(reminder.reminder_enabled);
        let checklist_item = store
            .add_checklist_item(AddChecklistItemInput {
                todo_id: todo.id,
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
            .set_todo_label(SetTodoLabelInput {
                todo_id: todo.id,
                label_id: label.id,
                assigned: true,
            })
            .await?;

        let matches = store
            .search_todos(SearchTodosInput {
                query: "MCP".to_string(),
                project_id: Some(project.id),
                board_id: None,
                limit: None,
            })
            .await?;
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, todo.id);
        assert_eq!(matches[0].checklist_items.len(), 1);
        assert!(matches[0].checklist_items[0].checked);
        assert_eq!(matches[0].labels[0].name, "Agent");

        let moved = store
            .move_todo(MoveTodoInput {
                todo_id: todo.id,
                list_id: done_list.id,
            })
            .await?;
        assert_eq!(moved.list_title, "Done");
        assert_eq!(store.get_board(board.id).await?.lists[1].todos.len(), 1);
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
        Ok(())
    }

    #[tokio::test]
    async fn only_explicit_external_writes_increment_the_change_revision() -> Result<()> {
        let store = store().await?;
        let project = store
            .create_project(CreateProjectInput {
                name: "Revision".to_string(),
            })
            .await?;
        let note = store
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

        store.record_external_change(ChangeDomain::Note).await?;
        let row = change_revision_row(&store).await?;
        assert_eq!(row.try_get::<i64>("", "revision")?, 1);
        assert_eq!(row.try_get::<i64>("", "board_revision")?, 0);
        assert_eq!(row.try_get::<i64>("", "note_revision")?, 1);
        Ok(())
    }

    async fn change_revision_row(store: &CastleStore) -> Result<sea_orm::QueryResult> {
        let row = store
            .db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT revision, board_revision, note_revision FROM castle_change_revision WHERE id = 1",
            ))
            .await?
            .context("revision row was not found")?;
        Ok(row)
    }
}
