//! A small background worker pool.
//!
//! Kemori must never block the UI on work that is not the user's keystroke. A few
//! worker threads share a request channel; the app sends requests and drains
//! results as they arrive, so expensive operations stay off the render path and
//! one slow formatter or installer cannot stall unrelated editor services.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::git::GitInfo;
use crate::language::LanguageService;
use crate::language::WorkspaceSymbol;
use crate::language::detection::Confidence;
use crate::language::diagnostics::Diagnostic;
use crate::language::format::FormatOutcome;
use crate::language::id::LanguageId;
use crate::language::tools::{Tool, ToolRegistry};
use crate::process::Stream;
use crate::project::tasks::Task;
use crate::search::{ReplaceOutcome, SearchMatch, SearchOptions};
pub use crate::task_run::TaskRunOutcome;

/// Work sent to the background thread.
enum Request {
    /// Detect the language of a file within a project.
    Detect { path: PathBuf, markers: Vec<String> },
    /// Compute diagnostics for a document snapshot.
    Diagnostics {
        path: PathBuf,
        language: LanguageId,
        text: String,
        revision: u64,
    },
    /// Format a document snapshot with the language's formatter.
    Format {
        path: PathBuf,
        language: LanguageId,
        text: String,
        revision: u64,
    },
    /// Scan a project for named definitions.
    WorkspaceSymbols { root: PathBuf, revision: u64 },
    /// Detect the languages present in a project, for project setup.
    ProjectLanguages { root: PathBuf },
    /// Search a project for a text query.
    SearchProject {
        root: PathBuf,
        query: String,
        options: SearchOptions,
        /// A replacement used only to build the per-line preview.
        replacement: Option<String>,
        revision: u64,
    },
    /// Replace a query across a project.
    ReplaceProject {
        root: PathBuf,
        query: String,
        options: SearchOptions,
        replacement: String,
        revision: u64,
        /// Open files with unsaved edits, which must not be overwritten.
        skip: Vec<PathBuf>,
    },
    /// Probe for the external tools Kemori can drive.
    DiscoverTools,
    /// Install a tool through its trusted package manager.
    InstallTool(Tool),
    /// Recompute git status for a repository root.
    RefreshGit { root: PathBuf },
    /// Recompute per-line git gutter markers for open documents.
    GitLineMarkers {
        root: PathBuf,
        paths: Vec<PathBuf>,
        revision: u64,
    },
    /// Show the diff for one path (working tree preferred, then staged).
    GitDiff {
        root: PathBuf,
        path: PathBuf,
        staged: Option<bool>,
    },
    /// Apply or revert one hunk.
    GitApplyPatch {
        root: PathBuf,
        path: PathBuf,
        staged: bool,
        patch: String,
        reverse: bool,
    },
    /// Push the current branch.
    GitPush { root: PathBuf },
    /// Pull from the upstream.
    GitPull { root: PathBuf },
    /// Fetch from all remotes.
    GitFetch { root: PathBuf },
    /// Create a GitHub repository with `gh` and push.
    GithubCreate { root: PathBuf, name: String },
    /// Stage all changes and commit them.
    GitCommit { root: PathBuf, message: String },
    /// Stage or unstage one path.
    GitStage {
        root: PathBuf,
        path: PathBuf,
        staged: bool,
    },
    /// Scaffold a new project.
    CreateProject {
        parent: PathBuf,
        name: String,
        language: LanguageId,
    },
    /// Run a run/test task provided by the project.
    RunTask { root: PathBuf, task: Task },
}

