//! Small app helpers, extracted from `mod.rs`.

use super::*;

pub(super) fn unique_copy_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let extension = path.extension().and_then(|e| e.to_str());
    for index in 1..1000 {
        let base = if index == 1 {
            format!("{stem} copy")
        } else {
            format!("{stem} copy {index}")
        };
        let name = match extension {
            Some(ext) => format!("{base}.{ext}"),
            None => base,
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem} copy"))
}

/// A compact display path, using `~` for the home directory when possible.
pub(super) fn display_path(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && let Ok(relative) = path.strip_prefix(&home)
    {
        return format!("~/{}", relative.display());
    }
    path.display().to_string()
}

/// Validate a project name against the chosen parent directory.
pub(super) fn validate_project_name(parent: &Path, name: &str) -> Result<(), String> {
    create::validate_name(name)?;
    let target = parent.join(name.trim());
    if target.exists() {
        return Err(format!("{} already exists", target.display()));
    }
    Ok(())
}

/// Pick a sensible file to open in a freshly created project.
pub(super) fn entry_file(files: &[PathBuf]) -> Option<PathBuf> {
    const PREFERRED: &[&str] = &["main.rs", "main.go", "__main__.py", "main.py"];
    files
        .iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| PREFERRED.contains(&name))
        })
        .or_else(|| {
            files.iter().find(|path| {
                matches!(
                    path.extension().and_then(|extension| extension.to_str()),
                    Some("rs" | "go" | "py" | "sh" | "bash")
                )
            })
        })
        .cloned()
}

/// Whether two optional/actual paths refer to the same file.
pub(super) fn same_file(a: Option<&Path>, b: &Path) -> bool {
    let Some(a) = a else {
        return false;
    };
    let a = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let b = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    a == b
}
pub(super) fn apply_text_edits(text: &str, edits: &[convert::TextEdit]) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    // Character offset where each line begins.
    let mut line_starts = vec![0usize];
    for (index, ch) in chars.iter().enumerate() {
        if *ch == '\n' {
            line_starts.push(index + 1);
        }
    }
    let total = chars.len();
    let offset = |line: usize, character: usize| -> usize {
        let start = line_starts.get(line).copied().unwrap_or(total);
        (start + character).min(total)
    };

    let mut sorted: Vec<&convert::TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|edit| Reverse((edit.start.0, edit.start.1)));
    for edit in sorted {
        let start = offset(edit.start.0, edit.start.1);
        let end = offset(edit.end.0, edit.end.1).max(start);
        chars.splice(start..end, edit.new_text.chars());
    }
    chars.into_iter().collect()
}

/// Whether a key is "shifted".
///
/// Real terminals vary: some report `Shift` explicitly, some report an
/// uppercase character for a shifted letter, and some (without the kitty
/// keyboard protocol) conflate `Ctrl+Shift+P` with `Ctrl+P`. Treating an
/// uppercase character as shifted recovers the former cases, so `Ctrl+Shift+P`
/// opens the command palette rather than quick open.
pub(super) fn key_shift(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::SHIFT)
        || matches!(key.code, KeyCode::Char(c) if c.is_ascii_uppercase())
}

/// Identifier words already present in a document, deduplicated in order.
///
/// Buffer completion is language-agnostic and always useful; providers only add
/// the parts a language knows (keywords, types, builtins).
pub(super) fn document_words(text: &str) -> Vec<Completion> {
    let mut seen = std::collections::HashSet::new();
    let mut words = Vec::new();
    for word in text.split(|c: char| !is_word_char(c)) {
        if word.chars().count() < 2 {
            continue;
        }
        if seen.insert(word.to_string()) {
            words.push(Completion::new(word.to_string(), CompletionKind::Variable));
        }
        if words.len() >= 2000 {
            break;
        }
    }
    words
}
