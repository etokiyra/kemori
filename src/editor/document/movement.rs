//! Extracted from `document/mod.rs`; see `docs/DECISIONS.md`.

use super::*;

impl Document {
    pub fn undo(&mut self) {
        if self.buffer.is_user_read_only() {
            return;
        }
        if let Some(edit) = self.history.undo() {
            // Undo the primitive ops in reverse application order so earlier
            // (lower) offsets are restored first.
            for op in edit.ops.iter().rev() {
                let inserted_len = op.inserted.chars().count();
                if inserted_len > 0 {
                    self.buffer.remove(op.start..op.start + inserted_len);
                }
                if !op.removed.is_empty() {
                    self.buffer.insert(op.start, &op.removed);
                }
            }
            self.cursor = self.buffer.clamp_position(edit.cursor_before);
            self.selection = None;
            self.cursors.clear();
            self.preferred_col = None;
            // Undo restores positions from arbitrary history entries; keeping
            // folds could leave a header over unrelated text.
            self.folded.clear();
            self.server_folds.clear();
            self.invalidate_highlight(self.cursor.row);
            self.refresh_dirty();
            self.diagnostics.clear();
            self.diagnostics_dirty = true;
        }
    }

    pub fn redo(&mut self) {
        if self.buffer.is_user_read_only() {
            return;
        }
        if let Some(edit) = self.history.redo() {
            for op in &edit.ops {
                let removed_len = op.removed.chars().count();
                if removed_len > 0 {
                    self.buffer.remove(op.start..op.start + removed_len);
                }
                if !op.inserted.is_empty() {
                    self.buffer.insert(op.start, &op.inserted);
                }
            }
            self.cursor = self.buffer.clamp_position(edit.cursor_after);
            self.selection = None;
            self.cursors.clear();
            self.preferred_col = None;
            self.folded.clear();
            self.server_folds.clear();
            self.invalidate_highlight(self.cursor.row);
            self.refresh_dirty();
            self.diagnostics.clear();
            self.diagnostics_dirty = true;
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub(super) fn normalize_newlines(&self, text: &str) -> String {
        let lf = text.replace("\r\n", "\n");
        match self.buffer.line_ending {
            LineEnding::Lf => lf,
            LineEnding::Crlf => lf.replace('\n', "\r\n"),
        }
    }

    // ----------------------------------------------------------------------
    // Selection
    // ----------------------------------------------------------------------

    pub fn selection_range(&self) -> Option<(Position, Position)> {
        let sel = self.selection?;
        if sel.column
            && let Some((top, bottom, left, right)) = self.column_bounds()
        {
            return (top != bottom || left != right)
                .then_some((Position::new(top, left), Position::new(bottom, right)));
        }
        let (start, end) = sel.range(self.cursor);
        if start == end {
            None
        } else {
            Some((start, end))
        }
    }

    /// The rectangular bounds of a column selection as `(top, bottom, left, right)`.
    ///
    /// `left` is inclusive and `right` is exclusive, matching the caret: an
    /// anchor at column 2 with the cursor at column 5 covers columns 2, 3 and 4.
    pub fn column_bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let selection = self.selection.filter(|selection| selection.column)?;
        let top = selection.anchor.row.min(self.cursor.row);
        let bottom = selection.anchor.row.max(self.cursor.row);
        let left = selection.anchor.col.min(self.cursor.col);
        let right = selection.anchor.col.max(self.cursor.col);
        Some((top, bottom, left, right))
    }

    /// Whether the primary selection is rectangular.
    pub fn has_column_selection(&self) -> bool {
        self.selection.is_some_and(Selection::is_column)
    }

    /// Start a column selection at the cursor when one is not already active.
    fn begin_column_selection(&mut self) {
        if !self.has_column_selection() {
            self.selection = Some(Selection::rect(self.cursor));
            self.cursors.clear();
            self.preferred_col = None;
        }
        self.history.break_coalesce();
    }

    /// Extend a rectangular selection one row up.
    pub fn extend_column_up(&mut self) {
        self.begin_column_selection();
        let Some(row) = self.visible_row_above(self.cursor.row) else {
            return;
        };
        let target_col = *self.preferred_col.get_or_insert(self.cursor.col);
        let col = target_col.min(self.buffer.line_char_len(row));
        self.cursor = Position::new(row, col);
    }

    /// Extend a rectangular selection one row down.
    pub fn extend_column_down(&mut self) {
        self.begin_column_selection();
        let Some(row) = self.visible_row_below(self.cursor.row) else {
            return;
        };
        let target_col = *self.preferred_col.get_or_insert(self.cursor.col);
        let col = target_col.min(self.buffer.line_char_len(row));
        self.cursor = Position::new(row, col);
    }

    /// Extend a rectangular selection one character left.
    pub fn extend_column_left(&mut self) {
        self.begin_column_selection();
        let col = self.cursor.col.saturating_sub(1);
        self.cursor = Position::new(self.cursor.row, col);
    }

    /// Extend a rectangular selection one character right.
    pub fn extend_column_right(&mut self) {
        self.begin_column_selection();
        let col = (self.cursor.col + 1).min(self.buffer.line_char_len(self.cursor.row));
        self.cursor = Position::new(self.cursor.row, col);
    }

    /// The inclusive row range a selection actually touches.
    ///
    /// A selection that ends at column 0 of a later row has zero selected
    /// columns on that row (the UI draws no highlight there), so the row is
    /// excluded. This keeps indent, outdent, delete-line, move-line and
    /// toggle-comment from acting on a line the user did not select.
    pub fn selected_rows(&self) -> Option<(usize, usize)> {
        if let Some((top, bottom, _, _)) = self.column_bounds() {
            return (top != bottom || self.selection_range().is_some()).then_some((top, bottom));
        }
        let (start, end) = self.selection_range()?;
        let last = if end.col == 0 && end.row > start.row {
            end.row - 1
        } else {
            end.row
        };
        Some((start.row, last))
    }

    pub fn has_selection(&self) -> bool {
        self.selection_range().is_some()
    }

    pub fn selected_text(&self) -> Option<String> {
        if let Some((top, bottom, left, right)) = self.column_bounds() {
            if left == right {
                return None;
            }
            let mut lines = Vec::with_capacity(bottom - top + 1);
            for row in top..=bottom {
                let text = self.buffer.line_text(row);
                lines.push(
                    text.chars()
                        .skip(left)
                        .take(right - left)
                        .collect::<String>(),
                );
            }
            return Some(lines.join("\n"));
        }
        let (start, end) = self.selection_range()?;
        let s = self.buffer.position_to_char(start);
        let e = self.buffer.position_to_char(end);
        Some(self.buffer.as_rope().slice(s..e).to_string())
    }

    pub fn select_all(&mut self) {
        let last_row = self.buffer.len_lines().saturating_sub(1);
        let last_col = self.buffer.line_char_len(last_row);
        self.selection = Some(Selection::new(Position::zero()));
        self.cursor = Position::new(last_row, last_col);
        self.preferred_col = None;
        self.history.break_coalesce();
    }

    /// Select the word under the cursor, then add a cursor at the next
    /// occurrence on each further call. This is Visual Studio Code's `Ctrl+D`.
    ///
    /// The primary selection moves to the newest occurrence and each earlier
    /// one becomes a secondary cursor, so the whole set can be edited at once.
    /// Once every occurrence is selected, further calls wrap to the first
    /// occurrence that is not already a cursor.
    pub fn select_next_occurrence(&mut self) {
        let (query, from) = match self.selection_range() {
            Some((_, end)) => {
                let text = self.selected_text().unwrap_or_default();
                if text.is_empty() || text.contains('\n') {
                    return;
                }
                (text, end)
            }
            None => {
                let cursor = self.clamped_cursor();
                let Some((start, end)) = self.word_bounds(cursor.row, cursor.col) else {
                    return;
                };
                self.selection = Some(Selection::new(start));
                self.cursor = end;
                self.preferred_col = None;
                self.history.break_coalesce();
                return;
            }
        };

        let matches = self.find_all_with(&query, true, true);
        if matches.is_empty() {
            return;
        }
        // Everything already selected: the primary plus the secondaries.
        let mut taken: Vec<(Position, Position)> =
            self.cursors.iter().map(|cursor| cursor.range()).collect();
        let Some((primary_start, primary_end)) = self.selection_range() else {
            return;
        };
        taken.push((primary_start, primary_end));

        // The next occurrence strictly after the primary, wrapping at the end,
        // skipping anything already selected.
        let next = matches
            .iter()
            .find(|m| **m >= (from, from) && !taken.contains(m))
            .copied()
            .or_else(|| matches.iter().copied().find(|m| !taken.contains(m)))
            .unwrap_or(matches[0]);

        // If nothing new can be added, stop growing (as VS Code does).
        if taken.contains(&next) {
            return;
        }
        // The previous primary becomes a secondary; the new match becomes primary.
        self.cursors
            .push(Cursor::from((primary_start, primary_end)));
        self.selection = Some(Selection::new(next.0));
        self.cursor = next.1;
        self.preferred_col = None;
        self.normalize_extra_cursors();
        self.history.break_coalesce();
    }

    /// The whole-word bounds around `(row, col)`, if the cursor is on a word.
    pub(super) fn word_bounds(&self, row: usize, col: usize) -> Option<(Position, Position)> {
        let chars: Vec<char> = self.buffer.line_text(row).chars().collect();
        if chars.is_empty() {
            return None;
        }
        let mut start = col.min(chars.len());
        if !(start < chars.len() && is_word_char(chars[start])) {
            // Not on a word character; fall back to the one just before it.
            if start > 0 && is_word_char(chars[start - 1]) {
                start -= 1;
            } else {
                return None;
            }
        }
        let mut end = start;
        while start > 0 && is_word_char(chars[start - 1]) {
            start -= 1;
        }
        while end < chars.len() && is_word_char(chars[end]) {
            end += 1;
        }
        (start < end).then(|| (Position::new(row, start), Position::new(row, end)))
    }

    pub fn clear_selection(&mut self) {
        // `Esc` first ends a multi-cursor session, then clears a selection.
        if !self.cursors.is_empty() {
            self.cursors.clear();
            return;
        }
        self.selection = None;
    }

    // ----------------------------------------------------------------------
    // Movement
    // ----------------------------------------------------------------------

    pub(super) fn set_cursor(&mut self, pos: Position, shift: bool) {
        self.set_cursor_inner(pos, shift, true);
    }

    pub(super) fn set_cursor_inner(&mut self, pos: Position, shift: bool, reset_preferred: bool) {
        let pos = self.snap_out_of_fold(self.buffer.clamp_position(pos));
        self.history.break_coalesce();
        // Navigating collapses a multi-cursor session onto the primary; that is
        // predictable, and a new session starts with `Ctrl+D`/`Ctrl+Alt+Arrow`.
        self.cursors.clear();
        if shift {
            if self.selection.is_none() {
                self.selection = Some(Selection::new(self.cursor));
            }
        } else {
            self.selection = None;
        }
        self.cursor = pos;
        if reset_preferred {
            self.preferred_col = None;
        }
    }

    pub fn move_left(&mut self, shift: bool) {
        if !shift && let Some(sel) = self.selection.take() {
            let (start, _) = sel.range(self.cursor);
            self.cursor = self.buffer.clamp_position(start);
            self.cursors.clear();
            self.preferred_col = None;
            return;
        }
        let pos = self.prev_visible_pos(self.cursor);
        if let Some(pos) = pos {
            self.set_cursor(pos, shift);
        }
    }

    pub fn move_right(&mut self, shift: bool) {
        if !shift && let Some(sel) = self.selection.take() {
            let (_, end) = sel.range(self.cursor);
            self.cursor = self.buffer.clamp_position(end);
            self.cursors.clear();
            self.preferred_col = None;
            return;
        }
        let pos = self.next_visible_pos(self.cursor);
        if let Some(pos) = pos {
            self.set_cursor(pos, shift);
        }
    }

    pub fn move_up(&mut self, shift: bool) {
        if self.wrap_width > 0 {
            self.move_visual(-1, shift);
            return;
        }
        let Some(row) = self.visible_row_above(self.cursor.row) else {
            return;
        };
        let target_col = *self.preferred_col.get_or_insert(self.cursor.col);
        let col = target_col.min(self.buffer.line_char_len(row));
        self.set_cursor_inner(Position::new(row, col), shift, false);
    }

    pub fn move_down(&mut self, shift: bool) {
        if self.wrap_width > 0 {
            self.move_visual(1, shift);
            return;
        }
        let Some(row) = self.visible_row_below(self.cursor.row) else {
            return;
        };
        let target_col = *self.preferred_col.get_or_insert(self.cursor.col);
        let col = target_col.min(self.buffer.line_char_len(row));
        self.set_cursor_inner(Position::new(row, col), shift, false);
    }

    /// The display column of `(row, col)`.
    pub fn display_col_of(&self, row: usize, col: usize) -> usize {
        self.line_layout(row).display_col(col)
    }

    /// The visual rows of a logical line at the current wrap width.
    pub fn row_segments(&self, row: usize, width: usize) -> Vec<(usize, usize)> {
        crate::editor::layout::wrap_segments(&self.line_layout(row), width)
    }

    /// A display layout for one logical line.
    pub fn line_layout(&self, row: usize) -> LineLayout {
        LineLayout::new(&self.buffer.line_text(row), self.indent_width.max(1))
    }

    /// The number of visual rows a logical line occupies at `width`.
    pub fn row_visual_count(&self, row: usize, width: usize) -> usize {
        self.row_segments(row, width).len()
    }

    /// Move the cursor up or down by one **visual** row when soft wrap is on.
    ///
    /// The display column is remembered across movements (like `preferred_col`
    /// for unwrapped lines), so moving through short and wrapped rows keeps the
    /// cursor in a stable column.
    pub(super) fn move_visual(&mut self, direction: isize, shift: bool) {
        let width = self.wrap_width;
        let row = self.cursor.row;
        let layout = self.line_layout(row);
        let display = layout.display_col(self.cursor.col);
        let segments = crate::editor::layout::wrap_segments(&layout, width);
        let seg = crate::editor::layout::segment_index(&segments, display);
        let preferred = *self.preferred_col.get_or_insert(display);

        let (target_row, target_seg) = if direction < 0 {
            if seg > 0 {
                (row, Some(seg - 1))
            } else {
                match self.visible_row_above(row) {
                    Some(previous) => (previous, None),
                    None => return,
                }
            }
        } else if seg + 1 < segments.len() {
            (row, Some(seg + 1))
        } else {
            match self.visible_row_below(row) {
                Some(next) => (next, Some(0)),
                None => return,
            }
        };

        let target_layout = self.line_layout(target_row);
        let target_segments = crate::editor::layout::wrap_segments(&target_layout, width);
        let seg_index = match target_seg {
            Some(index) => index.min(target_segments.len().saturating_sub(1)),
            // `None` means the last visual row of the previous logical line.
            None => target_segments.len().saturating_sub(1),
        };
        let (segment_start, segment_end) = target_segments[seg_index];
        // Keep the caret inside this visual row: for every row but the last, the
        // end column is the start of the next row.
        let upper = if seg_index + 1 == target_segments.len() {
            segment_end
        } else {
            segment_end.saturating_sub(1).max(segment_start)
        };
        let target_display = preferred.clamp(segment_start, upper);
        let col = target_layout.char_col(target_display);
        self.set_cursor_inner(Position::new(target_row, col), shift, false);
    }

    pub fn move_home(&mut self, shift: bool) {
        let line = self.buffer.line_text(self.cursor.row);
        let indent = line.chars().take_while(|c| c.is_whitespace()).count();
        let col = if self.cursor.col == indent { 0 } else { indent };
        self.set_cursor(Position::new(self.cursor.row, col), shift);
    }

    pub fn move_end(&mut self, shift: bool) {
        let col = self.buffer.line_char_len(self.cursor.row);
        self.set_cursor(Position::new(self.cursor.row, col), shift);
    }

    pub fn move_document_start(&mut self, shift: bool) {
        self.set_cursor(Position::zero(), shift);
    }

    pub fn move_document_end(&mut self, shift: bool) {
        let row = self.visible_row_at_or_below(self.buffer.len_lines().saturating_sub(1));
        let col = self.buffer.line_char_len(row);
        self.set_cursor(Position::new(row, col), shift);
    }

    pub fn move_word_left(&mut self, shift: bool) {
        let mut pos = self.cursor;
        // Skip whitespace before the cursor.
        while let Some(prev) = self.prev_pos(pos) {
            if self.char_at(prev).is_some_and(|c| c.is_whitespace()) {
                pos = prev;
            } else {
                break;
            }
        }
        // Skip the word.
        while let Some(prev) = self.prev_pos(pos) {
            if self.char_at(prev).is_some_and(is_word_char) {
                pos = prev;
            } else {
                break;
            }
        }
        self.set_cursor(pos, shift);
    }

    pub fn move_word_right(&mut self, shift: bool) {
        let mut pos = self.cursor;
        while self.char_at(pos).is_some_and(|c| c.is_whitespace()) {
            match self.next_pos(pos) {
                Some(next) => pos = next,
                None => break,
            }
        }
        while self.char_at(pos).is_some_and(is_word_char) {
            match self.next_pos(pos) {
                Some(next) => pos = next,
                None => break,
            }
        }
        self.set_cursor(pos, shift);
    }

    pub fn move_to(&mut self, pos: Position) {
        self.set_cursor(pos, false);
    }

    /// Jump to a 1-based line number, as used by "go to line".
    pub fn go_to_line(&mut self, line: usize) {
        let row = line
            .saturating_sub(1)
            .min(self.buffer.len_lines().saturating_sub(1));
        self.set_cursor(Position::new(row, 0), false);
    }

    /// The character at `pos`, if it exists.
    pub fn char_at(&self, pos: Position) -> Option<char> {
        if pos.row >= self.buffer.len_lines() {
            return None;
        }
        self.buffer.line_text(pos.row).chars().nth(pos.col)
    }

    pub(super) fn next_pos(&self, pos: Position) -> Option<Position> {
        if pos.col < self.buffer.line_char_len(pos.row) {
            Some(Position::new(pos.row, pos.col + 1))
        } else if pos.row + 1 < self.buffer.len_lines() {
            Some(Position::new(pos.row + 1, 0))
        } else {
            None
        }
    }

    pub(super) fn prev_pos(&self, pos: Position) -> Option<Position> {
        if pos.col > 0 {
            Some(Position::new(pos.row, pos.col - 1))
        } else if pos.row > 0 {
            Some(Position::new(
                pos.row - 1,
                self.buffer.line_char_len(pos.row - 1),
            ))
        } else {
            None
        }
    }

    /// The previous position, skipping rows hidden by a collapsed fold.
    pub(super) fn prev_visible_pos(&self, pos: Position) -> Option<Position> {
        let mut candidate = self.prev_pos(pos)?;
        while self.is_hidden(candidate.row) {
            if candidate.row == 0 {
                return None;
            }
            let row = candidate.row - 1;
            candidate = Position::new(row, self.buffer.line_char_len(row));
        }
        Some(candidate)
    }

    /// The next position, skipping rows hidden by a collapsed fold.
    pub(super) fn next_visible_pos(&self, pos: Position) -> Option<Position> {
        let mut candidate = self.next_pos(pos)?;
        let last = self.buffer.len_lines().saturating_sub(1);
        while self.is_hidden(candidate.row) {
            if candidate.row >= last {
                return None;
            }
            candidate = Position::new(candidate.row + 1, 0);
        }
        Some(candidate)
    }

    /// A safe copy of the current position, clamped to the buffer.
    pub fn clamped_cursor(&self) -> Position {
        self.buffer.clamp_position(self.cursor)
    }

    // ----------------------------------------------------------------------
    // Highlighting
    // ----------------------------------------------------------------------
}
