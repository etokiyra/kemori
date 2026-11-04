/// Verify a stdio language server by starting it and confirming it stays up.
///
/// The extracted VS Code servers reject every version query, so the only honest
/// check is to launch the real server and see that it does not exit with an
/// error. `stdin` is held open so the server initializes and waits; Kemori stops
/// it once the window elapses. A server that exits successfully on its own is
/// also accepted, and one that exits with a failure status is not.
fn probe_server(tool: Tool, path: PathBuf, mut command: Command) -> ToolStatus {
    command.args(tool.server_args());
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let Ok(mut child) = command.spawn() else {
        return ToolStatus {
            tool,
            available: false,
            version: None,
            path: Some(path),
            error: None,
        };
    };
    // Hold the write end so the server does not see EOF and shut down at once.
    let _stdin = child.stdin.take();
    let (available, error) = alive_or_clean(&mut child);
    ToolStatus {
        tool,
        available,
        version: None,
        path: Some(path),
        error,
    }
}

/// Whether a launched process is a usable server: still running when the window
/// elapses, or already exited cleanly. A process that fails immediately (the
/// exact symptom of a broken Node launcher or a missing shared library) is
/// rejected, and its first stderr line is kept for the error message.
fn alive_or_clean(child: &mut std::process::Child) -> (bool, Option<String>) {
    match wait_captured(child, PROBE_ALIVE_WINDOW, MAX_TOOL_OUTPUT) {
        Ok(captured) => {
            let available =
                captured.status.is_none() || captured.status.is_some_and(|status| status.success());
            let error = if available {
                None
            } else {
                probe_error(&captured.stderr)
            };
            (available, error)
        }
        Err(_) => (false, None),
    }
}

/// The first meaningful line of a failed probe's stderr, if any.
fn probe_error(stderr: &[u8]) -> Option<String> {
    let line = first_stderr_line(stderr);
    (!line.is_empty() && line != "installation failed").then_some(line)
}

/// Locate a tool's executable, trying its primary name and any alternates.
fn locate_tool(tool: Tool) -> Option<PathBuf> {
    std::iter::once(tool.program())
        .chain(tool.candidates().iter().copied())
        .find_map(locate)
}

/// Locate the stdio debug adapter shipped by `debugpy`.
///
/// `debugpy` installs two console scripts: `debugpy` (the launcher) and
/// `debugpy-adapter` (the DAP server). Prefer the adapter script; when only the
/// launcher was found, look for its sibling. `KEMORI_DEBUGPY_ADAPTER` is an
/// explicit override for tests and unusual installations.
pub fn debugpy_adapter_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("KEMORI_DEBUGPY_ADAPTER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(path) = locate("debugpy-adapter") {
        return Some(path);
    }
    let launcher = locate("debugpy")?;
    let sibling = launcher.with_file_name("debugpy-adapter");
    sibling.is_file().then_some(sibling)
}

/// Locate LLVM's DAP adapter, preferring the current `lldb-dap` name over the
/// older `lldb-vscode` one.
pub fn lldb_dap_path() -> Option<PathBuf> {
    locate_tool(Tool::LldbDap)
}

/// Whether `program` can be found, returning its full path.
///
/// `PATH` is searched first, then a handful of well-known user and system bin
/// directories. The latter matters because Kemori is often launched from a GUI or
/// a non-login shell whose `PATH` omits `~/.cargo/bin` and `~/.local/bin` —
/// exactly where rustup and pip put language tooling.
pub fn locate(program: &str) -> Option<PathBuf> {
    if let Some(found) = locate_on_path(program) {
        return Some(found);
    }
    known_bin_dirs()
        .into_iter()
        .find_map(|dir| candidate_in(&dir, program))
}

/// Search only `PATH`.
fn locate_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| candidate_in(&dir, program))
}

fn candidate_in(dir: &Path, program: &str) -> Option<PathBuf> {
    let candidate = dir.join(program);
    if candidate.is_file() {
        return Some(candidate);
    }
    let with_exe = dir.join(format!("{program}.exe"));
    with_exe.is_file().then_some(with_exe)
}

