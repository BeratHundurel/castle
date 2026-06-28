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
        .into_tuple::<(i64, i64, String)>()
        .all(db)
        .await?;

    let card_boards = cards
        .iter()
        .map(|(id, board_id, _)| (*id, *board_id))
        .collect::<HashMap<_, _>>();

    let entries = Entry::find()
        .select_only()
        .column(entry::Column::Id)
        .column(entry::Column::CardId)
        .column(entry::Column::Title)
        .column(entry::Column::Description)
        .into_tuple::<(i64, i64, String, String)>()
        .all(db)
        .await?;

    let mut documents = Vec::with_capacity(notes.len() + boards.len() + cards.len());

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
            body: String::new(),
        });
    }

    for (id, board_id, title) in cards {
        documents.push(SearchDocument {
            item_type: "card",
            item_id: id,
            parent_id: Some(board_id),
            project_id: board_projects.get(&board_id).copied().flatten(),
            title,
            body: String::new(),
        });
    }

    for (id, card_id, title, description) in entries {
        let Some(board_id) = card_boards.get(&card_id).copied() else {
            continue;
        };

        documents.push(SearchDocument {
            item_type: "entry",
            item_id: id,
            parent_id: Some(board_id),
            project_id: board_projects.get(&board_id).copied().flatten(),
            title,
            body: description,
        });
    }

    let txn = db.begin().await?;
    txn.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM search_index",
        [],
    ))
    .await?;

    insert_search_documents(&txn, &documents).await?;
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
                snippet(search_index, -1, char(1), char(2), '...', 12) AS snippet,
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

    let mut results = vec![];
    for row in rows {
        let item_type: String = row.try_get("", "item_type")?;
        let item_id: i64 = row.try_get("", "item_id")?;
        let open_id: i64 = row.try_get("", "open_id")?;
        let project_id: Option<i64> = row.try_get("", "project_id")?;
        let title: String = row.try_get("", "title")?;
        let highlighted_title: String = row.try_get("", "highlighted_title")?;
        let snippet: String = row.try_get("", "snippet")?;
        let preview: String = row.try_get("", "preview")?;

        let kind = match item_type.as_str() {
            "note" => SearchResultKind::Note,
            "board" => SearchResultKind::Board,
            "card" => SearchResultKind::Card,
            "entry" => SearchResultKind::Entry,
            _ => continue,
        };

        results.push(SearchResult {
            kind,
            item_id: item_id as u32,
            open_id: open_id as u32,
            project_id: project_id.map(|id| id as u32),
            title,
            highlighted_title,
            snippet,
            preview,
        });
    }

    Ok(results)
}

fn fts_query(query: &str) -> Option<String> {
    let terms = query
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .filter(|term| !term.is_empty())
        .map(|term| format!("{term}*"))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
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
    fn fts_query_rejects_punctuation_only_queries() {
        assert_eq!(fts_query("---"), None);
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

async fn insert_search_documents(
    db: &impl ConnectionTrait,
    documents: &[SearchDocument],
) -> Result<(), DbErr> {
    for chunk in documents.chunks(100) {
        if chunk.is_empty() {
            continue;
        }

        let placeholders = std::iter::repeat_n("(?, ?, ?, ?, ?, ?)", chunk.len())
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            "INSERT INTO search_index
                 (item_type, item_id, parent_id, project_id, title, body)
              VALUES {placeholders}"
        );

        let mut values: Vec<Value> = Vec::with_capacity(chunk.len() * 6);

        for doc in chunk {
            values.push(doc.item_type.into());
            values.push(doc.item_id.into());
            values.push(doc.parent_id.into());
            values.push(doc.project_id.into());
            values.push(doc.title.clone().into());
            values.push(doc.body.clone().into());
        }

        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            values,
        ))
        .await?;
    }

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
