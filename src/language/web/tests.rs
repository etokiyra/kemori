use super::*;

#[test]
fn descriptors_claim_the_right_extensions() {
    let ts = WebProvider::typescript().descriptor();
    assert_eq!(ts.id, LanguageId::TypeScript);
    assert!(ts.extensions.contains(&"ts") && ts.extensions.contains(&"tsx"));
    let js = WebProvider::javascript().descriptor();
    assert_eq!(js.id, LanguageId::JavaScript);
    assert!(js.extensions.contains(&"js") && js.extensions.contains(&"jsx"));
}

#[test]
fn highlights_comments_strings_and_keywords() {
    let provider = WebProvider::typescript();
    let (spans, state) =
        provider.highlight("const name = \"kemori\"; // hi", HighlightState::default());
    assert!(!state.in_block_comment);
    let kind_at = |col: usize| {
        spans
            .iter()
            .find(|span| span.range.contains(&col))
            .map(|span| span.kind)
    };
    assert_eq!(kind_at(0), Some(TokenKind::Keyword)); // const
    assert_eq!(kind_at(14), Some(TokenKind::String)); // "kemori"
    assert_eq!(kind_at(23), Some(TokenKind::Comment)); // // hi
}

#[test]
fn carries_block_comments_across_lines() {
    let provider = WebProvider::typescript();
    let (_, state) = provider.highlight("/* open", HighlightState::default());
    assert!(state.in_block_comment);
    let (spans, state) = provider.highlight(" still open */ let x = 1;", state);
    assert!(!state.in_block_comment);
    assert!(
        spans
            .iter()
            .any(|span| span.kind == TokenKind::Comment && span.range.contains(&0))
    );
}

#[test]
fn carries_template_literals_and_interpolation_across_lines() {
    let provider = WebProvider::typescript();
    // A template opened on one line stays a string on the next.
    let (spans, state) = provider.highlight("const s = `hello", HighlightState::default());
    assert_eq!(state.mode, LexMode::Template);
    assert_eq!(kind_at(&spans, 10), Some(TokenKind::String));

    // `${name}` resumes code, and the text after `}` is a string again.
    let (spans, state) = provider.highlight("world ${name} tail`", state);
    assert_eq!(state.mode, LexMode::None);
    assert_eq!(kind_at(&spans, 1), Some(TokenKind::String)); // "world "
    assert_eq!(kind_at(&spans, 9), Some(TokenKind::Plain)); // name
    assert_eq!(kind_at(&spans, 16), Some(TokenKind::String)); // " tail`"
}

#[test]
fn carries_jsx_tags_across_lines() {
    let provider = WebProvider::typescript();
    let (spans, state) = provider.highlight("const el = <Panel", HighlightState::default());
    assert_eq!(state.mode, LexMode::JsxTag);
    assert_eq!(kind_at(&spans, 12), Some(TokenKind::Type)); // Panel

    let (spans, state) = provider.highlight("  title=\"hi\"", state);
    assert_eq!(state.mode, LexMode::JsxTag);
    assert_eq!(kind_at(&spans, 2), Some(TokenKind::Attribute)); // title
    assert_eq!(kind_at(&spans, 8), Some(TokenKind::String)); // "hi"

    let (spans, state) = provider.highlight("  visible>", state);
    assert_eq!(state.mode, LexMode::None);
    assert_eq!(kind_at(&spans, 2), Some(TokenKind::Attribute)); // visible
    assert_eq!(kind_at(&spans, 9), Some(TokenKind::Operator)); // >
}

#[test]
fn an_open_interpolation_carries_its_depth() {
    let provider = WebProvider::typescript();
    let (_, state) = provider.highlight("`a ${b", HighlightState::default());
    assert_eq!(state.mode, LexMode::TemplateInterpolation(1));
    // The closing brace returns to template text, which needs a backtick.
    let (spans, state) = provider.highlight("} c`", state);
    assert_eq!(state.mode, LexMode::None);
    assert_eq!(kind_at(&spans, 0), Some(TokenKind::Operator)); // }
    assert_eq!(kind_at(&spans, 2), Some(TokenKind::String)); // c`
}

