use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result, bail};
use entity::{
    board, board::Entity as Board, card, card::Entity as Card, entry, entry::Entity as Entry, note,
    note::Entity as Note, note_alias, note_alias::Entity as NoteAlias, project,
    project::Entity as Project, saved_board_view, saved_board_view::Entity as SavedBoardView,
    workspace_link, workspace_link::Entity as WorkspaceLink, workspace_link_index_state,
    workspace_link_index_state::Entity as WorkspaceLinkIndexState,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, DbBackend,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Statement,
    TransactionSession, TransactionTrait,
};
mod model;

pub use model::*;

const ORIGIN_MANUAL: &str = "manual";
const ORIGIN_NOTE_WIKILINK: &str = "note_wikilink";
const ORIGIN_ENTRY_WIKILINK: &str = "entry_wikilink";

pub fn is_workspace_target(raw_target: &str) -> bool {
    prefixed_kind_and_id(raw_target).is_some_and(|(kind, _)| kind != WorkspaceItemKind::Note)
}

pub fn parse_workspace_target(raw_target: &str) -> Option<WorkspaceItemRef> {
    let (kind, id) = prefixed_kind_and_id(raw_target)?;
    (kind != WorkspaceItemKind::Note).then_some(WorkspaceItemRef { kind, id })
}

pub async fn load_existing_workspace_items(
    db: &impl ConnectionTrait,
    items: &[WorkspaceItemRef],
) -> Result<HashSet<WorkspaceItemRef>> {
    let ids = |kind| {
        items
            .iter()
            .filter(|item| item.kind == kind)
            .map(|item| item.id)
            .collect::<Vec<_>>()
    };
    let mut existing = HashSet::new();
    let board_ids = ids(WorkspaceItemKind::Board);
    if !board_ids.is_empty() {
        existing.extend(
            Board::find()
                .filter(board::Column::Id.is_in(board_ids))
                .select_only()
                .column(board::Column::Id)
                .into_tuple::<i64>()
                .all(db)
                .await?
                .into_iter()
                .map(|id| WorkspaceItemRef {
                    kind: WorkspaceItemKind::Board,
                    id,
                }),
        );
    }
    let list_ids = ids(WorkspaceItemKind::List);
    if !list_ids.is_empty() {
        existing.extend(
            Card::find()
                .filter(card::Column::Id.is_in(list_ids))
                .select_only()
                .column(card::Column::Id)
                .into_tuple::<i64>()
                .all(db)
                .await?
                .into_iter()
                .map(|id| WorkspaceItemRef {
                    kind: WorkspaceItemKind::List,
                    id,
                }),
        );
    }
    let card_ids = ids(WorkspaceItemKind::Card);
    if !card_ids.is_empty() {
        existing.extend(
            Entry::find()
                .filter(entry::Column::Id.is_in(card_ids))
                .select_only()
                .column(entry::Column::Id)
                .into_tuple::<i64>()
                .all(db)
                .await?
                .into_iter()
                .map(|id| WorkspaceItemRef {
                    kind: WorkspaceItemKind::Card,
                    id,
                }),
        );
    }
    Ok(existing)
}

pub(crate) async fn load_all_existing_workspace_items(
    db: &impl ConnectionTrait,
) -> Result<HashSet<WorkspaceItemRef>> {
    let mut existing = HashSet::new();
    existing.extend(
        Note::find()
            .select_only()
            .column(note::Column::Id)
            .into_tuple::<i64>()
            .all(db)
            .await?
            .into_iter()
            .map(|id| WorkspaceItemRef {
                kind: WorkspaceItemKind::Note,
                id,
            }),
    );
    existing.extend(
        Board::find()
            .select_only()
            .column(board::Column::Id)
            .into_tuple::<i64>()
            .all(db)
            .await?
            .into_iter()
            .map(|id| WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id,
            }),
    );
    existing.extend(
        Card::find()
            .select_only()
            .column(card::Column::Id)
            .into_tuple::<i64>()
            .all(db)
            .await?
            .into_iter()
            .map(|id| WorkspaceItemRef {
                kind: WorkspaceItemKind::List,
                id,
            }),
    );
    existing.extend(
        Entry::find()
            .select_only()
            .column(entry::Column::Id)
            .into_tuple::<i64>()
            .all(db)
            .await?
            .into_iter()
            .map(|id| WorkspaceItemRef {
                kind: WorkspaceItemKind::Card,
                id,
            }),
    );
    Ok(existing)
}

pub(crate) async fn load_saved_view_identities(
    db: &impl ConnectionTrait,
) -> Result<HashSet<(i64, i64)>> {
    Ok(SavedBoardView::find()
        .select_only()
        .column(saved_board_view::Column::Id)
        .column(saved_board_view::Column::BoardId)
        .into_tuple::<(i64, i64)>()
        .all(db)
        .await?
        .into_iter()
        .collect())
}

pub fn workspace_relation_signature(content: &str) -> Vec<String> {
    let mut signature = crate::note::links::parse_wikilinks(content)
        .into_iter()
        .filter(|link| is_workspace_target(&link.raw_target))
        .map(|link| format!("wikilink:{}", link.raw_target.trim().to_ascii_lowercase()))
        .collect::<std::collections::BTreeSet<_>>();
    signature.extend(
        crate::board::projection::parse_board_view_embeds(content)
            .into_iter()
            .map(|embed| {
                format!(
                    "embed:{}:{}",
                    embed.board_id,
                    embed
                        .view_id
                        .map_or_else(|| "all".to_string(), |view_id| view_id.to_string())
                )
            }),
    );
    signature.into_iter().collect()
}

