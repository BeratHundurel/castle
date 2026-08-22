use super::super::*;
use test_support as test_alloc;

#[test]
fn word_motions_distinguish_words_whitespace_and_punctuation() {
    let rope = Rope::from("one  two...three");
    assert_eq!(next_word_start(&rope, 0), 5);
    assert_eq!(next_word_start(&rope, 5), 8);
    assert_eq!(next_word_start(&rope, 8), 11);
    assert_eq!(previous_word_start(&rope, 16), 11);
    assert_eq!(word_end(&rope, 5), 7);
}

#[test]
fn big_word_motions_treat_punctuation_as_part_of_a_word() {
    let text = "one.two\t中-文  last";
    let rope = Rope::from(text);
    let middle = text.find("中-文").expect("middle WORD should be present");
    let last = text.find("last").expect("last WORD should be present");

    assert_eq!(next_big_word_start(&rope, 0), middle);
    assert_eq!(next_big_word_start(&rope, middle), last);
    assert_eq!(previous_big_word_start(&rope, last), middle);
    assert_eq!(previous_big_word_start(&rope, middle), 0);
    assert_eq!(big_word_end(&rope, 0), "one.two".len() - 1);
    assert_eq!(
        big_word_end(&rope, "one.two".len() - 1),
        middle + "中-".len()
    );
    assert_eq!(big_word_end(&rope, last), text.len() - 1);
}

#[test]
fn character_ranges_are_unicode_safe_and_stop_at_line_boundaries() {
    let rope = Rope::from("a中b\nxy");
    assert_eq!(forward_char_range(&rope, 1, 2), 1..5);
    assert_eq!(forward_char_range(&rope, 4, 99), 4..5);
    assert_eq!(backward_char_range(&rope, 4, 1), 1..4);
    assert_eq!(backward_char_range(&rope, 4, 99), 0..4);
    assert_eq!(backward_char_range(&rope, 6, 2), 6..6);
}

#[test]
fn line_join_edits_trim_indent_and_handle_crlf_blank_and_final_lines() {
    let crlf = Rope::from("one  \r\n\t two \r\n中");
    assert_eq!(
        join_line_edit(&crlf, 0, 2),
        Some((3..13, " two".to_string()))
    );
    assert_eq!(
        join_line_edit(&crlf, 0, 3),
        Some((3..18, " two 中".to_string()))
    );

    let blank = Rope::from("one\n\n  two\n");
    assert_eq!(
        join_line_edit(&blank, 0, 3),
        Some((3..10, " two".to_string()))
    );
    assert_eq!(join_line_edit(&blank, blank.len(), 2), None);

    let empty_first = Rope::from("  \n next");
    assert_eq!(
        join_line_edit(&empty_first, 0, 2),
        Some((0..8, "next".to_string()))
    );
}

#[test]
fn inner_word_selects_only_the_run_under_the_cursor() {
    let text = "Further testing showed naïve 中文... results";
    let rope = Rope::from(text);
    let testing = text.find("testing").expect("testing should be present");
    let naive = text.find("naïve").expect("naïve should be present");
    let chinese = text.find("中文").expect("Chinese word should be present");
    let punctuation = text.find("...").expect("punctuation should be present");

    let cases = [
        (0, 0.."Further".len()),
        (3, 0.."Further".len()),
        ("Further".len(), "Further".len()..testing),
        (testing + 2, testing..testing + "testing".len()),
        (naive + 2, naive..naive + "naïve".len()),
        (chinese + 3, chinese..chinese + "中文".len()),
        (punctuation + 1, punctuation..punctuation + 3),
    ];

    for (cursor, expected) in cases {
        assert_eq!(
            word_text_object_range(&rope, cursor, 1, VimTextObjectPrefix::Inner),
            expected,
            "unexpected inner-word range at byte offset {cursor}"
        );
    }
}

#[test]
fn inner_word_counts_include_intervening_space_without_the_following_word() {
    let rope = Rope::from("Further testing showed");
    assert_eq!(
        word_text_object_range(&rope, 0, 2, VimTextObjectPrefix::Inner),
        0.."Further testing".len()
    );
    assert_eq!(
        word_text_object_range(&rope, 7, 1, VimTextObjectPrefix::Inner),
        7..8
    );
}

