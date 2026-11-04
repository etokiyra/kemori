//! Virtual text rendering for inlay hints.

use super::*;

/// Hints for one logical row, keyed by the character column they precede.
pub(super) fn hints_for_row(doc: &Document, row: usize, char_count: usize) -> Vec<(usize, String)> {
    let mut hints: Vec<(usize, String)> = doc
        .inlay_hints()
        .iter()
        .filter(|hint| hint.position.row == row)
        .map(|hint| (hint.position.col.min(char_count), hint.label.clone()))
        .collect();
    hints.sort_by_key(|(col, _)| *col);
    hints
}

/// Code-lens titles for one logical row, in server order.
pub(super) fn code_lenses_for_row(doc: &Document, row: usize) -> Vec<String> {
    doc.code_lenses()
        .iter()
        .filter(|lens| lens.position.row == row)
        .map(|lens| lens.title.clone())
        .collect()
}

/// Append a row's code lenses, returning the display width consumed.
pub(super) fn push_code_lenses(
    context: &RowContext,
    spans: &mut Vec<Span>,
    base_bg: Option<Color>,
    last: bool,
) -> usize {
    if !last {
        return 0;
    }
    let mut width = 0;
    for title in &context.code_lenses {
        let label = format!("  {title}");
        width += label.chars().count();
        spans.push(Span::styled(
            label,
            with_bg(theme::muted().add_modifier(Modifier::ITALIC), base_bg),
        ));
    }
    width
}

/// Flush the pending text run and append any hints at `original`.
pub(super) fn push_hints(
    context: &RowContext,
    spans: &mut Vec<Span>,
    run: &mut String,
    run_style: &mut Option<Style>,
    original: usize,
    base_bg: Option<Color>,
) {
    for (_, label) in context
        .inlay_hints
        .iter()
        .filter(|(col, _)| *col == original)
    {
        if let Some(style) = run_style.take() {
            spans.push(Span::styled(std::mem::take(run), style));
        }
        spans.push(Span::styled(
            label.clone(),
            with_bg(theme::dim().add_modifier(Modifier::ITALIC), base_bg),
        ));
    }
}
