//! Extra document requests kept out of the main module.

use super::watched::WatchedFiles;
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

    /// Ask the server to organize imports over a document range.
    pub fn organize_imports(
        &mut self,
        path: &Path,
        start: (usize, usize),
        end: (usize, usize),
    ) -> Option<i64> {
        let params = json!({
            "textDocument": { "uri": path_to_uri(path) },
            "range": {
                "start": { "line": start.0, "character": start.1 },
                "end": { "line": end.0, "character": end.1 }
            },
            "context": {
                "diagnostics": [],
                "only": [ "source.organizeImports" ]
            }
        });
        self.send_request(
            RequestKind::OrganizeImports,
            "textDocument/codeAction",
            params,
        )
        .ok()
    }

    /// Ask the server to format only a range.
    pub fn range_formatting(
        &mut self,
        path: &Path,
        start: (usize, usize),
        end: (usize, usize),
        tab_size: usize,
        insert_spaces: bool,
    ) -> Option<i64> {
        let params = json!({
            "textDocument": { "uri": path_to_uri(path) },
            "range": {
                "start": { "line": start.0, "character": start.1 },
                "end": { "line": end.0, "character": end.1 }
            },
            "options": { "tabSize": tab_size, "insertSpaces": insert_spaces }
        });
        self.send_request(
            RequestKind::RangeFormatting,
            "textDocument/rangeFormatting",
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
            RequestKind::RangeFormatting => self.capabilities.document_range_formatting,
            RequestKind::OrganizeImports => self.capabilities.code_action,
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

    /// Record watched-file registrations from `client/registerCapability`.
    pub(super) fn register_capabilities(&mut self, params: &Value) {
        let Some(registrations) = params.get("registrations").and_then(Value::as_array) else {
            return;
        };
        for registration in registrations {
            if registration.get("method").and_then(Value::as_str)
                != Some("workspace/didChangeWatchedFiles")
            {
                continue;
            }
            let Some(id) = registration.get("id").and_then(Value::as_str) else {
                continue;
            };
            let globs = registration
                .pointer("/registerOptions/watchers")
                .and_then(Value::as_array)
                .map(|watchers| {
                    watchers
                        .iter()
                        .filter_map(|watcher| {
                            watcher.get("globPattern").and_then(|pattern| {
                                pattern.as_str().map(str::to_string).or_else(|| {
                                    pattern
                                        .get("pattern")
                                        .and_then(Value::as_str)
                                        .map(str::to_string)
                                })
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if globs.is_empty() {
                continue;
            }
            self.watched_files
                .retain(|registration| registration.id != id);
            self.watched_files.push(WatchedFiles {
                id: id.to_string(),
                globs,
            });
        }
    }

    /// Remove watched-file registrations from `client/unregisterCapability`.
    pub(super) fn unregister_capabilities(&mut self, params: &Value) {
        let Some(registrations) = params
            .get("unregisterations")
            .or_else(|| params.get("unregistrations"))
            .and_then(Value::as_array)
        else {
            return;
        };
        for registration in registrations {
            if let Some(id) = registration.get("id").and_then(Value::as_str) {
                self.watched_files.retain(|entry| entry.id != id);
            }
        }
    }

    /// Send `workspace/didChangeWatchedFiles` for changes matching a registration.
    pub fn did_change_watched_files(&mut self, changes: &[(PathBuf, WatchedFileChange)]) {
        if self.watched_files.is_empty() {
            return;
        }
        let mut payload = Vec::new();
        for (path, kind) in changes {
            let relative = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let relative = relative.trim_start_matches("./");
            let matched = self.watched_files.iter().any(|registration| {
                registration.globs.iter().any(|glob| {
                    crate::filesystem::gitignore::glob_matches(
                        glob.trim_start_matches("./"),
                        relative,
                    )
                })
            });
            if matched {
                payload.push(json!({
                    "uri": path_to_uri(path),
                    "type": *kind as u8
                }));
            }
        }
        if !payload.is_empty() {
            let _ = self.notify(
                "workspace/didChangeWatchedFiles",
                json!({ "changes": payload }),
            );
        }
    }

    pub(super) fn server_request_result(&mut self, method: &str, params: &Value) -> Value {
        match method {
            "client/registerCapability" => {
                self.register_capabilities(params);
                Value::Null
            }
            "client/unregisterCapability" => {
                self.unregister_capabilities(params);
                Value::Null
            }
            // A progress token was created by the server; no client-side setup
            // is needed before `$/progress` arrives.
            "window/workDoneProgress/create" => Value::Null,
            // Return one `null` configuration per requested item.
            "workspace/configuration" => {
                let count = params
                    .get("items")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0);
                Value::Array(vec![Value::Null; count])
            }
            "workspace/workspaceFolders" => {
                json!([{ "uri": path_to_uri(&self.root), "name": "workspace" }])
            }
            _ => Value::Null,
        }
    }

    /// Notify the server that a saved document was written.
    pub fn did_save(&mut self, path: &Path, text: &str) {
        if !self.versions.contains_key(path) {
            return;
        }
        let params = match self.capabilities.save {
            SaveKind::No => return,
            SaveKind::Save => json!({
                "textDocument": { "uri": path_to_uri(path) }
            }),
            SaveKind::IncludeText => json!({
                "textDocument": { "uri": path_to_uri(path) },
                "text": text
            }),
        };
        let _ = self.notify("textDocument/didSave", params);
    }
}

/// Whether a newer request of this kind should cancel every older one.
fn request_supersedes(kind: RequestKind) -> bool {
    !matches!(
        kind,
        RequestKind::DocumentDiagnostics
            | RequestKind::WorkspaceDiagnostics
            | RequestKind::ExecuteCommand
    )
}

impl Server {
    pub(super) fn send_request(
        &mut self,
        kind: RequestKind,
        method: &str,
        params: Value,
    ) -> io::Result<i64> {
        // Most feature requests are one-at-a-time in the app. Tell the server not
        // to waste work on an older one before replacing it. Diagnostics and
        // commands can legitimately have several requests in flight, so they are
        // excluded.
        if request_supersedes(kind) {
            let superseded: Vec<i64> = self
                .pending
                .iter()
                .filter(|(_, pending)| pending.kind == kind)
                .map(|(id, _)| *id)
                .collect();
            for id in superseded {
                self.pending.remove(&id);
                let _ = self.notify("$/cancelRequest", json!({ "id": id }));
            }
        }
        let id = self.request(method, params)?;
        self.pending.insert(
            id,
            PendingRequest {
                kind,
                deadline: Instant::now() + request_timeout(kind),
            },
        );
        Ok(id)
    }

    /// Cancel a request that the app no longer needs and forget its pending state.
    pub fn cancel_request(&mut self, id: i64) {
        self.pending.remove(&id);
        let _ = self.notify("$/cancelRequest", json!({ "id": id }));
    }
}
