/// A lock older than this is assumed to be left by a crashed instance.
const STALE_INSTALL_LOCK: Duration = Duration::from_secs(15 * 60);

/// The longest a `--version` probe may run. A first-run shim can be slow, but a
/// probe must never hang the background worker.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a stdio language server is given to prove it starts. A server that
/// is still running when the window closes is treated as usable.
const PROBE_ALIVE_WINDOW: Duration = Duration::from_millis(600);

/// The Node.js LTS release Kemori provisions when the user has no `node`/`npm`.
///
/// Pinned so the download and the SHA-256 Node.js publishes for it are
/// reproducible; the npm bundled with this release installs the npm-based
/// language servers into Kemori's managed prefix.
const NODE_VERSION: &str = "24.21.0";

/// The `rustup-init` release Kemori downloads when the user has no `rustup`.
///
/// rustup publishes a per-target binary and its SHA-256 under a versioned
/// archive URL, so the bootstrap is a verified download rather than the moving
/// `sh.rustup.rs` script.
const RUSTUP_VERSION: &str = "1.29.1";

/// The exact official Go toolchain release Kemori provisions. `go.dev/dl` publishes
/// the SHA-256 of every release's archives, so the version is pinned and the
/// download is still verified against upstream metadata.
const GO_VERSION: &str = "go1.27.1";

/// The exact Eclipse Adoptium JDK releases for jdtls (25) and Kotlin (21).
///
/// The Adoptium API publishes the checksum in the same response as the download
/// link; pinning `release_name` keeps the version from drifting.
const ADOPTIUM_JDK25: &str = "jdk-25.0.4.1+1";
const ADOPTIUM_JDK21: &str = "jdk-21.0.12.1+1";

/// The `lua-language-server` release Kemori provisions. It ships a self-contained
/// archive per platform (no runtime required), so the version is pinned for a
/// stable download URL.
const LUA_LS_VERSION: &str = "3.19.1";

/// The `kotlin-language-server` release Kemori provisions. Pinned because its
/// bundled Kotlin compiler must run on the dedicated JDK 21 Kemori installs.
const KOTLIN_LS_VERSION: &str = "1.3.13";

/// The Dart SDK release Kemori provisions. Pinned so the archive and Google's
/// published `.sha256sum` stay in step.
const DART_SDK_VERSION: &str = "3.13.5";

/// The Swift toolchain release Kemori provisions on supported Linux systems.
///
/// Swift ships one signed toolchain tarball per distribution release; the exact
/// asset is resolved from `/etc/os-release` at install time. The version is
/// pinned so the download URL, the signature URL and the verified key stay in
/// step.
const SWIFT_VERSION: &str = "6.4.0";

/// The ElixirLS release Kemori provisions. Its launcher needs a matching
/// Erlang/OTP and Elixir runtime, which Kemori installs alongside it.
const ELIXIR_LS_VERSION: &str = "0.31.1";

/// The Erlang/OTP and Elixir releases Kemori provisions. They are pinned as a
/// compatible pair: the Elixir build is compiled for this OTP major.
const OTP_VERSION: &str = "27.3.4";
const ELIXIR_VERSION: &str = "1.18.4";

/// The Hex release Kemori installs into the private Mix home. `mix local.hex`
/// accepts an exact version and Hex verifies the archive against the checksum
/// in `builds.hex.pm/installs/hex.csv`, so this is pinned rather than "latest".
const HEX_VERSION: &str = "2.5.1";

/// The rebar3 release Kemori installs into the private Mix home.
///
/// The GitHub release publishes a single escript and Kemori runs it through
/// `mix local.rebar rebar3 <path>`. GitHub reports no asset digest for this
/// release, so the SHA-256 is a Kemori-computed pin of the immutable tagged asset
/// (verified to run on the pinned Erlang/OTP). It is the newest rebar3
/// compatible with OTP 27.
const REBAR3_VERSION: &str = "3.24.0";
const REBAR3_SHA256: &str = "d2d31cfb98904b8e4917300a75f870de12cb5167cd6214d1043e973a56668a54";

/// `App::cpanminus`, the non-interactive CPAN client Kemori bootstraps so Perl
/// modules can be installed without an interactive `cpan` first run. The
/// archive and its SHA-256 (from MetaCPAN) are pinned together.
const CPANM_VERSION: &str = "1.7049";
const CPANM_SHA256: &str = "b9ffb88e62a06aa91bd7d5a28ef6bdbb942608aea90e3969aa29b33640035214";

/// The standalone `clangd` release Kemori provisions. The clangd project publishes
/// a small, self-contained bundle per platform (much smaller than a full LLVM
/// toolchain), with a SHA-256 digest on the GitHub asset.
const CLANGD_VERSION: &str = "23.1.0";

/// The prebuilt `asm-lsp` release Kemori provisions where one exists, with
/// `cargo install` as the fallback.
const ASM_LSP_VERSION: &str = "0.10.1";

/// The `phpactor` release whose `phpactor.phar` Kemori provisions. phpactor is a
/// single executable phar, so Composer is not required.
const PHPACTOR_VERSION: &str = "2026.06.23.0";

