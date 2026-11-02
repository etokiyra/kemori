//! Extracted from `app/mod.rs`: see `docs/DECISIONS.md`.

use super::*;

impl App {
    /// Display the unified diff for `path`.
    ///
    /// When `staged` is `None`, the working-tree diff is preferred and the
    /// staged diff is shown if the working tree is clean.
    pub(super) fn apply_diff_hunk(&mut self, reverse: bool) {
        let (root, path, staged, patch) = match &self.overlay {
            Overlay::Diff(diff) => {
                let root = match diff.root.clone() {
                    Some(root) => root,
                    None => {
                        self.set_error("This diff cannot be applied");
                        return;
                    }
                };
                let path = match diff.path.clone() {
                    Some(path) => path,
                    None => {
                        self.set_error("This diff has no target file");
                        return;
                    }
                };
                let patch = match diff.current_hunk_patch() {
                    Some(patch) => patch,
                    None => {
                        self.set_error("No hunk to apply");
                        return;
                    }
                };
                (root, path, diff.staged, patch)
            }
            _ => return,
        };

        self.background
            .git_apply_patch(root, path, staged, patch, reverse);
        self.set_status(if reverse {
            "Reverting hunk…".to_string()
        } else {
            "Applying hunk…".to_string()
        });
    }

    pub(super) fn git_push(&mut self) {
        self.start_git_operation("Push");
    }

    pub(super) fn git_pull(&mut self) {
        self.start_git_operation("Pull");
    }

    pub(super) fn git_fetch(&mut self) {
        self.start_git_operation("Fetch");
    }

    /// Start a push/pull/fetch on the worker, refusing a second concurrent one.
    fn start_git_operation(&mut self, label: &'static str) {
        if let Some(running) = self.pending_git_op {
            self.set_status(format!("{running} is already running"));
            return;
        }
        self.pending_git_op = Some(label);
        let root = self.workspace.root().to_path_buf();
        match label {
            "Push" => self.background.git_push(root),
            "Pull" => self.background.git_pull(root),
            _ => self.background.git_fetch(root),
        }
        self.set_status(format!("{label}ing…"));
    }

    pub(super) fn github_create_repo(&mut self) {
        if let Some(running) = self.pending_git_op {
            self.set_status(format!("{running} is already running"));
            return;
        }
        self.pending_git_op = Some("GitHub");
        let root = self.workspace.root().to_path_buf();
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("kemori-project")
            .to_string();
        self.background.github_create(root, name);
        self.set_status("Creating GitHub repository…");
    }

    pub(super) fn show_diff(&mut self, path: PathBuf, staged: Option<bool>) {
        let root = self.workspace.root().to_path_buf();
        let name = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();
        self.pending_git_diff = Some((path.clone(), staged));
        self.background.git_diff(root, path, staged);
        self.set_status(format!("Loading diff for {name}…"));
    }

    /// Show a diff that finished on the worker.
    pub(super) fn apply_git_diff(
        &mut self,
        root: PathBuf,
        path: PathBuf,
        staged: bool,
        result: Result<String, String>,
    ) {
        if self.workspace.root() != root.as_path() {
            return;
        }
        let text = match result {
            Ok(text) => text,
            Err(message) => {
                self.set_error(message);
                return;
            }
        };
        if text.is_empty() {
            self.set_status(format!("No changes to show for {}", path.display()));
            return;
        }
        let name = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let where_ = if staged { "staged" } else { "working tree" };
        let title = format!("{where_} · {name}");
        self.overlay =
            Overlay::Diff(DiffState::from_unified(title, &text).with_context(root, path, staged));
    }

    /// Build and show the changed-files picker from the current snapshot.
    pub(super) fn show_changed_files(&mut self) {
        let items = self.changed_files_items();
        let mut picker = Picker::new("Changed Files", "Filter files…  ·  Space stages", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// List the files changed in the working tree.
    pub(super) fn open_changed_files(&mut self) {
        if !self.workspace.git.available {
            self.set_status("Not a git repository");
            return;
        }
        if self.workspace.git.files.is_empty() {
            self.set_status("Working tree clean");
            return;
        }
        self.show_changed_files();
    }

    /// Rows for the changed-files picker, sorted by path.
    pub(super) fn changed_files_items(&self) -> Vec<PickerItem> {
        let root = self.workspace.root().to_path_buf();
        let mut entries: Vec<(PathBuf, GitFileStatus)> = self
            .workspace
            .git
            .files
            .iter()
            .map(|(path, status)| (path.clone(), *status))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
            .into_iter()
            .map(|(path, status)| {
                let relative = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                let stage = if self.workspace.git.is_staged(&path) {
                    "staged"
                } else {
                    "unstaged"
                };
                PickerItem::new(
                    relative,
                    format!("{} {} · {stage}", status.indicator(), status.label()),
                    PickerAction::OpenPath(path),
                )
            })
            .collect()
    }

    /// Stage or unstage the selected file (tree selection or active document).
    pub(super) fn toggle_stage_target(&mut self) {
        if !self.workspace.git.available {
            self.set_status("Not a git repository");
            return;
        }
        let Some(path) = self.file_op_target() else {
            self.set_status("Select a file to stage");
            return;
        };
        let staged = !self.workspace.git.is_staged(&path);
        self.dispatch_stage(path, staged, true);
    }

    /// Send a stage/unstage request and show a short status hint.
    pub(super) fn dispatch_stage(&mut self, path: PathBuf, staged: bool, refresh_picker: bool) {
        let root = self
            .workspace
            .git
            .repo_root
            .clone()
            .unwrap_or_else(|| self.workspace.root().to_path_buf());
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file")
            .to_string();
        self.pending_stage_refresh = refresh_picker;
        self.background.stage_path(root, path, staged);
        let action = if staged { "Staging" } else { "Unstaging" };
        self.set_status(format!("{action} {name}…"));
    }

    /// Show the diff for the active file: working tree first, then staged.
    pub(super) fn diff_active_file(&mut self) {
        if !self.workspace.git.available {
            self.set_status("Not a git repository");
            return;
        }
        let Some(path) = self
            .editor
            .active_document()
            .and_then(|doc| doc.buffer.path.clone())
        else {
            self.set_status("No file to diff");
            return;
        };
        self.show_diff(path, None);
    }

    /// Prompt for a commit message, then stage everything and commit.
    pub(super) fn commit_changes(&mut self) {
        if !self.workspace.git.available {
            self.set_status("Not a git repository");
            return;
        }
        if self.workspace.git.files.is_empty() {
            self.set_status("Nothing to commit");
            return;
        }
        self.open_prompt(
            PromptKind::CommitMessage,
            "Commit message",
            "describe the change",
        );
    }
}
