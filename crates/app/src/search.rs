use std::collections::HashMap;

use entity::{
    board, board::Entity as Board, card, card::Entity as Card, entry, entry::Entity as Entry, note,
    note::Entity as Note, project, project::Entity as Project,
};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DbBackend, DbErr, EntityTrait,
    QueryFilter, QuerySelect, Statement, TransactionTrait, Value,
    sea_query::{Query, SelectStatement},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SearchResultKind {
    Note,
    Board,
    Card,
    Entry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchResult {
    pub(crate) kind: SearchResultKind,
    pub(crate) item_id: u32,
    pub(crate) open_id: u32,
    pub(crate) project_id: Option<u32>,
    pub(crate) title: String,
    pub(crate) parent_title: Option<String>,
    pub(crate) highlighted_title: String,
    pub(crate) snippet: String,
    pub(crate) preview: String,
}

fn active_project_ids_query() -> SelectStatement {
    Query::select()
        .column(project::Column::Id)
        .from(Project)
        .and_where(project::Column::DeletedAt.is_null())
        .to_owned()
}

fn active_board_ids_query() -> SelectStatement {
    Query::select()
        .column(board::Column::Id)
        .from(Board)
        .and_where(board::Column::DeletedAt.is_null())
        .cond_where(
            Condition::any()
                .add(board::Column::ProjectId.is_null())
                .add(board::Column::ProjectId.in_subquery(active_project_ids_query())),
        )
        .to_owned()
}

fn active_card_ids_query() -> SelectStatement {
    Query::select()
        .column(card::Column::Id)
        .from(Card)
        .and_where(card::Column::DeletedAt.is_null())
        .and_where(card::Column::BoardId.in_subquery(active_board_ids_query()))
        .to_owned()
}

pub(crate) async fn rebuild_search_index(db: &DatabaseConnection) -> Result<(), DbErr> {
    let notes = Note::find()
        .filter(note::Column::DeletedAt.is_null())
        .filter(
            Condition::any()
                .add(note::Column::ProjectId.is_null())
                .add(note::Column::ProjectId.in_subquery(active_project_ids_query())),
        )
        .select_only()
        .column(note::Column::Id)
        .column(note::Column::ProjectId)
        .column(note::Column::Title)
        .column(note::Column::CachedContent)
        .into_tuple::<(i64, Option<i64>, String, String)>()
        .all(db)
        .await?;

    let boards = Board::find()
        .filter(board::Column::DeletedAt.is_null())
        .filter(
            Condition::any()
                .add(board::Column::ProjectId.is_null())
                .add(board::Column::ProjectId.in_subquery(active_project_ids_query())),
        )
        .select_only()
        .column(board::Column::Id)
        .column(board::Column::ProjectId)
        .column(board::Column::Title)
        .into_tuple::<(i64, Option<i64>, String)>()
        .all(db)
        .await?;

    let board_projects = boards
        .iter()
        .map(|(id, project_id, _)| (*id, *project_id))
        .collect::<HashMap<_, _>>();

    let cards = Card::find()
        .filter(card::Column::DeletedAt.is_null())
        .filter(card::Column::BoardId.in_subquery(active_board_ids_query()))
        .select_only()
        .column(card::Column::Id)
        .column(card::Column::BoardId)
        .column(card::Column::Title)
        .column(card::Column::Position)
        .into_tuple::<(i64, i64, String, i32)>()
        .all(db)
        .await?;

    let card_boards = cards
        .iter()
        .map(|(id, board_id, _, _)| (*id, *board_id))
        .collect::<HashMap<_, _>>();

    let mut cards_by_board: HashMap<i64, Vec<CardSearchSource>> = HashMap::new();
    for (index, (id, board_id, _, position)) in cards.iter().enumerate() {
        cards_by_board
            .entry(*board_id)
            .or_default()
            .push(CardSearchSource {
                index,
                id: *id,
                position: *position,
            });
    }

    for cards in cards_by_board.values_mut() {
        cards.sort_by_key(|card| (card.position, card.id));
    }

    let entries = Entry::find()
        .filter(entry::Column::DeletedAt.is_null())
        .filter(entry::Column::CardId.in_subquery(active_card_ids_query()))
        .select_only()
        .column(entry::Column::Id)
        .column(entry::Column::CardId)
        .column(entry::Column::Title)
        .column(entry::Column::Description)
        .column(entry::Column::Position)
        .into_tuple::<(i64, i64, String, String, i32)>()
        .all(db)
        .await?;

    let mut entries_by_card: HashMap<i64, Vec<EntrySearchSource>> = HashMap::new();
    for (id, card_id, title, description, position) in entries {
        entries_by_card
            .entry(card_id)
            .or_default()
            .push(EntrySearchSource {
                id,
                title,
                description,
                position,
            });
    }

    for entries in entries_by_card.values_mut() {
        entries.sort_by_key(|entry| (entry.position, entry.id));
    }

    let txn = db.begin().await?;
    txn.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM search_index",
        [],
    ))
    .await?;

    insert_search_documents(
        &txn,
        notes
            .into_iter()
            .map(|(id, project_id, title, content)| SearchDocument {
                item_type: "note",
                item_id: id,
                parent_id: Some(id),
                project_id,
                title,
                body: content,
            }),
    )
    .await?;

    insert_search_documents(
        &txn,
        boards
            .into_iter()
            .map(|(id, project_id, title)| SearchDocument {
                item_type: "board",
                item_id: id,
                parent_id: Some(id),
                project_id,
                title,
                body: search_board_body(
                    &cards,
                    cards_by_board.get(&id).map(Vec::as_slice),
                    &entries_by_card,
                ),
            }),
    )
    .await?;

    insert_search_documents(
        &txn,
        cards.into_iter().map(|(id, board_id, title, _)| {
            let body = search_card_body(&title, entries_by_card.get(&id).map(Vec::as_slice));

            SearchDocument {
                item_type: "card",
                item_id: id,
                parent_id: Some(board_id),
                project_id: board_projects.get(&board_id).copied().flatten(),
                title,
                body,
            }
        }),
    )
    .await?;

    insert_search_documents(
        &txn,
        entries_by_card.into_iter().flat_map(|(card_id, entries)| {
            let parent = card_boards
                .get(&card_id)
                .copied()
                .map(|board_id| (board_id, board_projects.get(&board_id).copied().flatten()));

            entries.into_iter().filter_map(move |entry| {
                let (board_id, project_id) = parent?;
                Some(SearchDocument {
                    item_type: "entry",
                    item_id: entry.id,
                    parent_id: Some(board_id),
                    project_id,
                    title: entry.title,
                    body: entry.description,
                })
            })
        }),
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

pub(crate) async fn search_workspace(
    db: &DatabaseConnection,
    query: &str,
    limit: u32,
) -> Result<Vec<SearchResult>, DbErr> {
    let rows = if let Some(query) = fts_query(query) {
        db.query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT
                item_type,
                item_id,
                COALESCE(parent_id, item_id) AS open_id,
                project_id,
                title,
                highlight(search_index, 4, char(1), char(2)) AS highlighted_title,
                snippet(search_index, 5, char(1), char(2), '...', 18) AS snippet,
                substr(
                    highlight(search_index, 5, char(1), char(2)),
                    max(
                        instr(highlight(search_index, 5, char(1), char(2)), char(1)) - 1200,
                        1
                    ),
                    5000
                ) AS preview
             FROM search_index
             WHERE search_index MATCH ?
             ORDER BY bm25(search_index)
             LIMIT ?",
            [query.into(), (limit as i64).into()],
        ))
        .await?
    } else {
        db.query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT
                item_type,
                item_id,
                COALESCE(parent_id, item_id) AS open_id,
                project_id,
                title,
                title AS highlighted_title,
                CASE
                    WHEN body = '' THEN title
                    ELSE substr(body, 1, 160)
                END AS snippet,
                substr(body, 1, 8000) AS preview
             FROM search_index
             ORDER BY rowid DESC
             LIMIT ?",
            [(limit as i64).into()],
        ))
        .await?
    };

    let mut search_rows = Vec::with_capacity(rows.len());
    let mut board_title_ids = Vec::new();

    for row in rows {
        let item_type: String = row.try_get("", "item_type")?;
        let open_id: i64 = row.try_get("", "open_id")?;

        if !board_title_ids.contains(&open_id) {
            board_title_ids.push(open_id);
        }

        search_rows.push(SearchRow {
            item_type,
            item_id: row.try_get("", "item_id")?,
            open_id,
            project_id: row.try_get("", "project_id")?,
            title: row.try_get("", "title")?,
            highlighted_title: row.try_get("", "highlighted_title")?,
            snippet: row.try_get("", "snippet")?,
            preview: row.try_get("", "preview")?,
        });
    }

    let board_titles = load_board_titles(db, &board_title_ids).await?;

    let mut results = Vec::with_capacity(search_rows.len());
    for row in search_rows {
        let kind = match row.item_type.as_str() {
            "note" => SearchResultKind::Note,
            "board" => SearchResultKind::Board,
            "card" => SearchResultKind::Card,
            "entry" => SearchResultKind::Entry,
            _ => continue,
        };

        results.push(SearchResult {
            kind,
            item_id: row.item_id as u32,
            open_id: row.open_id as u32,
            project_id: row.project_id.map(|id| id as u32),
            parent_title: board_titles.get(&row.open_id).cloned(),
            title: row.title,
            highlighted_title: row.highlighted_title,
            snippet: row.snippet,
            preview: row.preview,
        });
    }

    Ok(results)
}

