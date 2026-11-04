//! Parse `textDocument/inlayHint` responses.

use serde_json::Value;

/// One hint as the server reported it, before position-encoding conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawInlayHint {
    pub line: usize,
    pub col: usize,
    pub label: String,
    /// LSP `InlayHintKind`: 1 type, 2 parameter, otherwise unspecified.
    pub kind: Option<u64>,
    pub padding_left: bool,
    pub padding_right: bool,
}

/// Parse an inlay-hint result.
pub fn hints(value: &Value) -> Vec<RawInlayHint> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let position = item.get("position")?;
            let line = position.get("line")?.as_u64()? as usize;
            let col = position.get("character")?.as_u64()? as usize;
            let label = label_text(item.get("label")?)?;
            if label.is_empty() {
                return None;
            }
            Some(RawInlayHint {
                line,
                col,
                label,
                kind: item.get("kind").and_then(Value::as_u64),
                padding_left: item
                    .get("paddingLeft")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                padding_right: item
                    .get("paddingRight")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

/// A label is either a string or an array of `{ value }` parts.
fn label_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    let items = value.as_array()?;
    let text: String = items
        .iter()
        .filter_map(|part| part.get("value").and_then(Value::as_str))
        .collect();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_labels_positions_and_padding() {
        let value = json!([
            {
                "position": { "line": 1, "character": 8 },
                "label": ": i32",
                "kind": 1,
                "paddingLeft": true
            },
            {
                "position": { "line": 2, "character": 4 },
                "label": [ { "value": "count" }, { "value": ":" } ],
                "kind": 2,
                "paddingRight": true
            }
        ]);
        let hints = hints(&value);
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].line, 1);
        assert_eq!(hints[0].col, 8);
        assert_eq!(hints[0].label, ": i32");
        assert_eq!(hints[0].kind, Some(1));
        assert!(hints[0].padding_left);
        assert!(!hints[0].padding_right);
        assert_eq!(hints[1].label, "count:");
        assert!(hints[1].padding_right);
    }

    #[test]
    fn ignores_malformed_and_empty_hints() {
        let value = json!([
            { "position": { "line": 0, "character": 0 }, "label": "" },
            { "position": { "line": 0 }, "label": "x" },
            { "label": "x" }
        ]);
        assert!(hints(&value).is_empty());
        assert!(hints(&json!(null)).is_empty());
    }
}
