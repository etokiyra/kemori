// User read-only mode tests, included from `tests.rs`.

#[test]
fn read_only_blocks_edits_until_toggled_off() {
    let mut d = doc("alpha \nbeta");
    d.move_to(Position::new(0, 0));
    assert!(d.toggle_user_read_only());
    assert!(d.is_user_read_only());

    d.type_char('X');
    d.insert_text("Y");
    d.backspace();
    d.undo();
    assert_eq!(d.buffer.text(), "alpha \nbeta");

    assert!(!d.trim_trailing_whitespace());
    assert!(!d.ensure_final_newline());
    assert_eq!(d.buffer.text(), "alpha \nbeta");

    assert!(!d.toggle_user_read_only());
    d.type_char('X');
    assert_eq!(d.buffer.text(), "Xalpha \nbeta");
}
