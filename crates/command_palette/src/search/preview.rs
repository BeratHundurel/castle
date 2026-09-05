use gpui_kit::component::{ActiveTheme, h_flex, text::TextViewStyle};
use gpui_kit::{
    Context, HighlightStyle, IntoElement, ParentElement, SharedString, Styled, StyledText, div, px,
    relative,
};

use storage::workspace::search::SearchResult;

use crate::CommandPaletteView;

pub(super) fn search_result_snippet_source(result: &SearchResult) -> &str {
    if result.snippet.trim().is_empty() {
        "Title match"
    } else {
        &result.snippet
    }
}

pub(super) fn search_result_row_text(result: &SearchResult) -> String {
    let raw_snippet = search_result_snippet_source(result);
    let snippet = search_text_without_markers(raw_snippet, false);
    let title = search_text_without_markers(&result.highlighted_title, false);

    if snippet.trim().is_empty()
        || snippet.trim().eq_ignore_ascii_case("Title match")
        || snippet.trim().eq_ignore_ascii_case(title.trim())
    {
        title
    } else {
        search_result_row_markdown_text(raw_snippet)
    }
}

fn search_result_row_markdown_text(value: &str) -> String {
    let text = search_text_without_markers(value, false);
    let trimmed = text.trim_start();

    if let Some((_, heading)) = markdown_heading(trimmed) {
        heading.to_string()
    } else {
        text
    }
}

pub(super) fn highlighted_exact_search_text(
    value: &str,
    query: &str,
    cx: &mut Context<CommandPaletteView>,
) -> StyledText {
    let text = search_text_without_markers(value, false);
    let ranges: Vec<(std::ops::Range<usize>, HighlightStyle)> = exact_search_ranges(&text, query)
        .into_iter()
        .map(|range| (range, search_highlight_style(cx)))
        .collect();

    StyledText::new(SharedString::from(text)).with_highlights(ranges)
}

fn search_highlight_style(cx: &mut Context<CommandPaletteView>) -> HighlightStyle {
    HighlightStyle {
        color: Some(cx.theme().primary),
        font_weight: Some(gpui_kit::FontWeight::SEMIBOLD),
        background_color: Some(cx.theme().primary.opacity(0.18)),
        ..Default::default()
    }
}

pub(super) fn search_preview_markdown_style() -> TextViewStyle {
    TextViewStyle::default()
        .paragraph_gap(gpui_kit::rems(0.65))
        .heading_font_size(|level, _| match level {
            1 => px(20.),
            2 => px(17.),
            3 => px(15.),
            4 | 5 => px(14.),
            _ => px(13.),
        })
}

#[derive(Clone)]
pub(super) struct SearchPreviewBlock {
    pub(super) markdown: String,
    pub(super) is_match: bool,
}

pub(super) fn search_preview_blocks(value: &str, query: &str) -> Vec<SearchPreviewBlock> {
    let raw_blocks = split_search_preview_blocks(value);
    let terms = search_query_terms(query);
    let mut blocks = Vec::with_capacity(raw_blocks.len());
    let mut best: Option<(bool, usize, usize)> = None;

    for raw in raw_blocks {
        let marker_match = has_search_marker_match(raw);
        let markdown = search_preview_markdown(raw);
        if markdown.is_empty() {
            continue;
        }

        let block_index = blocks.len();
        let overlap = block_term_overlap(&markdown, &terms);
        let is_better = match best {
            None => marker_match || overlap > 0,
            Some((best_marked, best_overlap, _)) => {
                (marker_match, overlap) > (best_marked, best_overlap)
            }
        };
        if is_better {
            best = Some((marker_match, overlap, block_index));
        }
        blocks.push(SearchPreviewBlock {
            markdown,
            is_match: false,
        });
    }

    if let Some((_, _, index)) = best
        && let Some(block) = blocks.get_mut(index)
    {
        block.is_match = true;
    }

    blocks
}

