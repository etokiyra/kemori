//! TypeScript and JavaScript provider.
//!
//! One scanner serves both languages because their surface syntax overlaps
//! almost entirely; the two differ in a handful of keywords and types and in
//! which files they claim. Like the other providers this is entirely built-in
//! and offline: highlighting, structural diagnostics, symbols, completion,
//! hover and within-file navigation. `typescript-language-server` can be
//! provisioned for rename, code actions and richer, type-aware analysis.
//!
//! Multi-line block comments, template literals and JSX tags carry over between
//! lines; a `${ … }` interpolation resumes code highlighting and returns to
//! template text once its braces close.

use crate::language::completion::{Completion, CompletionKind};
use crate::language::data::{push_merged, scan_number, scan_quoted};
use crate::language::detection::LanguageDescriptor;
use crate::language::diagnostics::Diagnostic;
use crate::language::format::FormatOutcome;
use crate::language::id::LanguageId;
use crate::language::provider::{
    Capability, HighlightSpan, HighlightState, LanguageProvider, LexMode, TokenKind,
};
use crate::language::symbols::{Symbol, SymbolKind};

use std::path::Path;

/// Keywords common to JavaScript and TypeScript.
const KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "asserts",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "declare",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "finally",
    "for",
    "from",
    "function",
    "get",
    "if",
    "implements",
    "import",
    "in",
    "infer",
    "instanceof",
    "interface",
    "is",
    "keyof",
    "let",
    "namespace",
    "new",
    "of",
    "override",
    "private",
    "protected",
    "public",
    "readonly",
    "return",
    "satisfies",
    "set",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "try",
    "type",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// TypeScript primitive and utility types.
const TYPES: &[&str] = &[
    "any",
    "bigint",
    "boolean",
    "never",
    "number",
    "object",
    "string",
    "symbol",
    "undefined",
    "unknown",
    "Array",
    "ReadonlyArray",
    "Promise",
    "Record",
    "Partial",
    "Required",
    "Readonly",
    "Pick",
    "Omit",
    "Exclude",
    "Extract",
    "ReturnType",
    "Parameters",
];

const BUILTINS: &[&str] = &[
    "Array",
    "BigInt",
    "Boolean",
    "Date",
    "Error",
    "Function",
    "JSON",
    "Map",
    "Math",
    "Number",
    "Object",
    "Promise",
    "RegExp",
    "Set",
    "String",
    "Symbol",
    "WeakMap",
    "clearInterval",
    "clearTimeout",
    "console",
    "decodeURIComponent",
    "document",
    "encodeURIComponent",
    "exports",
    "fetch",
    "globalThis",
    "isFinite",
    "isNaN",
    "module",
    "parseFloat",
    "parseInt",
    "process",
    "queueMicrotask",
    "require",
    "setInterval",
    "setTimeout",
    "structuredClone",
    "window",
];

const CONSTANTS: &[&str] = &["true", "false", "null", "undefined", "NaN", "Infinity"];

/// A provider instance for one of the two web languages.
pub struct WebProvider {
    id: LanguageId,
}

impl WebProvider {
    pub fn typescript() -> Self {
        WebProvider {
            id: LanguageId::TypeScript,
        }
    }

    pub fn javascript() -> Self {
        WebProvider {
            id: LanguageId::JavaScript,
        }
    }
}

impl LanguageProvider for WebProvider {
    fn id(&self) -> LanguageId {
        self.id
    }

