use super::*;

pub(super) fn combined_operator_count(vim: &mut VimState) -> Option<u32> {
    let operator = vim.operator_count.take();
    let motion = vim.count.take();
    vim.pending_operator = None;
    match (operator, motion) {
        (None, None) => None,
        (Some(count), None) | (None, Some(count)) => Some(count),
        (Some(operator), Some(motion)) => Some(operator.saturating_mul(motion).min(MAX_COUNT)),
    }
}

pub(super) fn motion_for_key(
    rope: &Rope,
    cursor: usize,
    key: VimKey,
    count: Option<u32>,
    preferred_column: Option<u32>,
) -> Option<Motion> {
    let count_value = count.unwrap_or(1);
    let target = match key {
        VimKey::Left => {
            let line_start = rope.line_start_offset(row_at(rope, cursor));
            repeat_motion(cursor, count_value, |offset| {
                previous_boundary(rope, offset).max(line_start)
            })
        }
        VimKey::Right => repeat_motion(cursor, count_value, |offset| {
            next_boundary(rope, offset).min(normal_line_end(rope, row_at(rope, cursor)))
        }),
        VimKey::Down | VimKey::Up => {
            let row = row_at(rope, cursor);
            let delta = if key == VimKey::Down {
                i64::from(count_value)
            } else {
                -i64::from(count_value)
            };
            let target_row =
                (row as i64 + delta).clamp(0, rope.lines_len().saturating_sub(1) as i64) as usize;
            let column =
                preferred_column.unwrap_or_else(|| rope.offset_to_position(cursor).character);
            let target = rope.position_to_offset(&Position::new(target_row as u32, column));
            target.min(normal_line_end(rope, target_row))
        }
        VimKey::WordForward => {
            repeat_motion(cursor, count_value, |offset| next_word_start(rope, offset))
        }
        VimKey::WordBackward => repeat_motion(cursor, count_value, |offset| {
            previous_word_start(rope, offset)
        }),
        VimKey::WordEnd => repeat_motion(cursor, count_value, |offset| word_end(rope, offset)),
        VimKey::BigWordForward => repeat_motion(cursor, count_value, |offset| {
            next_big_word_start(rope, offset)
        }),
        VimKey::BigWordBackward => repeat_motion(cursor, count_value, |offset| {
            previous_big_word_start(rope, offset)
        }),
        VimKey::BigWordEnd => {
            repeat_motion(cursor, count_value, |offset| big_word_end(rope, offset))
        }
        VimKey::LineStart => rope.line_start_offset(row_at(rope, cursor)),
        VimKey::FirstNonBlank => first_non_blank(rope, cursor),
        VimKey::LineEnd => {
            let row = row_at(rope, cursor)
                .saturating_add(count_value as usize)
                .saturating_sub(1)
                .min(rope.lines_len().saturating_sub(1));
            normal_line_end(rope, row)
        }
        VimKey::Go => {
            let row = count
                .map(|count| (count as usize - 1).min(rope.lines_len().saturating_sub(1)))
                .unwrap_or(0);
            rope.line_start_offset(row)
        }
        VimKey::DocumentEnd => {
            let row = count
                .map(|count| (count as usize - 1).min(rope.lines_len().saturating_sub(1)))
                .unwrap_or_else(|| rope.lines_len().saturating_sub(1));
            rope.line_start_offset(row)
        }
        _ => return None,
    };
    Some(Motion {
        target,
        inclusive: matches!(key, VimKey::WordEnd | VimKey::BigWordEnd | VimKey::LineEnd),
        linewise: matches!(
            key,
            VimKey::Down | VimKey::Up | VimKey::Go | VimKey::DocumentEnd
        ),
    })
}

pub(super) fn repeat_motion(
    mut offset: usize,
    count: u32,
    mut motion: impl FnMut(usize) -> usize,
) -> usize {
    for _ in 0..count {
        let next = motion(offset);
        if next == offset {
            break;
        }
        offset = next;
    }
    offset
}

pub(super) fn operator_range(rope: &Rope, cursor: usize, motion: Motion) -> Range<usize> {
    if motion.target >= cursor {
        let end = if motion.inclusive {
            next_boundary(rope, motion.target)
        } else {
            motion.target
        };
        cursor..end
    } else {
        motion.target..cursor
    }
}

pub(super) fn linewise_motion_range(rope: &Rope, cursor: usize, target: usize) -> Range<usize> {
    let start_row = row_at(rope, cursor).min(row_at(rope, target));
    let end_row = row_at(rope, cursor).max(row_at(rope, target));
    line_rows_range(rope, start_row, end_row)
}

