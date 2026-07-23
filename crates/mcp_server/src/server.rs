use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    tool, tool_router,
};

use crate::{
    store::{CastleStore, ChangeDomain},
    types::*,
};

#[derive(Clone)]
pub(crate) struct CastleServer {
    store: CastleStore,
}

impl CastleServer {
    pub(crate) fn new(store: CastleStore) -> Self {
        Self { store }
    }
}

#[tool_router(server_handler)]
impl CastleServer {
    #[tool(
        description = "List active Castle projects with stable IDs and board counts",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn list_projects(
        &self,
        Parameters(_input): Parameters<EmptyInput>,
    ) -> Json<ToolResponse<Vec<ProjectSummary>>> {
        response(self.store.list_projects().await)
    }

    #[tool(
        description = "List active Castle boards, optionally filtered to one project ID",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn list_boards(
        &self,
        Parameters(input): Parameters<ProjectBoardsInput>,
    ) -> Json<ToolResponse<Vec<BoardSummary>>> {
        response(self.store.list_boards(input.project_id).await)
    }

    #[tool(
        description = "Read a complete Castle board including its lists and todos",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn get_board(
        &self,
        Parameters(input): Parameters<BoardInput>,
    ) -> Json<ToolResponse<BoardDetail>> {
        response(self.store.get_board(input.board_id).await)
    }

    #[tool(
        description = "Read one Castle todo with its project, board, and list context",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn get_todo(
        &self,
        Parameters(input): Parameters<TodoInput>,
    ) -> Json<ToolResponse<TodoDetail>> {
        response(self.store.get_todo(input.todo_id).await)
    }

    #[tool(
        description = "Search active Castle todos by title or description and return full context",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn search_todos(
        &self,
        Parameters(input): Parameters<SearchTodosInput>,
    ) -> Json<ToolResponse<Vec<TodoDetail>>> {
        response(self.store.search_todos(input).await)
    }

    #[tool(
        description = "Create a Castle project",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    async fn create_project(
        &self,
        Parameters(input): Parameters<CreateProjectInput>,
    ) -> Json<ToolResponse<ProjectSummary>> {
        mutation_response(
            &self.store,
            self.store.create_project(input).await,
            ChangeDomain::Workspace,
        )
        .await
    }

