use gpui::{Context, EntityInputHandler, Window};
use gpui_component::input::RopeExt;
use std::ops::Range;

use super::MarkdownEditorView;
use super::action::{ApplyMarkdownFormat, MarkdownFormat};

impl MarkdownEditorView {
    pub(super) fn apply_format(
        &mut self,
        action: &ApplyMarkdownFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self.editor.read(cx).selected_value().to_string();
        let replacement = match action.0 {
            MarkdownFormat::HeadingOne => Self::prefix_block(&selected, "# ", "Heading"),
            MarkdownFormat::HeadingTwo => Self::prefix_block(&selected, "## ", "Heading"),
            MarkdownFormat::HeadingThree => Self::prefix_block(&selected, "### ", "Heading"),
            MarkdownFormat::Bold => Self::wrap_inline(&selected, "**", "**", "bold text"),
            MarkdownFormat::Italic => Self::wrap_inline(&selected, "*", "*", "italic text"),
            MarkdownFormat::InlineCode => Self::wrap_inline(&selected, "`", "`", "code"),
            MarkdownFormat::Link => Self::wrap_inline(&selected, "[", "](https://)", "link text"),
            MarkdownFormat::BulletList => Self::prefix_lines(&selected, "- ", "List item"),
            MarkdownFormat::OrderedList => Self::numbered_lines(&selected),
            MarkdownFormat::Quote => Self::prefix_lines(&selected, "> ", "Quote"),
            MarkdownFormat::CodeBlock => Self::code_block(&selected),
        };

        self.editor.update(cx, |editor, cx| {
            editor.replace(replacement, window, cx);
            editor.focus(window, cx);
        });
    }

    pub(super) fn wrap_inline(
        selected: &str,
        prefix: &str,
        suffix: &str,
        placeholder: &str,
    ) -> String {
        let body = if selected.is_empty() {
            placeholder
        } else {
            selected
        };
        format!("{prefix}{body}{suffix}")
    }

    pub(super) fn prefix_block(selected: &str, prefix: &str, placeholder: &str) -> String {
        let body = selected.trim_start_matches('#').trim_start();
        let body = if body.is_empty() { placeholder } else { body };
        format!("{prefix}{body}")
    }

