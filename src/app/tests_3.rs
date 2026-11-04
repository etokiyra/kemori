#[test]
fn find_references_lists_every_occurrence() {
    let dir = temp_project("references");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {\n    helper();\n}\n\nfn helper() {}\n").unwrap();

    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(1, 4));

    app.execute_command(ids::FIND_REFERENCES);
    let Overlay::Picker(picker) = &app.overlay else {
        panic!("expected the references list");
    };
    assert_eq!(picker.filtered.len(), 2);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn completion_accepts_the_selected_candidate() {
    let dir = temp_project("complete");
    let file = dir.join("src/main.rs");
    fs::write(&file, "let counter = 0;\ncount\n").unwrap();

    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(1, 5));
    app.execute_command(ids::COMPLETE);
    assert!(app.completion.is_some(), "completion should open");

    // Candidates are ordered shortest-first, so move to `counter`.
    if let Some(state) = app.completion.as_mut() {
        state.move_down();
    }
    app.accept_completion();
    assert_eq!(
        app.editor.active_document().unwrap().buffer.line_text(1),
        "counter"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn completion_offers_language_keywords() {
    let dir = temp_project("complete-keywords");
    let file = dir.join("src/main.rs");

    let mut app = app_with_file(&file);
    app.execute_command(ids::COMPLETE);
    let state = app.completion.as_ref().expect("completion should open");
    assert!(state.items.iter().any(|item| item.label == "fn"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn typing_offers_completion_automatically() {
    let dir = temp_project("complete-auto");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {\n    let value = 1;\n}\n").unwrap();

    let mut app = app_with_file(&file);
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    assert!(
        app.completion.is_none(),
        "the popup waits for the typing pause"
    );

    std::thread::sleep(Duration::from_millis(160));
    app.poll_auto_completion();
    let state = app.completion.as_ref().expect("completion should open");
    assert!(
        state.items.iter().any(|item| item.label == "value"),
        "the buffer identifier should be offered"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn alt_shortcuts_toggle_inline_diagnostics_and_diff() {
    let dir = temp_project("alt-keys");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    let before = app.inline_diagnostics;
    app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::ALT));
    assert_ne!(
        app.inline_diagnostics, before,
        "Alt+I toggles inline diagnostics"
    );

    // Alt+D opens a diff; with no repository it explains itself but must not
    // panic or open an overlay.
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT));
    assert!(app.overlay.is_none());
    assert!(
        app.status_message()
            .unwrap_or("")
            .contains("Not a git repository"),
        "status was {:?}",
        app.status_message()
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn signature_help_response_populates_the_popup() {
    let dir = temp_project("signature");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.signature_request = Some((LanguageId::Rust, 7));
    app.handle_lsp_response(
        LanguageId::Rust,
        RequestKind::SignatureHelp,
        7,
        Ok(serde_json::json!({
            "signatures": [{
                "label": "fn add(a: i32, b: i32)",
                "parameters": [{ "label": "a: i32" }, { "label": "b: i32" }],
                "documentation": "Adds two numbers"
            }],
            "activeSignature": 0,
            "activeParameter": 1
        })),
    );
    let signature = app.signature.as_ref().expect("signature state");
    assert_eq!(signature.help.active, 0);
    assert_eq!(signature.help.parameter, 1);
    assert_eq!(
        signature.help.signatures[0].parameters,
        vec!["a: i32", "b: i32"]
    );

    // A superseded response is ignored.
    app.signature_request = Some((LanguageId::Rust, 9));
    app.signature = None;
    app.handle_lsp_response(
        LanguageId::Rust,
        RequestKind::SignatureHelp,
        8,
        Ok(serde_json::json!({ "signatures": [{ "label": "stale" }] })),
    );
    assert!(
        app.signature.is_none(),
        "a stale signature response is dropped"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn lsp_formatting_response_rewrites_the_buffer() {
    let dir = temp_project("lsp-format");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main(){}\n").unwrap();
    let mut app = app_with_file(&file);

    app.pending_lsp_format = Some((file.clone(), LanguageId::Rust, 3, 0));
    app.handle_lsp_response(
        LanguageId::Rust,
        RequestKind::Formatting,
        3,
        Ok(serde_json::json!([
            { "range": { "start": { "line": 0, "character": 9 },
                         "end": { "line": 0, "character": 9 } },
              "newText": " " }
        ])),
    );

    assert!(app.pending_lsp_format.is_none());
    assert_eq!(
        app.editor.active_document().unwrap().buffer.line_text(0),
        "fn main() {}"
    );

    // A superseded formatting response is ignored.
    app.pending_lsp_format = Some((file.clone(), LanguageId::Rust, 5, 0));
    let before = app.editor.active_document().unwrap().buffer.text();
    app.handle_lsp_response(
        LanguageId::Rust,
        RequestKind::Formatting,
        4,
        Ok(serde_json::json!([])),
    );
    assert_eq!(app.editor.active_document().unwrap().buffer.text(), before);
    assert!(app.pending_lsp_format.is_some());
    fs::remove_dir_all(&dir).ok();
}

/// A `WorkspaceEdit` JSON that replaces a range in `path`.
fn replace_edit(path: &Path, start: (u64, u64), end: (u64, u64), text: &str) -> Value {
    let mut changes = serde_json::Map::new();
    changes.insert(
        crate::language::lsp::path_to_uri(path),
        serde_json::json!([
            {
                "range": {
                    "start": { "line": start.0, "character": start.1 },
                    "end": { "line": end.0, "character": end.1 }
                },
                "newText": text
            }
        ]),
    );
    serde_json::json!({ "changes": changes })
}

#[test]
fn workspace_edit_applies_utf8_byte_offsets_at_character_boundaries() {
    let dir = temp_project("lsp-utf8-edit");
    let file = dir.join("src/main.rs");
    // `café` is four characters / five UTF-8 bytes starting at byte 4.
    fs::write(&file, "let café = 1;\n").unwrap();
    let mut app = app_with_file(&file);

    let applied = app.apply_workspace_edit_with_encoding(
        vec![convert::FileEdit {
            path: file.clone(),
            edits: vec![convert::TextEdit {
                start: (0, 4),
                end: (0, 9),
                new_text: "tea".to_string(),
            }],
        }],
        PositionEncoding::Utf8,
    );

    assert_eq!(applied, 1);
    assert_eq!(
        app.editor.active_document().unwrap().buffer.line_text(0),
        "let tea = 1;"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn workspace_edit_applies_utf16_code_unit_offsets() {
    let dir = temp_project("lsp-utf16-edit");
    let file = dir.join("src/main.rs");
    // Same text, but a UTF-16 server counts `café` as four units (4..8).
    fs::write(&file, "let café = 1;\n").unwrap();
    let mut app = app_with_file(&file);

    let applied = app.apply_workspace_edit_with_encoding(
        vec![convert::FileEdit {
            path: file.clone(),
            edits: vec![convert::TextEdit {
                start: (0, 4),
                end: (0, 8),
                new_text: "tea".to_string(),
            }],
        }],
        PositionEncoding::Utf16,
    );

    assert_eq!(applied, 1);
    assert_eq!(
        app.editor.active_document().unwrap().buffer.line_text(0),
        "let tea = 1;"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn stale_rename_response_is_discarded() {
    let dir = temp_project("lsp-stale-rename");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    let version = app.editor.active_document().unwrap().buffer.version;
    app.pending_rename_request = Some(PendingDocRequest {
        language: LanguageId::Rust,
        id: 1,
        path: file.clone(),
        version,
    });
    // The user keeps typing, so the response now describes older text.
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// x\n");
    let before = app.editor.active_document().unwrap().buffer.text();

    app.handle_lsp_response(
        LanguageId::Rust,
        RequestKind::Rename,
        1,
        Ok(replace_edit(&file, (0, 0), (0, 3), "renamed")),
    );

    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        before,
        "a stale rename must not edit the buffer"
    );
    assert!(app.pending_rename_request.is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn stale_formatting_response_is_discarded() {
    let dir = temp_project("lsp-stale-format");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    let stale_version = app.editor.active_document().unwrap().buffer.version;
    app.pending_lsp_format = Some((file.clone(), LanguageId::Rust, 2, stale_version));
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// x\n");
    let before = app.editor.active_document().unwrap().buffer.text();

    app.handle_lsp_response(
        LanguageId::Rust,
        RequestKind::Formatting,
        2,
        Ok(serde_json::json!([
            { "range": { "start": { "line": 0, "character": 0 },
                         "end": { "line": 0, "character": 3 } },
              "newText": "zzz" }
        ])),
    );

    assert_eq!(app.editor.active_document().unwrap().buffer.text(), before);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn diagnostics_for_a_dirty_buffer_are_dropped() {
    let dir = temp_project("lsp-dirty-diagnostics");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    let diagnostic = || {
        Diagnostic::new(
            crate::language::diagnostics::TextPos::new(0, 0),
            crate::language::diagnostics::TextPos::new(0, 1),
            Severity::Error,
            "boom",
        )
    };

    // The buffer changed but the server has not been told yet.
    app.editor.active_document_mut().unwrap().insert_text("x");
    app.apply_lsp_diagnostics(&file, vec![diagnostic()]);
    assert!(
        app.editor
            .active_document()
            .unwrap()
            .diagnostics()
            .is_empty(),
        "diagnostics over unsent text must be dropped"
    );

    // Once the change has been streamed, published diagnostics apply.
    app.editor
        .active_document_mut()
        .unwrap()
        .set_diagnostics_revision(1);
    app.apply_lsp_diagnostics(&file, vec![diagnostic()]);
    assert_eq!(app.editor.active_document().unwrap().diagnostics().len(), 1);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn workspace_pull_diagnostics_reach_unopened_files() {
    let dir = temp_project("lsp-workspace-pull");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {}\n").unwrap();
    let unopened = dir.join("src/other.rs");
    fs::write(&unopened, "fn other() { let x = 1; }\n").unwrap();
    let mut app = app_with_file(&file);

    let uri = crate::language::lsp::path_to_uri(&unopened);
    app.apply_workspace_lsp_diagnostics(
        LanguageId::Rust,
        &serde_json::json!({
            "items": [{
                "kind": "full",
                "uri": uri,
                "items": [{
                    "range": { "start": { "line": 0, "character": 18 },
                               "end": { "line": 0, "character": 19 } },
                    "severity": 1,
                    "message": "unused variable"
                }]
            }]
        }),
    );
    let problem = app
        .panel_problems()
        .into_iter()
        .find(|problem| problem.message == "unused variable")
        .expect("workspace pull diagnostics should reach Problems");
    assert_eq!(problem.path, unopened);
    assert_eq!(problem.severity, Severity::Error);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_timed_out_request_clears_its_pending_state() {
    let dir = temp_project("lsp-timeout-state");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.pending_lsp_format = Some((file.clone(), LanguageId::Rust, 9, 0));
    app.handle_lsp_response(
        LanguageId::Rust,
        RequestKind::Formatting,
        9,
        Err("request timed out".to_string()),
    );

    assert!(app.pending_lsp_format.is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn code_actions_are_not_applied_after_the_document_changes() {
    let dir = temp_project("lsp-stale-actions");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    let version = app.editor.active_document().unwrap().buffer.version;
    app.pending_code_actions_context = Some((file.clone(), version));
    app.pending_code_actions = vec![convert::CodeAction {
        title: "Add mut".to_string(),
        edit: Some(replace_edit(&file, (0, 0), (0, 3), "renamed")),
        command: None,
    }];
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// x\n");
    let before = app.editor.active_document().unwrap().buffer.text();

    app.apply_code_action(0);

    assert_eq!(app.editor.active_document().unwrap().buffer.text(), before);
    assert!(app.pending_code_actions.is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn motion_and_scene_commands_toggle() {
    let dir = temp_project("motion");
    let mut app = App::new(Some(&dir)).unwrap();

    assert!(app.motion, "motion defaults on");
    app.execute_command(ids::TOGGLE_MOTION);
    assert!(!app.motion, "motion toggles off");
    app.execute_command(ids::TOGGLE_MOTION);
    assert!(app.motion, "motion toggles back on");

    let scene = app.welcome_scene;
    app.execute_command(ids::WELCOME_SCENE);
    assert_ne!(app.welcome_scene, scene, "the scene cycles");

    // `v` on the welcome screen cycles too.
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    assert_ne!(app.welcome_scene, scene, "v cycles the scene");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn typing_a_space_dismisses_completion() {
    let dir = temp_project("complete-space");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {\n    let value = 1;\n}\n").unwrap();

    let mut app = app_with_file(&file);
    app.execute_command(ids::COMPLETE);
    assert!(app.completion.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(app.completion.is_none(), "a space should dismiss the popup");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn automatic_completion_is_silent_in_comments() {
    let dir = temp_project("complete-comment");
    let file = dir.join("src/main.rs");
    fs::write(&file, "// a comment with words\nfn main() {}\n").unwrap();

    let mut app = app_with_file(&file);
    let end_of_comment = app
        .editor
        .active_document()
        .unwrap()
        .buffer
        .line_text(0)
        .chars()
        .count();
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(0, end_of_comment));
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

    std::thread::sleep(Duration::from_millis(160));
    app.poll_auto_completion();
    assert!(
        app.completion.is_none(),
        "no popup should appear inside a comment"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn stale_completion_response_is_dropped() {
    let dir = temp_project("complete-stale");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.execute_command(ids::COMPLETE);
    assert!(app.completion.is_some());

    // A newer request is outstanding; the older response must be ignored.
    app.completion_request = Some((LanguageId::Rust, 2));
    app.handle_lsp_response(
        LanguageId::Rust,
        RequestKind::Completion,
        1,
        Ok(serde_json::json!([{ "label": "stale_member" }])),
    );
    assert!(
        !app.completion
            .as_ref()
            .unwrap()
            .items
            .iter()
            .any(|item| item.label == "stale_member"),
        "a superseded response must not be applied"
    );

    app.handle_lsp_response(
        LanguageId::Rust,
        RequestKind::Completion,
        2,
        Ok(serde_json::json!([{ "label": "fresh_member" }])),
    );
    assert!(
        app.completion
            .as_ref()
            .unwrap()
            .items
            .iter()
            .any(|item| item.label == "fresh_member"),
        "the current response should be applied"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn member_access_excludes_buffer_words() {
    let dir = temp_project("complete-member");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {\n    total = counter.\n}\n").unwrap();

    let mut app = app_with_file(&file);
    // Put the cursor right after the `.` on line 1.
    let line = app.editor.active_document().unwrap().buffer.line_text(1);
    let col = line.chars().count();
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(1, col));
    assert!(app.open_completion_inner(false));

    let state = app.completion.as_ref().unwrap();
    assert!(
        state
            .items
            .iter()
            .any(|item| item.label == "fn" || item.label == "let"),
        "language candidates remain available"
    );
    assert!(
        !state.items.iter().any(|item| item.label == "total"),
        "unrelated buffer words must not appear after a member operator"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn formatting_replaces_the_buffer_as_one_edit() {
    let dir = temp_project("format");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main(){let x=1;}\n").unwrap();

    let mut app = app_with_file(&file);
    let before = app.editor.active_document().unwrap().buffer.text();
    app.apply_formatted(&file, "fn main() {\n    let x = 1;\n}\n");
    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "fn main() {\n    let x = 1;\n}\n"
    );
    assert!(app.editor.active_document().unwrap().is_dirty());

    app.with_doc(|doc| doc.undo());
    assert_eq!(app.editor.active_document().unwrap().buffer.text(), before);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn formatting_is_unavailable_for_plain_text() {
    let dir = temp_project("format-plain");
    let file = dir.join("notes.txt");
    fs::write(&file, "hello\n").unwrap();

    let mut app = app_with_file(&file);
    app.format_document();
    assert!(
        app.pending_format.is_none(),
        "plain text has no formatter, so no request should be sent"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn format_document_runs_the_formatter() {
    // A rustup shim can exist without the component installed (a minimal
    // toolchain profile), so require a successful run, not merely a spawn.
    if !std::process::Command::new("rustfmt")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return; // Skip when rustfmt is unavailable.
    }
    let dir = temp_project("format-flow");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main(){let x=1;}\n").unwrap();

    let mut app = app_with_file(&file);
    app.format_document();

    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        app.apply_background_events();
        if app.pending_format.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let text = app.editor.active_document().unwrap().buffer.text();
    assert!(
        text.contains("fn main() {") && text.contains("    let x = 1;"),
        "expected formatted output, got:\n{text}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn attach_command_opens_a_target_prompt() {
    let dir = temp_project("attach-prompt");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);

    app.execute_command(ids::DEBUG_ATTACH);
    match &app.overlay {
        Overlay::Prompt(prompt) => {
            assert_eq!(prompt.kind, PromptKind::DebugAttach);
            assert!(prompt.placeholder.contains("127.0.0.1"));
        }
        _ => panic!("expected the attach prompt"),
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn format_on_save_formats_and_writes_again() {
    if !std::process::Command::new("rustfmt")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return; // Skip when rustfmt is unavailable.
    }
    let dir = temp_project("format-on-save");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main(){let x=1;}\n").unwrap();

    let mut app = app_with_file(&file);
    app.settings.format_on_save = true;
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// edit\n");

    app.save();
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        app.apply_background_events();
        if app.pending_format.is_none() && app.format_save_pending.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let text = fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("fn main() {") && text.contains("    let x = 1;"),
        "the formatted text should have been written back, got:\n{text}"
    );
    assert!(
        !app.editor.active_document().unwrap().is_dirty(),
        "the second save must leave the buffer clean"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn format_on_save_is_off_by_default() {
    let dir = temp_project("format-on-save-off");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main(){let x=1;}\n").unwrap();

    let mut app = app_with_file(&file);
    assert!(!app.settings.format_on_save);
    app.editor
        .active_document_mut()
        .unwrap()
        .insert_text("// edit\n");
    app.save();
    assert!(
        app.pending_format.is_none() && app.format_save_pending.is_none(),
        "format-on-save must not run unless the preference is enabled"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn hover_describes_the_symbol_under_the_cursor() {
    let dir = temp_project("hover");
    let file = dir.join("src/main.rs");
    fs::write(&file, "fn main() {\n    helper();\n}\n\nfn helper() {}\n").unwrap();

    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(1, 4));

    app.execute_command(ids::HOVER);
    let hover = app.hover.as_ref().expect("hover should open");
    assert!(hover.title.contains("helper"), "title was {}", hover.title);
    assert!(hover.body.iter().any(|line| line.contains("occurrence")));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn workspace_symbols_lists_definitions_across_files() {
    let dir = temp_project("workspace-symbols");
    let a = dir.join("src/main.rs");
    let b = dir.join("src/lib.rs");
    fs::write(&b, "pub fn beta() {}\n").unwrap();

    let mut app = app_with_file(&a);
    app.open_workspace_symbols();

    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        app.apply_background_events();
        if !matches!(app.overlay, Overlay::None) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let Overlay::Picker(picker) = &app.overlay else {
        panic!("expected the workspace symbol list");
    };
    let labels: Vec<String> = (0..picker.filtered.len())
        .filter_map(|index| picker.item(index))
        .map(|item| item.label.clone())
        .collect();
    assert!(
        labels.iter().any(|label| label == "beta"),
        "labels: {labels:?}"
    );
    assert!(
        labels.iter().any(|label| label == "main"),
        "labels: {labels:?}"
    );
    fs::remove_dir_all(&dir).ok();
}
