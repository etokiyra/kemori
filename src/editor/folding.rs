//! Folding: a view over rows, never an edit to the text.
//!
//! A fold is a header row plus the rows hidden beneath it. The buffer keeps
//! every line, so undo/redo, LSP positions and diagnostics are unaffected. The
//! editor owns this module's rules: movement and jumps land only on visible
//! rows, and an edit remaps or drops headers instead of leaving a fold over
//! unrelated text.
//!
//! Indentation is the zero-configuration default. When a language server
//! publishes `textDocument/foldingRange`, [`Document::set_server_folds`]
//! records those ranges and they take priority for the same header.

use std::collections::BTreeSet;

use crate::editor::document::Document;
use crate::editor::position::Position;

impl Document {
    /// The indentation of `row`, in characters.
    fn fold_indent(&self, row: usize) -> usize {
        let text = self.buffer.line_text(row);
        text.chars().take_while(|c| *c == ' ' || *c == '\t').count()
    }

    /// The rows hidden by folding `row`, if it is a foldable header.
    ///
    /// A server range wins when one was recorded for this header; otherwise the
    /// range is the run of more-indented lines below it, including blank lines.
    pub fn fold_range(&self, row: usize) -> Option<(usize, usize)> {
        let total = self.buffer.len_lines();
        if row + 1 >= total {
            return None;
        }
        if let Some(&end) = self.server_folds.get(&row) {
            let end = end.min(total.saturating_sub(1));
            if end > row {
                return Some((row + 1, end));
            }
        }
        let base = self.fold_indent(row);
        if self.fold_indent(row + 1) <= base {
            return None;
        }
        let mut end = row + 1;
        for candidate in (row + 2)..total {
            if self.fold_indent(candidate) > base {
                end = candidate;
            } else if self.buffer.line_text(candidate).trim().is_empty() {
                // Allow blank lines inside the fold.
                end = candidate;
            } else {
                break;
            }
        }
        (end > row).then_some((row + 1, end))
    }

    pub fn is_foldable(&self, row: usize) -> bool {
        self.fold_range(row).is_some()
    }

    /// Toggle the fold at the cursor.
    pub fn toggle_fold(&mut self) {
        let row = self.clamped_cursor().row;
        if self.folded.contains(&row) {
            self.folded.remove(&row);
        } else if self.fold_range(row).is_some() {
            self.folded.insert(row);
        }
        self.ensure_cursor_visible();
    }

    /// Fold the block at the cursor, if it is foldable.
    pub fn fold_at_cursor(&mut self) {
        let row = self.clamped_cursor().row;
        if self.fold_range(row).is_some() {
            self.folded.insert(row);
        }
        self.ensure_cursor_visible();
    }

    /// Unfold the fold at the cursor, if one is collapsed there.
    pub fn unfold_at_cursor(&mut self) {
        let row = self.clamped_cursor().row;
        self.folded.remove(&row);
    }

    /// Fold every foldable header.
    pub fn fold_all(&mut self) {
        let total = self.buffer.len_lines();
        for row in 0..total {
            if self.fold_range(row).is_some() {
                self.folded.insert(row);
            }
        }
        self.ensure_cursor_visible();
    }

    /// Remove every fold.
    pub fn unfold_all(&mut self) {
        self.folded.clear();
    }

    /// Replace the server-provided fold ranges.
    ///
    /// Each pair is `(header, last hidden line)`, both 0-based and inclusive.
    /// A server range never deletes an indentation fold that it does not cover;
    /// it only gives that header a more precise end.
    pub fn set_server_folds(&mut self, ranges: Vec<(usize, usize)>) {
        self.server_folds.clear();
        let total = self.buffer.len_lines();
        for (start, end) in ranges {
            if start < end && end < total {
                self.server_folds.insert(start, end);
            }
        }
        self.prune_folds();
    }

    /// Drop server ranges, e.g. after an edit invalidated them.
    pub fn clear_server_folds(&mut self) {
        self.server_folds.clear();
    }

    /// Whether `row` is hidden inside a collapsed fold.
    pub fn is_hidden(&self, row: usize) -> bool {
        self.folded.iter().any(|&header| {
            self.fold_range(header)
                .is_some_and(|(start, end)| row >= start && row <= end)
        })
    }

