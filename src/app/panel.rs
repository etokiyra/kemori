//! The bottom panel: problems, task output and the debug console.
//!
//! One surface below the editor keeps the feedback of the development loop —
//! diagnostics, command output and adapter messages — in one predictable place
//! instead of one-off popups. Each tab keeps its own state so switching tabs
//! never loses a scroll position or a selection.
//!
//! The panel never paints an opaque background and never takes focus on its
//! own: an event that produces output may open it, but the user decides when
//! the panel receives the keyboard.

use super::*;

/// The smallest useful panel: rounded border plus two content rows.
pub const PANEL_MIN_HEIGHT: u16 = 4;
/// The largest the panel may grow, so the editor always keeps usable space.
pub const PANEL_MAX_HEIGHT: u16 = 30;
/// The default height when the panel first opens.
const PANEL_DEFAULT_HEIGHT: u16 = 10;
/// One resize step, in rows.
const PANEL_RESIZE_STEP: u16 = 2;
/// A page of output or problems.
const PANEL_PAGE: i64 = 10;

/// One tab of the bottom panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelTab {
    /// Diagnostics across the open files.
    #[default]
    Problems,
    /// The captured output of the last run task.
    Output,
    /// An interactive shell rooted at the workspace.
    Terminal,
    /// Messages from the debug-adapter preview.
    DebugConsole,
}

impl PanelTab {
    /// Every tab, in display order.
    pub const ALL: [PanelTab; 4] = [
        PanelTab::Problems,
        PanelTab::Output,
        PanelTab::Terminal,
        PanelTab::DebugConsole,
    ];

    /// The label shown in the panel's tab strip.
    pub fn label(self) -> &'static str {
        match self {
            PanelTab::Problems => "Problems",
            PanelTab::Output => "Output",
            PanelTab::Terminal => "Terminal",
            PanelTab::DebugConsole => "Debug Console",
        }
    }

    pub(super) fn next(self) -> Self {
        match self {
            PanelTab::Problems => PanelTab::Output,
            PanelTab::Output => PanelTab::Terminal,
            PanelTab::Terminal => PanelTab::DebugConsole,
            PanelTab::DebugConsole => PanelTab::Problems,
        }
    }

    pub(super) fn previous(self) -> Self {
        match self {
            PanelTab::Problems => PanelTab::DebugConsole,
            PanelTab::Output => PanelTab::Problems,
            PanelTab::Terminal => PanelTab::Output,
            PanelTab::DebugConsole => PanelTab::Terminal,
        }
    }
}

/// One diagnostic surfaced in the Problems tab.
#[derive(Clone, Debug)]
pub struct ProblemEntry {
    pub path: PathBuf,
    /// The file name shown in the group header.
    pub file: String,
    /// Zero-based character position inside the file.
    pub position: Position,
    pub severity: Severity,
    pub message: String,
}

/// State shared by every tab, plus the per-tab state.
pub struct Panel {
    pub open: bool,
    pub tab: PanelTab,
    /// The requested height in rows, including the border.
    pub height: u16,
    /// The selected problem, counted across every open file.
    pub problem_selected: usize,
    /// The captured output of the last task.
    pub output: Option<TaskRunState>,
    /// Messages from the debug-adapter preview, oldest first.
    pub debug: Vec<String>,
    /// The selected debug-console message; the view scrolls to keep it visible.
    pub debug_selected: usize,
    /// Problems extracted from the last task's output.
    pub task_problems: Vec<ProblemEntry>,
    /// Built-in diagnostics from the last workspace scan.
    pub workspace_problems: Vec<ProblemEntry>,
    /// Pull-diagnostics reports from language servers, keyed by language.
    pub lsp_workspace_problems: HashMap<LanguageId, Vec<ProblemEntry>>,
    /// Whether a workspace scan has produced a result for this workspace.
    pub problems_scanned: bool,
    /// Completed task outputs from this session, oldest first.
    pub history: Vec<TaskRunState>,
}

impl Default for Panel {
    fn default() -> Self {
        Panel {
            open: false,
            tab: PanelTab::default(),
            height: PANEL_DEFAULT_HEIGHT,
            problem_selected: 0,
            output: None,
            debug: Vec::new(),
            debug_selected: 0,
            task_problems: Vec::new(),
            workspace_problems: Vec::new(),
            lsp_workspace_problems: HashMap::new(),
            problems_scanned: false,
            history: Vec::new(),
        }
    }
}

impl Panel {
    /// Grow by one step, bounded.
    pub fn grow(&mut self) {
        self.height = (self.height + PANEL_RESIZE_STEP).min(PANEL_MAX_HEIGHT);
    }

