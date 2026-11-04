/// The OmniSharp release asset for this platform, if one exists.
fn omnisharp_asset() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "omnisharp-linux-x64.tar.gz",
        ("linux", "aarch64") => "omnisharp-linux-arm64.tar.gz",
        ("macos", "x86_64") => "omnisharp-osx-x64.tar.gz",
        ("macos", "aarch64") => "omnisharp-osx-arm64.tar.gz",
        _ => return None,
    })
}

/// Eclipse JDT Language Server, installed together with a managed JDK.
///
/// `jdtls` tracks the newest snapshot and follows the JDK it requires; the
/// managed JDK is the Adoptium build the Adoptium API reports as latest for
/// that feature release, so the pair stays consistent.
fn jdtls_attempts() -> Vec<InstallAttempt> {
    let jdtls_url = format!(
        "https://download.eclipse.org/jdtls/milestones/{JDTLS_VERSION}/\
         jdt-language-server-{JDTLS_SNAPSHOT}.tar.gz"
    );
    let (Some(downloads), Some(dest)) = (downloads_dir(), jdtls_dir()) else {
        return Vec::new();
    };
    let jdk_archive = downloads.join("temurin.tar.gz");
    let jdtls_archive = downloads.join("jdtls.tar.gz");
    let jdk = match tools_dir() {
        Some(tools) => tools.join("jdk"),
        None => return Vec::new(),
    };
    vec![InstallAttempt::managed(
        "a Kemori-managed JDK and Eclipse JDT",
        vec![
            InstallStep::AdoptiumJdk {
                release: ADOPTIUM_JDK25,
                dest: jdk_archive.clone(),
            },
            InstallStep::Extract {
                archive: jdk_archive,
                dest: jdk,
                strip: 1,
            },
            InstallStep::Download {
                url: jdtls_url,
                dest: jdtls_archive.clone(),
                sha256: JDTLS_SHA256.to_string(),
            },
            InstallStep::Extract {
                archive: jdtls_archive,
                dest,
                strip: 0,
            },
        ],
    )]
}

/// A self-contained `lua-language-server` release for this platform, or `None`
/// when the project publishes none.
fn lua_ls_asset() -> Option<String> {
    let (triple, extension) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => ("linux-x64", "tar.gz"),
        ("linux", "aarch64") => ("linux-arm64", "tar.gz"),
        ("macos", "x86_64") => ("darwin-x64", "tar.gz"),
        ("macos", "aarch64") => ("darwin-arm64", "tar.gz"),
        ("windows", "x86_64") => ("win32-x64", "zip"),
        _ => return None,
    };
    Some(format!(
        "lua-language-server-{LUA_LS_VERSION}-{triple}.{extension}"
    ))
}

/// Download and unpack the self-contained Lua language server.
///
/// The archive extracts its `bin/`, `main.lua` and `script/` at the top level,
/// so `tools/lua-language-server/bin/lua-language-server` is the launcher. It
/// needs no runtime, which is why Kemori can provision it unconditionally.
fn lua_ls_attempts() -> Vec<InstallAttempt> {
    let Some(asset) = lua_ls_asset() else {
        return Vec::new();
    };
    let (Some(downloads), Some(dest)) = (downloads_dir(), lua_ls_dir()) else {
        return Vec::new();
    };
    let archive = downloads.join(&asset);
    vec![InstallAttempt::managed(
        "a self-contained lua-language-server",
        vec![
            InstallStep::GithubRelease {
                repo: "LuaLS/lua-language-server".to_string(),
                tag: LUA_LS_VERSION.to_string(),
                asset,
                dest: archive.clone(),
            },
            InstallStep::Extract {
                archive,
                dest,
                strip: 0,
            },
        ],
    )]
}

/// A dedicated JDK 21 and `kotlin-language-server`, both managed by Kemori.
///
/// The server's bundled Kotlin compiler rejects the four-part version string of
/// the JDK 25 that `jdtls` uses, so Kemori provisions its own JDK 21 in an
/// isolated directory instead of downgrading anything the user has.
fn kotlin_ls_attempts() -> Vec<InstallAttempt> {
    let (Some(downloads), Some(dest)) = (downloads_dir(), kotlin_ls_dir()) else {
        return Vec::new();
    };
    let Some(jdk) = kotlin_jdk_dir() else {
        return Vec::new();
    };
    let jdk_archive = downloads.join("temurin21.tar.gz");
    let ls_archive = downloads.join("kotlin-language-server.zip");
    let url = format!(
        "https://github.com/fwcd/kotlin-language-server/releases/download/{KOTLIN_LS_VERSION}/server.zip"
    );
    vec![InstallAttempt::managed(
        "a managed JDK 21 and kotlin-language-server",
        vec![
            InstallStep::AdoptiumJdk {
                release: ADOPTIUM_JDK21,
                dest: jdk_archive.clone(),
            },
            InstallStep::Extract {
                archive: jdk_archive,
                dest: jdk,
                strip: 1,
            },
            InstallStep::Download {
                url,
                dest: ls_archive.clone(),
                sha256: KOTLIN_LS_SHA256.to_string(),
            },
            // The distribution's archive root is `server/`.
            InstallStep::Extract {
                archive: ls_archive,
                dest,
                strip: 1,
            },
        ],
    )]
}

