use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};

use gpui::{
    Context, Entity, FontWeight, HighlightStyle, InteractiveText, IntoElement, ParentElement as _,
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

use crate::DB;

use super::{DocumentEditorView, DocumentInspectorTab};

#[derive(Clone)]
pub(super) struct WikiLinkCompletionProvider {
    state: Rc<RefCell<WikiLinkCompletionState>>,
}

struct WikiLinkCompletionState {
    note_id: i64,
    project_id: Option<i64>,
    enabled: bool,
    catalog: Arc<Vec<storage::note_links::NoteLinkCatalogEntry>>,
}

impl WikiLinkCompletionProvider {
    pub(super) fn new(note_id: i64) -> Self {
        Self {
            state: Rc::new(RefCell::new(WikiLinkCompletionState {
                note_id,
                project_id: None,
                enabled: false,
                catalog: Arc::new(Vec::new()),
            })),
        }
    }

    pub(super) fn update(
        &self,
        note_id: i64,
        project_id: Option<i64>,
        enabled: bool,
        catalog: Arc<Vec<storage::note_links::NoteLinkCatalogEntry>>,
    ) {
        *self.state.borrow_mut() = WikiLinkCompletionState {
            note_id,
            project_id,
            enabled,
            catalog,
        };
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
        let matches = wikilink_matches(
            query,
            state.note_id,
            state.project_id,
            state.catalog.as_ref(),
        );

        let replace_range = lsp_types::Range::new(
            text.offset_to_position(replace_range.start),
            text.offset_to_position(replace_range.end),
        );

        let items = matches
            .into_iter()
            .map(|candidate| CompletionItem {
                label: candidate.title.clone(),
                detail: candidate.project_name.clone(),
                kind: Some(CompletionItemKind::FILE),
                filter_text: Some(query.to_string()),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace_range,
                    new_text: wikilink_for_candidate(&candidate, state.catalog.as_ref()),
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
pub(super) struct WikiLinkPreviewPlugin {
    editor: Entity<DocumentEditorView>,
    project_id: Option<i64>,
    catalog: Arc<Vec<storage::note_links::NoteLinkCatalogEntry>>,
    indexed_links: Arc<storage::note_links::NoteLinkSet>,
}

impl WikiLinkPreviewPlugin {
    pub(super) fn new(
        editor: Entity<DocumentEditorView>,
        project_id: Option<i64>,
        catalog: Arc<Vec<storage::note_links::NoteLinkCatalogEntry>>,
        indexed_links: Arc<storage::note_links::NoteLinkSet>,
    ) -> Self {
        Self {
            editor,
            project_id,
            catalog,
            indexed_links,
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

        let editor = self.editor.clone();
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
                    editor.update(cx, |_, cx| {
                        cx.emit(super::DocumentEditorEvent::OpenNote {
                            note_id: *note_id,
                            source_offset: None,
                        });
                    });
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

impl DocumentEditorView {
    pub(super) fn load_note_links_async(
        note_id: u32,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    let links =
                        storage::note_links::load_note_links(db.as_ref(), note_id as i64).await?;
                    let catalog = storage::note_links::load_note_link_catalog(db.as_ref()).await?;
                    Ok::<_, anyhow::Error>((links, catalog))
                })
                .await;

            this.update(cx, |this, cx| {
                if this.note_id != note_id || this.note_links_generation != generation {
                    return;
                }
                this.note_links_loading = false;
                match result {
                    Ok(Ok((links, catalog))) => {
                        this.note_links = std::sync::Arc::new(links);
                        this.note_link_catalog = Arc::new(catalog);
                        this.wikilink_completion_provider.update(
                            this.note_id as i64,
                            this.project_id,
                            this.kind == super::DocumentKind::Markdown,
                            this.note_link_catalog.clone(),
                        );
                        this.note_links_error = None;
                    }
                    Ok(Err(error)) => this.note_links_error = Some(error.to_string().into()),
                    Err(error) => {
                        this.note_links_error = Some(format!("Link task failed: {error}").into())
                    }
                }
                cx.notify();
            })
            .ok();
        })
    }

    pub(super) fn refresh_note_links(&mut self, cx: &mut Context<Self>) {
        self.note_links_generation = self.note_links_generation.saturating_add(1);
        self.note_links_loading = true;
        self._note_links_task = Some(Self::load_note_links_async(
            self.note_id,
            self.note_links_generation,
            cx,
        ));
        cx.notify();
    }

    pub(super) fn show_outline_inspector(&mut self, cx: &mut Context<Self>) {
        self.inspector_tab = DocumentInspectorTab::Outline;
        cx.notify();
    }

    pub(super) fn show_links_inspector(&mut self, cx: &mut Context<Self>) {
        self.inspector_tab = DocumentInspectorTab::Links;
        self.refresh_note_links(cx);
    }
}

fn append_preview_node(
    node: &Node,
    block: &mut WikiLinkPreviewBlock,
    project_id: Option<i64>,
    catalog: &[storage::note_links::NoteLinkCatalogEntry],
    indexed_links: &storage::note_links::NoteLinkSet,
) {
    match node {
        Node::Text(text) => {
            append_preview_text(&text.value, block, project_id, catalog, indexed_links)
        }
        Node::InlineCode(code) => block.text.push_str(&code.value),
        Node::InlineMath(math) => block.text.push_str(&math.value),
        Node::Break(_) => block.text.push('\n'),
        Node::Image(image) => block.text.push_str(&image.alt),
        Node::Link(link) => {
            let start = block.text.len();
            for child in &link.children {
                append_preview_node(child, block, project_id, catalog, indexed_links);
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
                    append_preview_node(child, block, project_id, catalog, indexed_links);
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
) {
    let mut consumed = 0;
    for link in storage::note_links::parse_wikilinks(text) {
        block.text.push_str(&text[consumed..link.start_byte]);
        let start = block.text.len();
        block
            .text
            .push_str(link.display_text.as_deref().unwrap_or(&link.raw_target));
        let end = block.text.len();
        block.links.push(PreviewLink {
            range: start..end,
            target: resolve_preview_note(&link.raw_target, project_id, catalog, indexed_links)
                .and_then(|note_id| u32::try_from(note_id).ok())
                .map(PreviewLinkTarget::Note),
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
    format!("[[note:{}|{}]]", candidate.note_id, candidate.title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity::note;
    use gpui::{AppContext as _, EntityInputHandler as _};
    use gpui_component::input::{Enter, Position};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};
    use std::{path::PathBuf, sync::Arc};

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
        );

        assert_eq!(block.text, "See the plan.");
        assert_eq!(block.links[0].range, 4..12);
        assert!(matches!(
            block.links[0].target,
            Some(PreviewLinkTarget::Note(7))
        ));
    }

    #[gpui::test]
    fn editor_change_populates_wikilink_completion(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let (db, source_id, _target_id) = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                let source = note::ActiveModel {
                    title: Set("Source".into()),
                    cached_content: Set(String::new()),
                    created_at: Set(1),
                    updated_at: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                let target = note::ActiveModel {
                    title: Set("Target note".into()),
                    cached_content: Set(String::new()),
                    created_at: Set(1),
                    updated_at: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>((db, source.id as u32, target.id))
            })
            .expect("completion test database should initialize");
        let settings_dir =
            std::env::temp_dir().join(format!("castle-wikilink-completion-{}", std::process::id()));
        let mut editor_view = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(crate::app_settings::AppSettings::load(settings_dir));
            cx.set_global(crate::DB {
                conn: Arc::new(db),
                data_dir: PathBuf::new(),
            });
            cx.open_window(Default::default(), |window, cx| {
                let view = DocumentEditorView::view(source_id, window, cx);
                editor_view = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("completion test window should open")
        });
        let view = editor_view.expect("document editor should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
        for _ in 0..100 {
            cx.run_until_parked();
            if view.read_with(&cx, |editor, _| {
                !editor.is_loading && editor.note_link_catalog.len() == 2
            }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.editor.update(cx, |input, cx| {
                    input.set_value("[[Ta]]", window, cx);
                    input.set_cursor_position(Position::new(0, 4), window, cx);
                    input.replace_text_in_range(None, "r", window, cx);
                });
            });
        });
        cx.run_until_parked();

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.editor.update(cx, |input, cx| {
                    assert!(input.handle_action_for_context_menu(
                        Box::new(Enter {
                            secondary: false,
                            shift: false,
                        }),
                        window,
                        cx,
                    ));
                });
            });
        });
        cx.run_until_parked();
        view.read_with(&cx, |editor, cx| {
            assert_eq!(editor.editor.read(cx).value(), "[[Target note]]");
        });
    }
}