fn search_query_terms(query: &str) -> Vec<String> {
    let raw = query
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if raw.len() <= 1 {
        return raw.into_iter().map(|word| word.to_lowercase()).collect();
    }
    let mut terms = raw
        .iter()
        .filter(|word| word.chars().count() > 1)
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        terms = raw.into_iter().map(|word| word.to_lowercase()).collect();
    }
    terms
}

fn block_term_overlap(markdown: &str, terms: &[String]) -> usize {
    terms
        .iter()
        .filter(|term| block_contains_term(markdown, term))
        .count()
}

fn block_contains_term(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return false;
    }
    if needle_lower.is_ascii() {
        return contains_ascii_case_insensitive(haystack, needle_lower.as_bytes());
    }
    haystack.to_lowercase().contains(needle_lower)
}

fn contains_ascii_case_insensitive(haystack: &str, needle_lower: &[u8]) -> bool {
    let haystack = haystack.as_bytes();
    if needle_lower.len() > haystack.len() {
        return false;
    }

    let first = needle_lower[0];
    let last_start = haystack.len() - needle_lower.len();
    let mut start = 0;
    while start <= last_start {
        if haystack[start].to_ascii_lowercase() == first
            && haystack[start..start + needle_lower.len()].eq_ignore_ascii_case(needle_lower)
        {
            return true;
        }
        start += 1;
    }

    false
}

fn split_search_preview_blocks(value: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut block_start = None;
    let mut block_end = 0;
    let mut in_fence = false;
    let mut offset = 0;

    for raw_line in value.split_inclusive('\n') {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
        }

        if line.trim().is_empty() && !in_fence {
            if let Some(start) = block_start.take() {
                let block = value[start..block_end].trim();
                if !block.is_empty() {
                    blocks.push(block);
                }
            }
        } else {
            let line_start = offset;
            block_start.get_or_insert(line_start);
            block_end = line_start + line.len();
        }

        offset += raw_line.len();
    }

    if let Some(start) = block_start {
        let block = value[start..block_end].trim();
        if !block.is_empty() {
            blocks.push(block);
        }
    }

    blocks
}

fn search_preview_markdown(value: &str) -> String {
    let mut markdown = search_text_without_markers(value, true);
    let leading_whitespace = markdown.len() - markdown.trim_start().len();
    if leading_whitespace > 0 {
        markdown.drain(..leading_whitespace);
    }
    let trimmed_len = markdown.trim_end().len();
    markdown.truncate(trimmed_len);
    markdown
}

fn has_search_marker_match(value: &str) -> bool {
    let mut marker_has_text = None;

    for ch in value.chars() {
        match ch {
            '\u{1}' => marker_has_text = Some(false),
            '\u{2}' => {
                if marker_has_text.take() == Some(true) {
                    return true;
                }
            }
            '\r' => {}
            _ => {
                if let Some(has_text) = marker_has_text.as_mut() {
                    *has_text = true;
                }
            }
        }
    }

    marker_has_text == Some(true)
}

fn exact_search_ranges(haystack: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    if let Some(needle) = normalized_search_phrase(query)
        && !needle.is_empty()
        && haystack.is_ascii()
        && needle.is_ascii()
    {
        let mut haystack_lower = haystack.to_string();
        haystack_lower.make_ascii_lowercase();

        let mut needle_lower = needle;
        needle_lower.make_ascii_lowercase();

        let mut ranges = Vec::new();
        let mut search_start = 0;
        while let Some(offset) = haystack_lower[search_start..].find(&needle_lower) {
            let start = search_start + offset;
            let end = start + needle_lower.len();
            ranges.push(start..end);
            search_start = end;
        }
        if !ranges.is_empty() {
            return ranges;
        }
    }

    longest_term_run_ranges(haystack, &search_query_terms(query))
}

