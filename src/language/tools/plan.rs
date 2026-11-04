
/// One command in an install attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallCommand {
    pub program: String,
    pub args: Vec<String>,
    /// Extra environment variables for this command (for example the managed
    /// `MIX_HOME`/`HEX_HOME` an Elixir build needs).
    pub env: Vec<(String, String)>,
    /// Working directory for this command, when it must run somewhere specific
    /// (the ElixirLS release directory its installer expects).
    pub cwd: Option<PathBuf>,
}

impl InstallCommand {
    fn new(program: &str, args: &[&str]) -> Self {
        InstallCommand {
            program: program.to_string(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            env: Vec::new(),
            cwd: None,
        }
    }

    fn with_args(program: impl Into<String>, args: Vec<String>) -> Self {
        InstallCommand {
            program: program.into(),
            args,
            env: Vec::new(),
            cwd: None,
        }
    }

    /// Add an environment variable to this command.
    fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Run this command from `dir`.
    fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }
}

/// One executable step in an install strategy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallStep {
    /// Run a program with arguments.
    Run(InstallCommand),
    /// Download `url` to `dest` over HTTPS, verifying `sha256`.
    ///
    /// The digest is required: there is no way to construct this step without
    /// verification, so a new managed download must supply one (or use a step
    /// that verifies by another mechanism, such as [`InstallStep::GithubRelease`]).
    Download {
        url: String,
        dest: PathBuf,
        sha256: String,
    },
    /// Download and verify an Erlang/OTP or Elixir build from `builds.hex.pm`,
    /// whose SHA-256 the same service publishes in `builds.txt`. The archive
    /// still needs an [`InstallStep::Extract`].
    BobBuild { package: BobPackage, dest: PathBuf },
    /// Download and verify an Eclipse Adoptium JDK release, whose checksum
    /// Adoptium publishes in the same JSON document that carries the link. The
    /// `release` pins the exact version.
    AdoptiumJdk {
        release: &'static str,
        dest: PathBuf,
    },
    /// Download and verify the pinned .NET SDK for OmniSharp, whose SHA-512
    /// Microsoft publishes in the channel's `releases.json`. The archive still
    /// needs an [`InstallStep::Extract`].
    DotnetSdk { dest: PathBuf },
    /// Download and verify a Kemori-managed Node.js runtime, whose checksum the
    /// Node.js project publishes in `SHASUMS256.txt`. The archive still needs
    /// an [`InstallStep::Extract`].
    NodeRuntime { dest: PathBuf },
    /// Download and verify a Kemori-managed Dart SDK, whose checksum Google
    /// publishes beside the archive. The archive still needs an
    /// [`InstallStep::Extract`].
    DartSdk { dest: PathBuf },
    /// Download, verify and run the pinned `rustup-init` binary, bootstrapping
    /// the Rust toolchain without the unverified `sh.rustup.rs` script.
    RustupInit { dest: PathBuf },
    /// Download `url` and verify it against a detached GPG signature using the
    /// public keys at `keys_url`. Used for toolchains whose only published
    /// integrity data is a signature (the Swift toolchain). Fails closed: if
    /// `gpg` is missing or the signature does not verify, nothing is kept.
    DownloadGpg {
        url: String,
        dest: PathBuf,
        signature_url: String,
        keys_url: String,
    },
    /// Download a GitHub release asset and verify its SHA-256, which GitHub's
    /// API now reports as an asset `digest`. The archive still needs an
    /// [`InstallStep::Extract`].
    GithubRelease {
        repo: String,
        tag: String,
        asset: String,
        dest: PathBuf,
    },
    /// Create Kemori's Swift compatibility directory: a `libncurses.so.6` alias to
    /// the system's wide library and a link to the system's `libxml2.so.2`,
    /// without modifying the system.
    SwiftCompat { dest: PathBuf },
    /// Download and verify the latest stable official Go toolchain, whose
    /// version and SHA-256 `go.dev/dl` publishes in one JSON document. The
    /// archive still needs an [`InstallStep::Extract`].
    GoToolchain { dest: PathBuf },
    /// Mark a file executable. Used for a downloaded single-file program (a
    /// `.phar`, a prebuilt binary) that must run directly.
    MakeExecutable { path: PathBuf },
    /// Write a small local launcher. Used when the real program is a script that
    /// must be run through a runtime Kemori manages (the js-debug DAP server).
    WriteLauncher { path: PathBuf, contents: String },
    /// Extract a `.tar.gz`/`.tar.xz`/`.zip` archive into `dest`, optionally
    /// dropping `strip` leading path components.
    Extract {
        archive: PathBuf,
        dest: PathBuf,
        strip: usize,
    },
}

