//! Extracted from `app/mod.rs`: see `docs/DECISIONS.md`.

use super::*;

impl App {
    /// Ask the background worker to detect the active file's language.
    pub(super) fn request_detection_for_active(&mut self) {
        let fallback = self.workspace.marker_names();
        if let Some(path) = self
            .editor
            .active_document()
            .and_then(|doc| doc.buffer.path.clone())
        {
            // Use the file's nearest project markers so a monorepo subproject
            // contributes its own context.
            let markers = crate::project::nearest_markers(&path, &fallback);
            self.background.detect(path, markers);
        }
    }

    /// Reload clean files that changed on disk, and warn about dirty ones.
    ///
    /// Throttled, because it touches the filesystem. Returns `true` when the UI
    /// should repaint.
    pub(super) fn poll_external_changes(&mut self) -> bool {
        const INTERVAL: Duration = Duration::from_millis(1200);
        if self.last_disk_check.elapsed() < INTERVAL {
            return false;
        }
        self.last_disk_check = Instant::now();

        let mut message: Option<(bool, String)> = None;
        for doc in &mut self.editor.documents {
            let Some(path) = doc.buffer.path.clone() else {
                continue;
            };
            let Some(current) = filesystem::modified_time(&path) else {
                continue;
            };
            if doc.disk_modified() == Some(current) {
                continue;
            }
            if doc.is_dirty() {
                // Remember the new time so we warn once, not every tick.
                doc.record_disk_mtime();
                message = Some((
                    false,
                    format!("{} changed on disk — save to overwrite", doc.file_name()),
                ));
            } else if doc.reload_from_disk().is_ok() {
                message = Some((true, format!("{} reloaded from disk", doc.file_name())));
            }
        }

        match message {
            Some((true, message)) => {
                self.set_status(message);
                true
            }
            Some((false, message)) => {
                self.set_error(message);
                true
            }
            None => false,
        }
    }

    /// Ask the background worker to refresh git status.
    pub(super) fn request_git_refresh(&self) {
        self.background
            .refresh_git(self.workspace.root().to_path_buf());
    }

    /// Apply any finished background work. Returns `true` if something changed.
    pub(super) fn apply_background_events(&mut self) -> bool {
        let mut changed = false;
        while let Some(event) = self.background.try_recv() {
            changed |= self.apply_background_event(event);
        }
        changed
    }