pub fn resolve_stable_target<'a>(
    raw_target: &str,
    catalog: &'a [WorkspaceCatalogEntry],
) -> Option<&'a WorkspaceCatalogEntry> {
    resolve_prefixed_target(raw_target, catalog)
}

pub async fn load_workspace_link_catalog(
    db: &impl ConnectionTrait,
) -> Result<Vec<WorkspaceCatalogEntry>> {
    let projects = Project::find()
        .filter(project::Column::Archived.eq(false))
        .filter(project::Column::DeletedAt.is_null())
        .select_only()
        .column(project::Column::Id)
        .column(project::Column::Name)
        .into_tuple::<(i64, String)>()
        .all(db)
        .await?
        .into_iter()
        .collect::<HashMap<_, _>>();

    let notes = Note::find()
        .filter(note::Column::DeletedAt.is_null())
        .select_only()
        .column(note::Column::Id)
        .column(note::Column::Title)
        .column(note::Column::ProjectId)
        .into_tuple::<(i64, String, Option<i64>)>()
        .all(db)
        .await?;
    let boards = Board::find()
        .filter(board::Column::DeletedAt.is_null())
        .select_only()
        .column(board::Column::Id)
        .column(board::Column::Title)
        .column(board::Column::ProjectId)
        .into_tuple::<(i64, String, Option<i64>)>()
        .all(db)
        .await?
        .into_iter()
        .filter(|(_, _, project_id)| {
            project_id.is_none_or(|project_id| projects.contains_key(&project_id))
        })
        .collect::<Vec<_>>();
    let board_by_id = boards
        .iter()
        .map(|(id, title, project_id)| (*id, (title.clone(), *project_id)))
        .collect::<HashMap<_, _>>();
    let board_ids = board_by_id.keys().copied().collect::<Vec<_>>();
    let lists = if board_ids.is_empty() {
        Vec::new()
    } else {
        Card::find()
            .filter(card::Column::DeletedAt.is_null())
            .filter(card::Column::BoardId.is_in(board_ids))
            .order_by_asc(card::Column::Position)
            .select_only()
            .column(card::Column::Id)
            .column(card::Column::Title)
            .column(card::Column::BoardId)
            .into_tuple::<(i64, String, i64)>()
            .all(db)
            .await?
    };
    let list_by_id = lists
        .iter()
        .map(|(id, title, board_id)| (*id, (title.clone(), *board_id)))
        .collect::<HashMap<_, _>>();
    let list_ids = list_by_id.keys().copied().collect::<Vec<_>>();
    let cards = if list_ids.is_empty() {
        Vec::new()
    } else {
        Entry::find()
            .filter(entry::Column::DeletedAt.is_null())
            .filter(entry::Column::CardId.is_in(list_ids))
            .order_by_asc(entry::Column::Position)
            .select_only()
            .column(entry::Column::Id)
            .column(entry::Column::Title)
            .column(entry::Column::CardId)
            .into_tuple::<(i64, String, i64)>()
            .all(db)
            .await?
    };

    let mut catalog = Vec::with_capacity(notes.len() + boards.len() + lists.len() + cards.len());
    for (note_id, note_title, project_id) in notes {
        if project_id.is_some_and(|project_id| !projects.contains_key(&project_id)) {
            continue;
        }
        catalog.push(WorkspaceCatalogEntry {
            item: WorkspaceItemRef {
                kind: WorkspaceItemKind::Note,
                id: note_id,
            },
            title: note_title,
            project_id,
            project_name: project_id.and_then(|project_id| projects.get(&project_id).cloned()),
            board_id: None,
            board_title: None,
            list_id: None,
            list_title: None,
        });
    }
    for (board_id, board_title, project_id) in &boards {
        catalog.push(WorkspaceCatalogEntry {
            item: WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id: *board_id,
            },
            title: board_title.clone(),
            project_id: *project_id,
            project_name: project_id.and_then(|project_id| projects.get(&project_id).cloned()),
            board_id: Some(*board_id),
            board_title: Some(board_title.clone()),
            list_id: None,
            list_title: None,
        });
    }
    for (list_id, list_title, board_id) in &lists {
        let Some((board_title, project_id)) = board_by_id.get(board_id) else {
            continue;
        };
        catalog.push(WorkspaceCatalogEntry {
            item: WorkspaceItemRef {
                kind: WorkspaceItemKind::List,
                id: *list_id,
            },
            title: list_title.clone(),
            project_id: *project_id,
            project_name: project_id.and_then(|project_id| projects.get(&project_id).cloned()),
            board_id: Some(*board_id),
            board_title: Some(board_title.clone()),
            list_id: Some(*list_id),
            list_title: Some(list_title.clone()),
        });
    }
    for (card_id, card_title, list_id) in cards {
        let Some((list_title, board_id)) = list_by_id.get(&list_id) else {
            continue;
        };
        let Some((board_title, project_id)) = board_by_id.get(board_id) else {
            continue;
        };
        catalog.push(WorkspaceCatalogEntry {
            item: WorkspaceItemRef {
                kind: WorkspaceItemKind::Card,
                id: card_id,
            },
            title: card_title,
            project_id: *project_id,
            project_name: project_id.and_then(|project_id| projects.get(&project_id).cloned()),
            board_id: Some(*board_id),
            board_title: Some(board_title.clone()),
            list_id: Some(list_id),
            list_title: Some(list_title.clone()),
        });
    }
    Ok(catalog)
}

