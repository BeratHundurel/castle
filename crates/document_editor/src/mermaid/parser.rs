use std::ops::Range;

use gpui_kit::SharedString;

use super::renderer::is_supported_diagram;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MermaidDescriptor {
    pub(super) source: SharedString,
    pub(super) scale: u16,
    pub(super) range: Range<usize>,
}

pub(crate) fn parse_mermaid_blocks(source: &str) -> Vec<MermaidDescriptor> {
    let mut result = Vec::new();
    let mut line_start = 0usize;
    let mut opening: Option<(usize, usize, char, usize, Option<u16>)> = None;

    for line_with_newline in source.split_inclusive('\n') {
        let line = line_with_newline
            .strip_suffix('\n')
            .unwrap_or(line_with_newline);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some((fence_start, content_start, marker, marker_count, scale)) = opening {
            if is_closing_fence_for(line, marker, marker_count) {
                let content_end = line_start;
                let content = source
                    .get(content_start..content_end)
                    .unwrap_or_default()
                    .trim_end_matches(['\r', '\n']);
                if let Some(scale) = scale
                    && is_supported_diagram(content)
                {
                    result.push(MermaidDescriptor {
                        source: content.to_string().into(),
                        scale,
                        range: fence_start..line_start.saturating_add(line_with_newline.len()),
                    });
                }
                opening = None;
            }
        } else if let Some((marker, marker_count, info)) = opening_fence(line) {
            let scale = (marker == '`' && marker_count == 3)
                .then(|| parse_mermaid_info(info))
                .flatten();
            opening = Some((
                line_start,
                line_start.saturating_add(line_with_newline.len()),
                marker,
                marker_count,
                scale,
            ));
        }
        line_start = line_start.saturating_add(line_with_newline.len());
    }
    result
}

fn opening_fence_scale(line: &str) -> Option<u16> {
    let (marker, marker_count, info) = opening_fence(line)?;
    (marker == '`' && marker_count == 3)
        .then(|| parse_mermaid_info(info))
        .flatten()
}

fn opening_fence(line: &str) -> Option<(char, usize, &str)> {
    let indent = line.len().saturating_sub(line.trim_start().len());
    if indent > 3 {
        return None;
    }
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let marker_count = trimmed
        .chars()
        .take_while(|character| *character == marker)
        .count();
    if marker_count < 3 {
        return None;
    }
    Some((marker, marker_count, trimmed[marker_count..].trim()))
}

pub(super) fn parse_mermaid_info(info: &str) -> Option<u16> {
    let mut parts = info.split_whitespace();
    if parts.next()? != "mermaid" {
        return None;
    }
    let scale = parts
        .next()
        .and_then(|part| part.parse::<u16>().ok())
        .unwrap_or(100)
        .clamp(10, 500);
    Some(scale)
}

fn is_closing_fence(line: &str) -> bool {
    is_closing_fence_for(line, '`', 3)
}

fn is_closing_fence_for(line: &str, marker: char, minimum_count: usize) -> bool {
    let indent = line.len().saturating_sub(line.trim_start().len());
    if indent > 3 {
        return false;
    }
    let trimmed = line.trim_start();
    let marker_count = trimmed
        .chars()
        .take_while(|character| *character == marker)
        .count();
    marker_count >= minimum_count && trimmed[marker_count..].trim().is_empty()
}

pub(super) fn is_closed_mermaid_fence(source: &str) -> bool {
    let mut lines = source.lines();
    opening_fence_scale(lines.next().unwrap_or_default()).is_some()
        && lines.last().is_some_and(is_closing_fence)
}
