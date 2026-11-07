#[test]
fn a_successful_handshake_does_not_forgive_an_earlier_crash() {
    let dir = temp_project("restart-budget-reset");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.lsp.clear();
    app.lsp.insert(
        (app.workspace.root().to_path_buf(), LanguageId::Rust),
        LspJob {
            restarts: 1,
            ..LspJob::default()
        },
    );

    // Connecting must not reset the budget on its own.
    app.lsp_ready(LanguageId::Rust);
    assert_eq!(
        app.lsp
            .get(&(app.workspace.root().to_path_buf(), LanguageId::Rust))
            .unwrap()
            .restarts,
        1,
        "a fresh handshake must not grant a fresh restart budget"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn restart_server_reports_for_unsupported_files() {
    let dir = temp_project("restart-server");
    let file = dir.join("notes.txt");
    fs::write(&file, "hello\n").unwrap();
    let mut app = app_with_file(&file);

    app.execute_command(ids::RESTART_SERVER);
    assert!(app.lsp.is_empty());
    assert!(
        app.status_message()
            .unwrap_or("")
            .contains("No language server"),
        "status was {:?}",
        app.status_message()
    );
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn language_server_diagnostics_reach_the_document() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // Mock server: answer `initialize` (id 1) and publish one diagnostic
    // for the file passed as the first argument.
    const MOCK: &str = r#"#!/bin/sh
target="$1"
read -r header
len=$(printf '%s' "$header" | tr -dc '0-9')
read -r blank
dd bs=1 count="$len" of=/dev/null 2>/dev/null
resp='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
note=$(printf '{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file://%s","diagnostics":[{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":3}},"severity":1,"message":"boom"}]}}' "$target")
printf 'Content-Length: %s\r\n\r\n%s' "${#note}" "$note"
cat >/dev/null
"#;

    let dir = temp_project("lsp-wire");
    let file = dir.join("src/main.rs");
    let script = dir.join("mock-lsp.sh");
    {
        let mut handle = fs::File::create(&script).unwrap();
        handle.write_all(MOCK.as_bytes()).unwrap();
    }
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();

    let mut app = app_with_file(&file);
    app.start_lsp(
        LanguageId::Rust,
        script.to_str().unwrap(),
        &[file.to_str().unwrap()],
    );

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut applied = false;
    while Instant::now() < deadline {
        app.poll_lsp();
        applied = app
            .editor
            .active_document()
            .is_some_and(|doc| doc.diagnostics_from_lsp() && !doc.diagnostics().is_empty());
        if applied {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(applied, "expected language-server diagnostics");
    assert_eq!(app.lsp_status(), LspStatus::Ready);
    assert_eq!(
        app.editor.active_document().unwrap().diagnostics()[0].message,
        "boom"
    );
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn documents_opened_after_ready_are_synced_to_the_server() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // Mock server: answer `initialize`, append every `didOpen` URI to the
    // log file named by the first argument, and stay alive.
    const MOCK: &str = r#"#!/bin/sh
log="$1"
while read -r header; do
  header=$(printf '%s' "$header" | tr -d '\r')
  case "$header" in
    Content-Length:*) len=${header#Content-Length: } ;;
    *) continue ;;
  esac
  read -r blank
  body=$(dd bs=1 count="$len" 2>/dev/null)
  id=$(printf '%s' "$body" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  method=$(printf '%s' "$body" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  if [ "$method" = "textDocument/didOpen" ]; then
    uri=$(printf '%s' "$body" | sed -n 's/.*"uri":"\([^"]*\)".*/\1/p')
    printf '%s\n' "$uri" >> "$log"
  fi
  [ -z "$id" ] && continue
  case "$method" in
    initialize) result='{"capabilities":{}}' ;;
    *) result='null' ;;
  esac
  resp=$(printf '{"jsonrpc":"2.0","id":%s,"result":%s}' "$id" "$result")
  printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
done
"#;

    let dir = temp_project("lsp-late-open");
    let first = dir.join("src/main.rs");
    let second = dir.join("src/other.rs");
    fs::write(&second, "pub fn other() {}\n").unwrap();
    let log = dir.join("opens.log");
    let script = dir.join("mock-lsp.sh");
    {
        let mut handle = fs::File::create(&script).unwrap();
        handle.write_all(MOCK.as_bytes()).unwrap();
    }
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();

    let mut app = app_with_file(&first);
    // Drop any scheduled auto-start so only our mock runs.
    app.lsp.clear();
    app.start_lsp(
        LanguageId::Rust,
        script.to_str().unwrap(),
        &[log.to_str().unwrap()],
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline
        && !app
            .lsp
            .get(&(app.workspace.root().to_path_buf(), LanguageId::Rust))
            .and_then(|job| job.server.as_ref())
            .is_some_and(|server| server.is_ready())
    {
        app.poll_lsp();
        std::thread::sleep(Duration::from_millis(5));
    }

    // Opening a second file after the handshake must attach it too.
    app.open_path(second.clone());
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut synced = false;
    while Instant::now() < deadline && !synced {
        app.apply_background_events();
        app.poll_lsp();
        synced = fs::read_to_string(&log)
            .map(|log| log.contains("other.rs"))
            .unwrap_or(false);
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(
        synced,
        "a document opened after ready must be sent to the server"
    );
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn a_second_language_server_starts_alongside_the_first() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // A minimal server that answers `initialize` and stays alive.
    const MOCK: &str = r#"#!/bin/sh
while read -r header; do
  header=$(printf '%s' "$header" | tr -d '\r')
  case "$header" in
    Content-Length:*) len=${header#Content-Length: } ;;
    *) continue ;;
  esac
  read -r blank
  body=$(dd bs=1 count="$len" 2>/dev/null)
  id=$(printf '%s' "$body" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  method=$(printf '%s' "$body" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  [ -z "$id" ] && continue
  case "$method" in
    initialize) result='{"capabilities":{}}' ;;
    *) result='null' ;;
  esac
  resp=$(printf '{"jsonrpc":"2.0","id":%s,"result":%s}' "$id" "$result")
  printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
done
"#;

    let dir = temp_project("lsp-multi");
    let file = dir.join("src/main.rs");
    let script = dir.join("mock-lsp.sh");
    {
        let mut handle = fs::File::create(&script).unwrap();
        handle.write_all(MOCK.as_bytes()).unwrap();
    }
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();

    let mut app = app_with_file(&file);
    app.start_lsp(LanguageId::Rust, script.to_str().unwrap(), &[]);
    app.start_lsp(LanguageId::Python, script.to_str().unwrap(), &[]);
    assert_eq!(app.lsp.len(), 2, "each language gets its own server");

    let deadline = Instant::now() + Duration::from_secs(15);
    let ready = |app: &App| {
        [LanguageId::Rust, LanguageId::Python]
            .iter()
            .all(|language| {
                app.lsp
                    .get(&(app.workspace.root().to_path_buf(), *language))
                    .and_then(|job| job.server.as_ref())
                    .is_some_and(|server| server.is_ready())
            })
    };
    while !ready(&app) && Instant::now() < deadline {
        app.poll_lsp();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ready(&app), "both servers should complete the handshake");
    assert_eq!(app.lsp_status(), LspStatus::Ready);
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn language_server_answers_hover_completion_and_definition() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // A mock server that answers each feature request by method.
    const MOCK: &str = r#"#!/bin/sh
target="$1"
while read -r header; do
  header=$(printf '%s' "$header" | tr -d '\r')
  case "$header" in
    Content-Length:*) len=${header#Content-Length: } ;;
    *) continue ;;
  esac
  read -r blank
  body=$(dd bs=1 count="$len" 2>/dev/null)
  id=$(printf '%s' "$body" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  method=$(printf '%s' "$body" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  [ -z "$id" ] && continue
  case "$method" in
    initialize) result='{"capabilities":{"completionProvider":{"resolveProvider":true},"signatureHelpProvider":{},"hoverProvider":true,"definitionProvider":true,"referencesProvider":true,"renameProvider":true,"codeActionProvider":true,"workspaceSymbolProvider":true,"documentHighlightProvider":true,"inlayHintProvider":true,"codeLensProvider":true,"selectionRangeProvider":true,"callHierarchyProvider":true,"typeHierarchyProvider":true,"implementationProvider":true,"typeDefinitionProvider":true,"documentLinkProvider":true,"documentRangeFormattingProvider":true}}' ;;
    textDocument/documentHighlight) result='[{"range":{"start":{"line":1,"character":8},"end":{"line":1,"character":13}},"kind":1}]' ;;
    textDocument/inlayHint) result='[{"position":{"line":1,"character":4},"label":": i32","kind":1,"paddingLeft":true}]' ;;
    textDocument/codeLens) result='[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":8}},"command":{"title":"run test","command":"test.run","arguments":[]}}]' ;;
    textDocument/selectionRange) result='[{"range":{"start":{"line":2,"character":4},"end":{"line":2,"character":9}},"parent":{"range":{"start":{"line":2,"character":4},"end":{"line":2,"character":10}},"parent":{"range":{"start":{"line":0,"character":0},"end":{"line":3,"character":1}}}}}]' ;;
    textDocument/prepareCallHierarchy) result='[{"name":"value","kind":12,"detail":"detail","uri":"file://$target","range":{"start":{"line":0,"character":0},"end":{"line":0,"character":5}},"selectionRange":{"start":{"line":0,"character":4},"end":{"line":0,"character":9}}}]' ;;
    callHierarchy/incomingCalls) result='[{"from":{"name":"main","kind":12,"detail":"detail","uri":"file://$target","range":{"start":{"line":0,"character":0},"end":{"line":0,"character":5}},"selectionRange":{"start":{"line":0,"character":4},"end":{"line":0,"character":9}}},"fromRanges":[]}]' ;;
    textDocument/prepareTypeHierarchy) result='[{"name":"Thing","kind":12,"detail":"detail","uri":"file://$target","range":{"start":{"line":2,"character":0},"end":{"line":2,"character":5}},"selectionRange":{"start":{"line":2,"character":4},"end":{"line":2,"character":9}}}]' ;;
    typeHierarchy/supertypes) result='[{"name":"Base","kind":12,"detail":"detail","uri":"file://$target","range":{"start":{"line":1,"character":0},"end":{"line":1,"character":5}},"selectionRange":{"start":{"line":1,"character":4},"end":{"line":1,"character":9}}}]' ;;
    textDocument/signatureHelp) result='{"signatures":[{"label":"fn first(a: i32)","parameters":[{"label":"a: i32","documentation":"first parameter"}],"documentation":"First overload"},{"label":"fn second(a: i32, b: i32)","parameters":[{"label":"a: i32","documentation":"first parameter"},{"label":"b: i32","documentation":"second parameter"}],"documentation":"Second overload"}],"activeSignature":1,"activeParameter":1}' ;;
    textDocument/hover) result='{"contents":{"kind":"plaintext","value":"LSP HOVER TEXT"}}' ;;
    textDocument/completion) result='{"items":[{"label":"kemori_lsp_item","kind":3}]}' ;;
    completionItem/resolve) result='{"label":"kemori_lsp_item","detail":"resolved detail","documentation":{"kind":"markdown","value":"RESOLVED DOC"}}' ;;
    textDocument/definition) result="[{\"uri\":\"file://$target\",\"range\":{\"start\":{\"line\":2,\"character\":0},\"end\":{\"line\":2,\"character\":5}}}]" ;;
    textDocument/implementation) result="[{\"uri\":\"file://$target\",\"range\":{\"start\":{\"line\":3,\"character\":0},\"end\":{\"line\":3,\"character\":1}}}]" ;;
    textDocument/typeDefinition) result="[{\"uri\":\"file://$target\",\"range\":{\"start\":{\"line\":1,\"character\":4},\"end\":{\"line\":1,\"character\":9}}}]" ;;
    textDocument/documentLink) result="[{\"range\":{\"start\":{\"line\":0,\"character\":0},\"end\":{\"line\":0,\"character\":8}},\"target\":\"https://example.com\",\"tooltip\":\"Example\"},{\"range\":{\"start\":{\"line\":1,\"character\":0},\"end\":{\"line\":1,\"character\":4}},\"target\":\"file://$target\"}]" ;;
    textDocument/rangeFormatting) result='[{"range":{"start":{"line":1,"character":4},"end":{"line":1,"character":9}},"newText":"range"}]' ;;
    textDocument/references) result="[{\"uri\":\"file://$target\",\"range\":{\"start\":{\"line\":1,\"character\":4},\"end\":{\"line\":1,\"character\":9}}}]" ;;
    textDocument/rename) result="{\"changes\":{\"file://$target\":[{\"range\":{\"start\":{\"line\":1,\"character\":8},\"end\":{\"line\":1,\"character\":13}},\"newText\":\"renamed\"}]}}" ;;
    textDocument/codeAction) result="[{\"title\":\"Apply fix\",\"edit\":{\"changes\":{\"file://$target\":[{\"range\":{\"start\":{\"line\":0,\"character\":0},\"end\":{\"line\":0,\"character\":0}},\"newText\":\"FIX \"}]}}}]" ;;
    workspace/symbol) result="[{\"name\":\"kemori_ws_symbol\",\"kind\":12,\"location\":{\"uri\":\"file://$target\",\"range\":{\"start\":{\"line\":3,\"character\":0},\"end\":{\"line\":3,\"character\":3}}}}]" ;;
    *) result='null' ;;
  esac
  resp=$(printf '{"jsonrpc":"2.0","id":%s,"result":%s}' "$id" "$result")
  printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
