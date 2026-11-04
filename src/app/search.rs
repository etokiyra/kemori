//! Extracted from `app/mod.rs`: see `docs/DECISIONS.md`.

use super::*;
use std::collections::BTreeMap;

impl App {
    pub(super) fn replace_current(&mut self) {
        if self.read_only_block() {
            return;
        }
        let Some(current) = self.search.current else {
            self.find_next();
            return;
        };
        let Some(&(start, end)) = self.search.matches.get(current) else {
            return;
        };
        let replacement = self.search.replacement.clone();
        // Keep a scoped search anchored after a replacement of a different length.
        let scope_end_offset = match (self.search.scope, self.editor.active_document()) {
            (Some((_, scope_end)), Some(doc)) => Some(doc.buffer.position_to_char(scope_end)),
            _ => None,
        };
        let shift = match self.editor.active_document() {
            Some(doc) => {
                replacement.chars().count() as isize
                    - (doc.buffer.position_to_char(end) - doc.buffer.position_to_char(start))
                        as isize
            }
            None => 0,
        };
        if let Some(doc) = self.editor.active_document_mut() {
            doc.replace_range(start, end, &replacement);
        }
        if let (Some(end), Some(doc)) = (scope_end_offset, self.editor.active_document()) {
            let end = (end as isize + shift).max(0) as usize;
            let end = doc.buffer.char_to_position(end);
            if let Some((start, _)) = self.search.scope {
                self.search.scope = Some((start, end));
            }
        }
        self.refresh_search_matches();
        self.schedule_diagnostics();
        let next = current.min(self.search.matches.len().saturating_sub(1));
        self.jump_to_match(next);
    }

    pub(super) fn jump_to_match(&mut self, index: usize) {
        if self.search.matches.is_empty() {
            return;
        }
        let index = index.min(self.search.matches.len() - 1);
        self.search.current = Some(index);
        let (start, end) = self.search.matches[index];
        if let Some(doc) = self.editor.active_document_mut() {
            doc.selection = Some(Selection::new(start));
            doc.cursor = end;
            doc.preferred_col = None;
        }
    }

    pub(super) fn find_next(&mut self) {
        if self.search.matches.is_empty() {
            return;
        }
        let next = match self.search.current {
            Some(current) => (current + 1) % self.search.matches.len(),
            None => 0,
        };
        self.jump_to_match(next);
    }

    /// Jump to the first match at or after the cursor, wrapping around.
    pub(super) fn jump_to_first_from_cursor(&mut self) {
        if self.search.matches.is_empty() {
            return;
        }
        let cursor = self
            .editor
            .active_document()
            .map(|doc| doc.clamped_cursor())
            .unwrap_or_default();
        let index = self
            .search
            .matches
            .iter()
            .position(|(start, _)| *start >= cursor)
            .unwrap_or(0);
        self.jump_to_match(index);
    }

    pub(super) fn refresh_search_matches(&mut self) {
        let query = self.search.query.clone();
        let case_sensitive = self.search.case_sensitive;
        let whole_word = self.search.whole_word;
        let regex = self.search.regex;
        // A stale scope (for example after an outside edit) is clamped to the
        // current document so a find cannot look outside the text it started in.
        let scope = self
            .search
            .scope
            .map(|(start, end)| match self.editor.active_document() {
                Some(doc) => (
                    doc.buffer.clamp_position(start),
                    doc.buffer.clamp_position(end),
                ),
                None => (start, end),
            });

        let result: Result<Vec<_>, String> = if query.is_empty() {
            Ok(Vec::new())
        } else if let Some(doc) = self.editor.active_document() {
            if regex {
                doc.find_all_regex(&query, case_sensitive)
            } else {
                Ok(doc.find_all_with(&query, case_sensitive, whole_word))
            }
        } else {
            Ok(Vec::new())
        };

        self.search.matches = match result {
            Ok(matches) => {
                self.search.regex_error = None;
                matches_in_scope(matches, scope)
            }
            Err(message) => {
                self.search.regex_error = Some(message);
                Vec::new()
            }
        };
        if self
            .search
            .current
            .is_none_or(|current| current >= self.search.matches.len())
        {
            self.search.current = if self.search.matches.is_empty() {
                None
            } else {
                Some(0)
            };
        }
    }

    pub(super) fn open_search(&mut self, replace: bool) {
        self.open_search_scoped(replace, None);
    }

    /// Open find/replace restricted to the active contiguous selection.
    pub(super) fn open_search_in_selection(&mut self, replace: bool) {
        let Some(doc) = self.editor.active_document() else {
            self.set_status("No file open");
            return;
        };
        if doc.has_column_selection() {
            self.set_error("Find in Selection needs a contiguous selection");
            return;
        }
        let Some(scope) = doc.selection_range() else {
            self.set_status("Select text before searching inside a selection");
            return;
        };
        self.open_search_scoped(replace, Some(scope));
    }

