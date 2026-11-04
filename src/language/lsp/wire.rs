use super::*;

pub(super) fn parse_initialize(result: &Value) -> (PositionEncoding, ServerCapabilities) {
    let encoding = match result
        .pointer("/capabilities/positionEncoding")
        .and_then(Value::as_str)
    {
        Some("utf-8") => PositionEncoding::Utf8,
        Some("utf-32") => PositionEncoding::Utf32,
        _ => PositionEncoding::Utf16,
    };
    let capabilities = result.get("capabilities").unwrap_or(&Value::Null);
    let advertised = |name: &str| {
        capabilities
            .get(name)
            .is_some_and(|value| !value.is_null() && value != &Value::Bool(false))
    };
    let sync = parse_sync(capabilities.get("textDocumentSync"));
    let diagnostic_provider = capabilities.get("diagnosticProvider");
    let document_diagnostics =
        diagnostic_provider.is_some_and(|value| !value.is_null() && value != &Value::Bool(false));
    let workspace_diagnostics = diagnostic_provider
        .and_then(|value| value.get("workspaceDiagnostics"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (
        encoding,
        ServerCapabilities {
            completion: advertised("completionProvider"),
            hover: advertised("hoverProvider"),
            definition: advertised("definitionProvider"),
            references: advertised("referencesProvider"),
            rename: advertised("renameProvider"),
            code_action: advertised("codeActionProvider"),
            workspace_symbol: advertised("workspaceSymbolProvider"),
            signature_help: advertised("signatureHelpProvider"),
            document_formatting: advertised("documentFormattingProvider"),
            folding_range: advertised("foldingRangeProvider"),
            document_highlight: advertised("documentHighlightProvider"),
            inlay_hint: advertised("inlayHintProvider"),
            code_lens: advertised("codeLensProvider"),
            selection_range: advertised("selectionRangeProvider"),
            call_hierarchy: advertised("callHierarchyProvider"),
            type_hierarchy: advertised("typeHierarchyProvider"),
            implementation: advertised("implementationProvider"),
            type_definition: advertised("typeDefinitionProvider"),
            document_link: advertised("documentLinkProvider"),
            document_diagnostics,
            workspace_diagnostics,
            sync,
        },
    )
}

/// Parse `textDocumentSync`, which the protocol defines as either a numeric
/// `TextDocumentSyncKind` or an object carrying a `change` kind.
///
/// Kemori has always sent whole-document changes; an omitted field keeps that
/// behaviour (some servers rely on it), while an explicit `None` is honoured so
/// such a server is not spammed with changes it does not want.
pub(super) fn parse_sync(value: Option<&Value>) -> TextDocumentSyncKind {
    let kind = match value {
        Some(Value::Number(number)) => number.as_u64(),
        Some(object) => object.get("change").and_then(Value::as_u64),
        None => return TextDocumentSyncKind::Full,
    };
    match kind {
        Some(0) => TextDocumentSyncKind::None,
        Some(2) => TextDocumentSyncKind::Incremental,
        _ => TextDocumentSyncKind::Full,
    }
}

/// Parse a `textDocument/diagnostic` result.
///
/// `None` means the server reported the document unchanged (or sent a shape
/// Kemori does not understand), so the existing diagnostics stay in place. A
/// full report with no items is `Some(vec![])` and clears them.
pub fn parse_document_diagnostics(value: &Value) -> Option<Vec<Diagnostic>> {
    if value.get("kind").and_then(Value::as_str) != Some("full") {
        return None;
    }
    Some(
        value
            .get("items")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(parse_diagnostic).collect())
            .unwrap_or_default(),
    )
}

/// Parse a `workspace/diagnostic` report into per-file diagnostics.
pub fn parse_workspace_diagnostics(value: &Value) -> Vec<(PathBuf, Vec<Diagnostic>)> {
    value
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| item.get("kind").and_then(Value::as_str) == Some("full"))
                .filter_map(|item| {
                    let path = uri_to_path(item.get("uri")?.as_str()?)?;
                    let diagnostics = item
                        .get("items")
                        .and_then(Value::as_array)
                        .map(|items| items.iter().filter_map(parse_diagnostic).collect())
                        .unwrap_or_default();
                    Some((path, diagnostics))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn parse_publish_diagnostics(params: &Value) -> Option<ServerEvent> {
    let path = uri_to_path(params.get("uri")?.as_str()?)?;
    let version = params.get("version").and_then(Value::as_i64);
    let diagnostics = params
        .get("diagnostics")?
        .as_array()?
        .iter()
        .filter_map(parse_diagnostic)
        .collect();
    Some(ServerEvent::Diagnostics {
        path,
        version,
        diagnostics,
    })
}

/// Whether diagnostics stamped with `version` are behind the version the server
/// has since received.
///
/// A missing version on either side is treated as current, matching the
/// protocol's optional field and servers that do not version diagnostics.
pub(super) fn diagnostics_are_stale(version: Option<i64>, current: Option<i64>) -> bool {
    matches!((version, current), (Some(version), Some(current)) if version < current)
}

/// The deadline for a request of `kind`.
pub(super) fn request_timeout(kind: RequestKind) -> Duration {
    if kind == RequestKind::Formatting {
        FORMAT_REQUEST_TIMEOUT
    } else {
        REQUEST_TIMEOUT
    }
}

/// Remove and return every request whose deadline has passed.
pub(super) fn expire_requests(
    pending: &mut HashMap<i64, PendingRequest>,
    now: Instant,
) -> Vec<(i64, RequestKind)> {
    let expired: Vec<i64> = pending
        .iter()
        .filter(|(_, request)| request.deadline <= now)
        .map(|(id, _)| *id)
        .collect();
    expired
        .into_iter()
        .filter_map(|id| pending.remove(&id).map(|request| (id, request.kind)))
        .collect()
}

pub(super) fn parse_diagnostic(item: &Value) -> Option<Diagnostic> {
    let range = item.get("range")?;
    let start = text_pos(range.get("start")?)?;
    let end = text_pos(range.get("end")?)?;
    let severity = match item.get("severity").and_then(Value::as_u64) {
        Some(1) => Severity::Error,
        Some(2) => Severity::Warning,
        Some(3) => Severity::Info,
        _ => Severity::Hint,
    };
    let message = item.get("message").and_then(Value::as_str).unwrap_or("");
    Some(Diagnostic::new(start, end, severity, message))
}

pub(super) fn text_pos(value: &Value) -> Option<TextPos> {
    Some(TextPos::new(
        value.get("line")?.as_u64()? as usize,
        value.get("character")?.as_u64()? as usize,
    ))
}

/// The `textDocument` + `position` parameters shared by most requests.
pub(super) fn position_params(path: &Path, line: usize, col: usize) -> Value {
    json!({
        "textDocument": { "uri": path_to_uri(path) },
        "position": { "line": line, "character": col }
    })
}

/// Encode a filesystem path as a `file://` URI, percent-encoding the bytes LSP
/// cares about.
pub fn path_to_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                uri.push(byte as char)
            }
            other => uri.push_str(&format!("%{other:02X}")),
        }
    }
    uri
}

