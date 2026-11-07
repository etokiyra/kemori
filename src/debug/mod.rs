//! A real Debug Adapter Protocol client.
//!
//! Kemori launches a DAP adapter over stdio, completes the handshake, sends
//! breakpoints, starts the program and follows `stopped`, stack, scope and
//! variable responses. It is intentionally adapter-agnostic: a launch plan says
//! which executable to run and what to start, and the same client serves every
//! language.
//!
//! An adapter is either named explicitly through `KEMORI_DEBUG_ADAPTER` (with
//! optional `KEMORI_DEBUG_ADAPTER_ARGS`) or inferred for a language Kemori knows
//! how to start without a launch configuration. When neither is available the
//! command explains the prerequisite instead of pretending to debug.

pub mod protocol;
pub mod session;
mod swift;

use std::net::TcpListener;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::language::LanguageId;

pub use session::{
    Breakpoint, DebugEvent, DebugSession, Scope, StackFrame, Variable, parse_scopes,
    parse_stack_frames, parse_variables,
};

/// How the adapter exposes DAP.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaunchTransport {
    /// DAP over the adapter's stdin/stdout.
    #[default]
    Stdio,
    /// DAP over a TCP socket the adapter announces on stdout (`dlv dap`).
    Tcp,
}

/// What to attach to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachTarget {
    /// A local process id, when the user gave one.
    pub pid: Option<u32>,
    /// Host and port for a remote/debug socket target.
    pub host: String,
    pub port: u16,
}

/// How to start debugging one program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchPlan {
    /// The adapter executable.
    pub adapter: String,
    /// Arguments passed to the adapter itself (not the debuggee).
    pub adapter_args: Vec<String>,
    /// The DAP `adapterID`.
    pub adapter_id: String,
    /// The program the adapter should launch.
    pub program: PathBuf,
    /// A human-readable adapter name for the console.
    pub label: String,
    /// Whether the adapter speaks DAP over stdio or TCP.
    pub transport: LaunchTransport,
    /// A known TCP address for adapters that do not announce one.
    pub tcp_address: Option<String>,
    /// Adapter-specific launch mode (`dlv` needs `mode: "debug"`).
    pub launch_mode: Option<&'static str>,
    /// Extra environment for the adapter (managed runtimes).
    pub env: Vec<(String, String)>,
    /// An attach target instead of a program launch.
    pub attach: Option<AttachTarget>,
}

/// Build a launch plan for `path`, or explain what is missing.
///
/// The environment override wins: `KEMORI_DEBUG_ADAPTER` names the adapter and
/// `KEMORI_DEBUG_ADAPTER_ARGS` supplies whitespace-separated arguments. Without
/// one, Python uses `debugpy` (installed by the user) and Rust/C/C++ use a
/// discovered `gdb` with the conventional debug binary. Other languages report
/// the prerequisite rather than guessing a configuration.
pub fn plan_for(language: LanguageId, path: &Path, root: &Path) -> Result<LaunchPlan, String> {
    if let Ok(adapter) = std::env::var("KEMORI_DEBUG_ADAPTER") {
        let adapter = adapter.trim().to_string();
        if !adapter.is_empty() {
            let args = std::env::var("KEMORI_DEBUG_ADAPTER_ARGS")
                .or_else(|_| std::env::var("KODA_DEBUG_ADAPTER_ARGS"))
                .map(|args| args.split_whitespace().map(ToString::to_string).collect())
                .unwrap_or_default();
            return Ok(LaunchPlan {
                adapter_id: adapter_id(&adapter),
                label: adapter_id(&adapter),
                adapter,
                adapter_args: args,
                program: path.to_path_buf(),
                transport: LaunchTransport::Stdio,
                tcp_address: None,
                launch_mode: None,
                env: Vec::new(),
                attach: None,
            });
        }
    }
    match language {
        LanguageId::Python => Ok(python_plan(
            path,
            crate::language::tools::debugpy_adapter_path(),
        )),
        LanguageId::Go => go_plan(path, root),
        LanguageId::JavaScript | LanguageId::TypeScript => js_plan(path),
        LanguageId::CSharp => csharp_plan(path, root),
        LanguageId::Dart => dart_plan(path),
        LanguageId::Ruby => ruby_plan(path),
        LanguageId::Php => php_plan(path),
        LanguageId::Rust => {
            let program = rust_debug_binary(root).unwrap_or_else(|| path.to_path_buf());
            native_plan(program)
        }
        LanguageId::C | LanguageId::Cpp => {
            let program = conventional_binary(path).unwrap_or_else(|| path.to_path_buf());
            native_plan(program)
        }
        LanguageId::Swift => swift::swift_plan(path, root),
        _ => Err(format!(
            "No zero-config debug adapter for {} — set KEMORI_DEBUG_ADAPTER (and KEMORI_DEBUG_ADAPTER_ARGS) or install one",
            language.name()
        )),
    }
}

