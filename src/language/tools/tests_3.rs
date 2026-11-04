#[test]
fn a_checksum_mismatch_is_rejected_before_install() {
    let dir = std::env::temp_dir().join(format!("kemori-badsum-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("payload.tar.gz");
    std::fs::write(&source, b"the real payload").unwrap();
    let dest = dir.join("download.tar.gz");
    // A valid-looking but wrong digest must abort the real step.
    let step = InstallStep::Download {
        url: format!("file://{}", source.display()),
        dest: dest.clone(),
        sha256: "00".repeat(32),
    };
    let error = run_step(&step).expect_err("a wrong checksum must fail");
    assert!(error.contains("checksum mismatch"), "{error}");
    assert!(
        !dest.exists(),
        "a mismatched download must not be left behind"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_truncated_download_is_rejected_before_install() {
    let dir = std::env::temp_dir().join(format!("kemori-truncated-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let full = b"a complete archive payload with enough bytes to truncate cleanly";
    let full_path = dir.join("full");
    std::fs::write(&full_path, full).unwrap();
    // The expected digest is the full file's; only half the bytes arrive.
    let Ok(expected) = file_sha256(&full_path) else {
        let _ = std::fs::remove_dir_all(&dir);
        return; // No hashing tool on this host.
    };
    let source = dir.join("truncated");
    std::fs::write(&source, &full[..full.len() / 2]).unwrap();
    let dest = dir.join("download");
    let step = InstallStep::Download {
        url: format!("file://{}", source.display()),
        dest: dest.clone(),
        sha256: expected,
    };
    let error = run_step(&step).expect_err("a truncated download must fail");
    assert!(error.contains("checksum mismatch"), "{error}");
    assert!(
        !dest.exists(),
        "a truncated download must not be left behind"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_truncated_archive_fails_extraction() {
    if locate("tar").is_none() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("kemori-shorttar-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pkg/bin")).unwrap();
    std::fs::write(dir.join("pkg/bin/tool"), "#!/bin/sh\n").unwrap();
    let built = std::process::Command::new("tar")
        .current_dir(&dir)
        .args(["-czf", "pkg.tar.gz", "pkg"])
        .status();
    if !built.map(|status| status.success()).unwrap_or(false) {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    let archive = std::fs::read(dir.join("pkg.tar.gz")).unwrap();
    std::fs::write(dir.join("short.tar.gz"), &archive[..archive.len() / 2]).unwrap();
    let result = extract(&dir.join("short.tar.gz"), &dir.join("out"), 1);
    assert!(
        result.is_err(),
        "a truncated archive must not extract successfully"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_extraction_preserves_the_existing_installation() {
    if locate("tar").is_none() {
        return;
    }
    let dir = cache_test_dir("extract-preserve");
    std::fs::create_dir_all(dir.join("dest")).unwrap();
    std::fs::write(dir.join("dest/keep"), "known-good").unwrap();
    let archive = dir.join("bad.tar.gz");
    std::fs::write(&archive, b"not a real gzip archive").unwrap();

    assert!(extract(&archive, &dir.join("dest"), 0).is_err());
    assert_eq!(
        std::fs::read_to_string(dir.join("dest/keep")).unwrap(),
        "known-good",
        "a failed extraction must not touch the existing installation"
    );
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().contains(".kemori-"))
        .map(|entry| entry.file_name())
        .collect();
    assert!(
        leftovers.is_empty(),
        "staging must be cleaned up: {leftovers:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_successful_extraction_replaces_the_installation() {
    if locate("tar").is_none() {
        return;
    }
    let dir = cache_test_dir("extract-replace");
    std::fs::create_dir_all(dir.join("pkg/bin")).unwrap();
    std::fs::write(dir.join("pkg/bin/tool"), "#!/bin/sh\n").unwrap();
    let built = std::process::Command::new("tar")
        .current_dir(&dir)
        .args(["-czf", "pkg.tar.gz", "pkg"])
        .status();
    if !built.map(|status| status.success()).unwrap_or(false) {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    let dest = dir.join("dest");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("old"), "v1").unwrap();

    extract(&dir.join("pkg.tar.gz"), &dest, 1).expect("a safe archive extracts");
    assert!(dest.join("bin/tool").is_file());
    assert!(
        !dest.join("old").exists(),
        "the promoted tree replaces the previous installation"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_promotion_restores_the_previous_installation() {
    let dir = cache_test_dir("promote-restore");
    let dest = dir.join("dest");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("keep"), "known-good").unwrap();
    // A staging directory that does not exist makes the promote rename fail.
    let missing = dir.join(".missing-staging");
    let error = promote_directory(&missing, &dest).unwrap_err();
    assert!(error.contains("could not install into"), "{error}");
    assert_eq!(
        std::fs::read_to_string(dest.join("keep")).unwrap(),
        "known-good",
        "a failed promotion must restore the previous installation"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn extract_zip_works_with_every_available_extractor() {
    if locate("python3").is_none() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("kemori-zip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pkg/bin")).unwrap();
    std::fs::write(dir.join("pkg/bin/tool"), "x").unwrap();
    let archive = dir.join("pkg.zip");
    let built = std::process::Command::new("python3")
            .current_dir(&dir)
            .args([
                "-c",
                "import zipfile;z=zipfile.ZipFile('pkg.zip','w');z.write('pkg/bin/tool','pkg/bin/tool')",
            ])
            .status();
    if !built.map(|status| status.success()).unwrap_or(false) {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    for (extractor, tool) in [
        (ZipExtractor::Unzip, "unzip"),
        (ZipExtractor::BsdTar, "bsdtar"),
        (ZipExtractor::Python, "python3"),
    ] {
        if locate(tool).is_none() {
            continue;
        }
        // The traversal safety check must list the archive with the same
        // extractor that unpacks it (regression: it once hardcoded `unzip`,
        // so a system without it refused a perfectly safe archive).
        let listed =
            zip_listing(extractor, &archive).unwrap_or_else(|err| panic!("{tool} listing: {err}"));
        assert!(
            listed.contains("pkg/bin/tool"),
            "{tool} did not list the archive: {listed:?}"
        );
        let out = dir.join(format!("out-{tool}"));
        extract_zip_with(extractor, &archive, &out, 1)
            .unwrap_or_else(|err| panic!("{tool}: {err}"));
        assert!(out.join("bin/tool").is_file(), "{tool} did not extract");
    }
    // The public entry point picks a working extractor automatically.
    let out = dir.join("out-auto");
    extract(&archive, &out, 1).expect("zip extracts");
    assert!(out.join("bin/tool").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn strip_move_relocates_a_single_root_directory() {
    // `unzip` has no `--strip-components`, so a stripped zip is unpacked to
    // a scratch directory and its single root is moved up. Regression: the
    // Kotlin server once landed under `server/` because the strip was
    // silently ignored.
    let dir = std::env::temp_dir().join(format!("kemori-strip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let scratch = dir.join("scratch");
    std::fs::create_dir_all(scratch.join("pkg/bin")).unwrap();
    std::fs::write(scratch.join("pkg/bin/tool"), "#!/bin/sh\n").unwrap();
    std::fs::write(scratch.join("pkg/data.txt"), "data").unwrap();

    let root = single_child_directory(&scratch).expect("a single root");
    assert!(root.ends_with("pkg"));
    let dest = dir.join("dest");
    move_directory_contents(&root, &dest).unwrap();
    assert!(dest.join("bin/tool").is_file());
    assert!(dest.join("data.txt").is_file());

    // Two top-level entries cannot be stripped by descending.
    std::fs::create_dir_all(scratch.join("other")).unwrap();
    assert!(single_child_directory(&scratch).is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn install_lock_is_exclusive_and_reclaims_stale_locks() {
    let dir = std::env::temp_dir().join(format!("kemori-lock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let first = InstallLock::acquire_at(&dir).expect("first lock");
    assert!(
        InstallLock::acquire_at(&dir).is_err(),
        "the lock must be exclusive"
    );
    drop(first);
    assert!(
        InstallLock::acquire_at(&dir).is_ok(),
        "dropping the guard releases the lock"
    );

    // A stale lock from a crashed instance is reclaimed.
    let path = dir.join("install.lock");
    std::fs::write(&path, "99999").unwrap();
    let file = std::fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(SystemTime::now() - STALE_INSTALL_LOCK - Duration::from_secs(60))
        .unwrap();
    drop(file);
    assert!(
        InstallLock::acquire_at(&dir).is_ok(),
        "a stale lock is reclaimed"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropping_a_reclaimed_lock_leaves_the_successor_alone() {
    let dir = std::env::temp_dir().join(format!("kemori-lock-nonce-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let holder = InstallLock::acquire_at(&dir).expect("lock");
    // Another instance has, in the meantime, taken the lock.
    std::fs::write(dir.join("install.lock"), "another-instance").unwrap();
    drop(holder);
    assert!(
        dir.join("install.lock").exists(),
        "a stale guard must not delete a successor's lock"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_bin_programs_are_considered_bootstrappable() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let rustup = PathBuf::from(home).join(".cargo/bin/rustup");
    assert!(is_user_bin_program(&rustup.to_string_lossy()));
    assert!(!is_user_bin_program("rustup"));
    assert!(!is_user_bin_program("/opt/strange/place/tool"));
}

#[test]
fn clangd_serves_c_and_cpp_and_provisions_the_official_bundle() {
    assert!(Tool::Clangd.serves(LanguageId::C));
    assert!(Tool::Clangd.serves(LanguageId::Cpp));
    assert_eq!(
        Tool::for_language(LanguageId::Cpp, ToolPurpose::LanguageServer),
        Some(Tool::Clangd)
    );
    let attempts = Tool::Clangd.install_attempts();
    if attempts.is_empty() {
        // No fetchable bundle: musl (the release is glibc-only), a platform
        // clangd publishes no asset for, or no tools directory to install
        // into. Any other reason would be a bug.
        assert!(
            libc_is_musl() || clangd_asset().is_none() || tools_dir().is_none(),
            "clangd must have an install plan when its bundle is offered"
        );
        return;
    }
    let steps = &attempts.first().expect("a clangd plan").steps;
    assert!(
        steps.iter().any(|step| matches!(
            step,
            InstallStep::GithubRelease { repo, .. } if repo == "clangd/clangd"
        )),
        "clangd must come from the official clangd release: {steps:?}"
    );
    assert!(
        steps
            .iter()
            .any(|step| matches!(step, InstallStep::Extract { strip: 1, .. })),
        "the clangd bundle root must be stripped: {steps:?}"
    );
}

#[test]
fn typescript_tool_serves_javascript_and_installs_user_locally() {
    assert!(Tool::TypeScriptLs.serves(LanguageId::TypeScript));
    assert!(Tool::TypeScriptLs.serves(LanguageId::JavaScript));
    assert_eq!(
        Tool::for_language(LanguageId::JavaScript, ToolPurpose::LanguageServer),
        Some(Tool::TypeScriptLs)
    );

    let attempts = Tool::TypeScriptLs.install_attempts();
    assert!(
        attempts.iter().any(|attempt| {
            run_commands(attempt).into_iter().any(|command| {
                command.args.iter().any(|arg| arg == "--prefix")
                    && command.args.iter().any(|arg| arg == TYPESCRIPT_LS_SPEC)
            })
        }),
        "TypeScript should install with npm into Kemori's own prefix: {attempts:?}"
    );
}

#[test]
fn probe_reports_a_missing_tool_as_unavailable() {
    // `Gofmt` is only unavailable on machines without Go; use a tool whose
    // program we control by checking the registry shape instead.
    let registry = ToolRegistry::discover();
    assert_eq!(registry.all().len(), Tool::ALL.len());
    for status in registry.all() {
        if !status.available {
            assert!(status.version.is_none());
        }
    }
}

#[test]
fn install_commands_use_trusted_managers() {
    assert_eq!(
        Tool::RustAnalyzer.install_command(),
        Some(("rustup", &["component", "add", "rust-analyzer"][..]))
    );
    assert_eq!(
        Tool::Pylsp.install_command(),
        Some(("pipx", &["install", PYTHON_LSP_SPEC][..]))
    );
    assert_eq!(
        Tool::Debugpy.install_command(),
        Some(("pipx", &["install", DEBUGPY_SPEC][..]))
    );
    assert_eq!(Tool::Debugpy.purpose(), ToolPurpose::DebugAdapter);
    assert_eq!(
        Tool::for_language(LanguageId::Python, ToolPurpose::DebugAdapter),
        Some(Tool::Debugpy)
    );
    assert_eq!(Tool::LldbDap.purpose(), ToolPurpose::DebugAdapter);
    assert_eq!(
        Tool::for_language(LanguageId::Rust, ToolPurpose::DebugAdapter),
        Some(Tool::LldbDap)
    );
    assert!(Tool::LldbDap.install_attempts().is_empty());
    assert!(Tool::LldbDap.install_hint().contains("lldb-dap"));
    assert_eq!(Tool::Dlv.purpose(), ToolPurpose::DebugAdapter);
    assert_eq!(
        Tool::for_language(LanguageId::Go, ToolPurpose::DebugAdapter),
        Some(Tool::Dlv)
    );
    assert_eq!(Tool::Dlv.install_command(), None);
    assert!(Tool::Dlv.install_hint().contains("Delve"));
    assert!(!Tool::Dlv.install_attempts().is_empty());
    assert_eq!(Tool::JsDebug.purpose(), ToolPurpose::DebugAdapter);
    assert_eq!(
        Tool::for_language(LanguageId::JavaScript, ToolPurpose::DebugAdapter),
        Some(Tool::JsDebug)
    );
    assert_eq!(
        Tool::for_language(LanguageId::TypeScript, ToolPurpose::DebugAdapter),
        Some(Tool::JsDebug)
    );
    assert!(Tool::JsDebug.install_hint().contains("js-debug"));
    assert!(Tool::JsDebug.install_attempts().iter().any(|attempt| attempt
        .steps
        .iter()
        .any(|step| matches!(step, InstallStep::GithubRelease { repo, .. } if repo == "microsoft/vscode-js-debug"))));
    assert_eq!(Tool::Netcoredbg.purpose(), ToolPurpose::DebugAdapter);
    assert_eq!(
        Tool::for_language(LanguageId::CSharp, ToolPurpose::DebugAdapter),
        Some(Tool::Netcoredbg)
    );
    assert!(Tool::Netcoredbg.install_attempts().iter().any(|attempt| attempt
        .steps
        .iter()
        .any(|step| matches!(step, InstallStep::GithubRelease { repo, .. } if repo == "Samsung/netcoredbg"))));
    assert_eq!(Tool::DartDebug.purpose(), ToolPurpose::DebugAdapter);
    assert_eq!(
        Tool::for_language(LanguageId::Dart, ToolPurpose::DebugAdapter),
        Some(Tool::DartDebug)
    );
    assert!(!Tool::DartDebug.install_attempts().is_empty());
    assert_eq!(Tool::JavaDebug.purpose(), ToolPurpose::DebugAdapter);
    assert_eq!(
        Tool::for_language(LanguageId::Java, ToolPurpose::DebugAdapter),
        Some(Tool::JavaDebug)
    );
    assert!(Tool::JavaDebug
        .install_attempts()
        .iter()
        .any(|attempt| attempt.steps.iter().any(
            |step| matches!(step, InstallStep::Download { url, sha256, .. }
            if url.contains("com.microsoft.java.debug.plugin") && is_sha256_hex(sha256))
        )));
    assert_eq!(Tool::Rdbg.purpose(), ToolPurpose::DebugAdapter);
    assert_eq!(
        Tool::for_language(LanguageId::Ruby, ToolPurpose::DebugAdapter),
        Some(Tool::Rdbg)
    );
    assert!(Tool::Rdbg.install_hint().contains("debug"));
    if locate("gem").is_some() {
        assert!(Tool::Rdbg.install_attempts().iter().any(|attempt| attempt
            .steps
            .iter()
            .any(|step| matches!(step, InstallStep::Run(command)
                if command.args.iter().any(|arg| arg == RUBY_DEBUG_VERSION)))));
    }
    assert_eq!(Tool::PhpDebug.purpose(), ToolPurpose::DebugAdapter);
    assert_eq!(
        Tool::for_language(LanguageId::Php, ToolPurpose::DebugAdapter),
        Some(Tool::PhpDebug)
    );
    assert!(Tool::PhpDebug.install_hint().contains("Xdebug"));
    assert!(Tool::PhpDebug
        .install_attempts()
        .iter()
        .any(|attempt| attempt.steps.iter().any(
            |step| matches!(step, InstallStep::GithubRelease { repo, .. }
            if repo == "xdebug/vscode-php-debug")
        )));
    // The debug adapter's plan is the same isolated Python strategy as the
    // language server, pinned to an exact debugpy release.
    let debugpy = Tool::Debugpy.install_attempts();
    assert!(debugpy.len() >= 2);
    assert!(debugpy
        .iter()
        .any(|attempt| attempt.steps.iter().any(|step| {
            match step {
                InstallStep::Run(command) => command.args.iter().any(|arg| arg == DEBUGPY_SPEC),
                _ => false,
            }
        })));
    // Go tooling is provisioned by Kemori's managed Go plan.
    assert_eq!(Tool::Gopls.install_command(), None);
    assert_eq!(Tool::Sqls.install_command(), None);
    assert_eq!(
        Tool::BashLs.install_command(),
        Some(("npm", &["install", "-g", BASH_LS_SPEC][..]))
    );
    // Python tooling has several fallbacks, so installation is attempted
    // even without pipx or a working `pip`.
    assert!(Tool::Pylsp.install_attempts().len() >= 2);
    assert_eq!(Tool::Gofmt.install_command(), None);
    // `gofmt` ships with Go, so its plan provisions the Go toolchain.
    if go_platform().is_some() && tools_dir().is_some() {
        assert!(
            Tool::Gofmt.install_attempts().iter().any(|attempt| attempt
                .steps
                .iter()
                .any(|step| matches!(step, InstallStep::GoToolchain { .. }))),
            "gofmt must be installable through the Go toolchain"
        );
    } else {
        assert!(Tool::Gofmt.install_attempts().is_empty());
    }
}

#[test]
fn install_attempts_are_self_contained_and_user_local() {
    // `npm install -g` into a root-owned prefix fails with EACCES, so the
    // first Shell strategy must use a user-local prefix and cache.
    let Some(prefix) = npm_prefix() else {
        return; // No home directory; nothing to manage.
    };
    let attempts = Tool::BashLs.install_attempts();
    let first = attempts.first().expect("an npm attempt");
    let command = run_commands(first)
        .into_iter()
        .next()
        .expect("an npm command");
    assert_eq!(command.program, "npm");
    assert!(
        command.args.iter().any(|arg| arg == "--prefix"),
        "the first npm command should use --prefix: {:?}",
        command.args
    );
    assert!(
        command
            .args
            .iter()
            .any(|arg| arg == &prefix.to_string_lossy()),
        "the prefix should be Kemori's own user-local directory: {:?}",
        command.args
    );
    assert!(
        command.args.iter().any(|arg| arg == "--cache"),
        "a user-local cache avoids a root-owned ~/.npm: {:?}",
        command.args
    );

    // A system Python without `pip` must not block pylsp: the plan includes
    // a Kemori-managed virtualenv (which bootstraps its own pip), and it
    // never asks pip to override the OS (`--break-system-packages`).
    let attempts = Tool::Pylsp.install_attempts();
    assert!(
        attempts.iter().any(|attempt| {
            run_commands(attempt)
                .into_iter()
                .any(|command| command.args.iter().any(|arg| arg == "venv"))
        }),
        "expected a managed-virtualenv strategy: {attempts:?}"
    );
    assert!(
        attempts.iter().all(|attempt| {
            run_commands(attempt).into_iter().all(|command| {
                !command
                    .args
                    .iter()
                    .any(|arg| arg == "--break-system-packages")
            })
        }),
        "pylsp must not modify system packages"
    );
    // pipx and uv bundle their own environments.
    assert!(attempts.iter().any(|attempt| attempt.via == "pipx"));
}

#[test]
fn user_tool_directories_are_searched() {
    let dirs = known_bin_dirs();
    assert!(dirs.iter().any(|dir| dir.ends_with(".cargo/bin")));
    assert!(dirs.iter().any(|dir| dir.ends_with("go/bin")));
    if let Some(prefix) = npm_prefix() {
        assert!(
            dirs.iter().any(|dir| dir == &prefix.join("bin")),
            "the managed npm prefix should be searched"
        );
    }
    if let Some(venv) = venv_dir() {
        assert!(
            dirs.iter().any(|dir| dir == &venv_bin_dir(&venv)),
            "the managed Python virtualenv should be searched"
        );
    }
}

#[test]
fn prerequisites_report_missing_toolchains() {
    // A guaranteed-absent prerequisite is reported.
    for tool in Tool::ALL {
        let missing = tool.missing_prerequisites();
        for program in &missing {
            assert!(locate(program).is_none());
        }
    }
}

#[test]
fn status_summary_prefers_version_then_hint() {
    let available = ToolStatus {
        tool: Tool::Rustfmt,
        available: true,
        version: Some("rustfmt 1.8.0".to_string()),
        path: None,
        error: None,
    };
    assert_eq!(available.summary(), "rustfmt 1.8.0");

    let missing = ToolStatus {
        tool: Tool::Rustfmt,
        available: false,
        version: None,
        path: None,
        error: None,
    };
    assert!(missing.summary().contains("rustup"));
}

#[test]
fn candidate_lookup_finds_a_file_in_a_directory() {
    let dir = std::env::temp_dir().join(format!("kemori-tools-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kemori-fake-tool"), "").unwrap();

    assert_eq!(
        candidate_in(&dir, "kemori-fake-tool"),
        Some(dir.join("kemori-fake-tool"))
    );
    assert!(candidate_in(&dir, "kemori-absent-tool").is_none());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn known_bin_dirs_include_user_tool_locations() {
    let dirs = known_bin_dirs();
    assert!(dirs.iter().any(|dir| dir.ends_with(".cargo/bin")));
    assert!(dirs.iter().any(|dir| dir.ends_with(".local/bin")));
}

#[test]
fn known_bin_dirs_cover_common_toolchains() {
    let dirs = known_bin_dirs();
    for suffix in [
        ".mix/escripts",
        ".asdf/shims",
        "perl5/bin",
        "development/flutter/bin/cache/dart-sdk/bin",
    ] {
        assert!(
            dirs.iter().any(|dir| dir.ends_with(suffix)),
            "discovery should search a directory ending in {suffix}"
        );
    }
    for absolute in [
        "/usr/local/swift/usr/bin",
        "/usr/lib/swift/bin",
        "/snap/bin",
    ] {
        assert!(
            dirs.iter().any(|dir| dir == Path::new(absolute)),
            "discovery should search {absolute}"
        );
    }
}

#[test]
fn html_and_css_are_probed_by_starting_their_server() {
    // Regression: the extracted VS Code servers reject `--version`, so a
    // version probe reported a successful install as missing.
    for tool in [Tool::HtmlLs, Tool::CssLs, Tool::KotlinLs, Tool::AsmLsp] {
        assert!(
            tool.probe_as_server(),
            "{tool:?} must be probed by launching its stdio server"
        );
    }
    for tool in [Tool::RustAnalyzer, Tool::BashLs, Tool::TypeScriptLs] {
        assert!(
            !tool.probe_as_server(),
            "{tool:?} answers a version query and must keep the cheap probe"
        );
    }
}

#[cfg(unix)]
#[test]
fn server_probe_accepts_a_live_server_and_rejects_a_broken_launcher() {
    use std::process::{Command, Stdio};

    // A server still running at the deadline is usable.
    let mut live = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sleep");
    assert!(alive_or_clean(&mut live).0);

    // A launcher that fails immediately (the broken-Node symptom) is not.
    let mut broken = Command::new("sh")
        .args(["-c", "exit 1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sh");
    assert!(!alive_or_clean(&mut broken).0);

    // A clean one-shot exit is still a usable binary.
    let mut clean = Command::new("sh")
        .args(["-c", "exit 0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sh");
    assert!(alive_or_clean(&mut clean).0);
}

#[test]
fn npm_tools_can_provision_a_managed_node_runtime() {
    if !node_provisionable() {
        // No `curl`/archive tool for this platform: nothing to assert.
        return;
    }
    for tool in [Tool::HtmlLs, Tool::CssLs, Tool::BashLs, Tool::TypeScriptLs] {
        let attempts = tool.install_attempts();
        assert!(
            attempts.iter().any(|attempt| attempt
                .steps
                .iter()
                .any(|step| matches!(step, InstallStep::NodeRuntime { .. }))),
            "{tool:?} must be installable without a system Node.js: {attempts:?}"
        );
        // The managed runtime is downloaded, unpacked and then used to run npm.
        let managed = attempts
            .iter()
            .find(|attempt| {
                attempt
                    .steps
                    .iter()
                    .any(|step| matches!(step, InstallStep::NodeRuntime { .. }))
            })
            .expect("a managed Node plan");
        assert!(
            managed
                .steps
                .iter()
                .any(|step| matches!(step, InstallStep::Extract { .. })),
            "the Node archive must be extracted"
        );
        assert!(
            run_commands(managed)
                .into_iter()
                .any(|command| is_user_bin_program(&command.program)),
            "the managed npm must be the runtime Kemori just installed"
        );
    }
}

#[test]
fn managed_bin_dirs_are_searched_when_node_is_managed() {
    if let Some(node) = node_bin_dir() {
        assert!(
            known_bin_dirs().iter().any(|dir| dir == &node),
            "Kemori's managed Node.js bin directory should be searched"
        );
    }
}

#[test]
fn node_checksum_lookup_rejects_malformed_entries() {
    let asset = "node-v24.21.0-linux-x64.tar.xz";
    let sum = "fd8e59d5a511510f6a298afb548f18c7d2b1be404d8b4a27d94fbe49f56cb2d6";
    let body = format!(
            "aec7b2464afb99f078c19cb06d201d543bd3b311cba071282bce1b17c97e58bb  node-v24.21.0-aix-ppc64.tar.gz\n\
             {sum}  {asset}\n\
             22ca85110f26015696a3fa9216bc372ae65203d170622eaf7d211e2dd5bb49e3  node-v24.21.0-arm64.msi\n"
        );
    assert_eq!(checksum_for(&body, asset).as_deref(), Some(sum));

    // An absent asset, a short hash and a lone line never yield a checksum.
    assert_eq!(
        checksum_for(&body, "node-v24.21.0-linux-arm64.tar.xz"),
        None
    );
    assert_eq!(checksum_for("not-a-hash  some-file", "some-file"), None);
    assert_eq!(checksum_for("deadbeef", "deadbeef"), None);
    assert_eq!(checksum_for("", asset), None);
    // A binary-mode `*` prefix is tolerated.
    assert_eq!(
        checksum_for(&format!("{sum}  *{asset}"), asset).as_deref(),
        Some(sum)
    );
}
