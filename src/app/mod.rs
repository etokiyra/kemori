//! Application state and the event loop.
//!
//! This module is the conductor: it wires workspace, editor, language service,
//! commands and UI together, and translates terminal events into actions.

pub mod overlay;
pub mod panel;

pub use settings::{SETTINGS_ROWS, SettingsRow, SettingsState};
pub use status::{Status, Toast, ToastKind};

mod availability;
mod background;
mod commands;
mod completion;
pub mod debug;
mod diagnostics;
mod editor;
mod files;
mod git;
mod java_debug;
mod keys;
mod language;
mod lsp;
mod lsp_decorations;
mod lsp_documents;
mod lsp_highlights;
mod lsp_pending;
mod lsp_progress;
mod lsp_response;
mod mouse;
mod panes;
pub mod project_search;
mod run;
mod search;
mod session;
mod settings;
mod setup;
mod snippets;
mod status;
mod tasks;
mod terminal_panel;
mod trust;
mod view;

use std::cmp::Reverse;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use serde_json::Value;

use crate::background::{Background, Event as BackgroundEvent};
use crate::commands::{Command, CommandRegistry, ids};
use crate::editor::{Document, Editor, Position, Selection};
use crate::filesystem;
use crate::git::GitFileStatus;
use crate::language::completion::{Completion, CompletionKind};
use crate::language::diagnostics::{Diagnostic, Severity};
use crate::language::format;
use crate::language::format::FormatOutcome;
use crate::language::lsp::{PositionEncoding, RequestKind, Server, ServerEvent, convert};
use crate::language::provider::TokenKind;
use crate::language::symbols::is_ident_char as is_word_char;
use crate::language::tools::{Tool, ToolPurpose, ToolRegistry};
use crate::language::{Capability, LanguageId, LanguageService, WorkspaceSymbol};
use crate::project::Workspace;
use crate::project::create::{self, CreateOutcome};
use crate::project::tasks::Task;
use crate::recent::Recent;
use crate::session::Session;
use crate::terminal;
use crate::terminal::session::Terminals;
use crate::trust::TrustStore;
use crate::ui;
use overlay::{
    CompletionState, DiffState, DirPicker, Help, HoverState, NewProject, NewProjectStep, Overlay,
    Picker, PickerAction, PickerItem, Prompt, PromptKind, ReplacePreview, ReplacePreviewRow,
    Search, SearchField, SignatureState, TreeFilter,
};

/// How long typing must pause before diagnostics are recomputed. Short enough to
/// feel immediate, long enough not to reanalyse on every keystroke.
const DIAGNOSTICS_DEBOUNCE: Duration = Duration::from_millis(150);

/// How long typing must pause before Kemori offers completions automatically.
/// Short enough to feel instant, long enough that a burst of typing does not
/// build a candidate pool for every intermediate prefix.
const AUTOCOMPLETE_DELAY: Duration = Duration::from_millis(120);

/// Which surface receives keyboard input when no overlay is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Editor,
    FileTree,
    /// The bottom panel owns the keyboard (Problems, Output, Debug Console).
    Panel,
}

/// Which editor pane owns the cursor when the editor is split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Primary,
    Secondary,
}

/// How long to wait after opening a file before starting a language server.
///
/// Keeping this off the critical path means opening a file is instant and a
/// server is never spawned for the brief use of a throwaway process.
const LSP_START_DELAY: Duration = Duration::from_millis(600);

/// If a server does not finish its handshake within this window, Kemori gives up,
/// falls back to its built-in providers and (bounded by [`MAX_LSP_RESTARTS`])
/// tries once more.
const LSP_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(25);

/// How long to wait before an automatic restart after a server failure.
const LSP_RESTART_DELAY: Duration = Duration::from_secs(2);

/// How many automatic restarts Kemori attempts before settling on the built-in
/// providers. The counter resets whenever a server connects successfully.
const MAX_LSP_RESTARTS: u32 = 2;

