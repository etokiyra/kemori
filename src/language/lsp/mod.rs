//! Asynchronous Language Server Protocol client.
//!
//! When a language server is installed, Kemori starts one per workspace root and
//! speaks JSON-RPC over its stdio. This is deliberately minimal and grows
//! feature by feature: today it runs the lifecycle and turns
//! `textDocument/publishDiagnostics` into Kemori diagnostics. Completion, hover,
//! navigation, rename and code actions build on the same connection later.
//!
//! Nothing here is required for basic editing. If no server is installed Kemori
//! keeps using its built-in heuristic providers, so an offline machine still
//! has syntax highlighting, structural diagnostics, symbols and formatting.

pub mod change;
pub mod code_lens;
pub mod convert;
pub mod folding;
pub mod hierarchy;
pub mod inlay;
pub mod jsonrpc;
pub mod links;
pub mod selection;
pub use watched::WatchedFileChange;
pub mod watched;

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::language::diagnostics::{Diagnostic, Severity, TextPos};
use crate::language::id::LanguageId;

use jsonrpc::Message;

/// How long the server has to answer a request before Kemori gives up and clears
/// the pending state. Formatting can legitimately take longer than the others.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const FORMAT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// How a server counts the `character` offset in a `{line, character}` position.
///
/// LSP defaults to UTF-16 code units; Kemori works in Unicode scalar values. Kemori
/// asks for UTF-8 (which matches its own counting) and only the servers that do
/// not offer it fall back to UTF-16.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionEncoding {
    Utf8,
    #[default]
    Utf16,
    Utf32,
}

/// How a server wants documents synchronized, from `textDocumentSync`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextDocumentSyncKind {
    /// The server asked for no document synchronization.
    None,
    /// The server accepts whole-document changes. Kemori sends full text for both
    /// this and `Incremental`, which the protocol permits as the safe fallback.
    #[default]
    Full,
    /// The server prefers incremental changes; Kemori still sends full text
    /// (omitting `range`), which is a valid whole-document replacement.
    Incremental,
}

/// How a server wants saves reported, from `textDocumentSync.save`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SaveKind {
    /// No `didSave` notification.
    #[default]
    No,
    /// Send `didSave` with the document URI only.
    Save,
    /// Send `didSave` with the full text.
    IncludeText,
}

/// The server features Kemori knows how to use, from its `initialize` result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ServerCapabilities {
    completion: bool,
    hover: bool,
    definition: bool,
    references: bool,
    rename: bool,
    code_action: bool,
    workspace_symbol: bool,
    signature_help: bool,
    document_formatting: bool,
    folding_range: bool,
    /// `textDocument/documentHighlight`.
    document_highlight: bool,
    /// `textDocument/inlayHint`.
    inlay_hint: bool,
    /// `textDocument/codeLens`.
    code_lens: bool,
    /// `textDocument/selectionRange`.
    selection_range: bool,
    /// `callHierarchy/*`.
    call_hierarchy: bool,
    /// `typeHierarchy/*`.
    type_hierarchy: bool,
    /// `textDocument/implementation`.
    implementation: bool,
    /// `textDocument/typeDefinition`.
    type_definition: bool,
    /// `textDocument/documentLink`.
    document_link: bool,
    /// `textDocument/rangeFormatting`.
    document_range_formatting: bool,
    /// `textDocument/diagnostic` (pull diagnostics).
    document_diagnostics: bool,
    /// `workspace/diagnostic` (workspace-wide pull diagnostics).
    workspace_diagnostics: bool,
    sync: TextDocumentSyncKind,
    save: SaveKind,
}

/// The Kemori feature a server request belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestKind {
    Completion,
    Hover,
    Definition,
    References,
    Rename,
    CodeActions,
    WorkspaceSymbols,
    SignatureHelp,
    Formatting,
    FoldingRange,
    DocumentHighlight,
    InlayHint,
    CodeLens,
    SelectionRange,
    PrepareCallHierarchy,
    CallHierarchyIncoming,
    CallHierarchyOutgoing,
    PrepareTypeHierarchy,
    TypeHierarchySuper,
    TypeHierarchySub,
    Implementation,
    TypeDefinition,
    DocumentLink,
    OrganizeImports,
    RangeFormatting,
    /// Pull diagnostics for one document (`textDocument/diagnostic`).
    DocumentDiagnostics,
    /// Pull diagnostics for the whole workspace (`workspace/diagnostic`).
    WorkspaceDiagnostics,
    /// `workspace/executeCommand`, used for JDT LS debug commands.
    ExecuteCommand,
}

