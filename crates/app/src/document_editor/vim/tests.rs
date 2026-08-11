use super::*;
use crate::{AppServices, app_settings::AppSettings, test_alloc};
use entity::note;
use gpui_component::input::InputEvent;
use migration::{Migrator, MigratorTrait};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};
use std::{cell::Cell, path::PathBuf, rc::Rc, sync::Arc};

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

fn with_vim_editor(
    cx: &mut gpui::TestAppContext,
    initial_content: &str,
    test: impl FnOnce(gpui::Entity<DocumentEditorView>, &mut gpui::VisualTestContext),
) {
    let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
    let _runtime_guard = runtime.enter();
    cx.executor().allow_parking();
    let (db, note_id) = runtime
        .block_on(async {
            let db = Database::connect("sqlite::memory:").await?;
            Migrator::up(&db, None).await?;
            let note = note::ActiveModel {
                title: Set("Vim test".into()),
                cached_content: Set(initial_content.into()),
                created_at: Set(1),
                updated_at: Set(1),
                ..Default::default()
            }
            .insert(&db)
            .await?;
            Ok::<_, anyhow::Error>((db, note.id as u32))
        })
        .expect("Vim test database should initialize");
    let settings_dir = std::env::temp_dir().join(format!(
        "castle-vim-mode-focused-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos()
    ));
    let mut editor_view = None;
    let window = cx.update(|cx| {
        cx.set_global(gpui_component::Theme::default());
        gpui_component::init(cx);
        cx.set_global(AppSettings::load(settings_dir));
        AppSettings::set_editor_vim_mode(true, cx);
        AppSettings::set_editor_status_line_visible(false, cx);
        crate::keymap::init(cx);
        cx.set_global(AppServices::new(Arc::new(db), PathBuf::new()));
        cx.open_window(Default::default(), |window, cx| {
            let view = DocumentEditorView::view(note_id, window, cx);
            editor_view = Some(view.clone());
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("Vim test window should open")
    });
    let view = editor_view.expect("document editor should exist");
    let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

    for _ in 0..100 {
        cx.run_until_parked();
        if view.read_with(&cx, |editor, _| !editor.persistence.is_loading) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        view.read_with(&cx, |editor, _| !editor.persistence.is_loading),
        "Vim test editor should finish loading"
    );
    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test(initial_content, window, cx);
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
        let _ = window.draw(cx);
    });

    test(view, &mut cx);
}

fn set_vim_test_content(
    view: &gpui::Entity<DocumentEditorView>,
    content: &str,
    position: Position,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test(content, window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(position, window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
}

fn vim_test_value(view: &gpui::Entity<DocumentEditorView>, cx: &gpui::VisualTestContext) -> String {
    view.read_with(cx, |editor, cx| editor.editor.read(cx).value().to_string())
}

#[gpui::test]
fn counted_linewise_yank_and_paste_execute_through_the_keymap(cx: &mut gpui::TestAppContext) {
    with_vim_editor(cx, "one two\nthree four\nfive", |view, cx| {
        cx.simulate_keystrokes("2 y y");

        assert_eq!(vim_test_value(&view, cx), "one two\nthree four\nfive");
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("one two\nthree four\n".to_string())
        );

        cx.simulate_keystrokes("shift-g p");

        assert_eq!(
            vim_test_value(&view, cx),
            "one two\nthree four\nfive\none two\nthree four"
        );
        assert_eq!(
            view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
            "one two\nthree four\nfive\n".len()
        );
    });
}

#[gpui::test]
fn insert_line_start_open_line_and_direct_changes_round_trip(cx: &mut gpui::TestAppContext) {
    with_vim_editor(cx, "  alpha\nbeta", |view, cx| {
        cx.simulate_keystrokes("shift-i");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape");
        assert_eq!(vim_test_value(&view, cx), "  Xalpha\nbeta");

        set_vim_test_content(&view, "alpha beta\ngamma", Position::new(0, 0), cx);
        cx.simulate_keystrokes("shift-d");
        assert_eq!(vim_test_value(&view, cx), "\ngamma");
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("alpha beta".to_string())
        );

        set_vim_test_content(&view, "alpha beta\ngamma", Position::new(0, 0), cx);
        cx.simulate_keystrokes("shift-c");
        assert_eq!(
            view.read_with(cx, |editor, _| editor.vim_mode()),
            VimMode::Insert
        );
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape");
        assert_eq!(vim_test_value(&view, cx), "X\ngamma");

        set_vim_test_content(&view, "alpha\ngamma", Position::new(0, 0), cx);
        cx.simulate_keystrokes("o");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape");
        assert_eq!(vim_test_value(&view, cx), "alpha\nX\ngamma");
    });
}

