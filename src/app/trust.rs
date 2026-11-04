//! The workspace-trust gate.
//!
//! Opening a repository and *executing* it are different levels of trust. Kemori
//! always opens a project read-only-safe: detection, editing and language
//! intelligence work without asking. Running its tasks, starting its debug
//! adapter or opening a shell in it executes project code, so the first such
//! action asks once and remembers the answer for that root.
//!
//! The decision lives in Kemori's state directory (see [`crate::trust`]), never
//! in the project, so a cloned repository cannot grant itself trust.

use super::*;
use crate::app::overlay::PickerAction;
use crate::project::tasks::Task;

/// What the user was trying to run when the trust gate stopped them.
#[derive(Clone, Debug)]
pub(super) enum TrustIntent {
    /// Run a task detected from the project.
    RunTask(Task),
    /// Open an interactive shell rooted at the project.
    OpenTerminal,
    /// Start the debug-adapter handshake.
    DebugPreview,
}

/// The answer to the trust prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustDecision {
    /// Trust the project and remember it.
    Trust,
    /// Run this one action without remembering the decision.
    Once,
    /// Do not run anything.
    Cancel,
}

impl App {
    /// Whether the current workspace root is trusted for execution.
    pub fn workspace_trusted(&self) -> bool {
        self.trust.is_trusted(self.workspace.root())
    }

    /// Run `intent`, asking for workspace trust first when necessary.
    pub(super) fn ensure_trust(&mut self, intent: TrustIntent) {
        if self.workspace_trusted() {
            self.perform_trusted(intent);
            return;
        }
        self.pending_trust = Some(intent);
        let root = self.workspace.root().display().to_string();
        let items = vec![
            PickerItem::new(
                "Trust this project",
                format!("Run project code in {root} from now on"),
                PickerAction::ResolveTrust(TrustDecision::Trust),
            )
            .shortcut("Enter"),
            PickerItem::new(
                "Run once",
                "Run this action now without remembering the decision",
                PickerAction::ResolveTrust(TrustDecision::Once),
            ),
            PickerItem::new(
                "Cancel",
                "Keep the project untrusted; editing still works",
                PickerAction::ResolveTrust(TrustDecision::Cancel),
            ),
        ];
        let mut picker = Picker::new(
            "Workspace Trust",
            "This project can run code on your machine…",
            items,
        );
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Apply the user's answer to the trust prompt.
    pub(super) fn resolve_trust(&mut self, decision: TrustDecision) {
        self.overlay = Overlay::None;
        let intent = self.pending_trust.take();
        match decision {
            TrustDecision::Trust => {
                self.trust.trust(self.workspace.root());
                self.set_status("Workspace trusted for this project");
                if let Some(intent) = intent {
                    self.perform_trusted(intent);
                }
            }
            TrustDecision::Once => {
                if let Some(intent) = intent {
                    self.perform_trusted(intent);
                }
            }
            TrustDecision::Cancel => self.set_status("Workspace left untrusted"),
        }
    }

    /// Explicitly trust the current workspace from the palette.
    pub(super) fn trust_workspace(&mut self) {
        self.trust.trust(self.workspace.root());
        self.set_status("Workspace trusted for this project");
    }

    /// Forget the current workspace's trust decision.
    pub(super) fn revoke_workspace_trust(&mut self) {
        self.trust.revoke(self.workspace.root());
        self.set_status("Workspace trust revoked");
    }

    /// Execute an action that has already passed the trust gate.
    fn perform_trusted(&mut self, intent: TrustIntent) {
        match intent {
            TrustIntent::RunTask(task) => self.run_task(task),
            TrustIntent::OpenTerminal => self.open_terminal_trusted(),
            TrustIntent::DebugPreview => self.debug_start(),
        }
    }
}