    #[tool(
        description = "Create a Castle board inside a project or as a standalone board",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    async fn create_board(
        &self,
        Parameters(input): Parameters<CreateBoardInput>,
    ) -> Json<ToolResponse<BoardSummary>> {
        mutation_response(
            &self.store,
            self.store.create_board(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Create a kanban list such as Todo, Doing, or Done on a Castle board",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    async fn create_list(
        &self,
        Parameters(input): Parameters<CreateListInput>,
    ) -> Json<ToolResponse<ListDetail>> {
        mutation_response(
            &self.store,
            self.store.create_list(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Create a todo in a Castle board list",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    async fn create_todo(
        &self,
        Parameters(input): Parameters<CreateTodoInput>,
    ) -> Json<ToolResponse<TodoDetail>> {
        mutation_response(
            &self.store,
            self.store.create_todo(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Update a Castle todo's title, description, or due date",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn update_todo(
        &self,
        Parameters(input): Parameters<UpdateTodoInput>,
    ) -> Json<ToolResponse<TodoDetail>> {
        mutation_response(
            &self.store,
            self.store.update_todo(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Move a Castle todo to another list, for example from Todo to Doing or Done",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn move_todo(
        &self,
        Parameters(input): Parameters<MoveTodoInput>,
    ) -> Json<ToolResponse<TodoDetail>> {
        mutation_response(
            &self.store,
            self.store.move_todo(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "List active Castle notes with project context and stable IDs",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn list_notes(
        &self,
        Parameters(input): Parameters<ProjectNotesInput>,
    ) -> Json<ToolResponse<Vec<NoteSummary>>> {
        response(self.store.list_notes(input.project_id, input.limit).await)
    }

    #[tool(
        description = "Read a Castle note's current content and metadata",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn get_note(
        &self,
        Parameters(input): Parameters<NoteInput>,
    ) -> Json<ToolResponse<NoteDetail>> {
        response(self.store.get_note(input.note_id).await)
    }

    #[tool(
        description = "Search active Castle notes by title or cached content",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn search_notes(
        &self,
        Parameters(input): Parameters<SearchNotesInput>,
    ) -> Json<ToolResponse<Vec<NoteSummary>>> {
        response(self.store.search_notes(input).await)
    }

    #[tool(
        description = "Create a database-backed Castle note with initial content",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    async fn create_note(
        &self,
        Parameters(input): Parameters<CreateNoteInput>,
    ) -> Json<ToolResponse<NoteDetail>> {
        mutation_response(
            &self.store,
            self.store.create_note(input).await,
            ChangeDomain::Note,
        )
        .await
    }

    #[tool(
        description = "Update a Castle note's title, content, or pinned state; file-backed notes update their file too",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn update_note(
        &self,
        Parameters(input): Parameters<UpdateNoteInput>,
    ) -> Json<ToolResponse<NoteDetail>> {
        mutation_response(
            &self.store,
            self.store.update_note(input).await,
            ChangeDomain::Note,
        )
        .await
    }

    #[tool(
        description = "Move a Castle note to another project or make it standalone",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn move_note(
        &self,
        Parameters(input): Parameters<MoveNoteInput>,
    ) -> Json<ToolResponse<NoteDetail>> {
        mutation_response(
            &self.store,
            self.store.move_note(input).await,
            ChangeDomain::Note,
        )
        .await
    }

    #[tool(
        description = "Rename an active Castle project",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn rename_project(
        &self,
        Parameters(input): Parameters<RenameProjectInput>,
    ) -> Json<ToolResponse<ProjectSummary>> {
        mutation_response(
            &self.store,
            self.store.rename_project(input).await,
            ChangeDomain::Workspace,
        )
        .await
    }

    #[tool(
        description = "Rename an active Castle board",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn rename_board(
        &self,
        Parameters(input): Parameters<RenameBoardInput>,
    ) -> Json<ToolResponse<BoardSummary>> {
        mutation_response(
            &self.store,
            self.store.rename_board(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Rename an active Castle kanban list",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn rename_list(
        &self,
        Parameters(input): Parameters<RenameListInput>,
    ) -> Json<ToolResponse<ListDetail>> {
        mutation_response(
            &self.store,
            self.store.rename_list(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Enable or disable the system reminder for a Castle todo with a due date",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn set_todo_reminder(
        &self,
        Parameters(input): Parameters<SetTodoReminderInput>,
    ) -> Json<ToolResponse<TodoDetail>> {
        mutation_response(
            &self.store,
            self.store.set_todo_reminder(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Add an unchecked checklist item to a Castle todo",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    async fn add_checklist_item(
        &self,
        Parameters(input): Parameters<AddChecklistItemInput>,
    ) -> Json<ToolResponse<ChecklistItemDetail>> {
        mutation_response(
            &self.store,
            self.store.add_checklist_item(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Rename or check/uncheck a Castle todo checklist item",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn update_checklist_item(
        &self,
        Parameters(input): Parameters<UpdateChecklistItemInput>,
    ) -> Json<ToolResponse<ChecklistItemDetail>> {
        mutation_response(
            &self.store,
            self.store.update_checklist_item(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Create a reusable label on a Castle board",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    async fn create_board_label(
        &self,
        Parameters(input): Parameters<CreateBoardLabelInput>,
    ) -> Json<ToolResponse<LabelDetail>> {
        mutation_response(
            &self.store,
            self.store.create_board_label(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Assign or unassign a board label on a Castle todo",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn set_todo_label(
        &self,
        Parameters(input): Parameters<SetTodoLabelInput>,
    ) -> Json<ToolResponse<TodoDetail>> {
        mutation_response(
            &self.store,
            self.store.set_todo_label(input).await,
            ChangeDomain::Board,
        )
        .await
    }
}

fn response<T>(result: anyhow::Result<T>) -> Json<ToolResponse<T>> {
    Json(match result {
        Ok(data) => ToolResponse::success(data),
        Err(error) => ToolResponse::error(error),
    })
}

async fn mutation_response<T>(
    store: &CastleStore,
    result: anyhow::Result<T>,
    domain: ChangeDomain,
) -> Json<ToolResponse<T>> {
    match result {
        Ok(data) => response(store.record_external_change(domain).await.map(|()| data)),
        Err(error) => response(Err(error)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use anyhow::Result;
    use migration::{Migrator, MigratorTrait};
    use rmcp::ServiceExt;
    use sea_orm::Database;

    use super::*;

    #[tokio::test]
    async fn protocol_client_discovers_castle_tools() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let server = CastleServer::new(CastleStore::new(db));
        let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);

        let server_task = tokio::spawn(async move {
            let service = server.serve(server_transport).await?;
            service.waiting().await?;
            Ok::<(), anyhow::Error>(())
        });
        let client = ().serve(client_transport).await?;
        let tool_names = client
            .list_all_tools()
            .await?
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<HashSet<_>>();

        for expected in [
            "list_projects",
            "list_boards",
            "get_board",
            "get_todo",
            "search_todos",
            "create_project",
            "create_board",
            "create_list",
            "create_todo",
            "update_todo",
            "move_todo",
            "list_notes",
            "get_note",
            "search_notes",
            "create_note",
            "update_note",
            "move_note",
            "rename_project",
            "rename_board",
            "rename_list",
            "set_todo_reminder",
            "add_checklist_item",
            "update_checklist_item",
            "create_board_label",
            "set_todo_label",
        ] {
            assert!(tool_names.contains(expected), "missing MCP tool {expected}");
        }

        client.cancel().await?;
        server_task.abort();
        let _ = server_task.await;
        Ok(())
    }
}
