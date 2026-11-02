//! Compute the smallest `textDocument/didChange` range for an edit.
//!
//! The protocol allows a full-document replacement, but servers that advertise
//! incremental sync handle an unchanged prefix and suffix more cheaply. Koda
//! compares the old and new text by characters, then converts the changed span
//! to line/character offsets. This is deliberately a display-independent diff:
//! it only needs to be a valid range, not the prettiest one.

use serde_json::{Value, json};

/// The `contentChanges` array for an incremental change from `old` to `new`.
pub fn incremental(old: &str, new: &str) -> Value {
    let old_chars: Vec<char> = old.chars().collect();
    let new_chars: Vec<char> = new.chars().collect();

    let mut prefix = 0usize;
    while prefix < old_chars.len()
        && prefix < new_chars.len()
        && old_chars[prefix] == new_chars[prefix]
    {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < old_chars.len().saturating_sub(prefix)
        && suffix < new_chars.len().saturating_sub(prefix)
        && old_chars[old_chars.len() - 1 - suffix] == new_chars[new_chars.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let start_line = old_chars[..prefix].iter().filter(|c| **c == '\n').count();
    let start_col = old_chars[..prefix]
        .iter()
        .rev()
        .take_while(|c| **c != '\n')
        .count();
    let end_old = old_chars.len() - suffix;
    let end_line = old_chars[..end_old].iter().filter(|c| **c == '\n').count();
    let end_col = old_chars[..end_old]
        .iter()
        .rev()
        .take_while(|c| **c != '\n')
        .count();
    let changed_text: String = new_chars[prefix..new_chars.len().saturating_sub(suffix)]
        .iter()
        .collect();

    json!([{
        "range": {
            "start": { "line": start_line, "character": start_col },
            "end": { "line": end_line, "character": end_col }
        },
        "text": changed_text
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(value: &Value) -> (usize, usize, usize, usize, String) {
        let change = &value[0];
        let start = &change["range"]["start"];
        let end = &change["range"]["end"];
        (
            start["line"].as_u64().unwrap() as usize,
            start["character"].as_u64().unwrap() as usize,
            end["line"].as_u64().unwrap() as usize,
            end["character"].as_u64().unwrap() as usize,
            change["text"].as_str().unwrap().to_string(),
        )
    }

    #[test]
    fn appending_a_character_is_a_zero_width_range_at_the_end() {
        let value = incremental("one\ntwo\n", "one\ntwo!\n");
        assert_eq!(range(&value), (1, 3, 1, 3, "!".to_string()));
    }

    #[test]
    fn replacing_text_keeps_the_untouched_prefix_and_suffix() {
        let value = incremental("alpha beta gamma", "alpha BETA gamma");
        assert_eq!(range(&value), (0, 6, 0, 10, "BETA".to_string()));
    }

    #[test]
    fn inserting_a_line_reports_the_new_text_without_repeating_neighbours() {
        let value = incremental("a\nb\n", "a\nnew\nb\n");
        assert_eq!(range(&value), (1, 0, 1, 0, "new\n".to_string()));
    }

    #[test]
    fn identical_text_produces_an_empty_change() {
        let value = incremental("same", "same");
        assert_eq!(range(&value), (0, 4, 0, 4, String::new()));
    }
}
