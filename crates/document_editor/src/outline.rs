use gpui_kit::SharedString;
use std::{collections::HashSet, sync::Arc};

const JSON_OUTLINE_NODE_LIMIT: usize = 10_000;
const JSON_VALUE_PREVIEW_LIMIT: usize = 80;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutlineRow {
    pub(crate) node_index: Option<usize>,
    pub(crate) title: String,
    pub(crate) depth: usize,
    pub(crate) source_offset: usize,
    pub(crate) source_line: usize,
    pub(crate) preview_section_index: Option<usize>,
    pub(crate) has_children: bool,
    pub(crate) expanded: bool,
    pub(crate) disabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MarkdownOutline {
    items: Vec<OutlineRow>,
    pub(crate) sections: Vec<SharedString>,
    pub(crate) section_offsets: Vec<usize>,
}

struct MarkdownHeading {
    level: u8,
    title: String,
    source_line: usize,
    source_column: usize,
    source_line_offset: usize,
    source_byte_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct JsonOutlineNode {
    id: String,
    title: String,
    source_offset: usize,
    parent: Option<usize>,
    children: Vec<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct JsonOutline {
    nodes: Vec<JsonOutlineNode>,
    roots: Vec<usize>,
    expanded: HashSet<usize>,
    pub(crate) has_error: bool,
    pub(crate) truncated: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum DocumentOutline {
    #[default]
    None,
    Markdown(MarkdownOutline),
    Json(JsonOutline),
}

impl DocumentOutline {
    pub(crate) fn rows(&self) -> Vec<OutlineRow> {
        match self {
            Self::None => Vec::new(),
            Self::Markdown(outline) => outline.items.clone(),
            Self::Json(outline) => outline.rows(),
        }
    }

    pub(crate) fn markdown_sections(&self) -> &[SharedString] {
        match self {
            Self::Markdown(outline) => &outline.sections,
            Self::None | Self::Json(_) => &[],
        }
    }

    pub(crate) fn markdown_section_offsets(&self) -> &[usize] {
        match self {
            Self::Markdown(outline) => &outline.section_offsets,
            Self::None | Self::Json(_) => &[],
        }
    }

    pub(crate) fn json_has_error(&self) -> bool {
        matches!(self, Self::Json(outline) if outline.has_error)
    }

    pub(crate) fn active_markdown_index_for_line(&self, line: usize) -> Option<usize> {
        match self {
            Self::Markdown(outline) => outline.active_index_for_line(line),
            Self::None | Self::Json(_) => None,
        }
    }

    pub(crate) fn expand(&mut self, node_index: usize) -> bool {
        match self {
            Self::Json(outline) => outline.expand(node_index),
            Self::None | Self::Markdown(_) => false,
        }
    }

    pub(crate) fn collapse(&mut self, node_index: usize) -> bool {
        match self {
            Self::Json(outline) => outline.collapse(node_index),
            Self::None | Self::Markdown(_) => false,
        }
    }

    pub(crate) fn expand_all(&mut self) -> bool {
        match self {
            Self::Json(outline) => outline.expand_all(),
            Self::None | Self::Markdown(_) => false,
        }
    }

    pub(crate) fn collapse_all(&mut self) -> bool {
        match self {
            Self::Json(outline) => outline.collapse_all(),
            Self::None | Self::Markdown(_) => false,
        }
    }

    pub(crate) fn can_expand_all(&self) -> bool {
        matches!(self, Self::Json(outline) if outline.can_expand_all())
    }

    pub(crate) fn can_collapse_all(&self) -> bool {
        matches!(self, Self::Json(outline) if !outline.expanded.is_empty())
    }

    pub(crate) fn root_node_index(&self, node_index: usize) -> Option<usize> {
        match self {
            Self::Json(outline) => outline.root_node_index(node_index),
            Self::None | Self::Markdown(_) => None,
        }
    }

    pub(crate) fn parent_row_index(&self, node_index: usize) -> Option<usize> {
        match self {
            Self::Json(outline) => outline.parent_row_index(node_index),
            Self::None | Self::Markdown(_) => None,
        }
    }

    pub(crate) fn first_child_row_index(&self, node_index: usize) -> Option<usize> {
        match self {
            Self::Json(outline) => outline.first_child_row_index(node_index),
            Self::None | Self::Markdown(_) => None,
        }
    }

    pub(crate) fn preserve_json_expansion_from(&mut self, previous: &Self) {
        let (Self::Json(current), Self::Json(previous)) = (self, previous) else {
            return;
        };
        current.preserve_expansion_from(previous);
    }
}

impl MarkdownOutline {
    pub(crate) fn parse(source: &str) -> Self {
        let mut headings = Vec::<MarkdownHeading>::new();
        let mut in_fence = false;
        let mut fence_marker = None::<char>;
        let mut previous_line = None::<(&str, usize, usize, usize)>;
        let mut line_offset = 0usize;

        for (line_index, line) in source.lines().enumerate() {
            let source_byte_offset = line.as_ptr() as usize - source.as_ptr() as usize;
            let trimmed = line.trim_start();
            let marker = trimmed.chars().next();
            if matches!(marker, Some('`' | '~'))
                && trimmed.chars().take_while(|ch| Some(*ch) == marker).count() >= 3
            {
                if !in_fence {
                    in_fence = true;
                    fence_marker = marker;
                } else if marker == fence_marker {
                    in_fence = false;
                    fence_marker = None;
                }
            } else if !in_fence {
                let hash_count = trimmed.chars().take_while(|ch| *ch == '#').count();
                if (1..=6).contains(&hash_count)
                    && trimmed
                        .chars()
                        .nth(hash_count)
                        .is_some_and(char::is_whitespace)
                {
                    let title = clean_heading(&trimmed[hash_count..]);
                    if !title.is_empty() {
                        headings.push(MarkdownHeading {
                            level: hash_count as u8,
                            title,
                            source_line: line_index,
                            source_column: line.len().saturating_sub(trimmed.len()),
                            source_line_offset: line_offset,
                            source_byte_offset,
                        });
                    }
                } else if !trimmed.is_empty() {
                    let underline = trimmed.trim();
                    let level = if underline.len() >= 2 && underline.chars().all(|ch| ch == '=') {
                        Some(1)
                    } else if underline.len() >= 2 && underline.chars().all(|ch| ch == '-') {
                        Some(2)
                    } else {
                        None
                    };
                    if let (Some(level), Some((previous, line, offset, byte_offset))) =
                        (level, previous_line)
                    {
                        let previous = previous.trim();
                        if !previous.is_empty() {
                            headings.push(MarkdownHeading {
                                level,
                                title: clean_heading(previous),
                                source_line: line,
                                source_column: 0,
                                source_line_offset: offset,
                                source_byte_offset: byte_offset,
                            });
                        }
                    }
                }
            }

            previous_line = Some((line, line_index, line_offset, source_byte_offset));
            line_offset = line_offset.saturating_add(line.len()).saturating_add(1);
        }

        headings.sort_by_key(|heading| heading.source_line);
        headings.dedup_by_key(|heading| heading.source_line);

        let mut sections = Vec::with_capacity(headings.len().saturating_add(1));
        let mut section_offsets = Vec::with_capacity(headings.len().saturating_add(1));
        if let Some(first) = headings.first() {
            if first.source_line > 0 {
                sections.push(markdown_section(source, 0, first.source_byte_offset));
                section_offsets.push(0);
            }
        } else if !source.is_empty() {
            sections.push(SharedString::from(Arc::<str>::from(source)));
            section_offsets.push(0);
        }

        let section_offset = usize::from(!sections.is_empty());
        let items = headings
            .iter()
            .enumerate()
            .map(|(index, heading)| {
                let end = headings
                    .get(index + 1)
                    .map(|next| next.source_byte_offset)
                    .unwrap_or(source.len());
                sections.push(markdown_section(source, heading.source_byte_offset, end));
                section_offsets.push(heading.source_line_offset);
                OutlineRow {
                    node_index: Some(index),
                    title: heading.title.clone(),
                    depth: usize::from(heading.level.saturating_sub(1)),
                    source_offset: heading
                        .source_line_offset
                        .saturating_add(heading.source_column),
                    source_line: heading.source_line,
                    preview_section_index: Some(index + section_offset),
                    has_children: false,
                    expanded: false,
                    disabled: false,
                }
            })
            .collect();

        Self {
            items,
            sections,
            section_offsets,
        }
    }

    fn active_index_for_line(&self, line: usize) -> Option<usize> {
        self.items
            .partition_point(|item| item.source_line <= line)
            .checked_sub(1)
    }

    #[cfg(test)]
    fn active_index_for_line_with_comparisons(&self, line: usize) -> (Option<usize>, usize) {
        let mut comparisons = 0;
        let insertion_index = self.items.partition_point(|item| {
            comparisons += 1;
            item.source_line <= line
        });
        (insertion_index.checked_sub(1), comparisons)
    }
}

fn markdown_section(source: &str, start: usize, end: usize) -> SharedString {
    let source = &source[start..end];
    if source.contains('\r') {
        let mut section = String::with_capacity(source.len());
        for (index, line) in source.lines().enumerate() {
            if index > 0 {
                section.push('\n');
            }
            section.push_str(line);
        }
        return SharedString::from(section);
    }

    SharedString::from(Arc::<str>::from(
        source.strip_suffix('\n').unwrap_or(source),
    ))
}

impl JsonOutline {
    pub(crate) fn parse(source: &str) -> Self {
        let mut parser = tree_sitter::Parser::new();
        let language: tree_sitter::Language = tree_sitter_json::LANGUAGE.into();
        if parser.set_language(&language).is_err() {
            return Self {
                has_error: true,
                ..Self::default()
            };
        }
        let Some(tree) = parser.parse(source, None) else {
            return Self {
                has_error: true,
                ..Self::default()
            };
        };

        let root = tree.root_node();
        let has_error = root.has_error();
        let mut outline = Self {
            has_error,
            ..Self::default()
        };
        let Some(value) = root.named_child(0) else {
            return outline;
        };

        let mut pending = Vec::new();
        push_json_children(&mut pending, value, None, "$".to_string(), source);
        if pending.is_empty() {
            pending.push(PendingJsonNode {
                node: value,
                parent: None,
                prefix: "$".to_string(),
                path: "$".to_string(),
                source_offset: value.start_byte(),
            });
        }

        while let Some(pending_node) = pending.pop() {
            if outline.nodes.len() >= JSON_OUTLINE_NODE_LIMIT {
                outline.truncated = true;
                break;
            }

            let node_index = outline.nodes.len();
            let is_container = matches!(pending_node.node.kind(), "object" | "array");
            let child_count = json_child_count(pending_node.node);
            let title = if is_container {
                match pending_node.node.kind() {
                    "object" => format!("{}  ·  {{{child_count}}}", pending_node.prefix),
                    "array" => format!("{}  ·  [{child_count}]", pending_node.prefix),
                    _ => pending_node.prefix.clone(),
                }
            } else {
                let raw = pending_node
                    .node
                    .utf8_text(source.as_bytes())
                    .unwrap_or(pending_node.node.kind());
                format!(
                    "{}: {}",
                    pending_node.prefix,
                    truncate_preview(raw, JSON_VALUE_PREVIEW_LIMIT)
                )
            };

            outline.nodes.push(JsonOutlineNode {
                id: pending_node.path.clone(),
                title,
                source_offset: pending_node.source_offset,
                parent: pending_node.parent,
                children: Vec::new(),
            });
            if let Some(parent) = pending_node.parent {
                if let Some(parent_node) = outline.nodes.get_mut(parent) {
                    parent_node.children.push(node_index);
                }
            } else {
                outline.roots.push(node_index);
            }

            if is_container {
                push_json_children(
                    &mut pending,
                    pending_node.node,
                    Some(node_index),
                    pending_node.path,
                    source,
                );
            }
        }

        outline
    }

    fn rows(&self) -> Vec<OutlineRow> {
        let mut rows = Vec::with_capacity(self.nodes.len().min(JSON_OUTLINE_NODE_LIMIT));
        let mut pending = self
            .roots
            .iter()
            .rev()
            .map(|index| (*index, 0usize))
            .collect::<Vec<_>>();

        while let Some((node_index, depth)) = pending.pop() {
            let Some(node) = self.nodes.get(node_index) else {
                continue;
            };
            let expanded = self.expanded.contains(&node_index);
            rows.push(OutlineRow {
                node_index: Some(node_index),
                title: node.title.clone(),
                depth,
                source_offset: node.source_offset,
                source_line: 0,
                preview_section_index: None,
                has_children: !node.children.is_empty(),
                expanded,
                disabled: false,
            });
            if expanded {
                pending.extend(
                    node.children
                        .iter()
                        .rev()
                        .map(|child| (*child, depth.saturating_add(1))),
                );
            }
        }

        if self.truncated {
            rows.push(OutlineRow {
                node_index: None,
                title: format!("Outline limited to {JSON_OUTLINE_NODE_LIMIT} items"),
                depth: 0,
                source_offset: 0,
                source_line: 0,
                preview_section_index: None,
                has_children: false,
                expanded: false,
                disabled: true,
            });
        }
        rows
    }

    fn expand(&mut self, node_index: usize) -> bool {
        self.nodes
            .get(node_index)
            .is_some_and(|node| !node.children.is_empty())
            && self.expanded.insert(node_index)
    }

    fn collapse(&mut self, node_index: usize) -> bool {
        self.expanded.remove(&node_index)
    }

    fn expand_all(&mut self) -> bool {
        let expanded = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| (!node.children.is_empty()).then_some(index))
            .collect::<HashSet<_>>();
        if expanded == self.expanded {
            return false;
        }
        self.expanded = expanded;
        true
    }

    fn collapse_all(&mut self) -> bool {
        if self.expanded.is_empty() {
            return false;
        }
        self.expanded.clear();
        true
    }

    fn can_expand_all(&self) -> bool {
        self.nodes
            .iter()
            .enumerate()
            .any(|(index, node)| !node.children.is_empty() && !self.expanded.contains(&index))
    }

    fn root_node_index(&self, node_index: usize) -> Option<usize> {
        let mut current = node_index;
        loop {
            let node = self.nodes.get(current)?;
            let Some(parent) = node.parent else {
                return Some(current);
            };
            current = parent;
        }
    }

    fn parent_row_index(&self, node_index: usize) -> Option<usize> {
        let parent = self.nodes.get(node_index)?.parent?;
        self.rows()
            .iter()
            .position(|row| row.node_index == Some(parent))
    }

    fn first_child_row_index(&self, node_index: usize) -> Option<usize> {
        let child = *self.nodes.get(node_index)?.children.first()?;
        self.rows()
            .iter()
            .position(|row| row.node_index == Some(child))
    }

    fn preserve_expansion_from(&mut self, previous: &Self) {
        let expanded_ids = previous
            .expanded
            .iter()
            .filter_map(|index| previous.nodes.get(*index))
            .map(|node| node.id.as_str())
            .collect::<HashSet<_>>();
        self.expanded = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| expanded_ids.contains(node.id.as_str()).then_some(index))
            .collect();
    }
}

#[derive(Clone)]
struct PendingJsonNode<'a> {
    node: tree_sitter::Node<'a>,
    parent: Option<usize>,
    source_offset: usize,
    prefix: String,
    path: String,
}

fn push_json_children<'a>(
    pending: &mut Vec<PendingJsonNode<'a>>,
    node: tree_sitter::Node<'a>,
    parent: Option<usize>,
    parent_path: String,
    source: &str,
) {
    match node.kind() {
        "object" => {
            let mut cursor = node.walk();
            let pairs = node
                .named_children(&mut cursor)
                .filter(|child| child.kind() == "pair")
                .collect::<Vec<_>>();
            for (pair_index, pair) in pairs.into_iter().enumerate().rev() {
                let key = pair.child_by_field_name("key");
                let value = pair.child_by_field_name("value");
                let key_text = key
                    .and_then(|key| key.utf8_text(source.as_bytes()).ok())
                    .map(decode_json_key)
                    .unwrap_or_else(|| "<key>".to_string());
                let value = value.unwrap_or(pair);
                pending.push(PendingJsonNode {
                    node: value,
                    parent,
                    source_offset: key.map_or(pair.start_byte(), |key| key.start_byte()),
                    prefix: key_text.clone(),
                    path: format!("{parent_path}.{pair_index}:{key_text}"),
                });
            }
        }
        "array" => {
            let mut cursor = node.walk();
            let values = node.named_children(&mut cursor).collect::<Vec<_>>();
            for (index, value) in values.into_iter().enumerate().rev() {
                pending.push(PendingJsonNode {
                    node: value,
                    parent,
                    source_offset: value.start_byte(),
                    prefix: format!("[{index}]"),
                    path: format!("{parent_path}[{index}]"),
                });
            }
        }
        _ => {}
    }
}

