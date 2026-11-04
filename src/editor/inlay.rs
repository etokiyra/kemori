//! Decoration types the editor can render without knowing about LSP.

/// What an inlay hint describes, used for wording in future UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlayHintKind {
    Type,
    Parameter,
    Other,
}

/// A short piece of virtual text inserted at a document position.
///
/// The text is never part of the buffer: moves, undo and saving ignore it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlayHint {
    pub position: crate::editor::Position,
    pub label: String,
    pub kind: InlayHintKind,
    pub padding_left: bool,
    pub padding_right: bool,
}