/// The nixpkgs revision Kemori pins for Nix tooling (`nil`, `nixfmt`).
///
/// `nil` publishes no release assets and is not on crates.io, so Nix itself is
/// the only trustworthy source. A commit is immutable and Nix verifies every
/// store path it builds, so the install is reproducible without a Kemori-side
/// checksum.
const NIXPKGS_REV: &str = "55bfb4d7437bd4e09ab7cba58a43bd68eb310044";

/// The SHA-256 of the pinned `kotlin-language-server` `server.zip`.
///
/// GitHub publishes no asset digest for this release, so the value is a
/// Kemori-computed pin of the immutable release asset (recorded from a fresh
/// download and reviewed). A replaced or corrupted asset fails closed.
const KOTLIN_LS_SHA256: &str = "4fe7d71d087b307c7869036171bd9d8c6a4284cd7c25b89098b0a24eb2d9b6d2";

/// The Eclipse JDT Language Server milestone Kemori provisions.
///
/// Eclipse publishes a `.sha256` beside each milestone archive, and the
/// snapshot timestamp is part of the immutable filename, so the pinned version
/// and digest cannot drift.
const JDTLS_VERSION: &str = "1.61.0";
const JDTLS_SNAPSHOT: &str = "1.61.0-202609031315";
const JDTLS_SHA256: &str = "338e7e73d61836651ba2453919a0d34fa763eb4e7c03342092309bffb8934c64";

/// The OmniSharp release Kemori provisions, pinned to an exact tag.
const OMNISHARP_VERSION: &str = "v2.0.0";

/// The exact .NET SDK version Kemori provisions for OmniSharp.
///
/// Microsoft publishes a SHA-512 for every SDK archive in the channel's
/// `releases.json`, so a pinned version can be verified without the moving
/// `dotnet-install.sh` script.
const DOTNET_SDK_VERSION: &str = "10.0.401";
const DOTNET_CHANNEL: &str = "10.0";

// Pinned package-manager inputs. A package manager still resolves its own
// dependencies, but Kemori always names an exact version, so an install is
// reproducible and a new upstream release cannot arrive silently.
const GOPLS_SPEC: &str = "golang.org/x/tools/gopls@v0.23.0";
const SQLS_SPEC: &str = "github.com/sqls-server/sqls@v0.2.48";
const SHFMT_SPEC: &str = "mvdan.cc/sh/v3/cmd/shfmt@v3.14.1";
const BASH_LS_SPEC: &str = "bash-language-server@5.8.1";
const TYPESCRIPT_LS_SPEC: &str = "typescript-language-server@6.0.1";
const TYPESCRIPT_SPEC: &str = "typescript@7.0.2";
const VSCODE_LANGSERVERS_SPEC: &str = "vscode-langservers-extracted@4.10.0";
const PRETTIER_SPEC: &str = "prettier@3.9.9";
const PYTHON_LSP_SPEC: &str = "python-lsp-server==1.15.0";

/// The `debugpy` release Kemori provisions as Python's debug adapter.
///
/// Pinned exactly like the language server; `pipx`/`uv`/pip install it into an
/// isolated environment and the `debugpy-adapter` console script speaks DAP
/// over stdio.
const DEBUGPY_SPEC: &str = "debugpy==1.8.22";

/// The `vscode-php-debug` DAP adapter release. The VSIX is the official
/// distributable; its GitHub asset digest is verified before extraction.
const PHP_DEBUG_VERSION: &str = "1.40.2";

/// The `debug` gem Kemori provisions for Ruby's `rdbg` DAP server.
const RUBY_DEBUG_VERSION: &str = "1.11.1";

/// The pinned `java-debug` JDT LS extension Kemori loads into jdtls for Java
/// debugging. Maven Central has no .sha256 file, so the SHA-256 is a Kemori pin
/// of the immutable release artifact.
const JAVA_DEBUG_VERSION: &str = "0.53.1";
const JAVA_DEBUG_SHA256: &str = "4f4778d452a6a0665536f43ce4e32403a24be6593336b80dc85a322912859e24";

/// The `netcoredbg` release Kemori provisions for C# debugging. It ships a
/// self-contained archive per platform with a published SHA-256 digest.
const NETCOREDBG_VERSION: &str = "3.2.0-1092";

/// The `js-debug` DAP-only release Kemori provisions for JS/TS debugging.
/// The GitHub asset carries a published SHA-256 digest and is small (~1.2 MB).
const JS_DEBUG_VERSION: &str = "1.140.0";

/// The `dlv` (Delve) release Kemori provisions for Go debugging through the
/// existing managed Go toolchain.
const DLV_SPEC: &str = "github.com/go-delve/delve/cmd/dlv@v1.27.2";

const SOLARGRAPH_VERSION: &str = "0.60.4";
const PLS_SPEC: &str = "PLS@0.906";
const PERL_LS_SPEC: &str = "Perl::LanguageServer@2.6.2";

/// The longest a single install command may run before it is killed. Package
/// managers can legitimately take a while on a slow link, but a hung process
/// must never wedge the background worker forever.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// The most output Kemori keeps from an install command. A verbose or hostile
/// tool cannot exhaust memory; the excess is drained and discarded.
const MAX_TOOL_OUTPUT: usize = 256 * 1024;

/// The largest archive Kemori will download. The managed JDK and .NET SDK are
/// the biggest, and both are well under this.
const MAX_DOWNLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;