/// Something the app consumes from a running server.
#[derive(Clone, Debug)]
pub enum ServerEvent {
    /// The `initialize` handshake completed; the server accepts document
    /// notifications.
    Ready,
    /// Fresh diagnostics for a document. `version` is the document version the
    /// server computed them against, when it reported one.
    Diagnostics {
        path: PathBuf,
        version: Option<i64>,
        diagnostics: Vec<Diagnostic>,
    },
    /// The answer to a [`RequestKind`] request. The `id` is the JSON-RPC id the
    /// request was sent with, so callers can ignore a response that a newer
    /// request has superseded (for example while typing).
    Response {
        kind: RequestKind,
        id: i64,
        result: Result<Value, String>,
    },
    /// The server asked Kemori to apply a workspace edit. The app must apply it
    /// and then call [`Server::apply_edit_response`].
    ApplyEdit { id: Value, params: Value },
    /// The server could not start or exited unexpectedly.
    Failed(String),
}

/// A request awaiting a response, with the deadline that bounds it.
struct PendingRequest {
    kind: RequestKind,
    deadline: Instant,
}

/// A language server process and its message loop.
pub struct Server {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<Message>,
    next_id: i64,
    init_id: Option<i64>,
    ready: bool,
    root: PathBuf,
    versions: HashMap<PathBuf, i64>,
    /// Last text sent to the server for each open document, so a change can be
    /// reported incrementally when the server supports it.
    documents: HashMap<PathBuf, String>,
    pending: HashMap<i64, PendingRequest>,
    language: LanguageId,
    /// The encoding the server uses for `character` offsets.
    encoding: PositionEncoding,
    /// The features the server advertised.
    capabilities: ServerCapabilities,
    /// Watch registrations requested through `client/registerCapability`.
    watched_files: Vec<watched::WatchedFiles>,
    /// The last few stderr lines, for a more useful failure message.
    stderr: Arc<Mutex<String>>,
}

impl Server {
    /// Spawn a server and begin the `initialize` handshake.
    pub fn start(
        language: LanguageId,
        program: &str,
        args: &[&str],
        root: &Path,
    ) -> io::Result<Self> {
        Self::start_with_env(language, program, args, root, &[])
    }