/// A finished piece of background work.
pub enum Event {
    Detected {
        path: PathBuf,
        language: LanguageId,
        confidence: Confidence,
    },
    Diagnostics {
        path: PathBuf,
        revision: u64,
        diagnostics: Vec<Diagnostic>,
    },
    Formatted {
        path: PathBuf,
        revision: u64,
        outcome: FormatOutcome,
    },
    WorkspaceSymbols {
        revision: u64,
        symbols: Vec<WorkspaceSymbol>,
    },
    /// The languages detected in a project, for project setup.
    ProjectLanguages(Vec<LanguageId>),
    /// The result of a project-wide text search.
    SearchResults {
        revision: u64,
        result: Result<Vec<SearchMatch>, String>,
    },
    /// The result of a project-wide replacement.
    ProjectReplaced {
        revision: u64,
        result: Result<ReplaceOutcome, String>,
    },
    /// The result of probing for external tools.
    Tools(ToolRegistry),
    /// The result of installing a tool.
    ToolInstalled {
        tool: Tool,
        result: Result<String, String>,
    },
    Git(GitInfo),
    /// Per-line gutter markers for open documents, tagged with the request
    /// revision so a stale refresh cannot overwrite a newer one.
    GitLineMarkers {
        revision: u64,
        markers: Vec<(PathBuf, Vec<crate::git::GitLineMarker>)>,
    },
    /// The resolved diff for one path; `staged` says which side was shown.
    GitDiff {
        root: PathBuf,
        path: PathBuf,
        staged: bool,
        result: Result<String, String>,
    },
    /// The result of applying or reverting one hunk.
    GitPatchApplied {
        root: PathBuf,
        path: PathBuf,
        staged: bool,
        reverse: bool,
        result: Result<(), String>,
    },
    /// A push, pull, fetch or GitHub action, with a label for the status line.
    GitOperation {
        label: &'static str,
        result: Result<String, String>,
    },
    /// The result of staging and committing.
    GitCommitted {
        result: Result<String, String>,
    },
    /// The result of staging or unstaging one path.
    GitStaged {
        path: PathBuf,
        staged: bool,
        result: Result<(), String>,
    },
    /// The result of scaffolding a new project.
    ProjectCreated {
        name: String,
        language: LanguageId,
        outcome: crate::project::create::CreateOutcome,
    },
    /// A live chunk of a run task's output.
    TaskOutput {
        stream: Stream,
        text: String,
    },
    /// The final result of a run task.
    TaskRun(TaskRunOutcome),
}

/// Handle to the background worker.
pub struct Background {
    requests: mpsc::SyncSender<Request>,
    events: Receiver<Event>,
    /// Set to cancel the task currently running on a worker.
    task_cancel: Arc<AtomicBool>,
}

/// How many background operations may run at once. A small pool keeps a slow
/// formatter or installer from blocking detection, diagnostics and git.
const WORKER_THREADS: usize = 3;

/// The most queued requests before automatic snapshots are dropped. Control
/// requests still block briefly, but a flood of automatic work cannot grow the
/// queue without bound.
const REQUEST_QUEUE: usize = 128;

/// The most files a project-language scan reads. The walk is `.gitignore`-aware
/// and skips generated directories; the cap keeps a huge tree predictable.
const PROJECT_SCAN_LIMIT: usize = 4000;

