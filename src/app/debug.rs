//! The app side of the debugger: starting a plan, following adapter events and
//! turning stack/scopes/variables responses into the Debug Console.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::*;
use crate::debug::{DebugEvent, DebugSession, LaunchPlan, StackFrame, Variable};

/// Where the current debug session is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DebugStatus {
    #[default]
    Inactive,
    Starting,
    Running,
    Stopped,
    Exited,
    Failed,
}

impl DebugStatus {
    pub fn label(self) -> &'static str {
        match self {
            DebugStatus::Inactive => "inactive",
            DebugStatus::Starting => "starting",
            DebugStatus::Running => "running",
            DebugStatus::Stopped => "stopped",
            DebugStatus::Exited => "exited",
            DebugStatus::Failed => "failed",
        }
    }
}

/// One watch expression and its most recent value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watch {
    pub expression: String,
    pub value: Option<String>,
    pub kind: String,
}

/// The launch data JDT LS needs after `startDebugSession`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JavaLaunch {
    pub main_class: String,
    pub project_name: String,
    pub class_paths: Vec<String>,
    pub cwd: PathBuf,
}

/// One step of the jdtls-hosted Java debug setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JavaDebugStage {
    MainClass,
    Classpath,
    StartSession,
}

/// A pending `workspace/executeCommand` in the Java debug pipeline.
#[derive(Clone, Debug)]
pub struct JavaDebugPending {
    pub id: i64,
    pub stage: JavaDebugStage,
    pub path: PathBuf,
    pub main_class: Option<String>,
    pub project_name: Option<String>,
    pub class_paths: Vec<String>,
}

/// Debugger state shown in the Debug Console and driven by the debug commands.
#[derive(Default)]
pub struct DebugState {
    pub session: Option<DebugSession>,
    pub plan: Option<LaunchPlan>,
    pub status: DebugStatus,
    pub thread_id: Option<i64>,
    pub frames: Vec<StackFrame>,
    pub frame_selected: usize,
    pub variables: Vec<(String, Variable)>,
    pub watches: Vec<Watch>,
    /// Launch data for the jdtls-hosted Java adapter.
    pub java_launch: Option<JavaLaunch>,
    /// Whether `setBreakpoints`/`launch`/`configurationDone` were sent.
    pub configured: bool,
    /// Scope names keyed by the request that asked for their variables.
    pub pending_variable_scopes: HashMap<i64, String>,
    /// Watch expressions keyed by the evaluate request that is asking for them.
    pub pending_watches: HashMap<i64, String>,
}

impl App {
    /// Start the debugger for the active file.
    pub(super) fn debug_start(&mut self) {
        if self.debug.session.is_some() {
            self.set_status("A debug session is already running — stop it first");
            return;
        }
        let Some(doc) = self.editor.active_document() else {
            self.set_status("No file open");
            return;
        };
        let Some(path) = doc.buffer.path.clone() else {
            self.set_error("Save the file before debugging it");
            return;
        };
        let language = doc.buffer.language;
        let root = self.workspace.root().to_path_buf();
        if language == LanguageId::Java {
            self.start_java_debug(&path);
            return;
        }
        let plan = match crate::debug::plan_for(language, &path, &root) {
            Ok(plan) => plan,
            Err(message) => {
                self.set_error(message.clone());
                self.push_toast(ToastKind::Error, message);
                return;
            }
        };
        self.panel.debug.clear();
        self.start_debug_plan(plan, path);
    }

    /// Start an adapter session from an already-built plan.
    fn start_debug_plan(&mut self, plan: LaunchPlan, path: PathBuf) {
        self.debug_console(format!(
            "Starting {} for {}",
            plan.label,
            display_path(&path)
        ));
        let root = self.workspace.root().to_path_buf();
        let start = match plan.transport {
            crate::debug::LaunchTransport::Stdio => {
                DebugSession::start_with_env(&plan.adapter, &plan.adapter_args, &root, &plan.env)
            }
            crate::debug::LaunchTransport::Tcp => match plan.tcp_address.as_deref() {
                Some(address) => DebugSession::start_tcp_at_with_env(
                    &plan.adapter,
                    &plan.adapter_args,
                    &root,
                    Some(address),
                    &plan.env,
                ),
                None => DebugSession::start_tcp_with_env(
                    &plan.adapter,
                    &plan.adapter_args,
                    &root,
                    &plan.env,
                ),
            },
        };
        match start {
            Ok(mut session) => {
                if let Err(err) = session.initialize(&plan.adapter_id) {
                    self.debug_console(format!("Debugger: {err}"));
                    self.set_error(err);
                    return;
                }
                self.debug.session = Some(session);
                self.debug.plan = Some(plan);
                self.debug.status = DebugStatus::Starting;
                self.debug.configured = false;
                self.debug.frames.clear();
                self.debug.variables.clear();
                self.set_status("Starting the debugger…");
            }
            Err(message) => {
                self.debug_console(format!("Debugger: {message}"));
                self.set_error(message.clone());
                self.push_toast(ToastKind::Error, message);
            }
        }
    }

