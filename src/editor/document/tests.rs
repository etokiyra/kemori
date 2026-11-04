use super::*;
use crate::language::diagnostics::TextPos;

fn doc(text: &str) -> Document {
    Document::new(Buffer::from_text(text, None))
}

#[test]
fn insert_and_undo() {
    let mut d = doc("");
    d.insert_text("hello");
    assert_eq!(d.buffer.text(), "hello");
    d.undo();
    assert_eq!(d.buffer.text(), "");
    d.redo();
    assert_eq!(d.buffer.text(), "hello");
}

#[test]
fn backspace_joins_lines() {
    let mut d = doc("ab\ncd");
    d.move_to(Position::new(1, 0));
    d.backspace();
    assert_eq!(d.buffer.text(), "abcd");
}

#[test]
fn select_next_occurrence_adds_a_cursor_each_time() {
    let mut d = doc("let foo = foo + foo;");
    d.move_to(Position::new(0, 4));

    // First press selects the word under the cursor.
    d.select_next_occurrence();
    assert_eq!(d.selected_text().as_deref(), Some("foo"));
    assert_eq!(
        d.selection_range(),
        Some((Position::new(0, 4), Position::new(0, 7)))
    );
    assert!(!d.has_multiple_cursors());

    // Each further press keeps the previous occurrence as a secondary cursor
    // and moves the primary to the next one.
    d.select_next_occurrence();
    assert_eq!(
        d.selection_range(),
        Some((Position::new(0, 10), Position::new(0, 13)))
    );
    assert_eq!(d.cursors.len(), 1);
    assert_eq!(
        d.cursors[0].range(),
        (Position::new(0, 4), Position::new(0, 7))
    );

    d.select_next_occurrence();
    assert_eq!(
        d.selection_range(),
        Some((Position::new(0, 16), Position::new(0, 19)))
    );
    assert_eq!(d.cursors.len(), 2);

    // Every occurrence now has a cursor, so further presses stop growing.
    d.select_next_occurrence();
    assert_eq!(
        d.selection_range(),
        Some((Position::new(0, 16), Position::new(0, 19)))
    );
    assert_eq!(d.cursors.len(), 2);
}

#[test]
fn typing_replaces_every_selected_occurrence_in_one_undo_step() {
    let mut d = doc("foo = foo + foo;");
    d.move_to(Position::new(0, 0));
    d.select_next_occurrence();
    d.select_next_occurrence();
    d.select_next_occurrence();
    assert_eq!(d.cursors.len(), 2);

    d.insert_text("bar");
    assert_eq!(d.buffer.text(), "bar = bar + bar;");

    // One undo restores all three replacements.
    d.undo();
    assert_eq!(d.buffer.text(), "foo = foo + foo;");
    d.redo();
    assert_eq!(d.buffer.text(), "bar = bar + bar;");
}

#[test]
fn select_all_occurrences_puts_a_cursor_on_each() {
    let mut d = doc("let x = 1;\nlet y = 2;\nlet z = 3;\n");
    d.move_to(Position::new(0, 0));
    d.select_all_occurrences();
    // "let" appears three times: primary plus two secondaries.
    assert_eq!(d.selected_text().as_deref(), Some("let"));
    assert_eq!(d.cursors.len(), 2);
    d.insert_text("var");
    assert_eq!(d.buffer.text(), "var x = 1;\nvar y = 2;\nvar z = 3;\n");
}

#[test]
fn add_cursor_below_stacks_carets_and_types_across_lines() {
    let mut d = doc("aaa\naaa\naaa\n");
    d.move_to(Position::new(0, 3));
    d.add_cursor_below();
    d.add_cursor_below();
    // Primary on line 0, carets on lines 1 and 2: three in total.
    assert_eq!(d.cursors.len() + 1, 3);
    d.type_char('!');
    assert_eq!(d.buffer.text(), "aaa!\naaa!\naaa!\n");
    // Selections are distinct and the whole thing undoes in one step.
    d.undo();
    assert_eq!(d.buffer.text(), "aaa\naaa\naaa\n");
}

#[test]
fn multi_cursor_backspace_and_newline() {
    let mut d = doc("ab\nab\nab\n");
    d.move_to(Position::new(0, 1));
    d.add_cursor_below();
    d.add_cursor_below();
    d.backspace();
    assert_eq!(d.buffer.text(), "b\nb\nb\n");

    // Undo the backspaces, then insert a newline at every caret.
    d.undo();
    assert_eq!(d.buffer.text(), "ab\nab\nab\n");
    d.move_to(Position::new(0, 2));
    d.add_cursor_below();
    d.add_cursor_below();
    d.insert_newline();
    assert_eq!(d.buffer.text(), "ab\n\nab\n\nab\n\n");
}

