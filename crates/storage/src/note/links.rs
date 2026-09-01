use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result};
use entity::{
    note, note::Entity as Note, note_alias, note_alias::Entity as NoteAlias, note_link,
    note_link::Entity as NoteLink, note_link_index_state,
    note_link_index_state::Entity as NoteLinkIndexState, project, project::Entity as Project,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait,
    QueryFilter, Statement, TransactionSession, TransactionTrait,
};

const MAX_LINK_TARGET_BYTES: usize = 512;
const MAX_LINKS_PER_NOTE: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedWikiLink {
    pub raw_target: String,
    pub display_text: Option<String>,
    pub start_byte: usize,
    pub end_byte: usize,
    pub line_number: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedNoteLink {
    pub target_note_id: Option<i64>,
    pub raw_target: String,
    pub display_text: Option<String>,
    pub start_byte: usize,
    pub end_byte: usize,
    pub line_number: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteLinkCatalogEntry {
    pub note_id: i64,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
}

pub(crate) struct NoteIndexCatalogs<'a> {
    pub note_links: &'a [NoteLinkCatalogEntry],
    pub aliases: &'a [note_alias::Model],
    pub workspace: &'a crate::workspace::links::WorkspaceReferenceCatalog,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteLinkSet {
    pub inbound: Vec<NoteLinkReference>,
    pub outbound: Vec<NoteLinkReference>,
    pub unresolved: Vec<UnresolvedLinkReference>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnresolvedLinkReference {
    pub source_note_id: i64,
    pub source_title: String,
    pub source_project_name: Option<String>,
    pub target_kind: Option<crate::workspace::links::WorkspaceItemKind>,
    pub raw_target: String,
    pub display_text: Option<String>,
    pub start_byte: usize,
    pub end_byte: usize,
    pub line_number: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteLinkReference {
    pub source_note_id: i64,
    pub source_title: String,
    pub source_project_name: Option<String>,
    pub target_note_id: Option<i64>,
    pub target_title: Option<String>,
    pub target_project_name: Option<String>,
    pub raw_target: String,
    pub display_text: Option<String>,
    pub start_byte: usize,
    pub end_byte: usize,
    pub line_number: usize,
}

pub fn parse_wikilinks(content: &str) -> Vec<ParsedWikiLink> {
    let mut links = Vec::new();
    let mut offset = 0usize;
    let mut fence: Option<(u8, usize)> = None;

    for (line_index, line_with_newline) in content.split_inclusive('\n').enumerate() {
        let line = line_with_newline
            .strip_suffix('\n')
            .unwrap_or(line_with_newline);

        let trimmed = line.trim_start();
        if let Some((marker, minimum_len)) = fence {
            if fence_run(trimmed, marker) >= minimum_len {
                fence = None;
            }
            offset = offset.saturating_add(line_with_newline.len());
            continue;
        }

        let backticks = fence_run(trimmed, b'`');
        let tildes = fence_run(trimmed, b'~');
        if backticks >= 3 {
            fence = Some((b'`', backticks));
        } else if tildes >= 3 {
            fence = Some((b'~', tildes));
        } else {
            parse_line_wikilinks(line, offset, line_index + 1, &mut links);
            if links.len() >= MAX_LINKS_PER_NOTE {
                links.truncate(MAX_LINKS_PER_NOTE);
                break;
            }
        }

        offset = offset.saturating_add(line_with_newline.len());
    }

    links
}

fn parse_line_wikilinks(
    line: &str,
    line_offset: usize,
    line_number: usize,
    links: &mut Vec<ParsedWikiLink>,
) {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    let mut inline_code_delimiter = None;

    while index < bytes.len() {
        if bytes[index] == b'`' {
            let run = byte_run(&bytes[index..], b'`');
            inline_code_delimiter = match inline_code_delimiter {
                Some(delimiter) if delimiter == run => None,
                None => Some(run),
                current => current,
            };
            index += run;
            continue;
        }

        if inline_code_delimiter.is_none()
            && bytes[index] == b'['
            && bytes.get(index + 1) == Some(&b'[')
            && !is_escaped(bytes, index)
        {
            let content_start = index + 2;
            let search_end = bytes
                .len()
                .min(content_start.saturating_add(MAX_LINK_TARGET_BYTES + 2));

            if let Some(relative_end) =
                find_unescaped_closing_brackets(line, content_start, search_end, bytes)
            {
                let content_end = content_start + relative_end;
                let token_end = content_end + 2;
                let inner = &line[content_start..content_end];
                let (raw_target, display_text) = match split_unescaped_once(inner, '|') {
                    Some((target, display)) => (target.trim(), Some(display.trim())),
                    None => (inner.trim(), None),
                };
                if !raw_target.is_empty() {
                    links.push(ParsedWikiLink {
                        raw_target: raw_target.to_string(),
                        display_text: display_text
                            .filter(|display| !display.is_empty())
                            .map(crate::workspace::links::unescape_segment),
                        start_byte: line_offset + index,
                        end_byte: line_offset + token_end,
                        line_number,
                    });
                }
                index = token_end;
                continue;
            }
        }

        index += line[index..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);
    }
}

fn split_unescaped_once(value: &str, delimiter: char) -> Option<(&str, &str)> {
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == delimiter {
            return Some((&value[..index], &value[index + character.len_utf8()..]));
        }
    }
    None
}

fn find_unescaped_closing_brackets(
    line: &str,
    start: usize,
    end: usize,
    bytes: &[u8],
) -> Option<usize> {
    let mut cursor = start;
    while cursor < end {
        let relative = line[cursor..end].find("]]")?;
        let candidate = cursor + relative;
        if !is_escaped(bytes, candidate) {
            return Some(candidate - start);
        }
        cursor = candidate.saturating_add(1);
    }
    None
}

fn fence_run(line: &str, marker: u8) -> usize {
    byte_run(line.as_bytes(), marker)
}

fn byte_run(bytes: &[u8], marker: u8) -> usize {
    bytes.iter().take_while(|byte| **byte == marker).count()
}

fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let slash_count = bytes[..index]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    slash_count % 2 == 1
}

pub async fn load_note_link_catalog(
    db: &impl ConnectionTrait,
) -> Result<Vec<NoteLinkCatalogEntry>> {
    let project_names = Project::find()
        .filter(project::Column::Archived.eq(false))
        .filter(project::Column::DeletedAt.is_null())
        .all(db)
        .await?
        .into_iter()
        .map(|project| (project.id, project.name))
        .collect::<HashMap<_, _>>();

    Ok(Note::find()
        .filter(note::Column::DeletedAt.is_null())
        .all(db)
        .await?
        .into_iter()
        .filter(|note| {
            note.project_id
                .is_none_or(|project_id| project_names.contains_key(&project_id))
        })
        .map(|note| NoteLinkCatalogEntry {
            note_id: note.id,
            title: note.title,
            project_id: note.project_id,
            project_name: note
                .project_id
                .and_then(|project_id| project_names.get(&project_id).cloned()),
        })
        .collect())
}

pub async fn load_note_links(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: i64,
) -> Result<NoteLinkSet> {
    let catalog = load_note_link_catalog(db).await?;
    let by_id = catalog
        .iter()
        .map(|note| (note.note_id, note))
        .collect::<HashMap<_, _>>();
    by_id
        .get(&note_id)
        .with_context(|| format!("active note {note_id} was not found"))?;
    let outbound_models = NoteLink::find()
        .filter(note_link::Column::SourceNoteId.eq(note_id))
        .all(db)
        .await?;
    let inbound_models = NoteLink::find()
        .filter(note_link::Column::TargetNoteId.eq(note_id))
        .all(db)
        .await?;

    let to_reference = |link: note_link::Model| {
        let source = by_id.get(&link.source_note_id).copied();
        let target = link
            .target_note_id
            .and_then(|target_note_id| by_id.get(&target_note_id).copied());
        NoteLinkReference {
            source_note_id: link.source_note_id,
            source_title: source
                .map(|note| note.title.clone())
                .unwrap_or_else(|| "Unavailable note".to_string()),
            source_project_name: source.and_then(|note| note.project_name.clone()),
            target_note_id: target.map(|note| note.note_id),
            target_title: target.map(|note| note.title.clone()),
            target_project_name: target.and_then(|note| note.project_name.clone()),
            raw_target: link.raw_target,
            display_text: link.display_text,
            start_byte: link.start_byte.max(0) as usize,
            end_byte: link.end_byte.max(0) as usize,
            line_number: link.line_number.max(1) as usize,
        }
    };
    let workspace_catalog = crate::workspace::links::load_workspace_reference_catalog(db).await?;
    let mut outbound = Vec::new();
    let mut unresolved = Vec::new();
    for model in outbound_models {
        if crate::workspace::links::is_workspace_target(&model.raw_target) {
            if crate::workspace::links::resolve_workspace_item(
                &model.raw_target,
                &workspace_catalog,
            )
            .is_err()
            {
                unresolved.push(UnresolvedLinkReference {
                    source_note_id: model.source_note_id,
                    source_title: by_id
                        .get(&model.source_note_id)
                        .map(|note| note.title.clone())
                        .unwrap_or_else(|| "Unavailable note".to_string()),
                    source_project_name: by_id
                        .get(&model.source_note_id)
                        .and_then(|note| note.project_name.clone()),
                    target_kind: crate::workspace::links::parse_reference_target(&model.raw_target)
                        .map(|reference| reference.kind),
                    raw_target: model.raw_target,
                    display_text: model.display_text,
                    start_byte: model.start_byte.max(0) as usize,
                    end_byte: model.end_byte.max(0) as usize,
                    line_number: model.line_number.max(1) as usize,
                });
            }
            continue;
        }
        let reference = to_reference(model);
        if reference.target_note_id.is_none() {
            unresolved.push(UnresolvedLinkReference {
                source_note_id: reference.source_note_id,
                source_title: reference.source_title.clone(),
                source_project_name: reference.source_project_name.clone(),
                target_kind: None,
                raw_target: reference.raw_target.clone(),
                display_text: reference.display_text.clone(),
                start_byte: reference.start_byte,
                end_byte: reference.end_byte,
                line_number: reference.line_number,
            });
        }
        outbound.push(reference);
    }
    let inbound = inbound_models.into_iter().map(to_reference).collect();

    Ok(NoteLinkSet {
        inbound,
        outbound,
        unresolved,
    })
}

pub async fn index_note_links(
    db: &(impl ConnectionTrait + TransactionTrait),
    note_id: i64,
    content: &str,
    indexed_updated_at: i64,
) -> Result<Vec<IndexedNoteLink>> {
    let transaction = db.begin().await?;
    let indexed =
        index_note_links_in_connection(&transaction, note_id, content, indexed_updated_at).await?;
    transaction.commit().await?;
    Ok(indexed)
}

pub async fn index_note_links_in_connection(
    db: &impl ConnectionTrait,
    note_id: i64,
    content: &str,
    indexed_updated_at: i64,
) -> Result<Vec<IndexedNoteLink>> {
    let source = Note::find_by_id(note_id)
        .filter(note::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .with_context(|| format!("active note {note_id} was not found"))?;
    let parsed = parse_wikilinks(content);
    let catalog = load_note_link_catalog(db).await?;
    let normalized_targets = parsed
        .iter()
        .map(|link| normalize_name(&link.raw_target))
        .collect::<HashSet<_>>();
    let aliases = if normalized_targets.is_empty() {
        Vec::new()
    } else {
        NoteAlias::find()
            .filter(note_alias::Column::NormalizedAlias.is_in(normalized_targets))
            .all(db)
            .await?
    };
    let workspace_catalog = crate::workspace::links::load_workspace_reference_catalog(db).await?;
    index_note_links_with_catalog(
        db,
        note_id,
        source.project_id,
        content,
        indexed_updated_at,
        NoteIndexCatalogs {
            note_links: &catalog,
            aliases: &aliases,
            workspace: &workspace_catalog,
        },
    )
    .await
}

pub(crate) async fn index_note_links_with_catalog(
    db: &impl ConnectionTrait,
    note_id: i64,
    source_project_id: Option<i64>,
    content: &str,
    indexed_updated_at: i64,
    catalogs: NoteIndexCatalogs<'_>,
) -> Result<Vec<IndexedNoteLink>> {
    let embed_ranges = crate::board::projection::parse_board_view_embeds(content)
        .into_iter()
        .map(|embed| embed.start_byte..embed.end_byte)
        .collect::<Vec<_>>();
    let indexed = parse_wikilinks(content)
        .into_iter()
        .filter(|link| {
            !embed_ranges
                .iter()
                .any(|range| range.contains(&link.start_byte))
        })
        .map(|link| IndexedNoteLink {
            target_note_id: resolve_target(
                &link.raw_target,
                source_project_id,
                catalogs.note_links,
                catalogs.aliases,
                catalogs.workspace,
            ),
            raw_target: link.raw_target,
            display_text: link.display_text,
            start_byte: link.start_byte,
            end_byte: link.end_byte,
            line_number: link.line_number,
        })
        .collect::<Vec<_>>();

    NoteLink::delete_many()
        .filter(note_link::Column::SourceNoteId.eq(note_id))
        .exec(db)
        .await?;
    NoteLinkIndexState::delete_by_id(note_id).exec(db).await?;
    for (ordinal, link) in indexed.iter().enumerate() {
        note_link::ActiveModel {
            source_note_id: Set(note_id),
            ordinal: Set(ordinal as i32),
            target_note_id: Set(link.target_note_id),
            raw_target: Set(link.raw_target.clone()),
            display_text: Set(link.display_text.clone()),
            start_byte: Set(link.start_byte as i64),
            end_byte: Set(link.end_byte as i64),
            line_number: Set(link.line_number as i32),
        }
        .insert(db)
        .await?;
    }
    note_link_index_state::ActiveModel {
        note_id: Set(note_id),
        indexed_updated_at: Set(indexed_updated_at),
    }
    .insert(db)
    .await?;
    crate::workspace::links::index_note_workspace_links_with_catalog(
        db,
        note_id,
        content,
        indexed_updated_at,
        catalogs.workspace,
    )
    .await?;

    Ok(indexed)
}

pub async fn record_note_alias(
    db: &impl ConnectionTrait,
    note_id: i64,
    alias: &str,
    created_at: i64,
) -> Result<()> {
    let alias = alias.trim();
    if alias.is_empty() {
        return Ok(());
    }
    let normalized_alias = normalize_name(alias);
    let exists = NoteAlias::find()
        .filter(note_alias::Column::NoteId.eq(note_id))
        .filter(note_alias::Column::NormalizedAlias.eq(normalized_alias.clone()))
        .one(db)
        .await?
        .is_some();
    if !exists {
        note_alias::ActiveModel {
            note_id: Set(note_id),
            alias: Set(alias.to_string()),
            normalized_alias: Set(normalized_alias),
            created_at: Set(created_at),
            ..Default::default()
        }
        .insert(db)
        .await?;
    }
    Ok(())
}

pub async fn reindex_stale_notes(
    db: &(impl ConnectionTrait + TransactionTrait),
    limit: u64,
) -> Result<usize> {
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT n.id, n.cached_content, n.updated_at
            FROM note n
            LEFT JOIN note_link_index_state s ON s.note_id = n.id
            WHERE n.deleted_at IS NULL
              AND (s.note_id IS NULL OR s.indexed_updated_at != n.updated_at)
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
        index_note_links(db, note_id, &content, updated_at).await?;
    }
    Ok(count)
}

fn resolve_target(
    raw_target: &str,
    source_project_id: Option<i64>,
    catalog: &[NoteLinkCatalogEntry],
    aliases: &[note_alias::Model],
    workspace_catalog: &crate::workspace::links::WorkspaceReferenceCatalog,
) -> Option<i64> {
    if let Some(reference) = crate::workspace::links::parse_reference_target(raw_target) {
        return match crate::workspace::links::resolve_reference_target(
            raw_target,
            workspace_catalog,
        ) {
            Ok(crate::workspace::links::ResolvedWorkspaceReference::Item(item))
                if item.kind == crate::workspace::links::WorkspaceItemKind::Note =>
            {
                Some(item.id)
            }
            _ if reference.kind == crate::workspace::links::WorkspaceItemKind::Note => {
                resolve_note_target_alias(reference.segments.last()?, catalog, aliases)
            }
            _ => None,
        };
    }

    if let Some((project_name, title)) = raw_target.split_once('/') {
        let normalized_project = normalize_name(project_name);
        let normalized_title = normalize_name(title);
        return unique_note_id(catalog.iter().filter(|candidate| {
            normalize_name(&candidate.title) == normalized_title
                && candidate
                    .project_name
                    .as_deref()
                    .is_some_and(|name| normalize_name(name) == normalized_project)
        }));
    }

    let normalized_target = normalize_name(raw_target);
    if let Some(note_id) = unique_note_id(catalog.iter().filter(|candidate| {
        candidate.project_id == source_project_id
            && normalize_name(&candidate.title) == normalized_target
    })) {
        return Some(note_id);
    }
    if let Some(note_id) = unique_note_id(
        catalog
            .iter()
            .filter(|candidate| normalize_name(&candidate.title) == normalized_target),
    ) {
        return Some(note_id);
    }

    let active_note_ids = catalog
        .iter()
        .map(|candidate| candidate.note_id)
        .collect::<HashSet<_>>();
    let alias_note_ids = aliases
        .iter()
        .filter(|alias| {
            alias.normalized_alias == normalized_target && active_note_ids.contains(&alias.note_id)
        })
        .map(|alias| alias.note_id)
        .collect::<HashSet<_>>();
    (alias_note_ids.len() == 1)
        .then(|| alias_note_ids.into_iter().next())
        .flatten()
}

fn resolve_note_target_alias(
    alias: &str,
    catalog: &[NoteLinkCatalogEntry],
    aliases: &[note_alias::Model],
) -> Option<i64> {
    let active_note_ids = catalog
        .iter()
        .map(|candidate| candidate.note_id)
        .collect::<HashSet<_>>();
    let alias_note_ids = aliases
        .iter()
        .filter(|candidate| {
            candidate.normalized_alias == normalize_name(alias)
                && active_note_ids.contains(&candidate.note_id)
        })
        .map(|candidate| candidate.note_id)
        .collect::<HashSet<_>>();
    (alias_note_ids.len() == 1)
        .then(|| alias_note_ids.into_iter().next())
        .flatten()
}

fn unique_note_id<'a>(candidates: impl Iterator<Item = &'a NoteLinkCatalogEntry>) -> Option<i64> {
    let mut ids = candidates.map(|candidate| candidate.note_id);
    let first = ids.next()?;
    ids.next().is_none().then_some(first)
}

