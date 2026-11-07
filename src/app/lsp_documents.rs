//! Document open/close notifications for language servers.

use super::*;

impl App {
    /// Tell a language's server that a document was saved.
    pub(super) fn notify_lsp_save(&mut self, path: &Path, language: LanguageId, text: &str) {
        if let Some(server) = self
            .lsp
            .get_mut(&(self.workspace.root().to_path_buf(), language))
            .and_then(|job| job.server.as_mut())
        {
            server.did_save(path, text);
        }
    }

    /// Send watched-file changes to every ready server that registered interest.
    pub(super) fn notify_lsp_watched(
        &mut self,
        changes: &[(PathBuf, crate::language::lsp::WatchedFileChange)],
    ) {
        if changes.is_empty() {
            return;
        }
        let root = self.workspace.root().to_path_buf();
        for ((workspace, _), job) in self.lsp.iter_mut() {
            if *workspace != root {
                continue;
            }
            if let Some(server) = job.server.as_mut()
                && server.is_ready()
            {
                server.did_change_watched_files(changes);
            }
        }
    }

    /// Tell a language's server that a document was closed, if it had it open.
    pub(super) fn notify_lsp_close(&mut self, path: &Path, language: LanguageId) {
        if let Some(server) = self
            .lsp
            .get_mut(&(self.workspace.root().to_path_buf(), language))
            .and_then(|job| job.server.as_mut())
        {
            server.did_close(path);
        }
    }
}