#[test]
fn interpolation_braces_are_balanced() {
    let provider = WebProvider::typescript();
    // A nested `{}` inside the interpolation must not close it early.
    let (_, state) = provider.highlight("`a ${ ({x: 1}) } b`", HighlightState::default());
    assert_eq!(state.mode, LexMode::None);
}

#[test]
fn extracts_declarations() {
    let text = "import x from \"y\";\n\
                    export async function run() {}\n\
                    export default class Server {}\n\
                    interface Options {}\n\
                    type Handler = () => void;\n\
                    const PORT = 8080;\n";
    let symbols = web_symbols(text);
    let names: Vec<_> = symbols
        .iter()
        .map(|symbol| (symbol.name.as_str(), symbol.kind))
        .collect();
    assert!(names.contains(&("run", SymbolKind::Function)));
    assert!(names.contains(&("Server", SymbolKind::Type)));
    assert!(names.contains(&("Options", SymbolKind::Interface)));
    assert!(names.contains(&("Handler", SymbolKind::Type)));
    assert!(names.contains(&("PORT", SymbolKind::Constant)));
}

fn kind_at(spans: &[HighlightSpan], col: usize) -> Option<TokenKind> {
    spans
        .iter()
        .find(|span| span.range.contains(&col))
        .map(|span| span.kind)
}

#[test]
fn regex_literals_are_strings_but_division_is_an_operator() {
    let provider = WebProvider::javascript();
    let (spans, _) = provider.highlight("let re = /ab+c/gi;", HighlightState::default());
    assert_eq!(kind_at(&spans, 9), Some(TokenKind::String)); // /ab+c/gi
    assert_eq!(kind_at(&spans, 16), Some(TokenKind::String)); // flags included

    let (spans, _) = provider.highlight("let x = a / b / c;", HighlightState::default());
    assert_eq!(kind_at(&spans, 10), Some(TokenKind::Operator)); // first /
    assert_eq!(kind_at(&spans, 14), Some(TokenKind::Operator)); // second /

    // A regex after `return` is not division.
    let (spans, _) = provider.highlight("return /x/.test(s)", HighlightState::default());
    assert_eq!(kind_at(&spans, 7), Some(TokenKind::String));
}

#[test]
fn jsx_tags_and_attributes_highlight() {
    let provider = WebProvider::typescript();
    let (spans, _) = provider.highlight(
        "return <div className=\"card\">Hi</div>;",
        HighlightState::default(),
    );
    let div = "return <div className=\"card\">Hi</div>;";
    let div_col = div.find("div").unwrap();
    assert_eq!(kind_at(&spans, div_col), Some(TokenKind::Type));
    assert_eq!(kind_at(&spans, 12), Some(TokenKind::Attribute)); // className
    assert_eq!(kind_at(&spans, 22), Some(TokenKind::String)); // "card"
    // The closing tag's name is highlighted too.
    let close_col = div.rfind("div").unwrap();
    assert_eq!(kind_at(&spans, close_col), Some(TokenKind::Type));
}

#[test]
fn generics_and_comparisons_are_not_jsx() {
    let provider = WebProvider::typescript();
    let (spans, _) = provider.highlight("let xs: Array<Foo> = [];", HighlightState::default());
    assert_eq!(kind_at(&spans, 13), Some(TokenKind::Operator)); // <
    let (spans, _) = provider.highlight("if (a < b) {}", HighlightState::default());
    assert_eq!(kind_at(&spans, 6), Some(TokenKind::Operator)); // <
}

#[test]
fn jsx_fragments_and_self_closing_tags() {
    let provider = WebProvider::javascript();
    let (spans, _) = provider.highlight("return <></>;", HighlightState::default());
    assert_eq!(kind_at(&spans, 7), Some(TokenKind::Operator)); // <
    let (spans, _) =
        provider.highlight("const el = <Input value={x} />;", HighlightState::default());
    let input_col = "const el = <Input value={x} />;".find("Input").unwrap();
    assert_eq!(kind_at(&spans, input_col), Some(TokenKind::Type));
}

#[test]
fn web_providers_offer_prettier_formatting() {
    for provider in [WebProvider::typescript(), WebProvider::javascript()] {
        assert!(
            provider.capabilities().contains(&Capability::Formatting),
            "web providers expose Formatting"
        );
        assert_eq!(provider.formatter(), Some("prettier"));
    }
}