fn longest_term_run_ranges(haystack: &str, query_terms: &[String]) -> Vec<std::ops::Range<usize>> {
    if query_terms.len() < 2 {
        return Vec::new();
    }

    let words = haystack_word_spans(haystack);
    if words.is_empty() {
        return Vec::new();
    }

    let mut best_len = 1;
    let mut spans = Vec::new();
    for run_start in 0..query_terms.len() {
        for run_end in run_start + 2..=query_terms.len() {
            let run = &query_terms[run_start..run_end];
            let mut matched = false;
            for window in words.windows(run.len()) {
                if window
                    .iter()
                    .zip(run)
                    .all(|((word, _, _), term)| word == term)
                {
                    let start = window.first().map(|(_, start, _)| *start).unwrap_or(0);
                    let end = window.last().map(|(_, _, end)| *end).unwrap_or(0);
                    if run.len() > best_len {
                        best_len = run.len();
                        spans.clear();
                        spans.push(start..end);
                    } else if run.len() == best_len {
                        spans.push(start..end);
                    }
                    matched = true;
                }
            }
            if !matched {
                break;
            }
        }
    }

    spans.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<std::ops::Range<usize>> = Vec::with_capacity(spans.len());
    for span in spans {
        if let Some(last) = merged.last_mut()
            && span.start <= last.end
        {
            last.end = last.end.max(span.end);
            continue;
        }
        merged.push(span);
    }
    merged
}

fn haystack_word_spans(haystack: &str) -> Vec<(String, usize, usize)> {
    let mut words = Vec::new();
    let mut word_start = None;
    for (index, ch) in haystack.char_indices().chain([(haystack.len(), '\0')]) {
        if ch.is_alphanumeric() || ch == '_' {
            if word_start.is_none() {
                word_start = Some(index);
            }
        } else if let Some(start) = word_start.take() {
            words.push((haystack[start..index].to_lowercase(), start, index));
        }
    }
    words
}

pub(super) fn render_highlighted_preview_line(
    line: &str,
    query: &str,
    cx: &mut Context<CommandPaletteView>,
) -> gpui_kit::AnyElement {
    let theme = cx.theme().clone();
    let trimmed = line.trim_start();

    if let Some((level, text)) = markdown_heading(trimmed) {
        return div()
            .w_full()
            .min_w_0()
            .whitespace_normal()
            .text_size(search_preview_heading_size(level))
            .font_weight(gpui_kit::FontWeight::BOLD)
            .line_height(relative(1.28))
            .child(highlighted_preview_text(text, query, cx))
            .into_any_element();
    }

    if let Some(text) = markdown_list_item(trimmed) {
        return h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_2()
            .child(
                div()
                    .pt(px(2.))
                    .text_color(theme.muted_foreground)
                    .child("•"),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .line_height(relative(1.5))
                    .child(highlighted_preview_text(text, query, cx)),
            )
            .into_any_element();
    }

    if let Some((marker, text)) = markdown_ordered_list_item(trimmed) {
        return h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_2()
            .child(
                div()
                    .pt(px(1.))
                    .text_color(theme.muted_foreground)
                    .child(marker),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .line_height(relative(1.5))
                    .child(highlighted_preview_text(text, query, cx)),
            )
            .into_any_element();
    }

    if let Some(text) = trimmed.strip_prefix('>') {
        return div()
            .w_full()
            .min_w_0()
            .whitespace_normal()
            .border_l_2()
            .border_color(theme.border)
            .pl_3()
            .text_color(theme.muted_foreground)
            .line_height(relative(1.5))
            .child(highlighted_preview_text(text.trim_start(), query, cx))
            .into_any_element();
    }

    div()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .line_height(relative(1.5))
        .child(highlighted_preview_text(trimmed, query, cx))
        .into_any_element()
}

