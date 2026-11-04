//! The integrated terminal panel.
//!
//! The terminal is a PTY-backed shell (see [`crate::terminal::session`]) drawn
//! in the bottom panel. Keys are encoded to the byte sequences a real terminal
//! sends, so line editors, `vim`, `less` and prompts work. Kemori's own chords
//! stay available: `Ctrl+J` leaves the panel, `Ctrl+Shift+T` starts a session,
//! `Ctrl+Shift+PageUp`/`PageDown` switch panel tabs and `Alt+1…9` switches
//! sessions.

use super::*;
use crate::app::panel::PanelTab;

/// A half page of scrollback for `Shift+PageUp`/`Down`.
const TERMINAL_SCROLL_PAGE: isize = 10;

impl App {
    /// Open the terminal tab, starting a shell when none exists.
    pub(super) fn open_terminal(&mut self) {
        if self.terminals.is_empty() {
            self.ensure_trust(super::trust::TrustIntent::OpenTerminal);
            return;
        }
        self.open_panel(PanelTab::Terminal);
        self.focus = Focus::Panel;
        self.set_status("Terminal — Ctrl+J returns to the editor");
    }

    /// Start a new shell session in the workspace root, gated by trust.
    pub(super) fn start_terminal_session(&mut self) {
        self.ensure_trust(super::trust::TrustIntent::OpenTerminal);
    }

    /// Start the shell after the trust gate has passed.
    pub(super) fn open_terminal_trusted(&mut self) {
        let root = self.workspace.root().to_path_buf();
        let shell = self.terminal_shell.clone();
        match self
            .terminals
            .spawn_shell_with(&root, shell.as_deref(), 80, 24)
        {
            Ok(_) => {
                self.panel.open = true;
                self.panel.tab = PanelTab::Terminal;
                self.focus = Focus::Panel;
                self.set_status(format!(
                    "Terminal in {} — Ctrl+Shift+T for another",
                    root.display()
                ));
            }
            Err(err) => {
                let message = format!("Could not start a shell: {err}");
                self.set_error(message.clone());
                self.push_toast(ToastKind::Error, message);
            }
        }
    }

    /// Close the active session and its process group.
    pub(super) fn close_terminal_session(&mut self) {
        if self.terminals.is_empty() {
            self.set_status("No terminal session is open");
            return;
        }
        self.terminals.close_active();
        if self.terminals.is_empty() {
            self.set_status("Terminal session closed");
        } else {
            self.set_status("Terminal session closed — another is still open");
        }
    }

    /// Switch to the next session.
    pub(super) fn next_terminal_session(&mut self) {
        if self.terminals.is_empty() {
            self.set_status("No terminal session is open");
            return;
        }
        self.terminals.next();
        self.panel.tab = PanelTab::Terminal;
    }

    /// Select a session by zero-based index.
    fn select_terminal_session(&mut self, index: usize) {
        self.terminals.select(index);
        self.panel.tab = PanelTab::Terminal;
        self.focus = Focus::Panel;
    }

    /// Drain PTY output. Returns whether anything changed.
    pub(super) fn poll_terminal(&mut self) -> bool {
        self.terminals.poll()
    }

    /// Whether the terminal owns the keyboard right now.
    pub(super) fn terminal_focused(&self) -> bool {
        self.focus == Focus::Panel
            && self.panel.open
            && self.panel.tab == PanelTab::Terminal
            && self.overlay.is_none()
            && !self.search.open
    }

    /// Handle a key while the terminal owns the keyboard.
    ///
    /// Returns `false` for the handful of Kemori chords that must keep working
    /// (`Ctrl+J`, tab switching, the palette, function keys); every other key is
    /// encoded and sent to the shell.
    pub(super) fn handle_terminal_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key_shift(&key);

        if ctrl && shift {
            match key.code {
                KeyCode::Char(c) => match c.to_ascii_lowercase() {
                    't' => {
                        self.start_terminal_session();
                        return true;
                    }
                    'w' => {
                        self.close_terminal_session();
                        return true;
                    }
                    'c' => {
                        self.terminal_copy();
                        return true;
                    }
                    'v' => {
                        self.terminal_paste();
                        return true;
                    }
                    // The rest are application chords: palette, project search,
                    // save, save-all, run, changed files, git, diagnostics…
                    'p' | 'f' | 's' | 'a' | 'r' | 'g' | 'h' | 'm' | 'i' | 'u' | 'y' => {
                        return false;
                    }
                    _ => {}
                },
                // Ctrl+Shift+PageUp/PageDown switch panel tabs; Ctrl+PageUp/
                // PageDown switch editor tabs, and Ctrl+J leaves the panel.
                KeyCode::PageUp => {
                    self.switch_panel_tab(self.panel.tab.previous());
                    return true;
                }
                KeyCode::PageDown => {
                    self.switch_panel_tab(self.panel.tab.next());
                    return true;
                }
                _ => {}
            }
        }

        if ctrl && !shift {
            match key.code {
                KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Enter => return false,
                KeyCode::PageUp | KeyCode::PageDown => return false,
                KeyCode::Home => {
                    self.terminals.scroll_active(isize::MAX / 4);
                    return true;
                }
                KeyCode::End => {
                    self.terminals.scroll_active_to_bottom();
                    return true;
                }
                _ => {}
            }
        }