#[test]
fn multi_cursor_indent_is_grouped_and_deduplicates_rows() {
    let mut d = doc("one\ntwo\nthree\n");
    d.move_to(Position::new(0, 0));
    d.add_cursor_below();
    d.add_cursor_below();
    // Two cursors land on the same line as the primary is moved; ensure a
    // row is never indented twice.
    d.add_cursor_below();
    d.indent();
    assert_eq!(d.buffer.text(), "    one\n    two\n    three\n");
    d.undo();
    assert_eq!(d.buffer.text(), "one\ntwo\nthree\n");
}

#[test]
fn clear_selection_ends_a_multi_cursor_session() {
    let mut d = doc("a a a");
    d.move_to(Position::new(0, 0));
    d.select_all_occurrences();
    assert!(d.has_multiple_cursors());
    d.clear_selection();
    assert!(!d.has_multiple_cursors());
}

#[test]
fn moving_the_cursor_collapses_multi_cursor() {
    let mut d = doc("a a a");
    d.move_to(Position::new(0, 0));
    d.select_all_occurrences();
    assert!(d.has_multiple_cursors());
    d.move_left(false);
    assert!(!d.has_multiple_cursors());
}

#[test]
fn multi_cursor_delete_line_removes_each_line() {
    let mut d = doc("one\ntwo\nthree\n");
    d.move_to(Position::new(0, 0));
    d.add_cursor_below();
    // Carets on rows 0 and 1; `delete_line` acts on the primary only and
    // collapses, so exactly one line is removed.
    d.delete_line();
    assert_eq!(d.cursors.len(), 0);
    assert_eq!(d.buffer.text(), "two\nthree\n");
}

#[test]
fn auto_indent_on_newline() {
    let mut d = doc("    let x = 1;");
    d.move_end(false);
    d.insert_newline();
    assert_eq!(d.buffer.text(), "    let x = 1;\n    ");
}

#[test]
fn detects_and_uses_the_file_indentation_unit() {
    // Two-space JavaScript.
    let javascript = doc("function f() {\n  return 1;\n}");
    assert_eq!(javascript.indent_width(), 2);

    // Four-space Rust.
    let rust = doc("fn main() {\n    let x = 1;\n}");
    assert_eq!(rust.indent_width(), 4);

    // Tab-indented files keep the default width for inserted spaces.
    let tabbed = doc("fn main() {\n\tlet x = 1;\n}");
    assert_eq!(tabbed.indent_width(), INDENT_WIDTH);

    // New lines adopt the detected unit after an opening brace.
    let mut d = doc("function f() {\n  return 1;\n}");
    d.move_to(Position::new(0, 14));
    d.insert_newline();
    assert_eq!(d.buffer.text(), "function f() {\n  \n  return 1;\n}");
}

#[test]
fn indent_and_outdent_use_the_detected_unit() {
    let mut d = doc("function f() {\n  return 1;\n}");
    d.selection = Some(Selection::new(Position::new(1, 0)));
    d.cursor = Position::new(1, 2);
    d.outdent();
    assert_eq!(d.buffer.text(), "function f() {\nreturn 1;\n}");

    d.indent();
    assert_eq!(d.buffer.text(), "function f() {\n  return 1;\n}");
}

#[test]
fn indent_preference_overrides_detection() {
    let mut d = doc("function f() {\n  return 1;\n}");
    assert_eq!(d.indent_width(), 2, "detected from the file");

    d.set_indent_preference(Some(8), true);
    assert_eq!(d.indent_width(), 8, "pinned width wins");

    d.set_indent_preference(None, true);
    assert_eq!(d.indent_width(), 2, "auto returns to inference");
}

#[test]
fn pinned_indent_width_is_used_for_indentation() {
    let mut d = doc("fn main() {}");
    d.set_indent_preference(Some(8), true);
    d.move_to(Position::new(0, 0));
    d.indent();
    assert!(d.buffer.line_text(0).starts_with(&" ".repeat(8)));
}

#[test]
fn tab_preference_expands_empty_pairs_with_a_tab() {
    let mut d = doc("{}");
    d.set_indent_preference(Some(4), false);
    d.move_to(Position::new(0, 1));
    d.insert_newline();
    assert_eq!(d.buffer.text(), "{\n\t\n}");
}

