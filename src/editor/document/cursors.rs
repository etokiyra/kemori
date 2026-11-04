//! Extracted from `document/mod.rs`; see `docs/DECISIONS.md`.

use super::*;

impl Document {
    /// Whether there is more than one cursor.
    pub fn has_multiple_cursors(&self) -> bool {
        !self.cursors.is_empty()
    }

    /// Drop every secondary cursor, keeping the primary in place.
    pub fn clear_extra_cursors(&mut self) {
        self.cursors.clear();
    }

    /// The primary cursor's range (a zero-width caret when nothing is selected).
    pub fn primary_range(&self) -> (Position, Position) {
        self.selection_range().unwrap_or((self.cursor, self.cursor))
    }

    /// Every cursor as `(start, end, caret)`, primary first, ranges normalized.
    pub(super) fn all_cursor_specs(&self) -> Vec<(Position, Position, Position)> {
        let mut specs = Vec::with_capacity(self.cursors.len() + 1);
        let (start, end) = self.primary_range();
        specs.push((start, end, self.cursor));
        for cursor in &self.cursors {
            let (start, end) = cursor.range();
            specs.push((start, end, cursor.cursor));
        }
        specs
    }

    /// The text inside a normalized range.
    pub(super) fn range_text(&self, start: Position, end: Position) -> String {
        let s = self.buffer.position_to_char(start);
        let e = self.buffer.position_to_char(end);
        if e <= s {
            String::new()
        } else {
            self.buffer.as_rope().slice(s..e).to_string()
        }
    }

    /// Keep secondary cursors sorted, unique and distinct from the primary.
    pub(super) fn normalize_extra_cursors(&mut self) {
        let primary = self.primary_range();
        let mut seen = std::collections::HashSet::new();
        seen.insert(primary);
        let mut kept: Vec<Cursor> = Vec::new();
        for cursor in self.cursors.drain(..) {
            let range = cursor.range();
            if seen.insert(range) {
                kept.push(Cursor::from(range));
            }
        }
        kept.sort_by_key(|cursor| cursor.range());
        self.cursors = kept;
    }

    /// Apply replacements at every cursor as a single undoable edit.
    ///
    /// `edits` are normalized and applied from the bottom of the buffer upward
    /// so earlier edits stay valid; `cursors` holds the resulting cursors with
    /// the primary first. A zero-op call still moves the cursors, which is what
    /// makes bracket skip-over work at every cursor.
    pub(super) fn apply_multi_edit(
        &mut self,
        mut edits: Vec<MultiEdit>,
        cursors: Vec<Cursor>,
        coalesce: Option<Coalesce>,
    ) {
        if self.buffer.is_user_read_only() {
            return;
        }
        if edits.is_empty() || cursors.is_empty() {
            return;
        }
        // A genuine single cursor keeps the exact existing single-cursor path
        // (and its coalescing).
        if edits.len() == 1 && self.cursors.is_empty() {
            let edit = edits.remove(0);
            self.apply_edit_coalesced(edit.start, edit.end, &edit.inserted, coalesce);
            return;
        }

        let primary = cursors[0];
        let cursor_before = self.cursor;

        for edit in &mut edits {
            if edit.start > edit.end {
                std::mem::swap(&mut edit.start, &mut edit.end);
            }
        }
        let mut application = edits.clone();
        application.sort_by(|a, b| b.start.cmp(&a.start).then(b.end.cmp(&a.end)));

        let mut ops = Vec::new();
        let mut min_row = usize::MAX;
        self.clear_server_folds();
        for edit in &application {
            let start = self.buffer.clamp_position(edit.start);
            let end = self.buffer.clamp_position(edit.end);
            let start_char = self.buffer.position_to_char(start);
            let end_char = self.buffer.position_to_char(end);
            let removed = if end_char > start_char {
                self.buffer.remove(start_char..end_char)
            } else {
                String::new()
            };
            let inserted = self.normalize_newlines(&edit.inserted);
            if removed.is_empty() && inserted.is_empty() {
                continue;
            }
            if !inserted.is_empty() {
                self.buffer.insert(start_char, &inserted);
            }
            self.remap_folds_after_edit(
                start.row,
                start.col,
                removed.matches('\n').count(),
                inserted.matches('\n').count(),
            );
            min_row = min_row.min(start.row);
            ops.push(EditOp {
                start: start_char,
                removed,
                inserted,
            });
        }

        // Move every cursor even when the keystroke produced no text (a
        // skip-over), so all cursors stay coherent.
        self.cursor = self.buffer.clamp_position(primary.cursor);
        self.selection = if primary.anchor == primary.cursor {
            None
        } else {
            Some(Selection::new(primary.anchor))
        };
        self.cursors = cursors.into_iter().skip(1).collect();
        self.preferred_col = None;
        self.normalize_extra_cursors();

        if ops.is_empty() {
            return;
        }
        self.history.push(Edit {
            id: 0,
            ops,
            cursor_before,
            cursor_after: self.cursor,
            coalesce,
        });
        self.buffer.mark_dirty();
        if min_row != usize::MAX {
            self.invalidate_highlight(min_row);
        }
        self.diagnostics.clear();
        self.diagnostics_dirty = true;
    }