pub async fn link_note_to_item(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: i64,
    item: WorkspaceItemRef,
    created_at: i64,
) -> Result<()> {
    set_manual_note_link(db, note_id, item, true, created_at).await?;
    Ok(())
}

pub async fn set_manual_note_link(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: i64,
    item: WorkspaceItemRef,
    linked: bool,
    created_at: i64,
) -> Result<ManualLinkUpdate> {
    let txn = db.begin().await?;
    let changed =
        set_manual_note_link_in_connection(&txn, note_id, item, linked, created_at).await?;
    txn.commit().await?;
    Ok(ManualLinkUpdate {
        related_notes: load_related_notes(db, item).await?,
        changed,
    })
}

pub(crate) async fn set_manual_note_link_in_connection(
    db: &impl ConnectionTrait,
    note_id: i64,
    item: WorkspaceItemRef,
    linked: bool,
    created_at: i64,
) -> Result<bool> {
    if item.kind == WorkspaceItemKind::Note {
        bail!("manual workspace links require a board, list, or card target");
    }
    let catalog = load_workspace_link_catalog(db).await?;
    catalog_entry(&catalog, WorkspaceItemKind::Note, note_id)
        .with_context(|| format!("active note {note_id} was not found"))?;
    catalog_entry(&catalog, item.kind, item.id)
        .with_context(|| format!("active {} {} was not found", item.kind.as_str(), item.id))?;

    let existing = WorkspaceLink::find()
        .filter(workspace_link::Column::SourceNoteId.eq(note_id))
        .filter(workspace_link::Column::Origin.eq(ORIGIN_MANUAL))
        .filter(target_column(item.kind).eq(item.id))
        .one(db)
        .await?;
    let changed = match (linked, existing) {
        (true, None) => {
            workspace_link::ActiveModel {
                source_note_id: Set(Some(note_id)),
                origin: Set(ORIGIN_MANUAL.to_string()),
                ordinal: Set(0),
                created_at: Set(created_at),
                ..target_active_model(item)
            }
            .insert(db)
            .await?;
            true
        }
        (false, Some(existing)) => {
            WorkspaceLink::delete_by_id(existing.id).exec(db).await?;
            true
        }
        _ => false,
    };
    Ok(changed)
}

pub async fn create_card_from_note_selection(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: i64,
    list_id: i64,
    title: String,
    created_at: i64,
) -> Result<CreatedLinkedCard> {
    let title = title.trim();
    if title.is_empty() {
        bail!("card title must not be empty");
    }
    let catalog = load_workspace_link_catalog(db).await?;
    catalog_entry(&catalog, WorkspaceItemKind::Note, note_id)
        .with_context(|| format!("active note {note_id} was not found"))?;
    let list = catalog_entry(&catalog, WorkspaceItemKind::List, list_id)
        .with_context(|| format!("active list {list_id} was not found"))?;
    let board_id = list
        .board_id
        .with_context(|| format!("list {list_id} has no active board"))?;
    let position = Entry::find()
        .filter(entry::Column::CardId.eq(list_id))
        .filter(entry::Column::DeletedAt.is_null())
        .count(db)
        .await? as i32;
    let txn = db.begin().await?;
    let entry = entry::ActiveModel {
        title: Set(title.to_string()),
        description: Set(String::new()),
        card_id: Set(list_id),
        position: Set(position),
        ..Default::default()
    }
    .insert(&txn)
    .await?;
    workspace_link::ActiveModel {
        source_note_id: Set(Some(note_id)),
        target_entry_id: Set(Some(entry.id)),
        origin: Set(ORIGIN_MANUAL.to_string()),
        ordinal: Set(0),
        created_at: Set(created_at),
        ..Default::default()
    }
    .insert(&txn)
    .await?;
    update_index_state(&txn, "entry", entry.id, "").await?;
    txn.commit().await?;
    Ok(CreatedLinkedCard {
        entry_id: entry.id,
        board_id,
        list_id,
    })
}

pub async fn unlink_note_from_item(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: i64,
    item: WorkspaceItemRef,
) -> Result<bool> {
    Ok(set_manual_note_link(db, note_id, item, false, 0)
        .await?
        .changed)
}

pub async fn index_note_workspace_links(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: i64,
    content: &str,
    indexed_at: i64,
) -> Result<()> {
    let txn = db.begin().await?;
    index_note_workspace_links_in_connection(&txn, note_id, content, indexed_at).await?;
    txn.commit().await?;
    Ok(())
}

