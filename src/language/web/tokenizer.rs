use super::*;

pub(super) fn classify_word(word: &str) -> TokenKind {
    if KEYWORDS.contains(&word) {
        TokenKind::Keyword
    } else if CONSTANTS.contains(&word) {
        TokenKind::Constant
    } else if TYPES.contains(&word) {
        TokenKind::Type
    } else if BUILTINS.contains(&word) {
        TokenKind::Function
    } else {
        TokenKind::Plain
    }
}

/// The identifier following a `function`/`class`, skipping spaces and a
/// generator `*`.
pub(super) fn name_after(chars: &[char], from: usize) -> Option<(usize, usize)> {
    let mut j = from;
    while j < chars.len() && chars[j] == ' ' {
        j += 1;
    }
    if chars.get(j) == Some(&'*') {
        j += 1;
        while j < chars.len() && chars[j] == ' ' {
            j += 1;
        }
    }
    let start = j;
    if start >= chars.len() || !is_ident_start(chars[start]) {
        return None;
    }
    let mut end = start;
    while end < chars.len() && is_ident_continue(chars[end]) {
        end += 1;
    }
    Some((start, end))
}

/// End of a `*/` run starting at `from` (past the opening `/*`).
pub(super) fn find_block_end(chars: &[char], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == '*' && chars[i + 1] == '/' {
            return Some(i + 2);
        }
        i += 1;
    }
    None
}

/// Scan a template literal starting at `start` (the opening backtick).
///
/// Emits the template text and `${` boundaries, and returns the index reached
/// with the carry-over mode: `Template` (or an interpolation) when the literal is
/// still open at the end of the line, otherwise `None`.
pub(super) fn scan_template(
    chars: &[char],
    start: usize,
    spans: &mut Vec<HighlightSpan>,
) -> (usize, LexMode) {
    push_merged(
        spans,
        HighlightSpan::new(start, start + 1, TokenKind::String),
    );
    scan_template_text(chars, start + 1, spans)
}

/// Scan template text from `i` until the closing backtick or a `${`.
pub(super) fn scan_template_text(
    chars: &[char],
    mut i: usize,
    spans: &mut Vec<HighlightSpan>,
) -> (usize, LexMode) {
    let len = chars.len();
    let start = i;
    while i < len {
        match chars[i] {
            '\\' => i = (i + 2).min(len),
            '`' => {
                push_merged(spans, HighlightSpan::new(start, i + 1, TokenKind::String));
                return (i + 1, LexMode::None);
            }
            '$' if chars.get(i + 1) == Some(&'{') => {
                push_merged(spans, HighlightSpan::new(start, i, TokenKind::String));
                push_merged(spans, HighlightSpan::new(i, i + 2, TokenKind::Operator));
                return (i + 2, LexMode::TemplateInterpolation(1));
            }
            _ => i += 1,
        }
    }
    push_merged(spans, HighlightSpan::new(start, len, TokenKind::String));
    (len, LexMode::Template)
}

/// The word ending immediately before `end` (skipping whitespace), if any.
pub(super) fn word_before(chars: &[char], end: usize) -> String {
    let mut j = end;
    while j > 0 && chars[j - 1].is_whitespace() {
        j -= 1;
    }
    let mut k = j;
    while k > 0 && is_ident_continue(chars[k - 1]) {
        k -= 1;
    }
    chars[k..j].iter().collect()
}

/// Whether a `/` at `i` begins a regex literal rather than a division.
///
/// A regex may follow an expression-start operator or a keyword that takes an
/// expression; after a value (identifier, number, `)`, `]`) it is division.
pub(super) fn regex_can_start(chars: &[char], i: usize) -> bool {
    let mut j = i;
    while j > 0 && chars[j - 1].is_whitespace() {
        j -= 1;
    }
    if j == 0 {
        return true;
    }
    let prev = chars[j - 1];
    if matches!(
        prev,
        '(' | ','
            | '='
            | ':'
            | '['
            | '!'
            | '&'
            | '|'
            | '?'
            | '{'
            | '}'
            | ';'
            | '+'
            | '-'
            | '*'
            | '%'
            | '<'
            | '>'
            | '~'
            | '^'
    ) {
        return true;
    }
    if is_ident_continue(prev) {
        return matches!(
            word_before(chars, j).as_str(),
            "return"
                | "typeof"
                | "instanceof"
                | "in"
                | "of"
                | "new"
                | "delete"
                | "void"
                | "case"
                | "do"
                | "else"
                | "yield"
                | "await"
                | "default"
        );
    }
    false
}

/// The end of a regex literal starting at `/`, honouring escapes and classes.
pub(super) fn scan_regex(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut in_class = false;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '[' => {
                in_class = true;
                i += 1;
            }
            ']' => {
                in_class = false;
                i += 1;
            }
            '/' if !in_class => {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_ascii_alphabetic() {
                    j += 1;
                }
                return Some(j);
            }
            _ => i += 1,
        }
    }
    None
}

/// Whether a `<` at `i` opens a JSX element rather than a comparison/generic.
pub(super) fn jsx_can_start(chars: &[char], i: usize) -> bool {
    let Some(next) = chars.get(i + 1) else {
        return false;
    };
    // A closing tag `</…>` can follow text content, so it is accepted wherever
    // it appears (`< /` is not valid JavaScript).
    if *next == '/' {
        return chars
            .get(i + 2)
            .is_some_and(|c| c.is_alphabetic() || *c == '_' || *c == '$' || *c == '>');
    }
    if !(next.is_alphabetic() || *next == '_' || *next == '$' || *next == '>') {
        return false;
    }
    let mut j = i;
    while j > 0 && chars[j - 1].is_whitespace() {
        j -= 1;
    }
    if j == 0 {
        return true;
    }
    let prev = chars[j - 1];
    if matches!(
        prev,
        '(' | ',' | '=' | '>' | ':' | '[' | '!' | '&' | '|' | '?' | '{' | '}' | ';' | '+'
    ) {
        return true;
    }
    if is_ident_continue(prev) {
        return matches!(
            word_before(chars, j).as_str(),
            "return" | "default" | "else" | "yield" | "await" | "case" | "in" | "of"
        );
    }
    false
}

