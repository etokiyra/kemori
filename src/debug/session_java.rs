//! DAP connection and Java launch helpers for `session.rs`.

use super::*;

impl DebugSession {
    /// Connect to a DAP server hosted inside another process (JDT LS's
    /// java-debug). The session owns no child process, so stopping the debugger
    /// must not kill the language server.
    pub fn connect_tcp(address: &str) -> Result<DebugSession, String> {
        let stream = connect_tcp(address)?;
        let reader = stream
            .try_clone()
            .map_err(|err| format!("could not clone the debug socket: {err}"))?;
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("kemori-dap-reader".to_string())
            .spawn(move || {
                let mut reader = BufReader::new(reader);
                loop {
                    match read_message(&mut reader) {
                        Ok(Some(value)) => {
                            if tx.send(Incoming::Message(value)).is_err() {
                                break;
                            }
                        }
                        Ok(None) | Err(_) => {
                            let _ = tx.send(Incoming::End);
                            break;
                        }
                    }
                }
            })
            .map_err(|err| format!("could not start the debug reader: {err}"))?;
        Ok(DebugSession {
            child: None,
            writer: Box::new(stream),
            incoming: rx,
            next_seq: 1,
            pending: HashMap::new(),
            initialized: false,
            started: Instant::now(),
            last_stderr: String::new(),
        })
    }

    /// Attach to an existing debuggee or debug socket.
    pub fn attach(&mut self, arguments: Value) -> Result<i64, String> {
        self.request("attach", arguments)
    }

    /// Launch a Java program through the java-debug adapter.
    pub fn launch_java(
        &mut self,
        main_class: &str,
        project_name: &str,
        class_paths: &[String],
        cwd: &Path,
    ) -> Result<i64, String> {
        self.request(
            "launch",
            json!({
                "mainClass": main_class,
                "projectName": project_name,
                "classPaths": class_paths,
                "cwd": cwd.to_string_lossy(),
                "console": "internalConsole",
                "stopOnEntry": false,
            }),
        )
    }

    /// Launch a PHP file through the Xdebug DAP bridge.
    pub fn launch_php(&mut self, program: &Path, cwd: &Path) -> Result<i64, String> {
        self.request(
            "launch",
            json!({
                "program": program.to_string_lossy(),
                "cwd": cwd.to_string_lossy(),
                "port": 0,
                "runtimeExecutable": "php",
                "runtimeArgs": ["-dxdebug.start_with_request=yes"],
                "env": {
                    "XDEBUG_MODE": "debug,develop",
                    "XDEBUG_CONFIG": "client_port=${port}"
                },
                "stopOnEntry": false,
                "console": "internalConsole",
            }),
        )
    }
}