/// Parse an attach target: a process id, `host:port`, or plain `port`.
pub fn parse_attach_target(input: &str) -> Result<AttachTarget, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter a process id or host:port".to_string());
    }
    if let Ok(pid) = input.parse::<u32>() {
        return Ok(AttachTarget {
            pid: Some(pid),
            host: "127.0.0.1".to_string(),
            port: 0,
        });
    }
    let (host, port) = match input.rsplit_once(':') {
        Some((host, port)) => (
            if host.is_empty() { "127.0.0.1" } else { host },
            port.parse::<u16>()
                .map_err(|_| "Invalid attach port".to_string())?,
        ),
        None => (
            "127.0.0.1",
            input
                .parse::<u16>()
                .map_err(|_| "Enter a process id or host:port".to_string())?,
        ),
    };
    Ok(AttachTarget {
        pid: None,
        host: host.to_string(),
        port,
    })
}

/// Build an attach plan for `language`, reusing the provisioned adapter.
pub fn attach_plan_for(
    language: LanguageId,
    target: &AttachTarget,
    path: &Path,
    root: &Path,
) -> Result<LaunchPlan, String> {
    let mut plan = match language {
        LanguageId::Python => python_plan(path, crate::language::tools::debugpy_adapter_path()),
        LanguageId::Go => go_plan(path, root)?,
        LanguageId::JavaScript | LanguageId::TypeScript => js_plan(path)?,
        LanguageId::CSharp => csharp_attach_plan(path)?,
        LanguageId::Php => php_plan(path)?,
        LanguageId::Dart => dart_plan(path)?,
        LanguageId::Ruby => {
            return Err(
                "rdbg attach is not wired yet; use launch or KEMORI_DEBUG_ADAPTER".to_string(),
            );
        }
        LanguageId::Java => {
            return Err(
                "Java attach needs the JDT LS attach flow; use KEMORI_DEBUG_ADAPTER for now"
                    .to_string(),
            );
        }
        _ => {
            return Err(format!(
                "No debug adapter to attach for {}",
                language.name()
            ));
        }
    };
    plan.attach = Some(target.clone());
    plan.program = path.to_path_buf();
    Ok(plan)
}

fn csharp_attach_plan(path: &Path) -> Result<LaunchPlan, String> {
    let adapter = crate::language::tools::locate("netcoredbg").ok_or_else(|| {
        "netcoredbg is not installed — install it from Language Setup".to_string()
    })?;
    Ok(csharp_plan_with(adapter, path.to_path_buf()))
}

/// Build the DAP `attach` arguments for an adapter and target.
pub fn attach_arguments(adapter_id: &str, target: &AttachTarget) -> Value {
    if let Some(pid) = target.pid {
        return match adapter_id {
            "go" => json!({ "mode": "local", "processId": pid }),
            _ => json!({ "processId": pid }),
        };
    }
    match adapter_id {
        "python" => json!({ "connect": { "host": target.host, "port": target.port } }),
        "pwa-node" | "node" => {
            json!({ "address": target.host, "port": target.port })
        }
        "go" => json!({ "mode": "remote", "host": target.host, "port": target.port }),
        "php" => json!({ "hostname": target.host, "port": target.port }),
        "dart" => json!({
            "vmServiceUri": format!("http://{}:{}/", target.host, target.port)
        }),
        _ => json!({ "host": target.host, "port": target.port }),
    }
}

/// Build the Python plan around a discovered `debugpy-adapter`, falling back to
/// `python3 -m debugpy.adapter` when the package was not installed by a
/// console-script installer.
///
/// Public so a test can exercise the provisioned/discovered path with a mock
/// adapter script instead of installing the package.
pub fn python_plan(path: &Path, adapter: Option<PathBuf>) -> LaunchPlan {
    match adapter {
        Some(adapter) => LaunchPlan {
            adapter: adapter.to_string_lossy().into_owned(),
            adapter_args: Vec::new(),
            adapter_id: "python".to_string(),
            program: path.to_path_buf(),
            label: "debugpy".to_string(),
            transport: LaunchTransport::Stdio,
            tcp_address: None,
            launch_mode: None,
            env: Vec::new(),
            attach: None,
        },
        None => LaunchPlan {
            adapter: "python3".to_string(),
            adapter_args: vec!["-m".to_string(), "debugpy.adapter".to_string()],
            adapter_id: "python".to_string(),
            program: path.to_path_buf(),
            label: "debugpy (python3 -m)".to_string(),
            transport: LaunchTransport::Stdio,
            tcp_address: None,
            launch_mode: None,
            env: Vec::new(),
            attach: None,
        },
    }
}

