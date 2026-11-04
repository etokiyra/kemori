//! Extracted from `app/mod.rs`: see `docs/DECISIONS.md`.

use super::*;

#[path = "keys_find.rs"]
mod find_keys;
#[path = "keys_overlay.rs"]
mod overlay_keys;

impl App {
    pub(super) fn handle_paste(&mut self, text: &str) {
        match &mut self.overlay {
            Overlay::Prompt(prompt) => {
                prompt.input.push_str(text);
                return;
            }
            Overlay::Picker(picker) => {
                picker.query.push_str(text);
                picker.refilter();
                return;
            }
            Overlay::Help(_) => return,
            Overlay::Settings(_) => return,
            Overlay::DirPicker(_) => return,
            Overlay::Diff(_) => return,
            Overlay::ReplacePreview(_) => return,
            Overlay::ProjectSearch(state) => {
                for ch in text.chars().filter(|ch| *ch != '\n' && *ch != '\r') {
                    state.push_char(ch);
                }
                return;
            }
            Overlay::NewProject(flow) => {
                if flow.step == NewProjectStep::Name {
                    flow.name.push_str(text);
                    flow.error = None;
                }
                return;
            }
            Overlay::None => {}
        }
        if self.terminal_focused() {
            self.terminals.write_active(text.as_bytes());
            return;
        }
        self.completion = None;
        self.signature = None;
        if self.search.open {
            match self.search.field {
                SearchField::Query => self.search.query.push_str(text),
                SearchField::Replacement => self.search.replacement.push_str(text),
            }
            self.refresh_search_matches();
            self.jump_to_first_from_cursor();
            return;
        }
        if self.focus == Focus::Editor {
            if self.read_only_block() {
                return;
            }
            let text = text.to_string();
            self.with_doc(|doc| doc.insert_text(&text));
        }
    }