    fn open_search_scoped(&mut self, replace: bool, scope: Option<(Position, Position)>) {
        self.search.open = true;
        self.search.replace_mode = replace;
        self.search.field = SearchField::Query;
        self.search.scope = scope;
        if !replace {
            self.search.replacement.clear();
        }
        // Prefill from a single-line selection, so Ctrl+F searches the word the
        // user already highlighted. A scoped search already has its range, so
        // prefilling the whole selection would just search for itself.
        if scope.is_none()
            && let Some(text) = self
                .editor
                .active_document()
                .and_then(|doc| doc.selected_text())
        {
            let trimmed = text.trim();
            if !trimmed.is_empty() && !trimmed.contains('\n') {
                self.search.query = trimmed.to_string();
            }
        }
        self.refresh_search_matches();
        self.jump_to_first_from_cursor();
    }

    pub(super) fn find_previous(&mut self) {
        if self.search.matches.is_empty() {
            return;
        }
        let previous = match self.search.current {
            Some(0) | None => self.search.matches.len() - 1,
            Some(current) => current - 1,
        };
        self.jump_to_match(previous);
    }

    /// Open a read-only preview of what replace-all would change.
    pub(super) fn open_replace_preview(&mut self) {
        if !self.search.open {
            self.set_status("Open find and type a query first");
            return;
        }
        self.refresh_search_matches();
        if self.search.matches.is_empty() {
            self.set_status("No matches to preview");
            return;
        }
        let replacement = self.search.replacement.clone();
        let matches = self.search.matches.clone();
        let Some(doc) = self.editor.active_document() else {
            return;
        };
        let rows = preview_rows(doc, &matches, &replacement);
        let name = doc
            .buffer
            .path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "the active file".to_string());
        self.overlay = Overlay::ReplacePreview(ReplacePreview {
            title: format!("Replace in {name}"),
            rows,
            matches: matches.len(),
            scroll: 0,
        });
    }

    /// Handle keys while the replace preview is open.
    pub(super) fn handle_replace_preview_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.overlay = Overlay::None,
            KeyCode::Char('q') if !ctrl => self.overlay = Overlay::None,
            KeyCode::Enter => {
                self.overlay = Overlay::None;
                self.replace_all();
            }
            KeyCode::Char('a') if !ctrl => {
                self.overlay = Overlay::None;
                self.replace_all();
            }
            KeyCode::Up => self.scroll_replace_preview(-1),
            KeyCode::Down => self.scroll_replace_preview(1),
            KeyCode::PageUp => self.scroll_replace_preview(-10),
            KeyCode::PageDown => self.scroll_replace_preview(10),
            KeyCode::Home => self.scroll_replace_preview(i64::MIN),
            KeyCode::End => self.scroll_replace_preview(i64::MAX),
            _ => {}
        }
    }

    fn scroll_replace_preview(&mut self, delta: i64) {
        if let Overlay::ReplacePreview(preview) = &mut self.overlay {
            let max = preview.rows.len().saturating_sub(1);
            let next = (preview.scroll as i64).saturating_add(delta);
            preview.scroll = next.clamp(0, max as i64) as usize;
        }
    }

    /// Replace every current match in the active file as one undoable edit.
    pub(super) fn replace_all(&mut self) {
        if self.read_only_block() {
            return;
        }
        if !self.search.open || self.search.query.is_empty() {
            self.set_status("Open find and type a query first");
            return;
        }
        self.refresh_search_matches();
        let matches = self.search.matches.clone();
        if matches.is_empty() {
            self.set_status("No matches to replace");
            return;
        }

        let replacement = self.search.replacement.clone();
        let (text, end) = match self.editor.active_document() {
            Some(doc) => {
                let last = doc.buffer.len_lines().saturating_sub(1);
                let end = Position::new(last, doc.buffer.line_char_len(last));
                (doc.buffer.text(), end)
            }
            None => return,
        };

        // Map `(row, col)` positions to character offsets.
        let mut chars: Vec<char> = text.chars().collect();
        let total = chars.len();
        let mut line_starts = vec![0usize];
        for (index, ch) in chars.iter().enumerate() {
            if *ch == '\n' {
                line_starts.push(index + 1);
            }
        }
        let offset = |position: Position| -> usize {
            let start = line_starts.get(position.row).copied().unwrap_or(total);
            (start + position.col).min(total)
        };

        // Replacements change the scope's end offset; remember the shift before
        // mutating the character vector, since offsets are computed on the old text.
        let scope_start = self.search.scope.map(|(start, _)| start);
        let scope_end = self.search.scope.map(|(_, end)| {
            let end = offset(end);
            let shift: isize = matches
                .iter()
                .filter(|(_, finish)| offset(*finish) <= end)
                .map(|(start, finish)| {
                    replacement.chars().count() as isize
                        - (offset(*finish) - offset(*start)) as isize
                })
                .sum();
            (end as isize + shift).max(0) as usize
        });

        // Apply from the end so earlier offsets stay valid. Matches never
        // overlap, so this is safe.
        let mut replaced = 0usize;
        for (start, finish) in matches.iter().rev() {
            let from = offset(*start);
            let to = offset(*finish);
            if from >= to {
                continue;
            }
            chars.splice(from..to, replacement.chars());
            replaced += 1;
        }
        if replaced == 0 {
            self.set_status("No matches to replace");
            return;
        }

        let new_text: String = chars.into_iter().collect();
        self.with_doc(|doc| doc.replace_range(Position::zero(), end, &new_text));
        if let (Some(start), Some(end)) = (scope_start, scope_end)
            && let Some(doc) = self.editor.active_document()
        {
            self.search.scope = Some((start, doc.buffer.char_to_position(end)));
        }
        self.refresh_search_matches();
        self.set_status(format!("Replaced {replaced} occurrence(s)"));
    }
}