    /// Spawn a server with extra environment variables, used to point managed
    /// servers at Kemori-provisioned runtimes (the .NET SDK, a managed JDK).
    pub fn start_with_env(
        language: LanguageId,
        program: &str,
        args: &[&str],
        root: &Path,
        env: &[(String, String)],
    ) -> io::Result<Self> {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in env {
            command.env(key, value);
        }
        let mut child = command.spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no stdout"))?;
        let stderr_pipe = child.stderr.take();

        let (tx, rx) = mpsc::channel::<Message>();
        thread::Builder::new()
            .name("kemori-lsp-reader".to_string())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    match jsonrpc::read_message(&mut reader) {
                        Ok(Some(value)) => {
                            let Some(message) = jsonrpc::decode(value) else {
                                continue;
                            };
                            if tx.send(message).is_err() {
                                break;
                            }
                        }
                        // A message whose *body* was not valid JSON has already
                        // been consumed, so we are still framed correctly and can
                        // keep serving. Framing/I/O errors are fatal.
                        Err(err) if err.kind() == io::ErrorKind::Other => continue,
                        Ok(None) | Err(_) => break,
                    }
                }
            })?;

        // Keep the last few stderr lines so a crash can explain itself, without
        // letting server noise reach the terminal.
        let stderr = Arc::new(Mutex::new(String::new()));
        if let Some(pipe) = stderr_pipe {
            let buffer = Arc::clone(&stderr);
            let _ = thread::Builder::new()
                .name("kemori-lsp-stderr".to_string())
                .spawn(move || {
                    let reader = BufReader::new(pipe);
                    for line in reader.lines().map_while(Result::ok) {
                        let mut buffer = match buffer.lock() {
                            Ok(buffer) => buffer,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        if !buffer.is_empty() {
                            buffer.push('\n');
                        }
                        buffer.push_str(&line);
                        // Bound what we keep: only the tail matters.
                        if buffer.len() > 2048 {
                            let drain = buffer.len() - 2048;
                            buffer.drain(..drain);
                        }
                    }
                });
        }

        let mut server = Server {
            child,
            stdin,
            events: rx,
            next_id: 0,
            init_id: None,
            ready: false,
            root: root.to_path_buf(),
            versions: HashMap::new(),
            documents: HashMap::new(),
            pending: HashMap::new(),
            language,
            encoding: PositionEncoding::default(),
            capabilities: ServerCapabilities::default(),
            watched_files: Vec::new(),
            stderr,
        };

        let initialization_options = if language == LanguageId::Java {
            crate::language::tools::java_debug_jar()
                .filter(|path| path.is_file())
                .map(|path| {
                    json!({
                        "bundles": [path.to_string_lossy()],
                    })
                })
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let id = server.request(
            "initialize",
            json!({
                "processId": null,
                "clientInfo": { "name": "kemori" },
                "rootUri": path_to_uri(root),
                "rootPath": root.to_string_lossy(),
                "initializationOptions": initialization_options,
                "capabilities": {
                    "general": { "positionEncodings": ["utf-8", "utf-16"] },
                    "textDocument": {
                        "synchronization": { "dynamicRegistration": false, "didSave": true },
                        "publishDiagnostics": { "relatedInformation": false },
                        "completion": { "completionItem": { "snippetSupport": false } },
                        "hover": { "contentFormat": ["markdown", "plaintext"] },
                        "definition": {},
                        "references": {},
                        "rename": { "prepareSupport": false },
                        "codeAction": {},
                        "signatureHelp": {
                            "signatureInformation": {
                                "documentationFormat": ["markdown", "plaintext"],
                                "parameterInformation": { "labelOffsetSupport": true }
                            },
                            "contextSupport": true
                        },
                        "formatting": { "dynamicRegistration": false },
                        "foldingRange": { "dynamicRegistration": false },
                        "diagnostic": {
                            "dynamicRegistration": false,
                            "relatedDocumentSupport": false
                        }
                    },
                    "workspace": {
                        "configuration": true,
                        "symbol": {},
                        "didChangeWatchedFiles": { "dynamicRegistration": true },
                        "diagnostics": { "refreshSupport": false }
                    }
                },
                "workspaceFolders": null
            }),
        )?;
        server.init_id = Some(id);
        Ok(server)
    }

    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// The encoding the server uses for character offsets.
    pub fn position_encoding(&self) -> PositionEncoding {
        self.encoding
    }

    /// Read the negotiated position encoding and the advertised capabilities.
    fn absorb_initialize(&mut self, result: &Value) {
        let (encoding, capabilities) = parse_initialize(result);
        self.encoding = encoding;
        self.capabilities = capabilities;
    }

    /// Append the server's recent stderr, when there is any, to a failure
    /// message so a crash can explain itself.
    fn explain(&self, message: String) -> String {
        let stderr = match self.stderr.lock() {
            Ok(buffer) => buffer,
            Err(poisoned) => poisoned.into_inner(),
        };
        let detail = stderr.trim();
        if detail.is_empty() {
            message
        } else {
            format!("{message}: {detail}")
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Notify the server that a document was opened.
    pub fn did_open(&mut self, path: &Path, text: &str) {
        // A server that advertises no synchronization does not want documents.
        if self.capabilities.sync == TextDocumentSyncKind::None {
            return;
        }
        let version = self.next_version(path);
        self.documents.insert(path.to_path_buf(), text.to_string());
        let _ = self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": path_to_uri(path),
                    "languageId": language_id_for(self.language, path),
                    "version": version,
                    "text": text
                }
            }),
        );
    }

    /// Notify the server that a document changed (full-text sync).
    pub fn did_change(&mut self, path: &Path, text: &str) {
        if !self.versions.contains_key(path) {
            return self.did_open(path, text);
        }
        let version = self.next_version(path);
        let old = self.documents.get(path).cloned().unwrap_or_default();
        let content_changes = if self.capabilities.sync == TextDocumentSyncKind::Incremental {
            change::incremental(&old, text)
        } else {
            json!([ { "text": text } ])
        };
        self.documents.insert(path.to_path_buf(), text.to_string());
        let _ = self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": path_to_uri(path), "version": version },
                "contentChanges": content_changes
            }),
        );
    }

    /// Notify the server that a document was closed.
    pub fn did_close(&mut self, path: &Path) {
        if self.versions.remove(path).is_none() {
            return;
        }
        self.documents.remove(path);
        let _ = self.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": path_to_uri(path) } }),
        );
    }

    /// Whether the server has this document open.
    pub fn has_open_document(&self, path: &Path) -> bool {
        self.versions.contains_key(path)
    }

    /// Ask the server to format a document, returning the request id so a
    /// caller can discard a superseded response.
    pub fn formatting(&mut self, path: &Path, tab_size: usize, insert_spaces: bool) -> Option<i64> {
        let params = json!({
            "textDocument": { "uri": path_to_uri(path) },
            "options": { "tabSize": tab_size, "insertSpaces": insert_spaces }
        });
        self.send_request(RequestKind::Formatting, "textDocument/formatting", params)
            .ok()
    }

    /// Ask JDT LS to run a debug-related command (`vscode.java.*`), returning
    /// the request id so the app can consume the response.
    pub fn execute_command_request(&mut self, command: &str, arguments: Value) -> Option<i64> {
        self.send_request(
            RequestKind::ExecuteCommand,
            "workspace/executeCommand",
            json!({ "command": command, "arguments": arguments }),
        )
        .ok()
    }

    /// Ask the server for a document's folding ranges, returning the request id
    /// so a response for an older buffer can be ignored.
    pub fn folding_ranges(&mut self, path: &Path) -> Option<i64> {
        let params = json!({ "textDocument": { "uri": path_to_uri(path) } });
        self.send_request(
            RequestKind::FoldingRange,
            "textDocument/foldingRange",
            params,
        )
        .ok()
    }

    /// Ask the server for signature help, returning the request id so a caller
    /// can discard a superseded response.
    pub fn signature_help(&mut self, path: &Path, line: usize, col: usize) -> Option<i64> {
        let mut params = position_params(path, line, col);
        if let Some(object) = params.as_object_mut() {
            // 1 = Invoked; 2 = TriggerCharacter; 3 = ContentChange.
            object.insert("context".to_string(), json!({ "triggerKind": 1 }));
        }
        self.send_request(
            RequestKind::SignatureHelp,
            "textDocument/signatureHelp",
            params,
        )
        .ok()
    }

    /// Ask the server to complete at a position, returning the request id so a
    /// caller can discard the response if it becomes stale.
    pub fn completion(&mut self, path: &Path, line: usize, col: usize) -> Option<i64> {
        self.send_request(
            RequestKind::Completion,
            "textDocument/completion",
            position_params(path, line, col),
        )
        .ok()
    }

    /// Ask the server for hover information at a position, returning the
    /// request id so a late answer can be discarded.
    pub fn hover(&mut self, path: &Path, line: usize, col: usize) -> Option<i64> {
        self.send_request(
            RequestKind::Hover,
            "textDocument/hover",
            position_params(path, line, col),
        )
        .ok()
    }

    /// Ask the server for the definition at a position, returning the request id
    /// so the caller can ignore a response the cursor has moved past.
    pub fn definition(&mut self, path: &Path, line: usize, col: usize) -> Option<i64> {
        self.send_request(
            RequestKind::Definition,
            "textDocument/definition",
            position_params(path, line, col),
        )
        .ok()
    }

    /// Ask the server for every reference to the symbol at a position.
    pub fn references(&mut self, path: &Path, line: usize, col: usize) -> Option<i64> {
        let mut params = position_params(path, line, col);
        if let Some(object) = params.as_object_mut() {
            object.insert("context".to_string(), json!({ "includeDeclaration": true }));
        }
        self.send_request(RequestKind::References, "textDocument/references", params)
            .ok()
    }

    /// Ask the server to rename the symbol at a position.
    pub fn rename(&mut self, path: &Path, line: usize, col: usize, new_name: &str) -> Option<i64> {
        let mut params = position_params(path, line, col);
        if let Some(object) = params.as_object_mut() {
            object.insert("newName".to_string(), json!(new_name));
        }
        self.send_request(RequestKind::Rename, "textDocument/rename", params)
            .ok()
    }

    /// Ask the server for code actions over a range.
    pub fn code_action(
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
            "context": { "diagnostics": [] }
        });
        self.send_request(RequestKind::CodeActions, "textDocument/codeAction", params)
            .ok()
    }

    /// Ask the server for workspace-wide symbols matching `query`.
    ///
    /// An empty query asks for everything the server is willing to return,
    /// which the picker then filters locally.
    pub fn workspace_symbols(&mut self, query: &str) {
        let _ = self.send_request(
            RequestKind::WorkspaceSymbols,
            "workspace/symbol",
            json!({ "query": query }),
        );
    }

    /// Ask the server to execute one of its commands. The response is ignored;
    /// the server may follow up with a `workspace/applyEdit` request.
    pub fn execute_command(&mut self, command: &str, arguments: Value) {
        let _ = self.request(
            "workspace/executeCommand",
            json!({ "command": command, "arguments": arguments }),
        );
    }

    /// Answer the server's `workspace/applyEdit` request.
    pub fn apply_edit_response(&mut self, id: &Value, applied: bool) {
        let _ = self.respond(id, json!({ "applied": applied }));
    }

    /// Drain pending events without blocking.
    pub fn poll(&mut self) -> Vec<ServerEvent> {
        let mut events = Vec::new();

        // Expire requests the server never answered, so `pending` stays bounded
        // and the UI is not left waiting forever.
        let expired = expire_requests(&mut self.pending, Instant::now());
        for (id, kind) in expired {
            // The request is no longer tracked, but the server may still be
            // working on it; ask it to stop.
            let _ = self.notify("$/cancelRequest", json!({ "id": id }));
            events.push(ServerEvent::Response {
                kind,
                id,
                result: Err("request timed out".to_string()),
            });
        }

        loop {
            match self.events.try_recv() {
                Ok(Message::Response { id, result, error }) => {
                    let id_number = jsonrpc::id_as_i64(&id).unwrap_or(-1);
                    if Some(id_number) == self.init_id {
                        self.init_id = None;
                        match error {
                            Some(error) => {
                                events.push(ServerEvent::Failed(self.explain(error.message)))
                            }
                            None if result.is_some() => {
                                self.absorb_initialize(result.as_ref().unwrap_or(&Value::Null));
                                self.ready = true;
                                let _ = self.notify("initialized", json!({}));
                                events.push(ServerEvent::Ready);
                            }
                            None => {}
                        }
                    } else if let Some(request) = self.pending.remove(&id_number) {
                        let result = match error {
                            Some(error) => Err(error.message),
                            None => Ok(result.unwrap_or(Value::Null)),
                        };
                        events.push(ServerEvent::Response {
                            kind: request.kind,
                            id: id_number,
                            result,
                        });
                    }
                }
                Ok(Message::Notification { method, params }) => {
                    if method == "textDocument/publishDiagnostics"
                        && let Some(event) = parse_publish_diagnostics(&params)
                        && !self.is_stale_diagnostics(&event)
                    {
                        events.push(event);
                    }
                }
                Ok(Message::Request { id, method, params }) => {
                    if method == "workspace/applyEdit" {
                        // The app applies the edit and answers for us.
                        events.push(ServerEvent::ApplyEdit { id, params });
                    } else {
                        // Answer other server-initiated requests so it does not stall.
                        let result = self.server_request_result(&method, &params);
                        let _ = self.respond(&id, result);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    events.push(ServerEvent::Failed(
                        self.explain("language server exited".to_string()),
                    ));
                    break;
                }
            }
        }
        events
    }

    /// Whether published diagnostics are known to describe an older document
    /// version than the server has since received.
    ///
    /// A missing version is accepted for compatibility; a present one that is
    /// behind the last synchronized version is dropped so stale markers cannot
    /// appear over newer text.
    fn is_stale_diagnostics(&self, event: &ServerEvent) -> bool {
        let ServerEvent::Diagnostics {
            path,
            version: Some(version),
            ..
        } = event
        else {
            return false;
        };
        diagnostics_are_stale(Some(*version), self.versions.get(path).copied())
    }

    fn request(&mut self, method: &str, params: Value) -> io::Result<i64> {
        self.next_id += 1;
        let id = self.next_id;
        jsonrpc::write_message(
            &mut self.stdin,
            &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
        )?;
        Ok(id)
    }

    fn notify(&mut self, method: &str, params: Value) -> io::Result<()> {
        jsonrpc::write_message(
            &mut self.stdin,
            &json!({ "jsonrpc": "2.0", "method": method, "params": params }),
        )
    }

    fn respond(&mut self, id: &Value, result: Value) -> io::Result<()> {
        jsonrpc::write_message(
            &mut self.stdin,
            &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        )
    }

    fn next_version(&mut self, path: &Path) -> i64 {
        let version = self.versions.entry(path.to_path_buf()).or_insert(0);
        *version += 1;
        *version
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Parse the `initialize` result into the encoding and capabilities Kemori uses.
mod requests_extra;
mod wire;
pub use wire::lsp_language_id;
use wire::{
    diagnostics_are_stale, expire_requests, language_id_for, parse_initialize,
    parse_publish_diagnostics, position_params, request_timeout,
};
pub use wire::{parse_document_diagnostics, parse_workspace_diagnostics, path_to_uri, uri_to_path};

#[cfg(test)]
mod tests;
