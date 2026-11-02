//! Project-wide search and replace: state, keys and background hand-off.
//!
//! The overlay is one form with the same options the in-file find bar exposes,
//! plus include/exclude globs and a replacement preview. The scan itself runs on
//! the background worker; the overlay only receives a revisioned result, so a
//! slow search cannot overwrite a newer query.

use super::*;
use crate::search::{ReplaceOutcome, SearchMatch, SearchOptions};

/// Which part of the search form owns the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectSearchField {
    Query,
    Replacement,
    Include,
    Exclude,
    /// The result list; typing returns to the query.
    Results,
}

impl ProjectSearchField {
    /// The label shown beside the field.
    pub fn label(self) -> &'static str {
        match self {
            ProjectSearchField::Query => "Find",
            ProjectSearchField::Replacement => "Replace",
            ProjectSearchField::Include => "Include",
            ProjectSearchField::Exclude => "Exclude",
            ProjectSearchField::Results => "Results",
        }
    }

    fn next(self) -> Self {
        match self {
            ProjectSearchField::Query => ProjectSearchField::Replacement,
            ProjectSearchField::Replacement => ProjectSearchField::Include,
            ProjectSearchField::Include => ProjectSearchField::Exclude,
            ProjectSearchField::Exclude => ProjectSearchField::Results,
            ProjectSearchField::Results => ProjectSearchField::Query,
        }
    }

    fn previous(self) -> Self {
        match self {
            ProjectSearchField::Query => ProjectSearchField::Results,
            ProjectSearchField::Replacement => ProjectSearchField::Query,
            ProjectSearchField::Include => ProjectSearchField::Replacement,
            ProjectSearchField::Exclude => ProjectSearchField::Include,
            ProjectSearchField::Results => ProjectSearchField::Exclude,
        }
    }
}

/// State for the project search/replace overlay.
pub struct ProjectSearchState {
    /// The project root, so results can show relative paths.
    pub root: PathBuf,
    pub query: String,
    pub replacement: String,
    pub options: SearchOptions,
    pub field: ProjectSearchField,
    pub results: Vec<SearchMatch>,
    pub selected: usize,
    /// Set while a search or replacement is in flight.
    pub running: bool,
    /// A short result or instruction shown in the overlay's footer.
    pub status: Option<String>,
    /// A query or replacement error, shown in the footer.
    pub error: Option<String>,
    /// Replacing with an empty string needs a second confirmation.
    pub confirm_empty: bool,
}

impl ProjectSearchState {
    /// Build the form, optionally prefilled from the selection.
    pub fn new(prefill: &str, root: PathBuf) -> Self {
        ProjectSearchState {
            root,
            query: prefill.to_string(),
            replacement: String::new(),
            options: SearchOptions::default(),
            field: ProjectSearchField::Query,
            results: Vec::new(),
            selected: 0,
            running: false,
            status: None,
            error: None,
            confirm_empty: false,
        }
    }

    /// The text input that currently has focus, if any.
    pub fn active_input_mut(&mut self) -> Option<&mut String> {
        match self.field {
            ProjectSearchField::Query => Some(&mut self.query),
            ProjectSearchField::Replacement => Some(&mut self.replacement),
            ProjectSearchField::Include => Some(&mut self.options.include),
            ProjectSearchField::Exclude => Some(&mut self.options.exclude),
            ProjectSearchField::Results => None,
        }
    }

    /// The text currently in the focused field, if any.
    pub fn active_input(&self) -> Option<&str> {
        match self.field {
            ProjectSearchField::Query => Some(self.query.as_str()),
            ProjectSearchField::Replacement => Some(self.replacement.as_str()),
            ProjectSearchField::Include => Some(self.options.include.as_str()),
            ProjectSearchField::Exclude => Some(self.options.exclude.as_str()),
            ProjectSearchField::Results => None,
        }
    }

    pub fn next_field(&mut self) {
        self.field = self.field.next();
    }