fn highlighted_preview_text(
    value: &str,
    query: &str,
    cx: &mut Context<CommandPaletteView>,
) -> StyledText {
    let text = clean_inline_markdown(value);
    let ranges: Vec<(std::ops::Range<usize>, HighlightStyle)> = exact_search_ranges(&text, query)
        .into_iter()
        .map(|range| {
            (
                range,
                HighlightStyle {
                    color: Some(cx.theme().primary),
                    font_weight: Some(gpui_kit::FontWeight::SEMIBOLD),
                    background_color: Some(cx.theme().primary.opacity(0.12)),
                    ..Default::default()
                },
            )
        })
        .collect();

    StyledText::new(SharedString::from(text)).with_highlights(ranges)
}

fn markdown_heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|ch| *ch == '#').count();
    if (1..=6).contains(&level) && line.as_bytes().get(level) == Some(&b' ') {
        Some((level, line[level + 1..].trim()))
    } else {
        None
    }
}

fn markdown_list_item(line: &str) -> Option<&str> {
    line.strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
        .map(str::trim)
}

fn markdown_ordered_list_item(line: &str) -> Option<(String, &str)> {
    let marker_end = line
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .last()
        .map(|(index, ch)| index + ch.len_utf8())?;

    let marker = line.get(..marker_end)?;
    let rest = line.get(marker_end..)?;
    let text = rest
        .strip_prefix(". ")
        .or_else(|| rest.strip_prefix(") "))?;

    Some((format!("{marker}."), text.trim()))
}

fn search_preview_heading_size(level: usize) -> gpui_kit::Pixels {
    match level {
        1 => px(20.),
        2 => px(17.),
        3 => px(15.),
        _ => px(14.),
    }
}

fn clean_inline_markdown(value: &str) -> String {
    value
        .replace("**", "")
        .replace("__", "")
        .replace(['`', '[', ']'], "")
}

fn search_text_without_markers(value: &str, preserve_newlines: bool) -> String {
    let mut text = String::with_capacity(value.len());

    for ch in value.chars() {
        match ch {
            '\u{1}' | '\u{2}' => {}
            '\r' => {}
            '\n' if !preserve_newlines => text.push(' '),
            _ => text.push(ch),
        }
    }

    text
}

fn normalized_search_phrase(query: &str) -> Option<String> {
    let mut words = query.split_whitespace();
    let first = words.next()?;
    let mut phrase = String::with_capacity(query.len());
    phrase.push_str(first);
    for word in words {
        phrase.push(' ');
        phrase.push_str(word);
    }

    Some(phrase)
}

pub(super) fn search_result_preview_source(result: &SearchResult) -> &str {
    if result.preview.trim().is_empty() {
        search_result_snippet_source(result)
    } else {
        &result.preview
    }
}

#[cfg(test)]
mod tests {
    use super::{exact_search_ranges, search_preview_blocks};
    use std::{hint::black_box, time::Instant};
    use test_support as test_alloc;

    fn assert_preview_blocks(value: &str, query: &str, expected: &[(&str, bool)]) {
        let blocks = search_preview_blocks(value, query);

        assert_eq!(blocks.len(), expected.len());
        for (block, (markdown, is_match)) in blocks.iter().zip(expected) {
            assert_eq!(block.markdown, *markdown);
            assert_eq!(block.is_match, *is_match, "block: {markdown}");
        }
    }

    #[test]
    fn blank_lines_split_blocks_and_empty_blocks_are_discarded() {
        assert_preview_blocks(
            "\n\nFirst block.\n\n  \nSecond block.\nStill second.\n\n",
            "missing",
            &[
                ("First block.", false),
                ("Second block.\nStill second.", false),
            ],
        );
    }

    #[test]
    fn fenced_code_blank_lines_remain_in_the_same_block() {
        assert_preview_blocks(
            "Before.\n\n```rust\nfn first() {}\n\nfn second() {}\n```\n\nAfter.",
            "second",
            &[
                ("Before.", false),
                ("```rust\nfn first() {}\n\nfn second() {}\n```", true),
                ("After.", false),
            ],
        );
    }

    #[test]
    fn crlf_is_normalized_while_splitting_blocks() {
        assert_preview_blocks(
            "First line.\r\nSecond line.\r\n\r\nThird line.\r\n",
            "third",
            &[("First line.\nSecond line.", false), ("Third line.", true)],
        );
    }

