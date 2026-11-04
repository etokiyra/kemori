//! A document: a buffer plus cursor, selection, scroll and undo state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::editor::buffer::{Buffer, LineEnding};
use crate::editor::code_lens::CodeLens;
use crate::editor::history::{Coalesce, Edit, EditOp, History};
use crate::editor::inlay::InlayHint;
use crate::editor::layout::LineLayout;
use crate::editor::position::{Cursor, Position, Selection};
use crate::language::diagnostics::{Diagnostic, Severity};
use crate::language::id::LanguageId;
use crate::language::provider::{HighlightSpan, HighlightState, LanguageProvider, TokenKind};

use helpers::*;

/// The number of columns one level of indentation represents when a file has no
/// discernible style.
pub const INDENT_WIDTH: usize = 4;

/// Files at or above this character count open in large-file mode: syntax
/// highlighting, diagnostics, completion and language-server document sync are
/// skipped so the editor stays responsive. Saving, editing, search and undo are
/// unaffected.
pub const LARGE_FILE_CHARS: usize = 1_000_000;

/// A planned replacement at one cursor, used by multi-cursor edits.
#[derive(Clone, Debug)]
struct MultiEdit {
    start: Position,
    end: Position,
    inserted: String,
}

/// One open file.
pub struct Document {
    pub buffer: Buffer,
    pub cursor: Position,
    pub selection: Option<Selection>,
    /// Additional cursors beyond the primary one. Each carries its own anchor
    /// (and therefore its own selection). Kept sorted by start position and
    /// free of duplicates and of the primary cursor.
    pub cursors: Vec<Cursor>,
    /// Remembered column for vertical movement.
    pub preferred_col: Option<usize>,
    /// First visible line.
    pub scroll_top: usize,
    /// First visible column.
    pub scroll_left: usize,
    /// Which visual row of `scroll_top` is at the top when soft wrap is on.
    pub scroll_subline: usize,
    /// The text width soft wrap uses, in display columns. `0` disables wrapping
    /// and restores horizontal scrolling. Updated by the renderer.
    pub wrap_width: usize,
    history: History,
    highlight_states: Vec<HighlightState>,
    highlight_computed: usize,
    /// Diagnostics from the provider, sorted by position.
    diagnostics: Vec<Diagnostic>,
    /// Revision of the most recent diagnostics request for this document. Used
    /// to drop results that arrive after a newer request.
    diagnostics_revision: u64,
    /// Whether the text changed since diagnostics were last requested.
    diagnostics_dirty: bool,
    /// Whether the current diagnostics came from a language server.
    diagnostics_from_lsp: bool,
    /// The file's modification time when it was last read or written.
    disk_mtime: Option<SystemTime>,
    /// The indentation unit detected for this file, in spaces.
    indent_width: usize,
    /// Whether indentation is inserted as spaces (rather than one tab per level).
    use_spaces: bool,
    /// A user-chosen indent width that overrides per-file inference, if any.
    indent_preferred: Option<usize>,
    /// The history id of the top edit at the last save (or load). Comparing
    /// against it lets undo/redo restore the clean state, so undoing every edit
    /// no longer leaves a file marked modified.
    saved_id: Option<u64>,
    /// Whether the line ending changed since the last save. Kept separately
    /// because it is not a text edit and therefore has no history id.
    line_ending_dirty: bool,
    /// Git change markers for this document's lines (added/modified/removed).
    pub git_markers: Vec<crate::git::GitLineMarker>,
    /// Language-server document highlights for the symbol at the cursor.
    lsp_highlights: Vec<(Position, Position)>,
    /// Language-server inlay hints for this document's text.
    inlay_hints: Vec<InlayHint>,
    /// Buffer version the inlay hints were computed for, if any.
    inlay_hints_version: Option<u64>,
    /// Language-server code lenses for this document.
    code_lenses: Vec<CodeLens>,
    /// Buffer version the code lenses were computed for, if any.
    code_lenses_version: Option<u64>,
    /// Selection-range chain for the cached cursor, innermost first.
    selection_ranges: Vec<(Position, Position)>,
    /// Buffer version and cursor the chain was computed for.
    selection_ranges_key: Option<(u64, Position)>,
    /// Lines that begin a collapsed fold (0-based).
    pub folded: std::collections::BTreeSet<usize>,
    /// Server-published fold ranges: header row → last hidden row, inclusive.
    pub server_folds: std::collections::BTreeMap<usize, usize>,
    /// Breakpoint lines (0-based) for the debug adapter.
    pub breakpoints: std::collections::BTreeSet<usize>,
    /// Conditions attached to breakpoint lines.
    pub breakpoint_conditions: std::collections::BTreeMap<usize, String>,
    /// Log messages for logpoints (the debuggee does not stop).
    pub breakpoint_logs: std::collections::BTreeMap<usize, String>,
}