impl From<InstallCommand> for InstallStep {
    fn from(command: InstallCommand) -> Self {
        InstallStep::Run(command)
    }
}

/// An Erlang/Elixir build distributed by `builds.hex.pm` (the Erlang Ecosystem
/// Foundation's `bob` build service, which also backs `setup-beam`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BobPackage {
    /// An Erlang/OTP runtime.
    Erlang,
    /// An Elixir distribution compiled for a specific OTP major.
    Elixir,
}

/// A strategy for installing a tool: an ordered command sequence that Kemori runs
/// and then verifies by re-probing the tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallAttempt {
    /// A short description of the strategy.
    pub via: &'static str,
    pub steps: Vec<InstallStep>,
}

impl InstallAttempt {
    fn one(via: &'static str, command: InstallCommand) -> Self {
        InstallAttempt {
            via,
            steps: vec![InstallStep::Run(command)],
        }
    }

    fn sequence(via: &'static str, commands: Vec<InstallCommand>) -> Self {
        InstallAttempt {
            via,
            steps: commands.into_iter().map(InstallStep::Run).collect(),
        }
    }

    fn managed(via: &'static str, steps: Vec<InstallStep>) -> Self {
        InstallAttempt { via, steps }
    }
}

/// Install a `rustup` component, bootstrapping `rustup` itself when absent.
fn component_attempts(component: &'static str) -> Vec<InstallAttempt> {
    let mut attempts = vec![InstallAttempt::one(
        "rustup",
        InstallCommand::new("rustup", &["component", "add", component]),
    )];
    if let Some(home) = home_dir() {
        let rustup = home.join(".cargo/bin/rustup");
        if let Some(bootstrap) = rustup_bootstrap_with(InstallCommand::with_args(
            rustup.to_string_lossy().into_owned(),
            vec!["component".into(), "add".into(), component.into()],
        )) {
            attempts.push(bootstrap);
        }
    }
    attempts
}

/// Install a Nix package from the pinned nixpkgs revision into a Kemori-private
/// Nix profile.
///
/// Nix is the only practical source for `nil`/`nixfmt` (no prebuilt release, not
/// on crates.io), and it verifies every store path it builds. The profile is
/// kept under Kemori's tools directory so the user's own profile is untouched.
fn nix_attempts(package: &'static str) -> Vec<InstallAttempt> {
    let Some(profile) = nix_profile_dir() else {
        return Vec::new();
    };
    let installable = format!("github:NixOS/nixpkgs/{NIXPKGS_REV}#{package}");
    let command = InstallCommand::with_args(
        "nix",
        vec![
            "--extra-experimental-features".to_string(),
            "nix-command flakes".to_string(),
            "profile".to_string(),
            "install".to_string(),
            "--profile".to_string(),
            profile.to_string_lossy().into_owned(),
            installable,
        ],
    );
    vec![InstallAttempt::one("nix profile", command)]
}

/// The Nix profile Kemori installs Nix tooling into, when a tools directory is
/// available.
fn nix_profile_dir() -> Option<PathBuf> {
    Some(tools_dir()?.join("nix-profile"))
}

/// Install a Rust tool with `cargo`, bootstrapping the toolchain when absent.
fn cargo_attempts(package: &str, version: &str) -> Vec<InstallAttempt> {
    let args = cargo_install_args(package, version);
    let mut attempts = vec![InstallAttempt::one(
        "cargo install",
        InstallCommand::with_args("cargo", args.clone()),
    )];
    if let Some(home) = home_dir() {
        let cargo = home.join(".cargo/bin/cargo");
        if let Some(bootstrap) = rustup_bootstrap_with(InstallCommand::with_args(
            cargo.to_string_lossy().into_owned(),
            args,
        )) {
            attempts.push(bootstrap);
        }
    }
    attempts
}

/// The pinned `cargo install <crate> --version <version>` arguments.
fn cargo_install_args(package: &str, version: &str) -> Vec<String> {
    vec![
        "install".to_string(),
        package.to_string(),
        "--version".to_string(),
        version.to_string(),
    ]
}

fn home_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("HOME")?))
}

