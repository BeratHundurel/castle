use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextObjectClass {
    HorizontalSpace,
    LineBreak,
    Word,
    Punctuation,
}

fn text_object_class(ch: char) -> TextObjectClass {
    if matches!(ch, '\r' | '\n') {
        TextObjectClass::LineBreak
    } else if ch.is_whitespace() {
        TextObjectClass::HorizontalSpace
    } else if ch.is_alphanumeric() || ch == '_' {
        TextObjectClass::Word
    } else {
        TextObjectClass::Punctuation
    }
}

pub(super) fn text_object_run(rope: &Rope, offset: usize) -> Range<usize> {
    if rope.len() == 0 {
        return 0..0;
    }
    let offset = if offset >= rope.len() {
        previous_boundary(rope, rope.len())
    } else {
        rope.floor_char_boundary(offset)
    };
    let Some(class) = rope.char_at(offset).map(text_object_class) else {
        return offset..offset;
    };
    if class == TextObjectClass::LineBreak {
        let ch = rope.char_at(offset);
        let start = if ch == Some('\n') && offset > 0 {
            let previous = previous_boundary(rope, offset);
            if rope.char_at(previous) == Some('\r') {
                previous
            } else {
                offset
            }
        } else {
            offset
        };
        let mut end = next_boundary(rope, offset);
        if ch == Some('\r') && rope.char_at(end) == Some('\n') {
            end = next_boundary(rope, end);
        }
        return start..end;
    }

    let mut start = offset;
    while start > 0 {
        let previous = previous_boundary(rope, start);
        if rope.char_at(previous).map(text_object_class) != Some(class) {
            break;
        }
        start = previous;
    }

    let mut end = next_boundary(rope, offset);
    while end < rope.len() && rope.char_at(end).map(text_object_class) == Some(class) {
        end = next_boundary(rope, end);
    }
    start..end
}

pub(super) fn next_non_space_run(rope: &Rope, offset: usize) -> Option<Range<usize>> {
    let mut cursor = offset;
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(|ch| ch.is_whitespace()) {
        cursor = next_boundary(rope, cursor);
    }
    (cursor < rope.len()).then(|| text_object_run(rope, cursor))
}

pub(super) fn previous_non_space_run(rope: &Rope, offset: usize) -> Option<Range<usize>> {
    if offset == 0 {
        return None;
    }
    let mut cursor = previous_boundary(rope, offset);
    while cursor > 0 && rope.char_at(cursor).is_some_and(|ch| ch.is_whitespace()) {
        cursor = previous_boundary(rope, cursor);
    }
    if rope.char_at(cursor).is_some_and(|ch| ch.is_whitespace()) {
        None
    } else {
        Some(text_object_run(rope, cursor))
    }
}

pub(super) fn extend_through_word_runs(
    rope: &Rope,
    mut range: Range<usize>,
    additional_runs: u32,
) -> Range<usize> {
    for _ in 0..additional_runs {
        let Some(next) = next_non_space_run(rope, range.end) else {
            break;
        };
        range.end = next.end;
    }
    range
}

pub(super) fn word_text_object_range(
    rope: &Rope,
    cursor: usize,
    count: u32,
    prefix: VimTextObjectPrefix,
) -> Range<usize> {
    let run = text_object_run(rope, cursor);
    if run.is_empty() {
        return run;
    }
    let class = rope.char_at(run.start).map(text_object_class);
    let count = count.max(1);

    if prefix == VimTextObjectPrefix::Inner {
        return extend_through_word_runs(rope, run, count.saturating_sub(1));
    }

    if matches!(
        class,
        Some(TextObjectClass::HorizontalSpace | TextObjectClass::LineBreak)
    ) {
        if let Some(next) = next_non_space_run(rope, run.end) {
            return extend_through_word_runs(rope, run.start..next.end, count.saturating_sub(1));
        }
        if let Some(previous) = previous_non_space_run(rope, run.start) {
            return previous.start..run.end;
        }
        return run;
    }

    let mut range = extend_through_word_runs(rope, run, count.saturating_sub(1));
    let mut trailing = range.end;
    while trailing < rope.len()
        && rope
            .char_at(trailing)
            .is_some_and(|ch| ch.is_whitespace() && !matches!(ch, '\r' | '\n'))
    {
        trailing = next_boundary(rope, trailing);
    }
    if trailing > range.end {
        range.end = trailing;
        return range;
    }

    let mut leading = range.start;
    while leading > 0 {
        let previous = previous_boundary(rope, leading);
        let Some(ch) = rope.char_at(previous) else {
            break;
        };
        if !ch.is_whitespace() || matches!(ch, '\r' | '\n') {
            break;
        }
        leading = previous;
    }
    range.start = leading;
    range
}

