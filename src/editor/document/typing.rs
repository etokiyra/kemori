//! Extracted from `document/mod.rs`; see `docs/DECISIONS.md`.

use super::*;

impl Document {
    pub fn insert_text(&mut self, text: &str) {
        if self.has_column_selection() {
            if !self.replace_column_selection(text, None) {
                self.selection = None;
                let (start, end) = (self.cursor, self.cursor);
                self.apply_edit(start, end, text);
            }
            return;
        }
        if self.cursors.is_empty() {
            let (start, end) = self.selection_range().unwrap_or((self.cursor, self.cursor));
            self.apply_edit(start, end, text);
            return;
        }
        let specs = self.all_cursor_specs();
        let mut edits = Vec::with_capacity(specs.len());
        let mut cursors = Vec::with_capacity(specs.len());
        for (start, end, _) in specs {
            let after = start.advanced_by(text);
            edits.push(MultiEdit {
                start,
                end,
                inserted: text.to_string(),
            });
            cursors.push(Cursor::caret(after));
        }
        self.apply_multi_edit(edits, cursors, None);
    }

    pub fn insert_newline(&mut self) {
        if self.has_column_selection() {
            if self.delete_column_selection() {
                self.insert_newline_single();
            } else {
                self.selection = None;
                self.insert_newline_single();
            }
            return;
        }
        if self.cursors.is_empty() {
            self.insert_newline_single();
            return;
        }
        let specs = self.all_cursor_specs();
        let mut edits = Vec::with_capacity(specs.len());
        let mut cursors = Vec::with_capacity(specs.len());
        for (start, end, caret) in specs {
            let (edit, cursor) = self.plan_newline(start, end, caret);
            edits.push(edit);
            cursors.push(cursor);
        }
        self.apply_multi_edit(edits, cursors, None);
    }

    pub(super) fn insert_newline_single(&mut self) {
        let line = self.buffer.line_text(self.cursor.row);
        let leading: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        let before: String = line.chars().take(self.cursor.col).collect();
        let after = self.char_at(self.cursor);

        // Expanding an empty pair: `{|}` becomes a three-line indented block.
        if let Some(open) = before.chars().last().filter(|c| is_open_bracket(*c))
            && after == matching_close(open)
        {
            let outer = leading;
            let inner = format!("{outer}{}", self.indent_unit());
            let text = format!("\n{inner}\n{outer}");
            self.apply_edit(self.cursor, self.cursor, &text);
            self.cursor = Position::new(self.cursor.row.saturating_sub(1), inner.chars().count());
            return;
        }

        // Otherwise keep the indentation, deepening after an opening bracket.
        let mut indent = if line.trim().is_empty() && self.cursor.row > 0 {
            self.buffer
                .line_text(self.cursor.row - 1)
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>()
        } else {
            leading
        };
        if before.trim_end().ends_with(['{', '(', '[']) {
            indent.push_str(&self.indent_unit());
        }
        let text = format!("\n{indent}");
        let (start, end) = self.selection_range().unwrap_or((self.cursor, self.cursor));
        self.apply_edit(start, end, &text);
    }

    pub fn backspace(&mut self) {
        if self.cursors.is_empty() {
            self.backspace_single();
            return;
        }
        let specs = self.all_cursor_specs();
        let mut edits = Vec::with_capacity(specs.len());
        let mut cursors = Vec::with_capacity(specs.len());
        for (start, end, caret) in specs {
            let (edit, cursor) = self.plan_backspace(start, end, caret);
            edits.push(edit);
            cursors.push(cursor);
        }
        self.apply_multi_edit(edits, cursors, None);
    }

    pub(super) fn backspace_single(&mut self) {
        if self.delete_selection_single() {
            return;
        }
        if self.cursor.col > 0 {
            let before = self.char_at(Position::new(self.cursor.row, self.cursor.col - 1));
            let at = self.char_at(self.cursor);
            // Deleting into an empty pair removes both halves.
            if let (Some(open), Some(close)) = (before, at)
                && matching_close(open) == Some(close)
            {
                let start = Position::new(self.cursor.row, self.cursor.col - 1);
                let end = Position::new(self.cursor.row, self.cursor.col + 1);
                self.apply_edit(start, end, "");
                return;
            }
            let start = Position::new(self.cursor.row, self.cursor.col - 1);
            self.apply_edit_coalesced(start, self.cursor, "", Some(Coalesce::DeleteBackward));
        } else if self.cursor.row > 0 {
            let prev_row = self.cursor.row - 1;
            let start = Position::new(prev_row, self.buffer.line_char_len(prev_row));
            self.apply_edit_coalesced(start, self.cursor, "", Some(Coalesce::DeleteBackward));
        }
    }

