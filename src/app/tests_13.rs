#[cfg(unix)]
#[test]
fn workspace_folders_and_progress_are_handled() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // Log initialize, create a progress token, report begin/report, and finish
    // the token only when the test sends a completion request.
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
  if [ "$method" = "initialize" ]; then
    printf '%s\n' "$body" >> "$log"
    resp='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}'
    printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
    req='{"jsonrpc":"2.0","id":700,"method":"window/workDoneProgress/create","params":{"token":"progress-1"}}'
    printf 'Content-Length: %s\r\n\r\n%s' "${#req}" "$req"
    begin='{"jsonrpc":"2.0","method":"$/progress","params":{"token":"progress-1","value":{"kind":"begin","title":"Indexing"}}}'
    printf 'Content-Length: %s\r\n\r\n%s' "${#begin}" "$begin"
    report='{"jsonrpc":"2.0","method":"$/progress","params":{"token":"progress-1","value":{"kind":"report","message":"half","percentage":40}}}'
    printf 'Content-Length: %s\r\n\r\n%s' "${#report}" "$report"
    continue
  fi
  if [ "$method" = "textDocument/completion" ]; then
    resp=$(printf '{"jsonrpc":"2.0","id":%s,"result":null}' "$id")
    printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
    end='{"jsonrpc":"2.0","method":"$/progress","params":{"token":"progress-1","value":{"kind":"end"}}}'
    printf 'Content-Length: %s\r\n\r\n%s' "${#end}" "$end"
    continue
  fi
  [ -z "$method" ] && continue
  [ -z "$id" ] && continue
  resp=$(printf '{"jsonrpc":"2.0","id":%s,"result":null}' "$id")
  printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
done
"#;

    let dir = temp_project("lsp-progress");
    let file = dir.join("src/main.rs");
    let log = dir.join("initialize.log");
    let script = dir.join("mock-lsp.sh");
    {
        let mut handle = fs::File::create(&script).unwrap();
        handle.write_all(MOCK.as_bytes()).unwrap();
    }
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();

    let mut app = app_with_file(&file);
    app.lsp.clear();
    app.start_lsp(
        LanguageId::Rust,
        script.to_str().unwrap(),
        &[log.to_str().unwrap()],
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut progress = None;
    while Instant::now() < deadline
        && !progress
            .as_ref()
            .is_some_and(|progress: &crate::language::lsp::LspProgress| progress.percentage == Some(40))
    {
        app.poll_lsp();
        progress = app.lsp_progress.values().next().cloned();
        std::thread::sleep(Duration::from_millis(5));
    }
    let progress = progress.expect("the server's progress report");
    assert_eq!(progress.title, "Indexing");
    assert_eq!(progress.percentage, Some(40));

    let initialized = fs::read_to_string(&log).expect("initialize log");
    assert!(
        initialized.contains("\"workspaceFolders\":[")
            && initialized.contains("\"name\":\"workspace\""),
        "initialize should advertise the workspace folder: {initialized}"
    );

    // The mock finishes the progress token when it receives this request.
    {
        let server = app
            .lsp
            .get_mut(&(app.workspace.root().to_path_buf(), LanguageId::Rust))
            .and_then(|job| job.server.as_mut())
            .expect("mock server");
        server.completion(&file, 0, 0).expect("completion request");
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !app.lsp_progress.is_empty() {
        app.poll_lsp();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        app.lsp_progress.is_empty(),
        "the end progress notification should clear the token"
    );
    fs::remove_dir_all(&dir).ok();
}