    /// Shrink by one step, bounded.
    pub fn shrink(&mut self) {
        self.height = self
            .height
            .saturating_sub(PANEL_RESIZE_STEP)
            .max(PANEL_MIN_HEIGHT);
    }
}

impl App {
    /// Dispatch a panel command from the registry; unknown ids are ignored.
    pub(super) fn panel_command(&mut self, id: &str) {
        match id {
            ids::TOGGLE_PANEL => self.toggle_panel(),
            ids::PANEL_PROBLEMS => self.show_panel(PanelTab::Problems),
            ids::PANEL_OUTPUT => self.show_panel(PanelTab::Output),
            ids::PANEL_DEBUG_CONSOLE => self.show_panel(PanelTab::DebugConsole),
            ids::PANEL_CLEAR => self.clear_panel(),
            ids::PANEL_GROW => self.panel.grow(),
            ids::PANEL_SHRINK => self.panel.shrink(),
            _ => {}
        }
    }

    /// Show or hide the panel, and move keyboard focus in or out of it.
    ///
    /// A closed panel opens *focused* so the shortcut immediately becomes
    /// useful; a focused panel closes; an open but unfocused panel takes focus
    /// without stealing it from a dialog first (the caller checks overlays).
    pub(super) fn toggle_panel(&mut self) {
        if !self.panel.open {
            self.panel.open = true;
            self.focus = Focus::Panel;
        } else if self.focus == Focus::Panel {
            self.panel.open = false;
            self.focus = Focus::Editor;
        } else {
            self.focus = Focus::Panel;
        }
    }

    /// Open a specific tab, leaving keyboard focus alone.
    pub(super) fn open_panel(&mut self, tab: PanelTab) {
        self.panel.open = true;
        self.switch_panel_tab(tab);
    }

    /// Select a tab, running the one-time workspace scan when Problems is first
    /// shown in this workspace.
    pub(super) fn switch_panel_tab(&mut self, tab: PanelTab) {
        self.panel.tab = tab;
        if tab == PanelTab::Problems && !self.panel.problems_scanned {
            self.refresh_project_problems();
        }
    }

    /// Ask the worker for the built-in diagnostics of unopened project files.
    pub(super) fn refresh_project_problems(&mut self) {
        if self.pending_project_diagnostics.is_some() {
            return;
        }
        self.project_diagnostics_seq += 1;
        let revision = self.project_diagnostics_seq;
        self.pending_project_diagnostics = Some(revision);
        self.panel.problems_scanned = true;
        let skip = self
            .editor
            .documents
            .iter()
            .filter_map(|doc| doc.buffer.path.clone())
            .collect();
        self.background
            .project_diagnostics(self.workspace.root().to_path_buf(), skip, revision);
        self.panel.lsp_workspace_problems.clear();
        self.request_workspace_lsp_diagnostics();
        self.set_status("Scanning project problems…");
    }

    /// Apply the result of a workspace diagnostics scan.
    pub(super) fn apply_project_diagnostics(
        &mut self,
        revision: u64,
        problems: Vec<crate::language::diagnostics::FileDiagnostics>,
    ) -> bool {
        if self.pending_project_diagnostics != Some(revision) {
            return false;
        }
        self.pending_project_diagnostics = None;
        let root = self.workspace.root().to_path_buf();
        let mut entries = Vec::new();
        for file in problems {
            let display = project_relative(&root, &file.path);
            for diagnostic in file.diagnostics {
                entries.push(ProblemEntry {
                    path: file.path.clone(),
                    file: display.clone(),
                    position: Position::new(diagnostic.start.line, diagnostic.start.col),
                    severity: diagnostic.severity,
                    message: diagnostic.message,
                });
            }
        }
        let count = entries.len();
        self.panel.workspace_problems = entries;
        self.panel.problems_scanned = true;
        if count > 0 {
            self.set_status(format!("{count} project problem(s)"));
        } else {
            self.set_status("No project problems found");
        }
        true
    }

    /// Open a specific tab and move keyboard focus into the panel.
    pub(super) fn show_panel(&mut self, tab: PanelTab) {
        self.open_panel(tab);
        self.focus = Focus::Panel;
    }

