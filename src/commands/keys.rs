/// Editor movement keys that are not commands.
///
/// Shared by the F1 cheatsheet and the README keymap so the two cannot drift.
pub const EDITOR_KEYS: &[(&str, &str)] = &[
    ("Arrows", "move the cursor"),
    ("Ctrl+←/→", "move by word"),
    ("Shift+Arrows", "select"),
    ("Ctrl+Alt+Shift+←/→/↑/↓", "extend a rectangular selection"),
    ("Home / End", "line start / end"),
    ("Ctrl+Home / End", "document start / end"),
    ("PageUp / PageDown", "scroll a page"),
    ("F3 / Shift+F3", "find next / previous"),
    ("Ctrl+PageUp/Down", "previous / next tab"),
];

/// Context-local keys that are not registry commands (the find bar, the file
/// tree and the welcome screen).
pub const CONTEXT_KEYS: &[(&str, &str)] = &[
    (
        "Alt+C / Alt+W / Alt+R",
        "find / project search: toggle case / whole word / regex",
    ),
    ("Alt+Enter", "find / project search: replace every match"),
    ("Alt+P", "find/replace: preview replacements"),
    ("Alt+S", "find/replace: search the whole file again"),
    ("Space", "changed files: stage or unstage"),
    ("d", "changed files: show the diff"),
    ("/", "file tree: filter project files"),
    (".", "file tree: toggle hidden files"),
    (
        "Tab / Shift+Tab",
        "panel: cycle tabs; project search: next / previous field",
    ),
    (
        "↑ / ↓, PageUp / PageDown",
        "panel / project search: move through results",
    ),
    (
        "Enter",
        "panel / project search: open or search; welcome: open",
    ),
    (
        "c",
        "panel: clear task and project problems, or the current output",
    ),
    (
        "r",
        "panel: refresh project problems (Problems) / rerun the last task (Output)",
    ),
    (
        "x / h",
        "panel Output: cancel the running task / task history",
    ),
    ("Alt+↑↓", "panel: grow or shrink"),
    ("Esc", "panel / project search: close"),
    ("↑ / ↓", "welcome: choose an action"),
    ("v", "welcome: cycle the animated scene"),
];

/// Render the keymap as a Markdown table from the command registry plus the
/// editor and context keys.
///
/// `examples/keymap.rs` prints this to regenerate the README block, and
/// `tests/keymap.rs` fails when `README.md` drifts from it.
pub fn keymap_markdown() -> String {
    let registry = CommandRegistry::builtin();
    let mut out = String::from("| Category | Shortcut | Action |\n| --- | --- | --- |\n");

    let mut order: Vec<&'static str> = Vec::new();
    let mut groups: HashMap<&'static str, Vec<(&'static str, &'static str)>> = HashMap::new();
    for command in registry.all() {
        let Some(shortcut) = command.shortcut else {
            continue;
        };
        if !groups.contains_key(command.category) {
            order.push(command.category);
        }
        groups
            .entry(command.category)
            .or_default()
            .push((shortcut, command.title));
    }
    for category in order {
        for (shortcut, title) in &groups[category] {
            out.push_str(&format!("| {category} | `{shortcut}` | {title} |\n"));
        }
    }
    for (shortcut, title) in EDITOR_KEYS {
        out.push_str(&format!("| Editor | `{shortcut}` | {title} |\n"));
    }
    for (shortcut, title) in CONTEXT_KEYS {
        out.push_str(&format!("| Context | `{shortcut}` | {title} |\n"));
    }
    out
}
