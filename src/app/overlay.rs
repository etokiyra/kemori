//! Transient UI state: pickers, prompts and the search bar.

use std::path::{Path, PathBuf};

use super::SettingsState;
use super::project_search::ProjectSearchState;
use crate::editor::Position;
use crate::filesystem;
use crate::language::completion::Completion;

include!("overlay_states.rs");

/// What happens when a picker item is chosen.
#[derive(Clone, Debug)]
pub enum PickerAction {
    Command(&'static str),
    OpenPath(PathBuf),
    /// Open a file and place the cursor at a position (diagnostics, symbols).
    Reveal {
        path: PathBuf,
        position: Position,
    },
    /// Open a file and reveal an LSP-encoded position, converting it to a Kemori
    /// character position using the target line's text.
    RevealLsp {
        path: PathBuf,
        position: Position,
    },
    /// Show a short informational message in the statusline.
    Info(String),
    /// Install an external tool through its trusted package manager.
    InstallTool(crate::language::tools::Tool),
    /// Provision every missing tool the current project needs.
    ProjectSetup,
    /// Open the update/remove choices for a Kemori-managed tool.
    ToolActions(crate::language::tools::Tool),
    /// Reinstall a Kemori-managed tool at Kemori's pinned version.
    UpdateTool(crate::language::tools::Tool),
    /// Remove a Kemori-managed tool's files (never a user/system install).
    RemoveTool(crate::language::tools::Tool),
    /// Apply the code action at an index in the last response.
    ApplyCodeAction(usize),
    /// Run a code lens in the active document.
    RunCodeLens(usize),
    /// Continue a prepared hierarchy using the node at this index.
    HierarchyItem(usize),
    /// Open a link that is not a file path (copy it for the user).
    OpenLink(String),
    /// Delete a file or directory after confirmation.
    DeletePath(PathBuf),
    /// Show the unified diff for a path (staged or working tree).
    ShowDiff {
        path: PathBuf,
        staged: bool,
    },
    /// Run one of the project's own tasks in the project root.
    RunTask(crate::project::tasks::Task),
    /// Show one historical task output by index.
    ShowTaskOutput(usize),
    /// Override the active document's language.
    SetLanguage(crate::language::id::LanguageId),
    /// Switch to an already-open workspace by index.
    SwitchWorkspace(usize),
    /// Answer the workspace-trust prompt.
    ResolveTrust(crate::app::trust::TrustDecision),
    /// Insert a built-in snippet at the cursor.
    InsertSnippet(crate::editor::snippet::Snippet),
}

/// A single row in a picker.
#[derive(Clone, Debug)]
pub struct PickerItem {
    pub label: String,
    pub detail: String,
    pub shortcut: String,
    pub action: PickerAction,
    /// Whether the action can run right now.
    pub enabled: bool,
    /// Why the action is unavailable, when it is.
    pub hint: Option<String>,
}

impl PickerItem {
    pub fn new(label: impl Into<String>, detail: impl Into<String>, action: PickerAction) -> Self {
        PickerItem {
            label: label.into(),
            detail: detail.into(),
            shortcut: String::new(),
            action,
            enabled: true,
            hint: None,
        }
    }

    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = shortcut.into();
        self
    }

    pub fn disabled(mut self, hint: impl Into<String>) -> Self {
        self.enabled = false;
        self.hint = Some(hint.into());
        self
    }
}

/// A filterable list overlay, used for the command palette and quick open.
pub struct Picker {
    pub title: String,
    pub placeholder: String,
    items: Vec<PickerItem>,
    pub filtered: Vec<usize>,
    pub query: String,
    pub selected: usize,
}

impl Picker {
    pub fn new(
        title: impl Into<String>,
        placeholder: impl Into<String>,
        items: Vec<PickerItem>,
    ) -> Self {
        let filtered = (0..items.len()).collect();
        Picker {
            title: title.into(),
            placeholder: placeholder.into(),
            items,
            filtered,
            query: String::new(),
            selected: 0,
        }
    }

