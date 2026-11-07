# Known limitations

Grouped by subsystem. Each entry is a real, verified limitation, not a wish.
Open work that would remove one lives in [ROADMAP.md](../ROADMAP.md); this file
describes the current state.

## Rename migration

- **The old state directory is imported once.** On first launch Kemori copies
  settings, sessions and recents from `~/.local/state/koda` (or the
  `XDG_STATE_HOME` equivalent), never overwriting a current Kemori file. The old
  directory is left untouched, so rolling back is safe, but later changes made
  in Kemori are not written back to it. The importer is retained through 3.x.
- **Old environment variables survive for one release.** `KEMORI_REMOTE` and
  `KEMORI_DEBUG_ADAPTER` are preferred; `KODA_REMOTE` and
  `KODA_DEBUG_ADAPTER` still work as fallbacks. Both the state import and these
  fallbacks are scheduled for removal no earlier than 4.0, potentially as a
  final end-of-project cleanup; see [ROADMAP.md](../ROADMAP.md#very-late-end-of-project).

---

## Editor

- **Non-UTF-8 files open read-only.** A file that is not valid UTF-8 (but is not
  binary) is decoded lossily and opened read-only; saving is blocked until the
  user runs **Convert to UTF-8**. This keeps the original bytes intact rather
  than writing replacement characters back. Binary and oversized files are still
  refused.
- **`perf` is untested at the extremes.** There is no budget test for a 50 MB
  file or for startup time; large-file behaviour is only known to be
  rope-backed, not measured.
- **Signature help renders one documentation line per level.** Signature and
  parameter documentation are flattened to their first non-empty line; markdown
  is not rendered. Overload cycling uses unmodified PgUp/PgDn while the popup is
  visible.
- **Completion snippets are a subset of LSP snippet syntax.** `insertText`/
  `textEdit`, `$1`, `${1:default}` and `$0` work, but variables, choices,
  transforms and `additionalTextEdits` are not implemented. Unsupported
  placeholders remain literal text.
- **Workspace folders cover one root, and progress is statusline-only.** Kemori
  advertises the active project root rather than a multi-root list; multi-root
  workspaces remain Phase 4. Progress messages are not a navigable log and a
  server that never sends `end` leaves its token in the statusline until the
  session closes.
- **Cancellation is per-feature and server-dependent.** Kemori cancels older
  requests of the same feature (except diagnostics and commands) and timed-out
  requests, but it cannot guarantee a server stops computing or ignores the
  cancelled id. The app still validates every response before using it.
- **Watched-file notifications depend on server registration and simple globs.**
  Kemori only sends them after a server registers through
  `client/registerCapability`; a pattern relative to a `RelativePattern` base
  other than the workspace root is matched as a plain workspace-relative glob.
  Directory deletes are reported as one delete at the directory URI, not one per
  contained file.
- **Organize imports and format selection depend on server support.** Organize
  Imports needs `source.organizeImports` code actions and applies only the first
  returned action; Format Selection needs `documentRangeFormattingProvider` and
  has no built-in formatter fallback. Empty or unsupported results are reported
  rather than guessed.
- **Document links are a picker, not inline click targets.** File links open in
  the editor; URLs and other schemes are copied to the clipboard rather than
  launched. Links that require `documentLink/resolve` are skipped, and the
  command asks for the whole document on demand.
- **Implementation and type-definition lookup need their server capabilities.**
  Both jump to the first returned location; multiple results are not offered as
  a picker, and the built-in provider fallback used by Go to Definition does not
  apply to these commands.
- **Hierarchies need server support and are flat lists.** Incoming calls,
  outgoing calls, supertypes and subtypes each need their respective capability;
  otherwise the command explains why. Results are a single picker level, not a
  live expandable tree, and following a node opens its source rather than
  re-rooting the hierarchy.
- **Selection ranges need a server and follow its chain.** Without
  `selectionRangeProvider`, Expand/Shrink Selection report that no ranges are
  available. Only the primary caret is queried, the cached chain is skipped
  while a selection exists, and a column selection is treated as its enclosing
  linear range when shrinking.
- **Code lens requires a server command and has no resolve step.** Lenses are
  requested for the whole document, rendered at the end of their line and run
  through `workspace/executeCommand`. Servers that return only `data` and expect
  `codeLens/resolve` are listed but cannot be executed yet, and large files skip
  code lenses entirely.
- **Inlay hints are whole-document and server-defined.** A server without
  `inlayHintProvider` shows none. Kemori asks for the whole document rather than
  the viewport, does not resolve hint locations or commands, and renders all
  kinds with the same dim style. Large files skip inlay hints entirely.
- **Document highlights depend on the server and the primary cursor.** A server
  that does not advertise `documentHighlightProvider` shows none, only the
  primary cursor is queried, and read/write/text kinds share one background
  colour. Highlights are not persisted across sessions and are recomputed after
  edits or cursor movement.
- **Read-only mode is per open document and not persisted.** Toggling it off
  restores editing; it does not change file permissions on disk, and a reopened
  file starts editable. It is separate from the automatic non-UTF-8 state, which
  still requires **Convert to UTF-8**.
- **Large-file mode has a fixed threshold and no override.** At one million
  characters syntax highlighting, diagnostics, completion, language-server sync
  and git line markers turn off automatically. There is no per-file toggle to
  re-enable them, and files above the 64 MiB open limit are still refused.
- **Replace preview is line-oriented.** It lists whole lines for single-line
  matches and flattens a multi-line regex match onto its starting line as `⏎`
  text. It has no per-match accept/reject or inline diff; confirming applies all
  current matches at once (inside the selection scope when one is active).
- **Scoped search is contiguous-only.** **Find in Selection** stores one
  `(start, end)` range. A rectangular selection is refused rather than reduced
  to its bounding box. The range is clamped after external edits, and `Alt+S`
  clears it; there is no per-row or multi-range search scope.
- **Rectangular selection is keyboard-only.** `Ctrl+Alt+Shift+arrows` starts a
  column selection and Shift arrows extend it; Alt-drag with the mouse is not
  wired, and a rectangle only covers whole character columns (no virtual space
  past the end of short lines).
- **Snippets are lexical built-ins, not an extension format.** The picker lists
  curated snippets for supported languages; placeholders are `$1`, `${1}`,
  `${1:default}` and `$0`. Mirrors, variables, nested placeholders and
  user-defined snippet files are not supported.
- **Persistent undo is bounded and clean-file-only.** At most the last 100 undo
  steps per file are written to the per-project session, and only for files that
  are clean at quit. A file changed on disk after the session was saved has its
  history refused because the recorded offsets no longer describe its bytes.
- **Folding is not persisted.** Folds are recomputed from indentation (or a
  language server's folding ranges) and are not saved in sessions, so reopening
  a file starts unfolded. Undo/redo and reloading a file clear its folds rather
  than guessing where an old header now points; ordinary edits remap them.

## Search and project

- **Project setup detects languages by extension only.** The project scan walks
  up to 4000 files with the same bounded, `.gitignore`-aware rules as quick
  open, and counts a language only from a recognised file extension. A project
  whose only source is, say, a shebang-only script with no extension is not
  detected by the project scan; opening the file still detects it normally.
- **Project setup installs missing tools one at a time.** It reuses the single
  install path (and its advisory lock), so a project with several missing
  toolchains provisions them sequentially. The UI stays responsive, but a large
  managed toolchain (for example Swift) can take a while. A failure in one
  language does not stop the others; the summary reports installed, ready,
  failed and needs-attention counts.
- **Project setup is offered once per workspace.** After **Later** it does not
  re-prompt; reopen it with **Set Up Project…** in the command palette (or from
  **Language Setup…** for individual tools).
- **Task output streams, with session-only history.** A task's stdout/stderr is
  appended to the panel's **Output** tab live and the view follows the tail
  until the user scrolls up. `x` cancels, `r` reruns the last task and `h`
  reopens one of the last ten completed runs; the list is not persisted across
  launches. The display cannot retroactively reorder stdout and stderr — the
  final batch separates them.
- **Task references are a heuristic.** The Problems tab extracts up to 200
  `file:line[:col]` references from the last task's output, keeps only paths that
  exist, and infers severity from the words `error`/`warning` in the line. A
  compiler or test runner with an unusual format may not produce entries; the
  raw Output tab is always authoritative.
- **Run Active File covers interpreters, not compiled files.** Python, Shell,
  JavaScript, Ruby, PHP, Lua, Perl, Go, Dart and Elixir have a single-file
  runner. Rust, C/C++, Java, C#, Kotlin, Swift, Assembly and configuration
  languages are refused with a reason instead of guessing a build. The file must
  be saved first, because the task runs the bytes on disk, not the buffer.
- **A task can run to its ten-minute deadline if it is never cancelled.**
- **The Problems tab mixes live, built-in and language-server diagnostics.**
  Open documents use their live provider diagnostics or pulled/pushed server
  diagnostics. Files Kemori has not opened are scanned with the provider's
  built-in structural checks (up to 4,000 files and 2,000 problems) and, when a
  server advertises `workspaceDiagnostics`, with a `workspace/diagnostic`
  request. The built-in scan is a snapshot refreshed with `r`; the server request
  is sent on refresh and when a server connects while Problems is visible.
  Servers that only support pushes and not pull report open documents only.
- **The terminal is an ANSI subset.** The **Terminal** tab runs a real PTY shell
  with sessions, scrollback, copy/paste, resize and process-group shutdown, but
  the screen model ignores colour/SGR attributes (text follows the theme), does
  not implement mouse reporting, bracketed-paste mode or the full VT100
  repertoire, and terminal sessions are not persisted across launches. Selection
  is screen-wide (`Ctrl+Shift+C` copies the screen and scrollback), not a
  mouse-drag rectangle. A full-screen program that depends on an unimplemented
  escape sequence may render imperfectly.
- **Workspace trust covers execution, not language servers.** Tasks, the
  terminal and the debug-adapter preview ask once per project root. Language
  servers that Kemori provisions itself are not project code and start without a
  prompt; a server launched from the project's own environment is not yet
  distinguished. The decision is per root and stored in
  `$XDG_STATE_HOME/kemori/trust.json`; there is no global "trust everything"
  switch.
- **Project search is bounded and literal in its replacement.** At most 500
  matching lines are listed (replace-all still covers every file), and queries
  use either literal matching or the small regex subset (no groups,
  alternation or `{n,m}`). Replacement text is literal: `$1`-style capture
  references are not expanded. Include/exclude globs reuse the `.gitignore`
  matcher.
- **Project replace refuses unsaved buffers.** A file open with unsaved edits
  is skipped and reported instead of being overwritten on disk, so the user can
  save or close it and replace again. Files edited on disk between the search
  and the replace are replaced using their current contents, not the searched
  snapshot.

- **`.gitignore` discovery is capped.** At most 256 nested `.gitignore` files and
  4096 directories are read per project; in an enormous monorepo a deeper rule
  beyond the cap is not applied. The caps keep project open predictable.
- **The workspace model is single-project.** `Workspace` tracks one project and
  its language context; there is no navigable multi-project structure.

## LSP

- **Document sync follows the server.** Kemori sends incremental `didChange`
  ranges when a server asks for incremental sync and a full-document
  replacement otherwise (which the protocol permits as the safe fallback).
- **A server is launched per language, not per workspace root.** One server per
  `LanguageId` is tracked; a monorepo with several roots of the same language
  shares one server.
- **Pull diagnostics are advertised, not guaranteed.** Kemori requests
  `textDocument/diagnostic` and `workspace/diagnostic` only when the server
  advertises them; a push-only server keeps its existing behaviour. Workspace
  results are a snapshot taken on refresh, and an unopened file is read from disk
  only to convert positions (files over 8 MB are skipped). A server that requires
  files to be opened before analysing them will not report them in a workspace
  request.
- **Folding ranges are best-effort.** A server's `textDocument/foldingRange` is
  used when it advertises folding ranges; otherwise Kemori falls back to
  indentation-based folding.

## Provisioning

- **The verified cache has no eviction.** A verified download is stored under
  `<tools>/cache/<algorithm>-<digest>` and reused offline, so a reinstall or
  repair needs no network. Checksums and release metadata are cached by URL too,
  so metadata-driven installs can also fall back offline after their first
  successful fetch. Nothing evicts either cache, and removing a managed tool
  does not remove its cached archive, so they grow until the user clears them.
  The artifact cache uses hard links where the filesystem supports them, so it
  normally costs no extra disk over the downloads directory.
- **GPG-verified artifacts are not in the cache.** Swift's archive has no
  published digest to key on; it keeps the existing behaviour of reusing the
  already-verified file in the downloads directory (re-checked against its
  signature) rather than using the digest cache.
- **Atomicity is per step, not plan-wide.** A download is written to a temporary
  file and only renamed into place after verification, and an extraction stages
  into a sibling directory and promotes it with a rename, restoring the previous
  installation if promotion fails. But a plan that installs A and then fails on
  B can still leave A installed; Kemori reports the incomplete plan and the next
  attempt recovers. There is no whole-plan transaction.
- **Pre-flight is best-effort.** `install()` keeps only strategies whose every
  step is available and checks the tools directory and disk space before any
  download, so an unrunnable plan (for example Swift without `libxml2.so.2`) or
  a short disk is reported first. Network reachability, proxy behaviour, archive
  contents and permissions below the tools directory remain runtime checks.
- **`rustup component add` remains a delegation.** Hex is pinned by exact version
  and rebar3 is a Kemori-verified download. The only remaining floating
  provisioning input is rustup's component for the user's active toolchain:
  rustup chooses the component version and verifies toolchain artifacts against
  its signed manifests, and Kemori does not fetch or pin it independently.
- **Swift's extracted size is not estimated.** The disk check uses the download
  estimate before an install and an archive's own size before extraction, but
  there is no separate estimate of the unpacked size (Swift is ~1.1 GB download
  to ~3.5 GB extracted), so a disk with room for the download but not the
  extraction is caught only during extraction.
- **Removing a shared managed component removes its siblings.** Update/Remove
  act on the first directory Kemori owns beneath its tools root. Go-installed tools
  share `tools/gopath`, the npm servers share `tools/npm`, and the JDK and .NET
  runtimes are shared, so removing one tool can also remove another Kemori tool.
  Kemori never touches a user's own install.
- **Managed Swift is limited to swift.org's platforms.** The official Linux
  toolchains link against the distribution's libraries, so Kemori installs one only
  where swift.org builds for the running release (Ubuntu, Debian, Fedora, Amazon
  Linux, RHEL). Elsewhere it uses the portable UBI10 build plus a compatibility
  layer on glibc Linux, and on musl it only discovers an existing toolchain.
  Swift downloads are large (~1.1 GB; ~3.5 GB extracted) and need disk for both.
  A network failure is reported with a hint (network, proxy or TLS) rather than a
  bare `curl` error.
- **`Perl::LanguageServer` cannot build on Perl ≥ 5.41.** Its `Coro` dependency
  (latest release 6.57, 2020) does not compile against Perl 5.42's changed
  `Time::HiRes` API. Verified on this host (Perl 5.42.2): the install fails with
  `Module 'Coro' is not installed`. Kemori prefers **PLS**, which has no `Coro`
  dependency and installed successfully. A pre-installed `Perl::LanguageServer`
  is still discovered on any Perl.
- **PHP and Ruby still need their runtime.** Kemori provisions `phpactor.phar` and
  an isolated `solargraph` gem, but not a PHP or Ruby interpreter: official PHP
  is source-only and the portable static builds publish no checksum, and Ruby has
  no official portable binary. Both runtimes are reused when installed and
  reported as a prerequisite otherwise.
- **`clangd` and `asm-lsp` prebuilt bundles are glibc-only.** Their prebuilt
  releases link against glibc and libstdc++, so Kemori does not offer them on musl
  (Alpine/Void musl); a system clangd or the `cargo` fallback is used there. The
  official Go toolchain and Go-installed tools are static and work on musl.
- **Elixir/Erlang provisioning is Linux-only.** `bob_platform` returns `None`
  off Linux, so ElixirLS falls back to discovery on macOS.

## UI

- **Settings are global and fixed.** A **Settings** screen (theme, line numbers,
  animations, soft wrap, indent width, spaces vs tabs, inline diagnostics,
  automatic completion) writes one global store. There is no project-level
  settings file, no arbitrary keybinding configuration, and the preference list
  is fixed rather than extensible.
- **Bundled themes only.** Mellow (default), Midnight, Daylight and Ansi ship
  and are selectable at runtime; users cannot supply their own theme. The light
  theme is covered by deterministic render tests but has **not** been viewed on
  a real light terminal, so its readability is argued from contrast, not
  verified.
- **Terminal coverage is partial.** Colour support is detected once at startup
  (`NO_COLOR`, `COLORTERM`, `TERM`): a 16-colour terminal gets the bundled
  **Ansi** theme unless the user stored a choice, and `NO_COLOR` suppresses
  every colour code, with selections falling back to reversed text. Kemori asks
  for the kitty keyboard protocol at startup so `Ctrl+Shift` chords are reported
  distinctly; on a terminal that ignores the request those commands remain
  reachable through the command palette. Still open: an interactive check on a
  real light terminal (the Daylight theme is covered by deterministic render
  tests only).
- **No interactive diff/merge view.** The git diff is a read-only overlay that
  can apply or revert a hunk; a conflicting-file merge view does not exist.
- **Mouse support is partial.** The wheel scrolls the editor, a left click
  places the cursor and a drag selects text; there is no mouse-driven tree,
  tab, menu or scrollbar interaction.

## Language support

- **The authoritative contract is [LANGUAGE-SUPPORT.md](LANGUAGE-SUPPORT.md).**
  It records, per language, the built-in offline capabilities, the server and its
  pin and verification, the runtime prerequisite, the platforms and the
  verification status. This section lists only the known gaps.
- **Built-in depth varies.** Several languages have built-in highlighting,
  symbols and completion but thin or no built-in diagnostics; their quality
  comes from the language server where one is installed.
- **Formatters are discovery-only, not provisioned.** Python, Lua, Swift, Java,
  Ruby, Kotlin, SQL, PHP and C# format through installed `ruff`/`black`,
  `stylua`, `swift-format`, `google-java-format`, `rubocop`, `ktlint`,
  `sqlfluff`, `php-cs-fixer` or `csharpier`; Kemori does not provision those
  executables yet.
- **Most servers are not live-verified beyond Linux glibc x86_64.** Only `gopls`,
  `clangd`, `asm-lsp`, `PLS`, `sqls`, Dart, ElixirLS, Lua, Kotlin, HTML and CSS
  have had a live handshake, and only on Linux glibc. Every server on Linux musl
  and macOS is build/unit-verified only.
- **Several servers are platform-limited.** Elixir is Linux-only; Swift is
  native or portable on glibc Linux and discovery-only elsewhere; Assembly has
  no prebuilt aarch64 Linux binary; `clangd` is withheld on musl. See
  [LANGUAGE-SUPPORT.md](LANGUAGE-SUPPORT.md#platform-notes).
- **PHP and Ruby need their runtime.** Kemori provisions `phpactor.phar` and an
  isolated `solargraph` gem but not a PHP or Ruby interpreter (no verified
  portable upstream distribution), so their servers are Complete-with-a-
  prerequisite rather than zero-config. Built-in editing works offline with no
  runtime. See [Provisioning](#provisioning).
- **Nix's server needs Nix.** `nil` publishes no prebuilt release assets and is
  not the crates.io `nil` crate, so Kemori installs it (and `nixfmt`) from a pinned
  nixpkgs commit with `nix profile` into a Kemori-private profile. A system `nix`
  is therefore a prerequisite; on NixOS it is already present. The install is
  build/unit-verified here, not live-run because this host has no Nix. Nix
  editing, highlighting, diagnostics, symbols, completion and navigation are
  fully offline.

## Debugging

- **Every listed language now has a debug-adapter plan; DAP attach is still
  missing.** Kemori pins and installs `debugpy==1.8.22`,
  `dlv@v1.27.2`, `js-debug-dap-v1.140.0`, `netcoredbg-3.2.0-1092`,
  `java-debug-0.53.1`, Ruby's `debug` gem `1.11.1` and the
  `vscode-php-debug` bridge `1.40.2`, and uses the managed Dart SDK's
  `dart debug_adapter`. Rust/C/C++ discover `lldb-dap`/`lldb-vscode` and fall
  back to `gdb -i=dap`; `lldb-dap` itself is not downloaded because LLVM's
  official bundle is very large. Java requires a ready JDT LS and PHP requires a
  PHP runtime with Xdebug. Other adapters can be supplied through
  `KEMORI_DEBUG_ADAPTER` and `KEMORI_DEBUG_ADAPTER_ARGS`.
- **Attach support is adapter-specific.** Python, Go, JS/TS, C#, PHP and Dart
  accept a process id or `host:port`. Java attach needs the JDT LS attach flow
  and Ruby's rdbg attach is not wired, so both report a precise message.
- **Java debugging depends on a ready JDT LS.** Kemori loads the java-debug jar
  at JDT LS initialization, so a server that was already running before the jar
  was installed must be restarted. The active file's main class and runtime
  classpath are resolved through JDT LS; there is no launch-profile UI, VM
  argument editor or attach flow yet.
- **C# launch expects a built project in the workspace root.** The plan finds a
  `.csproj` directly in the workspace root and the newest matching
  `bin/Debug/*/<name>.dll`; project subdirectories, launch profiles, MSBuild
  arguments and `dotnet run` are not inferred yet.
- **JS/TS launch is one file, not a project configuration.** The JS/TS plan
  launches the active file with a reserved local port and managed Node.js. It
  does not infer a package's `main` entry, `npm start`, source-map directories,
  `ts-node`, or a browser target, so a TypeScript file compiled elsewhere may
  need a built JavaScript entry point and source maps.
- **Format on save applies to the active single-file save.** It writes, formats,
  and writes again. **Save All** still writes every dirty file without
  formatting them, and a very slow formatter finishes asynchronously after the
  first save rather than holding the save path.
- **The debug view is console-based.** Stack frames, scopes, variables and watch
  values are written to the **Debug Console** and can be reopened as pickers;
  there is no separate variables tree, hover evaluation, attach flow or
  multi-thread UI yet. Conditional breakpoints and logpoints are supported, but
  watch expressions evaluate only in the top frame and there is no expression
  editor beyond the prompt.
- **Launch configuration is inferred, not user-written.** The active file is
  the program for Python, and the conventional `target/debug/<package>` or
  `a.out` binary for Rust/C/C++. A project that needs arguments, environment
  variables, a remote target or a different binary cannot express that yet;
  `KEMORI_DEBUG_ADAPTER` only changes the adapter, not the launch arguments.

## Git

- **No merge tool and no hunk staging.** The changed-files list stages whole
  files. The diff overlay can apply or revert the hunk under the cursor, but it
  cannot stage or unstage a single hunk, and there is no merge-conflict view.
- **The SSH command runner is experimental.** `KEMORI_REMOTE` routes only git
  status through `ssh`; the file tree, editor, language servers and tasks stay
  local. It is an internal seam, not remote development.
- **Git operations are not cancellable.** Push, pull, fetch, diff, hunk
  apply/revert and GitHub creation run on the worker and report progress, so the
  UI stays responsive, but there is no cancel action once one has started.
- **A non-UTF-8 path may not round-trip.** Recent/session stores serialize paths
  lossily.

## Platform

- **Windows is not supported.** There is no Windows CI; provisioning still
  shells out to a Unix `sh` for the Erlang/OTP `Install` script, the Swift
  compatibility layer is `cfg(unix)`, and `language/format.rs` builds a child
  `PATH` with a hard-coded `:` separator. The code compiles in places but has no
  end-to-end path.
- **A `sudo` launch has the root account's tool context.** Kemori deliberately
  keeps its state in the root account's directory and only discovers tools on
  the system path (including `/run/current-system/sw/bin` and
  `/nix/var/nix/profiles/default/bin`); language servers installed only for the
  invoking user are not executed as root. This is a safety boundary, not a
  missing feature. System-wide packages make `sudo kemori` work, but running the
  whole IDE as root is still discouraged; see
  [README.md](../README.md#running-with-sudo).
- **Linux and macOS are supported.** CI builds and tests on Linux (glibc), in
  an Arch Linux container and on Alpine (musl), and on macOS. A job's presence
  is not proof it passed; see the "Platform and verification" matrix in the
  [README](../README.md#platform-and-verification) for what was observed.

## Packaging

- **Distribution packages cover Linux x86_64 only.** The release workflow
  builds `.deb` and `.rpm` on Ubuntu 22.04 (glibc 2.35) for x86_64, and the AUR
  packages declare `x86_64`. Linux aarch64 and macOS are served by the prebuilt
  tarballs or a source build, not by distro packages.
- **`.deb` and `.rpm` bundle the glibc build.** The musl archive remains the
  static option for other Linux environments. The distro packages do not
  include language servers or runtimes; those stay optional and are provisioned
  or discovered at runtime.
- **The AUR packages cannot currently be published.** AUR account registration
  has been disabled since the AUR incident a few months ago, so the `kemori` and
  `kemori-git` packages are not installable from the AUR yet. Until registration
  reopens, use crates.io, the `.deb`/`.rpm` assets, or `make install`. The
  packaging sources are maintained in `packaging/aur/`; the stable `pkgver` and
  checksum must be refreshed when the packages can be pushed.
- **Running the whole IDE as root is discouraged**, even though `sudo kemori`
  resolves from `/usr/bin` after a system-wide install. Use it only for files
  that need elevated privileges.
