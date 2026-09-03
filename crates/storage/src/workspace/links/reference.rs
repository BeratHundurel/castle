use std::fmt;

use super::{WorkspaceCatalogEntry, WorkspaceItemKind, WorkspaceItemRef};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static BOARD_VIEW_EMBED_PARSE_CALLS: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_board_view_embed_parse_call_count() {
    BOARD_VIEW_EMBED_PARSE_CALLS.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn board_view_embed_parse_call_count() -> usize {
    BOARD_VIEW_EMBED_PARSE_CALLS.with(Cell::get)
}

/// A saved board view exposed to the note reference resolver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceViewCatalogEntry {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
}

/// The immutable snapshot consumed by completion, indexing, and preview.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceReferenceCatalog {
    pub items: Vec<WorkspaceCatalogEntry>,
    pub views: Vec<WorkspaceViewCatalogEntry>,
    pub aliases: Vec<WorkspaceReferenceAlias>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceReferenceAlias {
    pub target: WorkspaceAliasTarget,
    pub alias: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkspaceAliasTarget {
    Project(i64),
    Item(WorkspaceItemRef),
    SavedView(i64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceReferencePath {
    pub kind: WorkspaceItemKind,
    pub segments: Vec<String>,
    pub view: Option<String>,
    pub display_text: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedBoardViewEmbed {
    pub board_path: Vec<String>,
    pub view_name: Option<String>,
    pub display_text: Option<String>,
    pub raw_target: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub line_number: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvedWorkspaceReference {
    Item(WorkspaceItemRef),
    BoardView { board_id: i64, view_id: Option<i64> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceReferenceResolveError {
    Invalid,
    Missing,
    Ambiguous,
}

impl fmt::Display for WorkspaceReferenceResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid workspace reference",
            Self::Missing => "workspace reference was not found",
            Self::Ambiguous => "workspace reference is ambiguous",
        })
    }
}

impl std::error::Error for WorkspaceReferenceResolveError {}

impl WorkspaceReferenceCatalog {
    pub fn item(&self, item: WorkspaceItemRef) -> Option<&WorkspaceCatalogEntry> {
        self.items.iter().find(|entry| entry.item == item)
    }

    pub fn view(&self, view_id: i64) -> Option<&WorkspaceViewCatalogEntry> {
        self.views.iter().find(|view| view.id == view_id)
    }

    pub fn item_path(&self, entry: &WorkspaceCatalogEntry) -> Vec<String> {
        entry.path_segments()
    }

    pub fn view_path(&self, view: &WorkspaceViewCatalogEntry) -> (Vec<String>, String) {
        let board = self
            .item(WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id: view.board_id,
            })
            .map(|entry| self.item_path(entry))
            .unwrap_or_default();
        (board, view.name.clone())
    }

    pub fn format_item_link(
        &self,
        item: WorkspaceItemRef,
        display_text: Option<&str>,
    ) -> Option<String> {
        let entry = self.item(item)?;
        let path = self.shortest_unique_path(entry);
        let mut formatted = format!("[[{}:{}]]", item.kind.as_str(), format_segments(&path));
        if let Some(display_text) = display_text.filter(|text| !text.trim().is_empty()) {
            formatted.insert_str(
                formatted.len() - 2,
                &format!("|{}", escape_segment(display_text.trim())),
            );
        }
        Some(formatted)
    }

    pub fn format_board_view(&self, board_id: i64, view_id: Option<i64>) -> Option<String> {
        self.format_board_view_with_display_text(board_id, view_id, None)
    }

    pub fn format_board_view_with_display_text(
        &self,
        board_id: i64,
        view_id: Option<i64>,
        display_text: Option<&str>,
    ) -> Option<String> {
        let board = self.item(WorkspaceItemRef {
            kind: WorkspaceItemKind::Board,
            id: board_id,
        })?;
        let path = self.shortest_unique_path(board);
        let view_name = match view_id {
            Some(view_id) => {
                let view = self.view(view_id)?;
                if view.board_id != board_id {
                    return None;
                }
                Some(view.name.as_str())
            }
            None => None,
        };
        let suffix = view_name
            .map(|name| format!("#{}", escape_segment(name)))
            .unwrap_or_default();
        let display = display_text
            .filter(|text| !text.trim().is_empty())
            .map(|text| format!("|{}", escape_segment(text.trim())))
            .unwrap_or_default();
        Some(format!(
            "![[board:{}{}{}]]",
            format_segments(&path),
            suffix,
            display
        ))
    }

    fn shortest_unique_path(&self, entry: &WorkspaceCatalogEntry) -> Vec<String> {
        let full = self.item_path(entry);
        for start in (0..full.len()).rev() {
            let candidate = &full[start..];
            let matches = self
                .items
                .iter()
                .filter(|other| other.item.kind == entry.item.kind)
                .filter(|other| path_matches_suffix(self, other, candidate))
                .count();
            if matches == 1 {
                return candidate.to_vec();
            }
        }
        full
    }
}

pub fn parse_reference_target(raw_target: &str) -> Option<WorkspaceReferencePath> {
    let (prefix, value) = raw_target.trim().split_once(':')?;
    let kind = WorkspaceItemKind::parse(prefix.trim())?;
    if has_unterminated_escape(value) || contains_unescaped_bracket(value) {
        return None;
    }
    let (value, display_text) = split_unescaped(value, '|')
        .map(|(target, display)| {
            if contains_unescaped(display, '|')
                || contains_unescaped(display, '#')
                || contains_unescaped(display, '[')
                || contains_unescaped(display, ']')
            {
                return ("", None);
            }
            (
                target,
                Some(unescape_segment(display.trim())).filter(|display| !display.is_empty()),
            )
        })
        .unwrap_or((value, None));
    if value.is_empty() {
        return None;
    }
    let (board, view) = if kind == WorkspaceItemKind::Board {
        match split_unescaped(value, '#') {
            Some((board, view)) if !contains_unescaped(view, '#') => {
                (board, Some(unescape_segment(view.trim())))
            }
            Some(_) => return None,
            None if contains_unescaped(value, '#') => return None,
            None => (value, None),
        }
    } else {
        if contains_unescaped(value, '#') {
            return None;
        }
        (value, None)
    };
    let segments = split_segments(board).ok()?;
    if matches!(
        kind,
        WorkspaceItemKind::Board | WorkspaceItemKind::List | WorkspaceItemKind::Card
    ) && segments
        .iter()
        .all(|segment| segment.parse::<i64>().is_ok())
    {
        return None;
    }
    (!segments.is_empty() && view.as_deref() != Some("")).then_some(WorkspaceReferencePath {
        kind,
        segments,
        view,
        display_text,
    })
}

pub fn parse_board_view_embeds(content: &str) -> Vec<ParsedBoardViewEmbed> {
    #[cfg(test)]
    BOARD_VIEW_EMBED_PARSE_CALLS.with(|count| count.set(count.get().saturating_add(1)));

    let mut embeds = Vec::new();
    let mut offset = 0usize;
    let mut fence: Option<(u8, usize)> = None;
    for (line_index, line_with_newline) in content.split_inclusive('\n').enumerate() {
        let line = line_with_newline
            .strip_suffix('\n')
            .unwrap_or(line_with_newline);
        let content_line = line.strip_suffix('\r').unwrap_or(line);
        let fence_line = content_line.trim_start();
        if let Some((marker, minimum_len)) = fence {
            if fence_run(fence_line, marker) >= minimum_len {
                fence = None;
            }
            offset = offset.saturating_add(line_with_newline.len());
            continue;
        }
        let backticks = fence_run(fence_line, b'`');
        let tildes = fence_run(fence_line, b'~');
        if backticks >= 3 {
            fence = Some((b'`', backticks));
            offset = offset.saturating_add(line_with_newline.len());
            continue;
        }
        if tildes >= 3 {
            fence = Some((b'~', tildes));
            offset = offset.saturating_add(line_with_newline.len());
            continue;
        }
        let leading = content_line.len() - content_line.trim_start().len();
        let trimmed = content_line.trim();
        if trimmed.starts_with("![[") && trimmed.ends_with("]]") {
            let inner = &trimmed[3..trimmed.len() - 2];
            if let Some(reference) = parse_reference_target(inner)
                && reference.kind == WorkspaceItemKind::Board
            {
                let raw_target = split_unescaped(inner, '|')
                    .map(|(target, _)| target.trim())
                    .unwrap_or(inner.trim())
                    .to_string();
                embeds.push(ParsedBoardViewEmbed {
                    board_path: reference.segments,
                    view_name: reference.view,
                    display_text: reference.display_text,
                    raw_target,
                    start_byte: offset + leading,
                    end_byte: offset + content_line.len(),
                    line_number: line_index + 1,
                });
            }
        }
        offset = offset.saturating_add(line_with_newline.len());
    }
    embeds
}

pub fn resolve_reference(
    reference: &WorkspaceReferencePath,
    catalog: &WorkspaceReferenceCatalog,
) -> Result<ResolvedWorkspaceReference, WorkspaceReferenceResolveError> {
    let candidates = catalog
        .items
        .iter()
        .filter(|entry| entry.item.kind == reference.kind)
        .filter(|entry| path_matches_suffix(catalog, entry, &reference.segments))
        .collect::<Vec<_>>();
    if reference.kind == WorkspaceItemKind::Board {
        if candidates.len() != 1 {
            return Err(match candidates.len() {
                0 => WorkspaceReferenceResolveError::Missing,
                _ => WorkspaceReferenceResolveError::Ambiguous,
            });
        }
        let board = candidates[0];
        if reference.view.is_none() {
            return Ok(ResolvedWorkspaceReference::Item(board.item));
        }
        let view_id = match reference.view.as_deref() {
            None => None,
            Some(view_name) => {
                let views = catalog
                    .views
                    .iter()
                    .filter(|view| view.board_id == board.item.id)
                    .filter(|view| {
                        alias_matches(
                            catalog,
                            WorkspaceAliasTarget::SavedView(view.id),
                            &view.name,
                            view_name,
                        )
                    })
                    .collect::<Vec<_>>();
                if views.len() != 1 {
                    return Err(match views.len() {
                        0 => WorkspaceReferenceResolveError::Missing,
                        _ => WorkspaceReferenceResolveError::Ambiguous,
                    });
                }
                Some(views[0].id)
            }
        };
        Ok(ResolvedWorkspaceReference::BoardView {
            board_id: board.item.id,
            view_id,
        })
    } else {
        match candidates.as_slice() {
            [entry] => Ok(ResolvedWorkspaceReference::Item(entry.item)),
            [] => Err(WorkspaceReferenceResolveError::Missing),
            _ => Err(WorkspaceReferenceResolveError::Ambiguous),
        }
    }
}

/// Resolve a board transclusion. A board link without a view resolves to an
/// item, while an embed always needs the board-view projection semantics where
/// an omitted view means “All cards.”
pub fn resolve_board_view_target(
    raw_target: &str,
    catalog: &WorkspaceReferenceCatalog,
) -> Result<ResolvedWorkspaceReference, WorkspaceReferenceResolveError> {
    let reference =
        parse_reference_target(raw_target).ok_or(WorkspaceReferenceResolveError::Invalid)?;
    if reference.kind != WorkspaceItemKind::Board {
        return Err(WorkspaceReferenceResolveError::Invalid);
    }
    let candidates = catalog
        .items
        .iter()
        .filter(|entry| entry.item.kind == WorkspaceItemKind::Board)
        .filter(|entry| path_matches_suffix(catalog, entry, &reference.segments))
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err(match candidates.len() {
            0 => WorkspaceReferenceResolveError::Missing,
            _ => WorkspaceReferenceResolveError::Ambiguous,
        });
    }
    let board = candidates[0];
    let view_id = match reference.view.as_deref() {
        None => None,
        Some(view_name) => {
            let views = catalog
                .views
                .iter()
                .filter(|view| view.board_id == board.item.id)
                .filter(|view| {
                    alias_matches(
                        catalog,
                        WorkspaceAliasTarget::SavedView(view.id),
                        &view.name,
                        view_name,
                    )
                })
                .collect::<Vec<_>>();
            if views.len() != 1 {
                return Err(match views.len() {
                    0 => WorkspaceReferenceResolveError::Missing,
                    _ => WorkspaceReferenceResolveError::Ambiguous,
                });
            }
            Some(views[0].id)
        }
    };
    Ok(ResolvedWorkspaceReference::BoardView {
        board_id: board.item.id,
        view_id,
    })
}

pub fn resolve_reference_target(
    raw_target: &str,
    catalog: &WorkspaceReferenceCatalog,
) -> Result<ResolvedWorkspaceReference, WorkspaceReferenceResolveError> {
    let reference =
        parse_reference_target(raw_target).ok_or(WorkspaceReferenceResolveError::Invalid)?;
    resolve_reference(&reference, catalog)
}

/// Resolve a readable item reference and return the catalog entry that owns
/// the target. Board-view transclusions are intentionally rejected here;
/// callers rendering a projection should use [`resolve_board_view_target`].
pub fn resolve_reference_entry<'a>(
    raw_target: &str,
    catalog: &'a WorkspaceReferenceCatalog,
) -> Result<&'a WorkspaceCatalogEntry, WorkspaceReferenceResolveError> {
    let item = match resolve_reference_target(raw_target, catalog)? {
        ResolvedWorkspaceReference::Item(item) => item,
        ResolvedWorkspaceReference::BoardView { .. } => {
            return Err(WorkspaceReferenceResolveError::Invalid);
        }
    };
    catalog
        .item(item)
        .ok_or(WorkspaceReferenceResolveError::Missing)
}

