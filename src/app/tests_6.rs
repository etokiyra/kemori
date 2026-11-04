#[test]
fn detects_typescript_and_javascript() {
    let dir = temp_project("web-langs");
    let cases = [
        (
            "app.ts",
            "export const total: number = 1;\n",
            LanguageId::TypeScript,
        ),
        (
            "view.tsx",
            "export const App = () => null;\n",
            LanguageId::TypeScript,
        ),
        ("index.js", "const total = 1;\n", LanguageId::JavaScript),
        (
            "component.jsx",
            "export default function App() { return null; }\n",
            LanguageId::JavaScript,
        ),
    ];
    for (name, content, expected) in cases {
        let file = dir.join(name);
        fs::write(&file, content).unwrap();
        let app = app_with_file(&file);
        let language = app.editor.active_document().unwrap().buffer.language;
        assert_eq!(language, expected, "detected {name} as {language:?}");
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn detects_python_from_extension_and_shebang() {
    let dir = temp_project("python");
    let file = dir.join("app.py");
    fs::write(&file, "def main():\n    pass\n").unwrap();
    let app = app_with_file(&file);
    assert_eq!(
        app.editor.active_document().unwrap().buffer.language,
        LanguageId::Python
    );

    // An extensionless script is recognised from its shebang.
    let script = dir.join("tool");
    fs::write(&script, "#!/usr/bin/env python3\nprint('hi')\n").unwrap();
    let app = app_with_file(&script);
    assert_eq!(
        app.editor.active_document().unwrap().buffer.language,
        LanguageId::Python
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn detects_shell_from_extension_and_shebang() {
    let dir = temp_project("shell");
    let file = dir.join("build.sh");
    fs::write(&file, "#!/usr/bin/env bash\necho hi\n").unwrap();
    let app = app_with_file(&file);
    assert_eq!(
        app.editor.active_document().unwrap().buffer.language,
        LanguageId::Shell
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn warns_when_a_dirty_file_changes_on_disk() {
    let dir = temp_project("external-dirty");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// mine\n");

    std::thread::sleep(Duration::from_millis(10));
    fs::write(&file, "fn theirs() {}\n").unwrap();
    app.last_disk_check = Instant::now() - Duration::from_secs(5);

    assert!(app.poll_external_changes());
    assert!(app.status.error);
    assert!(app
        .status_message()
        .unwrap_or("")
        .contains("changed on disk"));
    // The edit is preserved rather than silently overwritten.
    assert!(app
        .editor
        .active_document()
        .unwrap()
        .buffer
        .text()
        .starts_with("// mine"));
    fs::remove_dir_all(&dir).ok();
}

// ------------------------------------------------------------------
// Welcome screen and project creation
// ------------------------------------------------------------------

#[test]
fn startup_shows_the_welcome_screen_for_a_file_target() {
    let dir = temp_project("startup-file");
    let file = dir.join("src/main.rs");
    let app = App::new(Some(&file)).unwrap();

    assert!(
        app.editor.is_empty(),
        "a CLI file must not be opened automatically"
    );
    assert!(app.welcome_active());
    let labels: Vec<String> = app
        .welcome_items()
        .into_iter()
        .map(|item| item.label)
        .collect();
    assert!(labels.iter().any(|label| label.contains("Open a file")));
    assert!(labels
        .iter()
        .any(|label| label.contains("Create a new project")));
    assert!(
        labels.iter().any(|label| label.contains("main.rs")),
        "the CLI target should be offered: {labels:?}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn startup_shows_the_welcome_screen_for_a_directory_target() {
    let dir = temp_project("startup-dir");
    let app = App::new(Some(&dir)).unwrap();
    assert!(app.editor.is_empty());
    assert!(app.welcome_active());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn welcome_target_action_opens_the_file() {
    let dir = temp_project("welcome-open");
    let file = dir.join("src/main.rs");
    let mut app = App::new(Some(&file)).unwrap();

    // Find and activate the command-line target row.
    let action = app
        .welcome_items()
        .into_iter()
        .find(|item| matches!(item.action, WelcomeAction::OpenTarget(_)))
        .expect("a target action");
    app.activate_welcome(action.action);
    assert_eq!(app.editor.len(), 1);
    // Detection runs behind the queued git/tool probes; wait for the result.
    let deadline = Instant::now() + Duration::from_secs(15);
    while app
        .editor
        .active_document()
        .is_some_and(|doc| doc.buffer.language != LanguageId::Rust)
        && Instant::now() < deadline
    {
        app.pump_background(Duration::from_millis(20));
    }
    assert_eq!(
        app.editor.active_document().unwrap().buffer.language,
        LanguageId::Rust
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resume_restores_the_saved_session() {
    let dir = temp_project("welcome-resume");
    let file = dir.join("src/main.rs");
    let app = app_with_file(&file);
    app.save_session();

    let mut fresh = App::new(Some(&file)).unwrap();
    assert!(
        fresh.resume_session.is_some(),
        "a saved session should be offered"
    );
    assert!(fresh.editor.is_empty());
    fresh.activate_welcome(WelcomeAction::Resume);
    assert_eq!(fresh.editor.len(), 1);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resume_restores_persistent_undo_history() {
    let dir = temp_project("persistent-undo");
    let file = dir.join("src/main.rs");
    let original = "fn main() {}\n";
    fs::write(&file, original).unwrap();

    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// edit\n");
    app.save();
    app.save_session();

    let mut fresh = App::new(Some(&file)).unwrap();
    fresh.activate_welcome(WelcomeAction::Resume);
    let doc = fresh.editor.active_document_mut().unwrap();
    assert!(doc.can_undo(), "persistent undo should survive the session");
    doc.undo();
    assert_eq!(doc.buffer.text(), original);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn persistent_undo_is_refused_after_external_changes() {
    let dir = temp_project("persistent-undo-stale");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {}\n").unwrap();

    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// edit\n");
    app.save();
    app.save_session();

    fs::write(&file, "fn changed() {}\n").unwrap();
    let mut fresh = App::new(Some(&file)).unwrap();
    fresh.activate_welcome(WelcomeAction::Resume);
    assert!(
        !fresh.editor.active_document().unwrap().can_undo(),
        "history recorded against different bytes must be refused"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn starting_and_quitting_without_engaging_keeps_the_session() {
    let dir = temp_project("welcome-keep");
    let file = dir.join("src/main.rs");

    // Establish a session.
    let engaged = app_with_file(&file);
    engaged.save_session();
    let session_path = crate::session::session_path(engaged.workspace.root()).unwrap();
    let before = Session::load_from(&session_path).expect("a session");

    // A fresh start that quits untouched must not overwrite it.
    let untouched = App::new(Some(&file)).unwrap();
    untouched.save_session();
    assert_eq!(Session::load_from(&session_path), Some(before));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn home_command_returns_to_the_welcome_screen() {
    let dir = temp_project("welcome-home");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    assert!(!app.editor.is_empty());

    app.execute_command(ids::HOME);
    assert!(app.editor.is_empty());
    assert!(app.welcome_active());
    fs::remove_dir_all(&dir).ok();
}

/// Drive the new-project flow up to the language step with `name`.
fn new_project_to_language(app: &mut App, parent: &Path, name: &str) {
    app.execute_command(ids::NEW_PROJECT);
    if let Overlay::NewProject(flow) = &mut app.overlay {
        flow.browser.current = parent.to_path_buf();
        flow.browser.selected = 0;
        flow.browser.refresh();
    }
    // Choose the current folder.
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    for c in name.chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
}

#[test]
fn new_project_flow_creates_and_opens_a_project() {
    let dir = temp_project("new-project");
    let parent = dir.join("projects");
    fs::create_dir_all(&parent).unwrap();
    let mut app = App::new(Some(&dir)).unwrap();

    new_project_to_language(&mut app, &parent, "myapp");
    // Language step defaults to Rust; Enter starts creation.
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.pending_project);

    let deadline = Instant::now() + Duration::from_secs(15);
    while app.pending_project && Instant::now() < deadline {
        app.pump_background(Duration::from_millis(20));
    }
    // Detection runs on the background worker behind the git and tool
    // probes `open_workspace` queues, so wait for the result.
    let deadline = Instant::now() + Duration::from_secs(15);
    while app
        .editor
        .active_document()
        .is_some_and(|doc| doc.buffer.language != LanguageId::Rust)
        && Instant::now() < deadline
    {
        app.pump_background(Duration::from_millis(20));
    }

    let root = parent.join("myapp");
    assert!(root.join("Cargo.toml").is_file(), "project not created");
    assert_eq!(
        app.workspace.root().canonicalize().unwrap(),
        root.canonicalize().unwrap()
    );
    assert_eq!(app.editor.len(), 1, "the entry file should be open");
    assert_eq!(
        app.editor.active_document().unwrap().buffer.language,
        LanguageId::Rust
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn new_project_rejects_invalid_and_existing_names() {
    let dir = temp_project("new-project-errors");
    let parent = dir.join("projects");
    fs::create_dir_all(parent.join("taken")).unwrap();
    let mut app = App::new(Some(&dir)).unwrap();

    // A traversal-style name stays on the name step with an error.
    new_project_to_language(&mut app, &parent, "..");
    assert!(matches!(
        &app.overlay,
        Overlay::NewProject(flow) if flow.step == NewProjectStep::Name && flow.error.is_some()
    ));

    // An existing directory is refused too.
    if let Overlay::NewProject(flow) = &mut app.overlay {
        flow.name.clear();
    }
    for c in "taken".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        &app.overlay,
        Overlay::NewProject(flow) if flow.error.as_deref().is_some_and(|e| e.contains("exists"))
    ));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn new_project_can_be_cancelled_and_stepped_back() {
    let dir = temp_project("new-project-cancel");
    let parent = dir.join("projects");
    fs::create_dir_all(&parent).unwrap();
    let mut app = App::new(Some(&dir)).unwrap();

    new_project_to_language(&mut app, &parent, "cancelme");
    // On the language step, Esc goes back to the name step.
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(matches!(
        &app.overlay,
        Overlay::NewProject(flow) if flow.step == NewProjectStep::Name
    ));
    // Then back to the parent step, then cancel.
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(matches!(
        &app.overlay,
        Overlay::NewProject(flow) if flow.step == NewProjectStep::Parent
    ));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.overlay.is_none());

    // Cancelling left nothing behind.
    assert!(!parent.join("cancelme").exists());
    assert!(!app.pending_project);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn opening_a_project_switches_the_workspace() {
    let dir = temp_project("switch");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    // A second project with its own Cargo.toml and source.
    let other = dir.join("other");
    fs::create_dir_all(other.join("src")).unwrap();
    fs::write(other.join("Cargo.toml"), "[package]\nname = \"other\"\n").unwrap();
    fs::write(other.join("src/lib.rs"), "pub fn other() {}\n").unwrap();

    assert!(app.open_workspace(other.clone()));
    app.pump_background(Duration::from_millis(300));
    assert_eq!(
        app.workspace.root().canonicalize().unwrap(),
        other.canonicalize().unwrap()
    );
    assert!(
        app.editor.is_empty(),
        "the editor resets for the new project"
    );
    assert!(app.welcome_active());
    fs::remove_dir_all(&dir).ok();
}

// ---- Project setup -------------------------------------------------------

use crate::language::setup::{LanguageSetup, ProjectSetupPlan, ProjectSetupRun, SetupState};

fn ready_plan(language: LanguageId) -> ProjectSetupPlan {
    ProjectSetupPlan {
        languages: vec![LanguageSetup {
            language,
            state: SetupState::Ready,
            detail: "built-in support — no server needed".to_string(),
        }],
    }
}

#[test]
fn project_setup_is_skipped_when_every_language_is_ready() {
    let dir = temp_project("setup-ready");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.tools = Some(ToolRegistry::discover_cached());
    // Markdown has no server: nothing to install, nothing to flag.
    app.project_languages = Some(vec![LanguageId::Markdown]);
    app.project_setup_offered = false;
    app.overlay = Overlay::None;

    assert!(!app.maybe_offer_project_setup());
    assert!(app.project_setup_offered, "the offer must be recorded once");
    assert!(app.overlay.is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn project_setup_reports_when_nothing_is_detected() {
    let dir = temp_project("setup-empty");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.tools = Some(ToolRegistry::discover_cached());
    app.project_languages = Some(Vec::new());
    app.overlay = Overlay::None;

    app.open_project_setup();
    assert!(app.overlay.is_none());
    assert!(app
        .status_message()
        .is_some_and(|message| message.contains("No project languages")));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn starting_setup_with_nothing_missing_reports_already_set_up() {
    let dir = temp_project("setup-nothing");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.tools = Some(ToolRegistry::discover_cached());
    app.project_languages = Some(vec![LanguageId::Markdown]);

    app.start_project_setup();
    assert!(app.project_setup.is_none());
    assert!(app
        .status_message()
        .is_some_and(|message| message.contains("already set up")));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn finishing_setup_reports_installed_and_failed_tools() {
    let dir = temp_project("setup-finish");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.project_setup = Some(ProjectSetupRun {
        plan: ready_plan(LanguageId::Markdown),
        queue: std::collections::VecDeque::new(),
        installed: vec![Tool::LuaLs],
        failed: Vec::new(),
    });
    app.advance_project_setup();
    assert!(app.project_setup.is_none(), "the run is finished");
    assert!(
        app.status_message()
            .is_some_and(|message| message.contains("1 installed")),
        "got {:?}",
        app.status_message()
    );

    // A failed tool is reported honestly rather than as success.
    app.project_setup = Some(ProjectSetupRun {
        plan: ready_plan(LanguageId::Markdown),
        queue: std::collections::VecDeque::new(),
        installed: Vec::new(),
        failed: vec![(Tool::LuaLs, "download failed".to_string())],
    });
    app.advance_project_setup();
    assert!(
        app.status_message()
            .is_some_and(|message| message.contains("1 failed")),
        "got {:?}",
        app.status_message()
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_single_language_offer_is_suppressed_for_project_languages() {
    let dir = temp_project("setup-suppress");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.tools = Some(ToolRegistry::discover_cached());
    app.project_languages = Some(vec![LanguageId::Rust]);
    app.overlay = Overlay::None;
    app.lsp.clear();

    // The project flow owns Rust, so the per-file offer stays quiet.
    assert!(!app.maybe_offer_tool_setup());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_setup_overlay_lists_languages_and_offers_set_up() {
    let dir = temp_project("setup-overlay");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.show_project_setup(ProjectSetupPlan {
        languages: vec![
            LanguageSetup {
                language: LanguageId::Rust,
                state: SetupState::Ready,
                detail: "rust-analyzer is available".to_string(),
            },
            LanguageSetup {
                language: LanguageId::Go,
                state: SetupState::NeedsInstall(Tool::Gopls),
                detail: "Kemori installs gopls".to_string(),
            },
            LanguageSetup {
                language: LanguageId::Php,
                state: SetupState::Prerequisite("php".to_string()),
                detail: "phpactor needs a PHP runtime".to_string(),
            },
        ],
    });

    let picker = match &app.overlay {
        Overlay::Picker(picker) => picker,
        _ => panic!("expected a setup picker"),
    };
    assert_eq!(picker.title, "Project setup");
    let mut labels = Vec::new();
    let mut has_set_up = false;
    for index in 0..picker.filtered.len() {
        if let Some(item) = picker.item(index) {
            labels.push(item.label.clone());
            if matches!(item.action, PickerAction::ProjectSetup) {
                has_set_up = true;
            }
        }
    }
    assert!(
        labels.iter().any(|label| label.contains("Rust")),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|label| label.contains("Go · install")),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|label| label.contains("PHP · needs php")),
        "{labels:?}"
    );
    assert!(has_set_up, "a missing tool must offer Set up project");
    fs::remove_dir_all(&dir).ok();
}

/// Move the Settings selection to `row` directly, for deterministic setup.
fn select_setting(app: &mut App, row: SettingsRow) {
    match &mut app.overlay {
        Overlay::Settings(state) => {
            state.selected = SETTINGS_ROWS
                .iter()
                .position(|candidate| *candidate == row)
                .expect("row is in the settings list");
        }
        _ => panic!("the settings screen is not open"),
    }
}

#[test]
fn settings_screen_opens_navigates_and_closes() {
    let dir = temp_project("settings-open");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.execute_command(ids::SETTINGS);
    let Overlay::Settings(state) = &app.overlay else {
        panic!("settings did not open");
    };
    assert_eq!(
        state.selected_row(),
        SettingsRow::Theme,
        "first actionable row"
    );

    // Down skips the section header; up skips it back.
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let Overlay::Settings(state) = &app.overlay else {
        panic!("settings did not stay open");
    };
    assert_eq!(
        state.selected_row(),
        SettingsRow::SoftWrap,
        "skips the Editor header"
    );
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    let Overlay::Settings(state) = &app.overlay else {
        panic!("settings did not stay open");
    };
    assert_eq!(state.selected_row(), SettingsRow::Animations);

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.overlay.is_none(), "Esc leaves the settings screen");

    crate::ui::theme::set_theme(ThemeId::Mellow);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn changing_the_theme_applies_immediately() {
    let dir = temp_project("settings-theme");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    assert_eq!(app.settings.theme, ThemeId::Mellow, "Mellow is the default");

    app.execute_command(ids::SETTINGS);
    select_setting(&mut app, SettingsRow::Theme);
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.settings.theme, ThemeId::Midnight);
    assert_eq!(
        crate::ui::theme::theme_id(),
        ThemeId::Midnight,
        "the theme is applied live"
    );
    assert!(app.settings_dirty, "a change is marked for persistence");

    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.settings.theme, ThemeId::Mellow);

    crate::ui::theme::set_theme(ThemeId::Mellow);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn boolean_and_editor_settings_apply_live() {
    let dir = temp_project("settings-editor");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.execute_command(ids::SETTINGS);
    select_setting(&mut app, SettingsRow::LineNumbers);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(!app.settings.line_numbers);
    assert!(!app.show_line_numbers, "line numbers hide immediately");

    select_setting(&mut app, SettingsRow::SoftWrap);
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(app.settings.soft_wrap);
    assert!(app.wrap, "soft wrap turns on immediately");

    select_setting(&mut app, SettingsRow::AutoCompletion);
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert!(!app.settings.auto_completion);
    app.schedule_auto_completion();
    assert!(
        app.completion_due.is_none(),
        "automatic completion is suppressed"
    );

    select_setting(&mut app, SettingsRow::Animations);
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert!(!app.settings.motion);
    assert!(!app.motion, "animations stop immediately");

    select_setting(&mut app, SettingsRow::InlineDiagnostics);
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert!(!app.settings.inline_diagnostics);
    assert!(
        !app.inline_diagnostics,
        "inline diagnostics hide immediately"
    );

    select_setting(&mut app, SettingsRow::FormatOnSave);
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert!(app.settings.format_on_save);
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert!(!app.settings.format_on_save);

    crate::ui::theme::set_theme(ThemeId::Mellow);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn indentation_preference_applies_to_open_documents() {
    let dir = temp_project("settings-indent");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.execute_command(ids::SETTINGS);
    // Cycle Indent width: Auto -> 2 -> 4 -> 8 -> Auto. Pin 2.
    select_setting(&mut app, SettingsRow::IndentWidth);
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.settings.indent_width, Some(2));
    assert_eq!(app.editor.active_document().unwrap().indent_width(), 2);

    // With spaces off, indentation inserts a tab.
    select_setting(&mut app, SettingsRow::UseSpaces);
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert!(!app.settings.use_spaces);
    let doc = app.editor.active_document_mut().unwrap();
    doc.move_to(Position::new(0, 0));
    doc.indent();
    assert!(doc.buffer.line_text(0).starts_with('\t'));

    crate::ui::theme::set_theme(ThemeId::Mellow);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn reset_restores_defaults() {
    let dir = temp_project("settings-reset");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.execute_command(ids::SETTINGS);
    select_setting(&mut app, SettingsRow::Theme);
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    select_setting(&mut app, SettingsRow::LineNumbers);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_ne!(app.settings, Settings::default());

    select_setting(&mut app, SettingsRow::Reset);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.settings, Settings::default());
    assert_eq!(crate::ui::theme::theme_id(), ThemeId::Mellow);
    assert!(app.show_line_numbers);
    assert!(!app.wrap);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn new_with_settings_applies_preferences() {
    let dir = temp_project("settings-load");
    let file = dir.join("src/main.rs");
    let settings = Settings {
        theme: ThemeId::Midnight,
        line_numbers: false,
        soft_wrap: true,
        indent_width: Some(2),
        use_spaces: false,
        auto_completion: false,
        format_on_save: true,
        motion: false,
        inline_diagnostics: false,
    };
    let mut app = App::new_with_settings(Some(&dir), settings).unwrap();
    assert_eq!(crate::ui::theme::theme_id(), ThemeId::Midnight);
    assert!(!app.show_line_numbers);
    assert!(app.wrap);
    assert!(app.settings.format_on_save);
    assert!(!app.motion);
    assert!(!app.inline_diagnostics);

    app.open_path(file);
    assert_eq!(app.editor.active_document().unwrap().indent_width(), 2);

    crate::ui::theme::set_theme(ThemeId::Mellow);
    fs::remove_dir_all(&dir).ok();
}
