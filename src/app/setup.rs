//! Tool installation, project setup plans and managed-tool actions.
//!
//! Extracted from `app/commands.rs`; see `docs/DECISIONS.md`.

use super::*;

use crate::language::setup::{
    ProjectSetupPlan, ProjectSetupRun, SetupState, SetupSummary, plan_project,
};

impl App {
    /// Install a tool through its trusted package manager, on the worker.
    pub(super) fn install_tool(&mut self, tool: Tool) {
        if self.pending_install.is_some() {
            self.set_status("An install is already running");
            return;
        }
        self.pending_install = Some(tool);
        self.background.install_tool(tool);
        self.set_status(format!("Installing {}…", tool.label()));
    }

    /// Offer project setup once per workspace, when there is something to say.
    ///
    /// Called from the interactive event loop, so it never blocks the first
    /// frame and never appears when Kemori is embedded (tests, previews). If the
    /// project needs nothing the flag is still set, so the offer is not repeated
    /// on every iteration.
    pub(super) fn maybe_offer_project_setup(&mut self) -> bool {
        if !self.overlay.is_none()
            || self.pending_install.is_some()
            || self.project_setup.is_some()
            || self.project_setup_offered
            || !self.engaged
        {
            return false;
        }
        let Some(languages) = self.project_languages.clone() else {
            return false; // the scan has not finished
        };
        if languages.is_empty() {
            self.project_setup_offered = true;
            return false;
        }
        let Some(tools) = self.tools.clone() else {
            return false; // wait for the tool probe
        };
        let plan = plan_project(&languages, &tools);
        self.project_setup_offered = true;
        if !plan.needs_setup() && !plan.has_attention() {
            return false; // every language is ready: nothing to report
        }
        self.show_project_setup(plan);
        true
    }

    /// Open the project setup summary on demand (palette, or after "Later").
    pub(super) fn open_project_setup(&mut self) {
        let Some(tools) = self.tools.clone() else {
            self.background.discover_tools();
            self.set_status("Checking language tools…");
            return;
        };
        let languages = self
            .project_languages
            .clone()
            .unwrap_or_else(|| self.fallback_languages());
        if languages.is_empty() {
            self.set_status("No project languages detected");
            return;
        }
        let plan = plan_project(&languages, &tools);
        self.show_project_setup(plan);
    }

    /// The languages to plan for when the project scan has not run yet: the
    /// active file's language plus the detected project kind.
    fn fallback_languages(&self) -> Vec<LanguageId> {
        let mut languages = Vec::new();
        if let Some(language) = self
            .editor
            .active_document()
            .map(|doc| doc.buffer.language)
            .filter(|language| *language != LanguageId::Unknown)
        {
            languages.push(language);
        }
        let project = self.workspace.project.kind.language();
        if project != LanguageId::Unknown && !languages.contains(&project) {
            languages.push(project);
        }
        languages
    }