    pub fn delete_forward(&mut self) {
        if self.cursors.is_empty() {
            self.delete_forward_single();
            return;
        }
        let specs = self.all_cursor_specs();
        let mut edits = Vec::with_capacity(specs.len());
        let mut cursors = Vec::with_capacity(specs.len());
        for (start, end, caret) in specs {
            let (edit, cursor) = self.plan_delete_forward(start, end, caret);
            edits.push(edit);
            cursors.push(cursor);
        }
        self.apply_multi_edit(edits, cursors, None);
    }

    pub(super) fn delete_forward_single(&mut self) {
        if self.delete_selection_single() {
            return;
        }
        let line_len = self.buffer.line_char_len(self.cursor.row);
        if self.cursor.col < line_len {
            let end = Position::new(self.cursor.row, self.cursor.col + 1);
            self.apply_edit_coalesced(self.cursor, end, "", Some(Coalesce::DeleteForward));
        } else if self.cursor.row + 1 < self.buffer.len_lines() {
            let end = Position::new(self.cursor.row + 1, 0);
            self.apply_edit_coalesced(self.cursor, end, "", Some(Coalesce::DeleteForward));
        }
    }

    /// Delete the active selection (every cursor's selection), returning `true`
    /// if anything was removed.
    pub fn delete_selection(&mut self) -> bool {
        if self.has_column_selection() {
            return self.delete_column_selection();
        }
        if self.cursors.is_empty() {
            return self.delete_selection_single();
        }
        let specs = self.all_cursor_specs();
        if !specs.iter().any(|(start, end, _)| start != end) {
            return false;
        }
        let mut edits = Vec::with_capacity(specs.len());
        let mut cursors = Vec::with_capacity(specs.len());
        for (start, end, _) in specs {
            edits.push(MultiEdit {
                start,
                end,
                inserted: String::new(),
            });
            cursors.push(Cursor::caret(start));
        }
        self.apply_multi_edit(edits, cursors, None);
        true
    }

    pub(super) fn delete_selection_single(&mut self) -> bool {
        if self.has_column_selection() {
            return self.delete_column_selection();
        }
        match self.selection_range() {
            Some((start, end)) => {
                self.apply_edit(start, end, "");
                true
            }
            None => false,
        }
    }

    /// Delete the rectangular selection, returning whether anything was removed.
    ///
    /// Rows shorter than the rectangle contribute nothing; a single row
    /// collapses to the ordinary selection path.
    pub(super) fn delete_column_selection(&mut self) -> bool {
        self.replace_column_selection("", None)
    }

    /// Replace the rectangular selection with `inserted` on every row.
    ///
    /// Returns `false` when the rectangle is empty (the caller keeps normal
    /// caret behaviour). Multiple rows become multiple cursors, matching how
    /// typing over a converted multi-cursor selection behaves.
    pub(super) fn replace_column_selection(
        &mut self,
        inserted: &str,
        coalesce: Option<Coalesce>,
    ) -> bool {
        let Some((top, bottom, left, right)) = self.column_bounds() else {
            return false;
        };
        if top == bottom && left == right {
            return false;
        }
        let mut edits = Vec::new();
        let mut cursors = Vec::new();
        for row in top..=bottom {
            let line_len = self.buffer.line_char_len(row);
            let start = Position::new(row, left.min(line_len));
            let end = Position::new(row, right.min(line_len));
            if start == end && inserted.is_empty() {
                continue;
            }
            edits.push(MultiEdit {
                start,
                end,
                inserted: inserted.to_string(),
            });
            cursors.push(Cursor::caret(start.advanced_by(inserted)));
        }
        if edits.is_empty() {
            self.selection = None;
            self.preferred_col = None;
            return true;
        }
        self.apply_multi_edit(edits, cursors, coalesce);
        true
    }

