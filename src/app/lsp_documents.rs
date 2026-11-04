//! Document open/close notifications for language servers.

use super::*;

impl App {
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
