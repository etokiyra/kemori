//! Extracted from `document/mod.rs`; see `docs/DECISIONS.md`.

use super::*;

pub(super) fn is_bracket(c: char) -> bool {
    is_open_bracket(c) || is_close_bracket(c)
}

pub(super) fn is_open_bracket(c: char) -> bool {
    matches!(c, '(' | '[' | '{')
}

pub(super) fn is_close_bracket(c: char) -> bool {
    matches!(c, ')' | ']' | '}')
}

pub(super) fn matching_close(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        _ => None,
    }
}

/// How many leading characters outdent should remove from a line.
pub(super) fn leading_outdent(line: &str, width: usize) -> usize {
    match line.chars().next() {
        Some('\t') => 1,
        Some(' ') => line
            .chars()
            .take_while(|c| *c == ' ')
            .take(width.max(1))
            .count(),
        _ => 0,
    }
}

/// Infer a file's indentation unit from its leading whitespace.
///
/// The smallest increase in indentation between consecutive non-blank lines is
/// taken as one level, which matches 2-space JavaScript and 4-space Rust alike.
/// A file indented with tabs keeps Kemori's default width for any spaces it
/// inserts, since Kemori does not convert tabs.
pub(super) fn detect_indent_width(text: &str) -> usize {
    let mut tabs = 0usize;
    let mut spaces = 0usize;
    let mut unit: Option<usize> = None;
    let mut previous = 0usize;
    let mut seen = false;

    for line in text.lines().take(2000) {
        if line.trim().is_empty() {
            continue;
        }
        let leading: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        let width = if leading.contains('\t') {
            tabs += 1;
            // Tabs have no fixed width here; treat each as one level.
            leading.chars().filter(|c| *c == '\t').count() * INDENT_WIDTH
        } else {
            if !leading.is_empty() {
                spaces += 1;
            }
            leading.chars().count()
        };

        if seen && width > previous {
            let delta = width - previous;
            if (1..=8).contains(&delta) {
                unit = Some(unit.map_or(delta, |current| current.min(delta)));
            }
        }
        previous = width;
        seen = true;
    }

    if tabs > spaces && tabs > 0 {
        // Tab-indented file: keep the conventional width for inserted spaces.
        INDENT_WIDTH
    } else {
        unit.unwrap_or(INDENT_WIDTH).clamp(1, 8)
    }
}

pub(super) fn brackets_match(open: char, close: char) -> bool {
    matches!((open, close), ('(', ')') | ('[', ']') | ('{', '}'))
}

pub(super) fn is_word_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Whether `needle` matches `hay` at `offset`, honouring case sensitivity.
pub(super) fn matches_at(
    hay: &[char],
    offset: usize,
    needle: &[char],
    case_sensitive: bool,
) -> bool {
    let window = &hay[offset..offset + needle.len()];
    if case_sensitive {
        window == needle
    } else {
        window
            .iter()
            .zip(needle)
            .all(|(a, b)| a == b || a.to_lowercase().eq(b.to_lowercase()))
    }
}

/// Whether the match at `offset` of length `len` is a whole word.
pub(super) fn is_word_boundary(hay: &[char], offset: usize, len: usize) -> bool {
    let before_ok = offset == 0 || !is_word_char(hay[offset - 1]);
    let after = offset + len;
    let after_ok = after >= hay.len() || !is_word_char(hay[after]);
    before_ok && after_ok
}