/// Build preview rows from matches against the document's current text.
///
/// Single-line matches are grouped by row so each row shows every replacement
/// it would receive. A regex that spans lines is shown as its matched text on
/// the starting line, with newlines rendered as `⏎` so the preview stays one row.
fn preview_rows(
    doc: &Document,
    matches: &[(Position, Position)],
    replacement: &str,
) -> Vec<ReplacePreviewRow> {
    let mut by_row: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    let mut spanning = Vec::new();
    for &(start, end) in matches {
        if start.row == end.row {
            by_row
                .entry(start.row)
                .or_default()
                .push((start.col, end.col));
        } else {
            spanning.push((start, end));
        }
    }

    let mut rows = Vec::new();
    for (row, mut spans) in by_row {
        spans.sort_unstable();
        let before = doc.buffer.line_text(row);
        let chars: Vec<char> = before.chars().collect();
        let mut after = String::new();
        let mut cursor = 0;
        for (start, end) in spans {
            let start = start.min(chars.len());
            let end = end.min(chars.len()).max(start);
            if start < cursor {
                continue;
            }
            after.extend(&chars[cursor..start]);
            after.push_str(replacement);
            cursor = end;
        }
        after.extend(&chars[cursor..]);
        rows.push(ReplacePreviewRow {
            line: row + 1,
            before: single_line(&before),
            after: single_line(&after),
        });
    }
    for (start, end) in spanning {
        let before = doc
            .buffer
            .as_rope()
            .slice(doc.buffer.position_to_char(start)..doc.buffer.position_to_char(end))
            .to_string();
        rows.push(ReplacePreviewRow {
            line: start.row + 1,
            before: single_line(&before),
            after: single_line(replacement),
        });
    }
    rows.sort_by_key(|row| row.line);
    rows
}

/// Render multi-line text on one preview row.
fn single_line(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\n', "⏎")
        .replace('\r', "")
}

/// Keep only matches fully inside `scope`.
fn matches_in_scope(
    matches: Vec<(Position, Position)>,
    scope: Option<(Position, Position)>,
) -> Vec<(Position, Position)> {
    match scope {
        Some((start, end)) => matches
            .into_iter()
            .filter(|(match_start, match_end)| *match_start >= start && *match_end <= end)
            .collect(),
        None => matches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_filter_keeps_only_contained_matches() {
        let matches = vec![
            (Position::new(0, 0), Position::new(0, 3)),
            (Position::new(1, 0), Position::new(1, 3)),
            (Position::new(2, 0), Position::new(2, 3)),
        ];
        let scoped = matches_in_scope(
            matches.clone(),
            Some((Position::new(1, 0), Position::new(2, 0))),
        );
        assert_eq!(scoped, vec![(Position::new(1, 0), Position::new(1, 3))]);

        let overlapping =
            matches_in_scope(matches, Some((Position::new(0, 2), Position::new(0, 3))));
        assert!(overlapping.is_empty(), "partial overlaps are excluded");
    }

    #[test]
    fn preview_groups_matches_by_line() {
        let doc = Document::new(crate::editor::Buffer::from_text(
            "foo foo\nbar foo\nnone\n",
            None,
        ));
        let matches = doc.find_all_with("foo", true, false);
        let rows = preview_rows(&doc, &matches, "X");
        assert_eq!(
            rows,
            vec![
                ReplacePreviewRow {
                    line: 1,
                    before: "foo foo".to_string(),
                    after: "X X".to_string(),
                },
                ReplacePreviewRow {
                    line: 2,
                    before: "bar foo".to_string(),
                    after: "bar X".to_string(),
                },
            ]
        );
    }
}