pub(crate) async fn index_note_workspace_links_in_connection(
    db: &impl ConnectionTrait,
    note_id: i64,
    content: &str,
    indexed_at: i64,
) -> Result<()> {
    let catalog = load_workspace_link_catalog(db).await?;
    let mut requested_targets = crate::note::links::parse_wikilinks(content)
        .iter()
        .filter_map(|link| parse_workspace_target(&link.raw_target))
        .collect::<Vec<_>>();
    requested_targets.extend(
        crate::board::projection::parse_board_view_embeds(content)
            .into_iter()
            .map(|embed| WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id: embed.board_id,
            }),
    );
    let existing_targets = load_existing_workspace_items(db, &requested_targets).await?;
    let saved_views = load_saved_view_identities(db).await?;
    index_note_workspace_links_with_catalog(
        db,
        note_id,
        content,
        indexed_at,
        &catalog,
        &existing_targets,
        &saved_views,
    )
    .await
}

pub(crate) async fn index_note_workspace_links_with_catalog(
    db: &impl ConnectionTrait,
    note_id: i64,
    content: &str,
    indexed_at: i64,
    catalog: &[WorkspaceCatalogEntry],
    existing_targets: &HashSet<WorkspaceItemRef>,
    saved_views: &HashSet<(i64, i64)>,
) -> Result<()> {
    let source = catalog_entry(catalog, WorkspaceItemKind::Note, note_id)
        .with_context(|| format!("active note {note_id} was not found"))?;
    let parsed = crate::note::links::parse_wikilinks(content)
        .into_iter()
        .filter(|link| is_workspace_target(&link.raw_target))
        .collect::<Vec<_>>();
    let embeds = crate::board::projection::parse_board_view_embeds(content)
        .into_iter()
        .filter_map(|embed| {
            let board = WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id: embed.board_id,
            };
            (existing_targets.contains(&board)
                && embed
                    .view_id
                    .is_none_or(|view_id| saved_views.contains(&(view_id, embed.board_id))))
            .then_some((embed, board))
        })
        .collect::<Vec<_>>();

    WorkspaceLink::delete_many()
        .filter(workspace_link::Column::SourceNoteId.eq(note_id))
        .filter(workspace_link::Column::Origin.eq(ORIGIN_NOTE_WIKILINK))
        .exec(db)
        .await?;
    WorkspaceLink::delete_many()
        .filter(workspace_link::Column::SourceNoteId.eq(note_id))
        .filter(workspace_link::Column::Origin.eq("embed"))
        .exec(db)
        .await?;
    for (ordinal, link) in parsed.into_iter().enumerate() {
        let Some(target) = parse_workspace_target(&link.raw_target)
            .filter(|target| existing_targets.contains(target))
        else {
            continue;
        };
        let mut model = target_active_model(target);
        model.source_note_id = Set(Some(note_id));
        model.origin = Set(ORIGIN_NOTE_WIKILINK.to_string());
        model.ordinal = Set(ordinal as i32);
        model.raw_target = Set(Some(link.raw_target));
        model.display_text = Set(link.display_text);
        model.start_byte = Set(Some(link.start_byte as i64));
        model.end_byte = Set(Some(link.end_byte as i64));
        model.line_number = Set(Some(link.line_number as i32));
        model.created_at = Set(indexed_at);
        model.insert(db).await?;
    }
    for (ordinal, (embed, target)) in embeds.into_iter().enumerate() {
        let mut model = target_active_model(target);
        model.source_note_id = Set(Some(note_id));
        model.target_saved_view_id = Set(embed.view_id);
        model.origin = Set("embed".to_string());
        model.ordinal = Set(ordinal as i32);
        model.raw_target = Set(Some(format!("board:{}", embed.board_id)));
        model.display_text = Set(embed.fallback_title);
        model.start_byte = Set(Some(embed.start_byte as i64));
        model.end_byte = Set(Some(embed.end_byte as i64));
        model.created_at = Set(indexed_at);
        model.insert(db).await?;
    }
    update_index_state(db, "note", source.item.id, content).await?;
    Ok(())
}

pub async fn index_entry_workspace_links(
    db: &(impl ConnectionTrait + TransactionTrait),
    entry_id: i64,
    description: &str,
    indexed_at: i64,
) -> Result<()> {
    let txn = db.begin().await?;
    index_entry_workspace_links_in_connection(&txn, entry_id, description, indexed_at).await?;
    txn.commit().await?;
    Ok(())
}

pub async fn index_entry_workspace_links_in_connection(
    db: &impl ConnectionTrait,
    entry_id: i64,
    description: &str,
    indexed_at: i64,
) -> Result<()> {
    let catalog = load_workspace_link_catalog(db).await?;
    let aliases = NoteAlias::find().all(db).await?;
    let parsed = crate::note::links::parse_wikilinks(description);
    let stable_targets = parsed
        .iter()
        .filter_map(|link| parse_workspace_target(&link.raw_target))
        .collect::<Vec<_>>();
    let existing_targets = load_existing_workspace_items(db, &stable_targets).await?;

    index_entry_workspace_links_with_catalog(
        db,
        entry_id,
        description,
        indexed_at,
        &catalog,
        &aliases,
        &existing_targets,
    )
    .await
}