    pub fn refilter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.items.len()).collect();
        } else {
            let mut scored: Vec<(i32, usize)> = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    let haystack = format!("{} {}", item.label, item.detail);
                    fuzzy_score(&self.query, &haystack).map(|score| (score, i))
                })
                .collect();
            scored.sort_by_key(|(score, i)| (*score, *i));
            self.filtered = scored.into_iter().map(|(_, i)| i).collect();
        }
        self.selected = 0;
    }

    pub fn item(&self, filtered_index: usize) -> Option<&PickerItem> {
        let idx = *self.filtered.get(filtered_index)?;
        self.items.get(idx)
    }

    pub fn selected_item(&self) -> Option<&PickerItem> {
        self.item(self.selected)
    }

    /// Append items that are not already present, preserving the selection.
    ///
    /// Used to merge language-server workspace symbols into a picker that was
    /// already populated by Kemori's built-in scan.
    pub fn extend_items(&mut self, items: Vec<PickerItem>) {
        let selected = self
            .selected_item()
            .map(|item| (item.label.clone(), item.detail.clone()));
        let mut seen: std::collections::HashSet<(String, String)> = self
            .items
            .iter()
            .map(|item| (item.label.clone(), item.detail.clone()))
            .collect();
        for item in items {
            if seen.insert((item.label.clone(), item.detail.clone())) {
                self.items.push(item);
            }
        }
        self.refilter();
        if let Some((label, detail)) = selected
            && let Some(index) = self
                .filtered
                .iter()
                .position(|&i| self.items[i].label == label && self.items[i].detail == detail)
        {
            self.selected = index;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
        }
    }

    pub fn push_char(&mut self, c: char) {
        self.query.push(c);
        self.refilter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.refilter();
    }
}

/// The kind of text prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptKind {
    GotoLine,
    OpenPath,
    SaveAs,
    Rename,
    NewFile,
    RenameFile,
    CommitMessage,
    CopyFile,
    DebugWatch,
    DebugAttach,
    BreakpointCondition,
    BreakpointLog,
}

/// A single-line text prompt.
pub struct Prompt {
    pub kind: PromptKind,
    pub label: String,
    pub placeholder: String,
    pub input: String,
}

impl Prompt {
    pub fn new(kind: PromptKind, label: impl Into<String>, placeholder: impl Into<String>) -> Self {
        Prompt {
            kind,
            label: label.into(),
            placeholder: placeholder.into(),
            input: String::new(),
        }
    }

    pub fn push_char(&mut self, c: char) {
        self.input.push(c);
    }

    pub fn backspace(&mut self) {
        self.input.pop();
    }
}

/// The currently visible overlay, if any.
pub enum Overlay {
    None,
    Picker(Picker),
    Prompt(Prompt),
    /// The keyboard-shortcuts cheatsheet.
    Help(Help),
    /// The settings screen.
    Settings(SettingsState),
    /// A directory browser, used to open a project.
    DirPicker(DirPicker),
    /// The guided "create a new project" flow.
    NewProject(NewProject),
    /// A read-only unified diff.
    Diff(DiffState),
    /// Project-wide search and replace.
    ProjectSearch(ProjectSearchState),
    /// A read-only preview of the in-file replacements before applying them.
    ReplacePreview(ReplacePreview),
}

impl Overlay {
    pub fn is_none(&self) -> bool {
        matches!(self, Overlay::None)
    }
}

/// The role of a line inside a unified diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLineKind {
    /// A `diff`/`index`/`---`/`+++` header line.
    Header,
    /// An `@@ … @@` hunk marker.
    Hunk,
    /// An added line.
    Add,
    /// A removed line.
    Remove,
    /// Unchanged context.
    Context,
}

/// One line of a rendered diff.
#[derive(Clone, Debug)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