/// How long a server must stay ready before a later crash earns a fresh restart
/// budget. Without this, a server that initializes and then crashes on real
/// work would reset its budget on every handshake and restart forever.
const LSP_STABLE_UPTIME: Duration = Duration::from_secs(60);

/// The state of the language-server connection, for the statusline and setup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LspStatus {
    /// No server is running (and none is expected).
    #[default]
    Offline,
    /// A server was started and the handshake is in progress.
    Starting,
    /// The server is initialized and serving features.
    Ready,
    /// A server could not be started or exited.
    Failed(String),
}

/// How to launch a language server: the program, its arguments and any extra
/// environment (used to point managed servers at Kemori-provisioned runtimes).
struct LspLaunch {
    program: String,
    args: &'static [&'static str],
    env: Vec<(String, String)>,
}

/// One language's language-server job.
///
/// Kemori keeps a server per language rather than one per session, so a workspace
/// that mixes languages — or a split pane showing two — gets tooling for each
/// instead of only the first language it happened to detect.
#[derive(Default)]
struct LspJob {
    /// The running server, once a scheduled start has fired.
    server: Option<Server>,
    /// A scheduled start that has not fired yet.
    start_at: Option<Instant>,
    /// When the current handshake started, for the timeout.
    started_at: Option<Instant>,
    /// Automatic restarts attempted since the server last connected.
    restarts: u32,
    /// When the server last became ready, for the stable-uptime budget reset.
    ready_at: Option<Instant>,
    /// The most recent failure, for the status summary.
    failed: Option<String>,
}

/// The region produced by the last paste, used by yank-pop.
#[derive(Clone, Copy)]
struct Yank {
    doc: usize,
    start: Position,
    end: Position,
    /// Buffer version at the time of the paste; any later edit invalidates it.
    version: u64,
}

/// A scheduled automatic completion, pinned to the document state it was
/// requested for so a stray timer cannot pop a popup after the user moved on.
struct PendingCompletion {
    due_at: Instant,
    doc: usize,
    cursor: Position,
    version: u64,
}

/// The remaining tab stops of an inserted snippet.
struct SnippetState {
    doc: usize,
    stops: Vec<Position>,
    index: usize,
}

/// A document-sensitive language-server request.
///
/// The response may only be applied if it is still the newest request for its
/// kind and the document has not changed since it was issued, so a slow server
/// cannot edit or move the cursor over newer text.
#[derive(Clone)]
struct PendingDocRequest {
    language: LanguageId,
    id: i64,
    path: PathBuf,
    version: u64,
}

impl PendingDocRequest {
    /// Whether `(language, id)` is still this request and the document at
    /// `path` is still open with the same buffer version.
    fn is_current(&self, app: &App, language: LanguageId, id: i64) -> bool {
        if self.language != language || self.id != id {
            return false;
        }
        app.editor
            .documents
            .iter()
            .find(|doc| same_file(doc.buffer.path.as_deref(), &self.path))
            .is_some_and(|doc| doc.buffer.version == self.version)
    }
}

/// Which hierarchy the user asked for after preparing a symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HierarchyGoal {
    Incoming,
    Outgoing,
    Super,
    Sub,
}

/// An action offered on the welcome screen.
pub enum WelcomeAction {
    OpenFile,
    OpenProject,
    NewProject,
    Resume,
    OpenTarget(PathBuf),
    OpenRecentProject(PathBuf),
    OpenRecentFile(PathBuf),
    Help,
}

/// A row on the welcome screen.
pub struct WelcomeItem {
    pub label: String,
    pub detail: String,
    pub action: WelcomeAction,
}