pub(crate) async fn index_entry_workspace_links_with_catalog(
    db: &impl ConnectionTrait,
    entry_id: i64,
    description: &str,
    indexed_at: i64,
    catalog: &[WorkspaceCatalogEntry],
    aliases: &[note_alias::Model],
    existing_targets: &HashSet<WorkspaceItemRef>,
) -> Result<()> {
    let source = catalog_entry(catalog, WorkspaceItemKind::Card, entry_id)
        .with_context(|| format!("active card {entry_id} was not found"))?;
    let parsed = crate::note::links::parse_wikilinks(description);
    WorkspaceLink::delete_many()
        .filter(workspace_link::Column::SourceEntryId.eq(entry_id))
        .filter(workspace_link::Column::Origin.eq(ORIGIN_ENTRY_WIKILINK))
        .exec(db)
        .await?;
    for (ordinal, link) in parsed.into_iter().enumerate() {
        let target = parse_workspace_target(&link.raw_target)
            .filter(|target| existing_targets.contains(target))
            .or_else(|| {
                resolve_note_target(&link.raw_target, source.project_id, catalog, aliases)
                    .map(|entry| entry.item)
            });
        let Some(target) = target else {
            continue;
        };
        let mut model = target_active_model(target);
        model.source_entry_id = Set(Some(entry_id));
        model.origin = Set(ORIGIN_ENTRY_WIKILINK.to_string());
        model.ordinal = Set(ordinal as i32);
        model.raw_target = Set(Some(link.raw_target));
        model.display_text = Set(link.display_text);
        model.start_byte = Set(Some(link.start_byte as i64));
        model.end_byte = Set(Some(link.end_byte as i64));
        model.line_number = Set(Some(link.line_number as i32));
        model.created_at = Set(indexed_at);
        model.insert(db).await?;
    }
    update_index_state(db, "entry", entry_id, description).await?;
    Ok(())
}

pub async fn load_related_notes(
    db: &(impl ConnectionTrait + TransactionTrait),
    item: WorkspaceItemRef,
) -> Result<Vec<RelatedNote>> {
    if item.kind == WorkspaceItemKind::Note {
        bail!("related notes require a board, list, or card target");
    }
    load_related_notes_for_items(db, &[item])
        .await?
        .remove(&item)
        .with_context(|| format!("active {} {} was not found", item.kind.as_str(), item.id))
}

pub async fn load_related_notes_for_entries(
    db: &(impl ConnectionTrait + TransactionTrait),
    entry_ids: &[i64],
) -> Result<HashMap<i64, Vec<RelatedNote>>> {
    let items = entry_ids
        .iter()
        .copied()
        .map(|id| WorkspaceItemRef {
            kind: WorkspaceItemKind::Card,
            id,
        })
        .collect::<Vec<_>>();
    Ok(load_related_notes_for_items(db, &items)
        .await?
        .into_iter()
        .map(|(item, notes)| (item.id, notes))
        .collect())
}

pub async fn load_related_notes_for_items(
    db: &(impl ConnectionTrait + TransactionTrait),
    items: &[WorkspaceItemRef],
) -> Result<HashMap<WorkspaceItemRef, Vec<RelatedNote>>> {
    let requested = items
        .iter()
        .copied()
        .filter(|item| item.kind != WorkspaceItemKind::Note)
        .collect::<HashSet<_>>();
    if requested.is_empty() {
        return Ok(HashMap::new());
    }
    let catalog = load_workspace_link_catalog(db).await?;
    let active_items = catalog
        .iter()
        .filter(|entry| requested.contains(&entry.item))
        .map(|entry| (entry.item, entry))
        .collect::<HashMap<_, _>>();
    if active_items.is_empty() {
        return Ok(HashMap::new());
    }
    let board_ids = active_items
        .keys()
        .filter(|item| item.kind == WorkspaceItemKind::Board)
        .map(|item| item.id)
        .collect::<Vec<_>>();
    let list_ids = active_items
        .keys()
        .filter(|item| item.kind == WorkspaceItemKind::List)
        .map(|item| item.id)
        .collect::<Vec<_>>();
    let card_ids = active_items
        .keys()
        .filter(|item| item.kind == WorkspaceItemKind::Card)
        .map(|item| item.id)
        .collect::<Vec<_>>();
    let mut condition = Condition::any();
    if !board_ids.is_empty() {
        condition = condition.add(workspace_link::Column::TargetBoardId.is_in(board_ids));
    }
    if !list_ids.is_empty() {
        condition = condition.add(workspace_link::Column::TargetCardId.is_in(list_ids));
    }
    if !card_ids.is_empty() {
        condition = condition
            .add(workspace_link::Column::TargetEntryId.is_in(card_ids.clone()))
            .add(
                Condition::all()
                    .add(workspace_link::Column::SourceEntryId.is_in(card_ids))
                    .add(workspace_link::Column::TargetNoteId.is_not_null()),
            );
    }
    let links = WorkspaceLink::find().filter(condition).all(db).await?;
    let notes = catalog
        .iter()
        .filter(|entry| entry.item.kind == WorkspaceItemKind::Note)
        .map(|entry| (entry.item.id, entry))
        .collect::<HashMap<_, _>>();
    let mut grouped = active_items
        .keys()
        .copied()
        .map(|item| (item, HashMap::<i64, RelatedNote>::new()))
        .collect::<HashMap<_, _>>();
    for link in links {
        let item = link
            .target_board_id
            .map(|id| WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id,
            })
            .or_else(|| {
                link.target_card_id.map(|id| WorkspaceItemRef {
                    kind: WorkspaceItemKind::List,
                    id,
                })
            })
            .or_else(|| {
                link.target_entry_id.map(|id| WorkspaceItemRef {
                    kind: WorkspaceItemKind::Card,
                    id,
                })
            })
            .or_else(|| {
                link.source_entry_id.map(|id| WorkspaceItemRef {
                    kind: WorkspaceItemKind::Card,
                    id,
                })
            });
        let Some(item) = item.filter(|item| active_items.contains_key(item)) else {
            continue;
        };
        let Some(note_id) = link.source_note_id.or(link.target_note_id) else {
            continue;
        };
        let Some(note) = notes.get(&note_id).copied() else {
            continue;
        };
        let origin = link_origin(&link.origin);
        let related = grouped.entry(item).or_default();
        let row = related.entry(note_id).or_insert_with(|| RelatedNote {
            note_id,
            title: note.title.clone(),
            project_id: note.project_id,
            project_name: note.project_name.clone(),
            origins: Vec::new(),
        });
        if !row.origins.contains(&origin) {
            row.origins.push(origin);
        }
    }
    Ok(grouped
        .into_iter()
        .map(|(item, notes)| {
            let project_id = active_items.get(&item).and_then(|entry| entry.project_id);
            let mut notes = notes.into_values().collect::<Vec<_>>();
            notes.sort_by_key(|note| {
                (
                    note.project_id != project_id,
                    note.title.to_lowercase(),
                    note.note_id,
                )
            });
            (item, notes)
        })
        .collect())
}