#[test]
fn around_word_prefers_trailing_space_then_falls_back_to_leading_space() {
    let text = "Further testing";
    let rope = Rope::from(text);
    let testing = text.find("testing").expect("testing should be present");
    assert_eq!(
        word_text_object_range(&rope, 2, 1, VimTextObjectPrefix::Around),
        0..testing
    );
    assert_eq!(
        word_text_object_range(&rope, testing, 1, VimTextObjectPrefix::Around),
        "Further".len()..text.len()
    );
    assert_eq!(
        word_text_object_range(&rope, "Further".len(), 1, VimTextObjectPrefix::Around),
        "Further".len()..text.len()
    );
}

#[test]
fn word_text_objects_do_not_merge_line_breaks_with_horizontal_space() {
    let rope = Rope::from("one  \r\n\n\ttwo");
    assert_eq!(
        word_text_object_range(&rope, 3, 1, VimTextObjectPrefix::Inner),
        3..5
    );
    assert_eq!(
        word_text_object_range(&rope, 5, 1, VimTextObjectPrefix::Inner),
        5..7
    );
    assert_eq!(
        word_text_object_range(&rope, 7, 1, VimTextObjectPrefix::Inner),
        7..8
    );
    assert_eq!(
        word_text_object_range(&rope, 8, 1, VimTextObjectPrefix::Inner),
        8..9
    );
}

#[test]
fn quote_text_objects_handle_around_inner_unicode_and_escapes() {
    let text = "say \"Further \\\"naïve\\\" 中\" now";
    let rope = Rope::from(text);
    let cursor = text
        .find("naïve")
        .expect("quoted Unicode text should exist");
    let opening = text.find('"').expect("opening quote should exist");
    let closing = text.rfind('"').expect("closing quote should exist");

    assert_eq!(
        quote_text_object_range(&rope, cursor, VimTextObjectPrefix::Inner, '"'),
        opening + 1..closing
    );
    assert_eq!(
        quote_text_object_range(&rope, cursor, VimTextObjectPrefix::Around, '"'),
        opening..closing + 1
    );

    let cases = [("'one'", '\''), ("`two`", '`')];
    for (source, delimiter) in cases {
        let rope = Rope::from(source);
        assert_eq!(
            quote_text_object_range(&rope, 2, VimTextObjectPrefix::Inner, delimiter),
            1..source.len() - 1
        );
    }
}

#[test]
fn quote_text_objects_stay_on_the_current_line_and_require_a_pair() {
    let rope = Rope::from("\"open\nclose\"");
    assert_eq!(
        quote_text_object_range(&rope, 2, VimTextObjectPrefix::Inner, '"'),
        2..2
    );
    let unmatched = Rope::from("before \"after");
    assert_eq!(
        quote_text_object_range(&unmatched, 9, VimTextObjectPrefix::Around, '"'),
        9..9
    );
}

#[test]
fn pair_text_objects_choose_the_innermost_nested_pair() {
    let text = "call(outer[中 + inner(x)]) tail";
    let rope = Rope::from(text);
    let cursor = text.find('x').expect("nested value should exist");
    let inner_open = text.find("(x)").expect("inner pair should exist");
    let bracket_open = text.find('[').expect("bracket should exist");
    let bracket_close = text.find(']').expect("closing bracket should exist");

    assert_eq!(
        pair_text_object_range(&rope, cursor, VimTextObjectPrefix::Inner, '(', ')'),
        inner_open + 1..inner_open + 2
    );
    assert_eq!(
        pair_text_object_range(&rope, cursor, VimTextObjectPrefix::Around, '(', ')'),
        inner_open..inner_open + 3
    );
    assert_eq!(
        pair_text_object_range(&rope, cursor, VimTextObjectPrefix::Inner, '[', ']'),
        bracket_open + 1..bracket_close
    );

    let unmatched = Rope::from("one {two");
    assert_eq!(
        pair_text_object_range(&unmatched, 6, VimTextObjectPrefix::Inner, '{', '}'),
        6..6
    );
}

#[test]
fn linewise_visual_ranges_include_complete_crlf_and_final_lines() {
    let rope = Rope::from("one\r\ntwo\nthree");
    assert_eq!(line_rows_range(&rope, 0, 1), 0..9);
    assert_eq!(line_rows_range(&rope, 1, 2), 5..14);
}

