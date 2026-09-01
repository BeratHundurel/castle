use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};

use gpui::{
    App, FontWeight, HighlightStyle, InteractiveText, IntoElement, ParentElement as _, Styled as _,
    StyledText, Task, UnderlineStyle, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _,
    input::{CompletionProvider, Rope, RopeExt as _},
    text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast::Node},
};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit, TextEdit,
};

use crate::{WorkspaceNavigationHandler, WorkspaceNavigationTarget};

#[derive(Clone)]
pub struct WorkspaceReferenceCompletionProvider {
    state: Rc<RefCell<WikiLinkCompletionState>>,
}

pub type WikiLinkCompletionProvider = WorkspaceReferenceCompletionProvider;

struct WikiLinkCompletionState {
    note_id: i64,
    project_id: Option<i64>,
    enabled: bool,
    catalog: Arc<Vec<storage::note::links::NoteLinkCatalogEntry>>,
    workspace_catalog: Arc<storage::workspace::links::WorkspaceReferenceCatalog>,
}

impl WorkspaceReferenceCompletionProvider {
    pub fn new(note_id: i64) -> Self {
        Self {
            state: Rc::new(RefCell::new(WikiLinkCompletionState {
                note_id,
                project_id: None,
                enabled: false,
                catalog: Arc::new(Vec::new()),
                workspace_catalog: Arc::new(Default::default()),
            })),
        }
    }

    pub fn update(
        &self,
        note_id: i64,
        project_id: Option<i64>,
        enabled: bool,
        catalog: Arc<Vec<storage::note::links::NoteLinkCatalogEntry>>,
        workspace_catalog: Arc<Vec<storage::workspace::links::WorkspaceCatalogEntry>>,
    ) {
        let workspace_catalog = storage::workspace::links::WorkspaceReferenceCatalog {
            items: workspace_catalog.as_ref().clone(),
            ..Default::default()
        };
        *self.state.borrow_mut() = WikiLinkCompletionState {
            note_id,
            project_id,
            enabled,
            catalog,
            workspace_catalog: Arc::new(workspace_catalog),
        };
    }

    pub fn update_reference_catalog(
        &self,
        note_id: i64,
        project_id: Option<i64>,
        enabled: bool,
        catalog: Arc<storage::workspace::links::WorkspaceReferenceCatalog>,
    ) {
        let note_catalog = catalog
            .items
            .iter()
            .filter(|entry| entry.item.kind == storage::workspace::links::WorkspaceItemKind::Note)
            .map(|entry| storage::note::links::NoteLinkCatalogEntry {
                note_id: entry.item.id,
                title: entry.title.clone(),
                project_id: entry.project_id,
                project_name: entry.project_name.clone(),
            })
            .collect();
        *self.state.borrow_mut() = WikiLinkCompletionState {
            note_id,
            project_id,
            enabled,
            catalog: Arc::new(note_catalog),
            workspace_catalog: catalog,
        };
    }

    pub fn update_for_workspace_source(
        &self,
        project_id: Option<i64>,
        workspace_catalog: Arc<Vec<storage::workspace::links::WorkspaceCatalogEntry>>,
    ) {
        let note_catalog = workspace_catalog
            .iter()
            .filter(|entry| entry.item.kind == storage::workspace::links::WorkspaceItemKind::Note)
            .map(|entry| storage::note::links::NoteLinkCatalogEntry {
                note_id: entry.item.id,
                title: entry.title.clone(),
                project_id: entry.project_id,
                project_name: entry.project_name.clone(),
            })
            .collect();
        self.update(
            -1,
            project_id,
            true,
            Arc::new(note_catalog),
            workspace_catalog,
        );
    }
}

impl CompletionProvider for WorkspaceReferenceCompletionProvider {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _: lsp_types::CompletionContext,
        _: &mut Window,
        _: &mut App,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        let content = text.to_string();
        let Some(query_context) = reference_query_at_cursor(&content, offset) else {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        };

        let state = self.state.borrow();
        if !state.enabled {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        }
        let matches = reference_completion_candidates(
            &query_context.query,
            query_context.mode,
            state.note_id,
            state.project_id,
            state.catalog.as_ref(),
            state.workspace_catalog.as_ref(),
        );

        let replace_range = lsp_types::Range::new(
            text.offset_to_position(query_context.replace_range.start),
            text.offset_to_position(query_context.replace_range.end),
        );

        let surrounding_newlines = query_context.surrounding_newlines;
        let items = matches
            .into_iter()
            .map(|candidate| {
                let new_text = if let Some((prefix, suffix)) = surrounding_newlines
                    && matches!(query_context.mode, ReferenceCompletionMode::Embed)
                {
                    format!(
                        "{}{}{}",
                        if prefix { "\n" } else { "" },
                        candidate.new_text,
                        if suffix { "\n" } else { "" },
                    )
                } else {
                    candidate.new_text
                };
                CompletionItem {
                    label: candidate.label,
                    detail: candidate.detail,
                    kind: Some(candidate.kind),
                    filter_text: Some(query_context.query.clone()),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: replace_range,
                        new_text,
                    })),
                    ..Default::default()
                }
            })
            .collect();

        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _: usize, _: &str, _: &mut App) -> bool {
        true
    }
}

#[derive(Clone, Debug)]
enum PreviewLinkTarget {
    Note(u32),
    Workspace(WorkspaceNavigationTarget),
    External(String),
}

#[derive(Clone, Debug)]
struct PreviewLink {
    range: Range<usize>,
    target: Option<PreviewLinkTarget>,
}

#[derive(Clone, Debug)]
struct WikiLinkPreviewBlock {
    text: String,
    links: Vec<PreviewLink>,
    heading_depth: Option<u8>,
    source_offset: usize,
}

#[derive(Clone)]
pub struct WikiLinkPreviewPlugin {
    open_target: WorkspaceNavigationHandler,
    project_id: Option<i64>,
    catalog: Arc<Vec<storage::note::links::NoteLinkCatalogEntry>>,
    indexed_links: Arc<storage::note::links::NoteLinkSet>,
    workspace_catalog: Arc<storage::workspace::links::WorkspaceReferenceCatalog>,
}