/// The `rustup-init` build target for this platform, or `None` where rustup
/// publishes no build.
fn rustup_target() -> Option<&'static str> {
    let musl = libc_is_musl();
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") if musl => "x86_64-unknown-linux-musl",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") if musl => "aarch64-unknown-linux-musl",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => return None,
    })
}

/// The first SHA-256 in a checksum body (`<hex> *./name` or `<hex>  name`).
fn first_sha256(body: &str) -> Option<String> {
    body.split_whitespace()
        .find(|token| is_sha256_hex(token))
        .map(str::to_string)
}

/// Download, verify and run the pinned `rustup-init` binary.
///
/// The old bootstrap ran the moving `sh.rustup.rs` script with no integrity
/// check. rustup publishes a per-target `rustup-init` binary and its SHA-256
/// under a versioned archive URL, so Kemori downloads the exact binary, verifies
/// it, and runs it non-interactively; rustup then verifies the toolchain it
/// installs through its own signed manifests.
fn rustup_init(dest: &Path) -> Result<(), String> {
    let target =
        rustup_target().ok_or_else(|| "rustup publishes no build for this platform".to_string())?;
    let base = format!("https://static.rust-lang.org/rustup/archive/{RUSTUP_VERSION}/{target}");
    let sums = curl_text(&format!("{base}/rustup-init.sha256"))
        .map_err(|err| format!("could not fetch the rustup checksum: {err}"))?;
    let checksum = first_sha256(&sums)
        .ok_or_else(|| "rustup published no checksum for rustup-init".to_string())?;
    download(&format!("{base}/rustup-init"), dest, Some(&checksum))?;
    make_executable(dest)?;
    run_command(&InstallCommand::with_args(
        dest.to_string_lossy().into_owned(),
        vec![
            "-y".into(),
            "--no-modify-path".into(),
            "--profile".into(),
            "minimal".into(),
        ],
    ))
}

/// Bootstrap the Rust toolchain with the verified `rustup-init` binary, then run
/// `final_command`. Skipped on Windows, where Kemori has no supported path.
fn rustup_bootstrap_with(final_command: InstallCommand) -> Option<InstallAttempt> {
    if cfg!(windows) || home_dir().is_none() || rustup_target().is_none() {
        return None;
    }
    let dest = downloads_dir()?.join("rustup-init");
    Some(InstallAttempt::managed(
        "the official rustup installer",
        vec![
            InstallStep::RustupInit { dest },
            InstallStep::Run(final_command),
        ],
    ))
}

/// Install Go tooling with `go install`, provisioning Go itself when absent.
///
/// `go install` writes to a Kemori-private `GOBIN`, so the tools never land in
/// the user's `~/go` and never mix with a system Go's installations. The first
/// attempt uses whatever `go` the user has; the second downloads the official
/// Go toolchain into Kemori's data directory.
fn go_attempts(packages: &[&str]) -> Vec<InstallAttempt> {
    let install = go_install_command(packages);
    let mut attempts = vec![InstallAttempt::one("go install", install.clone())];
    if let Some(managed) = managed_go_attempt(Some(install)) {
        attempts.push(managed);
    }
    attempts
}

/// Download the official Go toolchain (and optionally run a final command with
/// it), when Kemori can provision Go on this platform.
fn managed_go_attempt(final_command: Option<InstallCommand>) -> Option<InstallAttempt> {
    go_platform()?;
    let downloads = downloads_dir()?;
    let dest = go_dir()?;
    let archive = downloads.join("go.tar.gz");
    let mut steps = vec![
        InstallStep::GoToolchain {
            dest: archive.clone(),
        },
        InstallStep::Extract {
            archive,
            dest,
            strip: 1,
        },
    ];
    if let Some(command) = final_command {
        steps.push(InstallStep::Run(command));
    }
    Some(InstallAttempt::managed("the official Go toolchain", steps))
}

/// Install the Go toolchain alone; `gofmt` ships with it.
fn go_toolchain_attempts() -> Vec<InstallAttempt> {
    managed_go_attempt(None).into_iter().collect()
}

