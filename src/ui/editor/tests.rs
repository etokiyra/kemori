use super::*;
use crate::editor::buffer::Buffer;
use crate::language::diagnostics::TextPos;

fn diagnostic(message: &str, severity: Severity) -> Diagnostic {
    Diagnostic::new(TextPos::new(0, 0), TextPos::new(0, 1), severity, message)
}

fn doc(text: &str) -> Document {
    Document::new(Buffer::from_text(text, None))
}

#[test]
fn code_lenses_are_collected_for_their_row() {
    let mut d = doc("fn main() {}\n");
    d.set_code_lenses(
        1,
        vec![crate::editor::CodeLens {
            position: Position::new(0, 0),
            end: Position::new(0, 3),
            title: "1 reference".to_string(),
            command: Some("test".to_string()),
            arguments: None,
        }],
    );
    let row = build_row(
        &mut d,
        &crate::language::rust::RustProvider,
        &Search::default(),
        0,
        &[],
        &std::collections::HashSet::new(),
    );
    assert_eq!(row.code_lenses, vec!["1 reference".to_string()]);
}

#[test]
fn inlay_hints_are_keyed_by_row_and_column() {
    let mut d = doc("let value = 1;");
    d.set_inlay_hints(
        1,
        vec![
            crate::editor::InlayHint {
                position: Position::new(0, 3),
                label: " : i32".to_string(),
                kind: crate::editor::InlayHintKind::Type,
                padding_left: true,
                padding_right: false,
            },
            crate::editor::InlayHint {
                position: Position::new(1, 0),
                label: "other".to_string(),
                kind: crate::editor::InlayHintKind::Other,
                padding_left: false,
                padding_right: false,
            },
        ],
    );
    let row = build_row(
        &mut d,
        &crate::language::rust::RustProvider,
        &Search::default(),
        0,
        &[],
        &std::collections::HashSet::new(),
    );
    assert_eq!(row.inlay_hints, vec![(3, " : i32".to_string())]);
}

#[test]
fn document_highlights_mark_the_expected_characters() {
    let mut d = doc("let value = value;");
    d.set_document_highlights(vec![
        (Position::new(0, 4), Position::new(0, 9)),
        (Position::new(0, 12), Position::new(0, 17)),
    ]);
    let row = build_row(
        &mut d,
        &crate::language::rust::RustProvider,
        &Search::default(),
        0,
        &[],
        &std::collections::HashSet::new(),
    );
    assert!(row.highlighted[4..9].iter().all(|flag| *flag));
    assert!(row.highlighted[12..17].iter().all(|flag| *flag));
    assert!(!row.highlighted[0]);
    assert!(!row.highlighted[9]);
}

#[test]
fn inline_note_fits_or_declines() {
    let diagnostic = diagnostic("unused variable `x`", Severity::Warning);
    let (note, severity) = inline_note(&diagnostic, 1, 40).expect("note");
    assert!(note.contains("unused variable"));
    assert_eq!(severity, Severity::Warning);
    // A line too narrow to say anything useful produces no note.
    assert!(inline_note(&diagnostic, 1, 8).is_none());
}

#[test]
fn inline_note_reports_additional_diagnostics() {
    let diagnostic = diagnostic("boom", Severity::Error);
    let (note, _) = inline_note(&diagnostic, 3, 40).unwrap();
    assert!(note.ends_with("(+2)"), "note was {note}");
}

#[test]
fn inline_note_truncates_and_stays_within_budget() {
    let diagnostic = diagnostic(&"x".repeat(100), Severity::Error);
    let (note, _) = inline_note(&diagnostic, 1, 30).unwrap();
    assert!(note.chars().count() <= 30);
    assert!(note.contains('…'));
}

#[test]
fn wrapped_scroll_keeps_the_cursor_inside_the_viewport() {
    let mut d = doc(&"x".repeat(400));
    d.wrap_width = 20;
    // Put the cursor at the far end of the very long line.
    d.move_to(Position::new(0, 400));
    scroll_viewport(&mut d, 20, 10, SCROLL_OFF);

    let cursor = d.clamped_cursor();
    let seg = segment_of(&d, cursor.row, cursor.col, 20);
    let offset = visual_offset(&d, 20, d.scroll_top, d.scroll_subline, cursor.row, seg);
    assert!(
        offset < 10,
        "cursor visual row {offset} should be inside a 10-row viewport"
    );
    // With scrolloff at the bottom, there is context below the cursor.
    assert!(d.scroll_top <= cursor.row);
}

#[test]
fn wrapped_scroll_backs_up_at_the_top() {
    // The cursor on line 1 with wrapping should leave scrolloff above it.
    let mut d = doc("first wrapped line that is quite long\nsecond\nthird\nfourth\nfifth\n");
    d.wrap_width = 10;
    d.move_to(Position::new(1, 0));
    scroll_viewport(&mut d, 10, 8, SCROLL_OFF);
    let cursor = d.clamped_cursor();
    let seg = segment_of(&d, cursor.row, cursor.col, 10);
    let offset = visual_offset(&d, 10, d.scroll_top, d.scroll_subline, cursor.row, seg);
    assert!(offset < 8);
}

#[test]
fn unwrapped_scroll_matches_logical_lines() {
    let mut d = doc(&(0..100).map(|i| format!("line {i}\n")).collect::<String>());
    d.wrap_width = 0;
    d.move_to(Position::new(90, 0));
    scroll_viewport(&mut d, 40, 10, SCROLL_OFF);
    assert!(d.scroll_top <= 90 && 90 < d.scroll_top + 10);
    assert_eq!(d.scroll_subline, 0);
}

#[test]
fn a_viewport_shorter_than_the_scroll_margin_keeps_the_cursor_visible() {
    // A bottom panel can leave one or two editor rows. The scroll margin
    // must shrink with the viewport instead of pushing the cursor off the
    // top, which used to show the end of the file while the statusline
    // still said line 1.
    let mut d = doc("one\ntwo\nthree\nfour\n");
    d.wrap_width = 0;
    d.move_to(Position::new(0, 0));
    scroll_viewport(&mut d, 40, 2, SCROLL_OFF);
    assert_eq!(d.scroll_top, 0);

    let mut wrapped = doc(&"x".repeat(80));
    wrapped.wrap_width = 20;
    wrapped.move_to(Position::new(0, 0));
    scroll_viewport(&mut wrapped, 20, 2, SCROLL_OFF);
    assert_eq!(wrapped.scroll_top, 0);
    assert_eq!(wrapped.scroll_subline, 0);
}
