#[cfg(unix)]
#[test]
fn superseded_requests_are_cancelled() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // The first completion is left unanswered; the second must cancel it.
    const MOCK: &str = r#"#!/bin/sh
log="$1"
count=0
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
  if [ "$method" = "$/cancelRequest" ]; then
    printf '%s\n' "$body" >> "$log"
    continue
  fi
  if [ "$method" = "textDocument/completion" ]; then
    count=$((count + 1))
    if [ "$count" -eq 1 ]; then
      continue
    fi
  fi
  [ -z "$id" ] && continue
  resp=$(printf '{"jsonrpc":"2.0","id":%s,"result":null}' "$id")
  printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
done
"#;

    let dir = temp_project("lsp-cancel");
    let file = dir.join("src/main.rs");
    let log = dir.join("cancel.log");
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

    let first = {
        let server = app
            .lsp
            .get_mut(&(app.workspace.root().to_path_buf(), LanguageId::Rust))
            .and_then(|job| job.server.as_mut())
            .expect("mock server");
        server.completion(&file, 0, 0).expect("first request")
    };
    let second = {
        let server = app
            .lsp
            .get_mut(&(app.workspace.root().to_path_buf(), LanguageId::Rust))
            .and_then(|job| job.server.as_mut())
            .expect("mock server");
        server.completion(&file, 0, 0).expect("second request")
    };
    assert_ne!(first, second);

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut cancelled = false;
    while Instant::now() < deadline && !cancelled {
        cancelled = fs::read_to_string(&log)
            .map(|log| log.contains("$/cancelRequest") && log.contains(&format!("{first}")))
            .unwrap_or(false);
        if !cancelled {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    assert!(cancelled, "the superseded request should be cancelled");
    fs::remove_dir_all(&dir).ok();
}
