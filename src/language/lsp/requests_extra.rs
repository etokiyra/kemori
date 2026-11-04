//! Extra document requests kept out of the main module.

use super::*;

impl Server {
    /// Ask the server for the document highlights at a position.
    pub fn document_highlight(&mut self, path: &Path, line: usize, col: usize) -> Option<i64> {
        self.send_request(
            RequestKind::DocumentHighlight,
            "textDocument/documentHighlight",
            position_params(path, line, col),
        )
        .ok()
    }

    /// Ask the server for inlay hints over a document range.
    pub fn inlay_hints(&mut self, path: &Path, end_line: usize, end_col: usize) -> Option<i64> {
        let params = json!({
            "textDocument": { "uri": path_to_uri(path) },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": end_line, "character": end_col }
            }
        });
        self.send_request(RequestKind::InlayHint, "textDocument/inlayHint", params)
            .ok()
    }

    /// Ask the server for code lenses over a document.
    pub fn code_lenses(&mut self, path: &Path) -> Option<i64> {
        let params = json!({ "textDocument": { "uri": path_to_uri(path) } });
        self.send_request(RequestKind::CodeLens, "textDocument/codeLens", params)
            .ok()
    }

    /// Ask the server for the selection-range chain at a position.
    pub fn selection_ranges(&mut self, path: &Path, line: usize, col: usize) -> Option<i64> {
        let params = json!({
            "textDocument": { "uri": path_to_uri(path) },
            "positions": [ { "line": line, "character": col } ]
        });
        self.send_request(
            RequestKind::SelectionRange,
            "textDocument/selectionRange",
            params,
        )
        .ok()
    }

    /// Prepare a call hierarchy at a position.
    pub fn prepare_call_hierarchy(&mut self, path: &Path, line: usize, col: usize) -> Option<i64> {
        self.send_request(
            RequestKind::PrepareCallHierarchy,
            "textDocument/prepareCallHierarchy",
            position_params(path, line, col),
        )
        .ok()
    }

    /// Ask for the calls into or out of a prepared item.
    pub fn call_hierarchy(&mut self, item: &Value, incoming: bool) -> Option<i64> {
        let (kind, method) = if incoming {
            (
                RequestKind::CallHierarchyIncoming,
                "callHierarchy/incomingCalls",
            )
        } else {
            (
                RequestKind::CallHierarchyOutgoing,
                "callHierarchy/outgoingCalls",
            )
        };
        self.send_request(kind, method, json!({ "item": item }))
            .ok()
    }

    /// Prepare a type hierarchy at a position.
    pub fn prepare_type_hierarchy(&mut self, path: &Path, line: usize, col: usize) -> Option<i64> {
        self.send_request(
            RequestKind::PrepareTypeHierarchy,
            "textDocument/prepareTypeHierarchy",
            position_params(path, line, col),
        )
        .ok()
    }

    /// Ask for an item's supertypes or subtypes.
    pub fn type_hierarchy(&mut self, item: &Value, supertypes: bool) -> Option<i64> {
        let (kind, method) = if supertypes {
            (RequestKind::TypeHierarchySuper, "typeHierarchy/supertypes")
        } else {
            (RequestKind::TypeHierarchySub, "typeHierarchy/subtypes")
        };
        self.send_request(kind, method, json!({ "item": item }))
            .ok()
    }

    /// Ask for implementations of the symbol at a position.
    pub fn implementation(&mut self, path: &Path, line: usize, col: usize) -> Option<i64> {
        self.send_request(
            RequestKind::Implementation,
            "textDocument/implementation",
            position_params(path, line, col),
        )
        .ok()
    }

    /// Ask for the type definition at a position.
    pub fn type_definition(&mut self, path: &Path, line: usize, col: usize) -> Option<i64> {
        self.send_request(
            RequestKind::TypeDefinition,
            "textDocument/typeDefinition",
            position_params(path, line, col),
        )
        .ok()
    }

    /// Ask the server for document links.
    pub fn document_links(&mut self, path: &Path) -> Option<i64> {
        let params = json!({ "textDocument": { "uri": path_to_uri(path) } });
        self.send_request(
            RequestKind::DocumentLink,
            "textDocument/documentLink",
            params,
        )
        .ok()
    }

    /// Ask for pull diagnostics for one document.
    ///
    /// Only meaningful when the server advertised a `diagnosticProvider`; the
    /// caller checks [`Server::supports`] first so a push-only server is never
    /// sent a request it cannot answer.
    pub fn document_diagnostics(&mut self, path: &Path) -> Option<i64> {
        let params = json!({ "textDocument": { "uri": path_to_uri(path) } });
        self.send_request(
            RequestKind::DocumentDiagnostics,
            "textDocument/diagnostic",
            params,
        )
        .ok()
    }

    /// Ask for a workspace-wide pull-diagnostics report.
    pub fn workspace_diagnostics(&mut self) -> Option<i64> {
        self.send_request(
            RequestKind::WorkspaceDiagnostics,
            "workspace/diagnostic",
            json!({ "previousResultIds": [] }),
        )
        .ok()
    }

    /// Whether the server advertised support for `kind`.
    pub fn supports(&self, kind: RequestKind) -> bool {
        match kind {
            RequestKind::Completion => self.capabilities.completion,
            RequestKind::Hover => self.capabilities.hover,
            RequestKind::Definition => self.capabilities.definition,
            RequestKind::Implementation => self.capabilities.implementation,
            RequestKind::TypeDefinition => self.capabilities.type_definition,
            RequestKind::DocumentLink => self.capabilities.document_link,
            RequestKind::References => self.capabilities.references,
            RequestKind::Rename => self.capabilities.rename,
            RequestKind::CodeActions => self.capabilities.code_action,
            RequestKind::WorkspaceSymbols => self.capabilities.workspace_symbol,
            RequestKind::SignatureHelp => self.capabilities.signature_help,
            RequestKind::Formatting => self.capabilities.document_formatting,
            RequestKind::FoldingRange => self.capabilities.folding_range,
            RequestKind::DocumentHighlight => self.capabilities.document_highlight,
            RequestKind::InlayHint => self.capabilities.inlay_hint,
            RequestKind::CodeLens => self.capabilities.code_lens,
            RequestKind::SelectionRange => self.capabilities.selection_range,
            RequestKind::PrepareCallHierarchy
            | RequestKind::CallHierarchyIncoming
            | RequestKind::CallHierarchyOutgoing => self.capabilities.call_hierarchy,
            RequestKind::PrepareTypeHierarchy
            | RequestKind::TypeHierarchySuper
            | RequestKind::TypeHierarchySub => self.capabilities.type_hierarchy,
            RequestKind::DocumentDiagnostics => self.capabilities.document_diagnostics,
            RequestKind::WorkspaceDiagnostics => self.capabilities.workspace_diagnostics,
            // JDT LS always supports `workspace/executeCommand`; the app only
            // uses it for its own Java debug commands.
            RequestKind::ExecuteCommand => true,
        }
    }
}