    /// Show the setup summary as a picker: one row per language, then the
    /// action rows.
    pub(super) fn show_project_setup(&mut self, plan: ProjectSetupPlan) {
        let mut items = Vec::new();
        for entry in &plan.languages {
            let tag = match &entry.state {
                SetupState::Ready => "ready".to_string(),
                SetupState::NeedsInstall(_) => "install".to_string(),
                SetupState::Prerequisite(program) => format!("needs {program}"),
                SetupState::Unavailable(_) => "unavailable".to_string(),
            };
            let label = format!("{} · {tag}", entry.language.name());
            items.push(PickerItem::new(
                label,
                entry.detail.clone(),
                PickerAction::Info(entry.detail.clone()),
            ));
        }

        let missing = plan.installable_tools();
        if !missing.is_empty() {
            let noun = if missing.len() == 1 { "tool" } else { "tools" };
            items.push(
                PickerItem::new(
                    "Set up project",
                    format!("Install {} missing language {noun}", missing.len()),
                    PickerAction::ProjectSetup,
                )
                .shortcut("Enter"),
            );
        }
        items.push(PickerItem::new(
            "Later",
            "Set up this project from the command palette another time",
            PickerAction::Info(
                "Project setup postponed — run “Set Up Project…” any time".to_string(),
            ),
        ));

        let mut picker = Picker::new("Project setup", "Choose…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Begin provisioning every missing tool the project needs.
    ///
    /// Installs run one at a time through the existing provisioning path; the
    /// queue is planned from the current tool state, so a ready server is never
    /// reinstalled.
    pub(super) fn start_project_setup(&mut self) {
        if self.pending_install.is_some() || self.project_setup.is_some() {
            self.set_status("An install is already running");
            return;
        }
        let Some(languages) = self.project_languages.clone() else {
            self.set_status("No project languages detected");
            return;
        };
        let Some(tools) = self.tools.clone() else {
            self.background.discover_tools();
            self.set_status("Checking language tools…");
            return;
        };
        let plan = plan_project(&languages, &tools);
        let queue: std::collections::VecDeque<Tool> = plan.installable_tools().into();
        if queue.is_empty() {
            self.set_status("Project is already set up");
            return;
        }
        self.project_setup = Some(ProjectSetupRun {
            plan,
            queue,
            installed: Vec::new(),
            failed: Vec::new(),
        });
        self.advance_project_setup();
    }

    /// Start the next queued install, or finish the run.
    pub(super) fn advance_project_setup(&mut self) {
        let next = self
            .project_setup
            .as_mut()
            .and_then(|run| run.queue.pop_front());
        match next {
            Some(tool) => {
                self.pending_install = Some(tool);
                self.background.install_tool(tool);
                self.set_status(format!("Setting up project — installing {}…", tool.label()));
            }
            None => {
                if self.project_setup.is_some() {
                    self.finish_project_setup();
                }
            }
        }
    }

    /// Report the outcome of a project setup run, distinguishing full success,
    /// partial success and outstanding attention.
    fn finish_project_setup(&mut self) {
        let Some(run) = self.project_setup.take() else {
            return;
        };
        let failed: Vec<Tool> = run.failed.iter().map(|(tool, _)| *tool).collect();
        let summary = SetupSummary::from_run(&run.plan, &run.installed, &failed);
        let headline = summary.headline();
        let kind = if summary.failed == 0 {
            ToastKind::Success
        } else {
            ToastKind::Error
        };
        self.set_status(headline.clone());
        self.push_toast(kind, headline);
        // Re-probe so Language Setup and the next plan reflect the new state.
        self.background.discover_tools();
    }

    /// Offer, once per language, to install a missing language server.
    ///
    /// This is called from the interactive event loop rather than at startup, so
    /// the prompt never blocks the first frame and never appears when Kemori is
    /// embedded (tests, previews). The user always chooses; dismissing it keeps
    /// Kemori's built-in intelligence, and **Language Setup…** stays available.
    pub(super) fn maybe_offer_tool_setup(&mut self) -> bool {
        if !self.overlay.is_none()
            || self.pending_install.is_some()
            || self.project_setup.is_some()
            || !self.lsp.is_empty()
        {
            return false;
        }
        let Some(tools) = self.tools.as_ref() else {
            return false;
        };
        // Prefer the active file's language; with no file open, use the
        // project's detected language so opening a project is enough to be
        // offered its tooling.
        let language = self
            .editor
            .active_document()
            .map(|doc| doc.buffer.language)
            .filter(|language| *language != LanguageId::Unknown)
            .unwrap_or_else(|| self.workspace.project.kind.language());
        if language == LanguageId::Unknown {
            return false;
        }
        // Project setup is the primary entry point for a project's languages, so
        // do not also prompt for a language the project plan already covers.
        if self
            .project_languages
            .as_ref()
            .is_some_and(|languages| languages.contains(&language))
        {
            return false;
        }
        // An available server means there is nothing to offer; otherwise offer
        // the first installable candidate.
        if Tool::available_server(language, tools).is_some() {
            return false;
        }
        let Some(tool) = Tool::installable_server(language, tools) else {
            return false;
        };
        if !self.setup_offered.insert(language) {
            return false;
        }

        let install = PickerItem::new(
            format!("Install {}", tool.label()),
            tool.setup_reason(),
            PickerAction::InstallTool(tool),
        )
        .shortcut("Enter");
        let later = PickerItem::new(
            "Not now",
            "Keep Kemori's built-in intelligence — install later from Language Setup…",
            PickerAction::Info("Install language tools any time from Language Setup…".to_string()),
        );
        let mut picker = Picker::new(
            format!("{} is not installed", tool.label()),
            "Choose…",
            vec![install, later],
        );
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
        true
    }

    /// Offer Update and Remove for a Kemori-managed tool.
    pub(super) fn tool_actions(&mut self, tool: Tool) {
        let Some(status) = self
            .tools
            .as_ref()
            .and_then(|tools| tools.status(tool))
            .cloned()
        else {
            self.set_status("Tool status is unavailable");
            return;
        };
        if !status.is_managed() {
            self.set_status(format!("{} is not managed by Kemori", tool.label()));
            return;
        }
        let size = status
            .managed_size()
            .map(|bytes| format!(" ({})", crate::language::tools::human_bytes(bytes)))
            .unwrap_or_default();
        let update = PickerItem::new(
            format!("Update {}", tool.label()),
            format!("Reinstall Kemori's pinned version{size}"),
            PickerAction::UpdateTool(tool),
        )
        .shortcut("Enter");
        let remove = PickerItem::new(
            format!("Remove {}", tool.label()),
            format!("Delete Kemori's managed files{size}"),
            PickerAction::RemoveTool(tool),
        );
        let cancel = PickerItem::new(
            "Cancel",
            "Change nothing",
            PickerAction::Info("Nothing changed".to_string()),
        );
        let mut picker = Picker::new(
            format!("{} (Kemori-managed)", tool.label()),
            "Choose…",
            vec![update, remove, cancel],
        );
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Remove a Kemori-managed tool's files.
    ///
    /// Refuses to touch anything Kemori does not own, and never removes the shared
    /// tools root itself. A `discover` afterwards refreshes the setup view.
    pub(super) fn remove_tool(&mut self, tool: Tool) {
        if self.pending_install.is_some() {
            self.set_status("An install is already running");
            return;
        }
        let Some(status) = self
            .tools
            .as_ref()
            .and_then(|tools| tools.status(tool))
            .cloned()
        else {
            self.set_status("Tool status is unavailable");
            return;
        };
        if !status.is_managed() {
            self.set_error(format!(
                "{} is not managed by Kemori — not removing it",
                tool.label()
            ));
            return;
        }
        let Some(dir) = status.managed_dir() else {
            self.set_error(format!("Could not locate {}'s files", tool.label()));
            return;
        };
        if crate::language::tools::tools_dir().is_some_and(|root| dir == root) {
            self.set_error("Refusing to remove the shared tools directory");
            return;
        }
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {
                self.push_toast(ToastKind::Info, format!("Removed {}", tool.label()));
                self.background.discover_tools();
            }
            Err(err) => self.set_error(format!("Could not remove {}: {err}", tool.label())),
        }
    }
}