/// Install `Perl::LanguageServer` into an isolated `local::lib`.
fn perl_attempts() -> Vec<InstallAttempt> {
    perl_module_attempts(PERL_LS_SPEC)
}

/// Install `PLS` into an isolated `local::lib`.
///
/// PLS is preferred over `Perl::LanguageServer` because it has no `Coro`
/// dependency, so it builds and runs on current Perls.
fn pls_attempts() -> Vec<InstallAttempt> {
    perl_module_attempts(PLS_SPEC)
}

/// Bootstrap `cpanm` and install `package` into an isolated `local::lib`.
///
/// Kemori bootstraps `cpanm` from a checksum-verified `App::cpanminus` archive —
/// so an interactive, unconfigured `cpan` is never run — and uses it with
/// `--local-lib`, which never touches the system Perl. A system `perl` is still
/// required to run `cpanm`; Kemori does not build a Perl runtime.
fn perl_module_attempts(package: &str) -> Vec<InstallAttempt> {
    if locate("perl").is_none() {
        return Vec::new();
    }
    let (Some(downloads), Some(lib), Some(cpanm)) =
        (downloads_dir(), perl_local_lib_dir(), cpanm_dir())
    else {
        return Vec::new();
    };
    let archive = downloads.join(format!("App-cpanminus-{CPANM_VERSION}.tar.gz"));
    let url = format!(
        "https://cpan.metacpan.org/authors/id/M/MI/MIYAGAWA/App-cpanminus-{CPANM_VERSION}.tar.gz"
    );
    let cpanm_script = cpanm.join("bin/cpanm");
    vec![InstallAttempt::managed(
        "a checksum-verified cpanm and an isolated local::lib",
        vec![
            InstallStep::Download {
                url,
                dest: archive.clone(),
                sha256: CPANM_SHA256.to_string(),
            },
            InstallStep::Extract {
                archive,
                dest: cpanm,
                strip: 1,
            },
            InstallStep::Run(
                InstallCommand::with_args(
                    "perl",
                    vec![
                        cpanm_script.to_string_lossy().into_owned(),
                        "--local-lib".to_string(),
                        lib.to_string_lossy().into_owned(),
                        "--notest".to_string(),
                        package.to_string(),
                    ],
                )
                // Some Makefile.PLs prompt; take the default answer instead of
                // hanging a background install.
                .env("PERL_MM_USE_DEFAULT", "1"),
            ),
        ],
    )]
}

/// The Swift toolchain archive URL and its detached signature URL.
fn swift_toolchain_urls(tag: &str, suffix: &str) -> (String, String) {
    let asset = format!("swift-{SWIFT_VERSION}-RELEASE-{suffix}.tar.gz");
    let url = format!(
        "https://download.swift.org/swift-{SWIFT_VERSION}-release/{tag}/swift-{SWIFT_VERSION}-RELEASE/{asset}"
    );
    (url.clone(), format!("{url}.sig"))
}

/// A GPG-verified Swift toolchain from swift.org.
///
/// swift.org publishes one toolchain per distribution release and signs it with
/// a detached signature, so Kemori verifies the signature against swift.org's
/// published keys and extracts the `usr/` tree. A distribution swift.org does
/// not build for gets the portable UBI10 build plus a managed compatibility
/// layer, so the same verified artifact runs without touching the system. When
/// even that cannot run (a missing compatibility library, musl, a non-Linux
/// host) the attempt is empty and Kemori explains exactly why.
fn swift_attempts() -> Vec<InstallAttempt> {
    let Some(toolchain) = swift_toolchain() else {
        return Vec::new();
    };
    let (Some(downloads), Some(dest), Some(compat)) =
        (downloads_dir(), swift_dir(), swift_compat_dir())
    else {
        return Vec::new();
    };
    let (url, signature_url) = swift_toolchain_urls(toolchain.tag, toolchain.suffix);
    let archive = downloads.join(format!(
        "swift-{SWIFT_VERSION}-RELEASE-{}.tar.gz",
        toolchain.suffix
    ));
    let mut steps = vec![
        InstallStep::DownloadGpg {
            url,
            signature_url,
            keys_url: "https://www.swift.org/keys/all-keys.asc".to_string(),
            dest: archive.clone(),
        },
        InstallStep::Extract {
            archive,
            dest,
            strip: 1,
        },
    ];
    // A build for another distribution needs the compatibility layer; a native
    // build already links against this distribution's libraries.
    if !toolchain.native {
        steps.push(InstallStep::SwiftCompat { dest: compat });
    }
    vec![InstallAttempt::managed(
        "a GPG-verified Swift toolchain",
        steps,
    )]
}