mod cursors;
mod highlight;
mod movement;
mod typing;

impl Document {
    pub fn new(buffer: Buffer) -> Self {
        // Scanning a huge file for indentation costs a full copy; keep the
        // conventional default instead.
        let indent_width = if buffer.len_chars() >= LARGE_FILE_CHARS {
            INDENT_WIDTH
        } else {
            detect_indent_width(&buffer.text())
        };
        Document {
            buffer,
            cursor: Position::zero(),
            selection: None,
            cursors: Vec::new(),
            preferred_col: None,
            scroll_top: 0,
            scroll_left: 0,
            scroll_subline: 0,
            wrap_width: 0,
            history: History::default(),
            highlight_states: vec![HighlightState::default()],
            highlight_computed: 1,
            diagnostics: Vec::new(),
            diagnostics_revision: 0,
            diagnostics_dirty: false,
            diagnostics_from_lsp: false,
            disk_mtime: None,
            indent_width,
            use_spaces: true,
            indent_preferred: None,
            saved_id: None,
            line_ending_dirty: false,
            git_markers: Vec::new(),
            lsp_highlights: Vec::new(),
            inlay_hints: Vec::new(),
            inlay_hints_version: None,
            code_lenses: Vec::new(),
            code_lenses_version: None,
            selection_ranges: Vec::new(),
            selection_ranges_key: None,
            folded: std::collections::BTreeSet::new(),
            server_folds: std::collections::BTreeMap::new(),
            breakpoints: std::collections::BTreeSet::new(),
            breakpoint_conditions: std::collections::BTreeMap::new(),
            breakpoint_logs: std::collections::BTreeMap::new(),
        }
    }

    /// The indentation unit detected for this file, in spaces.
    pub fn indent_width(&self) -> usize {
        self.indent_width
    }

    /// Apply a user indentation preference.
    ///
    /// `width` of `None` keeps per-file inference (the zero-config default); a
    /// width pins the unit instead. `use_spaces` selects whether indentation is
    /// inserted as spaces or as one tab per level. The preference survives a
    /// reload.
    pub fn set_indent_preference(&mut self, width: Option<usize>, use_spaces: bool) {
        self.indent_preferred = width;
        self.use_spaces = use_spaces;
        self.indent_width = width.map(|width| width.max(1)).unwrap_or_else(|| {
            if self.is_large_file() {
                INDENT_WIDTH
            } else {
                detect_indent_width(&self.buffer.text())
            }
        });
    }