/// The `go install` command for a set of pinned `module@version` specs, with a
/// Kemori-private `GOPATH`/`GOBIN` so installations stay isolated.
fn go_install_command(specs: &[&str]) -> InstallCommand {
    let mut args = vec!["install".to_string()];
    for spec in specs {
        args.push((*spec).to_string());
    }
    let mut command = InstallCommand::with_args("go", args);
    if let (Some(gopath), Some(gobin)) = (go_path(), go_bin_dir()) {
        command = command
            .env("GOPATH", gopath.to_string_lossy().into_owned())
            .env("GOBIN", gobin.to_string_lossy().into_owned())
            .env("GOFLAGS", "-mod=mod");
    }
    // Only pin `GOROOT` to a managed Go that is actually present; an invalid
    // `GOROOT` makes a perfectly good system `go` refuse to run, which would
    // force a needless managed download.
    if let Some(goroot) = go_dir().filter(|dir| dir.join("bin/go").is_file()) {
        command = command.env("GOROOT", goroot.to_string_lossy().into_owned());
    }
    command
}

/// The standalone `clangd` bundle for this platform, or `None`.
fn clangd_asset() -> Option<String> {
    Some(match std::env::consts::OS {
        "linux" => format!("clangd-linux-{CLANGD_VERSION}.zip"),
        "macos" => format!("clangd-mac-{CLANGD_VERSION}.zip"),
        "windows" => format!("clangd-windows-{CLANGD_VERSION}.zip"),
        _ => return None,
    })
}

/// Download the official, self-contained `clangd` release.
fn clangd_attempts() -> Vec<InstallAttempt> {
    let Some(asset) = clangd_asset() else {
        return Vec::new();
    };
    // The prebuilt clangd bundles link against glibc and libstdc++, so they
    // cannot run on musl systems.
    if libc_is_musl() {
        return Vec::new();
    }
    let (Some(downloads), Some(dest)) = (downloads_dir(), clangd_dir()) else {
        return Vec::new();
    };
    let archive = downloads.join(&asset);
    vec![InstallAttempt::managed(
        "the official clangd release",
        vec![
            InstallStep::GithubRelease {
                repo: "clangd/clangd".to_string(),
                tag: CLANGD_VERSION.to_string(),
                asset,
                dest: archive.clone(),
            },
            // The archive root is `clangd_<version>/`.
            InstallStep::Extract {
                archive,
                dest,
                strip: 1,
            },
        ],
    )]
}

/// The prebuilt `asm-lsp` archive for this platform, or `None` when the project
/// publishes none (for example aarch64 Linux).
fn asm_lsp_asset() -> Option<String> {
    // The prebuilt binaries are glibc/macOS builds; musl uses the cargo plan.
    if libc_is_musl() {
        return None;
    }
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "asm-lsp-x86_64-unknown-linux-gnu.tar.gz".to_string(),
        ("macos", "x86_64") => "asm-lsp-x86_64-apple-darwin.tar.gz".to_string(),
        ("macos", "aarch64") => "asm-lsp-aarch64-apple-darwin.tar.gz".to_string(),
        _ => return None,
    })
}

/// Install `asm-lsp`, preferring the prebuilt release over a `cargo` build.
fn asm_lsp_attempts() -> Vec<InstallAttempt> {
    let mut attempts = Vec::new();
    if let Some(asset) = asm_lsp_asset()
        && let (Some(downloads), Some(dest)) = (downloads_dir(), asm_lsp_dir())
    {
        let archive = downloads.join(&asset);
        attempts.push(InstallAttempt::managed(
            "the prebuilt asm-lsp release",
            vec![
                InstallStep::GithubRelease {
                    repo: "bergercookie/asm-lsp".to_string(),
                    tag: format!("v{ASM_LSP_VERSION}"),
                    asset,
                    dest: archive.clone(),
                },
                // The archive contains a single `asm-lsp` binary at its root.
                InstallStep::Extract {
                    archive,
                    dest,
                    strip: 0,
                },
            ],
        ));
    }
    attempts.extend(cargo_attempts("asm-lsp", ASM_LSP_VERSION));
    attempts
}

/// Install `phpactor.phar`, which needs only a PHP runtime.
fn phpactor_attempts() -> Vec<InstallAttempt> {
    if locate("php").is_none() {
        return Vec::new();
    }
    let Some(dest) = phpactor_dir() else {
        return Vec::new();
    };
    let phar = dest.join("phpactor");
    vec![InstallAttempt::managed(
        "the official phpactor.phar",
        vec![
            InstallStep::GithubRelease {
                repo: "phpactor/phpactor".to_string(),
                tag: PHPACTOR_VERSION.to_string(),
                asset: "phpactor.phar".to_string(),
                dest: phar.clone(),
            },
            // The phar starts with `#!/usr/bin/env php`; make it runnable under
            // the name `phpactor`.
            InstallStep::MakeExecutable { path: phar },
        ],
    )]
}

