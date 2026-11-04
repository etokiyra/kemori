use super::*;

#[test]
fn every_tool_has_a_language_and_hint() {
    for &tool in Tool::ALL {
        assert!(!tool.program().is_empty());
        assert!(!tool.install_hint().is_empty());
        assert!(matches!(
            tool.language(),
            LanguageId::Rust
                | LanguageId::Go
                | LanguageId::Python
                | LanguageId::Shell
                | LanguageId::TypeScript
                | LanguageId::JavaScript
                | LanguageId::C
                | LanguageId::Java
                | LanguageId::CSharp
                | LanguageId::Php
                | LanguageId::Lua
                | LanguageId::Kotlin
                | LanguageId::Sql
                | LanguageId::Ruby
                | LanguageId::Assembly
                | LanguageId::Perl
                | LanguageId::Dart
                | LanguageId::Elixir
                | LanguageId::Swift
                | LanguageId::Html
                | LanguageId::Css
                | LanguageId::Nix
        ));
    }
}

/// The `Run` commands inside a strategy's steps.
fn run_commands(attempt: &InstallAttempt) -> Vec<&InstallCommand> {
    attempt
        .steps
        .iter()
        .filter_map(|step| match step {
            InstallStep::Run(command) => Some(command),
            _ => None,
        })
        .collect()
}

#[test]
fn managed_tools_have_download_plans() {
    // jdtls brings its own JDK; OmniSharp brings the .NET SDK it needs.
    if tools_dir().is_some() {
        let attempts = Tool::Jdtls.install_attempts();
        let steps = &attempts.first().expect("a jdtls plan").steps;
        assert!(steps.iter().any(|step| matches!(
            step,
            InstallStep::AdoptiumJdk {
                release: ADOPTIUM_JDK25,
                ..
            }
        )));
        assert!(steps
            .iter()
            .any(|step| matches!(step, InstallStep::Extract { .. })));
    }
    if omnisharp_asset().is_some() && tools_dir().is_some() {
        let attempts = Tool::OmniSharp.install_attempts();
        let steps = &attempts.first().expect("an OmniSharp plan").steps;
        assert!(steps
            .iter()
            .any(|step| matches!(step, InstallStep::DotnetSdk { .. })));
        assert!(steps
            .iter()
            .any(|step| matches!(step, InstallStep::GithubRelease { .. })));
        assert!(steps
            .iter()
            .any(|step| matches!(step, InstallStep::Extract { .. })));
    }
}

/// Package arguments of an `npm install` command, skipping flags and the
/// values of the flags that take one.
fn npm_packages(args: &[String]) -> Vec<&str> {
    let mut packages = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--prefix" || arg == "--cache" {
            i += 2;
            continue;
        }
        if arg == "install" || arg.starts_with('-') {
            i += 1;
            continue;
        }
        packages.push(arg);
        i += 1;
    }
    packages
}