fn search_card_body(title: &str, entries: Option<&[EntrySearchSource]>) -> String {
    let mut body = String::with_capacity(search_card_body_capacity(title, entries));
    append_search_card_body(&mut body, title, entries);
    body
}

fn append_search_card_body(body: &mut String, title: &str, entries: Option<&[EntrySearchSource]>) {
    body.push_str("## ");
    body.push_str(title);

    if let Some(entries) = entries {
        for entry in entries {
            body.push_str("\n- ");
            body.push_str(&entry.title);

            if !entry.description.trim().is_empty() {
                body.push_str(": ");
                body.push_str(&entry.description);
            }
        }
    }
}

fn search_board_body(
    cards: &[(i64, i64, String, i32)],
    board_cards: Option<&[CardSearchSource]>,
    entries_by_card: &HashMap<i64, Vec<EntrySearchSource>>,
) -> String {
    let Some(board_cards) = board_cards else {
        return String::new();
    };

    let mut body = String::with_capacity(
        board_cards
            .iter()
            .map(|card| {
                let (card_id, _, card_title, _) = &cards[card.index];
                search_card_body_capacity(
                    card_title,
                    entries_by_card.get(card_id).map(Vec::as_slice),
                ) + 2
            })
            .sum::<usize>()
            .saturating_sub(2),
    );

    for (index, card) in board_cards.iter().enumerate() {
        if index > 0 {
            body.push_str("\n\n");
        }

        let (card_id, _, card_title, _) = &cards[card.index];
        append_search_card_body(
            &mut body,
            card_title,
            entries_by_card.get(card_id).map(Vec::as_slice),
        );
    }

    body
}

