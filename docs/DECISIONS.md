# Design decisions

The *why* behind Kemori's implementation. `AGENTS.md` states the rules; this file
keeps the rationale that would otherwise bloat it. Read the section for the
subsystem you are about to touch — for example, read
[Provisioning](#provisioning) before changing `language/tools.rs`, and
[LSP](#lsp) before changing `language/lsp/`.

If a decision here is reversed, say so in [CHANGELOG.md](../CHANGELOG.md), update
this file, and add or change a test.

---

## Editor

- **Rope-backed buffers** (`ropey`). Efficient edits and slicing on large files.
- **Operation-based undo.** Each edit records `(start, removed, inserted,
  cursor_before, cursor_after)`. Cheap and exact.
- **Read-only mode is a separate user flag.** `Buffer` keeps the existing
  `read_only` flag for lossily opened non-UTF-8 files and adds
  `user_read_only` toggled by the user. Saves check either flag, but
  `Convert to UTF-8` only clears the lossy one, so conversion cannot silently
  re-enable writing. Mutations are refused in the document edit entry points,
  while the app guards command paths that would otherwise report false success.
- **Large-file mode is a threshold, not a file type.** A document reports
  `is_large_file()` when the rope reaches one million characters. The reduced
  feature set lives at the call sites (highlight spans, diagnostics scheduling,
  completion, LSP sync and git markers), so normal editing, undo and search keep
  the same code paths and behaviour. This avoids a second editor mode and keeps
  the switch automatic; there is no per-file setting.
- **Replace preview is computed, not applied.** The preview state is a list of
  affected rows plus the match count; it is built from the same match set the
  replace-all path will use. Confirming calls replace-all, so there is no second
  replacement implementation and the edit remains one undo step. Multi-line
  regex matches are flattened onto their starting line for display only.
- **Find/replace scoping reuses match filtering.** A scoped search stores the
  original contiguous selection as a `(start, end)` pair in `Search`; the normal
  matcher still scans the document, then non-overlapping matches outside the
  pair are discarded. This keeps regex, case and whole-word behaviour identical
  to whole-file search. Replace updates the stored end by the total length delta
  of the replacements it made, so subsequent passes stay inside the region
  instead of drifting past it.
- **Rectangular selection is part of the ordinary selection model.** A
  `Selection` carries a `column` flag with the same anchor as a linear
  selection. Bounds are derived from the anchor and cursor columns, edits across
  rows become one multi-cursor edit, and short rows clamp to their length.
  Starting requires the explicit column chord; once active, plain Shift arrows
  keep the rectangle, so there is no hidden mode to turn off.
- **Snippets are data with lexical placeholders.** Each language owns a small
  static snippet table; the editor expands `$1`/`${1:default}`/`$0` into text
  plus character offsets. Insertion is one normal edit (so one undo step), Tab
  and Shift+Tab walk the recorded offsets, and Escape or cursor navigation ends
  snippet mode. No extension language, mirror variables or user snippet files
  are introduced, which keeps snippets aligned with the zero-configuration rule.
- **Undo history can outlive the process for clean files.** The operation-based
  history is serialized into the existing per-project session (at most the last
  100 steps), together with the save-point id and an FNV-1a fingerprint of the
  buffer text. Restoring is refused when the file's bytes changed or the buffer
  was dirty at capture time, because edit offsets would otherwise point into
  different text. This keeps normal undo undoable without turning a session file
  into an unbounded log.
- **Editor history tracks a save point.** Each edit has a unique id and the
  document remembers the id at its save point, so undo/redo restore the clean
  state exactly; coalescing is broken when a document is saved.
- **Inferred indentation.** Each document detects its indentation unit from the
  file's leading whitespace (falling back to four), and uses it for
  auto-indent, `Tab` and indent/outdent. This is zero-configuration and keeps
  the editor core language-agnostic: the width is a property of the file, not a
  per-language branch.
- **Multiple cursors are a small, explicit model.** The primary stays on
  `Document` as `cursor`/`selection`; `Document::cursors` holds the secondary
  `Cursor`s (anchor + cursor), kept sorted, unique and distinct from the
  primary. Edits are planned per cursor against the pre-edit text and applied
  bottom-up, then recorded as one `Edit` whose `ops` list makes the whole
  keystroke a single undo step. `apply_multi_edit` is the only path that mutates
  several ranges, and it moves cursors even for a zero-op keystroke so bracket
  skip-over works everywhere. Navigation deliberately collapses to the primary;
  every other single-point edit clears the extras so a stale cursor can never
  survive an unrelated operation.
- **Soft wrap is a display-only transform with one source of truth.** A position
  is always a character position; wrapping only adds a derived *visual row*.
  `editor/layout.rs` owns both the character ↔ display-column mapping (tabs
  expand there) and the wrap boundaries, and the renderer and the editor's
  Up/Down movement both call it, so they cannot disagree. The viewport is
  `scroll_top` (a logical row) plus `scroll_subline` (which visual row of it is
  on screen); the cursor's logical line is always kept at or above the top, which
  bounds scrolling work to the visible rows instead of scanning the document.
  Wrapping never edits the buffer, so undo/redo, diagnostics, LSP positions and
  multiple cursors are unaffected; turning it off restores horizontal scrolling.
- **Saves are atomic and lossless.** Writes go to a sibling temporary file that
  is flushed and renamed over the original, preserving permissions. Binary and
  oversized files are refused; a non-UTF-8 text file opens read-only as lossy
  text and cannot be written back until the user converts it to UTF-8, so the
  original bytes are never silently replaced. Filesystem rename/copy never
  overwrite an existing destination.
- **Split editor.** The editor keeps a single active document; a split stores
  one document index per pane and `editor.active` follows the focused pane, so
  every existing editing path keeps working unchanged. Panes share the tab
  strip, and closing a pane's file collapses the split.
- **Folding is a view over visible rows.** A fold is a header row plus the
  hidden range below it; the text is never removed. Cursor movement, selections
  and jumps only land on visible rows, and every edit remaps or drops fold
  headers so a header cannot slide onto unrelated text. Folding starts from the
  file's indentation (zero-config) and prefers language-server folding ranges
  when a server publishes them.

---

## Language and detection

- **Line-based highlight cache.** Providers return `(spans, next_state)` so
  multi-line constructs work. `HighlightState` carries a block-comment flag and
  depth (Rust's comments nest, so the open level survives a line break) plus a
  small shared vocabulary — `LexMode` (template literals and their `${}`
  interpolation) and `Embed` (an open HTML `<script>`/`<style>` region) — rather
  than language-specific state on the editor. The HTML tokenizer delegates an
  embedded body to the JavaScript or CSS tokenizer, which carries its own state,
  so one line can be split between two languages. The cache is invalidated from
  the edited line down.
- **Detection is deterministic and file-first.** Provider descriptors are sorted
  by language id before the engine is built, the content-hint weight is capped
  at exactly three, and the project-context bonus applies only to a language
  with a file-level signal (name, shebang or extension) — a project marker never
  overrides a file's own nature.
- **Highlighting engine.** Kemori uses hand-written, line-based tokenizers that
  carry a small state. They have not been benchmarked against tree-sitter in this
  repository; a migration would require an explicit OK and is not planned. This
  stays a documented non-decision until measurement exists.
- **The web tokenizer uses lookback heuristics, not a parser.** A `/` in
  expression position starts a regex literal (division after a value does not),
  and `<` opens a JSX tag only after an expression start, so `Array<Foo>` and
  `a < b` stay operators. This keeps the built-in highlighter predictable on
  incomplete and ambiguous code; it is a lexical aid, not a semantic parser.
- **Formatting reuses trusted tools through one pipeline.** A provider declares
  the `Formatting` capability and implements `format()` by calling a shared
  helper in `language/format.rs` (`rustfmt`, `gofmt`, `prettier`,
  `clang-format`, `shfmt`, `perltidy`, `rubocop`, `ktlint`, `sqlfluff`,
  `php-cs-fixer`, `csharpier`, plus the managed `dart format` and
  `mix format`). Every helper feeds a buffer snapshot on stdin and takes stdout,
  so unsaved edits are formatted in place and the result applies as one
  undoable edit. The provider's `formatter()` names the executable, and
  `Tool::for_language(language, Formatter)` lets the palette explain a missing
  formatter before the user invokes it. Language-server formatting is preferred
  when the server advertises it.
- **Format on save is save → format → save.** Enabling the preference in
  Settings saves the active file immediately, starts the same formatter path as
  `Format Document` (server first, built-in tool second), and writes the buffer
  again when the result applies. The first write is deliberately not rolled
  back: a missing or failed formatter leaves a saved file and a clear error,
  never unsaved work. The second write is suppressed if the user closes the
  document while formatting is in flight.
- **A language may have more than one candidate server.** `Tool::ALL` order is
  the preference order and `Tool::available_server` / `Tool::installable_server`
  choose the first server that is actually available or installable. Perl is the
  first user: **PLS** (no `Coro` dependency, so it builds on current Perls) is
  preferred, with `Perl::LanguageServer` discovered as a fallback. The editor
  still tracks one running server per language.
- **The support contract is explicit and tiered.** A language is Complete,
  Complete-with-a-prerequisite, Offline only or Platform-limited, and the claim
  must match the code; "an LSP starts" is not enough. The contract, the
  per-language matrix and the verification status live in
  [LANGUAGE-SUPPORT.md](LANGUAGE-SUPPORT.md), and a test
  (`tests/language_support.rs`) fails when a registered language has no row, so
  adding a language cannot silently skip its documentation.
- **`Capability` is provider-scoped today, which under-reports LSP features.**
  Providers declare built-in capabilities, but the palette gates the LSP-backed
  `Format Document`, `Rename Symbol` and `Code Actions` commands on them as
  well, so the palette advertises those only where a provider declares them
  (Rust/Go for rename and code actions). The keybindings do not consult the
  list, so the features work for any attached server that supports them. The
  direction is to keep provider capabilities for built-in features and consult
  the attached server's advertised capabilities for server-backed commands; do
  not add a second capability system before the existing one is fixed.

---

## LSP

- **One language server per language.** The app tracks servers in a map keyed by
  `LanguageId`; each has its own handshake, restart budget and failure state.
  Feature requests resolve against the active document's language, and a
  language's server failing never disturbs another. Workspace-wide requests
  prefer the active language's server, then any ready one.
- **Language servers are attached for the whole session.** A document detected
  after the handshake is sent `didOpen`, closing sends `didClose`, the restart
  budget is only forgiven after a stable uptime (so a crash-after-initialize
  loop is bounded), and one malformed message does not tear down a live
  connection. Results are still matched to their request id and document.
- **The LSP encoding boundary is centralized.** Kemori's internal positions count
  Unicode scalar values. Everything crossing the LSP boundary is converted in
  `language/lsp/convert.rs` (`char_to_lsp`/`lsp_to_char`, and `edits_to_chars`)
  using the actual line text and the negotiated `PositionEncoding` (UTF-8,
  UTF-16 or UTF-32). Outbound requests convert the cursor; inbound diagnostics,
  edits, formatting and locations convert back before use. Conversion always
  clamps to a character boundary and never splits a code point. Do not add
  ad-hoc character/byte arithmetic in the UI.
- **Pull diagnostics are used when the server offers them.** A server that
  advertises `diagnosticProvider` is asked `textDocument/diagnostic` when it
  connects and after a document changes; a server that also advertises
  `workspaceDiagnostics` answers `workspace/diagnostic` when the Problems panel
  refreshes. Unopened files are read only to convert the negotiated position
  encoding into character columns, and their diagnostics join the built-in scan
  in the same panel with duplicate rows removed. Push diagnostics remain fully
  supported for servers that do not implement pull.
- **Document highlights are cursor-driven and superseding.** Cursor movement and
  edits request `textDocument/documentHighlight` at the primary cursor. The
  response is converted through the same encoding boundary as diagnostics and
  painted as a background under token styles; edits clear the old ranges
  immediately. Because requests can overlap, the handler checks the pending id
  before taking it, so a stale response cannot discard the newer request.
- **Inlay hints are decorations with a text version.** `InlayHint` is an editor
  type independent of LSP; the app converts server positions and stores the
  resulting hints with the buffer version that produced them. The renderer
  inserts them before the character at their position as dim virtual text, so
  they never affect widths, selection, undo or saving. Re-requesting is keyed to
  the version, not cursor movement, and large files opt out.
- **Document links are resolved into a picker, not rendered as underlines.**
  The terminal has no reliable modifier for clickable spans, so the command
  presents every target with its tooltip and line. `file:` targets use the
  normal reveal path; other schemes are copied to the clipboard, which avoids
  launching a browser from the editor. Data-only links are skipped until a
  resolve step exists.
- **Implementation and type definition share the definition request slot.**
  All three requests resolve to locations, so they use the same pending
  `PendingDocRequest` and reveal path; the response kind only changes the empty
  and failure wording. This keeps one validated jump pipeline rather than three
  nearly identical ones.
- **Hierarchies are two-phase pickers.** Preparing a symbol and expanding one
  direction are separate requests, matching the LSP protocol. The first response
  either continues immediately for a single node or asks the user which node to
  expand; the follow-up response is shown as a jumpable list rather than a
  persistent tree, which fits the palette UI and avoids another navigation mode.
  Call and type hierarchies share the same pending-request validation.
- **Selection ranges are cached for the caret, not the selection.** The server
  returns an innermost-to-outermost chain; the app stores it with the buffer
  version and caret position and only replaces it while the document has no
  selection. Expand picks the smallest cached range that strictly contains the
  current selection, and shrink walks the chain backwards, so repeated key
  presses behave predictably even after the caret moves to a range boundary.
- **Code lenses are decorations with an opaque command.** The editor stores a
  title plus serialized command/arguments strings; only the app parses the
  payload and sends `workspace/executeCommand` to the active server. This keeps
  JSON and LSP out of the editor while still making lenses runnable. Lens
  responses are versioned like inlay hints, and a data-only lens is reported as
  unresolved instead of silently doing nothing.
- **Server responses are validated against the document they describe.** A
  document-sensitive request records `(language, id, path, buffer version)` at
  request time; a response is applied only if it is still the newest request and
  the document is open at the same version (navigation also requires it to be
  active). Code actions are re-validated when applied. Never apply an edit or
  move the cursor from a response that has been superseded.
- **Requests and subprocesses are bounded.** Every LSP request has a deadline
  (formatting a longer one); expired requests are removed and their UI state
  cleared. External processes run through `process::wait_captured` (or
  `process::wait_streaming` when live output is required), which drains both
  pipes, caps output and kills the child at a deadline. New subprocess call
  sites must use one of them rather than `Command::output` or
  `wait_with_output`.
- **Background work is isolated.** A small worker pool runs background requests,
  so one slow operation cannot stall unrelated services. Automatic,
  superseding snapshots (diagnostics) may be dropped when the queue is
  saturated; user-initiated work must not be dropped.
- **Adversarial input is bounded.** Search highlighting runs on the UI thread,
  so the regex engine carries a per-line step budget and the `.gitignore` glob
  matcher is polynomial; a crafted pattern degrades to "no match" instead of
  freezing the editor.

---

## Debugging

- **A real DAP client, not a handshake.** `debug/session.rs` spawns an adapter
  over stdio, frames JSON with `Content-Length`, tracks request ids and maps
  responses back to their command. It sends `initialize`, waits for the
  `initialized` event, sets breakpoints, launches and sends
  `configurationDone`; `stopped`, `continued`, `exited` and `terminated` become
  typed events. The client is adapter-agnostic and the same code serves every
  language. Adapters that only offer TCP (`dlv dap`) announce a listen address
  on stdout; Kemori waits for it, connects, and then uses the socket with the
  same framing and request loop. Adapters that need a caller-supplied port
  (js-debug's `dapDebugServer.js`) get a reserved local port in the launch plan.
- **Stack, scopes, variables and watches are pulled on demand.** A stop requests
  `stackTrace`; the top frame requests `scopes`; each scope requests
  `variables`; every watch expression is evaluated in the top frame. Results are
  appended to the Debug Console and kept in app state so **Stack Frames…** can
  jump to a source line, **Variables…** can list values and **Watch
  Expressions…** can show the last result. Reverse adapter requests (for example
  `runInTerminal`) are answered with an explicit failure rather than left
  hanging.
- **Breakpoints are data, not just markers.** A breakpoint can carry a condition
  or a log message; both are sent in `setBreakpoints` as `condition` and
  `logMessage`, and the gutter draws a diamond for either. Toggling the
  breakpoint off clears its extras, and changing breakpoints while a session is
  live resends them.
- **Attaching reuses the launch adapter but sends `attach`.** The prompt accepts
  a process id or `host:port`; a per-adapter argument builder maps it to
  `processId`, `connect`, `address`/`port`, `vmServiceUri` or the Go remote mode.
  Java's JDT-hosted attach and Ruby's rdbg attach are refused with a precise
  message instead of a broken session.
- **Java debugging is hosted by JDT LS.** `java-debug` is not a standalone DAP
  binary: it is an Eclipse extension jar. Kemori therefore pins and installs
  `com.microsoft.java.debug.plugin-0.53.1.jar`, passes it to the Java language
  server as `initializationOptions.bundles`, and drives the Java debug commands
  (`vscode.java.resolveMainClass`, `resolveClasspath`, `startDebugSession`) over
  the existing LSP connection. The resulting DAP session connects to the
  returned port and owns no child process, so stopping the debugger never kills
  the language server. If the server was already running without the jar, it
  must be restarted to load it.
- **Launch plans are explicit and honest.** `KEMORI_DEBUG_ADAPTER` (with
  optional whitespace-separated `KEMORI_DEBUG_ADAPTER_ARGS`) always wins.
  Python prefers the `debugpy-adapter` console script and falls back to
  `python3 -m debugpy.adapter`; `debugpy` is a pinned Kemori-managed tool
  (`debugpy==1.8.22`) with the same isolated Python install strategies as the
  language server. Rust/C/C++ prefer a discovered `lldb-dap` (or legacy
  `lldb-vscode`), falling back to `gdb -i=dap`; the program is
  `target/debug/<package>` or `a.out` when it exists. Go uses a pinned
  `dlv@v1.27.2` and the active file's package directory with `mode: "debug"`.
  JS/TS use a pinned `js-debug-dap-v1.140.0`, managed Node.js, and a reserved
  local TCP port. C# uses a pinned `netcoredbg` and the newest built
  `bin/Debug/*/<project>.dll`, reporting a missing build. Dart uses the managed
  SDK's `dart debug_adapter` over stdio. Ruby installs the pinned `debug` gem and
  runs `rdbg --open=vscode` over TCP. PHP installs the verified
  `vscode-php-debug` bridge and launches the file with Xdebug enabled; the PHP
  runtime and Xdebug remain prerequisites. Other languages report the missing
  prerequisite instead of inventing a launch configuration. A plan that starts
  but whose adapter fails names the adapter's stderr.
- **A debug session is another kind of project execution.** Starting it passes
  through the workspace-trust gate, like tasks and the terminal.

---

## Provisioning

- **User-local tool installs.** Provisioning never writes to a system-owned
  location: `rustup`/`go install` place tools in the user's home, Python uses a
  virtualenv Kemori manages under its data directory (which seeds its own `pip`,
  so a Python without `pip` or one marked externally managed still works), with
  `--user`/`pipx`/`uv` as fallbacks, and npm-based tools use a prefix Kemori
  manages under its data directory (with a matching cache). A missing
  permission therefore cannot make an install fail, and every resulting bin
  directory is searched when locating tools, alongside the usual user bin
  directories.
- **Verified install attempts.** A tool's provisioning is an ordered list of
  self-contained steps — run a command, download an archive (optionally
  verifying a published SHA-256), fetch and verify an Adoptium JDK, or extract.
  Extraction lists the archive first and refuses absolute or `..` paths, failing
  closed if it cannot list it, and unpacks a zip with `unzip`, `bsdtar` or a
  Python interpreter so it does not depend on one distribution's tooling. Kemori
  re-probes the tool after every attempt rather than trusting a package
  manager's exit code, and reports a missing prerequisite (a whole absent
  toolchain) instead of offering an install that cannot run.
  Installs into the shared managed directory are guarded by an advisory lock
  (with stale-lock recovery) so concurrent Kemori instances cannot corrupt the same
  npm prefix or virtualenv.
- **Managed toolchains.** Some language servers need a whole toolchain, so Kemori
  provisions one under its own data directory and launches the server with it:
  the .NET SDK for OmniSharp (`DOTNET_ROOT`), a checksum-verified Eclipse Adoptium
  JDK for Eclipse JDT (`JAVA_HOME`), a dedicated JDK 21 for Kotlin (kept separate
  from JDT's JDK 25), a checksum-verified Dart SDK for `dart language-server`, a
  coordinated Erlang/OTP + Elixir + ElixirLS stack for Elixir, and the official
  Swift toolchain for `sourcekit-lsp`. `Tool::launch_env` supplies the environment
  for both probing and launching; the user's system environment is never modified.
- **Every managed download is verified and pinned.** A `Download` step carries a
  required SHA-256 (there is no way to construct one without a digest), and the
  other download steps verify upstream data: `GithubRelease` (the SHA-256 digest
  GitHub reports for a release asset), `BobBuild` (the checksum `builds.hex.pm`
  publishes), `AdoptiumJdk` (the checksum in the Adoptium API response),
  `NodeRuntime` (`SHASUMS256.txt`), `DartSdk` (the sibling `.sha256sum`),
  `GoToolchain` (the `go.dev` release JSON), `DotnetSdk` (the SHA-512 in
  Microsoft's `releases.json`), `RustupInit` (the `.sha256` beside the versioned
  `rustup-init` binary), `DownloadGpg` (Swift's detached signature, verified
  against a private keyring) and the pinned `App::cpanminus`. Every input is an
  exact version: `jdtls` is a milestone snapshot with its published `.sha256`
  rather than `-latest`; `lua-language-server`, `kotlin-language-server` and
  OmniSharp use a pinned tag with a GitHub digest or a pinned SHA-256; the Go
  toolchain and the Adoptium JDKs are pinned by version; and package-manager
  installs (`go`, npm, pip, gem, cpan, `cargo`) name an exact version, leaving
  only `rustup component add` and `mix local.hex`/`local.rebar` as delegations
  to the manager (tracked in [LIMITATIONS.md](LIMITATIONS.md#provisioning)).
  Downloads use a stall-detecting, long-transfer timeout and are refused when
  the target filesystem is too small (the download-size estimate, or an
  archive's own size before extraction). HTTP(S) proxy variables are forwarded
  to `curl`. Each install is bounded, locked and re-probed by launching the
  **real server**, and a verified download that still cannot run (a missing
  shared library, a broken launcher) reports the real reason instead of a
  generic failure. Hashing goes through whichever tool the system has —
  `sha256sum`/`sha512sum`, `shasum`, `openssl dgst` or a Python interpreter —
  and a value that is not the right length of hex is rejected, so a malformed
  tool output cannot pass verification.
- **Platform support is resolved, never guessed.** `swift_platform` and
  `bob_platform` map the running distribution from `/etc/os-release` (including
  `ID_LIKE` derivatives) to the exact upstream artifact. A distribution upstream
  does not build for is reported as unsupported — Kemori does not download a
  toolchain that cannot run there — while a glibc system with no explicit `bob`
  build uses a best-effort target and still verifies the result. Perl bootstraps a
  checksum-verified `cpanm` so an interactive, unconfigured `cpan` is never run,
  and installs into an isolated `local::lib` (`PERL5LIB`).
- **Swift is portable, not allowlisted.** A distribution swift.org does not build
  for gets the portable **UBI10** toolchain plus a managed compatibility layer:
  `libncurses.so.6` is aliased to the system's `libncursesw.so.6` (the same
  library, a different soname) and `libxml2.so.2` is linked from the system, both
  from a directory under Kemori's data directory and used through a scoped
  `LD_LIBRARY_PATH`. The system is never modified and no library is bundled; if
  the distribution cannot provide `libxml2.so.2`, Kemori names the package instead
  of downloading a toolchain that cannot run. `libc_is_musl` keeps the glibc
  build off musl systems.
- **A managed toolchain can provision the tool that installs another tool.** The
  official Go toolchain is one managed component that unlocks `gopls`, `sqls` and
  `shfmt`: `go_attempts` first tries the user's `go`, then a managed Go archive,
  and every `go install` writes to a Kemori-private `GOPATH`/`GOBIN`, so nothing
  lands in `~/go`. `gofmt` has no `go install` step — it ships with Go — so its
  plan is the toolchain archive alone. The same "try the ambient tool, then
  provision a self-contained one" shape covers `cargo`/`rustup` for `asm-lsp`.
  Downloads verify the checksum the source actually publishes (`go.dev/dl` JSON,
  GitHub asset digests) and fail closed.
- **Prefer a small, self-contained upstream bundle over a system toolchain.**
  `clangd` comes from the clangd project's own release (a ~120 MB bundle with its
  resource headers) rather than requiring LLVM; `asm-lsp` prefers its prebuilt
  release; `phpactor.phar` removes the Composer dependency. These bundles are
  gated behind `libc_is_musl` where they link glibc. Native dependencies are
  checked, not assumed: only a build whose libraries are present is offered, and
  a missing one is named.
- **A runtime Kemori cannot verify is a prerequisite, not a download.** PHP and
  Ruby interpreters are discovered and reused, never fetched: official PHP is
  source-only and the third-party static builds publish no checksum, and Ruby has
  no official portable binary. Kemori still isolates what it can — `solargraph`
  installs into a private gem home with `GEM_HOME`/`GEM_PATH` scoped to the
  process — and reports the missing runtime precisely.
- **Provisioning is isolated and bounded.** Install subprocesses run from Kemori's
  own tools directory (never the user's project, so a local `.npmrc` cannot
  hijack `npm`), downloads and commands have time and output limits, checksum
  verification fails closed, and the install lock records a nonce so a reclaimed
  holder cannot delete a successor's lock.
- **Downloads pass through a verified, digest-keyed cache.** A download is
  written to a temporary sibling file, verified, and only then renamed into
  place; a verified copy is stored under `<tools>/cache/<algorithm>-<digest>`
  and reused without re-downloading. Cache identity is the expected digest,
  never the filename, and a cached file is re-verified before use — a corrupt
  entry is discarded and redownloaded, and a failed download or checksum
  mismatch never enters the cache. Checksum and release metadata is cached by
  URL alongside it, so a metadata-driven step can resolve an expected digest
  offline; a cached digest is still applied to the artifact, so it can only
  enable an offline install, never bypass verification. Hard links keep the
  artifact cache from duplicating large archives.
- **Installation is atomic per step.** An extraction stages into a sibling
  directory and promotes it with a same-filesystem rename, renaming the previous
  installation aside first and restoring it if promotion fails. This is per
  `Extract` step, not a whole-plan transaction: a plan that installs A then
  fails on B may leave A installed, and Kemori reports the incomplete plan rather
  than a false success.
- **Install plans are pre-flighted.** `install()` keeps only strategies whose
  every step is available and validates the tools directory and disk space
  before any download, so an obviously impossible plan is refused before work
  starts. Network and archive contents stay runtime checks.
- **Package-manager delegations are explicit.** Hex is pinned to an exact
  version; rebar3 is a checksum-verified download registered through
  `mix local.rebar rebar3 <path>`. Only `rustup component add` remains a
  delegation to an upstream manager's own verification (rustup's signed
  manifests), and it is documented as such in
  [LIMITATIONS.md](LIMITATIONS.md#provisioning).

---

## Project, search and git

- **Project setup is planned, then delegated.** Opening a project detects its
  languages with a bounded, `.gitignore`-aware, generated-directory-skipping
  walk (the same one quick open uses) and extension-based classification. That
  list is turned into an explicit plan (`language::setup`) whose states are
  `Ready`, `NeedsInstall`, `Prerequisite` and `Unavailable`; the planner decides
  *what* is needed and the existing provisioning subsystem decides *how* to
  install it. Execution installs the plan's missing tools one at a time through
  the normal, locked provisioning path, records each result, and never
  reinstalls an already-available server. Detection is deliberately
  extension-only so the scan is cheap and deterministic; a script whose language
  only a shebang or its content would reveal is not counted by the project scan
  (opening it still detects it normally).

- **Search and replace share one matcher, and replacement is a background job.**
  Project search and project replace compile the same query (literal, whole-word
  or the small built-in regex) with the same include/exclude globs, so the
  preview and the write cannot disagree. The scan runs on the worker; replace
  re-reads each file, writes through the atomic file helper and reports every
  changed file. Open buffers with unsaved edits are skipped and named rather
  than overwritten, and clean open buffers reload immediately after the write.
  Replacement text is literal (the regex engine has no groups). An empty
  replacement is treated as a delete and requires a second confirmation.

- **Task output is streamed, bounded and cancellable.** A run task is spawned
  with piped stdout/stderr; `process::wait_streaming` forwards UTF-8 chunks to
  the UI while keeping the same per-stream byte cap and deadline as
  `wait_captured`. The reader threads drain after the cap, so a chatty child
  cannot block or exhaust memory. A shared cancellation flag is checked on every
  poll and kills the child; the result distinguishes cancellation from a
  timeout. The panel follows the tail while the user has not scrolled up, and
  the final batch replaces the live view with stdout and stderr separated (the
  live interleaving is arrival order, not a claim about the child's write
  order). Only one task runs at a time. Completed outputs are kept for the
  session (the last ten) so the user can reopen one without rerunning it, and
  `file:line[:col]` references are extracted into Problems using the same
  conservative parser as the output jump; that extraction is a heuristic, and
  the raw output stays authoritative. **Run Active File** is a small
  language→interpreter table in `project/tasks.rs`, not a build system: only
  languages with a genuine single-file runner are offered, the command runs
  with the project root as its working directory, and an unsaved buffer is
  refused because the child reads the file on disk.

- **Workspace diagnostics are a bounded built-in snapshot.** Opening Problems
  asks the worker for one scan of the unopened project files. The scan is
  extension-detected, `.gitignore`-aware, capped by files and problems, and runs
  only providers that declare structural diagnostics; open files are skipped
  because their live diagnostics are authoritative. The result is a snapshot,
  not a file watcher: `r` refreshes it. LSP diagnostics stay limited to open
  documents until pull diagnostics are implemented, and the UI says so rather
  than implying a full type-check of the project.

- **Deterministic project scaffolding.** `project/create.rs` writes conventional
  project files directly instead of invoking `cargo`, `go` or `pip`, so
  creation is instant, offline and toolchain-independent. It never deletes a
  partial project: a failure returns the root so the UI can explain what
  happened.
- **Lazy, `.gitignore`-aware file tree.** Directory listings are read only when
  expanded; ignored directories (`target`, `node_modules`, …) are skipped, and
  the project's `.gitignore` rules (including nested files and
  `.git/info/exclude`) hide generated and local files from the tree, quick open,
  the inline filter and workspace symbol search. Toggling hidden files reveals
  them again.
- **Git via subprocess.** We shell out to `git` instead of linking a heavy
  library. Failures degrade gracefully (no repo → no git UI).
- **Git is parsed losslessly.** `git status` is read as `--porcelain -z` raw
  bytes, so leading status columns, spaces, non-ASCII names and renames survive;
  staging is scoped to the workspace root.
- **Adversarial input is bounded** (shared with search). The regex engine
  carries a per-line step budget and the `.gitignore` glob matcher is
  polynomial; a crafted pattern degrades to "no match" instead of freezing the
  editor.

---

## Terminal

- **The integrated terminal is a Unix PTY built on `libc`.** `openpty` allocates
  the pair, the shell is started in a new session with the PTY as its
  controlling terminal, and `TIOCSWINSZ` keeps it in step with the panel.
  Windows is out of scope, so a raw Unix layer is smaller and more direct than a
  process-management framework; `libc` was already a transitive dependency.
  Sessions run in the project root, are owned by Kemori, and are closed by
  signalling the whole process group (SIGHUP, SIGTERM, SIGKILL) so a foreground
  job cannot leak.
- **The screen is a small ANSI interpreter, not a full VT emulator.** It handles
  cursor movement, erase, scroll regions, tab stops, alternate-screen programs,
  OSC titles and UTF-8 split across PTY reads, with a bounded scrollback.
  Colour attributes are deliberately ignored: terminal text uses the theme's
  text role, which keeps the transparent terminal-native surface intact. This is
  an honest subset; full 24-bit SGR and mouse reporting are not emulated.
- **Keys are encoded, not re-dispatched.** While the terminal owns the keyboard
  Kemori turns key events into the byte sequences a terminal sends (including
  modifiers and function keys), so line editors and full-screen programs work.
  A small allow-list keeps application chords available: `Ctrl+J` leaves the
  panel, `Ctrl+Shift+PageUp`/`PageDown` switch panel tabs, `Ctrl+Shift+T`/
  `Ctrl+Shift+W` manage sessions, and the palette, save, search and diagnostics
  function keys pass through. `Ctrl+C` belongs to the shell, not to Kemori.
- **Execution needs workspace trust.** Tasks, the terminal and the debug-adapter
  preview run project code, so they pass through one gate; the decision is per
  root, stored under Kemori's state directory, and never inside the project.
  Editing, git, search and the language servers Kemori provisions itself do not
  require it. This keeps "open a cloned repository" safe while still allowing
  the user to trust it explicitly, or run one action once.

---

## UI

- **Centralised, data-driven themes + art.** All colour flows through
  `ui/theme.rs`; all ASCII personality through `ui/art.rs`. Widgets render
  semantic roles (accent, text, selection, panel, border, syntax…), never
  literal colours. Four themes are built from those roles — **Mellow** (the
  default), **Midnight**, **Daylight** and **Ansi** — and the active `ThemeId`
  is a thread-local so a swap reaches every widget without threading a `Theme`
  through the whole render tree or churning call sites. Adding a theme means
  adding one `Theme` value and one `ThemeId`, nothing else. This keeps Kemori
  recognisable and easy to evolve.
- **Scenes are drawn on a canvas.** `ui/art.rs` composes each welcome scene by
  placing glyphs at coordinates on a small `Canvas`, then turning runs of equal
  style into spans. This keeps the art symmetric and lets one element animate
  without the rest drifting; every scene is padded to an equal width so
  per-line centering stays aligned. The welcome composer budgets its height and
  drops the scene for just the familiar, or nothing, on small terminals.
- **Motion is a toggle, not a config file.** `App::motion` (palette:
  **Toggle Animations**) freezes the scene and the busy sparkle. Zero
  configuration is preserved; reduced motion is one keypress away.
- **Overlays are centered and cleared** with `Clear` before drawing so they are
  readable over the editor.
- **The bottom panel is one surface with per-tab state.** Problems, task output
  and debug-console messages share a single keyboard-focusable panel instead of
  separate modal views, so a jump to a reference does not destroy the output the
  user was reading. The panel never opens focused on its own: an event may make
  it visible, but focus moves in only through `Ctrl+J` or a palette command.
  Problems are derived from the documents on demand rather than copied into a
  second store, so they cannot go stale; Output and Debug Console keep their own
  content and scroll positions. The panel is bounded at both ends (a minimum
  that keeps it useful, a maximum that always leaves the editor readable) and
  uses the terminal background like every other surface.
- **Transient status messages** expire so the language/git summary returns.
- **Welcome-first startup.** Kemori opens on the welcome home screen. A path from
  the command line is offered as an action, a saved session as a resume action,
  and recent projects/files are loaded from the state directory; nothing is
  opened until the user chooses. Session saving is gated on the user engaging
  with a project, so an untouched session is never overwritten.
- **OSC 52 clipboard.** Copy works over SSH and in most terminals; internal
  clipboard is the fallback and bracketed paste handles system paste.

---

## Platform

- **Linux and macOS are supported; Windows is not yet supported.** There is no
  Windows CI, several installers require a Unix `sh`, and path/launcher
  conventions are not handled. See [LIMITATIONS.md](LIMITATIONS.md#platform).

---

## Product and configuration

- **Zero configuration by default; customisation is in-app.** Kemori must work
  with no file to write. Customisation is reached from inside Kemori (a
  **Settings** screen in the palette); the on-disk settings store under Kemori's
  state directory is an implementation detail the user never has to maintain. A
  missing settings file must change nothing. This is a deliberate boundary
  against becoming a config-file-first tool.
- **Settings are global, small and write-on-change-verified.** There is exactly
  one settings store (`$XDG_STATE_HOME/kemori/settings.json`), never a
  project-level file, and the surface is a fixed list of typed preferences — no
  generic preferences framework, no config DSL. A malformed file is renamed to
  `settings.json.corrupt` and the defaults load; unknown fields are ignored so a
  file written by a newer Kemori still loads. The store is written on quit and
  only when something actually changed, matching the session and recent stores,
  so Kemori never creates a settings file for a user who changed nothing; this
  also keeps the test suite from touching a real user store. A preference that
  affects a subsystem is applied through the smallest interface (the editor
  gains one indentation preference; the theme a thread-local; automatic
  completion a single guard), not by passing a `Settings` object through every
  call.
- **Arbitrary keybindings and user themes are deliberately out.** Kemori keeps a
  conventional, keyboard-first keymap and a curated, bundled theme set. A
  keybinding editor and a theme language/marketplace are recurring "obvious next
  features" that would trade Kemori's zero-configuration identity for a
  configuration surface; they are excluded on purpose, not pending.
- **"Supported" means complete, not "an LSP starts".** A language is supported
  only when detection, offline built-in features, provisioning, server launch,
  server-backed features, lifecycle (version/update/remove) and recovery all
  hold. The contract is written down in
  [PRODUCT.md](PRODUCT.md#what-supported-means); the current per-language state
  is in [LIMITATIONS.md](LIMITATIONS.md).
- **The full product direction lives in [PRODUCT.md](PRODUCT.md).** It is the
  single authoritative statement; `ROADMAP.md` turns it into work and
  `LIMITATIONS.md` records the honest current state.

---

## Brand and rename

- **Koda became Kemori in 3.0.** The name "Koda" is shared by several unrelated
  terminal and AI coding projects, including a commercial AI assistant and a
  separate Rust coding agent. Search results and AI overviews merged the
  entities. A unique name is the only durable fix; author metadata cannot
  disambiguate a shared product name.
- **The public identity is the crate, library, binary, repository, docs and
  package name together.** Kemori uses `kemori` for all of them. Do not
  reintroduce a split like `koda-ide`/`koda`, which made the old entity harder
  to resolve.
- **History is not rewritten.** Old changelog entries, old release notes, old
  tags and immutable crates.io versions keep the old name as a historical
  record. The current README carries the deprecation notice, the 3.0.0
  changelog entry explains the rename, and the latest old-crate release is a
  deprecation shim.
- **Compatibility is narrow and temporary.** The only runtime references to the
  old name are the one-time `~/.local/state/koda` import and the
  `KODA_REMOTE`/`KODA_DEBUG_ADAPTER` fallbacks. Removing them is deliberately
  deferred to a very late stage, not before 4.0 and potentially to the end of
  the project; removing them earlier would lose user state or silently break
  configured environments.

---

## Splitting `src/app/mod.rs`

`src/app/mod.rs` was 8876 lines of state, an `impl App` and its tests. It has
been split into sibling modules under `src/app/` by **mechanical extraction
only** — one extraction per commit, no behaviour change, the full suite green
after every commit. A child module of `app` sees `App`'s private fields, so each
module holds its own `impl App` block; extracted methods are `pub(super)` so the
event loop and sibling modules can call them.

The originally proposed module set was realised (tests, git, lsp, diagnostics,
session, files, keys, language, commands). Because the proposal's own estimates
for `keys.rs`/`language.rs` exceeded the 800-line rule and it omitted the editing,
pane, search, status and background-event code, the production code was
distributed a little further so every production file is under 800 lines:

| Module | Contents | Lines |
| --- | --- | --- |
| `mod.rs` | state types, constructor, small helpers | 789 |
| `availability.rs` | command availability and format checks | 167 |
| `background.rs` | worker-event application, external-change polling, pumping | 431 |
| `commands.rs` | command dispatch, palette, prompts, quick open | 468 |
| `completion.rs` | completion popup and hover | 366 |
| `diagnostics.rs` | diagnostics polling, navigation and list | 187 |
| `editor.rs` | cursor moves, clipboard/kill-ring, comment, matching bracket | 286 |
| `files.rs` | save/close/revert and file create/rename/copy/delete | 370 |
| `git.rs` | diff, changed files, staging, commit | 257 |
| `keys.rs` | key dispatch (global, editor, tree, overlay, search, completion) | 864 |
| `language.rs` | navigation, symbols, rename, code actions, formatting | 587 |
| `lsp.rs` | server lifecycle and response handling | 884 |
| `mouse.rs` | editor hit-testing, click and drag selection | 82 |
| `overlay.rs` | picker, prompt, search and settings state | 916 |
| `panel.rs` | bottom panel state, task output and problem extraction | 650 |
| `panes.rs` | split panes and tabs | 170 |
| `project_search.rs` | project search/replace state, keys and hand-off | 547 |
| `run.rs` | the main event loop | 86 |
| `search.rs` | find/replace | 202 |
| `session.rs` | session persistence, workspace and welcome actions | 484 |
| `settings.rs` | Settings screen and preference application | 211 |
| `setup.rs` | tool installation, project setup plans and tool actions | 369 |
| `status.rs` | statusline messages and toasts | 132 |
| `tasks.rs` | task picker, streaming task output and Run Active File | 228 |
| `view.rs` | tree/diagnostic/wrap/fold toggles and reveal | 108 |

`app/tests.rs` is the deliberately single test module moved whole from `mod.rs`.
`app/keys.rs`, `app/lsp.rs`, `app/overlay.rs`, `language/web/mod.rs`,
`language/lsp/mod.rs` and `ui/overlay.rs` have since been split or reduced as
well; every `.rs` file under `src/` is now under 800 lines. `commands.rs` was
brought back under 800 by moving availability into `availability.rs` and
tool/project setup into `setup.rs`. `git`, `project/create`, `app/overlay` and
`app/tests` had their test modules moved beside them.

The same mechanical rule was later applied to `editor/document.rs` (a directory
of `mod`/`cursors`/`typing`/`movement`/`highlight`/`helpers`/`tests`, with
cross-module inherent methods promoted to `pub(super)`) and to
`language/tools.rs`. Because `tools` is one module of free functions whose
visibility crosses almost every item, its parts are `include!`d in
`tools/mod.rs` rather than declared as separate modules: the extraction is
purely textual, so no visibility annotation or call site changes and the
compiler still type-checks the whole module as one unit. Each included file is
under 800 lines, and the test module is split into three included parts.