    /// Every diagnostic in every open file, in tab order.
    ///
    /// Derived on demand so it can never go stale: the panel is a view of the
    /// documents, not a second diagnostics store. Unsaved buffers are skipped
    /// because they have no path to jump back to.
    pub fn panel_problems(&self) -> Vec<ProblemEntry> {
        let root = self.workspace.root();
        let mut problems = Vec::new();
        for doc in &self.editor.documents {
            let Some(path) = doc.buffer.path.clone() else {
                continue;
            };
            let file = project_relative(root, &path);
            for diagnostic in doc.diagnostics() {
                problems.push(ProblemEntry {
                    path: path.clone(),
                    file: file.clone(),
                    position: Position::new(diagnostic.start.line, diagnostic.start.col),
                    severity: diagnostic.severity,
                    message: diagnostic.message.clone(),
                });
            }
        }
        problems.extend(self.panel.task_problems.iter().cloned());
        problems.extend(self.panel.workspace_problems.iter().cloned());
        for entries in self.panel.lsp_workspace_problems.values() {
            problems.extend(entries.iter().cloned());
        }
        // The same problem can arrive from a live document and a workspace
        // scan; keep one copy so the count is honest.
        let mut seen = std::collections::HashSet::new();
        problems.retain(|problem| {
            seen.insert((
                problem.path.clone(),
                problem.position.row,
                problem.position.col,
                problem.message.clone(),
            ))
        });
        // Group by file, then position, so the panel never repeats a header.
        problems.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.position.row.cmp(&b.position.row))
                .then_with(|| a.position.col.cmp(&b.position.col))
        });
        problems
    }

    /// Handle a key while the panel has focus.
    pub(super) fn handle_panel_key(&mut self, key: KeyEvent) {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if alt {
            match key.code {
                KeyCode::Up => self.panel.grow(),
                KeyCode::Down => self.panel.shrink(),
                _ => {}
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            // The global handler owned the chord (Ctrl+J toggles the panel).
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.focus = Focus::Editor,
            KeyCode::Tab => self.switch_panel_tab(self.panel.tab.next()),
            KeyCode::BackTab => self.switch_panel_tab(self.panel.tab.previous()),
            KeyCode::Char('1') => self.switch_panel_tab(PanelTab::Problems),
            KeyCode::Char('2') => self.switch_panel_tab(PanelTab::Output),
            KeyCode::Char('3') => self.switch_panel_tab(PanelTab::Terminal),
            KeyCode::Char('4') => self.switch_panel_tab(PanelTab::DebugConsole),
            KeyCode::Up => self.move_panel_selection(-1),
            KeyCode::Down => self.move_panel_selection(1),
            KeyCode::PageUp => self.move_panel_selection(-PANEL_PAGE),
            KeyCode::PageDown => self.move_panel_selection(PANEL_PAGE),
            KeyCode::Home => self.panel_edge(false),
            KeyCode::End => self.panel_edge(true),
            KeyCode::Enter => self.activate_panel_selection(),
            KeyCode::Char('c') => self.clear_panel(),
            KeyCode::Char('x') if self.panel.tab == PanelTab::Output => self.cancel_task(),
            KeyCode::Char('r') if self.panel.tab == PanelTab::Output => self.rerun_task(),
            KeyCode::Char('r') if self.panel.tab == PanelTab::Problems => {
                self.refresh_project_problems()
            }
            KeyCode::Char('h') if self.panel.tab == PanelTab::Output => self.open_task_history(),
            _ => {}
        }
    }

    /// Move the tab's cursor or scroll offset by `delta`.
    fn move_panel_selection(&mut self, delta: i64) {
        match self.panel.tab {
            PanelTab::Problems => {
                let count = self.panel_problems().len();
                if count == 0 {
                    self.panel.problem_selected = 0;
                } else {
                    let next = self.panel.problem_selected as i64 + delta;
                    self.panel.problem_selected = next.clamp(0, count as i64 - 1) as usize;
                }
            }
            PanelTab::Output => {
                if let Some(run) = self.panel.output.as_mut() {
                    run.select_by(delta);
                }
            }
            PanelTab::Terminal => {
                // Dragging through the scrollback mirrors the arrow keys.
                self.terminals.scroll_active(-delta as isize);
            }
            PanelTab::DebugConsole => {
                let last = self.panel.debug.len().saturating_sub(1);
                let next = self.panel.debug_selected as i64 + delta;
                self.panel.debug_selected = next.clamp(0, last as i64) as usize;
            }
        }
    }

    /// Jump to the first or last row of the current tab.
    fn panel_edge(&mut self, end: bool) {
        match self.panel.tab {
            PanelTab::Problems => {
                let count = self.panel_problems().len();
                self.panel.problem_selected = if end { count.saturating_sub(1) } else { 0 };
            }
            PanelTab::Output => {
                if let Some(run) = self.panel.output.as_mut() {
                    run.select_by(if end { i64::MAX } else { i64::MIN });
                }
            }
            PanelTab::Terminal => {
                if end {
                    self.terminals.scroll_active_to_bottom();
                } else {
                    self.terminals.scroll_active(isize::MAX / 4);
                }
            }
            PanelTab::DebugConsole => {
                self.panel.debug_selected = if end {
                    self.panel.debug.len().saturating_sub(1)
                } else {
                    0
                };
            }
        }
    }

    /// Open the file behind the selected problem or output line.
    fn activate_panel_selection(&mut self) {
        match self.panel.tab {
            PanelTab::Problems => {
                let target = self
                    .panel_problems()
                    .get(self.panel.problem_selected)
                    .map(|problem| (problem.path.clone(), problem.position));
                if let Some((path, position)) = target {
                    self.focus = Focus::Editor;
                    self.reveal(path, position);
                }
            }
            PanelTab::Output => {
                let target = self.panel.output.as_ref().and_then(|run| run.jump_target());
                match target {
                    Some((path, position)) => {
                        self.focus = Focus::Editor;
                        self.reveal(path, position);
                    }
                    None => self.set_status("No file reference on the selected line"),
                }
            }
            PanelTab::Terminal => {}
            PanelTab::DebugConsole => {}
        }
    }

    /// Clear the current tab's content where that is meaningful.
    ///
    /// Document diagnostics are live data and cannot be cleared; a task's
    /// extracted problems can be, while the document problems stay visible.
    pub(super) fn clear_panel(&mut self) {
        match self.panel.tab {
            PanelTab::Problems => {
                let had_task = !self.panel.task_problems.is_empty();
                let had_workspace = !self.panel.workspace_problems.is_empty();
                let had_lsp = !self.panel.lsp_workspace_problems.is_empty();
                self.panel.task_problems.clear();
                self.panel.workspace_problems.clear();
                self.panel.lsp_workspace_problems.clear();
                if had_task || had_workspace || had_lsp {
                    self.set_status("Task and project problems cleared");
                } else {
                    self.set_status(
                        "Problems follow the open files; close or fix a file to clear it",
                    );
                }
            }
            PanelTab::Output => {
                self.panel.output = None;
                self.set_status("Output cleared");
            }
            PanelTab::Terminal => {
                // Ctrl+L is the shell's own clear-screen command.
                self.terminals.write_active(b"\x0c");
                self.set_status("Sent clear to the terminal");
            }
            PanelTab::DebugConsole => {
                self.panel.debug.clear();
                self.panel.debug_selected = 0;
                self.set_status("Debug console cleared");
            }
        }
    }

    /// Append a debug-adapter message and show the console tab.
    pub(super) fn debug_console(&mut self, message: impl Into<String>) {
        self.panel.debug.push(message.into());
        // Bound the console so a talkative adapter cannot grow it forever.
        if self.panel.debug.len() > 500 {
            let drop = self.panel.debug.len() - 500;
            self.panel.debug.drain(..drop);
            self.panel.debug_selected = self.panel.debug_selected.saturating_sub(drop);
        }
        self.panel.debug_selected = self.panel.debug.len().saturating_sub(1);
        self.open_panel(PanelTab::DebugConsole);
    }
}