pub async fn load_note_workspace_links(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: i64,
) -> Result<NoteWorkspaceLinks> {
    let catalog = load_workspace_link_catalog(db).await?;
    catalog_entry(&catalog, WorkspaceItemKind::Note, note_id)
        .with_context(|| format!("active note {note_id} was not found"))?;
    let links = WorkspaceLink::find()
        .filter(
            Condition::any()
                .add(workspace_link::Column::SourceNoteId.eq(note_id))
                .add(workspace_link::Column::TargetNoteId.eq(note_id)),
        )
        .order_by_asc(workspace_link::Column::Id)
        .all(db)
        .await?;
    let mut references = Vec::new();
    for link in links {
        let inbound = link.target_note_id == Some(note_id);
        let item = if inbound {
            link.source_entry_id
                .and_then(|id| catalog_entry(&catalog, WorkspaceItemKind::Card, id).cloned())
        } else {
            target_item(&link).and_then(|item| {
                (item.kind != WorkspaceItemKind::Note)
                    .then(|| catalog_entry(&catalog, item.kind, item.id).cloned())
                    .flatten()
            })
        };
        let Some(item) = item else {
            continue;
        };
        references.push(WorkspaceLinkReference {
            item,
            origin: link_origin(&link.origin),
            source_offset: link.start_byte.map(|offset| offset.max(0) as usize),
            line_number: link.line_number.map(|line| line.max(1) as usize),
            inbound,
        });
    }
    references.sort_by_key(|reference| {
        (
            reference.item.item.kind.as_str(),
            reference.item.breadcrumb().to_lowercase(),
        )
    });
    Ok(NoteWorkspaceLinks { references })
}

pub async fn reindex_stale_note_workspace_links(
    db: &(impl ConnectionTrait + TransactionTrait),
    limit: u64,
) -> Result<usize> {
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT n.id, n.cached_content, n.updated_at
            FROM note n
            LEFT JOIN project p ON p.id = n.project_id
            LEFT JOIN workspace_link_index_state s
              ON s.source_kind = 'note' AND s.source_id = n.id
            WHERE n.deleted_at IS NULL
              AND (n.project_id IS NULL OR (p.deleted_at IS NULL AND p.archived = 0))
              AND (s.source_id IS NULL OR s.indexed_content != n.cached_content)
            ORDER BY n.id
            LIMIT ?
            "#,
            [(limit.max(1) as i64).into()],
        ))
        .await?;
    let count = rows.len();
    for row in rows {
        let note_id = row.try_get::<i64>("", "id")?;
        let content = row.try_get::<String>("", "cached_content")?;
        let updated_at = row.try_get::<i64>("", "updated_at")?;
        index_note_workspace_links(db, note_id, &content, updated_at).await?;
    }
    Ok(count)
}

pub async fn reindex_stale_entry_links(
    db: &(impl ConnectionTrait + TransactionTrait),
    limit: u64,
) -> Result<usize> {
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT e.id, e.description
            FROM entry e
            JOIN card c ON c.id = e.card_id AND c.deleted_at IS NULL
            JOIN board b ON b.id = c.board_id AND b.deleted_at IS NULL
            LEFT JOIN project p ON p.id = b.project_id
            LEFT JOIN workspace_link_index_state s
              ON s.source_kind = 'entry' AND s.source_id = e.id
            WHERE e.deleted_at IS NULL
              AND (b.project_id IS NULL OR (p.deleted_at IS NULL AND p.archived = 0))
              AND (s.source_id IS NULL OR s.indexed_content != e.description)
            ORDER BY e.id
            LIMIT ?
            "#,
            [(limit.max(1) as i64).into()],
        ))
        .await?;
    let count = rows.len();
    for row in rows {
        let entry_id = row.try_get::<i64>("", "id")?;
        let description = row.try_get::<String>("", "description")?;
        index_entry_workspace_links(db, entry_id, &description, 0).await?;
    }
    Ok(count)
}