#[test]
fn tab_preference_inserts_and_removes_a_tab() {
    let mut d = doc("fn main() {}");
    d.set_indent_preference(Some(4), false);
    d.move_to(Position::new(0, 0));
    d.indent();
    assert!(d.buffer.line_text(0).starts_with('\t'));
    d.outdent();
    assert_eq!(d.buffer.line_text(0), "fn main() {}");
}

#[test]
fn selection_delete() {
    let mut d = doc("hello world");
    d.selection = Some(Selection::new(Position::new(0, 0)));
    d.cursor = Position::new(0, 5);
    assert_eq!(d.selected_text().as_deref(), Some("hello"));
    d.delete_selection();
    assert_eq!(d.buffer.text(), " world");
}

#[test]
fn find_all_matches() {
    let d = doc("foo bar foo");
    let matches = d.find_all("foo");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0], (Position::new(0, 0), Position::new(0, 3)));
    assert_eq!(matches[1], (Position::new(0, 8), Position::new(0, 11)));
}

#[test]
fn find_all_regex_matches_and_reports_errors() {
    let d = doc("let count = 42;\nlet total = 7;");
    let matches = d.find_all_regex(r"\d+", false).unwrap();
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0], (Position::new(0, 12), Position::new(0, 14)));
    assert_eq!(matches[1], (Position::new(1, 12), Position::new(1, 13)));

    assert!(d.find_all_regex("(a|b)", false).is_err());
}

#[test]
fn find_all_honours_case_and_whole_word() {
    let d = doc("Foo foo food foo");

    // Case-insensitive by default for the find bar.
    assert_eq!(d.find_all_with("foo", false, false).len(), 4);
    assert_eq!(d.find_all_with("foo", true, false).len(), 3);

    // Whole word excludes `food`.
    let whole = d.find_all_with("foo", false, true);
    assert_eq!(
        whole,
        vec![
            (Position::new(0, 0), Position::new(0, 3)),
            (Position::new(0, 4), Position::new(0, 7)),
            (Position::new(0, 13), Position::new(0, 16)),
        ]
    );
}

#[test]
fn word_movement() {
    let mut d = doc("let value = 1;");
    d.move_word_right(false);
    assert_eq!(d.cursor, Position::new(0, 3));
    d.move_word_right(false);
    assert_eq!(d.cursor, Position::new(0, 9));
}

#[test]
fn wrapped_vertical_movement_steps_through_visual_rows() {
    let mut d = doc("abcdefghij klmnopqrst uvwxyz\nshort\n");
    d.wrap_width = 10;
    d.move_to(Position::new(0, 0));

    // Down moves within the same logical line, visual row by visual row.
    d.move_down(false);
    assert_eq!(d.cursor.row, 0, "still on the wrapped line");
    assert!(d.cursor.col > 0, "moved to a later visual row");
    let second = d.cursor.col;

    d.move_down(false);
    assert_eq!(d.cursor.row, 0);
    assert!(d.cursor.col > second);

    // One more step leaves the wrapped line for the next logical line.
    d.move_down(false);
    assert_eq!(d.cursor.row, 1);
}

#[test]
fn wrapped_movement_keeps_the_display_column() {
    // The two logical lines wrap to different widths; the display column
    // (not the character column) should be preserved across the step.
    let mut d = doc("aaaaaaaaaa bbbb\nccccc ddddd eeeee\n");
    d.wrap_width = 11;
    d.move_to(Position::new(0, 7)); // display column 7
    d.move_down(false); // next visual row of line 0
    assert_eq!(d.cursor.row, 0);
    d.move_down(false); // line 1, first visual row
    assert_eq!(d.cursor.row, 1);
    assert_eq!(d.display_col_of(1, d.cursor.col), 7);
}

#[test]
fn wrapping_does_not_change_the_text_or_undo() {
    let mut d = doc("one long line that will wrap\n");
    d.wrap_width = 8;
    assert!(d.row_segments(0, 8).len() > 1);
    d.move_end(false);
    d.insert_text("!");
    assert_eq!(d.buffer.text(), "one long line that will wrap!\n");
    d.undo();
    assert_eq!(d.buffer.text(), "one long line that will wrap\n");
}

#[test]
fn matches_brackets_on_current_line() {
    let service = crate::language::LanguageService::builtin();
    let provider = service.provider(LanguageId::Rust);
    let mut d = doc("fn main() {\n    let x = (1 + 2);\n}\n");
    d.move_to(Position::new(1, 13));
    assert_eq!(
        d.matching_brackets(provider),
        Some((Position::new(1, 12), Position::new(1, 18)))
    );
}