    // ----------------------------------------------------------------------
    // Indentation and line operations
    // ----------------------------------------------------------------------

    /// Indent the selected lines, or insert one indentation step at the cursor.
    pub fn indent(&mut self) {
        if !self.cursors.is_empty() {
            self.indent_multi(true);
            return;
        }
        match self.selected_rows() {
            Some((start, end)) => self.reindent_lines(start, end, true),
            None => self.insert_tab(),
        }
    }

    /// Outdent the selected lines, or one step on the current line.
    pub fn outdent(&mut self) {
        if !self.cursors.is_empty() {
            self.indent_multi(false);
            return;
        }
        let (start, end) = self
            .selected_rows()
            .unwrap_or((self.cursor.row, self.cursor.row));
        self.reindent_lines(start, end, false);
    }

    /// Indent or outdent every cursor's line as one undoable edit.
    ///
    /// Rows are made unique first, so overlapping cursors on the same line
    /// cannot indent it twice. Cursors shift with the whitespace they gained or
    /// lost, and selections are preserved.
    pub(super) fn indent_multi(&mut self, increase: bool) {
        let specs = self.all_cursor_specs();
        let width = self.indent_width.max(1);
        let unit = self.indent_unit();
        let mut rows: Vec<usize> = Vec::new();
        for (start, end, caret) in &specs {
            if start != end {
                let last = if end.col == 0 && end.row > start.row {
                    end.row - 1
                } else {
                    end.row
                };
                rows.extend(start.row..=last);
            } else {
                rows.push(caret.row);
            }
        }
        rows.sort_unstable();
        rows.dedup();

        let mut edits = Vec::new();
        let mut deltas: Vec<(usize, isize)> = Vec::new();
        for &row in &rows {
            let line = self.buffer.line_text(row);
            if increase {
                if line.trim().is_empty() {
                    continue;
                }
                edits.push(MultiEdit {
                    start: Position::new(row, 0),
                    end: Position::new(row, 0),
                    inserted: unit.clone(),
                });
                deltas.push((row, unit.chars().count() as isize));
            } else {
                let remove = leading_outdent(&line, width);
                if remove == 0 {
                    continue;
                }
                edits.push(MultiEdit {
                    start: Position::new(row, 0),
                    end: Position::new(row, remove),
                    inserted: String::new(),
                });
                deltas.push((row, -(remove as isize)));
            }
        }
        if edits.is_empty() {
            return;
        }

        let shift = |pos: Position| -> Position {
            match deltas.iter().find(|(row, _)| *row == pos.row) {
                Some((_, delta)) => {
                    let col = (pos.col as isize + delta).max(0) as usize;
                    Position::new(pos.row, col)
                }
                None => pos,
            }
        };
        let mut cursors = Vec::with_capacity(specs.len());
        for (start, end, caret) in &specs {
            let new_caret = shift(*caret);
            let anchor = if start == end {
                None
            } else if caret == start {
                Some(shift(*end))
            } else {
                Some(shift(*start))
            };
            cursors.push(Cursor::selecting(anchor.unwrap_or(new_caret), new_caret));
        }
        self.apply_multi_edit(edits, cursors, None);
    }

    pub(super) fn insert_tab(&mut self) {
        if !self.use_spaces {
            self.apply_edit(self.cursor, self.cursor, "\t");
            return;
        }
        let width = self.indent_width.max(1);
        let spaces = width - (self.cursor.col % width);
        self.apply_edit(self.cursor, self.cursor, &" ".repeat(spaces));
    }

    pub(super) fn reindent_lines(&mut self, from_row: usize, to_row: usize, increase: bool) {
        let last = self.buffer.len_lines().saturating_sub(1);
        let from_row = from_row.min(last);
        let to_row = to_row.min(last).max(from_row);
        let newline = self.buffer.newline();

        let mut changed = false;
        let lines: Vec<String> = (from_row..=to_row)
            .map(|row| {
                let line = self.buffer.line_text(row);
                if increase {
                    if line.trim().is_empty() {
                        line
                    } else {
                        changed = true;
                        format!("{}{line}", self.indent_unit())
                    }
                } else {
                    let remove = leading_outdent(&line, self.indent_width);
                    if remove > 0 {
                        changed = true;
                        line.chars().skip(remove).collect()
                    } else {
                        line
                    }
                }
            })
            .collect();
        if !changed {
            return;
        }

        let block = lines.join(newline);
        let start = Position::new(from_row, 0);
        let end = Position::new(to_row, self.buffer.line_char_len(to_row));
        self.apply_edit(start, end, &block);

        // Keep the affected lines selected so repeated indent/outdent works.
        let end_col = self.buffer.line_char_len(to_row);
        self.selection = Some(Selection::new(Position::new(from_row, 0)));
        self.cursor = Position::new(to_row, end_col);
        self.preferred_col = None;
        self.history.break_coalesce();
    }

