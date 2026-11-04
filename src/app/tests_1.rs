use super::*;
use std::fs;

use super::{SETTINGS_ROWS, SettingsRow};
use crate::settings::{Settings, ThemeId};

fn temp_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kemori-app-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
    fs::write(dir.join("src/main.rs"), "fn main() {\n    let x = 1;\n}\n").unwrap();
    // Resolve symlinks (macOS exposes the temp dir as `/var` → `/private/var`)
    // so the paths a test builds match what Kemori stores after canonicalising
    // the workspace root.
    fs::canonicalize(&dir).unwrap()
}

/// Build an app and open `file`, settling detection, as tests expect a
/// document to be active. Kemori itself launches into the welcome screen.
///
/// Detection runs on the background worker behind any queued git/tool
/// probes, so wait for the result rather than assuming a fixed delay. Only
/// extensions a provider recognises are waited on; an unknown extension is
/// expected to stay [`LanguageId::Unknown`].
fn app_with_file(file: &Path) -> App {
    let mut app = App::new(Some(file)).unwrap();
    app.open_path(file.to_path_buf());
    let expected_known = std::fs::read(file)
        .map(|bytes| bytes.starts_with(b"#!"))
        .unwrap_or(false)
        || file
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| app.language.language_for_extension(extension).is_some());
    if expected_known {
        // Detection runs on the background worker behind any queued git and
        // tool probes. Under a fully loaded test machine (many worker
        // threads and subprocess probes) it can take several seconds, so the
        // deadline is generous rather than tight.
        let deadline = Instant::now() + Duration::from_secs(15);
        while app
            .editor
            .active_document()
            .is_some_and(|doc| doc.buffer.language == LanguageId::Unknown)
            && Instant::now() < deadline
        {
            app.pump_background(Duration::from_millis(20));
        }
    } else {
        app.pump_background(Duration::from_millis(300));
    }
    app
}

