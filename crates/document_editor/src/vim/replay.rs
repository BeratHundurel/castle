use super::*;

pub(super) fn replace_visual_text(selected: &str, target: &str) -> String {
    if target == "\n" {
        return target.to_string();
    }
    let mut replacement = String::with_capacity(selected.len().max(target.len()));
    for ch in selected.chars() {
        if matches!(ch, '\r' | '\n') {
            replacement.push(ch);
        } else {
            replacement.push_str(target);
        }
    }
    replacement
}

pub(super) fn normalized_replay_steps(steps: &[VimReplayStep]) -> (Vec<VimReplayStep>, u32) {
    let mut normalized = Vec::with_capacity(steps.len());
    let mut combined_count = 1_u32;
    let mut index = 0;
    while index < steps.len() {
        let Some(VimReplayStep::Key(VimKey::Digit(first))) = steps.get(index) else {
            normalized.push(steps[index].clone());
            index += 1;
            continue;
        };
        if *first == 0 {
            normalized.push(steps[index].clone());
            index += 1;
            continue;
        }
        let mut group = 0_u32;
        while let Some(VimReplayStep::Key(VimKey::Digit(digit))) = steps.get(index) {
            group = group
                .saturating_mul(10)
                .saturating_add(u32::from(*digit))
                .min(MAX_COUNT);
            index += 1;
        }
        combined_count = combined_count.saturating_mul(group).min(MAX_COUNT);
    }
    (normalized, combined_count)
}

pub(super) fn replay_is_open_line(steps: &[VimReplayStep]) -> bool {
    matches!(
        steps.first(),
        Some(VimReplayStep::Key(VimKey::OpenBelow | VimKey::OpenAbove))
    )
}

pub(super) fn replay_repeats_insert_text(steps: &[VimReplayStep]) -> bool {
    matches!(
        steps.first(),
        Some(VimReplayStep::Key(
            VimKey::Insert | VimKey::Append | VimKey::InsertLineStart | VimKey::AppendLineEnd
        ))
    )
}

pub(super) fn signed_char_distance(rope: &Rope, from: usize, to: usize) -> isize {
    if to >= from {
        rope.slice(from..to).chars().count() as isize
    } else {
        -(rope.slice(to..from).chars().count() as isize)
    }
}

pub(super) fn move_by_chars(rope: &Rope, mut offset: usize, delta: isize) -> usize {
    if delta >= 0 {
        for _ in 0..delta as usize {
            let next = next_boundary(rope, offset);
            if next == offset {
                break;
            }
            offset = next;
        }
    } else {
        for _ in 0..delta.unsigned_abs() {
            let previous = previous_boundary(rope, offset);
            if previous == offset {
                break;
            }
            offset = previous;
        }
    }
    offset
}

pub(super) fn insert_patch_between(
    before: &Rope,
    after: &Rope,
    anchor: usize,
    cursor: usize,
) -> Option<VimInsertPatch> {
    if before == after {
        return None;
    }

    let mut before_start = 0;
    let mut after_start = 0;
    let mut before_chars = before.chars();
    let mut after_chars = after.chars();
    while before_start < anchor {
        match (before_chars.next(), after_chars.next()) {
            (Some(left), Some(right)) if left == right => {
                before_start += left.len_utf8();
                after_start += right.len_utf8();
            }
            _ => break,
        }
    }

    let mut before_end = before.len();
    let mut after_end = after.len();
    while before_end > before_start && after_end > after_start {
        let before_previous = previous_boundary(before, before_end);
        let after_previous = previous_boundary(after, after_end);
        if before.char_at(before_previous) != after.char_at(after_previous) {
            break;
        }
        before_end = before_previous;
        after_end = after_previous;
    }

    Some(VimInsertPatch {
        start_delta: signed_char_distance(before, anchor, before_start),
        end_delta: signed_char_distance(before, anchor, before_end),
        replacement: after.slice(after_start..after_end).to_string(),
        cursor_delta: signed_char_distance(after, after_start, cursor),
    })
}

pub(super) fn rope_replacement_between(
    before: &Rope,
    after: &Rope,
) -> Option<(Range<usize>, String)> {
    if before == after {
        return None;
    }

    let mut before_start = 0;
    let mut after_start = 0;
    let mut before_chars = before.chars();
    let mut after_chars = after.chars();
    loop {
        match (before_chars.next(), after_chars.next()) {
            (Some(left), Some(right)) if left == right => {
                before_start += left.len_utf8();
                after_start += right.len_utf8();
            }
            _ => break,
        }
    }

    let mut before_end = before.len();
    let mut after_end = after.len();
    while before_end > before_start && after_end > after_start {
        let before_previous = previous_boundary(before, before_end);
        let after_previous = previous_boundary(after, after_end);
        if before.char_at(before_previous) != after.char_at(after_previous) {
            break;
        }
        before_end = before_previous;
        after_end = after_previous;
    }

    Some((
        before_start..before_end,
        after.slice(after_start..after_end).to_string(),
    ))
}

pub(super) fn leading_indent(text: &str) -> String {
    text.chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .collect()
}