/// A coordinated Erlang/OTP + Elixir + ElixirLS toolchain.
///
/// Erlang/OTP and Elixir come from `builds.hex.pm` (the Erlang Ecosystem
/// Foundation's build service, which also backs the official `setup-beam`
/// action) and are checksum-verified; the OTP tree gets its `Install -minimal`
/// pass so `erl`/`erlc` exist. ElixirLS is the official release, built once at
/// install time with the managed Mix so the first launch is fast. Everything is
/// isolated: a private `MIX_HOME`/`HEX_HOME`, and runtimes on a scoped `PATH`.
fn elixir_ls_attempts() -> Vec<InstallAttempt> {
    // Only offered where `bob` publishes Erlang/Elixir builds for this platform.
    if bob_platform().is_none() {
        return Vec::new();
    }
    let (Some(downloads), Some(otp), Some(elixir), Some(ls)) =
        (downloads_dir(), otp_dir(), elixir_dir(), elixir_ls_dir())
    else {
        return Vec::new();
    };
    let Some(mix) = mix_home() else {
        return Vec::new();
    };
    let otp_archive = downloads.join(format!("OTP-{OTP_VERSION}.tar.gz"));
    let elixir_archive = downloads.join(format!(
        "v{ELIXIR_VERSION}-otp-{}.zip",
        otp_major(OTP_VERSION)
    ));
    let ls_archive = downloads.join(format!("elixir-ls-v{ELIXIR_LS_VERSION}.zip"));
    let rebar_archive = downloads.join("rebar3");
    let installer = otp.join("Install");
    let quiet_install = ls.join("quiet_install.exs");
    vec![InstallAttempt::managed(
        "Erlang/OTP, Elixir and a built ElixirLS",
        vec![
            InstallStep::BobBuild {
                package: BobPackage::Erlang,
                dest: otp_archive.clone(),
            },
            InstallStep::Extract {
                archive: otp_archive,
                dest: otp.clone(),
                strip: 1,
            },
            // `bob` ships the Erlang source layout; `Install -minimal` creates
            // the `bin/erl` wrappers without compiling anything.
            InstallStep::Run(
                InstallCommand::with_args(
                    "sh",
                    vec![
                        installer.to_string_lossy().into_owned(),
                        "-minimal".to_string(),
                        otp.to_string_lossy().into_owned(),
                    ],
                )
                .cwd(otp.clone()),
            ),
            InstallStep::BobBuild {
                package: BobPackage::Elixir,
                dest: elixir_archive.clone(),
            },
            InstallStep::Extract {
                archive: elixir_archive,
                dest: elixir,
                strip: 0,
            },
            // Install Hex and rebar into the private Mix home so the build never
            // prompts and never writes to the user's `~/.mix`. Hex is pinned to
            // an exact version; rebar3 is a pinned, checksum-verified escript
            // registered from the local file rather than fetched by Mix.
            InstallStep::Run(
                InstallCommand::new("mix", &["local.hex", HEX_VERSION, "--force"])
                    .env("MIX_HOME", mix.to_string_lossy().into_owned()),
            ),
            InstallStep::Download {
                url: format!(
                    "https://github.com/erlang/rebar3/releases/download/{REBAR3_VERSION}/rebar3"
                ),
                dest: rebar_archive.clone(),
                sha256: REBAR3_SHA256.to_string(),
            },
            InstallStep::MakeExecutable {
                path: rebar_archive.clone(),
            },
            InstallStep::Run(
                InstallCommand::with_args(
                    "mix",
                    vec![
                        "local.rebar".into(),
                        "rebar3".into(),
                        rebar_archive.to_string_lossy().into_owned(),
                        "--force".into(),
                    ],
                )
                .env("MIX_HOME", mix.to_string_lossy().into_owned()),
            ),
            InstallStep::GithubRelease {
                repo: "elixir-lsp/elixir-ls".to_string(),
                tag: format!("v{ELIXIR_LS_VERSION}"),
                asset: format!("elixir-ls-v{ELIXIR_LS_VERSION}.zip"),
                dest: ls_archive.clone(),
            },
            InstallStep::Extract {
                archive: ls_archive,
                dest: ls.clone(),
                strip: 0,
            },
            // Build the release once, with the managed runtime and a private
            // Mix/Hex home, so launching the server never has to compile.
            InstallStep::Run(
                InstallCommand::new("elixir", &[quiet_install.to_string_lossy().as_ref()])
                    .cwd(ls.clone())
                    .env("MIX_HOME", mix.to_string_lossy().into_owned())
                    .env("HEX_HOME", mix.to_string_lossy().into_owned())
                    .env("MIX_ENV", "prod"),
            ),
        ],
    )]
}

