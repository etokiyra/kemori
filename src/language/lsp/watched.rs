//! Dynamic watched-file registrations.

/// A watched-file registration the server asked for.
#[derive(Clone, Debug, Default)]
pub(super) struct WatchedFiles {
    pub(super) id: String,
    pub(super) globs: Vec<String>,
}

/// The kind of file change sent to `workspace/didChangeWatchedFiles`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WatchedFileChange {
    Created = 1,
    Changed = 2,
    Deleted = 3,
}
