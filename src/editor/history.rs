//! Undo/redo history.
//!
//! Each [`Edit`] records just enough information to invert a single logical change:
//! the character offset it began at, the text that was removed, the text that was
//! inserted, and the cursor before/after. Consecutive single-character edits are
//! *coalesced* into one undo step, so undo feels like other editors rather than
//! unwinding a word one letter at a time.

use crate::editor::position::Position;

use serde_json::{Value, json};

/// How an edit may be merged with the previous one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coalesce {
    /// Typing characters forward.
    Insert,
    /// Backspacing into previous text.
    DeleteBackward,
    /// Deleting forward.
    DeleteForward,
}

/// A single primitive replacement inside an [`Edit`].
///
/// One edit usually has one op, but a multi-cursor keystroke produces several
/// and is undone/redone as a single step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditOp {
    /// Character offset where the change began.
    pub start: usize,
    /// Text removed by the change (re-inserted on undo).
    pub removed: String,
    /// Text inserted by the change (removed on undo).
    pub inserted: String,
}

/// A reversible change.
#[derive(Clone, Debug)]
pub struct Edit {
    /// A process-unique identity, used to compare against the save point even
    /// after edits have been undone and re-applied.
    pub id: u64,
    /// The primitive replacements, in the order they were applied.
    pub ops: Vec<EditOp>,
    pub cursor_before: Position,
    pub cursor_after: Position,
    /// Whether this edit may merge with the one before it.
    pub coalesce: Option<Coalesce>,
}

impl Edit {
    /// A single-op edit.
    pub fn single(
        start: usize,
        removed: String,
        inserted: String,
        cursor_before: Position,
        cursor_after: Position,
        coalesce: Option<Coalesce>,
    ) -> Self {
        Edit {
            id: 0,
            ops: vec![EditOp {
                start,
                removed,
                inserted,
            }],
            cursor_before,
            cursor_after,
            coalesce,
        }
    }

    /// Whether this edit consists of exactly one primitive op.
    fn is_single(&self) -> bool {
        self.ops.len() == 1
    }
}

/// A bounded undo/redo stack.
#[derive(Debug)]
pub struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    limit: usize,
    next_id: u64,
}

impl Default for History {
    fn default() -> Self {
        History::new(1000)
    }
}

impl History {
    pub fn new(limit: usize) -> Self {
        History {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            next_id: 1,
        }
    }

    /// Record a new edit, merging it with the previous one when appropriate.
    pub fn push(&mut self, mut edit: Edit) {
        self.redo.clear();
        if let Some(kind) = edit.coalesce
            && let Some(last) = self.undo.last_mut()
            && last.coalesce == Some(kind)
            && merge(last, &edit, kind)
        {
            return;
        }
        edit.id = self.next_id;
        self.next_id += 1;
        self.undo.push(edit);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
    }

    /// Prevent the next edit from merging into the current step.
    ///
    /// Called when the cursor moves, so typing in two separate places becomes two
    /// undo steps.
    pub fn break_coalesce(&mut self) {
        if let Some(last) = self.undo.last_mut() {
            last.coalesce = None;
        }
    }

    /// Take the next edit to undo.
    pub fn undo(&mut self) -> Option<Edit> {
        let edit = self.undo.pop()?;
        if let Some(last) = self.undo.last_mut() {
            last.coalesce = None;
        }
        self.redo.push(edit.clone());
        Some(edit)
    }

