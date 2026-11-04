//! Inlay-hint and code-lens requests and their response conversion.

use super::*;
use crate::editor::{CodeLens, InlayHint, InlayHintKind};
use crate::language::lsp::hierarchy::HierarchyItem;

impl App {
    /// Request inlay hints for the active document when the text changed.
    ///
    /// Hints depend on text, not the cursor, so the stored version prevents a
    /// request on every arrow key. The whole document is requested because the
    /// server decides which hints are worth returning.
    pub(super) fn request_inlay_hints(&mut self) {
        let active = self.editor.active_document().and_then(|doc| {
            if doc.is_large_file() {
                return None;
            }
            Some((
                doc.buffer.path.clone()?,
                doc.buffer.language,
                doc.buffer.version,
            ))
        });
        let Some((path, language, version)) = active else {
            return;
        };
        let already_current = self
            .editor
            .documents
            .iter()
            .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
            .is_some_and(|doc| doc.inlay_hints_version() == Some(version));
        if already_current {
            return;
        }
        if !self.lsp_supports(RequestKind::InlayHint) {
            if let Some(doc) = self
                .editor
                .documents
                .iter_mut()
                .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
            {
                doc.clear_inlay_hints();
            }
            return;
        }
        let (end_line, end_col) = self
            .editor
            .documents
            .iter()
            .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
            .map(|doc| {
                let last = doc.buffer.len_lines().saturating_sub(1);
                (last, doc.buffer.line_char_len(last))
            })
            .unwrap_or((0, 0));
        let id = self
            .lsp
            .get_mut(&(self.workspace.root().to_path_buf(), language))
            .and_then(|job| job.server.as_mut())
            .and_then(|server| server.inlay_hints(&path, end_line, end_col));
        match id {
            Some(id) => {
                self.pending_inlay_hints = Some(PendingDocRequest {
                    language,
                    id,
                    path,
                    version,
                });
            }
            None => {
                if let Some(doc) = self
                    .editor
                    .documents
                    .iter_mut()
                    .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
                {
                    doc.clear_inlay_hints();
                }
            }
        }
    }

    /// Apply an inlay-hint response, converting positions to character columns.
    pub(super) fn apply_inlay_hints(
        &mut self,
        language: LanguageId,
        path: &Path,
        version: u64,
        value: &Value,
    ) {
        let encoding = self.lsp_encoding(language);
        let Some(doc) = self
            .editor
            .documents
            .iter_mut()
            .find(|doc| same_file(doc.buffer.path.as_deref(), path))
        else {
            return;
        };
        let hints = crate::language::lsp::inlay::hints(value)
            .into_iter()
            .map(|hint| {
                let mut label = hint.label;
                if hint.padding_left {
                    label.insert(0, ' ');
                }
                if hint.padding_right {
                    label.push(' ');
                }
                InlayHint {
                    position: Position::new(
                        hint.line,
                        convert::lsp_to_char(&doc.buffer.line_text(hint.line), hint.col, encoding),
                    ),
                    label,
                    kind: match hint.kind {
                        Some(1) => InlayHintKind::Type,
                        Some(2) => InlayHintKind::Parameter,
                        _ => InlayHintKind::Other,
                    },
                    padding_left: hint.padding_left,
                    padding_right: hint.padding_right,
                }
            })
            .collect();
        doc.set_inlay_hints(version, hints);
    }

    /// Request code lenses for the active document when the text changed.
    pub(super) fn request_code_lenses(&mut self) {
        let active = self.editor.active_document().and_then(|doc| {
            if doc.is_large_file() {
                return None;
            }
            Some((
                doc.buffer.path.clone()?,
                doc.buffer.language,
                doc.buffer.version,
            ))
        });
        let Some((path, language, version)) = active else {
            return;
        };
        let already_current = self
            .editor
            .documents
            .iter()
            .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
            .is_some_and(|doc| doc.code_lenses_version() == Some(version));
        if already_current {
            return;
        }
        if !self.lsp_supports(RequestKind::CodeLens) {
            if let Some(doc) = self
                .editor
                .documents
                .iter_mut()
                .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
            {
                doc.clear_code_lenses();
            }
            return;
        }
        let id = self
            .lsp
            .get_mut(&(self.workspace.root().to_path_buf(), language))
            .and_then(|job| job.server.as_mut())
            .and_then(|server| server.code_lenses(&path));
        match id {
            Some(id) => {
                self.pending_code_lens = Some(PendingDocRequest {
                    language,
                    id,
                    path,
                    version,
                });
            }
            None => {
                if let Some(doc) = self
                    .editor
                    .documents
                    .iter_mut()
                    .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
                {
                    doc.clear_code_lenses();
                }
            }
        }
    }

