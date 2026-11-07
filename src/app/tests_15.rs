#[cfg(unix)]
#[test]
fn accepting_a_snippet_completion_expands_tab_stops() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

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
  if [ "$method" = "initialize" ]; then
    result='{"capabilities":{"completionProvider":true}}'
  elif [ "$method" = "textDocument/completion" ]; then
    result='{"items":[{"label":"fn_item","kind":3,"insertTextFormat":2,"textEdit":{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"newText":"fn ${1:name}() {\n    $0\n}"}}]}'
  else
    result='null'
  fi
  [ -z "$id" ] && continue
  resp=$(printf '{"jsonrpc":"2.0","id":%s,"result":%s}' "$id" "$result")
  printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
done
"#;

    let dir = temp_project("lsp-snippet-completion");
    let file = dir.join("src/empty.rs");
    fs::write(&file, "").unwrap();
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
    app.start_lsp(LanguageId::Rust, script.to_str().unwrap(), &[]);
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

    app.open_completion();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut ready = false;
    while Instant::now() < deadline && !ready {
        app.poll_lsp();
        ready = app.completion.as_ref().is_some_and(|state| {
            state.items.iter().any(|item| item.label == "fn_item")
        });
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(ready, "expected the snippet completion item");

    if let Some(state) = app.completion.as_mut()
        && let Some(index) = state
            .items
            .iter()
            .position(|item| item.label == "fn_item")
    {
        state.selected = index;
    }
    app.accept_completion();

    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "fn name() {\n    \n}"
    );
    assert_eq!(
        app.editor.active_document().unwrap().clamped_cursor(),
        Position::new(0, 3)
    );
    assert!(app.snippet.is_some(), "snippet tab stops should be active");
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(
        app.editor.active_document().unwrap().clamped_cursor(),
        Position::new(1, 4)
    );
    fs::remove_dir_all(&dir).ok();
}