/// Assert a `Run` step names an exact version for every package-manager
/// install. The remaining delegation Kemori cannot pin independently
/// (`rustup component add`) is intentionally not matched; Hex is pinned by
/// version, and rebar3 is a verified `Download` step.
fn assert_run_is_pinned(tool: Tool, command: &InstallCommand) {
    let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
    for arg in &args {
        assert!(
            !arg.contains("@latest") && *arg != "latest",
            "{tool:?} uses a floating install input: {command:?}"
        );
    }
    match command.program.as_str() {
        "go" => {
            if args.first() == Some(&"install") {
                for spec in &args[1..] {
                    assert!(
                        spec.contains('@'),
                        "{tool:?} must pin its go install input: {command:?}"
                    );
                }
            }
        }
        "npm" => {
            for package in npm_packages(&command.args) {
                assert!(
                    package.contains('@'),
                    "{tool:?} must pin its npm input: {command:?}"
                );
            }
        }
        "pipx" | "uv" | "pip" | "pip3" => {
            assert!(
                args.iter().any(|arg| arg.contains("==")),
                "{tool:?} must pin its pip input: {command:?}"
            );
        }
        "python" | "python3" => {
            if args.contains(&"pip") {
                assert!(
                    args.iter().any(|arg| arg.contains("==")),
                    "{tool:?} must pin its pip input: {command:?}"
                );
            }
        }
        "gem" => assert!(
            args.contains(&"--version"),
            "{tool:?} must pin its gem input: {command:?}"
        ),
        "cargo" => {
            if args.first() == Some(&"install") {
                assert!(
                    args.contains(&"--version"),
                    "{tool:?} must pin its cargo input: {command:?}"
                );
            }
        }
        "nix" => {
            // `github:NixOS/nixpkgs/<40-hex-commit>#<package>` is immutable
            // and Nix verifies every store path it builds.
            assert!(
                args.iter().any(|arg| {
                    arg.strip_prefix("github:NixOS/nixpkgs/")
                        .is_some_and(|rest| {
                            rest.split_once('#').is_some_and(|(rev, package)| {
                                rev.len() == 40
                                    && rev.chars().all(|c| c.is_ascii_hexdigit())
                                    && !package.is_empty()
                            })
                        })
                }),
                "{tool:?} must pin its nixpkgs revision: {command:?}"
            );
        }
        "perl" if args.iter().any(|arg| arg.ends_with("cpanm")) => {
            let spec = args.last().copied().unwrap_or("");
            assert!(
                spec.contains('@'),
                "{tool:?} must pin its cpan input: {command:?}"
            );
        }
        _ => {}
    }
}

/// Every managed install input is either exactly versioned or verified.
///
/// This iterates the real installer plans, so adding a new managed installer
/// without a pinned version or a digest fails here (and a plain `Download`
/// without a digest cannot even be constructed).
#[test]
fn every_managed_install_is_pinned_and_verified() {
    for &tool in Tool::ALL {
        for attempt in tool.install_attempts() {
            assert!(!attempt.via.is_empty(), "{tool:?} has an unnamed strategy");
            for step in &attempt.steps {
                match step {
                    InstallStep::Download { url, sha256, .. } => {
                        assert!(
                            !url.contains("latest"),
                            "{tool:?} downloads a floating URL: {url}"
                        );
                        assert!(
                            is_sha256_hex(sha256),
                            "{tool:?} has an invalid SHA-256 for {url}"
                        );
                    }
                    InstallStep::GithubRelease {
                        repo, tag, asset, ..
                    } => {
                        assert!(!repo.is_empty() && !asset.is_empty());
                        assert!(
                            !tag.is_empty() && tag != "latest",
                            "{tool:?} uses a floating GitHub tag: {tag}"
                        );
                    }
                    InstallStep::DownloadGpg {
                        url,
                        signature_url,
                        keys_url,
                        ..
                    } => {
                        assert!(!url.contains("latest"));
                        assert!(!signature_url.is_empty() && !keys_url.is_empty());
                    }
                    InstallStep::Run(command) => assert_run_is_pinned(tool, command),
                    // These verify internally (a checksum, signature or
                    // digest read from upstream metadata) or touch no
                    // download at all.
                    InstallStep::BobBuild { .. }
                    | InstallStep::AdoptiumJdk { .. }
                    | InstallStep::DotnetSdk { .. }
                    | InstallStep::NodeRuntime { .. }
                    | InstallStep::DartSdk { .. }
                    | InstallStep::RustupInit { .. }
                    | InstallStep::GoToolchain { .. }
                    | InstallStep::SwiftCompat { .. }
                    | InstallStep::MakeExecutable { .. }
                    | InstallStep::WriteLauncher { .. }
                    | InstallStep::Extract { .. } => {}
                }
            }
        }
    }
}

#[test]
fn lua_tool_is_self_contained_and_managed() {
    assert_eq!(
        Tool::for_language(LanguageId::Lua, ToolPurpose::LanguageServer),
        Some(Tool::LuaLs)
    );
    assert!(
        Tool::LuaLs.server_args().is_empty(),
        "lua-language-server speaks stdio with no arguments"
    );
    if lua_ls_asset().is_none() || tools_dir().is_none() {
        return; // No published archive for this platform.
    }
    let attempts = Tool::LuaLs.install_attempts();
    let steps = &attempts.first().expect("a Lua language server plan").steps;
    assert!(steps
        .iter()
        .any(|step| matches!(step, InstallStep::GithubRelease { .. })));
    assert!(steps
        .iter()
        .any(|step| matches!(step, InstallStep::Extract { strip: 0, .. })));
}