pub async fn repair_workspace_link_index_batch(
    db: &(impl ConnectionTrait + TransactionTrait),
    limit: u64,
) -> Result<WorkspaceLinkRepairBatch> {
    let limit = limit.max(1);
    let note_catalog = crate::note::links::load_note_link_catalog(db).await?;
    let workspace_catalog = load_workspace_link_catalog(db).await?;
    let aliases = NoteAlias::find().all(db).await?;
    let existing_items = load_all_existing_workspace_items(db).await?;
    let saved_views = load_saved_view_identities(db).await?;

    let note_rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT n.id, n.project_id, n.cached_content, n.updated_at
            FROM note n
            LEFT JOIN note_link_index_state s ON s.note_id = n.id
            WHERE n.deleted_at IS NULL
              AND (s.note_id IS NULL OR s.indexed_updated_at != n.updated_at)
            ORDER BY n.id
            LIMIT ?
            "#,
            [(limit as i64).into()],
        ))
        .await?;
    let indexed_notes = note_rows.len();
    for row in note_rows {
        let note_id = row.try_get::<i64>("", "id")?;
        let project_id = row.try_get::<Option<i64>>("", "project_id")?;
        let content = row.try_get::<String>("", "cached_content")?;
        let updated_at = row.try_get::<i64>("", "updated_at")?;
        let txn = db.begin().await?;
        crate::note::links::index_note_links_with_catalog(
            &txn,
            note_id,
            project_id,
            &content,
            updated_at,
            crate::note::links::NoteIndexCatalogs {
                note_links: &note_catalog,
                aliases: &aliases,
                workspace: &workspace_catalog,
                existing_workspace_items: &existing_items,
                saved_views: &saved_views,
            },
        )
        .await?;
        txn.commit().await?;
    }

    let remaining = limit.saturating_sub(indexed_notes as u64);
    let workspace_note_rows = if remaining == 0 {
        Vec::new()
    } else {
        db.query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT n.id, n.cached_content, n.updated_at
            FROM note n
            LEFT JOIN project p ON p.id = n.project_id
            LEFT JOIN workspace_link_index_state s
              ON s.source_kind = 'note' AND s.source_id = n.id
            WHERE n.deleted_at IS NULL
              AND (n.project_id IS NULL OR (p.deleted_at IS NULL AND p.archived = 0))
              AND (s.source_id IS NULL OR s.indexed_content != n.cached_content)
            ORDER BY n.id
            LIMIT ?
            "#,
            [(remaining as i64).into()],
        ))
        .await?
    };
    let indexed_workspace_notes = workspace_note_rows.len();
    for row in workspace_note_rows {
        let note_id = row.try_get::<i64>("", "id")?;
        let content = row.try_get::<String>("", "cached_content")?;
        let updated_at = row.try_get::<i64>("", "updated_at")?;
        let txn = db.begin().await?;
        index_note_workspace_links_with_catalog(
            &txn,
            note_id,
            &content,
            updated_at,
            &workspace_catalog,
            &existing_items,
            &saved_views,
        )
        .await?;
        txn.commit().await?;
    }

    let remaining = remaining.saturating_sub(indexed_workspace_notes as u64);
    let entry_rows = if remaining == 0 {
        Vec::new()
    } else {
        db.query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT e.id, e.description
            FROM entry e
            JOIN card c ON c.id = e.card_id AND c.deleted_at IS NULL
            JOIN board b ON b.id = c.board_id AND b.deleted_at IS NULL
            LEFT JOIN project p ON p.id = b.project_id
            LEFT JOIN workspace_link_index_state s
              ON s.source_kind = 'entry' AND s.source_id = e.id
            WHERE e.deleted_at IS NULL
              AND (b.project_id IS NULL OR (p.deleted_at IS NULL AND p.archived = 0))
              AND (s.source_id IS NULL OR s.indexed_content != e.description)
            ORDER BY e.id
            LIMIT ?
            "#,
            [(remaining as i64).into()],
        ))
        .await?
    };
    let indexed_entries = entry_rows.len();
    for row in entry_rows {
        let entry_id = row.try_get::<i64>("", "id")?;
        let description = row.try_get::<String>("", "description")?;
        let txn = db.begin().await?;
        index_entry_workspace_links_with_catalog(
            &txn,
            entry_id,
            &description,
            0,
            &workspace_catalog,
            &aliases,
            &existing_items,
        )
        .await?;
        txn.commit().await?;
    }
    let indexed_total = indexed_notes + indexed_workspace_notes + indexed_entries;
    Ok(WorkspaceLinkRepairBatch {
        indexed_notes,
        indexed_workspace_notes,
        indexed_entries,
        has_more: indexed_total as u64 == limit,
    })
}

fn target_active_model(item: WorkspaceItemRef) -> workspace_link::ActiveModel {
    let mut model = <workspace_link::ActiveModel as Default>::default();
    match item.kind {
        WorkspaceItemKind::Note => model.target_note_id = Set(Some(item.id)),
        WorkspaceItemKind::Board => model.target_board_id = Set(Some(item.id)),
        WorkspaceItemKind::List => model.target_card_id = Set(Some(item.id)),
        WorkspaceItemKind::Card => model.target_entry_id = Set(Some(item.id)),
    }
    model
}

