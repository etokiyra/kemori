use super::*;

#[cfg(unix)]
const MOCK_ADAPTER: &str = r#"#!/bin/sh
seq=100
while IFS= read -r header; do
  [ -z "$header" ] && continue
  len=$(printf '%s' "$header" | tr -dc '0-9')
  IFS= read -r blank
  body=$(dd bs=1 count="$len" 2>/dev/null)
  req=$(printf '%s' "$body" | sed -n 's/.*"seq":\([0-9]*\).*/\1/p')
  cmd=$(printf '%s' "$body" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')
  case "$cmd" in
    initialize)
      resp="{\"seq\":$seq,\"type\":\"response\",\"request_seq\":$req,\"success\":true,\"command\":\"initialize\",\"body\":{}}"
      seq=$((seq+1))
      printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
      ev="{\"seq\":$seq,\"type\":\"event\",\"event\":\"initialized\"}"
      seq=$((seq+1))
      printf 'Content-Length: %s\r\n\r\n%s' "${#ev}" "$ev"
      ;;
    launch|attach)
      resp="{\"seq\":$seq,\"type\":\"response\",\"request_seq\":$req,\"success\":true,\"command\":\"launch\",\"body\":{}}"
      seq=$((seq+1))
      printf 'Content-Length: %s\r\n\r\n%s' "${#resp}" "$resp"
      ev="{\"seq\":$seq,\"type\":\"event\",\"event\":\"stopped\",\"body\":{\"reason\":\"breakpoint\",\"threadId\":1}}"
      seq=$((seq+1))
      printf 'Content-Length: %s\r\n\r\n%s' "${#ev}" "$ev"
      ;;
  esac
done
"#;

#[cfg(unix)]
fn serve_mock_dap(listener: std::net::TcpListener) {
    use std::io::BufReader;
    let (stream, _) = listener.accept().expect("accept the DAP client");
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;
    while let Ok(Some(message)) = crate::debug::protocol::read_message(&mut reader) {
        if message.get("type").and_then(Value::as_str) != Some("request") {
            continue;
        }
        let id = message.get("seq").and_then(Value::as_i64).unwrap_or(0);
        let command = message.get("command").and_then(Value::as_str).unwrap_or("");
        let response = json!({
            "seq": id + 1000,
            "type": "response",
            "request_seq": id,
            "success": true,
            "command": command,
            "body": {},
        });
        let _ = crate::debug::protocol::write_message(&mut writer, &response);
        if command == "initialize" {
            let _ = crate::debug::protocol::write_message(
                &mut writer,
                &json!({ "seq": id + 2000, "type": "event", "event": "initialized" }),
            );
        }
        if command == "launch" {
            let _ = crate::debug::protocol::write_message(
                &mut writer,
                &json!({
                    "seq": id + 3000,
                    "type": "event",
                    "event": "stopped",
                    "body": { "reason": "breakpoint", "threadId": 1 }
                }),
            );
        }
    }
}

#[test]
fn parses_stack_scopes_and_variables() {
    let frames = parse_stack_frames(&json!({
        "stackFrames": [
            { "id": 3, "name": "main", "line": 4, "column": 2,
              "source": { "path": "/tmp/main.py" } },
            { "id": 4, "name": "<module>", "line": 1, "column": 1 }
        ]
    }));
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].id, 3);
    assert_eq!(frames[0].path, Some(PathBuf::from("/tmp/main.py")));
    assert_eq!(frames[0].line, 4);

    let scopes = parse_scopes(&json!({
        "scopes": [
            { "name": "Locals", "variablesReference": 5 },
            { "name": "Globals", "variablesReference": 0 }
        ]
    }));
    assert_eq!(scopes.len(), 1);
    assert_eq!(scopes[0].name, "Locals");

    let variables = parse_variables(&json!({
        "variables": [
            { "name": "x", "value": "42", "type": "int" }
        ]
    }));
    assert_eq!(variables[0].name, "x");
    assert_eq!(variables[0].value, "42");
    assert_eq!(variables[0].kind, "int");
}