impl WikiLinkPreviewPlugin {
    pub fn new(
        open_target: WorkspaceNavigationHandler,
        project_id: Option<i64>,
        catalog: Arc<Vec<storage::note::links::NoteLinkCatalogEntry>>,
        indexed_links: Arc<storage::note::links::NoteLinkSet>,
        workspace_catalog: Arc<storage::workspace::links::WorkspaceReferenceCatalog>,
    ) -> Self {
        Self {
            open_target,
            project_id,
            catalog,
            indexed_links,
            workspace_catalog,
        }
    }

    pub fn new_for_workspace(
        open_target: WorkspaceNavigationHandler,
        project_id: Option<i64>,
        workspace_catalog: Arc<Vec<storage::workspace::links::WorkspaceCatalogEntry>>,
    ) -> Self {
        Self::new_for_workspace_reference_catalog(
            open_target,
            project_id,
            Arc::new(storage::workspace::links::WorkspaceReferenceCatalog {
                items: workspace_catalog.as_ref().clone(),
                ..Default::default()
            }),
        )
    }

    pub fn new_for_workspace_reference_catalog(
        open_target: WorkspaceNavigationHandler,
        project_id: Option<i64>,
        workspace_catalog: Arc<storage::workspace::links::WorkspaceReferenceCatalog>,
    ) -> Self {
        let catalog = workspace_catalog
            .items
            .iter()
            .filter(|entry| entry.item.kind == storage::workspace::links::WorkspaceItemKind::Note)
            .map(|entry| storage::note::links::NoteLinkCatalogEntry {
                note_id: entry.item.id,
                title: entry.title.clone(),
                project_id: entry.project_id,
                project_name: entry.project_name.clone(),
            })
            .collect();
        Self {
            open_target,
            project_id,
            catalog: Arc::new(catalog),
            indexed_links: Arc::new(storage::note::links::NoteLinkSet::default()),
            workspace_catalog,
        }
    }
}

impl MarkdownPlugin for WikiLinkPreviewPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "castle-wikilink-preview"
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let heading_depth = match node {
            Node::Paragraph(_) => None,
            Node::Heading(heading) => Some(heading.depth),
            _ => return None,
        };

        let mut block = WikiLinkPreviewBlock {
            text: String::new(),
            links: Vec::new(),
            heading_depth,
            source_offset: cx.offset()
                + node
                    .position()
                    .map(|position| position.start.offset)
                    .unwrap_or_default(),
        };

        append_preview_node(
            node,
            &mut block,
            self.project_id,
            &self.catalog,
            &self.indexed_links,
            &self.workspace_catalog,
        );
        (!block.links.is_empty()).then(|| {
            MarkdownNode::new(self.name(), block.clone())
                .text(block.text)
                .markdown(cx.node_source(node).unwrap_or_default())
        })
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        let Some(block) = node.data::<WikiLinkPreviewBlock>() else {
            return div().into_any_element();
        };

        let clickable = block
            .links
            .iter()
            .filter_map(|link| {
                link.target
                    .clone()
                    .map(|target| (link.range.clone(), target))
            })
            .collect::<Vec<_>>();

        let ranges = clickable
            .iter()
            .map(|(range, _)| range.clone())
            .collect::<Vec<_>>();

        let targets = clickable
            .into_iter()
            .map(|(_, target)| target)
            .collect::<Vec<_>>();

        let highlights = block.links.iter().map(|link| {
            let resolved = link.target.is_some();
            (
                link.range.clone(),
                HighlightStyle {
                    color: Some(if resolved {
                        cx.theme().link
                    } else {
                        cx.theme().warning
                    }),
                    font_weight: Some(FontWeight::MEDIUM),
                    underline: resolved.then_some(UnderlineStyle {
                        thickness: gpui::px(1.),
                        color: Some(cx.theme().link),
                        wavy: false,
                    }),
                    ..Default::default()
                },
            )
        });

        let open_target = self.open_target.clone();
        let text = InteractiveText::new(
            ("wikilink-preview", block.source_offset),
            StyledText::new(block.text.clone()).with_highlights(highlights),
        )
        .on_click(ranges, move |index, _, cx| {
            let Some(target) = targets.get(index) else {
                return;
            };
            match target {
                PreviewLinkTarget::Note(note_id) => {
                    open_target(
                        WorkspaceNavigationTarget::Note {
                            note_id: *note_id,
                            source_offset: None,
                        },
                        cx,
                    );
                }
                PreviewLinkTarget::Workspace(target) => {
                    open_target(*target, cx);
                }
                PreviewLinkTarget::External(url) => cx.open_url(url),
            }
        });

        div()
            .w_full()
            .whitespace_normal()
            .when(block.heading_depth.is_some(), |this| {
                this.font_weight(FontWeight::SEMIBOLD)
            })
            .child(text)
            .into_any_element()
    }
}

fn append_preview_node(
    node: &Node,
    block: &mut WikiLinkPreviewBlock,
    project_id: Option<i64>,
    catalog: &[storage::note::links::NoteLinkCatalogEntry],
    indexed_links: &storage::note::links::NoteLinkSet,
    workspace_catalog: &storage::workspace::links::WorkspaceReferenceCatalog,
) {
    match node {
        Node::Text(text) => append_preview_text(
            &text.value,
            block,
            project_id,
            catalog,
            indexed_links,
            workspace_catalog,
        ),
        Node::InlineCode(code) => block.text.push_str(&code.value),
        Node::InlineMath(math) => block.text.push_str(&math.value),
        Node::Break(_) => block.text.push('\n'),
        Node::Image(image) => block.text.push_str(&image.alt),
        Node::Link(link) => {
            let start = block.text.len();
            for child in &link.children {
                append_preview_node(
                    child,
                    block,
                    project_id,
                    catalog,
                    indexed_links,
                    workspace_catalog,
                );
            }
            let end = block.text.len();
            if start < end {
                block.links.push(PreviewLink {
                    range: start..end,
                    target: Some(PreviewLinkTarget::External(link.url.clone())),
                });
            }
        }
        _ => {
            if let Some(children) = node.children() {
                for child in children {
                    append_preview_node(
                        child,
                        block,
                        project_id,
                        catalog,
                        indexed_links,
                        workspace_catalog,
                    );
                }
            } else {
                block.text.push_str(&node.to_string());
            }
        }
    }
}

