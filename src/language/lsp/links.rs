//! Parse `textDocument/documentLink` responses.

use serde_json::Value;

/// One link reported by the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentLinkItem {
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub target: String,
    pub tooltip: String,
}

/// Parse a document-link result. Links without a target need a resolve step and
/// are skipped.
pub fn links(value: &Value) -> Vec<DocumentLinkItem> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let range = item.get("range")?;
            let start = range.get("start")?;
            let end = range.get("end")?;
            let target = item.get("target")?.as_str()?.to_string();
            if target.is_empty() {
                return None;
            }
            Some(DocumentLinkItem {
                line: start.get("line")?.as_u64()? as usize,
                start_col: start.get("character")?.as_u64()? as usize,
                end_col: end.get("character")?.as_u64()? as usize,
                target,
                tooltip: item
                    .get("tooltip")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_targets_ranges_and_tooltips() {
        let value = json!([
            {
                "range": {
                    "start": { "line": 3, "character": 8 },
                    "end": { "line": 3, "character": 16 }
                },
                "target": "file:///tmp/other.rs",
                "tooltip": "Open other.rs"
            },
            {
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 4 }
                },
                "target": "https://example.com"
            }
        ]);
        let links = links(&value);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].line, 3);
        assert_eq!(links[0].start_col, 8);
        assert_eq!(links[0].target, "file:///tmp/other.rs");
        assert_eq!(links[0].tooltip, "Open other.rs");
        assert_eq!(links[1].target, "https://example.com");
    }

    #[test]
    fn skips_links_that_need_resolving() {
        let value = json!([
            {
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 4 }
                },
                "data": { "id": 1 }
            }
        ]);
        assert!(links(&value).is_empty());
        assert!(links(&json!(null)).is_empty());
    }
}