    /// The string inserted for one level of indentation.
    pub(super) fn indent_unit(&self) -> String {
        if self.use_spaces {
            " ".repeat(self.indent_width.max(1))
        } else {
            "\t".to_string()
        }
    }

    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        let mut document = Document::new(Buffer::from_path(path)?);
        document.record_disk_mtime();
        Ok(document)
    }

    pub fn from_path_lossy(path: &Path) -> std::io::Result<Self> {
        let mut document = Document::new(Buffer::from_path_lossy(path)?);
        document.record_disk_mtime();
        Ok(document)
    }

    pub fn is_read_only(&self) -> bool {
        self.buffer.is_read_only()
    }

    /// Whether the file needs UTF-8 conversion before it can be written.
    pub fn is_lossy_read_only(&self) -> bool {
        self.buffer.is_lossy_read_only()
    }

    /// Whether the user enabled read-only mode for this document.
    pub fn is_user_read_only(&self) -> bool {
        self.buffer.is_user_read_only()
    }

    /// Flip user read-only mode and return the new state.
    pub fn toggle_user_read_only(&mut self) -> bool {
        self.buffer.toggle_user_read_only()
    }

    /// Whether this document is large enough to use the reduced feature set.
    pub fn is_large_file(&self) -> bool {
        self.buffer.len_chars() >= LARGE_FILE_CHARS
    }

    pub fn convert_to_utf8(&mut self) {
        self.buffer.convert_to_utf8();
    }

    pub fn file_name(&self) -> String {
        self.buffer.file_name()
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer.dirty || self.line_ending_dirty
    }

    /// Serialize this document's undo history for session persistence.
    ///
    /// Only meaningful for a clean document; callers should skip dirty buffers.
    pub fn history_snapshot(&self) -> serde_json::Value {
        self.history.to_value(self.saved_id, 100)
    }

    /// Restore a history snapshot after verifying the caller checked the text.
    ///
    /// Returns whether a usable history was restored. The buffer is left clean if
    /// the snapshot's save point is at the top of the stack.
    pub fn restore_history(&mut self, value: &serde_json::Value) -> bool {
        let Some((history, saved_id)) = History::from_value(value) else {
            return false;
        };
        self.history = history;
        self.saved_id = saved_id;
        self.refresh_dirty();
        true
    }

    /// The line ending this document will use when it is saved.
    pub fn line_ending(&self) -> LineEnding {
        self.buffer.line_ending
    }

    /// Change the saved line ending without rewriting the rope.
    ///
    /// Returns whether anything changed. The conversion is applied by
    /// [`Buffer::text_with_line_ending`] when the file is written, so undo
    /// offsets stay valid.
    pub fn set_line_ending(&mut self, ending: LineEnding) -> bool {
        if self.buffer.line_ending == ending {
            return false;
        }
        self.buffer.line_ending = ending;
        self.buffer.mark_dirty();
        self.line_ending_dirty = true;
        true
    }

    /// Remove trailing spaces and tabs from every line as one undoable edit.
    pub fn trim_trailing_whitespace(&mut self) -> bool {
        if self.buffer.is_user_read_only() {
            return false;
        }
        let text = self.buffer.text();
        let mut out = String::with_capacity(text.len());
        let mut changed = false;
        let mut lines = text.split('\n').peekable();
        while let Some(line) = lines.next() {
            let (body, had_cr) = match line.strip_suffix('\r') {
                Some(body) => (body, true),
                None => (line, false),
            };
            let trimmed = body.trim_end_matches([' ', '\t']);
            if trimmed.len() != body.len() {
                changed = true;
            }
            out.push_str(trimmed);
            if had_cr {
                out.push('\r');
            }
            if lines.peek().is_some() {
                out.push('\n');
            }
        }
        if !changed {
            return false;
        }
        let end = self.buffer_end();
        self.replace_range(Position::zero(), end, &out);
        true
    }

    /// Add a final newline when the file has none.
    pub fn ensure_final_newline(&mut self) -> bool {
        if self.buffer.is_user_read_only() {
            return false;
        }
        if self.buffer.text().ends_with('\n') {
            return false;
        }
        let end = self.buffer_end();
        self.replace_range(end, end, "\n");
        true
    }

    fn buffer_end(&self) -> Position {
        let row = self.buffer.len_lines().saturating_sub(1);
        Position::new(row, self.buffer.line_char_len(row))
    }

    pub fn set_language(&mut self, language: LanguageId) {
        if self.buffer.language != language {
            self.buffer.language = language;
            self.invalidate_highlight(0);
        }
    }

    pub fn mark_clean(&mut self) {
        self.mark_saved();
    }

    /// Make the current history position the clean/save point.
    pub(super) fn mark_saved(&mut self) {
        // A later edit must not coalesce into the edit that was saved, or the
        // top id (and therefore the clean state) would be unchanged.
        self.history.break_coalesce();
        self.saved_id = self.history.top_id();
        self.line_ending_dirty = false;
        self.buffer.mark_clean();
    }

    /// Recompute dirty state after moving through history. An undo/redo that
    /// returns to the last save point makes the document clean again.
    pub(super) fn refresh_dirty(&mut self) {
        self.buffer.dirty = self.history.top_id() != self.saved_id || self.line_ending_dirty;
    }

    // ----------------------------------------------------------------------
    // Disk state
    // ----------------------------------------------------------------------

    /// Remember the file's current modification time.
    pub fn record_disk_mtime(&mut self) {
        self.disk_mtime = self
            .buffer
            .path
            .as_ref()
            .and_then(|path| crate::filesystem::modified_time(path));
    }

    /// The modification time recorded when the file was last read or written.
    pub fn disk_modified(&self) -> Option<SystemTime> {
        self.disk_mtime
    }

    /// Reload the buffer from disk, discarding history and diagnostics.
    ///
    /// The cursor is preserved (clamped) and the buffer ends up clean.
    pub fn reload_from_disk(&mut self) -> std::io::Result<bool> {
        let Some(path) = self.buffer.path.clone() else {
            return Ok(false);
        };
        let text = crate::editor::buffer::read_text(&path)?;
        let cursor = self.cursor;

        self.buffer.replace_contents(&text);
        self.history.clear();
        self.mark_saved();
        self.indent_width = self
            .indent_preferred
            .unwrap_or_else(|| detect_indent_width(&text));
        self.selection = None;
        self.cursors.clear();
        self.preferred_col = None;
        self.folded.clear();
        self.server_folds.clear();
        self.cursor = self.buffer.clamp_position(cursor);
        self.invalidate_highlight(0);
        self.clear_diagnostics();
        self.diagnostics_dirty = true;
        self.record_disk_mtime();
        Ok(true)
    }

    /// Write the buffer and record the resulting modification time.
    pub fn save(&mut self) -> std::io::Result<bool> {
        let saved = self.buffer.save()?;
        if saved {
            self.mark_saved();
            self.record_disk_mtime();
        }
        Ok(saved)
    }

    /// Write the buffer to `path` and record the modification time.
    pub fn save_as(&mut self, path: &Path) -> std::io::Result<bool> {
        let saved = self.buffer.save_as(path)?;
        if saved {
            self.mark_saved();
            self.record_disk_mtime();
        }
        Ok(saved)
    }

    /// Point the document at a new path after a rename or move on disk.
    pub fn set_path(&mut self, path: PathBuf) {
        self.buffer.path = Some(path);
        self.record_disk_mtime();
    }

    // ----------------------------------------------------------------------
    // Diagnostics
    // ----------------------------------------------------------------------

    /// The diagnostics currently attached to this document.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The revision of the most recent diagnostics request for this document.
    pub fn diagnostics_revision(&self) -> u64 {
        self.diagnostics_revision
    }

    /// Whether the text has changed since diagnostics were last requested.
    pub fn diagnostics_dirty(&self) -> bool {
        self.diagnostics_dirty
    }

    /// Record that a diagnostics request with `revision` was dispatched. The
    /// pending edit, if any, is considered handled.
    pub fn set_diagnostics_revision(&mut self, revision: u64) {
        self.diagnostics_revision = revision;
        self.diagnostics_dirty = false;
    }

    /// Replace the diagnostics if `revision` is still the newest request.
    ///
    /// Results computed from an older snapshot are ignored so markers never
    /// reflect text that has since changed.
    pub fn apply_diagnostics(&mut self, revision: u64, mut diagnostics: Vec<Diagnostic>) -> bool {
        if revision != self.diagnostics_revision {
            return false;
        }
        diagnostics.sort_by_key(|diagnostic| (diagnostic.start.line, diagnostic.start.col));
        self.diagnostics = diagnostics;
        true
    }

    /// Discard diagnostics, e.g. because the document just changed.
    pub fn clear_diagnostics(&mut self) {
        self.diagnostics.clear();
    }

    /// Replace diagnostics with ones published by a language server.
    pub fn set_lsp_diagnostics(&mut self, mut diagnostics: Vec<Diagnostic>) {
        diagnostics.sort_by_key(|diagnostic| (diagnostic.start.line, diagnostic.start.col));
        self.diagnostics = diagnostics;
        self.diagnostics_from_lsp = true;
        self.diagnostics_dirty = false;
    }

    /// Whether the current diagnostics came from a language server.
    pub fn diagnostics_from_lsp(&self) -> bool {
        self.diagnostics_from_lsp
    }

    /// Language-server highlights for the symbol at the cursor.
    pub fn document_highlights(&self) -> &[(Position, Position)] {
        &self.lsp_highlights
    }

    /// Replace the document highlights, usually from a server response.
    pub fn set_document_highlights(&mut self, highlights: Vec<(Position, Position)>) {
        self.lsp_highlights = highlights;
    }

    /// Drop document highlights, e.g. when the cursor leaves a symbol.
    pub fn clear_document_highlights(&mut self) {
        self.lsp_highlights.clear();
    }

    /// Language-server inlay hints for this document.
    pub fn inlay_hints(&self) -> &[InlayHint] {
        &self.inlay_hints
    }

    /// The buffer version the current hints describe, if any.
    pub fn inlay_hints_version(&self) -> Option<u64> {
        self.inlay_hints_version
    }

    /// Replace the inlay hints with a response for `version`.
    pub fn set_inlay_hints(&mut self, version: u64, hints: Vec<InlayHint>) {
        self.inlay_hints = hints;
        self.inlay_hints_version = Some(version);
    }

    /// Drop inlay hints, e.g. because the text changed.
    pub fn clear_inlay_hints(&mut self) {
        self.inlay_hints.clear();
        self.inlay_hints_version = None;
    }

    /// Language-server code lenses for this document.
    pub fn code_lenses(&self) -> &[CodeLens] {
        &self.code_lenses
    }

    /// The buffer version the current lenses describe, if any.
    pub fn code_lenses_version(&self) -> Option<u64> {
        self.code_lenses_version
    }

    /// Replace the code lenses with a response for `version`.
    pub fn set_code_lenses(&mut self, version: u64, lenses: Vec<CodeLens>) {
        self.code_lenses = lenses;
        self.code_lenses_version = Some(version);
    }

    /// Drop code lenses, e.g. because the text changed.
    pub fn clear_code_lenses(&mut self) {
        self.code_lenses.clear();
        self.code_lenses_version = None;
    }

    /// The selection-range chain for the cached cursor, innermost first.
    pub fn selection_ranges(&self) -> &[(Position, Position)] {
        &self.selection_ranges
    }

    /// The buffer version and cursor the chain describes, if any.
    pub fn selection_ranges_key(&self) -> Option<(u64, Position)> {
        self.selection_ranges_key
    }

    /// Replace the selection-range chain for `version` at `cursor`.
    pub fn set_selection_ranges(
        &mut self,
        version: u64,
        cursor: Position,
        ranges: Vec<(Position, Position)>,
    ) {
        self.selection_ranges = ranges;
        self.selection_ranges_key = Some((version, cursor));
    }

    /// Drop the cached selection-range chain.
    pub fn clear_selection_ranges(&mut self) {
        self.selection_ranges.clear();
        self.selection_ranges_key = None;
    }

    /// Hand diagnostic ownership back to the built-in providers, e.g. after a
    /// language server exits.
    pub fn use_builtin_diagnostics(&mut self) {
        if self.diagnostics_from_lsp {
            self.diagnostics_from_lsp = false;
            self.diagnostics.clear();
            self.diagnostics_revision = 0;
        }
    }

    /// Number of `(errors, warnings)`; other severities are not counted.
    pub fn diagnostic_counts(&self) -> (usize, usize) {
        let mut errors = 0;
        let mut warnings = 0;
        for diagnostic in &self.diagnostics {
            match diagnostic.severity {
                Severity::Error => errors += 1,
                Severity::Warning => warnings += 1,
                _ => {}
            }
        }
        (errors, warnings)
    }

    /// The most severe diagnostic that *starts* on `row`, for the gutter marker.
    pub fn diagnostic_severity_on_line(&self, row: usize) -> Option<Severity> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.start.line == row)
            .map(|diagnostic| diagnostic.severity)
            .max()
    }

    /// The git change marker for `row`, if any.
    pub fn git_change_on_line(&self, row: usize) -> Option<crate::git::GitLineChange> {
        self.git_markers
            .iter()
            .find(|marker| marker.line == row)
            .map(|marker| marker.change)
    }

    /// Replace this document's git change markers.
    pub fn set_git_markers(&mut self, markers: Vec<crate::git::GitLineMarker>) {
        self.git_markers = markers;
    }

    pub fn toggle_breakpoint(&mut self) {
        let row = self.clamped_cursor().row;
        if self.breakpoints.contains(&row) {
            self.breakpoints.remove(&row);
            self.breakpoint_conditions.remove(&row);
            self.breakpoint_logs.remove(&row);
        } else {
            self.breakpoints.insert(row);
        }
    }

    pub fn has_breakpoint(&self, row: usize) -> bool {
        self.breakpoints.contains(&row)
    }

    /// Whether a breakpoint on `row` has a condition or log message.
    pub fn breakpoint_is_conditional(&self, row: usize) -> bool {
        self.breakpoint_conditions.contains_key(&row) || self.breakpoint_logs.contains_key(&row)
    }

    /// Attach (or clear, with `None`) a condition to the breakpoint on `row`.
    pub fn set_breakpoint_condition(&mut self, row: usize, condition: Option<String>) {
        self.breakpoints.insert(row);
        match condition.filter(|condition| !condition.trim().is_empty()) {
            Some(condition) => {
                self.breakpoint_logs.remove(&row);
                self.breakpoint_conditions.insert(row, condition);
            }
            None => {
                self.breakpoint_conditions.remove(&row);
            }
        }
    }

    /// Turn the breakpoint on `row` into a logpoint (or clear it).
    pub fn set_breakpoint_log(&mut self, row: usize, message: Option<String>) {
        self.breakpoints.insert(row);
        match message.filter(|message| !message.trim().is_empty()) {
            Some(message) => {
                self.breakpoint_conditions.remove(&row);
                self.breakpoint_logs.insert(row, message);
            }
            None => {
                self.breakpoint_logs.remove(&row);
            }
        }
    }

    // ----------------------------------------------------------------------
    // Editing
    // ----------------------------------------------------------------------

    /// The core edit primitive. Replaces the text between `start` and `end` with
    /// `inserted`, recording a single reversible [`Edit`].
    pub(super) fn apply_edit(&mut self, start: Position, end: Position, inserted: &str) {
        self.apply_edit_coalesced(start, end, inserted, None);
    }

    /// Like [`apply_edit`], but hints how the edit may merge with the previous one.
    pub(super) fn apply_edit_coalesced(
        &mut self,
        start: Position,
        end: Position,
        inserted: &str,
        coalesce: Option<Coalesce>,
    ) {
        if self.buffer.is_user_read_only() {
            return;
        }
        let start = self.buffer.clamp_position(start);
        let end = self.buffer.clamp_position(end);
        let (a, b) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let start_char = self.buffer.position_to_char(a);
        let end_char = self.buffer.position_to_char(b);
        let removed = if end_char > start_char {
            self.buffer.remove(start_char..end_char)
        } else {
            String::new()
        };
        let inserted = self.normalize_newlines(inserted);
        if removed.is_empty() && inserted.is_empty() {
            return;
        }
        if !inserted.is_empty() {
            self.buffer.insert(start_char, &inserted);
        }
        // A fold header is a line identity: keep it attached to its text as
        // lines move, and drop it when an edit swallows the block. A server's
        // ranges describe the pre-edit text, so fall back to indentation.
        let removed_lines = removed.matches('\n').count();
        let inserted_lines = inserted.matches('\n').count();
        self.clear_server_folds();
        self.remap_folds_after_edit(a.row, a.col, removed_lines, inserted_lines);
        let cursor_after = a.advanced_by(&inserted);
        self.history.push(Edit::single(
            start_char,
            removed,
            inserted,
            self.cursor,
            cursor_after,
            coalesce,
        ));
        self.cursor = cursor_after;
        self.selection = None;
        // A single-point edit collapses any multi-cursor session.
        self.cursors.clear();
        self.preferred_col = None;
        self.buffer.mark_dirty();
        self.invalidate_highlight(a.row);
        // Diagnostics describe the previous text; drop them until recomputed.
        self.diagnostics.clear();
        self.diagnostics_dirty = true;
    }

    // ----------------------------------------------------------------------
    // Multiple cursors
    // ----------------------------------------------------------------------
}

mod helpers;

#[cfg(test)]
mod tests;
