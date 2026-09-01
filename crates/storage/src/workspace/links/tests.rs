use super::*;
use entity::{board, card, entry, note, project, workspace_reference_alias};
use migration::{Migrator, MigratorTrait};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, Database, QueryFilter, QueryOrder};

#[test]
fn relation_signature_ignores_prose_and_duplicate_occurrences() {
    let original = "Before [[board:Roadmap|Old]] and [[board:Roadmap|Duplicate]].\n\n![[board:Roadmap#Current]]";
    let edited = "After [[board:Roadmap|New]].\n\n![[board:Roadmap#Current]]";

    assert_eq!(
        workspace_relation_signature(original),
        workspace_relation_signature(edited)
    );
    assert_ne!(
        workspace_relation_signature(original),
        workspace_relation_signature("[[card:Different]]")
    );
}

#[test]
fn relation_signature_keeps_escaped_path_delimiters_distinct() {
    assert_ne!(
        workspace_relation_signature(r"[[board:Road\/Map]]"),
        workspace_relation_signature(r"[[board:Road / Map]]")
    );
}

#[tokio::test]
async fn manual_and_wikilink_origins_are_deduplicated() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let note = note::ActiveModel {
        title: Set("Research".to_string()),
        cached_content: Set(String::new()),
        file_managed_by_app: Set(false),
        created_at: Set(0),
        updated_at: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let board = board::ActiveModel {
        title: Set("Roadmap".to_string()),
        last_selected_view_id: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let list = card::ActiveModel {
        title: Set("Ideas".to_string()),
        board_id: Set(board.id),
        position: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let item = entry::ActiveModel {
        title: Set("Explore links".to_string()),
        description: Set("[[Research]]".to_string()),
        card_id: Set(list.id),
        position: Set(0),
        reminder_enabled: Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await?;

    link_note_to_item(
        &db,
        note.id,
        WorkspaceItemRef {
            kind: WorkspaceItemKind::Card,
            id: item.id,
        },
        0,
    )
    .await?;
    index_entry_workspace_links(&db, item.id, "[[Research]]", 0).await?;
    index_entry_workspace_links(&db, item.id, "[[Research]]", 1).await?;

    let related = load_related_notes(
        &db,
        WorkspaceItemRef {
            kind: WorkspaceItemKind::Card,
            id: item.id,
        },
    )
    .await?;
    assert_eq!(related.len(), 1);
    assert!(related[0].origins.contains(&WorkspaceLinkOrigin::Manual));
    assert!(related[0].origins.contains(&WorkspaceLinkOrigin::Wikilink));
    Ok(())
}

#[tokio::test]
async fn manual_link_updates_are_idempotent_and_return_canonical_state() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let note = note::ActiveModel {
        title: Set("Research".to_string()),
        cached_content: Set(String::new()),
        file_managed_by_app: Set(false),
        created_at: Set(0),
        updated_at: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let board = board::ActiveModel {
        title: Set("Roadmap".to_string()),
        last_selected_view_id: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let item = WorkspaceItemRef {
        kind: WorkspaceItemKind::Board,
        id: board.id,
    };

    let linked = set_manual_note_link(&db, note.id, item, true, 1).await?;
    assert!(linked.changed);
    assert_eq!(linked.related_notes.len(), 1);
    let duplicate = set_manual_note_link(&db, note.id, item, true, 2).await?;
    assert!(!duplicate.changed);
    assert_eq!(duplicate.related_notes, linked.related_notes);

    let unlinked = set_manual_note_link(&db, note.id, item, false, 3).await?;
    assert!(unlinked.changed);
    assert!(unlinked.related_notes.is_empty());
    let duplicate = set_manual_note_link(&db, note.id, item, false, 4).await?;
    assert!(!duplicate.changed);
    assert!(duplicate.related_notes.is_empty());
    Ok(())
}

#[tokio::test]
async fn note_workspace_links_resolve_readable_references() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let note = note::ActiveModel {
        title: Set("Brief".to_string()),
        cached_content: Set(String::new()),
        file_managed_by_app: Set(false),
        created_at: Set(0),
        updated_at: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let board = board::ActiveModel {
        title: Set("Before rename".to_string()),
        last_selected_view_id: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    index_note_workspace_links(&db, note.id, "[[board:Before rename|Roadmap]]", 0).await?;
    board::ActiveModel {
        id: Set(board.id),
        title: Set("After rename".to_string()),
        ..Default::default()
    }
    .update(&db)
    .await?;

    let links = load_note_workspace_links(&db, note.id).await?;
    assert_eq!(links.references[0].item.title, "After rename");
    Ok(())
}

#[tokio::test]
async fn rename_operations_record_historical_reference_aliases_transactionally() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let project = project::ActiveModel {
        name: Set("Old Project".to_string()),
        archived: Set(false),
        position: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let board = board::ActiveModel {
        title: Set("Old Board".to_string()),
        project_id: Set(Some(project.id)),
        last_selected_view_id: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let list = card::ActiveModel {
        title: Set("Old List".to_string()),
        board_id: Set(board.id),
        position: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let entry = entry::ActiveModel {
        title: Set("Old Card".to_string()),
        description: Set(String::new()),
        card_id: Set(list.id),
        position: Set(0),
        reminder_enabled: Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let view = crate::board::properties::create_board_view(
        &db,
        board.id,
        "Old View".to_string(),
        Default::default(),
    )
    .await?;

    crate::workspace::rename_project(&db, project.id as u32, "New Project".to_string()).await?;
    crate::workspace::persist_workspace_title(
        &db,
        crate::workspace::WorkspaceTitleTarget::Board(board.id as u32),
        "New Board".to_string(),
    )
    .await?;
    crate::board::commands::rename_board_list(&db, list.id as u32, "New List".to_string()).await?;
    crate::board::commands::update_board_card(
        &db,
        entry.id as u32,
        "New Card".to_string(),
        String::new(),
        1,
    )
    .await?;
    crate::board::properties::rename_board_view(&db, view.id, "New View".to_string()).await?;

    let catalog = load_workspace_reference_catalog(&db).await?;
    assert_eq!(
        resolve_reference_target("board:Old Board", &catalog),
        Ok(ResolvedWorkspaceReference::Item(WorkspaceItemRef {
            kind: WorkspaceItemKind::Board,
            id: board.id,
        }))
    );
    assert_eq!(
        resolve_reference_target("list:Old List", &catalog),
        Ok(ResolvedWorkspaceReference::Item(WorkspaceItemRef {
            kind: WorkspaceItemKind::List,
            id: list.id,
        }))
    );
    assert_eq!(
        resolve_reference_target("card:Old Card", &catalog),
        Ok(ResolvedWorkspaceReference::Item(WorkspaceItemRef {
            kind: WorkspaceItemKind::Card,
            id: entry.id,
        }))
    );
    assert_eq!(
        resolve_board_view_target("board:New Board#Old View", &catalog),
        Ok(ResolvedWorkspaceReference::BoardView {
            board_id: board.id,
            view_id: Some(view.id),
        })
    );
    assert_eq!(
        resolve_reference_target(
            "card:Old Project / New Board / New List / New Card",
            &catalog
        ),
        Ok(ResolvedWorkspaceReference::Item(WorkspaceItemRef {
            kind: WorkspaceItemKind::Card,
            id: entry.id,
        }))
    );
    assert_eq!(
        workspace_reference_alias::Entity::find().count(&db).await?,
        5
    );
    Ok(())
}

#[tokio::test]
async fn reindexing_keeps_an_existing_binding_when_a_duplicate_name_appears() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let note = note::ActiveModel {
        title: Set("Brief".to_string()),
        cached_content: Set(String::new()),
        file_managed_by_app: Set(false),
        created_at: Set(0),
        updated_at: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let first = board::ActiveModel {
        title: Set("Roadmap".to_string()),
        last_selected_view_id: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let content = "[[board:Roadmap]]";
    index_note_workspace_links(&db, note.id, content, 0).await?;
    board::ActiveModel {
        title: Set("Roadmap".to_string()),
        last_selected_view_id: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;

    index_note_workspace_links(&db, note.id, content, 1).await?;
    let links = load_note_workspace_links(&db, note.id).await?;
    assert_eq!(links.references.len(), 1);
    assert_eq!(links.references[0].item.item.id, first.id);
    Ok(())
}

#[tokio::test]
async fn board_transclusions_index_readable_board_and_saved_view_targets() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let note = note::ActiveModel {
        title: Set("Brief".to_string()),
        cached_content: Set(String::new()),
        file_managed_by_app: Set(false),
        created_at: Set(0),
        updated_at: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let board = board::ActiveModel {
        title: Set("Roadmap".to_string()),
        last_selected_view_id: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let view = crate::board::properties::create_board_view(
        &db,
        board.id,
        "Current".to_string(),
        Default::default(),
    )
    .await?;
    let content = "![[board:Roadmap#Current]]\n![[board:Roadmap]]\n";
    crate::note::links::index_note_links(&db, note.id, content, 0).await?;
    let links = WorkspaceLink::find()
        .filter(workspace_link::Column::SourceNoteId.eq(note.id))
        .filter(workspace_link::Column::Origin.eq("embed"))
        .order_by_asc(workspace_link::Column::Ordinal)
        .all(&db)
        .await?;
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].target_board_id, Some(board.id));
    assert_eq!(links[0].target_saved_view_id, Some(view.id));
    assert_eq!(links[1].target_saved_view_id, None);
    assert!(
        crate::note::links::load_note_links(&db, note.id)
            .await?
            .outbound
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn reindexing_does_not_turn_a_deleted_saved_view_into_all_cards() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let note = note::ActiveModel {
        title: Set("Brief".to_string()),
        cached_content: Set(String::new()),
        file_managed_by_app: Set(false),
        created_at: Set(0),
        updated_at: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let board = board::ActiveModel {
        title: Set("Roadmap".to_string()),
        last_selected_view_id: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let view = crate::board::properties::create_board_view(
        &db,
        board.id,
        "Current".to_string(),
        Default::default(),
    )
    .await?;
    let content = "![[board:Roadmap#Current]]";
    crate::note::links::index_note_links(&db, note.id, content, 0).await?;
    crate::board::properties::delete_board_view(&db, view.id).await?;
    crate::note::links::index_note_links(&db, note.id, content, 1).await?;

    assert_eq!(
        WorkspaceLink::find()
            .filter(workspace_link::Column::SourceNoteId.eq(note.id))
            .filter(workspace_link::Column::Origin.eq("embed"))
            .count(&db)
            .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn managed_note_creation_indexes_workspace_targets_immediately() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let board = board::ActiveModel {
        title: Set("Roadmap".to_string()),
        last_selected_view_id: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let note = crate::workspace::create_managed_note(
        &db,
        None,
        "Brief".to_string(),
        "brief.md".to_string(),
        "[[board:Roadmap]]".to_string(),
    )
    .await?;

    let links = load_note_workspace_links(&db, i64::from(note.id)).await?;
    assert_eq!(links.references.len(), 1);
    assert_eq!(links.references[0].item.item.kind, WorkspaceItemKind::Board);
    assert_eq!(links.references[0].item.item.id, board.id);

    WorkspaceLink::delete_many()
        .filter(workspace_link::Column::SourceNoteId.eq(i64::from(note.id)))
        .filter(workspace_link::Column::Origin.eq(ORIGIN_NOTE_WIKILINK))
        .exec(&db)
        .await?;
    WorkspaceLinkIndexState::delete_by_id(("note".to_string(), i64::from(note.id)))
        .exec(&db)
        .await?;
    assert_eq!(reindex_stale_note_workspace_links(&db, 8).await?, 1);
    assert_eq!(
        load_note_workspace_links(&db, i64::from(note.id))
            .await?
            .references
            .len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn soft_deleted_targets_hide_without_losing_restorable_relations() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let note = note::ActiveModel {
        title: Set("Research".to_string()),
        cached_content: Set(String::new()),
        file_managed_by_app: Set(false),
        created_at: Set(0),
        updated_at: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let board = board::ActiveModel {
        title: Set("Roadmap".to_string()),
        last_selected_view_id: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let item = WorkspaceItemRef {
        kind: WorkspaceItemKind::Board,
        id: board.id,
    };
    link_note_to_item(&db, note.id, item, 0).await?;

    board::ActiveModel {
        id: Set(board.id),
        deleted_at: Set(Some(10)),
        ..Default::default()
    }
    .update(&db)
    .await?;
    assert!(
        load_workspace_link_catalog(&db)
            .await?
            .iter()
            .all(|entry| entry.item != item)
    );
    assert_eq!(WorkspaceLink::find().count(&db).await?, 1);

    board::ActiveModel {
        id: Set(board.id),
        deleted_at: Set(None),
        ..Default::default()
    }
    .update(&db)
    .await?;
    assert_eq!(load_related_notes(&db, item).await?.len(), 1);
    Board::delete_by_id(board.id).exec(&db).await?;
    assert_eq!(WorkspaceLink::find().count(&db).await?, 0);
    Ok(())
}

#[tokio::test]
async fn linked_note_creation_rolls_back_when_the_relationship_is_invalid() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;

    let result = crate::workspace::create_managed_linked_note(
        &db,
        None,
        "Draft".to_string(),
        "draft.md".to_string(),
        "# Draft".to_string(),
        WorkspaceItemRef {
            kind: WorkspaceItemKind::Board,
            id: 999,
        },
    )
    .await;

    assert!(result.is_err());
    assert_eq!(Note::find().count(&db).await?, 0);
    assert_eq!(WorkspaceLink::find().count(&db).await?, 0);
    assert_eq!(WorkspaceLinkIndexState::find().count(&db).await?, 0);
    Ok(())
}

#[tokio::test]
async fn repair_batches_never_index_more_than_the_requested_bound() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    for id in 1..=5 {
        note::ActiveModel {
            id: Set(id),
            title: Set(format!("Note {id}")),
            cached_content: Set("[[Missing]]".to_string()),
            file_managed_by_app: Set(false),
            created_at: Set(id),
            updated_at: Set(id),
            ..Default::default()
        }
        .insert(&db)
        .await?;
    }

    let first = repair_workspace_link_index_batch(&db, 2).await?;
    assert_eq!(
        first.indexed_notes + first.indexed_workspace_notes + first.indexed_entries,
        2
    );
    assert!(first.has_more);

    let second = repair_workspace_link_index_batch(&db, 2).await?;
    assert_eq!(
        second.indexed_notes + second.indexed_workspace_notes + second.indexed_entries,
        2
    );
    let third = repair_workspace_link_index_batch(&db, 2).await?;
    assert_eq!(
        third.indexed_notes + third.indexed_workspace_notes + third.indexed_entries,
        1
    );
    assert!(!third.has_more);
    Ok(())
}
