use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};

use gpui::{
    Context, FontWeight, HighlightStyle, InteractiveText, IntoElement, ParentElement as _,
    Styled as _, StyledText, Task, UnderlineStyle, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _,
    input::{CompletionProvider, InputState, Rope, RopeExt as _},
    text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast::Node},
};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit, TextEdit,
};

use crate::{WorkspaceNavigationHandler, WorkspaceNavigationTarget};

#[derive(Clone)]
pub struct WikiLinkCompletionProvider {
    state: Rc<RefCell<WikiLinkCompletionState>>,
}

struct WikiLinkCompletionState {
    note_id: i64,
    project_id: Option<i64>,
    enabled: bool,
    catalog: Arc<Vec<storage::note_links::NoteLinkCatalogEntry>>,
    workspace_catalog: Arc<Vec<storage::workspace_links::WorkspaceCatalogEntry>>,
}

impl WikiLinkCompletionProvider {
    pub fn new(note_id: i64) -> Self {
        Self {
            state: Rc::new(RefCell::new(WikiLinkCompletionState {
                note_id,
                project_id: None,
                enabled: false,
                catalog: Arc::new(Vec::new()),
                workspace_catalog: Arc::new(Vec::new()),
            })),
        }
    }

    pub fn update(
        &self,
        note_id: i64,
        project_id: Option<i64>,
        enabled: bool,
        catalog: Arc<Vec<storage::note_links::NoteLinkCatalogEntry>>,
        workspace_catalog: Arc<Vec<storage::workspace_links::WorkspaceCatalogEntry>>,
    ) {
        *self.state.borrow_mut() = WikiLinkCompletionState {
            note_id,
            project_id,
            enabled,
            catalog,
            workspace_catalog,
        };
    }

