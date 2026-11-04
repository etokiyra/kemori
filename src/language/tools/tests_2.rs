    #[test]
    fn perl_prefers_pls_and_installs_it_with_cpanm() {
        let position = |tool: Tool| Tool::ALL.iter().position(|t| *t == tool).unwrap();
        assert!(
            position(Tool::Pls) < position(Tool::PerlLs),
            "PLS must be the preferred Perl server"
        );
        assert_eq!(Tool::Pls.program(), "pls");
        assert!(Tool::Pls.server_args().is_empty());
        assert!(Tool::Pls.probe_as_server());

        let attempts = Tool::Pls.install_attempts();
        if locate("perl").is_none() || tools_dir().is_none() {
            assert!(attempts.is_empty());
            return;
        }
        let steps = &attempts.first().expect("a PLS plan").steps;
        assert!(
            steps
                .iter()
                .any(|step| matches!(step, InstallStep::Run(command)
                if command.program == "perl" && command.args.iter().any(|arg| arg == PLS_SPEC))),
            "the PLS plan must install PLS: {steps:?}"
        );
        // Both Perl servers share the isolated local::lib.
        if let Some(lib) = perl_local_lib_dir() {
            let env = launch_env(Tool::Pls);
            assert_eq!(
                env.iter()
                    .find(|(key, _)| key == "PERL5LIB")
                    .map(|(_, value)| value.clone()),
                Some(format!("{}/lib/perl5", lib.display()))
            );
        }
    }

    #[test]
    fn a_later_server_candidate_is_used_when_the_first_is_missing() {
        // Only `Perl::LanguageServer` is installed; PLS is not.
        let registry = ToolRegistry {
            statuses: vec![
                ToolStatus {
                    tool: Tool::Pls,
                    available: false,
                    version: None,
                    path: None,
                    error: None,
                },
                ToolStatus {
                    tool: Tool::PerlLs,
                    available: true,
                    version: None,
                    path: None,
                    error: None,
                },
            ],
        };
        assert_eq!(
            Tool::available_server(LanguageId::Perl, &registry),
            Some(Tool::PerlLs),
            "Kemori must fall back to another installed Perl server"
        );
    }

    #[test]
    fn elixir_formatting_uses_mix() {
        use crate::language::provider::{Capability, LanguageProvider};
        let provider = crate::language::elixir::ElixirProvider;
        assert!(
            provider.capabilities().contains(&Capability::Formatting),
            "Elixir exposes Formatting"
        );
        assert_eq!(provider.formatter(), Some("mix"));
    }

    #[test]
    fn swift_toolchain_resolves_or_is_explained() {
        match swift_toolchain() {
            Some(toolchain) => {
                assert!(!toolchain.tag.is_empty() && !toolchain.suffix.is_empty());
                assert_eq!(toolchain.native, swift_native_asset().is_some());
            }
            None => assert!(
                libc_is_musl()
                    || std::env::consts::OS != "linux"
                    || !matches!(std::env::consts::ARCH, "x86_64" | "aarch64")
            ),
        }
    }

    #[test]
    fn swift_compat_needs_system_libraries() {
        let dir = std::env::temp_dir().join(format!("kemori-swift-compat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        match swift_compat(&dir) {
            Ok(()) => {
                // Every required library was present and linked.
                assert!(dir.join("libxml2.so.2").exists());
            }
            Err(message) => {
                // The first library the compatibility layer cannot resolve is
                // named; a minimal system may lack any of them.
                assert!(
                    ["libxml2", "libncurses", "libpanel", "libform"]
                        .iter()
                        .any(|library| message.contains(library)),
                    "unexpected error: {message}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn swift_plan_is_gpg_verified() {
        let attempts = Tool::SwiftLs.install_attempts();
        let Some(toolchain) = swift_toolchain() else {
            assert!(attempts.is_empty());
            return;
        };
        if tools_dir().is_none() {
            assert!(attempts.is_empty());
            return;
        }
        let steps = &attempts.first().expect("a Swift plan").steps;
        let (url, signature_url, keys_url) = steps
            .iter()
            .find_map(|step| match step {
                InstallStep::DownloadGpg {
                    url,
                    signature_url,
                    keys_url,
                    ..
                } => Some((url.clone(), signature_url.clone(), keys_url.clone())),
                _ => None,
            })
            .expect("Swift must verify a GPG signature");
        assert!(url.ends_with(".tar.gz"), "unexpected URL: {url}");
        assert_eq!(signature_url, format!("{url}.sig"));
        assert!(
            keys_url.contains("swift.org"),
            "unexpected key URL: {keys_url}"
        );
        // A non-native distribution needs the compatibility layer.
        assert_eq!(
            steps
                .iter()
                .any(|step| matches!(step, InstallStep::SwiftCompat { .. })),
            !toolchain.native
        );
        assert!(
            steps
                .iter()
                .any(|step| matches!(step, InstallStep::Extract { strip: 1, .. })),
            "the toolchain archive root must be stripped: {steps:?}"
        );
    }

    #[test]
    fn elixir_plan_is_a_coordinated_toolchain() {
        let attempts = Tool::ElixirLs.install_attempts();
        if bob_platform().is_none() || tools_dir().is_none() {
            assert!(attempts.is_empty());
            return;
        }
        let steps = &attempts.first().expect("an Elixir plan").steps;
        assert!(
            steps.iter().any(|step| matches!(
                step,
                InstallStep::BobBuild {
                    package: BobPackage::Erlang,
                    ..
                }
            )),
            "an Erlang/OTP build is required: {steps:?}"
        );
        assert!(
            steps.iter().any(|step| matches!(
                step,
                InstallStep::BobBuild {
                    package: BobPackage::Elixir,
                    ..
                }
            )),
            "an Elixir build is required: {steps:?}"
        );
        assert!(
            steps.iter().any(|step| matches!(
                step,
                InstallStep::GithubRelease { asset, .. } if asset.contains("elixir-ls")
            )),
            "the official ElixirLS release is required: {steps:?}"
        );
        // The server is built once with the managed Elixir and a private Mix.
        assert!(
            steps.iter().any(|step| matches!(
                step,
                InstallStep::Run(command)
                    if command.program == "elixir"
                        && command.env.iter().any(|(key, _)| key == "MIX_HOME")
            )),
            "ElixirLS must be built with a private MIX_HOME: {steps:?}"
        );
    }

    #[test]
    fn kotlin_uses_a_dedicated_jdk_21() {
        // The Kotlin plan must fetch JDK 21, while jdtls keeps JDK 25.
        let attempts = Tool::KotlinLs.install_attempts();
        let steps = &attempts.first().expect("a Kotlin plan").steps;
        assert!(
            steps.iter().any(|step| matches!(
                step,
                InstallStep::AdoptiumJdk {
                    release: ADOPTIUM_JDK21,
                    ..
                }
            )),
            "Kotlin must install a JDK 21: {steps:?}"
        );
        let jdtls = Tool::Jdtls.install_attempts();
        assert!(
            jdtls
                .first()
                .expect("a jdtls plan")
                .steps
                .iter()
                .any(|step| matches!(
                    step,
                    InstallStep::AdoptiumJdk {
                        release: ADOPTIUM_JDK25,
                        ..
                    }
                )),
            "jdtls must keep its JDK 25"
        );
        // They live in separate directories and the launcher points Kotlin at
        // its own JAVA_HOME.
        assert!(tools_dir().is_some());
        assert_ne!(kotlin_jdk_dir(), jdtls_dir());
        let env = launch_env(Tool::KotlinLs);
        let java_home = env
            .iter()
            .find(|(key, _)| key == "JAVA_HOME")
            .map(|(_, value)| value.clone())
            .expect("Kotlin launch sets JAVA_HOME");
        assert_eq!(Some(PathBuf::from(java_home)), kotlin_jdk_dir());
    }

    #[test]
    fn nix_profiles_are_part_of_tool_discovery() {
        let dirs = known_bin_dirs();
        assert!(dirs.contains(&PathBuf::from("/run/current-system/sw/bin")));
        assert!(dirs.contains(&PathBuf::from("/nix/var/nix/profiles/default/bin")));
        if let Some(tools) = tools_dir() {
            assert!(dirs.contains(&tools.join("nix-profile/bin")));
        }
    }

    #[test]
    fn nix_tools_target_the_pinned_nixpkgs_revision() {
        for (tool, package) in [(Tool::Nil, "nil"), (Tool::Nixfmt, "nixfmt")] {
            let attempts = tool.install_attempts();
            let attempt = attempts.first().expect("a nix install plan");
            let commands = run_commands(attempt);
            let command = commands.first().expect("a nix command");
            assert_eq!(command.program, "nix");
            let expected = format!("github:NixOS/nixpkgs/{NIXPKGS_REV}#{package}");
            assert!(
                command.args.iter().any(|arg| arg == &expected),
                "{tool:?} must install {expected}: {command:?}"
            );
            assert!(
                command
                    .args
                    .windows(2)
                    .any(|pair| { pair[0] == "--profile" && pair[1].ends_with("nix-profile") })
            );
        }
        assert_eq!(
            Tool::for_language(LanguageId::Nix, ToolPurpose::LanguageServer),
            Some(Tool::Nil)
        );
        assert_eq!(
            Tool::for_language(LanguageId::Nix, ToolPurpose::Formatter),
            Some(Tool::Nixfmt)
        );
    }

    #[test]
    fn formatters_map_to_languages_and_install_safely() {
        for (language, tool) in [
            (LanguageId::JavaScript, Tool::Prettier),
            (LanguageId::TypeScript, Tool::Prettier),
            (LanguageId::Html, Tool::Prettier),
            (LanguageId::Css, Tool::Prettier),
            (LanguageId::Json, Tool::Prettier),
            (LanguageId::Yaml, Tool::Prettier),
            (LanguageId::Markdown, Tool::Prettier),
            (LanguageId::C, Tool::ClangFormat),
            (LanguageId::Cpp, Tool::ClangFormat),
            (LanguageId::Shell, Tool::Shfmt),
            (LanguageId::Perl, Tool::PerlTidy),
            (LanguageId::Nix, Tool::Nixfmt),
        ] {
            assert_eq!(
                Tool::for_language(language, ToolPurpose::Formatter),
                Some(tool),
                "formatter for {language:?}"
            );
        }
        assert_eq!(
            Tool::Prettier.install_command(),
            Some(("npm", &["install", "-g", PRETTIER_SPEC][..]))
        );
        // `shfmt` is installed by Kemori's managed Go plan, not a package-manager
        // command.
        assert_eq!(Tool::Shfmt.install_command(), None);
    }

    #[test]
    fn make_executable_sets_the_exec_bit() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = std::env::temp_dir().join(format!("kemori-exec-{}", std::process::id()));
            let _ = std::fs::remove_file(&path);
            std::fs::write(&path, "#!/bin/sh\n").unwrap();
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
            make_executable(&path).expect("chmod");
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_ne!(mode & 0o100, 0, "the owner exec bit must be set");
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn unknown_program_is_not_located() {
        assert!(locate("kemori-definitely-not-a-real-tool").is_none());
    }

    #[test]
    fn extract_rejects_parent_paths() {
        if locate("tar").is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("kemori-traverse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("inner")).unwrap();
        std::fs::write(dir.join("inner/file.txt"), "x").unwrap();
        let built = std::process::Command::new("tar")
            .current_dir(dir.join("inner"))
            .args([
                "-cf",
                "../evil.tar",
                "--transform",
                "s,^,../escaped/,",
                "file.txt",
            ])
            .status();
        if !built.map(|status| status.success()).unwrap_or(false) {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let error = extract(&dir.join("evil.tar"), &dir.join("out"), 1).unwrap_err();
        assert!(
            error.contains("parent path"),
            "expected a traversal refusal, got: {error}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_accepts_a_normal_archive() {
        if locate("tar").is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("kemori-extract-{}", std::process::id()));
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
        extract(&dir.join("pkg.tar.gz"), &dir.join("out"), 1).expect("a safe archive extracts");
        assert!(dir.join("out/bin/tool").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disk_space_is_checked_when_measurable() {
        let dir = std::env::temp_dir();
        if free_disk_bytes(&dir).is_none() {
            return; // `df` unavailable; the check degrades to a no-op.
        }
        ensure_disk_space(&dir, 0).expect("zero bytes always fits");
        let error = ensure_disk_space(&dir, u64::MAX).expect_err("an impossible request must fail");
        assert!(error.contains("free disk space"), "{error}");
    }

    #[test]
    fn managed_paths_and_sizes_are_recognised() {
        let Some(tools) = tools_dir() else {
            return;
        };
        assert!(is_managed_path(&tools.join("clangd/bin/clangd")));
        assert!(!is_managed_path(Path::new("/usr/bin/clangd")));
        assert_eq!(
            managed_component(&tools.join("clangd/bin/clangd")),
            Some(tools.join("clangd"))
        );
    }

    #[test]
    fn only_managed_installs_are_updateable_or_removable() {
        let Some(tools) = tools_dir() else {
            return;
        };
        let managed = ToolStatus {
            tool: Tool::Clangd,
            available: true,
            version: Some("clangd 23".to_string()),
            path: Some(tools.join("clangd/bin/clangd")),
            error: None,
        };
        assert!(managed.is_managed());
        assert_eq!(managed.managed_dir(), Some(tools.join("clangd")));
        assert_eq!(
            managed.managed_size(),
            directory_size(&tools.join("clangd"))
        );

        let external = ToolStatus {
            tool: Tool::Clangd,
            available: true,
            version: None,
            path: Some(PathBuf::from("/usr/bin/clangd")),
            error: None,
        };
        assert!(
            !external.is_managed(),
            "a system install must never be removed"
        );
        assert_eq!(external.managed_dir(), None);
    }

    #[test]
    fn directory_size_sums_files_recursively() {
        let dir = std::env::temp_dir().join(format!("kemori-size-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("x"), vec![0u8; 10]).unwrap();
        std::fs::write(dir.join("a/b/y"), vec![0u8; 32]).unwrap();
        assert_eq!(directory_size(&dir), Some(42));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn proxy_environment_is_selected_for_curl() {
        // No proxy configured: nothing is forwarded.
        assert_eq!(proxy_from_env(|_| None), None);
        // A configured proxy is selected (here from the lowercase variable).
        let env = |key: &str| (key == "https_proxy").then(|| "http://proxy:3128".to_string());
        assert_eq!(proxy_from_env(env).as_deref(), Some("http://proxy:3128"));
        // HTTPS_PROXY wins over HTTP_PROXY.
        let env = |key: &str| match key {
            "HTTPS_PROXY" => Some("http://secure:8080".to_string()),
            "HTTP_PROXY" => Some("http://plain:3128".to_string()),
            _ => None,
        };
        assert_eq!(proxy_from_env(env).as_deref(), Some("http://secure:8080"));
        // An empty value is not a proxy.
        let env = |key: &str| (key == "HTTPS_PROXY").then(String::new);
        assert_eq!(proxy_from_env(env), None);
    }

    fn cache_test_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kemori-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cache_hit_avoids_the_network() {
        let dir = cache_test_dir("cache-hit");
        let cache = dir.join("cache");
        let payload = dir.join("payload.tar.gz");
        std::fs::write(&payload, b"payload contents").unwrap();
        let Ok(sha) = file_sha256(&payload) else {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        };
        cache_store(&cache, &sha, &payload, HashKind::Sha256).unwrap();

        // A URL that cannot be fetched: only the verified cache can satisfy this.
        let dest = dir.join("download.tar.gz");
        download_verified(
            "file:///definitely/missing",
            &dest,
            &sha,
            HashKind::Sha256,
            Some(&cache),
        )
        .expect("a verified cache entry is used without the network");
        assert_eq!(std::fs::read(&dest).unwrap(), b"payload contents");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_miss_downloads_then_caches() {
        let dir = cache_test_dir("cache-miss");
        let cache = dir.join("cache");
        let source = dir.join("source");
        std::fs::write(&source, b"fresh contents").unwrap();
        let sha = file_sha256(&source).unwrap();
        let dest = dir.join("download");
        download_verified(
            &format!("file://{}", source.display()),
            &dest,
            &sha,
            HashKind::Sha256,
            Some(&cache),
        )
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"fresh contents");
        // The verified artifact is cached under its digest for next time.
        assert!(
            cache_lookup(&cache, &sha, HashKind::Sha256)
                .unwrap()
                .is_some()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_cache_entry_is_rejected_and_replaced() {
        let dir = cache_test_dir("cache-corrupt");
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let source = dir.join("source");
        std::fs::write(&source, b"real contents").unwrap();
        let sha = file_sha256(&source).unwrap();
        // A file at the right key with the wrong contents must not be trusted.
        std::fs::write(cache.join(cache_key(&sha, HashKind::Sha256)), b"tampered").unwrap();
        assert!(
            cache_lookup(&cache, &sha, HashKind::Sha256)
                .unwrap()
                .is_none()
        );

        let dest = dir.join("download");
        download_verified(
            &format!("file://{}", source.display()),
            &dest,
            &sha,
            HashKind::Sha256,
            Some(&cache),
        )
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"real contents");
        let cached = cache_lookup(&cache, &sha, HashKind::Sha256)
            .unwrap()
            .unwrap();
        assert!(verify_hash(&cached, &sha, HashKind::Sha256).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_mismatched_download_is_never_cached() {
        let dir = cache_test_dir("cache-mismatch");
        let cache = dir.join("cache");
        let source = dir.join("source");
        std::fs::write(&source, b"real contents").unwrap();
        let dest = dir.join("download");
        let wrong = "ab".repeat(32);
        let error = download_verified(
            &format!("file://{}", source.display()),
            &dest,
            &wrong,
            HashKind::Sha256,
            Some(&cache),
        )
        .unwrap_err();
        assert!(error.contains("checksum mismatch"), "{error}");
        assert!(!dest.exists());
        assert!(
            !cache.is_dir() || std::fs::read_dir(&cache).unwrap().next().is_none(),
            "an unverified file must never enter the cache"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_download_leaves_no_cache_entry() {
        let dir = cache_test_dir("cache-failed");
        let cache = dir.join("cache");
        let sha = "cd".repeat(32);
        let dest = dir.join("download");
        let error = download_verified(
            "file:///definitely/missing",
            &dest,
            &sha,
            HashKind::Sha256,
            Some(&cache),
        )
        .unwrap_err();
        assert!(
            error.contains("could not copy") || error.contains("download failed"),
            "{error}"
        );
        assert!(!dest.exists());
        assert!(
            !cache.is_dir() || std::fs::read_dir(&cache).unwrap().next().is_none(),
            "a failed download must not create a trusted cache entry"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tools_dir_writability_is_checked() {
        let dir = cache_test_dir("writable");
        ensure_tools_dir_writable(&dir).expect("a temp directory is writable");
        // A regular file where a directory is expected cannot be written into.
        let file = dir.join("not-a-dir");
        std::fs::write(&file, b"x").unwrap();
        assert!(ensure_tools_dir_writable(&file).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runnable_strategies_are_available_and_gate_installation() {
        for &tool in Tool::ALL {
            let runnable = filter_runnable(tool.install_attempts());
            for attempt in &runnable {
                assert!(
                    attempt.steps.iter().all(step_available),
                    "{tool:?} kept a strategy with an unavailable step"
                );
            }
            // If no strategy can run, Kemori must not claim the tool is installable.
            if runnable.is_empty() {
                assert!(!can_install(tool), "{tool:?} has no runnable strategy");
            }
        }
    }

    #[test]
    fn download_failure_hints_are_actionable() {
        assert!(download_failure_hint("could not resolve host: example.com").contains("DNS"));
        assert!(download_failure_hint("Failed to connect to proxy").contains("HTTPS_PROXY"));
        assert!(download_failure_hint("Operation timed out after 30s").contains("timed out"));
        assert!(download_failure_hint("SSL certificate problem").contains("TLS"));
        assert!(download_failure_hint("the requested URL returned error: 404").contains("retry"));
    }

    #[test]
    fn a_partial_plan_keeps_earlier_work_and_reports_the_failing_step() {
        let dir = cache_test_dir("partial-plan");
        let source = dir.join("source");
        std::fs::write(&source, b"good payload for the partial plan test").unwrap();
        let sha = file_sha256(&source).unwrap();

        // Step A of a plan succeeds.
        let a = dir.join("a");
        run_step(&InstallStep::Download {
            url: format!("file://{}", source.display()),
            dest: a.clone(),
            sha256: sha.clone(),
        })
        .expect("the first step succeeds");
        assert!(a.is_file());

        // Step B fails; the failure is explicit and earlier work remains. Kemori
        // reports the incomplete plan rather than a false success.
        let b = dir.join("b");
        let error = run_step(&InstallStep::Download {
            url: "file:///definitely/missing".to_string(),
            dest: b.clone(),
            sha256: "cd".repeat(32),
        })
        .expect_err("the second step fails");
        assert!(!error.is_empty());
        assert!(!b.exists());
        assert!(a.is_file(), "earlier work in an incomplete plan remains");

        // Do not leave this test's payload in the real digest cache.
        if let Some(cache) = cache_dir() {
            let _ = std::fs::remove_file(cache.join(cache_key(&sha, HashKind::Sha256)));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn metadata_cache_round_trips_and_is_url_keyed() {
        let url = "https://example.invalid/kemori-metadata-test";
        remember_metadata(url, "{\"body\":1}").unwrap();
        assert_eq!(cached_metadata(url).as_deref(), Some("{\"body\":1}"));
        assert_eq!(cached_metadata("https://example.invalid/other"), None);
        if let Some(path) = metadata_cache_path(url) {
            let _ = std::fs::remove_file(path);
        }
    }