#[test]
fn opens_rust_file_and_detects_language() {
    let dir = temp_project("open");
    let file = dir.join("src/main.rs");
    let app = app_with_file(&file);
    assert_eq!(app.editor.len(), 1);
    assert_eq!(
        app.editor.active_document().unwrap().buffer.language,
        LanguageId::Rust
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn saves_and_marks_clean() {
    let dir = temp_project("save");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// hello\n");
    assert!(app.editor.active_document().unwrap().is_dirty());

    app.execute_command(ids::SAVE);
    assert!(!app.editor.active_document().unwrap().is_dirty());
    assert!(fs::read_to_string(&file).unwrap().starts_with("// hello\n"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn revert_discards_edits_and_reloads_from_disk() {
    let dir = temp_project("revert");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// changed\n");
    assert!(app.editor.active_document().unwrap().is_dirty());

    app.execute_command(ids::REVERT);
    let doc = app.editor.active_document().unwrap();
    assert!(!doc.is_dirty());
    assert!(!doc.buffer.text().contains("// changed"));
    assert!(doc.buffer.text().contains("fn main"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn new_file_creates_and_opens_it() {
    let dir = temp_project("new-file");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.execute_command(ids::NEW_FILE);
    assert!(matches!(app.overlay, Overlay::Prompt(_)));
    app.submit_prompt(PromptKind::NewFile, "created.rs".to_string());

    let created = dir.join("src/created.rs");
    assert!(created.is_file());
    assert_eq!(
        app.editor.active_document().unwrap().buffer.path.as_deref(),
        Some(created.as_path())
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn duplicate_file_creates_a_copy_and_opens_it() {
    let dir = temp_project("duplicate");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.execute_command(ids::DUPLICATE_FILE);
    let copy = dir.join("src/main copy.rs");
    assert!(copy.is_file(), "expected {}", copy.display());
    assert_eq!(
        app.editor.active_document().unwrap().buffer.path.as_deref(),
        Some(copy.as_path())
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn copy_file_prompts_and_writes_the_copy() {
    let dir = temp_project("copy-file");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.execute_command(ids::COPY_FILE);
    assert!(matches!(
        &app.overlay,
        Overlay::Prompt(prompt) if prompt.kind == PromptKind::CopyFile
    ));
    app.submit_prompt(PromptKind::CopyFile, "src/backup.rs".to_string());
    assert!(dir.join("src/backup.rs").is_file());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn rename_updates_the_open_document() {
    let dir = temp_project("rename-file");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.execute_command(ids::RENAME_FILE);
    assert!(matches!(app.overlay, Overlay::Prompt(_)));
    app.submit_prompt(PromptKind::RenameFile, "renamed.rs".to_string());

    assert!(!file.exists());
    let renamed = dir.join("src/renamed.rs");
    assert!(renamed.is_file());
    assert_eq!(
        app.editor.active_document().unwrap().buffer.path.as_deref(),
        Some(renamed.as_path())
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn delete_removes_the_file_and_closes_its_tab() {
    let dir = temp_project("delete-file");
    let a = dir.join("src/main.rs");
    let b = dir.join("src/extra.rs");
    fs::write(&b, "pub fn extra() {}\n").unwrap();
    let mut app = app_with_file(&a);
    app.open_path(b.clone());
    assert_eq!(app.editor.len(), 2);

    app.delete_path(&b);
    assert!(!b.exists());
    assert_eq!(app.editor.len(), 1);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn kill_ring_supports_yank_pop() {
    let dir = temp_project("kill-ring");
    let file = dir.join("src/main.rs");
    fs::write(&file, "").unwrap();
    let mut app = app_with_file(&file);

    app.push_kill("first");
    app.push_kill("second");
    app.paste();
    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "second"
    );

    app.yank_pop();
    assert_eq!(app.editor.active_document().unwrap().buffer.text(), "first");

    // Nothing older: the text is left alone.
    app.yank_pop();
    assert_eq!(app.editor.active_document().unwrap().buffer.text(), "first");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn yank_pop_is_refused_after_an_edit() {
    let dir = temp_project("yank-edit");
    let file = dir.join("src/main.rs");
    fs::write(&file, "").unwrap();
    let mut app = app_with_file(&file);
    app.push_kill("first");
    app.push_kill("second");
    app.paste();

    // Any further edit invalidates the yank-pop target.
    app.with_doc(|doc| doc.type_char('!'));
    app.yank_pop();
    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "second!"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn toggle_comment_adds_prefix() {
    let dir = temp_project("comment");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(1, 0));

    app.execute_command(ids::TOGGLE_COMMENT);

    let doc = app.editor.active_document().unwrap();
    assert!(doc.buffer.line_text(1).contains("// let x = 1;"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn command_palette_opens() {
    let dir = temp_project("palette");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.execute_command(ids::PALETTE);
    assert!(!app.overlay.is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn split_shows_two_documents_and_focus_switches() {
    let dir = temp_project("split");
    let a = dir.join("src/main.rs");
    let b = dir.join("src/lib.rs");
    fs::write(&a, "fn main() {}\n").unwrap();
    fs::write(&b, "pub fn lib() {}\n").unwrap();
    let mut app = app_with_file(&a);
    app.open_path(b.clone());
    assert_eq!(app.editor.len(), 2);
    let active = app.editor.active_index();

    app.execute_command(ids::SPLIT);
    assert!(app.split);
    assert_eq!(app.pane_left_index(), active);
    assert_eq!(app.pane_right_index(), Some((active + 1) % 2));
    assert_eq!(app.focus_pane, Pane::Primary);

    app.execute_command(ids::FOCUS_PANE);
    assert_eq!(app.focus_pane, Pane::Secondary);
    assert_eq!(app.editor.active_index(), (active + 1) % 2);

    // Typing edits only the focused (right) document.
    app.with_doc(|doc| doc.insert_text("// right\n"));
    let right = (active + 1) % 2;
    assert!(
        app.editor.documents[right]
            .buffer
            .text()
            .contains("// right")
    );
    assert!(
        !app.editor.documents[active]
            .buffer
            .text()
            .contains("// right")
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn alt_v_splits_and_alt_o_switches_panes() {
    let dir = temp_project("alt-split");
    let a = dir.join("src/main.rs");
    let b = dir.join("src/lib.rs");
    fs::write(&b, "pub fn lib() {}\n").unwrap();
    let mut app = app_with_file(&a);
    app.open_path(b.clone());

    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT));
    assert!(app.split);

    let before = app.editor.active_index();
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::ALT));
    assert_ne!(app.editor.active_index(), before);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn closing_a_split_document_collapses_the_split() {
    let dir = temp_project("split-close");
    let a = dir.join("src/main.rs");
    let b = dir.join("src/lib.rs");
    fs::write(&a, "fn main() {}\n").unwrap();
    fs::write(&b, "pub fn lib() {}\n").unwrap();
    let mut app = app_with_file(&a);
    app.open_path(b.clone());
    app.execute_command(ids::SPLIT);
    assert!(app.split);

    // Close the focused (left) document; the split collapses to one pane.
    app.execute_command(ids::CLOSE_TAB);
    assert_eq!(app.editor.len(), 1);
    assert!(!app.split);
    assert!(app.pane_right_index().is_none());
    assert_eq!(app.editor.active_index(), app.pane_left_index());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn focus_tree_command_toggles_focus() {
    let dir = temp_project("focus");
    let mut app = App::new(Some(&dir)).unwrap();
    assert_eq!(app.focus, Focus::Editor);

    app.execute_command(ids::FOCUS_TREE);
    assert_eq!(app.focus, Focus::FileTree);

    app.execute_command(ids::FOCUS_TREE);
    assert_eq!(app.focus, Focus::Editor);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ctrl_b_focuses_the_file_panel_then_hides_it() {
    let dir = temp_project("ctrl-b");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    assert!(app.tree_visible);
    assert_eq!(app.focus, Focus::Editor);

    // First press moves focus into the files.
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert!(app.tree_visible);
    assert_eq!(app.focus, Focus::FileTree);

    // Second press hides it and returns to the editor.
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert!(!app.tree_visible);
    assert_eq!(app.focus, Focus::Editor);

    // Third press reveals and focuses it again.
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert!(app.tree_visible);
    assert_eq!(app.focus, Focus::FileTree);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ctrl_shift_p_opens_the_command_palette() {
    let dir = temp_project("palette-shift");
    let file = dir.join("src/main.rs");

    // Terminals that report an uppercase `P` with only Control still open
    // the command palette, not quick open.
    let mut app = app_with_file(&file);
    app.handle_key(KeyEvent::new(KeyCode::Char('P'), KeyModifiers::CONTROL));
    match &app.overlay {
        Overlay::Picker(picker) => assert_eq!(picker.title, "Command Palette"),
        _ => panic!("expected the command palette"),
    }

    // A plain `Ctrl+P` stays quick open.
    let mut app = app_with_file(&file);
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    match &app.overlay {
        Overlay::Picker(picker) => assert_eq!(picker.title, "Quick Open"),
        _ => panic!("expected quick open"),
    }

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn palette_marks_unavailable_commands() {
    let dir = temp_project("palette-avail");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.execute_command(ids::PALETTE);

    let Overlay::Picker(picker) = &app.overlay else {
        panic!("expected the command palette");
    };
    let find = |needle: &str| {
        (0..picker.filtered.len())
            .filter_map(|index| picker.item(index))
            .find(|item| item.label.contains(needle))
    };

    assert!(find("File: Save").expect("save").enabled);
    // Formatting availability tracks whether the tool is actually installed.
    let rustfmt_available = app
        .tools
        .as_ref()
        .map(|tools| tools.available(Tool::Rustfmt))
        .unwrap_or_else(|| format::is_available("rustfmt"));
    assert_eq!(
        find("Format Document").expect("format").enabled,
        rustfmt_available
    );
    assert!(
        !find("Rename Symbol").expect("rename").enabled,
        "rename is not available for Rust yet"
    );
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn running_project_code_asks_for_workspace_trust_first() {
    let dir = temp_project("trust-gate");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    let task = Task {
        label: "Echo".to_string(),
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "echo TRUSTED\n".to_string()],
    };

    app.run_picker_action(PickerAction::RunTask(task.clone()));
    assert!(!app.pending_task, "the task must not start before trust");
    match &app.overlay {
        Overlay::Picker(picker) => assert_eq!(picker.title, "Workspace Trust"),
        _ => panic!("expected the trust prompt"),
    }

    app.resolve_trust(super::trust::TrustDecision::Trust);
    assert!(app.workspace_trusted());
    assert!(app.pending_task, "trusting the project starts the action");

    let deadline = Instant::now() + Duration::from_secs(15);
    while app.pending_task && Instant::now() < deadline {
        app.pump_background(Duration::from_millis(20));
    }
    assert!(!app.pending_task);
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn the_terminal_starts_after_trust_and_uses_the_panel() {
    let dir = temp_project("trust-terminal");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    // Tests must not start the user's configured shell; the production path is
    // covered by the PTY/session tests with an explicit program.
    app.terminal_shell = Some("/bin/sh".to_string());

    app.open_terminal();
    match &app.overlay {
        Overlay::Picker(picker) => assert_eq!(picker.title, "Workspace Trust"),
        _ => panic!("expected the trust prompt"),
    }
    assert!(app.terminals.is_empty());

    app.resolve_trust(super::trust::TrustDecision::Trust);
    assert!(!app.terminals.is_empty(), "a session should start");
    assert_eq!(app.panel.tab, panel::PanelTab::Terminal);
    assert!(app.panel.open);
    assert_eq!(app.focus, Focus::Panel);
    app.terminals.kill_all();
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn a_run_task_executes_and_shows_its_output() {
    // A Makefile is the one marker whose "task" requires no ecosystem tool
    // beyond `make`, so the whole rail runs without cargo or npm.
    let dir = temp_project("run-task");
    fs::write(dir.join("Makefile"), "all:\n\t@echo KEMORI_TASK_RAN\n").unwrap();
    let file = dir.join("notes.txt");
    fs::write(&file, "hello\n").unwrap();

    let mut app = app_with_file(&file);
    app.execute_command(ids::RUN_TASKS);
    let task = match &app.overlay {
        Overlay::Picker(picker) => {
            let item = (0..picker.filtered.len())
                .filter_map(|index| picker.item(index))
                .find(|item| item.label.contains("make"))
                .expect("expected the make task in the picker");
            item.action.clone()
        }
        _ => panic!("expected the task picker"),
    };
    assert!(matches!(task, PickerAction::RunTask(_)));

    app.overlay = Overlay::None;
    // Running project code is gated on workspace trust; this test exercises the
    // run rail, so record the decision first.
    let root = app.workspace.root().to_path_buf();
    app.trust.trust(&root);
    app.run_picker_action(task);
    assert!(
        app.status_message()
            .is_some_and(|message| message.contains("Running")),
        "the statusline should say the task is running"
    );

    // The task finishes quickly; wait for the final result in the panel.
    let deadline = Instant::now() + Duration::from_secs(30);
    while app.pending_task && Instant::now() < deadline {
        app.pump_background(Duration::from_millis(50));
    }
    assert!(!app.pending_task, "the pending flag must clear");
    match &app.panel.output {
        Some(run) => {
            assert!(run.success, "the task should have exited cleanly");
            assert_eq!(run.command, "make");
            assert!(
                run.lines
                    .iter()
                    .any(|line| line.contains("KEMORI_TASK_RAN")),
                "the output should contain the make script's echo"
            );
        }
        None => panic!("expected the task output view"),
    }
    assert!(app.panel.open, "the panel should open with the output");
    assert_eq!(app.panel.tab, super::panel::PanelTab::Output);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn task_output_appends_partial_lines_and_follows_the_tail() {
    use crate::app::panel::TaskRunState;

    let mut run = TaskRunState::from_output(&PathBuf::from("."), "T", "cmd", "running", true, "");
    run.append("one\ntw");
    run.append("o\n");
    assert_eq!(run.lines, vec!["one".to_string(), "two".to_string()]);
    assert_eq!(run.selected, 1, "follow keeps the cursor on the last line");
    assert!(run.follow);

    // Scrolling up must stop the view from being yanked back by new output.
    run.select_by(-1);
    assert!(!run.follow);
    run.append("three\n");
    assert_eq!(run.selected, 0);
    assert_eq!(run.lines.len(), 3);

    run.select_by(i64::MAX);
    assert!(run.follow);
    assert_eq!(run.selected, 2);
}

#[cfg(unix)]
#[test]
fn a_running_task_can_be_cancelled() {
    let dir = temp_project("task-cancel");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.run_task(Task {
        label: "Slow".to_string(),
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "sleep 30".to_string()],
    });
    assert!(app.pending_task);
    app.cancel_task();
    assert!(
        app.status_message()
            .is_some_and(|message| message.contains("Cancelling"))
    );

    let deadline = Instant::now() + Duration::from_secs(15);
    while app.pending_task && Instant::now() < deadline {
        app.pump_background(Duration::from_millis(20));
    }
    assert!(!app.pending_task, "cancelling must end the task promptly");
    let run = app.panel.output.as_ref().expect("output state");
    assert_eq!(run.outcome, "cancelled");
    assert!(!run.success);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn task_output_references_become_problems() {
    let dir = temp_project("task-problems-parse");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {\n    let x = 1;\n}\n").unwrap();

    let problems = super::panel::task_problems(
        &dir,
        "src/main.rs:2:5: error: boom\n\
         not a reference\n\
         src/main.rs:2:5: error: duplicate\n\
         src/missing.rs:1:1: error: ignored",
    );
    assert_eq!(
        problems.len(),
        1,
        "duplicates and missing files are dropped"
    );
    assert_eq!(problems[0].file, "src/main.rs");
    assert_eq!(problems[0].position, Position::new(1, 4));
    assert_eq!(problems[0].severity, Severity::Error);
    assert_eq!(problems[0].message, "error: boom");
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn a_finished_task_adds_its_errors_to_the_problems_tab() {
    let dir = temp_project("task-problems-live");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.run_task(Task {
        label: "Check".to_string(),
        program: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "printf 'src/main.rs:2:7: error: boom\\n'".to_string(),
        ],
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    while app.pending_task && Instant::now() < deadline {
        app.pump_background(Duration::from_millis(20));
    }

    let problem = app
        .panel_problems()
        .into_iter()
        .find(|problem| problem.message.contains("boom"))
        .expect("the task error should appear in Problems");
    assert_eq!(problem.severity, Severity::Error);
    assert_eq!(problem.position, Position::new(1, 6));

    app.panel.tab = super::panel::PanelTab::Problems;
    app.execute_command(ids::PANEL_CLEAR);
    assert!(app.panel.task_problems.is_empty());

    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn run_active_file_runs_a_standalone_script() {
    let dir = temp_project("run-active");
    let script = dir.join("script.sh");
    fs::write(&script, "echo KEMORI_ACTIVE_FILE_RAN\n").unwrap();
    let mut app = app_with_file(&script);
    let root = app.workspace.root().to_path_buf();
    app.trust.trust(&root);

    app.execute_command(ids::RUN_ACTIVE_FILE);
    let deadline = Instant::now() + Duration::from_secs(15);
    while app.pending_task && Instant::now() < deadline {
        app.pump_background(Duration::from_millis(20));
    }
    let run = app.panel.output.as_ref().expect("task output");
    assert!(run.success);
    assert_eq!(run.command, format!("sh {}", script.display()));
    assert!(
        run.lines
            .iter()
            .any(|line| line.contains("KEMORI_ACTIVE_FILE_RAN"))
    );
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn run_active_file_refuses_an_unsaved_buffer() {
    let dir = temp_project("run-active-dirty");
    let script = dir.join("script.sh");
    fs::write(&script, "echo old\n").unwrap();
    let mut app = app_with_file(&script);
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("# new\n");

    app.execute_command(ids::RUN_ACTIVE_FILE);
    assert!(!app.pending_task, "a dirty file must not start a task");
    assert!(
        app.status_message()
            .is_some_and(|message| message.contains("Save the file")),
        "the reason should be explicit: {:?}",
        app.status_message()
    );
    fs::remove_dir_all(&dir).ok();
}