/// Install `solargraph` into a Kemori-private gem home.
///
/// A system Ruby is still required; Kemori only isolates the gems so the user's
/// global gem environment is untouched.
fn ruby_attempts() -> Vec<InstallAttempt> {
    if locate("gem").is_none() {
        return Vec::new();
    }
    let Some(gems) = gem_home() else {
        return Vec::new();
    };
    let bindir = gems.join("bin");
    vec![InstallAttempt::one(
        "gem install (isolated gem home)",
        InstallCommand::with_args(
            "gem",
            vec![
                "install".to_string(),
                "--no-document".to_string(),
                "--install-dir".to_string(),
                gems.to_string_lossy().into_owned(),
                "--bindir".to_string(),
                bindir.to_string_lossy().into_owned(),
                "--version".to_string(),
                SOLARGRAPH_VERSION.to_string(),
                "solargraph".to_string(),
            ],
        ),
    )]
}

/// Strategies for installing the Python language server, from most isolated to
/// least. The virtualenv attempt bootstraps its own `pip`, so it works even
/// when the system Python has no `pip` module or is externally managed.
fn python_attempts() -> Vec<InstallAttempt> {
    python_package_attempts(PYTHON_LSP_SPEC)
}

/// Install the pinned Python debug adapter. It shares the same isolated
/// environments as the language server: `pipx`, `uv`, Kemori's virtualenv, or
/// a `--user` install.
fn debugpy_attempts() -> Vec<InstallAttempt> {
    python_package_attempts(DEBUGPY_SPEC)
}

fn python_package_attempts(spec: &str) -> Vec<InstallAttempt> {
    let mut attempts = vec![
        InstallAttempt::one("pipx", InstallCommand::new("pipx", &["install", spec])),
        InstallAttempt::one(
            "uv",
            InstallCommand::new("uv", &["tool", "install", spec]),
        ),
    ];

    if let Some(venv) = venv_dir() {
        let dir = venv.to_string_lossy().into_owned();
        let pip = venv_program(&venv, "pip").to_string_lossy().into_owned();
        for python in ["python3", "python"] {
            attempts.push(InstallAttempt::sequence(
                "a Kemori-managed virtualenv",
                vec![
                    InstallCommand::with_args(
                        python,
                        vec!["-m".into(), "venv".into(), dir.clone()],
                    ),
                    InstallCommand::with_args(
                        pip.clone(),
                        vec!["install".into(), spec.to_string()],
                    ),
                ],
            ));
        }
    }

    for python in ["python3", "python"] {
        // `ensurepip` seeds pip when the interpreter has none.
        attempts.push(InstallAttempt::sequence(
            "ensurepip",
            vec![
                InstallCommand::new(python, &["-m", "ensurepip", "--user"]),
                InstallCommand::new(python, &["-m", "pip", "install", "--user", spec]),
            ],
        ));
        attempts.push(InstallAttempt::one(
            "pip --user",
            InstallCommand::new(python, &["-m", "pip", "install", "--user", spec]),
        ));
    }
    attempts.push(InstallAttempt::one(
        "pip3 --user",
        InstallCommand::new("pip3", &["install", "--user", spec]),
    ));

    attempts
}

/// Strategies for installing an npm package into a user-local prefix.
///
/// The first two use whatever `npm` the user already has; the last provisions a
/// Kemori-managed Node.js runtime (with its bundled npm) so the install still
/// succeeds on a machine with no Node.js at all.
fn npm_attempts(packages: &[&str]) -> Vec<InstallAttempt> {
    let mut attempts = Vec::new();
    if let Some(prefix) = npm_prefix() {
        // A user-local prefix and cache: `npm install -g` into a root-owned
        // prefix, or a `~/.npm` left root-owned by a past `sudo npm`, would
        // otherwise fail with EACCES.
        let cache = prefix.with_file_name("npm-cache");
        attempts.push(InstallAttempt::one(
            "npm (user-local prefix)",
            InstallCommand::with_args("npm", npm_args(Some((&prefix, &cache)), packages)),
        ));
    }
    // Fall back to whatever global prefix the user's npm (nvm, fnm, volta, …)
    // already uses.
    attempts.push(InstallAttempt::one(
        "npm",
        InstallCommand::with_args("npm", npm_args(None, packages)),
    ));

    // No Node.js at all: install one under Kemori's data directory and use its
    // bundled npm, so zero-configuration still holds.
    if node_provisionable()
        && let (Some(prefix), Some(archive), Some(dest), Some(npm)) =
            (npm_prefix(), node_archive(), node_dir(), managed_npm())
    {
        let cache = prefix.with_file_name("npm-cache");
        attempts.push(InstallAttempt::managed(
            "a Kemori-managed Node.js runtime",
            vec![
                InstallStep::NodeRuntime {
                    dest: archive.clone(),
                },
                InstallStep::Extract {
                    archive,
                    dest,
                    strip: 1,
                },
                InstallStep::Run(InstallCommand::with_args(
                    npm.to_string_lossy().into_owned(),
                    npm_args(Some((&prefix, &cache)), packages),
                )),
            ],
        ));
    }
    attempts
}