#[test]
fn matches_brackets_across_lines() {
    let service = crate::language::LanguageService::builtin();
    let provider = service.provider(LanguageId::Rust);
    let mut d = doc("fn main() {\n}\n");
    d.move_to(Position::new(0, 11));
    assert_eq!(
        d.matching_brackets(provider),
        Some((Position::new(0, 10), Position::new(1, 0)))
    );
}

#[test]
fn unmatched_bracket_returns_none() {
    let service = crate::language::LanguageService::builtin();
    let provider = service.provider(LanguageId::Rust);
    let mut d = doc("fn main() {\n");
    d.move_to(Position::new(0, 11));
    assert_eq!(d.matching_brackets(provider), None);
}

#[test]
fn goto_matching_bracket_jumps_to_the_partner() {
    let service = crate::language::LanguageService::builtin();
    let provider = service.provider(LanguageId::Rust);
    let mut d = doc("fn main() {}\n");

    d.move_to(Position::new(0, 7)); // on `(`
    d.goto_matching_bracket(provider);
    assert_eq!(d.cursor, Position::new(0, 8));

    d.move_to(Position::new(0, 10)); // on `{`
    d.goto_matching_bracket(provider);
    assert_eq!(d.cursor, Position::new(0, 11));
}

#[test]
fn delete_line_removes_the_current_line() {
    let mut d = doc("one\ntwo\nthree\n");
    d.move_to(Position::new(1, 1));
    d.delete_line();
    assert_eq!(d.buffer.text(), "one\nthree\n");
    assert_eq!(d.cursor, Position::new(1, 0));
}

#[test]
fn typing_coalesces_into_one_undo_step() {
    let mut d = doc("");
    d.type_char('a');
    d.type_char('b');
    d.type_char('c');
    assert_eq!(d.buffer.text(), "abc");
    d.undo();
    assert_eq!(d.buffer.text(), "");
    d.redo();
    assert_eq!(d.buffer.text(), "abc");
}

#[test]
fn movement_breaks_the_undo_group() {
    let mut d = doc("");
    d.type_char('a');
    d.type_char('b');
    d.move_left(false);
    d.type_char('c');
    assert_eq!(d.buffer.text(), "acb");
    d.undo();
    assert_eq!(d.buffer.text(), "ab");
}

#[test]
fn backspace_coalesces() {
    let mut d = doc("abcd");
    d.move_end(false);
    d.backspace();
    d.backspace();
    assert_eq!(d.buffer.text(), "ab");
    d.undo();
    assert_eq!(d.buffer.text(), "abcd");
}

#[test]
fn coalesced_backspace_undo_restores_the_cursor() {
    // Regression: merging backspaces overwrote `cursor_before`, so undo
    // left the cursor one character short of where the run began.
    let mut d = doc("abcd");
    d.move_end(false);
    d.backspace();
    d.backspace();
    d.undo();
    assert_eq!(d.cursor, Position::new(0, 4));
    d.redo();
    assert_eq!(d.cursor, Position::new(0, 2));
}

#[test]
fn undoing_to_the_save_point_marks_the_document_clean() {
    let mut d = doc("hello");
    d.mark_saved();
    assert!(!d.is_dirty());

    d.insert_text("!");
    assert!(d.is_dirty());
    d.undo();
    assert!(
        !d.is_dirty(),
        "undoing the only edit returns to the save point"
    );

    d.redo();
    assert!(d.is_dirty(), "redoing past the save point is dirty again");
}

#[test]
fn indent_skips_a_trailing_line_selected_at_column_zero() {
    // Regression: Shift+Down leaves the cursor at column 0 of the row after
    // the highlighted lines; that row used to be indented too.
    let mut d = doc("a\nb\nc");
    d.selection = Some(Selection::new(Position::new(0, 0)));
    d.cursor = Position::new(2, 0);
    d.indent();
    assert_eq!(d.buffer.text(), "    a\n    b\nc");
}

#[test]
fn delete_line_skips_a_trailing_line_selected_at_column_zero() {
    let mut d = doc("a\nb\nc");
    d.selection = Some(Selection::new(Position::new(0, 0)));
    d.cursor = Position::new(2, 0);
    d.delete_line();
    assert_eq!(d.buffer.text(), "c");
}

