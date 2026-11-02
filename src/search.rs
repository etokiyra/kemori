//! Project-wide text search and replace.
//!
//! A fast, dependency-free scan over the files Kemori already knows about
//! (respecting `.gitignore`). Literal and regular-expression queries share one
//! matcher, so the preview and the applied replacement use exactly the same
//! rules. Binary, oversized and non-UTF-8 files are skipped: a replacement must
//! never write lossy bytes back to disk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::regex::Regex;

/// A single matching line in a project file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    pub path: PathBuf,
    pub line: usize,
    /// Zero-based character column of the first match on the line.
    pub col: usize,
    /// The trimmed line text, for display.
    pub text: String,
    /// How many matches the line contains.
    pub count: usize,
    /// The line after replacement, when a replacement was supplied.
    pub preview: Option<String>,
}

/// How a project query is matched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchOptions {
    /// Match case exactly instead of case-insensitively.
    pub case_sensitive: bool,
    /// Match whole words only.
    pub whole_word: bool,
    /// Interpret the query as a regular expression.
    pub regex: bool,
    /// Comma- or whitespace-separated globs; empty means every file.
    pub include: String,
    /// Comma- or whitespace-separated globs to skip.
    pub exclude: String,
}

/// A file a replacement did not touch, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedFile {
    pub path: PathBuf,
    pub reason: String,
}

/// What a project-wide replacement changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplaceOutcome {
    pub files_changed: usize,
    pub replacements: usize,
    /// Every file whose contents were written, in discovery order.
    pub changed: Vec<PathBuf>,
    pub skipped: Vec<SkippedFile>,
    /// A file that matched but could not be written.
    pub errors: Vec<(PathBuf, String)>,
}

/// Files larger than this are skipped: they are almost never source, and
/// reading them would stall the scan.
const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// How many files to consider, mirroring quick open's bound.
const MAX_FILES: usize = 8000;
/// The most matching lines a project search returns.
pub const SEARCH_LIMIT: usize = 500;