/// Build the Go plan around a provisioned/discovered `dlv`.
pub fn go_plan(path: &Path, root: &Path) -> Result<LaunchPlan, String> {
    let adapter = crate::language::tools::locate("dlv").ok_or_else(|| {
        "Delve (`dlv`) is not installed — install it from Language Setup".to_string()
    })?;
    Ok(go_plan_with(path, root, adapter))
}

/// Build the Go plan around an already-resolved `dlv`.
pub fn go_plan_with(path: &Path, root: &Path, adapter: PathBuf) -> LaunchPlan {
    // Delve debugs a package directory; a single-file main package's parent is
    // the right default.
    let program = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(root)
        .to_path_buf();
    LaunchPlan {
        adapter: adapter.to_string_lossy().into_owned(),
        adapter_args: vec!["dap".to_string(), "--listen=127.0.0.1:0".to_string()],
        adapter_id: "go".to_string(),
        program,
        label: "dlv".to_string(),
        transport: LaunchTransport::Tcp,
        tcp_address: None,
        launch_mode: Some("debug"),
        env: crate::language::tools::launch_env(crate::language::tools::Tool::Dlv),
        attach: None,
    }
}

/// Build the JS/TS plan around the provisioned js-debug DAP server.
pub fn js_plan(path: &Path) -> Result<LaunchPlan, String> {
    let adapter = crate::language::tools::locate("js-debug-dap")
        .ok_or_else(|| "js-debug is not installed — install it from Language Setup".to_string())?;
    let port = free_tcp_port()?;
    Ok(js_plan_with(path, adapter, port))
}

/// Build the JS/TS plan around an already-resolved launcher and port.
pub fn js_plan_with(path: &Path, adapter: PathBuf, port: u16) -> LaunchPlan {
    LaunchPlan {
        adapter: adapter.to_string_lossy().into_owned(),
        adapter_args: vec![port.to_string(), "127.0.0.1".to_string()],
        adapter_id: "pwa-node".to_string(),
        program: path.to_path_buf(),
        label: "js-debug".to_string(),
        transport: LaunchTransport::Tcp,
        tcp_address: Some(format!("127.0.0.1:{port}")),
        launch_mode: None,
        env: crate::language::tools::launch_env(crate::language::tools::Tool::JsDebug),
        attach: None,
    }
}

fn free_tcp_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|err| format!("could not reserve a debug port: {err}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|err| format!("could not read the debug port: {err}"))
}

/// Build the PHP plan around the provisioned DAP bridge.
pub fn php_plan(path: &Path) -> Result<LaunchPlan, String> {
    let adapter = crate::language::tools::locate("php-debug-dap").ok_or_else(|| {
        "The PHP debug bridge is not installed — install it from Language Setup".to_string()
    })?;
    if crate::language::tools::locate("php").is_none() {
        return Err("PHP is not installed — install a PHP runtime with Xdebug enabled".to_string());
    }
    Ok(php_plan_with(path, adapter))
}

/// Build a PHP plan from an already-resolved bridge launcher.
pub fn php_plan_with(path: &Path, adapter: PathBuf) -> LaunchPlan {
    LaunchPlan {
        adapter: adapter.to_string_lossy().into_owned(),
        adapter_args: Vec::new(),
        adapter_id: "php".to_string(),
        program: path.to_path_buf(),
        label: "php-debug".to_string(),
        transport: LaunchTransport::Stdio,
        tcp_address: None,
        launch_mode: None,
        env: crate::language::tools::launch_env(crate::language::tools::Tool::PhpDebug),
        attach: None,
    }
}

/// Build the Ruby plan around rdbg's VS Code DAP server.
pub fn ruby_plan(path: &Path) -> Result<LaunchPlan, String> {
    let adapter = crate::language::tools::locate("rdbg").ok_or_else(|| {
        "rdbg is not installed — install the Ruby debugger from Language Setup".to_string()
    })?;
    let port = free_tcp_port()?;
    Ok(ruby_plan_with(path, adapter, port))
}