    pub fn update_for_workspace_source(
        &self,
        project_id: Option<i64>,
        workspace_catalog: Arc<Vec<storage::workspace_links::WorkspaceCatalogEntry>>,
    ) {
        let note_catalog = workspace_catalog
            .iter()
            .filter(|entry| entry.item.kind == storage::workspace_links::WorkspaceItemKind::Note)
            .map(|entry| storage::note_links::NoteLinkCatalogEntry {
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

impl CompletionProvider for WikiLinkCompletionProvider {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _: lsp_types::CompletionContext,
        _: &mut Window,
        _: &mut Context<InputState>,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        let content = text.to_string();
        let Some((replace_range, query)) = wikilink_query_at_cursor(&content, offset) else {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        };

        let state = self.state.borrow();
        if !state.enabled {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        }
        let matches = wikilink_completion_candidates(
            query,
            state.note_id,
            state.project_id,
            state.catalog.as_ref(),
            state.workspace_catalog.as_ref(),
        );

        let replace_range = lsp_types::Range::new(
            text.offset_to_position(replace_range.start),
            text.offset_to_position(replace_range.end),
        );

        let items = matches
            .into_iter()
            .map(|candidate| CompletionItem {
                label: candidate.label,
                detail: candidate.detail,
                kind: Some(candidate.kind),
                filter_text: Some(query.to_string()),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace_range,
                    new_text: candidate.new_text,
                })),
                ..Default::default()
            })
            .collect();

        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _: usize, _: &str, _: &mut Context<InputState>) -> bool {
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
    catalog: Arc<Vec<storage::note_links::NoteLinkCatalogEntry>>,
    indexed_links: Arc<storage::note_links::NoteLinkSet>,
    workspace_catalog: Arc<Vec<storage::workspace_links::WorkspaceCatalogEntry>>,
}

impl WikiLinkPreviewPlugin {
    pub fn new(
        open_target: WorkspaceNavigationHandler,
        project_id: Option<i64>,
        catalog: Arc<Vec<storage::note_links::NoteLinkCatalogEntry>>,
        indexed_links: Arc<storage::note_links::NoteLinkSet>,
        workspace_catalog: Arc<Vec<storage::workspace_links::WorkspaceCatalogEntry>>,
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
        workspace_catalog: Arc<Vec<storage::workspace_links::WorkspaceCatalogEntry>>,
    ) -> Self {
        let catalog = workspace_catalog
            .iter()
            .filter(|entry| entry.item.kind == storage::workspace_links::WorkspaceItemKind::Note)
            .map(|entry| storage::note_links::NoteLinkCatalogEntry {
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
            indexed_links: Arc::new(storage::note_links::NoteLinkSet::default()),
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
    catalog: &[storage::note_links::NoteLinkCatalogEntry],
    indexed_links: &storage::note_links::NoteLinkSet,
    workspace_catalog: &[storage::workspace_links::WorkspaceCatalogEntry],
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
    catalog: &[storage::note_links::NoteLinkCatalogEntry],
    indexed_links: &storage::note_links::NoteLinkSet,
    workspace_catalog: &[storage::workspace_links::WorkspaceCatalogEntry],
) {
    let mut consumed = 0;
    for link in storage::note_links::parse_wikilinks(text) {
        block.text.push_str(&text[consumed..link.start_byte]);
        let start = block.text.len();
        block
            .text
            .push_str(link.display_text.as_deref().unwrap_or(&link.raw_target));
        let end = block.text.len();
        let target =
            storage::workspace_links::resolve_stable_target(&link.raw_target, workspace_catalog)
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
    catalog: &[storage::note_links::NoteLinkCatalogEntry],
    indexed_links: &storage::note_links::NoteLinkSet,
) -> Option<i64> {
    if let Some(target) = indexed_links.outbound.iter().find(|link| {
        link.raw_target.eq_ignore_ascii_case(raw_target) && link.target_note_id.is_some()
    }) {
        return target.target_note_id;
    }
    if let Some(note_id) = raw_target
        .strip_prefix("note:")
        .and_then(|value| value.parse::<i64>().ok())
        && catalog.iter().any(|note| note.note_id == note_id)
    {
        return Some(note_id);
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
    mut candidates: impl Iterator<Item = &'a storage::note_links::NoteLinkCatalogEntry>,
) -> Option<i64> {
    let first = candidates.next()?.note_id;
    candidates.next().is_none().then_some(first)
}

pub fn workspace_navigation_target(
    entry: &storage::workspace_links::WorkspaceCatalogEntry,
) -> Option<WorkspaceNavigationTarget> {
    let item_id = u32::try_from(entry.item.id).ok()?;
    match entry.item.kind {
        storage::workspace_links::WorkspaceItemKind::Note => {
            Some(WorkspaceNavigationTarget::Note {
                note_id: item_id,
                source_offset: None,
            })
        }
        storage::workspace_links::WorkspaceItemKind::Board => {
            Some(WorkspaceNavigationTarget::board(item_id))
        }
        storage::workspace_links::WorkspaceItemKind::List => Some(WorkspaceNavigationTarget::list(
            u32::try_from(entry.board_id?).ok()?,
            item_id,
        )),
        storage::workspace_links::WorkspaceItemKind::Card => Some(WorkspaceNavigationTarget::card(
            u32::try_from(entry.board_id?).ok()?,
            item_id,
        )),
    }
}

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

struct WikiLinkCompletionCandidate {
    label: String,
    detail: Option<String>,
    kind: CompletionItemKind,
    new_text: String,
}

fn wikilink_completion_candidates(
    query: &str,
    note_id: i64,
    project_id: Option<i64>,
    note_catalog: &[storage::note_links::NoteLinkCatalogEntry],
    workspace_catalog: &[storage::workspace_links::WorkspaceCatalogEntry],
) -> Vec<WikiLinkCompletionCandidate> {
    let (selected_kind, query) = query
        .split_once(':')
        .and_then(|(prefix, query)| {
            let kind = match prefix.trim().to_ascii_lowercase().as_str() {
                "note" => storage::workspace_links::WorkspaceItemKind::Note,
                "board" => storage::workspace_links::WorkspaceItemKind::Board,
                "list" => storage::workspace_links::WorkspaceItemKind::List,
                "card" => storage::workspace_links::WorkspaceItemKind::Card,
                _ => return None,
            };
            Some((Some(kind), query.trim()))
        })
        .unwrap_or((None, query));

    let mut candidates = Vec::new();
    if selected_kind.is_none()
        || selected_kind == Some(storage::workspace_links::WorkspaceItemKind::Note)
    {
        candidates.extend(
            wikilink_matches(query, note_id, project_id, note_catalog)
                .into_iter()
                .map(|note| WikiLinkCompletionCandidate {
                    label: note.title.clone(),
                    detail: note.project_name.clone(),
                    kind: CompletionItemKind::FILE,
                    new_text: wikilink_for_candidate(&note, note_catalog),
                }),
        );
    }

    if selected_kind != Some(storage::workspace_links::WorkspaceItemKind::Note) {
        let normalized_query = query.to_lowercase();
        let mut workspace_matches = workspace_catalog
            .iter()
            .filter(|entry| entry.item.kind != storage::workspace_links::WorkspaceItemKind::Note)
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
                storage::workspace_links::WorkspaceItemKind::Board => CompletionItemKind::MODULE,
                storage::workspace_links::WorkspaceItemKind::List => CompletionItemKind::FOLDER,
                storage::workspace_links::WorkspaceItemKind::Card => CompletionItemKind::VALUE,
                storage::workspace_links::WorkspaceItemKind::Note => CompletionItemKind::FILE,
            };
            WikiLinkCompletionCandidate {
                label: entry.title.clone(),
                detail: Some(entry.breadcrumb()),
                kind,
                new_text: entry.stable_link(),
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
    catalog: &[storage::note_links::NoteLinkCatalogEntry],
) -> Vec<storage::note_links::NoteLinkCatalogEntry> {
    let query = query.to_lowercase();
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
            format!("{project}/{title}").contains(&query).then_some((
                !title.starts_with(&query),
                candidate.project_id != project_id,
                title,
                candidate.clone(),
            ))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        (&left.0, &left.1, &left.2, left.3.note_id).cmp(&(
            &right.0,
            &right.1,
            &right.2,
            right.3.note_id,
        ))
    });
    matches
        .into_iter()
        .take(12)
        .map(|(_, _, _, candidate)| candidate)
        .collect()
}

fn wikilink_for_candidate(
    candidate: &storage::note_links::NoteLinkCatalogEntry,
    catalog: &[storage::note_links::NoteLinkCatalogEntry],
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
    storage::workspace_links::stable_workspace_link(
        storage::workspace_links::WorkspaceItemRef {
            kind: storage::workspace_links::WorkspaceItemKind::Note,
            id: candidate.note_id,
        },
        candidate.title.as_ref(),
    )
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
    fn preview_builds_a_clickable_resolved_label() {
        let catalog = vec![storage::note_links::NoteLinkCatalogEntry {
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
            &storage::note_links::NoteLinkSet::default(),
            &[],
        );

        assert_eq!(block.text, "See the plan.");
        assert_eq!(block.links[0].range, 4..12);
        assert!(matches!(
            block.links[0].target,
            Some(PreviewLinkTarget::Note(7))
        ));
    }
}