/// The root application object.
pub struct App {
    pub workspace: Workspace,
    /// Additional projects opened this session, for multi-project navigation.
    pub workspaces: Vec<Workspace>,
    pub editor: Editor,
    pub language: Arc<LanguageService>,
    background: Background,
    pub commands: CommandRegistry,
    pub overlay: Overlay,
    pub search: Search,
    /// Active `$/progress` reports, keyed by the server's token.
    pub lsp_progress: std::collections::BTreeMap<String, crate::language::lsp::LspProgress>,
    pub clipboard: String,
    /// Recent kills, most recent last, for yank-pop.
    kill_ring: Vec<String>,
    /// Index of the entry currently yanked, within the ring.
    kill_index: usize,
    /// The region produced by the last paste, so `Alt+Y` can replace it.
    last_yank: Option<Yank>,
    pub recent_files: Vec<PathBuf>,
    /// Global recent projects and files, shown on the welcome screen.
    pub recent: Recent,
    /// The path supplied on the command line, offered from the welcome screen.
    welcome_target: Option<PathBuf>,
    /// A session loaded for the current workspace, offered as "Resume".
    resume_session: Option<Session>,
    /// Selected row on the welcome screen.
    pub welcome_selected: usize,
    /// Whether the user engaged with a project this session. Gates session
    /// saving so starting Kemori and quitting does not wipe an untouched session.
    engaged: bool,
    /// Whether a project creation is running on the worker.
    pending_project: bool,
    /// Inline file-tree filter, when active.
    pub tree_filter: Option<TreeFilter>,
    /// Completion popup, when open.
    pub completion: Option<CompletionState>,
    /// Active snippet tab-stop navigation, when a snippet was inserted.
    snippet: Option<SnippetState>,
    /// When a paused keystroke should offer automatic completion.
    completion_due: Option<PendingCompletion>,
    /// The id of the newest language-server completion request (with its
    /// language), so an older response can be discarded when the user has typed
    /// on.
    completion_request: Option<(LanguageId, i64)>,
    /// Hover popup, when open.
    pub hover: Option<HoverState>,
    /// Signature-help popup, when open.
    pub signature: Option<SignatureState>,
    /// The id of the newest signature-help request, so an older response can be
    /// discarded when the cursor has moved on.
    signature_request: Option<(LanguageId, i64)>,
    /// The id of the newest hover request, so a late answer cannot replace a
    /// newer one.
    hover_request: Option<(LanguageId, i64)>,
    /// An in-flight completion-item resolve request.
    pending_completion_resolve: Option<(LanguageId, i64, String)>,
    /// Screen position of the editor cursor, updated during rendering.
    pub cursor_screen: Option<(u16, u16)>,
    pub tree_visible: bool,
    /// Whether diagnostic messages are shown at the end of their line.
    pub inline_diagnostics: bool,
    /// Whether long lines soft-wrap instead of scrolling horizontally.
    pub wrap: bool,
    /// Whether the editor shows two panes side by side.
    pub split: bool,
    /// The document shown in the left (primary) pane.
    pane_left: usize,
    /// The document shown in the right pane, when split.
    pane_right: Option<usize>,
    /// Which pane receives editing.
    pub focus_pane: Pane,
    pub focus: Focus,
    /// The bottom panel and its per-tab state.
    pub panel: panel::Panel,
    /// PTY-backed terminal sessions shown in the panel.
    pub terminals: Terminals,
    /// An optional shell override for a terminal session. `None` uses `$SHELL`.
    pub terminal_shell: Option<String>,
    /// The debugger session and the stack/variables it reported.
    pub debug: debug::DebugState,
    /// A pending jdtls `workspace/executeCommand` in the Java debug pipeline.
    pending_java_debug: Option<debug::JavaDebugPending>,
    /// Project roots the user has trusted for running code.
    pub trust: TrustStore,
    /// The action waiting for a workspace-trust answer.
    pending_trust: Option<trust::TrustIntent>,
    pub status: Status,
    /// Transient notifications shown above the statusline.
    pub toasts: Vec<Toast>,
    /// Height of the editor viewport, updated during rendering.
    pub viewport_height: usize,
    pub should_quit: bool,
    quit_armed: bool,
    /// Whether `z` was pressed and the next editor key selects a fold action.
    fold_prefix: bool,
    /// Tab index armed for a forced close (dirty, first Ctrl+W).
    close_armed: Option<usize>,
    /// Monotonic id for diagnostics requests.
    diagnostics_seq: u64,
    /// When set, diagnostics should be recomputed once typing pauses.
    diagnostics_dirty_at: Option<Instant>,
    /// Monotonic id for format requests.
    format_seq: u64,
    /// The format request awaiting a result, if any.
    pending_format: Option<(PathBuf, u64)>,
    /// A file saved while format-on-save is waiting to write the result.
    format_save_pending: Option<PathBuf>,
    /// A language-server formatting request awaiting a result, with the buffer
    /// version it was computed against.
    pending_lsp_format: Option<(PathBuf, LanguageId, i64, u64)>,
    /// An in-flight folding-range request.
    pending_folding: Option<PendingDocRequest>,
    /// An in-flight document-highlight request.
    pending_highlights: Option<PendingDocRequest>,
    /// An in-flight inlay-hint request.
    pending_inlay_hints: Option<PendingDocRequest>,
    /// An in-flight code-lens request.
    pending_code_lens: Option<PendingDocRequest>,
    /// An in-flight selection-range request and the cursor it was for.
    pending_selection_ranges: Option<(PendingDocRequest, Position)>,
    /// An in-flight hierarchy prepare request and what to do with the result.
    pending_hierarchy_prepare: Option<(PendingDocRequest, HierarchyGoal)>,
    /// An in-flight incoming/outgoing/supertype/subtype request.
    pending_hierarchy: Option<PendingDocRequest>,
    /// Prepared hierarchy nodes awaiting the user's choice.
    hierarchy_items: Vec<crate::language::lsp::hierarchy::HierarchyItem>,
    /// Which follow-up the prepared nodes should use.
    hierarchy_goal: Option<HierarchyGoal>,
    /// An in-flight document-link request.
    pending_document_links: Option<PendingDocRequest>,
    /// An in-flight organize-imports request.
    pending_organize_imports: Option<PendingDocRequest>,
    /// An in-flight range-formatting request.
    pending_range_format: Option<PendingDocRequest>,
    /// An in-flight definition request.
    pending_definition: Option<PendingDocRequest>,
    /// An in-flight references request.
    pending_references: Option<PendingDocRequest>,
    /// An in-flight rename request.
    pending_rename_request: Option<PendingDocRequest>,
    /// An in-flight code-action request.
    pending_code_action_request: Option<PendingDocRequest>,
    /// The document and version the offered code actions were computed against.
    pending_code_actions_context: Option<(PathBuf, u64)>,
    /// Monotonic id for workspace symbol scans.
    workspace_symbols_seq: u64,
    /// The workspace symbol scan awaiting a result, if any.
    pending_workspace_symbols: Option<u64>,
    /// A query to apply when the next workspace-symbol picker opens.
    pending_workspace_symbols_query: Option<String>,
    /// Whether a language server workspace-symbol request is in flight.
    ws_lsp_pending: bool,
    /// Monotonic id for project text searches.
    project_search_seq: u64,
    /// The project search awaiting a result, if any.
    pending_project_search: Option<u64>,
    /// Monotonic id for project replacements.
    project_replace_seq: u64,
    /// The project replacement awaiting a result, if any.
    pending_project_replace: Option<u64>,
    /// Monotonic id for built-in workspace diagnostics.
    project_diagnostics_seq: u64,
    /// The workspace diagnostics scan awaiting a result, if any.
    pending_project_diagnostics: Option<u64>,
    /// In-flight document pull-diagnostics requests, keyed by server and id.
    pending_document_diagnostics: HashMap<(LanguageId, i64), (PathBuf, u64)>,
    /// In-flight workspace pull-diagnostics requests, keyed by server and id.
    pending_workspace_diagnostics: HashMap<(LanguageId, i64), ()>,
    /// Animation frame, advanced while something on screen animates.
    pub anim_phase: usize,
    /// When the animation frame last advanced.
    anim_last: Instant,
    /// Whether the welcome scene and busy indicators animate. Users can turn
    /// motion off from the palette.
    pub motion: bool,
    /// The active welcome scene.
    pub welcome_scene: crate::ui::art::WelcomeScene,
    /// External tools Kemori has probed for, once discovery completes.
    pub tools: Option<ToolRegistry>,
    /// One language server per language.
    lsp: HashMap<(PathBuf, LanguageId), LspJob>,
    /// The symbol awaiting a new name, from the rename prompt.
    pending_rename: Option<(PathBuf, usize, usize)>,
    /// The path awaiting a new name, from the file-rename prompt.
    pending_rename_file: Option<PathBuf>,
    /// The path awaiting a destination, from the file-copy prompt.
    pending_copy_file: Option<PathBuf>,
    /// A tool install in progress, if any.
    pending_install: Option<Tool>,
    /// Whether a git commit is running on the worker.
    pending_commit: bool,
    /// A push, pull, fetch or GitHub action running on the worker.
    pending_git_op: Option<&'static str>,
    /// The diff request awaiting a result, if any.
    pending_git_diff: Option<(PathBuf, Option<bool>)>,
    /// Monotonic id for gutter-marker refreshes.
    git_markers_revision: u64,
    /// Whether a background run task is in flight, guarding a second run.
    pending_task: bool,
    /// The last task started, so it can be rerun.
    last_task: Option<Task>,
    /// Whether a stage/unstage is running and the changed-files picker should
    /// be rebuilt when the refreshed snapshot arrives.
    pending_stage_refresh: bool,
    /// Code actions from the most recent server response.
    pending_code_actions: Vec<convert::CodeAction>,
    /// When open files were last checked for on-disk changes.
    last_disk_check: Instant,
    /// Last rendered editor body rectangle, for mouse hit-testing.
    pub editor_body_rect: Option<ratatui::layout::Rect>,
    /// Anchor for an in-progress mouse drag selection.
    pub mouse_drag_anchor: Option<Position>,
    /// Languages for which Kemori has already offered to install a missing
    /// language server this session.
    setup_offered: std::collections::HashSet<LanguageId>,
    /// The languages detected in the current project, once scanned.
    project_languages: Option<Vec<LanguageId>>,
    /// Whether the project-language scan has been requested for this workspace.
    project_scan_started: bool,
    /// Whether project setup has been offered for this workspace.
    project_setup_offered: bool,
    /// An in-progress "Set up this project" run, if any.
    project_setup: Option<crate::language::setup::ProjectSetupRun>,
    /// The user's global preferences.
    pub settings: crate::settings::Settings,
    /// Whether the preferences changed this session and should be written on
    /// quit. Kemori only writes a settings file once the user changes something.
    settings_dirty: bool,
    /// Whether the editor gutter shows line numbers.
    pub show_line_numbers: bool,
}