#[test]
fn new_language_tools_use_trusted_managers() {
    // Go tooling (`sqls`) is provisioned by Kemori's managed Go plan.
    assert_eq!(Tool::Sqls.install_command(), None);
    if go_platform().is_some() && tools_dir().is_some() {
        assert!(
            Tool::Sqls.install_attempts().iter().any(|attempt| attempt
                .steps
                .iter()
                .any(|step| matches!(step, InstallStep::GoToolchain { .. }))),
            "sqls must be able to provision Go"
        );
    }
    // `solargraph` installs into an isolated gem home.
    assert_eq!(Tool::RubyLs.install_command(), None);
    if locate("gem").is_some() && tools_dir().is_some() {
        assert!(
            Tool::RubyLs.install_attempts().iter().any(|attempt| attempt
                .steps
                .iter()
                .any(|step| matches!(step, InstallStep::Run(command)
                        if command.args.iter().any(|arg| arg == "--install-dir")))),
            "solargraph must use an isolated gem home"
        );
    }
    // `asm-lsp` uses the prebuilt release when one exists.
    assert_eq!(Tool::AsmLsp.install_command(), None);
    if asm_lsp_asset().is_some() && tools_dir().is_some() {
        assert!(
            Tool::AsmLsp
                .install_attempts()
                .iter()
                .any(|attempt| attempt.steps.iter().any(
                    |step| matches!(step, InstallStep::GithubRelease { repo, .. }
                        if repo == "bergercookie/asm-lsp")
                )),
            "asm-lsp should prefer the prebuilt release"
        );
    }
    // The multi-component toolchains are managed, so they advertise no
    // single package-manager command; whether they can be installed depends
    // on the platform and on the base tools being present.
    for tool in [
        Tool::ElixirLs,
        Tool::SwiftLs,
        Tool::PerlLs,
        Tool::DartAnalyzer,
        Tool::Clangd,
        Tool::Phpactor,
    ] {
        assert!(
            tool.install_command().is_none(),
            "{tool:?} must not advertise a package-manager command"
        );
    }
    let archives = locate("curl").is_some() && locate("tar").is_some();
    // Perl bootstraps cpanm; it needs a system perl plus the archive tools.
    assert_eq!(
        can_install(Tool::PerlLs),
        locate("perl").is_some() && archives
    );
    // Elixir is managed only where `bob` publishes builds. Its OTP runtime
    // is a tarball and the Elixir/ElixirLS archives are zips, so a zip
    // extractor is required too (any of unzip, bsdtar or Python).
    assert_eq!(
        can_install(Tool::ElixirLs),
        bob_platform().is_some() && archives && zip_extractor().is_some()
    );
    // The Dart SDK is managed as a zip, so Kemori can install it where
    // published and a zip extractor is available.
    assert_eq!(
        can_install(Tool::DartAnalyzer),
        dart_sdk_asset().is_some() && locate("curl").is_some() && zip_extractor().is_some()
    );
    // Swift is managed where swift.org publishes a signed toolchain (native
    // or the portable UBI10 build with its compatibility layer), and only
    // when the signature can be verified and every library is available.
    let swift_expected = match swift_toolchain() {
        None => false,
        Some(toolchain) => {
            locate("curl").is_some()
                && locate("gpg").is_some()
                && locate("tar").is_some()
                && (toolchain.native || swift_missing_libs().is_empty())
        }
    };
    assert_eq!(can_install(Tool::SwiftLs), swift_expected);
    assert_eq!(
        Tool::for_language(LanguageId::Dart, ToolPurpose::LanguageServer),
        Some(Tool::DartAnalyzer)
    );
    assert_eq!(
        Tool::for_language(LanguageId::Elixir, ToolPurpose::LanguageServer),
        Some(Tool::ElixirLs)
    );
    assert_eq!(
        Tool::for_language(LanguageId::Swift, ToolPurpose::LanguageServer),
        Some(Tool::SwiftLs)
    );
}

