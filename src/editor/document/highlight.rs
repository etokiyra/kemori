//! Extracted from `document/mod.rs`; see `docs/DECISIONS.md`.

use super::*;

impl Document {
    pub(super) fn invalidate_highlight(&mut self, row: usize) {
        self.highlight_computed = self.highlight_computed.min(row).max(1);
        self.lsp_highlights.clear();
        self.inlay_hints.clear();
        self.inlay_hints_version = None;
        self.code_lenses.clear();
        self.code_lenses_version = None;
        self.selection_ranges.clear();
        self.selection_ranges_key = None;
    }

    /// State at the beginning of `row`, computing and caching as needed.
    pub(super) fn highlight_state(
        &mut self,
        provider: &dyn LanguageProvider,
        row: usize,
    ) -> HighlightState {
        let row = row.min(self.buffer.len_lines().saturating_sub(1));
        while self.highlight_computed <= row {
            let prev = self.highlight_computed - 1;
            let text = self.buffer.line_text(prev);
            let state = self.highlight_states[prev];
            let (_, next) = provider.highlight(&text, state);
            if self.highlight_states.len() > self.highlight_computed {
                self.highlight_states[self.highlight_computed] = next;
            } else {
                self.highlight_states.push(next);
            }
            self.highlight_computed += 1;
        }
        self.highlight_states[row]
    }

    /// Highlighted spans for a single line.
    pub fn highlight_spans(
        &mut self,
        provider: &dyn LanguageProvider,
        row: usize,
    ) -> Vec<HighlightSpan> {
        if self.is_large_file() || row >= self.buffer.len_lines() {
            return Vec::new();
        }
        let state = self.highlight_state(provider, row);
        let text = self.buffer.line_text(row);
        provider.highlight(&text, state).0
    }

    /// The highlighted token at a character position.
    ///
    /// Used to keep automatic completion quiet inside comments and strings,
    /// where document words would only be noise.
    pub fn token_kind_at(
        &mut self,
        provider: &dyn LanguageProvider,
        row: usize,
        col: usize,
    ) -> TokenKind {
        self.highlight_spans(provider, row)
            .into_iter()
            .find(|span| span.range.contains(&col))
            .map(|span| span.kind)
            .unwrap_or(TokenKind::Plain)
    }

    // ----------------------------------------------------------------------
    // Search
    // ----------------------------------------------------------------------

    /// All occurrences of `query` in the buffer, as `(start, end)` positions.
    ///
    /// Searches line by line so a keystroke never copies the whole file. This is
    /// a case-sensitive substring search; the find bar uses
    /// [`Self::find_all_with`] for its options.
    pub fn find_all(&self, query: &str) -> Vec<(Position, Position)> {
        self.find_all_with(query, true, false)
    }

    /// All occurrences of `query`, honouring case sensitivity and whole-word
    /// matching.
    pub fn find_all_with(
        &self,
        query: &str,
        case_sensitive: bool,
        whole_word: bool,
    ) -> Vec<(Position, Position)> {
        let needle: Vec<char> = query.chars().collect();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut matches = Vec::new();
        for row in 0..self.buffer.len_lines() {
            let line = self.buffer.line_text(row);
            let hay: Vec<char> = line.chars().collect();
            if hay.len() < needle.len() {
                continue;
            }
            let mut offset = 0usize;
            while offset + needle.len() <= hay.len() {
                if matches_at(&hay, offset, &needle, case_sensitive)
                    && (!whole_word || is_word_boundary(&hay, offset, needle.len()))
                {
                    matches.push((
                        Position::new(row, offset),
                        Position::new(row, offset + needle.len()),
                    ));
                    offset += needle.len();
                } else {
                    offset += 1;
                }
            }
        }
        matches
    }

    /// All occurrences of `pattern` interpreted as a regular expression.
    ///
    /// Returns an error for unsupported syntax so the find bar can explain it.
    /// Zero-width matches are skipped because they are rarely useful to step
    /// through.
    pub fn find_all_regex(
        &self,
        pattern: &str,
        case_sensitive: bool,
    ) -> Result<Vec<(Position, Position)>, String> {
        let regex = crate::regex::Regex::new(pattern)?;
        let case_insensitive = !case_sensitive;
        let mut matches = Vec::new();
        for row in 0..self.buffer.len_lines() {
            let line = self.buffer.line_text(row);
            let chars: Vec<char> = line.chars().collect();
            let mut from = 0usize;
            while from <= chars.len() {
                let Some((start, end)) = regex.find(&chars, from, case_insensitive) else {
                    break;
                };
                if end == start {
                    from = start + 1;
                    continue;
                }
                matches.push((Position::new(row, start), Position::new(row, end)));
                from = end;
            }
        }
        Ok(matches)
    }

    // ----------------------------------------------------------------------
    // Brackets
    // ----------------------------------------------------------------------

    /// The bracket pair surrounding the cursor, if any.
    ///
    /// Looks at the character under the cursor first, then the one before it.
    /// Matching is nesting- and type-aware, and skips brackets inside comments
    /// and strings using the provider's highlighting.
    pub fn matching_brackets(
        &mut self,
        provider: &dyn LanguageProvider,
    ) -> Option<(Position, Position)> {
        let cursor = self.clamped_cursor();
        let at = self.char_at(cursor);
        let before = if cursor.col > 0 {
            self.char_at(Position::new(cursor.row, cursor.col - 1))
        } else {
            None
        };

        let (position, bracket) =
            at.filter(|c| is_bracket(*c))
                .map(|c| (cursor, c))
                .or_else(|| {
                    before
                        .filter(|c| is_bracket(*c))
                        .map(|c| (Position::new(cursor.row, cursor.col.saturating_sub(1)), c))
                })?;

        let forward = is_open_bracket(bracket);
        let mut stack = vec![bracket];
        let mut cache: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
        let mut current = position;
        let mut steps = 0usize;

        loop {
            steps += 1;
            if steps > 100_000 {
                return None;
            }
            current = if forward {
                self.next_pos(current)?
            } else {
                self.prev_pos(current)?
            };

            cache.entry(current.row).or_insert_with(|| {
                self.highlight_spans(provider, current.row)
                    .into_iter()
                    .filter(|span| matches!(span.kind, TokenKind::Comment | TokenKind::String))
                    .map(|span| (span.range.start, span.range.end))
                    .collect()
            });
            if cache[&current.row]
                .iter()
                .any(|(start, end)| current.col >= *start && current.col < *end)
            {
                continue;
            }

            let Some(c) = self.char_at(current) else {
                continue;
            };

            if forward {
                if is_open_bracket(c) {
                    stack.push(c);
                } else if is_close_bracket(c) {
                    if stack.last().is_some_and(|open| brackets_match(*open, c)) {
                        stack.pop();
                        if stack.is_empty() {
                            return Some((position, current));
                        }
                    } else {
                        return None;
                    }
                }
            } else if is_close_bracket(c) {
                stack.push(c);
            } else if is_open_bracket(c) {
                if stack.last().is_some_and(|close| brackets_match(c, *close)) {
                    stack.pop();
                    if stack.is_empty() {
                        return Some((position, current));
                    }
                } else {
                    return None;
                }
            }
        }
    }

    /// Move the cursor to the bracket matching the one under (or just before)
    /// it, if the provider finds a pair.
    pub fn goto_matching_bracket(&mut self, provider: &dyn LanguageProvider) {
        if let Some((_, partner)) = self.matching_brackets(provider) {
            self.cursor = self.buffer.clamp_position(partner);
            self.selection = None;
            self.cursors.clear();
            self.preferred_col = None;
        }
    }
}
