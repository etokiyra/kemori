//! Parse `textDocument/completion` and completion resolve results.

use serde_json::Value;

use crate::language::completion::{Completion, CompletionEdit, CompletionKind};

/// Convert a completion list or array into Kemori completions.
pub fn completions(value: &Value) -> Vec<Completion> {
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .or_else(|| value.as_array());
    let Some(items) = items else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let label = item.get("label")?.as_str()?;
            if label.is_empty() {
                return None;
            }
            let detail = item
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let documentation = documentation(item.get("documentation"));
            let mut completion = Completion::new(
                label,
                completion_kind(item.get("kind").and_then(Value::as_u64)),
            )
            .with_metadata(detail, documentation, Some(item.to_string()));
            completion.edit = edit(item);
            Some(completion)
        })
        .collect()
}

/// Read the resolved detail, documentation and edit from a resolve result.
pub fn resolved(value: &Value) -> (String, String, CompletionEdit) {
    (
        value
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        documentation(value.get("documentation")),
        edit(value),
    )
}

fn edit(value: &Value) -> CompletionEdit {
    let snippet = value.get("insertTextFormat").and_then(Value::as_u64) == Some(2);
    let text_edit = value.get("textEdit");
    let text_edit_range = text_edit.and_then(|text_edit| {
        let range = text_edit.get("range")?;
        let start = range.get("start")?;
        let end = range.get("end")?;
        Some((
            (
                start.get("line")?.as_u64()? as usize,
                start.get("character")?.as_u64()? as usize,
            ),
            (
                end.get("line")?.as_u64()? as usize,
                end.get("character")?.as_u64()? as usize,
            ),
        ))
    });
    let insert_text = text_edit
        .and_then(|text_edit| text_edit.get("newText"))
        .or_else(|| value.get("insertText"))
        .and_then(Value::as_str)
        .map(str::to_string);
    CompletionEdit {
        insert_text,
        snippet,
        text_edit_range,
    }
}

fn documentation(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    if let Some(object) = value.as_object() {
        return object
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
    }
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .filter_map(|item| item.get("value").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

fn completion_kind(kind: Option<u64>) -> CompletionKind {
    match kind {
        Some(2..=4) => CompletionKind::Function,
        Some(5 | 6 | 10 | 12 | 20) => CompletionKind::Variable,
        Some(7 | 8 | 13 | 22 | 25) => CompletionKind::Type,
        Some(9 | 19) => CompletionKind::Text,
        Some(14) => CompletionKind::Keyword,
        Some(21) => CompletionKind::Constant,
        _ => CompletionKind::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_documentation_detail_and_raw_item() {
        let value = json!({
            "items": [{
                "label": "value",
                "kind": 6,
                "detail": "i32",
                "documentation": { "kind": "markdown", "value": "The value." }
            }]
        });
        let items = completions(&value);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].detail, "i32");
        assert_eq!(items[0].documentation, "The value.");
        assert!(items[0].raw.as_deref().unwrap().contains("\"label\""));
    }

    #[test]
    fn parses_resolved_metadata() {
        let (detail, documentation, edit) = resolved(&json!({
            "detail": "fn() -> i32",
            "documentation": "Returns a value.",
            "insertTextFormat": 2,
            "insertText": "fn ${1:name}()"
        }));
        assert_eq!(detail, "fn() -> i32");
        assert_eq!(documentation, "Returns a value.");
        assert!(edit.snippet);
        assert_eq!(edit.insert_text.as_deref(), Some("fn ${1:name}()"));
    }

    #[test]
    fn parses_snippet_text_edits() {
        let value = json!({
            "items": [{
                "label": "fn_item",
                "insertTextFormat": 2,
                "textEdit": {
                    "range": {
                        "start": { "line": 1, "character": 2 },
                        "end": { "line": 1, "character": 5 }
                    },
                    "newText": "fn ${1:name}() {\n    $0\n}"
                }
            }]
        });
        let items = completions(&value);
        assert_eq!(items.len(), 1);
        assert!(items[0].edit.snippet);
        assert_eq!(items[0].edit.text_edit_range, Some(((1, 2), (1, 5))));
        assert!(
            items[0]
                .edit
                .insert_text
                .as_deref()
                .unwrap()
                .contains("${1:name}")
        );
    }
}