    /// The nearest visible row at or above `row`.
    pub(super) fn visible_row_at_or_above(&self, row: usize) -> usize {
        let mut row = row.min(self.buffer.len_lines().saturating_sub(1));
        while row > 0 && self.is_hidden(row) {
            row -= 1;
        }
        row
    }

    /// The nearest visible row at or below `row`.
    pub(super) fn visible_row_at_or_below(&self, row: usize) -> usize {
        let last = self.buffer.len_lines().saturating_sub(1);
        let mut row = row.min(last);
        while row < last && self.is_hidden(row) {
            row += 1;
        }
        row
    }

    /// The previous visible row, or `None` at the top.
    pub(super) fn visible_row_above(&self, row: usize) -> Option<usize> {
        if row == 0 {
            return None;
        }
        let mut candidate = row - 1;
        while self.is_hidden(candidate) {
            if candidate == 0 {
                return None;
            }
            candidate -= 1;
        }
        Some(candidate)
    }

    /// The next visible row, or `None` at the bottom.
    pub(super) fn visible_row_below(&self, row: usize) -> Option<usize> {
        let last = self.buffer.len_lines().saturating_sub(1);
        if row >= last {
            return None;
        }
        let mut candidate = row + 1;
        while self.is_hidden(candidate) {
            if candidate >= last {
                return None;
            }
            candidate += 1;
        }
        Some(candidate)
    }

    /// Move a position out of a collapsed fold, onto its header.
    pub(super) fn snap_out_of_fold(&self, pos: Position) -> Position {
        if !self.is_hidden(pos.row) {
            return pos;
        }
        let header = self.visible_row_at_or_above(pos.row);
        let col = pos.col.min(self.buffer.line_char_len(header));
        Position::new(header, col)
    }

    /// Keep the primary cursor on a visible row after a fold toggle.
    pub(super) fn ensure_cursor_visible(&mut self) {
        let snapped = self.snap_out_of_fold(self.cursor);
        if snapped != self.cursor {
            self.cursor = snapped;
            self.selection = None;
            self.cursors.clear();
            self.preferred_col = None;
        }
    }

    /// Remap fold headers after an edit.
    ///
    /// `start_row` is where the edit began, and the line counts describe the
    /// newlines removed and inserted. Headers below the edit shift with it;
    /// headers swallowed by a removal are dropped rather than left pointing at
    /// the wrong block.
    pub(super) fn remap_folds_after_edit(
        &mut self,
        start_row: usize,
        start_col: usize,
        removed_lines: usize,
        inserted_lines: usize,
    ) {
        if self.folded.is_empty() && self.server_folds.is_empty() {
            return;
        }
        let delta = inserted_lines as isize - removed_lines as isize;
        let removed_end = start_row + removed_lines;
        let remap = |row: usize| -> Option<usize> {
            if row < start_row {
                return Some(row);
            }
            if row > removed_end {
                return Some((row as isize + delta).max(0) as usize);
            }
            if row == start_row {
                if removed_lines == 0 {
                    // Inserting a line at column 0 moves the old header text
                    // down with it; inserting mid-line keeps the header row.
                    if start_col == 0 && inserted_lines > 0 {
                        return Some(row + inserted_lines);
                    }
                    return Some(row);
                }
                // The header's own line break was removed, so its block is no
                // longer identifiable.
                return None;
            }
            None
        };
        self.folded = self
            .folded
            .iter()
            .filter_map(|row| remap(*row))
            .collect::<BTreeSet<_>>();
        let old = std::mem::take(&mut self.server_folds);
        for (start, end) in old {
            if let Some(start) = remap(start) {
                let end = (end as isize + delta).max(0) as usize;
                self.server_folds.insert(start, end);
            }
        }
        self.prune_folds();
    }

