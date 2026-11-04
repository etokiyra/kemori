//! Java debugging through the running JDT LS and its java-debug extension.

use super::*;
use crate::app::debug::{DebugStatus, JavaDebugPending, JavaDebugStage, JavaLaunch};
use crate::debug::{DebugSession, LaunchPlan, LaunchTransport};
use serde_json::json;

impl App {
    /// Start Java debugging through the running JDT LS and its java-debug
    /// extension. The first step resolves the main class; later responses walk
    /// classpath and start-session before connecting DAP.
    pub(super) fn start_java_debug(&mut self, path: &Path) {
        let root = self.workspace.root().to_path_buf();
        let ready = self
            .lsp
            .get(&(root.clone(), LanguageId::Java))
            .and_then(|job| job.server.as_ref())
            .is_some_and(|server| server.is_ready());
        if !ready {
            self.set_error(
                "The Java language server is not ready yet — open a Java file and try again",
            );
            return;
        }
        if self.debug.session.is_some() {
            self.set_status("A debug session is already running — stop it first");
            return;
        }
        self.panel.debug.clear();
        let pending = JavaDebugPending {
            id: 0,
            stage: JavaDebugStage::MainClass,
            path: path.to_path_buf(),
            main_class: None,
            project_name: None,
            class_paths: Vec::new(),
        };
        self.send_java_command(
            "vscode.java.resolveMainClass",
            json!([crate::language::lsp::path_to_uri(path)]),
            pending,
        );
    }

    fn send_java_command(
        &mut self,
        command: &str,
        arguments: Value,
        mut pending: JavaDebugPending,
    ) -> bool {
        let root = self.workspace.root().to_path_buf();
        let id = self
            .lsp
            .get_mut(&(root, LanguageId::Java))
            .and_then(|job| job.server.as_mut())
            .and_then(|server| server.execute_command_request(command, arguments));
        let Some(id) = id else {
            self.set_error("The Java language server did not accept the debug command");
            return false;
        };
        pending.id = id;
        self.pending_java_debug = Some(pending);
        self.debug_console(format!("Java debug: {command}"));
        self.set_status("Preparing Java debugging…");
        true
    }

    /// Advance the Java debug pipeline from a `workspace/executeCommand`
    /// response.
    pub(super) fn handle_java_debug_response(&mut self, id: i64, value: &Value) {
        let Some(pending) = self.pending_java_debug.take() else {
            return;
        };
        if pending.id != id {
            self.pending_java_debug = Some(pending);
            return;
        }
        match pending.stage {
            JavaDebugStage::MainClass => {
                let Some((main_class, project_name)) = java_main_class(value) else {
                    self.set_error("Could not resolve a Java main class for this file");
                    return;
                };
                self.debug_console(format!("Java main class: {main_class}"));
                let next = JavaDebugPending {
                    stage: JavaDebugStage::Classpath,
                    main_class: Some(main_class.clone()),
                    project_name: Some(project_name.clone()),
                    ..pending
                };
                self.send_java_command(
                    "vscode.java.resolveClasspath",
                    json!([main_class, project_name]),
                    next,
                );
            }
            JavaDebugStage::Classpath => {
                let class_paths = java_class_paths(value);
                let next = JavaDebugPending {
                    stage: JavaDebugStage::StartSession,
                    class_paths,
                    ..pending
                };
                self.send_java_command("vscode.java.startDebugSession", json!([]), next);
            }
            JavaDebugStage::StartSession => {
                let Some(port) = java_debug_port(value) else {
                    self.set_error("The Java debug server did not return a port");
                    return;
                };
                let address = format!("127.0.0.1:{port}");
                match DebugSession::connect_tcp(&address) {
                    Ok(mut session) => {
                        if let Err(err) = session.initialize("java") {
                            self.debug_fail(&err);
                            return;
                        }
                        let Some(main_class) = pending.main_class.clone() else {
                            self.set_error("No Java main class resolved");
                            return;
                        };
                        let project_name = pending.project_name.clone().unwrap_or_default();
                        self.debug_console(format!(
                            "Connected to the Java debug server on {address}"
                        ));
                        self.debug.java_launch = Some(JavaLaunch {
                            main_class,
                            project_name,
                            class_paths: pending.class_paths,
                            cwd: self.workspace.root().to_path_buf(),
                        });
                        self.debug.plan = Some(LaunchPlan {
                            adapter: "jdtls".to_string(),
                            adapter_args: Vec::new(),
                            adapter_id: "java".to_string(),
                            program: pending.path,
                            label: "java-debug".to_string(),
                            transport: LaunchTransport::Tcp,
                            tcp_address: Some(address),
                            launch_mode: None,
                            env: Vec::new(),
                            attach: None,
                        });
                        self.debug.session = Some(session);
                        self.debug.status = DebugStatus::Starting;
                        self.debug.configured = false;
                        self.debug.frames.clear();
                        self.debug.variables.clear();
                        self.set_status("Starting Java debugging…");
                    }
                    Err(err) => self.debug_fail(&err),
                }
            }
        }
    }
}

/// Parse the first `vscode.java.resolveMainClass` result.
fn java_main_class(value: &Value) -> Option<(String, String)> {
    let item = value.as_array()?.first()?;
    if let Some(object) = item.as_object() {
        let main = object.get("mainClass")?.as_str()?.to_string();
        let project = object
            .get("projectName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        return Some((main, project));
    }
    let array = item.as_array()?;
    Some((
        array.first()?.as_str()?.to_string(),
        array
            .get(1)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    ))
}

/// Parse `vscode.java.resolveClasspath`'s `String[][]` result into paths.
fn java_class_paths(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    entry
                        .as_array()
                        .and_then(|pair| pair.first())
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse the port returned by `vscode.java.startDebugSession`.
fn java_debug_port(value: &Value) -> Option<u16> {
    value
        .as_u64()
        .and_then(|port| u16::try_from(port).ok())
        .or_else(|| value.as_str().and_then(|port| port.parse().ok()))
        .or_else(|| {
            value
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|port| u16::try_from(port).ok())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_java_main_class_objects_and_arrays() {
        assert_eq!(
            java_main_class(&serde_json::json!([
                { "mainClass": "com.example.App", "projectName": "demo", "filePath": "/a.java" }
            ])),
            Some(("com.example.App".to_string(), "demo".to_string()))
        );
        assert_eq!(
            java_main_class(&serde_json::json!([["com.example.App", "demo", "/a.java"]])),
            Some(("com.example.App".to_string(), "demo".to_string()))
        );
        assert!(java_main_class(&serde_json::json!([])).is_none());
    }

    #[test]
    fn parses_java_classpath_pairs() {
        let paths = java_class_paths(&serde_json::json!([
            ["/project/bin", "/project/src"],
            ["/lib/dep.jar", null],
        ]));
        assert_eq!(paths, vec!["/project/bin", "/lib/dep.jar"]);
    }

    #[test]
    fn parses_the_java_debug_port() {
        assert_eq!(java_debug_port(&serde_json::json!(4321)), Some(4321));
        assert_eq!(java_debug_port(&serde_json::json!("4321")), Some(4321));
        assert_eq!(
            java_debug_port(&serde_json::json!({ "port": 4321 })),
            Some(4321)
        );
        assert!(java_debug_port(&serde_json::json!(null)).is_none());
    }
}