#[test]
fn auto_pairs_and_skips_closing() {
    let mut d = doc("");
    d.type_char('(');
    assert_eq!(d.buffer.text(), "()");
    assert_eq!(d.cursor, Position::new(0, 1));
    d.type_char(')');
    assert_eq!(d.buffer.text(), "()");
    assert_eq!(d.cursor, Position::new(0, 2));
}

#[test]
fn auto_pair_wraps_a_selection() {
    let mut d = doc("foo");
    d.selection = Some(Selection::new(Position::new(0, 0)));
    d.cursor = Position::new(0, 3);
    d.type_char('(');
    assert_eq!(d.buffer.text(), "(foo)");
    assert_eq!(d.cursor, Position::new(0, 4));
}

#[test]
fn backspace_removes_an_empty_pair() {
    let mut d = doc("");
    d.type_char('(');
    d.backspace();
    assert_eq!(d.buffer.text(), "");
}

#[test]
fn smart_newline_expands_braces() {
    let mut d = doc("");
    d.type_char('{');
    d.insert_newline();
    assert_eq!(d.buffer.text(), "{\n    \n}");
    assert_eq!(d.cursor, Position::new(1, 4));
}

#[test]
fn indent_and_outdent_selection() {
    let mut d = doc("a\nb\nc");
    d.selection = Some(Selection::new(Position::new(0, 0)));
    d.cursor = Position::new(1, 1);
    d.indent();
    assert_eq!(d.buffer.text(), "    a\n    b\nc");
    d.outdent();
    assert_eq!(d.buffer.text(), "a\nb\nc");
}

#[test]
fn tab_inserts_to_the_next_stop() {
    let mut d = doc("ab");
    d.move_end(false);
    d.indent();
    assert_eq!(d.buffer.text(), "ab  ");
    let mut d = doc("");
    d.indent();
    assert_eq!(d.buffer.text(), "    ");
}

#[test]
fn move_line_up_and_down() {
    let mut d = doc("a\nb\nc");
    d.move_to(Position::new(1, 0));
    d.move_line_up();
    assert_eq!(d.buffer.text(), "b\na\nc");
    assert_eq!(d.cursor.row, 0);
    d.move_line_down();
    assert_eq!(d.buffer.text(), "a\nb\nc");
    assert_eq!(d.cursor.row, 1);
}

#[test]
fn duplicate_line() {
    let mut d = doc("a\nb");
    d.move_to(Position::new(0, 0));
    d.duplicate_line();
    assert_eq!(d.buffer.text(), "a\na\nb");
    assert_eq!(d.cursor.row, 1);
}

#[test]
fn diagnostics_apply_and_reject_stale_results() {
    let mut d = doc("fn main() {\n");
    let diagnostic = Diagnostic::new(
        TextPos::new(0, 10),
        TextPos::new(0, 11),
        Severity::Error,
        "unclosed `{`",
    );

    d.set_diagnostics_revision(2);
    assert!(d.apply_diagnostics(2, vec![diagnostic.clone()]));
    assert_eq!(d.diagnostics().len(), 1);
    assert_eq!(d.diagnostic_counts(), (1, 0));
    assert_eq!(d.diagnostic_severity_on_line(0), Some(Severity::Error));
    assert_eq!(d.diagnostic_severity_on_line(1), None);

    // A result from an older request must not replace the current one.
    assert!(!d.apply_diagnostics(1, vec![diagnostic]));
}

#[test]
fn editing_clears_diagnostics_and_marks_them_dirty() {
    let mut d = doc("fn main() {\n");
    d.set_diagnostics_revision(1);
    d.apply_diagnostics(
        1,
        vec![Diagnostic::new(
            TextPos::new(0, 10),
            TextPos::new(0, 11),
            Severity::Error,
            "unclosed `{`",
        )],
    );
    assert!(!d.diagnostics().is_empty());
    assert!(!d.diagnostics_dirty());

    d.insert_text("// hi\n");
    assert!(d.diagnostics().is_empty());
    assert!(d.diagnostics_dirty());
}