    pub fn previous_field(&mut self) {
        self.field = self.field.previous();
    }

    /// Append a character to the focused field, or start editing the query.
    pub fn push_char(&mut self, ch: char) {
        if self.field == ProjectSearchField::Results {
            self.field = ProjectSearchField::Query;
        }
        if let Some(input) = self.active_input_mut() {
            input.push(ch);
            self.stale();
        }
    }

    pub fn backspace(&mut self) {
        if self.field == ProjectSearchField::Results {
            self.field = ProjectSearchField::Query;
        }
        if let Some(input) = self.active_input_mut() {
            input.pop();
            self.stale();
        }
    }

    /// Mark results as out of date after an edit.
    fn stale(&mut self) {
        self.confirm_empty = false;
        if !self.results.is_empty() {
            self.status = Some("Enter searches with the new text".to_string());
        }
    }

    pub fn set_results(&mut self, results: Vec<SearchMatch>) {
        self.results = results;
        self.selected = 0;
        self.running = false;
        self.error = None;
        self.status = Some(match self.results.is_empty() {
            true => "No matches".to_string(),
            false => format!(
                "{} match(es) in {} file(s)",
                self.results.len(),
                self.file_count()
            ),
        });
    }

    pub fn file_count(&self) -> usize {
        let mut files: Vec<&Path> = Vec::new();
        for entry in &self.results {
            if !files.contains(&entry.path.as_path()) {
                files.push(&entry.path);
            }
        }
        files.len()
    }

    pub fn move_selection(&mut self, delta: i64) {
        if self.results.is_empty() {
            self.selected = 0;
        } else {
            let next = (self.selected as i64).saturating_add(delta);
            self.selected = next.clamp(0, self.results.len() as i64 - 1) as usize;
        }
    }

    pub fn selected_match(&self) -> Option<&SearchMatch> {
        self.results.get(self.selected)
    }
}

impl App {
    /// Open project search, prefilled from a single-line selection.
    pub(super) fn open_project_search(&mut self) {
        // A result from a previous form must never land in a new one.
        self.pending_project_search = None;
        self.pending_project_replace = None;
        let prefill = self
            .editor
            .active_document()
            .and_then(|doc| doc.selected_text())
            .filter(|text| !text.contains('\n') && !text.trim().is_empty())
            .unwrap_or_default();
        self.overlay = Overlay::ProjectSearch(ProjectSearchState::new(
            prefill.trim(),
            self.workspace.root().to_path_buf(),
        ));
    }

    /// Handle a key while the project search overlay is open.
    pub(super) fn handle_project_search_key(&mut self, key: KeyEvent) {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if alt {
            // Option toggles mirror the in-file find bar.
            if let Overlay::ProjectSearch(state) = &mut self.overlay {
                match key.code {
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        state.options.case_sensitive = !state.options.case_sensitive;
                    }
                    KeyCode::Char('w') | KeyCode::Char('W') => {
                        state.options.whole_word = !state.options.whole_word;
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        state.options.regex = !state.options.regex;
                    }
                    KeyCode::Enter => {
                        self.replace_project_matches();
                        return;
                    }
                    _ => return,
                }
            }
            self.run_project_search();
            return;
        }
        if ctrl {
            if key.code == KeyCode::Enter {
                self.replace_project_matches();
            }
            return;
        }

