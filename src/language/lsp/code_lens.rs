//! Parse `textDocument/codeLens` responses.

use serde_json::Value;

/// One lens as the server reported it, before position-encoding conversion.
#[derive(Clone, Debug, PartialEq)]
pub struct RawCodeLens {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub title: String,
    pub command: Option<String>,
    /// Serialized `arguments` array, kept opaque for the app.
    pub arguments: Option<String>,
}

/// Parse a code-lens result.
pub fn lenses(value: &Value) -> Vec<RawCodeLens> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let range = item.get("range")?;
            let start = range.get("start")?;
            let end = range.get("end")?;
            let command = item.get("command");
            let title = command
                .and_then(|command| command.get("title"))
                .and_then(Value::as_str)
                .filter(|title| !title.is_empty())?;
            Some(RawCodeLens {
                start_line: start.get("line")?.as_u64()? as usize,
                start_col: start.get("character")?.as_u64()? as usize,
                end_line: end.get("line")?.as_u64()? as usize,
                end_col: end.get("character")?.as_u64()? as usize,
                title: title.to_string(),
                command: command
                    .and_then(|command| command.get("command"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                arguments: command
                    .and_then(|command| command.get("arguments"))
                    .map(Value::to_string),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_commands_positions_and_arguments() {
        let value = json!([
            {
                "range": {
                    "start": { "line": 1, "character": 0 },
                    "end": { "line": 1, "character": 8 }
                },
                "command": {
                    "title": "2 references",
                    "command": "rust-analyzer.showReferences",
                    "arguments": [ "/tmp/a.rs", 1, 2 ]
                }
            }
        ]);
        let lenses = lenses(&value);
        assert_eq!(lenses.len(), 1);
        assert_eq!(lenses[0].title, "2 references");
        assert_eq!(lenses[0].start_line, 1);
        assert_eq!(lenses[0].end_col, 8);
        assert_eq!(
            lenses[0].command.as_deref(),
            Some("rust-analyzer.showReferences")
        );
        assert!(
            lenses[0]
                .arguments
                .as_deref()
                .unwrap()
                .contains("/tmp/a.rs")
        );
    }

    #[test]
    fn ignores_lenses_without_a_title() {
        assert!(lenses(&json!(null)).is_empty());
        assert!(
            lenses(&json!([
                { "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } } }
            ]))
            .is_empty()
        );
    }
}