done
"#;

    let dir = temp_project("lsp-features");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {\n    let value = 1;\n    value;\n}\n").unwrap();
    let script = dir.join("mock-lsp.sh");
    {
        let mut handle = fs::File::create(&script).unwrap();
        handle.write_all(MOCK.as_bytes()).unwrap();
    }
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();

    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .set_language(LanguageId::Rust);
    app.start_lsp(
        LanguageId::Rust,
        script.to_str().unwrap(),
        &[file.to_str().unwrap()],
    );
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(2, 4));

    let wait = |app: &mut App, predicate: &dyn Fn(&App) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            app.poll_lsp();
            if predicate(app) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    };

    assert!(
        wait(&mut app, &|app| app
            .lsp
            .get(&(app.workspace.root().to_path_buf(), LanguageId::Rust))
            .and_then(|job| job.server.as_ref())
            .is_some_and(|server| server.is_ready())),
        "server should become ready"
    );

    // Document highlights for the symbol at the cursor.
    assert!(
        app.lsp
            .get(&(app.workspace.root().to_path_buf(), LanguageId::Rust))
            .and_then(|job| job.server.as_ref())
            .is_some_and(|server| server.supports(RequestKind::DocumentHighlight)),
        "server should advertise document highlights"
    );
    app.request_document_highlights();
    assert!(
        wait(&mut app, &|app| app.editor.active_document().is_some_and(
            |doc| doc
                .document_highlights()
                .iter()
                .any(|(start, end)| *start == Position::new(1, 8) && *end == Position::new(1, 13))
        )),
        "expected the server's document highlights"
    );

    // Inlay hints.
    assert!(
        wait(&mut app, &|app| app.editor.active_document().is_some_and(
            |doc| doc
                .inlay_hints()
                .iter()
                .any(|hint| hint.position == Position::new(1, 4) && hint.label == " : i32")
        )),
        "expected the server's inlay hint"
    );

    // Code lenses.
    assert!(
        wait(&mut app, &|app| app.editor.active_document().is_some_and(
            |doc| {
                doc.code_lenses().iter().any(|lens| {
                    lens.title == "run test" && lens.command.as_deref() == Some("test.run")
                })
            }
        )),
        "expected the server's code lens"
    );
    app.execute_command(ids::CODE_LENS);
    assert!(
        matches!(&app.overlay, Overlay::Picker(picker) if picker.title == "Code Lens"),
        "expected the code lens picker"
    );
    app.overlay = Overlay::None;

    // Selection ranges: expand twice, then shrink once.
    assert!(
        wait(&mut app, &|app| app
            .editor
            .active_document()
            .is_some_and(|doc| doc.selection_ranges().len() == 3)),
        "expected the server's selection-range chain"
    );
    app.execute_command(ids::EXPAND_SELECTION);
    assert_eq!(
        app.editor.active_document().unwrap().selection_range(),
        Some((Position::new(2, 4), Position::new(2, 9)))
    );
    app.execute_command(ids::EXPAND_SELECTION);
    assert_eq!(
        app.editor.active_document().unwrap().selection_range(),
        Some((Position::new(2, 4), Position::new(2, 10)))
    );
    app.execute_command(ids::SHRINK_SELECTION);
    assert_eq!(
        app.editor.active_document().unwrap().selection_range(),
        Some((Position::new(2, 4), Position::new(2, 9)))
    );
    // Leave the selection behind for the cursor-driven features below.
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(2, 4));

    // Call hierarchy: incoming calls open a jumpable picker.
    app.execute_command(ids::INCOMING_CALLS);
    assert!(
        wait(&mut app, &|app| matches!(
            &app.overlay,
            Overlay::Picker(picker) if picker.title == "Incoming Calls"
        )),
        "expected the incoming-calls picker"
    );
    app.overlay = Overlay::None;

    // Type hierarchy: supertypes open the same kind of picker.
    app.execute_command(ids::SUPERTYPES);
    assert!(
        wait(&mut app, &|app| matches!(
            &app.overlay,
            Overlay::Picker(picker) if picker.title == "Supertypes"
        )),
        "expected the supertypes picker"
    );
    app.overlay = Overlay::None;

    // Hover.
    app.open_hover();
    assert!(
        wait(&mut app, &|app| app
            .hover
            .as_ref()
            .is_some_and(|hover| hover.title.contains("LSP HOVER"))),
        "expected the server's hover"
    );

    // Completion.
    app.hover = None;
    app.open_completion();
    assert!(
        wait(&mut app, &|app| app.completion.as_ref().is_some_and(
            |state| state
                .items
                .iter()
                .any(|item| item.label == "kemori_lsp_item")
        )),
        "expected the server's completion"
    );

    // Completion resolve fills in documentation for the selected item.
    if let Some(state) = app.completion.as_mut()
        && let Some(index) = state
            .items
            .iter()
            .position(|item| item.label == "kemori_lsp_item")
    {
        state.selected = index;
    }
    app.resolve_selected_completion();
    assert!(
        wait(&mut app, &|app| app.completion.as_ref().is_some_and(
            |state| state
                .items
                .iter()
                .any(|item| item.documentation.contains("RESOLVED DOC"))
        )),
        "expected resolved completion documentation"
    );

    // Signature help keeps overloads, the active parameter and docs.
    app.request_lsp_signature();
    assert!(
        wait(&mut app, &|app| app.signature.as_ref().is_some_and(
            |state| state.help.signatures.len() == 2
        )),
        "expected two signature overloads"
    );
    {
        let help = &app.signature.as_ref().unwrap().help;
        assert_eq!(help.active, 1);
        assert_eq!(help.parameter, 1);
        assert_eq!(
            help.signatures[1].parameter_documentation[1],
            "second parameter"
        );
    }
    app.cycle_signature(1);
    assert_eq!(app.signature.as_ref().unwrap().help.active, 0);
    app.signature = None;

    // Definition (same file, line 2 column 0).
    app.completion = None;
    app.goto_definition();
    assert!(
        wait(&mut app, &|app| app.editor.active_document().is_some_and(
            |doc| doc.clamped_cursor() == Position::new(2, 0)
        )),
        "expected the server's definition to move the cursor"
    );

    // Implementation and type definition.
    app.execute_command(ids::IMPLEMENTATION);
    assert!(
        wait(&mut app, &|app| app.editor.active_document().is_some_and(
            |doc| doc.clamped_cursor() == Position::new(3, 0)
        )),
        "expected the implementation location"
    );
    app.execute_command(ids::TYPE_DEFINITION);
    assert!(
        wait(&mut app, &|app| app.editor.active_document().is_some_and(
            |doc| doc.clamped_cursor() == Position::new(1, 4)
        )),
        "expected the type-definition location"
    );

    // Document links open a picker with the server's targets.
    app.execute_command(ids::DOCUMENT_LINKS);
    assert!(
        wait(&mut app, &|app| matches!(
            &app.overlay,
            Overlay::Picker(picker) if picker.title == "Document Links"
        )),
        "expected the document-links picker"
    );
    app.overlay = Overlay::None;

    // References (the server returns one location).
    app.find_references();
    assert!(
        wait(&mut app, &|app| matches!(app.overlay, Overlay::Picker(_))),
        "expected the references picker"
    );

    // Rename (the server returns a workspace edit).
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(1, 8));
    app.rename_symbol();
    assert!(
        matches!(app.overlay, Overlay::Prompt(_)),
        "rename should prompt for a new name"
    );
    app.submit_prompt(PromptKind::Rename, "renamed".to_string());
    assert!(
        wait(&mut app, &|app| app.editor.active_document().is_some_and(
            |doc| doc.buffer.line_text(1).contains("renamed")
        )),
        "expected the rename edit to apply"
    );
    assert_eq!(
        app.editor.active_document().unwrap().buffer.line_text(1),
        "    let renamed = 1;"
    );

    // Code actions (the server returns one edit-based action).
    app.code_actions();
    assert!(
        wait(&mut app, &|app| matches!(app.overlay, Overlay::Picker(_))),
        "expected the code actions picker"
    );
    app.apply_code_action(0);
    assert!(
        wait(&mut app, &|app| app.editor.active_document().is_some_and(
            |doc| doc.buffer.line_text(0).starts_with("FIX ")
        )),
        "expected the code action edit to apply"
    );

    // Workspace symbols: the server's results are merged into the picker.
    app.open_workspace_symbols();
    assert!(
        wait(&mut app, &|app| match &app.overlay {
            Overlay::Picker(picker) => (0..picker.filtered.len())
                .filter_map(|index| picker.item(index))
                .any(|item| item.label == "kemori_ws_symbol"),
            _ => false,
        }),
        "expected the server's workspace symbol"
    );

    // Format Selection: the server returns one range edit.
    app.editor.active_document_mut().unwrap().selection =
        Some(Selection::new(Position::new(1, 4)));
    app.editor.active_document_mut().unwrap().cursor = Position::new(1, 9);
    app.execute_command(ids::FORMAT_SELECTION);
    assert!(
        wait(&mut app, &|app| app.editor.active_document().is_some_and(
            |doc| doc.buffer.line_text(1).contains("range")
        )),
        "expected the range formatting edit to apply"
    );

    // Organize Imports: the server's action edit is applied.
    app.execute_command(ids::ORGANIZE_IMPORTS);
    assert!(
        wait(&mut app, &|app| app.editor.active_document().is_some_and(
            |doc| doc.buffer.line_text(0).matches("FIX ").count() >= 2
        )),
        "expected organize-imports to apply its edit"
    );

    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn the_palette_advertises_server_served_commands() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // A mock server that advertises rename and code actions — enough to prove
    // the palette consults the attached server, not just the provider's
    // built-in capability list.
    const MOCK: &str = r#"#!/bin/sh
