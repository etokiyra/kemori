//! One DAP adapter process: requests, events and bounded stderr capture.
//!
//! The session owns the adapter's stdin and a reader thread. It speaks the
//! protocol directly, so the app sees typed events instead of JSON, and it maps
//! responses back to the command that produced them.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::protocol::{read_message, write_message};

/// How long Kemori waits for an adapter to answer `initialize` before naming it
/// broken. The reader thread stays alive either way.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// One breakpoint sent to an adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Breakpoint {
    /// Zero-based line in Kemori; DAP lines are one-based.
    pub line: usize,
    pub condition: Option<String>,
    pub log_message: Option<String>,
}

/// A stack frame from a `stackTrace` response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackFrame {
    pub id: i64,
    pub name: String,
    pub path: Option<PathBuf>,
    /// One-based, as DAP reports it.
    pub line: usize,
    pub column: usize,
}

/// A scope from a `scopes` response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub name: String,
    variables_reference: i64,
}

impl Scope {
    /// The handle to pass to `variables`.
    pub fn variables_reference(&self) -> i64 {
        self.variables_reference
    }
}

/// A variable from a `variables` response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    pub value: String,
    pub kind: String,
}

/// Something the app consumes from a running adapter.
#[derive(Clone, Debug)]
pub enum DebugEvent {
    /// The adapter is ready for breakpoints and `launch`.
    Initialized,
    Ready,
    Stopped {
        reason: String,
        thread_id: Option<i64>,
        description: Option<String>,
    },
    Continued,
    Exited(Option<i64>),
    Terminated,
    Output {
        category: String,
        text: String,
    },
    Response {
        /// The request sequence this answers, for callers that tag requests.
        seq: i64,
        command: String,
        success: bool,
        body: Value,
        message: Option<String>,
    },
    /// The adapter wrote to stderr or the protocol failed.
    Failed(String),
}

/// The reply to one request, for callers that need a value immediately.
pub struct Response {
    pub success: bool,
    pub body: Value,
    pub message: Option<String>,
}

enum Incoming {
    Message(Value),
    Stderr(String),
    End,
}

/// A running debug adapter.
pub struct DebugSession {
    child: Option<Child>,
    writer: Box<dyn Write + Send>,
    incoming: Receiver<Incoming>,
    next_seq: i64,
    pending: HashMap<i64, String>,
    /// Whether the adapter answered `initialize`.
    initialized: bool,
    started: Instant,
    last_stderr: String,
}

impl DebugSession {
    /// Spawn `program` with `args` in `root` and start its reader thread.
    pub fn start(program: &str, args: &[String], root: &Path) -> Result<DebugSession, String> {
        Self::start_with_env(program, args, root, &[])
    }

