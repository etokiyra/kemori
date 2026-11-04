/// Environment a tool needs to run, pointing it at Kemori-managed runtimes and
/// prepending their bin directories to `PATH` so child processes find them.
pub fn launch_env(tool: Tool) -> Vec<(String, String)> {
    let mut env = Vec::new();
    match tool {
        Tool::OmniSharp => {
            if let Some(dir) = dotnet_dir() {
                let dir = dir.to_string_lossy().into_owned();
                env.push(("DOTNET_ROOT".to_string(), dir.clone()));
                env.push(("DOTNET_ROOT_X64".to_string(), dir.clone()));
                env.push(("PATH".to_string(), prepend_path(&dir)));
            }
        }
        Tool::Jdtls => {
            if let Some(jdk) = jdk_dir() {
                let jdk = jdk.to_string_lossy().into_owned();
                env.push(("JAVA_HOME".to_string(), jdk.clone()));
                env.push(("PATH".to_string(), prepend_path(&format!("{jdk}/bin"))));
            }
        }
        // The Kotlin compiler bundled with the server cannot parse JDK 25's
        // four-part version, so the server runs on Kemori's dedicated JDK 21.
        Tool::KotlinLs => {
            if let Some(jdk) = kotlin_jdk_dir() {
                let jdk = jdk.to_string_lossy().into_owned();
                env.push(("JAVA_HOME".to_string(), jdk.clone()));
                env.push(("PATH".to_string(), prepend_path(&format!("{jdk}/bin"))));
            }
        }
        // npm-based servers are launched through `#!/usr/bin/env node`, so a
        // Kemori-managed Node.js must be on `PATH` when the user has none.
        Tool::BashLs
        | Tool::TypeScriptLs
        | Tool::HtmlLs
        | Tool::CssLs
        | Tool::JsDebug
        | Tool::PhpDebug => {
            if let Some(node) = node_bin_dir() {
                env.push(("PATH".to_string(), prepend_path(&node.to_string_lossy())));
            }
        }
        // A server installed into Kemori's isolated Perl local::lib needs that
        // library and its bin directory, and nothing from the system Perl.
        Tool::PerlLs | Tool::Pls => {
            if let Some(lib) = perl_local_lib_dir() {
                let lib = lib.to_string_lossy().into_owned();
                env.push(("PERL5LIB".to_string(), format!("{lib}/lib/perl5")));
                env.push(("PATH".to_string(), prepend_path(&format!("{lib}/bin"))));
            }
        }
        // ElixirLS is a shell launcher that runs `elixir`; when Kemori installed
        // it, it needs the managed OTP and Elixir runtimes on `PATH` and a
        // private Mix/Hex home so the user's `~/.mix` is never touched. A
        // system-provided ElixirLS is left on the user's own environment.
        Tool::ElixirLs => {
            if elixir_ls_dir().is_some_and(|dir| dir.is_dir()) {
                if let Some(dir) = elixir_ls_dir() {
                    env.push(("PATH".to_string(), prepend_path(&dir.to_string_lossy())));
                }
                env.extend(elixir_env());
            }
        }
        // The managed Swift toolchain's `sourcekit-lsp` finds its sibling
        // binaries through `usr/bin`, and its libraries through the toolchain's
        // own lib directory plus Kemori's compatibility directory (which aliases
        // the sonames a non-native distribution does not provide).
        Tool::SwiftLs => {
            if let Some(dir) = swift_dir() {
                env.push((
                    "PATH".to_string(),
                    prepend_path(&dir.join("usr/bin").to_string_lossy()),
                ));
                let mut libs: Vec<String> = Vec::new();
                if let Some(compat) = swift_compat_dir() {
                    libs.push(compat.to_string_lossy().into_owned());
                }
                libs.push(
                    dir.join("usr/lib/swift/linux")
                        .to_string_lossy()
                        .into_owned(),
                );
                env.push((
                    "LD_LIBRARY_PATH".to_string(),
                    prepend_colon(&libs.join(":")),
                ));
            }
        }
        // `solargraph` is installed into a Kemori-private gem home; its launcher
        // needs that home and its bin directory on the environment.
        Tool::RubyLs | Tool::Rdbg => {
            if let Some(gems) = gem_home() {
                let gems = gems.to_string_lossy().into_owned();
                env.push(("GEM_HOME".to_string(), gems.clone()));
                env.push(("GEM_PATH".to_string(), gems.clone()));
                env.push(("PATH".to_string(), prepend_path(&format!("{gems}/bin"))));
            }
        }
        // `phpactor`'s phar runs through `#!/usr/bin/env php`; put the managed
        // phpactor directory on `PATH` so a bare `phpactor` also resolves.
        Tool::Phpactor => {
            if let Some(dir) = phpactor_dir() {
                env.push(("PATH".to_string(), prepend_path(&dir.to_string_lossy())));
            }
        }
        // Go-installed tools are launched by their resolved path; a Kemori-managed
        // Go only needs its bin directories when a child process invokes `go`.
        Tool::Gopls | Tool::Sqls | Tool::Shfmt | Tool::Dlv => {
            if let Some(bin) = go_bin_dir() {
                env.push(("PATH".to_string(), prepend_path(&bin.to_string_lossy())));
            }
            if let Some(goroot) = go_dir().filter(|dir| dir.is_dir()) {
                env.push(("GOROOT".to_string(), goroot.to_string_lossy().into_owned()));
                env.push((
                    "PATH".to_string(),
                    prepend_path(&goroot.join("bin").to_string_lossy()),
                ));
            }
        }
        _ => {}
    }
    env
}

