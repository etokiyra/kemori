#[cfg(unix)]
#[test]
fn did_save_and_watched_files_use_server_registrations() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // A mock server that asks for watched-file registration during initialize,
    // then logs didSave and watched-file notifications.
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
  if [ "$method" = "textDocument/didSave" ] || [ "$method" = "workspace/didChangeWatchedFiles" ]; then
    printf '%s\n' "$body" >> "$log"
  fi
  if [ "$method" = "initialize" ]; then
    resp='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"textDocumentSync":{"change":1,"save":{"includeText":true}}}}}'
    printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
    req='{"jsonrpc":"2.0","id":900,"method":"client/registerCapability","params":{"registrations":[{"id":"watch-1","method":"workspace/didChangeWatchedFiles","registerOptions":{"watchers":[{"globPattern":"**/*.rs"}]}}]}}'
    printf 'Content-Length: %s\r\n\r\n%s' "${#req}" "$req"
    continue
  fi
  [ -z "$method" ] && continue
  [ -z "$id" ] && continue
  resp=$(printf '{"jsonrpc":"2.0","id":%s,"result":null}' "$id")
  printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
done
"#;

    let dir = temp_project("lsp-save-watch");
    let file = dir.join("src/main.rs");
    let log = dir.join("events.log");
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

    // Save the open document: the server asked for text, so it must arrive.
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    app.save();
    app.poll_lsp();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saved = false;
    while Instant::now() < deadline && !saved {
        saved = fs::read_to_string(&log)
            .map(|log| {
                log.contains("textDocument/didSave")
                    && log.contains("main.rs")
                    && log.contains("\"text\":")
            })
            .unwrap_or(false);
        if !saved {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    assert!(saved, "the save notification should include the text");

    // Creating a matching file must reach the registered watcher.
    app.submit_prompt(PromptKind::NewFile, "src/extra.rs".to_string());
    app.poll_lsp();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut watched = false;
    while Instant::now() < deadline && !watched {
        watched = fs::read_to_string(&log)
            .map(|log| {
                log.contains("workspace/didChangeWatchedFiles")
                    && log.contains("extra.rs")
                    && log.contains("\"type\":1")
            })
            .unwrap_or(false);
        if !watched {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    assert!(watched, "the registered watcher should see the new file");

    fs::remove_dir_all(&dir).ok();
}