#[test]
fn line_ranges_include_newlines_and_handle_final_lines() {
    let rope = Rope::from("one\r\ntwo\nthree");
    assert_eq!(line_count_range(&rope, 0, 2), 0..9);
    assert_eq!(line_count_range(&rope, 9, 2), 9..14);
    assert_eq!(normal_line_end(&rope, 0), 2);
}

#[test]
fn line_break_inference_prefers_the_current_then_nearest_line() {
    let mixed = Rope::from("one\r\ntwo\nthree");
    assert_eq!(line_break_for_row(&mixed, 0), "\r\n");
    assert_eq!(line_break_for_row(&mixed, 1), "\n");
    assert_eq!(line_break_for_row(&mixed, 2), "\n");

    assert_eq!(line_break_for_row(&Rope::from("one"), 0), "\n");
}

#[test]
fn motions_cover_lines_tabs_unicode_and_document_edges() {
    let rope = Rope::from("one two\n\t中 x\nlast");
    let cases = [
        (0, VimKey::WordForward, Some(1), 4),
        (6, VimKey::WordBackward, Some(1), 4),
        (0, VimKey::WordEnd, Some(1), 2),
        (0, VimKey::BigWordForward, Some(2), 9),
        (13, VimKey::BigWordBackward, Some(1), 9),
        (8, VimKey::BigWordEnd, Some(1), 9),
        (8, VimKey::Left, Some(1), 8),
        (6, VimKey::Right, Some(1), 6),
        (8, VimKey::FirstNonBlank, Some(1), 9),
        (9, VimKey::LineEnd, Some(1), 13),
        (0, VimKey::Down, Some(1), 8),
        (13, VimKey::Go, None, 0),
        (13, VimKey::Go, Some(2), 8),
        (0, VimKey::DocumentEnd, None, 15),
        (15, VimKey::DocumentEnd, Some(1), 0),
        (0, VimKey::DocumentEnd, Some(2), 8),
    ];

    for (cursor, key, count, expected) in cases {
        assert_eq!(
            motion_for_key(&rope, cursor, key, count, None).map(|motion| motion.target),
            Some(expected),
            "unexpected target for {key:?} from {cursor} with count {count:?}"
        );
    }
}

#[test]
fn normal_cursor_clamping_handles_empty_and_whitespace_only_lines() {
    let empty = Rope::from("");
    assert_eq!(clamp_normal_offset(&empty, 8), 0);

    let rope = Rope::from("  \n\n中\n");
    assert_eq!(first_non_blank(&rope, 1), 0);
    assert_eq!(clamp_normal_offset(&rope, 2), 1);
    assert_eq!(clamp_normal_offset(&rope, 3), 3);
    assert_eq!(clamp_normal_offset(&rope, 7), 4);
    assert_eq!(clamp_normal_offset(&rope, 8), 8);
}

#[test]
fn operator_ranges_distinguish_characterwise_and_linewise_motions() {
    let rope = Rope::from("one two\nthree\nfour");
    let right = motion_for_key(&rope, 4, VimKey::Right, Some(1), None)
        .map(|motion| operator_range(&rope, 4, motion));
    let word_end = motion_for_key(&rope, 4, VimKey::WordEnd, Some(1), None)
        .map(|motion| operator_range(&rope, 4, motion));
    let big_word_end = motion_for_key(&rope, 0, VimKey::BigWordEnd, Some(1), None)
        .map(|motion| operator_range(&rope, 0, motion));
    let down = motion_for_key(&rope, 4, VimKey::Down, Some(1), None)
        .map(|motion| linewise_motion_range(&rope, 4, motion.target));

    assert_eq!(right, Some(4..5));
    assert_eq!(word_end, Some(4..7));
    assert_eq!(big_word_end, Some(0..3));
    assert_eq!(down, Some(0..14));
}

#[test]
fn inclusive_visual_ranges_respect_multibyte_characters() {
    let rope = Rope::from("a中b");
    assert_eq!(inclusive_range(&rope, 1, 1), 1..4);
    assert_eq!(inclusive_range(&rope, 4, 1), 1..5);
}