    #[test]
    fn unicode_content_is_preserved_and_can_be_selected_by_markers() {
        assert_preview_blocks(
            "İstanbul and 東京.\n\nSelected \u{1}naïve café\u{2} text.",
            "naïve café",
            &[
                ("İstanbul and 東京.", false),
                ("Selected naïve café text.", true),
            ],
        );
    }

    #[test]
    fn incomplete_opening_marker_selects_the_rest_of_its_block() {
        assert_preview_blocks(
            "Earlier search text.\n\nIncomplete \u{1}selected text.",
            "search",
            &[
                ("Earlier search text.", false),
                ("Incomplete selected text.", true),
            ],
        );
    }

    #[test]
    fn complete_fts_markers_select_the_containing_block() {
        assert_preview_blocks(
            "Unrelated.\n\nComplete \u{1}selected text\u{2} here.",
            "missing",
            &[
                ("Unrelated.", false),
                ("Complete selected text here.", true),
            ],
        );
    }

    #[test]
    fn empty_and_unmatched_closing_markers_do_not_select_a_block() {
        assert_preview_blocks(
            "Empty markers \u{1}\u{2}.\n\nUnmatched closing marker \u{2}.",
            "missing",
            &[
                ("Empty markers .", false),
                ("Unmatched closing marker .", false),
            ],
        );
    }

    #[test]
    fn marked_fts_match_wins_over_an_earlier_query_occurrence() {
        assert_preview_blocks(
            "An earlier search occurrence.\n\nThe selected \u{1}search\u{2} occurrence.",
            "search",
            &[
                ("An earlier search occurrence.", false),
                ("The selected search occurrence.", true),
            ],
        );
    }

    #[test]
    fn exact_query_is_used_when_preview_has_no_fts_markers() {
        assert_preview_blocks(
            "First block.\n\nContains search here.",
            "search",
            &[("First block.", false), ("Contains search here.", true)],
        );
    }

    #[test]
    fn no_query_or_marker_match_leaves_every_block_unselected() {
        assert_preview_blocks(
            "First block.\n\nSecond block.",
            "absent",
            &[("First block.", false), ("Second block.", false)],
        );
    }

    #[test]
    fn heading_and_list_markdown_is_preserved() {
        assert_preview_blocks(
            "## Heading\n- first item\n- selected item\n\n1. Ordered item",
            "selected item",
            &[
                ("## Heading\n- first item\n- selected item", true),
                ("1. Ordered item", false),
            ],
        );
    }

    #[test]
    fn multi_space_query_is_normalized_for_exact_selection() {
        assert_preview_blocks(
            "Unrelated.\n\nA normalized search phrase appears here.",
            "  normalized   search\tphrase  ",
            &[
                ("Unrelated.", false),
                ("A normalized search phrase appears here.", true),
            ],
        );
    }

    #[test]
    fn multi_word_query_selects_block_where_terms_cooccur() {
        assert_preview_blocks(
            "The preface was filler.\n\n## Why the first root fix was still not enough",
            "the root fix was",
            &[
                ("The preface was filler.", false),
                ("## Why the first root fix was still not enough", true),
            ],
        );
    }

    #[test]
    fn marked_multi_word_query_prefers_cooccurrence_over_earlier_marker() {
        assert_preview_blocks(
            "The \u{1}preamble\u{2} \u{1}was\u{2} filler.\n\n## Why \u{1}the\u{2} first \u{1}root\u{2} \u{1}fix\u{2} \u{1}was\u{2} still not enough",
            "the root fix was",
            &[
                ("The preamble was filler.", false),
                ("## Why the first root fix was still not enough", true),
            ],
        );
    }