        match key.code {
            KeyCode::Esc => self.overlay = Overlay::None,
            KeyCode::Tab => {
                if let Overlay::ProjectSearch(state) = &mut self.overlay {
                    state.next_field();
                }
            }
            KeyCode::BackTab => {
                if let Overlay::ProjectSearch(state) = &mut self.overlay {
                    state.previous_field();
                }
            }
            KeyCode::Enter => {
                let on_results = matches!(
                    &self.overlay,
                    Overlay::ProjectSearch(state) if state.field == ProjectSearchField::Results
                );
                if on_results {
                    self.open_selected_project_match();
                } else {
                    self.run_project_search();
                }
            }
            KeyCode::Up => self.move_project_selection(-1),
            KeyCode::Down => self.move_project_selection(1),
            KeyCode::PageUp => self.move_project_selection(-10),
            KeyCode::PageDown => self.move_project_selection(10),
            KeyCode::Home => self.move_project_selection(i64::MIN),
            KeyCode::End => self.move_project_selection(i64::MAX),
            KeyCode::Backspace => {
                if let Overlay::ProjectSearch(state) = &mut self.overlay {
                    state.backspace();
                }
            }
            KeyCode::Char(ch) if !ctrl => {
                if let Overlay::ProjectSearch(state) = &mut self.overlay {
                    state.push_char(ch);
                }
            }
            _ => {}
        }
    }

    fn move_project_selection(&mut self, delta: i64) {
        if let Overlay::ProjectSearch(state) = &mut self.overlay {
            state.move_selection(delta);
        }
    }

    /// Ask the worker to search with the form's current options.
    pub(super) fn run_project_search(&mut self) {
        let (query, options, replacement) = match &mut self.overlay {
            Overlay::ProjectSearch(state) => {
                if state.query.is_empty() {
                    state.error = Some("Type something to find".to_string());
                    state.status = None;
                    return;
                }
                state.running = true;
                state.error = None;
                state.status = Some("Searching…".to_string());
                let replacement =
                    (!state.replacement.is_empty()).then(|| state.replacement.clone());
                (state.query.clone(), state.options.clone(), replacement)
            }
            _ => return,
        };

        self.project_search_seq += 1;
        let revision = self.project_search_seq;
        self.pending_project_search = Some(revision);
        self.background.search_project(
            self.workspace.root().to_path_buf(),
            query,
            options,
            replacement,
            revision,
        );
    }

    /// Replace every match, refusing to touch dirty open buffers.
    fn replace_project_matches(&mut self) {
        let (query, options, replacement) = match &mut self.overlay {
            Overlay::ProjectSearch(state) => {
                if state.query.is_empty() {
                    state.error = Some("Type something to find".to_string());
                    return;
                }
                if state.results.is_empty() {
                    state.error = Some("Search first, then replace".to_string());
                    return;
                }
                if state.replacement.is_empty() && !state.confirm_empty {
                    // Deleting every match is a real operation, but it deserves
                    // a deliberate second press rather than a stray key.
                    state.confirm_empty = true;
                    state.status = Some("Press Alt+Enter again to delete every match".to_string());
                    return;
                }
                state.running = true;
                state.error = None;
                state.confirm_empty = false;
                (
                    state.query.clone(),
                    state.options.clone(),
                    state.replacement.clone(),
                )
            }
            _ => return,
        };
        if self.pending_project_replace.is_some() {
            return;
        }

        let skip = self
            .editor
            .documents
            .iter()
            .filter(|doc| doc.is_dirty())
            .filter_map(|doc| doc.buffer.path.clone())
            .collect::<Vec<_>>();
        self.project_replace_seq += 1;
        let revision = self.project_replace_seq;
        self.pending_project_replace = Some(revision);
        self.background.replace_project(
            self.workspace.root().to_path_buf(),
            query,
            options,
            replacement,
            revision,
            skip,
        );
        self.set_status("Replacing in project…");
    }

    fn open_selected_project_match(&mut self) {
        let target = match &self.overlay {
            Overlay::ProjectSearch(state) => state
                .selected_match()
                .map(|entry| (entry.path.clone(), Position::new(entry.line, entry.col))),
            _ => None,
        };
        let Some((path, position)) = target else {
            return;
        };
        self.overlay = Overlay::None;
        self.focus = Focus::Editor;
        self.reveal(path, position);
    }

    /// Apply a finished project search.
    pub(super) fn apply_project_search(
        &mut self,
        revision: u64,
        result: Result<Vec<SearchMatch>, String>,
    ) -> bool {
        if self.pending_project_search != Some(revision) {
            return false;
        }
        self.pending_project_search = None;
        let mut changed = true;
        let mut status = None;
        let mut error = None;
        if let Overlay::ProjectSearch(state) = &mut self.overlay {
            match result {
                Ok(matches) => {
                    let count = matches.len();
                    let files = {
                        let mut files = Vec::new();
                        for entry in &matches {
                            if !files.contains(&entry.path) {
                                files.push(entry.path.clone());
                            }
                        }
                        files.len()
                    };
                    state.set_results(matches);
                    status = Some(if count == 0 {
                        "No matches in project".to_string()
                    } else if count >= crate::search::SEARCH_LIMIT {
                        format!(
                            "{}+ match(es) in {files} file(s); narrow the query or filters",
                            crate::search::SEARCH_LIMIT
                        )
                    } else {
                        format!("{count} match(es) in {files} file(s)")
                    });
                }
                Err(message) => {
                    state.running = false;
                    state.error = Some(message.clone());
                    state.status = None;
                    error = Some(message);
                }
            }
        } else {
            changed = false;
        }
        if let Some(message) = status {
            self.set_status(message);
        }
        if let Some(message) = error {
            self.set_error(message);
        }
        changed
    }

    /// Apply a finished project replacement.
    pub(super) fn apply_project_replace(
        &mut self,
        revision: u64,
        result: Result<ReplaceOutcome, String>,
    ) -> bool {
        if self.pending_project_replace != Some(revision) {
            return false;
        }
        self.pending_project_replace = None;
        match result {
            Ok(outcome) => {
                let skipped = outcome.skipped.len();
                let errors = outcome.errors.len();
                let mut message = format!(
                    "Replaced {} occurrence(s) in {} file(s)",
                    outcome.replacements, outcome.files_changed
                );
                if let Some(names) = file_names(&outcome.changed) {
                    message.push_str(&format!(": {names}"));
                }
                if skipped > 0 {
                    message.push_str(&format!("; skipped {skipped} unsaved file(s)"));
                }
                if errors > 0 {
                    message.push_str(&format!("; {errors} file(s) failed"));
                }
                // Reload open clean buffers immediately rather than waiting for
                // the next external-change poll. A dirty buffer was skipped by
                // the worker and is never reloaded here.
                for doc in &mut self.editor.documents {
                    if doc.is_dirty() {
                        continue;
                    }
                    if doc
                        .buffer
                        .path
                        .as_ref()
                        .is_some_and(|path| outcome.changed.contains(path))
                    {
                        let _ = doc.reload_from_disk();
                    }
                }
                if let Some((path, error)) = outcome.errors.first() {
                    let name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("file");
                    self.set_error(format!("{message} — {name}: {error}"));
                } else if skipped > 0 {
                    self.set_status(message.clone());
                    self.push_toast(ToastKind::Info, message.clone());
                } else {
                    self.set_status(message.clone());
                    self.push_toast(ToastKind::Success, message.clone());
                }
                if let Overlay::ProjectSearch(state) = &mut self.overlay {
                    state.running = false;
                    state.status = Some(message);
                }
                // The project tree and git status should reflect the new
                // contents immediately.
                self.workspace.refresh();
                self.request_git_refresh();
                true
            }
            Err(error) => {
                if let Overlay::ProjectSearch(state) = &mut self.overlay {
                    state.running = false;
                    state.error = Some(error.clone());
                }
                self.set_error(error);
                true
            }
        }
    }
}

/// A short, human list of the files a replacement changed.
fn file_names(paths: &[PathBuf]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let mut names: Vec<String> = paths
        .iter()
        .take(3)
        .map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file")
                .to_string()
        })
        .collect();
    if paths.len() > 3 {
        names.push(format!("and {} more", paths.len() - 3));
    }
    Some(names.join(", "))
}
