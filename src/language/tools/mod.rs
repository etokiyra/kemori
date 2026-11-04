//! Discovery of the external tools Kemori can drive.
//!
//! Kemori hides tool management: it probes for the language servers and
//! formatters it knows about, installs missing ones through their official
//! package managers, and reports what it finds.
//!
//! A probe checks that the program exists *and* that it runs, because a
//! `rustup` shim can exist for a component that is not installed. Lookups
//! search `PATH` and then a handful of well-known user bin directories, so a
//! tool installed by rustup or pip is found even when Kemori was launched from a
//! GUI or a non-login shell whose `PATH` omits them.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use crate::language::id::LanguageId;
use crate::process::wait_captured;

include!("spec.rs");
include!("hints.rs");
/// A tool Kemori knows how to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    RustAnalyzer,
    Gopls,
    Dlv,
    JsDebug,
    Netcoredbg,
    JavaDebug,
    Rdbg,
    PhpDebug,
    Pylsp,
    Debugpy,
    BashLs,
    TypeScriptLs,
    Clangd,
    Jdtls,
    OmniSharp,
    KotlinLs,
    Phpactor,
    LuaLs,
    Nil,
    Sqls,
    RubyLs,
    AsmLsp,
    PerlLs,
    Pls,
    DartAnalyzer,
    DartDebug,
    ElixirLs,
    SwiftLs,
    HtmlLs,
    CssLs,
    LldbDap,
    Rustfmt,
    Gofmt,
    Prettier,
    ClangFormat,
    Shfmt,
    PerlTidy,
    Nixfmt,
}

/// What a tool is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolPurpose {
    LanguageServer,
    Formatter,
    DebugAdapter,
}

