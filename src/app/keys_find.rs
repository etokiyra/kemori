//! Find/replace and completion key dispatch, extracted from `keys.rs`.

use super::*;

impl App {
    pub(crate) fn handle_search_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if alt {
            match key.code {
                KeyCode::Char(c) => match c.to_ascii_lowercase() {
                    'c' => self.search.case_sensitive = !self.search.case_sensitive,
                    'w' => self.search.whole_word = !self.search.whole_word,
                    'r' => self.search.regex = !self.search.regex,
                    's' => {
                        if self.search.scope.take().is_some() {
                            self.set_status("Searching the whole file");
                        } else {
                            self.set_status("Use Find in Selection to restrict the search range");
                        }
                    }
                    'p' => {
                        self.open_replace_preview();
                        return;
                    }
                    _ => return,
                },
                KeyCode::Enter => {
                    self.replace_all();
                    return;
                }
                _ => return,
            }
            self.refresh_search_matches();
            self.jump_to_first_from_cursor();
            return;
        }
        match key.code {
            KeyCode::Esc => self.search.close(),
            KeyCode::Enter => {
                if self.search.replace_mode && self.search.field == SearchField::Replacement {
                    self.replace_current();
                } else if shift {
                    self.find_previous();
                } else {
                    self.find_next();
                }
            }
            KeyCode::Tab => {
                if self.search.replace_mode {
                    self.search.field = match self.search.field {
                        SearchField::Query => SearchField::Replacement,
                        SearchField::Replacement => SearchField::Query,
                    };
                }
            }
            KeyCode::Up => self.find_previous(),
            KeyCode::Down => self.find_next(),
            KeyCode::Backspace => {
                match self.search.field {
                    SearchField::Query => {
                        self.search.query.pop();
                    }
                    SearchField::Replacement => {
                        self.search.replacement.pop();
                    }
                }
                self.refresh_search_matches();
                self.jump_to_first_from_cursor();
            }
            KeyCode::Char(c) if !ctrl => {
                match self.search.field {
                    SearchField::Query => self.search.query.push(c),
                    SearchField::Replacement => self.search.replacement.push(c),
                }
                self.refresh_search_matches();
                self.jump_to_first_from_cursor();
            }
            _ => {}
        }
    }

    /// Handle a key while the completion popup is open. Returns `false` for keys
    /// the popup does not claim, after dismissing it.
    pub(super) fn handle_completion_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => {
                self.completion = None;
                self.completion_due = None;
                true
            }
            KeyCode::Up => {
                if let Some(state) = self.completion.as_mut() {
                    state.move_up();
                }
                true
            }
            KeyCode::Down => {
                if let Some(state) = self.completion.as_mut() {
                    state.move_down();
                }
                true
            }
            KeyCode::PageUp => {
                if let Some(state) = self.completion.as_mut() {
                    state.move_by(-8);
                }
                true
            }
            KeyCode::PageDown => {
                if let Some(state) = self.completion.as_mut() {
                    state.move_by(8);
                }
                true
            }
            KeyCode::Enter | KeyCode::Tab => {
                self.accept_completion();
                true
            }
            KeyCode::Backspace => {
                self.with_doc(|doc| doc.backspace());
                self.after_completion_edit();
                true
            }
            KeyCode::Char(c) if !ctrl => {
                self.with_doc(|doc| doc.type_char(c));
                self.after_typed_char(c);
                true
            }
            _ => {
                self.completion = None;
                self.completion_due = None;
                false
            }
        }
    }
}
