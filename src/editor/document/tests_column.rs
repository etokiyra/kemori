// Rectangular selection tests, included from `tests.rs`.

#[test]
fn column_selection_spans_rows_and_deletes() {
    let mut d = doc("alpha\nbeta\ngamma");
    d.move_to(Position::new(0, 2));
    d.extend_column_right();
    d.extend_column_down();
    d.extend_column_down();

    assert_eq!(d.column_bounds(), Some((0, 2, 2, 3)));
    assert_eq!(d.selected_text().as_deref(), Some("p\nt\nm"));
    assert_eq!(
        d.selection_range(),
        Some((Position::new(0, 2), Position::new(2, 3)))
    );

    d.delete_selection();
    assert_eq!(d.buffer.text(), "alha\nbea\ngama");
    assert!(!d.has_column_selection());
}

#[test]
fn typing_replaces_a_column_selection_on_every_row() {
    let mut d = doc("alpha\nbeta\ngamma");
    d.move_to(Position::new(0, 2));
    d.extend_column_right();
    d.extend_column_down();
    d.extend_column_down();

    d.type_char('X');
    assert_eq!(d.buffer.text(), "alXha\nbeXa\ngaXma");
    assert!(d.has_multiple_cursors());
    assert!(!d.has_selection());
}

#[test]
fn column_selection_clamps_short_rows() {
    let mut d = doc("ab\nabcdef");
    d.move_to(Position::new(0, 0));
    d.extend_column_down();
    d.extend_column_right();
    d.extend_column_right();
    d.extend_column_right();

    // The lower row defines the right edge; the short first row stops at its end.
    assert_eq!(d.column_bounds(), Some((0, 1, 0, 3)));
    assert_eq!(d.selected_text().as_deref(), Some("ab\nabc"));
}

#[test]
fn column_selection_undo_restores_the_whole_block() {
    let mut d = doc("alpha\nbeta");
    d.move_to(Position::new(0, 1));
    d.extend_column_right();
    d.extend_column_down();
    d.type_char('X');
    assert_eq!(d.buffer.text(), "aXpha\nbXta");

    d.undo();
    assert_eq!(d.buffer.text(), "alpha\nbeta");
}