fn search_card_body_capacity(title: &str, entries: Option<&[EntrySearchSource]>) -> usize {
    let entries_capacity = entries
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    let description_capacity = if entry.description.trim().is_empty() {
                        0
                    } else {
                        2 + entry.description.len()
                    };

                    3 + entry.title.len() + description_capacity
                })
                .sum::<usize>()
        })
        .unwrap_or(0);

    3 + title.len() + entries_capacity
}

fn fts_query(query: &str) -> Option<String> {
    let raw_terms = query
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();

    let multi_term = raw_terms.len() > 1;
    let mut terms = raw_terms
        .iter()
        .filter_map(|term| fts_query_term(term, multi_term))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        terms = raw_terms.iter().map(|term| (*term).to_string()).collect();
    }

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

fn fts_query_term(term: &str, multi_term: bool) -> Option<String> {
    let char_count = term.chars().count();

    if multi_term && char_count == 1 {
        return None;
    }

    if multi_term && char_count <= 2 {
        Some(term.to_string())
    } else {
        Some(format!("{term}*"))
    }
}

struct SearchDocument {
    item_type: &'static str,
    item_id: i64,
    parent_id: Option<i64>,
    project_id: Option<i64>,
    title: String,
    body: String,
}

struct SearchRow {
    item_type: String,
    item_id: i64,
    open_id: i64,
    project_id: Option<i64>,
    title: String,
    highlighted_title: String,
    snippet: String,
    preview: String,
}