    /// Take the next edit to redo.
    pub fn redo(&mut self) -> Option<Edit> {
        let edit = self.redo.pop()?;
        if let Some(last) = self.undo.last_mut() {
            last.coalesce = None;
        }
        self.undo.push(edit.clone());
        Some(edit)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Number of undoable edits.
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Number of redoable edits.
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// The identity of the top undo edit, or `None` when history is at the
    /// initial state. Used to test whether the document is back at its save
    /// point.
    pub fn top_id(&self) -> Option<u64> {
        self.undo.last().map(|edit| edit.id)
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Serialize the last `max_edits` undo steps and `saved_id` for a session.
    ///
    /// Persistent undo is bounded: a session file must not grow with a huge
    /// editing history, and edits older than the window are discarded.
    pub fn to_value(&self, saved_id: Option<u64>, max_edits: usize) -> Value {
        let start = self.undo.len().saturating_sub(max_edits.max(1));
        json!({
            "undo": self.undo[start..].iter().map(edit_to_value).collect::<Vec<_>>(),
            "redo": self.redo.iter().map(edit_to_value).collect::<Vec<_>>(),
            "next_id": self.next_id,
            "saved_id": saved_id,
        })
    }

    /// Restore a history snapshot from [`History::to_value`].
    pub fn from_value(value: &Value) -> Option<(History, Option<u64>)> {
        let undo = value
            .get("undo")?
            .as_array()?
            .iter()
            .filter_map(edit_from_value)
            .collect::<Vec<_>>();
        let redo: Vec<Edit> = value
            .get("redo")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(edit_from_value).collect())
            .unwrap_or_default();
        let max_id = undo
            .iter()
            .chain(redo.iter())
            .map(|edit| edit.id)
            .max()
            .unwrap_or(0);
        let next_id = value
            .get("next_id")
            .and_then(Value::as_u64)
            .unwrap_or(max_id + 1)
            .max(max_id + 1);
        let saved_id = value.get("saved_id").and_then(Value::as_u64);
        Some((
            History {
                undo,
                redo,
                limit: 1000,
                next_id,
            },
            saved_id,
        ))
    }
}

fn edit_to_value(edit: &Edit) -> Value {
    json!({
        "id": edit.id,
        "ops": edit.ops.iter().map(|op| json!({
            "start": op.start,
            "removed": op.removed,
            "inserted": op.inserted,
        })).collect::<Vec<_>>(),
        "cursor_before": [edit.cursor_before.row, edit.cursor_before.col],
        "cursor_after": [edit.cursor_after.row, edit.cursor_after.col],
        "coalesce": match edit.coalesce {
            Some(Coalesce::Insert) => Some("insert"),
            Some(Coalesce::DeleteBackward) => Some("delete_backward"),
            Some(Coalesce::DeleteForward) => Some("delete_forward"),
            None => None,
        },
    })
}

fn edit_from_value(value: &Value) -> Option<Edit> {
    let ops = value
        .get("ops")?
        .as_array()?
        .iter()
        .filter_map(|op| {
            Some(EditOp {
                start: op.get("start")?.as_u64()? as usize,
                removed: op.get("removed")?.as_str()?.to_string(),
                inserted: op.get("inserted")?.as_str()?.to_string(),
            })
        })
        .collect::<Vec<_>>();
    if ops.is_empty() {
        return None;
    }
    let position = |key: &str| -> Option<Position> {
        let pair = value.get(key)?.as_array()?;
        Some(Position::new(
            pair.first()?.as_u64()? as usize,
            pair.get(1)?.as_u64()? as usize,
        ))
    };
    let coalesce = match value.get("coalesce").and_then(Value::as_str) {
        Some("insert") => Some(Coalesce::Insert),
        Some("delete_backward") => Some(Coalesce::DeleteBackward),
        Some("delete_forward") => Some(Coalesce::DeleteForward),
        _ => None,
    };
    Some(Edit {
        id: value.get("id")?.as_u64()?,
        ops,
        cursor_before: position("cursor_before")?,
        cursor_after: position("cursor_after")?,
        coalesce,
    })
}

fn merge(last: &mut Edit, edit: &Edit, kind: Coalesce) -> bool {
    // Only single-op edits coalesce; a multi-cursor group is always its own step.
    if !last.is_single() || !edit.is_single() {
        return false;
    }
    let last_op = &mut last.ops[0];
    let edit_op = &edit.ops[0];
    match kind {
        Coalesce::Insert => {
            if !last_op.removed.is_empty() || !edit_op.removed.is_empty() {
                return false;
            }
            if last_op.start + last_op.inserted.chars().count() != edit_op.start {
                return false;
            }
            last_op.inserted.push_str(&edit_op.inserted);
            last.cursor_after = edit.cursor_after;
            true
        }
        Coalesce::DeleteBackward => {
            if !last_op.inserted.is_empty() || !edit_op.inserted.is_empty() {
                return false;
            }
            if edit_op.start + edit_op.removed.chars().count() != last_op.start {
                return false;
            }
            last_op.removed = format!("{}{}", edit_op.removed, last_op.removed);
            last_op.start = edit_op.start;
            // `cursor_before` must stay at the *first* deletion's right edge,
            // while `cursor_after` follows the most recent deletion's left edge,
            // so both undo and redo land where the user left off.
            last.cursor_after = edit.cursor_after;
            true
        }
        Coalesce::DeleteForward => {
            if !last_op.inserted.is_empty() || !edit_op.inserted.is_empty() {
                return false;
            }
            if edit_op.start != last_op.start {
                return false;
            }
            last_op.removed.push_str(&edit_op.removed);
            last.cursor_after = edit.cursor_after;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(start: usize, removed: &str, inserted: &str, id: u64) -> Edit {
        let mut edit = Edit::single(
            start,
            removed.to_string(),
            inserted.to_string(),
            Position::new(0, start),
            Position::new(0, start + inserted.chars().count()),
            None,
        );
        edit.id = id;
        edit
    }

    #[test]
    fn history_round_trips_through_json() {
        let mut history = History::new(1000);
        history.push(edit(0, "", "hello", 0));
        history.push(edit(5, "", " world", 0));
        let value = history.to_value(Some(2), 100);
        let (mut restored, saved) = History::from_value(&value).expect("history");
        assert_eq!(saved, Some(2));
        assert_eq!(restored.undo_len(), 2);
        let undone = restored.undo().expect("undo");
        assert_eq!(undone.ops[0].inserted, " world");
    }

    #[test]
    fn history_snapshot_is_bounded() {
        let mut history = History::new(1000);
        for index in 0..20 {
            history.push(edit(index, "", "x", 0));
        }
        let value = history.to_value(None, 5);
        assert_eq!(
            value.get("undo").and_then(Value::as_array).unwrap().len(),
            5
        );
    }
}