fn append_preview_text(
    text: &str,
    block: &mut WikiLinkPreviewBlock,
    project_id: Option<i64>,
    catalog: &[storage::note::links::NoteLinkCatalogEntry],
    indexed_links: &storage::note::links::NoteLinkSet,
    workspace_catalog: &storage::workspace::links::WorkspaceReferenceCatalog,
) {
    let mut consumed = 0;
    for link in storage::note::links::parse_wikilinks(text) {
        block.text.push_str(&text[consumed..link.start_byte]);
        let start = block.text.len();
        block
            .text
            .push_str(link.display_text.as_deref().unwrap_or(&link.raw_target));
        let end = block.text.len();
        let target =
            storage::workspace::links::resolve_reference_entry(&link.raw_target, workspace_catalog)
                .ok()
                .and_then(workspace_navigation_target)
                .map(PreviewLinkTarget::Workspace)
                .or_else(|| {
                    resolve_preview_note(&link.raw_target, project_id, catalog, indexed_links)
                        .and_then(|note_id| u32::try_from(note_id).ok())
                        .map(PreviewLinkTarget::Note)
                });
        block.links.push(PreviewLink {
            range: start..end,
            target,
        });
        consumed = link.end_byte;
    }
    block.text.push_str(&text[consumed..]);
}

fn resolve_preview_note(
    raw_target: &str,
    project_id: Option<i64>,
    catalog: &[storage::note::links::NoteLinkCatalogEntry],
    indexed_links: &storage::note::links::NoteLinkSet,
) -> Option<i64> {
    if let Some(target) = indexed_links.outbound.iter().find(|link| {
        link.raw_target.eq_ignore_ascii_case(raw_target) && link.target_note_id.is_some()
    }) {
        return target.target_note_id;
    }
    if let Some(reference) = storage::workspace::links::parse_reference_target(raw_target) {
        if reference.kind != storage::workspace::links::WorkspaceItemKind::Note {
            return None;
        }
        let target = catalog.iter().filter(|note| {
            let mut path = Vec::new();
            if let Some(project) = note.project_name.as_ref() {
                path.push(project.as_str());
            }
            path.push(note.title.as_str());
            reference.segments.len() <= path.len()
                && path[path.len() - reference.segments.len()..]
                    .iter()
                    .zip(&reference.segments)
                    .all(|(actual, requested)| actual.eq_ignore_ascii_case(requested))
        });
        return unique_catalog_note(target);
    }
    if let Some((project, title)) = raw_target.split_once('/') {
        return unique_catalog_note(catalog.iter().filter(|note| {
            note.project_name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(project.trim()))
                && note.title.eq_ignore_ascii_case(title.trim())
        }));
    }
    let local = unique_catalog_note(catalog.iter().filter(|note| {
        note.project_id == project_id && note.title.eq_ignore_ascii_case(raw_target.trim())
    }));
    local.or_else(|| {
        unique_catalog_note(
            catalog
                .iter()
                .filter(|note| note.title.eq_ignore_ascii_case(raw_target.trim())),
        )
    })
}

fn unique_catalog_note<'a>(
    mut candidates: impl Iterator<Item = &'a storage::note::links::NoteLinkCatalogEntry>,
) -> Option<i64> {
    let first = candidates.next()?.note_id;
    candidates.next().is_none().then_some(first)
}

pub fn workspace_navigation_target(
    entry: &storage::workspace::links::WorkspaceCatalogEntry,
) -> Option<WorkspaceNavigationTarget> {
    let item_id = u32::try_from(entry.item.id).ok()?;
    match entry.item.kind {
        storage::workspace::links::WorkspaceItemKind::Note => {
            Some(WorkspaceNavigationTarget::Note {
                note_id: item_id,
                source_offset: None,
            })
        }
        storage::workspace::links::WorkspaceItemKind::Board => {
            Some(WorkspaceNavigationTarget::board(item_id))
        }
        storage::workspace::links::WorkspaceItemKind::List => Some(
            WorkspaceNavigationTarget::list(u32::try_from(entry.board_id?).ok()?, item_id),
        ),
        storage::workspace::links::WorkspaceItemKind::Card => Some(
            WorkspaceNavigationTarget::card(u32::try_from(entry.board_id?).ok()?, item_id),
        ),
    }
}