/// Directories where language tooling commonly lives.
fn known_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".cargo/bin"));
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join("bin"));
        dirs.push(home.join("go/bin"));
        dirs.push(home.join(".bun/bin"));
        dirs.push(home.join(".deno/bin"));
        dirs.push(home.join(".npm-global/bin"));
        dirs.push(home.join(".local/share/pnpm"));
        dirs.push(home.join(".dotnet/tools"));
        // Toolchains installed under the home directory rather than on the
        // login PATH: ElixirLS escripts, asdf shims, local::lib Perl, Swift and
        // a Flutter-bundled Dart SDK.
        dirs.push(home.join(".mix/escripts"));
        dirs.push(home.join(".asdf/shims"));
        dirs.push(home.join("perl5/bin"));
        dirs.push(home.join(".local/share/swiftly/bin"));
        dirs.push(home.join(".swiftly/bin"));
        dirs.push(home.join("development/flutter/bin/cache/dart-sdk/bin"));
        dirs.push(home.join(".pub-cache/bin"));
        // Nix profiles: `nil`, `nixfmt` and language toolchains are commonly
        // installed through them, and a GUI launch or `sudo` may not inherit the
        // shell's Nix profile on `PATH`.
        dirs.push(home.join(".nix-profile/bin"));
        dirs.push(home.join(".local/state/nix/profile/bin"));
        dirs.push(home.join(".local/state/nix/profiles/profile/bin"));
    }
    // The system-wide NixOS profile and the daemon's default profile are not
    // per-user, so they are searched even when HOME points at another account
    // (for example under `sudo`).
    dirs.push(PathBuf::from("/run/current-system/sw/bin"));
    dirs.push(PathBuf::from("/nix/var/nix/profiles/default/bin"));
    // Tools Kemori installed itself, plus the npm prefix, Python virtualenv,
    // managed Node.js and managed runtimes it maintains.
    if let Some(tools) = tools_dir() {
        if let Some(prefix) = npm_prefix() {
            dirs.push(prefix.join("bin"));
            dirs.push(prefix.join("node_modules/.bin"));
            dirs.push(prefix);
        }
        if let Some(venv) = venv_dir() {
            dirs.push(venv_bin_dir(&venv));
        }
        if let Some(node) = node_bin_dir() {
            dirs.push(node);
        }
        dirs.push(tools.join("omnisharp"));
        dirs.push(tools.join("jdtls/bin"));
        dirs.push(tools.join("lua-language-server/bin"));
        dirs.push(tools.join("kotlin-language-server/bin"));
        dirs.push(tools.join("dart-sdk/bin"));
        dirs.push(tools.join("perl5/bin"));
        dirs.push(tools.join("cpanm/bin"));
        dirs.push(tools.join("swift/usr/bin"));
        dirs.push(tools.join("otp/bin"));
        dirs.push(tools.join("elixir/bin"));
        dirs.push(tools.join("elixir-ls"));
        dirs.push(tools.join("go/bin"));
        dirs.push(tools.join("gopath/bin"));
        dirs.push(tools.join("clangd/bin"));
        dirs.push(tools.join("netcoredbg"));
        dirs.push(tools.join("asm-lsp"));
        dirs.push(tools.join("phpactor"));
        dirs.push(tools.join("nix-profile/bin"));
        dirs.push(tools.join("gems/bin"));
        dirs.push(tools.join("jdk/bin"));
        dirs.push(tools.join("dotnet"));
        dirs.push(tools.join("bin"));
    }
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/opt/llvm/bin"));
    dirs.push(PathBuf::from("/opt/homebrew/opt/llvm/bin"));
    dirs.push(PathBuf::from(
        "/Applications/Xcode.app/Contents/SharedFrameworks/LLDB.framework/Resources",
    ));
    // Distribution LLVM packages install `lldb-dap` under a versioned prefix.
    if let Ok(entries) = std::fs::read_dir("/usr/lib") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("llvm-") {
                dirs.push(entry.path().join("bin"));
            }
        }
    }
    dirs.push(PathBuf::from("/usr/local/go/bin"));
    dirs.push(PathBuf::from("/usr/local/swift/usr/bin"));
    dirs.push(PathBuf::from("/usr/lib/swift/bin"));
    dirs.push(PathBuf::from("/snap/bin"));
    dirs.push(PathBuf::from("/usr/bin"));
    dirs.push(PathBuf::from("/bin"));
    dirs
}