/// Decode a `file://` URI back into a path.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // Drop an optional authority component (`file://localhost/...`).
    let rest = match rest.find('/') {
        Some(slash) => &rest[slash..],
        None => rest,
    };
    let bytes = rest.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(hex) = bytes.get(index + 1..index + 3)
            && let Ok(hex) = std::str::from_utf8(hex)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            decoded.push(byte);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    let mut decoded = String::from_utf8_lossy(&decoded).into_owned();
    // `file:///C:/x` decodes to `/C:/x`; drop the leading slash so the path is
    // usable on Windows. Harmless on other platforms, where no drive letter
    // follows.
    let bytes = decoded.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        decoded.remove(0);
    }
    if decoded.is_empty() {
        return None;
    }
    Some(PathBuf::from(decoded))
}

/// The LSP `languageId` string for a language.
pub fn lsp_language_id(language: LanguageId) -> &'static str {
    match language {
        LanguageId::Rust => "rust",
        LanguageId::Go => "go",
        LanguageId::Python => "python",
        LanguageId::Shell => "shellscript",
        LanguageId::Markdown => "markdown",
        LanguageId::Nix => "nix",
        LanguageId::Json => "json",
        LanguageId::Toml => "toml",
        LanguageId::Yaml => "yaml",
        LanguageId::TypeScript => "typescript",
        LanguageId::JavaScript => "javascript",
        LanguageId::C => "c",
        LanguageId::Cpp => "cpp",
        LanguageId::Java => "java",
        LanguageId::CSharp => "csharp",
        LanguageId::Html => "html",
        LanguageId::Css => "css",
        _ => "plaintext",
    }
}

/// The `languageId` for a specific file, refining TSX/JSX to their React ids.
pub(super) fn language_id_for(language: LanguageId, path: &Path) -> &'static str {
    match (
        language,
        path.extension().and_then(|extension| extension.to_str()),
    ) {
        (LanguageId::TypeScript, Some("tsx")) => "typescriptreact",
        (LanguageId::JavaScript, Some("jsx")) => "javascriptreact",
        _ => lsp_language_id(language),
    }
}
