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

/// A single completion suggestion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: String,
    pub documentation: String,
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
}