#[gpui::test]
fn visual_paste_replaces_the_inclusive_selection(cx: &mut gpui::TestAppContext) {
    with_vim_editor(cx, "abcd", |view, cx| {
        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(
                "X".to_string(),
                VIM_CLIPBOARD_CHARACTERWISE.to_string(),
            ));
        });

        cx.simulate_keystrokes("v l p");

        assert_eq!(vim_test_value(&view, cx), "Xcd");
        assert_eq!(
            view.read_with(cx, |editor, _| editor.vim_mode()),
            VimMode::Normal
        );
        assert_eq!(
            view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
            0
        );
    });
}

#[gpui::test]
fn explicit_line_counts_distinguish_g_from_bare_g_in_motions_and_operators(
    cx: &mut gpui::TestAppContext,
) {
    with_vim_editor(cx, "first\nmiddle\nlast", |view, cx| {
        cx.simulate_keystrokes("shift-g");
        assert_eq!(
            view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
            "first\nmiddle\n".len()
        );

        cx.simulate_keystrokes("1 shift-g");

        assert_eq!(
            view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
            0
        );

        cx.simulate_keystrokes("2 shift-g");
        assert_eq!(
            view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
            "first\n".len()
        );

        set_vim_test_content(&view, "first\nmiddle\nlast", Position::new(1, 0), cx);
        cx.simulate_keystrokes("d 1 shift-g");
        assert_eq!(vim_test_value(&view, cx), "last");
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("first\nmiddle\n".to_string())
        );
    });
}

#[gpui::test]
fn invalid_operator_sequences_consume_the_key_and_clear_counts(cx: &mut gpui::TestAppContext) {
    with_vim_editor(cx, "abc", |view, cx| {
        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(
                "X".to_string(),
                VIM_CLIPBOARD_CHARACTERWISE.to_string(),
            ));
        });

        cx.simulate_keystrokes("d p");

        assert_eq!(vim_test_value(&view, cx), "abc");
        assert_eq!(
            view.read_with(cx, |editor, _| editor.vim_state.state.command_text()),
            ""
        );
        cx.simulate_keystrokes("x");
        assert_eq!(vim_test_value(&view, cx), "bc");

        set_vim_test_content(&view, "abc", Position::new(0, 0), cx);
        cx.simulate_keystrokes("2 d p x");
        assert_eq!(vim_test_value(&view, cx), "bc");

        set_vim_test_content(&view, "abc", Position::new(0, 0), cx);
        cx.simulate_keystrokes("d x");
        assert_eq!(vim_test_value(&view, cx), "abc");
        cx.simulate_keystrokes("x");
        assert_eq!(vim_test_value(&view, cx), "bc");
    });
}

