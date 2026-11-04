//! Extracted from `app/mod.rs`: see `docs/DECISIONS.md`.

use super::*;

impl App {
    pub(super) fn run_picker_action(&mut self, action: PickerAction) {
        match action {
            PickerAction::Command(id) => self.execute_command(id),
            PickerAction::OpenPath(path) => self.open_path(path),
            PickerAction::Reveal { path, position } => self.reveal(path, position),
            PickerAction::RevealLsp { path, position } => self.reveal_lsp(path, position),
            PickerAction::Info(message) => self.set_status(message),
            PickerAction::InstallTool(tool) => self.install_tool(tool),
            PickerAction::ProjectSetup => self.start_project_setup(),
            PickerAction::ToolActions(tool) => self.tool_actions(tool),
            PickerAction::UpdateTool(tool) => self.install_tool(tool),
            PickerAction::RemoveTool(tool) => self.remove_tool(tool),
            PickerAction::ApplyCodeAction(index) => self.apply_code_action(index),
            PickerAction::RunCodeLens(index) => self.run_code_lens(index),
            PickerAction::HierarchyItem(index) => self.resolve_hierarchy_item(index),
            PickerAction::OpenLink(target) => self.open_link(target),
            PickerAction::DeletePath(path) => self.delete_path(&path),
            PickerAction::ShowDiff { path, staged } => self.show_diff(path, Some(staged)),
            PickerAction::RunTask(task) => {
                self.ensure_trust(super::trust::TrustIntent::RunTask(task))
            }
            PickerAction::ShowTaskOutput(index) => self.show_task_history(index),
            PickerAction::SetLanguage(language) => self.set_language(language),
            PickerAction::SwitchWorkspace(index) => self.switch_workspace(index),
            PickerAction::ResolveTrust(decision) => self.resolve_trust(decision),
            PickerAction::InsertSnippet(snippet) => self.insert_snippet(&snippet),
        }
    }

    pub(super) fn submit_prompt(&mut self, kind: PromptKind, input: String) {
        let input = input.trim().to_string();
        match kind {
            PromptKind::GotoLine => match input.parse::<usize>() {
                Ok(line) => self.with_doc(|d| d.go_to_line(line)),
                Err(_) => self.set_error("Not a valid line number"),
            },
            PromptKind::OpenPath => {
                if input.is_empty() {
                    self.set_error("No path provided");
                } else {
                    self.open_path(self.resolve_path(&input));
                }
            }
            PromptKind::SaveAs => {
                if input.is_empty() {
                    self.set_error("No path provided");
                } else {
                    self.save_as(self.resolve_path(&input));
                }
            }
            PromptKind::Rename => {
                if input.is_empty() {
                    self.set_error("No new name provided");
                    return;
                }
                let Some((path, row, col)) = self.pending_rename.take() else {
                    return;
                };
                // The prompt may be answered after the user switched tabs, so
                // resolve the language and version from the document itself.
                let language = self
                    .editor
                    .documents
                    .iter()
                    .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
                    .map(|doc| doc.buffer.language)
                    .or_else(|| self.lsp_language());
                let Some(language) = language else {
                    self.set_status("Rename needs a language server");
                    return;
                };
                let lsp_col = self.lsp_col(language, &path, row, col);
                let version = self.document_version(&path).unwrap_or(0);
                if let Some(server) = self
                    .lsp
                    .get_mut(&(self.workspace.root().to_path_buf(), language))
                    .and_then(|job| job.server.as_mut())
                {
                    self.pending_rename_request =
                        server
                            .rename(&path, row, lsp_col, &input)
                            .map(|id| PendingDocRequest {
                                language,
                                id,
                                path: path.clone(),
                                version,
                            });
                }
                self.set_status("Renaming…");
            }
            PromptKind::NewFile => {
                if input.is_empty() {
                    self.set_error("No file name provided");
                    return;
                }
                let path = if Path::new(&input).is_absolute() {
                    PathBuf::from(&input)
                } else {
                    self.new_file_dir().join(&input)
                };
                match filesystem::create_empty_file(&path) {
                    Ok(()) => {
                        self.tree_visible = true;
                        self.workspace.tree.refresh();
                        self.workspace.tree.select_path(&path);
                        self.open_path(path);
                    }
                    Err(err) => self.set_error(format!("Could not create file: {err}")),
                }
            }
            PromptKind::RenameFile => {
                let Some(old) = self.pending_rename_file.take() else {
                    return;
                };
                if input.is_empty() {
                    self.set_error("No new name provided");
                    return;
                }
                let new = if Path::new(&input).is_absolute() {
                    PathBuf::from(&input)
                } else {
                    old.parent()
                        .map(|parent| parent.join(&input))
                        .unwrap_or_else(|| PathBuf::from(&input))
                };
                if new == old {
                    self.set_status("Name unchanged");
                    return;
                }
                match filesystem::rename_path(&old, &new) {
                    Ok(()) => {
                        self.after_file_rename(&old, &new);
                        self.set_status(format!("Renamed to {}", new.display()));
                    }
                    Err(err) => self.set_error(format!("Rename failed: {err}")),
                }
            }
            PromptKind::CopyFile => {
                let Some(source) = self.pending_copy_file.take() else {
                    return;
                };
                if input.is_empty() {
                    self.set_error("No destination provided");
                    return;
                }
                let destination = if Path::new(&input).is_absolute() {
                    PathBuf::from(&input)
                } else {
                    self.workspace.root().join(&input)
                };
                match filesystem::copy_file(&source, &destination) {
                    Ok(()) => {
                        self.workspace.tree.refresh();
                        self.workspace.tree.select_path(&destination);
                        self.set_status(format!("Copied to {}", destination.display()));
                    }
                    Err(err) => self.set_error(format!("Copy failed: {err}")),
                }
            }
            PromptKind::DebugWatch => self.add_debug_watch(input),
            PromptKind::DebugAttach => self.start_attach(&input),
            PromptKind::BreakpointCondition => self.set_breakpoint_condition(input),
            PromptKind::BreakpointLog => self.set_breakpoint_log(input),
            PromptKind::CommitMessage => {
                if input.is_empty() {
                    self.set_error("No commit message provided");
                    return;
                }
                if self.pending_commit {
                    self.set_status("A commit is already running");
                    return;
                }
                self.pending_commit = true;
                self.background
                    .commit_all(self.workspace.root().to_path_buf(), input.clone());
                self.set_status(format!("Committing: {input}"));
            }
        }
    }