fn json_child_count(node: tree_sitter::Node<'_>) -> usize {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).count()
}

fn decode_json_key(raw: &str) -> String {
    serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.trim_matches('"').to_string())
}

fn truncate_preview(value: &str, limit: usize) -> String {
    let mut preview = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        preview.push('…');
    }
    preview
}

fn clean_heading(value: &str) -> String {
    value
        .trim()
        .trim_end_matches('#')
        .trim()
        .trim_matches(|ch| matches!(ch, '*' | '_' | '`'))
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        DocumentOutline, JSON_OUTLINE_NODE_LIMIT, JsonOutline, MarkdownOutline, truncate_preview,
    };
    use test_support as test_alloc;

    #[test]
    fn preview_section_snapshots_share_markdown_content() {
        const SECTION_BYTES: usize = 128 * 1024;
        const SNAPSHOTS: usize = 8;

        let body = "x".repeat(SECTION_BYTES);
        let source = format!("# One\n{body}\n# Two\n{body}\n# Three\n{body}\n# Four\n{body}");
        let outline = MarkdownOutline::parse(&source);
        let legacy_sections = outline
            .sections
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let legacy_allocation = test_alloc::start_measurement();
        let legacy_started = std::time::Instant::now();
        for _ in 0..SNAPSHOTS {
            std::hint::black_box(legacy_sections.clone());
        }
        let legacy_elapsed = legacy_started.elapsed();
        let legacy_allocation = legacy_allocation.finish();

        let allocation = test_alloc::start_measurement();
        let shared_started = std::time::Instant::now();
        for _ in 0..SNAPSHOTS {
            std::hint::black_box(outline.sections.clone());
        }
        let shared_elapsed = shared_started.elapsed();
        let allocation = allocation.finish();

        assert!(
            allocation.allocated_bytes < legacy_allocation.allocated_bytes / 100,
            "shared preview snapshots allocated {} bytes versus {} bytes for owned strings",
            allocation.allocated_bytes,
            legacy_allocation.allocated_bytes
        );
        println!(
            "markdown_bytes={} snapshots={SNAPSHOTS} owned_string_clone_micros={} owned_string_allocated_bytes={} shared_clone_micros={} shared_peak_heap_growth_bytes={} shared_retained_heap_growth_bytes={} shared_allocated_bytes={}",
            source.len(),
            legacy_elapsed.as_micros(),
            legacy_allocation.allocated_bytes,
            shared_elapsed.as_micros(),
            allocation.peak_growth_bytes,
            allocation.retained_growth_bytes,
            allocation.allocated_bytes
        );
    }

    #[test]
    fn parses_markdown_headings_and_preview_sections() {
        let outline = MarkdownOutline::parse("Intro\n\n# One\nBody\n## Two\nMore");
        assert_eq!(outline.items.len(), 2);
        assert_eq!(outline.sections.len(), 3);
        assert_eq!(outline.section_offsets, vec![0, 7, 18]);
        assert_eq!(outline.items[1].preview_section_index, Some(2));
        assert_eq!(outline.items[1].depth, 1);
    }

    #[test]
    fn active_markdown_index_tracks_the_latest_heading_at_each_boundary() {
        let outline =
            MarkdownOutline::parse("Preamble\n# One\nBody\n## Two\nMore\n### Three\nTail");

        assert_eq!(outline.active_index_for_line(0), None);
        assert_eq!(outline.active_index_for_line(1), Some(0));
        assert_eq!(outline.active_index_for_line(3), Some(1));
        assert_eq!(outline.active_index_for_line(6), Some(2));
        assert_eq!(outline.active_index_for_line(100), Some(2));
    }

    #[test]
    fn active_markdown_index_large_heading_fixture_proves_logarithmic_work() {
        const HEADING_COUNT: usize = 16_384;
        const LOOKUPS: usize = 64;
        const LOOKUP_LINE: usize = 1;
        const BINARY_SEARCH_COMPARISONS: usize = 15;

        let source = (0..HEADING_COUNT)
            .map(|index| format!("# Heading {index}\nBody\n"))
            .collect::<String>();
        let outline = MarkdownOutline::parse(&source);
        let mut baseline_comparisons = 0;
        let mut optimized_comparisons = 0;
        for _ in 0..LOOKUPS {
            let baseline_index = outline
                .items
                .iter()
                .enumerate()
                .rev()
                .find(|(_, item)| {
                    baseline_comparisons += 1;
                    item.source_line <= LOOKUP_LINE
                })
                .map(|(index, _)| index);
            let (optimized_index, comparisons) =
                outline.active_index_for_line_with_comparisons(LOOKUP_LINE);
            optimized_comparisons += comparisons;

            assert_eq!(optimized_index, baseline_index);
        }

        assert_eq!(outline.active_index_for_line(LOOKUP_LINE), Some(0));
        assert_eq!(
            baseline_comparisons,
            HEADING_COUNT * LOOKUPS,
            "reverse scan should inspect every heading for the first-heading lookup"
        );
        assert_eq!(
            optimized_comparisons,
            BINARY_SEARCH_COMPARISONS * LOOKUPS,
            "upper-bound lookup should use the fixed binary-search work for this fixture"
        );
        println!(
            "markdown_headings={HEADING_COUNT} active_index_lookups={LOOKUPS} reverse_scan_comparisons={baseline_comparisons} binary_search_comparisons={optimized_comparisons}",
        );
    }

    #[test]
    fn parses_atx_and_setext_headings_with_exact_sections() {
        let outline =
            MarkdownOutline::parse("# One ###\nBody\nTwo\n===\nTail\n### _Three_ ###\nMore");

        assert_eq!(
            outline
                .items
                .iter()
                .map(|item| (
                    item.title.as_str(),
                    item.depth,
                    item.source_line,
                    item.source_offset,
                    item.preview_section_index,
                ))
                .collect::<Vec<_>>(),
            vec![
                ("One", 0, 0, 0, Some(0)),
                ("Two", 0, 2, 15, Some(1)),
                ("Three", 2, 5, 28, Some(2)),
            ]
        );
        assert_eq!(
            outline
                .sections
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["# One ###\nBody", "Two\n===\nTail", "### _Three_ ###\nMore",]
        );
        assert_eq!(outline.section_offsets, vec![0, 15, 28]);
    }

    #[test]
    fn ignores_atx_and_setext_headings_inside_matching_fences() {
        let source = "~~~markdown\n# Hidden\n~~~\n# Visible\n```md\nSetext hidden\n---\n```\nOutside\n=======";
        let outline = MarkdownOutline::parse(source);

        assert_eq!(
            outline
                .items
                .iter()
                .map(|item| (item.title.as_str(), item.source_line, item.source_offset))
                .collect::<Vec<_>>(),
            vec![
                (
                    "Visible",
                    3,
                    source.find("# Visible").expect("heading should exist")
                ),
                (
                    "Outside",
                    8,
                    source.find("Outside").expect("heading should exist")
                ),
            ]
        );
        assert_eq!(
            outline
                .sections
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec![
                "~~~markdown\n# Hidden\n~~~",
                "# Visible\n```md\nSetext hidden\n---\n```",
                "Outside\n=======",
            ]
        );
        assert_eq!(
            outline.section_offsets,
            vec![
                0,
                source.find("# Visible").expect("heading should exist"),
                source.find("Outside").expect("heading should exist")
            ]
        );
    }

    #[test]
    fn markdown_offsets_count_utf8_bytes() {
        let source = "Préface\n  ## Café\nBody\nRésumé\n-------";
        let outline = MarkdownOutline::parse(source);

        assert_eq!(
            outline
                .items
                .iter()
                .map(|item| (item.title.as_str(), item.source_line, item.source_offset))
                .collect::<Vec<_>>(),
            vec![
                (
                    "Café",
                    1,
                    source.find("## Café").expect("heading should exist")
                ),
                (
                    "Résumé",
                    3,
                    source.find("Résumé").expect("heading should exist")
                ),
            ]
        );
        assert_eq!(
            outline.section_offsets,
            vec![
                0,
                source.find("  ## Café").expect("section should exist"),
                source.find("Résumé").expect("section should exist"),
            ]
        );
    }

    #[test]
    fn handles_empty_input_preamble_and_trailing_newlines() {
        let empty = MarkdownOutline::parse("");
        assert!(empty.items.is_empty());
        assert!(empty.sections.is_empty());
        assert!(empty.section_offsets.is_empty());

        let plain = MarkdownOutline::parse("Plain text\n");
        assert!(plain.items.is_empty());
        assert_eq!(plain.sections[0].as_ref(), "Plain text\n");
        assert_eq!(plain.section_offsets, vec![0]);

        let outline = MarkdownOutline::parse("Intro\n\n# One\nBody\n");
        assert_eq!(
            outline
                .sections
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["Intro\n", "# One\nBody"]
        );
        assert_eq!(outline.section_offsets, vec![0, 7]);
    }

    #[test]
    fn normalizes_crlf_heading_sections_and_offsets_like_lf() {
        let outline = MarkdownOutline::parse("Intro\r\n# One\r\nBody\r\n## Two\r\nLast\r\n");

        assert_eq!(
            outline
                .items
                .iter()
                .map(|item| (item.title.as_str(), item.source_line, item.source_offset))
                .collect::<Vec<_>>(),
            vec![("One", 1, 6), ("Two", 3, 17)]
        );
        assert_eq!(
            outline
                .sections
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["Intro", "# One\nBody", "## Two\nLast"]
        );
        assert_eq!(outline.section_offsets, vec![0, 6, 17]);
    }

    #[test]
    fn parses_many_markdown_lines_with_bounded_allocations() {
        const BODY_LINES: usize = 50_000;
        const PARSER_OVERHEAD_BUDGET: usize = 64 * 1024;

        let source = format!("# Root\n{}", "body line\n".repeat(BODY_LINES));
        let measurement = test_alloc::start_measurement();
        let started = std::time::Instant::now();
        let outline = MarkdownOutline::parse(&source);
        let elapsed = started.elapsed();
        let allocation = measurement.finish();
        std::hint::black_box(&outline);

        assert_eq!(outline.items.len(), 1);
        assert_eq!(outline.sections.len(), 1);
        assert_eq!(outline.sections[0].len(), source.len() - 1);
        assert!(
            allocation.allocated_bytes < source.len() + PARSER_OVERHEAD_BUDGET,
            "many-line outline allocated {} bytes for {} source bytes with a {} byte overhead budget",
            allocation.allocated_bytes,
            source.len(),
            PARSER_OVERHEAD_BUDGET
        );
        println!(
            "markdown_bytes={} body_lines={BODY_LINES} elapsed_micros={} allocated_bytes={} peak_heap_growth_bytes={} retained_heap_growth_bytes={}",
            source.len(),
            elapsed.as_micros(),
            allocation.allocated_bytes,
            allocation.peak_growth_bytes,
            allocation.retained_growth_bytes
        );
    }

    #[test]
    fn ignores_markdown_headings_inside_fences() {
        let outline = MarkdownOutline::parse("```md\n# Hidden\n```\n# Visible\n");
        assert_eq!(outline.items.len(), 1);
        assert_eq!(outline.items[0].title, "Visible");
    }

    #[test]
    fn builds_nested_json_rows_with_scalar_previews() {
        let mut outline = DocumentOutline::Json(JsonOutline::parse(
            r#"{"name":"Castle","items":[{"done":true},2]}"#,
        ));
        let rows = outline.rows();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].title.contains("name: \"Castle\""));
        assert!(rows[1].title.contains("items"));

        assert!(outline.expand(rows[1].node_index.expect("array node should exist")));
        let expanded = outline.rows();
        assert_eq!(expanded.len(), 4);
        assert!(expanded[2].title.starts_with("[0]"));
    }

    #[test]
    fn expands_and_collapses_all_json_containers() {
        let mut outline = DocumentOutline::Json(JsonOutline::parse(
            r#"{"items":[{"nested":[1,2]}],"meta":{"ready":true}}"#,
        ));

        assert!(outline.can_expand_all());
        assert!(!outline.can_collapse_all());
        assert!(outline.expand_all());
        assert_eq!(outline.rows().len(), 7);
        assert!(!outline.can_expand_all());
        assert!(outline.can_collapse_all());

        assert!(outline.collapse_all());
        assert_eq!(outline.rows().len(), 2);
        assert!(outline.can_expand_all());
        assert!(!outline.can_collapse_all());
    }

    #[test]
    fn keeps_parseable_nodes_for_invalid_json() {
        let outline = JsonOutline::parse(r#"{"valid": 1, "editing": {"#);
        assert!(outline.has_error);
        assert!(!outline.rows().is_empty());
    }

    #[test]
    fn outlines_root_scalars_and_decodes_escaped_keys() {
        let scalar = JsonOutline::parse("\"hello\"");
        assert_eq!(scalar.rows()[0].title, "$: \"hello\"");

        let outline = JsonOutline::parse(r#"{"line\nkey":1,"café":2}"#);
        let rows = outline.rows();
        assert!(rows[0].title.starts_with("line\nkey:"));
        assert!(rows[1].title.starts_with("café:"));
    }

    #[test]
    fn source_offsets_are_utf8_byte_offsets() {
        let source = r#"{"é":"first","target":"second"}"#;
        let outline = JsonOutline::parse(source);
        let target = outline
            .rows()
            .into_iter()
            .find(|row| row.title.starts_with("target:"))
            .expect("target row should exist");

        assert_eq!(
            &source[target.source_offset..target.source_offset + 8],
            "\"target\""
        );
    }

    #[test]
    fn traverses_deep_json_without_recursive_outline_code() {
        let depth = 256;
        let source = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        let mut outline = DocumentOutline::Json(JsonOutline::parse(&source));

        for _ in 0..depth.saturating_sub(1) {
            let rows = outline.rows();
            let row = rows.last().expect("nested row should exist");
            let node_index = row.node_index.expect("nested node should exist");
            if !row.has_children {
                break;
            }
            assert!(outline.expand(node_index));
        }

        assert!(!outline.rows().is_empty());
    }

    #[test]
    fn truncates_long_unicode_previews_safely() {
        let value = "ö".repeat(100);
        let preview = truncate_preview(&value, 80);
        assert_eq!(preview.chars().count(), 81);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn caps_large_json_outlines() {
        let values = (0..JSON_OUTLINE_NODE_LIMIT + 20)
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let outline = JsonOutline::parse(&format!("[{values}]"));
        assert!(outline.truncated);
        assert_eq!(outline.nodes.len(), JSON_OUTLINE_NODE_LIMIT);
        assert!(
            outline
                .rows()
                .last()
                .is_some_and(|row| row.disabled && row.title.contains("limited"))
        );
    }
}
