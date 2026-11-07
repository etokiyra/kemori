//! Parse `textDocument/completion` and completion resolve results.

use serde_json::Value;

use crate::language::completion::{Completion, CompletionKind};

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
            Some(
                Completion::new(
                    label,
                    completion_kind(item.get("kind").and_then(Value::as_u64)),
                )
                .with_metadata(detail, documentation, Some(item.to_string())),
            )
        })
        .collect()
}

/// Read the resolved detail/documentation from a `completionItem/resolve` result.
pub fn resolved(value: &Value) -> (String, String) {
    (
        value
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        documentation(value.get("documentation")),
    )
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
        let (detail, documentation) = resolved(&json!({
            "detail": "fn() -> i32",
            "documentation": "Returns a value."
        }));
        assert_eq!(detail, "fn() -> i32");
        assert_eq!(documentation, "Returns a value.");
    }
}