    /// Attach to an existing process or debug socket using the active language's
    /// adapter.
    pub(super) fn start_attach(&mut self, target: &str) {
        if self.debug.session.is_some() {
            self.set_status("A debug session is already running — stop it first");
            return;
        }
        let Some(doc) = self.editor.active_document() else {
            self.set_status("No file open");
            return;
        };
        let Some(path) = doc.buffer.path.clone() else {
            self.set_error("Save the file before attaching");
            return;
        };
        let language = doc.buffer.language;
        let root = self.workspace.root().to_path_buf();
        let target = match crate::debug::parse_attach_target(target) {
            Ok(target) => target,
            Err(message) => {
                self.set_error(message);
                return;
            }
        };
        let plan = match crate::debug::attach_plan_for(language, &target, &path, &root) {
            Ok(plan) => plan,
            Err(message) => {
                self.set_error(message.clone());
                self.push_toast(ToastKind::Error, message);
                return;
            }
        };
        self.panel.debug.clear();
        self.debug_console(match (target.pid, target.port) {
            (Some(pid), _) => format!("Attaching to process {pid}"),
            (None, port) => format!("Attaching to {}:{port}", target.host),
        });
        self.start_debug_plan(plan, path);
    }

    /// Drain adapter messages. Returns whether anything changed.
    pub(super) fn poll_debug(&mut self) -> bool {
        let Some(session) = self.debug.session.as_mut() else {
            return false;
        };
        let events = session.poll();
        let timed_out = session.handshake_timed_out();
        let stderr = session.last_stderr().to_string();
        if events.is_empty() {
            if timed_out {
                self.debug_fail("the adapter did not answer initialize");
                return true;
            }
            return false;
        }
        for event in events {
            self.handle_debug_event(event);
        }
        if !stderr.is_empty() && self.debug.status == DebugStatus::Failed {
            self.debug_console(format!("adapter stderr: {stderr}"));
        }
        true
    }

    fn handle_debug_event(&mut self, event: DebugEvent) {
        match event {
            DebugEvent::Ready => {
                if self.debug.status == DebugStatus::Starting {
                    self.set_status("Debug adapter initialized");
                }
            }
            DebugEvent::Initialized => self.debug_configure(),
            DebugEvent::Stopped {
                reason,
                thread_id,
                description,
            } => {
                self.debug.status = DebugStatus::Stopped;
                self.debug.thread_id = thread_id;
                self.debug.frames.clear();
                self.debug.variables.clear();
                let detail = description.unwrap_or_default();
                self.debug_console(format!(
                    "Stopped ({reason}){}",
                    if detail.is_empty() {
                        String::new()
                    } else {
                        format!(": {detail}")
                    }
                ));
                if let Some(thread_id) = thread_id
                    && let Some(session) = self.debug.session.as_mut()
                {
                    let _ = session.stack_trace(thread_id);
                }
                self.set_status("Paused at a breakpoint — F5 continues");
            }
            DebugEvent::Continued => {
                self.debug.status = DebugStatus::Running;
                self.debug.frames.clear();
                self.debug.variables.clear();
                self.set_status("Debugger running…");
            }
            DebugEvent::Exited(code) => {
                self.debug.status = DebugStatus::Exited;
                let text = match code {
                    Some(code) => format!("Debuggee exited with code {code}"),
                    None => "Debuggee exited".to_string(),
                };
                self.debug_console(text.clone());
                self.set_status(text);
            }
            DebugEvent::Terminated => {
                self.debug.session = None;
                self.debug.thread_id = None;
                self.debug.frames.clear();
                self.debug.variables.clear();
                self.debug.java_launch = None;
                if self.debug.status != DebugStatus::Exited
                    && self.debug.status != DebugStatus::Failed
                {
                    self.debug.status = DebugStatus::Inactive;
                }
                self.debug_console("Debug session ended");
            }
            DebugEvent::Output { category, text } => {
                let text = text.trim_end_matches(['\n', '\r']);
                if !text.is_empty() {
                    self.debug_console(format!("{category}: {text}"));
                }
            }
            DebugEvent::Failed(message) => self.debug_fail(&message),
            DebugEvent::Response {
                seq,
                command,
                success,
                body,
                message,
            } => self.handle_debug_response(seq, &command, success, body, message),
        }
    }