impl App {
    /// Run Kemori. `target` is the optional CLI path argument.
    pub fn start(target: Option<String>) -> io::Result<()> {
        let target = target.map(PathBuf::from);
        // Import the pre-rename state once, before settings and sessions load.
        crate::recent::migrate_legacy_state();
        // Colour support is a terminal capability, detected once: everything
        // rendered afterwards consults the recorded depth.
        let mut settings = crate::settings::Settings::load();
        let depth = ui::theme::ColorDepth::detect();
        ui::theme::set_color_depth(depth);
        // The RGB themes cannot render faithfully on a 16-colour terminal, so a
        // user who never chose a theme gets the bundled 16-colour palette. A
        // deliberate choice from the Settings screen is always honoured.
        if depth == ui::theme::ColorDepth::Palette16 && !crate::settings::theme_is_stored() {
            settings.theme = crate::settings::ThemeId::Ansi;
        }
        let mut app = App::new_with_settings(target.as_deref(), settings)?;
        // Trust is a user decision stored in Kemori's state directory; tests
        // build an app with an empty store instead of reading the real one.
        app.trust = TrustStore::load();
        let mut terminal = terminal::init()?;
        let result = app.run(&mut terminal);
        terminal::restore();
        app.save_session();
        app.recent.save();
        app.persist_settings();
        result
    }