    pub(super) fn prefix_lines(selected: &str, prefix: &str, placeholder: &str) -> String {
        if selected.is_empty() {
            return format!("{prefix}{placeholder}");
        }

        selected
            .lines()
            .map(|line| format!("{prefix}{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(super) fn numbered_lines(selected: &str) -> String {
        if selected.is_empty() {
            return "1. List item".to_string();
        }

        selected
            .lines()
            .enumerate()
            .map(|(index, line)| format!("{}. {}", index + 1, line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(super) fn code_block(selected: &str) -> String {
        let body = if selected.is_empty() {
            "code"
        } else {
            selected
        };
        format!("```\n{body}\n```")
    }

    pub(super) fn continue_markdown_after_enter(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (text, cursor) = {
            let editor = self.editor.read(cx);
            (editor.text().to_string(), editor.cursor())
        };

        let Some(edit) = markdown_enter_edit(&text, cursor) else {
            return;
        };

        self.editor.update(cx, |editor, cx| {
            let rope = editor.text();
            let start_utf16 = rope.offset_to_offset_utf16(edit.range.start);
            let end_utf16 = rope.offset_to_offset_utf16(edit.range.end);

            EntityInputHandler::replace_text_in_range(
                editor,
                Some(start_utf16..end_utf16),
                &edit.replacement,
                window,
                cx,
            );
            editor.focus(window, cx);
        });
    }
}

#[derive(Debug, PartialEq, Eq)]
struct MarkdownEnterEdit {
    range: Range<usize>,
    replacement: String,
}

#[derive(Debug, PartialEq, Eq)]
enum MarkdownLineContinuation {
    Continue(String),
    Exit { marker_start: usize },
}

#[derive(Debug, PartialEq, Eq)]
struct ListMarker {
    marker_len: usize,
    next_marker: String,
}

fn markdown_enter_edit(text: &str, cursor: usize) -> Option<MarkdownEnterEdit> {
    if cursor > text.len() {
        return None;
    }

    let current_line_start = text[..cursor].rfind('\n').map(|index| index + 1)?;
    let current_line = &text[current_line_start..cursor];
    if current_line.chars().any(|ch| !ch.is_whitespace()) {
        return None;
    }

    let previous_line_end = current_line_start.saturating_sub(1);
    let previous_line_start = text[..previous_line_end]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let previous_line = &text[previous_line_start..previous_line_end];

    match markdown_line_continuation(previous_line)? {
        MarkdownLineContinuation::Continue(prefix) => {
            if prefix == current_line {
                return None;
            }

            Some(MarkdownEnterEdit {
                range: current_line_start..cursor,
                replacement: prefix,
            })
        }
        MarkdownLineContinuation::Exit { marker_start } => Some(MarkdownEnterEdit {
            range: previous_line_start + marker_start..cursor,
            replacement: String::new(),
        }),
    }
}

fn markdown_line_continuation(line: &str) -> Option<MarkdownLineContinuation> {
    let indent_end = markdown_indent_end(line);
    let quote_end = markdown_quote_prefix_end(line, indent_end);
    let base_prefix = &line[..quote_end];
    let rest = &line[quote_end..];

    if let Some(marker) = markdown_list_marker(rest) {
        if rest[marker.marker_len..].trim().is_empty() {
            return Some(MarkdownLineContinuation::Exit {
                marker_start: quote_end,
            });
        }

        return Some(MarkdownLineContinuation::Continue(format!(
            "{base_prefix}{}",
            marker.next_marker
        )));
    }

    if quote_end > indent_end {
        if rest.trim().is_empty() {
            return Some(MarkdownLineContinuation::Exit {
                marker_start: indent_end,
            });
        }

        return Some(MarkdownLineContinuation::Continue(base_prefix.to_string()));
    }

    None
}

fn markdown_indent_end(line: &str) -> usize {
    line.char_indices()
        .find(|(_, ch)| *ch != ' ' && *ch != '\t')
        .map_or(line.len(), |(index, _)| index)
}

fn markdown_quote_prefix_end(line: &str, mut index: usize) -> usize {
    loop {
        let rest = &line[index..];
        if !rest.starts_with('>') {
            return index;
        }

        index += 1;
        while let Some(ch) = line[index..].chars().next() {
            if ch != ' ' && ch != '\t' {
                break;
            }
            index += ch.len_utf8();
        }
    }
}

fn markdown_list_marker(rest: &str) -> Option<ListMarker> {
    markdown_bullet_marker(rest).or_else(|| markdown_ordered_marker(rest))
}

fn markdown_bullet_marker(rest: &str) -> Option<ListMarker> {
    let bullet = rest.chars().next()?;
    if !matches!(bullet, '-' | '*' | '+') {
        return None;
    }

    let marker_end = bullet.len_utf8();
    let whitespace_len = markdown_whitespace_len(&rest[marker_end..]);
    if whitespace_len == 0 {
        return None;
    }

    let marker_len = marker_end + whitespace_len;
    let marker = &rest[..marker_len];

    if let Some(task_len) = markdown_task_marker_len(&rest[marker_len..]) {
        return Some(ListMarker {
            marker_len: marker_len + task_len,
            next_marker: format!("{marker}[ ] "),
        });
    }

    Some(ListMarker {
        marker_len,
        next_marker: marker.to_string(),
    })
}

fn markdown_ordered_marker(rest: &str) -> Option<ListMarker> {
    let digit_end = rest
        .char_indices()
        .find(|(_, ch)| !ch.is_ascii_digit())
        .map_or(rest.len(), |(index, _)| index);

    if digit_end == 0 {
        return None;
    }

    let delimiter = rest[digit_end..].chars().next()?;
    if !matches!(delimiter, '.' | ')') {
        return None;
    }

    let delimiter_end = digit_end + delimiter.len_utf8();
    let whitespace_len = markdown_whitespace_len(&rest[delimiter_end..]);
    if whitespace_len == 0 {
        return None;
    }

    let number = rest[..digit_end].parse::<u64>().ok()?;
    let whitespace = &rest[delimiter_end..delimiter_end + whitespace_len];

    Some(ListMarker {
        marker_len: delimiter_end + whitespace_len,
        next_marker: format!("{}{}{}", number.saturating_add(1), delimiter, whitespace),
    })
}

fn markdown_task_marker_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.len() < 4
        || bytes[0] != b'['
        || !matches!(bytes[1], b' ' | b'x' | b'X')
        || bytes[2] != b']'
        || !matches!(bytes[3], b' ' | b'\t')
    {
        return None;
    }

    Some(3 + markdown_whitespace_len(&rest[3..]))
}

fn markdown_whitespace_len(rest: &str) -> usize {
    rest.char_indices()
        .take_while(|(_, ch)| *ch == ' ' || *ch == '\t')
        .last()
        .map_or(0, |(index, ch)| index + ch.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continues_bullet_lists() {
        assert_eq!(
            markdown_enter_edit("- item\n", "- item\n".len()),
            Some(MarkdownEnterEdit {
                range: 7..7,
                replacement: "- ".to_string(),
            })
        );
    }

    #[test]
    fn continues_indented_bullet_lists() {
        assert_eq!(
            markdown_enter_edit("  - item\n  ", "  - item\n  ".len()),
            Some(MarkdownEnterEdit {
                range: 9..11,
                replacement: "  - ".to_string(),
            })
        );
    }

    #[test]
    fn exits_empty_bullet_lists() {
        assert_eq!(
            markdown_enter_edit("- \n", "- \n".len()),
            Some(MarkdownEnterEdit {
                range: 0..3,
                replacement: String::new(),
            })
        );
    }

    #[test]
    fn increments_ordered_lists() {
        assert_eq!(
            markdown_enter_edit("9. item\n", "9. item\n".len()),
            Some(MarkdownEnterEdit {
                range: 8..8,
                replacement: "10. ".to_string(),
            })
        );
    }

    #[test]
    fn continues_tasks_as_unchecked() {
        assert_eq!(
            markdown_enter_edit("- [x] done\n", "- [x] done\n".len()),
            Some(MarkdownEnterEdit {
                range: 11..11,
                replacement: "- [ ] ".to_string(),
            })
        );
    }

    #[test]
    fn continues_blockquotes() {
        assert_eq!(
            markdown_enter_edit("> quote\n", "> quote\n".len()),
            Some(MarkdownEnterEdit {
                range: 8..8,
                replacement: "> ".to_string(),
            })
        );
    }

    #[test]
    fn exits_empty_blockquotes() {
        assert_eq!(
            markdown_enter_edit("> \n", "> \n".len()),
            Some(MarkdownEnterEdit {
                range: 0..3,
                replacement: String::new(),
            })
        );
    }

    #[test]
    fn ignores_plain_paragraphs() {
        assert_eq!(markdown_enter_edit("plain\n", "plain\n".len()), None);
    }
}