    /// Move the current line (or selected lines) up one row.
    pub fn move_line_up(&mut self) {
        self.move_lines(-1);
    }

    /// Move the current line (or selected lines) down one row.
    pub fn move_line_down(&mut self) {
        self.move_lines(1);
    }

    pub(super) fn move_lines(&mut self, direction: i32) {
        let last = self.buffer.len_lines().saturating_sub(1);
        let (start_row, end_row) = match self.selected_rows() {
            Some((start, end)) => (start.min(last), end.min(last)),
            None => {
                let row = self.cursor.row.min(last);
                (row, row)
            }
        };
        let newline = self.buffer.newline();
        let col = self.cursor.col;

        if direction < 0 {
            if start_row == 0 {
                return;
            }
            let target = start_row - 1;
            let above = self.buffer.line_text(target);
            let mut lines: Vec<String> = (start_row..=end_row)
                .map(|row| self.buffer.line_text(row))
                .collect();
            lines.push(above);

            let start = Position::new(target, 0);
            let end = Position::new(end_row, self.buffer.line_char_len(end_row));
            self.apply_edit(start, end, &lines.join(newline));

            let first = start_row - 1;
            let last_row = end_row - 1;
            self.place_after_line_move(first, last_row, col);
        } else {
            let below = end_row + 1;
            if below > last {
                return;
            }
            let below_text = self.buffer.line_text(below);
            let lines: Vec<String> = (start_row..=end_row)
                .map(|row| self.buffer.line_text(row))
                .collect();
            let block = format!("{below_text}{newline}{}", lines.join(newline));

            let start = Position::new(start_row, 0);
            let end = Position::new(below, self.buffer.line_char_len(below));
            self.apply_edit(start, end, &block);

            self.place_after_line_move(start_row + 1, end_row + 1, col);
        }
    }

    pub(super) fn place_after_line_move(&mut self, first_row: usize, last_row: usize, col: usize) {
        if self.selection.is_some() {
            self.selection = Some(Selection::new(Position::new(first_row, 0)));
            self.cursor = Position::new(last_row, col);
        } else {
            self.cursor = Position::new(first_row, col);
            self.selection = None;
        }
        self.preferred_col = None;
        self.history.break_coalesce();
    }

    /// Duplicate the current line below itself.
    pub fn duplicate_line(&mut self) {
        let last = self.buffer.len_lines().saturating_sub(1);
        let row = self.cursor.row.min(last);
        let text = self.buffer.line_text(row);
        let end = Position::new(row, self.buffer.line_char_len(row));
        let col = self.cursor.col;
        self.apply_edit(end, end, &format!("\n{text}"));
        let new_row = (row + 1).min(self.buffer.len_lines().saturating_sub(1));
        self.cursor = Position::new(new_row, col.min(self.buffer.line_char_len(new_row)));
        self.selection = None;
        self.preferred_col = None;
        self.history.break_coalesce();
    }

    /// Delete the current line, or every line the selection touches.
    pub fn delete_line(&mut self) {
        let (first, last) = self
            .selected_rows()
            .unwrap_or((self.cursor.row, self.cursor.row));
        let start = Position::new(first, 0);
        let end = if last + 1 < self.buffer.len_lines() {
            Position::new(last + 1, 0)
        } else {
            let final_row = self.buffer.len_lines().saturating_sub(1);
            Position::new(final_row, self.buffer.line_char_len(final_row))
        };
        self.replace_range(start, end, "");
        self.cursor = self.buffer.clamp_position(start);
        self.selection = None;
        self.preferred_col = None;
        self.history.break_coalesce();
    }
}