#[test]
fn perl_runs_as_a_perl_one_liner_with_an_isolated_lib() {
    assert_eq!(Tool::PerlLs.program(), "perl");
    assert!(
        Tool::PerlLs
            .server_args()
            .contains(&"-MPerl::LanguageServer"),
        "Perl is launched through the module: {:?}",
        Tool::PerlLs.server_args()
    );
    if let Some(lib) = perl_local_lib_dir() {
        let env = launch_env(Tool::PerlLs);
        let perl5lib = env
            .iter()
            .find(|(key, _)| key == "PERL5LIB")
            .map(|(_, value)| value.clone());
        assert_eq!(
            perl5lib,
            Some(format!("{}/lib/perl5", lib.display())),
            "the managed local::lib must be on PERL5LIB"
        );
    }
}

#[test]
fn elixir_has_alternate_launcher_names() {
    assert!(Tool::ElixirLs.candidates().contains(&"language_server.sh"));
    assert!(Tool::RustAnalyzer.candidates().is_empty());
    assert_eq!(Tool::ElixirLs.program(), "elixir-ls");
}

#[test]
fn dart_sdk_plan_is_checksum_verified() {
    let attempts = Tool::DartAnalyzer.install_attempts();
    if dart_sdk_asset().is_none() || tools_dir().is_none() {
        assert!(attempts.is_empty());
        return;
    }
    let steps = &attempts.first().expect("a Dart plan").steps;
    assert!(
        steps
            .iter()
            .any(|step| matches!(step, InstallStep::DartSdk { .. })),
        "Dart must download the SDK: {steps:?}"
    );
    assert!(
        steps
            .iter()
            .any(|step| matches!(step, InstallStep::Extract { strip: 1, .. })),
        "the SDK archive root must be stripped: {steps:?}"
    );
    // The sibling `.sha256sum` uses `<hash> *<name>`.
    let body = "ea864bc64df30a6b8bdf30b2e32550f7717d9a890de8f40293aeabb924fe232b *dartsdk-linux-x64-release.zip\n";
    assert_eq!(
        checksum_for(body, "dartsdk-linux-x64-release.zip").as_deref(),
        Some("ea864bc64df30a6b8bdf30b2e32550f7717d9a890de8f40293aeabb924fe232b")
    );
    assert_eq!(checksum_for(body, "missing.zip"), None);
}

#[test]
fn go_plan_provisions_the_official_toolchain() {
    let attempts = Tool::Gopls.install_attempts();
    assert!(!attempts.is_empty(), "gopls must have an install plan");
    if go_platform().is_none() || tools_dir().is_none() {
        return;
    }
    let managed = attempts
        .iter()
        .find(|attempt| {
            attempt
                .steps
                .iter()
                .any(|step| matches!(step, InstallStep::GoToolchain { .. }))
        })
        .expect("a managed Go attempt");
    assert!(
        managed
            .steps
            .iter()
            .any(|step| matches!(step, InstallStep::Extract { strip: 1, .. })),
        "the Go archive root must be stripped: {:?}",
        managed.steps
    );
    let run = managed
        .steps
        .iter()
        .find_map(|step| match step {
            InstallStep::Run(command) if command.program == "go" => Some(command),
            _ => None,
        })
        .expect("a go install run");
    assert!(
        run.args.iter().any(|arg| arg == GOPLS_SPEC),
        "the run must install pinned gopls: {:?}",
        run.args
    );
    // Tool installations stay inside Kemori's private GOPATH/GOBIN.
    if let Some(gobin) = go_bin_dir() {
        assert!(
            run.env.iter().any(
                |(key, value)| key == "GOBIN" && value == &gobin.to_string_lossy().into_owned()
            ),
            "GOBIN must point at Kemori's private bin directory: {:?}",
            run.env
        );
    }
}