pub(super) fn line_count_range(rope: &Rope, cursor: usize, count: u32) -> Range<usize> {
    let start_row = row_at(rope, cursor);
    let end_row = start_row
        .saturating_add(count as usize)
        .saturating_sub(1)
        .min(rope.lines_len().saturating_sub(1));
    line_rows_range(rope, start_row, end_row)
}

pub(super) fn forward_char_range(rope: &Rope, cursor: usize, count: u32) -> Range<usize> {
    let start = cursor;
    let line_end = line_content_end(rope, row_at(rope, start));
    let end = repeat_motion(start, count, |offset| {
        next_boundary(rope, offset).min(line_end)
    });
    start..end
}

pub(super) fn backward_char_range(rope: &Rope, cursor: usize, count: u32) -> Range<usize> {
    let line_start = rope.line_start_offset(row_at(rope, cursor));
    let start = repeat_motion(cursor, count, |offset| {
        previous_boundary(rope, offset).max(line_start)
    });
    start..cursor
}

pub(super) fn join_line_edit(
    rope: &Rope,
    cursor: usize,
    count: u32,
) -> Option<(Range<usize>, String)> {
    let start_row = row_at(rope, cursor);
    if start_row + 1 >= rope.lines_len() {
        return None;
    }
    let end_row = start_row
        .saturating_add(count.max(2) as usize)
        .saturating_sub(1)
        .min(rope.lines_len().saturating_sub(1));

    let current_start = rope.line_start_offset(start_row);
    let mut range_start = line_content_end(rope, start_row);
    while range_start > current_start
        && rope
            .char_at(previous_boundary(rope, range_start))
            .is_some_and(|ch| matches!(ch, ' ' | '\t'))
    {
        range_start = previous_boundary(rope, range_start);
    }

    let range_end = line_content_end(rope, end_row);
    let mut joined = String::new();
    for row in start_row + 1..=end_row {
        let line = rope
            .slice(rope.line_start_offset(row)..line_content_end(rope, row))
            .to_string();
        let line = line.trim_matches([' ', '\t']);
        if line.is_empty() {
            continue;
        }
        if !joined.is_empty() {
            joined.push(' ');
        }
        joined.push_str(line);
    }
    if range_start > current_start && !joined.is_empty() {
        joined.insert(0, ' ');
    }
    Some((range_start..range_end, joined))
}

pub(super) fn line_rows_range(rope: &Rope, start_row: usize, end_row: usize) -> Range<usize> {
    let start = rope.line_start_offset(start_row);
    let end = if end_row + 1 < rope.lines_len() {
        rope.line_start_offset(end_row + 1)
    } else {
        rope.len()
    };
    start..end
}

pub(super) fn inclusive_range(rope: &Rope, anchor: usize, head: usize) -> Range<usize> {
    let start = anchor.min(head);
    let end = next_boundary(rope, anchor.max(head));
    start..end
}

pub(super) fn row_at(rope: &Rope, offset: usize) -> usize {
    rope.offset_to_point(offset.min(rope.len())).row
}

pub(super) fn line_content_end(rope: &Rope, row: usize) -> usize {
    let start = rope.line_start_offset(row);
    let mut end = rope.line_end_offset(row).min(rope.len());
    if end > start && rope.char_at(previous_boundary(rope, end)) == Some('\r') {
        end = previous_boundary(rope, end);
    }
    end
}

pub(super) fn line_break_after_row(rope: &Rope, row: usize) -> Option<&'static str> {
    if row + 1 >= rope.lines_len() {
        return None;
    }
    let offset = line_content_end(rope, row);
    match rope.char_at(offset) {
        Some('\r') if rope.char_at(next_boundary(rope, offset)) == Some('\n') => Some("\r\n"),
        Some('\n') => Some("\n"),
        _ => None,
    }
}

pub(super) fn line_break_for_row(rope: &Rope, row: usize) -> &'static str {
    if let Some(line_break) = line_break_after_row(rope, row) {
        return line_break;
    }
    for distance in 1..rope.lines_len() {
        if let Some(line_break) = row
            .checked_sub(distance)
            .and_then(|row| line_break_after_row(rope, row))
        {
            return line_break;
        }
        if let Some(line_break) = row
            .checked_add(distance)
            .filter(|row| *row < rope.lines_len())
            .and_then(|row| line_break_after_row(rope, row))
        {
            return line_break;
        }
    }
    "\n"
}