/// A unified diff, scrolled with the arrows.
///
/// `path`, `root` and `staged` let individual hunks be applied or reverted
/// interactively; they are `None` for a diff that cannot be mutated.
pub struct DiffState {
    pub title: String,
    pub lines: Vec<DiffLine>,
    pub scroll: usize,
    pub path: Option<PathBuf>,
    pub root: Option<PathBuf>,
    pub staged: bool,
    /// Indices into `lines` where each `@@` hunk header begins.
    pub hunk_starts: Vec<usize>,
    /// Index of the hunk currently selected for apply/reject.
    pub current_hunk: usize,
}
impl DiffState {
    /// Build state from the output of `git diff`, classifying each line.
    pub fn from_unified(title: impl Into<String>, text: &str) -> Self {
        let lines: Vec<DiffLine> = text.lines().map(classify_diff_line).collect();
        let hunk_starts: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.kind == DiffLineKind::Hunk)
            .map(|(i, _)| i)
            .collect();
        DiffState {
            title: title.into(),
            lines,
            scroll: 0,
            path: None,
            root: None,
            staged: false,
            hunk_starts,
            current_hunk: 0,
        }
    }

    /// Attach mutation context so individual hunks can be applied or reverted.
    pub fn with_context(mut self, root: PathBuf, path: PathBuf, staged: bool) -> Self {
        self.root = Some(root);
        self.path = Some(path);
        self.staged = staged;
        self
    }

    pub fn scroll_by(&mut self, delta: i64) {
        self.scroll = (self.scroll as i64 + delta).max(0) as usize;
    }

    /// The patch text for the currently selected hunk, including the file
    /// headers git needs to apply it. `None` when there is no hunk.
    pub fn current_hunk_patch(&self) -> Option<String> {
        if self.hunk_starts.is_empty() {
            return None;
        }
        let index = self
            .current_hunk
            .min(self.hunk_starts.len().saturating_sub(1));
        let start = self.hunk_starts[index];
        let end = self
            .hunk_starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.lines.len());
        let mut patch = String::new();
        for line in &self.lines {
            if line.kind == DiffLineKind::Header {
                patch.push_str(&line.text);
                patch.push('\n');
            }
        }
        for line in &self.lines[start..end] {
            patch.push_str(&line.text);
            patch.push('\n');
        }
        Some(patch)
    }

    pub fn next_hunk(&mut self) {
        if !self.hunk_starts.is_empty() {
            self.current_hunk = (self.current_hunk + 1).min(self.hunk_starts.len() - 1);
        }
    }

    pub fn prev_hunk(&mut self) {
        self.current_hunk = self.current_hunk.saturating_sub(1);
    }
}

fn classify_diff_line(line: &str) -> DiffLine {
    let kind = if line.starts_with("@@") {
        DiffLineKind::Hunk
    } else if line.starts_with("+++")
        || line.starts_with("---")
        || line.starts_with("diff ")
        || line.starts_with("index ")
    {
        DiffLineKind::Header
    } else if line.starts_with('+') {
        DiffLineKind::Add
    } else if line.starts_with('-') {
        DiffLineKind::Remove
    } else {
        DiffLineKind::Context
    };
    DiffLine {
        kind,
        text: line.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Directory browsing
// ---------------------------------------------------------------------------

/// What a row in the directory browser represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirEntryKind {
    /// Confirm the directory currently being browsed.
    ChooseCurrent,
    /// Move to the parent directory.
    Parent,
    /// Descend into a subdirectory.
    Directory,
}

/// A row in the directory browser.
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub kind: DirEntryKind,
}

/// A keyboard directory browser: navigate with the arrows, descend with Enter,
/// and confirm the current folder with the `ChooseCurrent` row.
pub struct DirBrowser {
    pub current: PathBuf,
    pub entries: Vec<DirEntry>,
    pub selected: usize,
    /// A message shown when the directory could not be read.
    pub error: Option<String>,
}

impl DirBrowser {
    pub fn new(start: &Path) -> Self {
        let mut browser = DirBrowser {
            current: start.to_path_buf(),
            entries: Vec::new(),
            selected: 0,
            error: None,
        };
        browser.refresh();
        browser
    }

    /// Re-read the current directory's subdirectories.
    pub fn refresh(&mut self) {
        let mut entries = vec![DirEntry {
            name: "use this folder".to_string(),
            path: self.current.clone(),
            kind: DirEntryKind::ChooseCurrent,
        }];
        if let Some(parent) = self.current.parent() {
            entries.push(DirEntry {
                name: "..".to_string(),
                path: parent.to_path_buf(),
                kind: DirEntryKind::Parent,
            });
        }
        match filesystem::read_dir_sorted(&self.current) {
            Ok(list) => {
                self.error = None;
                for entry in list.into_iter().filter(|entry| entry.is_dir) {
                    entries.push(DirEntry {
                        name: entry.name,
                        path: entry.path,
                        kind: DirEntryKind::Directory,
                    });
                }
            }
            Err(err) => {
                self.error = Some(format!("cannot read this folder: {err}"));
            }
        }
        self.entries = entries;
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    pub fn selected(&self) -> Option<&DirEntry> {
        self.entries.get(self.selected)
    }

    /// Activate the highlighted row. Returns the chosen directory when the user
    /// confirmed the current folder; navigating returns `None`.
    pub fn activate(&mut self) -> Option<PathBuf> {
        let entry = self.selected()?.clone();
        match entry.kind {
            DirEntryKind::ChooseCurrent => Some(self.current.clone()),
            DirEntryKind::Parent | DirEntryKind::Directory => {
                self.current = entry.path;
                self.selected = 0;
                self.refresh();
                None
            }
        }
    }
}

/// A directory picker overlay, used by "Open Project…".
pub struct DirPicker {
    pub browser: DirBrowser,
}

impl DirPicker {
    pub fn new(start: &Path) -> Self {
        DirPicker {
            browser: DirBrowser::new(start),
        }
    }
}

// ---------------------------------------------------------------------------
// New project flow
// ---------------------------------------------------------------------------

/// Which step of the new-project flow is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewProjectStep {
    Parent,
    Name,
    Language,
}