    /// Execute a command by id.
    pub fn execute_command(&mut self, id: &str) {
        match id {
            ids::SAVE => self.save(),
            ids::SAVE_ALL => self.save_all(),
            ids::SAVE_AS => self.open_prompt(PromptKind::SaveAs, "Save as", "path/to/file"),
            ids::OPEN => self.open_prompt(PromptKind::OpenPath, "Open file", "path/to/file.rs"),
            ids::QUICK_OPEN => self.open_quick_open(),
            ids::CLOSE_TAB => self.close_tab(),
            ids::CLOSE_ALL => self.close_all(),
            ids::REVERT => self.revert_file(),
            ids::CONVERT_TO_UTF8 => self.convert_to_utf8(),
            ids::CONVERT_LF => self.convert_line_endings(crate::editor::buffer::LineEnding::Lf),
            ids::CONVERT_CRLF => self.convert_line_endings(crate::editor::buffer::LineEnding::Crlf),
            ids::TRIM_TRAILING_WHITESPACE => self.trim_trailing_whitespace(),
            ids::TOGGLE_READ_ONLY => self.toggle_read_only(),
            ids::ENSURE_FINAL_NEWLINE => self.ensure_final_newline(),
            ids::NEW_FILE => self.new_file(),
            ids::RENAME_FILE => self.rename_selected(),
            ids::DELETE_FILE => self.delete_selected(),
            ids::DUPLICATE_FILE => self.duplicate_file(),
            ids::COPY_FILE => self.copy_file(),
            ids::QUIT => self.request_quit(),
            ids::HOME => self.go_home(),
            ids::WELCOME_SCENE => {
                self.welcome_scene = self.welcome_scene.next();
                self.set_status(format!("Welcome scene: {}", self.welcome_scene.label()));
            }
            ids::TOGGLE_MOTION => {
                self.motion = !self.motion;
                self.settings.motion = self.motion;
                self.settings_dirty = true;
                let message = if self.motion {
                    "Animations on"
                } else {
                    "Animations off"
                };
                self.set_status(message);
            }
            ids::OPEN_PROJECT => self.open_dir_picker(),
            ids::SWITCH_WORKSPACE => self.switch_workspace_picker(),
            ids::NEW_PROJECT => self.open_new_project(),
            ids::SETUP_PROJECT => self.open_project_setup(),
            ids::UNDO => self.with_doc(|d| d.undo()),
            ids::REDO => self.with_doc(|d| d.redo()),
            ids::SELECT_ALL => self.with_doc(|d| d.select_all()),
            ids::SELECT_NEXT => self.with_doc(|d| d.select_next_occurrence()),
            ids::SELECT_ALL_OCCURRENCES => self.with_doc(|d| d.select_all_occurrences()),
            ids::EXPAND_SELECTION => self.expand_selection(),
            ids::SHRINK_SELECTION => self.shrink_selection(),
            ids::ADD_CURSOR_BELOW => self.with_doc(|d| d.add_cursor_below()),
            ids::ADD_CURSOR_ABOVE => self.with_doc(|d| d.add_cursor_above()),
            ids::COPY => self.copy(),
            ids::CUT => self.cut(),
            ids::PASTE => self.paste(),
            ids::YANK_POP => self.yank_pop(),
            ids::COMPLETE => self.open_completion(),
            ids::SNIPPET => self.open_snippet_picker(),
            ids::HOVER => self.open_hover(),
            ids::SETUP => self.language_setup(),
            ids::SET_LANGUAGE => self.set_language_picker(),
            ids::LANGUAGE_SUPPORT => self.language_support(),
            ids::RESTART_SERVER => self.restart_language_server(),
            ids::WORKSPACE_SYMBOLS => self.open_workspace_symbols(),
            ids::PROJECT_SEARCH => self.open_project_search(),
            ids::RUN_TASKS => self.open_task_picker(),
            ids::RUN_ACTIVE_FILE => self.run_active_file(),
            ids::CANCEL_TASK => self.cancel_task(),
            ids::RERUN_TASK => self.rerun_task(),
            ids::TASK_HISTORY => self.open_task_history(),
            ids::PANEL_TERMINAL => self.open_terminal(),
            ids::TERMINAL_NEW => self.start_terminal_session(),
            ids::TERMINAL_CLOSE => self.close_terminal_session(),
            ids::TERMINAL_NEXT => self.next_terminal_session(),
            ids::TRUST_WORKSPACE => self.trust_workspace(),
            ids::REVOKE_TRUST => self.revoke_workspace_trust(),
            ids::REFRESH_PROBLEMS => self.refresh_project_problems(),
            ids::CHANGED_FILES => self.open_changed_files(),
            ids::GIT_COMMIT => self.commit_changes(),
            ids::GIT_TOGGLE_STAGE => self.toggle_stage_target(),
            ids::DIFF => self.diff_active_file(),
            ids::GIT_PUSH => self.git_push(),
            ids::GIT_PULL => self.git_pull(),
            ids::GIT_FETCH => self.git_fetch(),
            ids::GITHUB_CREATE_REPO => self.github_create_repo(),
            ids::FIND => self.open_search(false),
            ids::FIND_IN_SELECTION => self.open_search_in_selection(false),
            ids::REPLACE => self.open_search(true),
            ids::REPLACE_IN_SELECTION => self.open_search_in_selection(true),
            ids::REPLACE_ALL => self.replace_all(),
            ids::GOTO_LINE => self.open_prompt(PromptKind::GotoLine, "Go to line", "42"),
            ids::TOGGLE_COMMENT => self.toggle_comment(),
            ids::INDENT => self.with_doc(|d| d.indent()),
            ids::OUTDENT => self.with_doc(|d| d.outdent()),
            ids::MOVE_LINE_UP => self.with_doc(|d| d.move_line_up()),
            ids::MOVE_LINE_DOWN => self.with_doc(|d| d.move_line_down()),
            ids::DUPLICATE_LINE => self.with_doc(|d| d.duplicate_line()),
            ids::DELETE_LINE => self.with_doc(|d| d.delete_line()),
            ids::MATCHING_BRACKET => self.goto_matching_bracket(),
            ids::TOGGLE_BREAKPOINT => self.toggle_breakpoint(),
            ids::DEBUG_START => self.ensure_trust(super::trust::TrustIntent::DebugPreview),
            ids::DEBUG_ATTACH => self.open_prompt(
                PromptKind::DebugAttach,
                "Attach to process id or host:port",
                "e.g. 1234 or 127.0.0.1:5678",
            ),
            ids::DEBUG_CONTINUE => self.debug_continue(),
            ids::DEBUG_STEP_OVER => self.debug_step(crate::commands::DebugStep::Over),
            ids::DEBUG_STEP_INTO => self.debug_step(crate::commands::DebugStep::Into),
            ids::DEBUG_STEP_OUT => self.debug_step(crate::commands::DebugStep::Out),
            ids::DEBUG_PAUSE => self.debug_pause(),
            ids::DEBUG_STOP => self.debug_stop(),
            ids::DEBUG_STACK => self.debug_stack_picker(),
            ids::DEBUG_VARIABLES => self.debug_variables_picker(),
            ids::DEBUG_ADD_WATCH => self.open_prompt(
                PromptKind::DebugWatch,
                "Watch expression",
                "e.g. user.name or items.len()",
            ),
            ids::DEBUG_WATCHES => self.debug_watches_picker(),
            ids::DEBUG_SET_CONDITION => self.open_prompt(
                PromptKind::BreakpointCondition,
                "Breakpoint condition (empty clears)",
                "e.g. i > 10",
            ),
            ids::DEBUG_SET_LOGPOINT => self.open_prompt(
                PromptKind::BreakpointLog,
                "Logpoint message (empty clears)",
                "e.g. value is {value}",
            ),
            ids::TOGGLE_TREE => self.toggle_tree(),
            ids::FOCUS_TREE => self.focus_tree(),
            ids::TOGGLE_HIDDEN => self.toggle_hidden(),
            ids::TOGGLE_INLINE_DIAGNOSTICS => self.toggle_inline_diagnostics(),
            ids::TOGGLE_WRAP => self.toggle_wrap(),
            ids::FOLD_TOGGLE => self.toggle_fold(),
            ids::FOLD_CLOSE => self.fold_close(),
            ids::FOLD_OPEN => self.fold_open(),
            ids::FOLD_ALL => self.fold_all(),
            ids::UNFOLD_ALL => self.unfold_all(),
            ids::REFRESH => self.refresh_workspace(),
            ids::SPLIT => self.toggle_split(),
            ids::FOCUS_PANE => self.focus_other_pane(),
            ids::FILTER_TREE => self.open_tree_filter(),
            ids::NEXT_TAB => self.next_tab(),
            ids::PREV_TAB => self.previous_tab(),
            ids::PALETTE => self.open_command_palette(),
            ids::HELP => self.toggle_help(),
            ids::SETTINGS => self.open_settings(),
            ids::RENAME => self.rename_symbol(),
            ids::CODE_ACTIONS => self.code_actions(),
            ids::CODE_LENS => self.open_code_lens_picker(),
            ids::INCOMING_CALLS => self.incoming_calls(),
            ids::OUTGOING_CALLS => self.outgoing_calls(),
            ids::SUPERTYPES => self.supertypes(),
            ids::SUBTYPES => self.subtypes(),
            ids::FORMAT => self.format_document(),
            ids::GOTO_DEFINITION => self.goto_definition(),
            ids::IMPLEMENTATION => self.goto_implementation(),
            ids::TYPE_DEFINITION => self.goto_type_definition(),
            ids::DOCUMENT_LINKS => self.open_document_links(),
            ids::FIND_REFERENCES => self.find_references(),
            ids::SHOW_SYMBOLS => self.open_symbols(),
            ids::DIAGNOSTICS_NEXT => self.goto_diagnostic(1),
            ids::DIAGNOSTICS_PREV => self.goto_diagnostic(-1),
            ids::DIAGNOSTICS_LIST => self.open_diagnostics_list(),
            ids::HUNK_NEXT => self.goto_hunk(1),
            ids::HUNK_PREV => self.goto_hunk(-1),
            _ => self.panel_command(id),
        }
    }
    pub(super) fn open_command_palette(&mut self) {
        let items = self
            .commands
            .all()
            .iter()
            .map(|command| {
                let (enabled, hint) = self.command_availability(command);
                let mut item = PickerItem::new(
                    command.palette_label(),
                    command.description.to_string(),
                    PickerAction::Command(command.id),
                )
                .shortcut(command.shortcut.unwrap_or(""));
                if !enabled {
                    item = item.disabled(hint.unwrap_or_else(|| "unavailable".to_string()));
                }
                item
            })
            .collect();
        let mut picker = Picker::new("Command Palette", "Type a command…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Show which language tools Kemori found, and how to install the rest.
    pub(super) fn language_setup(&mut self) {
        let Some(tools) = self.tools.clone() else {
            // Discovery is still running; ask again and tell the user.
            self.background.discover_tools();
            self.set_status("Checking language tools…");
            return;
        };

        let mut items = Vec::new();
        for status in tools.all() {
            let tool = status.tool;
            let purpose = match tool.purpose() {
                crate::language::tools::ToolPurpose::LanguageServer => "language server",
                crate::language::tools::ToolPurpose::Formatter => "formatter",
                crate::language::tools::ToolPurpose::DebugAdapter => "debug adapter",
            };
            let label = format!("{}  ·  {} {purpose}", tool.label(), tool.language().name());
            let item = if status.available {
                // A managed tool shows its version and on-disk size, and can be
                // updated or removed. A user/system install is read-only.
                let detail = match status.managed_size() {
                    Some(bytes) => format!(
                        "{} · {}{}",
                        status.summary(),
                        crate::language::tools::human_bytes(bytes),
                        if status.is_managed() {
                            " · Kemori-managed"
                        } else {
                            ""
                        }
                    ),
                    None => status.summary(),
                };
                if status.is_managed() {
                    PickerItem::new(label, detail, PickerAction::ToolActions(tool))
                        .shortcut("Enter")
                } else {
                    PickerItem::new(label, detail.clone(), PickerAction::Info(detail))
                }
            } else if crate::language::tools::can_install(tool) {
                let hint = tool.setup_reason();
                PickerItem::new(label, hint, PickerAction::InstallTool(tool)).shortcut("Enter")
            } else {
                // Explain exactly why Kemori cannot install it, and what the user
                // can do instead, rather than repeating the generic install hint.
                let reason = tool.setup_reason();
                PickerItem::new(label, reason.clone(), PickerAction::Info(reason.clone()))
                    .disabled(reason)
            };
            items.push(item);
        }
        let mut picker = Picker::new("Language Setup", "Language tools…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }
    pub(super) fn language_support(&mut self) {
        let items: Vec<_> = crate::language::support::all()
            .into_iter()
            .map(|(language, class)| {
                let detail = class.to_string();
                PickerItem::new(
                    language.name().to_string(),
                    detail.clone(),
                    PickerAction::Info(detail),
                )
            })
            .collect();
        let mut picker = Picker::new("Language Support", "Language…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    pub(super) fn open_prompt(&mut self, kind: PromptKind, label: &str, placeholder: &str) {
        self.overlay = Overlay::Prompt(Prompt::new(kind, label, placeholder));
    }

    pub(super) fn toggle_tree(&mut self) {
        // `Ctrl+B` is the file panel: reveal and focus it, focus it when
        // another surface has focus, and hide it once it is focused.
        if !self.tree_visible {
            self.tree_visible = true;
            self.focus = Focus::FileTree;
        } else if self.focus == Focus::FileTree {
            self.tree_visible = false;
            self.focus = Focus::Editor;
        } else {
            self.focus = Focus::FileTree;
        }
    }

    pub(super) fn open_quick_open(&mut self) {
        let root = self.workspace.root().to_path_buf();
        let files = filesystem::collect_files(&root, 8000);

        // Recently-opened files come first, so Ctrl+P then Enter reopens the last
        // file without typing.
        let mut ordered: Vec<PathBuf> = Vec::new();
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        for path in self.recent_files.iter().rev() {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if !seen.insert(canonical) {
                continue;
            }
            if path.is_file() {
                ordered.push(path.clone());
            }
        }
        for path in files {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if seen.insert(canonical) {
                ordered.push(path);
            }
        }

        let items = ordered
            .into_iter()
            .map(|path| {
                let label = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let detail = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                PickerItem::new(label, detail, PickerAction::OpenPath(path))
            })
            .collect();
        let mut picker = Picker::new("Quick Open", "Type a file name…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Show the keyboard-shortcuts cheatsheet, or hide it if it is already up.
    pub(super) fn toggle_help(&mut self) {
        self.completion = None;
        self.hover = None;
        self.overlay = match self.overlay {
            Overlay::Help(_) => Overlay::None,
            _ => Overlay::Help(Help::default()),
        };
    }

    /// Re-read the project tree and git status on demand.
    pub(super) fn refresh_workspace(&mut self) {
        self.workspace.refresh();
        self.request_git_refresh();
        self.set_status("Refreshed");
    }
}