struct EntrySearchSource {
    id: i64,
    title: String,
    description: String,
    position: i32,
}

struct CardSearchSource {
    index: usize,
    id: i64,
    position: i32,
}

async fn load_board_titles(
    db: &DatabaseConnection,
    board_ids: &[i64],
) -> Result<HashMap<i64, String>, DbErr> {
    if board_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders = std::iter::repeat_n("?", board_ids.len())
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!("SELECT id, title FROM board WHERE id IN ({placeholders})");
    let values = board_ids
        .iter()
        .map(|id| (*id).into())
        .collect::<Vec<Value>>();

    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            values,
        ))
        .await?;

    let mut board_titles = HashMap::with_capacity(rows.len());
    for row in rows {
        board_titles.insert(row.try_get("", "id")?, row.try_get("", "title")?);
    }

    Ok(board_titles)
}

async fn insert_search_documents(
    db: &impl ConnectionTrait,
    documents: impl IntoIterator<Item = SearchDocument>,
) -> Result<(), DbErr> {
    let mut chunk = Vec::with_capacity(100);

    for document in documents {
        chunk.push(document);

        if chunk.len() == 100 {
            insert_search_document_chunk(db, &mut chunk).await?;
        }
    }

    if !chunk.is_empty() {
        insert_search_document_chunk(db, &mut chunk).await?;
    }

    Ok(())
}

async fn insert_search_document_chunk(
    db: &impl ConnectionTrait,
    chunk: &mut Vec<SearchDocument>,
) -> Result<(), DbErr> {
    let placeholders = std::iter::repeat_n("(?, ?, ?, ?, ?, ?)", chunk.len())
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "INSERT INTO search_index
                 (item_type, item_id, parent_id, project_id, title, body)
              VALUES {placeholders}"
    );

    let mut values: Vec<Value> = Vec::with_capacity(chunk.len() * 6);

    for doc in chunk.drain(..) {
        values.push(doc.item_type.into());
        values.push(doc.item_id.into());
        values.push(doc.parent_id.into());
        values.push(doc.project_id.into());
        values.push(doc.title.into());
        values.push(doc.body.into());
    }

    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        sql,
        values,
    ))
    .await?;

    Ok(())
}

