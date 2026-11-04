//! Small, language-owned snippets with tab-stop expansion.
//!
//! A snippet is plain data: a body with `$1`/`${1:default}` placeholders, an
//! optional final `$0`, and a label shown in the picker. Expansion is lexical
//! and deliberately simple; snippets are editing aids, not a templating language.

use crate::editor::position::Position;
use crate::language::LanguageId;

/// One built-in snippet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snippet {
    pub prefix: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    pub body: &'static str,
}

impl Snippet {
    /// A snippet definition, usable in a `constant` table.
    pub const fn new(
        prefix: &'static str,
        label: &'static str,
        detail: &'static str,
        body: &'static str,
    ) -> Self {
        Snippet {
            prefix,
            label,
            detail,
            body,
        }
    }
}

/// The result of expanding a snippet body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpandedSnippet {
    pub text: String,
    /// Absolute character offsets for `$1`, `$2`, … in ascending order.
    pub stops: Vec<usize>,
    /// The final `$0` position, or the end of the text.
    pub final_stop: usize,
}

/// Expand placeholders in `body`.
///
/// Supported forms: `$1`, `${1}`, `${1:default}` and `$0`/`${0}`. A `$` that
/// does not start a placeholder is kept literally.
pub fn expand(body: &str) -> ExpandedSnippet {
    let mut text = String::new();
    let mut stops = std::collections::BTreeMap::new();
    let mut final_stop = None;
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                text.push(next);
            } else {
                text.push(ch);
            }
            continue;
        }
        if ch != '$' {
            text.push(ch);
            continue;
        }
        // `$0`, `$12`, `${1}`, `${1:default}`
        if let Some(digit) = chars.peek().and_then(|c| c.to_digit(10)) {
            chars.next();
            let mut number = digit;
            while let Some(next) = chars.peek().and_then(|c| c.to_digit(10)) {
                number = number.saturating_mul(10).saturating_add(next);
                chars.next();
            }
            record_stop(&mut stops, number, text.chars().count(), &mut final_stop);
            continue;
        }
        if chars.peek() == Some(&'{') {
            chars.next();
            let mut number = 0u32;
            let mut saw_digit = false;
            while let Some(next) = chars.peek().and_then(|c| c.to_digit(10)) {
                number = number.saturating_mul(10).saturating_add(next);
                chars.next();
                saw_digit = true;
            }
            if !saw_digit {
                text.push_str("${");
                continue;
            }
            if chars.peek() == Some(&':') {
                chars.next();
                let offset = text.chars().count();
                record_stop(&mut stops, number, offset, &mut final_stop);
                for next in chars.by_ref() {
                    if next == '}' {
                        break;
                    }
                    text.push(next);
                }
                continue;
            }
            if chars.peek() == Some(&'}') {
                chars.next();
                record_stop(&mut stops, number, text.chars().count(), &mut final_stop);
                continue;
            }
            // Malformed; keep what we read literally.
            text.push_str("${");
            text.push_str(&number.to_string());
            continue;
        }
        text.push('$');
    }
    let end = text.chars().count();
    ExpandedSnippet {
        text,
        stops: stops.into_values().collect(),
        final_stop: final_stop.unwrap_or(end),
    }
}

fn record_stop(
    stops: &mut std::collections::BTreeMap<u32, usize>,
    number: u32,
    offset: usize,
    final_stop: &mut Option<usize>,
) {
    if number == 0 {
        *final_stop = Some(offset);
    } else {
        stops.entry(number).or_insert(offset);
    }
}

/// The document position at `offset` characters after `start` in `text`.
pub fn position_at(start: Position, text: &str, offset: usize) -> Position {
    let prefix: String = text.chars().take(offset).collect();
    start.advanced_by(&prefix)
}

