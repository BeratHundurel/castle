use std::collections::HashMap;

use entity::{
    board, board::Entity as Board, card, card::Entity as Card, entry, entry::Entity as Entry, note,
    note::Entity as Note,
};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, EntityTrait, QuerySelect, Statement,
    TransactionTrait, Value,
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

pub(crate) async fn rebuild_search_index(db: &DatabaseConnection) -> Result<(), DbErr> {
    let notes = Note::find()
        .select_only()
        .column(note::Column::Id)
        .column(note::Column::ProjectId)
        .column(note::Column::Title)
        .column(note::Column::CachedContent)
        .into_tuple::<(i64, Option<i64>, String, String)>()
        .all(db)
        .await?;

    let boards = Board::find()
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

    let entry_count = entries_by_card.values().map(Vec::len).sum::<usize>();
    let mut documents = Vec::with_capacity(notes.len() + boards.len() + cards.len() + entry_count);

    for (id, project_id, title, content) in notes {
        documents.push(SearchDocument {
            item_type: "note",
            item_id: id,
            parent_id: Some(id),
            project_id,
            title,
            body: content,
        });
    }

    for (id, project_id, title) in boards {
        documents.push(SearchDocument {
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
        });
    }

    for (id, board_id, title, _) in cards {
        let body = search_card_body(&title, entries_by_card.get(&id).map(Vec::as_slice));

        documents.push(SearchDocument {
            item_type: "card",
            item_id: id,
            parent_id: Some(board_id),
            project_id: board_projects.get(&board_id).copied().flatten(),
            title,
            body,
        });
    }

    for (card_id, entries) in entries_by_card {
        let Some(board_id) = card_boards.get(&card_id).copied() else {
            continue;
        };

        for entry in entries {
            documents.push(SearchDocument {
                item_type: "entry",
                item_id: entry.id,
                parent_id: Some(board_id),
                project_id: board_projects.get(&board_id).copied().flatten(),
                title: entry.title,
                body: entry.description,
            });
        }
    }

    let txn = db.begin().await?;
    txn.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM search_index",
        [],
    ))
    .await?;

    insert_search_documents(&txn, documents).await?;
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
    documents: Vec<SearchDocument>,
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
    use super::fts_query;

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
