// Install hints, extracted from `mod.rs`.

impl Tool {
    pub fn install_hint(self) -> &'static str {
        match self {
            Tool::RustAnalyzer => "install with `rustup component add rust-analyzer`",
            Tool::Gopls => "Kemori installs gopls with `go install`, provisioning Go if missing",
            Tool::Dlv => "Kemori installs Delve with `go install`, provisioning Go if missing",
            Tool::Pylsp => "install `python-lsp-server` into a Kemori-managed environment",
            Tool::Debugpy => {
                "Kemori installs the pinned `debugpy` adapter into an isolated environment"
            }
            Tool::LldbDap => {
                "install LLVM's lldb-dap (your distribution's lldb package or `brew install llvm`)"
            }
            Tool::JsDebug => {
                "Kemori downloads the official js-debug DAP bundle and manages Node.js if needed"
            }
            Tool::Netcoredbg => {
                "Kemori downloads the verified netcoredbg adapter for this platform"
            }
            Tool::DartDebug => "Kemori installs the Dart SDK, which includes `dart debug_adapter`",
            Tool::JavaDebug => "Kemori installs the pinned java-debug extension for JDT LS",
            Tool::Rdbg => {
                "Kemori installs the pinned `debug` gem into an isolated gem home (needs Ruby)"
            }
            Tool::PhpDebug => {
                "Kemori downloads the official PHP DAP bridge; Xdebug must be enabled in PHP"
            }
            Tool::BashLs => "install with npm — Kemori provisions Node.js if missing",
            Tool::TypeScriptLs => "install with npm — Kemori provisions Node.js if missing",
            Tool::Clangd => "Kemori can install the official clangd release",
            Tool::Jdtls => "Kemori can install a managed JDK and Eclipse JDT",
            Tool::OmniSharp => "Kemori can install the .NET SDK and OmniSharp",
            Tool::KotlinLs => "Kemori can install a managed JDK 21 and kotlin-language-server",
            Tool::Phpactor => "Kemori can install phpactor.phar (needs a PHP runtime)",
            Tool::LuaLs => "Kemori can install a self-contained lua-language-server",
            Tool::Sqls => "Kemori installs sqls with `go install`, provisioning Go if missing",
            Tool::RubyLs => "Kemori installs solargraph into an isolated gem home (needs Ruby)",
            Tool::AsmLsp => "Kemori can install the prebuilt asm-lsp release",
            Tool::PerlLs => {
                "Kemori bootstraps cpanm and installs Perl::LanguageServer into an isolated local::lib"
            }
            Tool::Pls => "Kemori bootstraps cpanm and installs PLS into an isolated local::lib",
            Tool::DartAnalyzer => "Kemori can install a managed Dart SDK for the analysis server",
            Tool::ElixirLs => "Kemori can install Erlang/OTP, Elixir and ElixirLS",
            Tool::SwiftLs => {
                "Kemori can install a GPG-verified Swift toolchain on supported Linux distributions"
            }
            Tool::HtmlLs | Tool::CssLs => "install with npm — Kemori provisions Node.js if missing",
            Tool::Rustfmt => "install with `rustup component add rustfmt`",
            Tool::Gofmt => "Kemori can install the official Go toolchain, which includes gofmt",
            Tool::Prettier => "install with npm — Kemori provisions Node.js if missing",
            Tool::ClangFormat => "it ships with the Clang/LLVM toolchain",
            Tool::Shfmt => "install it with `go install mvdan.cc/sh/v3/cmd/shfmt@v3.14.1`",
            Tool::PerlTidy => "install it with `cpan Perl::Tidy`",
            Tool::Nil => "Kemori installs nil from a pinned nixpkgs revision with `nix profile`",
            Tool::Nixfmt => {
                "Kemori installs nixfmt from a pinned nixpkgs revision with `nix profile`"
            }
        }
    }
}