impl Tool {
    /// Every tool Kemori looks for, in the order it is presented.
    pub const ALL: &'static [Tool] = &[
        Tool::RustAnalyzer,
        Tool::Gopls,
        Tool::Dlv,
        Tool::JsDebug,
        Tool::Netcoredbg,
        Tool::JavaDebug,
        Tool::Rdbg,
        Tool::PhpDebug,
        Tool::Pylsp,
        Tool::Debugpy,
        Tool::BashLs,
        Tool::TypeScriptLs,
        Tool::Clangd,
        Tool::Jdtls,
        Tool::OmniSharp,
        Tool::KotlinLs,
        Tool::Phpactor,
        Tool::LuaLs,
        Tool::Nil,
        Tool::Sqls,
        Tool::RubyLs,
        Tool::AsmLsp,
        // PLS is preferred over Perl::LanguageServer: it has no `Coro`
        // dependency, so it builds on current Perls.
        Tool::Pls,
        Tool::PerlLs,
        Tool::DartAnalyzer,
        Tool::DartDebug,
        Tool::ElixirLs,
        Tool::SwiftLs,
        Tool::HtmlLs,
        Tool::CssLs,
        Tool::LldbDap,
        Tool::Rustfmt,
        Tool::Gofmt,
        Tool::Prettier,
        Tool::ClangFormat,
        Tool::Shfmt,
        Tool::PerlTidy,
        Tool::Nixfmt,
    ];

    pub fn program(self) -> &'static str {
        match self {
            Tool::RustAnalyzer => "rust-analyzer",
            Tool::Gopls => "gopls",
            Tool::Dlv => "dlv",
            Tool::JsDebug => "js-debug-dap",
            Tool::Netcoredbg => "netcoredbg",
            Tool::JavaDebug => "java-debug",
            Tool::Rdbg => "rdbg",
            Tool::PhpDebug => "php-debug-dap",
            Tool::Pylsp => "pylsp",
            Tool::Debugpy => "debugpy",
            Tool::BashLs => "bash-language-server",
            Tool::TypeScriptLs => "typescript-language-server",
            Tool::Clangd => "clangd",
            Tool::Jdtls => "jdtls",
            Tool::OmniSharp => "OmniSharp",
            Tool::KotlinLs => "kotlin-language-server",
            Tool::Phpactor => "phpactor",
            Tool::LuaLs => "lua-language-server",
            Tool::Nil => "nil",
            Tool::Sqls => "sqls",
            Tool::RubyLs => "solargraph",
            Tool::AsmLsp => "asm-lsp",
            // `Perl::LanguageServer` has no installed script; it is launched
            // through `perl`. PLS is a normal executable.
            Tool::PerlLs => "perl",
            Tool::Pls => "pls",
            Tool::DartAnalyzer => "dart",
            Tool::DartDebug => "dart",
            Tool::ElixirLs => "elixir-ls",
            Tool::SwiftLs => "sourcekit-lsp",
            Tool::HtmlLs => "vscode-html-language-server",
            Tool::CssLs => "vscode-css-language-server",
            Tool::LldbDap => "lldb-dap",
            Tool::Rustfmt => "rustfmt",
            Tool::Gofmt => "gofmt",
            Tool::Prettier => "prettier",
            Tool::ClangFormat => "clang-format",
            Tool::Shfmt => "shfmt",
            Tool::PerlTidy => "perltidy",
            Tool::Nixfmt => "nixfmt",
        }
    }

    /// Additional executable names a tool may be installed under.
    ///
    /// Some ecosystems ship different launcher names depending on how the tool
    /// was installed (for example ElixirLS's `language_server.sh` release
    /// script versus the `elixir-ls` binary packaged by Homebrew/Mason). Kemori
    /// probes every name so an existing install is still found.
    pub fn candidates(self) -> &'static [&'static str] {
        match self {
            Tool::ElixirLs => &["language_server.sh", "language_server.bat"],
            Tool::LldbDap => &["lldb-vscode"],
            _ => &[],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::RustAnalyzer => "rust-analyzer",
            Tool::Gopls => "gopls",
            Tool::Dlv => "dlv",
            Tool::JsDebug => "js-debug-dap",
            Tool::Netcoredbg => "netcoredbg",
            Tool::JavaDebug => "java-debug",
            Tool::Rdbg => "rdbg",
            Tool::PhpDebug => "php-debug-dap",
            Tool::Pylsp => "pylsp",
            Tool::Debugpy => "debugpy",
            Tool::BashLs => "bash-language-server",
            Tool::TypeScriptLs => "typescript-language-server",
            Tool::Clangd => "clangd",
            Tool::Jdtls => "jdtls",
            Tool::OmniSharp => "omnisharp",
            Tool::KotlinLs => "kotlin-language-server",
            Tool::Phpactor => "phpactor",
            Tool::LuaLs => "lua-language-server",
            Tool::Nil => "nil",
            Tool::Sqls => "sqls",
            Tool::RubyLs => "solargraph",
            Tool::AsmLsp => "asm-lsp",
            Tool::PerlLs => "perl-language-server",
            Tool::Pls => "pls",
            Tool::DartAnalyzer => "dart",
            Tool::DartDebug => "dart",
            Tool::ElixirLs => "elixir-ls",
            Tool::SwiftLs => "sourcekit-lsp",
            Tool::HtmlLs => "vscode-html-language-server",
            Tool::CssLs => "vscode-css-language-server",
            Tool::LldbDap => "lldb-dap",
            Tool::Rustfmt => "rustfmt",
            Tool::Gofmt => "gofmt",
            Tool::Prettier => "prettier",
            Tool::ClangFormat => "clang-format",
            Tool::Shfmt => "shfmt",
            Tool::PerlTidy => "perltidy",
            Tool::Nixfmt => "nixfmt",
        }
    }

    pub fn language(self) -> LanguageId {
        match self {
            Tool::RustAnalyzer | Tool::Rustfmt => LanguageId::Rust,
            Tool::Gopls | Tool::Gofmt | Tool::Dlv => LanguageId::Go,
            Tool::Pylsp | Tool::Debugpy => LanguageId::Python,
            Tool::BashLs => LanguageId::Shell,
            Tool::TypeScriptLs => LanguageId::TypeScript,
            Tool::JsDebug => LanguageId::TypeScript,
            Tool::Clangd => LanguageId::C,
            Tool::LldbDap => LanguageId::C,
            Tool::Jdtls => LanguageId::Java,
            Tool::JavaDebug => LanguageId::Java,
            Tool::OmniSharp => LanguageId::CSharp,
            Tool::Netcoredbg => LanguageId::CSharp,
            Tool::KotlinLs => LanguageId::Kotlin,
            Tool::Phpactor => LanguageId::Php,
            Tool::PhpDebug => LanguageId::Php,
            Tool::LuaLs => LanguageId::Lua,
            Tool::Nil | Tool::Nixfmt => LanguageId::Nix,
            Tool::Sqls => LanguageId::Sql,
            Tool::RubyLs => LanguageId::Ruby,
            Tool::Rdbg => LanguageId::Ruby,
            Tool::AsmLsp => LanguageId::Assembly,
            Tool::PerlLs => LanguageId::Perl,
            Tool::Pls => LanguageId::Perl,
            Tool::DartAnalyzer => LanguageId::Dart,
            Tool::DartDebug => LanguageId::Dart,
            Tool::ElixirLs => LanguageId::Elixir,
            Tool::SwiftLs => LanguageId::Swift,
            Tool::HtmlLs => LanguageId::Html,
            Tool::CssLs => LanguageId::Css,
            Tool::Prettier => LanguageId::JavaScript,
            Tool::ClangFormat => LanguageId::C,
            Tool::Shfmt => LanguageId::Shell,
            Tool::PerlTidy => LanguageId::Perl,
        }
    }

    /// Whether this tool serves `language`. The TypeScript server also handles
    /// JavaScript; clangd serves both C and C++.
    pub fn serves(self, language: LanguageId) -> bool {
        self.language() == language
            || (self == Tool::TypeScriptLs && language == LanguageId::JavaScript)
            || (self == Tool::JsDebug && language == LanguageId::JavaScript)
            || (self == Tool::Clangd && language == LanguageId::Cpp)
            || (self == Tool::ClangFormat && language == LanguageId::Cpp)
            || (self == Tool::LldbDap
                && matches!(language, LanguageId::C | LanguageId::Cpp | LanguageId::Rust))
            || (self == Tool::Prettier
                && matches!(
                    language,
                    LanguageId::TypeScript
                        | LanguageId::JavaScript
                        | LanguageId::Html
                        | LanguageId::Css
                        | LanguageId::Json
                        | LanguageId::Yaml
                        | LanguageId::Markdown
                ))
    }

    pub fn purpose(self) -> ToolPurpose {
        match self {
            Tool::RustAnalyzer
            | Tool::Gopls
            | Tool::Pylsp
            | Tool::BashLs
            | Tool::TypeScriptLs
            | Tool::Clangd
            | Tool::Jdtls
            | Tool::OmniSharp
            | Tool::KotlinLs
            | Tool::Phpactor
            | Tool::LuaLs
            | Tool::Nil
            | Tool::Sqls
            | Tool::RubyLs
            | Tool::AsmLsp
            | Tool::PerlLs
            | Tool::Pls
            | Tool::DartAnalyzer
            | Tool::ElixirLs
            | Tool::SwiftLs
            | Tool::HtmlLs
            | Tool::CssLs => ToolPurpose::LanguageServer,
            Tool::Debugpy => ToolPurpose::DebugAdapter,
            Tool::Dlv => ToolPurpose::DebugAdapter,
            Tool::JsDebug => ToolPurpose::DebugAdapter,
            Tool::Netcoredbg => ToolPurpose::DebugAdapter,
            Tool::JavaDebug => ToolPurpose::DebugAdapter,
            Tool::Rdbg => ToolPurpose::DebugAdapter,
            Tool::PhpDebug => ToolPurpose::DebugAdapter,
            Tool::DartDebug => ToolPurpose::DebugAdapter,
            Tool::LldbDap => ToolPurpose::DebugAdapter,
            Tool::Rustfmt
            | Tool::Gofmt
            | Tool::Prettier
            | Tool::ClangFormat
            | Tool::Shfmt
            | Tool::PerlTidy
            | Tool::Nixfmt => ToolPurpose::Formatter,
        }
    }

    /// Arguments that make the tool print its version. `gofmt` has no version
    /// flag, so it is probed with no arguments against empty stdin.
    fn version_args(self) -> &'static [&'static str] {
        match self {
            Tool::RustAnalyzer
            | Tool::Rustfmt
            | Tool::Pylsp
            | Tool::Debugpy
            | Tool::BashLs
            | Tool::TypeScriptLs
            | Tool::Clangd
            | Tool::KotlinLs
            | Tool::Phpactor
            | Tool::LuaLs
            | Tool::Nil
            | Tool::Sqls
            | Tool::RubyLs
            | Tool::AsmLsp
            | Tool::PerlLs
            | Tool::Pls
            | Tool::DartAnalyzer
            | Tool::ElixirLs
            | Tool::SwiftLs
            | Tool::HtmlLs
            | Tool::CssLs
            | Tool::Prettier
            | Tool::ClangFormat
            | Tool::Shfmt
            | Tool::PerlTidy
            | Tool::Nixfmt => &["--version"],
            Tool::Gopls => &["version"],
            Tool::Dlv => &["version"],
            Tool::Netcoredbg => &["--version"],
            Tool::DartDebug => &["--version"],
            Tool::JavaDebug => &["--version"],
            Tool::Rdbg => &["--version"],
            // `jdtls` and `OmniSharp` have no `--version`; `--help` proves they
            // launch (and, for OmniSharp, that the .NET runtime is present).
            Tool::Jdtls | Tool::OmniSharp => &["--help"],
            Tool::LldbDap | Tool::JsDebug => &["--help"],
            Tool::Gofmt => &[],
            Tool::PhpDebug => &[],
        }
    }

    /// Arguments that start this tool as a language server.
    pub fn server_args(self) -> &'static [&'static str] {
        match self {
            // `bash-language-server` needs its `start` subcommand.
            Tool::BashLs => &["start"],
            // `typescript-language-server` speaks stdio when asked.
            Tool::TypeScriptLs => &["--stdio"],
            // `OmniSharp` needs LSP mode and zero-based (LSP) positions.
            Tool::OmniSharp => &["-z", "--languageserver"],
            // `phpactor` speaks LSP through its `language-server` subcommand.
            Tool::Phpactor => &["language-server"],
            // `solargraph` needs its `stdio` subcommand.
            Tool::RubyLs => &["stdio"],
            // The Dart SDK's analysis server is launched as a subcommand.
            Tool::DartAnalyzer => &["language-server"],
            // `Perl::LanguageServer` has no installed script; it runs as a Perl
            // one-liner over stdio.
            Tool::PerlLs => &["-MPerl::LanguageServer", "-e", "Perl::LanguageServer->run"],
            // The extracted VS Code servers speak stdio.
            Tool::HtmlLs | Tool::CssLs => &["--stdio"],
            _ => &[],
        }
    }

    /// Whether this tool can only be probed by launching its stdio server.
    ///
    /// The extracted VS Code servers reject `--version` and `--help` — they
    /// require a connection mode — so a version probe reports a perfectly good
    /// install as missing. They are verified by starting the server and
    /// confirming it stays up instead.
    fn probe_as_server(self) -> bool {
        matches!(
            self,
            Tool::HtmlLs
                | Tool::CssLs
                | Tool::KotlinLs
                | Tool::Sqls
                | Tool::PerlLs
                | Tool::Pls
                | Tool::ElixirLs
                | Tool::SwiftLs
                | Tool::AsmLsp
                | Tool::PhpDebug
        )
    }

    /// A rough download size for a managed install, when Kemori runs one.
    ///
    /// Used to warn about a large download before it starts. It is an estimate,
    /// not a promise: the archive a distribution serves can change.
    pub fn estimated_download_bytes(self) -> Option<u64> {
        match self {
            Tool::SwiftLs if swift_toolchain().is_some() => Some(1_150_000_000),
            Tool::DartAnalyzer if dart_sdk_asset().is_some() => Some(240_000_000),
            Tool::DartDebug if dart_sdk_asset().is_some() => Some(240_000_000),
            Tool::ElixirLs if bob_platform().is_some() => Some(90_000_000),
            Tool::KotlinLs => Some(260_000_000),
            Tool::Jdtls => Some(260_000_000),
            Tool::OmniSharp => Some(280_000_000),
            Tool::LuaLs => Some(15_000_000),
            Tool::Debugpy => Some(6_000_000),
            Tool::JsDebug if locate("node").is_none() => Some(75_000_000),
            Tool::JsDebug => Some(1_500_000),
            Tool::Netcoredbg => Some(3_500_000),
            Tool::JavaDebug => Some(3_000_000),
            Tool::Rdbg => Some(2_000_000),
            Tool::PhpDebug if locate("node").is_none() => Some(75_000_000),
            Tool::PhpDebug => Some(2_000_000),
            // clangd's self-contained bundle, including its clang resource
            // headers, is about 120 MB per platform.
            Tool::Clangd if clangd_asset().is_some() && !libc_is_musl() => Some(120_000_000),
            // Go's official archive is about 70 MB.
            Tool::Gopls | Tool::Sqls | Tool::Gofmt if go_platform().is_some() => Some(70_000_000),
            Tool::Dlv if go_platform().is_some() => Some(70_000_000),
            _ => None,
        }
    }

    /// A precise, actionable reason a tool is not usable, for Language Setup.
    ///
    /// Unlike [`Tool::install_hint`] this distinguishes "Kemori can install this",
    /// "this platform is unsupported", and "a prerequisite is missing", so the
    /// UI never tells the user to install something Kemori would do itself.
    pub fn setup_reason(self) -> String {
        if can_install(self) {
            let mut reason = self.install_hint().to_string();
            if let Some(bytes) = self.estimated_download_bytes() {
                reason.push_str(&format!(" (about {})", human_bytes(bytes)));
            }
            return reason;
        }
        match self {
            Tool::SwiftLs if swift_toolchain().is_none() => "no Swift toolchain is published for \
                 this platform (musl or a non-Linux host); install the Swift toolchain manually \
                 and Kemori will use it"
                .to_string(),
            Tool::SwiftLs if locate("gpg").is_none() => {
                "install `gnupg` so Kemori can verify the Swift toolchain signature".to_string()
            }
            Tool::SwiftLs => {
                // `can_install` was false for another reason: a library the
                // toolchain needs is missing.
                let missing = swift_missing_libs();
                let hint = if distro_is_arch() {
                    " (on Arch: `libxml2-legacy`)"
                } else {
                    ""
                };
                format!(
                    "the Swift toolchain needs {}{hint} from your distribution; install it and \
                     Kemori can install Swift",
                    missing.join(", ")
                )
            }
            Tool::ElixirLs => "no Erlang/Elixir build is published for this platform; install \
                 Erlang and Elixir and Kemori will use ElixirLS"
                .to_string(),
            Tool::PerlLs | Tool::Pls if locate("perl").is_none() => {
                "install Perl and Kemori can set up its language server automatically".to_string()
            }
            _ => {
                let missing = self.missing_prerequisites();
                if missing.is_empty() {
                    self.install_hint().to_string()
                } else {
                    format!("needs {} — {}", missing.join(" or "), self.install_hint())
                }
            }
        }
    }

    /// The tool for a language and purpose, if Kemori knows one.
    pub fn for_language(language: LanguageId, purpose: ToolPurpose) -> Option<Tool> {
        Tool::ALL
            .iter()
            .copied()
            .find(|tool| tool.serves(language) && tool.purpose() == purpose)
    }

    /// The first language server for `language` that is installed and usable.
    ///
    /// A language may have more than one candidate server — Perl has PLS and
    /// `Perl::LanguageServer` — so Kemori picks the first *available* one in
    /// preference order rather than committing to a single tool.
    pub fn available_server(language: LanguageId, tools: &ToolRegistry) -> Option<Tool> {
        Tool::ALL.iter().copied().find(|tool| {
            tool.serves(language)
                && tool.purpose() == ToolPurpose::LanguageServer
                && tools.available(*tool)
        })
    }

    /// The first installed debug adapter for `language`.
    pub fn available_debug_adapter(language: LanguageId, tools: &ToolRegistry) -> Option<Tool> {
        Tool::ALL.iter().copied().find(|tool| {
            tool.serves(language)
                && tool.purpose() == ToolPurpose::DebugAdapter
                && tools.available(*tool)
        })
    }

    /// The first language server for `language` that Kemori could install.
    pub fn installable_server(language: LanguageId, tools: &ToolRegistry) -> Option<Tool> {
        Tool::ALL.iter().copied().find(|tool| {
            tool.serves(language)
                && tool.purpose() == ToolPurpose::LanguageServer
                && !tools.available(*tool)
                && can_install(*tool)
        })
    }

    /// The preferred install command, when one exists.
    ///
    /// This is the human-facing headline command, used to describe the tool;
    /// the actual install uses [`Tool::install_attempts`], which picks a
    /// self-contained, user-local strategy.
    pub fn install_command(self) -> Option<(&'static str, &'static [&'static str])> {
        match self {
            Tool::RustAnalyzer => Some(("rustup", &["component", "add", "rust-analyzer"])),
            Tool::Rustfmt => Some(("rustup", &["component", "add", "rustfmt"])),
            Tool::Pylsp => Some(("pipx", &["install", PYTHON_LSP_SPEC])),
            Tool::Debugpy => Some(("pipx", &["install", DEBUGPY_SPEC])),
            Tool::BashLs => Some(("npm", &["install", "-g", BASH_LS_SPEC])),
            Tool::TypeScriptLs => Some((
                "npm",
                &["install", "-g", TYPESCRIPT_LS_SPEC, TYPESCRIPT_SPEC],
            )),
            // `clangd`, `phpactor`, `lua-language-server`, Kotlin, Dart, Elixir,
            // Swift and Go tooling are installed by Kemori's own managed plans.
            Tool::Clangd => None,
            Tool::Phpactor => None,
            Tool::LuaLs => None,
            Tool::KotlinLs => None,
            // Go tooling is installed by Kemori's managed plan, which provisions
            // Go itself when it is missing.
            Tool::Gopls | Tool::Sqls => None,
            Tool::Dlv => None,
            Tool::JsDebug => None,
            Tool::Netcoredbg => None,
            Tool::JavaDebug => None,
            Tool::Rdbg => None,
            Tool::PhpDebug => None,
            // `solargraph` installs into a Kemori-private gem home.
            Tool::RubyLs => None,
            // `asm-lsp` uses a prebuilt release, falling back to `cargo`.
            Tool::AsmLsp => None,
            // Perl, Dart, Elixir and Swift servers are installed by Kemori's own
            // plans rather than a single package-manager command.
            Tool::PerlLs
            | Tool::Pls
            | Tool::DartAnalyzer
            | Tool::DartDebug
            | Tool::ElixirLs
            | Tool::SwiftLs => None,
            // `jdtls` and `OmniSharp` are installed by Kemori's own managed
            // download plan rather than a single package-manager command.
            Tool::Jdtls | Tool::OmniSharp => None,
            Tool::HtmlLs | Tool::CssLs => {
                Some(("npm", &["install", "-g", VSCODE_LANGSERVERS_SPEC]))
            }
            Tool::Gofmt => None,
            Tool::Prettier => Some(("npm", &["install", "-g", PRETTIER_SPEC])),
            Tool::Shfmt => None,
            // `clang-format`, `perltidy` and `nixfmt` ship with their language
            // toolchains or distributions.
            Tool::ClangFormat | Tool::PerlTidy | Tool::Nixfmt => None,
            // `nil` is installed by Kemori's managed Nix profile plan.
            Tool::Nil => None,
            Tool::LldbDap => None,
        }
    }

    /// Programs this tool needs before it can be installed, so Kemori can explain
    /// when a whole toolchain is missing.
    pub fn prerequisites(self) -> &'static [&'static str] {
        match self {
            Tool::RustAnalyzer | Tool::Rustfmt => &["rustup"],
            // `gopls`/`sqls`/`shfmt` are installed by Kemori's own plan, which
            // provisions the official Go toolchain when none is present.
            Tool::Gopls | Tool::Gofmt | Tool::Sqls | Tool::Shfmt => &[],
            Tool::Dlv => &[],
            Tool::JsDebug => &[],
            Tool::Netcoredbg => &[],
            Tool::JavaDebug => &["jdtls"],
            Tool::Rdbg => &["gem"],
            Tool::PhpDebug => &[],
            Tool::Pylsp => &["python3"],
            Tool::Debugpy => &["python3"],
            Tool::LldbDap => &[],
            // Kemori can provision Node.js itself, so npm is not a hard
            // prerequisite. The install plan checks for `curl` and an archive
            // tool before offering the managed runtime.
            Tool::BashLs | Tool::TypeScriptLs => &[],
            Tool::HtmlLs | Tool::CssLs => &[],
            // `jdtls` is a Python launcher script.
            Tool::Jdtls => &["python3"],
            // `clangd` uses a self-contained bundle; `phpactor` needs only a PHP
            // runtime; `asm-lsp` uses a prebuilt release.
            Tool::Clangd | Tool::OmniSharp => &[],
            Tool::Phpactor => &["php"],
            Tool::LuaLs => &[],
            // The dedicated JDK 21 is provided by Kemori's managed download plan,
            // so no system Java is required.
            Tool::KotlinLs => &[],
            // `solargraph` installs into a Kemori-private gem home but still needs
            // a system Ruby.
            Tool::RubyLs => &["gem"],
            Tool::AsmLsp => &[],
            Tool::PerlLs | Tool::Pls => &["perl"],
            // Dart, Erlang/Elixir and the Swift toolchain are provided by
            // Kemori's managed download plans.
            Tool::DartAnalyzer | Tool::DartDebug | Tool::ElixirLs | Tool::SwiftLs => &[],
            // Nix tooling comes from Nix itself; Kemori never installs a package
            // manager for it.
            Tool::Nil | Tool::Nixfmt => &["nix"],
            // Formatters.
            Tool::Prettier => &[],
            Tool::ClangFormat | Tool::PerlTidy => &[],
        }
    }

    /// The prerequisites that are not present.
    pub fn missing_prerequisites(self) -> Vec<&'static str> {
        self.prerequisites()
            .iter()
            .copied()
            .filter(|program| locate(program).is_none())
            .collect()
    }

    /// The ordered strategies Kemori will try to install this tool.
    ///
    /// Each attempt is a short command sequence; Kemori re-probes the tool after
    /// every attempt and stops at the first that yields a working binary. Every
    /// strategy is self-contained and targets a location the user can write to
    /// — a Kemori-managed Python virtualenv or npm prefix, `--user` installs, or
    /// the official `rustup` installer — so a missing permission or a Python
    /// without `pip` never blocks provisioning.
    pub fn install_attempts(self) -> Vec<InstallAttempt> {
        match self {
            Tool::RustAnalyzer => component_attempts("rust-analyzer"),
            Tool::Rustfmt => component_attempts("rustfmt"),
            // Go tooling installs with `go install`, provisioning the official
            // Go toolchain when none is present.
            Tool::Gopls => go_attempts(&[GOPLS_SPEC]),
            Tool::Dlv => go_attempts(&[DLV_SPEC]),
            Tool::JsDebug => js_debug_attempts(),
            Tool::Netcoredbg => netcoredbg_attempts(),
            Tool::JavaDebug => java_debug_attempts(),
            Tool::Rdbg => rdbg_attempts(),
            Tool::PhpDebug => php_debug_attempts(),
            Tool::Pylsp => python_attempts(),
            Tool::Debugpy => debugpy_attempts(),
            Tool::BashLs => npm_attempts(&[BASH_LS_SPEC]),
            Tool::TypeScriptLs => npm_attempts(&[TYPESCRIPT_LS_SPEC, TYPESCRIPT_SPEC]),
            // The clangd project publishes a small, self-contained bundle, so
            // Kemori does not need a full LLVM toolchain.
            Tool::Clangd => clangd_attempts(),
            // `phpactor` is a single phar; Kemori downloads it directly and only
            // needs a PHP runtime, not Composer.
            Tool::Phpactor => phpactor_attempts(),
            // `lua-language-server` ships a self-contained, runtime-free archive
            // per platform, so Kemori manages it like the JDK and OmniSharp.
            Tool::LuaLs => lua_ls_attempts(),
            // `kotlin-language-server` needs a JDK whose version its bundled
            // compiler understands, so Kemori installs a dedicated JDK 21.
            Tool::KotlinLs => kotlin_ls_attempts(),
            Tool::Sqls => go_attempts(&[SQLS_SPEC]),
            // `solargraph` installs into a Kemori-private gem home when a system
            // Ruby is available.
            Tool::RubyLs => ruby_attempts(),
            // A prebuilt `asm-lsp` release where one exists, otherwise `cargo`
            // (bootstrapping the Rust toolchain when needed).
            Tool::AsmLsp => asm_lsp_attempts(),
            // The Dart SDK is self-contained and managed by Kemori.
            Tool::DartAnalyzer => dart_sdk_attempts(),
            Tool::DartDebug => dart_sdk_attempts(),
            // `Perl::LanguageServer` bootstraps cpanm and installs into an
            // isolated local::lib.
            Tool::PerlLs => perl_attempts(),
            // PLS has no `Coro` dependency, so it is the preferred server.
            Tool::Pls => pls_attempts(),
            // Erlang/OTP, Elixir and ElixirLS are installed as one toolchain.
            Tool::ElixirLs => elixir_ls_attempts(),
            // The Swift toolchain is GPG-verified and only offered where
            // swift.org publishes a build for the running distribution.
            Tool::SwiftLs => swift_attempts(),
            Tool::Jdtls => jdtls_attempts(),
            Tool::OmniSharp => omnisharp_attempts(),
            Tool::HtmlLs | Tool::CssLs => npm_attempts(&[VSCODE_LANGSERVERS_SPEC]),
            // `gofmt` ships with Go, so Kemori provisions the toolchain.
            Tool::Gofmt => go_toolchain_attempts(),
            Tool::Prettier => npm_attempts(&[PRETTIER_SPEC]),
            Tool::Shfmt => go_attempts(&[SHFMT_SPEC]),
            // `nil` has no prebuilt assets; a pinned nixpkgs revision is the
            // only exact, integrity-checked source.
            Tool::Nil => nix_attempts("nil"),
            // `clang-format` and `perltidy` ship with their toolchains;
            // `nixfmt` comes from the same pinned nixpkgs revision as `nil`.
            Tool::Nixfmt => nix_attempts("nixfmt"),
            Tool::ClangFormat | Tool::PerlTidy => Vec::new(),
            Tool::LldbDap => Vec::new(),
        }
    }
}

// Provisioning plans and the install engine are split across included files so
// each source file stays within the project limit without changing behaviour.
include!("plan.rs");
include!("install.rs");
include!("download.rs");
include!("runtimes.rs");
include!("remote.rs");
include!("toolchains.rs");
include!("archive.rs");
include!("hash.rs");
include!("status.rs");
include!("status_types.rs");
include!("registry.rs");
include!("discovery.rs");
include!("paths.rs");
include!("env.rs");
include!("attempts_more.rs");

#[cfg(test)]
mod tests {
    include!("tests_1.rs");
    include!("tests_2.rs");
    include!("tests_3.rs");
    include!("tests_4.rs");
}
