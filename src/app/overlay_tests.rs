use super::*;
use crate::language::completion::CompletionKind;

#[test]
fn classifies_unified_diff_lines() {
    let diff = DiffState::from_unified(
        "working tree · a.rs",
        "diff --git a/a.rs b/a.rs\n\
             index 111..222 100644\n\
             --- a/a.rs\n\
             +++ b/a.rs\n\
             @@ -1,2 +1,2 @@\n\
             - old\n\
             + new\n\
             \u{20}unchanged\n",
    );
    assert_eq!(diff.title, "working tree · a.rs");
    let kinds: Vec<_> = diff.lines.iter().map(|line| line.kind).collect();
    assert_eq!(kinds[0], DiffLineKind::Header);
    assert_eq!(kinds[4], DiffLineKind::Hunk);
    assert_eq!(kinds[5], DiffLineKind::Remove);
    assert_eq!(kinds[6], DiffLineKind::Add);
    assert_eq!(kinds[7], DiffLineKind::Context);
    assert!(diff.lines[6].text.starts_with("+ new"));
}

#[test]
fn completion_matches_fuzzy_subsequences() {
    let pool = vec![
        Completion::new("main_result", CompletionKind::Variable),
        Completion::new("map", CompletionKind::Variable),
        Completion::new("maximum", CompletionKind::Variable),
    ];
    let state = CompletionState::new(pool, "mrs".to_string());
    let labels: Vec<&str> = state.items.iter().map(|item| item.label.as_str()).collect();
    assert!(labels.contains(&"main_result"), "labels: {labels:?}");
    assert!(!labels.contains(&"map"), "labels: {labels:?}");
}

#[test]
fn completion_ranks_prefix_matches_first() {
    let pool = vec![
        Completion::new("account", CompletionKind::Variable),
        Completion::new("counter", CompletionKind::Variable),
        Completion::new("count", CompletionKind::Variable),
    ];
    let state = CompletionState::new(pool, "count".to_string());
    let labels: Vec<&str> = state.items.iter().map(|item| item.label.as_str()).collect();
    assert_eq!(labels.first(), Some(&"count"), "labels: {labels:?}");
    assert!(labels.contains(&"counter"));
    assert!(labels.contains(&"account"));
}