#[gpui::test]
fn open_line_preserves_crlf_above_below_and_at_the_final_line(cx: &mut gpui::TestAppContext) {
    with_vim_editor(cx, "one\r\ntwo", |view, cx| {
        cx.simulate_keystrokes("o");
        assert_eq!(vim_test_value(&view, cx), "one\r\n\r\ntwo");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape");
        assert_eq!(vim_test_value(&view, cx), "one\r\nX\r\ntwo");

        set_vim_test_content(&view, "one\r\ntwo", Position::new(1, 0), cx);
        cx.simulate_keystrokes("shift-o");
        assert_eq!(vim_test_value(&view, cx), "one\r\n\r\ntwo");
        cx.simulate_input("Y");
        cx.simulate_keystrokes("escape");
        assert_eq!(vim_test_value(&view, cx), "one\r\nY\r\ntwo");

        set_vim_test_content(&view, "one\r\ntwo", Position::new(1, 0), cx);
        cx.simulate_keystrokes("o");
        assert_eq!(vim_test_value(&view, cx), "one\r\ntwo\r\n");
        cx.simulate_input("Z");
        cx.simulate_keystrokes("escape");
        assert_eq!(vim_test_value(&view, cx), "one\r\ntwo\r\nZ");

        set_vim_test_content(&view, "one\r\ntwo", Position::new(0, 0), cx);
        cx.simulate_keystrokes("shift-o");
        assert_eq!(vim_test_value(&view, cx), "\r\none\r\ntwo");
        cx.simulate_input("A");
        cx.simulate_keystrokes("escape");
        assert_eq!(vim_test_value(&view, cx), "A\r\none\r\ntwo");
    });
}

#[gpui::test]
fn character_find_repeats_reverses_and_composes_with_operators(cx: &mut gpui::TestAppContext) {
    with_vim_editor(cx, "", |view, cx| {
        set_vim_test_content(&view, "a-b-a-b tail", Position::new(0, 0), cx);
        cx.simulate_keystrokes("f b");
        assert_eq!(
            view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
            2
        );
        cx.simulate_keystrokes(";");
        assert_eq!(
            view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
            6
        );
        cx.simulate_keystrokes(",");
        assert_eq!(
            view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
            2
        );

        cx.simulate_keystrokes("f z");
        cx.simulate_keystrokes(";");
        assert_eq!(
            view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
            6,
            "a failed find must preserve the previous successful find"
        );

        set_vim_test_content(&view, "abXcdXef", Position::new(0, 0), cx);
        cx.simulate_keystrokes("d t shift-x->X");
        assert_eq!(vim_test_value(&view, cx), "XcdXef");
        set_vim_test_content(&view, "abXcdXef", Position::new(0, 0), cx);
        cx.simulate_keystrokes("d 2 f shift-x->X");
        assert_eq!(vim_test_value(&view, cx), "ef");

        set_vim_test_content(&view, "(a)b", Position::new(0, 0), cx);
        cx.simulate_keystrokes("f shift-0->)");
        assert_eq!(
            view.read_with(cx, |editor, cx| editor.editor.read(cx).cursor()),
            2
        );
    });
}

#[gpui::test]
fn replace_character_handles_counts_unicode_crlf_visual_ranges_and_failure(
    cx: &mut gpui::TestAppContext,
) {
    with_vim_editor(cx, "", |view, cx| {
        set_vim_test_content(&view, "a中bc", Position::new(0, 0), cx);
        cx.simulate_keystrokes("2 r x");
        assert_eq!(vim_test_value(&view, cx), "xxbc");
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("a中".to_string())
        );

        set_vim_test_content(&view, "abc", Position::new(0, 1), cx);
        cx.simulate_keystrokes("r λ");
        assert_eq!(vim_test_value(&view, cx), "aλc");

        set_vim_test_content(&view, "ab\r\ncd", Position::new(0, 1), cx);
        cx.simulate_keystrokes("r enter");
        assert_eq!(vim_test_value(&view, cx), "a\r\n\r\ncd");

        set_vim_test_content(&view, "abcd\r\nef", Position::new(0, 0), cx);
        cx.simulate_keystrokes("v 2 l r z");
        assert_eq!(vim_test_value(&view, cx), "zzzd\r\nef");

        set_vim_test_content(&view, "ab", Position::new(0, 1), cx);
        cx.simulate_keystrokes("2 r x");
        assert_eq!(vim_test_value(&view, cx), "ab", "r must fail atomically");
    });
}