#[allow(dead_code)]
fn wikilink_query_at_cursor(text: &str, cursor: usize) -> Option<(Range<usize>, &str)> {
    if cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    let line_start = text[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let opening = text[line_start..cursor].rfind("[[")? + line_start;
    if opening > 0 && text.as_bytes().get(opening - 1) == Some(&b'\\') {
        return None;
    }
    let query = &text[opening + 2..cursor];
    let replace_end = if text[cursor..].starts_with("]]") {
        cursor + 2
    } else {
        cursor
    };
    (!query.contains([']', '|', '`']) && query.len() <= 128)
        .then_some((opening..replace_end, query))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReferenceCompletionMode {
    Link(Option<storage::workspace::links::WorkspaceItemKind>),
    Embed,
    Slash(storage::workspace::links::WorkspaceItemKind),
}

struct ReferenceQuery<'a> {
    replace_range: Range<usize>,
    query: String,
    mode: ReferenceCompletionMode,
    surrounding_newlines: Option<(bool, bool)>,
    _marker: std::marker::PhantomData<&'a str>,
}

fn reference_query_at_cursor(text: &str, cursor: usize) -> Option<ReferenceQuery<'_>> {
    if cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    let line_start = text[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let line = &text[line_start..cursor];
    if let Some(opening) = line.rfind("![[") {
        let opening = line_start + opening;
        let query = &text[opening + 3..cursor];
        if !is_escaped_at(text, opening)
            && !contains_unescaped_query_delimiter(query)
            && query.len() <= 256
        {
            let replace_end = if text[cursor..].starts_with("]]") {
                cursor + 2
            } else {
                cursor
            };
            return Some(ReferenceQuery {
                replace_range: opening..replace_end,
                query: query.to_string(),
                mode: ReferenceCompletionMode::Embed,
                surrounding_newlines: None,
                _marker: std::marker::PhantomData,
            });
        }
    }
    if let Some(opening) = line.rfind("[[") {
        let opening = line_start + opening;
        if !is_escaped_at(text, opening)
            && (opening == 0 || text.as_bytes().get(opening - 1) != Some(&b'!'))
        {
            let query = &text[opening + 2..cursor];
            if !contains_unescaped_query_delimiter(query) && query.len() <= 256 {
                let replace_end = if text[cursor..].starts_with("]]") {
                    cursor + 2
                } else {
                    cursor
                };
                let (kind, query) = reference_kind_query(query);
                return Some(ReferenceQuery {
                    replace_range: opening..replace_end,
                    query: query.to_string(),
                    mode: ReferenceCompletionMode::Link(kind),
                    surrounding_newlines: None,
                    _marker: std::marker::PhantomData,
                });
            }
        }
    }
    let slash = line
        .char_indices()
        .rev()
        .find_map(|(index, character)| (character == '/').then_some(index))?;
    let slash = line_start + slash;
    if slash > line_start
        && text[..slash]
            .chars()
            .next_back()
            .is_some_and(|character| !character.is_whitespace())
    {
        return None;
    }
    let command = text[slash + 1..cursor].split_once(char::is_whitespace);
    let (command, query) = command.unwrap_or((&text[slash + 1..cursor], ""));
    let kind = match command.trim().to_ascii_lowercase().as_str() {
        "board" => storage::workspace::links::WorkspaceItemKind::Board,
        "list" => storage::workspace::links::WorkspaceItemKind::List,
        "card" => storage::workspace::links::WorkspaceItemKind::Card,
        "board-view" => storage::workspace::links::WorkspaceItemKind::Board,
        _ => return None,
    };
    let mode = if command.eq_ignore_ascii_case("board-view") {
        ReferenceCompletionMode::Embed
    } else {
        ReferenceCompletionMode::Slash(kind)
    };
    let surrounding_newlines = if command.eq_ignore_ascii_case("board-view") {
        let line_prefix = &line[..slash.saturating_sub(line_start)];
        let suffix_end = text[cursor..]
            .find('\n')
            .map_or(text.len(), |end| cursor + end);
        let line_suffix = &text[cursor..suffix_end];
        Some((
            !line_prefix.trim().is_empty(),
            !line_suffix.trim().is_empty(),
        ))
    } else {
        None
    };
    Some(ReferenceQuery {
        replace_range: slash..cursor,
        query: query.trim().to_string(),
        mode,
        surrounding_newlines,
        _marker: std::marker::PhantomData,
    })
}

fn reference_kind_query(
    query: &str,
) -> (Option<storage::workspace::links::WorkspaceItemKind>, &str) {
    query
        .split_once(':')
        .and_then(|(prefix, query)| {
            Some((
                Some(storage::workspace::links::WorkspaceItemKind::from_str(
                    prefix,
                )?),
                query.trim(),
            ))
        })
        .unwrap_or((None, query.trim()))
}

fn is_escaped_at(text: &str, index: usize) -> bool {
    let slash_count = text.as_bytes()[..index]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    slash_count % 2 == 1
}

fn contains_unescaped_query_delimiter(value: &str) -> bool {
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if matches!(character, '|' | '`' | ']') {
            return true;
        }
    }
    false
}