fn path_matches_suffix(
    catalog: &WorkspaceReferenceCatalog,
    entry: &WorkspaceCatalogEntry,
    candidate: &[String],
) -> bool {
    let options = path_options(catalog, entry);
    if candidate.len() > options.len() {
        return false;
    }
    options[options.len() - candidate.len()..]
        .iter()
        .zip(candidate)
        .all(|(segment_options, requested)| {
            segment_options
                .iter()
                .any(|option| normalize(option) == normalize(requested))
        })
}

fn path_options(
    catalog: &WorkspaceReferenceCatalog,
    entry: &WorkspaceCatalogEntry,
) -> Vec<Vec<String>> {
    let mut options = Vec::new();
    if let (Some(project_id), Some(project_name)) = (entry.project_id, entry.project_name.as_ref())
    {
        options.push(alias_options(
            catalog,
            WorkspaceAliasTarget::Project(project_id),
            project_name,
        ));
    }
    match entry.item.kind {
        WorkspaceItemKind::Note | WorkspaceItemKind::Board => {
            options.push(alias_options(
                catalog,
                WorkspaceAliasTarget::Item(entry.item),
                &entry.title,
            ));
        }
        WorkspaceItemKind::List => {
            let board = WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id: entry.board_id.unwrap_or_default(),
            };
            options.push(alias_options(
                catalog,
                WorkspaceAliasTarget::Item(board),
                entry.board_title.as_deref().unwrap_or("Unavailable board"),
            ));
            options.push(alias_options(
                catalog,
                WorkspaceAliasTarget::Item(entry.item),
                &entry.title,
            ));
        }
        WorkspaceItemKind::Card => {
            let board = WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id: entry.board_id.unwrap_or_default(),
            };
            let list = WorkspaceItemRef {
                kind: WorkspaceItemKind::List,
                id: entry.list_id.unwrap_or_default(),
            };
            options.push(alias_options(
                catalog,
                WorkspaceAliasTarget::Item(board),
                entry.board_title.as_deref().unwrap_or("Unavailable board"),
            ));
            options.push(alias_options(
                catalog,
                WorkspaceAliasTarget::Item(list),
                entry.list_title.as_deref().unwrap_or("Unavailable list"),
            ));
            options.push(alias_options(
                catalog,
                WorkspaceAliasTarget::Item(entry.item),
                &entry.title,
            ));
        }
    }
    options
}

