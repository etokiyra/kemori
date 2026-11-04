//! Snippet picker and tab-stop navigation.
//!
//! Snippets stay out of the language providers: they are plain text templates
//! with lexical placeholders. The picker lists the active language's built-ins,
//! insertion is one undoable edit, and Tab/Shift+Tab move through the remaining
//! stops until the snippet ends or Escape dismisses it.

use super::*;
use crate::editor::snippet::{self, Snippet};

impl App {
    /// Open the picker for the active language's built-in snippets.
    pub(super) fn open_snippet_picker(&mut self) {
        let Some(doc) = self.editor.active_document() else {
            self.set_status("No file open");
            return;
        };
        let language = doc.buffer.language;
        let snippets = snippet::builtin(language);
        if snippets.is_empty() {
            self.set_status(format!("No built-in snippets for {}", language.name()));
            return;
        }
        let items = snippets
            .iter()
            .map(|snippet| {
                PickerItem::new(
                    snippet.label,
                    format!("{} · {}", snippet.prefix, snippet.detail),
                    PickerAction::InsertSnippet(snippet.clone()),
                )
            })
            .collect();
        let mut picker = Picker::new(
            "Snippets",
            format!("Insert a {} snippet…", language.name()),
            items,
        );
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Insert a snippet at the cursor, replacing any selection.
    pub(super) fn insert_snippet(&mut self, snippet: &Snippet) {
        if self.read_only_block() {
            return;
        }
        let index = self.editor.active_index();
        let expanded = snippet::expand(snippet.body);
        let Some(doc) = self.editor.documents.get_mut(index) else {
            return;
        };
        let (start, end) = doc.selection_range().unwrap_or((doc.cursor, doc.cursor));
        doc.replace_range(start, end, &expanded.text);
        let mut stops: Vec<Position> = expanded
            .stops
            .iter()
            .map(|offset| snippet::position_at(start, &expanded.text, *offset))
            .collect();
        let final_stop = snippet::position_at(start, &expanded.text, expanded.final_stop);
        doc.move_to(stops.first().copied().unwrap_or(final_stop));
        self.after_edit();
        // Move to the first stop (already done), and remember all stops plus the
        // final position. With no explicit stops the cursor is simply at the end.
        if !stops.is_empty() {
            stops.push(final_stop);
            self.snippet = Some(SnippetState {
                doc: index,
                stops,
                index: 0,
            });
        } else {
            self.snippet = None;
        }
        self.set_status(format!("Inserted snippet: {}", snippet.label));
    }

    /// Handle Tab/Shift+Tab while a snippet is active. Returns whether the key
    /// was consumed by snippet navigation.
    pub(super) fn handle_snippet_key(&mut self, key: KeyEvent) -> bool {
        let Some(state) = self.snippet.as_ref() else {
            return false;
        };
        if state.doc != self.editor.active_index() {
            self.snippet = None;
            return false;
        }
        match key.code {
            KeyCode::Tab => {
                self.advance_snippet(1);
                true
            }
            KeyCode::BackTab => {
                self.advance_snippet(-1);
                true
            }
            KeyCode::Esc => {
                self.snippet = None;
                true
            }
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown => {
                // Explicit navigation leaves the snippet; Tab no longer hijacks
                // the user's normal indentation.
                self.snippet = None;
                false
            }
            _ => false,
        }
    }

    fn advance_snippet(&mut self, direction: i32) {
        let Some(state) = self.snippet.as_mut() else {
            return;
        };
        let next = state.index as i32 + direction;
        if next < 0 || next as usize >= state.stops.len() {
            self.snippet = None;
            return;
        }
        let target = state.stops[next as usize];
        let index = state.doc;
        state.index = next as usize;
        if let Some(doc) = self.editor.documents.get_mut(index) {
            doc.move_to(target);
        }
    }
}