impl Background {
    /// Spawn the workers, sharing the language service with the UI thread.
    pub fn spawn(language: Arc<LanguageService>) -> Self {
        let (request_tx, request_rx) = mpsc::sync_channel::<Request>(REQUEST_QUEUE);
        let (event_tx, event_rx) = mpsc::channel::<Event>();
        let request_rx = Arc::new(Mutex::new(request_rx));
        let task_cancel = Arc::new(AtomicBool::new(false));

        for index in 0..WORKER_THREADS {
            let requests = Arc::clone(&request_rx);
            let events = event_tx.clone();
            let language = Arc::clone(&language);
            let task_cancel = Arc::clone(&task_cancel);
            let _ = thread::Builder::new()
                .name(format!("kemori-background-{index}"))
                .spawn(move || {
                    loop {
                        // Hold the lock only while waiting, so the other workers
                        // can run their operations concurrently.
                        let request = {
                            let receiver = match requests.lock() {
                                Ok(receiver) => receiver,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            receiver.recv()
                        };
                        let Ok(request) = request else { break };
                        handle_request(request, &language, &events, &task_cancel);
                    }
                });
        }
        drop(event_tx);

        Background {
            requests: request_tx,
            events: event_rx,
            task_cancel,
        }
    }

    /// Queue a request that must not be dropped.
    fn send(&self, request: Request) {
        let _ = self.requests.send(request);
    }

    /// Queue an automatic request, dropping it when the queue is saturated.
    ///
    /// Used for diagnostics snapshots, which a later edit supersedes anyway.
    fn try_send(&self, request: Request) {
        let _ = self.requests.try_send(request);
    }

    /// Ask for a file's language to be detected.
    pub fn detect(&self, path: PathBuf, markers: Vec<String>) {
        self.send(Request::Detect { path, markers });
    }

    /// Ask for diagnostics on a document snapshot.
    ///
    /// `revision` lets the app discard results that arrive out of order.
    pub fn diagnose(&self, path: PathBuf, language: LanguageId, text: String, revision: u64) {
        self.try_send(Request::Diagnostics {
            path,
            language,
            text,
            revision,
        });
    }

    /// Ask for a document snapshot to be formatted.
    pub fn format(&self, path: PathBuf, language: LanguageId, text: String, revision: u64) {
        self.send(Request::Format {
            path,
            language,
            text,
            revision,
        });
    }

    /// Ask for a project-wide symbol scan.
    pub fn workspace_symbols(&self, root: PathBuf, revision: u64) {
        let _ = self
            .requests
            .send(Request::WorkspaceSymbols { root, revision });
    }

    /// Ask for the languages present in a project, for project setup.
    pub fn detect_project_languages(&self, root: PathBuf) {
        let _ = self.requests.send(Request::ProjectLanguages { root });
    }

    /// Ask for a project-wide text search.
    pub fn search_project(
        &self,
        root: PathBuf,
        query: String,
        options: SearchOptions,
        replacement: Option<String>,
        revision: u64,
    ) {
        let _ = self.requests.send(Request::SearchProject {
            root,
            query,
            options,
            replacement,
            revision,
        });
    }

    /// Ask for a project-wide replacement.
    pub fn replace_project(
        &self,
        root: PathBuf,
        query: String,
        options: SearchOptions,
        replacement: String,
        revision: u64,
        skip: Vec<PathBuf>,
    ) {
        let _ = self.requests.send(Request::ReplaceProject {
            root,
            query,
            options,
            replacement,
            revision,
            skip,
        });
    }

    /// Ask for the external tools to be probed.
    pub fn discover_tools(&self) {
        let _ = self.requests.send(Request::DiscoverTools);
    }

    /// Ask for a tool to be installed through its trusted package manager.
    pub fn install_tool(&self, tool: Tool) {
        let _ = self.requests.send(Request::InstallTool(tool));
    }

    /// Ask for git status to be refreshed.
    pub fn refresh_git(&self, root: PathBuf) {
        let _ = self.requests.send(Request::RefreshGit { root });
    }

    /// Ask for gutter markers for the given open documents.
    pub fn git_line_markers(&self, root: PathBuf, paths: Vec<PathBuf>, revision: u64) {
        let _ = self.requests.send(Request::GitLineMarkers {
            root,
            paths,
            revision,
        });
    }

    /// Ask for a path's diff. `staged` of `None` prefers the working tree and
    /// falls back to the index when the working tree is clean.
    pub fn git_diff(&self, root: PathBuf, path: PathBuf, staged: Option<bool>) {
        let _ = self.requests.send(Request::GitDiff { root, path, staged });
    }

    /// Ask to apply or revert one hunk.
    pub fn git_apply_patch(
        &self,
        root: PathBuf,
        path: PathBuf,
        staged: bool,
        patch: String,
        reverse: bool,
    ) {
        let _ = self.requests.send(Request::GitApplyPatch {
            root,
            path,
            staged,
            patch,
            reverse,
        });
    }

    /// Ask to push the current branch.
    pub fn git_push(&self, root: PathBuf) {
        let _ = self.requests.send(Request::GitPush { root });
    }

    /// Ask to pull from the upstream.
    pub fn git_pull(&self, root: PathBuf) {
        let _ = self.requests.send(Request::GitPull { root });
    }

    /// Ask to fetch from all remotes.
    pub fn git_fetch(&self, root: PathBuf) {
        let _ = self.requests.send(Request::GitFetch { root });
    }

    /// Ask `gh` to create and push a GitHub repository.
    pub fn github_create(&self, root: PathBuf, name: String) {
        let _ = self.requests.send(Request::GithubCreate { root, name });
    }

    /// Ask to stage all changes and commit them.
    pub fn commit_all(&self, root: PathBuf, message: String) {
        let _ = self.requests.send(Request::GitCommit { root, message });
    }

    /// Ask to stage or unstage one path.
    pub fn stage_path(&self, root: PathBuf, path: PathBuf, staged: bool) {
        let _ = self.requests.send(Request::GitStage { root, path, staged });
    }

    /// Ask to scaffold a new project.
    pub fn create_project(&self, parent: PathBuf, name: String, language: LanguageId) {
        let _ = self.requests.send(Request::CreateProject {
            parent,
            name,
            language,
        });
    }

    /// Ask for a project task to run in the project root.
    pub fn run_task(&self, root: PathBuf, task: Task) {
        self.task_cancel.store(false, Ordering::Relaxed);
        let _ = self.requests.send(Request::RunTask { root, task });
    }

    /// Cancel the task currently running on a worker, if any.
    pub fn cancel_task(&self) {
        self.task_cancel.store(true, Ordering::Relaxed);
    }

    /// Take the next finished event, if any.
    pub fn try_recv(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    /// Wait up to `timeout` for the next event.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.events.recv_timeout(timeout).ok()
    }
}

/// Execute one request and publish its event(s).
fn handle_request(
    request: Request,
    language: &LanguageService,
    events: &Sender<Event>,
    task_cancel: &Arc<AtomicBool>,
) {
    match request {
        Request::Detect { path, markers } => {
            let result = language.detect_file(&path, &markers);
            let _ = events.send(Event::Detected {
                path,
                language: result.language,
                confidence: result.confidence,
            });
        }
        Request::Diagnostics {
            path,
            language: id,
            text,
            revision,
        } => {
            let diagnostics = language.provider(id).diagnostics(&text);
            let _ = events.send(Event::Diagnostics {
                path,
                revision,
                diagnostics,
            });
        }
        Request::Format {
            path,
            language: id,
            text,
            revision,
        } => {
            let outcome = language.provider(id).format(&path, &text);
            let _ = events.send(Event::Formatted {
                path,
                revision,
                outcome,
            });
        }
        Request::WorkspaceSymbols { root, revision } => {
            let symbols = language.workspace_symbols(&root, 3000);
            let _ = events.send(Event::WorkspaceSymbols { revision, symbols });
        }
        Request::ProjectLanguages { root } => {
            let languages = language.detect_project_languages(&root, PROJECT_SCAN_LIMIT);
            let _ = events.send(Event::ProjectLanguages(languages));
        }
        Request::SearchProject {
            root,
            query,
            options,
            replacement,
            revision,
        } => {
            let result =
                crate::search::search_project(&root, &query, &options, replacement.as_deref(), 500);
            let _ = events.send(Event::SearchResults { revision, result });
        }
        Request::ReplaceProject {
            root,
            query,
            options,
            replacement,
            revision,
            skip,
        } => {
            let skip: HashSet<PathBuf> = skip.into_iter().collect();
            let result =
                crate::search::replace_project(&root, &query, &options, &replacement, &skip);
            let _ = events.send(Event::ProjectReplaced { revision, result });
        }
        Request::DiscoverTools => {
            let _ = events.send(Event::Tools(ToolRegistry::discover_cached()));
        }
        Request::InstallTool(tool) => {
            let result = crate::language::tools::install(tool);
            let _ = events.send(Event::ToolInstalled { tool, result });
            // Re-probe so the setup view and server startup see the new state
            // immediately.
            let _ = events.send(Event::Tools(ToolRegistry::discover_cached()));
        }
        Request::RefreshGit { root } => {
            // Routed through the pluggable command runner: local unless
            // KEMORI_REMOTE names an SSH host (an experimental, git-only seam).
            // The pre-rename KODA_REMOTE spelling still works for one release.
            let remote = std::env::var("KEMORI_REMOTE")
                .or_else(|_| std::env::var("KODA_REMOTE"))
                .ok();
            let runner = crate::remote::runner_for(remote.as_deref());
            let _ = events.send(Event::Git(GitInfo::detect_with(runner.as_ref(), &root)));
        }
        Request::GitLineMarkers {
            root,
            paths,
            revision,
        } => {
            let markers = paths
                .into_iter()
                .map(|path| {
                    let markers = crate::git::line_changes(&root, &path);
                    (path, markers)
                })
                .collect();
            let _ = events.send(Event::GitLineMarkers { revision, markers });
        }
        Request::GitDiff { root, path, staged } => {
            let resolved = match staged {
                Some(side) => crate::git::diff(&root, &path, side)
                    .map(|text| (text, side))
                    .map_err(|err| format!("Could not diff {}: {err}", path.display())),
                None => match crate::git::diff(&root, &path, false) {
                    Ok(text) if !text.is_empty() => Ok((text, false)),
                    Ok(_) => crate::git::diff(&root, &path, true)
                        .map(|text| (text, true))
                        .map_err(|err| format!("Could not diff {}: {err}", path.display())),
                    Err(err) => Err(format!("Could not diff {}: {err}", path.display())),
                },
            };
            let event = match resolved {
                Ok((text, staged)) => Event::GitDiff {
                    root,
                    path,
                    staged,
                    result: Ok(text),
                },
                Err(message) => Event::GitDiff {
                    root,
                    path,
                    staged: staged.unwrap_or(false),
                    result: Err(message),
                },
            };
            let _ = events.send(event);
        }
        Request::GitApplyPatch {
            root,
            path,
            staged,
            patch,
            reverse,
        } => {
            let result = crate::git::apply_patch(&root, &patch, reverse, staged);
            let _ = events.send(Event::GitPatchApplied {
                root: root.clone(),
                path,
                staged,
                reverse,
                result,
            });
            let _ = events.send(Event::Git(GitInfo::detect(&root)));
        }
        Request::GitPush { root } => {
            let result = crate::git::push(&root);
            let _ = events.send(Event::GitOperation {
                label: "Push",
                result,
            });
            let _ = events.send(Event::Git(GitInfo::detect(&root)));
        }
        Request::GitPull { root } => {
            let result = crate::git::pull(&root);
            let _ = events.send(Event::GitOperation {
                label: "Pull",
                result,
            });
            let _ = events.send(Event::Git(GitInfo::detect(&root)));
        }
        Request::GitFetch { root } => {
            let result = crate::git::fetch(&root);
            let _ = events.send(Event::GitOperation {
                label: "Fetch",
                result,
            });
            let _ = events.send(Event::Git(GitInfo::detect(&root)));
        }
        Request::GithubCreate { root, name } => {
            let result = if !crate::github::available() {
                Err("GitHub CLI (gh) is not installed".to_string())
            } else if crate::github::auth_status().is_none() {
                Err("Not logged in to GitHub — run `gh auth login`".to_string())
            } else {
                crate::github::create_repo(&root, &name, false)
            };
            let _ = events.send(Event::GitOperation {
                label: "GitHub",
                result,
            });
            let _ = events.send(Event::Git(GitInfo::detect(&root)));
        }
        Request::GitCommit { root, message } => {
            let result = crate::git::commit_all(&root, &message);
            let _ = events.send(Event::GitCommitted { result });
            // Refresh so the status bar and changed-files list reflect the new
            // state immediately.
            let _ = events.send(Event::Git(GitInfo::detect(&root)));
        }
        Request::GitStage { root, path, staged } => {
            let result = if staged {
                crate::git::stage(&root, &path)
            } else {
                crate::git::unstage(&root, &path)
            };
            let _ = events.send(Event::GitStaged {
                path,
                staged,
                result,
            });
            let _ = events.send(Event::Git(GitInfo::detect(&root)));
        }
        Request::CreateProject {
            parent,
            name,
            language,
        } => {
            let outcome = crate::project::create::create(&parent, &name, language);
            let _ = events.send(Event::ProjectCreated {
                name,
                language,
                outcome,
            });
        }
        Request::RunTask { root, task } => {
            let sender = events.clone();
            let outcome = crate::task_run::run(&root, task, task_cancel, move |stream, text| {
                let _ = sender.send(Event::TaskOutput {
                    stream,
                    text: text.to_string(),
                });
            });
            let _ = events.send(Event::TaskRun(outcome));
        }
    }
}