    /// Build the application state for a target path, using the default
    /// preferences. Tests use this so they are independent of the user's
    /// settings file; the real CLI loads settings first.
    pub fn new(target: Option<&std::path::Path>) -> io::Result<Self> {
        App::new_with_settings(target, crate::settings::Settings::default())
    }

    /// Build the application state for a target path with explicit preferences.
    pub fn new_with_settings(
        target: Option<&std::path::Path>,
        settings: crate::settings::Settings,
    ) -> io::Result<Self> {
        let language = Arc::new(LanguageService::builtin());
        let background = Background::spawn(Arc::clone(&language));
        let workspace = Workspace::open(target)?;
        let mut app = App {
            workspace,
            workspaces: Vec::new(),
            editor: Editor::new(),
            language,
            background,
            commands: CommandRegistry::builtin(),
            overlay: Overlay::None,
            search: Search::default(),
            lsp_progress: std::collections::BTreeMap::new(),
            clipboard: String::new(),
            kill_ring: Vec::new(),
            kill_index: 0,
            last_yank: None,
            recent_files: Vec::new(),
            recent: Recent::default(),
            welcome_target: None,
            resume_session: None,
            welcome_selected: 0,
            engaged: false,
            pending_project: false,
            tree_filter: None,
            completion: None,
            snippet: None,
            completion_due: None,
            completion_request: None,
            hover: None,
            signature: None,
            signature_request: None,
            hover_request: None,
            pending_completion_resolve: None,
            cursor_screen: None,
            tree_visible: true,
            inline_diagnostics: true,
            wrap: settings.soft_wrap,
            show_line_numbers: settings.line_numbers,
            split: false,
            pane_left: 0,
            pane_right: None,
            focus_pane: Pane::Primary,
            focus: Focus::Editor,
            panel: panel::Panel::default(),
            terminals: Terminals::default(),
            terminal_shell: None,
            debug: debug::DebugState::default(),
            pending_java_debug: None,
            trust: TrustStore::default(),
            pending_trust: None,
            status: Status::default(),
            toasts: Vec::new(),
            viewport_height: 20,
            should_quit: false,
            quit_armed: false,
            fold_prefix: false,
            close_armed: None,
            diagnostics_seq: 0,
            diagnostics_dirty_at: None,
            format_seq: 0,
            pending_format: None,
            format_save_pending: None,
            pending_lsp_format: None,
            pending_folding: None,
            pending_highlights: None,
            pending_inlay_hints: None,
            pending_code_lens: None,
            pending_selection_ranges: None,
            pending_hierarchy_prepare: None,
            pending_hierarchy: None,
            hierarchy_items: Vec::new(),
            hierarchy_goal: None,
            pending_document_links: None,
            pending_organize_imports: None,
            pending_range_format: None,
            pending_definition: None,
            pending_references: None,
            pending_rename_request: None,
            pending_code_action_request: None,
            pending_code_actions_context: None,
            workspace_symbols_seq: 0,
            pending_workspace_symbols: None,
            pending_workspace_symbols_query: None,
            ws_lsp_pending: false,
            project_search_seq: 0,
            pending_project_search: None,
            project_replace_seq: 0,
            pending_project_replace: None,
            project_diagnostics_seq: 0,
            pending_project_diagnostics: None,
            pending_document_diagnostics: HashMap::new(),
            pending_workspace_diagnostics: HashMap::new(),
            anim_phase: 0,
            anim_last: Instant::now(),
            motion: true,
            welcome_scene: crate::ui::art::WelcomeScene::default(),
            tools: None,
            lsp: HashMap::new(),
            pending_rename: None,
            pending_rename_file: None,
            pending_copy_file: None,
            pending_install: None,
            pending_commit: false,
            pending_git_op: None,
            pending_git_diff: None,
            git_markers_revision: 0,
            pending_task: false,
            last_task: None,
            pending_stage_refresh: false,
            pending_code_actions: Vec::new(),
            last_disk_check: Instant::now(),
            editor_body_rect: None,
            mouse_drag_anchor: None,
            setup_offered: std::collections::HashSet::new(),
            project_languages: None,
            project_scan_started: false,
            project_setup_offered: false,
            project_setup: None,
            settings,
            settings_dirty: false,
        };

        // The welcome screen is always the first view. A path from the command
        // line is preserved as an obvious action rather than opened
        // immediately, and a saved session for the workspace is offered as a
        // "Resume" action instead of being restored silently.
        app.recent = Recent::load();
        app.welcome_target =
            target.map(|path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf()));
        app.resume_session = crate::session::session_path(app.workspace.root())
            .and_then(|path| Session::load_from(&path));

