//! Language-server response handling, extracted from `lsp.rs`.

use super::*;

impl App {
    /// Apply a language-server feature response for `language`.
    pub(super) fn handle_lsp_response(
        &mut self,
        language: LanguageId,
        kind: RequestKind,
        id: i64,
        result: Result<Value, String>,
    ) {
        let value = match result {
            Ok(value) => value,
            Err(message) => {
                // Clear any loading state this request owned so a timeout or
                // error cannot leave the UI waiting forever.
                self.clear_lsp_pending_for(kind);
                if kind != RequestKind::WorkspaceSymbols {
                    self.set_error(format!("Language server: {message}"));
                }
                return;
            }
        };
        match kind {
            RequestKind::Completion => {
                // Ignore a response that a newer request has superseded: the
                // user typed on, so these candidates no longer match the cursor.
                if self.completion_request != Some((language, id)) {
                    return;
                }
                self.completion_request = None;
                let items = convert::completions(&value);
                if let Some(state) = self.completion.as_mut() {
                    state.extend(items);
                }
                self.resolve_selected_completion();
            }
            RequestKind::CompletionResolve => {
                let current = self.pending_completion_resolve.as_ref().is_some_and(
                    |(pending_language, pending_id, _)| {
                        *pending_language == language && *pending_id == id
                    },
                );
                if !current {
                    return;
                }
                let Some((_, _, label)) = self.pending_completion_resolve.take() else {
                    return;
                };
                let (detail, documentation, edit) =
                    crate::language::lsp::completion::resolved(&value);
                if let Some(state) = self.completion.as_mut() {
                    state.update_item(&label, detail, documentation, edit);
                }
            }
            RequestKind::Hover => {
                if self.hover_request != Some((language, id)) {
                    return;
                }
                self.hover_request = None;
                match convert::hover(&value) {
                    Some(hover) => {
                        self.hover = Some(HoverState {
                            title: hover.title,
                            kind: hover.kind,
                            body: hover.body,
                        });
                    }
                    None if self.hover.is_none() => {
                        self.set_status("No information available");
                    }
                    None => {}
                }
            }
            RequestKind::Definition | RequestKind::Implementation | RequestKind::TypeDefinition => {
                let Some(request) = self.pending_definition.take() else {
                    return;
                };
                if !request.is_current(self, language, id)
                    || !self.active_document_is(&request.path)
                {
                    self.set_status("The document changed; run the lookup again");
                    return;
                }
                let locations = convert::locations(&value);
                match locations.into_iter().next() {
                    Some(location) => {
                        self.reveal_lsp(location.path, Position::new(location.line, location.col));
                    }
                    None => self.set_status(match kind {
                        RequestKind::Implementation => "No implementation found",
                        RequestKind::TypeDefinition => "No type definition found",
                        _ => "No definition found",
                    }),
                }
            }
            RequestKind::References => {
                let Some(request) = self.pending_references.take() else {
                    return;
                };
                if !request.is_current(self, language, id)
                    || !self.active_document_is(&request.path)
                {
                    self.set_status("The document changed; find references again");
                    return;
                }
                let locations = convert::locations(&value);
                if locations.is_empty() {
                    self.set_status("No references found");
                } else {
                    self.open_location_picker("References", locations);
                }
            }
            RequestKind::Rename => {
                let Some(request) = self.pending_rename_request.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    self.set_status("The document changed; rename again");
                    return;
                }
                let files = convert::workspace_edit(&value);
                let applied = self.apply_workspace_edit(files, language);
                if applied > 0 {
                    self.set_status(format!("Renamed in {applied} place(s)"));
                } else {
                    self.set_status("Nothing to rename");
                }
            }
            RequestKind::OrganizeImports => {
                let current = self
                    .pending_organize_imports
                    .as_ref()
                    .is_some_and(|request| request.language == language && request.id == id);
                if !current {
                    return;
                }
                let Some(request) = self.pending_organize_imports.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    self.set_status("The document changed; organize imports again");
                    return;
                }
                let mut actions = convert::code_actions(&value);
                if actions.is_empty() {
                    self.set_status("No import changes");
                    return;
                }
                let action = actions.remove(0);
                let mut applied = 0;
                if let Some(edit) = action.edit {
                    applied = self.apply_workspace_edit(convert::workspace_edit(&edit), language);
                }
                let ran = if let Some(command) = action.command {
                    if let Some(server) = self.active_server_mut() {
                        server.execute_command(&command.command, command.arguments);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if applied > 0 {
                    self.after_edit();
                    self.set_status("Organized imports");
                } else if ran {
                    self.set_status("Organized imports");
                } else {
                    self.set_status("No import changes");
                }
            }
            RequestKind::RangeFormatting => {
                let current = self
                    .pending_range_format
                    .as_ref()
                    .is_some_and(|request| request.language == language && request.id == id);
                if !current {
                    return;
                }
                let Some(request) = self.pending_range_format.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    self.set_status("The document changed; format the selection again");
                    return;
                }
                let edits = convert::formatting_edits(&value);
                if edits.is_empty() {
                    self.set_status("No formatting changes");
                    return;
                }
                let applied = self.apply_workspace_edit(
                    vec![convert::FileEdit {
                        path: request.path.clone(),
                        edits,
                    }],
                    language,
                );
                if applied > 0 {
                    self.after_edit();
                    self.set_status("Formatted selection");
                } else {
                    self.set_status("Nothing to format");
                }
            }
            RequestKind::CodeActions => {
                let Some(request) = self.pending_code_action_request.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    self.set_status("The document changed; run code actions again");
                    return;
                }
                let actions = convert::code_actions(&value);
                if actions.is_empty() {
                    self.set_status("No code actions available");
                    return;
                }
                self.pending_code_actions_context = Some((request.path.clone(), request.version));
                self.pending_code_actions = actions;
                let items = self
                    .pending_code_actions
                    .iter()
                    .enumerate()
                    .map(|(index, action)| {
                        PickerItem::new(
                            action.title.clone(),
                            String::new(),
                            PickerAction::ApplyCodeAction(index),
                        )
                    })
                    .collect();
                let mut picker = Picker::new("Code Actions", "Filter actions…", items);
                picker.refilter();
                self.overlay = Overlay::Picker(picker);
            }
            RequestKind::WorkspaceSymbols => {
                self.ws_lsp_pending = false;
                let root = self.workspace.root().to_path_buf();
                let items: Vec<PickerItem> = convert::workspace_symbols(&value)
                    .into_iter()
                    .take(2000)
                    .map(|item| {
                        let relative = item
                            .path
                            .strip_prefix(&root)
                            .unwrap_or(&item.path)
                            .display()
                            .to_string();
                        let detail =
                            format!("{}  ·  {relative}:{}", item.kind.label(), item.line + 1);
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
                if !items.is_empty() {
                    self.merge_workspace_symbols(items);
                }
            }
            RequestKind::SignatureHelp => {
                if self.signature_request != Some((language, id)) {
                    return;
                }
                self.signature_request = None;
                self.signature = convert::signature_help(&value).map(|help| SignatureState {
                    help,
                    anchor: self.cursor_screen,
                });
            }
            RequestKind::Formatting => {
                // Compare before taking, so a stale response does not cancel
                // the newer request that is still in flight.
                let current = self
                    .pending_lsp_format
                    .as_ref()
                    .map(|(_, pending_language, pending_id, _)| (*pending_language, *pending_id));
                if current != Some((language, id)) {
                    return;
                }
                let Some((path, _, _, version)) = self.pending_lsp_format.take() else {
                    return;
                };
                // A formatting response computed against older text must not be
                // applied to the current buffer.
                if self.document_version(&path) != Some(version) {
                    self.cancel_format_save(&path);
                    self.set_status("Formatting response was stale; try again");
                    return;
                }
                let edits = convert::formatting_edits(&value);
                if edits.is_empty() {
                    self.cancel_format_save(&path);
                    self.set_status("No formatting changes");
                    return;
                }
                let applied = self.apply_workspace_edit(
                    vec![convert::FileEdit {
                        path: path.clone(),
                        edits,
                    }],
                    language,
                );
                if applied > 0 {
                    self.after_edit();
                    self.set_status("Formatted");
                    self.save_after_format(&path);
                } else {
                    self.cancel_format_save(&path);
                    self.set_status("Nothing to format");
                }
            }
            RequestKind::FoldingRange => {
                let Some(request) = self.pending_folding.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    return;
                }
                let ranges = crate::language::lsp::folding::ranges(&value);
                if let Some(doc) = self
                    .editor
                    .documents
                    .iter_mut()
                    .find(|doc| same_file(doc.buffer.path.as_deref(), &request.path))
                {
                    doc.set_server_folds(ranges);
                }
            }
            RequestKind::DocumentHighlight => {
                let current = self
                    .pending_highlights
                    .as_ref()
                    .is_some_and(|request| request.language == language && request.id == id);
                if !current {
                    return;
                }
                let Some(request) = self.pending_highlights.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    return;
                }
                let ranges = convert::document_highlights(&value);
                let encoding = self.lsp_encoding(language);
                if let Some(doc) = self
                    .editor
                    .documents
                    .iter_mut()
                    .find(|doc| same_file(doc.buffer.path.as_deref(), &request.path))
                {
                    let highlights = ranges
                        .into_iter()
                        .map(|((start_line, start_col), (end_line, end_col))| {
                            (
                                Position::new(
                                    start_line,
                                    convert::lsp_to_char(
                                        &doc.buffer.line_text(start_line),
                                        start_col,
                                        encoding,
                                    ),
                                ),
                                Position::new(
                                    end_line,
                                    convert::lsp_to_char(
                                        &doc.buffer.line_text(end_line),
                                        end_col,
                                        encoding,
                                    ),
                                ),
                            )
                        })
                        .collect();
                    doc.set_document_highlights(highlights);
                }
            }
            RequestKind::InlayHint => {
                let current = self
                    .pending_inlay_hints
                    .as_ref()
                    .is_some_and(|request| request.language == language && request.id == id);
                if !current {
                    return;
                }
                let Some(request) = self.pending_inlay_hints.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    return;
                }
                self.apply_inlay_hints(language, &request.path, request.version, &value);
            }
            RequestKind::CodeLens => {
                let current = self
                    .pending_code_lens
                    .as_ref()
                    .is_some_and(|request| request.language == language && request.id == id);
                if !current {
                    return;
                }
                let Some(request) = self.pending_code_lens.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    return;
                }
                self.apply_code_lenses(language, &request.path, request.version, &value);
            }
            RequestKind::SelectionRange => {
                let current = self
                    .pending_selection_ranges
                    .as_ref()
                    .is_some_and(|(request, _)| request.language == language && request.id == id);
                if !current {
                    return;
                }
                let Some((request, cursor)) = self.pending_selection_ranges.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    return;
                }
                if self
                    .editor
                    .active_document()
                    .is_none_or(|doc| doc.clamped_cursor() != cursor)
                {
                    return;
                }
                self.apply_selection_ranges(
                    language,
                    &request.path,
                    request.version,
                    cursor,
                    &value,
                );
            }
            RequestKind::PrepareCallHierarchy | RequestKind::PrepareTypeHierarchy => {
                let current = self
                    .pending_hierarchy_prepare
                    .as_ref()
                    .is_some_and(|(request, _)| request.language == language && request.id == id);
                if !current {
                    return;
                }
                let Some((request, goal)) = self.pending_hierarchy_prepare.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    return;
                }
                let items = crate::language::lsp::hierarchy::items(&value);
                self.handle_prepared_hierarchy(goal, items);
            }
            RequestKind::CallHierarchyIncoming
            | RequestKind::CallHierarchyOutgoing
            | RequestKind::TypeHierarchySuper
            | RequestKind::TypeHierarchySub => {
                let current = self
                    .pending_hierarchy
                    .as_ref()
                    .is_some_and(|request| request.language == language && request.id == id);
                if !current {
                    return;
                }
                let Some(request) = self.pending_hierarchy.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    return;
                }
                let (title, empty, items) = match kind {
                    RequestKind::CallHierarchyIncoming => (
                        "Incoming Calls",
                        "No incoming calls",
                        crate::language::lsp::hierarchy::calls(&value, true),
                    ),
                    RequestKind::CallHierarchyOutgoing => (
                        "Outgoing Calls",
                        "No outgoing calls",
                        crate::language::lsp::hierarchy::calls(&value, false),
                    ),
                    RequestKind::TypeHierarchySuper => (
                        "Supertypes",
                        "No supertypes",
                        crate::language::lsp::hierarchy::items(&value),
                    ),
                    _ => (
                        "Subtypes",
                        "No subtypes",
                        crate::language::lsp::hierarchy::items(&value),
                    ),
                };
                self.open_hierarchy_picker(title, empty, items);
            }
            RequestKind::DocumentLink => {
                let current = self
                    .pending_document_links
                    .as_ref()
                    .is_some_and(|request| request.language == language && request.id == id);
                if !current {
                    return;
                }
                let Some(request) = self.pending_document_links.take() else {
                    return;
                };
                if !request.is_current(self, language, id) {
                    return;
                }
                self.show_document_links(&request.path, &value);
            }
            RequestKind::DocumentDiagnostics => {
                let Some((path, version)) =
                    self.pending_document_diagnostics.remove(&(language, id))
                else {
                    return;
                };
                if self.document_version(&path) != Some(version) {
                    // The buffer moved on; the server will answer the newer
                    // request instead.
                    return;
                }
                if let Some(diagnostics) = crate::language::lsp::parse_document_diagnostics(&value)
                {
                    self.apply_lsp_diagnostics(&path, diagnostics);
                }
            }
            RequestKind::WorkspaceDiagnostics => {
                if self
                    .pending_workspace_diagnostics
                    .remove(&(language, id))
                    .is_none()
                {
                    return;
                }
                self.apply_workspace_lsp_diagnostics(language, &value);
            }
            RequestKind::ExecuteCommand => self.handle_java_debug_response(id, &value),
        }
    }
}
