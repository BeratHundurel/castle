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
        description = "Read a complete Castle board including its lists and entries",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn get_board(
        &self,
        Parameters(input): Parameters<BoardInput>,
    ) -> Json<ToolResponse<BoardDetail>> {
        response(self.store.get_board(input.board_id).await)
    }

    #[tool(
        description = "Read a board's custom property definitions, select options, and assigned entry values",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn get_board_properties(
        &self,
        Parameters(input): Parameters<BoardInput>,
    ) -> Json<ToolResponse<BoardPropertiesDetail>> {
        response(self.store.board_properties(input.board_id).await)
    }

    #[tool(
        description = "Create a typed custom property on a Castle board without assigning workflow meaning",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    async fn create_board_property(
        &self,
        Parameters(input): Parameters<CreateBoardPropertyInput>,
    ) -> Json<ToolResponse<BoardPropertyDefinitionDetail>> {
        mutation_response(
            &self.store,
            self.store.create_board_property(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Create an explicit option for a select-type board property",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    async fn create_board_property_option(
        &self,
        Parameters(input): Parameters<CreateBoardPropertyOptionInput>,
    ) -> Json<ToolResponse<BoardPropertyOptionDetail>> {
        mutation_response(
            &self.store,
            self.store.create_board_property_option(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Set a typed custom property value on a board entry",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn set_entry_property(
        &self,
        Parameters(input): Parameters<SetEntryPropertyInput>,
    ) -> Json<ToolResponse<EntryPropertyValueDetail>> {
        mutation_response(
            &self.store,
            self.store.set_entry_property(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Clear one custom property value from a board entry",
        annotations(
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn clear_entry_property(
        &self,
        Parameters(input): Parameters<ClearEntryPropertyInput>,
    ) -> Json<ToolResponse<()>> {
        mutation_response(
            &self.store,
            self.store.clear_entry_property(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Read one Castle board entry with its project, board, and list context",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn get_entry(
        &self,
        Parameters(input): Parameters<EntryInput>,
    ) -> Json<ToolResponse<EntryDetail>> {
        response(self.store.get_entry(input.entry_id).await)
    }

    #[tool(
        description = "Search active Castle board entries by title or description and return full context",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn search_entries(
        &self,
        Parameters(input): Parameters<SearchEntriesInput>,
    ) -> Json<ToolResponse<Vec<EntryDetail>>> {
        response(self.store.search_entries(input).await)
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
        description = "Create a named list on a Castle board without assigning workflow semantics",
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
        description = "Create an entry in a Castle board list",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    async fn create_entry(
        &self,
        Parameters(input): Parameters<CreateEntryInput>,
    ) -> Json<ToolResponse<EntryDetail>> {
        mutation_response(
            &self.store,
            self.store.create_entry(input).await,
            ChangeDomain::Link,
        )
        .await
    }

    #[tool(
        description = "Update a Castle board entry's title, description, or optional due date",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn update_entry(
        &self,
        Parameters(input): Parameters<UpdateEntryInput>,
    ) -> Json<ToolResponse<EntryDetail>> {
        mutation_response(
            &self.store,
            self.store.update_entry(input).await,
            ChangeDomain::Link,
        )
        .await
    }

    #[tool(
        description = "Move a Castle board entry to another list without interpreting either list's meaning",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn move_entry(
        &self,
        Parameters(input): Parameters<MoveEntryInput>,
    ) -> Json<ToolResponse<EntryDetail>> {
        mutation_response(
            &self.store,
            self.store.move_entry(input).await,
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
        description = "Read inbound, outbound, and unresolved wikilinks for a Castle note",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn get_note_links(
        &self,
        Parameters(input): Parameters<NoteInput>,
    ) -> Json<ToolResponse<NoteLinksDetail>> {
        response(self.store.get_note_links(input.note_id).await)
    }

    #[tool(
        description = "List a note's board/list/card relationships, or the notes related to one exact workspace item",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn list_workspace_relations(
        &self,
        Parameters(input): Parameters<WorkspaceRelationsInput>,
    ) -> Json<ToolResponse<Vec<RelatedItemDetail>>> {
        response(self.store.list_workspace_relations(input).await)
    }

    #[tool(
        description = "Link an active Castle note to an exact board, list, or card after validating its hierarchy",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn link_note_to_workspace_item(
        &self,
        Parameters(input): Parameters<NoteWorkspaceRelationInput>,
    ) -> Json<ToolResponse<Vec<RelatedItemDetail>>> {
        mutation_response(
            &self.store,
            self.store.link_note_to_workspace_item(input).await,
            ChangeDomain::Link,
        )
        .await
    }

    #[tool(
        description = "Remove only the manual relationship between a note and an exact board, list, or card",
        annotations(
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn unlink_note_from_workspace_item(
        &self,
        Parameters(input): Parameters<NoteWorkspaceRelationInput>,
    ) -> Json<ToolResponse<Vec<RelatedItemDetail>>> {
        mutation_response(
            &self.store,
            self.store.unlink_note_from_workspace_item(input).await,
            ChangeDomain::Link,
        )
        .await
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
            ChangeDomain::Link,
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
            ChangeDomain::Link,
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
        description = "Enable or disable the system reminder for a board entry with a due date",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn set_entry_reminder(
        &self,
        Parameters(input): Parameters<SetEntryReminderInput>,
    ) -> Json<ToolResponse<EntryDetail>> {
        mutation_response(
            &self.store,
            self.store.set_entry_reminder(input).await,
            ChangeDomain::Board,
        )
        .await
    }

    #[tool(
        description = "Add an unchecked checklist item to a Castle board entry",
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
        description = "Rename or check/uncheck a Castle board entry checklist item",
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
        description = "Assign or unassign a board label on a Castle board entry",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn set_entry_label(
        &self,
        Parameters(input): Parameters<SetEntryLabelInput>,
    ) -> Json<ToolResponse<EntryDetail>> {
        mutation_response(
            &self.store,
            self.store.set_entry_label(input).await,
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
            "get_board_properties",
            "get_entry",
            "search_entries",
            "create_project",
            "create_board",
            "create_list",
            "create_entry",
            "update_entry",
            "move_entry",
            "list_notes",
            "get_note",
            "get_note_links",
            "list_workspace_relations",
            "link_note_to_workspace_item",
            "unlink_note_from_workspace_item",
            "search_notes",
            "create_note",
            "update_note",
            "move_note",
            "rename_project",
            "rename_board",
            "rename_list",
            "set_entry_reminder",
            "add_checklist_item",
            "update_checklist_item",
            "create_board_label",
            "set_entry_label",
            "create_board_property",
            "create_board_property_option",
            "set_entry_property",
            "clear_entry_property",
        ] {
            assert!(tool_names.contains(expected), "missing MCP tool {expected}");
        }

        client.cancel().await?;
        server_task.abort();
        let _ = server_task.await;
        Ok(())
    }
}