        // Apply the user's preferences before the first frame so the initial
        // render already reflects them.
        app.apply_settings();
        app.request_git_refresh();
        // Probe for external tools on the worker so startup never waits on it.
        app.background.discover_tools();
        // Apply the initial detection and git snapshot before the first frame so
        // startup is deterministic.
        app.pump_background(Duration::from_millis(300));
        Ok(app)
    }

    // ----------------------------------------------------------------------
    // Animation
    // ----------------------------------------------------------------------

    /// Whether anything on screen animates right now.
    fn wants_animation(&self) -> bool {
        self.motion && (self.editor.is_empty() || self.busy().is_some())
    }

    fn animation_interval(&self) -> Duration {
        if self.busy().is_some() {
            // Busy work is short-lived, so spin smoothly while it lasts.
            Duration::from_millis(90)
        } else {
            // The welcome scene drifts gently.
            Duration::from_millis(450)
        }
    }

    /// Advance the animation frame once enough time has passed.
    fn tick_animation(&mut self) -> bool {
        if !self.wants_animation() {
            return false;
        }
        if self.anim_last.elapsed() >= self.animation_interval() {
            self.anim_last = Instant::now();
            self.anim_phase = self.anim_phase.wrapping_add(1);
            return true;
        }
        false
    }

    // ----------------------------------------------------------------------
    // Event handling
    // ----------------------------------------------------------------------

    // ----------------------------------------------------------------------
    // Commands
    // ----------------------------------------------------------------------

    // ----------------------------------------------------------------------
    // Editing helpers
    // ----------------------------------------------------------------------

    // ----------------------------------------------------------------------
    // File and tab operations
    // ----------------------------------------------------------------------

    // ----------------------------------------------------------------------
    // Welcome screen and project opening
    // ----------------------------------------------------------------------

    // ----------------------------------------------------------------------
    // Session persistence
    // ----------------------------------------------------------------------

    // ----------------------------------------------------------------------
    // Diagnostics
    // ----------------------------------------------------------------------

    // ----------------------------------------------------------------------
    // Language server
    // ----------------------------------------------------------------------

    // ----------------------------------------------------------------------
    // Search
    // ----------------------------------------------------------------------

    // ----------------------------------------------------------------------
    // Overlays
    // ----------------------------------------------------------------------

    // ----------------------------------------------------------------------
    // Misc
    // ----------------------------------------------------------------------
}

/// A sibling copy path that does not yet exist: `name copy.ext`,
/// `name copy 2.ext`, and so on.
mod helpers;
use helpers::*;

#[cfg(test)]
mod tests;
