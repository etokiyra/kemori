//! Parse call- and type-hierarchy responses.

use std::path::PathBuf;

use serde_json::Value;

use super::uri_to_path;

/// A hierarchy node the user can jump to.
#[derive(Clone, Debug)]
pub struct HierarchyItem {
    pub name: String,
    pub detail: String,
    pub path: PathBuf,
    pub line: usize,
    pub col: usize,
    /// The original item, replayed into incoming/outgoing/supertype requests.
    pub raw: Value,
}

/// Nodes from a `prepareCallHierarchy` or `prepareTypeHierarchy` result.
pub fn items(value: &Value) -> Vec<HierarchyItem> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items.iter().filter_map(item).collect()
}

/// The other end of each entry in a `callHierarchy/incomingCalls` or
/// `outgoingCalls` result.
pub fn calls(value: &Value, incoming: bool) -> Vec<HierarchyItem> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|call| item(call.get(if incoming { "from" } else { "to" })?))
        .collect()
}

fn item(value: &Value) -> Option<HierarchyItem> {
    let name = value.get("name")?.as_str()?.to_string();
    let uri = value.get("uri")?.as_str()?;
    let path = uri_to_path(uri)?;
    let start = value
        .get("selectionRange")
        .or_else(|| value.get("range"))
        .and_then(|range| range.get("start"));
    let line = start
        .and_then(|start| start.get("line"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let col = start
        .and_then(|start| start.get("character"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    Some(HierarchyItem {
        name,
        detail: value
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        path,
        line,
        col,
        raw: value.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(name: &str, line: u64) -> Value {
        json!({
            "name": name,
            "kind": 12,
            "detail": "detail",
            "uri": "file:///tmp/a.rs",
            "range": {
                "start": { "line": line, "character": 0 },
                "end": { "line": line, "character": 5 }
            },
            "selectionRange": {
                "start": { "line": line, "character": 4 },
                "end": { "line": line, "character": 9 }
            }
        })
    }

    #[test]
    fn parses_prepared_items_and_call_ends() {
        let prepared = json!([node("main", 1), node("helper", 3)]);
        let items = items(&prepared);
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].name, "helper");
        assert_eq!(items[1].line, 3);
        assert_eq!(items[1].col, 4);

        let incoming = json!([{ "from": node("caller", 5), "fromRanges": [] }]);
        let incoming_items = calls(&incoming, true);
        assert_eq!(incoming_items.len(), 1);
        assert_eq!(incoming_items[0].name, "caller");

        let outgoing = json!([{ "to": node("callee", 7), "fromRanges": [] }]);
        assert_eq!(calls(&outgoing, false)[0].name, "callee");
    }

    #[test]
    fn ignores_malformed_entries() {
        assert!(items(&json!(null)).is_empty());
        assert!(calls(&json!([{ "from": { "name": "x" } }]), true).is_empty());
        assert!(items(&json!([{ "uri": "file:///tmp/a.rs" }])).is_empty());
    }
}