        if alt {
            match key.code {
                KeyCode::Up => {
                    self.panel.grow();
                    return true;
                }
                KeyCode::Down => {
                    self.panel.shrink();
                    return true;
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.start_terminal_session();
                    return true;
                }
                KeyCode::Char(c @ '1'..='9') => {
                    let index = (c as u8 - b'1') as usize;
                    self.select_terminal_session(index);
                    return true;
                }
                _ => {}
            }
        }

        if shift && !ctrl && !alt {
            match key.code {
                KeyCode::PageUp => {
                    self.terminals.scroll_active(TERMINAL_SCROLL_PAGE);
                    return true;
                }
                KeyCode::PageDown => {
                    self.terminals.scroll_active(-TERMINAL_SCROLL_PAGE);
                    return true;
                }
                KeyCode::Home => {
                    self.terminals.scroll_active(isize::MAX / 4);
                    return true;
                }
                KeyCode::End => {
                    self.terminals.scroll_active_to_bottom();
                    return true;
                }
                _ => {}
            }
        }

        // Application-level function keys stay available.
        if matches!(
            key.code,
            KeyCode::F(1) | KeyCode::F(5) | KeyCode::F(8) | KeyCode::F(9)
        ) {
            return false;
        }

        if let Some(bytes) = terminal_key_bytes(key) {
            self.terminals.write_active(&bytes);
            self.terminals.scroll_active_to_bottom();
        }
        true
    }

    /// Copy the whole screen and scrollback to the clipboard.
    pub(super) fn terminal_copy(&mut self) {
        let Some(session) = self.terminals.active() else {
            self.set_status("No terminal session is open");
            return;
        };
        let text = session.screen().plain_text();
        let text = text.trim_end().to_string();
        if text.is_empty() {
            self.set_status("Terminal is empty");
            return;
        }
        self.clipboard = text;
        crate::terminal::set_clipboard(&self.clipboard);
        self.set_status("Copied the terminal screen");
    }

    /// Paste Kemori's clipboard into the shell.
    pub(super) fn terminal_paste(&mut self) {
        if self.clipboard.is_empty() {
            self.set_status("Clipboard is empty");
            return;
        }
        let text = self.clipboard.clone();
        self.terminals.write_active(text.as_bytes());
        self.terminals.scroll_active_to_bottom();
    }
}

/// Encode a key as the byte sequence a terminal sends.
fn terminal_key_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key_shift(&key);
    let modifier = if ctrl || alt || shift {
        1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl)
    } else {
        0
    };
    let modified_csi = |final_char: char| {
        if modifier == 0 {
            format!("\x1b[{final_char}")
        } else {
            format!("\x1b[1;{modifier}{final_char}")
        }
    };
    let modified_tilde = |code: u8| {
        if modifier == 0 {
            format!("\x1b[{code}~")
        } else {
            format!("\x1b[{code};{modifier}~")
        }
    };
    let bytes = match key.code {
        KeyCode::Char(c) => {
            if ctrl && !alt {
                return control_byte(c).map(|byte| vec![byte]);
            }
            let mut bytes = Vec::new();
            if alt {
                bytes.push(0x1b);
            }
            let mut buffer = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
            bytes
        }
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Tab => {
            if shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            }
        }
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Insert => modified_tilde(2).into_bytes(),
        KeyCode::Delete => modified_tilde(3).into_bytes(),
        KeyCode::Up => modified_csi('A').into_bytes(),
        KeyCode::Down => modified_csi('B').into_bytes(),
        KeyCode::Right => modified_csi('C').into_bytes(),
        KeyCode::Left => modified_csi('D').into_bytes(),
        KeyCode::Home => modified_csi('H').into_bytes(),
        KeyCode::End => modified_csi('F').into_bytes(),
        KeyCode::PageUp => modified_tilde(5).into_bytes(),
        KeyCode::PageDown => modified_tilde(6).into_bytes(),
        KeyCode::F(number @ 1..=4) => {
            let final_char = ['P', 'Q', 'R', 'S'][(number - 1) as usize];
            format!("\x1bO{final_char}").into_bytes()
        }
        KeyCode::F(number @ 5..=12) => {
            const CODES: [u8; 8] = [15, 17, 18, 19, 20, 21, 23, 24];
            modified_tilde(CODES[(number - 5) as usize]).into_bytes()
        }
        _ => return None,
    };
    Some(bytes)
}

/// The C0 control byte for a `Ctrl` chord, if it has one.
fn control_byte(c: char) -> Option<u8> {
    match c {
        ' ' | '@' => Some(0),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        '?' => Some(0x7f),
        c if c.is_ascii_alphabetic() => Some((c.to_ascii_lowercase() as u8) & 0x1f),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn encodes_common_keys() {
        assert_eq!(
            terminal_key_bytes(key(KeyCode::Enter)),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            terminal_key_bytes(key(KeyCode::Up)),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            terminal_key_bytes(key(KeyCode::F(1))),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(
            terminal_key_bytes(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(vec![0x03])
        );
    }

    #[test]
    fn encodes_modified_arrows() {
        let event = KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(terminal_key_bytes(event), Some(b"\x1b[1;5D".to_vec()));
        let event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
        assert_eq!(terminal_key_bytes(event), Some(b"\x1bx".to_vec()));
    }

    #[test]
    fn untracked_keys_are_ignored() {
        let mut event = KeyEvent::new(KeyCode::Null, KeyModifiers::NONE);
        event.kind = KeyEventKind::Press;
        assert_eq!(terminal_key_bytes(event), None);
    }
}