fn alias_options(
    catalog: &WorkspaceReferenceCatalog,
    target: WorkspaceAliasTarget,
    current: &str,
) -> Vec<String> {
    let mut options = vec![current.to_string()];
    options.extend(
        catalog
            .aliases
            .iter()
            .filter(|alias| alias.target == target)
            .map(|alias| alias.alias.clone()),
    );
    options
}

fn alias_matches(
    catalog: &WorkspaceReferenceCatalog,
    target: WorkspaceAliasTarget,
    current: &str,
    requested: &str,
) -> bool {
    normalize(current) == normalize(requested)
        || catalog
            .aliases
            .iter()
            .any(|alias| alias.target == target && normalize(&alias.alias) == normalize(requested))
}

fn split_segments(value: &str) -> Result<Vec<String>, ()> {
    let raw_segments = split_unescaped_all(value, '/')?;
    if raw_segments.iter().any(|segment| segment.trim().is_empty()) {
        return Err(());
    }
    let segments = raw_segments
        .into_iter()
        .map(|segment| unescape_segment(segment.trim()))
        .collect::<Vec<_>>();
    (!segments.is_empty()).then_some(segments).ok_or(())
}

fn split_unescaped(value: &str, delimiter: char) -> Option<(&str, &str)> {
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

fn split_unescaped_all(value: &str, delimiter: char) -> Result<Vec<&str>, ()> {
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == delimiter {
            segments.push(&value[start..index]);
            start = index + character.len_utf8();
        }
    }
    if escaped {
        return Err(());
    }
    segments.push(&value[start..]);
    Ok(segments)
}

