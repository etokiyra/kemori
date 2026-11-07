#[test]
fn project_replace_requires_confirmation_before_deleting_matches() {
    use crate::search::SearchMatch;

    let dir = temp_project("project-replace-confirm");
    let file = dir.join("src/main.rs");
    fs::write(&file, "needle\n").unwrap();
    let mut app = app_with_file(&file);
    app.execute_command(ids::PROJECT_SEARCH);
    if let Overlay::ProjectSearch(state) = &mut app.overlay {
        state.query = "needle".to_string();
        state.results = vec![SearchMatch {
            path: file.clone(),
            line: 0,
            col: 0,
            text: "needle".to_string(),
            count: 1,
            preview: Some(String::new()),
        }];
    }

    // An empty replacement is a delete: the first press asks, it does not act.
    app.handle_project_search_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    assert!(app.pending_project_replace.is_none());
    let Overlay::ProjectSearch(state) = &app.overlay else {
        panic!("expected the project search form");
    };
    assert!(state.confirm_empty);
    assert!(state
        .status
        .as_deref()
        .is_some_and(|status| status.contains("again")));
    assert_eq!(fs::read_to_string(&file).unwrap(), "needle\n");

    // Typing a replacement clears the pending confirmation.
    app.handle_project_search_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.handle_project_search_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    let Overlay::ProjectSearch(state) = &app.overlay else {
        panic!("expected the project search form");
    };
    assert!(!state.confirm_empty);
    assert_eq!(fs::read_to_string(&file).unwrap(), "needle\n");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn project_search_lists_matches_across_files() {
    let dir = temp_project("project-search");
    let a = dir.join("src/main.rs");
    let b = dir.join("src/lib.rs");
    fs::write(&a, "fn main() { needle(); }\n").unwrap();
    fs::write(&b, "pub fn needle() {}\n").unwrap();

    let mut app = app_with_file(&a);
    app.execute_command(ids::PROJECT_SEARCH);
    let Overlay::ProjectSearch(state) = &mut app.overlay else {
        panic!("expected the project search form");
    };
    assert!(state.query.is_empty());
    state.query = "needle".to_string();
    app.run_project_search();

    let deadline = Instant::now() + Duration::from_secs(15);
    while app.pending_project_search.is_some() && Instant::now() < deadline {
        app.apply_background_events();
        std::thread::sleep(Duration::from_millis(5));
    }
    let Overlay::ProjectSearch(state) = &app.overlay else {
        panic!("the search form should stay open");
    };
    assert_eq!(state.results.len(), 2, "expected both files to match");
    assert_eq!(state.file_count(), 2);
    assert!(state
        .status
        .as_deref()
        .is_some_and(|s| s.contains("2 match")));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn project_replace_updates_clean_files_and_skips_dirty_ones() {
    let dir = temp_project("project-replace");
    let clean = dir.join("src/main.rs");
    let dirty = dir.join("src/lib.rs");
    fs::write(&clean, "fn main() { needle(); }\n").unwrap();
    fs::write(&dirty, "pub fn needle() {}\n").unwrap();

    let mut app = app_with_file(&clean);
    app.open_path(dirty.clone());
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// unsaved\n");
    assert!(app.editor.active_document().unwrap().is_dirty());

    app.execute_command(ids::PROJECT_SEARCH);
    let Overlay::ProjectSearch(state) = &mut app.overlay else {
        panic!("expected the project search form");
    };
    state.query = "needle".to_string();
    state.replacement = "value".to_string();
    app.run_project_search();
    let deadline = Instant::now() + Duration::from_secs(15);
    while app.pending_project_search.is_some() && Instant::now() < deadline {
        app.apply_background_events();
        std::thread::sleep(Duration::from_millis(5));
    }

    // Alt+Enter starts the replacement; the dirty open buffer is reported and
    // left untouched.
    let replace = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);
    app.handle_project_search_key(replace);
    while app.pending_project_replace.is_some() && Instant::now() < deadline {
        app.apply_background_events();
        std::thread::sleep(Duration::from_millis(5));
    }

    assert_eq!(
        fs::read_to_string(&clean).unwrap(),
        "fn main() { value(); }\n"
    );
    let reloaded = app
        .editor
        .documents
        .iter()
        .find(|doc| doc.buffer.path.as_deref() == Some(clean.as_path()))
        .expect("clean file is open");
    assert!(
        reloaded.buffer.text().contains("value()"),
        "a clean open buffer should reload immediately"
    );
    assert_eq!(
        fs::read_to_string(&dirty).unwrap(),
        "pub fn needle() {}\n",
        "a dirty buffer must never be overwritten on disk"
    );
    let Overlay::ProjectSearch(state) = &app.overlay else {
        panic!("the search form should stay open");
    };
    assert!(
        state
            .status
            .as_deref()
            .is_some_and(|status| status.contains("skipped 1")),
        "the skipped file should be reported: {:?}",
        state.status
    );
    assert!(
        state
            .status
            .as_deref()
            .is_some_and(|status| status.contains("main.rs")),
        "every changed file should be named: {:?}",
        state.status
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn changed_files_lists_git_status() {
    let dir = temp_project("changed-files");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    let mut files = std::collections::HashMap::new();
    files.insert(file.clone(), GitFileStatus::Modified);
    let mut staged = std::collections::HashSet::new();
    staged.insert(file.clone());
    app.workspace.git = crate::git::GitInfo {
        repo_root: Some(dir.clone()),
        branch: Some("main".to_string()),
        files,
        staged,
        available: true,
    };

    app.execute_command(ids::CHANGED_FILES);
    let Overlay::Picker(picker) = &app.overlay else {
        panic!("expected the changed-files picker");
    };
    assert!(!picker.filtered.is_empty());
    let item = picker.item(0).expect("an item");
    assert!(matches!(item.action, PickerAction::OpenPath(_)));
    assert!(item.detail.contains('M'), "detail was {}", item.detail);
    assert!(
        item.detail.contains("modified · staged"),
        "detail was {}",
        item.detail
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn commit_flow_reports_state_and_prompts() {
    let dir = temp_project("commit");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    // Not a repository: the command explains instead of prompting.
    app.workspace.git = crate::git::GitInfo::default();
    app.execute_command(ids::GIT_COMMIT);
    assert!(matches!(app.overlay, Overlay::None));
    assert!(app
        .status_message()
        .unwrap_or("")
        .contains("Not a git repository"));

    // With changes, it opens a commit-message prompt and dispatches work.
    let mut files = std::collections::HashMap::new();
    files.insert(file.clone(), GitFileStatus::Modified);
    app.workspace.git = crate::git::GitInfo {
        repo_root: Some(dir.clone()),
        branch: Some("main".to_string()),
        files,
        staged: std::collections::HashSet::new(),
        available: true,
    };
    app.execute_command(ids::GIT_COMMIT);
    assert!(matches!(app.overlay, Overlay::Prompt(_)));

    app.submit_prompt(PromptKind::CommitMessage, "a message".to_string());
    assert!(app.pending_commit);
    assert_eq!(app.busy(), Some("committing"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn space_in_changed_files_stages_the_selected_file() {
    let dir = temp_project("stage-space");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    let mut files = std::collections::HashMap::new();
    files.insert(file.clone(), GitFileStatus::Modified);
    app.workspace.git = crate::git::GitInfo {
        repo_root: Some(dir.clone()),
        branch: Some("main".to_string()),
        files,
        staged: std::collections::HashSet::new(),
        available: true,
    };

    app.execute_command(ids::CHANGED_FILES);
    assert!(matches!(app.overlay, Overlay::Picker(_)));

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(app.pending_stage_refresh);
    assert!(
        app.status_message().unwrap_or("").contains("Staging"),
        "status was {:?}",
        app.status_message()
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn mouse_click_and_drag_map_to_editor_positions() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    let dir = temp_project("mouse");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.editor_body_rect = Some(ratatui::layout::Rect::new(0, 0, 80, 20));
    app.show_line_numbers = true;

    // Gutter is one digit plus two columns; screen row 1 is document line 0.
    let down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 2,
        modifiers: KeyModifiers::NONE,
    };
    assert!(app.handle_mouse(down));
    let doc = app.editor.active_document().unwrap();
    assert_eq!((doc.cursor.row, doc.cursor.col), (1, 2));
    assert_eq!(app.mouse_drag_anchor, Some(Position::new(1, 2)));

    let drag = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 3,
        row: 1,
        modifiers: KeyModifiers::NONE,
    };
    assert!(app.handle_mouse(drag));
    let doc = app.editor.active_document().unwrap();
    assert_eq!((doc.cursor.row, doc.cursor.col), (0, 0));
    assert_eq!(
        doc.selection.map(|sel| sel.anchor),
        Some(Position::new(1, 2))
    );

    let up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 3,
        row: 1,
        modifiers: KeyModifiers::NONE,
    };
    assert!(!app.handle_mouse(up));
    assert!(app.mouse_drag_anchor.is_none());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ctrl_k_chord_folds_and_unfolds() {
    let dir = temp_project("fold-chord");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert!(app.fold_prefix);
    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
    let doc = app.editor.active_document().unwrap();
    assert!(doc.folded.contains(&0));
    assert!(doc.is_hidden(1));

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let doc = app.editor.active_document().unwrap();
    assert!(doc.folded.is_empty());
    assert!(!doc.is_hidden(1));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn git_operations_are_queued_on_the_worker() {
    let dir = temp_project("git-queued");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.git_fetch();
    // Nothing has run yet: the UI only recorded the request.
    assert_eq!(app.pending_git_op, Some("Fetch"));
    assert!(
        app.status_message().unwrap_or("").contains("Fetching"),
        "status was {:?}",
        app.status_message()
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    while app.pending_git_op.is_some() && Instant::now() < deadline {
        app.pump_background(Duration::from_millis(50));
    }
    assert!(app.pending_git_op.is_none(), "fetch did not finish");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn diff_active_file_shows_working_tree_changes() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return; // Skip when git is unavailable.
    }
    let dir = temp_project("diff-file");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
    };
    assert!(git(&["init", "-q"]).unwrap().status.success());
    assert!(git(&["config", "user.email", "kemori@example.com"])
        .unwrap()
        .status
        .success());
    assert!(git(&["config", "user.name", "Kemori Test"])
        .unwrap()
        .status
        .success());
    assert!(git(&["add", "-A"]).unwrap().status.success());
    assert!(git(&["commit", "-q", "-m", "init"])
        .unwrap()
        .status
        .success());

    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !app.workspace.git.available && Instant::now() < deadline {
        app.pump_background(Duration::from_millis(50));
    }
    assert!(app.workspace.git.available, "expected a git repository");

    // Change the file on disk so the working tree differs from the index.
    fs::write(&file, "fn main() {\n    let y = 2;\n}\n").unwrap();
    app.diff_active_file();
    // The diff is computed on the worker; the UI thread only queues it.
    assert!(app.pending_git_diff.is_some());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.pending_git_diff.is_some() && Instant::now() < deadline {
        app.pump_background(Duration::from_millis(50));
    }
    assert!(app.pending_git_diff.is_none(), "diff did not finish");

    match &app.overlay {
        Overlay::Diff(diff) => assert!(
            diff.lines
                .iter()
                .any(|line| line.kind == overlay::DiffLineKind::Add && line.text.contains("let y")),
            "expected an added line, got {:?}",
            diff.lines.iter().map(|line| &line.text).collect::<Vec<_>>()
        ),
        _ => panic!("expected a diff overlay"),
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ctrl_shift_s_opens_save_as() {
    let dir = temp_project("save-as");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.handle_key(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert!(matches!(
        &app.overlay,
        Overlay::Prompt(prompt) if prompt.kind == PromptKind::SaveAs
    ));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ctrl_n_opens_the_new_file_prompt() {
    let dir = temp_project("ctrl-n");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));
    assert!(matches!(
        &app.overlay,
        Overlay::Prompt(prompt) if prompt.kind == PromptKind::NewFile
    ));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn alt_m_jumps_to_the_matching_bracket() {
    let dir = temp_project("alt-m");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {}\n").unwrap();
    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(0, 7));

    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::ALT));
    assert_eq!(
        app.editor.active_document().unwrap().clamped_cursor(),
        Position::new(0, 8)
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn f3_advances_to_the_next_match() {
    let dir = temp_project("f3");
    let file = dir.join("src/main.rs");
    fs::write(&file, "foo\nfoo\nfoo\n").unwrap();
    let mut app = app_with_file(&file);
    app.execute_command(ids::FIND);
    app.search.query = "foo".to_string();
    app.refresh_search_matches();
    app.jump_to_match(0);

    app.handle_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
    assert_eq!(app.search.current, Some(1));
    app.handle_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::SHIFT));
    assert_eq!(app.search.current, Some(0));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn toasts_deduplicate_and_cap() {
    let dir = temp_project("toasts");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.push_toast(ToastKind::Success, "Saved");
    app.push_toast(ToastKind::Success, "Saved");
    assert_eq!(app.toasts.len(), 1, "duplicate toasts are ignored");

    for index in 0..6 {
        app.push_toast(ToastKind::Info, format!("message {index}"));
    }
    assert_eq!(app.toasts.len(), 4, "toasts are capped");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn busy_reports_pending_background_work() {
    let dir = temp_project("busy");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    assert!(app.busy().is_none());

    app.pending_format = Some((file.clone(), 1));
    assert_eq!(app.busy(), Some("formatting"));

    app.pending_format = None;
    app.pending_workspace_symbols = Some(1);
    assert_eq!(app.busy(), Some("searching symbols"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn language_setup_lists_discovered_tools() {
    let dir = temp_project("setup");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    // Tool discovery happens on the worker; wait briefly for it.
    let deadline = Instant::now() + Duration::from_secs(15);
    while app.tools.is_none() && Instant::now() < deadline {
        app.apply_background_events();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app.tools.is_some(), "tool discovery should complete");

    app.execute_command(ids::SETUP);
    let Overlay::Picker(picker) = &app.overlay else {
        panic!("expected the language setup list");
    };
    assert!(picker.filtered.len() >= crate::language::tools::Tool::ALL.len());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn installing_a_tool_reports_progress() {
    let dir = temp_project("install");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.install_tool(Tool::Gofmt);
    assert_eq!(app.busy(), Some("installing"));
    assert!(app.pending_install.is_some());

    // Simulate the worker's result.
    app.apply_background_event(BackgroundEvent::ToolInstalled {
        tool: Tool::Gofmt,
        result: Err("gofmt: it ships with the Go toolchain".to_string()),
    });
    assert!(app.pending_install.is_none());
    assert!(app.status.error);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn language_setup_offers_install_for_missing_tools() {
    let dir = temp_project("setup-install");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    let deadline = Instant::now() + Duration::from_secs(15);
    while app.tools.is_none() && Instant::now() < deadline {
        app.apply_background_events();
        std::thread::sleep(Duration::from_millis(5));
    }
    // Only meaningful when rust-analyzer is missing and rustup is present.
    if !app
        .tools
        .as_ref()
        .is_some_and(|tools| !tools.available(Tool::RustAnalyzer))
        || !crate::language::tools::can_install(Tool::RustAnalyzer)
    {
        fs::remove_dir_all(&dir).ok();
        return;
    }

    app.execute_command(ids::SETUP);
    let Overlay::Picker(picker) = &app.overlay else {
        panic!("expected the language setup list");
    };
    let item = (0..picker.filtered.len())
        .filter_map(|index| picker.item(index))
        .find(|item| item.label.contains("rust-analyzer"))
        .expect("rust-analyzer entry");
    assert!(
        item.enabled,
        "a missing installable tool should be actionable"
    );
    assert!(matches!(
        item.action,
        PickerAction::InstallTool(Tool::RustAnalyzer)
    ));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn offers_to_install_a_missing_language_server_once() {
    let dir = temp_project("offer-install");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    let deadline = Instant::now() + Duration::from_secs(15);
    while app.tools.is_none() && Instant::now() < deadline {
        app.apply_background_events();
        std::thread::sleep(Duration::from_millis(5));
    }

    let missing = app
        .tools
        .as_ref()
        .is_some_and(|tools| !tools.available(Tool::RustAnalyzer));
    let installable = crate::language::tools::can_install(Tool::RustAnalyzer);
    let offered = app.maybe_offer_tool_setup();
    assert_eq!(
        offered,
        missing && installable,
        "the offer must track tool availability and installability"
    );
    if offered {
        let Overlay::Picker(picker) = &app.overlay else {
            panic!("expected the install prompt");
        };
        assert!(matches!(
            picker.item(0).map(|item| &item.action),
            Some(PickerAction::InstallTool(Tool::RustAnalyzer))
        ));
        // Dismissing must not re-offer in the same session.
        app.overlay = Overlay::None;
        assert!(!app.maybe_offer_tool_setup());
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn offers_setup_for_the_project_language_with_no_file_open() {
    let dir = temp_project("project-offer");
    let mut app = App::new(None).unwrap();
    assert!(app.open_workspace(dir.clone()));
    assert_eq!(app.workspace.project.kind.language(), LanguageId::Rust);

    let deadline = Instant::now() + Duration::from_secs(15);
    while app.tools.is_none() && Instant::now() < deadline {
        app.apply_background_events();
        std::thread::sleep(Duration::from_millis(5));
    }

    let missing = app
        .tools
        .as_ref()
        .is_some_and(|tools| !tools.available(Tool::RustAnalyzer));
    let installable = crate::language::tools::can_install(Tool::RustAnalyzer);
    let offered = app.maybe_offer_tool_setup();
    assert_eq!(
        offered,
        missing && installable,
        "opening a Rust project must offer Rust tooling without an open file"
    );
    if offered {
        let Overlay::Picker(picker) = &app.overlay else {
            panic!("expected the install prompt");
        };
        assert!(matches!(
            picker.item(0).map(|item| &item.action),
            Some(PickerAction::InstallTool(Tool::RustAnalyzer))
        ));
    }
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn lsp_handshake_timeout_falls_back_to_builtin() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // A server that accepts input and never answers `initialize`.
    const MOCK: &str = "#!/bin/sh\ncat >/dev/null\n";
    let dir = temp_project("lsp-timeout");
    let file = dir.join("src/main.rs");
    let script = dir.join("mock-lsp.sh");
    {
        let mut handle = fs::File::create(&script).unwrap();
        handle.write_all(MOCK.as_bytes()).unwrap();
    }
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();

    let mut app = app_with_file(&file);
    app.start_lsp(LanguageId::Rust, script.to_str().unwrap(), &[]);
    // Pretend the handshake has been pending for too long.
    if let Some(job) = app
        .lsp
        .get_mut(&(app.workspace.root().to_path_buf(), LanguageId::Rust))
    {
        job.started_at = Some(Instant::now() - Duration::from_secs(60));
    }

    assert!(app.poll_lsp_health());
    assert!(matches!(app.lsp_status(), LspStatus::Failed(_)));
    let job = app
        .lsp
        .get(&(app.workspace.root().to_path_buf(), LanguageId::Rust))
        .unwrap();
    assert!(job.server.is_none());
    assert!(job.started_at.is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn automatic_restart_stops_after_the_budget() {
    let dir = temp_project("restart-budget");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.lsp.insert(
        (app.workspace.root().to_path_buf(), LanguageId::Rust),
        LspJob {
            restarts: MAX_LSP_RESTARTS,
            ..LspJob::default()
        },
    );

    app.schedule_lsp_restart(LanguageId::Rust);
    assert!(
        app.lsp
            .get(&(app.workspace.root().to_path_buf(), LanguageId::Rust))
            .unwrap()
            .start_at
            .is_none(),
        "the budget is exhausted"
    );
    fs::remove_dir_all(&dir).ok();
}
