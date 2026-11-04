//! Document-highlight requests and cursor tracking.

use super::*;

impl App {
    /// Request document highlights for the symbol at the active cursor.
    ///
    /// Called after cursor movement and edits. A newer request replaces the
    /// pending id; the response handler ignores anything stale. Documents
    /// without a ready server lose their old highlights so a previous symbol's
    /// ranges cannot linger.
    pub(super) fn request_document_highlights(&mut self) {
        let active = self.editor.active_document().and_then(|doc| {
            if doc.is_large_file() {
                return None;
            }
            Some((
                doc.buffer.path.clone()?,
                doc.buffer.language,
                doc.clamped_cursor(),
                doc.buffer.version,
            ))
        });
        let Some((path, language, cursor, version)) = active else {
            return;
        };
        if !self.lsp_supports(RequestKind::DocumentHighlight) {
            if let Some(doc) = self
                .editor
                .documents
                .iter_mut()
                .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
            {
                doc.clear_document_highlights();
            }
            return;
        }
        let col = self.lsp_col(language, &path, cursor.row, cursor.col);
        let id = self
            .lsp
            .get_mut(&(self.workspace.root().to_path_buf(), language))
            .and_then(|job| job.server.as_mut())
            .and_then(|server| server.document_highlight(&path, cursor.row, col));
        match id {
            Some(id) => {
                self.pending_highlights = Some(PendingDocRequest {
                    language,
                    id,
                    path,
                    version,
                });
            }
            None => {
                if let Some(doc) = self
                    .editor
                    .documents
                    .iter_mut()
                    .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
                {
                    doc.clear_document_highlights();
                }
            }
        }
    }
}