#[gpui::test]
fn dot_repeats_normal_operator_find_and_visual_changes(cx: &mut gpui::TestAppContext) {
    with_vim_editor(cx, "", |view, cx| {
        set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
        cx.simulate_keystrokes("x");
        cx.simulate_keystrokes("w .");
        assert_eq!(vim_test_value(&view, cx), "ne wo");
        cx.simulate_keystrokes("u");
        assert_eq!(vim_test_value(&view, cx), "ne two");

        set_vim_test_content(&view, "one\ntwo\nthree\n", Position::new(0, 0), cx);
        cx.simulate_keystrokes("d d .");
        assert_eq!(vim_test_value(&view, cx), "three\n");

        set_vim_test_content(&view, "aXbXcX", Position::new(0, 0), cx);
        cx.simulate_keystrokes("d f shift-x->X .");
        assert_eq!(vim_test_value(&view, cx), "cX");

        set_vim_test_content(&view, "abcdef", Position::new(0, 0), cx);
        cx.simulate_keystrokes("v l d l .");
        assert_eq!(vim_test_value(&view, cx), "cf");
    });
}

#[gpui::test]
fn dot_replays_insert_change_open_line_unicode_and_replacement(cx: &mut gpui::TestAppContext) {
    with_vim_editor(cx, "", |view, cx| {
        set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
        cx.simulate_keystrokes("3 i");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape");
        assert_eq!(vim_test_value(&view, cx), "XXXone two");

        set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
        cx.simulate_keystrokes("i");
        cx.simulate_input("λ");
        cx.simulate_keystrokes("escape w .");
        assert_eq!(vim_test_value(&view, cx), "λone λtwo");

        set_vim_test_content(&view, "one two three", Position::new(0, 0), cx);
        cx.simulate_keystrokes("c w");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape w .");
        assert_eq!(vim_test_value(&view, cx), "Xtwo X");

        set_vim_test_content(&view, "one\r\ntwo", Position::new(0, 0), cx);
        cx.simulate_keystrokes("o");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape shift-g .");
        assert_eq!(vim_test_value(&view, cx), "one\r\nX\r\ntwo\r\nX");

        set_vim_test_content(&view, "abcd", Position::new(0, 0), cx);
        cx.simulate_keystrokes("r x l .");
        assert_eq!(vim_test_value(&view, cx), "xxcd");
    });
}

#[gpui::test]
fn dot_count_overrides_the_original_count_and_failed_commands_do_not_replace_it(
    cx: &mut gpui::TestAppContext,
) {
    with_vim_editor(cx, "", |view, cx| {
        set_vim_test_content(&view, "abcdefghij", Position::new(0, 0), cx);
        cx.simulate_keystrokes("2 x l 3 .");
        assert_eq!(vim_test_value(&view, cx), "cghij");

        set_vim_test_content(&view, "abcdef", Position::new(0, 0), cx);
        cx.simulate_keystrokes("x d p .");
        assert_eq!(vim_test_value(&view, cx), "cdef");
    });
}

#[gpui::test]
fn dot_repeats_text_objects_line_changes_paste_and_join(cx: &mut gpui::TestAppContext) {
    with_vim_editor(cx, "", |view, cx| {
        set_vim_test_content(&view, "say \"one\" then \"two\"", Position::new(0, 6), cx);
        cx.simulate_keystrokes("d i shift-'->\" w w .");
        assert_eq!(vim_test_value(&view, cx), "say \"\" then \"\"");

        set_vim_test_content(&view, "one tail\nnext tail", Position::new(0, 4), cx);
        cx.simulate_keystrokes("shift-c");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape j 0 w .");
        assert_eq!(vim_test_value(&view, cx), "one X\nnext X");

        set_vim_test_content(&view, "a\n  b\nc\n  d", Position::new(0, 0), cx);
        cx.simulate_keystrokes("shift-j j .");
        assert_eq!(vim_test_value(&view, cx), "a b\nc d");

        set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
        cx.simulate_keystrokes("y w shift-4->$ p .");
        assert_eq!(vim_test_value(&view, cx), "one twoone one ");
    });
}

