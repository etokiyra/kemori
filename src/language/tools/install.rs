
/// A best-effort advisory lock over the managed tools directory.
///
/// Two Kemori instances sharing a data directory must not install into the same
/// npm prefix or Python virtualenv at once. The lock is a file created with
/// `create_new`; a stale one (from a crashed instance) is reclaimed by age.
struct InstallLock {
    path: PathBuf,
    /// Identifies this acquisition, so `Drop` only removes the lock if it is
    /// still ours. Without it, a holder that was reclaimed as stale could
    /// delete the new holder's lock on exit.
    nonce: String,
}

impl InstallLock {
    fn acquire() -> Result<Self, String> {
        let dir = tools_dir().ok_or_else(|| "no data directory for tools".to_string())?;
        Self::acquire_at(&dir)
    }

    fn acquire_at(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        let path = dir.join("install.lock");
        let nonce = format!(
            "{}:{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or(0)
        );

        if let Ok(file) = Self::create_new(&path, &nonce) {
            drop(file);
            return Ok(InstallLock { path, nonce });
        }

        // The lock exists. Reclaim it only if it is old enough to be from a
        // crashed instance; a live install must never be disturbed.
        let stale = std::fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age > STALE_INSTALL_LOCK);
        if !stale {
            return Err(
                "another Kemori instance is installing tools — try again shortly".to_string(),
            );
        }
        let _ = std::fs::remove_file(&path);
        match Self::create_new(&path, &nonce) {
            Ok(file) => {
                drop(file);
                Ok(InstallLock { path, nonce })
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                Err("another Kemori instance is installing tools — try again shortly".to_string())
            }
            Err(err) => Err(err.to_string()),
        }
    }

    fn create_new(path: &Path, nonce: &str) -> std::io::Result<std::fs::File> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        let _ = writeln!(file, "{nonce}");
        Ok(file)
    }
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        // Only remove the lock if it is still the one we created.
        if let Ok(contents) = std::fs::read_to_string(&self.path)
            && contents.trim() == self.nonce
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Install a tool by trying each strategy and verifying the result.
///
/// Intended for the background worker. A strategy's commands run in order; if
/// one fails the strategy is abandoned, and after every strategy Kemori re-probes
/// the tool — a package manager can report success without producing a binary
/// Kemori can find. Returns a short success message or the most recent
/// actionable error.
///
/// A lock around the managed tools directory keeps two Kemori instances from
/// installing into the same npm prefix or Python virtualenv at once.
pub fn install(tool: Tool) -> Result<String, String> {
    let attempts = tool.install_attempts();
    if attempts.is_empty() {
        return Err(format!(
            "{} cannot be installed automatically — {}",
            tool.label(),
            tool.setup_reason()
        ));
    }
    // Pre-flight the plan before any install subprocess runs: keep only
    // strategies whose every step can actually run, so an obvious failure is
    // reported before an earlier tool in the plan is installed and before a
    // large download starts.
    let runnable = filter_runnable(attempts);
    if runnable.is_empty() {
        return Err(format!(
            "{} cannot be installed — {}",
            tool.label(),
            tool.setup_reason()
        ));
    }

    let _lock = InstallLock::acquire()?;
    preflight(tool)?;

    let mut last_error = None;
    for attempt in &runnable {
        let mut completed = true;
        for step in &attempt.steps {
            if let Err(message) = run_step(step) {
                last_error = Some(format!("{}: {message}", tool.label()));
                completed = false;
                break;
            }
        }
        // Only a strategy whose steps all succeeded may be trusted, even if a
        // stale binary from a previous attempt happens to probe as available.
        if completed {
            let status = probe(tool);
            if status.available {
                return Ok(format!("Installed {}", tool.label()));
            }
            // The steps worked but the tool does not run: keep the reason (a
            // missing library, a broken launcher) rather than a generic failure.
            if let Some(detail) = status.error {
                last_error = Some(format!("{}: {detail}", tool.label()));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| format!("could not install {}", tool.label())))
}

/// The strategies whose every step can actually run on this host.
fn filter_runnable(attempts: Vec<InstallAttempt>) -> Vec<InstallAttempt> {
    attempts
        .into_iter()
        .filter(|attempt| attempt.steps.iter().all(step_available))
        .collect()
}

/// Validate an install plan before any destructive work begins.
///
/// This checks only what Kemori can know cheaply and reliably: that its tools
/// directory exists and is writable, and that the target filesystem has room for
/// the estimated download. Network availability, proxy reachability and archive
/// contents are necessarily runtime checks and are not predicted here.
fn preflight(tool: Tool) -> Result<(), String> {
    let dir = tools_dir().ok_or_else(|| "no data directory for managed tools".to_string())?;
    ensure_tools_dir_writable(&dir)?;
    if let Some(bytes) = tool.estimated_download_bytes() {
        ensure_disk_space(&dir, bytes)?;
    }
    Ok(())
}

/// Confirm Kemori can create and write inside its managed tools directory.
fn ensure_tools_dir_writable(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|err| {
        format!(
            "could not create Kemori's tools directory {}: {err}",
            dir.display()
        )
    })?;
    let probe = temp_sibling(&dir.join("write-probe"))?;
    std::fs::write(&probe, b"").map_err(|err| {
        format!(
            "Kemori's tools directory {} is not writable: {err}",
            dir.display()
        )
    })?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// Execute one install step.
fn run_step(step: &InstallStep) -> Result<(), String> {
    match step {
        InstallStep::Run(command) => run_command(command),
        InstallStep::Download { url, dest, sha256 } => download(url, dest, Some(sha256)),
        InstallStep::BobBuild { package, dest } => bob_build(*package, dest),
        InstallStep::AdoptiumJdk { release, dest } => adoptium_jdk(release, dest),
        InstallStep::DotnetSdk { dest } => dotnet_sdk(dest),
        InstallStep::NodeRuntime { dest } => node_runtime(dest),
        InstallStep::DartSdk { dest } => dart_sdk(dest),
        InstallStep::RustupInit { dest } => rustup_init(dest),
        InstallStep::DownloadGpg {
            url,
            dest,
            signature_url,
            keys_url,
        } => download_gpg(url, dest, signature_url, keys_url),
        InstallStep::GithubRelease {
            repo,
            tag,
            asset,
            dest,
        } => github_release(repo, tag, asset, dest),
        InstallStep::SwiftCompat { dest } => swift_compat(dest),
        InstallStep::GoToolchain { dest } => go_toolchain(dest),
        InstallStep::MakeExecutable { path } => make_executable(path),
        InstallStep::WriteLauncher { path, contents } => write_launcher(path, contents),
        InstallStep::Extract {
            archive,
            dest,
            strip,
        } => extract(archive, dest, *strip),
    }
}

/// A `Command` for an install subprocess, run from Kemori's own tools directory.
///
/// Installing from the user's project directory lets a repository-local
/// `.npmrc` (or an equivalent per-directory config) redirect a package manager
/// to an attacker-controlled registry. Running from a Kemori-managed directory
/// removes that vector while leaving the user's own `~/.npmrc` and similar
/// configuration in effect.
///
/// Kemori's managed runtimes (Node.js, Erlang/OTP, Elixir, Dart, Perl, Swift) are
/// put on `PATH`, so a managed `elixir`, `npm` or `perl` and any
/// `#!/usr/bin/env` launcher work even before the user has installed anything.
fn install_command(program: &str) -> Command {
    let mut command = Command::new(program);
    if let Some(dir) = install_work_dir() {
        command.current_dir(dir);
    }
    if let Some(path) = managed_path() {
        command.env("PATH", path);
    }
    command
}

/// `PATH` with every Kemori-managed runtime bin directory first.
fn managed_path() -> Option<String> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(node) = node_bin_dir() {
        dirs.push(node);
    }
    if let Some(tools) = tools_dir() {
        dirs.push(tools.join("otp/bin"));
        dirs.push(tools.join("elixir/bin"));
        dirs.push(tools.join("dart-sdk/bin"));
        dirs.push(tools.join("perl5/bin"));
        dirs.push(tools.join("swift/usr/bin"));
        dirs.push(tools.join("go/bin"));
        dirs.push(tools.join("gopath/bin"));
        dirs.push(tools.join("clangd/bin"));
        dirs.push(tools.join("asm-lsp"));
        dirs.push(tools.join("phpactor"));
        dirs.push(tools.join("nix-profile/bin"));
        dirs.push(tools.join("gems/bin"));
        dirs.push(tools.join("kotlin-jdk/bin"));
        dirs.push(tools.join("jdk/bin"));
        dirs.push(tools.join("dotnet"));
    }
    if dirs.is_empty() {
        return None;
    }
    let prefix: Vec<String> = dirs
        .iter()
        .map(|dir| dir.to_string_lossy().into_owned())
        .collect();
    let existing = std::env::var("PATH").unwrap_or_default();
    Some(format!("{}:{existing}", prefix.join(":")))
}