#[test]
fn go_install_pins_goroot_only_when_managed_go_exists() {
    // Regression: a `GOROOT` pointing at a not-yet-downloaded managed Go
    // makes a good system `go` refuse to run, forcing a needless download.
    let command = go_install_command(&["golang.org/x/tools/gopls"]);
    let managed = go_dir().is_some_and(|dir| dir.join("bin/go").is_file());
    let has_goroot = command.env.iter().any(|(key, _)| key == "GOROOT");
    assert_eq!(has_goroot, managed);
    // Tool installs always stay inside Kemori's private GOBIN.
    assert!(command.env.iter().any(|(key, _)| key == "GOBIN"));
}

#[test]
fn phpactor_plan_is_a_verified_phar() {
    let attempts = Tool::Phpactor.install_attempts();
    if locate("php").is_none() || tools_dir().is_none() {
        assert!(attempts.is_empty());
        return;
    }
    let steps = &attempts.first().expect("a phpactor plan").steps;
    assert!(
        steps.iter().any(|step| matches!(
            step,
            InstallStep::GithubRelease { repo, asset, .. }
                if repo == "phpactor/phpactor" && asset == "phpactor.phar"
        )),
        "phpactor must come from the verified phar release: {steps:?}"
    );
    assert!(
        steps
            .iter()
            .any(|step| matches!(step, InstallStep::MakeExecutable { .. })),
        "the phar must be made executable: {steps:?}"
    );
}

#[test]
fn swift_platform_maps_supported_distributions() {
    assert_eq!(
        swift_platform("ubuntu", "", "24.04", "x86_64"),
        Some(("ubuntu2404", "ubuntu24.04"))
    );
    assert_eq!(
        swift_platform("debian", "", "12", "aarch64"),
        Some(("debian12", "debian12"))
    );
    // Derivatives are mapped through ID_LIKE.
    assert_eq!(
        swift_platform("pop", "ubuntu", "22.04", "x86_64"),
        Some(("ubuntu2204", "ubuntu22.04"))
    );
    // Unsupported distribution or architecture is reported, never guessed.
    assert_eq!(swift_platform("endeavouros", "arch", "", "x86_64"), None);
    assert_eq!(swift_platform("ubuntu", "", "24.04", "riscv64"), None);
}