/// Built-in snippets for a language, in picker order.
pub fn builtin(language: LanguageId) -> &'static [Snippet] {
    match language {
        LanguageId::Rust => RUST,
        LanguageId::Python => PYTHON,
        LanguageId::Go => GO,
        LanguageId::JavaScript | LanguageId::TypeScript => JS,
        LanguageId::C | LanguageId::Cpp => C,
        LanguageId::Java => JAVA,
        LanguageId::CSharp => CSHARP,
        LanguageId::Php => PHP,
        LanguageId::Ruby => RUBY,
        LanguageId::Lua => LUA,
        LanguageId::Shell => SHELL,
        LanguageId::Kotlin => KOTLIN,
        LanguageId::Swift => SWIFT,
        LanguageId::Dart => DART,
        LanguageId::Sql => SQL,
        LanguageId::Perl => PERL,
        LanguageId::Elixir => ELIXIR,
        _ => &[],
    }
}

const RUST: &[Snippet] = &[
    Snippet::new(
        "fn",
        "Function",
        "fn name(args) { … }",
        "fn ${1:name}(${2}) {\n    $0\n}",
    ),
    Snippet::new(
        "impl",
        "Implementation",
        "impl Type { … }",
        "impl ${1:Type} {\n    $0\n}",
    ),
    Snippet::new(
        "main",
        "Main function",
        "fn main() { … }",
        "fn main() {\n    $0\n}",
    ),
    Snippet::new(
        "test",
        "Test function",
        "#[test] fn …",
        "#[test]\nfn ${1:test_name}() {\n    $0\n}",
    ),
];
const PYTHON: &[Snippet] = &[
    Snippet::new(
        "def",
        "Function",
        "def name(args): …",
        "def ${1:name}(${2}):\n    $0",
    ),
    Snippet::new(
        "class",
        "Class",
        "class Name: …",
        "class ${1:Name}:\n    def __init__(self${2}):\n        $0",
    ),
    Snippet::new(
        "ifmain",
        "Main guard",
        "if __name__ == …",
        "if __name__ == \"__main__\":\n    $0",
    ),
];
const GO: &[Snippet] = &[
    Snippet::new(
        "func",
        "Function",
        "func name(args) { … }",
        "func ${1:name}(${2}) ${3:error} {\n\t$0\n}",
    ),
    Snippet::new(
        "main",
        "Main function",
        "func main() { … }",
        "func main() {\n\t$0\n}",
    ),
    Snippet::new(
        "test",
        "Test function",
        "func TestX(t *testing.T) { … }",
        "func Test${1:Name}(t *testing.T) {\n\t$0\n}",
    ),
];
const JS: &[Snippet] = &[
    Snippet::new(
        "fn",
        "Function",
        "function name(args) { … }",
        "function ${1:name}(${2}) {\n  $0\n}",
    ),
    Snippet::new(
        "arrow",
        "Arrow function",
        "const name = (args) => { … }",
        "const ${1:name} = (${2}) => {\n  $0\n};",
    ),
    Snippet::new(
        "log",
        "Console log",
        "console.log(…)",
        "console.log(${1:value});$0",
    ),
];
const C: &[Snippet] = &[
    Snippet::new(
        "main",
        "Main function",
        "int main(int argc, char **argv) { … }",
        "int main(int argc, char **argv) {\n    $0\n    return 0;\n}",
    ),
    Snippet::new(
        "fn",
        "Function",
        "return name(args) { … }",
        "${1:void} ${2:name}(${3}) {\n    $0\n}",
    ),
];
const JAVA: &[Snippet] = &[
    Snippet::new(
        "main",
        "Main method",
        "public static void main(String[] args) { … }",
        "public static void main(String[] args) {\n    $0\n}",
    ),
    Snippet::new(
        "class",
        "Class",
        "public class Name { … }",
        "public class ${1:Name} {\n    $0\n}",
    ),
];
const CSHARP: &[Snippet] = &[
    Snippet::new(
        "main",
        "Main method",
        "static void Main(string[] args) { … }",
        "static void Main(string[] args) {\n    $0\n}",
    ),
    Snippet::new(
        "class",
        "Class",
        "public class Name { … }",
        "public class ${1:Name} {\n    $0\n}",
    ),
];
const PHP: &[Snippet] = &[
    Snippet::new(
        "fn",
        "Function",
        "function name(args) { … }",
        "function ${1:name}(${2}) {\n    $0\n}",
    ),
    Snippet::new(
        "class",
        "Class",
        "class Name { … }",
        "class ${1:Name} {\n    $0\n}",
    ),
];
const RUBY: &[Snippet] = &[
    Snippet::new(
        "def",
        "Method",
        "def name(args) … end",
        "def ${1:name}(${2})\n  $0\nend",
    ),
    Snippet::new(
        "class",
        "Class",
        "class Name … end",
        "class ${1:Name}\n  $0\nend",
    ),
];
const LUA: &[Snippet] = &[
    Snippet::new(
        "function",
        "Function",
        "function name(args) … end",
        "function ${1:name}(${2})\n  $0\nend",
    ),
    Snippet::new(
        "if",
        "If block",
        "if condition then … end",
        "if ${1:condition} then\n  $0\nend",
    ),
];
const SHELL: &[Snippet] = &[
    Snippet::new(
        "if",
        "If block",
        "if condition; then … fi",
        "if ${1:condition}; then\n    $0\nfi",
    ),
    Snippet::new(
        "for",
        "For loop",
        "for item in items; do … done",
        "for ${1:item} in ${2:items}; do\n    $0\ndone",
    ),
];
const KOTLIN: &[Snippet] = &[
    Snippet::new(
        "fun",
        "Function",
        "fun name(args) { … }",
        "fun ${1:name}(${2}) {\n    $0\n}",
    ),
    Snippet::new(
        "main",
        "Main function",
        "fun main() { … }",
        "fun main() {\n    $0\n}",
    ),
];
const SWIFT: &[Snippet] = &[
    Snippet::new(
        "func",
        "Function",
        "func name(args) { … }",
        "func ${1:name}(${2}) {\n    $0\n}",
    ),
    Snippet::new(
        "class",
        "Class",
        "class Name { … }",
        "class ${1:Name} {\n    $0\n}",
    ),
];
const DART: &[Snippet] = &[
    Snippet::new(
        "main",
        "Main function",
        "void main() { … }",
        "void main() {\n  $0\n}",
    ),
    Snippet::new(
        "class",
        "Class",
        "class Name { … }",
        "class ${1:Name} {\n  $0\n}",
    ),
];
const SQL: &[Snippet] = &[Snippet::new(
    "select",
    "Select query",
    "SELECT … FROM …",
    "SELECT ${1:*}\nFROM ${2:table};$0",
)];
const PERL: &[Snippet] = &[Snippet::new(
    "sub",
    "Subroutine",
    "sub name { … }",
    "sub ${1:name} {\n    $0\n}",
)];
const ELIXIR: &[Snippet] = &[Snippet::new(
    "def",
    "Function",
    "def name(args) do … end",
    "def ${1:name}(${2}) do\n  $0\nend",
)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_placeholders_and_orders_stops() {
        let expanded = expand("fn ${1:name}(${2}) {\n    $0\n}");
        assert_eq!(expanded.text, "fn name() {\n    \n}");
        // `$1` at 3, `$2` after `name(` at 8.
        assert_eq!(expanded.stops, vec![3, 8]);
        assert_eq!(expanded.final_stop, expanded.text.chars().count() - 2);
    }

    #[test]
    fn keeps_dollar_escapes_and_lone_dollars() {
        assert_eq!(expand("\\$HOME $x $").text, "$HOME $x $");
    }

    #[test]
    fn builtins_cover_the_core_languages() {
        assert!(builtin(LanguageId::Rust).iter().any(|s| s.prefix == "fn"));
        assert!(
            builtin(LanguageId::Python)
                .iter()
                .any(|s| s.prefix == "def")
        );
        assert!(builtin(LanguageId::Go).iter().any(|s| s.prefix == "func"));
        assert!(!builtin(LanguageId::Json).iter().any(|_| true));
    }

    #[test]
    fn position_at_advances_across_lines() {
        assert_eq!(
            position_at(Position::new(2, 3), "ab\ncd", 4),
            Position::new(3, 1)
        );
    }
}