/// Strategies for installing the official js-debug DAP bundle.
///
/// The bundle is a small verified GitHub release asset. It is a Node.js program,
/// so Kemori also provisions its managed Node.js runtime when the user has none,
/// then writes a launcher that runs it through `node`.
fn js_debug_attempts() -> Vec<InstallAttempt> {
    let (Some(downloads), Some(dest), Some(launcher)) =
        (downloads_dir(), js_debug_dir(), js_debug_launcher())
    else {
        return Vec::new();
    };
    let asset = format!("js-debug-dap-v{JS_DEBUG_VERSION}.tar.gz");
    let archive = downloads.join(&asset);
    let script = dest.join("src/dapDebugServer.js");
    let mut steps = Vec::new();
    if locate("node").is_none() {
        if !node_provisionable() {
            return Vec::new();
        }
        let (Some(node_archive), Some(node_dest)) = (node_archive(), node_dir()) else {
            return Vec::new();
        };
        steps.push(InstallStep::NodeRuntime {
            dest: node_archive.clone(),
        });
        steps.push(InstallStep::Extract {
            archive: node_archive,
            dest: node_dest,
            strip: 1,
        });
    }
    steps.push(InstallStep::GithubRelease {
        repo: "microsoft/vscode-js-debug".to_string(),
        tag: format!("v{JS_DEBUG_VERSION}"),
        asset,
        dest: archive.clone(),
    });
    steps.push(InstallStep::Extract {
        archive,
        dest,
        strip: 1,
    });
    steps.push(InstallStep::WriteLauncher {
        path: launcher,
        contents: format!(
            "#!/bin/sh\nexec node \"{}\" \"$@\"\n",
            script.to_string_lossy()
        ),
    });
    vec![InstallAttempt::managed(
        "the official js-debug DAP release",
        steps,
    )]
}

/// The verified `netcoredbg` archive for this platform, if one exists.
fn netcoredbg_asset() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "netcoredbg-linux-amd64.tar.gz",
        ("linux", "aarch64") => "netcoredbg-linux-arm64.tar.gz",
        ("macos", "aarch64") => "netcoredbg-osx-arm64.zip",
        _ => return None,
    })
}

/// Strategies for installing the self-contained netcoredbg DAP adapter.
fn netcoredbg_attempts() -> Vec<InstallAttempt> {
    let Some(asset) = netcoredbg_asset() else {
        return Vec::new();
    };
    // The Linux archive links glibc/libstdc++, so it is not offered on musl.
    if libc_is_musl() {
        return Vec::new();
    }
    let (Some(downloads), Some(dest)) = (downloads_dir(), netcoredbg_dir()) else {
        return Vec::new();
    };
    let archive = downloads.join(asset);
    vec![InstallAttempt::managed(
        "the official netcoredbg release",
        vec![
            InstallStep::GithubRelease {
                repo: "Samsung/netcoredbg".to_string(),
                tag: NETCOREDBG_VERSION.to_string(),
                asset: asset.to_string(),
                dest: archive.clone(),
            },
            InstallStep::Extract {
                archive,
                dest,
                strip: 1,
            },
        ],
    )]
}

/// The `npm install -g` arguments for a package list, optionally targeting a
/// user-local prefix and cache (so a system-owned prefix cannot make it fail).
fn npm_args(prefix_and_cache: Option<(&Path, &Path)>, packages: &[&str]) -> Vec<String> {
    let mut args = vec!["install".to_string(), "-g".to_string()];
    if let Some((prefix, cache)) = prefix_and_cache {
        args.push("--prefix".to_string());
        args.push(prefix.to_string_lossy().into_owned());
        args.push("--cache".to_string());
        args.push(cache.to_string_lossy().into_owned());
    }
    args.extend(packages.iter().map(|package| (*package).to_string()));
    args
}