pub(super) fn is_text_object_key(key: VimKey) -> bool {
    matches!(
        key,
        VimKey::WordForward
            | VimKey::DoubleQuote
            | VimKey::SingleQuote
            | VimKey::Backtick
            | VimKey::Parenthesis
            | VimKey::ParenthesisClose
            | VimKey::Bracket
            | VimKey::BracketClose
            | VimKey::Brace
            | VimKey::BraceClose
    )
}

pub(super) fn text_object_range(
    rope: &Rope,
    cursor: usize,
    count: u32,
    prefix: VimTextObjectPrefix,
    key: VimKey,
) -> Range<usize> {
    match key {
        VimKey::WordForward => word_text_object_range(rope, cursor, count, prefix),
        VimKey::DoubleQuote => quote_text_object_range(rope, cursor, prefix, '"'),
        VimKey::SingleQuote => quote_text_object_range(rope, cursor, prefix, '\''),
        VimKey::Backtick => quote_text_object_range(rope, cursor, prefix, '`'),
        VimKey::Parenthesis | VimKey::ParenthesisClose => {
            pair_text_object_range(rope, cursor, prefix, '(', ')')
        }
        VimKey::Bracket | VimKey::BracketClose => {
            pair_text_object_range(rope, cursor, prefix, '[', ']')
        }
        VimKey::Brace | VimKey::BraceClose => {
            pair_text_object_range(rope, cursor, prefix, '{', '}')
        }
        _ => cursor..cursor,
    }
}

pub(super) fn quote_text_object_range(
    rope: &Rope,
    cursor: usize,
    prefix: VimTextObjectPrefix,
    quote: char,
) -> Range<usize> {
    let row = row_at(rope, cursor);
    let line_start = rope.line_start_offset(row);
    let line_end = line_content_end(rope, row);
    let mut opening = None;
    let mut offset = line_start;
    while offset < line_end {
        if rope.char_at(offset) == Some(quote) && !is_escaped(rope, offset, line_start) {
            if let Some(start) = opening.take() {
                if cursor >= start && cursor <= offset {
                    return if prefix == VimTextObjectPrefix::Inner {
                        next_boundary(rope, start)..offset
                    } else {
                        start..next_boundary(rope, offset)
                    };
                }
            } else {
                opening = Some(offset);
            }
        }
        offset = next_boundary(rope, offset);
    }
    cursor..cursor
}

pub(super) fn is_escaped(rope: &Rope, offset: usize, line_start: usize) -> bool {
    let mut slash_count = 0;
    let mut scan = offset;
    while scan > line_start {
        scan = previous_boundary(rope, scan);
        if rope.char_at(scan) != Some('\\') {
            break;
        }
        slash_count += 1;
    }
    slash_count % 2 == 1
}

pub(super) fn pair_text_object_range(
    rope: &Rope,
    cursor: usize,
    prefix: VimTextObjectPrefix,
    open: char,
    close: char,
) -> Range<usize> {
    if rope.len() == 0 {
        return 0..0;
    }
    let mut offset = if cursor >= rope.len() {
        previous_boundary(rope, rope.len())
    } else {
        rope.floor_char_boundary(cursor)
    };
    let mut depth = 0_u32;
    let opening = loop {
        match rope.char_at(offset) {
            Some(ch) if ch == close => depth = depth.saturating_add(1),
            Some(ch) if ch == open => {
                if depth == 0 {
                    break Some(offset);
                }
                depth -= 1;
                if depth == 0 {
                    break Some(offset);
                }
            }
            _ => {}
        }
        if offset == 0 {
            break None;
        }
        offset = previous_boundary(rope, offset);
    };
    let Some(opening) = opening else {
        return cursor..cursor;
    };

    depth = 0;
    offset = next_boundary(rope, opening);
    let closing = loop {
        if offset >= rope.len() {
            break None;
        }
        match rope.char_at(offset) {
            Some(ch) if ch == open => depth = depth.saturating_add(1),
            Some(ch) if ch == close => {
                if depth == 0 {
                    break Some(offset);
                }
                depth -= 1;
            }
            _ => {}
        }
        offset = next_boundary(rope, offset);
    };
    let Some(closing) = closing else {
        return cursor..cursor;
    };

    if prefix == VimTextObjectPrefix::Inner {
        next_boundary(rope, opening)..closing
    } else {
        opening..next_boundary(rope, closing)
    }
}