    #[test]
    fn multi_word_highlight_falls_back_to_longest_term_run() {
        let ranges = exact_search_ranges(
            "Why the first root fix was still not enough",
            "the root fix was",
        );
        assert_eq!(ranges, vec![14..26]);

        let single = exact_search_ranges("root and Root", "root");
        assert_eq!(single, vec![0..4, 9..13]);
    }

    #[tokio::test]
    async fn multi_word_search_selects_cooccurrence_from_storage_preview() {
        use entity::note;
        use migration::{Migrator, MigratorTrait};
        use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};

        let db = Database::connect("sqlite::memory:")
            .await
            .expect("search chain test database should connect");
        Migrator::up(&db, None)
            .await
            .expect("search chain test database should migrate");
        let filler = "The preliminary note was just filler text. ".repeat(160);
        note::ActiveModel {
            title: Set("The Bug".to_string()),
            project_id: Set(None),
            file_path: Set(None),
            file_managed_by_app: Set(false),
            cached_content: Set(format!(
                "{filler}\n\n## Why the first root fix was still not enough\n\nThe first repair attempt was incomplete."
            )),
            file_missing_since: Set(None),
            created_at: Set(1),
            updated_at: Set(1),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("search chain test note should insert");
        storage::workspace::search::rebuild_search_index(&db)
            .await
            .expect("search chain test index should build");
        let results = storage::workspace::search::search_workspace(&db, "the root fix was", 10)
            .await
            .expect("search chain test search should run");
        assert_eq!(results.len(), 1);

        let blocks = search_preview_blocks(&results[0].preview, "the root fix was");
        let selected = blocks
            .iter()
            .find(|block| block.is_match)
            .expect("search chain should select a block");
        assert!(
            selected.markdown.contains("first root fix"),
            "selected block should be the co-occurrence, got: {}",
            selected.markdown
        );
        let line = selected
            .markdown
            .lines()
            .find(|line| line.contains("root fix"))
            .expect("selected block should contain the term run");
        let ranges = exact_search_ranges(line, "the root fix was");
        assert_eq!(ranges.len(), 1);
        assert_eq!(&line[ranges[0].clone()], "root fix was");
    }

    #[test]
    fn large_preview_allocation_budget() {
        const BLOCKS: usize = 512;
        const REPEATED_LINES: usize = 10;

        let mut preview = String::with_capacity(800 * 1024);
        for block_index in 0..BLOCKS {
            if block_index > 0 {
                preview.push_str("\n\n");
            }
            preview.push_str("## Project notes\n");
            for _ in 0..REPEATED_LINES {
                preview.push_str(
                    "- Planning details, implementation notes, and follow-up context for the team.\n",
                );
            }
            if block_index == BLOCKS - 2 {
                preview.push_str("The selected \u{1}allocation target\u{2} is in this block.\n");
            }
            preview.push_str("Closing context for this preview block.");
        }

        let started = Instant::now();
        let measurement = test_alloc::start_measurement();
        let blocks = search_preview_blocks(black_box(&preview), black_box("allocation target"));
        black_box(&blocks);
        let allocation = measurement.finish();
        let elapsed = started.elapsed();

        assert_eq!(blocks.len(), BLOCKS);
        assert!(blocks[BLOCKS - 2].is_match);
        assert_eq!(blocks.iter().filter(|block| block.is_match).count(), 1);
        let allocation_budget = preview.len() * 11 / 10;
        assert!(
            allocation.allocated_bytes < allocation_budget,
            "allocated_bytes={} input_bytes={}",
            allocation.allocated_bytes,
            preview.len()
        );
        assert!(allocation.peak_growth_bytes < allocation_budget);
        assert!(allocation.retained_growth_bytes < allocation_budget);
        eprintln!(
            "search_preview input_bytes={} blocks={BLOCKS} elapsed_micros={} allocated_bytes={} peak_heap_growth_bytes={} retained_heap_growth_bytes={}",
            preview.len(),
            elapsed.as_micros(),
            allocation.allocated_bytes,
            allocation.peak_growth_bytes,
            allocation.retained_growth_bytes
        );
    }
}
