//! Selection overlay geometry.

use super::*;

/// The selected column range on a row, if the selection covers it.
pub(super) fn selection_columns(doc: &Document, row: usize) -> Option<(usize, usize)> {
    if let Some((top, bottom, left, right)) = doc.column_bounds() {
        return (row >= top && row <= bottom).then_some((left, right));
    }
    let (start, end) = doc.selection_range()?;
    if row < start.row || row > end.row {
        return None;
    }
    let from = if row == start.row { start.col } else { 0 };
    let to = if row == end.row {
        end.col
    } else {
        doc.buffer.line_char_len(row)
    };
    Some((from, to))
}