pub(super) fn normal_line_end(rope: &Rope, row: usize) -> usize {
    let start = rope.line_start_offset(row);
    let end = line_content_end(rope, row);
    if end > start {
        previous_boundary(rope, end)
    } else {
        start
    }
}

pub(super) fn first_non_blank(rope: &Rope, cursor: usize) -> usize {
    let row = row_at(rope, cursor);
    let start = rope.line_start_offset(row);
    let end = line_content_end(rope, row);
    let mut offset = start;
    while offset < end {
        match rope.char_at(offset) {
            Some(' ' | '\t') => offset = next_boundary(rope, offset),
            _ => break,
        }
    }
    if offset == end { start } else { offset }
}

pub(super) fn clamp_normal_offset(rope: &Rope, offset: usize) -> usize {
    let offset = offset.min(rope.len());
    let row = row_at(rope, offset);
    let start = rope.line_start_offset(row);
    let end = line_content_end(rope, row);
    if end > start {
        offset.min(previous_boundary(rope, end))
    } else {
        start
    }
}

pub(super) fn next_boundary(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len() {
        return rope.len();
    }
    rope.char_at(offset)
        .map_or(rope.len(), |ch| (offset + ch.len_utf8()).min(rope.len()))
}

pub(super) fn previous_boundary(rope: &Rope, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    rope.floor_char_boundary(offset.min(rope.len()).saturating_sub(1))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Space,
    Word,
    Punctuation,
}

fn char_class(ch: char) -> CharClass {
    if ch.is_whitespace() {
        CharClass::Space
    } else if ch.is_alphanumeric() || ch == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

pub(super) fn next_word_start(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len() {
        return rope.len();
    }
    let mut cursor = offset;
    let class = rope.char_at(cursor).map(char_class);
    while cursor < rope.len() && rope.char_at(cursor).map(char_class) == class {
        cursor = next_boundary(rope, cursor);
    }
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = next_boundary(rope, cursor);
    }
    cursor
}

pub(super) fn previous_word_start(rope: &Rope, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let mut cursor = previous_boundary(rope, offset);
    while cursor > 0 && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = previous_boundary(rope, cursor);
    }
    let class = rope.char_at(cursor).map(char_class);
    while cursor > 0 {
        let previous = previous_boundary(rope, cursor);
        if rope.char_at(previous).map(char_class) != class {
            break;
        }
        cursor = previous;
    }
    cursor
}

pub(super) fn word_end(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len() {
        return rope.len();
    }
    let mut cursor = offset;
    if let Some(class) = rope.char_at(cursor).map(char_class) {
        let next = next_boundary(rope, cursor);
        if next < rope.len() && rope.char_at(next).map(char_class) != Some(class) {
            cursor = next;
        }
    }
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = next_boundary(rope, cursor);
    }
    let class = rope.char_at(cursor).map(char_class);
    let mut end = cursor;
    while cursor < rope.len() && rope.char_at(cursor).map(char_class) == class {
        end = cursor;
        cursor = next_boundary(rope, cursor);
    }
    end
}

pub(super) fn next_big_word_start(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len() {
        return rope.len();
    }
    let mut cursor = offset;
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(|ch| !ch.is_whitespace()) {
        cursor = next_boundary(rope, cursor);
    }
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = next_boundary(rope, cursor);
    }
    cursor
}

pub(super) fn previous_big_word_start(rope: &Rope, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let mut cursor = previous_boundary(rope, offset);
    while cursor > 0 && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = previous_boundary(rope, cursor);
    }
    while cursor > 0 {
        let previous = previous_boundary(rope, cursor);
        if rope.char_at(previous).is_some_and(char::is_whitespace) {
            break;
        }
        cursor = previous;
    }
    cursor
}

pub(super) fn big_word_end(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len() {
        return rope.len();
    }
    let mut cursor = offset;
    let next = next_boundary(rope, cursor);
    if next < rope.len()
        && rope.char_at(cursor).is_some_and(|ch| !ch.is_whitespace())
        && rope.char_at(next).is_some_and(char::is_whitespace)
    {
        cursor = next;
    }
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(char::is_whitespace) {
        cursor = next_boundary(rope, cursor);
    }
    let mut end = cursor;
    while cursor < rope.len() && rope.char_at(cursor).is_some_and(|ch| !ch.is_whitespace()) {
        end = cursor;
        cursor = next_boundary(rope, cursor);
    }
    end
}
