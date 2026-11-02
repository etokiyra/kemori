//! Dev harness: install a managed tool through Kemori's real provisioning path.
//!
//! ```text
//! XDG_DATA_HOME=/tmp/kemori-tools cargo run --example install_check -- omnisharp
//! ```
//!
//! Not part of the app; it exists so the managed download/verify/extract flow
//! can be exercised against the real network and probed, end to end.

fn main() {
    let tool = match std::env::args().nth(1).as_deref() {
        Some("omnisharp") => kemori::language::tools::Tool::OmniSharp,
        Some("jdtls") => kemori::language::tools::Tool::Jdtls,
        Some("html") => kemori::language::tools::Tool::HtmlLs,
        Some("css") => kemori::language::tools::Tool::CssLs,
        Some("lua") => kemori::language::tools::Tool::LuaLs,
        Some("kotlin") => kemori::language::tools::Tool::KotlinLs,
        Some("sql") => kemori::language::tools::Tool::Sqls,
        Some("asm") => kemori::language::tools::Tool::AsmLsp,
        Some("dart") => kemori::language::tools::Tool::DartAnalyzer,
        Some("elixir") => kemori::language::tools::Tool::ElixirLs,
        Some("swift") => kemori::language::tools::Tool::SwiftLs,
        Some("perl") => kemori::language::tools::Tool::PerlLs,
        Some("pls") => kemori::language::tools::Tool::Pls,
        Some("gopls") => kemori::language::tools::Tool::Gopls,
        Some("shfmt") => kemori::language::tools::Tool::Shfmt,
        Some("clangd") => kemori::language::tools::Tool::Clangd,
        Some("gofmt") => kemori::language::tools::Tool::Gofmt,
        Some("phpactor") => kemori::language::tools::Tool::Phpactor,
        Some("ruby") => kemori::language::tools::Tool::RubyLs,
        other => {
            eprintln!(
                "usage: install_check <omnisharp|jdtls|html|css|lua|kotlin|sql|asm|dart|elixir|swift|perl|pls|gopls|shfmt|clangd|phpactor|ruby> (got {other:?})"
            );
            std::process::exit(2);
        }
    };

    println!("installing {:?}…", tool);
    match kemori::language::tools::install(tool) {
        Ok(message) => {
            println!("OK: {message}");
            let registry = kemori::language::tools::ToolRegistry::discover();
            if let Some(status) = registry.status(tool) {
                println!(
                    "probe: available={} version={:?}",
                    status.available, status.version
                );
            }
        }
        Err(error) => {
            eprintln!("ERR: {error}");
            std::process::exit(1);
        }
    }
}
