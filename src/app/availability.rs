//! Command availability: whether the palette can run a command, and why not.
//!
//! Extracted from `app/commands.rs`; see `docs/DECISIONS.md`.

use super::*;

impl App {
    /// Whether a command can run right now, and why not when it cannot.
    pub(super) fn command_availability(&self, command: &Command) -> (bool, Option<String>) {
        let document = self.editor.active_document();
        if command.needs_doc && document.is_none() {
            return (false, Some("no file open".to_string()));
        }
        if let Some(capability) = command.capability {
            let language = document
                .map(|doc| doc.buffer.language)
                .unwrap_or(LanguageId::Unknown);
            // A server-backed command works through the attached server even
            // when the provider declares no built-in capability, so an
            // advertised request overrides the provider gate. Without this the
            // palette under-reported rename and code actions for every
            // language whose server provides them.
            let served_by_server = command
                .server_request()
                .is_some_and(|kind| self.lsp_supports(kind));
            if !served_by_server
                && !self
                    .language
                    .provider(language)
                    .capabilities()
                    .contains(&capability)
            {
                return (
                    false,
                    Some(format!("not available for {}", language.name())),
                );
            }
        }
        match command.id {
            ids::UNDO if !document.is_some_and(|doc| doc.can_undo()) => {
                (false, Some("nothing to undo".to_string()))
            }
            ids::REDO if !document.is_some_and(|doc| doc.can_redo()) => {
                (false, Some("nothing to redo".to_string()))
            }
            ids::COPY | ids::CUT if !document.is_some_and(|doc| doc.has_selection()) => {
                (false, Some("nothing selected".to_string()))
            }
            ids::YANK_POP if self.last_yank.is_none() => {
                (false, Some("nothing to yank".to_string()))
            }
            ids::REPLACE_ALL if !self.search.open || self.search.query.is_empty() => {
                (false, Some("open find first".to_string()))
            }
            ids::NEXT_TAB | ids::PREV_TAB if self.editor.len() < 2 => {
                (false, Some("only one tab".to_string()))
            }
            ids::CLOSE_TAB | ids::CLOSE_ALL | ids::SAVE_ALL if self.editor.is_empty() => {
                (false, Some("no files open".to_string()))
            }
            ids::REVERT if !document.is_some_and(|doc| doc.is_dirty()) => {
                (false, Some("no changes to revert".to_string()))
            }
            ids::RENAME_FILE | ids::DELETE_FILE if self.file_op_target().is_none() => {
                (false, Some("select a file first".to_string()))
            }
            ids::DUPLICATE_FILE | ids::COPY_FILE if self.file_op_target().is_none() => {
                (false, Some("select a file first".to_string()))
            }
            ids::GIT_COMMIT if !self.workspace.git.available => {
                (false, Some("not a git repository".to_string()))
            }
            ids::RUN_TASKS if crate::project::tasks::detect(self.workspace.root()).is_empty() => (
                false,
                Some("no tasks detected for this project".to_string()),
            ),
            ids::CANCEL_TASK if !self.pending_task => {
                (false, Some("no task is running".to_string()))
            }
            ids::RUN_ACTIVE_FILE => match document {
                Some(doc) if doc.is_dirty() => (false, Some("save the file first".to_string())),
                Some(doc) => match doc.buffer.path.as_deref().and_then(|path| {
                    crate::project::tasks::active_file_task(doc.buffer.language, path)
                }) {
                    Some(_) => (true, None),
                    None => (
                        false,
                        Some(format!(
                            "no runner for {} files",
                            doc.buffer.language.name()
                        )),
                    ),
                },
                None => (false, Some("no file open".to_string())),
            },
            ids::RERUN_TASK if self.last_task.is_none() => {
                (false, Some("run a task first".to_string()))
            }
            ids::TASK_HISTORY if self.panel.history.is_empty() => {
                (false, Some("no task history yet".to_string()))
            }
            ids::REFRESH_PROBLEMS if self.pending_project_diagnostics.is_some() => {
                (false, Some("a project scan is already running".to_string()))
            }
            ids::GIT_COMMIT if self.workspace.git.files.is_empty() => {
                (false, Some("nothing to commit".to_string()))
            }
            ids::GIT_TOGGLE_STAGE if !self.workspace.git.available => {
                (false, Some("not a git repository".to_string()))
            }
            ids::GIT_TOGGLE_STAGE if self.file_op_target().is_none() => {
                (false, Some("select a file first".to_string()))
            }
            ids::DIAGNOSTICS_NEXT | ids::DIAGNOSTICS_PREV
                if !document.is_some_and(|doc| !doc.diagnostics().is_empty()) =>
            {
                (false, Some("no diagnostics".to_string()))
            }
            ids::DIAGNOSTICS_LIST
                if !self
                    .editor
                    .documents
                    .iter()
                    .any(|doc| !doc.diagnostics().is_empty()) =>
            {
                (false, Some("no diagnostics".to_string()))
            }
            ids::FORMAT => self.format_availability(document),
            ids::RENAME if !self.lsp_supports(RequestKind::Rename) => {
                (false, Some("needs a language server".to_string()))
            }
            ids::CODE_ACTIONS if !self.lsp_supports(RequestKind::CodeActions) => {
                (false, Some("needs a language server".to_string()))
            }
            _ => (true, None),
        }
    }

    /// Whether formatting can run, and why not when it cannot.
    pub(super) fn format_availability(
        &self,
        document: Option<&Document>,
    ) -> (bool, Option<String>) {
        // A language server may format even when the provider has no formatter.
        if self.lsp_supports(RequestKind::Formatting) {
            return (true, None);
        }
        let language = document
            .map(|doc| doc.buffer.language)
            .unwrap_or(LanguageId::Unknown);
        let Some(tool) =
            Tool::for_language(language, crate::language::tools::ToolPurpose::Formatter)
        else {
            return (true, None);
        };
        // Prefer the probed registry; fall back to a PATH check before it lands.
        let available = match &self.tools {
            Some(tools) => tools.available(tool),
            None => format::is_available(tool.program()),
        };
        if available {
            (true, None)
        } else {
            (false, Some(format!("{} is not installed", tool.program())))
        }
    }
}