    /// Plan a newline at one cursor (`start`/`end` normalized, `caret` actual).
    pub(super) fn plan_newline(
        &self,
        start: Position,
        end: Position,
        caret: Position,
    ) -> (MultiEdit, Cursor) {
        let line = self.buffer.line_text(caret.row);
        let leading: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        let before: String = line.chars().take(caret.col).collect();
        let after = self.char_at(caret);

        // Expanding an empty pair: `{|}` becomes a three-line indented block.
        if let Some(open) = before.chars().last().filter(|c| is_open_bracket(*c))
            && after == matching_close(open)
        {
            let outer = leading;
            let inner = format!("{outer}{}", self.indent_unit());
            let text = format!("\n{inner}\n{outer}");
            let cursor = start.advanced_by(&text);
            let cursor = Position::new(cursor.row.saturating_sub(1), inner.chars().count());
            return (
                MultiEdit {
                    start,
                    end,
                    inserted: text,
                },
                Cursor::caret(cursor),
            );
        }

        let mut indent = if line.trim().is_empty() && caret.row > 0 {
            self.buffer
                .line_text(caret.row - 1)
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
        let cursor = start.advanced_by(&text);
        (
            MultiEdit {
                start,
                end,
                inserted: text,
            },
            Cursor::caret(cursor),
        )
    }

    /// Plan a backspace at one cursor.
    pub(super) fn plan_backspace(
        &self,
        start: Position,
        end: Position,
        caret: Position,
    ) -> (MultiEdit, Cursor) {
        if start != end {
            return (
                MultiEdit {
                    start,
                    end,
                    inserted: String::new(),
                },
                Cursor::caret(start),
            );
        }
        if caret.col > 0 {
            let before = self.char_at(Position::new(caret.row, caret.col - 1));
            let at = self.char_at(caret);
            // Deleting into an empty pair removes both halves.
            if let (Some(open), Some(close)) = (before, at)
                && matching_close(open) == Some(close)
            {
                let s = Position::new(caret.row, caret.col - 1);
                let e = Position::new(caret.row, caret.col + 1);
                return (
                    MultiEdit {
                        start: s,
                        end: e,
                        inserted: String::new(),
                    },
                    Cursor::caret(s),
                );
            }
            let s = Position::new(caret.row, caret.col - 1);
            return (
                MultiEdit {
                    start: s,
                    end: caret,
                    inserted: String::new(),
                },
                Cursor::caret(s),
            );
        }
        if caret.row > 0 {
            let prev = caret.row - 1;
            let s = Position::new(prev, self.buffer.line_char_len(prev));
            return (
                MultiEdit {
                    start: s,
                    end: caret,
                    inserted: String::new(),
                },
                Cursor::caret(s),
            );
        }
        (
            MultiEdit {
                start,
                end,
                inserted: String::new(),
            },
            Cursor::caret(caret),
        )
    }

    /// Plan a forward delete at one cursor.
    pub(super) fn plan_delete_forward(
        &self,
        start: Position,
        end: Position,
        caret: Position,
    ) -> (MultiEdit, Cursor) {
        if start != end {
            return (
                MultiEdit {
                    start,
                    end,
                    inserted: String::new(),
                },
                Cursor::caret(start),
            );
        }
        let line_len = self.buffer.line_char_len(caret.row);
        if caret.col < line_len {
            let e = Position::new(caret.row, caret.col + 1);
            return (
                MultiEdit {
                    start: caret,
                    end: e,
                    inserted: String::new(),
                },
                Cursor::caret(caret),
            );
        }
        if caret.row + 1 < self.buffer.len_lines() {
            let e = Position::new(caret.row + 1, 0);
            return (
                MultiEdit {
                    start: caret,
                    end: e,
                    inserted: String::new(),
                },
                Cursor::caret(caret),
            );
        }
        (
            MultiEdit {
                start,
                end,
                inserted: String::new(),
            },
            Cursor::caret(caret),
        )
    }

    /// Add a caret one row below every current cursor.
    pub fn add_cursor_below(&mut self) {
        self.add_cursor_vertical(1);
    }

    /// Add a caret one row above every current cursor.
    pub fn add_cursor_above(&mut self) {
        self.add_cursor_vertical(-1);
    }

    pub(super) fn add_cursor_vertical(&mut self, direction: isize) {
        let specs = self.all_cursor_specs();
        let mut added: Vec<Cursor> = Vec::new();
        for (_, _, caret) in specs {
            let row = if direction < 0 {
                self.visible_row_above(caret.row)
            } else {
                self.visible_row_below(caret.row)
            };
            let Some(row) = row else { continue };
            let col = caret.col.min(self.buffer.line_char_len(row));
            added.push(Cursor::caret(Position::new(row, col)));
        }
        if added.is_empty() {
            return;
        }
        // Fold the primary's caret into place and append the new ones.
        self.selection = None;
        let mut cursors: Vec<Cursor> = self.cursors.clone();
        cursors.push(Cursor::caret(self.cursor));
        cursors.extend(added);
        // The primary stays where it is; everything else is normalized below.
        self.cursors = cursors;
        self.normalize_extra_cursors();
        self.preferred_col = None;
        self.history.break_coalesce();
    }

    /// Select every occurrence of the current word/selection and add a cursor
    /// at each, as Visual Studio Code's "Select All Occurrences".
    pub fn select_all_occurrences(&mut self) {
        let query = match self.selection_range() {
            Some((start, end)) => {
                let text = self.range_text(start, end);
                if text.is_empty() || text.contains('\n') {
                    return;
                }
                text
            }
            None => {
                let cursor = self.clamped_cursor();
                match self.word_bounds(cursor.row, cursor.col) {
                    Some((start, end)) => self.range_text(start, end),
                    None => return,
                }
            }
        };
        if query.is_empty() {
            return;
        }
        let matches = self.find_all_with(&query, true, true);
        if matches.len() < 2 {
            return;
        }
        // Primary takes the first match; the rest become secondary cursors.
        self.selection = Some(Selection::new(matches[0].0));
        self.cursor = matches[0].1;
        self.cursors = matches.into_iter().skip(1).map(Cursor::from).collect();
        self.preferred_col = None;
        self.normalize_extra_cursors();
        self.history.break_coalesce();
    }

    /// Replace the text between `start` and `end` with `text`, as a single edit.
    pub fn replace_range(&mut self, start: Position, end: Position, text: &str) {
        self.apply_edit(start, end, text);
    }

    pub fn insert_char(&mut self, c: char) {
        let (start, end) = self.selection_range().unwrap_or((self.cursor, self.cursor));
        self.apply_edit_coalesced(start, end, &c.to_string(), Some(Coalesce::Insert));
    }

    /// Type a character, applying automatic pairing where it helps.
    ///
    /// With multiple cursors, every cursor types independently and the whole
    /// keystroke is one undo step.
    pub fn type_char(&mut self, c: char) {
        if self.has_column_selection() {
            if !self.replace_column_selection(&c.to_string(), Some(Coalesce::Insert)) {
                self.selection = None;
                self.type_char_single(c);
            }
            return;
        }
        if self.cursors.is_empty() {
            self.type_char_single(c);
            return;
        }
        let specs = self.all_cursor_specs();
        let mut edits = Vec::with_capacity(specs.len());
        let mut cursors = Vec::with_capacity(specs.len());
        for (start, end, caret) in specs {
            let (edit, cursor) = self.plan_type_char(c, start, end, caret);
            edits.push(edit);
            cursors.push(cursor);
        }
        self.apply_multi_edit(edits, cursors, Some(Coalesce::Insert));
    }

    /// Type a character on a single cursor, applying automatic pairing.
    pub(super) fn type_char_single(&mut self, c: char) {
        if (is_close_bracket(c) || c == '"') && self.char_at(self.cursor) == Some(c) {
            if let Some(next) = self.next_pos(self.cursor) {
                self.set_cursor(next, false);
            }
            return;
        }

        if let Some(close) = matching_close(c) {
            // Wrap a selection: `(selected)`.
            if let Some((start, end)) = self.selection_range() {
                let selected = self
                    .buffer
                    .as_rope()
                    .slice(self.buffer.position_to_char(start)..self.buffer.position_to_char(end))
                    .to_string();
                let text = format!("{c}{selected}{close}");
                self.apply_edit(start, end, &text);
                let after = start.advanced_by(&text);
                self.cursor = Position::new(after.row, after.col.saturating_sub(1));
                return;
            }

            // Insert a pair when the cursor sits at a natural boundary.
            let boundary = self.char_at(self.cursor).is_none_or(|next| {
                next.is_whitespace() || is_close_bracket(next) || next == ';' || next == ','
            });
            if boundary {
                let text = format!("{c}{close}");
                self.apply_edit(self.cursor, self.cursor, &text);
                self.cursor = Position::new(self.cursor.row, self.cursor.col.saturating_sub(1));
                return;
            }
        }

        self.insert_char(c);
    }

    /// Plan typing `c` at one cursor range `(start, end)` whose caret is `caret`.
    pub(super) fn plan_type_char(
        &self,
        c: char,
        start: Position,
        end: Position,
        caret: Position,
    ) -> (MultiEdit, Cursor) {
        // Skip over an already-inserted partner (no edit, just move).
        if (is_close_bracket(c) || c == '"') && start == end && self.char_at(caret) == Some(c) {
            let next = self.next_pos(caret).unwrap_or(caret);
            return (
                MultiEdit {
                    start,
                    end,
                    inserted: String::new(),
                },
                Cursor::caret(next),
            );
        }
        if let Some(close) = matching_close(c) {
            if start != end {
                let selected = self.range_text(start, end);
                let text = format!("{c}{selected}{close}");
                let inside = Position::new(start.row, start.col + 1);
                return (
                    MultiEdit {
                        start,
                        end,
                        inserted: text,
                    },
                    Cursor::caret(inside),
                );
            }
            let boundary = self.char_at(caret).is_none_or(|next| {
                next.is_whitespace() || is_close_bracket(next) || next == ';' || next == ','
            });
            if boundary {
                let text = format!("{c}{close}");
                let inside = Position::new(start.row, start.col + 1);
                return (
                    MultiEdit {
                        start,
                        end,
                        inserted: text,
                    },
                    Cursor::caret(inside),
                );
            }
        }
        let text = c.to_string();
        let after = start.advanced_by(&text);
        (
            MultiEdit {
                start,
                end,
                inserted: text,
            },
            Cursor::caret(after),
        )
    }
}
