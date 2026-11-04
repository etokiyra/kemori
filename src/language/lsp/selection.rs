//! Parse `textDocument/selectionRange` responses.

use serde_json::Value;

/// One range in the chain, innermost first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawSelectionRange {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// Flatten the first position's `parent` chain from innermost to outermost.
pub fn ranges(value: &Value) -> Vec<RawSelectionRange> {
    let Some(first) = value.as_array().and_then(|items| items.first()) else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    let mut node = Some(first);
    while let Some(current) = node {
        let Some(range) = current.get("range") else {
            break;
        };
        let Some(start) = range.get("start") else {
            break;
        };
        let Some(end) = range.get("end") else {
            break;
        };
        if let (Some(start_line), Some(start_col), Some(end_line), Some(end_col)) = (
            start.get("line").and_then(Value::as_u64),
            start.get("character").and_then(Value::as_u64),
            end.get("line").and_then(Value::as_u64),
            end.get("character").and_then(Value::as_u64),
        ) {
            ranges.push(RawSelectionRange {
                start_line: start_line as usize,
                start_col: start_col as usize,
                end_line: end_line as usize,
                end_col: end_col as usize,
            });
        }
        node = current.get("parent");
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flattens_the_parent_chain_innermost_first() {
        let value = json!([
            {
                "range": {
                    "start": { "line": 2, "character": 4 },
                    "end": { "line": 2, "character": 9 }
                },
                "parent": {
                    "range": {
                        "start": { "line": 2, "character": 4 },
                        "end": { "line": 2, "character": 10 }
                    },
                    "parent": {
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 4, "character": 1 }
                        }
                    }
                }
            }
        ]);
        let ranges = ranges(&value);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0].start_line, 2);
        assert_eq!(ranges[0].end_col, 9);
        assert_eq!(ranges[1].end_col, 10);
        assert_eq!(ranges[2].start_line, 0);
    }

    #[test]
    fn empty_or_malformed_results_are_ignored() {
        assert!(ranges(&json!(null)).is_empty());
        assert!(ranges(&json!([])).is_empty());
        assert!(ranges(&json!([{}])).is_empty());
    }
}