/// A Kemori-managed Dart SDK for the analysis server (`dart language-server`).
///
/// The SDK archive extracts to a `dart-sdk/` root, so it is stripped by one
/// level. Only Dart is installed; Flutter is not downloaded.
fn dart_sdk_attempts() -> Vec<InstallAttempt> {
    let (Some(downloads), Some(dest)) = (downloads_dir(), dart_sdk_dir()) else {
        return Vec::new();
    };
    let Some(asset) = dart_sdk_asset() else {
        return Vec::new();
    };
    let archive = downloads.join(asset);
    vec![InstallAttempt::managed(
        "a managed Dart SDK",
        vec![
            InstallStep::DartSdk {
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

/// OmniSharp plus the .NET SDK it runs on, both managed by Kemori.
fn omnisharp_attempts() -> Vec<InstallAttempt> {
    let Some(asset) = omnisharp_asset() else {
        return Vec::new();
    };
    let (Some(dotnet), Some(downloads), Some(dest)) =
        (dotnet_dir(), downloads_dir(), omnisharp_dir())
    else {
        return Vec::new();
    };
    let dotnet_archive = downloads.join("dotnet-sdk.tar.gz");
    let archive = downloads.join("omnisharp.tar.gz");
    vec![InstallAttempt::managed(
        "the .NET SDK and OmniSharp",
        vec![
            InstallStep::DotnetSdk {
                dest: dotnet_archive.clone(),
            },
            InstallStep::Extract {
                archive: dotnet_archive,
                dest: dotnet,
                strip: 0,
            },
            InstallStep::GithubRelease {
                repo: "OmniSharp/omnisharp-roslyn".to_string(),
                tag: OMNISHARP_VERSION.to_string(),
                asset: asset.to_string(),
                dest: archive.clone(),
            },
            InstallStep::Extract {
                archive,
                dest,
                strip: 0,
            },
        ],
    )]
}

fn venv_bin_dir(venv: &Path) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts")
    } else {
        venv.join("bin")
    }
}

fn venv_program(venv: &Path, name: &str) -> PathBuf {
    if cfg!(windows) {
        venv_bin_dir(venv).join(format!("{name}.exe"))
    } else {
        venv_bin_dir(venv).join(name)
    }
}

/// Install the pinned `debug` gem (which provides `rdbg`) into Kemori's isolated
/// gem home. A system Ruby is still required.
fn rdbg_attempts() -> Vec<InstallAttempt> {
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
                RUBY_DEBUG_VERSION.to_string(),
                "debug".to_string(),
            ],
        ),
    )]
}

/// Strategies for installing the official PHP DAP bridge.
///
/// The bridge is the `vscode-php-debug` extension's standalone adapter. It is a
/// Node.js program, so managed Node.js is provisioned when the user has none.
/// Xdebug itself is a PHP-runtime prerequisite, not something Kemori downloads.
fn php_debug_attempts() -> Vec<InstallAttempt> {
    let (Some(downloads), Some(dest), Some(launcher)) =
        (downloads_dir(), php_debug_dir(), php_debug_launcher())
    else {
        return Vec::new();
    };
    let asset = format!("php-debug-{PHP_DEBUG_VERSION}.vsix");
    // A `.vsix` is a zip; save it with a `.zip` name so the shared extractor
    // recognizes it without a special case.
    let archive = downloads.join("php-debug.zip");
    let script = dest.join("out/phpDebug.js");
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
        repo: "xdebug/vscode-php-debug".to_string(),
        tag: format!("v{PHP_DEBUG_VERSION}"),
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
        "the official PHP DAP bridge",
        steps,
    )]
}

/// Install the pinned `java-debug` extension jar loaded into JDT LS.
fn java_debug_attempts() -> Vec<InstallAttempt> {
    let Some(dest) = java_debug_jar() else {
        return Vec::new();
    };
    let url = format!(
        "https://repo1.maven.org/maven2/com/microsoft/java/com.microsoft.java.debug.plugin/\
         {JAVA_DEBUG_VERSION}/com.microsoft.java.debug.plugin-{JAVA_DEBUG_VERSION}.jar"
    );
    vec![InstallAttempt::managed(
        "the pinned java-debug JDT extension",
        vec![InstallStep::Download {
            url,
            dest,
            sha256: JAVA_DEBUG_SHA256.to_string(),
        }],
    )]
}