    /// Spawn a stdio adapter with extra environment variables.
    pub fn start_with_env(
        program: &str,
        args: &[String],
        root: &Path,
        env: &[(String, String)],
    ) -> Result<DebugSession, String> {
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
        let mut child = spawn_adapter(&mut command, program)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "the debug adapter has no stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "the debug adapter has no stdout".to_string())?;
        let stderr = child.stderr.take();
        let (tx, rx) = mpsc::channel();
        let message_tx = tx.clone();
        thread::Builder::new()
            .name("kemori-dap-reader".to_string())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    match read_message(&mut reader) {
                        Ok(Some(value)) => {
                            if message_tx.send(Incoming::Message(value)).is_err() {
                                break;
                            }
                        }
                        Ok(None) | Err(_) => {
                            let _ = message_tx.send(Incoming::End);
                            break;
                        }
                    }
                }
            })
            .map_err(|err| format!("could not start the debug reader: {err}"))?;
        spawn_stderr_reader(stderr, tx);
        Ok(DebugSession {
            child: Some(child),
            writer: Box::new(stdin),
            incoming: rx,
            next_seq: 1,
            pending: HashMap::new(),
            initialized: false,
            started: Instant::now(),
            last_stderr: String::new(),
        })
    }

    /// Spawn a DAP adapter that listens for a TCP connection instead of speaking
    /// DAP over stdio (Go's `dlv dap`).
    ///
    /// The adapter prints its listen address; Kemori waits for it, connects and
    /// then uses the socket exactly like a stdio stream.
    pub fn start_tcp(program: &str, args: &[String], root: &Path) -> Result<DebugSession, String> {
        Self::start_tcp_with_env(program, args, root, &[])
    }

    /// Spawn a TCP adapter with extra environment variables.
    pub fn start_tcp_with_env(
        program: &str,
        args: &[String],
        root: &Path,
        env: &[(String, String)],
    ) -> Result<DebugSession, String> {
        Self::start_tcp_at_with_env(program, args, root, None, env)
    }

    /// Spawn a TCP adapter at a known address (js-debug), or wait for one it
    /// announces on stdout (`dlv dap`).
    pub fn start_tcp_at_with_env(
        program: &str,
        args: &[String],
        root: &Path,
        address: Option<&str>,
        env: &[(String, String)],
    ) -> Result<DebugSession, String> {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in env {
            command.env(key, value);
        }
        let mut child = spawn_adapter(&mut command, program)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "the debug adapter has no stdout".to_string())?;
        let stderr = child.stderr.take();
        let address = match address {
            Some(address) => {
                // The adapter does not announce its port; drain its output so a
                // chatty server cannot block on a full pipe.
                let _ = thread::Builder::new()
                    .name("kemori-dap-drain".to_string())
                    .spawn(move || {
                        let reader = BufReader::new(stdout);
                        for _ in reader.lines().map_while(Result::ok) {}
                    });
                address.to_string()
            }
            None => {
                let (line_tx, line_rx) = mpsc::channel::<String>();
                thread::Builder::new()
                    .name("kemori-dap-listen".to_string())
                    .spawn(move || {
                        let reader = BufReader::new(stdout);
                        for line in reader.lines().map_while(Result::ok) {
                            if line_tx.send(line).is_err() {
                                break;
                            }
                        }
                    })
                    .map_err(|err| format!("could not read the adapter's listen address: {err}"))?;
                let address = wait_for_listen_address(&line_rx)?;
                let _ = thread::Builder::new()
                    .name("kemori-dap-drain".to_string())
                    .spawn(move || while line_rx.recv().is_ok() {});
                address
            }
        };
        let stream = connect_tcp(&address)?;
        let reader = stream
            .try_clone()
            .map_err(|err| format!("could not clone the debug socket: {err}"))?;
        let (tx, rx) = mpsc::channel();
        let message_tx = tx.clone();
        thread::Builder::new()
            .name("kemori-dap-reader".to_string())
            .spawn(move || {
                let mut reader = BufReader::new(reader);
                loop {
                    match read_message(&mut reader) {
                        Ok(Some(value)) => {
                            if message_tx.send(Incoming::Message(value)).is_err() {
                                break;
                            }
                        }
                        Ok(None) | Err(_) => {
                            let _ = message_tx.send(Incoming::End);
                            break;
                        }
                    }
                }
            })
            .map_err(|err| format!("could not start the debug reader: {err}"))?;
        spawn_stderr_reader(stderr, tx);
        Ok(DebugSession {
            child: Some(child),
            writer: Box::new(stream),
            incoming: rx,
            next_seq: 1,
            pending: HashMap::new(),
            initialized: false,
            started: Instant::now(),
            last_stderr: String::new(),
        })
    }

    /// Whether the adapter answered `initialize`.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Whether the adapter handshake has been in flight too long.
    pub fn handshake_timed_out(&self) -> bool {
        !self.initialized && self.started.elapsed() >= HANDSHAKE_TIMEOUT
    }

    /// The adapter's recent stderr, for an actionable failure.
    pub fn last_stderr(&self) -> &str {
        self.last_stderr.trim()
    }

    /// Send `initialize`; the adapter's `initialized` event follows.
    pub fn initialize(&mut self, adapter_id: &str) -> Result<i64, String> {
        self.request(
            "initialize",
            json!({
                "clientID": "kemori",
                "clientName": "Kemori",
                "adapterID": adapter_id,
                "locale": "en-US",
                "linesStartAt1": true,
                "columnsStartAt1": true,
                "pathFormat": "path",
                "supportsVariableType": true,
                "supportsVariablePaging": false,
                "supportsRunInTerminalRequest": false
            }),
        )
    }

    pub fn set_breakpoints(
        &mut self,
        path: &Path,
        breakpoints: &[Breakpoint],
    ) -> Result<i64, String> {
        let points: Vec<Value> = breakpoints
            .iter()
            .map(|breakpoint| {
                let mut point = json!({ "line": breakpoint.line + 1 });
                if let Some(condition) = &breakpoint.condition {
                    point["condition"] = json!(condition);
                }
                if let Some(message) = &breakpoint.log_message {
                    point["logMessage"] = json!(message);
                }
                point
            })
            .collect();
        self.request(
            "setBreakpoints",
            json!({
                "source": { "path": path.to_string_lossy() },
                "breakpoints": points,
                "lines": breakpoints.iter().map(|breakpoint| breakpoint.line + 1).collect::<Vec<_>>(),
            }),
        )
    }

    /// Launch a program. `stop_on_entry` asks the adapter to stop before the
    /// first statement, which is useful when there is no breakpoint yet.
    pub fn launch(
        &mut self,
        program: &Path,
        cwd: &Path,
        mode: Option<&str>,
    ) -> Result<i64, String> {
        let mut arguments = json!({
            "program": program.to_string_lossy(),
            "cwd": cwd.to_string_lossy(),
            "stopOnEntry": false,
            "console": "internalConsole",
            "justMyCode": true,
        });
        if let Some(mode) = mode {
            arguments["mode"] = json!(mode);
        }
        self.request("launch", arguments)
    }

    pub fn configuration_done(&mut self) -> Result<i64, String> {
        self.request("configurationDone", json!({}))
    }

    pub fn continue_(&mut self, thread_id: Option<i64>) -> Result<i64, String> {
        let mut args = json!({});
        if let Some(thread_id) = thread_id {
            args["threadId"] = json!(thread_id);
            args["singleThread"] = json!(false);
        }
        self.request("continue", args)
    }

    pub fn next(&mut self, thread_id: Option<i64>) -> Result<i64, String> {
        self.request("next", thread_args(thread_id))
    }

    pub fn step_in(&mut self, thread_id: Option<i64>) -> Result<i64, String> {
        self.request("stepIn", thread_args(thread_id))
    }

    pub fn step_out(&mut self, thread_id: Option<i64>) -> Result<i64, String> {
        self.request("stepOut", thread_args(thread_id))
    }

    pub fn pause(&mut self, thread_id: Option<i64>) -> Result<i64, String> {
        self.request("pause", thread_args(thread_id))
    }

    pub fn stack_trace(&mut self, thread_id: i64) -> Result<i64, String> {
        self.request(
            "stackTrace",
            json!({ "threadId": thread_id, "startFrame": 0, "levels": 40 }),
        )
    }

    pub fn scopes(&mut self, frame_id: i64) -> Result<i64, String> {
        self.request("scopes", json!({ "frameId": frame_id }))
    }

    pub fn variables(&mut self, reference: i64) -> Result<i64, String> {
        self.request(
            "variables",
            json!({ "variablesReference": reference, "start": 0, "count": 200 }),
        )
    }

    /// Evaluate an expression in the selected frame, for a watch.
    pub fn evaluate(&mut self, expression: &str, frame_id: Option<i64>) -> Result<i64, String> {
        let mut arguments = json!({
            "expression": expression,
            "context": "watch",
        });
        if let Some(frame_id) = frame_id {
            arguments["frameId"] = json!(frame_id);
        }
        self.request("evaluate", arguments)
    }

    /// Ask the adapter to detach; the child is killed when the session drops.
    pub fn disconnect(&mut self) {
        let _ = self.request(
            "disconnect",
            json!({ "terminateDebuggee": true, "restart": false }),
        );
    }

    fn request(&mut self, command: &str, arguments: Value) -> Result<i64, String> {
        let seq = self.next_seq;
        self.next_seq += 1;
        let message = json!({
            "seq": seq,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });
        write_message(&mut self.writer, &message)
            .map_err(|err| format!("could not write to the debug adapter: {err}"))?;
        self.pending.insert(seq, command.to_string());
        Ok(seq)
    }

    /// Drain adapter messages into typed events.
    pub fn poll(&mut self) -> Vec<DebugEvent> {
        let mut events = Vec::new();
        loop {
            match self.incoming.try_recv() {
                Ok(Incoming::Message(value)) => {
                    events.extend(self.handle_message(value));
                }
                Ok(Incoming::Stderr(line)) => {
                    if self.last_stderr.len() > 2048 {
                        let drain = self.last_stderr.len() - 2048;
                        self.last_stderr.drain(..drain);
                    }
                    if !self.last_stderr.is_empty() {
                        self.last_stderr.push('\n');
                    }
                    self.last_stderr.push_str(&line);
                    events.push(DebugEvent::Output {
                        category: "stderr".to_string(),
                        text: line,
                    });
                }
                Ok(Incoming::End) => {
                    events.push(DebugEvent::Terminated);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    events.push(DebugEvent::Terminated);
                    break;
                }
            }
        }
        if let Some(child) = self.child.as_mut()
            && let Ok(Some(status)) = child.try_wait()
        {
            events.push(DebugEvent::Exited(status.code().map(i64::from)));
        }
        events
    }

    fn handle_message(&mut self, value: Value) -> Vec<DebugEvent> {
        match value.get("type").and_then(Value::as_str) {
            Some("response") => {
                let request_seq = value
                    .get("request_seq")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let command = self
                    .pending
                    .remove(&request_seq)
                    .unwrap_or_else(|| "unknown".to_string());
                let success = value
                    .get("success")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let body = value.get("body").cloned().unwrap_or(Value::Null);
                let message = value
                    .get("message")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                if command == "initialize" && success {
                    self.initialized = true;
                }
                let is_initialize = command == "initialize";
                let mut events = vec![DebugEvent::Response {
                    seq: request_seq,
                    command,
                    success,
                    body,
                    message,
                }];
                if self.initialized && is_initialize {
                    events.push(DebugEvent::Ready);
                }
                events
            }
            Some("event") => match value.get("event").and_then(Value::as_str) {
                Some("initialized") => vec![DebugEvent::Initialized],
                Some("stopped") => {
                    let body = value.get("body").cloned().unwrap_or(Value::Null);
                    vec![DebugEvent::Stopped {
                        reason: body
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("stopped")
                            .to_string(),
                        thread_id: body.get("threadId").and_then(Value::as_i64),
                        description: body
                            .get("description")
                            .and_then(Value::as_str)
                            .or_else(|| body.get("text").and_then(Value::as_str))
                            .map(ToString::to_string),
                    }]
                }
                Some("continued") => vec![DebugEvent::Continued],
                Some("exited") => vec![DebugEvent::Exited(
                    value.pointer("/body/exitCode").and_then(Value::as_i64),
                )],
                Some("terminated") => vec![DebugEvent::Terminated],
                Some("output") => {
                    let category = value
                        .pointer("/body/category")
                        .and_then(Value::as_str)
                        .unwrap_or("console");
                    let text = value
                        .pointer("/body/output")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    vec![DebugEvent::Output {
                        category: category.to_string(),
                        text: text.to_string(),
                    }]
                }
                Some("capabilities") => vec![],
                Some(_) | None => vec![],
            },
            Some("request") => {
                // Reverse requests (runInTerminal, etc.) are not implemented;
                // answer explicitly so the adapter does not wait forever.
                if let Some(id) = value.get("seq").and_then(Value::as_i64) {
                    let response = json!({
                        "seq": self.next_seq,
                        "type": "response",
                        "request_seq": id,
                        "success": false,
                        "message": "Kemori does not implement this reverse request",
                    });
                    self.next_seq += 1;
                    let _ = write_message(&mut self.writer, &response);
                }
                vec![]
            }
            _ => vec![],
        }
    }
}

