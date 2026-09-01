use super::*;
use crate::workspace::api::{
    BoardPropertyKindInput, CreateBoardPropertyInput, SetEntryPropertyInput,
};
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

use crate::MutationOrigin;

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
    assert_eq!(matches[0].labels.len(), 1);
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

    let missing_content = "See [[card:Missing card|Unavailable card]]".to_string();
    let missing = store
        .create_note(CreateNoteInput {
            title: "Missing target".to_string(),
            content: missing_content.clone(),
            project_id: None,
        })
        .await?;
    let links = store.get_note_links(missing.id).await?;
    assert_eq!(links.unresolved.len(), 1);
    assert_eq!(links.unresolved[0].target_kind.as_deref(), Some("card"));
    assert_eq!(links.unresolved[0].start_byte, 4);
    assert_eq!(links.unresolved[0].end_byte, missing_content.len());
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
            description: Some("See [[note:API research]]".to_string()),
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
async fn local_mutations_do_not_bump_and_external_mutations_bump_the_owned_domain() -> Result<()> {
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
