use super::*;

pub(super) fn vim_find_kind_for_key(key: VimKey) -> Option<VimFindKind> {
    match key {
        VimKey::FindForward => Some(VimFindKind::Forward),
        VimKey::FindBackward => Some(VimFindKind::Backward),
        VimKey::TillForward => Some(VimFindKind::TillForward),
        VimKey::TillBackward => Some(VimFindKind::TillBackward),
        _ => None,
    }
}

pub(super) fn vim_literal_for_key(key: VimKey) -> Option<String> {
    let literal = match key {
        VimKey::Digit(digit) => {
            return char::from_digit(u32::from(digit), 10).map(|ch| ch.to_string());
        }
        VimKey::Left => "h",
        VimKey::Down => "j",
        VimKey::Up => "k",
        VimKey::Right => "l",
        VimKey::WordForward => "w",
        VimKey::WordBackward => "b",
        VimKey::WordEnd => "e",
        VimKey::BigWordForward => "W",
        VimKey::BigWordBackward => "B",
        VimKey::BigWordEnd => "E",
        VimKey::FindForward => "f",
        VimKey::FindBackward => "F",
        VimKey::TillForward => "t",
        VimKey::TillBackward => "T",
        VimKey::RepeatFind => ";",
        VimKey::RepeatFindReverse => ",",
        VimKey::LiteralEnter => "\n",
        VimKey::LiteralTab => "\t",
        VimKey::LiteralSpace => " ",
        VimKey::FirstNonBlank => "^",
        VimKey::LineEnd => "$",
        VimKey::Go => "g",
        VimKey::DocumentEnd => "G",
        VimKey::Insert => "i",
        VimKey::Append => "a",
        VimKey::InsertLineStart => "I",
        VimKey::AppendLineEnd => "A",
        VimKey::OpenBelow => "o",
        VimKey::OpenAbove => "O",
        VimKey::Visual => "v",
        VimKey::VisualLine => "V",
        VimKey::DoubleQuote => "\"",
        VimKey::SingleQuote => "'",
        VimKey::Backtick => "`",
        VimKey::Parenthesis => "(",
        VimKey::ParenthesisClose => ")",
        VimKey::Bracket => "[",
        VimKey::BracketClose => "]",
        VimKey::Brace => "{",
        VimKey::BraceClose => "}",
        VimKey::DeleteChar => "x",
        VimKey::DeletePreviousChar => "X",
        VimKey::SubstituteChar => "s",
        VimKey::SubstituteLine => "S",
        VimKey::ReplaceChar => "r",
        VimKey::YankLine => "Y",
        VimKey::JoinLines => "J",
        VimKey::Delete => "d",
        VimKey::Yank => "y",
        VimKey::Change => "c",
        VimKey::DeleteToLineEnd => "D",
        VimKey::ChangeToLineEnd => "C",
        VimKey::PasteAfter => "p",
        VimKey::PasteBefore => "P",
        VimKey::Undo => "u",
        VimKey::RepeatLastChange => ".",
        VimKey::LineStart | VimKey::Redo | VimKey::Search | VimKey::Escape => return None,
    };
    Some(literal.to_string())
}

pub(super) fn target_matches(rope: &Rope, offset: usize, line_end: usize, target: &str) -> bool {
    let end = offset.saturating_add(target.len());
    end <= line_end
        && rope.is_char_boundary(offset)
        && rope.is_char_boundary(end)
        && rope.slice(offset..end) == target
}

pub(super) fn find_forward_occurrence(
    rope: &Rope,
    mut offset: usize,
    line_end: usize,
    target: &str,
) -> Option<usize> {
    while offset < line_end {
        if target_matches(rope, offset, line_end, target) {
            return Some(offset);
        }
        offset = next_boundary(rope, offset);
    }
    None
}

pub(super) fn find_backward_occurrence(
    rope: &Rope,
    line_start: usize,
    before: usize,
    target: &str,
) -> Option<usize> {
    let mut offset = line_start;
    let mut found = None;
    while offset < before {
        if target_matches(rope, offset, before, target) {
            found = Some(offset);
        }
        offset = next_boundary(rope, offset);
    }
    found
}

pub(super) fn find_char_motion(
    rope: &Rope,
    cursor: usize,
    kind: VimFindKind,
    target: &str,
    count: u32,
    repeating: bool,
) -> Option<Motion> {
    if target.is_empty() || target.contains(['\r', '\n']) {
        return None;
    }
    let row = row_at(rope, cursor);
    let line_start = rope.line_start_offset(row);
    let line_end = line_content_end(rope, row);
    let mut occurrence = None;

    match kind {
        VimFindKind::Forward | VimFindKind::TillForward => {
            let mut search_start = next_boundary(rope, cursor).min(line_end);
            for index in 0..count.max(1) {
                let mut found = find_forward_occurrence(rope, search_start, line_end, target)?;
                if repeating
                    && index == 0
                    && kind == VimFindKind::TillForward
                    && found == search_start
                {
                    search_start = found.saturating_add(target.len()).min(line_end);
                    found = find_forward_occurrence(rope, search_start, line_end, target)?;
                }
                occurrence = Some(found);
                search_start = found.saturating_add(target.len()).min(line_end);
            }
        }
        VimFindKind::Backward | VimFindKind::TillBackward => {
            let mut before = cursor;
            for index in 0..count.max(1) {
                let mut found = find_backward_occurrence(rope, line_start, before, target)?;
                if repeating
                    && index == 0
                    && kind == VimFindKind::TillBackward
                    && found.saturating_add(target.len()) == before
                {
                    before = found;
                    found = find_backward_occurrence(rope, line_start, before, target)?;
                }
                occurrence = Some(found);
                before = found;
            }
        }
    }

    let occurrence = occurrence?;
    let target_offset = match kind {
        VimFindKind::Forward | VimFindKind::Backward => occurrence,
        VimFindKind::TillForward => previous_boundary(rope, occurrence),
        VimFindKind::TillBackward => occurrence.saturating_add(target.len()),
    };
    Some(Motion {
        target: target_offset,
        inclusive: matches!(kind, VimFindKind::Forward | VimFindKind::TillForward),
        linewise: false,
    })
}