fn prepend_path(dir: &str) -> String {
    match std::env::var_os("PATH") {
        Some(existing) => format!("{dir}:{}", existing.to_string_lossy()),
        None => dir.to_string(),
    }
}

/// Prepend a colon-separated list of directories to `PATH`.
fn prepend_colon(dirs: &str) -> String {
    match std::env::var_os("PATH") {
        Some(existing) => format!("{dirs}:{}", existing.to_string_lossy()),
        None => dirs.to_string(),
    }
}

/// The environment the managed Erlang/Elixir runtime needs: its `bin`
/// directories on `PATH` and a private Mix/Hex home.
///
/// Only the directories Kemori actually installed are used, so a system Elixir is
/// left on the user's own configuration.
pub fn elixir_env() -> Vec<(String, String)> {
    let mut env = Vec::new();
    let mut bins: Vec<String> = Vec::new();
    if let Some(dir) = elixir_dir().filter(|dir| dir.is_dir()) {
        bins.push(dir.join("bin").to_string_lossy().into_owned());
    }
    if let Some(dir) = otp_dir().filter(|dir| dir.is_dir()) {
        bins.push(dir.join("bin").to_string_lossy().into_owned());
    }
    if !bins.is_empty() {
        env.push(("PATH".to_string(), prepend_colon(&bins.join(":"))));
    }
    if let Some(home) = mix_home().filter(|dir| dir.is_dir()) {
        let home = home.to_string_lossy().into_owned();
        env.push(("MIX_HOME".to_string(), home.clone()));
        env.push(("HEX_HOME".to_string(), home));
    }
    env
}

/// Whether this host uses musl rather than glibc.
///
/// musl systems (Alpine, Void musl) cannot run the glibc-targeted precompiled
/// runtimes, so Kemori reports them as unsupported instead of downloading a build
/// that fails.
pub fn libc_is_musl() -> bool {
    let alpine = std::path::Path::new("/etc/alpine-release").exists();
    let musl_loader = ["/lib/ld-musl-x86_64.so.1", "/lib/ld-musl-aarch64.so.1"]
        .iter()
        .any(|loader| std::path::Path::new(loader).exists());
    musl_from_markers(alpine, musl_loader)
}

/// The musl decision from its two markers, so the logic is unit-testable.
fn musl_from_markers(alpine_release: bool, musl_loader: bool) -> bool {
    alpine_release || musl_loader
}