/// Search every text file under `root` for `query`, up to `limit` matches.
///
/// At most one match is reported per line, with `count` recording how many
/// occurrences that line holds. This keeps the result list legible on minified
/// or repetitive files while still allowing an accurate replacement preview.
pub fn search_project(
    root: &Path,
    query: &str,
    options: &SearchOptions,
    replacement: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchMatch>, String> {
    if query.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let pattern = Pattern::new(query, options)?;
    let include = split_globs(&options.include);
    let exclude = split_globs(&options.exclude);
    let mut matches = Vec::new();

    for path in crate::filesystem::collect_files(root, MAX_FILES) {
        if matches.len() >= limit {
            break;
        }
        if !path_allowed(root, &path, &include, &exclude) {
            continue;
        }
        let Some(text) = read_text(&path) else {
            continue;
        };
        for (row, line) in text.lines().enumerate() {
            if matches.len() >= limit {
                break;
            }
            let found = pattern.find_all(line);
            let Some(first) = found.first() else {
                continue;
            };
            matches.push(SearchMatch {
                path: path.clone(),
                line: row,
                col: first.0,
                text: line.trim().to_string(),
                count: found.len(),
                preview: replacement.map(|text| replace_line(line, &found, text)),
            });
        }
    }
    Ok(matches)
}

/// Replace every match in the project, atomically per file.
///
/// `skip` names files the caller must not touch (typically open buffers with
/// unsaved edits); they are reported in [`ReplaceOutcome::skipped`] instead of
/// being overwritten behind the user's back.
pub fn replace_project(
    root: &Path,
    query: &str,
    options: &SearchOptions,
    replacement: &str,
    skip: &HashSet<PathBuf>,
) -> Result<ReplaceOutcome, String> {
    let mut outcome = ReplaceOutcome::default();
    if query.is_empty() {
        return Ok(outcome);
    }
    let pattern = Pattern::new(query, options)?;
    let include = split_globs(&options.include);
    let exclude = split_globs(&options.exclude);

    for path in crate::filesystem::collect_files(root, MAX_FILES) {
        if !path_allowed(root, &path, &include, &exclude) {
            continue;
        }
        if skip.contains(&path) {
            outcome.skipped.push(SkippedFile {
                path,
                reason: "unsaved changes".to_string(),
            });
            continue;
        }
        let Some(text) = read_text(&path) else {
            // A non-UTF-8 file is left untouched rather than written lossily.
            if path.is_file() && std::fs::metadata(&path).is_ok_and(|m| m.len() <= MAX_FILE_BYTES) {
                outcome.skipped.push(SkippedFile {
                    path,
                    reason: "not valid UTF-8".to_string(),
                });
            }
            continue;
        };
        let (updated, count) = replace_text(&text, &pattern, replacement);
        if count == 0 {
            continue;
        }
        match crate::filesystem::write_atomic(&path, &updated) {
            Ok(()) => {
                outcome.files_changed += 1;
                outcome.replacements += count;
                outcome.changed.push(path);
            }
            Err(err) => outcome.errors.push((path, err.to_string())),
        }
    }
    Ok(outcome)
}

/// Read a file as text, refusing binary and oversized files.
fn read_text(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn split_globs(input: &str) -> Vec<String> {
    input
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn path_allowed(root: &Path, path: &Path, include: &[String], exclude: &[String]) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
    let basename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let matches = |pattern: &str| {
        crate::filesystem::gitignore::glob_matches(pattern, &relative)
            || crate::filesystem::gitignore::glob_matches(pattern, basename)
    };
    if !include.is_empty() && !include.iter().any(|pattern| matches(pattern)) {
        return false;
    }
    if exclude.iter().any(|pattern| matches(pattern)) {
        return false;
    }
    true
}

/// A compiled query: a literal (optionally case-folded) or a regex.
struct Pattern {
    regex: Option<Regex>,
    needle: Vec<char>,
    case_sensitive: bool,
    whole_word: bool,
}

impl Pattern {
    fn new(query: &str, options: &SearchOptions) -> Result<Self, String> {
        let regex = if options.regex {
            Some(Regex::new(query)?)
        } else {
            None
        };
        let needle = if regex.is_some() {
            Vec::new()
        } else if options.case_sensitive {
            query.chars().collect()
        } else {
            fold(query).0
        };
        Ok(Pattern {
            regex,
            needle,
            case_sensitive: options.case_sensitive,
            whole_word: options.whole_word,
        })
    }

    /// All non-overlapping matches on `line`, as character offsets.
    fn find_all(&self, line: &str) -> Vec<(usize, usize)> {
        let chars: Vec<char> = line.chars().collect();
        if let Some(regex) = &self.regex {
            let mut found = Vec::new();
            let mut from = 0;
            while from <= chars.len() {
                let Some((start, end)) = regex.find(&chars, from, !self.case_sensitive) else {
                    break;
                };
                if self.boundaries_ok(&chars, start, end) {
                    found.push((start, end));
                }
                // A zero-width match must still advance or the loop never ends.
                from = if end > start { end } else { start + 1 };
            }
            return found;
        }

        let (haystack, map) = if self.case_sensitive {
            (chars.clone(), (0..chars.len()).collect())
        } else {
            fold(line)
        };
        let mut found = Vec::new();
        let mut from = 0;
        while let Some(start_hay) = find_slice(&haystack, &self.needle, from) {
            let end_hay = start_hay + self.needle.len();
            let start = map[start_hay];
            let end = map[end_hay - 1] + 1;
            if self.boundaries_ok(&chars, start, end) {
                found.push((start, end));
                from = end_hay;
            } else {
                from = start_hay + 1;
            }
        }
        found
    }

    fn boundaries_ok(&self, chars: &[char], start: usize, end: usize) -> bool {
        if !self.whole_word {
            return true;
        }
        let before = start == 0 || !is_word_char(chars[start - 1]);
        let after = end >= chars.len() || !is_word_char(chars[end]);
        before && after
    }
}

/// Fold text to lowercase and map each folded character to its original
/// character index, so columns stay correct when a character expands.
fn fold(text: &str) -> (Vec<char>, Vec<usize>) {
    let mut folded = Vec::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len());
    for (index, ch) in text.chars().enumerate() {
        for lower in ch.to_lowercase() {
            folded.push(lower);
            map.push(index);
        }
    }
    (folded, map)
}

fn find_slice(haystack: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= haystack.len() || haystack.len() - from < needle.len() {
        return None;
    }
    (from..=haystack.len() - needle.len()).find(|&index| haystack[index..].starts_with(needle))
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Apply `matches` to one line, returning the new line.
fn replace_line(line: &str, matches: &[(usize, usize)], replacement: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut last = 0;
    for &(start, end) in matches {
        out.extend(chars[last..start].iter());
        out.push_str(replacement);
        last = end;
    }
    out.extend(chars[last..].iter());
    out
}

/// Replace every match in a whole file, preserving its line endings.
fn replace_text(text: &str, pattern: &Pattern, replacement: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    for chunk in text.split_inclusive('\n') {
        let (body, ending) = split_line_ending(chunk);
        let found = pattern.find_all(body);
        if found.is_empty() {
            out.push_str(chunk);
            continue;
        }
        out.push_str(&replace_line(body, &found, replacement));
        out.push_str(ending);
        count += found.len();
    }
    (out, count)
}

fn split_line_ending(chunk: &str) -> (&str, &str) {
    if let Some(body) = chunk.strip_suffix("\r\n") {
        (body, "\r\n")
    } else if let Some(body) = chunk.strip_suffix('\n') {
        (body, "\n")
    } else {
        (chunk, "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kemori-search-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        dir
    }

    fn all() -> SearchOptions {
        SearchOptions {
            case_sensitive: true,
            ..SearchOptions::default()
        }
    }

    #[test]
    fn finds_case_insensitive_matches() {
        let dir = project("basic");
        fs::write(dir.join("src/a.rs"), "fn main() {\n    let Name = 1;\n}\n").unwrap();
        fs::write(dir.join("src/b.rs"), "// nothing here\n").unwrap();

        let matches = search_project(&dir, "name", &SearchOptions::default(), None, 100).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line, 1);
        assert_eq!(matches[0].col, 8);
        assert!(matches[0].text.contains("let Name"));
        assert_eq!(matches[0].count, 1);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reports_columns_in_the_original_text() {
        let dir = project("unicode-col");
        // `İ` lowercases to two code points (`i` + combining dot), which used to
        // push every later column off by one.
        fs::write(dir.join("src/a.rs"), "İfoo\n").unwrap();
        let matches = search_project(&dir, "foo", &SearchOptions::default(), None, 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].col, 1, "column counted in the original line");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn case_sensitivity_whole_words_and_regex_are_honoured() {
        let dir = project("options");
        fs::write(
            dir.join("src/a.rs"),
            "let name = 1;\nlet surname = 2;\nlet Name = 3;\n",
        )
        .unwrap();

        let exact = search_project(&dir, "Name", &all(), None, 10).unwrap();
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].line, 2);

        let words = search_project(
            &dir,
            "name",
            &SearchOptions {
                whole_word: true,
                ..SearchOptions::default()
            },
            None,
            10,
        )
        .unwrap();
        assert_eq!(words.len(), 2, "surname must not match a whole-word query");

        let regex = search_project(
            &dir,
            r"let\s+Name",
            &SearchOptions {
                regex: true,
                case_sensitive: true,
                ..SearchOptions::default()
            },
            None,
            10,
        )
        .unwrap();
        assert_eq!(regex.len(), 1);
        assert_eq!(regex[0].line, 2);

        let invalid = search_project(
            &dir,
            "(unclosed",
            &SearchOptions {
                regex: true,
                ..SearchOptions::default()
            },
            None,
            10,
        );
        assert!(invalid.is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn include_and_exclude_globs_filter_files() {
        let dir = project("globs");
        fs::write(dir.join("src/a.rs"), "needle\n").unwrap();
        fs::write(dir.join("src/b.txt"), "needle\n").unwrap();
        fs::write(dir.join("Cargo.toml"), "needle\n").unwrap();
        let only_rust = search_project(
            &dir,
            "needle",
            &SearchOptions {
                include: "*.rs".to_string(),
                ..SearchOptions::default()
            },
            None,
            10,
        )
        .unwrap();
        assert_eq!(only_rust.len(), 1);
        assert!(only_rust[0].path.ends_with("a.rs"));

        let without_manifest = search_project(
            &dir,
            "needle",
            &SearchOptions {
                exclude: "Cargo.toml".to_string(),
                ..SearchOptions::default()
            },
            None,
            10,
        )
        .unwrap();
        assert_eq!(without_manifest.len(), 2);
        assert!(
            without_manifest
                .iter()
                .all(|entry| !entry.path.ends_with("Cargo.toml"))
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn previews_show_the_replaced_line() {
        let dir = project("preview");
        fs::write(dir.join("src/a.rs"), "let name = name;\n").unwrap();
        let matches = search_project(
            &dir,
            "name",
            &SearchOptions {
                case_sensitive: true,
                ..SearchOptions::default()
            },
            Some("value"),
            10,
        )
        .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].count, 2);
        assert_eq!(matches[0].preview.as_deref(), Some("let value = value;"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replaces_every_occurrence_and_preserves_line_endings() {
        let dir = project("replace");
        let path = dir.join("src/a.rs");
        fs::write(&path, "Name one\r\nname two\nName three").unwrap();
        fs::write(dir.join("src/b.rs"), "nothing\n").unwrap();

        let outcome = replace_project(
            &dir,
            "name",
            &SearchOptions::default(),
            "value",
            &HashSet::new(),
        )
        .unwrap();
        assert_eq!(outcome.files_changed, 1);
        assert_eq!(outcome.replacements, 3);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "value one\r\nvalue two\nvalue three"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replacement_skips_dirty_open_files_and_reports_them() {
        let dir = project("replace-skip");
        let path = dir.join("src/a.rs");
        fs::write(&path, "needle\n").unwrap();
        let mut skip = HashSet::new();
        skip.insert(path.clone());

        let outcome =
            replace_project(&dir, "needle", &SearchOptions::default(), "x", &skip).unwrap();
        assert_eq!(outcome.files_changed, 0);
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].path, path);
        assert_eq!(fs::read_to_string(&path).unwrap(), "needle\n");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replacement_leaves_non_utf8_files_alone() {
        let dir = project("replace-binary");
        let path = dir.join("src/a.rs");
        fs::write(&path, b"needle\n\xff\n").unwrap();
        let outcome = replace_project(
            &dir,
            "needle",
            &SearchOptions::default(),
            "x",
            &HashSet::new(),
        )
        .unwrap();
        assert_eq!(outcome.files_changed, 0);
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].reason, "not valid UTF-8");
        assert_eq!(fs::read(&path).unwrap(), b"needle\n\xff\n");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn skips_binary_and_ignored_files() {
        let dir = project("skip");
        fs::write(dir.join("src/keep.txt"), "needle\n").unwrap();
        fs::write(dir.join("src/blob.bin"), b"needle\0binary").unwrap();
        fs::create_dir_all(dir.join("generated")).unwrap();
        fs::write(dir.join("generated/gen.txt"), "needle\n").unwrap();
        fs::write(dir.join(".gitignore"), "generated/\n").unwrap();

        let matches = search_project(&dir, "needle", &SearchOptions::default(), None, 100).unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].path.ends_with("keep.txt"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn respects_the_limit_and_empty_query() {
        let dir = project("limit");
        fs::write(dir.join("src/many.txt"), "hit\nhit\nhit\n").unwrap();
        assert_eq!(
            search_project(&dir, "hit", &SearchOptions::default(), None, 2)
                .unwrap()
                .len(),
            2
        );
        assert!(
            search_project(&dir, "", &SearchOptions::default(), None, 10)
                .unwrap()
                .is_empty()
        );
        fs::remove_dir_all(&dir).ok();
    }
}