/// The directory install subprocesses run from, created on demand.
fn install_work_dir() -> Option<PathBuf> {
    let dir = tools_dir()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

/// Run a program with a timeout and bounded output, reporting its first stderr
/// line on failure.
///
/// A hung package manager would otherwise stall background work, and a verbose
/// tool could exhaust memory through unbounded capture.
fn run_command(command: &InstallCommand) -> Result<(), String> {
    use std::process::Stdio;

    let mut child = install_command(&command.program);
    child.args(&command.args);
    if let Some(dir) = &command.cwd {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("could not create {}: {err}", dir.display()))?;
        child.current_dir(dir);
    }
    for (key, value) in &command.env {
        child.env(key, value);
    }
    let mut child = child
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("could not run {}: {err}", command.program))?;

    let captured = crate::process::wait_captured(&mut child, COMMAND_TIMEOUT, MAX_TOOL_OUTPUT)
        .map_err(|err| format!("could not run {}: {err}", command.program))?;
    match captured.status {
        None => Err(format!(
            "{} timed out after {} minutes",
            command.program,
            COMMAND_TIMEOUT.as_secs() / 60
        )),
        Some(status) if status.success() => Ok(()),
        // Package managers print the actual error last (the syntax error, the
        // missing module), so the tail line is the useful one.
        Some(status) => Err(format!(
            "{} exited with {status}: {}",
            command.program,
            last_stderr_line(&captured.stderr)
        )),
    }
}