while read -r header; do
  header=$(printf '%s' "$header" | tr -d '\r')
  case "$header" in
    Content-Length:*) len=${header#Content-Length: } ;;
    *) continue ;;
  esac
  read -r blank
  body=$(dd bs=1 count="$len" 2>/dev/null)
  id=$(printf '%s' "$body" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  method=$(printf '%s' "$body" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  [ -z "$id" ] && continue
  case "$method" in
    initialize) result='{"capabilities":{"renameProvider":true,"codeActionProvider":true}}' ;;
    *) result='null' ;;
  esac
  resp=$(printf '{"jsonrpc":"2.0","id":%s,"result":%s}' "$id" "$result")
  printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
done
"#;

    let dir = temp_project("palette-server-backed");
    // Python's provider declares no Rename or CodeActions capability, so only
    // an attached server can make those commands available.
    let file = dir.join("src/main.py");
    fs::write(&file, "def main():\n    pass\n").unwrap();
    let script = dir.join("mock-lsp.sh");
    {
        let mut handle = fs::File::create(&script).unwrap();
        handle.write_all(MOCK.as_bytes()).unwrap();
    }
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();

    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .set_language(LanguageId::Python);

    let enabled_for = |app: &App, needle: &str| -> bool {
        let Overlay::Picker(picker) = &app.overlay else {
            panic!("expected the command palette");
        };
        (0..picker.filtered.len())
            .filter_map(|index| picker.item(index))
            .find(|item| item.label.contains(needle))
            .map(|item| item.enabled)
            .unwrap_or(false)
    };

    // Without a server, rename and code actions are unavailable for Python.
    app.execute_command(ids::PALETTE);
    assert!(
        !enabled_for(&app, "Rename Symbol"),
        "rename must stay hidden without a server"
    );
    assert!(
        !enabled_for(&app, "Code Actions"),
        "code actions must stay hidden without a server"
    );

    let wait = |app: &mut App, predicate: &dyn Fn(&App) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            app.poll_lsp();
            if predicate(app) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    };
    app.start_lsp(
        LanguageId::Python,
        script.to_str().unwrap(),
        &[file.to_str().unwrap()],
    );
    assert!(
        wait(&mut app, &|app| app
            .lsp
            .get(&(app.workspace.root().to_path_buf(), LanguageId::Python))
            .and_then(|job| job.server.as_ref())
            .is_some_and(|server| server.is_ready())),
        "server should become ready"
    );

    // The server advertises both requests, so the palette advertises them even
    // though the provider never did.
    app.execute_command(ids::PALETTE);
    assert!(
        enabled_for(&app, "Rename Symbol"),
        "an attached rename server must un-hide rename"
    );
    assert!(
        enabled_for(&app, "Code Actions"),
        "an attached code-action server must un-hide code actions"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn captures_and_restores_open_files() {
    let dir = temp_project("session-restore");
    let a = dir.join("src/main.rs");
    let b = dir.join("src/lib.rs");
    fs::write(&b, "pub fn lib() {\n    let x = 1;\n}\n").unwrap();

    let mut app = app_with_file(&a);
    app.open_path(b.clone());
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(1, 4));

    let session = app.capture_session();
    assert_eq!(session.files.len(), 2);
    assert_eq!(session.active, 1);

    let mut fresh = App::new(Some(&dir)).unwrap();
    fresh.restore_session(session);
    assert_eq!(fresh.editor.len(), 2);
    assert_eq!(fresh.editor.active_index(), 1);
    assert_eq!(
        fresh.editor.active_document().unwrap().clamped_cursor(),
        Position::new(1, 4)
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn reloads_clean_files_changed_on_disk() {
    let dir = temp_project("external-clean");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    std::thread::sleep(Duration::from_millis(10));
    fs::write(&file, "fn changed() {}\n").unwrap();
    app.last_disk_check = Instant::now() - Duration::from_secs(5);

    assert!(app.poll_external_changes());
    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "fn changed() {}\n"
    );
    fs::remove_dir_all(&dir).ok();
}