#[cfg(unix)]
#[test]
fn drives_a_mock_adapter_to_a_stopped_event() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let dir = std::env::temp_dir().join(format!("kemori-dap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("adapter.sh");
    std::fs::write(&script, MOCK_ADAPTER).unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let mut session =
        DebugSession::start(script.to_str().unwrap(), &[], &dir).expect("spawn adapter");
    session.initialize("mock").unwrap();

    let mut initialized = false;
    let mut stopped = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !stopped {
        for event in session.poll() {
            match event {
                DebugEvent::Initialized => {
                    initialized = true;
                    session
                        .launch(Path::new("/tmp/program"), &dir, None)
                        .unwrap();
                    session.configuration_done().unwrap();
                }
                DebugEvent::Stopped {
                    reason, thread_id, ..
                } => {
                    assert_eq!(reason, "breakpoint");
                    assert_eq!(thread_id, Some(1));
                    stopped = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(initialized, "the initialized event should arrive");
    assert!(stopped, "the adapter should stop on a breakpoint");
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn drives_an_lldb_plan_to_a_stopped_event() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let dir = std::env::temp_dir().join(format!("kemori-lldb-dap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("lldb-dap");
    std::fs::write(&script, MOCK_ADAPTER).unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let plan = super::super::lldb_plan(PathBuf::from("/tmp/program"), script.clone());
    let mut session = DebugSession::start(&plan.adapter, &plan.adapter_args, &dir)
        .expect("spawn the mock lldb-dap");
    session.initialize(&plan.adapter_id).unwrap();

    let mut stopped = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !stopped {
        for event in session.poll() {
            match event {
                DebugEvent::Initialized => {
                    session
                        .launch(&plan.program, &dir, plan.launch_mode)
                        .unwrap();
                    session.configuration_done().unwrap();
                }
                DebugEvent::Stopped {
                    reason, thread_id, ..
                } => {
                    assert_eq!(reason, "breakpoint");
                    assert_eq!(thread_id, Some(1));
                    stopped = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(stopped, "the lldb plan should stop on a breakpoint");
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn drives_a_tcp_dlv_plan_to_a_stopped_event() {
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a mock DAP server");
    let port = listener.local_addr().unwrap().port();
    let dir = std::env::temp_dir().join(format!("kemori-dlv-dap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("dlv");
    std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf 'DAP server listening at: 127.0.0.1:{port}\\n'\nwhile :; do sleep 1; done\n"
            ),
        )
        .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let _server = std::thread::spawn(move || serve_mock_dap(listener));

    let plan = super::super::go_plan_with(
        Path::new("/tmp/project/cmd/app/main.go"),
        &dir,
        script.clone(),
    );
    assert_eq!(plan.transport, super::super::LaunchTransport::Tcp);
    let mut session =
        DebugSession::start_tcp_with_env(&plan.adapter, &plan.adapter_args, &dir, &plan.env)
            .expect("start the TCP adapter");
    session.initialize(&plan.adapter_id).unwrap();

    let mut stopped = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !stopped {
        for event in session.poll() {
            match event {
                DebugEvent::Initialized => {
                    session
                        .launch(&plan.program, &dir, plan.launch_mode)
                        .unwrap();
                    session.configuration_done().unwrap();
                }
                DebugEvent::Stopped {
                    reason, thread_id, ..
                } => {
                    assert_eq!(reason, "breakpoint");
                    assert_eq!(thread_id, Some(1));
                    stopped = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(stopped, "the dlv TCP plan should stop on a breakpoint");
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn attaches_through_the_mock_adapter() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let dir = std::env::temp_dir().join(format!("kemori-attach-dap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("debugpy-adapter");
    std::fs::write(&script, MOCK_ADAPTER).unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let target = super::super::AttachTarget {
        pid: None,
        host: "127.0.0.1".to_string(),
        port: 5678,
    };
    let mut plan = super::super::python_plan(Path::new("/tmp/main.py"), Some(script));
    plan.attach = Some(target.clone());
    let mut session = DebugSession::start(&plan.adapter, &plan.adapter_args, &dir)
        .expect("spawn the mock attach adapter");
    session.initialize(&plan.adapter_id).unwrap();

    let mut stopped = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !stopped {
        for event in session.poll() {
            match event {
                DebugEvent::Initialized => {
                    session
                        .attach(super::super::attach_arguments(&plan.adapter_id, &target))
                        .unwrap();
                    session.configuration_done().unwrap();
                }
                DebugEvent::Stopped {
                    reason, thread_id, ..
                } => {
                    assert_eq!(reason, "breakpoint");
                    assert_eq!(thread_id, Some(1));
                    stopped = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(stopped, "attach should reach a stopped event");
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn drives_a_php_plan_to_a_stopped_event() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let dir = std::env::temp_dir().join(format!("kemori-php-dap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("php-debug-dap");
    std::fs::write(&script, MOCK_ADAPTER).unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let plan = super::super::php_plan_with(Path::new("/tmp/project/index.php"), script.clone());
    let mut session = DebugSession::start(&plan.adapter, &plan.adapter_args, &dir)
        .expect("spawn the mock PHP bridge");
    session.initialize(&plan.adapter_id).unwrap();

    let mut stopped = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !stopped {
        for event in session.poll() {
            match event {
                DebugEvent::Initialized => {
                    session.launch_php(&plan.program, &dir).unwrap();
                    session.configuration_done().unwrap();
                }
                DebugEvent::Stopped {
                    reason, thread_id, ..
                } => {
                    assert_eq!(reason, "breakpoint");
                    assert_eq!(thread_id, Some(1));
                    stopped = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(stopped, "the PHP bridge should stop on a breakpoint");
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn drives_a_ruby_rdbg_plan_to_a_stopped_event() {
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a mock DAP server");
    let port = listener.local_addr().unwrap().port();
    let dir = std::env::temp_dir().join(format!("kemori-rdbg-dap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("rdbg");
    std::fs::write(&script, "#!/bin/sh\nwhile :; do sleep 1; done\n").unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let _server = std::thread::spawn(move || serve_mock_dap(listener));
    let plan = super::super::ruby_plan_with(Path::new("/tmp/project/app.rb"), script.clone(), port);
    let mut session = DebugSession::start_tcp_at_with_env(
        &plan.adapter,
        &plan.adapter_args,
        &dir,
        plan.tcp_address.as_deref(),
        &plan.env,
    )
    .expect("start the rdbg adapter");
    session.initialize(&plan.adapter_id).unwrap();

    let mut stopped = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !stopped {
        for event in session.poll() {
            match event {
                DebugEvent::Initialized => {
                    session
                        .launch(&plan.program, &dir, plan.launch_mode)
                        .unwrap();
                    session.configuration_done().unwrap();
                }
                DebugEvent::Stopped {
                    reason, thread_id, ..
                } => {
                    assert_eq!(reason, "breakpoint");
                    assert_eq!(thread_id, Some(1));
                    stopped = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(stopped, "the rdbg plan should stop on a breakpoint");
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn drives_a_java_launch_to_a_stopped_event() {
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a mock DAP server");
    let port = listener.local_addr().unwrap().port();
    let dir = std::env::temp_dir().join(format!("kemori-java-dap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let _server = std::thread::spawn(move || serve_mock_dap(listener));
    let mut session = DebugSession::connect_tcp(&format!("127.0.0.1:{port}"))
        .expect("connect to the Java DAP server");
    session.initialize("java").unwrap();

    let mut stopped = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !stopped {
        for event in session.poll() {
            match event {
                DebugEvent::Initialized => {
                    session
                        .launch_java(
                            "com.example.App",
                            "demo",
                            &["/tmp/classes".to_string()],
                            &dir,
                        )
                        .unwrap();
                    session.configuration_done().unwrap();
                }
                DebugEvent::Stopped {
                    reason, thread_id, ..
                } => {
                    assert_eq!(reason, "breakpoint");
                    assert_eq!(thread_id, Some(1));
                    stopped = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(stopped, "the Java launch should stop on a breakpoint");
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn drives_a_netcoredbg_plan_to_a_stopped_event() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let dir = std::env::temp_dir().join(format!("kemori-netcoredbg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("netcoredbg");
    std::fs::write(&script, MOCK_ADAPTER).unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let plan = super::super::csharp_plan_with(script.clone(), PathBuf::from("/tmp/app.dll"));
    let mut session = DebugSession::start(&plan.adapter, &plan.adapter_args, &dir)
        .expect("spawn the mock netcoredbg");
    session.initialize(&plan.adapter_id).unwrap();

    let mut stopped = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !stopped {
        for event in session.poll() {
            match event {
                DebugEvent::Initialized => {
                    session
                        .launch(&plan.program, &dir, plan.launch_mode)
                        .unwrap();
                    session.configuration_done().unwrap();
                }
                DebugEvent::Stopped {
                    reason, thread_id, ..
                } => {
                    assert_eq!(reason, "breakpoint");
                    assert_eq!(thread_id, Some(1));
                    stopped = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(stopped, "the netcoredbg plan should stop on a breakpoint");
    let _ = std::fs::remove_dir_all(dir);
}
#[cfg(unix)]
#[test]
fn drives_a_dart_plan_to_a_stopped_event() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let dir = std::env::temp_dir().join(format!("kemori-dart-dap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("dart");
    std::fs::write(&script, MOCK_ADAPTER).unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let plan =
        super::super::dart_plan_with(Path::new("/tmp/project/bin/main.dart"), script.clone());
    let mut session = DebugSession::start(&plan.adapter, &plan.adapter_args, &dir)
        .expect("spawn the mock Dart adapter");
    session.initialize(&plan.adapter_id).unwrap();

    let mut stopped = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !stopped {
        for event in session.poll() {
            match event {
                DebugEvent::Initialized => {
                    session
                        .launch(&plan.program, &dir, plan.launch_mode)
                        .unwrap();
                    session.configuration_done().unwrap();
                }
                DebugEvent::Stopped {
                    reason, thread_id, ..
                } => {
                    assert_eq!(reason, "breakpoint");
                    assert_eq!(thread_id, Some(1));
                    stopped = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(stopped, "the Dart plan should stop on a breakpoint");
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn drives_a_js_debug_plan_to_a_stopped_event() {
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a mock DAP server");
    let port = listener.local_addr().unwrap().port();
    let dir = std::env::temp_dir().join(format!("kemori-js-dap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("js-debug-dap");
    std::fs::write(&script, "#!/bin/sh\nwhile :; do sleep 1; done\n").unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let _server = std::thread::spawn(move || serve_mock_dap(listener));
    let plan = super::super::js_plan_with(Path::new("/tmp/project/index.js"), script.clone(), port);
    assert_eq!(plan.transport, super::super::LaunchTransport::Tcp);
    assert_eq!(
        plan.tcp_address.as_deref(),
        Some(format!("127.0.0.1:{port}").as_str())
    );
    let mut session = DebugSession::start_tcp_at_with_env(
        &plan.adapter,
        &plan.adapter_args,
        &dir,
        plan.tcp_address.as_deref(),
        &plan.env,
    )
    .expect("start the js-debug adapter");
    session.initialize(&plan.adapter_id).unwrap();

    let mut stopped = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !stopped {
        for event in session.poll() {
            match event {
                DebugEvent::Initialized => {
                    session
                        .launch(&plan.program, &dir, plan.launch_mode)
                        .unwrap();
                    session.configuration_done().unwrap();
                }
                DebugEvent::Stopped {
                    reason, thread_id, ..
                } => {
                    assert_eq!(reason, "breakpoint");
                    assert_eq!(thread_id, Some(1));
                    stopped = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(stopped, "the js-debug plan should stop on a breakpoint");
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}
