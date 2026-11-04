//! Code-lens decoration data.

use crate::editor::Position;

/// A language-server code lens, rendered as virtual text on its line.
///
/// `command` and `arguments` are opaque serialized payloads; the editor does
/// not interpret them, so it stays independent of JSON and LSP.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeLens {
    pub position: Position,
    pub end: Position,
    pub title: String,
    pub command: Option<String>,
    pub arguments: Option<String>,
}