/// Build a Ruby plan from an already-resolved `rdbg` and port.
pub fn ruby_plan_with(path: &Path, adapter: PathBuf, port: u16) -> LaunchPlan {
    let mut args = vec![
        "--open=vscode".to_string(),
        "--port".to_string(),
        port.to_string(),
    ];
    args.push(path.to_string_lossy().into_owned());
    LaunchPlan {
        adapter: adapter.to_string_lossy().into_owned(),
        adapter_args: args,
        adapter_id: "rdbg".to_string(),
        program: path.to_path_buf(),
        label: "rdbg".to_string(),
        transport: LaunchTransport::Tcp,
        tcp_address: Some(format!("127.0.0.1:{port}")),
        launch_mode: None,
        env: crate::language::tools::launch_env(crate::language::tools::Tool::Rdbg),
        attach: None,
    }
}

/// Build the Dart plan around the SDK's `dart debug_adapter`.
pub fn dart_plan(path: &Path) -> Result<LaunchPlan, String> {
    let adapter = crate::language::tools::locate("dart").ok_or_else(|| {
        "The Dart SDK is not installed — install it from Language Setup".to_string()
    })?;
    Ok(dart_plan_with(path, adapter))
}

/// Build a Dart plan from an already-resolved `dart` executable.
pub fn dart_plan_with(path: &Path, adapter: PathBuf) -> LaunchPlan {
    LaunchPlan {
        adapter: adapter.to_string_lossy().into_owned(),
        adapter_args: vec!["debug_adapter".to_string()],
        adapter_id: "dart".to_string(),
        program: path.to_path_buf(),
        label: "dart debug_adapter".to_string(),
        transport: LaunchTransport::Stdio,
        tcp_address: None,
        launch_mode: None,
        env: Vec::new(),
        attach: None,
    }
}

/// Build the C# plan around the provisioned netcoredbg adapter and the project's
/// built debug assembly.
pub fn csharp_plan(path: &Path, root: &Path) -> Result<LaunchPlan, String> {
    let adapter = crate::language::tools::locate("netcoredbg").ok_or_else(|| {
        "netcoredbg is not installed — install it from Language Setup".to_string()
    })?;
    let stem = csproj_stem(root).ok_or_else(|| {
        "No .csproj found in the project root; open the project folder or set a launch plan"
            .to_string()
    })?;
    let program = find_debug_dll(root, &stem).ok_or_else(|| {
        format!("No debug build found; run `dotnet build` first (expected bin/Debug/*/{stem}.dll)")
    })?;
    let _ = path;
    Ok(csharp_plan_with(adapter, program))
}

/// Build a C# plan from an already-resolved adapter and assembly.
pub fn csharp_plan_with(adapter: PathBuf, program: PathBuf) -> LaunchPlan {
    LaunchPlan {
        adapter: adapter.to_string_lossy().into_owned(),
        adapter_args: vec!["--interpreter=vscode".to_string()],
        adapter_id: "coreclr".to_string(),
        program,
        label: "netcoredbg".to_string(),
        transport: LaunchTransport::Stdio,
        tcp_address: None,
        launch_mode: None,
        env: Vec::new(),
        attach: None,
    }
}

fn csproj_stem(root: &Path) -> Option<String> {
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter_map(|entry| entry.path().extension().map(|_| entry.path()))
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("csproj"))
        .and_then(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
}

/// Find the newest `bin/Debug/<tfm>/<stem>.dll` under `root`.
fn find_debug_dll(root: &Path, stem: &str) -> Option<PathBuf> {
    fn collect(dir: &Path, stem: &str, out: &mut Vec<PathBuf>, depth: usize) {
        if depth > 6 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, stem, out, depth + 1);
            } else if path.file_name().and_then(|name| name.to_str())
                == Some(&format!("{stem}.dll"))
            {
                out.push(path);
            }
        }
    }
    let base = root.join("bin/Debug");
    if !base.is_dir() {
        return None;
    }
    let mut candidates = Vec::new();
    collect(&base, stem, &mut candidates, 0);
    candidates.into_iter().max_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    })
}

fn native_plan(program: PathBuf) -> Result<LaunchPlan, String> {
    if let Some(adapter) = crate::language::tools::lldb_dap_path() {
        return Ok(lldb_plan(program, adapter));
    }
    if crate::language::tools::locate("gdb").is_some() {
        return Ok(gdb_plan(program));
    }
    Err("No native debug adapter found — install `lldb-dap` (LLVM) or `gdb`".to_string())
}