/// State for the guided "create a new project" flow.
pub struct NewProject {
    pub step: NewProjectStep,
    pub browser: DirBrowser,
    /// The chosen parent directory (updated when step one completes).
    pub parent: PathBuf,
    pub name: String,
    pub error: Option<String>,
    /// Index into [`crate::project::create::CREATABLE`].
    pub language: usize,
}

impl NewProject {
    pub fn new(start: &Path) -> Self {
        NewProject {
            step: NewProjectStep::Parent,
            browser: DirBrowser::new(start),
            parent: start.to_path_buf(),
            name: String::new(),
            error: None,
            language: 0,
        }
    }
}

/// Scroll state for the help cheatsheet.
#[derive(Default)]
pub struct Help {
    pub scroll: usize,
}

/// A simple subsequence fuzzy matcher. Lower scores are better.
pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let text_chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut score = 0i32;
    let mut cursor = 0usize;
    let mut last_match: Option<usize> = None;

    for q in query.to_lowercase().chars() {
        let mut found = None;
        let mut i = cursor;
        while i < text_chars.len() {
            if text_chars[i] == q {
                found = Some(i);
                break;
            }
            i += 1;
        }
        let i = found?;
        if let Some(last) = last_match
            && i == last + 1
        {
            score -= 5;
        }
        score += i as i32;
        last_match = Some(i);
        cursor = i + 1;
    }
    Some(score)
}

/// Inline file-tree filter state.
///
/// The project file list is collected once when the filter opens, then filtered
/// in memory on each keystroke, so typing stays fast even in large projects.
pub struct TreeFilter {
    pub root: PathBuf,
    all: Vec<PathBuf>,
    pub query: String,
    pub matches: Vec<PathBuf>,
    pub selected: usize,
}

impl TreeFilter {
    pub fn new(root: &Path) -> Self {
        let all = crate::filesystem::collect_files(root, 8000);
        let mut filter = TreeFilter {
            root: root.to_path_buf(),
            all,
            query: String::new(),
            matches: Vec::new(),
            selected: 0,
        };
        filter.refilter();
        filter
    }

    pub fn refilter(&mut self) {
        if self.query.is_empty() {
            self.matches = self.all.iter().take(500).cloned().collect();
        } else {
            let query = self.query.to_lowercase();
            let root = self.root.clone();
            let mut scored: Vec<(i32, PathBuf)> = self
                .all
                .iter()
                .filter_map(|path| {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    // Score the path relative to the project root: the absolute
                    // prefix ("/home/you/project") would otherwise match almost
                    // any query.
                    let relative = path.strip_prefix(&root).unwrap_or(path);
                    let full = relative.to_string_lossy();
                    let name_score = fuzzy_score(&query, name);
                    let path_score = fuzzy_score(&query, &full).map(|score| score + 200);
                    match (name_score, path_score) {
                        (Some(a), Some(b)) => Some((a.min(b), path.clone())),
                        (Some(a), None) => Some((a, path.clone())),
                        (None, Some(b)) => Some((b, path.clone())),
                        (None, None) => None,
                    }
                })
                .collect();
            scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            self.matches = scored.into_iter().map(|(_, path)| path).collect();
        }
        self.selected = 0;
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.matches.len() {
            self.selected += 1;
        }
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.matches.get(self.selected).map(PathBuf::as_path)
    }

    pub fn push_char(&mut self, c: char) {
        self.query.push(c);
        self.refilter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.refilter();
    }
}

#[cfg(test)]
#[path = "overlay_tests.rs"]
mod tests;