    fn display_name(&self) -> &'static str {
        self.id.name()
    }

    fn descriptor(&self) -> LanguageDescriptor {
        match self.id {
            LanguageId::TypeScript => LanguageDescriptor {
                id: LanguageId::TypeScript,
                extensions: &["ts", "tsx", "mts", "cts"],
                project_markers: &["tsconfig.json", "package.json"],
                file_names: &[],
                shebangs: &[],
                // TypeScript is a superset of JavaScript, so it shares the
                // generic hints and adds its own; otherwise a `.tsx` file with
                // only JS-shaped content would lose to the JavaScript provider.
                content_hints: &[
                    "interface ",
                    "type ",
                    ": string",
                    ": number",
                    "=>",
                    "const ",
                ],
            },
            _ => LanguageDescriptor {
                id: LanguageId::JavaScript,
                extensions: &["js", "jsx", "mjs", "cjs"],
                project_markers: &["package.json"],
                file_names: &[],
                shebangs: &["node"],
                content_hints: &["const ", "=>", "console.log", "function "],
            },
        }
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[
            Capability::SyntaxHighlighting,
            Capability::Diagnostics,
            Capability::DocumentSymbols,
            Capability::GotoDefinition,
            Capability::GotoReference,
            Capability::Completion,
            Capability::Hover,
            Capability::Formatting,
        ]
    }

    fn diagnostics(&self, text: &str) -> Vec<Diagnostic> {
        crate::language::diagnostics::check_delimiters(self, text)
    }

    fn symbols(&self, text: &str) -> Vec<Symbol> {
        web_symbols(text)
    }

    fn definition(
        &self,
        text: &str,
        line: usize,
        col: usize,
    ) -> Option<crate::language::symbols::Symbol> {
        let word = crate::language::symbols::word_at(text, line, col)?;
        web_symbols(text)
            .into_iter()
            .find(|symbol| symbol.name == word)
    }

    fn references(
        &self,
        text: &str,
        line: usize,
        col: usize,
    ) -> Vec<crate::language::symbols::Location> {
        match crate::language::symbols::word_at(text, line, col) {
            Some(word) => crate::language::symbols::locations_of_word(text, &word),
            None => Vec::new(),
        }
    }

    fn completions(&self, _text: &str, _line: usize, _col: usize) -> Vec<Completion> {
        let mut completions: Vec<Completion> = KEYWORDS
            .iter()
            .map(|word| Completion::new(*word, CompletionKind::Keyword))
            .collect();
        completions.extend(
            TYPES
                .iter()
                .map(|word| Completion::new(*word, CompletionKind::Type)),
        );
        completions.extend(
            BUILTINS
                .iter()
                .map(|word| Completion::new(*word, CompletionKind::Function)),
        );
        completions.extend(
            CONSTANTS
                .iter()
                .map(|word| Completion::new(*word, CompletionKind::Constant)),
        );
        completions
    }

    fn format(&self, path: &Path, text: &str) -> FormatOutcome {
        crate::language::format::prettier(path, text)
    }

    fn formatter(&self) -> Option<&'static str> {
        Some("prettier")
    }

    fn hover(&self, text: &str, line: usize, col: usize) -> Option<crate::language::hover::Hover> {
        let symbols = web_symbols(text);
        crate::language::hover::describe(text, line, col, &symbols)
    }

    fn line_comment(&self) -> &'static str {
        "//"
    }

    fn highlight(&self, line: &str, state: HighlightState) -> (Vec<HighlightSpan>, HighlightState) {
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut spans = Vec::new();
        let mut i = 0;
        let mut mode = state.mode;
        let mut in_block_comment = state.in_block_comment;

        // Finish a block comment carried over from the previous line.
        if in_block_comment {
            match find_block_end(&chars, 0) {
                Some(end) => {
                    push_merged(&mut spans, HighlightSpan::new(0, end, TokenKind::Comment));
                    i = end;
                    in_block_comment = false;
                }
                None => {
                    if len > 0 {
                        push_merged(&mut spans, HighlightSpan::new(0, len, TokenKind::Comment));
                    }
                    return (
                        spans,
                        HighlightState {
                            in_block_comment: true,
                            block_comment_depth: 0,
                            mode,
                            embed: state.embed,
                        },
                    );
                }
            }
        }

        // Resume template text carried over from the previous line.
        if mode == LexMode::Template {
            let (next, next_mode) = scan_template_text(&chars, i, &mut spans);
            i = next;
            mode = next_mode;
        }

        // Resume the attributes of a JSX tag that spanned lines.
        if mode == LexMode::JsxTag {
            let (next, next_mode) = scan_jsx_attributes(&chars, i, &mut spans);
            i = next;
            mode = next_mode;
        }

        let first_nonspace = chars.iter().position(|c| !c.is_whitespace());

        while i < len {
            let c = chars[i];

            // Inside `${ ... }` the braces must be balanced so template text can
            // resume once the interpolation closes.
            if let LexMode::TemplateInterpolation(depth) = mode {
                if c == '{' {
                    push_merged(
                        &mut spans,
                        HighlightSpan::new(i, i + 1, TokenKind::Operator),
                    );
                    mode = LexMode::TemplateInterpolation(depth.saturating_add(1));
                    i += 1;
                    continue;
                }
                if c == '}' {
                    push_merged(
                        &mut spans,
                        HighlightSpan::new(i, i + 1, TokenKind::Operator),
                    );
                    i += 1;
                    if depth <= 1 {
                        let (next, next_mode) = scan_template_text(&chars, i, &mut spans);
                        i = next;
                        mode = next_mode;
                    } else {
                        mode = LexMode::TemplateInterpolation(depth - 1);
                    }
                    continue;
                }
            }

            if c == '/' && chars.get(i + 1) == Some(&'/') {
                push_merged(&mut spans, HighlightSpan::new(i, len, TokenKind::Comment));
                break;
            }

            if c == '/' && chars.get(i + 1) == Some(&'*') {
                match find_block_end(&chars, i + 2) {
                    Some(end) => {
                        push_merged(&mut spans, HighlightSpan::new(i, end, TokenKind::Comment));
                        i = end;
                    }
                    None => {
                        push_merged(&mut spans, HighlightSpan::new(i, len, TokenKind::Comment));
                        return (
                            spans,
                            HighlightState {
                                in_block_comment: true,
                                block_comment_depth: 0,
                                mode,
                                embed: state.embed,
                            },
                        );
                    }
                }
                continue;
            }

            // A `/` in expression position starts a regex literal; otherwise it
            // stays a division operator. This is the usual lookback heuristic.
            if c == '/'
                && regex_can_start(&chars, i)
                && let Some(end) = scan_regex(&chars, i)
            {
                push_merged(&mut spans, HighlightSpan::new(i, end, TokenKind::String));
                i = end;
                continue;
            }

            if c == '\'' || c == '"' {
                let end = scan_quoted(&chars, i, c);
                push_merged(&mut spans, HighlightSpan::new(i, end, TokenKind::String));
                i = end;
                continue;
            }

            if c == '`' {
                let saved = mode;
                let (end, returned) = scan_template(&chars, i, &mut spans);
                i = end;
                // A nested template that closes on this line restores the
                // enclosing interpolation; otherwise the returned mode carries.
                mode = match (saved, returned) {
                    (LexMode::TemplateInterpolation(depth), LexMode::None) => {
                        LexMode::TemplateInterpolation(depth)
                    }
                    _ => returned,
                };
                continue;
            }

            if c.is_ascii_digit() {
                let end = scan_number(&chars, i);
                push_merged(&mut spans, HighlightSpan::new(i, end, TokenKind::Number));
                i = end;
                continue;
            }

            // A decorator only appears at the start of a line.
            if c == '@' && Some(i) == first_nonspace {
                let mut j = i + 1;
                while j < len && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == '.')
                {
                    j += 1;
                }
                push_merged(&mut spans, HighlightSpan::new(i, j, TokenKind::Attribute));
                i = j;
                continue;
            }

            if is_ident_start(c) {
                let mut j = i;
                while j < len && is_ident_continue(chars[j]) {
                    j += 1;
                }
                let word: String = chars[i..j].iter().collect();
                push_merged(&mut spans, HighlightSpan::new(i, j, classify_word(&word)));

                // `function name` / `class Name`: highlight the name too.
                let name_kind = match word.as_str() {
                    "function" => Some(TokenKind::Function),
                    "class" => Some(TokenKind::Type),
                    _ => None,
                };
                if let Some(kind) = name_kind
                    && let Some((start, end)) = name_after(&chars, j)
                {
                    push_merged(&mut spans, HighlightSpan::new(start, end, kind));
                    i = end;
                    continue;
                }
                i = j;
                continue;
            }

            // A `<` in expression position opens a JSX element; a comparison or
            // generic angle bracket stays an operator.
            if c == '<' && jsx_can_start(&chars, i) {
                let (end, jsx_mode) = highlight_jsx(&chars, i, &mut spans);
                if end > i {
                    i = end;
                    if jsx_mode != LexMode::None {
                        mode = jsx_mode;
                    }
                    continue;
                }
            }

            if is_operator(c) {
                push_merged(
                    &mut spans,
                    HighlightSpan::new(i, i + 1, TokenKind::Operator),
                );
            }
            i += 1;
        }

        (
            spans,
            HighlightState {
                in_block_comment,
                block_comment_depth: 0,
                mode,
                embed: state.embed,
            },
        )
    }
}

mod tokenizer;
use tokenizer::*;

#[cfg(test)]
mod tests;
