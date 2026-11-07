use super::wire::*;
use super::*;

#[test]
fn uri_round_trips_paths_with_spaces() {
    let path = PathBuf::from("/tmp/kemori lsp/main.rs");
    let uri = path_to_uri(&path);
    assert_eq!(uri, "file:///tmp/kemori%20lsp/main.rs");
    assert_eq!(uri_to_path(&uri).as_deref(), Some(path.as_path()));
}

#[test]
fn decodes_windows_drive_uris() {
    assert_eq!(
        uri_to_path("file:///C:/tmp/a.rs"),
        Some(PathBuf::from("C:/tmp/a.rs"))
    );
}

#[test]
fn parses_did_save_options() {
    assert_eq!(parse_save(Some(&json!({ "save": true }))), SaveKind::Save);
    assert_eq!(
        parse_save(Some(&json!({ "save": { "includeText": true } }))),
        SaveKind::IncludeText
    );
    assert_eq!(
        parse_save(Some(&json!({ "save": { "includeText": false } }))),
        SaveKind::Save
    );
    assert_eq!(parse_save(Some(&json!({ "change": 1 }))), SaveKind::No);
    assert_eq!(parse_save(None), SaveKind::No);
}

#[test]
fn reads_capabilities_and_position_encoding() {
    let (encoding, caps) = parse_initialize(&json!({
        "capabilities": {
            "positionEncoding": "utf-8",
            "completionProvider": { "resolveProvider": true },
            "hoverProvider": true,
            "renameProvider": { "prepareProvider": false },
            "documentHighlightProvider": true,
            "inlayHintProvider": true,
            "codeLensProvider": true,
            "selectionRangeProvider": true,
            "callHierarchyProvider": true,
            "typeHierarchyProvider": true,
            "implementationProvider": true,
            "typeDefinitionProvider": true,
            "documentLinkProvider": true,
            "documentRangeFormattingProvider": true,
            "textDocumentSync": { "change": 1, "save": { "includeText": true } }
        }
    }));
    assert_eq!(encoding, PositionEncoding::Utf8);
    assert!(caps.completion);
    assert!(caps.completion_resolve);
    assert!(caps.hover);
    assert!(caps.rename);
    assert!(caps.document_highlight);
    assert!(caps.inlay_hint);
    assert!(caps.code_lens);
    assert!(caps.selection_range);
    assert!(caps.call_hierarchy);
    assert!(caps.type_hierarchy);
    assert!(caps.implementation);
    assert!(caps.type_definition);
    assert!(caps.document_link);
    assert!(caps.document_range_formatting);
    assert_eq!(caps.save, SaveKind::IncludeText);
    assert!(!caps.definition);
    assert!(!caps.workspace_symbol);
    assert!(!caps.document_diagnostics);
    assert!(!caps.workspace_diagnostics);

    // A diagnosticProvider advertises pull diagnostics; the workspace flag
    // requires the nested boolean.
    let (_, caps) = parse_initialize(&json!({
        "capabilities": {
            "diagnosticProvider": {
                "identifier": "example",
                "interFileDependencies": true,
                "workspaceDiagnostics": true
            }
        }
    }));
    assert!(caps.document_diagnostics);
    assert!(caps.workspace_diagnostics);

    // No declaration means the protocol default, UTF-16.
    let (encoding, caps) = parse_initialize(&json!({ "capabilities": {} }));
    assert_eq!(encoding, PositionEncoding::Utf16);
    assert!(!caps.completion);
}

#[test]
fn parses_pull_diagnostics() {
    let value = json!({
        "kind": "full",
        "resultId": "1",
        "items": [
            {
                "range": { "start": { "line": 2, "character": 4 }, "end": { "line": 2, "character": 9 } },
                "severity": 1,
                "message": "type mismatch"
            }
        ]
    });
    let diagnostics = parse_document_diagnostics(&value).expect("full report");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].start, TextPos::new(2, 4));
    assert_eq!(diagnostics[0].message, "type mismatch");
    assert!(parse_document_diagnostics(&json!({ "kind": "unchanged" })).is_none());

    let workspace = parse_workspace_diagnostics(&json!({
        "items": [
            {
                "kind": "full",
                "uri": "file:///tmp/a.rs",
                "items": [
                    {
                        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } },
                        "severity": 2,
                        "message": "unused"
                    }
                ]
            },
            { "kind": "unchanged", "uri": "file:///tmp/b.rs" }
        ]
    }));
    assert_eq!(workspace.len(), 1);
    assert_eq!(workspace[0].0, PathBuf::from("/tmp/a.rs"));
    assert_eq!(workspace[0].1.len(), 1);
}