fn fence_run(line: &str, marker: u8) -> usize {
    line.as_bytes()
        .iter()
        .take_while(|byte| **byte == marker)
        .count()
}

pub fn escape_segment(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '/' | '#' | '|' | '[' | ']') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

pub fn unescape_segment(value: &str) -> String {
    let mut unescaped = String::with_capacity(value.len());
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            unescaped.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            unescaped.push(character);
        }
    }
    if escaped {
        unescaped.push('\\');
    }
    unescaped
}

fn has_unterminated_escape(value: &str) -> bool {
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        }
    }
    escaped
}

fn contains_unescaped_bracket(value: &str) -> bool {
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if matches!(character, '[' | ']') {
            return true;
        }
    }
    false
}

fn contains_unescaped(value: &str, delimiter: char) -> bool {
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == delimiter {
            return true;
        }
    }
    false
}

fn format_segments(segments: &[String]) -> String {
    segments
        .iter()
        .map(|segment| escape_segment(segment))
        .collect::<Vec<_>>()
        .join(" / ")
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

impl WorkspaceItemKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "note" => Some(Self::Note),
            "board" => Some(Self::Board),
            "list" => Some(Self::List),
            "card" => Some(Self::Card),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_readable_board_view_targets() {
        let parsed =
            parse_reference_target(r"board:Castle / Main\#Board#Current\|View|Dashboard\|label")
                .expect("readable board view should parse");
        assert_eq!(parsed.kind, WorkspaceItemKind::Board);
        assert_eq!(parsed.segments, vec!["Castle", "Main#Board"]);
        assert_eq!(parsed.view.as_deref(), Some("Current|View"));
        assert_eq!(parsed.display_text.as_deref(), Some("Dashboard|label"));

        let escaped = parse_reference_target(r"card:Launch\/v2|Open \[card\]")
            .expect("escaped card reference should parse");
        assert_eq!(escaped.segments, vec!["Launch/v2"]);
        assert_eq!(escaped.display_text.as_deref(), Some("Open [card]"));
    }

    #[test]
    fn parses_only_standalone_board_embeds() {
        let content = "before ![[board:Roadmap]]\n  ![[board:Roadmap#Current]]\n```md\n![[board:Hidden]]\n```\n";
        let embeds = parse_board_view_embeds(content);
        assert_eq!(embeds.len(), 1);
        assert_eq!(embeds[0].view_name.as_deref(), Some("Current"));
        assert_eq!(embeds[0].line_number, 2);

        let crlf = "![[board:Roadmap]]\r\n";
        let crlf_embeds = parse_board_view_embeds(crlf);
        assert_eq!(
            &crlf[crlf_embeds[0].start_byte..crlf_embeds[0].end_byte],
            "![[board:Roadmap]]"
        );
    }

    #[test]
    fn formatter_uses_the_shortest_unique_hierarchy_and_escapes_delimiters() {
        let catalog = WorkspaceReferenceCatalog {
            items: vec![
                WorkspaceCatalogEntry {
                    item: WorkspaceItemRef {
                        kind: WorkspaceItemKind::Board,
                        id: 1,
                    },
                    title: "Road/Map".into(),
                    project_id: Some(10),
                    project_name: Some("Castle".into()),
                    board_id: Some(1),
                    board_title: Some("Road/Map".into()),
                    list_id: None,
                    list_title: None,
                },
                WorkspaceCatalogEntry {
                    item: WorkspaceItemRef {
                        kind: WorkspaceItemKind::Board,
                        id: 2,
                    },
                    title: "Road/Map".into(),
                    project_id: Some(11),
                    project_name: Some("Other".into()),
                    board_id: Some(2),
                    board_title: Some("Road/Map".into()),
                    list_id: None,
                    list_title: None,
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            catalog.format_item_link(
                WorkspaceItemRef {
                    kind: WorkspaceItemKind::Board,
                    id: 1,
                },
                Some("Open | now"),
            ),
            Some("[[board:Castle / Road\\/Map|Open \\| now]]".into())
        );
        assert_eq!(
            catalog.format_board_view_with_display_text(1, None, Some("Open | board")),
            Some("![[board:Castle / Road\\/Map|Open \\| board]]".into())
        );
    }

    #[test]
    fn malformed_references_are_rejected_and_matching_is_case_insensitive() {
        assert!(parse_reference_target("board:Roadmap / ").is_none());
        assert!(parse_reference_target("board:Roadmap#").is_none());
        assert!(parse_reference_target("board:Roadmap#Current#Other").is_none());
        assert!(parse_reference_target("card:Roadmap#Current").is_none());
        assert!(parse_reference_target("board:Roadmap|One|Two").is_none());
        assert!(parse_reference_target(r"board:Roadmap\").is_none());
        assert!(parse_reference_target("board:42").is_none());
        assert!(parse_reference_target("card:42").is_none());
        assert!(parse_reference_target("unknown:Roadmap").is_none());

        let catalog = WorkspaceReferenceCatalog {
            items: vec![WorkspaceCatalogEntry {
                item: WorkspaceItemRef {
                    kind: WorkspaceItemKind::Board,
                    id: 4,
                },
                title: "Roadmap".into(),
                project_id: None,
                project_name: None,
                board_id: Some(4),
                board_title: Some("Roadmap".into()),
                list_id: None,
                list_title: None,
            }],
            ..Default::default()
        };
        assert_eq!(
            resolve_reference_target("BOARD:roadMAP", &catalog),
            Ok(ResolvedWorkspaceReference::Item(WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id: 4,
            }))
        );
        assert_eq!(
            resolve_board_view_target("board:ROADMAP", &catalog),
            Ok(ResolvedWorkspaceReference::BoardView {
                board_id: 4,
                view_id: None,
            })
        );
    }

    #[test]
    fn formatter_round_trips_every_item_kind_and_escaped_view_name() {
        let note = WorkspaceCatalogEntry {
            item: WorkspaceItemRef {
                kind: WorkspaceItemKind::Note,
                id: 1,
            },
            title: "Note/One".into(),
            project_id: Some(10),
            project_name: Some("Proj/ect".into()),
            board_id: None,
            board_title: None,
            list_id: None,
            list_title: None,
        };
        let board = WorkspaceCatalogEntry {
            item: WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id: 2,
            },
            title: "Board#One".into(),
            project_id: Some(10),
            project_name: Some("Proj/ect".into()),
            board_id: Some(2),
            board_title: Some("Board#One".into()),
            list_id: None,
            list_title: None,
        };
        let list = WorkspaceCatalogEntry {
            item: WorkspaceItemRef {
                kind: WorkspaceItemKind::List,
                id: 3,
            },
            title: "List|One".into(),
            project_id: Some(10),
            project_name: Some("Proj/ect".into()),
            board_id: Some(2),
            board_title: Some("Board#One".into()),
            list_id: Some(3),
            list_title: Some("List|One".into()),
        };
        let card = WorkspaceCatalogEntry {
            item: WorkspaceItemRef {
                kind: WorkspaceItemKind::Card,
                id: 4,
            },
            title: r"Card[One]\Tail".into(),
            project_id: Some(10),
            project_name: Some("Proj/ect".into()),
            board_id: Some(2),
            board_title: Some("Board#One".into()),
            list_id: Some(3),
            list_title: Some("List|One".into()),
        };
        let catalog = WorkspaceReferenceCatalog {
            items: vec![note, board, list, card],
            views: vec![WorkspaceViewCatalogEntry {
                id: 5,
                board_id: 2,
                name: r"View#One|Two\Tail".into(),
                project_id: Some(10),
                project_name: Some("Proj/ect".into()),
            }],
            ..Default::default()
        };

        for item in [
            WorkspaceItemRef {
                kind: WorkspaceItemKind::Note,
                id: 1,
            },
            WorkspaceItemRef {
                kind: WorkspaceItemKind::Board,
                id: 2,
            },
            WorkspaceItemRef {
                kind: WorkspaceItemKind::List,
                id: 3,
            },
            WorkspaceItemRef {
                kind: WorkspaceItemKind::Card,
                id: 4,
            },
        ] {
            let formatted = catalog
                .format_item_link(item, None)
                .expect("catalog item should format");
            let parsed = parse_reference_target(&formatted[2..formatted.len() - 2])
                .expect("formatted item should parse");
            assert_eq!(
                resolve_reference(&parsed, &catalog),
                Ok(ResolvedWorkspaceReference::Item(item))
            );
        }

        let formatted_view = catalog
            .format_board_view(2, Some(5))
            .expect("saved view should format");
        let embeds = parse_board_view_embeds(&formatted_view);
        assert_eq!(embeds.len(), 1);
        assert_eq!(embeds[0].view_name.as_deref(), Some(r"View#One|Two\Tail"));
        assert_eq!(
            resolve_board_view_target(&embeds[0].raw_target, &catalog),
            Ok(ResolvedWorkspaceReference::BoardView {
                board_id: 2,
                view_id: Some(5),
            })
        );
    }

    #[test]
    fn aliases_resolve_every_hierarchy_level_and_ambiguity_is_explicit() {
        let board = WorkspaceItemRef {
            kind: WorkspaceItemKind::Board,
            id: 4,
        };
        let list = WorkspaceItemRef {
            kind: WorkspaceItemKind::List,
            id: 5,
        };
        let card = WorkspaceItemRef {
            kind: WorkspaceItemKind::Card,
            id: 6,
        };
        let catalog = WorkspaceReferenceCatalog {
            items: vec![
                WorkspaceCatalogEntry {
                    item: board,
                    title: "Roadmap".into(),
                    project_id: Some(1),
                    project_name: Some("Castle".into()),
                    board_id: Some(4),
                    board_title: Some("Roadmap".into()),
                    list_id: None,
                    list_title: None,
                },
                WorkspaceCatalogEntry {
                    item: list,
                    title: "Doing".into(),
                    project_id: Some(1),
                    project_name: Some("Castle".into()),
                    board_id: Some(4),
                    board_title: Some("Roadmap".into()),
                    list_id: Some(5),
                    list_title: Some("Doing".into()),
                },
                WorkspaceCatalogEntry {
                    item: card,
                    title: "Ship it".into(),
                    project_id: Some(1),
                    project_name: Some("Castle".into()),
                    board_id: Some(4),
                    board_title: Some("Roadmap".into()),
                    list_id: Some(5),
                    list_title: Some("Doing".into()),
                },
                WorkspaceCatalogEntry {
                    item: WorkspaceItemRef {
                        kind: WorkspaceItemKind::Board,
                        id: 7,
                    },
                    title: "Roadmap".into(),
                    project_id: Some(2),
                    project_name: Some("Other".into()),
                    board_id: Some(7),
                    board_title: Some("Roadmap".into()),
                    list_id: None,
                    list_title: None,
                },
            ],
            views: vec![WorkspaceViewCatalogEntry {
                id: 8,
                board_id: 4,
                name: "Current".into(),
                project_id: Some(1),
                project_name: Some("Castle".into()),
            }],
            aliases: vec![
                WorkspaceReferenceAlias {
                    target: WorkspaceAliasTarget::Item(board),
                    alias: "Old board".into(),
                },
                WorkspaceReferenceAlias {
                    target: WorkspaceAliasTarget::Item(list),
                    alias: "In progress".into(),
                },
                WorkspaceReferenceAlias {
                    target: WorkspaceAliasTarget::Item(card),
                    alias: "Launch".into(),
                },
                WorkspaceReferenceAlias {
                    target: WorkspaceAliasTarget::SavedView(8),
                    alias: "Now".into(),
                },
            ],
        };

        assert_eq!(
            resolve_workspace_item_for_test("board:Old board", &catalog),
            Ok(board)
        );
        assert_eq!(
            resolve_workspace_item_for_test("list:In progress", &catalog),
            Ok(list)
        );
        assert_eq!(
            resolve_workspace_item_for_test("card:Launch", &catalog),
            Ok(card)
        );
        assert_eq!(
            resolve_board_view_target("board:Castle / Roadmap#Now", &catalog),
            Ok(ResolvedWorkspaceReference::BoardView {
                board_id: 4,
                view_id: Some(8),
            })
        );
        assert_eq!(
            resolve_workspace_item_for_test("board:Roadmap", &catalog),
            Err(WorkspaceReferenceResolveError::Ambiguous)
        );
    }

    fn resolve_workspace_item_for_test(
        raw_target: &str,
        catalog: &WorkspaceReferenceCatalog,
    ) -> Result<WorkspaceItemRef, WorkspaceReferenceResolveError> {
        match resolve_reference_target(raw_target, catalog)? {
            ResolvedWorkspaceReference::Item(item) => Ok(item),
            ResolvedWorkspaceReference::BoardView { .. } => {
                Err(WorkspaceReferenceResolveError::Invalid)
            }
        }
    }
}