    fn handle_debug_response(
        &mut self,
        seq: i64,
        command: &str,
        success: bool,
        body: Value,
        message: Option<String>,
    ) {
        if !success {
            let detail = message.unwrap_or_else(|| "the adapter rejected the request".to_string());
            self.debug_console(format!("{command}: {detail}"));
            if matches!(
                command,
                "initialize" | "launch" | "attach" | "configurationDone"
            ) {
                self.debug_fail(&detail);
            }
            return;
        }
        match command {
            "initialize" => {
                self.debug.status = DebugStatus::Starting;
            }
            "setBreakpoints" => {
                let count = body
                    .get("breakpoints")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0);
                if count > 0 {
                    self.debug_console(format!("{count} breakpoint(s) accepted"));
                }
            }
            "launch" => {
                self.debug.status = DebugStatus::Running;
                self.set_status("Debugger launched");
            }
            "attach" => {
                self.debug.status = DebugStatus::Running;
                self.set_status("Debugger attached");
            }
            "configurationDone" => {
                if self.debug.status == DebugStatus::Starting {
                    self.debug.status = DebugStatus::Running;
                }
            }
            "stackTrace" => {
                let frames = crate::debug::parse_stack_frames(&body);
                self.debug.frames = frames;
                self.debug.frame_selected = 0;
                let mut lines = Vec::with_capacity(self.debug.frames.len());
                for frame in &self.debug.frames {
                    let location = match &frame.path {
                        Some(path) => format!("{}:{}", display_path(path), frame.line),
                        None => "<no source>".to_string(),
                    };
                    lines.push(format!("  {}  {location}", frame.name));
                }
                for line in lines {
                    self.debug_console(line);
                }
                if let Some(frame) = self.debug.frames.first()
                    && let Some(session) = self.debug.session.as_mut()
                {
                    let _ = session.scopes(frame.id);
                }
                self.evaluate_watches();
            }
            "scopes" => {
                let scopes = crate::debug::parse_scopes(&body);
                for scope in scopes {
                    if let Some(session) = self.debug.session.as_mut()
                        && let Ok(id) = session.variables(scope.variables_reference())
                    {
                        self.debug
                            .pending_variable_scopes
                            .insert(id, scope.name.clone());
                        self.debug_console(format!("{}:", scope.name));
                    }
                }
            }
            "variables" => {
                let scope = self
                    .debug
                    .pending_variable_scopes
                    .remove(&seq)
                    .unwrap_or_else(|| "scope".to_string());
                for variable in crate::debug::parse_variables(&body) {
                    self.debug_console(format!(
                        "  {} = {}{}",
                        variable.name,
                        variable.value,
                        if variable.kind.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", variable.kind)
                        }
                    ));
                    self.debug.variables.push((scope.clone(), variable));
                }
            }
            "evaluate" => {
                let Some(expression) = self.debug.pending_watches.remove(&seq) else {
                    return;
                };
                let value = body
                    .get("result")
                    .and_then(Value::as_str)
                    .unwrap_or("<unavailable>")
                    .to_string();
                let kind = body
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                for watch in &mut self.debug.watches {
                    if watch.expression == expression {
                        watch.value = Some(value.clone());
                        watch.kind = kind.clone();
                    }
                }
                self.debug_console(format!(
                    "watch {expression} = {value}{}",
                    if kind.is_empty() {
                        String::new()
                    } else {
                        format!(" ({kind})")
                    }
                ));
            }
            "continue" | "next" | "stepIn" | "stepOut" | "pause" => {
                self.debug.status = DebugStatus::Running;
                self.debug.frames.clear();
                self.debug.variables.clear();
            }
            _ => {}
        }
    }

    /// Send breakpoints, launch and `configurationDone`, once the adapter says it
    /// is ready.
    fn debug_configure(&mut self) {
        if self.debug.configured {
            return;
        }
        let Some(plan) = self.debug.plan.clone() else {
            return;
        };
        self.debug.configured = true;
        let root = self.workspace.root().to_path_buf();
        let breakpoints: Vec<(PathBuf, Vec<crate::debug::Breakpoint>)> = self
            .editor
            .documents
            .iter()
            .filter_map(|doc| {
                let path = doc.buffer.path.clone()?;
                (!doc.breakpoints.is_empty()).then(|| {
                    let points = doc
                        .breakpoints
                        .iter()
                        .map(|line| crate::debug::Breakpoint {
                            line: *line,
                            condition: doc.breakpoint_conditions.get(line).cloned(),
                            log_message: doc.breakpoint_logs.get(line).cloned(),
                        })
                        .collect();
                    (path, points)
                })
            })
            .collect();
        let java_launch = self.debug.java_launch.clone();
        let Some(session) = self.debug.session.as_mut() else {
            return;
        };
        for (path, lines) in breakpoints {
            let _ = session.set_breakpoints(&path, &lines);
        }
        let launched = if let Some(target) = &plan.attach {
            session.attach(crate::debug::attach_arguments(&plan.adapter_id, target))
        } else if plan.adapter_id == "php" {
            session.launch_php(&plan.program, &root)
        } else {
            match &java_launch {
                Some(java) if plan.adapter_id == "java" => session.launch_java(
                    &java.main_class,
                    &java.project_name,
                    &java.class_paths,
                    &java.cwd,
                ),
                _ => session.launch(&plan.program, &root, plan.launch_mode),
            }
        };
        if let Err(err) = launched {
            self.debug_fail(&err);
            return;
        }
        let _ = session.configuration_done();
        self.debug_console(format!(
            "Launching {} with {}",
            display_path(&plan.program),
            plan.label
        ));
    }

    pub(super) fn debug_fail(&mut self, message: &str) {
        self.debug.status = DebugStatus::Failed;
        self.debug.session = None;
        self.debug_console(format!("Debugger failed: {message}"));
        let text = format!("Debugger failed: {message}");
        self.set_error(text.clone());
        self.push_toast(ToastKind::Error, text);
    }

    /// Continue a stopped debuggee.
    pub(super) fn debug_continue(&mut self) {
        if self.debug.session.is_none() {
            self.set_status("No debug session is running");
            return;
        }
        let thread = self.debug.thread_id;
        if let Some(session) = self.debug.session.as_mut()
            && let Err(err) = session.continue_(thread)
        {
            self.debug_fail(&err);
            return;
        }
        self.set_status("Continuing…");
    }

    /// Step over, into or out of the current statement.
    pub(super) fn debug_step(&mut self, kind: crate::commands::DebugStep) {
        if self.debug.session.is_none() {
            self.set_status("No debug session is running");
            return;
        }
        let thread = self.debug.thread_id;
        let result = match (self.debug.session.as_mut(), kind) {
            (Some(session), crate::commands::DebugStep::Over) => session.next(thread),
            (Some(session), crate::commands::DebugStep::Into) => session.step_in(thread),
            (Some(session), crate::commands::DebugStep::Out) => session.step_out(thread),
            (None, _) => return,
        };
        if let Err(err) = result {
            self.debug_fail(&err);
            return;
        }
        self.debug.status = DebugStatus::Running;
        self.debug.frames.clear();
        self.debug.variables.clear();
        self.set_status(match kind {
            crate::commands::DebugStep::Over => "Stepping over…",
            crate::commands::DebugStep::Into => "Stepping into…",
            crate::commands::DebugStep::Out => "Stepping out…",
        });
    }

    /// Pause a running debuggee.
    pub(super) fn debug_pause(&mut self) {
        if self.debug.session.is_none() {
            self.set_status("No debug session is running");
            return;
        }
        let thread = self.debug.thread_id;
        if let Some(session) = self.debug.session.as_mut()
            && let Err(err) = session.pause(thread)
        {
            self.debug_fail(&err);
            return;
        }
        self.set_status("Pausing…");
    }

    /// End the debug session and its debuggee.
    pub(super) fn debug_stop(&mut self) {
        if let Some(mut session) = self.debug.session.take() {
            session.disconnect();
        }
        self.debug.status = DebugStatus::Inactive;
        self.debug.thread_id = None;
        self.debug.frames.clear();
        self.debug.variables.clear();
        self.debug.java_launch = None;
        self.debug.configured = false;
        self.debug_console("Debugger stopped");
        self.set_status("Debugger stopped");
    }

    /// Open a picker over the current stack, revealing the chosen frame.
    pub(super) fn debug_stack_picker(&mut self) {
        if self.debug.frames.is_empty() {
            self.set_status("No stack frames — the debugger is not stopped");
            return;
        }
        let items = self
            .debug
            .frames
            .iter()
            .map(|frame| {
                let detail = match &frame.path {
                    Some(path) => format!("{}:{}", display_path(path), frame.line),
                    None => "<no source>".to_string(),
                };
                let action = match &frame.path {
                    Some(path) => PickerAction::Reveal {
                        path: path.clone(),
                        position: Position::new(
                            frame.line.saturating_sub(1),
                            frame.column.saturating_sub(1),
                        ),
                    },
                    None => PickerAction::Info(frame.name.clone()),
                };
                PickerItem::new(frame.name.clone(), detail, action)
            })
            .collect();
        let mut picker = Picker::new("Stack Frames", "Choose a frame…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Open a picker over the variables captured at the last stop.
    pub(super) fn debug_variables_picker(&mut self) {
        if self.debug.variables.is_empty() {
            self.set_status("No variables — the debugger is not stopped");
            return;
        }
        let items = self
            .debug
            .variables
            .iter()
            .map(|(scope, variable)| {
                PickerItem::new(
                    format!("{} = {}", variable.name, variable.value),
                    format!(
                        "{scope}{}",
                        if variable.kind.is_empty() {
                            String::new()
                        } else {
                            format!(" · {}", variable.kind)
                        }
                    ),
                    PickerAction::Info(format!("{} = {}", variable.name, variable.value)),
                )
            })
            .collect();
        let mut picker = Picker::new("Variables", "Filter variables…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Add a watch expression, evaluating it immediately when stopped.
    pub(super) fn add_debug_watch(&mut self, expression: String) {
        let expression = expression.trim().to_string();
        if expression.is_empty() {
            self.set_error("No watch expression provided");
            return;
        }
        if self
            .debug
            .watches
            .iter()
            .any(|watch| watch.expression == expression)
        {
            self.set_status("That watch already exists");
            return;
        }
        self.debug.watches.push(Watch {
            expression: expression.clone(),
            value: None,
            kind: String::new(),
        });
        if self.debug.status == DebugStatus::Stopped {
            self.evaluate_watches();
            self.set_status(format!("Watching {expression}"));
        } else {
            self.set_status(format!(
                "Watching {expression} — evaluates on the next stop"
            ));
        }
    }

    /// Evaluate every watch in the top frame of the current stop.
    fn evaluate_watches(&mut self) {
        if self.debug.watches.is_empty() {
            return;
        }
        let frame_id = self.debug.frames.first().map(|frame| frame.id);
        let expressions: Vec<String> = self
            .debug
            .watches
            .iter()
            .map(|watch| watch.expression.clone())
            .collect();
        for expression in expressions {
            if let Some(session) = self.debug.session.as_mut()
                && let Ok(id) = session.evaluate(&expression, frame_id)
            {
                self.debug.pending_watches.insert(id, expression);
            }
        }
    }

    /// Open a picker over the watch expressions and their last values.
    pub(super) fn debug_watches_picker(&mut self) {
        let mut items: Vec<PickerItem> = self
            .debug
            .watches
            .iter()
            .map(|watch| {
                let value = watch
                    .value
                    .clone()
                    .unwrap_or_else(|| "<not evaluated>".to_string());
                PickerItem::new(
                    watch.expression.clone(),
                    value.clone(),
                    PickerAction::Info(format!("{} = {value}", watch.expression)),
                )
            })
            .collect();
        items.push(
            PickerItem::new(
                "Add watch expression…",
                "Evaluate an expression at the next stop",
                PickerAction::Command(ids::DEBUG_ADD_WATCH),
            )
            .shortcut("Enter"),
        );
        let mut picker = Picker::new("Watch Expressions", "Choose or add a watch…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Send the current breakpoints for `path` to a live adapter.
    pub(super) fn sync_debug_breakpoints(&mut self, path: &Path) {
        let Some(session) = self.debug.session.as_mut() else {
            return;
        };
        let points: Vec<crate::debug::Breakpoint> = self
            .editor
            .documents
            .iter()
            .find(|doc| same_file(doc.buffer.path.as_deref(), path))
            .map(|doc| {
                doc.breakpoints
                    .iter()
                    .map(|line| crate::debug::Breakpoint {
                        line: *line,
                        condition: doc.breakpoint_conditions.get(line).cloned(),
                        log_message: doc.breakpoint_logs.get(line).cloned(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let _ = session.set_breakpoints(path, &points);
    }
}