#[test]
fn parses_published_diagnostics() {
    let params = json!({
        "uri": "file:///tmp/main.rs",
        "diagnostics": [
            {
                "range": { "start": { "line": 1, "character": 2 }, "end": { "line": 1, "character": 5 } },
                "severity": 1,
                "message": "unused variable"
            },
            {
                "range": { "start": { "line": 3, "character": 0 }, "end": { "line": 3, "character": 1 } },
                "severity": 2,
                "message": "try using `mut`"
            }
        ]
    });
    let ServerEvent::Diagnostics {
        path,
        version,
        diagnostics,
    } = parse_publish_diagnostics(&params).expect("event")
    else {
        panic!("expected diagnostics");
    };
    assert_eq!(path, PathBuf::from("/tmp/main.rs"));
    assert_eq!(version, None);
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].start, TextPos::new(1, 2));
    assert_eq!(diagnostics[1].severity, Severity::Warning);
    assert_eq!(diagnostics[1].message, "try using `mut`");
}

#[cfg(unix)]
#[test]
fn initializes_and_receives_diagnostics_from_a_mock_server() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    // A tiny shell server: answer `initialize` (id 1), publish one
    // diagnostic, then wait for the client to close our stdin.
    const MOCK: &str = r#"#!/bin/sh
read -r header
len=$(printf '%s' "$header" | tr -dc '0-9')
read -r blank
dd bs=1 count="$len" of=/dev/null 2>/dev/null
resp='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
note='{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///tmp/main.rs","diagnostics":[{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":3}},"severity":1,"message":"boom"}]}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#note}" "$note"
cat >/dev/null
"#;

    let dir = std::env::temp_dir().join(format!("kemori-lsp-mock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("mock-lsp.sh");
    {
        let mut file = std::fs::File::create(&script).unwrap();
        file.write_all(MOCK.as_bytes()).unwrap();
    }
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    let mut server = Server::start(LanguageId::Rust, script.to_str().unwrap(), &[], &dir).unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut ready = false;
    let mut diagnostics = None;
    while Instant::now() < deadline && !(ready && diagnostics.is_some()) {
        for event in server.poll() {
            match event {
                ServerEvent::Ready => ready = true,
                ServerEvent::Diagnostics { diagnostics: d, .. } => diagnostics = Some(d),
                ServerEvent::Response { .. } => {}
                ServerEvent::ApplyEdit { .. } => {}
                ServerEvent::Progress(_) => {}
                ServerEvent::Failed(_) => {}
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(ready, "the server should complete initialize");
    let diagnostics = diagnostics.expect("diagnostics");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "boom");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parses_a_published_diagnostic_version() {
    let params = json!({
        "uri": "file:///tmp/a.rs",
        "version": 7,
        "diagnostics": []
    });
    match parse_publish_diagnostics(&params).expect("event") {
        ServerEvent::Diagnostics { version, .. } => assert_eq!(version, Some(7)),
        other => panic!("expected diagnostics, got {other:?}"),
    }
}

#[test]
fn diagnostics_staleness_rules() {
    assert!(diagnostics_are_stale(Some(1), Some(2)), "older is stale");
    assert!(!diagnostics_are_stale(Some(2), Some(2)));
    assert!(!diagnostics_are_stale(Some(3), Some(2)), "newer is kept");
    assert!(!diagnostics_are_stale(None, Some(2)), "missing is kept");
    assert!(!diagnostics_are_stale(Some(1), None));
}

#[test]
fn parses_text_document_sync_forms() {
    assert_eq!(parse_sync(Some(&json!(0))), TextDocumentSyncKind::None);
    assert_eq!(parse_sync(Some(&json!(1))), TextDocumentSyncKind::Full);
    assert_eq!(
        parse_sync(Some(&json!(2))),
        TextDocumentSyncKind::Incremental
    );
    assert_eq!(
        parse_sync(Some(&json!({ "change": 0 }))),
        TextDocumentSyncKind::None
    );
    assert_eq!(
        parse_sync(Some(&json!({ "change": 2 }))),
        TextDocumentSyncKind::Incremental
    );
    // An omitted field keeps Kemori's long-standing full-text sync.
    assert_eq!(parse_sync(None), TextDocumentSyncKind::Full);

    let (_, caps) = parse_initialize(&json!({ "capabilities": { "textDocumentSync": 0 } }));
    assert_eq!(caps.sync, TextDocumentSyncKind::None);
}

#[test]
fn requests_expire_and_bounded() {
    // Formatting is allowed longer than the other requests.
    assert!(request_timeout(RequestKind::Formatting) > request_timeout(RequestKind::Completion));

    let now = Instant::now();
    let mut pending = HashMap::new();
    pending.insert(
        1,
        PendingRequest {
            kind: RequestKind::Hover,
            deadline: now - Duration::from_secs(1),
        },
    );
    pending.insert(
        2,
        PendingRequest {
            kind: RequestKind::Definition,
            deadline: now + Duration::from_secs(60),
        },
    );

    let expired = expire_requests(&mut pending, now);
    assert_eq!(expired, vec![(1, RequestKind::Hover)]);
    assert!(!pending.contains_key(&1), "expired entries are removed");
    assert!(pending.contains_key(&2), "live entries are kept");
}