/// Highlight a JSX tag starting at `<`, returning the index past it.
pub(super) fn highlight_jsx(
    chars: &[char],
    start: usize,
    spans: &mut Vec<HighlightSpan>,
) -> (usize, LexMode) {
    let len = chars.len();
    let mut i = start + 1;
    let closing = chars.get(i) == Some(&'/');
    if closing {
        i += 1;
    }
    // A fragment `<>` or closing fragment `</>`.
    if chars.get(i) == Some(&'>') {
        push_merged(spans, HighlightSpan::new(start, i + 1, TokenKind::Operator));
        return (i + 1, LexMode::None);
    }
    let name_start = i;
    while i < len && (is_ident_continue(chars[i]) || matches!(chars[i], '.' | ':' | '-')) {
        i += 1;
    }
    if i == name_start {
        return (start, LexMode::None); // Not a tag; let the caller treat `<` as an operator.
    }
    push_merged(spans, HighlightSpan::new(name_start, i, TokenKind::Type));
    scan_jsx_attributes(chars, i, spans)
}

/// Highlight JSX tag attributes from `i` until the closing `>`.
///
/// Returns the index reached and a carry-over mode: `JsxTag` when the tag is
/// still open at the end of the line (its attributes continue on the next one).
pub(super) fn scan_jsx_attributes(
    chars: &[char],
    mut i: usize,
    spans: &mut Vec<HighlightSpan>,
) -> (usize, LexMode) {
    let len = chars.len();
    while i < len && chars[i] != '>' {
        let c = chars[i];
        if c.is_whitespace() || c == '/' {
            i += 1;
            continue;
        }
        if c == '{' {
            i = scan_braces(chars, i);
            continue;
        }
        if c == '"' || c == '\'' {
            let end = scan_quoted(chars, i, c);
            push_merged(spans, HighlightSpan::new(i, end, TokenKind::String));
            i = end;
            continue;
        }
        if c == '=' {
            push_merged(spans, HighlightSpan::new(i, i + 1, TokenKind::Operator));
            i += 1;
            continue;
        }
        let attr_start = i;
        while i < len
            && !chars[i].is_whitespace()
            && !matches!(chars[i], '=' | '>' | '/' | '"' | '\'' | '{')
        {
            i += 1;
        }
        push_merged(
            spans,
            HighlightSpan::new(attr_start, i, TokenKind::Attribute),
        );
    }
    if i < len {
        push_merged(spans, HighlightSpan::new(i, i + 1, TokenKind::Operator));
        return (i + 1, LexMode::None);
    }
    (len, LexMode::JsxTag)
}

/// The index just past the `}` matching the `{` at `start`.
pub(super) fn scan_braces(chars: &[char], start: usize) -> usize {
    let mut depth = 0usize;
    let mut i = start;
    while i < chars.len() {
        match chars[i] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    chars.len()
}

/// Words that may precede a declaration keyword.
const MODIFIERS: &[&str] = &[
    "export",
    "default",
    "async",
    "declare",
    "abstract",
    "public",
    "private",
    "protected",
    "static",
    "readonly",
    "override",
];

/// Named top-level declarations in a document.
pub(super) fn web_symbols(text: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    for (row, line) in text.lines().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let mut i = skip_ws(&chars, 0);
        // Skip leading modifiers so `export default class` still registers.
        loop {
            let (word, next) = word_at(&chars, i);
            if MODIFIERS.contains(&word.as_str()) {
                i = skip_ws(&chars, next);
            } else {
                break;
            }
        }
        let (keyword, next) = word_at(&chars, i);
        let kind = match keyword.as_str() {
            "function" => SymbolKind::Function,
            "class" => SymbolKind::Type,
            "interface" => SymbolKind::Interface,
            "enum" => SymbolKind::Enum,
            "type" => SymbolKind::Type,
            "namespace" | "module" => SymbolKind::Module,
            "const" => SymbolKind::Constant,
            "let" | "var" => SymbolKind::Variable,
            _ => continue,
        };
        let mut j = skip_ws(&chars, next);
        if keyword == "function" && chars.get(j) == Some(&'*') {
            j = skip_ws(&chars, j + 1);
        }
        let (name, _) = word_at(&chars, j);
        if name.is_empty() {
            continue;
        }
        symbols.push(Symbol::new(name, kind, row, j));
    }
    symbols
}

pub(super) fn skip_ws(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

/// The identifier starting at `i` and the index just past it.
pub(super) fn word_at(chars: &[char], i: usize) -> (String, usize) {
    if i >= chars.len() || !is_ident_start(chars[i]) {
        return (String::new(), i);
    }
    let mut end = i;
    while end < chars.len() && is_ident_continue(chars[end]) {
        end += 1;
    }
    (chars[i..end].iter().collect(), end)
}

pub(super) fn is_ident_start(c: char) -> bool {
    c == '_' || c == '$' || c.is_alphabetic()
}

pub(super) fn is_ident_continue(c: char) -> bool {
    c == '_' || c == '$' || c.is_alphanumeric()
}

pub(super) fn is_operator(c: char) -> bool {
    matches!(
        c,
        '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '!' | '&' | '|' | '^' | '~' | '?' | ':'
    )
}
