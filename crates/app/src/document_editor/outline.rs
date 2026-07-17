use std::collections::HashSet;

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
    pub(crate) sections: Vec<String>,
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

    pub(crate) fn markdown_sections(&self) -> &[String] {
        match self {
            Self::Markdown(outline) => &outline.sections,
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
        let lines = source.lines().collect::<Vec<_>>();
        let mut line_offsets = Vec::with_capacity(lines.len());
        let mut offset = 0usize;
        for line in &lines {
            line_offsets.push(offset);
            offset = offset.saturating_add(line.len()).saturating_add(1);
        }

        let mut headings = Vec::<(u8, String, usize, usize)>::new();
        let mut in_fence = false;
        let mut fence_marker = None::<char>;

        for (line_index, line) in lines.iter().enumerate() {
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
                continue;
            }
            if in_fence {
                continue;
            }

            let hash_count = trimmed.chars().take_while(|ch| *ch == '#').count();
            if (1..=6).contains(&hash_count)
                && trimmed
                    .chars()
                    .nth(hash_count)
                    .is_some_and(char::is_whitespace)
            {
                let title = clean_heading(&trimmed[hash_count..]);
                if !title.is_empty() {
                    headings.push((
                        hash_count as u8,
                        title,
                        line_index,
                        line.len().saturating_sub(trimmed.len()),
                    ));
                }
                continue;
            }

            if line_index > 0 && !trimmed.is_empty() {
                let underline = trimmed.trim();
                let level = if underline.len() >= 2 && underline.chars().all(|ch| ch == '=') {
                    Some(1)
                } else if underline.len() >= 2 && underline.chars().all(|ch| ch == '-') {
                    Some(2)
                } else {
                    None
                };
                if let Some(level) = level {
                    let previous = lines[line_index - 1].trim();
                    if !previous.is_empty() {
                        headings.push((level, clean_heading(previous), line_index - 1, 0));
                    }
                }
            }
        }

        headings.sort_by_key(|heading| heading.2);
        headings.dedup_by_key(|heading| heading.2);

        let mut sections = Vec::with_capacity(headings.len().saturating_add(1));
        if let Some(first) = headings.first() {
            if first.2 > 0 {
                sections.push(lines[..first.2].join("\n"));
            }
        } else if !source.is_empty() {
            sections.push(source.to_string());
        }

        let section_offset = usize::from(!sections.is_empty());
        let items = headings
            .iter()
            .enumerate()
            .map(|(index, (level, title, source_line, source_column))| {
                let end = headings
                    .get(index + 1)
                    .map(|heading| heading.2)
                    .unwrap_or(lines.len());
                sections.push(lines[*source_line..end].join("\n"));
                OutlineRow {
                    node_index: Some(index),
                    title: title.clone(),
                    depth: usize::from(level.saturating_sub(1)),
                    source_offset: line_offsets
                        .get(*source_line)
                        .copied()
                        .unwrap_or_default()
                        .saturating_add(*source_column),
                    source_line: *source_line,
                    preview_section_index: Some(index + section_offset),
                    has_children: false,
                    expanded: false,
                    disabled: false,
                }
            })
            .collect();

        Self { items, sections }
    }

    fn active_index_for_line(&self, line: usize) -> Option<usize> {
        self.items
            .iter()
            .enumerate()
            .rev()
            .find(|(_, item)| item.source_line <= line)
            .map(|(index, _)| index)
    }
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

    #[test]
    fn parses_markdown_headings_and_preview_sections() {
        let outline = MarkdownOutline::parse("Intro\n\n# One\nBody\n## Two\nMore");
        assert_eq!(outline.items.len(), 2);
        assert_eq!(outline.sections.len(), 3);
        assert_eq!(outline.items[1].preview_section_index, Some(2));
        assert_eq!(outline.items[1].depth, 1);
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