/// Build a plan around LLVM's `lldb-dap` adapter.
///
/// Public so tests can drive a mock adapter through the real plan shape without
/// installing LLVM.
pub fn lldb_plan(program: PathBuf, adapter: PathBuf) -> LaunchPlan {
    LaunchPlan {
        adapter: adapter.to_string_lossy().into_owned(),
        adapter_args: Vec::new(),
        adapter_id: "lldb".to_string(),
        program,
        label: "lldb-dap".to_string(),
        transport: LaunchTransport::Stdio,
        tcp_address: None,
        launch_mode: None,
        env: Vec::new(),
        attach: None,
    }
}

fn gdb_plan(program: PathBuf) -> LaunchPlan {
    LaunchPlan {
        adapter: "gdb".to_string(),
        adapter_args: vec!["-i=dap".to_string(), "--quiet".to_string()],
        adapter_id: "gdb".to_string(),
        program,
        label: "gdb".to_string(),
        transport: LaunchTransport::Stdio,
        tcp_address: None,
        launch_mode: None,
        env: Vec::new(),
        attach: None,
    }
}

/// The binary `cargo build` produces for this package, if it exists.
fn rust_debug_binary(root: &Path) -> Option<PathBuf> {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).ok()?;
    let mut package = None;
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package
            && package.is_none()
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "name"
        {
            package = Some(value.trim().trim_matches('"').to_string());
        }
    }
    let candidate = root.join("target/debug").join(package?);
    candidate.is_file().then_some(candidate)
}

/// `a.out` or the source's stem beside it, as a conventional compiled binary.
fn conventional_binary(source: &Path) -> Option<PathBuf> {
    let parent = source.parent()?;
    let a_out = parent.join("a.out");
    if a_out.is_file() {
        return Some(a_out);
    }
    let stem = source.file_stem()?;
    let candidate = parent.join(stem);
    candidate.is_file().then_some(candidate)
}

