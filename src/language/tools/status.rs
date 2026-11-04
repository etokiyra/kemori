/// Render a byte count for a download warning (`1.1 GB`, `240 MB`).
pub fn human_bytes(bytes: u64) -> String {
    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{} MB", bytes.div_ceil(MB))
    }
}

fn first_stderr_line(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("installation failed")
        .trim()
        .to_string()
}

/// The most useful line of a failed install command's stderr.
///
/// Package managers print the concrete error (the missing module, the compiler
/// error) before a trailing summary, so prefer the last line that names a
/// failure and otherwise fall back to the last line.
fn last_stderr_line(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines
        .iter()
        .rev()
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("error")
                || lower.contains("failed")
                || lower.contains("fatal")
                || lower.contains("cannot")
                || lower.contains("not found")
        })
        .or_else(|| lines.last())
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| "installation failed".to_string())
}

/// Whether every step of at least one install strategy can run here.
///
/// Kemori only offers to install a tool it can really install, so it never
/// promises an install and then fails because no package manager, `curl` or
/// archive tool exists.
pub fn can_install(tool: Tool) -> bool {
    // `jdtls` is a Python launcher script; without a Python interpreter it
    // cannot run even though its download plan only needs `curl` and `tar`.
    if tool == Tool::Jdtls && locate("python3").is_none() && locate("python").is_none() {
        return false;
    }
    tool.install_attempts()
        .iter()
        .any(|attempt| attempt.steps.iter().all(step_available))
}

fn step_available(step: &InstallStep) -> bool {
    match step {
        InstallStep::Run(command) => {
            locate(&command.program).is_some()
                || is_user_bin_program(&command.program)
                // A program supplied by a Kemori-managed runtime an earlier step
                // in the same attempt installs (for example `elixir` before the
                // ElixirLS build).
                || is_kemori_provided_program(&command.program)
        }
        InstallStep::Download { .. }
        | InstallStep::AdoptiumJdk { .. }
        | InstallStep::DotnetSdk { .. }
        | InstallStep::NodeRuntime { .. }
        | InstallStep::DartSdk { .. }
        | InstallStep::GithubRelease { .. } => locate("curl").is_some(),
        InstallStep::RustupInit { .. } => locate("curl").is_some() && rustup_target().is_some(),
        // The official Go toolchain is resolved from a published JSON document
        // at install time; only `curl` and a known platform are needed here.
        InstallStep::GoToolchain { .. } => locate("curl").is_some() && go_platform().is_some(),
        // A file can always be marked executable on a Unix host.
        InstallStep::MakeExecutable { .. } => cfg!(unix),
        // A launcher is a local shell script, only useful on Unix.
        InstallStep::WriteLauncher { .. } => cfg!(unix),
        // A `bob` build also needs a supported platform directory.
        InstallStep::BobBuild { .. } => locate("curl").is_some() && bob_platform().is_some(),
        // The compatibility layer can only be built when every library it needs
        // is available on the system.
        InstallStep::SwiftCompat { .. } => swift_missing_libs().is_empty(),
        // A signature-verified download also needs `gpg`.
        InstallStep::DownloadGpg { .. } => locate("curl").is_some() && locate("gpg").is_some(),
        InstallStep::Extract { archive, .. } => {
            let zip = archive.extension().and_then(|ext| ext.to_str()) == Some("zip");
            if zip {
                // Any of `unzip`, `bsdtar` or Python can unpack a zip.
                zip_extractor().is_some()
            } else {
                locate("tar").is_some()
            }
        }
    }
}

/// Whether `program` is an absolute path inside a user bin directory.
///
/// Such a program may not exist yet but is produced by an earlier step in the
/// strategy — the `rustup` bootstrap installs `~/.cargo/bin/rustup`, and a
/// managed virtualenv's `pip` is created by `python -m venv` just before it is
/// used. Treating these as available lets Kemori offer the strategy it can run
/// instead of rejecting it up front.
fn is_user_bin_program(program: &str) -> bool {
    let path = Path::new(program);
    path.is_absolute() && known_bin_dirs().iter().any(|dir| path.starts_with(dir))
}

/// Whether a program is supplied by a runtime Kemori installs itself.
///
/// An install attempt may run one of these after an earlier step has produced
/// it, so a missing binary is not a reason to withhold the offer.
fn is_kemori_provided_program(program: &str) -> bool {
    matches!(
        program,
        "elixir" | "mix" | "erl" | "escript" | "go" | "cargo"
    )
}