#[gpui::test]
fn dot_captures_insert_backspace_markdown_continuation_and_insert_counts(
    cx: &mut gpui::TestAppContext,
) {
    with_vim_editor(cx, "", |view, cx| {
        set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
        cx.simulate_keystrokes("i");
        cx.simulate_input("abc");
        cx.simulate_keystrokes("backspace escape w .");
        assert_eq!(vim_test_value(&view, cx), "abone abtwo");

        set_vim_test_content(&view, "one two", Position::new(0, 0), cx);
        cx.simulate_keystrokes("i");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape w 3 .");
        assert_eq!(vim_test_value(&view, cx), "Xone XXXtwo");

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.kind = super::super::DocumentKind::Markdown;
                editor.replace_content_for_test("- one\n- two", window, cx);
                editor.editor.update(cx, |input, cx| {
                    input.set_cursor_position(Position::new(0, 5), window, cx);
                });
                editor.reset_vim_command();
                editor.focus_source_mode(window, cx);
            });
        });
        cx.simulate_keystrokes("shift-a enter");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape shift-g .");
        assert_eq!(vim_test_value(&view, cx), "- one\n- X\n- two\n- X");

        set_vim_test_content(&view, "one", Position::new(0, 0), cx);
        cx.simulate_keystrokes("o");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape 2 .");
        assert_eq!(vim_test_value(&view, cx), "one\nX\nX\nX");
    });
}

#[gpui::test]
fn visual_line_dot_is_one_modal_undo_step_and_redoes_as_one_step(cx: &mut gpui::TestAppContext) {
    with_vim_editor(cx, "one\ntwo\nthree\nfour\n", |view, cx| {
        cx.simulate_keystrokes("shift-v j d .");
        assert_eq!(vim_test_value(&view, cx), "");
        cx.simulate_keystrokes("u");
        assert_eq!(vim_test_value(&view, cx), "three\nfour\n");
        cx.simulate_keystrokes("ctrl-r");
        assert_eq!(vim_test_value(&view, cx), "");
    });
}

#[gpui::test]
fn compound_dot_replay_emits_one_editor_change(cx: &mut gpui::TestAppContext) {
    with_vim_editor(cx, "one two", |view, cx| {
        let changes = Rc::new(Cell::new(0));
        cx.update(|_, cx| {
            let input = view.read(cx).editor.clone();
            let changes = changes.clone();
            cx.subscribe(&input, move |_, event: &InputEvent, _| {
                if matches!(event, InputEvent::Change) {
                    changes.set(changes.get() + 1);
                }
            })
            .detach();
        });

        cx.simulate_keystrokes("c i w");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape");
        changes.set(0);

        cx.simulate_keystrokes("w .");
        assert_eq!(vim_test_value(&view, cx), "X X");
        assert_eq!(changes.get(), 1);
    });
}

#[gpui::test]
fn cancelled_and_failed_character_arguments_preserve_the_last_change(
    cx: &mut gpui::TestAppContext,
) {
    with_vim_editor(cx, "abcdef", |view, cx| {
        cx.simulate_keystrokes("x f escape l .");
        assert_eq!(vim_test_value(&view, cx), "bdef");

        set_vim_test_content(&view, "abc", Position::new(0, 0), cx);
        cx.simulate_keystrokes("x 9 r z .");
        assert_eq!(vim_test_value(&view, cx), "c");
    });
}