#[test]
fn operator_counts_multiply_and_saturate() {
    let mut vim = VimState::new(true);
    vim.operator_count = Some(2);
    vim.count = Some(3);
    assert_eq!(combined_operator_count(&mut vim), Some(6));

    vim.operator_count = Some(MAX_COUNT);
    vim.count = Some(MAX_COUNT);
    assert_eq!(combined_operator_count(&mut vim), Some(MAX_COUNT));

    let mut no_count = VimState::new(true);
    no_count.pending_operator = Some(VimOperator::Delete);
    assert_eq!(combined_operator_count(&mut no_count), None);

    let mut digits = VimState::new(true);
    for _ in 0..12 {
        digits.push_digit(9);
    }
    assert_eq!(digits.count, Some(MAX_COUNT));
}

#[test]
fn command_text_preserves_operator_and_motion_count_order() {
    let mut vim = VimState::new(true);
    vim.operator_count = Some(2);
    vim.pending_operator = Some(VimOperator::Delete);
    vim.count = Some(3);
    vim.pending_g = true;
    assert_eq!(vim.command_text(), "2d3g");

    vim.pending_g = false;
    vim.pending_text_object = Some(VimTextObjectPrefix::Inner);
    assert_eq!(vim.command_text(), "2d3i");
}

#[test]
fn command_reset_clears_invalid_sequence_state() {
    let mut vim = VimState::new(true);
    vim.count = Some(24);
    vim.operator_count = Some(3);
    vim.pending_operator = Some(VimOperator::Change);
    vim.pending_g = true;
    vim.pending_text_object = Some(VimTextObjectPrefix::Around);

    vim.reset_command();

    assert_eq!(vim.count, None);
    assert_eq!(vim.operator_count, None);
    assert_eq!(vim.pending_operator, None);
    assert!(!vim.pending_g);
    assert_eq!(vim.pending_text_object, None);
}

#[test]
fn character_find_motions_cover_directions_counts_unicode_and_line_edges() {
    let cases = [
        ("a-b-a-b", 0, VimFindKind::Forward, "b", 1, Some(2)),
        ("a-b-a-b", 0, VimFindKind::Forward, "b", 2, Some(6)),
        ("a-b-a-b", 0, VimFindKind::TillForward, "b", 1, Some(1)),
        ("a-b-a-b", 6, VimFindKind::Backward, "a", 1, Some(4)),
        ("a-b-a-b", 6, VimFindKind::TillBackward, "a", 1, Some(5)),
        ("a中b中", 0, VimFindKind::Forward, "中", 2, Some(5)),
        ("x\r\nyx", 0, VimFindKind::Forward, "x", 1, None),
        ("\t a\t", 0, VimFindKind::Forward, "\t", 1, Some(3)),
        ("", 0, VimFindKind::Forward, "x", 1, None),
    ];
    for (text, cursor, kind, target, count, expected) in cases {
        let rope = Rope::from(text);
        assert_eq!(
            find_char_motion(&rope, cursor, kind, target, count, false).map(|motion| motion.target),
            expected,
            "unexpected {kind:?} result for {text:?}"
        );
    }
}

#[test]
fn repeated_till_motions_skip_the_previous_adjacent_target() {
    let rope = Rope::from("a,x,x");
    let first = find_char_motion(&rope, 0, VimFindKind::TillForward, "x", 1, false)
        .expect("first till should find x");
    assert_eq!(first.target, 1);
    let repeated = find_char_motion(&rope, first.target, VimFindKind::TillForward, "x", 1, true)
        .expect("repeat should skip the x already used by t");
    assert_eq!(repeated.target, 3);

    let backward = find_char_motion(&rope, 4, VimFindKind::TillBackward, "x", 1, true)
        .expect("reverse till repeat should find the previous x");
    assert_eq!(backward.target, 3);
}

#[test]
fn find_motions_preserve_operator_inclusivity() {
    let rope = Rope::from("abXcdXef");
    let forward =
        find_char_motion(&rope, 0, VimFindKind::Forward, "X", 1, false).expect("f should find X");
    let till = find_char_motion(&rope, 0, VimFindKind::TillForward, "X", 1, false)
        .expect("t should find X");
    let backward =
        find_char_motion(&rope, 7, VimFindKind::Backward, "X", 1, false).expect("F should find X");
    let till_backward = find_char_motion(&rope, 7, VimFindKind::TillBackward, "X", 1, false)
        .expect("T should find X");

    assert_eq!(operator_range(&rope, 0, forward), 0..3);
    assert_eq!(operator_range(&rope, 0, till), 0..2);
    assert_eq!(operator_range(&rope, 7, backward), 5..7);
    assert_eq!(operator_range(&rope, 7, till_backward), 6..7);
}

