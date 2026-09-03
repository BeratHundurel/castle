use gpui::{Context, EntityInputHandler, Window};
use gpui_component::{WindowExt as _, input::RopeExt, notification::Notification};
use std::ops::Range;
use std::path::Path;

use super::action::{ApplyMarkdownFormat, MarkdownFormat};
use super::smart_editing::format_task_lines;
use super::{DocumentEditorView, DocumentKind};

impl DocumentEditorView {
    pub(super) fn format_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.kind, DocumentKind::Markdown | DocumentKind::Json) {
            window.push_notification(
                Notification::info("Formatting is available for Markdown and JSON documents."),
                cx,
            );
            return;
        }

        let kind = self.kind;
        let (source, input_selection) = self.editor.read_with(cx, |editor, _| {
            (editor.text().to_string(), editor.selected_range())
        });
        let vim_selection = self.vim_visual_range(cx);
        let selection = vim_selection.clone().unwrap_or(input_selection);
        let background = cx.background_executor().clone();

        self.persistence.format_task = Some(cx.spawn_in(window, async move |this, cx| {
            let (source, result) = background
                .spawn(async move {
                    let result = format_document_text(kind, &source);
                    (source, result)
                })
                .await;

            this.update_in(cx, |this, window, cx| {
                if this.kind != kind || *this.editor.read(cx).text() != source {
                    window.push_notification(
                        Notification::warning(
                            "Formatting was skipped because the document changed.",
                        ),
                        cx,
                    );
                    cx.notify();
                    return;
                }

                match result {
                    Ok(Some(formatted)) => {
                        let mapped_selection =
                            map_range_after_format(&source, &formatted, selection.clone());
                        let source_mode = this.mode.shows_source();
                        this.editor.update(cx, |editor, cx| {
                            let document_end =
                                editor.text().offset_to_offset_utf16(editor.text().len());
                            EntityInputHandler::replace_text_in_range(
                                editor,
                                Some(0..document_end),
                                &formatted,
                                window,
                                cx,
                            );
                            editor.set_selected_range(mapped_selection.clone(), cx);
                            if source_mode {
                                editor.focus(window, cx);
                            }
                        });

                        if vim_selection.is_some() {
                            this.finish_vim_visual_edit(mapped_selection.start, window, cx);
                        } else {
                            this.reset_vim_command();
                        }
                        window.push_notification(Notification::success("Document formatted."), cx);
                    }
                    Ok(None) => {
                        window.push_notification(
                            Notification::info("Document is already formatted."),
                            cx,
                        );
                    }
                    Err(error) => {
                        window.push_notification(
                            Notification::error(format!("Could not format document: {error}")),
                            cx,
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    pub(super) fn apply_format(
        &mut self,
        action: &ApplyMarkdownFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind != DocumentKind::Markdown || !self.mode.shows_source() {
            return;
        }

        let vim_range = self.vim_visual_range(cx);
        if action.0 == MarkdownFormat::Footnote {
            let insertion_range = vim_range
                .clone()
                .unwrap_or_else(|| self.editor.read(cx).selected_range());
            let cursor = self.insert_footnote(insertion_range, window, cx);
            if let (Some(cursor), Some(_)) = (cursor, vim_range) {
                self.finish_vim_visual_edit(cursor, window, cx);
            }
            return;
        }

        let selected = if let Some(range) = vim_range.as_ref() {
            self.editor.read(cx).text().slice(range.clone()).to_string()
        } else {
            self.editor.read(cx).selected_value().to_string()
        };
        let replacement = match action.0 {
            MarkdownFormat::HeadingOne => Self::prefix_block(&selected, "# ", "Heading"),
            MarkdownFormat::HeadingTwo => Self::prefix_block(&selected, "## ", "Heading"),
            MarkdownFormat::HeadingThree => Self::prefix_block(&selected, "### ", "Heading"),
            MarkdownFormat::HeadingFour => Self::prefix_block(&selected, "#### ", "Heading"),
            MarkdownFormat::HeadingFive => Self::prefix_block(&selected, "##### ", "Heading"),
            MarkdownFormat::HeadingSix => Self::prefix_block(&selected, "###### ", "Heading"),
            MarkdownFormat::Bold => Self::wrap_inline(&selected, "**", "**", "bold text"),
            MarkdownFormat::Italic => Self::wrap_inline(&selected, "*", "*", "italic text"),
            MarkdownFormat::InlineCode => Self::wrap_inline(&selected, "`", "`", "code"),
            MarkdownFormat::Link => Self::wrap_inline(&selected, "[", "](https://)", "link text"),
            MarkdownFormat::Task => format_task_lines(&selected),
            MarkdownFormat::Strikethrough => {
                Self::wrap_inline(&selected, "~~", "~~", "struck text")
            }
            MarkdownFormat::Highlight => Self::wrap_inline(
                &selected,
                r#"<mark style="background-color: #fef08a">"#,
                "</mark>",
                "highlighted text",
            ),
            MarkdownFormat::Footnote => return,
            MarkdownFormat::BulletList => Self::prefix_lines(&selected, "- ", "List item"),
            MarkdownFormat::OrderedList => Self::numbered_lines(&selected),
            MarkdownFormat::Quote => Self::prefix_lines(&selected, "> ", "Quote"),
            MarkdownFormat::CodeBlock => Self::code_block(&selected),
        };

        if let Some(range) = vim_range {
            self.editor.update(cx, |editor, cx| {
                let start = editor.text().offset_to_offset_utf16(range.start);
                let end = editor.text().offset_to_offset_utf16(range.end);
                EntityInputHandler::replace_text_in_range(
                    editor,
                    Some(start..end),
                    &replacement,
                    window,
                    cx,
                );
            });
            self.finish_vim_visual_edit(range.start, window, cx);
        } else {
            self.editor.update(cx, |editor, cx| {
                editor.replace(replacement, window, cx);
                editor.focus(window, cx);
            });
        }
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

pub(super) fn format_document_text(
    kind: DocumentKind,
    source: &str,
) -> Result<Option<String>, String> {
    match kind {
        DocumentKind::Markdown => format_markdown(source),
        DocumentKind::Json => format_json(source),
        DocumentKind::PlainText => Ok(None),
    }
}

fn format_markdown(source: &str) -> Result<Option<String>, String> {
    use dprint_plugin_markdown::configuration::{ConfigurationBuilder, TextWrap};

    let mut builder = ConfigurationBuilder::new();
    builder.line_width(80).text_wrap(TextWrap::Maintain);
    preserve_markdown_newlines(&mut builder, source);

    dprint_plugin_markdown::format_text(source, &builder.build(), |_, _, _| Ok(None))
        .map_err(|error| error.to_string())
}

fn format_json(source: &str) -> Result<Option<String>, String> {
    use dprint_plugin_json::configuration::{ConfigurationBuilder, TrailingCommaKind};

    let mut builder = ConfigurationBuilder::new();
    builder
        .line_width(80)
        .use_tabs(false)
        .indent_width(2)
        .array_prefer_single_line(true)
        .object_prefer_single_line(false)
        .trailing_commas(TrailingCommaKind::Never);
    preserve_json_newlines(&mut builder, source);

    let seeded_source = multiline_root_json_object(source);
    let format_source = seeded_source.as_deref().unwrap_or(source);
    let formatted = dprint_plugin_json::format_text(
        Path::new("document.json"),
        format_source,
        &builder.build(),
    )
    .map_err(|error| error.to_string())?;
    let formatted = match formatted {
        Some(formatted) => formatted,
        None => format_source.to_string(),
    };

    Ok((formatted != source).then_some(formatted))
}

fn multiline_root_json_object(source: &str) -> Option<String> {
    let (opening_index, opening) = source
        .char_indices()
        .find(|(_, character)| !character.is_whitespace() && *character != '\u{feff}')?;
    if opening != '{' {
        return None;
    }

    let content_end = source.trim_end_matches(char::is_whitespace).len();
    let inner = source.get(opening_index + opening.len_utf8()..content_end)?;
    if inner.contains('\n') || inner.trim_start().starts_with('}') {
        return None;
    }

    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut seeded = String::with_capacity(source.len() + newline.len());
    seeded.push_str(&source[..opening_index + opening.len_utf8()]);
    seeded.push_str(newline);
    seeded.push_str(&source[opening_index + opening.len_utf8()..]);
    Some(seeded)
}

fn preserve_markdown_newlines(
    builder: &mut dprint_plugin_markdown::configuration::ConfigurationBuilder,
    source: &str,
) {
    if source.contains("\r\n")
        && let Ok(kind) = "crlf".parse()
    {
        builder.new_line_kind(kind);
    }
}

fn preserve_json_newlines(
    builder: &mut dprint_plugin_json::configuration::ConfigurationBuilder,
    source: &str,
) {
    if source.contains("\r\n")
        && let Ok(kind) = "crlf".parse()
    {
        builder.new_line_kind(kind);
    }
}

pub(super) fn map_range_after_format(
    before: &str,
    after: &str,
    range: Range<usize>,
) -> Range<usize> {
    let start = map_offset_after_format(before, after, range.start);
    let end = map_offset_after_format(before, after, range.end);
    start.min(end)..start.max(end)
}

fn map_offset_after_format(before: &str, after: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(before, offset.min(before.len()));
    let prefix = common_prefix_len(before, after);
    if offset <= prefix {
        return offset;
    }

    let suffix = common_suffix_len(&before[prefix..], &after[prefix..]);
    if offset >= before.len().saturating_sub(suffix) {
        return after
            .len()
            .saturating_sub(before.len().saturating_sub(offset));
    }

    let significant_chars = before[..offset]
        .chars()
        .filter(|character| !character.is_whitespace())
        .count();
    if significant_chars == 0 {
        return 0;
    }

    let mut seen = 0;
    for (index, character) in after.char_indices() {
        if !character.is_whitespace() {
            seen += 1;
            if seen == significant_chars {
                return index + character.len_utf8();
            }
        }
    }

    after.len()
}

fn common_prefix_len(left: &str, right: &str) -> usize {
    let mut length = 0;
    for ((index, left_character), right_character) in left.char_indices().zip(right.chars()) {
        if left_character != right_character {
            break;
        }
        length = index + left_character.len_utf8();
    }
    length
}

fn common_suffix_len(left: &str, right: &str) -> usize {
    let mut length = 0;
    for (left_character, right_character) in left.chars().rev().zip(right.chars().rev()) {
        if left_character != right_character {
            break;
        }
        length += left_character.len_utf8();
    }
    length
}

fn floor_char_boundary(value: &str, mut index: usize) -> usize {
    while !value.is_char_boundary(index) {
        index = index.saturating_sub(1);
    }
    index
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

pub(super) fn markdown_newline_prefix(line: &str) -> String {
    match markdown_line_continuation(line.trim_end_matches(['\r', '\n'])) {
        Some(MarkdownLineContinuation::Continue(prefix)) => prefix,
        Some(MarkdownLineContinuation::Exit { .. }) | None => line
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .collect(),
    }
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
    fn smart_markdown_formats_cover_headings_tasks_and_inline_marks() {
        for (prefix, expected) in [
            ("# ", "# Heading"),
            ("## ", "## Heading"),
            ("### ", "### Heading"),
            ("#### ", "#### Heading"),
            ("##### ", "##### Heading"),
            ("###### ", "###### Heading"),
        ] {
            assert_eq!(
                DocumentEditorView::prefix_block("Heading", prefix, "Heading"),
                expected
            );
        }
        assert_eq!(
            format_task_lines("Draft\n- already"),
            "- [ ] Draft\n- [ ] already"
        );
        assert_eq!(
            DocumentEditorView::wrap_inline("important", "~~", "~~", "struck text"),
            "~~important~~"
        );
        assert_eq!(
            DocumentEditorView::wrap_inline(
                "important",
                r#"<mark style="background-color: #fef08a">"#,
                "</mark>",
                "highlighted text",
            ),
            r#"<mark style="background-color: #fef08a">important</mark>"#
        );
        assert_eq!(
            DocumentEditorView::wrap_inline(
                "first line\nsecond line",
                r#"<mark style="background-color: #fef08a">"#,
                "</mark>",
                "highlighted text",
            ),
            r#"<mark style="background-color: #fef08a">first line
second line</mark>"#
        );
    }

    #[test]
    fn formats_json_with_prettier_compatible_layout() {
        let source = r#"{"alpha":1,"array":[1,2,3],"nested":{"x":true,"y":"value"}}"#;
        let Ok(Some(formatted)) = format_json(source) else {
            panic!("unformatted valid JSON should produce formatted output");
        };

        assert_eq!(
            formatted,
            concat!(
                "{\n",
                "  \"alpha\": 1,\n",
                "  \"array\": [1, 2, 3],\n",
                "  \"nested\": { \"x\": true, \"y\": \"value\" }\n",
                "}\n"
            )
        );
    }

    #[test]
    fn rejects_invalid_json_without_replacement_text() {
        assert!(format_json(r#"{"unfinished": }"#).is_err());
    }

    #[test]
    fn formatters_preserve_crlf_documents() {
        let Ok(Some(json)) = format_json("{\"alpha\":1}\r\n") else {
            panic!("unformatted valid JSON should produce formatted output");
        };
        let Ok(Some(markdown)) = format_markdown("#  Heading\r\n\r\nText\r\n") else {
            panic!("unformatted Markdown should produce formatted output");
        };

        assert_eq!(json, "{\r\n  \"alpha\": 1\r\n}\r\n");
        assert_eq!(markdown, "# Heading\r\n\r\nText\r\n");
    }

    #[test]
    fn formats_gfm_without_changing_castle_wikilinks() {
        let source = concat!(
            "#  Title\n\n",
            "_some italic_ and __bold__\n\n",
            "* one\n* two\n\n",
            "[[Project/Note]]\n\n",
            "- [x] task\n\n",
            "| a|b |\n|--|--|\n|1|2|\n"
        );
        let Ok(Some(formatted)) = format_markdown(source) else {
            panic!("unformatted Markdown should produce formatted output");
        };

        assert_eq!(
            formatted,
            concat!(
                "# Title\n\n",
                "_some italic_ and **bold**\n\n",
                "- one\n- two\n\n",
                "[[Project/Note]]\n\n",
                "- [x] task\n\n",
                "| a | b |\n",
                "| - | - |\n",
                "| 1 | 2 |\n"
            )
        );
    }

    #[test]
    fn formatting_preserves_board_transclusion_bytes() {
        let block = "![[board:Roadmap#Current]]\n";
        let source = format!("#  Board context\n\n{block}");
        let Ok(Some(formatted)) = format_markdown(&source) else {
            panic!("heading should be formatted");
        };
        assert!(formatted.contains(block));
    }

    #[test]
    fn maps_cursor_to_the_same_json_content() {
        let before = r#"{"alpha":1,"beta":2}"#;
        let after = "{\n  \"alpha\": 1,\n  \"beta\": 2\n}\n";
        let before_offset = before.find('2').unwrap_or(before.len()) + 1;
        let after_offset = after.find('2').unwrap_or(after.len()) + 1;

        assert_eq!(
            map_offset_after_format(before, after, before_offset),
            after_offset
        );
    }

    #[test]
    fn maps_selection_at_unchanged_document_end() {
        let before = "#  Heading\nbody";
        let after = "# Heading\n\nbody\n";

        assert_eq!(
            map_range_after_format(before, after, before.len()..before.len()),
            after.len()..after.len()
        );
    }

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
    fn continues_all_markdown_list_markers_and_preserves_crlf() {
        assert_eq!(markdown_newline_prefix("* item"), "* ");
        assert_eq!(markdown_newline_prefix("+ item"), "+ ");
        assert_eq!(markdown_newline_prefix("3) item\r\n"), "4) ");
        assert_eq!(markdown_newline_prefix("- [X]\titem\r\n"), "- [ ] ");
        assert_eq!(
            markdown_enter_edit("+ item\r\n", "+ item\r\n".len()),
            Some(MarkdownEnterEdit {
                range: 8..8,
                replacement: "+ ".to_string(),
            })
        );
    }

    #[test]
    fn exits_empty_ordered_lists_and_nested_blockquotes() {
        assert_eq!(
            markdown_enter_edit("12) \n", "12) \n".len()),
            Some(MarkdownEnterEdit {
                range: 0..5,
                replacement: String::new(),
            })
        );
        assert_eq!(markdown_newline_prefix("> > quote"), "> > ");
        assert_eq!(
            markdown_enter_edit("> > \n", "> > \n".len()),
            Some(MarkdownEnterEdit {
                range: 0..5,
                replacement: String::new(),
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