    /// Drop headers that no longer describe a foldable block.
    pub(super) fn prune_folds(&mut self) {
        let total = self.buffer.len_lines();
        let folded = std::mem::take(&mut self.folded);
        self.folded = folded
            .into_iter()
            .filter(|row| *row < total && self.fold_range(*row).is_some())
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::buffer::Buffer;

    fn doc(text: &str) -> Document {
        Document::new(Buffer::from_text(text, None))
    }

    const BLOCK: &str = "fn main() {\n    let x = 1;\n    let y = 2;\n}\nfn other() {}\n";

    #[test]
    fn indentation_defines_a_fold_and_hides_below_the_header() {
        let d = doc(BLOCK);
        assert_eq!(d.fold_range(0), Some((1, 2)));
        assert_eq!(d.fold_range(3), None);
        assert!(!d.is_hidden(0));
        assert!(!d.is_hidden(3));
    }

    #[test]
    fn collapsing_a_fold_keeps_movement_on_visible_rows() {
        let mut d = doc(BLOCK);
        d.toggle_fold();
        assert!(d.is_hidden(1));
        assert!(d.is_hidden(2));

        d.move_down(false);
        assert_eq!(d.cursor.row, 3);
        d.move_up(false);
        assert_eq!(d.cursor.row, 0);

        // A jump into the hidden range lands on the header instead of
        // stranding the caret on an invisible line.
        d.move_to(Position::new(2, 4));
        assert_eq!(d.cursor.row, 0);
        assert!(!d.is_hidden(d.cursor.row));
    }

    #[test]
    fn moving_left_at_a_fold_boundary_skips_the_hidden_lines() {
        let mut d = doc(BLOCK);
        d.toggle_fold();
        d.move_to(Position::new(3, 0));
        d.move_left(false);
        assert_eq!(d.cursor.row, 0);
        assert_eq!(d.cursor.col, "fn main() {".len());
    }

    #[test]
    fn fold_all_and_unfold_all_are_bounded() {
        let mut d = doc("fn a() {\n    x\n}\nfn b() {\n    y\n}\n");
        d.fold_all();
        assert!(d.is_hidden(1));
        assert!(d.is_hidden(4));
        d.unfold_all();
        assert!(!d.is_hidden(1));
        assert!(!d.is_hidden(4));
    }

    #[test]
    fn inserting_lines_above_a_header_shifts_it() {
        let mut d = doc(BLOCK);
        d.move_to(Position::new(0, 0));
        d.toggle_fold();
        assert!(d.folded.contains(&0));

        d.move_to(Position::new(0, 0));
        d.insert_text("\n// new\n");
        // The header moved down by two lines and still hides its block.
        assert!(d.folded.contains(&2), "folded: {:?}", d.folded);
        assert!(d.is_hidden(3));
    }

    #[test]
    fn deleting_lines_above_a_header_shifts_it() {
        let mut d = doc("// one\n// two\nfn main() {\n    body\n}\n");
        d.move_to(Position::new(2, 0));
        d.toggle_fold();
        assert!(d.folded.contains(&2));

        d.replace_range(Position::new(0, 0), Position::new(1, 0), "");
        assert!(d.folded.contains(&1), "folded: {:?}", d.folded);
        assert!(d.is_hidden(2));
    }

    #[test]
    fn deleting_a_header_drops_its_fold() {
        let mut d = doc("fn main() {\n    body\n}\n");
        d.move_to(Position::new(0, 0));
        d.toggle_fold();
        d.replace_range(Position::new(0, 0), Position::new(1, 0), "");
        assert!(!d.folded.contains(&0));
        assert!(d.folded.is_empty());
    }

    #[test]
    fn server_ranges_win_over_indentation() {
        let mut d = doc("header\n  one\ntwo\n  three\n");
        d.set_server_folds(vec![(0, 3)]);
        assert_eq!(d.fold_range(0), Some((1, 3)));
        d.toggle_fold();
        assert!(d.is_hidden(2));
        assert!(d.is_hidden(3));
    }

    #[test]
    fn an_edit_clears_stale_server_ranges() {
        let mut d = doc(BLOCK);
        d.set_server_folds(vec![(0, 3)]);
        assert!(d.is_foldable(0));
        d.insert_text("x");
        assert!(d.server_folds.is_empty());
    }

    #[test]
    fn a_selection_can_end_on_a_visible_row_after_a_fold() {
        let mut d = doc(BLOCK);
        d.toggle_fold();
        d.move_down(true);
        assert_eq!(d.cursor.row, 3);
        assert!(d.selection.is_some());
        assert!(!d.is_hidden(d.cursor.row));
    }
}
