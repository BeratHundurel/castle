use gpui_kit::component::input::RopeExt as _;
use gpui_kit::{Context, EntityInputHandler, Focusable as _, KeyDownEvent, Window};
use std::ops::Range;

use super::action::{MoveLineDown, MoveLineUp, ToggleTask};
use super::vim::VimMode;
use super::{DocumentEditorView, DocumentKind};

const APOSTROPHE: char = '\u{27}';
const BACKTICK: char = '\u{60}';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LineMoveDirection {
    Up,
    Down,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct LineMoveEdit {
    pub(super) replacement_range: Range<usize>,
    pub(super) replacement: String,
    pub(super) selected_range: Range<usize>,
    pub(super) cursor: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct TaskEdit {
    pub(super) replacement_range: Range<usize>,
    pub(super) replacement: String,
    pub(super) selected_range: Range<usize>,
    pub(super) cursor: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct FootnoteEdit {
    pub(super) replacement: String,
    pub(super) cursor: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct PairEdit {
    replacement_range: Range<usize>,
    replacement: String,
    cursor: usize,
}

impl DocumentEditorView {
    pub(super) fn on_smart_edit_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.source_editor_is_active(window, cx) || event.is_held {
            return;
        }

        let Some(character) = key_character(event) else {
            return;
        };

        if !is_smart_edit_character(character) {
            return;
        }

        let text = self.editor.read(cx).text().to_string();
        let selected_range = self.editor.read(cx).selected_range();
        let cursor = self.editor.read(cx).cursor().min(text.len());

        if let Some(closing) = closing_character(character)
            && selected_range.is_empty()
            && text
                .get(cursor..)
                .is_some_and(|remaining| remaining.starts_with(closing))
        {
            let next_cursor = cursor + closing.len_utf8();
            self.editor.update(cx, |editor, cx| {
                editor.set_selected_range(next_cursor..next_cursor, cx);
                editor.focus(window, cx);
            });
            window.prevent_default();
            cx.stop_propagation();
            return;
        }

        let Some(edit) = pair_edit(&text, selected_range, cursor, character) else {
            return;
        };

        self.editor.update(cx, |editor, cx| {
            let rope = editor.text();
            let start = rope.offset_to_offset_utf16(edit.replacement_range.start);
            let end = rope.offset_to_offset_utf16(edit.replacement_range.end);
            EntityInputHandler::replace_text_in_range(
                editor,
                Some(start..end),
                &edit.replacement,
                window,
                cx,
            );
            editor.set_selected_range(edit.cursor..edit.cursor, cx);
            editor.focus(window, cx);
        });
        window.prevent_default();
        cx.stop_propagation();
    }

    pub(super) fn on_action_move_line_up(
        &mut self,
        _: &MoveLineUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_current_lines(LineMoveDirection::Up, window, cx);
    }

    pub(super) fn on_action_move_line_down(
        &mut self,
        _: &MoveLineDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_current_lines(LineMoveDirection::Down, window, cx);
    }

    fn move_current_lines(
        &mut self,
        direction: LineMoveDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.source_editor_is_active(window, cx) {
            return;
        }

        let text = self.editor.read(cx).text().to_string();
        let selected_range = self.editor.read(cx).selected_range();
        let cursor = self.editor.read(cx).cursor();
        let Some(edit) = move_line_edit(&text, selected_range, cursor, direction) else {
            return;
        };

        self.editor.update(cx, |editor, cx| {
            let rope = editor.text();
            let start = rope.offset_to_offset_utf16(edit.replacement_range.start);
            let end = rope.offset_to_offset_utf16(edit.replacement_range.end);
            EntityInputHandler::replace_text_in_range(
                editor,
                Some(start..end),
                &edit.replacement,
                window,
                cx,
            );
            let selection = if edit.selected_range.is_empty() {
                edit.cursor..edit.cursor
            } else {
                edit.selected_range.clone()
            };
            editor.set_selected_range(selection, cx);
            editor.focus(window, cx);
        });
    }

    pub(super) fn on_action_toggle_task(
        &mut self,
        _: &ToggleTask,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind != DocumentKind::Markdown || !self.source_editor_is_active(window, cx) {
            return;
        }

        let text = self.editor.read(cx).text().to_string();
        let selected_range = self.editor.read(cx).selected_range();
        let cursor = self.editor.read(cx).cursor();
        let Some(edit) = task_toggle_edit(&text, selected_range, cursor) else {
            return;
        };

        self.editor.update(cx, |editor, cx| {
            let rope = editor.text();
            let start = rope.offset_to_offset_utf16(edit.replacement_range.start);
            let end = rope.offset_to_offset_utf16(edit.replacement_range.end);
            EntityInputHandler::replace_text_in_range(
                editor,
                Some(start..end),
                &edit.replacement,
                window,
                cx,
            );
            let selection = if edit.selected_range.is_empty() {
                edit.cursor..edit.cursor
            } else {
                edit.selected_range.clone()
            };
            editor.set_selected_range(selection, cx);
            editor.focus(window, cx);
        });
    }

    pub(super) fn insert_footnote(
        &mut self,
        selection: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        if self.kind != DocumentKind::Markdown || !self.mode.shows_source() {
            return None;
        }

        let text = self.editor.read(cx).text().to_string();
        let edit = insert_footnote_edit(&text, selection)?;
        let cursor = edit.cursor;

        self.editor.update(cx, |editor, cx| {
            let end = editor.text().offset_to_offset_utf16(editor.text().len());
            EntityInputHandler::replace_text_in_range(
                editor,
                Some(0..end),
                &edit.replacement,
                window,
                cx,
            );
            editor.set_selected_range(cursor..cursor, cx);
            editor.focus(window, cx);
        });

        Some(cursor)
    }

    fn source_editor_is_active(&self, window: &Window, cx: &mut Context<Self>) -> bool {
        self.mode.shows_source()
            && !self.persistence.is_loading
            && !(self.vim_is_enabled() && self.vim_mode() != VimMode::Insert)
            && self.editor.focus_handle(cx).is_focused(window)
    }
}

fn key_character(event: &KeyDownEvent) -> Option<char> {
    event
        .keystroke
        .key_char
        .as_deref()
        .and_then(single_character)
        .or_else(|| single_character(&event.keystroke.key))
}

fn single_character(value: &str) -> Option<char> {
    let mut characters = value.chars();
    let character = characters.next()?;
    characters.next().is_none().then_some(character)
}

fn pair_for_character(character: char) -> Option<(char, char)> {
    match character {
        '(' => Some(('(', ')')),
        '[' => Some(('[', ']')),
        '{' => Some(('{', '}')),
        '"' => Some(('"', '"')),
        APOSTROPHE => Some((APOSTROPHE, APOSTROPHE)),
        BACKTICK => Some((BACKTICK, BACKTICK)),
        _ => None,
    }
}

fn closing_character(character: char) -> Option<char> {
    match character {
        ')' | ']' | '}' | '"' => Some(character),
        APOSTROPHE | BACKTICK => Some(character),
        _ => None,
    }
}

fn is_smart_edit_character(character: char) -> bool {
    pair_for_character(character).is_some() || closing_character(character).is_some()
}

fn should_pair_character(text: &str, cursor: usize, character: char, has_selection: bool) -> bool {
    if has_selection || (character != '"' && character != APOSTROPHE) {
        return true;
    }

    text.get(..cursor)
        .and_then(|before| before.chars().next_back())
        .is_none_or(|previous| !previous.is_alphanumeric())
}

fn pair_edit(
    text: &str,
    selected_range: Range<usize>,
    cursor: usize,
    character: char,
) -> Option<PairEdit> {
    let (opening, closing) = pair_for_character(character)?;
    if !should_pair_character(text, cursor, character, !selected_range.is_empty()) {
        return None;
    }

    let selected = text.get(selected_range.clone())?;
    let replacement = format!("{opening}{selected}{closing}");
    let cursor = selected_range.start + opening.len_utf8() + selected.len();

    Some(PairEdit {
        replacement_range: selected_range,
        replacement,
        cursor,
    })
}

pub(super) fn smart_link_target(text: &str) -> Option<&str> {
    let target = text.trim();
    let (scheme, remainder) = target.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    if remainder.is_empty()
        || remainder.starts_with('/')
        || remainder
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return None;
    }

    Some(target)
}

pub(super) fn markdown_link_for_paste(selected: &str, target: &str) -> Option<String> {
    let target = smart_link_target(target)?;
    let label = selected.trim();
    if label.is_empty() || label.contains('\r') || label.contains('\n') {
        return None;
    }

    let leading_len = selected.len() - selected.trim_start().len();
    let trailing_len = selected.len() - selected.trim_end().len();
    let trailing_start = selected.len().saturating_sub(trailing_len);
    let link = format!("[{label}]({target})");

    Some(format!(
        "{}{}{}",
        &selected[..leading_len],
        link,
        &selected[trailing_start..]
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineSpan {
    start: usize,
    content_end: usize,
    end: usize,
}

fn line_spans(text: &str) -> Vec<LineSpan> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut line_start = 0;
    let mut cursor = 0;

    while cursor < bytes.len() {
        let separator_len = match bytes[cursor] {
            b'\n' => Some(1),
            b'\r' => Some(if bytes.get(cursor + 1) == Some(&b'\n') {
                2
            } else {
                1
            }),
            _ => None,
        };

        if let Some(separator_len) = separator_len {
            spans.push(LineSpan {
                start: line_start,
                content_end: cursor,
                end: cursor + separator_len,
            });
            cursor += separator_len;
            line_start = cursor;
            continue;
        }

        let Some(character) = text[cursor..].chars().next() else {
            break;
        };
        cursor += character.len_utf8();
    }

    spans.push(LineSpan {
        start: line_start,
        content_end: text.len(),
        end: text.len(),
    });
    spans
}

fn line_index_at_offset(spans: &[LineSpan], offset: usize) -> usize {
    spans
        .iter()
        .position(|span| offset < span.end)
        .unwrap_or_else(|| spans.len().saturating_sub(1))
}

fn move_line_edit(
    text: &str,
    selection: Range<usize>,
    cursor: usize,
    direction: LineMoveDirection,
) -> Option<LineMoveEdit> {
    let spans = line_spans(text);
    let selection_start = selection.start.min(text.len());
    let selection_end = selection.end.min(text.len()).max(selection_start);
    let start_line = line_index_at_offset(&spans, selection_start);
    let end_probe = if selection_start == selection_end {
        selection_end
    } else {
        selection_end.saturating_sub(1)
    };
    let end_line = line_index_at_offset(&spans, end_probe);
    let trailing_empty_line = spans
        .last()
        .is_some_and(|span| span.start == text.len() && span.content_end == text.len());
    if trailing_empty_line
        && (start_line == spans.len().saturating_sub(1)
            || (direction == LineMoveDirection::Down
                && end_line + 1 == spans.len().saturating_sub(1)))
    {
        return None;
    }

    let (replacement_range, content_order, separator_start, separator_end, forward) =
        match direction {
            LineMoveDirection::Up if start_line > 0 => {
                let previous_line = start_line - 1;
                let mut order = (start_line..=end_line).collect::<Vec<_>>();
                order.push(previous_line);
                (
                    spans[previous_line].start..spans[end_line].end,
                    order,
                    previous_line,
                    end_line,
                    false,
                )
            }
            LineMoveDirection::Down if end_line + 1 < spans.len() => {
                let next_line = end_line + 1;
                let mut order = vec![next_line];
                order.extend(start_line..=end_line);
                (
                    spans[start_line].start..spans[next_line].end,
                    order,
                    start_line,
                    next_line,
                    true,
                )
            }
            _ => return None,
        };

    let mut replacement = String::new();
    for (content_line, separator_line) in content_order
        .iter()
        .copied()
        .zip(separator_start..=separator_end)
    {
        replacement.push_str(text.get(spans[content_line].start..spans[content_line].content_end)?);
        replacement
            .push_str(text.get(spans[separator_line].content_end..spans[separator_line].end)?);
    }

    let offset = if forward {
        let moved_content_len = spans[separator_end].content_end - spans[separator_end].start;
        let separator_len = spans[separator_start].end - spans[separator_start].content_end;
        moved_content_len + separator_len
    } else {
        spans[start_line].start - spans[separator_start].start
    };
    let map_offset = |value: usize| {
        if forward {
            value.saturating_add(offset)
        } else {
            value.saturating_sub(offset)
        }
    };
    let moved_selection_end =
        if selection_start != selection_end && selection_end == spans[end_line].end {
            if forward {
                replacement_range.end
            } else {
                let selected_content_len = (start_line..=end_line)
                    .map(|line| spans[line].content_end - spans[line].start)
                    .sum::<usize>();
                let selected_separator_len = (separator_start..separator_end)
                    .map(|line| spans[line].end - spans[line].content_end)
                    .sum::<usize>();
                replacement_range.start + selected_content_len + selected_separator_len
            }
        } else {
            map_offset(selection_end)
        };

    Some(LineMoveEdit {
        replacement_range,
        replacement,
        selected_range: map_offset(selection_start)..moved_selection_end,
        cursor: map_offset(cursor.min(text.len())),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TaskMarker {
    checkbox_index: usize,
    checked: bool,
}

fn markdown_indent_end(line: &str) -> usize {
    line.char_indices()
        .find(|(_, character)| *character != ' ' && *character != '\t')
        .map_or(line.len(), |(index, _)| index)
}

fn bullet_marker_end(line: &str) -> Option<usize> {
    let indent_end = markdown_indent_end(line);
    let bullet = line[indent_end..].chars().next()?;
    if !matches!(bullet, '-' | '*' | '+') {
        return None;
    }

    let marker_end = indent_end + bullet.len_utf8();
    let mut whitespace_end = marker_end;
    while let Some(character) = line[whitespace_end..].chars().next() {
        if !matches!(character, ' ' | '\t') {
            break;
        }
        whitespace_end += character.len_utf8();
    }

    (whitespace_end > marker_end || whitespace_end == line.len()).then_some(whitespace_end)
}

fn task_marker(line: &str) -> Option<TaskMarker> {
    let marker_end = bullet_marker_end(line)?;
    let bytes = line.as_bytes();
    if bytes.get(marker_end) != Some(&b'[')
        || !matches!(bytes.get(marker_end + 1), Some(b' ' | b'x' | b'X'))
        || bytes.get(marker_end + 2) != Some(&b']')
    {
        return None;
    }

    let content_start = marker_end + 3;
    if let Some(character) = line[content_start..].chars().next()
        && !matches!(character, ' ' | '\t')
    {
        return None;
    }

    Some(TaskMarker {
        checkbox_index: marker_end + 1,
        checked: matches!(bytes.get(marker_end + 1), Some(b'x' | b'X')),
    })
}

fn format_task_line(line: &str) -> String {
    if task_marker(line).is_some() {
        return line.to_string();
    }

    if let Some(marker_end) = bullet_marker_end(line) {
        let separator = if !line[..marker_end]
            .chars()
            .last()
            .is_some_and(|character| character.is_whitespace())
        {
            " "
        } else {
            ""
        };
        return format!(
            "{}{separator}[ ] {}",
            &line[..marker_end],
            &line[marker_end..]
        );
    }

    let indent_end = markdown_indent_end(line);
    format!("{}- [ ] {}", &line[..indent_end], &line[indent_end..])
}

pub(super) fn format_task_lines(selected: &str) -> String {
    if selected.is_empty() {
        return "- [ ] Task".to_string();
    }

    let spans = line_spans(selected);
    let mut formatted = String::new();
    for span in spans {
        if span.start == selected.len() && span.end == selected.len() {
            continue;
        }
        let Some(line) = selected.get(span.start..span.content_end) else {
            continue;
        };
        if line.trim().is_empty() {
            formatted.push_str(line);
        } else {
            formatted.push_str(&format_task_line(line));
        }
        if let Some(separator) = selected.get(span.content_end..span.end) {
            formatted.push_str(separator);
        }
    }
    formatted
}

fn map_offset_after_insert(offset: usize, insertion: usize, amount: usize) -> usize {
    if offset < insertion {
        offset
    } else {
        offset.saturating_add(amount)
    }
}

fn task_toggle_edit(text: &str, selection: Range<usize>, cursor: usize) -> Option<TaskEdit> {
    let spans = line_spans(text);
    let line_index = line_index_at_offset(&spans, cursor.min(text.len()));
    let span = spans.get(line_index).copied()?;
    let line = text.get(span.start..span.content_end)?;

    let (replacement, insertion) = if let Some(marker) = task_marker(line) {
        let mut replacement = line.to_string();
        let character = if marker.checked { ' ' } else { 'x' };
        replacement.replace_range(
            marker.checkbox_index..marker.checkbox_index + 1,
            &character.to_string(),
        );
        (replacement, None)
    } else {
        let replacement = format_task_line(line);
        let insertion = if let Some(marker_end) = bullet_marker_end(line) {
            marker_end
        } else {
            markdown_indent_end(line)
        };
        (replacement, Some(insertion))
    };

    let absolute_insertion = insertion.map(|offset| span.start + offset);
    let amount = replacement.len().saturating_sub(line.len());
    let map_offset = |offset: usize| {
        if let Some(insertion) = absolute_insertion {
            map_offset_after_insert(offset, insertion, amount)
        } else {
            offset
        }
    };

    Some(TaskEdit {
        replacement_range: span.start..span.content_end,
        replacement,
        selected_range: map_offset(selection.start.min(text.len()))
            ..map_offset(selection.end.min(text.len())),
        cursor: map_offset(cursor.min(text.len())),
    })
}

fn next_footnote_id(text: &str) -> Option<usize> {
    let mut id = 1usize;
    loop {
        let reference = format!("[^{id}]");
        let definition = format!("[^{id}]:");
        if !text.contains(&reference) && !text.contains(&definition) {
            return Some(id);
        }
        if id == usize::MAX {
            return None;
        }
        id += 1;
    }
}

fn footnote_separator(source: &str, newline: &str) -> String {
    let blank_line = format!("{newline}{newline}");
    if source.ends_with(&blank_line) {
        String::new()
    } else if source.ends_with(newline) {
        newline.to_string()
    } else {
        blank_line
    }
}

pub(super) fn insert_footnote_edit(text: &str, selection: Range<usize>) -> Option<FootnoteEdit> {
    let insertion = selection.end.min(text.len());
    if !text.is_char_boundary(insertion) {
        return None;
    }

    let id = next_footnote_id(text)?;
    let reference = format!("[^{id}]");
    let mut replacement = String::with_capacity(text.len() + reference.len() + 16);
    replacement.push_str(text.get(..insertion)?);
    replacement.push_str(&reference);
    replacement.push_str(text.get(insertion..)?);

    let newline = if replacement.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let separator = footnote_separator(&replacement, newline);
    replacement.push_str(&separator);
    replacement.push_str(&format!("[^{id}]: "));

    Some(FootnoteEdit {
        replacement,
        cursor: insertion + reference.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_pairs_and_closers() {
        assert_eq!(pair_for_character('('), Some(('(', ')')));
        assert_eq!(pair_for_character(BACKTICK), Some((BACKTICK, BACKTICK)));
        assert_eq!(pair_for_character(')'), None);
        assert_eq!(closing_character(']'), Some(']'));
        assert_eq!(closing_character('a'), None);
    }

    #[test]
    fn only_delimiters_need_smart_edit_document_context() {
        assert!(!is_smart_edit_character('a'));
        assert!(!is_smart_edit_character(' '));
        assert!(is_smart_edit_character('('));
        assert!(is_smart_edit_character(']'));
    }

    #[test]
    fn avoids_pairing_apostrophes_inside_words() {
        assert!(!should_pair_character("don", 3, APOSTROPHE, false));
        assert!(should_pair_character("word ", 5, APOSTROPHE, false));
        assert!(should_pair_character("word", 4, APOSTROPHE, true));
    }

    #[test]
    fn pairs_a_unicode_selection_and_places_the_cursor_inside_it() {
        assert_eq!(
            pair_edit("a中b", 1..4, 4, '('),
            Some(PairEdit {
                replacement_range: 1..4,
                replacement: "(中)".to_string(),
                cursor: 5,
            })
        );
    }

    #[test]
    fn rejects_non_boundary_pair_selections_without_panicking() {
        assert_eq!(pair_edit("a中b", 2..4, 4, '('), None);
        assert_eq!(pair_edit("don't", 5..5, 5, APOSTROPHE), None);
    }

    #[test]
    fn recognizes_http_targets_and_trims_clipboard_newlines() {
        assert_eq!(
            smart_link_target("  https://example.com/docs\n"),
            Some("https://example.com/docs")
        );
        assert_eq!(smart_link_target("ftp://example.com"), None);
        assert_eq!(smart_link_target("https:///docs"), None);
        assert_eq!(smart_link_target("https://example.com/a b"), None);
    }

    #[test]
    fn creates_links_from_selected_text_and_preserves_padding() {
        assert_eq!(
            markdown_link_for_paste("  Castle  ", "https://example.com"),
            Some("  [Castle](https://example.com)  ".to_string())
        );
        assert_eq!(
            markdown_link_for_paste("two\nlines", "https://example.com"),
            None
        );
        assert_eq!(
            markdown_link_for_paste("label", " HTTPS://example.com/a?x=1&y=2\r\n"),
            Some("[label](HTTPS://example.com/a?x=1&y=2)".to_string())
        );
        assert_eq!(markdown_link_for_paste("", "https://example.com"), None);
        assert_eq!(markdown_link_for_paste("label", "https://"), None);
    }

    #[test]
    fn moves_a_line_up_and_keeps_the_cursor_with_it() {
        let edit = move_line_edit("one\ntwo\nthree", 4..4, 5, LineMoveDirection::Up);
        assert_eq!(
            edit,
            Some(LineMoveEdit {
                replacement_range: 0..8,
                replacement: "two\none\n".to_string(),
                selected_range: 0..0,
                cursor: 1,
            })
        );
    }

    #[test]
    fn moves_a_line_down_without_joining_the_last_line() {
        let edit = move_line_edit("one\ntwo\nthree", 4..4, 5, LineMoveDirection::Down);
        assert_eq!(
            edit,
            Some(LineMoveEdit {
                replacement_range: 4..13,
                replacement: "three\ntwo".to_string(),
                selected_range: 10..10,
                cursor: 11,
            })
        );
    }

    #[test]
    fn moves_selected_lines_as_a_block() {
        let edit = move_line_edit("one\ntwo\nthree\nfour", 4..14, 13, LineMoveDirection::Down);
        assert_eq!(
            edit,
            Some(LineMoveEdit {
                replacement_range: 4..18,
                replacement: "four\ntwo\nthree".to_string(),
                selected_range: 9..18,
                cursor: 18,
            })
        );
    }

    #[test]
    fn preserves_crlf_when_moving_lines() {
        let edit = move_line_edit("one\r\ntwo\r\nthree", 5..5, 7, LineMoveDirection::Down);
        assert_eq!(
            edit,
            Some(LineMoveEdit {
                replacement_range: 5..15,
                replacement: "three\r\ntwo".to_string(),
                selected_range: 12..12,
                cursor: 14,
            })
        );
    }

    #[test]
    fn moves_unicode_lines_and_keeps_the_cursor_at_the_same_content() {
        assert_eq!(
            move_line_edit("α\n中\nlast", 3..3, 6, LineMoveDirection::Up),
            Some(LineMoveEdit {
                replacement_range: 0..7,
                replacement: "中\nα\n".to_string(),
                selected_range: 0..0,
                cursor: 3,
            })
        );
    }

    #[test]
    fn moves_a_line_with_mixed_separators_without_dropping_bytes() {
        let edit = move_line_edit("first\r\nsecond\nthird", 7..7, 10, LineMoveDirection::Up)
            .expect("the middle line should move up");
        let moved = format!(
            "{}{}{}",
            &"first\r\nsecond\nthird"[..edit.replacement_range.start],
            edit.replacement,
            &"first\r\nsecond\nthird"[edit.replacement_range.end..]
        );

        assert_eq!(moved, "second\r\nfirst\nthird");
        assert_eq!(edit.cursor, 3);
    }

    #[test]
    fn does_not_move_at_document_edges() {
        assert_eq!(
            move_line_edit("one\ntwo", 0..0, 0, LineMoveDirection::Up),
            None
        );
        assert_eq!(
            move_line_edit("one\ntwo", 4..4, 4, LineMoveDirection::Down),
            None
        );
        assert_eq!(
            move_line_edit("one\ntwo\n", 4..4, 4, LineMoveDirection::Down),
            None
        );
    }

    #[test]
    fn formats_tasks_without_duplicating_existing_task_markers() {
        assert_eq!(
            format_task_lines("Write\n- already"),
            "- [ ] Write\n- [ ] already"
        );
        assert_eq!(format_task_lines("- [x] done"), "- [x] done");
        assert_eq!(format_task_lines("-"), "- [ ] ");
        assert_eq!(format_task_lines("- "), "- [ ] ");
    }

    #[test]
    fn preserves_task_marker_style_indentation_and_crlf() {
        assert_eq!(
            format_task_lines("  * first\r\n\t+ [X]\tsecond\r\n\r\nthird"),
            "  * [ ] first\r\n\t+ [X]\tsecond\r\n\r\n- [ ] third"
        );
    }

    #[test]
    fn toggles_a_task_and_can_promote_a_plain_line() {
        let checked = task_toggle_edit("- [ ] draft", 5..5, 5);
        assert_eq!(
            checked,
            Some(TaskEdit {
                replacement_range: 0..11,
                replacement: "- [x] draft".to_string(),
                selected_range: 5..5,
                cursor: 5,
            })
        );

        assert_eq!(
            task_toggle_edit("  * [X]\tDone", 7..7, 7),
            Some(TaskEdit {
                replacement_range: 0..12,
                replacement: "  * [ ]\tDone".to_string(),
                selected_range: 7..7,
                cursor: 7,
            })
        );

        let promoted = task_toggle_edit("draft", 5..5, 5);
        assert_eq!(
            promoted,
            Some(TaskEdit {
                replacement_range: 0..5,
                replacement: "- [ ] draft".to_string(),
                selected_range: 11..11,
                cursor: 11,
            })
        );
    }

    #[test]
    fn inserts_unique_footnotes_after_the_selection() {
        let edit = insert_footnote_edit("Read this", 5..9);
        assert_eq!(
            edit,
            Some(FootnoteEdit {
                replacement: "Read this[^1]\n\n[^1]: ".to_string(),
                cursor: 13,
            })
        );

        let edit = insert_footnote_edit("Existing[^1]\n\n[^1]: source", 0..0);
        assert_eq!(
            edit,
            Some(FootnoteEdit {
                replacement: "[^2]Existing[^1]\n\n[^1]: source\n\n[^2]: ".to_string(),
                cursor: 4,
            })
        );
    }

    #[test]
    fn preserves_crlf_for_footnote_definitions() {
        let edit = insert_footnote_edit("Read\r\n", 4..4);
        assert_eq!(
            edit,
            Some(FootnoteEdit {
                replacement: "Read[^1]\r\n\r\n[^1]: ".to_string(),
                cursor: 8,
            })
        );
    }

    #[test]
    fn inserts_a_footnote_after_multibyte_selected_text() {
        assert_eq!(
            insert_footnote_edit("Café", 3..5),
            Some(FootnoteEdit {
                replacement: "Café[^1]\n\n[^1]: ".to_string(),
                cursor: 9,
            })
        );
    }
}