async fn update_index_state(
    db: &impl ConnectionTrait,
    source_kind: &str,
    source_id: i64,
    content: &str,
) -> Result<()> {
    WorkspaceLinkIndexState::delete_by_id((source_kind.to_string(), source_id))
        .exec(db)
        .await?;
    workspace_link_index_state::ActiveModel {
        source_kind: Set(source_kind.to_string()),
        source_id: Set(source_id),
        indexed_content: Set(content.to_string()),
    }
    .insert(db)
    .await?;
    Ok(())
}

fn target_column(kind: WorkspaceItemKind) -> workspace_link::Column {
    match kind {
        WorkspaceItemKind::Note => workspace_link::Column::TargetNoteId,
        WorkspaceItemKind::Board => workspace_link::Column::TargetBoardId,
        WorkspaceItemKind::List => workspace_link::Column::TargetCardId,
        WorkspaceItemKind::Card => workspace_link::Column::TargetEntryId,
    }
}

fn target_item(link: &workspace_link::Model) -> Option<WorkspaceItemRef> {
    link.target_note_id
        .map(|id| WorkspaceItemRef {
            kind: WorkspaceItemKind::Note,
            id,
        })
        .or_else(|| {
            link.target_board_id.map(|id| WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id,
            })
        })
        .or_else(|| {
            link.target_card_id.map(|id| WorkspaceItemRef {
                kind: WorkspaceItemKind::List,
                id,
            })
        })
        .or_else(|| {
            link.target_entry_id.map(|id| WorkspaceItemRef {
                kind: WorkspaceItemKind::Card,
                id,
            })
        })
}

fn link_origin(origin: &str) -> WorkspaceLinkOrigin {
    match origin {
        ORIGIN_MANUAL => WorkspaceLinkOrigin::Manual,
        "embed" => WorkspaceLinkOrigin::Embed,
        _ => WorkspaceLinkOrigin::Wikilink,
    }
}

fn prefixed_kind_and_id(raw_target: &str) -> Option<(WorkspaceItemKind, i64)> {
    let (prefix, id) = raw_target.trim().split_once(':')?;
    let kind = match prefix.trim().to_ascii_lowercase().as_str() {
        "note" => WorkspaceItemKind::Note,
        "board" => WorkspaceItemKind::Board,
        "list" => WorkspaceItemKind::List,
        "card" => WorkspaceItemKind::Card,
        _ => return None,
    };
    Some((kind, id.trim().parse().ok()?))
}

fn resolve_prefixed_target<'a>(
    raw_target: &str,
    catalog: &'a [WorkspaceCatalogEntry],
) -> Option<&'a WorkspaceCatalogEntry> {
    let (kind, id) = prefixed_kind_and_id(raw_target)?;
    catalog_entry(catalog, kind, id)
}

fn resolve_note_target<'a>(
    raw_target: &str,
    source_project_id: Option<i64>,
    catalog: &'a [WorkspaceCatalogEntry],
    aliases: &[note_alias::Model],
) -> Option<&'a WorkspaceCatalogEntry> {
    if let Some(target) = resolve_prefixed_target(raw_target, catalog) {
        return (target.item.kind == WorkspaceItemKind::Note).then_some(target);
    }
    if let Some((project_name, title)) = raw_target.split_once('/') {
        let project = normalize(project_name);
        let title = normalize(title);
        return unique_entry(catalog.iter().filter(|entry| {
            entry.item.kind == WorkspaceItemKind::Note
                && normalize(&entry.title) == title
                && entry
                    .project_name
                    .as_deref()
                    .is_some_and(|name| normalize(name) == project)
        }));
    }
    let target = normalize(raw_target);
    if let Some(entry) = unique_entry(catalog.iter().filter(|entry| {
        entry.item.kind == WorkspaceItemKind::Note
            && entry.project_id == source_project_id
            && normalize(&entry.title) == target
    })) {
        return Some(entry);
    }
    if let Some(entry) = unique_entry(catalog.iter().filter(|entry| {
        entry.item.kind == WorkspaceItemKind::Note && normalize(&entry.title) == target
    })) {
        return Some(entry);
    }
    let alias_ids = aliases
        .iter()
        .filter(|alias| alias.normalized_alias == target)
        .map(|alias| alias.note_id)
        .collect::<HashSet<_>>();
    (alias_ids.len() == 1)
        .then(|| {
            alias_ids
                .into_iter()
                .next()
                .and_then(|note_id| catalog_entry(catalog, WorkspaceItemKind::Note, note_id))
        })
        .flatten()
}

fn unique_entry<'a>(
    mut entries: impl Iterator<Item = &'a WorkspaceCatalogEntry>,
) -> Option<&'a WorkspaceCatalogEntry> {
    let first = entries.next()?;
    entries.next().is_none().then_some(first)
}

fn catalog_entry(
    catalog: &[WorkspaceCatalogEntry],
    kind: WorkspaceItemKind,
    id: i64,
) -> Option<&WorkspaceCatalogEntry> {
    catalog
        .iter()
        .find(|entry| entry.item.kind == kind && entry.item.id == id)
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

#[cfg(test)]
mod tests;
