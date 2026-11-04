//! Overlay key dispatch, extracted from `keys.rs`.

use super::*;

impl App {
    pub(super) fn handle_overlay_key(&mut self, key: KeyEvent) {
        // Settings edits application state, so it is handled directly rather
        // than through the picker `Outcome` plumbing below.
        if matches!(self.overlay, Overlay::Settings(_)) {
            self.handle_settings_key(key);
            return;
        }
        if matches!(self.overlay, Overlay::ProjectSearch(_)) {
            self.handle_project_search_key(key);
            return;
        }
        if matches!(self.overlay, Overlay::ReplacePreview(_)) {
            self.handle_replace_preview_key(key);
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        enum Outcome {
            Nothing,
            Close,
            Run(PickerAction, Option<String>),
            Submit(PromptKind, String),
            Stage(PathBuf, bool),
            OpenDir(PathBuf),
            CreateProject {
                parent: PathBuf,
                name: String,
                language: LanguageId,
            },
        }

        let outcome = match &mut self.overlay {
            Overlay::None => Outcome::Nothing,
            Overlay::Picker(picker) => match key.code {
                KeyCode::Esc => Outcome::Close,
                KeyCode::Up => {
                    picker.move_up();
                    Outcome::Nothing
                }
                KeyCode::Down => {
                    picker.move_down();
                    Outcome::Nothing
                }
                KeyCode::Enter => match picker.selected_item() {
                    Some(item) => Outcome::Run(
                        item.action.clone(),
                        item.hint.clone().filter(|_| !item.enabled),
                    ),
                    None => Outcome::Close,
                },
                KeyCode::Backspace => {
                    picker.backspace();
                    Outcome::Nothing
                }
                KeyCode::Char(' ') if picker.title == "Changed Files" => {
                    match picker.selected_item().map(|item| item.action.clone()) {
                        Some(PickerAction::OpenPath(path)) => {
                            let staged = !self.workspace.git.is_staged(&path);
                            Outcome::Stage(path, staged)
                        }
                        _ => Outcome::Nothing,
                    }
                }
                KeyCode::Char('d') if picker.title == "Changed Files" => {
                    match picker.selected_item().map(|item| item.action.clone()) {
                        Some(PickerAction::OpenPath(path)) => {
                            let staged = self.workspace.git.is_staged(&path);
                            Outcome::Run(PickerAction::ShowDiff { path, staged }, None)
                        }
                        _ => Outcome::Nothing,
                    }
                }
                KeyCode::Char(c) if !ctrl => {
                    picker.push_char(c);
                    Outcome::Nothing
                }
                _ => Outcome::Nothing,
            },
            Overlay::Prompt(prompt) => match key.code {
                KeyCode::Esc => Outcome::Close,
                KeyCode::Enter => Outcome::Submit(prompt.kind, prompt.input.clone()),
                KeyCode::Backspace => {
                    prompt.backspace();
                    Outcome::Nothing
                }
                KeyCode::Char(c) if !ctrl => {
                    prompt.push_char(c);
                    Outcome::Nothing
                }
                _ => Outcome::Nothing,
            },
            Overlay::DirPicker(picker) => match key.code {
                KeyCode::Esc => Outcome::Close,
                KeyCode::Up => {
                    picker.browser.move_up();
                    Outcome::Nothing
                }
                KeyCode::Down => {
                    picker.browser.move_down();
                    Outcome::Nothing
                }
                KeyCode::Backspace | KeyCode::Left => {
                    if let Some(parent) = picker.browser.current.parent() {
                        picker.browser.current = parent.to_path_buf();
                        picker.browser.selected = 0;
                        picker.browser.refresh();
                    }
                    Outcome::Nothing
                }
                KeyCode::Enter => match picker.browser.activate() {
                    Some(directory) => Outcome::OpenDir(directory),
                    None => Outcome::Nothing,
                },
                _ => Outcome::Nothing,
            },
            Overlay::NewProject(flow) => match flow.step {
                NewProjectStep::Parent => match key.code {
                    KeyCode::Esc => Outcome::Close,
                    KeyCode::Up => {
                        flow.browser.move_up();
                        Outcome::Nothing
                    }
                    KeyCode::Down => {
                        flow.browser.move_down();
                        Outcome::Nothing
                    }
                    KeyCode::Backspace | KeyCode::Left => {
                        if let Some(parent) = flow.browser.current.parent() {
                            flow.browser.current = parent.to_path_buf();
                            flow.browser.selected = 0;
                            flow.browser.refresh();
                        }
                        Outcome::Nothing
                    }
                    KeyCode::Enter => {
                        if let Some(directory) = flow.browser.activate() {
                            flow.parent = directory;
                            flow.step = NewProjectStep::Name;
                            flow.error = None;
                        }
                        Outcome::Nothing
                    }
                    _ => Outcome::Nothing,
                },
                NewProjectStep::Name => match key.code {
                    KeyCode::Esc => {
                        flow.step = NewProjectStep::Parent;
                        flow.error = None;
                        Outcome::Nothing
                    }
                    KeyCode::Enter => {
                        match validate_project_name(&flow.parent, &flow.name) {
                            Ok(()) => {
                                flow.name = flow.name.trim().to_string();
                                flow.step = NewProjectStep::Language;
                                flow.error = None;
                            }
                            Err(message) => flow.error = Some(message),
                        }
                        Outcome::Nothing
                    }
                    KeyCode::Backspace => {
                        flow.name.pop();
                        flow.error = None;
                        Outcome::Nothing
                    }
                    KeyCode::Char(c) if !ctrl => {
                        flow.name.push(c);
                        flow.error = None;
                        Outcome::Nothing
                    }
                    _ => Outcome::Nothing,
                },
                NewProjectStep::Language => match key.code {
                    KeyCode::Esc => {
                        flow.step = NewProjectStep::Name;
                        flow.error = None;
                        Outcome::Nothing
                    }
                    KeyCode::Up => {
                        flow.language = flow.language.saturating_sub(1);
                        flow.error = None;
                        Outcome::Nothing
                    }
                    KeyCode::Down => {
                        if flow.language + 1 < create::CREATABLE.len() {
                            flow.language += 1;
                        }
                        flow.error = None;
                        Outcome::Nothing
                    }
                    KeyCode::Enter => {
                        let index = flow.language.min(create::CREATABLE.len().saturating_sub(1));
                        let language = create::CREATABLE[index];
                        match validate_project_name(&flow.parent, &flow.name) {
                            Ok(()) => Outcome::CreateProject {
                                parent: flow.parent.clone(),
                                name: flow.name.trim().to_string(),
                                language,
                            },
                            Err(message) => {
                                flow.error = Some(message);
                                Outcome::Nothing
                            }
                        }
                    }
                    _ => Outcome::Nothing,
                },
            },
            Overlay::Help(help) => match key.code {
                KeyCode::Esc => Outcome::Close,
                KeyCode::Up => {
                    help.scroll = help.scroll.saturating_sub(1);
                    Outcome::Nothing
                }
                KeyCode::Down => {
                    help.scroll = help.scroll.saturating_add(1);
                    Outcome::Nothing
                }
                KeyCode::PageUp => {
                    help.scroll = help.scroll.saturating_sub(8);
                    Outcome::Nothing
                }
                KeyCode::PageDown => {
                    help.scroll = help.scroll.saturating_add(8);
                    Outcome::Nothing
                }
                _ => Outcome::Nothing,
            },
            // Handled before this match; the arm keeps the match exhaustive.
            Overlay::Settings(_) => Outcome::Nothing,
            Overlay::ProjectSearch(_) => Outcome::Nothing,
            Overlay::ReplacePreview(_) => Outcome::Nothing,
            Overlay::Diff(diff) => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => Outcome::Close,
                KeyCode::Up => {
                    diff.scroll_by(-1);
                    Outcome::Nothing
                }
                KeyCode::Down => {
                    diff.scroll_by(1);
                    Outcome::Nothing
                }
                KeyCode::PageUp => {
                    diff.scroll_by(-16);
                    Outcome::Nothing
                }
                KeyCode::PageDown => {
                    diff.scroll_by(16);
                    Outcome::Nothing
                }
                KeyCode::Home => {
                    diff.scroll = 0;
                    Outcome::Nothing
                }
                KeyCode::End => {
                    diff.scroll = diff.lines.len();
                    Outcome::Nothing
                }
                KeyCode::Char('a') => {
                    self.apply_diff_hunk(false);
                    Outcome::Nothing
                }
                KeyCode::Char('r') => {
                    self.apply_diff_hunk(true);
                    Outcome::Nothing
                }
                KeyCode::Char('n') => {
                    diff.next_hunk();
                    Outcome::Nothing
                }
                KeyCode::Char('p') => {
                    diff.prev_hunk();
                    Outcome::Nothing
                }
                _ => Outcome::Nothing,
            },
        };

        match outcome {
            Outcome::Nothing => {}
            Outcome::Close => self.overlay = Overlay::None,
            Outcome::Run(action, hint) => {
                self.overlay = Overlay::None;
                match hint {
                    Some(hint) => self.set_status(hint),
                    None => self.run_picker_action(action),
                }
            }
            Outcome::Submit(kind, input) => {
                self.overlay = Overlay::None;
                self.submit_prompt(kind, input);
            }
            Outcome::Stage(path, staged) => self.dispatch_stage(path, staged, true),
            Outcome::OpenDir(directory) => {
                self.overlay = Overlay::None;
                self.open_project(directory);
            }
            Outcome::CreateProject {
                parent,
                name,
                language,
            } => {
                self.overlay = Overlay::None;
                self.start_project_creation(parent, name, language);
            }
        }
    }
}
