use gpui::{
    Context, HighlightStyle, IntoElement, ParentElement, SharedString, Styled, StyledText, div, px,
    relative,
};
use gpui_component::{ActiveTheme, h_flex, text::TextViewStyle};

use crate::app_shell::AppShell;
use crate::search::SearchResult;

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
    cx: &mut Context<AppShell>,
) -> StyledText {
    let text = search_text_without_markers(value, false);
    let ranges: Vec<(std::ops::Range<usize>, HighlightStyle)> = exact_search_ranges(&text, query)
        .into_iter()
        .map(|range| (range, search_highlight_style(cx)))
        .collect();

    StyledText::new(SharedString::from(text)).with_highlights(ranges)
}

fn search_highlight_style(cx: &mut Context<AppShell>) -> HighlightStyle {
    HighlightStyle {
        color: Some(cx.theme().primary),
        font_weight: Some(gpui::FontWeight::SEMIBOLD),
        background_color: Some(cx.theme().primary.opacity(0.18)),
        ..Default::default()
    }
}

pub(super) fn search_preview_markdown_style() -> TextViewStyle {
    TextViewStyle::default()
        .paragraph_gap(gpui::rems(0.65))
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
    let normalized_query = normalized_search_phrase(query);
    let mut blocks = Vec::with_capacity(raw_blocks.len());
    let mut marker_match_index = None;
    let mut exact_match_index = None;

    for raw in raw_blocks {
        let marker_match = has_search_marker_match(raw);
        let markdown = search_preview_markdown(raw);
        if markdown.is_empty() {
            continue;
        }

        let block_index = blocks.len();
        if marker_match_index.is_none() && marker_match {
            marker_match_index = Some(block_index);
        }
        if exact_match_index.is_none()
            && contains_exact_search(&markdown, normalized_query.as_deref())
        {
            exact_match_index = Some(block_index);
        }
        blocks.push(SearchPreviewBlock {
            markdown,
            is_match: false,
        });
    }

    if let Some(block) = marker_match_index
        .or(exact_match_index)
        .and_then(|index| blocks.get_mut(index))
    {
        block.is_match = true;
    }

    blocks
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

fn contains_exact_search(haystack: &str, needle: Option<&str>) -> bool {
    let Some(needle) = needle else {
        return false;
    };

    if needle.is_empty() || !haystack.is_ascii() || !needle.is_ascii() {
        return false;
    }

    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.len() > haystack.len() {
        return false;
    }

    let first_lower = needle[0].to_ascii_lowercase();
    let first_upper = needle[0].to_ascii_uppercase();
    let last_start = haystack.len() - needle.len();
    let mut start = 0;
    while start <= last_start {
        let first = haystack[start];
        if (first == first_lower || first == first_upper)
            && haystack[start..start + needle.len()].eq_ignore_ascii_case(needle)
        {
            return true;
        }
        start += 1;
    }

    false
}

pub(super) fn render_highlighted_preview_line(
    line: &str,
    query: &str,
    cx: &mut Context<AppShell>,
) -> gpui::AnyElement {
    let theme = cx.theme().clone();
    let trimmed = line.trim_start();

    if let Some((level, text)) = markdown_heading(trimmed) {
        return div()
            .w_full()
            .min_w_0()
            .whitespace_normal()
            .text_size(search_preview_heading_size(level))
            .font_weight(gpui::FontWeight::BOLD)
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

fn highlighted_preview_text(value: &str, query: &str, cx: &mut Context<AppShell>) -> StyledText {
    let text = clean_inline_markdown(value);
    let ranges: Vec<(std::ops::Range<usize>, HighlightStyle)> = exact_search_ranges(&text, query)
        .into_iter()
        .map(|range| {
            (
                range,
                HighlightStyle {
                    color: Some(cx.theme().primary),
                    font_weight: Some(gpui::FontWeight::SEMIBOLD),
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

fn search_preview_heading_size(level: usize) -> gpui::Pixels {
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

fn exact_search_ranges(haystack: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    let Some(needle) = normalized_search_phrase(query) else {
        return Vec::new();
    };

    if needle.is_empty() || !haystack.is_ascii() || !needle.is_ascii() {
        return Vec::new();
    }

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

    ranges
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
    use super::search_preview_blocks;
    use crate::test_alloc;
    use std::{hint::black_box, time::Instant};

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
