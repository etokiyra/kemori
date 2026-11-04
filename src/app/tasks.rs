//! Extracted from `app/mod.rs`: see `docs/DECISIONS.md`.
//!
//! Run and test tasks: the project's own commands, detected from its markers
//! and run with bounded output — no configuration, no per-project files.

use super::*;
use crate::app::panel::{PanelTab, TaskRunState, task_problems};
use crate::background::TaskRunOutcome;
use crate::project::tasks::{self, Task};

impl App {
    /// Open the task picker for the current project.
    pub(super) fn open_task_picker(&mut self) {
        let root = self.workspace.root().to_path_buf();
        let tasks = tasks::detect(&root);
        if tasks.is_empty() {
            self.set_status("No tasks detected for this project");
            return;
        }
        let items = tasks
            .into_iter()
            .map(|task| {
                PickerItem::new(
                    task.label.clone(),
                    task.command_line(),
                    PickerAction::RunTask(task),
                )
                .shortcut("Enter")
            })
            .collect();
        let mut picker = Picker::new("Tasks", "Run a project task…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Start a task on the background worker.
    ///
    /// One task at a time: the live output would otherwise be ambiguous, and a
    /// second run is rejected while the first is still alive.
    pub(super) fn run_task(&mut self, task: Task) {
        if self.pending_task {
            self.set_status("A task is already running");
            return;
        }
        self.pending_task = true;
        self.last_task = Some(task.clone());
        self.panel.task_problems.clear();
        // The previous result moves to history rather than disappearing when
        // the next task starts.
        if let Some(previous) = self.panel.output.take()
            && previous.outcome != "running"
        {
            self.panel.history.push(previous);
            if self.panel.history.len() > 10 {
                self.panel.history.remove(0);
            }
        }
        // Start the Output tab immediately so streaming is visible from the
        // first chunk instead of arriving only when the task exits.
        self.panel.output = Some(TaskRunState::from_output(
            self.workspace.root(),
            task.label.clone(),
            &task.command_line(),
            "running",
            true,
            "",
        ));
        self.open_panel(PanelTab::Output);
        self.background
            .run_task(self.workspace.root().to_path_buf(), task.clone());
        self.set_status(format!("Running {}…", task.command_line()));
    }

    /// Append a live output chunk to the Output tab.
    pub(super) fn append_task_output(&mut self, text: &str) {
        if let Some(run) = self.panel.output.as_mut() {
            run.append(text);
        }
    }

    /// Run the active file with its language's zero-config runner.
    pub(super) fn run_active_file(&mut self) {
        let Some(doc) = self.editor.active_document() else {
            self.set_status("No file open");
            return;
        };
        if doc.is_dirty() {
            self.set_error("Save the file before running it");
            return;
        }
        let language = doc.buffer.language;
        let Some(path) = doc.buffer.path.clone() else {
            self.set_status("This file has no path on disk");
            return;
        };
        let Some(task) = tasks::active_file_task(language, &path) else {
            self.set_error(format!(
                "No zero-config runner for {} files",
                language.name()
            ));
            return;
        };
        self.ensure_trust(super::trust::TrustIntent::RunTask(task));
    }

    /// Cancel the running task.
    pub(super) fn cancel_task(&mut self) {
        if !self.pending_task {
            self.set_status("No task is running");
            return;
        }
        self.background.cancel_task();
        self.set_status("Cancelling task…");
    }

    /// Run the last task again.
    pub(super) fn rerun_task(&mut self) {
        if self.pending_task {
            self.set_status("A task is already running");
            return;
        }
        match self.last_task.clone() {
            Some(task) => self.ensure_trust(super::trust::TrustIntent::RunTask(task)),
            None => self.set_status("No task to rerun"),
        }
    }

    /// Open the completed task outputs from this session.
    pub(super) fn open_task_history(&mut self) {
        if self.panel.history.is_empty() {
            self.set_status("No task history yet");
            return;
        }
        let items = self
            .panel
            .history
            .iter()
            .enumerate()
            .rev()
            .map(|(index, run)| {
                let detail = run
                    .lines
                    .first()
                    .cloned()
                    .unwrap_or_else(|| run.command.clone());
                PickerItem::new(
                    format!("{} · {}", run.title, run.outcome),
                    detail,
                    PickerAction::ShowTaskOutput(index),
                )
            })
            .collect();
        let mut picker = Picker::new("Task Output History", "Choose a task run…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Show a historical task output in the Output tab.
    pub(super) fn show_task_history(&mut self, index: usize) {
        let Some(run) = self.panel.history.get(index).cloned() else {
            return;
        };
        self.panel.output = Some(run);
        self.open_panel(PanelTab::Output);
    }

    /// Show the final result of a finished task.
    pub(super) fn show_task_run(&mut self, outcome: TaskRunOutcome) {
        let TaskRunOutcome {
            task,
            exit_code,
            timed_out,
            spawn_error,
            cancelled,
            text,
            truncated,
        } = outcome;
        if let Some(error) = spawn_error {
            self.panel.output = Some(TaskRunState::from_output(
                self.workspace.root(),
                task.label.clone(),
                &task.command_line(),
                "not started",
                false,
                &error,
            ));
            self.open_panel(PanelTab::Output);
            self.set_error(error.clone());
            self.push_toast(ToastKind::Error, error);
            return;
        }
        let (success, verdict) = if cancelled {
            (false, "cancelled".to_string())
        } else if timed_out {
            (false, "killed — ran past the deadline".to_string())
        } else if exit_code == Some(0) {
            (true, "success".to_string())
        } else {
            match exit_code {
                Some(code) => (false, format!("exit code {code}")),
                None => (false, "terminated by a signal".to_string()),
            }
        };
        let text = if truncated {
            format!("{}\n… output truncated at the cap", text.trim_end())
        } else if text.is_empty() {
            "(no output)".to_string()
        } else {
            text
        };
        self.panel.task_problems = task_problems(self.workspace.root(), &text);
        let follow = self.panel.output.as_ref().is_some_and(|run| run.follow);
        let mut run = TaskRunState::from_output(
            self.workspace.root(),
            task.label.clone(),
            &task.command_line(),
            &verdict,
            success,
            &text,
        );
        if follow {
            run.select_by(i64::MAX);
        }
        self.panel.output = Some(run);
        self.open_panel(PanelTab::Output);
        self.set_status(format!("{}: {verdict}", task.label));
    }
}