    pub(super) fn apply_background_event(&mut self, event: BackgroundEvent) -> bool {
        match event {
            BackgroundEvent::Detected {
                path,
                language,
                confidence,
            } => {
                let changed = if let Some(doc) = self
                    .editor
                    .documents
                    .iter_mut()
                    .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
                {
                    let changed = doc.buffer.language != language;
                    doc.set_language(language);
                    if changed {
                        // The provider changed, so any earlier analysis is void:
                        // reset the revision to force a fresh first pass.
                        doc.clear_diagnostics();
                        doc.set_diagnostics_revision(0);
                    }
                    changed
                } else {
                    false
                };
                if confidence == crate::language::detection::Confidence::Low {
                    self.set_status(format!(
                        "Low-confidence detection for {} — use Set Language… to override",
                        path.display()
                    ));
                }
                if changed {
                    self.maybe_start_lsp(language);
                    // If a server for this language is already ready, attach the
                    // newly detected document to it. Handshake-time `lsp_ready`
                    // only covers the files open at that instant.
                    self.sync_document_to_lsp(&path, language);
                }
                changed
            }
            BackgroundEvent::Diagnostics {
                path,
                revision,
                diagnostics,
            } => self
                .editor
                .documents
                .iter_mut()
                .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
                .is_some_and(|doc| doc.apply_diagnostics(revision, diagnostics)),
            BackgroundEvent::Formatted {
                path,
                revision,
                outcome,
            } => {
                let current =
                    self.pending_format
                        .as_ref()
                        .is_some_and(|(pending_path, pending_revision)| {
                            *pending_path == path && *pending_revision == revision
                        });
                if !current {
                    return false;
                }
                self.pending_format = None;
                self.apply_format_outcome(&path, outcome);
                true
            }
            BackgroundEvent::Git(info) => {
                let root = info.repo_root.clone();
                self.workspace.git = info;
                match root {
                    Some(root) => {
                        let paths: Vec<PathBuf> = self
                            .editor
                            .documents
                            .iter()
                            .filter(|doc| !doc.is_large_file())
                            .filter_map(|doc| doc.buffer.path.clone())
                            .collect();
                        if !paths.is_empty() {
                            self.git_markers_revision = self.git_markers_revision.wrapping_add(1);
                            self.background.git_line_markers(
                                root,
                                paths,
                                self.git_markers_revision,
                            );
                        }
                    }
                    None => {
                        for doc in &mut self.editor.documents {
                            doc.set_git_markers(Vec::new());
                        }
                    }
                }
                if self.pending_stage_refresh {
                    self.pending_stage_refresh = false;
                    if matches!(&self.overlay, Overlay::Picker(picker) if picker.title == "Changed Files")
                    {
                        self.show_changed_files();
                    }
                }
                true
            }
            BackgroundEvent::GitLineMarkers { revision, markers } => {
                if revision != self.git_markers_revision {
                    return false;
                }
                for (path, marks) in markers {
                    if let Some(doc) = self
                        .editor
                        .documents
                        .iter_mut()
                        .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
                    {
                        doc.set_git_markers(marks);
                    }
                }
                true
            }
            BackgroundEvent::GitDiff {
                root,
                path,
                staged,
                result,
            } => {
                let current =
                    self.pending_git_diff
                        .as_ref()
                        .is_some_and(|(pending_path, pending_side)| {
                            *pending_path == path
                                && pending_side.is_none_or(|expected| expected == staged)
                        });
                if !current {
                    return false;
                }
                self.pending_git_diff = None;
                self.apply_git_diff(root, path, staged, result);
                true
            }
            BackgroundEvent::GitPatchApplied {
                root,
                path,
                staged,
                reverse,
                result,
            } => {
                if self.workspace.root() != root.as_path() {
                    return false;
                }
                match result {
                    Ok(()) => {
                        self.set_status(if reverse {
                            "Hunk reverted"
                        } else {
                            "Hunk applied"
                        });
                        // Refresh so the diff and gutter reflect the new state.
                        self.request_git_refresh();
                        // Reopen the diff for the same file.
                        self.show_diff(path, Some(staged));
                    }
                    Err(message) => self.set_error(format!("Could not apply hunk: {message}")),
                }
                true
            }
            BackgroundEvent::GitOperation { label, result } => {
                self.pending_git_op = None;
                match result {
                    Ok(message) => {
                        self.set_status(message.clone());
                        self.push_toast(ToastKind::Success, message);
                    }
                    Err(message) => {
                        let text = format!("{label} failed — {message}");
                        self.set_error(text.clone());
                        self.push_toast(ToastKind::Error, text);
                    }
                }
                true
            }
            BackgroundEvent::GitStaged {
                path,
                staged,
                result,
            } => {
                match result {
                    Ok(()) => {
                        let action = if staged { "Staged" } else { "Unstaged" };
                        let name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("file")
                            .to_string();
                        self.set_status(format!("{action} {name}"));
                    }
                    Err(message) => {
                        let text = format!("Git: {message}");
                        self.set_error(text.clone());
                        self.push_toast(ToastKind::Error, text);
                    }
                }
                true
            }
            BackgroundEvent::GitCommitted { result } => {
                self.pending_commit = false;
                match result {
                    Ok(message) => {
                        self.set_status(message.clone());
                        self.push_toast(ToastKind::Success, message);
                    }
                    Err(message) => {
                        let text = format!("Commit failed — {message}");
                        self.set_error(text.clone());
                        self.push_toast(ToastKind::Error, text);
                    }
                }
                true
            }
            BackgroundEvent::WorkspaceSymbols { revision, symbols } => {
                if self.pending_workspace_symbols != Some(revision) {
                    return false;
                }
                self.pending_workspace_symbols = None;
                self.open_workspace_symbol_picker(symbols);
                true
            }
            BackgroundEvent::SearchResults { revision, result } => {
                self.apply_project_search(revision, result)
            }
            BackgroundEvent::ProjectReplaced { revision, result } => {
                self.apply_project_replace(revision, result)
            }
            BackgroundEvent::Tools(registry) => {
                self.tools = Some(registry);
                // Discovery may finish after a file was already detected.
                if let Some(language) = self
                    .editor
                    .active_document()
                    .map(|doc| doc.buffer.language)
                    .filter(|language| *language != LanguageId::Unknown)
                {
                    self.maybe_start_lsp(language);
                }
                true
            }
            BackgroundEvent::ProjectLanguages(languages) => {
                self.project_languages = Some(languages);
                true
            }
            BackgroundEvent::ProjectDiagnostics { revision, problems } => {
                self.apply_project_diagnostics(revision, problems)
            }
            BackgroundEvent::TaskOutput { stream: _, text } => {
                self.append_task_output(&text);
                true
            }
            BackgroundEvent::TaskRun(outcome) => {
                self.pending_task = false;
                self.show_task_run(outcome);
                true
            }
            BackgroundEvent::ToolInstalled { tool, result } => {
                self.pending_install = None;
                let in_setup = self.project_setup.is_some();
                match &result {
                    Ok(message) => {
                        self.set_status(message.clone());
                        self.push_toast(ToastKind::Success, message.clone());
                    }
                    Err(message) => {
                        let text = format!("Install failed — {message}");
                        self.set_error(text.clone());
                        self.push_toast(ToastKind::Error, text);
                    }
                }
                if let Some(run) = self.project_setup.as_mut() {
                    match result {
                        Ok(_) => run.installed.push(tool),
                        Err(message) => run.failed.push((tool, message)),
                    }
                }
                // Continue an in-progress project setup with the next tool.
                if in_setup {
                    self.advance_project_setup();
                }
                true
            }
            BackgroundEvent::ProjectCreated {
                name,
                language: _,
                outcome,
            } => {
                self.pending_project = false;
                match outcome {
                    CreateOutcome::Created { root, files } => {
                        self.push_toast(ToastKind::Success, format!("Created {name}"));
                        self.recent.add_project(&root);
                        if !self.open_workspace(root) {
                            return true;
                        }
                        if let Some(entry) = entry_file(&files) {
                            self.open_path(entry);
                        }
                    }
                    CreateOutcome::Failed {
                        root: Some(root),
                        message,
                    } => {
                        let text = format!(
                            "Could not finish creating {name}: {message}. A partial project is at {}",
                            root.display()
                        );
                        self.set_error(text.clone());
                        self.push_toast(ToastKind::Error, text);
                    }
                    CreateOutcome::Failed {
                        root: None,
                        message,
                    } => {
                        let text = format!("Could not create {name}: {message}");
                        self.set_error(text.clone());
                        self.push_toast(ToastKind::Error, text);
                    }
                }
                true
            }
        }
    }
    /// Wait up to `timeout` for startup background work to settle.
    ///
    /// Exposed so embedders (and tests) can drive pending detection and git
    /// work deterministically before rendering.
    pub fn pump_background(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match self.background.recv_timeout(remaining) {
                Some(event) => {
                    self.apply_background_event(event);
                }
                None => break,
            }
        }
        self.apply_background_events();
    }
}
