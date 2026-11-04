// Large-file mode tests, included from `tests.rs`.

#[test]
fn large_files_skip_highlighting_and_indent_detection() {
    let text = "let value = 1;\n".repeat(LARGE_FILE_CHARS / 14 + 1);
    let mut d = doc(&text);
    assert!(d.is_large_file());
    assert_eq!(d.indent_width(), INDENT_WIDTH);
    assert!(
        d.highlight_spans(&crate::language::rust::RustProvider, 0)
            .is_empty(),
        "large files must render plain text"
    );

    let mut small = doc("let value = 1;\n");
    assert!(!small.is_large_file());
    assert!(!small
        .highlight_spans(&crate::language::rust::RustProvider, 0)
        .is_empty());
}