    /// Shortcuts that work regardless of focus.
    pub(super) fn handle_global_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key_shift(&key);
        if key.code == KeyCode::F(1) {
            self.completion = None;
            self.toggle_help();
            return true;
        }
        if key.code == KeyCode::F(5) {
            self.completion = None;
            if self.debug.session.is_some() {
                if shift {
                    self.execute_command(ids::DEBUG_STOP);
                } else {
                    self.execute_command(ids::DEBUG_CONTINUE);
                }
            } else {
                self.refresh_workspace();
            }
            return true;
        }
        if key.code == KeyCode::F(10) {
            self.completion = None;
            self.execute_command(ids::DEBUG_STEP_OVER);
            return true;
        }
        if key.code == KeyCode::F(11) {
            self.completion = None;
            if shift {
                self.execute_command(ids::DEBUG_STEP_OUT);
            } else {
                self.execute_command(ids::DEBUG_STEP_INTO);
            }
            return true;
        }
        if key.code == KeyCode::F(8) {
            self.completion = None;
            self.goto_diagnostic(if shift { -1 } else { 1 });
            return true;
        }
        if key.code == KeyCode::F(9) {
            self.completion = None;
            self.execute_command(ids::TOGGLE_BREAKPOINT);
            return true;
        }
        if key.code == KeyCode::F(3) {
            // Repeat the last find, reopening the bar when it was closed.
            self.completion = None;
            if !self.search.open {
                self.search.open = true;
                self.refresh_search_matches();
            }
            if shift {
                self.find_previous();
            } else {
                self.find_next();
            }
            return true;
        }
        // Pane and view shortcuts work from any surface.
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Char('v') | KeyCode::Char('V') => {
                    self.execute_command(ids::SPLIT);
                    return true;
                }
                KeyCode::Char('o') | KeyCode::Char('O') => {
                    self.execute_command(ids::FOCUS_PANE);
                    return true;
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    self.execute_command(ids::DIFF);
                    return true;
                }
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    self.execute_command(ids::TOGGLE_INLINE_DIAGNOSTICS);
                    return true;
                }
                KeyCode::Char('z') | KeyCode::Char('Z') => {
                    self.execute_command(ids::TOGGLE_WRAP);
                    return true;
                }
                KeyCode::Char('[') => {
                    self.execute_command(ids::HUNK_PREV);
                    return true;
                }
                KeyCode::Char(']') => {
                    self.execute_command(ids::HUNK_NEXT);
                    return true;
                }
                _ => {}
            }
        }
        if !ctrl {
            return false;
        }
        // Any Ctrl chord other than the completion trigger dismisses completion.
        if key.code != KeyCode::Char(' ') {
            self.completion = None;
        }
        // The bottom panel is contextual like the sidebar: show and focus it,
        // close it when it already owns the keyboard, or take focus when it is
        // merely visible.
        if key.code == KeyCode::Char('j') || key.code == KeyCode::Char('J') {
            self.toggle_panel();
            return true;
        }
        // Some terminals without the kitty keyboard protocol report Ctrl+J as
        // Ctrl+Enter. Only treat that as the panel shortcut when no overlay owns
        // the keyboard, so project search can use Ctrl+Enter to replace.
        if key.code == KeyCode::Enter {
            if self.overlay.is_none() && !self.search.open {
                self.toggle_panel();
                return true;
            }
            return false;
        }
        match key.code {
            KeyCode::Char(c) => {
                match (c.to_ascii_lowercase(), shift) {
                    (' ', _) => self.execute_command(ids::COMPLETE),
                    ('q', _) => self.request_quit(),
                    ('s', true) => self.execute_command(ids::SAVE_AS),
                    ('s', false) => self.execute_command(ids::SAVE),
                    ('n', _) => self.execute_command(ids::NEW_FILE),
                    ('p', true) => self.open_command_palette(),
                    ('p', false) => self.open_quick_open(),
                    ('o', _) => self.execute_command(ids::OPEN),
                    ('f', true) => self.execute_command(ids::PROJECT_SEARCH),
                    ('f', false) => self.open_search(false),
                    ('h', true) => self.open_hover(),
                    ('h', false) => self.open_search(true),
                    ('g', true) => self.execute_command(ids::CHANGED_FILES),
                    ('a', true) => self.execute_command(ids::SAVE_ALL),
                    ('r', true) => self.execute_command(ids::RUN_TASKS),
                    ('c', true) => self.execute_command(ids::GIT_COMMIT),
                    ('u', true) => self.execute_command(ids::GIT_PUSH),
                    ('y', true) => self.execute_command(ids::GIT_PULL),
                    ('g', false) => self.execute_command(ids::GOTO_LINE),
                    ('b', _) => self.toggle_tree(),
                    ('e', _) => self.focus_tree(),
                    ('w', _) => self.execute_command(ids::CLOSE_TAB),
                    ('t', true) => self.open_terminal(),
                    ('t', false) => self.open_workspace_symbols(),
                    ('m', true) => self.execute_command(ids::DIAGNOSTICS_LIST),
                    ('i', true) => self.execute_command(ids::FORMAT),
                    _ => return false,
                }
                true
            }
            KeyCode::Tab => {
                if shift {
                    self.previous_tab();
                } else {
                    self.next_tab();
                }
                true
            }
            KeyCode::PageUp => {
                self.previous_tab();
                true
            }
            KeyCode::PageDown => {
                self.next_tab();
                true
            }
            _ => false,
        }
    }

    /// Handle the key after a `Ctrl+K` fold prefix.
    fn handle_fold_key(&mut self, key: KeyEvent) -> bool {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        match key.code {
            KeyCode::Char('z') => self.execute_command(ids::FOLD_TOGGLE),
            KeyCode::Char('c') => self.execute_command(ids::FOLD_CLOSE),
            KeyCode::Char('o') => self.execute_command(ids::FOLD_OPEN),
            KeyCode::Char('0') => self.execute_command(ids::FOLD_ALL),
            KeyCode::Char('j') => self.execute_command(ids::UNFOLD_ALL),
            KeyCode::Esc => {
                self.set_status("Fold command cancelled");
            }
            _ => self.set_status("Unknown fold command — Ctrl+K z/c/o/0/j"),
        }
        true
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) {
        if self.fold_prefix {
            self.fold_prefix = false;
            if self.handle_fold_key(key) {
                return;
            }
        }
        // The terminal consumes keys before the application keymap, because
        // `Ctrl+C`, arrows and `Esc` belong to the shell while it has focus.
        if self.terminal_focused() && self.handle_terminal_key(key) {
            return;
        }
        // A hover popup is informational; the next key dismisses it.
        if self.hover.is_some() {
            self.hover = None;
            if key.code == KeyCode::Esc {
                return;
            }
        }
        // Signature help persists while literal characters and Backspace keep
        // the call open; any other key dismisses it.
        let keep_signature = matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace)
            && !key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT);
        if !keep_signature {
            self.signature = None;
            self.signature_request = None;
        }
        if self.handle_global_key(key) {
            return;
        }
        if !self.overlay.is_none() {
            self.completion = None;
            self.handle_overlay_key(key);
            return;
        }
        if self.search.open {
            self.completion = None;
            self.handle_search_key(key);
            return;
        }
        if self.completion.is_some() && self.handle_completion_key(key) {
            return;
        }
        match self.focus {
            Focus::FileTree => self.handle_tree_key(key),
            Focus::Panel => self.handle_panel_key(key),
            Focus::Editor if self.editor.is_empty() => self.handle_welcome_key(key),
            Focus::Editor => self.handle_editor_key(key),
        }
    }
    pub(super) fn handle_editor_key(&mut self, key: KeyEvent) {
        if self.handle_snippet_key(key) {
            return;
        }
        let shift = key_shift(&key);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        if self
            .editor
            .active_document()
            .is_some_and(|doc| doc.is_user_read_only())
        {
            let edits = match key.code {
                KeyCode::Enter
                | KeyCode::Tab
                | KeyCode::BackTab
                | KeyCode::Backspace
                | KeyCode::Delete => true,
                KeyCode::Char(c) => {
                    if !ctrl && !alt {
                        true
                    } else if ctrl {
                        matches!(
                            c.to_ascii_lowercase(),
                            'z' | 'y' | 'x' | 'v' | 'd' | 'k' | '/'
                        )
                    } else {
                        matches!(c.to_ascii_lowercase(), 'y')
                    }
                }
                KeyCode::Up | KeyCode::Down => alt && !ctrl,
                _ => false,
            };
            if edits {
                self.set_error("Read-only mode is on — toggle it off to edit");
                return;
            }
        }

        // Signature help is transient: any key that is not a literal character
        // dismisses it.
        let typing = matches!(key.code, KeyCode::Char(_)) && !ctrl && !alt;
        if !typing {
            self.signature = None;
            self.signature_request = None;
        }

        match key.code {
            KeyCode::Char(c) => {
                if ctrl {
                    match c.to_ascii_lowercase() {
                        'z' => {
                            if shift {
                                self.with_doc(|d| d.redo());
                            } else {
                                self.with_doc(|d| d.undo());
                            }
                        }
                        'y' => self.with_doc(|d| d.redo()),
                        'a' => self.with_doc(|d| d.select_all()),
                        'r' if alt => self.toggle_read_only(),
                        'c' => self.copy(),
                        'x' => self.cut(),
                        'v' => self.paste(),
                        'd' if shift => self.with_doc(|d| d.duplicate_line()),
                        'd' => self.with_doc(|d| d.select_next_occurrence()),
                        'l' if shift => self.with_doc(|d| d.select_all_occurrences()),
                        'k' if shift => self.with_doc(|d| d.delete_line()),
                        'k' => self.fold_prefix = true,
                        'm' => self.goto_matching_bracket(),
                        '/' => self.toggle_comment(),
                        '.' if shift => self.open_code_lens_picker(),
                        '.' => self.code_actions(),
                        _ => {}
                    }
                } else if alt {
                    match c {
                        'y' | 'Y' => self.yank_pop(),
                        'm' | 'M' => self.goto_matching_bracket(),
                        _ => {}
                    }
                } else {
                    self.with_doc(|d| d.type_char(c));
                    self.after_typed_char(c);
                }
            }
            KeyCode::Enter => self.with_doc(|d| d.insert_newline()),
            KeyCode::Tab => self.with_doc(|d| d.indent()),
            KeyCode::BackTab => self.with_doc(|d| d.outdent()),
            KeyCode::Backspace => self.with_doc(|d| d.backspace()),
            KeyCode::Delete => self.with_doc(|d| d.delete_forward()),
            KeyCode::Left => {
                if alt && shift && !ctrl {
                    self.shrink_selection();
                } else {
                    self.with_doc(|d| {
                        if ctrl && alt && shift {
                            d.extend_column_left();
                        } else if ctrl {
                            d.move_word_left(shift);
                        } else if shift && d.has_column_selection() {
                            d.extend_column_left();
                        } else {
                            d.move_left(shift);
                        }
                    });
                }
            }
            KeyCode::Right => {
                if alt && shift && !ctrl {
                    self.expand_selection();
                } else {
                    self.with_doc(|d| {
                        if ctrl && alt && shift {
                            d.extend_column_right();
                        } else if ctrl {
                            d.move_word_right(shift);
                        } else if shift && d.has_column_selection() {
                            d.extend_column_right();
                        } else {
                            d.move_right(shift);
                        }
                    });
                }
            }
            KeyCode::Up => {
                let column = self
                    .editor
                    .active_document()
                    .is_some_and(|doc| doc.has_column_selection());
                if ctrl && alt && shift {
                    self.with_doc(|d| d.extend_column_up());
                } else if ctrl && alt {
                    self.with_doc(|d| d.add_cursor_above());
                } else if alt {
                    self.with_doc(|d| d.move_line_up());
                } else if shift && column {
                    self.with_doc(|d| d.extend_column_up());
                } else {
                    self.with_doc(|d| d.move_up(shift));
                }
            }
            KeyCode::Down => {
                let column = self
                    .editor
                    .active_document()
                    .is_some_and(|doc| doc.has_column_selection());
                if ctrl && alt && shift {
                    self.with_doc(|d| d.extend_column_down());
                } else if ctrl && alt {
                    self.with_doc(|d| d.add_cursor_below());
                } else if alt {
                    if shift {
                        self.with_doc(|d| d.duplicate_line());
                    } else {
                        self.with_doc(|d| d.move_line_down());
                    }
                } else if shift && column {
                    self.with_doc(|d| d.extend_column_down());
                } else {
                    self.with_doc(|d| d.move_down(shift));
                }
            }
            KeyCode::Home => self.with_doc(|d| {
                if ctrl {
                    d.move_document_start(shift);
                } else {
                    d.move_home(shift);
                }
            }),
            KeyCode::End => self.with_doc(|d| {
                if ctrl {
                    d.move_document_end(shift);
                } else {
                    d.move_end(shift);
                }
            }),
            KeyCode::PageUp => self.page_move(-1),
            KeyCode::PageDown => self.page_move(1),
            KeyCode::Esc => self.with_doc(|d| d.clear_selection()),
            _ => {}
        }
    }

    pub(super) fn handle_tree_key(&mut self, key: KeyEvent) {
        if self.tree_filter.is_some() {
            self.handle_tree_filter_key(key);
            return;
        }
        match key.code {
            KeyCode::Up => self.workspace.tree.select_up(),
            KeyCode::Down => self.workspace.tree.select_down(),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char(' ') => {
                if let Some(path) = self.workspace.tree.activate_selected() {
                    self.open_path(path);
                }
            }
            KeyCode::Left => self.workspace.tree.collapse_selected(),
            KeyCode::Char('/') => self.open_tree_filter(),
            KeyCode::Char('.') => self.toggle_hidden(),
            KeyCode::Esc => self.focus = Focus::Editor,
            _ => {}
        }
    }

    pub(super) fn handle_tree_filter_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.tree_filter = None,
            KeyCode::Up => {
                if let Some(filter) = self.tree_filter.as_mut() {
                    filter.move_up();
                }
            }
            KeyCode::Down => {
                if let Some(filter) = self.tree_filter.as_mut() {
                    filter.move_down();
                }
            }
            KeyCode::Enter => {
                let path = self
                    .tree_filter
                    .as_ref()
                    .and_then(|filter| filter.selected_path().map(Path::to_path_buf));
                self.tree_filter = None;
                if let Some(path) = path {
                    self.open_path(path);
                }
            }
            KeyCode::Backspace => {
                if let Some(filter) = self.tree_filter.as_mut() {
                    filter.backspace();
                }
            }
            KeyCode::Char(c) if !ctrl => {
                if let Some(filter) = self.tree_filter.as_mut() {
                    filter.push_char(c);
                }
            }
            _ => {}
        }
    }
}
