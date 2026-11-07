// Search and popup state; included from `overlay.rs`.

/// Which search field has focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchField {
    Query,
    Replacement,
}

/// State for the find/replace bar.
pub struct Search {
    pub open: bool,
    pub replace_mode: bool,
    pub field: SearchField,
    pub query: String,
    pub replacement: String,
    /// Match case exactly rather than case-insensitively.
    pub case_sensitive: bool,
    /// Match whole words only.
    pub whole_word: bool,
    /// Interpret the query as a regular expression.
    pub regex: bool,
    /// The last regex compilation error, shown in the bar.
    pub regex_error: Option<String>,
    /// Matches are restricted to this contiguous range when present.
    pub scope: Option<(Position, Position)>,
    pub matches: Vec<(Position, Position)>,
    pub current: Option<usize>,
}

impl Default for Search {
    fn default() -> Self {
        Search {
            open: false,
            replace_mode: false,
            field: SearchField::Query,
            query: String::new(),
            replacement: String::new(),
            case_sensitive: false,
            whole_word: false,
            regex: false,
            regex_error: None,
            scope: None,
            matches: Vec::new(),
            current: None,
        }
    }
}

impl Search {
    pub fn close(&mut self) {
        self.open = false;
        self.replace_mode = false;
        self.field = SearchField::Query;
        self.matches.clear();
        self.current = None;
        self.regex_error = None;
        self.scope = None;
    }
}

/// One affected line in a replacement preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplacePreviewRow {
    /// 1-based line number shown in the gutter.
    pub line: usize,
    pub before: String,
    pub after: String,
}

/// A read-only preview of every line a replace-all would change.
pub struct ReplacePreview {
    pub title: String,
    pub rows: Vec<ReplacePreviewRow>,
    /// Number of individual matches, which can be greater than the row count.
    pub matches: usize,
    pub scroll: usize,
}

impl ReplacePreview {
    pub fn scroll_by(&mut self, delta: isize) {
        self.scroll = (self.scroll as isize + delta).max(0) as usize;
    }
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

/// The hover popup's state.
pub struct HoverState {
    pub title: String,
    pub kind: Option<crate::language::symbols::SymbolKind>,
    pub body: Vec<String>,
}

/// The signature-help popup's state.
pub struct SignatureState {
    pub help: crate::language::lsp::convert::SignatureHelp,
    /// The screen anchor captured when the response arrived.
    pub anchor: Option<(u16, u16)>,
}

/// The completion popup's state: a candidate pool filtered by the typed prefix.
pub struct CompletionState {
    pool: Vec<Completion>,
    pub prefix: String,
    pub items: Vec<Completion>,
    pub selected: usize,
}

impl CompletionState {
    pub fn new(pool: Vec<Completion>, prefix: String) -> Self {
        let mut state = CompletionState {
            pool,
            prefix,
            items: Vec::new(),
            selected: 0,
        };
        state.refilter();
        state
    }

    /// Recompute `items` from the pool for the current prefix.
    ///
    /// Matching is a fuzzy subsequence so `mrs` still finds `main_result`, but
    /// the ordering keeps prefix and short matches first: `if` ranks above
    /// `impl`, and an exact prefix beats a scattered match.
    pub fn refilter(&mut self) {
        let lower = self.prefix.to_lowercase();
        if lower.is_empty() {
            self.items = self.pool.clone();
        } else {
            let mut scored: Vec<(i32, &Completion)> = self
                .pool
                .iter()
                .filter_map(|item| {
                    fuzzy_score(&lower, &item.label).map(|score| {
                        // A prefix match is a strong signal; bias it upward.
                        let bias = if item.label.to_lowercase().starts_with(&lower) {
                            -100
                        } else {
                            0
                        };
                        (score + bias, item)
                    })
                })
                .collect();
            scored.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| a.1.label.len().cmp(&b.1.label.len()))
                    .then_with(|| a.1.label.cmp(&b.1.label))
            });
            self.items = scored.into_iter().map(|(_, item)| item.clone()).collect();
        }
        if self.selected >= self.items.len() {
            self.selected = self.items.len().saturating_sub(1);
        }
    }

    /// Merge additional candidates (e.g. from a language server) and re-filter.
    pub fn extend(&mut self, items: Vec<Completion>) {
        for item in items {
            if let Some(existing) = self
                .pool
                .iter_mut()
                .find(|existing| existing.label == item.label)
            {
                existing.set_metadata(item.detail, item.documentation);
                if item.raw.is_some() {
                    existing.raw = item.raw;
                }
            } else {
                self.pool.push(item);
            }
        }
        self.refilter();
    }

    pub fn set_prefix(&mut self, prefix: String) {
        self.prefix = prefix;
        self.selected = 0;
        self.refilter();
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
    }

    pub fn move_by(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let last = self.items.len() as i32 - 1;
        self.selected = (self.selected as i32 + delta).clamp(0, last) as usize;
    }

    pub fn selected_item(&self) -> Option<&Completion> {
        self.items.get(self.selected)
    }

    /// Merge resolved metadata into every copy of `label`.
    pub fn update_item(
        &mut self,
        label: &str,
        detail: String,
        documentation: String,
        edit: crate::language::completion::CompletionEdit,
    ) {
        for item in &mut self.pool {
            if item.label == label {
                item.set_metadata(detail.clone(), documentation.clone());
                item.apply_edit(edit.clone());
            }
        }
        for item in &mut self.items {
            if item.label == label {
                item.set_metadata(detail.clone(), documentation.clone());
                item.apply_edit(edit.clone());
            }
        }
    }
}
