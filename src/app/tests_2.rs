#[test]
fn task_history_reopens_a_completed_run() {
    use crate::app::panel::TaskRunState;

    let dir = temp_project("task-history");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    let build = TaskRunState::from_output(&dir, "Build", "make", "success", true, "done");
    let test =
        TaskRunState::from_output(&dir, "Test", "cargo test", "exit code 1", false, "failed");
    app.panel.history.push(build);
    app.panel.history.push(test);
    app.panel.output = None;

    app.execute_command(ids::TASK_HISTORY);
    let Overlay::Picker(picker) = &app.overlay else {
        panic!("expected the task history picker");
    };
    assert_eq!(picker.title, "Task Output History");
    assert_eq!(picker.filtered.len(), 2);
    assert!(
        picker
            .item(0)
            .is_some_and(|item| item.label.contains("Test")),
        "the newest run is listed first"
    );

    app.overlay = Overlay::None;
    app.show_task_history(0);
    let run = app.panel.output.as_ref().expect("output");
    assert_eq!(run.command, "make");
    assert_eq!(app.panel.tab, super::panel::PanelTab::Output);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_task_output_parser_reads_compiler_style_references() {
    use crate::app::panel::{reference_position, TaskRunState};

    // rustc's "path:line:col" form, the most common.
    assert_eq!(
        reference_position("src/main.rs:5:9: warning: unused variable"),
        Some(("src/main.rs".to_string(), 5, Some(9)))
    );
    // Line-only form.
    assert_eq!(
        reference_position("tests/render.rs:41 ok"),
        Some(("tests/render.rs".to_string(), 41, None))
    );
    // A plain line number is not a file reference.
    assert_eq!(reference_position("exit code 1: something"), None);
    assert_eq!(reference_position("(no output)"), None);
    assert_eq!(reference_position("── stderr ──"), None);
    // Resolved against the project root, 1-based, into a character position.
    let dir = temp_project("task-jump");
    let run = TaskRunState::from_output(
        &dir,
        "Test",
        "cargo test",
        "exit code 1",
        false,
        "src/main.rs:2:5: note: this is the call",
    );
    let (path, position) = run.jump_target().unwrap();
    assert_eq!(path, dir.join("src/main.rs"));
    assert_eq!(position, Position::new(1, 4));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_run_state_builder_and_jump_target_work_together() {
    let dir = temp_project("task-jump-run");
    let file = dir.join("src/main.rs");
    let run = crate::app::panel::TaskRunState::from_output(
        &dir,
        "Build",
        "make",
        "success",
        true,
        "hello\nsrc/main.rs:3:1\nbye",
    );
    let mut run = run;
    run.select_by(1); // move the row cursor onto the reference
    let (path, position) = run.jump_target().expect("expected a reference");
    assert_eq!(path, file);
    assert_eq!(position, Position::new(2, 0));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn quick_open_lists_recent_files_first() {
    let dir = temp_project("recents");
    let a = dir.join("src/main.rs");
    let b = dir.join("src/lib.rs");
    fs::write(&b, "pub fn lib() {}\n").unwrap();

    let mut app = app_with_file(&a);
    app.open_path(b.clone());
    app.execute_command(ids::QUICK_OPEN);

    let Overlay::Picker(picker) = &app.overlay else {
        panic!("expected quick open");
    };
    let first = picker.item(0).expect("an item");
    match &first.action {
        PickerAction::OpenPath(path) => {
            assert_eq!(path, &b.canonicalize().unwrap());
        }
        _ => panic!("expected a file"),
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn closing_a_dirty_tab_asks_first() {
    let dir = temp_project("close-dirty");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// x\n");

    app.execute_command(ids::CLOSE_TAB);
    assert_eq!(app.editor.len(), 1, "the dirty tab should stay open");

    app.execute_command(ids::CLOSE_TAB);
    assert_eq!(app.editor.len(), 0, "the second press closes it");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_all_writes_every_dirty_file() {
    let dir = temp_project("save-all");
    let a = dir.join("src/main.rs");
    let b = dir.join("src/lib.rs");
    fs::write(&b, "pub fn f() {}\n").unwrap();

    let mut app = app_with_file(&a);
    app.open_path(b.clone());
    app.editor.documents[0].insert_text("// a\n");
    app.editor.documents[1].insert_text("// b\n");

    app.execute_command(ids::SAVE_ALL);
    assert!(!app.editor.has_unsaved());
    assert!(fs::read_to_string(&a).unwrap().starts_with("// a\n"));
    assert!(fs::read_to_string(&b).unwrap().starts_with("// b\n"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn toggle_hidden_flips_tree_state() {
    let dir = temp_project("hidden");
    let mut app = App::new(Some(&dir)).unwrap();
    assert!(!app.workspace.tree.show_hidden);
    app.execute_command(ids::TOGGLE_HIDDEN);
    assert!(app.workspace.tree.show_hidden);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn refresh_picks_up_new_files() {
    let dir = temp_project("refresh");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    assert!(!app
        .workspace
        .tree
        .entries()
        .iter()
        .any(|entry| entry.name == "added.rs"));

    fs::write(dir.join("added.rs"), "pub fn added() {}\n").unwrap();
    app.execute_command(ids::REFRESH);
    assert!(app
        .workspace
        .tree
        .entries()
        .iter()
        .any(|entry| entry.name == "added.rs"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn find_prefills_from_the_selection() {
    let dir = temp_project("find-prefill");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    {
        let doc = app.editor.active_document_mut().unwrap();
        doc.selection = Some(Selection::new(Position::new(0, 0)));
        doc.cursor = Position::new(0, 2);
    }

    app.execute_command(ids::FIND);
    assert_eq!(app.search.query, "fn");
    assert!(!app.search.matches.is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn search_options_change_the_matches() {
    let dir = temp_project("search-options");
    let file = dir.join("src/main.rs");
    fs::write(&file, "Foo foo food foo\n").unwrap();
    let mut app = app_with_file(&file);

    app.execute_command(ids::FIND);
    app.search.query = "foo".to_string();
    app.refresh_search_matches();
    assert_eq!(app.search.matches.len(), 4, "case-insensitive substring");

    // Alt+C toggles case sensitivity.
    app.handle_search_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT));
    assert!(app.search.case_sensitive);
    assert_eq!(app.search.matches.len(), 3);

    // Alt+W toggles whole-word matching.
    app.handle_search_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::ALT));
    assert!(app.search.whole_word);
    assert_eq!(app.search.matches.len(), 2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn regex_search_matches_and_reports_errors() {
    let dir = temp_project("search-regex");
    let file = dir.join("src/main.rs");
    fs::write(&file, "let count = 42;\nlet total = 7;\n").unwrap();
    let mut app = app_with_file(&file);

    app.execute_command(ids::FIND);
    app.handle_search_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::ALT));
    assert!(app.search.regex);

    app.search.query = r"\d+".to_string();
    app.refresh_search_matches();
    assert_eq!(app.search.matches.len(), 2);
    assert!(app.search.regex_error.is_none());

    // Unsupported syntax is reported instead of matching silently wrong.
    app.search.query = "(a|b)".to_string();
    app.refresh_search_matches();
    assert!(app.search.matches.is_empty());
    assert!(app.search.regex_error.is_some());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn replace_all_applies_every_match_as_one_edit() {
    let dir = temp_project("replace-all");
    let file = dir.join("src/main.rs");
    fs::write(&file, "foo foo foo\nbar foo\n").unwrap();
    let mut app = app_with_file(&file);

    app.execute_command(ids::REPLACE);
    app.search.query = "foo".to_string();
    app.search.replacement = "baz".to_string();
    app.refresh_search_matches();
    assert_eq!(app.search.matches.len(), 4);

    app.replace_all();
    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "baz baz baz\nbar baz\n"
    );

    // A single undo restores the original.
    app.with_doc(|d| d.undo());
    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "foo foo foo\nbar foo\n"
    );
    fs::remove_dir_all(&dir).ok();
}

/// Spin the background channel until the active document has diagnostics.
fn wait_for_diagnostics(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        app.apply_background_events();
        if app
            .editor
            .active_document()
            .is_some_and(|doc| !doc.diagnostics().is_empty())
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn diagnostics_are_computed_in_the_background() {
    let dir = temp_project("diagnostics");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {\n    let x = 1;\n").unwrap();

    let mut app = app_with_file(&file);
    assert_eq!(
        app.editor.active_document().unwrap().buffer.language,
        LanguageId::Rust
    );
    app.poll_diagnostics();
    wait_for_diagnostics(&mut app);

    let doc = app.editor.active_document().unwrap();
    assert!(
        !doc.diagnostics().is_empty(),
        "expected an unclosed-brace diagnostic"
    );
    assert_eq!(doc.diagnostic_counts().0, 1);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn next_diagnostic_moves_the_cursor() {
    let dir = temp_project("diagnostics-next");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {\n    let x = 1;\n").unwrap();

    let mut app = app_with_file(&file);
    app.poll_diagnostics();
    wait_for_diagnostics(&mut app);
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(0, 0));

    app.execute_command(ids::DIAGNOSTICS_NEXT);
    let cursor = app.editor.active_document().unwrap().clamped_cursor();
    assert_eq!(cursor, Position::new(0, 10));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_bottom_panel_toggles_and_cycles_tabs() {
    let dir = temp_project("panel-keys");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    assert!(!app.panel.open, "the panel starts hidden");

    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert!(app.panel.open);
    assert_eq!(app.focus, Focus::Panel);
    assert_eq!(app.panel.tab, panel::PanelTab::Problems);

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.panel.tab, panel::PanelTab::Output);
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(app.panel.tab, panel::PanelTab::Problems);
    app.handle_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));
    assert_eq!(app.panel.tab, panel::PanelTab::Terminal);
    // In the terminal Tab belongs to the shell, so panel tabs use
    // Ctrl+Shift+PageDown.
    app.handle_key(KeyEvent::new(
        KeyCode::PageDown,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(app.panel.tab, panel::PanelTab::DebugConsole);

    // Esc returns the keyboard to the editor without hiding the panel.
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Editor);
    assert!(app.panel.open);

    // A second Ctrl+J focuses the visible panel, and a third hides it.
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert_eq!(app.focus, Focus::Panel);
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert!(!app.panel.open);
    assert_eq!(app.focus, Focus::Editor);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ctrl_enter_also_toggles_the_panel() {
    // Terminals without the kitty keyboard protocol report Ctrl+J as Ctrl+Enter.
    let dir = temp_project("panel-ctrl-enter");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
    assert!(app.panel.open);
    assert_eq!(app.focus, Focus::Panel);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_panel_problems_tab_lists_and_reveals_diagnostics() {
    let dir = temp_project("panel-problems");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    {
        let doc = app.editor.active_document_mut().unwrap();
        doc.set_diagnostics_revision(1);
        doc.apply_diagnostics(
            1,
            vec![Diagnostic::new(
                crate::language::diagnostics::TextPos::new(1, 4),
                crate::language::diagnostics::TextPos::new(1, 9),
                Severity::Warning,
                "unused value",
            )],
        );
        doc.move_to(Position::new(0, 0));
    }

    let problems = app.panel_problems();
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].file, "src/main.rs");
    assert_eq!(problems[0].position, Position::new(1, 4));
    assert_eq!(problems[0].severity, Severity::Warning);

    app.execute_command(ids::PANEL_PROBLEMS);
    assert!(app.panel.open);
    assert_eq!(app.focus, Focus::Panel);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.focus, Focus::Editor, "Enter returns to the editor");
    assert_eq!(
        app.editor.active_document().unwrap().clamped_cursor(),
        Position::new(1, 4)
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_problems_tab_scans_unopened_project_files() {
    let dir = temp_project("problems-workspace");
    let open = dir.join("src/main.rs");
    let broken = dir.join("src/broken.rs");
    fs::write(&broken, "fn main() {\n").unwrap();

    let mut app = app_with_file(&open);
    app.execute_command(ids::PANEL_PROBLEMS);
    let deadline = Instant::now() + Duration::from_secs(15);
    while app.pending_project_diagnostics.is_some() && Instant::now() < deadline {
        app.pump_background(Duration::from_millis(20));
    }

    assert!(
        app.panel
            .workspace_problems
            .iter()
            .any(|problem| problem.path.ends_with("broken.rs")),
        "the unopened broken file should be reported"
    );
    assert!(
        app.panel
            .workspace_problems
            .iter()
            .all(|problem| problem.path != open),
        "open files keep their live diagnostics and are skipped by the scan"
    );
    assert!(app
        .panel_problems()
        .iter()
        .any(|problem| problem.path.ends_with("broken.rs")));

    app.panel.tab = super::panel::PanelTab::Problems;
    app.execute_command(ids::PANEL_CLEAR);
    assert!(app.panel.workspace_problems.is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_panel_resizes_within_bounds() {
    let dir = temp_project("panel-resize");
    let mut app = App::new(Some(&dir)).unwrap();

    for _ in 0..100 {
        app.execute_command(ids::PANEL_GROW);
    }
    assert_eq!(app.panel.height, panel::PANEL_MAX_HEIGHT);

    for _ in 0..100 {
        app.execute_command(ids::PANEL_SHRINK);
    }
    assert_eq!(app.panel.height, panel::PANEL_MIN_HEIGHT);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn clearing_the_panel_keeps_problems_but_empties_output() {
    let dir = temp_project("panel-clear");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.panel.tab = panel::PanelTab::Output;
    app.panel.output = Some(panel::TaskRunState::from_output(
        &dir,
        "Test",
        "cargo test",
        "success",
        true,
        "all good",
    ));
    app.execute_command(ids::PANEL_CLEAR);
    assert!(app.panel.output.is_none());

    app.debug_console("adapter started");
    assert_eq!(app.panel.tab, panel::PanelTab::DebugConsole);
    assert_eq!(app.panel.debug, vec!["adapter started".to_string()]);
    app.execute_command(ids::PANEL_CLEAR);
    assert!(app.panel.debug.is_empty());

    // Problems are derived from the documents, so Clear cannot hide them.
    app.execute_command(ids::PANEL_PROBLEMS);
    app.execute_command(ids::PANEL_CLEAR);
    assert_eq!(app.panel.tab, panel::PanelTab::Problems);
    app.debug_console("still there");
    assert_eq!(app.panel.debug.len(), 1);

    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn a_mock_debug_adapter_stops_and_reports_frames_and_variables() {
    use std::os::unix::fs::PermissionsExt;

    const MOCK: &str = r#"#!/bin/sh
seq=100
reply() {
  printf 'Content-Length: %s\r\n\r\n%s' "${#1}" "$1"
}
while IFS= read -r header; do
  [ -z "$header" ] && continue
  len=$(printf '%s' "$header" | tr -dc '0-9')
  IFS= read -r blank
  body=$(dd bs=1 count="$len" 2>/dev/null)
  req=$(printf '%s' "$body" | sed -n 's/.*"seq":\([0-9]*\).*/\1/p')
  cmd=$(printf '%s' "$body" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')
  case "$cmd" in
    initialize)
      reply "{\"seq\":$seq,\"type\":\"response\",\"request_seq\":$req,\"success\":true,\"command\":\"initialize\",\"body\":{}}"
      seq=$((seq+1))
      reply "{\"seq\":$seq,\"type\":\"event\",\"event\":\"initialized\"}"
      ;;
    launch)
      reply "{\"seq\":$seq,\"type\":\"response\",\"request_seq\":$req,\"success\":true,\"command\":\"launch\",\"body\":{}}"
      seq=$((seq+1))
      reply "{\"seq\":$seq,\"type\":\"event\",\"event\":\"stopped\",\"body\":{\"reason\":\"breakpoint\",\"threadId\":1}}"
      ;;
    stackTrace)
      reply "{\"seq\":$seq,\"type\":\"response\",\"request_seq\":$req,\"success\":true,\"command\":\"stackTrace\",\"body\":{\"stackFrames\":[{\"id\":7,\"name\":\"main\",\"line\":3,\"column\":1,\"source\":{\"path\":\"/tmp/mock.py\"}}]}}"
      ;;
    scopes)
      reply "{\"seq\":$seq,\"type\":\"response\",\"request_seq\":$req,\"success\":true,\"command\":\"scopes\",\"body\":{\"scopes\":[{\"name\":\"Locals\",\"variablesReference\":5}]}}"
      ;;
    variables)
      reply "{\"seq\":$seq,\"type\":\"response\",\"request_seq\":$req,\"success\":true,\"command\":\"variables\",\"body\":{\"variables\":[{\"name\":\"x\",\"value\":\"42\",\"type\":\"int\"}]}}"
      ;;
    evaluate)
      reply "{\"seq\":$seq,\"type\":\"response\",\"request_seq\":$req,\"success\":true,\"command\":\"evaluate\",\"body\":{\"result\":\"42\",\"type\":\"int\",\"variablesReference\":0}}"
      ;;
    *)
      reply "{\"seq\":$seq,\"type\":\"response\",\"request_seq\":$req,\"success\":true,\"command\":\"$cmd\",\"body\":{}}"
      ;;
  esac
  seq=$((seq+1))
done
"#;
    let dir = temp_project("mock-debug");
    let file = dir.join("src/main.py");
    fs::write(&file, "print('x')\n").unwrap();
    let script = dir.join("adapter.sh");
    fs::write(&script, MOCK).unwrap();
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).unwrap();

    let mut app = app_with_file(&file);
    let mut session = crate::debug::DebugSession::start(script.to_str().unwrap(), &[], &dir)
        .expect("spawn the mock adapter");
    session.initialize("mock").unwrap();
    app.debug.session = Some(session);
    app.debug.plan = Some(crate::debug::python_plan(&file, Some(script.clone())));
    app.debug.status = super::debug::DebugStatus::Starting;
    app.add_debug_watch("x".to_string());

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline
        && (app.debug.frames.is_empty()
            || app.debug.variables.is_empty()
            || app.debug.watches.iter().all(|watch| watch.value.is_none()))
    {
        app.poll_debug();
        std::thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(app.debug.status, super::debug::DebugStatus::Stopped);
    assert_eq!(app.debug.frames.len(), 1);
    assert_eq!(app.debug.frames[0].name, "main");
    assert_eq!(
        app.debug.frames[0].path,
        Some(PathBuf::from("/tmp/mock.py"))
    );
    assert!(
        app.debug
            .variables
            .iter()
            .any(|(scope, variable)| scope == "Locals"
                && variable.name == "x"
                && variable.value == "42"),
        "variables should come from the adapter"
    );
    let watch = app
        .debug
        .watches
        .iter()
        .find(|watch| watch.expression == "x")
        .expect("the watch should exist");
    assert_eq!(watch.value.as_deref(), Some("42"));
    assert_eq!(watch.kind, "int");
    app.debug_stop();
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn diagnostics_list_opens_when_there_are_problems() {
    let dir = temp_project("diagnostics-list");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {\n").unwrap();

    let mut app = app_with_file(&file);
    app.poll_diagnostics();
    wait_for_diagnostics(&mut app);

    app.execute_command(ids::DIAGNOSTICS_LIST);
    let Overlay::Picker(picker) = &app.overlay else {
        panic!("expected the diagnostics list");
    };
    assert!(!picker.filtered.is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn symbol_outline_lists_definitions() {
    let dir = temp_project("symbols");
    let file = dir.join("src/main.rs");

    let mut app = app_with_file(&file);
    app.execute_command(ids::SHOW_SYMBOLS);

    let Overlay::Picker(picker) = &app.overlay else {
        panic!("expected the symbol list");
    };
    let labels: Vec<String> = (0..picker.filtered.len())
        .filter_map(|index| picker.item(index))
        .map(|item| item.label.clone())
        .collect();
    assert!(labels.iter().any(|label| label == "main"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn go_to_definition_jumps_to_the_symbol() {
    let dir = temp_project("definition");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {\n    helper();\n}\n\nfn helper() {}\n").unwrap();

    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(1, 4));

    app.execute_command(ids::GOTO_DEFINITION);
    let cursor = app.editor.active_document().unwrap().clamped_cursor();
    assert_eq!(cursor, Position::new(4, 3));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn go_to_definition_falls_back_to_the_project() {
    let dir = temp_project("definition-project");
    let a = dir.join("src/main.rs");
    let b = dir.join("src/lib.rs");
    fs::write(&a, "fn main() {\n    helper();\n}\n").unwrap();
    fs::write(&b, "pub fn helper() {}\n").unwrap();
    let mut app = app_with_file(&a);
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(1, 4));

    app.execute_command(ids::GOTO_DEFINITION);

    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        app.apply_background_events();
        if matches!(app.overlay, Overlay::Picker(_)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let Overlay::Picker(picker) = &app.overlay else {
        panic!("expected the workspace symbol picker");
    };
    let labels: Vec<String> = (0..picker.filtered.len())
        .filter_map(|index| picker.item(index))
        .map(|item| item.label.clone())
        .collect();
    assert!(
        labels.iter().any(|label| label == "helper"),
        "labels: {labels:?}"
    );
    fs::remove_dir_all(&dir).ok();
}