/// Spawn an adapter, retrying the transient `ETXTBSY` a just-written test or
/// launcher script can report on Linux while the kernel releases its writer.
fn spawn_adapter(command: &mut Command, program: &str) -> Result<Child, String> {
    for attempt in 0..5 {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(err) if err.raw_os_error() == Some(26) && attempt < 4 => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => return Err(format!("could not start debug adapter `{program}`: {err}")),
        }
    }
    Err(format!("could not start debug adapter `{program}`"))
}

impl Drop for DebugSession {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn spawn_stderr_reader(stderr: Option<std::process::ChildStderr>, tx: mpsc::Sender<Incoming>) {
    let Some(stderr) = stderr else {
        return;
    };
    let _ = thread::Builder::new()
        .name("kemori-dap-stderr".to_string())
        .spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if tx.send(Incoming::Stderr(line)).is_err() {
                    break;
                }
            }
        });
}

/// Wait for an adapter line naming its DAP listen address.
fn wait_for_listen_address(lines: &Receiver<String>) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("the debug adapter did not report a DAP listen address".to_string());
        }
        match lines.recv_timeout(remaining) {
            Ok(line) => {
                if let Some(address) = dap_listen_address(&line) {
                    return Ok(address);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err("the debug adapter did not report a DAP listen address".to_string());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("the debug adapter exited before listening".to_string());
            }
        }
    }
}