#[gpui::test]
fn modal_focus_edits_history_clipboard_search_and_live_settings(cx: &mut gpui::TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
    let _runtime_guard = runtime.enter();
    cx.executor().allow_parking();
    let (db, note_id) = runtime
        .block_on(async {
            let db = Database::connect("sqlite::memory:").await?;
            Migrator::up(&db, None).await?;
            let note = note::ActiveModel {
                title: Set("Vim test".into()),
                cached_content: Set("alpha beta".into()),
                created_at: Set(1),
                updated_at: Set(1),
                ..Default::default()
            }
            .insert(&db)
            .await?;
            Ok::<_, anyhow::Error>((db, note.id as u32))
        })
        .expect("Vim test database should initialize");
    let settings_dir = std::env::temp_dir().join(format!(
        "castle-vim-mode-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos()
    ));
    let mut editor_view = None;
    let window = cx.update(|cx| {
        cx.set_global(gpui_component::Theme::default());
        gpui_component::init(cx);
        cx.set_global(AppSettings::load(settings_dir));
        AppSettings::set_editor_vim_mode(true, cx);
        AppSettings::set_editor_status_line_visible(false, cx);
        crate::keymap::init(cx);
        cx.set_global(AppServices::new(Arc::new(db), PathBuf::new()));
        cx.open_window(Default::default(), |window, cx| {
            let view = DocumentEditorView::view(note_id, window, cx);
            editor_view = Some(view.clone());
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("Vim test window should open")
    });
    let view = editor_view.expect("document editor should exist");
    let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

    for _ in 0..100 {
        cx.run_until_parked();
        if view.read_with(&cx, |editor, _| !editor.persistence.is_loading) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let changes = Rc::new(Cell::new(0));
    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("alpha beta", window, cx);
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
            let input = editor.editor.clone();
            let changes = changes.clone();
            cx.subscribe(&input, move |_, _, event: &InputEvent, _| {
                if matches!(event, InputEvent::Change) {
                    changes.set(changes.get() + 1);
                }
            })
            .detach();
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });

    assert!(cx.debug_bounds("vim-mode-overlay").is_some());
    assert!(cx.update(|window, cx| view.read(cx).focus_handle.is_focused(window)));
    assert!(!cx.update(|window, cx| view.read(cx).editor.focus_handle(cx).is_focused(window)));

    cx.simulate_input("q");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "alpha beta"
    );
    assert_eq!(changes.get(), 0);

    cx.simulate_keystrokes("l y w");
    assert_eq!(changes.get(), 0);
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("lpha ".to_string())
    );

    cx.simulate_keystrokes("x");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "apha beta"
    );
    assert_eq!(changes.get(), 1);

    cx.simulate_keystrokes("u");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "alpha beta"
    );
    assert_eq!(changes.get(), 2);

    cx.simulate_keystrokes("ctrl-r");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "apha beta"
    );
    cx.simulate_keystrokes("u");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "alpha beta"
    );

    cx.simulate_keystrokes("i");
    assert!(cx.update(|window, cx| view.read(cx).editor.focus_handle(cx).is_focused(window)));
    cx.simulate_input("Z");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        view.read_with(&cx, |editor, _| editor.vim_mode()),
        VimMode::Normal
    );
    assert!(cx.update(|window, cx| view.read(cx).focus_handle.is_focused(window)));
    let after_insert = view.read_with(&cx, |editor, cx| editor.editor.read(cx).value().to_string());
    cx.simulate_input("q");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        after_insert
    );

    cx.simulate_keystrokes("ctrl-f");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let search_focus = cx.update(|window, cx| {
        let editor = view.read(cx);
        (
            editor.focus_handle.is_focused(window),
            editor.editor.focus_handle(cx).is_focused(window),
            editor.vim_state.search_active,
        )
    });
    assert!(
        search_focus.0,
        "unexpected search-close focus state: {search_focus:?}"
    );

    cx.update(|window, cx| {
        AppSettings::set_editor_vim_mode(false, cx);
        view.update(cx, |editor, cx| editor.sync_vim_setting(window, cx));
    });
    assert!(!view.read_with(&cx, |editor, _| editor.vim_is_enabled()));
    assert!(cx.update(|window, cx| view.read(cx).editor.focus_handle(cx).is_focused(window)));
    cx.simulate_input("!");
    assert_ne!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        after_insert
    );

    cx.update(|window, cx| {
        AppSettings::set_editor_vim_mode(true, cx);
        view.update(cx, |editor, cx| editor.sync_vim_setting(window, cx));
    });
    assert_eq!(
        view.read_with(&cx, |editor, _| editor.vim_mode()),
        VimMode::Normal
    );
    assert!(cx.update(|window, cx| view.read(cx).focus_handle.is_focused(window)));

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.kind = super::super::DocumentKind::Markdown;
            editor.mode = EditorMode::Source;
            editor.replace_content_for_test("- item", window, cx);
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    cx.simulate_keystrokes("shift-a enter");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "- item\n- "
    );
    assert_eq!(
        view.read_with(&cx, |editor, _| editor.vim_mode()),
        VimMode::Insert
    );
    cx.simulate_keystrokes("escape v l");
    assert_eq!(
        view.read_with(&cx, |editor, _| editor.vim_mode()),
        VimMode::Visual
    );
    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.set_mode(EditorMode::Preview, window, cx);
        });
    });
    assert_eq!(
        view.read_with(&cx, |editor, _| editor.vim_mode()),
        VimMode::Normal
    );
    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.set_mode(EditorMode::Source, window, cx);
        });
    });
    assert!(cx.update(|window, cx| view.read(cx).focus_handle.is_focused(window)));

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("Further testing showed", window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(0, 0), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
        let _ = window.draw(cx);
    });
    cx.simulate_keystrokes("v");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.simulate_keystrokes("i w");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    view.read_with(&cx, |editor, cx| {
        let range = editor
            .vim_visual_range(cx)
            .expect("viw should leave a Visual selection");
        assert_eq!(range, 0.."Further".len());
        let input = editor.editor.read(cx);
        let source_bounds = editor
            .analysis
            .source_bounds
            .expect("source bounds should be available after drawing");
        let selection = super::super::render::vim_selection_bounds(input, range, source_bounds);
        let cursor = super::super::render::vim_cursor_bounds(input, input.cursor())
            .expect("Visual cursor should have bounds");
        assert_eq!(selection.len(), 1);
        assert!(selection[0].size.width > cursor.size.width * 4.);
        assert_eq!(selection[0].size.height, cursor.size.height);
    });
    cx.simulate_keystrokes("y");
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("Further".to_string())
    );
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "Further testing showed"
    );

    cx.simulate_keystrokes("w d i w");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "Further  showed"
    );

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("one.two  three", window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(0, 0), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    let before_motion = changes.get();
    cx.simulate_keystrokes("shift-w shift-e 2 shift-b");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).cursor()),
        0
    );
    assert_eq!(changes.get(), before_motion);

    let before_delete_word = changes.get();
    cx.simulate_keystrokes("d shift-w");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "three"
    );
    assert_eq!(changes.get(), before_delete_word + 1);

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("a中b", window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(0, 2), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    let before_delete_previous = changes.get();
    cx.simulate_keystrokes("shift-x");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "ab"
    );
    assert_eq!(changes.get(), before_delete_previous + 1);

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("a中bc", window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(0, 0), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    let before_substitute = changes.get();
    cx.simulate_keystrokes("2 s");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "bc"
    );
    assert_eq!(changes.get(), before_substitute + 1);
    assert_eq!(
        view.read_with(&cx, |editor, _| editor.vim_mode()),
        VimMode::Insert
    );
    cx.simulate_input("Z");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "Zbc"
    );

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("  one\r\nnext", window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(0, 3), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    let before_substitute_line = changes.get();
    cx.simulate_keystrokes("shift-s");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "  \r\nnext"
    );
    assert_eq!(changes.get(), before_substitute_line + 1);
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).cursor()),
        2
    );
    cx.simulate_input("X");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "  X\r\nnext"
    );

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("one\ntwo", window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(0, 0), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    let before_yank_line = changes.get();
    cx.simulate_keystrokes("shift-y");
    assert_eq!(changes.get(), before_yank_line);
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("one\n".to_string())
    );

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("one  \r\n\t two \r\n中", window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(0, 0), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    let before_join = changes.get();
    cx.simulate_keystrokes("3 shift-j");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "one two 中"
    );
    assert_eq!(changes.get(), before_join + 1);

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test(
                "zero\none \"Further testing\" tail\nthree",
                window,
                cx,
            );
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(1, 8), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    let before_quote_selection = changes.get();
    cx.simulate_keystrokes("v i shift-'->\"");
    assert_eq!(
        view.read_with(&cx, |editor, _| editor.vim_mode()),
        VimMode::Visual
    );
    view.read_with(&cx, |editor, cx| {
        let range = editor
            .vim_visual_range(cx)
            .expect("vi double quote should select its contents");
        assert_eq!(
            editor.editor.read(cx).text().slice(range).to_string(),
            "Further testing"
        );
    });
    assert_eq!(changes.get(), before_quote_selection);
    cx.simulate_keystrokes("y");
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("Further testing".to_string())
    );

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("(\"I will testign some braces\")", window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(0, 10), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    let before_parenthesis_selection = changes.get();
    cx.simulate_keystrokes("v i shift-9->(");
    view.read_with(&cx, |editor, cx| {
        let range = editor
            .vim_visual_range(cx)
            .expect("vi parenthesis should select its contents");
        assert_eq!(
            editor.editor.read(cx).text().slice(range).to_string(),
            "\"I will testign some braces\""
        );
    });
    assert_eq!(changes.get(), before_parenthesis_selection);
    cx.simulate_keystrokes("escape");

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("  alpha", window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(0, 4), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    let before_symbol_motions = changes.get();
    cx.simulate_keystrokes("shift-6->^ shift-4->$");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).cursor()),
        6
    );
    assert_eq!(changes.get(), before_symbol_motions);

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("say \"naïve 中\" now", window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(0, 7), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    let before_delete_quote = changes.get();
    cx.simulate_keystrokes("d i shift-'->\"");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "say \"\" now"
    );
    assert_eq!(changes.get(), before_delete_quote + 1);
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("naïve 中".to_string())
    );

    cx.update(|window, cx| {
        view.update(cx, |editor, cx| {
            editor.replace_content_for_test("one\r\n two\r\nthree", window, cx);
            editor.editor.update(cx, |input, cx| {
                input.set_cursor_position(Position::new(0, 1), window, cx);
            });
            editor.reset_vim_command();
            editor.focus_source_mode(window, cx);
        });
    });
    let before_visual_line = changes.get();
    cx.simulate_keystrokes("shift-v j");
    assert_eq!(
        view.read_with(&cx, |editor, _| editor.vim_mode()),
        VimMode::VisualLine
    );
    view.read_with(&cx, |editor, cx| {
        let range = editor
            .vim_visual_range(cx)
            .expect("Vj should select two complete lines");
        assert_eq!(
            editor.editor.read(cx).text().slice(range).to_string(),
            "one\r\n two\r\n"
        );
    });
    assert_eq!(changes.get(), before_visual_line);
    cx.simulate_keystrokes("d");
    assert_eq!(
        view.read_with(&cx, |editor, cx| editor.editor.read(cx).value()),
        "three"
    );
    assert_eq!(changes.get(), before_visual_line + 1);
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("one\r\n two\r\n".to_string())
    );
}