#[test]
fn visual_replacement_preserves_line_breaks_and_repeats_unicode_targets() {
    assert_eq!(replace_visual_text("ab\r\n中", "λ"), "λλ\r\nλ");
    assert_eq!(replace_visual_text("abc", "\n"), "\n");
    assert_eq!(replace_visual_text("", "x"), "");
}

#[test]
fn repeat_recipes_combine_counts_without_losing_zero_motions() {
    let (steps, count) = normalized_replay_steps(&[
        VimReplayStep::Key(VimKey::Digit(2)),
        VimReplayStep::Key(VimKey::Delete),
        VimReplayStep::Key(VimKey::Digit(3)),
        VimReplayStep::Key(VimKey::WordForward),
    ]);
    assert_eq!(count, 6);
    assert!(matches!(
        steps.as_slice(),
        [
            VimReplayStep::Key(VimKey::Delete),
            VimReplayStep::Key(VimKey::WordForward)
        ]
    ));

    let (steps, count) = normalized_replay_steps(&[
        VimReplayStep::Key(VimKey::Delete),
        VimReplayStep::Key(VimKey::Digit(0)),
    ]);
    assert_eq!(count, 1);
    assert!(matches!(
        steps.last(),
        Some(VimReplayStep::Key(VimKey::Digit(0)))
    ));
}

#[test]
fn insert_patch_tracks_unicode_edits_relative_to_the_insert_anchor() {
    let before = Rope::from("a中c");
    let after = Rope::from("aλ中c");
    let patch = insert_patch_between(&before, &after, 1, "aλ".len())
        .expect("insert should produce a patch");
    assert_eq!(patch.start_delta, 0);
    assert_eq!(patch.end_delta, 0);
    assert_eq!(patch.replacement, "λ");
    assert_eq!(patch.cursor_delta, 1);
}

#[test]
fn local_motion_on_a_large_rope_does_not_materialize_the_document() {
    let rope = Rope::from(format!("{}target word", "line\n".repeat(500_000)));
    let start = rope.len() - "target word".len();
    let allocation = test_alloc::start_measurement();
    for _ in 0..128 {
        std::hint::black_box(next_word_start(&rope, start));
        std::hint::black_box(previous_word_start(&rope, rope.len()));
    }
    let allocation = allocation.finish();

    assert!(
        allocation.allocated_bytes < rope.len() / 4,
        "local motions allocated {} bytes for a {} byte rope",
        allocation.allocated_bytes,
        rope.len()
    );
    assert_eq!(next_word_start(&rope, start), start + "target ".len());
    assert_eq!(
        previous_word_start(&rope, rope.len()),
        start + "target ".len()
    );
}

#[test]
fn find_and_repeat_planning_on_a_large_rope_stay_local() {
    let prefix = "line\n".repeat(500_000);
    let line_start = prefix.len();
    let rope = Rope::from(format!("{prefix}alpha,target,target"));
    let mut edited = rope.clone();
    let insert_at = line_start + "alpha".len();
    edited.insert(insert_at, "λ");
    let steps = [
        VimReplayStep::Key(VimKey::Digit(2)),
        VimReplayStep::Key(VimKey::Delete),
        VimReplayStep::Key(VimKey::Digit(3)),
        VimReplayStep::Key(VimKey::FindForward),
        VimReplayStep::Literal(",".to_string()),
    ];

    let allocation = test_alloc::start_measurement();
    for _ in 0..128 {
        std::hint::black_box(find_char_motion(
            &rope,
            line_start,
            VimFindKind::Forward,
            ",",
            2,
            false,
        ));
    }
    let patch = insert_patch_between(&rope, &edited, insert_at, insert_at + "λ".len())
        .expect("local insertion should produce a repeat patch");
    let (normalized, count) = normalized_replay_steps(&steps);
    let allocation = allocation.finish();

    assert!(
        allocation.allocated_bytes < rope.len() / 4,
        "find and repeat planning allocated {} bytes for a {} byte rope",
        allocation.allocated_bytes,
        rope.len()
    );
    assert_eq!(patch.replacement, "λ");
    assert_eq!(count, 6);
    assert_eq!(normalized.len(), 3);
}