fn adapter_id(program: &str) -> String {
    Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_python_through_debugpy() {
        let path = Path::new("/tmp/main.py");
        let fallback = python_plan(path, None);
        assert_eq!(fallback.adapter, "python3");
        assert_eq!(fallback.adapter_id, "python");
        assert_eq!(fallback.program, PathBuf::from("/tmp/main.py"));

        let discovered = python_plan(path, Some(PathBuf::from("/opt/debugpy-adapter")));
        assert_eq!(discovered.adapter, "/opt/debugpy-adapter");
        assert!(discovered.adapter_args.is_empty());
        assert_eq!(discovered.label, "debugpy");
    }

    #[test]
    fn plans_native_through_lldb_dap() {
        let plan = lldb_plan(
            PathBuf::from("/tmp/target/debug/app"),
            PathBuf::from("/usr/bin/lldb-dap"),
        );
        assert_eq!(plan.adapter, "/usr/bin/lldb-dap");
        assert!(plan.adapter_args.is_empty());
        assert_eq!(plan.adapter_id, "lldb");
        assert_eq!(plan.label, "lldb-dap");
        assert_eq!(plan.program, PathBuf::from("/tmp/target/debug/app"));
    }

    #[test]
    fn plans_go_through_dlv_over_tcp() {
        let plan = go_plan_with(
            Path::new("/tmp/project/cmd/app/main.go"),
            Path::new("/tmp/project"),
            PathBuf::from("/usr/bin/dlv"),
        );
        assert_eq!(plan.adapter, "/usr/bin/dlv");
        assert_eq!(
            plan.adapter_args,
            vec!["dap".to_string(), "--listen=127.0.0.1:0".to_string()]
        );
        assert_eq!(plan.adapter_id, "go");
        assert_eq!(plan.transport, LaunchTransport::Tcp);
        assert_eq!(plan.launch_mode, Some("debug"));
        assert_eq!(plan.program, PathBuf::from("/tmp/project/cmd/app"));
    }

    #[test]
    fn plans_javascript_through_js_debug_over_tcp() {
        let plan = js_plan_with(
            Path::new("/tmp/project/index.js"),
            PathBuf::from("/home/user/.local/share/kemori/tools/bin/js-debug-dap"),
            9911,
        );
        assert_eq!(plan.adapter_id, "pwa-node");
        assert_eq!(plan.transport, LaunchTransport::Tcp);
        assert_eq!(plan.tcp_address.as_deref(), Some("127.0.0.1:9911"));
        assert_eq!(
            plan.adapter_args,
            vec!["9911".to_string(), "127.0.0.1".to_string()]
        );
        assert_eq!(plan.program, PathBuf::from("/tmp/project/index.js"));
    }

    #[test]
    fn plans_csharp_through_netcoredbg() {
        let plan = csharp_plan_with(
            PathBuf::from("/opt/netcoredbg/netcoredbg"),
            PathBuf::from("/tmp/project/bin/Debug/net10.0/app.dll"),
        );
        assert_eq!(plan.adapter_id, "coreclr");
        assert_eq!(plan.adapter_args, vec!["--interpreter=vscode".to_string()]);
        assert_eq!(plan.transport, LaunchTransport::Stdio);
        assert_eq!(
            plan.program,
            PathBuf::from("/tmp/project/bin/Debug/net10.0/app.dll")
        );
    }

    #[test]
    fn plans_dart_through_the_sdk_debug_adapter() {
        let plan = dart_plan_with(
            Path::new("/tmp/project/bin/main.dart"),
            PathBuf::from("/opt/dart-sdk/bin/dart"),
        );
        assert_eq!(plan.adapter_id, "dart");
        assert_eq!(plan.adapter_args, vec!["debug_adapter".to_string()]);
        assert_eq!(plan.transport, LaunchTransport::Stdio);
        assert_eq!(plan.program, PathBuf::from("/tmp/project/bin/main.dart"));
    }

    #[test]
    fn plans_ruby_through_rdbg_over_tcp() {
        let plan = ruby_plan_with(
            Path::new("/tmp/project/app.rb"),
            PathBuf::from("/opt/gems/bin/rdbg"),
            9876,
        );
        assert_eq!(plan.adapter_id, "rdbg");
        assert_eq!(plan.transport, LaunchTransport::Tcp);
        assert_eq!(plan.tcp_address.as_deref(), Some("127.0.0.1:9876"));
        assert_eq!(
            plan.adapter_args,
            vec![
                "--open=vscode".to_string(),
                "--port".to_string(),
                "9876".to_string(),
                "/tmp/project/app.rb".to_string()
            ]
        );
    }

    #[test]
    fn plans_php_through_the_xdebug_bridge() {
        let plan = php_plan_with(
            Path::new("/tmp/project/index.php"),
            PathBuf::from("/opt/kemori/bin/php-debug-dap"),
        );
        assert_eq!(plan.adapter_id, "php");
        assert_eq!(plan.transport, LaunchTransport::Stdio);
        assert_eq!(plan.program, PathBuf::from("/tmp/project/index.php"));
    }

    #[test]
    fn parses_attach_targets() {
        let pid = parse_attach_target("1234").unwrap();
        assert_eq!(pid.pid, Some(1234));
        let socket = parse_attach_target("127.0.0.1:5678").unwrap();
        assert_eq!(socket.host, "127.0.0.1");
        assert_eq!(socket.port, 5678);
        let port = parse_attach_target(":5678").unwrap();
        assert_eq!(port.host, "127.0.0.1");
        assert_eq!(port.port, 5678);
        assert!(parse_attach_target("").is_err());
    }

    #[test]
    fn builds_attach_arguments_per_adapter() {
        let socket = AttachTarget {
            pid: None,
            host: "127.0.0.1".to_string(),
            port: 5678,
        };
        assert_eq!(
            attach_arguments("python", &socket)["connect"]["port"],
            serde_json::json!(5678)
        );
        assert_eq!(
            attach_arguments("go", &socket)["mode"],
            serde_json::json!("remote")
        );
        let pid = AttachTarget {
            pid: Some(99),
            host: String::new(),
            port: 0,
        };
        assert_eq!(
            attach_arguments("coreclr", &pid)["processId"],
            serde_json::json!(99)
        );
    }

    #[test]
    fn builds_an_attach_plan_for_python() {
        let target = AttachTarget {
            pid: None,
            host: "127.0.0.1".to_string(),
            port: 5678,
        };
        let plan = attach_plan_for(
            LanguageId::Python,
            &target,
            Path::new("/tmp/main.py"),
            Path::new("/tmp"),
        )
        .expect("python attach plan");
        assert_eq!(plan.attach, Some(target));
        assert_eq!(plan.adapter_id, "python");
    }

    #[test]
    fn names_the_prerequisite_for_an_unknown_language() {
        let result = plan_for(
            LanguageId::Kotlin,
            Path::new("/tmp/a.kt"),
            Path::new("/tmp"),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("KEMORI_DEBUG_ADAPTER"));
    }
}