fn dap_listen_address(line: &str) -> Option<String> {
    let (_, rest) = line.split_once("DAP server listening at:")?;
    let address = rest.trim().trim_matches(['"', '\'']);
    (!address.is_empty()).then(|| address.to_string())
}

fn connect_tcp(address: &str) -> Result<TcpStream, String> {
    let mut last = None;
    for _ in 0..50 {
        match TcpStream::connect(address) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(err) => {
                last = Some(err);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Err(format!(
        "could not connect to the debug adapter at {address}: {}",
        last.map(|err| err.to_string())
            .unwrap_or_else(|| "unknown error".to_string())
    ))
}

fn thread_args(thread_id: Option<i64>) -> Value {
    match thread_id {
        Some(thread_id) => json!({ "threadId": thread_id, "singleThread": false }),
        None => json!({}),
    }
}

/// Parse a `stackTrace` response body.
pub fn parse_stack_frames(body: &Value) -> Vec<StackFrame> {
    body.get("stackFrames")
        .and_then(Value::as_array)
        .map(|frames| {
            frames
                .iter()
                .filter_map(|frame| {
                    Some(StackFrame {
                        id: frame.get("id")?.as_i64()?,
                        name: frame
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("frame")
                            .to_string(),
                        path: frame
                            .pointer("/source/path")
                            .and_then(Value::as_str)
                            .map(PathBuf::from),
                        line: frame.get("line").and_then(Value::as_u64).unwrap_or(1) as usize,
                        column: frame.get("column").and_then(Value::as_u64).unwrap_or(1) as usize,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a `scopes` response body.
pub fn parse_scopes(body: &Value) -> Vec<Scope> {
    body.get("scopes")
        .and_then(Value::as_array)
        .map(|scopes| {
            scopes
                .iter()
                .filter_map(|scope| {
                    let reference = scope.get("variablesReference")?.as_i64()?;
                    if reference <= 0 {
                        return None;
                    }
                    Some(Scope {
                        name: scope
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("scope")
                            .to_string(),
                        variables_reference: reference,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a `variables` response body.
pub fn parse_variables(body: &Value) -> Vec<Variable> {
    body.get("variables")
        .and_then(Value::as_array)
        .map(|variables| {
            variables
                .iter()
                .map(|variable| Variable {
                    name: variable
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string(),
                    value: variable
                        .get("value")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    kind: variable
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

#[path = "session_java.rs"]
mod java;