#[test]
fn lsp_diagnostics_replace_builtin_ones() {
    let mut d = doc("fn main() {\n}\n");
    d.set_diagnostics_revision(1);
    d.apply_diagnostics(
        1,
        vec![Diagnostic::new(
            TextPos::new(0, 0),
            TextPos::new(0, 1),
            Severity::Error,
            "builtin",
        )],
    );
    assert!(!d.diagnostics_from_lsp());

    d.set_lsp_diagnostics(vec![
        Diagnostic::new(
            TextPos::new(1, 0),
            TextPos::new(1, 1),
            Severity::Warning,
            "lsp warn",
        ),
        Diagnostic::new(
            TextPos::new(0, 2),
            TextPos::new(0, 3),
            Severity::Error,
            "lsp err",
        ),
    ]);
    assert!(d.diagnostics_from_lsp());
    // Sorted by position regardless of arrival order.
    assert_eq!(d.diagnostics()[0].message, "lsp err");
    assert_eq!(d.diagnostic_counts(), (1, 1));

    // Handing ownership back clears them and re-arms the built-in pass.
    d.use_builtin_diagnostics();
    assert!(!d.diagnostics_from_lsp());
    assert!(d.diagnostics().is_empty());
    assert_eq!(d.diagnostics_revision(), 0);
}

#[test]
fn breakpoint_conditions_and_logpoints_round_trip() {
    let mut d = doc("one\ntwo\nthree\n");
    d.move_to(Position::new(1, 0));
    d.toggle_breakpoint();
    assert!(d.has_breakpoint(1));
    d.set_breakpoint_condition(1, Some("i > 2".to_string()));
    assert!(d.breakpoint_is_conditional(1));
    assert_eq!(
        d.breakpoint_conditions.get(&1).map(String::as_str),
        Some("i > 2")
    );

    // A logpoint replaces the condition; both never apply at once.
    d.set_breakpoint_log(2, Some("x = {x}".to_string()));
    assert!(d.breakpoint_is_conditional(2));
    assert!(!d.breakpoint_conditions.contains_key(&2));

    // Toggling a breakpoint off clears its extras.
    d.toggle_breakpoint();
    assert!(!d.has_breakpoint(1));
    assert!(!d.breakpoint_is_conditional(1));

    // An empty message clears the logpoint but leaves the breakpoint.
    d.set_breakpoint_log(2, None);
    assert!(d.has_breakpoint(2));
    assert!(!d.breakpoint_is_conditional(2));
}

#[test]
fn history_round_trips_on_a_matching_document() {
    let mut d = doc("a");
    d.insert_text("bc");
    let snapshot = d.history_snapshot();

    let mut restored = doc("bca");
    assert!(restored.restore_history(&snapshot));
    assert!(restored.can_undo());
    restored.undo();
    assert_eq!(restored.buffer.text(), "a");
}

#[test]
fn changes_line_endings_and_writes_them_on_save() {
    let dir = std::env::temp_dir().join(format!("kemori-eol-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.rs");
    std::fs::write(&path, "a\nb\n").unwrap();
    let mut d = Document::from_path(&path).unwrap();

    assert!(d.set_line_ending(LineEnding::Crlf));
    assert!(d.is_dirty());
    // The rope is unchanged; conversion happens on save.
    assert_eq!(d.buffer.text(), "a\nb\n");
    d.save().unwrap();
    assert!(!d.is_dirty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\r\nb\r\n");

    assert!(d.set_line_ending(LineEnding::Lf));
    d.save().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\nb\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn trims_trailing_whitespace_as_one_undo_step() {
    let mut d = doc("a  \n\tb\t\n\nc   ");
    assert!(d.trim_trailing_whitespace());
    assert_eq!(d.buffer.text(), "a\n\tb\n\nc");
    assert!(d.can_undo());
    d.undo();
    assert_eq!(d.buffer.text(), "a  \n\tb\t\n\nc   ");
}

#[test]
fn adds_a_final_newline_only_when_missing() {
    let mut d = doc("a\nb");
    assert!(d.ensure_final_newline());
    assert_eq!(d.buffer.text(), "a\nb\n");
    assert!(!d.ensure_final_newline());
}

#[test]
fn reload_from_disk_replaces_contents_and_cleans() {
    let dir = std::env::temp_dir().join(format!("kemori-reload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.rs");
    std::fs::write(&path, "fn a() {}\n").unwrap();

    let mut d = Document::from_path(&path).unwrap();
    d.move_to(Position::new(0, 3));
    d.insert_text("// edit\n");
    assert!(d.is_dirty());
    assert!(d.can_undo());

    std::fs::write(&path, "fn b() {}\n").unwrap();
    assert!(d.reload_from_disk().unwrap());
    assert_eq!(d.buffer.text(), "fn b() {}\n");
    assert!(!d.is_dirty());
    assert!(!d.can_undo());

    let _ = std::fs::remove_dir_all(&dir);
}

include!("tests_column.rs");
include!("tests_large.rs");
include!("tests_read_only.rs");