/// A path relative to the project root, for display.
fn project_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Extract `file:line[:col]` problems from a task's output.
///
/// Compiler output is not a protocol, so this is deliberately conservative:
/// only the first whitespace-delimited token of a line is considered, the file
/// must exist, and duplicates at the same position are dropped. The whole line
/// keeps its context as the message.
pub(super) fn task_problems(root: &Path, text: &str) -> Vec<ProblemEntry> {
    let mut problems: Vec<ProblemEntry> = Vec::new();
    for line in text.lines() {
        if problems.len() >= 200 {
            break;
        }
        let Some((path, row, col)) = reference_position(line) else {
            continue;
        };
        let path = PathBuf::from(path);
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        if !path.is_file() {
            continue;
        }
        let position = Position::new(row.saturating_sub(1), col.unwrap_or(1).saturating_sub(1));
        if problems
            .iter()
            .any(|problem| problem.path == path && problem.position == position)
        {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        let severity = if lower.contains("error") {
            Severity::Error
        } else if lower.contains("warning") {
            Severity::Warning
        } else {
            Severity::Info
        };
        let message = line
            .trim()
            .split_once(' ')
            .map(|(_, rest)| rest.trim())
            .filter(|rest| !rest.is_empty())
            .unwrap_or_else(|| line.trim())
            .to_string();
        problems.push(ProblemEntry {
            file: project_relative(root, &path),
            path,
            position,
            severity,
            message,
        });
    }
    problems
}

/// The captured output of a run task: plain text with a known exit result.
///
/// Output arrives in one bounded batch from the background worker, so the view
/// is a read-only panel — the task is abstracted by Kemori, not streamed live.
/// `Up`/`Down` move a row cursor instead of scrolling freely, because the row
/// the user is on is what `Enter` can turn into a source jump.
#[derive(Clone)]
pub struct TaskRunState {
    pub title: String,
    /// The task's command line, shown in the header.
    pub command: String,
    /// How the task ended: the exit code, or a reason.
    pub outcome: String,
    /// Whether the task exited cleanly; the header is coloured from it.
    pub success: bool,
    /// The project root, so a relative `file:line` reference can be resolved.
    pub root: PathBuf,
    pub lines: Vec<String>,
    pub selected: usize,
    /// Whether new output should keep the cursor on the last line.
    pub follow: bool,
    /// The incomplete last line of a live stream.
    partial: String,
}

impl TaskRunState {
    /// Build state from captured output, with the cursor on the first line.
    pub fn from_output(
        root: &Path,
        title: impl Into<String>,
        command: &str,
        outcome: &str,
        success: bool,
        text: &str,
    ) -> Self {
        let lines: Vec<String> = text
            .lines()
            .map(|line| line.trim_end().to_string())
            .collect();
        TaskRunState {
            title: title.into(),
            command: command.to_string(),
            outcome: outcome.to_string(),
            success,
            root: root.to_path_buf(),
            lines,
            selected: 0,
            follow: true,
            partial: String::new(),
        }
    }

    /// Append a live chunk, keeping the incomplete final line for the next one.
    pub fn append(&mut self, text: &str) {
        self.partial.push_str(text);
        if let Some(index) = self.partial.rfind('\n') {
            let complete: String = self.partial.drain(..=index).collect();
            self.lines.extend(
                complete
                    .lines()
                    .map(|line| line.trim_end_matches('\r').to_string()),
            );
        }
        if self.follow {
            self.selected = self.lines.len().saturating_sub(1);
        }
    }

    /// Move the row cursor by `delta` lines, clamped to the output.
    pub fn select_by(&mut self, delta: i64) {
        if self.lines.is_empty() {
            self.selected = 0;
        } else {
            let next = (self.selected as i64).saturating_add(delta);
            self.selected = next.clamp(0, self.lines.len() as i64 - 1) as usize;
        }
        self.follow = self.selected + 1 >= self.lines.len();
    }

    /// The file and position the selected line references, resolved against the
    /// project root (an absolute reference wins), e.g. `src/main.rs:5:9`.
    pub fn jump_target(&self) -> Option<(PathBuf, Position)> {
        let line = self.lines.get(self.selected)?;
        let (path, row, col) = reference_position(line)?;
        let path = PathBuf::from(&path);
        let path = if path.is_absolute() {
            path
        } else {
            self.root.join(path)
        };
        // A compiler points at 1-based lines and 1-based columns; Kemori stores
        // 0-based characters.
        let column = col.as_ref().map_or(0, |col| col.saturating_sub(1));
        let row = row.saturating_sub(1);
        Some((path, Position::new(row, column)))
    }
}

/// A `path:line[:column]` reference in a task output line.
///
/// Only the first whitespace-delimited token is considered, which keeps rustc,
/// go vet and pytest output (most of which start the problem line this way)
/// parseable without regular-expression machinery. The line position comes
/// *before* the column, so `src/main.rs:5:9` is line 5, column 9. The path must
/// contain a separator or an extension dot, so `make:13` or `exit code 1` is
/// not mistaken for a file.
pub(super) fn reference_position(line: &str) -> Option<(String, usize, Option<usize>)> {
    let token = line.split_whitespace().next()?;
    // rustc ends the reference with a colon before the message, and a dangling
    // empty segment would stop the numeric drain below.
    let token = token.trim_end_matches(':');
    let mut segments: Vec<&str> = token.split(':').collect();
    // Drain the trailing numeric segments: they are the column, then the row.
    let mut numbers: Vec<usize> = Vec::new();
    while let Some(last) = segments.last() {
        match last.parse::<usize>() {
            Ok(number) => {
                numbers.push(number);
                segments.pop();
            }
            Err(_) => break,
        }
    }
    let (row, column) = match numbers.as_slice() {
        [row] => (*row, None),
        [col, row, ..] => (*row, Some(*col)),
        _ => return None,
    };
    if segments.is_empty() {
        return None;
    }
    let path = segments.join(":");
    if !path.contains('/') && !path.contains('.') {
        return None;
    }
    Some((path, row, column))
}