fn split_unescaped_fragment(value: &str, delimiter: char) -> Option<(&str, &str)> {
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

fn reference_entry_matches_query(
    catalog: &storage::workspace::links::WorkspaceReferenceCatalog,
    entry: &storage::workspace::links::WorkspaceCatalogEntry,
    normalized_query: &str,
) -> bool {
    if normalized_query.is_empty() {
        return true;
    }
    if reference_entry_paths(catalog, entry)
        .iter()
        .any(|path| path.contains(normalized_query))
    {
        return true;
    }
    false
}

fn reference_entry_paths(
    catalog: &storage::workspace::links::WorkspaceReferenceCatalog,
    entry: &storage::workspace::links::WorkspaceCatalogEntry,
) -> Vec<String> {
    let segments = catalog.item_path(entry);
    let mut targets = Vec::new();
    if let (Some(project_id), Some(_)) = (entry.project_id, entry.project_name.as_ref()) {
        targets.push(Some(
            storage::workspace::links::WorkspaceAliasTarget::Project(project_id),
        ));
    }
    match entry.item.kind {
        storage::workspace::links::WorkspaceItemKind::Note
        | storage::workspace::links::WorkspaceItemKind::Board => {
            targets.push(Some(storage::workspace::links::WorkspaceAliasTarget::Item(
                entry.item,
            )));
        }
        storage::workspace::links::WorkspaceItemKind::List => {
            targets.push(entry.board_id.map(|id| {
                storage::workspace::links::WorkspaceAliasTarget::Item(
                    storage::workspace::links::WorkspaceItemRef {
                        kind: storage::workspace::links::WorkspaceItemKind::Board,
                        id,
                    },
                )
            }));
            targets.push(Some(storage::workspace::links::WorkspaceAliasTarget::Item(
                entry.item,
            )));
        }
        storage::workspace::links::WorkspaceItemKind::Card => {
            targets.push(entry.board_id.map(|id| {
                storage::workspace::links::WorkspaceAliasTarget::Item(
                    storage::workspace::links::WorkspaceItemRef {
                        kind: storage::workspace::links::WorkspaceItemKind::Board,
                        id,
                    },
                )
            }));
            targets.push(entry.list_id.map(|id| {
                storage::workspace::links::WorkspaceAliasTarget::Item(
                    storage::workspace::links::WorkspaceItemRef {
                        kind: storage::workspace::links::WorkspaceItemKind::List,
                        id,
                    },
                )
            }));
            targets.push(Some(storage::workspace::links::WorkspaceAliasTarget::Item(
                entry.item,
            )));
        }
    }
    let mut paths = vec![Vec::new()];
    for (segment, target) in segments.into_iter().zip(targets) {
        let mut options = vec![segment];
        if let Some(target) = target {
            options.extend(
                catalog
                    .aliases
                    .iter()
                    .filter(|alias| alias.target == target)
                    .map(|alias| alias.alias.clone()),
            );
        }
        let mut next = Vec::with_capacity(paths.len().saturating_mul(options.len()));
        for path in paths {
            for option in &options {
                let mut candidate = path.clone();
                candidate.push(option.to_lowercase());
                next.push(candidate);
            }
        }
        paths = next;
    }
    paths.into_iter().map(|path| path.join(" / ")).collect()
}

fn reference_completion_candidates(
    query: &str,
    mode: ReferenceCompletionMode,
    note_id: i64,
    project_id: Option<i64>,
    note_catalog: &[storage::note::links::NoteLinkCatalogEntry],
    catalog: &storage::workspace::links::WorkspaceReferenceCatalog,
) -> Vec<WikiLinkCompletionCandidate> {
    let mut candidates = Vec::new();
    let (selected_kind, query) = reference_kind_query(query);
    let normalized_query = storage::workspace::links::unescape_segment(query).to_lowercase();

    if matches!(mode, ReferenceCompletionMode::Embed)
        && selected_kind
            .is_some_and(|kind| kind != storage::workspace::links::WorkspaceItemKind::Board)
    {
        return candidates;
    }

    if matches!(
        mode,
        ReferenceCompletionMode::Link(None)
            | ReferenceCompletionMode::Link(Some(
                storage::workspace::links::WorkspaceItemKind::Note
            ))
    ) {
        let typed_note = matches!(
            mode,
            ReferenceCompletionMode::Link(Some(storage::workspace::links::WorkspaceItemKind::Note))
        );
        candidates.extend(
            wikilink_matches(query, note_id, project_id, note_catalog)
                .into_iter()
                .map(|note| WikiLinkCompletionCandidate {
                    label: note.title.clone(),
                    detail: Some(format!(
                        "note · {}",
                        note.project_name
                            .as_ref()
                            .map(|project| format!("{project} / {}", note.title))
                            .unwrap_or_else(|| note.title.clone())
                    )),
                    kind: CompletionItemKind::FILE,
                    new_text: if typed_note {
                        catalog
                            .format_item_link(
                                storage::workspace::links::WorkspaceItemRef {
                                    kind: storage::workspace::links::WorkspaceItemKind::Note,
                                    id: note.note_id,
                                },
                                None,
                            )
                            .unwrap_or_else(|| wikilink_for_candidate(&note, note_catalog))
                    } else {
                        wikilink_for_candidate(&note, note_catalog)
                    },
                    rank: note_completion_rank(&note, &normalized_query, project_id),
                }),
        );
    }

    let item_kind = match mode {
        ReferenceCompletionMode::Link(kind) => selected_kind.or(kind),
        ReferenceCompletionMode::Slash(kind) => Some(selected_kind.unwrap_or(kind)),
        ReferenceCompletionMode::Embed => None,
    };
    if matches!(
        mode,
        ReferenceCompletionMode::Link(_) | ReferenceCompletionMode::Slash(_)
    ) {
        let kind = item_kind;
        let mut matches = catalog
            .items
            .iter()
            .filter(|entry| entry.item.kind != storage::workspace::links::WorkspaceItemKind::Note)
            .filter(|entry| kind.is_none_or(|kind| entry.item.kind == kind))
            .filter(|entry| reference_entry_matches_query(catalog, entry, &normalized_query))
            .collect::<Vec<_>>();
        matches.sort_by_key(|entry| {
            workspace_completion_rank(catalog, entry, &normalized_query, project_id)
        });
        candidates.extend(matches.into_iter().filter_map(|entry| {
            let new_text = catalog.format_item_link(entry.item, None)?;
            Some(WikiLinkCompletionCandidate {
                label: entry.title.clone(),
                detail: Some(format!(
                    "{} · {}",
                    entry.item.kind.as_str(),
                    catalog.item_path(entry).join(" / ")
                )),
                kind: completion_kind(entry.item.kind),
                new_text,
                rank: workspace_completion_rank(catalog, entry, &normalized_query, project_id),
            })
        }));
    }

    if matches!(mode, ReferenceCompletionMode::Embed) {
        let (board_query, view_query) = split_unescaped_fragment(query, '#')
            .map(|(board, view)| (board.trim(), Some(view.trim())))
            .unwrap_or((query.trim(), None));
        let normalized_board_query =
            storage::workspace::links::unescape_segment(board_query).to_lowercase();
        let normalized_view_query = view_query
            .map(storage::workspace::links::unescape_segment)
            .map(|query| query.to_lowercase());
        let mut matches = Vec::new();
        for board in catalog
            .items
            .iter()
            .filter(|entry| entry.item.kind == storage::workspace::links::WorkspaceItemKind::Board)
        {
            let board_path = catalog.item_path(board).join(" / ");
            let board_matches =
                reference_entry_matches_query(catalog, board, &normalized_board_query);
            if normalized_view_query.is_none() && board_matches {
                if let Some(new_text) = catalog.format_board_view(board.item.id, None) {
                    matches.push(WikiLinkCompletionCandidate {
                        label: format!("{} · All cards", board.title),
                        detail: Some(format!("board · {board_path}")),
                        kind: CompletionItemKind::MODULE,
                        new_text,
                        rank: embed_completion_rank(
                            catalog,
                            board,
                            &board_path,
                            "All cards",
                            &normalized_board_query,
                            project_id,
                            &[],
                        ),
                    });
                }
            }
            for view in catalog
                .views
                .iter()
                .filter(|view| view.board_id == board.item.id)
            {
                let label = format!("{} · {}", board.title, view.name);
                let view_matches = normalized_view_query.as_ref().is_none_or(|query| {
                    view.name.to_lowercase().contains(query)
                        || catalog.aliases.iter().any(|alias| {
                            alias.target
                                == storage::workspace::links::WorkspaceAliasTarget::SavedView(
                                    view.id,
                                )
                                && alias.alias.to_lowercase().contains(query)
                        })
                });
                if board_matches && view_matches {
                    if let Some(new_text) = catalog.format_board_view(board.item.id, Some(view.id))
                    {
                        let view_aliases = catalog
                            .aliases
                            .iter()
                            .filter(|alias| {
                                alias.target
                                    == storage::workspace::links::WorkspaceAliasTarget::SavedView(
                                        view.id,
                                    )
                            })
                            .map(|alias| alias.alias.to_lowercase())
                            .collect::<Vec<_>>();
                        matches.push(WikiLinkCompletionCandidate {
                            label,
                            detail: Some(format!("saved view · {board_path} / {}", view.name)),
                            kind: CompletionItemKind::VALUE,
                            new_text,
                            rank: embed_completion_rank(
                                catalog,
                                board,
                                &board_path,
                                &view.name,
                                normalized_view_query
                                    .as_deref()
                                    .unwrap_or(&normalized_board_query),
                                project_id,
                                &view_aliases,
                            ),
                        });
                    }
                }
            }
        }
        candidates.extend(matches);
    }

    candidates.sort_by_key(|candidate| candidate.rank.clone());
    candidates.truncate(12);
    candidates
}

fn completion_kind(kind: storage::workspace::links::WorkspaceItemKind) -> CompletionItemKind {
    match kind {
        storage::workspace::links::WorkspaceItemKind::Board => CompletionItemKind::MODULE,
        storage::workspace::links::WorkspaceItemKind::List => CompletionItemKind::FOLDER,
        storage::workspace::links::WorkspaceItemKind::Card => CompletionItemKind::VALUE,
        storage::workspace::links::WorkspaceItemKind::Note => CompletionItemKind::FILE,
    }
}

fn workspace_completion_rank(
    catalog: &storage::workspace::links::WorkspaceReferenceCatalog,
    entry: &storage::workspace::links::WorkspaceCatalogEntry,
    query: &str,
    project_id: Option<i64>,
) -> (bool, bool, bool, String, i64) {
    let title = entry.title.to_lowercase();
    let breadcrumb = catalog.item_path(entry).join(" / ").to_lowercase();
    let paths = reference_entry_paths(catalog, entry);
    let item_aliases = catalog
        .aliases
        .iter()
        .filter(|alias| {
            alias.target == storage::workspace::links::WorkspaceAliasTarget::Item(entry.item)
        })
        .map(|alias| alias.alias.to_lowercase())
        .collect::<Vec<_>>();
    let exact = title == query
        || breadcrumb == query
        || paths.iter().any(|path| path == query)
        || item_aliases.iter().any(|alias| alias == query);
    let prefix = title.starts_with(query)
        || breadcrumb.starts_with(query)
        || paths.iter().any(|path| path.starts_with(query))
        || item_aliases.iter().any(|alias| alias.starts_with(query));
    (
        !exact,
        !prefix,
        entry.project_id != project_id,
        breadcrumb,
        entry.item.id,
    )
}

fn note_completion_rank(
    note: &storage::note::links::NoteLinkCatalogEntry,
    query: &str,
    project_id: Option<i64>,
) -> (bool, bool, bool, String, i64) {
    let title = note.title.to_lowercase();
    let breadcrumb = note
        .project_name
        .as_ref()
        .map(|project| format!("{} / {title}", project.to_lowercase()))
        .unwrap_or_else(|| title.clone());
    (
        !(title == query || breadcrumb == query),
        !(title.starts_with(query) || breadcrumb.starts_with(query)),
        note.project_id != project_id,
        breadcrumb,
        note.note_id,
    )
}

fn embed_completion_rank(
    catalog: &storage::workspace::links::WorkspaceReferenceCatalog,
    board: &storage::workspace::links::WorkspaceCatalogEntry,
    board_path: &str,
    view_name: &str,
    query: &str,
    project_id: Option<i64>,
    aliases: &[String],
) -> (bool, bool, bool, String, i64) {
    let board_title = board.title.to_lowercase();
    let path = board_path.to_lowercase();
    let view = view_name.to_lowercase();
    let label = format!("{board_title} · {view}");
    let board_paths = reference_entry_paths(catalog, board);
    let board_aliases = catalog
        .aliases
        .iter()
        .filter(|alias| {
            alias.target == storage::workspace::links::WorkspaceAliasTarget::Item(board.item)
        })
        .map(|alias| alias.alias.to_lowercase())
        .collect::<Vec<_>>();
    let exact_alias = aliases.iter().any(|alias| alias == query);
    let prefix_alias = aliases.iter().any(|alias| alias.starts_with(query));
    (
        !(board_title == query
            || path == query
            || view == query
            || label == query
            || board_paths.iter().any(|path| path == query)
            || board_aliases.iter().any(|alias| alias == query)
            || exact_alias),
        !(board_title.starts_with(query)
            || path.starts_with(query)
            || view.starts_with(query)
            || label.starts_with(query)
            || board_paths.iter().any(|path| path.starts_with(query))
            || board_aliases.iter().any(|alias| alias.starts_with(query))
            || prefix_alias),
        board.project_id != project_id,
        format!("{path} · {label}"),
        board.item.id,
    )
}

struct WikiLinkCompletionCandidate {
    label: String,
    detail: Option<String>,
    kind: CompletionItemKind,
    new_text: String,
    rank: (bool, bool, bool, String, i64),
}

#[allow(dead_code)]
fn wikilink_completion_candidates(
    query: &str,
    note_id: i64,
    project_id: Option<i64>,
    note_catalog: &[storage::note::links::NoteLinkCatalogEntry],
    workspace_catalog: &[storage::workspace::links::WorkspaceCatalogEntry],
) -> Vec<WikiLinkCompletionCandidate> {
    let (selected_kind, query) = query
        .split_once(':')
        .and_then(|(prefix, query)| {
            let kind = match prefix.trim().to_ascii_lowercase().as_str() {
                "note" => storage::workspace::links::WorkspaceItemKind::Note,
                "board" => storage::workspace::links::WorkspaceItemKind::Board,
                "list" => storage::workspace::links::WorkspaceItemKind::List,
                "card" => storage::workspace::links::WorkspaceItemKind::Card,
                _ => return None,
            };
            Some((Some(kind), query.trim()))
        })
        .unwrap_or((None, query));

    let mut candidates = Vec::new();
    if selected_kind.is_none()
        || selected_kind == Some(storage::workspace::links::WorkspaceItemKind::Note)
    {
        candidates.extend(
            wikilink_matches(query, note_id, project_id, note_catalog)
                .into_iter()
                .map(|note| WikiLinkCompletionCandidate {
                    label: note.title.clone(),
                    detail: note.project_name.clone(),
                    kind: CompletionItemKind::FILE,
                    new_text: wikilink_for_candidate(&note, note_catalog),
                    rank: note_completion_rank(
                        &note,
                        &storage::workspace::links::unescape_segment(query).to_lowercase(),
                        project_id,
                    ),
                }),
        );
    }

    if selected_kind != Some(storage::workspace::links::WorkspaceItemKind::Note) {
        let normalized_query = query.to_lowercase();
        let mut workspace_matches = workspace_catalog
            .iter()
            .filter(|entry| entry.item.kind != storage::workspace::links::WorkspaceItemKind::Note)
            .filter(|entry| selected_kind.is_none_or(|kind| entry.item.kind == kind))
            .filter(|entry| {
                normalized_query.is_empty()
                    || entry.title.to_lowercase().contains(&normalized_query)
                    || entry
                        .breadcrumb()
                        .to_lowercase()
                        .contains(&normalized_query)
            })
            .cloned()
            .collect::<Vec<_>>();
        workspace_matches.sort_by_key(|entry| {
            (
                entry.project_id != project_id,
                entry.item.kind.as_str(),
                !entry.title.to_lowercase().starts_with(&normalized_query),
                entry.breadcrumb().to_lowercase(),
                entry.item.id,
            )
        });
        candidates.extend(workspace_matches.into_iter().map(|entry| {
            let kind = match entry.item.kind {
                storage::workspace::links::WorkspaceItemKind::Board => CompletionItemKind::MODULE,
                storage::workspace::links::WorkspaceItemKind::List => CompletionItemKind::FOLDER,
                storage::workspace::links::WorkspaceItemKind::Card => CompletionItemKind::VALUE,
                storage::workspace::links::WorkspaceItemKind::Note => CompletionItemKind::FILE,
            };
            WikiLinkCompletionCandidate {
                label: entry.title.clone(),
                detail: Some(entry.breadcrumb()),
                kind,
                new_text: entry.stable_link(),
                rank: (
                    !(entry.title.eq_ignore_ascii_case(&normalized_query)
                        || entry.breadcrumb().eq_ignore_ascii_case(&normalized_query)),
                    !(entry.title.to_lowercase().starts_with(&normalized_query)
                        || entry
                            .breadcrumb()
                            .to_lowercase()
                            .starts_with(&normalized_query)),
                    entry.project_id != project_id,
                    entry.breadcrumb().to_lowercase(),
                    entry.item.id,
                ),
            }
        }));
    }

    candidates.truncate(12);
    candidates
}

fn wikilink_matches(
    query: &str,
    note_id: i64,
    project_id: Option<i64>,
    catalog: &[storage::note::links::NoteLinkCatalogEntry],
) -> Vec<storage::note::links::NoteLinkCatalogEntry> {
    let query = storage::workspace::links::unescape_segment(query).to_lowercase();
    let mut matches = catalog
        .iter()
        .filter(|candidate| candidate.note_id != note_id)
        .filter_map(|candidate| {
            let title = candidate.title.to_lowercase();
            let project = candidate
                .project_name
                .as_deref()
                .unwrap_or("")
                .to_lowercase();
            let readable_breadcrumb = if project.is_empty() {
                title.clone()
            } else {
                format!("{project} / {title}")
            };
            let compact_breadcrumb = if project.is_empty() {
                title.clone()
            } else {
                format!("{project}/{title}")
            };
            let matches = [
                title.as_str(),
                readable_breadcrumb.as_str(),
                compact_breadcrumb.as_str(),
            ]
            .iter()
            .any(|candidate| candidate.contains(&query));
            matches.then_some((
                ![
                    title.as_str(),
                    readable_breadcrumb.as_str(),
                    compact_breadcrumb.as_str(),
                ]
                .iter()
                .any(|candidate| *candidate == query),
                ![
                    title.as_str(),
                    readable_breadcrumb.as_str(),
                    compact_breadcrumb.as_str(),
                ]
                .iter()
                .any(|candidate| candidate.starts_with(&query)),
                candidate.project_id != project_id,
                readable_breadcrumb,
                candidate.clone(),
            ))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        (&left.0, &left.1, &left.2, &left.3, left.4.note_id).cmp(&(
            &right.0,
            &right.1,
            &right.2,
            &right.3,
            right.4.note_id,
        ))
    });
    matches
        .into_iter()
        .map(|(_, _, _, _, candidate)| candidate)
        .collect()
}

fn wikilink_for_candidate(
    candidate: &storage::note::links::NoteLinkCatalogEntry,
    catalog: &[storage::note::links::NoteLinkCatalogEntry],
) -> String {
    let normalized_title = candidate.title.trim().to_lowercase();
    let same_title = catalog
        .iter()
        .filter(|note| note.title.trim().to_lowercase() == normalized_title)
        .collect::<Vec<_>>();
    if same_title.len() == 1 {
        return format!("[[{}]]", candidate.title);
    }
    if let Some(project_name) = candidate.project_name.as_deref() {
        let same_scope = same_title
            .iter()
            .filter(|note| note.project_id == candidate.project_id)
            .count();
        if same_scope == 1 {
            return format!("[[{project_name}/{}]]", candidate.title);
        }
    }
    let item = storage::workspace::links::WorkspaceItemRef {
        kind: storage::workspace::links::WorkspaceItemKind::Note,
        id: candidate.note_id,
    };
    storage::workspace::links::stable_workspace_link(item, candidate.title.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_only_an_open_wikilink_at_the_cursor() {
        assert_eq!(wikilink_query_at_cursor("a [[Al", 6), Some((2..6, "Al")));
        assert_eq!(wikilink_query_at_cursor("[[The]]", 5), Some((0..7, "The")));
        assert_eq!(wikilink_query_at_cursor("[[done]]", 8), None);
        assert_eq!(wikilink_query_at_cursor(r"\[[no", 5), None);
    }

    #[test]
    fn contextual_reference_queries_cover_links_embeds_and_slash_commands() {
        let link = reference_query_at_cursor("before [[card:Road", "before [[card:Road".len())
            .expect("link query should be detected");
        assert_eq!(link.query, "Road");
        assert_eq!(
            link.mode,
            ReferenceCompletionMode::Link(
                Some(storage::workspace::links::WorkspaceItemKind::Card,)
            )
        );

        let embed_text = "![[board:Roadmap#Cu";
        let embed = reference_query_at_cursor(embed_text, embed_text.len())
            .expect("embed query should be detected");
        assert_eq!(embed.query, "board:Roadmap#Cu");
        assert_eq!(embed.mode, ReferenceCompletionMode::Embed);

        let slash_text = "  /card Road";
        let slash = reference_query_at_cursor(slash_text, slash_text.len())
            .expect("slash query should be detected");
        assert_eq!(slash.query, "Road");
        assert_eq!(
            slash.mode,
            ReferenceCompletionMode::Slash(storage::workspace::links::WorkspaceItemKind::Card,)
        );
        let slash_embed =
            reference_query_at_cursor("Intro /board-view Road", "Intro /board-view Road".len())
                .expect("board-view slash query should be detected");
        assert_eq!(slash_embed.surrounding_newlines, Some((true, false)));
        assert!(reference_query_at_cursor(r"\[[Road", 6).is_none());
    }

    #[test]
    fn reference_completion_candidates_use_readable_replacement_text_and_saved_views() {
        let board = storage::workspace::links::WorkspaceCatalogEntry {
            item: storage::workspace::links::WorkspaceItemRef {
                kind: storage::workspace::links::WorkspaceItemKind::Board,
                id: 1,
            },
            title: "Roadmap".into(),
            project_id: None,
            project_name: None,
            board_id: Some(1),
            board_title: Some("Roadmap".into()),
            list_id: None,
            list_title: None,
        };
        let catalog = storage::workspace::links::WorkspaceReferenceCatalog {
            items: vec![board],
            views: vec![storage::workspace::links::WorkspaceViewCatalogEntry {
                id: 2,
                board_id: 1,
                name: "Current".into(),
                project_id: None,
                project_name: None,
            }],
            ..Default::default()
        };
        let all = reference_completion_candidates(
            "",
            ReferenceCompletionMode::Embed,
            -1,
            None,
            &[],
            &catalog,
        );
        assert_eq!(all.len(), 2);
        assert!(
            all.iter()
                .any(|candidate| candidate.new_text == "![[board:Roadmap]]")
        );
        assert!(
            all.iter()
                .any(|candidate| candidate.new_text == "![[board:Roadmap#Current]]")
        );

        let typed = reference_completion_candidates(
            "card:",
            ReferenceCompletionMode::Embed,
            -1,
            None,
            &[],
            &catalog,
        );
        assert!(typed.is_empty());

        let note = storage::note::links::NoteLinkCatalogEntry {
            note_id: 3,
            title: "Target note".into(),
            project_id: Some(4),
            project_name: Some("Castle".into()),
        };
        let note_catalog = storage::workspace::links::WorkspaceReferenceCatalog {
            items: vec![
                catalog.items[0].clone(),
                storage::workspace::links::WorkspaceCatalogEntry {
                    item: storage::workspace::links::WorkspaceItemRef {
                        kind: storage::workspace::links::WorkspaceItemKind::Note,
                        id: note.note_id,
                    },
                    title: note.title.clone(),
                    project_id: note.project_id,
                    project_name: note.project_name.clone(),
                    board_id: None,
                    board_title: None,
                    list_id: None,
                    list_title: None,
                },
            ],
            ..Default::default()
        };
        let typed_note = reference_completion_candidates(
            "Castle / Target",
            ReferenceCompletionMode::Link(Some(storage::workspace::links::WorkspaceItemKind::Note)),
            -1,
            Some(4),
            &[note],
            &note_catalog,
        );
        assert_eq!(typed_note.len(), 1);
        assert_eq!(typed_note[0].new_text, "[[note:Target note]]");
    }

    #[test]
    fn preview_builds_a_clickable_resolved_label() {
        let catalog = vec![storage::note::links::NoteLinkCatalogEntry {
            note_id: 7,
            title: "Roadmap".into(),
            project_id: Some(2),
            project_name: Some("Castle".into()),
        }];
        let mut block = WikiLinkPreviewBlock {
            text: String::new(),
            links: Vec::new(),
            heading_depth: None,
            source_offset: 0,
        };
        append_preview_text(
            "See [[Roadmap|the plan]].",
            &mut block,
            Some(2),
            &catalog,
            &storage::note::links::NoteLinkSet::default(),
            &storage::workspace::links::WorkspaceReferenceCatalog::default(),
        );

        assert_eq!(block.text, "See the plan.");
        assert_eq!(block.links[0].range, 4..12);
        assert!(matches!(
            block.links[0].target,
            Some(PreviewLinkTarget::Note(7))
        ));
    }
}
