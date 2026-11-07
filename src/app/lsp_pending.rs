//! In-flight LSP request bookkeeping.

use super::*;

impl App {
    /// Forget the in-flight request bookkeeping for one feature.
    pub(super) fn clear_lsp_pending_for(&mut self, kind: RequestKind) {
        match kind {
            RequestKind::Completion => self.completion_request = None,
            RequestKind::Hover => self.hover_request = None,
            RequestKind::SignatureHelp => self.signature_request = None,
            RequestKind::Definition | RequestKind::Implementation | RequestKind::TypeDefinition => {
                self.pending_definition = None
            }
            RequestKind::DocumentLink => self.pending_document_links = None,
            RequestKind::References => self.pending_references = None,
            RequestKind::Rename => self.pending_rename_request = None,
            RequestKind::CodeActions => self.pending_code_action_request = None,
            RequestKind::OrganizeImports => self.pending_organize_imports = None,
            RequestKind::RangeFormatting => self.pending_range_format = None,
            RequestKind::Formatting => self.pending_lsp_format = None,
            RequestKind::FoldingRange => self.pending_folding = None,
            RequestKind::DocumentHighlight => self.pending_highlights = None,
            RequestKind::InlayHint => self.pending_inlay_hints = None,
            RequestKind::CodeLens => self.pending_code_lens = None,
            RequestKind::SelectionRange => self.pending_selection_ranges = None,
            RequestKind::PrepareCallHierarchy | RequestKind::PrepareTypeHierarchy => {
                self.pending_hierarchy_prepare = None;
            }
            RequestKind::CallHierarchyIncoming
            | RequestKind::CallHierarchyOutgoing
            | RequestKind::TypeHierarchySuper
            | RequestKind::TypeHierarchySub => self.pending_hierarchy = None,
            RequestKind::WorkspaceSymbols => self.ws_lsp_pending = false,
            RequestKind::DocumentDiagnostics => self.pending_document_diagnostics.clear(),
            RequestKind::WorkspaceDiagnostics => {
                self.pending_workspace_diagnostics.clear();
            }
            RequestKind::ExecuteCommand => self.pending_java_debug = None,
        }
    }
}
