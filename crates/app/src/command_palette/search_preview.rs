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
    let blocks = split_search_preview_blocks(value)
        .into_iter()
        .map(|raw| {
            let markdown = search_text_without_markers(&raw, true).trim().to_string();
            let exact_match = !exact_search_ranges(&markdown, query).is_empty();
            let marker_match = !search_marker_ranges(&raw, true).is_empty();

            (markdown, exact_match, marker_match)
        })
        .filter(|(markdown, _, _)| !markdown.is_empty())
        .collect::<Vec<_>>();

    let selected_index = blocks
        .iter()
        .position(|(_, exact_match, _)| *exact_match)
        .or_else(|| blocks.iter().position(|(_, _, marker_match)| *marker_match));

    blocks
        .into_iter()
        .enumerate()
        .map(|(index, (markdown, _, _))| SearchPreviewBlock {
            markdown,
            is_match: selected_index == Some(index),
        })
        .collect()
}

fn split_search_preview_blocks(value: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut in_fence = false;

    for line in value.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
        }

        if line.trim().is_empty() && !in_fence {
            if !current.trim().is_empty() {
                blocks.push(current.trim().to_string());
                current.clear();
            }
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }

    if !current.trim().is_empty() {
        blocks.push(current.trim().to_string());
    }

    blocks
}

fn search_marker_ranges(value: &str, preserve_newlines: bool) -> Vec<std::ops::Range<usize>> {
    let mut text = String::with_capacity(value.len());
    let mut match_start = None;
    let mut ranges = Vec::new();

    for ch in value.chars() {
        match ch {
            '\u{1}' => match_start = Some(text.len()),
            '\u{2}' => {
                if let Some(start) = match_start.take()
                    && start < text.len()
                {
                    ranges.push(start..text.len());
                }
            }
            '\r' => {}
            '\n' if !preserve_newlines => text.push(' '),
            _ => text.push(ch),
        }
    }

    if let Some(start) = match_start
        && start < text.len()
    {
        ranges.push(start..text.len());
    }

    ranges
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
    let phrase = query.split_whitespace().collect::<Vec<_>>().join(" ");

    if phrase.is_empty() {
        None
    } else {
        Some(phrase)
    }
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