#[allow(dead_code)]
pub(crate) async fn delete_search_item(
    db: &DatabaseConnection,
    item_type: &str,
    item_id: u32,
) -> Result<(), DbErr> {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM search_index WHERE item_type = ? AND item_id = ?",
        [item_type.into(), (item_id as i64).into()],
    ))
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{fts_query, rebuild_search_index};
    use crate::test_alloc;
    use anyhow::Result;
    use entity::{board, card, entry, note, project};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait, Database};

    #[tokio::test]
    async fn streamed_rebuild_preserves_search_documents() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let project = project::ActiveModel {
            name: Set("Search proof".to_string()),
            archived: Set(false),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        note::ActiveModel {
            id: Set(1),
            title: Set("Note title".to_string()),
            project_id: Set(Some(project.id)),
            file_path: Set(None),
            file_managed_by_app: Set(false),
            cached_content: Set("Note body".to_string()),
            file_missing_since: Set(None),
            created_at: Set(1),
            updated_at: Set(1),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        board::ActiveModel {
            id: Set(1),
            title: Set("Board title".to_string()),
            project_id: Set(Some(project.id)),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        card::ActiveModel {
            id: Set(1),
            title: Set("List title".to_string()),
            board_id: Set(1),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        entry::ActiveModel {
            id: Set(1),
            title: Set("Entry title".to_string()),
            description: Set("Entry description".to_string()),
            card_id: Set(1),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let deleted_project = project::ActiveModel {
            name: Set("Deleted search hierarchy".to_string()),
            archived: Set(false),
            position: Set(1),
            deleted_at: Set(Some(1)),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        note::ActiveModel {
            id: Set(2),
            title: Set("Excluded note".to_string()),
            project_id: Set(Some(deleted_project.id)),
            file_path: Set(None),
            file_managed_by_app: Set(false),
            cached_content: Set("Excluded body".to_string()),
            file_missing_since: Set(None),
            created_at: Set(2),
            updated_at: Set(2),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        board::ActiveModel {
            id: Set(2),
            title: Set("Excluded board".to_string()),
            project_id: Set(Some(deleted_project.id)),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        card::ActiveModel {
            id: Set(2),
            title: Set("Excluded list".to_string()),
            board_id: Set(2),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        entry::ActiveModel {
            id: Set(2),
            title: Set("Excluded entry".to_string()),
            description: Set("Excluded description".to_string()),
            card_id: Set(2),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        rebuild_search_index(&db).await?;

        let rows = db
            .query_all_raw(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT item_type, title, body FROM search_index ORDER BY rowid",
            ))
            .await?;
        let documents = rows
            .into_iter()
            .map(|row| {
                Ok::<_, sea_orm::DbErr>((
                    row.try_get::<String>("", "item_type")?,
                    row.try_get::<String>("", "title")?,
                    row.try_get::<String>("", "body")?,
                ))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;

        assert_eq!(
            documents,
            vec![
                (
                    "note".to_string(),
                    "Note title".to_string(),
                    "Note body".to_string(),
                ),
                (
                    "board".to_string(),
                    "Board title".to_string(),
                    "## List title\n- Entry title: Entry description".to_string(),
                ),
                (
                    "card".to_string(),
                    "List title".to_string(),
                    "## List title\n- Entry title: Entry description".to_string(),
                ),
                (
                    "entry".to_string(),
                    "Entry title".to_string(),
                    "Entry description".to_string(),
                ),
            ]
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore = "performance proof; run explicitly with one test thread"]
    async fn rebuild_search_index_heap_benchmark() -> Result<()> {
        const BOARD_COUNT: usize = 120;
        const CARDS_PER_BOARD: usize = 2;
        const ENTRIES_PER_CARD: usize = 2;
        const DESCRIPTION_BYTES: usize = 64 * 1024;

        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let project = project::ActiveModel {
            name: Set("Search benchmark".to_string()),
            archived: Set(false),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let description = "x".repeat(DESCRIPTION_BYTES);
        let mut card_id = 1_i64;
        let mut entry_id = 1_i64;

        for board_index in 0..BOARD_COUNT {
            let board_id = board_index as i64 + 1;
            board::ActiveModel {
                id: Set(board_id),
                title: Set(format!("Board {board_index}")),
                project_id: Set(Some(project.id)),
                ..Default::default()
            }
            .insert(&db)
            .await?;

            for card_index in 0..CARDS_PER_BOARD {
                let current_card_id = card_id;
                card_id += 1;

                card::ActiveModel {
                    id: Set(current_card_id),
                    title: Set(format!("Card {board_index}-{card_index}")),
                    board_id: Set(board_id),
                    position: Set(card_index as i32),
                    ..Default::default()
                }
                .insert(&db)
                .await?;

                for entry_index in 0..ENTRIES_PER_CARD {
                    entry::ActiveModel {
                        id: Set(entry_id),
                        title: Set(format!("Entry {board_index}-{card_index}-{entry_index}")),
                        description: Set(description.clone()),
                        card_id: Set(current_card_id),
                        position: Set(entry_index as i32),
                        ..Default::default()
                    }
                    .insert(&db)
                    .await?;
                    entry_id += 1;
                }
            }
        }

        drop(description);
        let allocation = test_alloc::start_measurement();
        rebuild_search_index(&db).await?;
        let allocation = allocation.finish();

        let expected_documents = BOARD_COUNT
            + BOARD_COUNT * CARDS_PER_BOARD
            + BOARD_COUNT * CARDS_PER_BOARD * ENTRIES_PER_CARD;
        let indexed_documents = db
            .query_one_raw(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM search_index",
            ))
            .await?
            .ok_or_else(|| anyhow::anyhow!("search index count query returned no row"))?
            .try_get::<i64>("", "count")? as usize;

        assert_eq!(indexed_documents, expected_documents);
        println!(
            "documents={expected_documents} source_description_bytes={} peak_heap_growth_bytes={} total_allocated_bytes={}",
            BOARD_COUNT * CARDS_PER_BOARD * ENTRIES_PER_CARD * DESCRIPTION_BYTES,
            allocation.peak_growth_bytes,
            allocation.allocated_bytes,
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore = "performance proof; run explicitly with one test thread"]
    async fn deleted_hierarchy_filter_heap_benchmark() -> Result<()> {
        const EXCLUDED_NOTE_COUNT: usize = 64;
        const BODY_BYTES: usize = 1024 * 1024;

        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let active_project = project::ActiveModel {
            name: Set("Active".to_string()),
            archived: Set(false),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let deleted_project = project::ActiveModel {
            name: Set("Deleted".to_string()),
            archived: Set(false),
            position: Set(1),
            deleted_at: Set(Some(1)),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        note::ActiveModel {
            id: Set(1),
            title: Set("Indexed note".to_string()),
            project_id: Set(Some(active_project.id)),
            file_path: Set(None),
            file_managed_by_app: Set(false),
            cached_content: Set("active".to_string()),
            file_missing_since: Set(None),
            created_at: Set(0),
            updated_at: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let body = "x".repeat(BODY_BYTES);
        for index in 0..EXCLUDED_NOTE_COUNT {
            note::ActiveModel {
                id: Set(index as i64 + 2),
                title: Set(format!("Excluded note {index}")),
                project_id: Set(Some(deleted_project.id)),
                file_path: Set(None),
                file_managed_by_app: Set(false),
                cached_content: Set(body.clone()),
                file_missing_since: Set(None),
                created_at: Set(index as i64 + 1),
                updated_at: Set(index as i64 + 1),
                ..Default::default()
            }
            .insert(&db)
            .await?;
        }

        drop(body);
        let allocation = test_alloc::start_measurement();
        rebuild_search_index(&db).await?;
        let allocation = allocation.finish();

        let indexed_documents = db
            .query_one_raw(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM search_index",
            ))
            .await?
            .ok_or_else(|| anyhow::anyhow!("search index count query returned no row"))?
            .try_get::<i64>("", "count")?;

        assert_eq!(indexed_documents, 1);
        println!(
            "excluded_note_body_bytes={} peak_heap_growth_bytes={} total_allocated_bytes={}",
            EXCLUDED_NOTE_COUNT * BODY_BYTES,
            allocation.peak_growth_bytes,
            allocation.allocated_bytes,
        );

        Ok(())
    }

    #[test]
    fn fts_query_splits_hyphenated_terms() {
        assert_eq!(fts_query("edge-case"), Some("edge* case*".to_string()));
    }

    #[test]
    fn fts_query_ignores_repeated_punctuation() {
        assert_eq!(
            fts_query("Rust / GPUI: search"),
            Some("Rust* GPUI* search*".to_string())
        );
    }

    #[test]
    fn fts_query_does_not_prefix_single_letter_terms_in_phrases() {
        assert_eq!(
            fts_query("This is a working"),
            Some("This* is working*".to_string())
        );
    }

    #[test]
    fn fts_query_keeps_single_letter_query_searchable() {
        assert_eq!(fts_query("a"), Some("a*".to_string()));
    }

    #[test]
    fn fts_query_keeps_two_letter_terms_exact_in_phrases() {
        assert_eq!(fts_query("ui state"), Some("ui state*".to_string()));
    }

    #[test]
    fn fts_query_rejects_punctuation_only_queries() {
        assert_eq!(fts_query("---"), None);
    }
}
