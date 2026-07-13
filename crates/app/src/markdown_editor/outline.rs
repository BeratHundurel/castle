#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutlineItem {
    pub(crate) level: u8,
    pub(crate) title: String,
    pub(crate) source_line: usize,
    pub(crate) source_column: usize,
    pub(crate) preview_section_index: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MarkdownOutline {
    pub(crate) items: Vec<OutlineItem>,
    pub(crate) sections: Vec<String>,
}

impl MarkdownOutline {
    pub(crate) fn parse(source: &str) -> Self {
        let lines = source.lines().collect::<Vec<_>>();
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
                OutlineItem {
                    level: *level,
                    title: title.clone(),
                    source_line: *source_line,
                    source_column: *source_column,
                    preview_section_index: index + section_offset,
                }
            })
            .collect();

        Self { items, sections }
    }

    pub(crate) fn active_index_for_line(&self, line: usize) -> Option<usize> {
        self.items
            .iter()
            .enumerate()
            .rev()
            .find(|(_, item)| item.source_line <= line)
            .map(|(index, _)| index)
    }
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
    use super::MarkdownOutline;

    #[test]
    fn parses_atx_setext_unicode_and_duplicate_headings() {
        let outline =
            MarkdownOutline::parse("# Plan\n\n## Ölçümler\n\nPlan\n----\n\n## Ölçümler\n");
        let titles = outline
            .items
            .iter()
            .map(|item| (item.level, item.title.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            titles,
            vec![(1, "Plan"), (2, "Ölçümler"), (2, "Plan"), (2, "Ölçümler")]
        );
    }

    #[test]
    fn ignores_heading_syntax_inside_fences_and_empty_headings() {
        let outline = MarkdownOutline::parse("```md\n# Hidden\n```\n###   \n# Visible\n");
        assert_eq!(outline.items.len(), 1);
        assert_eq!(outline.items[0].title, "Visible");
    }

    #[test]
    fn creates_preview_sections_and_tracks_active_heading() {
        let outline = MarkdownOutline::parse("Intro\n\n# One\nBody\n## Two\nMore");
        assert_eq!(outline.sections.len(), 3);
        assert_eq!(outline.items[1].preview_section_index, 2);
        assert_eq!(outline.active_index_for_line(3), Some(0));
        assert_eq!(outline.active_index_for_line(4), Some(1));
    }
}
