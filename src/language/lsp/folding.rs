//! Parse `textDocument/foldingRange` responses.
//!
//! A range hides the lines between `startLine` and `endLine`, both inclusive
//! and zero-based in the protocol. Koda keeps the pair as `(header, last hidden
//! line)` and rejects ranges that hide nothing.

use serde_json::Value;

/// Convert a folding-range result into `(header, last hidden)` pairs.
pub fn ranges(value: &Value) -> Vec<(usize, usize)> {
    let Some(ranges) = value.as_array() else {
        return Vec::new();
    };
    ranges
        .iter()
        .filter_map(|range| {
            let start = range.get("startLine")?.as_u64()? as usize;
            let end = range.get("endLine")?.as_u64()? as usize;
            (end > start).then_some((start, end))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_only_ranges_that_hide_lines() {
        let value = json!([
            { "startLine": 2, "endLine": 6 },
            { "startLine": 4, "startCharacter": 1, "endLine": 4, "endCharacter": 9 },
            { "startLine": 8, "endLine": 7 }
        ]);
        assert_eq!(ranges(&value), vec![(2, 6)]);
        assert!(ranges(&json!(null)).is_empty());
    }
}
