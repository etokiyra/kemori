//! Server progress state for the statusline.

use super::*;
use crate::language::lsp::{LspProgress, ProgressKind};

impl App {
    /// Apply one `$/progress` update.
    pub(super) fn apply_lsp_progress(&mut self, progress: LspProgress) {
        match progress.kind {
            ProgressKind::End => {
                self.lsp_progress.remove(&progress.token);
            }
            ProgressKind::Begin => {
                self.lsp_progress.insert(progress.token.clone(), progress);
            }
            ProgressKind::Report => {
                if let Some(existing) = self.lsp_progress.get_mut(&progress.token) {
                    if !progress.title.is_empty() {
                        existing.title = progress.title;
                    }
                    if !progress.message.is_empty() {
                        existing.message = progress.message;
                    }
                    if progress.percentage.is_some() {
                        existing.percentage = progress.percentage;
                    }
                    existing.kind = ProgressKind::Report;
                } else {
                    self.lsp_progress.insert(progress.token.clone(), progress);
                }
            }
        }
    }
}