    /// Apply a code-lens response, converting positions to character columns.
    pub(super) fn apply_code_lenses(
        &mut self,
        language: LanguageId,
        path: &Path,
        version: u64,
        value: &Value,
    ) {
        let encoding = self.lsp_encoding(language);
        let Some(doc) = self
            .editor
            .documents
            .iter_mut()
            .find(|doc| same_file(doc.buffer.path.as_deref(), path))
        else {
            return;
        };
        let lenses = crate::language::lsp::code_lens::lenses(value)
            .into_iter()
            .map(|lens| CodeLens {
                position: Position::new(
                    lens.start_line,
                    convert::lsp_to_char(
                        &doc.buffer.line_text(lens.start_line),
                        lens.start_col,
                        encoding,
                    ),
                ),
                end: Position::new(
                    lens.end_line,
                    convert::lsp_to_char(
                        &doc.buffer.line_text(lens.end_line),
                        lens.end_col,
                        encoding,
                    ),
                ),
                title: lens.title,
                command: lens.command,
                arguments: lens.arguments,
            })
            .collect();
        doc.set_code_lenses(version, lenses);
    }

    /// Offer the active document's code lenses in a picker.
    pub(super) fn open_code_lens_picker(&mut self) {
        let Some(doc) = self.editor.active_document() else {
            self.set_status("No file open");
            return;
        };
        if doc.code_lenses().is_empty() {
            self.set_status("No code lenses — attach a language server that provides them");
            return;
        }
        let items = doc
            .code_lenses()
            .iter()
            .enumerate()
            .map(|(index, lens)| {
                PickerItem::new(
                    lens.title.clone(),
                    format!("line {}", lens.position.row + 1),
                    PickerAction::RunCodeLens(index),
                )
            })
            .collect();
        let mut picker = Picker::new("Code Lens", "Run a code lens…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Execute the command carried by a code lens.
    pub(super) fn run_code_lens(&mut self, index: usize) {
        let Some(lens) = self
            .editor
            .active_document()
            .and_then(|doc| doc.code_lenses().get(index))
            .cloned()
        else {
            return;
        };
        let Some(command) = lens.command.clone() else {
            self.set_status("This code lens needs resolving, which is not supported yet");
            return;
        };
        let arguments = lens
            .arguments
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .unwrap_or(Value::Null);
        let Some(server) = self.active_server_mut() else {
            self.set_status("No ready language server for this document");
            return;
        };
        server.execute_command(&command, arguments);
        self.set_status(format!("Running code lens: {}", lens.title));
    }

    /// Request the selection-range chain for the active caret.
    ///
    /// Skipped while a selection exists: the chain belongs to the caret the
    /// user started expanding from, and replacing it mid-expansion would make
    /// repeated Expand Selection jump unpredictably.
    pub(super) fn request_selection_ranges(&mut self) {
        let active = self.editor.active_document().and_then(|doc| {
            if doc.is_large_file() || doc.has_selection() {
                return None;
            }
            Some((
                doc.buffer.path.clone()?,
                doc.buffer.language,
                doc.buffer.version,
                doc.clamped_cursor(),
            ))
        });
        let Some((path, language, version, cursor)) = active else {
            return;
        };
        let key = (version, cursor);
        let already_current = self
            .editor
            .documents
            .iter()
            .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
            .is_some_and(|doc| doc.selection_ranges_key() == Some(key));
        if already_current {
            return;
        }
        if !self.lsp_supports(RequestKind::SelectionRange) {
            if let Some(doc) = self
                .editor
                .documents
                .iter_mut()
                .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
            {
                doc.clear_selection_ranges();
            }
            return;
        }
        let col = self.lsp_col(language, &path, cursor.row, cursor.col);
        let id = self
            .lsp
            .get_mut(&(self.workspace.root().to_path_buf(), language))
            .and_then(|job| job.server.as_mut())
            .and_then(|server| server.selection_ranges(&path, cursor.row, col));
        match id {
            Some(id) => {
                self.pending_selection_ranges = Some((
                    PendingDocRequest {
                        language,
                        id,
                        path,
                        version,
                    },
                    cursor,
                ));
            }
            None => {
                if let Some(doc) = self
                    .editor
                    .documents
                    .iter_mut()
                    .find(|doc| same_file(doc.buffer.path.as_deref(), &path))
                {
                    doc.clear_selection_ranges();
                }
            }
        }
    }

    /// Apply a selection-range response for the cursor it was requested for.
    pub(super) fn apply_selection_ranges(
        &mut self,
        language: LanguageId,
        path: &Path,
        version: u64,
        cursor: Position,
        value: &Value,
    ) {
        let encoding = self.lsp_encoding(language);
        let Some(doc) = self
            .editor
            .documents
            .iter_mut()
            .find(|doc| same_file(doc.buffer.path.as_deref(), path))
        else {
            return;
        };
        let ranges = crate::language::lsp::selection::ranges(value)
            .into_iter()
            .map(|range| {
                (
                    Position::new(
                        range.start_line,
                        convert::lsp_to_char(
                            &doc.buffer.line_text(range.start_line),
                            range.start_col,
                            encoding,
                        ),
                    ),
                    Position::new(
                        range.end_line,
                        convert::lsp_to_char(
                            &doc.buffer.line_text(range.end_line),
                            range.end_col,
                            encoding,
                        ),
                    ),
                )
            })
            .collect();
        doc.set_selection_ranges(version, cursor, ranges);
    }

    /// Expand the selection to the next enclosing server range.
    pub(super) fn expand_selection(&mut self) {
        let Some(doc) = self.editor.active_document() else {
            return;
        };
        let ranges = doc.selection_ranges().to_vec();
        if ranges.is_empty() {
            self.set_status("No selection ranges — attach a language server that provides them");
            return;
        }
        let (start, end) = doc.selection_range().unwrap_or((doc.cursor, doc.cursor));
        let next = ranges
            .into_iter()
            .find(|(s, e)| *s <= start && *e >= end && (*s < start || *e > end));
        let Some((start, end)) = next else {
            self.set_status("Already at the outermost selection");
            return;
        };
        self.with_doc(|doc| {
            doc.selection = Some(Selection::new(start));
            doc.cursor = end;
            doc.preferred_col = None;
        });
    }

    /// Shrink the selection to the largest server range inside it.
    pub(super) fn shrink_selection(&mut self) {
        let Some(doc) = self.editor.active_document() else {
            return;
        };
        let ranges = doc.selection_ranges().to_vec();
        let Some((start, end)) = doc.selection_range() else {
            self.set_status("Nothing to shrink");
            return;
        };
        let smaller = ranges
            .into_iter()
            .rev()
            .find(|(s, e)| *s >= start && *e <= end && (*s > start || *e < end));
        let Some((start, end)) = smaller else {
            self.set_status("Already at the smallest selection");
            return;
        };
        self.with_doc(|doc| {
            doc.selection = Some(Selection::new(start));
            doc.cursor = end;
            doc.preferred_col = None;
        });
    }

    /// The position encoding the server for `language` negotiated, or the
    /// protocol default when there is no server.
    pub(super) fn lsp_encoding(&self, language: LanguageId) -> PositionEncoding {
        self.lsp
            .get(&(self.workspace.root().to_path_buf(), language))
            .and_then(|job| job.server.as_ref())
            .map(Server::position_encoding)
            .unwrap_or_default()
    }

    /// Prepare an incoming-call hierarchy at the cursor.
    pub(super) fn incoming_calls(&mut self) {
        self.prepare_hierarchy(HierarchyGoal::Incoming);
    }

    /// Prepare an outgoing-call hierarchy at the cursor.
    pub(super) fn outgoing_calls(&mut self) {
        self.prepare_hierarchy(HierarchyGoal::Outgoing);
    }

    /// Prepare a supertype hierarchy at the cursor.
    pub(super) fn supertypes(&mut self) {
        self.prepare_hierarchy(HierarchyGoal::Super);
    }

    /// Prepare a subtype hierarchy at the cursor.
    pub(super) fn subtypes(&mut self) {
        self.prepare_hierarchy(HierarchyGoal::Sub);
    }

    fn prepare_hierarchy(&mut self, goal: HierarchyGoal) {
        let Some(doc) = self.editor.active_document() else {
            self.set_status("No file open");
            return;
        };
        let Some(path) = doc.buffer.path.clone() else {
            self.set_status("Save the file before asking for a hierarchy");
            return;
        };
        let language = doc.buffer.language;
        let cursor = doc.clamped_cursor();
        let version = doc.buffer.version;
        let kind = match goal {
            HierarchyGoal::Incoming | HierarchyGoal::Outgoing => RequestKind::PrepareCallHierarchy,
            HierarchyGoal::Super | HierarchyGoal::Sub => RequestKind::PrepareTypeHierarchy,
        };
        if !self.lsp_supports(kind) {
            self.set_status("This language server does not provide that hierarchy");
            return;
        }
        let col = self.lsp_col(language, &path, cursor.row, cursor.col);
        let id = self
            .lsp
            .get_mut(&(self.workspace.root().to_path_buf(), language))
            .and_then(|job| job.server.as_mut())
            .and_then(|server| match goal {
                HierarchyGoal::Incoming | HierarchyGoal::Outgoing => {
                    server.prepare_call_hierarchy(&path, cursor.row, col)
                }
                HierarchyGoal::Super | HierarchyGoal::Sub => {
                    server.prepare_type_hierarchy(&path, cursor.row, col)
                }
            });
        match id {
            Some(id) => {
                self.pending_hierarchy_prepare = Some((
                    PendingDocRequest {
                        language,
                        id,
                        path,
                        version,
                    },
                    goal,
                ));
            }
            None => self.set_status("The language server did not accept the hierarchy request"),
        }
    }

    /// Continue after the server prepared one or more hierarchy nodes.
    pub(super) fn handle_prepared_hierarchy(
        &mut self,
        goal: HierarchyGoal,
        items: Vec<HierarchyItem>,
    ) {
        if items.is_empty() {
            self.set_status("No symbol at the cursor for a hierarchy");
            return;
        }
        if let [item] = items.as_slice() {
            let item = item.clone();
            self.request_hierarchy_followup(item, goal);
            return;
        }
        self.hierarchy_items = items;
        self.hierarchy_goal = Some(goal);
        let title = match goal {
            HierarchyGoal::Incoming => "Incoming Calls: choose symbol",
            HierarchyGoal::Outgoing => "Outgoing Calls: choose symbol",
            HierarchyGoal::Super => "Supertypes: choose symbol",
            HierarchyGoal::Sub => "Subtypes: choose symbol",
        };
        let items = self
            .hierarchy_items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                PickerItem::new(
                    item.name.clone(),
                    format!("{}:{}", item.path.display(), item.line + 1),
                    PickerAction::HierarchyItem(index),
                )
            })
            .collect();
        let mut picker = Picker::new(title, "Filter symbols…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Use the user's prepared-symbol choice to request its hierarchy.
    pub(super) fn resolve_hierarchy_item(&mut self, index: usize) {
        let Some(goal) = self.hierarchy_goal else {
            return;
        };
        let Some(item) = self.hierarchy_items.get(index).cloned() else {
            return;
        };
        self.request_hierarchy_followup(item, goal);
    }

    fn request_hierarchy_followup(&mut self, item: HierarchyItem, goal: HierarchyGoal) {
        let Some(doc) = self.editor.active_document() else {
            return;
        };
        let Some(path) = doc.buffer.path.clone() else {
            return;
        };
        let language = doc.buffer.language;
        let version = doc.buffer.version;
        let id = self
            .lsp
            .get_mut(&(self.workspace.root().to_path_buf(), language))
            .and_then(|job| job.server.as_mut())
            .and_then(|server| match goal {
                HierarchyGoal::Incoming => server.call_hierarchy(&item.raw, true),
                HierarchyGoal::Outgoing => server.call_hierarchy(&item.raw, false),
                HierarchyGoal::Super => server.type_hierarchy(&item.raw, true),
                HierarchyGoal::Sub => server.type_hierarchy(&item.raw, false),
            });
        match id {
            Some(id) => {
                self.pending_hierarchy = Some(PendingDocRequest {
                    language,
                    id,
                    path,
                    version,
                });
            }
            None => self.set_status("The language server did not answer the hierarchy request"),
        }
    }

    /// Show hierarchy results as a jumpable picker.
    pub(super) fn open_hierarchy_picker(
        &mut self,
        title: &str,
        empty: &str,
        items: Vec<HierarchyItem>,
    ) {
        if items.is_empty() {
            self.set_status(empty);
            return;
        }
        let items = items
            .into_iter()
            .map(|item| {
                let detail = if item.detail.is_empty() {
                    format!("{}:{}", item.path.display(), item.line + 1)
                } else {
                    format!(
                        "{}  ·  {}:{}",
                        item.detail,
                        item.path.display(),
                        item.line + 1
                    )
                };
                PickerItem::new(
                    item.name,
                    detail,
                    PickerAction::RevealLsp {
                        path: item.path,
                        position: Position::new(item.line, item.col),
                    },
                )
            })
            .collect();
        let mut picker = Picker::new(title, "Filter results…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Ask the active document's server for document links.
    pub(super) fn open_document_links(&mut self) {
        let Some(doc) = self.editor.active_document() else {
            self.set_status("No file open");
            return;
        };
        let Some(path) = doc.buffer.path.clone() else {
            self.set_status("Save the file before asking for document links");
            return;
        };
        let language = doc.buffer.language;
        let version = doc.buffer.version;
        if !self.lsp_supports(RequestKind::DocumentLink) {
            self.set_status("This language server does not provide document links");
            return;
        }
        let id = self
            .lsp
            .get_mut(&(self.workspace.root().to_path_buf(), language))
            .and_then(|job| job.server.as_mut())
            .and_then(|server| server.document_links(&path));
        match id {
            Some(id) => {
                self.pending_document_links = Some(PendingDocRequest {
                    language,
                    id,
                    path,
                    version,
                });
                self.set_status("Loading document links…");
            }
            None => self.set_status("The language server did not accept the document-link request"),
        }
    }

    /// Turn a document-link response into a jumpable picker.
    pub(super) fn show_document_links(&mut self, path: &Path, value: &Value) {
        let links = crate::language::lsp::links::links(value);
        if links.is_empty() {
            self.set_status("No document links");
            return;
        }
        let encoding = self.lsp_encoding(
            self.editor
                .documents
                .iter()
                .find(|doc| same_file(doc.buffer.path.as_deref(), path))
                .map(|doc| doc.buffer.language)
                .unwrap_or(LanguageId::Unknown),
        );
        let items = links
            .into_iter()
            .map(|link| {
                let line = link.line;
                let col = self
                    .editor
                    .documents
                    .iter()
                    .find(|doc| same_file(doc.buffer.path.as_deref(), path))
                    .map(|doc| {
                        convert::lsp_to_char(&doc.buffer.line_text(line), link.start_col, encoding)
                    })
                    .unwrap_or(link.start_col);
                let label = if link.tooltip.is_empty() {
                    link.target.clone()
                } else {
                    link.tooltip
                };
                let detail = format!("line {}  ·  {}", line + 1, link.target);
                let action = match crate::language::lsp::uri_to_path(&link.target) {
                    Some(path) => PickerAction::Reveal {
                        path,
                        position: Position::new(line, col),
                    },
                    None => PickerAction::OpenLink(link.target),
                };
                PickerItem::new(label, detail, action)
            })
            .collect();
        let mut picker = Picker::new("Document Links", "Filter links…", items);
        picker.refilter();
        self.overlay = Overlay::Picker(picker);
    }

    /// Open a non-file link by copying it to the clipboard.
    pub(super) fn open_link(&mut self, target: String) {
        terminal::set_clipboard(&target);
        self.set_status(format!("Copied link: {target}"));
    }
}
