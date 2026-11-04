#[test]
fn column_selection_keys_extend_and_type() {
    let dir = temp_project("column-selection");
    let file = dir.join("src/notes.txt");
    fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();
    let mut app = app_with_file(&file);
    app.editor
        .active_document_mut()
        .unwrap()
        .move_to(Position::new(0, 2));

    let chord = KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT;
    app.handle_key(KeyEvent::new(KeyCode::Right, chord));
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));

    let doc = app.editor.active_document().unwrap();
    assert_eq!(doc.column_bounds(), Some((0, 2, 2, 3)));
    assert_eq!(doc.selected_text().as_deref(), Some("p\nt\nm"));

    app.handle_key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE));
    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "alXha\nbeXa\ngaXma\n"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn find_in_selection_scopes_matches_and_alt_s_widens() {
    let dir = temp_project("find-in-selection");
    let file = dir.join("src/notes.txt");
    fs::write(&file, "foo zero\nfoo one\nfoo two\nfoo three\n").unwrap();
    let mut app = app_with_file(&file);
    {
        let doc = app.editor.active_document_mut().unwrap();
        doc.selection = Some(Selection::new(Position::new(1, 0)));
        doc.cursor = Position::new(3, 0);
    }

    app.execute_command(ids::FIND_IN_SELECTION);
    assert!(app.search.open);
    assert_eq!(
        app.search.scope,
        Some((Position::new(1, 0), Position::new(3, 0)))
    );

    app.search.query = "foo".to_string();
    app.refresh_search_matches();
    assert_eq!(
        app.search.matches,
        vec![
            (Position::new(1, 0), Position::new(1, 3)),
            (Position::new(2, 0), Position::new(2, 3)),
        ]
    );

    // Alt+S releases the scope and finds the matches outside it too.
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
    assert_eq!(app.search.scope, None);
    assert_eq!(app.search.matches.len(), 4);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn replace_in_selection_leaves_outside_matches_alone() {
    let dir = temp_project("replace-in-selection");
    let file = dir.join("src/notes.txt");
    fs::write(&file, "foo zero\nfoo one\nfoo two\nfoo three\n").unwrap();
    let mut app = app_with_file(&file);
    {
        let doc = app.editor.active_document_mut().unwrap();
        doc.selection = Some(Selection::new(Position::new(1, 0)));
        doc.cursor = Position::new(3, 0);
    }

    app.execute_command(ids::REPLACE_IN_SELECTION);
    app.search.query = "foo".to_string();
    app.search.replacement = "barbar".to_string();
    app.replace_all();

    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "foo zero\nbarbar one\nbarbar two\nfoo three\n"
    );
    assert_eq!(
        app.search.scope,
        Some((Position::new(1, 0), Position::new(3, 0))),
        "the scope end should follow the longer replacements"
    );
    assert!(app.search.matches.is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn replace_preview_lists_changes_and_applies_on_enter() {
    let dir = temp_project("replace-preview");
    let file = dir.join("src/notes.txt");
    fs::write(&file, "foo one\nfoo two\noutside\n").unwrap();
    let mut app = app_with_file(&file);

    app.execute_command(ids::REPLACE);
    app.search.query = "foo".to_string();
    app.search.replacement = "bar".to_string();
    app.refresh_search_matches();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT));

    match &app.overlay {
        Overlay::ReplacePreview(preview) => {
            assert_eq!(preview.matches, 2);
            assert_eq!(preview.rows.len(), 2);
            assert_eq!(preview.rows[0].before, "foo one");
            assert_eq!(preview.rows[0].after, "bar one");
        }
        _ => panic!("expected the replace preview"),
    }
    // Preview must not edit the buffer.
    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "foo one\nfoo two\noutside\n"
    );

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.overlay.is_none());
    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "bar one\nbar two\noutside\n"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn large_files_use_the_reduced_feature_set() {
    let dir = temp_project("large-file");
    let file = dir.join("src/big.txt");
    let text = format!(
        "{}\n",
        "x".repeat(crate::editor::document::LARGE_FILE_CHARS + 8)
    );
    fs::write(&file, text).unwrap();
    let mut app = app_with_file(&file);

    assert!(app.editor.active_document().unwrap().is_large_file());
    assert!(app.lsp_language().is_none(), "LSP sync is skipped");

    app.schedule_diagnostics();
    assert!(
        app.diagnostics_dirty_at.is_none(),
        "large files must not queue diagnostics"
    );

    app.execute_command(ids::COMPLETE);
    assert!(app.completion.is_none(), "completion is limited");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn read_only_mode_blocks_editing_and_saving() {
    let dir = temp_project("read-only");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    let before = app.editor.active_document().unwrap().buffer.text();

    app.execute_command(ids::TOGGLE_READ_ONLY);
    assert!(app.editor.active_document().unwrap().is_user_read_only());
    assert!(app.status.message.contains("Read-only mode on"));

    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        before,
        "typing must not change a read-only document"
    );
    assert!(app.status.error);
    assert!(app.status.message.contains("Read-only mode"));

    app.save();
    assert!(app.status.error);
    assert!(app.status.message.contains("read-only mode"));

    app.execute_command(ids::TOGGLE_READ_ONLY);
    assert!(!app.editor.active_document().unwrap().is_user_read_only());
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    assert!(app
        .editor
        .active_document()
        .unwrap()
        .buffer
        .text()
        .starts_with('x'));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn snippet_insertion_and_tab_stops_work() {
    let dir = temp_project("snippets");
    let file = dir.join("src/main.rs");
    let mut app = app_with_file(&file);
    app.editor.active_document_mut().unwrap().select_all();

    app.execute_command(ids::SNIPPET);
    let action = match &app.overlay {
        Overlay::Picker(picker) => picker.selected_item().expect("a snippet").action.clone(),
        _ => panic!("expected the snippet picker"),
    };
    app.run_picker_action(action);
    app.overlay = Overlay::None;
    assert_eq!(
        app.editor.active_document().unwrap().buffer.text(),
        "fn name() {\n    \n}"
    );
    assert_eq!(
        app.editor.active_document().unwrap().clamped_cursor(),
        Position::new(0, 3)
    );
    assert!(app.snippet.is_some(), "the snippet should have tab stops");
    let stops = app.snippet.as_ref().unwrap().stops.clone();
    assert_eq!(stops[1], Position::new(0, 8), "stops: {stops:?}");

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert!(app.snippet.is_some(), "snippet state survived Tab");
    assert_eq!(
        app.editor.active_document().unwrap().clamped_cursor(),
        Position::new(0, 8)
    );
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(
        app.editor.active_document().unwrap().clamped_cursor(),
        Position::new(1, 4)
    );
    fs::remove_dir_all(&dir).ok();
}