fn normalize_name(value: &str) -> String {
    value.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity::{board, note, project};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database, EntityTrait};

    #[test]
    fn parser_skips_code_and_tracks_unicode_byte_ranges() {
        let content = "[[Alpha]] and `[[inline]]`\n```md\n[[fenced]]\n```\né [[Beta|B]]";
        let links = parse_wikilinks(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].raw_target, "Alpha");
        assert_eq!(links[0].line_number, 1);
        assert_eq!(
            &content[links[1].start_byte..links[1].end_byte],
            "[[Beta|B]]"
        );
        assert_eq!(links[1].display_text.as_deref(), Some("B"));
        assert_eq!(links[1].line_number, 5);
    }

    #[test]
    fn parser_ignores_escaped_and_empty_links() {
        let links = parse_wikilinks(r"\[[escaped]] [ [not a link] ] [[ ]] [[valid]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].raw_target, "valid");

        let escaped = parse_wikilinks(r"[[card:Launch|Open \| card]]");
        assert_eq!(escaped[0].display_text.as_deref(), Some("Open | card"));
        let escaped_source = r"[[card:Launch|Open \[card\]".to_string() + "]]";
        let escaped_brackets = parse_wikilinks(&escaped_source);
        assert_eq!(
            escaped_brackets[0].display_text.as_deref(),
            Some("Open [card]")
        );
    }

    #[tokio::test]
    async fn index_resolves_local_scoped_stable_and_alias_links() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let first_project = create_project(&db, "First").await?;
        let second_project = create_project(&db, "Second").await?;
        let source = create_note(&db, "Source", Some(first_project.id)).await?;
        let local = create_note(&db, "Shared", Some(first_project.id)).await?;
        let remote = create_note(&db, "Shared", Some(second_project.id)).await?;
        let renamed = create_note(&db, "Current", None).await?;
        record_note_alias(&db, renamed.id, "Previous", 1).await?;

        let links = index_note_links(
            &db,
            source.id,
            "[[Shared]] [[Second/Shared]] [[note:Second / Shared|Stable]] [[Previous]] [[Missing]]",
            10,
        )
        .await?;

        assert_eq!(links[0].target_note_id, Some(local.id));
        assert_eq!(links[1].target_note_id, Some(remote.id));
        assert_eq!(links[2].target_note_id, Some(remote.id));
        assert_eq!(links[3].target_note_id, Some(renamed.id));
        assert_eq!(links[4].target_note_id, None);
        assert_eq!(
            NoteLink::find()
                .filter(note_link::Column::SourceNoteId.eq(source.id))
                .all(&db)
                .await?
                .len(),
            5
        );
        Ok(())
    }

    #[tokio::test]
    async fn stale_reindex_is_bounded_and_idempotent() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        create_note(&db, "One", None).await?;
        create_note(&db, "Two", None).await?;

        assert_eq!(reindex_stale_notes(&db, 1).await?, 1);
        assert_eq!(reindex_stale_notes(&db, 8).await?, 1);
        assert_eq!(reindex_stale_notes(&db, 8).await?, 0);
        Ok(())
    }

    #[tokio::test]
    async fn stable_workspace_targets_distinguish_active_deleted_and_missing() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let source = create_note(&db, "Source", None).await?;
        let board = board::ActiveModel {
            title: Set("Roadmap".to_string()),
            last_selected_view_id: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let content = "before [[board:Roadmap|Roadmap]] after";

        index_note_links(&db, source.id, &content, 1).await?;
        let links = load_note_links(&db, source.id).await?;
        assert!(links.unresolved.is_empty());
        assert_eq!(
            crate::workspace::links::load_note_workspace_links(&db, source.id)
                .await?
                .references
                .len(),
            1
        );

        board::ActiveModel {
            id: Set(board.id),
            deleted_at: Set(Some(2)),
            ..Default::default()
        }
        .update(&db)
        .await?;
        index_note_links(&db, source.id, &content, 2).await?;
        let links = load_note_links(&db, source.id).await?;
        assert_eq!(links.unresolved.len(), 1);
        assert_eq!(
            links.unresolved[0].target_kind,
            Some(crate::workspace::links::WorkspaceItemKind::Board)
        );
        assert!(
            crate::workspace::links::load_note_workspace_links(&db, source.id)
                .await?
                .references
                .is_empty()
        );

        board::Entity::delete_by_id(board.id).exec(&db).await?;
        index_note_links(&db, source.id, &content, 3).await?;
        let links = load_note_links(&db, source.id).await?;
        assert_eq!(links.unresolved.len(), 1);
        assert_eq!(
            links.unresolved[0].target_kind,
            Some(crate::workspace::links::WorkspaceItemKind::Board)
        );
        assert_eq!(links.unresolved[0].raw_target, "board:Roadmap");
        assert_eq!(
            &content[links.unresolved[0].start_byte..links.unresolved[0].end_byte],
            "[[board:Roadmap|Roadmap]]"
        );
        Ok(())
    }

    async fn create_project(
        db: &(impl ConnectionTrait + TransactionTrait),
        name: &str,
    ) -> Result<project::Model> {
        Ok(project::ActiveModel {
            name: Set(name.to_string()),
            archived: Set(false),
            position: Set(0),
            ..Default::default()
        }
        .insert(db)
        .await?)
    }

    async fn create_note(
        db: &(impl ConnectionTrait + TransactionTrait),
        title: &str,
        project_id: Option<i64>,
    ) -> Result<note::Model> {
        Ok(note::ActiveModel {
            title: Set(title.to_string()),
            project_id: Set(project_id),
            file_path: Set(None),
            file_managed_by_app: Set(false),
            cached_content: Set(String::new()),
            file_missing_since: Set(None),
            created_at: Set(1),
            updated_at: Set(1),
            ..Default::default()
        }
        .insert(db)
        .await?)
    }
}