#[test]
fn file_sha256_matches_a_known_digest() {
    let path = std::env::temp_dir().join(format!("kemori-sha-{}", std::process::id()));
    std::fs::write(&path, b"abc").unwrap();
    let hash = file_sha256(&path).expect("a SHA-256 tool");
    assert_eq!(
        hash.to_lowercase(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn sha256_hex_is_validated() {
    assert!(is_sha256_hex(&"a".repeat(64)));
    assert!(is_sha256_hex(&"A".repeat(64)));
    assert!(!is_sha256_hex("abc"));
    assert!(!is_sha256_hex(&"z".repeat(64)));
}

#[test]
fn musl_is_detected_from_either_marker() {
    assert!(musl_from_markers(true, false));
    assert!(musl_from_markers(false, true));
    assert!(!musl_from_markers(false, false));
}

#[test]
fn bob_platform_maps_supported_distributions() {
    assert_eq!(
        bob_platform_for("ubuntu", "", "24.04"),
        Some("ubuntu-24.04")
    );
    assert_eq!(
        bob_platform_for("linuxmint", "ubuntu debian", "22.04"),
        Some("ubuntu-22.04")
    );
    assert_eq!(bob_platform_for("endeavouros", "arch", ""), None);
}

#[test]
fn github_asset_digest_is_parsed() {
    let hash = "a".repeat(64);
    let body = format!(
        r#"{{"assets":[{{"name":"a.zip","digest":"sha256:{hash}"}},{{"name":"b.zip","digest":null}}]}}"#
    );
    assert_eq!(
        github_asset_sha256(&body, "a.zip").as_deref(),
        Some(&hash[..])
    );
    assert_eq!(github_asset_sha256(&body, "b.zip"), None);
    assert_eq!(github_asset_sha256(&body, "missing.zip"), None);
    assert_eq!(github_asset_sha256("not json", "a.zip"), None);
}

#[test]
fn builds_txt_checksum_reads_the_hex_column() {
    let hash = "b".repeat(64);
    let body = format!(
            "OTP-27.3.3 10e20b1dbe39b056fab430e50b08cb4f3696ae87 2025-04-16T14:41:39Z d45ab837970f6c40596285441432e968c2932544eeb0ae0cf792ea0b30a90923\nOTP-27.3.4 c388a2d1b3f9918652276d4798692dd4d8ef97fc 2025-05-09T18:03:06Z {hash}\nOTP-24.3 0863bd30aabd035c83158c78046c5ffda16127e1 2024-04-26T03:41:09Z\n"
        );
    assert_eq!(
        builds_txt_checksum(&body, "OTP-27.3.4").as_deref(),
        Some(&hash[..])
    );
    // An entry without a checksum column is not trusted.
    assert_eq!(builds_txt_checksum(&body, "OTP-24.3"), None);
    assert_eq!(builds_txt_checksum(&body, "OTP-99"), None);
}

#[test]
fn install_errors_prefer_the_concrete_failure() {
    let stderr = b"Building Coro-6.57 ... ! Installing Coro failed. See build.log\n! Installing the dependencies failed: Module 'Coro' is not installed\n! Bailing out\nFAIL\n";
    let message = last_stderr_line(stderr);
    assert!(message.contains("Coro"), "unexpected message: {message}");
    assert_eq!(last_stderr_line(b""), "installation failed");
}

#[test]
fn swift_toolchain_urls_match_swift_org() {
    let (url, signature) = swift_toolchain_urls("ubuntu2404", "ubuntu24.04");
    assert_eq!(
            url,
            "https://download.swift.org/swift-6.4.0-release/ubuntu2404/swift-6.4.0-RELEASE/swift-6.4.0-RELEASE-ubuntu24.04.tar.gz"
        );
    assert_eq!(signature, format!("{url}.sig"));
}

#[test]
fn every_tool_has_an_actionable_setup_reason() {
    for tool in Tool::ALL {
        assert!(
            !tool.setup_reason().is_empty(),
            "{tool:?} must explain how to set it up"
        );
    }
}

#[test]
fn human_bytes_is_readable() {
    assert_eq!(human_bytes(240_000_000), "240 MB");
    assert_eq!(human_bytes(1_150_000_000), "1.1 GB");
}

#[test]
fn otp_major_is_the_first_component() {
    assert_eq!(otp_major("27.3.4"), "27");
    assert_eq!(otp_major("29.1"), "29");
}

#[test]
fn elixir_pins_hex_and_rebar() {
    if bob_platform().is_none() {
        return;
    }
    let attempts = Tool::ElixirLs.install_attempts();
    let Some(attempt) = attempts.first() else {
        return;
    };
    let mut hex_pinned = false;
    let mut rebar_downloaded = false;
    let mut rebar_registered = false;
    for step in &attempt.steps {
        match step {
            InstallStep::Run(command) if command.program == "mix" => {
                if command.args.iter().any(|arg| arg == "local.hex") {
                    assert!(
                        command.args.iter().any(|arg| arg == HEX_VERSION),
                        "Hex must be pinned: {command:?}"
                    );
                    hex_pinned = true;
                }
                if command.args.iter().any(|arg| arg == "local.rebar") {
                    // rebar3 is registered from the verified local file, not
                    // fetched from the network by `mix`.
                    assert!(
                        command.args.iter().any(|arg| arg == "rebar3"),
                        "rebar must be registered from a local file: {command:?}"
                    );
                    rebar_registered = true;
                }
            }
            InstallStep::Download { url, sha256, .. }
                if url.contains("rebar3") && sha256.as_str() == REBAR3_SHA256 =>
            {
                rebar_downloaded = true;
            }
            _ => {}
        }
    }
    assert!(hex_pinned, "the Elixir plan must pin Hex");
    assert!(
        rebar_downloaded,
        "rebar3 must be downloaded and checksum-verified"
    );
    assert!(
        rebar_registered,
        "the verified rebar3 must be registered with Mix"
    );
}
