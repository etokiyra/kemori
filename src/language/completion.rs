//! Completion candidates.
//!
//! Kemori's built-in completion is intentionally simple and always available: it
//! merges language keywords/types supplied by the provider with identifiers
//! already present in the buffer. The popup, filtering and acceptance are
//! language-agnostic, so a language server can supply richer candidates later
//! without changing the UI.

/// What kind of thing a completion suggests, used for the kind glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Keyword,
    Type,
    Function,
    Constant,
    Variable,
    Text,
}

impl CompletionKind {
    pub fn glyph(self) -> char {
        match self {
            CompletionKind::Keyword => 'k',
            CompletionKind::Type => 't',
            CompletionKind::Function => 'f',
            CompletionKind::Constant => 'c',
            CompletionKind::Variable => 'v',
            CompletionKind::Text => '·',
        }
    }
}

/// How a server wants a completion inserted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompletionEdit {
    /// The text to insert instead of the label.
    pub insert_text: Option<String>,
    /// The text uses snippet placeholders (`insertTextFormat: 2`).
    pub snippet: bool,
    /// The LSP range the text replaces, in `(line, character)` units.
    pub text_edit_range: Option<((usize, usize), (usize, usize))>,
}

/// A single completion suggestion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: String,
    pub documentation: String,
    /// Insert text, snippet flag and optional range.
    pub edit: CompletionEdit,
    /// The original LSP item, serialized for `completionItem/resolve`.
    pub raw: Option<String>,
}

impl Completion {
    pub fn new(label: impl Into<String>, kind: CompletionKind) -> Self {
        Completion {
            label: label.into(),
            kind,
            detail: String::new(),
            documentation: String::new(),
            edit: CompletionEdit::default(),
            raw: None,
        }
    }

    /// Attach the short detail line and long documentation.
    pub fn with_metadata(
        mut self,
        detail: String,
        documentation: String,
        raw: Option<String>,
    ) -> Self {
        self.detail = detail;
        self.documentation = documentation;
        self.raw = raw;
        self
    }

    /// Replace the resolved detail/documentation.
    pub fn set_metadata(&mut self, detail: String, documentation: String) {
        if !detail.is_empty() {
            self.detail = detail;
        }
        if !documentation.is_empty() {
            self.documentation = documentation;
        }
    }

    /// Merge a resolved edit into this item.
    pub fn apply_edit(&mut self, edit: CompletionEdit) {
        if edit.insert_text.is_some() {
            self.edit.insert_text = edit.insert_text;
        }
        if edit.snippet {
            self.edit.snippet = true;
        }
        if edit.text_edit_range.is_some() {
            self.edit.text_edit_range = edit.text_edit_range;
        }
    }
}
