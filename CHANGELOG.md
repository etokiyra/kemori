# Changelog

All notable changes to Kemori are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Kemori was previously published as **Koda** (crate `koda-ide`, binary `koda`).
Historical entries below retain the old name; 3.0.0 is the rename release.

## [Unreleased]

### Added
- **One-line installer.** `curl -fsSL https://kiyra.lol/kemori/install.sh | sh`
  detects the platform, downloads the matching prebuilt archive, verifies it
  against the published `SHA256SUMS`, and installs `kemori` into a directory
  that is already on `PATH` — `/usr/local/bin` when it is writable, otherwise
  `~/.local/bin`, `~/bin` or `~/.bin` — without `sudo` and without touching
  shell startup files. See [`scripts/install.sh`](scripts/install.sh).

### Fixed
- **The glibc Linux binary no longer requires `GLIBC_2.39`.** It was built on
  `ubuntu-latest` and inherited that runner's glibc, so it refused to start on
  Debian 12, Ubuntu 22.04, RHEL 9 and anything older. The release job now pins
  `ubuntu-22.04`, and the installer checks that the downloaded binary can load
  before trusting it, falling back to the static musl build when it cannot.
- **A recorded demo.** [`docs/images/demo.gif`](docs/images/demo.gif) shows a
  project being opened, the missing-`rust-analyzer` offer, the welcome scenes
  and Quick Open. It is produced from [`docs/demo.tape`](docs/demo.tape) by
  [`scripts/demo.sh`](scripts/demo.sh).
- **Organize imports and format selection.** **Organize Imports** asks the
  server for `source.organizeImports` code actions and applies the returned
  edit or command. **Format Selection** uses `textDocument/rangeFormatting` on
  the active selection, validates the buffer version, and applies the returned
  edits. Both report missing server support and empty results precisely.
- **LSP save and watched-file notifications.** Saves now send
  `textDocument/didSave` when the server requests it, including the full text
  when it asks for it. Servers that register `workspace/didChangeWatchedFiles`
  through `client/registerCapability` receive matching create/change/delete
  notifications for file operations, saves and external disk changes; watcher
  globs are respected and registrations can be withdrawn.
- **Superseded LSP requests are cancelled.** When a newer request replaces an
  older one of the same feature, Kemori sends `$/cancelRequest` and forgets the
  old pending id; timed-out requests are cancelled the same way. Diagnostics and
  `workspace/executeCommand` are exempt because they can legitimately overlap.
- **Swift debugging uses discovered lldb-dap.** SwiftPM builds are found in
  `.build/debug` (including triple-specific build directories), or beside a
  standalone source; the plan drives LLVM's discovered `lldb-dap`/`lldb-vscode`.
  A missing build or adapter is reported as a precise prerequisite.
- **Richer signature help.** Every overload is retained with its own
  documentation and parameter documentation, the server's active parameter is
  clamped to the selected signature, and PgUp/PgDn cycle overloads while the
  popup is open. The active parameter's documentation is shown beneath the
  signature.
- **Completion snippets are inserted.** The client advertises snippet support;
  completion items with `insertTextFormat: 2`, `insertText` or `textEdit` are
  expanded through the editor snippet engine. `$1`/`${1:default}`/`$0` stops
  work with Tab/Shift+Tab, and server text-edit ranges are converted before the
  insertion.
- **Completion documentation and resolve.** Initial completion items keep their
  detail and documentation, and servers with `completionProvider.resolveProvider`
  are asked to resolve the selected item. The popup shows the selected item's
  first documentation/detail line; resolved metadata merges back into the list.
- **LSP workspace folders and progress.** `initialize` now advertises the
  workspace root as a `workspaceFolder` and the client capability for folder
  support; `workspace/workspaceFolders` returns it. Servers can create work-done
  tokens, and `$/progress` begin/report/end updates are merged by token and shown
  in the statusline until they finish.

## [3.2.1] - 2026-11-04

### Fixed
- The tool-provisioning test suite no longer expects a managed `netcoredbg`
  install plan on musl. That archive links glibc/libstdc++, so musl builds
  intentionally have no plan; the assertion is now skipped there instead of
  failing CI.

## [3.2.0] - 2026-11-04

### Added
- **Integrated terminal.** `Ctrl+Shift+T` opens a PTY-backed shell rooted at
  the project in a new **Terminal** tab of the bottom panel (the palette also
  offers it). It runs the user's `$SHELL`, supports several sessions
  (`Alt+1…9`, `Ctrl+Shift+T`, `Ctrl+Shift+W`), copy/paste (`Ctrl+Shift+C`/
  `Ctrl+Shift+V`), scrollback (`Shift+PageUp`/`PageDown`, `Ctrl+Home`/`End`),
  live resize and process-group shutdown on close. Keys are encoded as a real
  terminal sends them, so line editors, `vim`, `less`, prompts and `Ctrl+C`
  work. A small ANSI screen model handles cursor movement, erase, scrolling,
  alternate-screen applications and UTF-8 split across reads.
- **More built-in formatters.** Python now uses `ruff format` when installed and
  falls back to `black`; Lua uses `stylua`, Swift uses the toolchain's
  `swift-format`, and Java uses `google-java-format`. All four read the unsaved
  buffer from stdin, so formatting includes edits; a missing executable is
  reported with the exact install hint instead of a generic failure. Kemori does
  not provision these four yet.
- **Editor convenience commands.** The palette can now switch a file to **LF**
  or **CRLF** line endings (written on the next save; the rope is not rewritten,
  so undo stays valid), **Trim Trailing Whitespace** as one undoable edit, and
  **Ensure Final Newline**. Saves now apply the document's chosen line ending,
  including mixed-ending files that previously wrote their raw bytes.
- **Every source file is under the 800-line rule.** `editor/document.rs` and
  `language/tools.rs` became directory modules, and `git`, `project/create`,
  `app/keys`, `app/lsp`, `language/lsp`, `language/web`, `ui/overlay` and
  `app/tests` were split mechanically too. Some provisioning/UI parts use
  textual `include!` where that preserved the module's visibility graph without
  any behaviour change. The largest file is 798 lines.
- **A real Debug Adapter Protocol debugger.** **Start Debugging** launches a
  DAP adapter, completes the handshake, sends the breakpoints of every open
  file, starts the program and follows `stopped`/`continued`/`exited`/`terminated`
  events. Breakpoints are live and can be added or removed while the session
  runs. When the debuggee stops, Kemori requests the stack, scopes and variables
  and writes them into the Debug Console; `F5` continues, `F10` steps over,
  `F11` steps into, `Shift+F11` steps out, `Shift+F5` stops, and the palette
  opens **Stack Frames…** (jumping to the chosen source line) and **Variables…**
  pickers. Python uses `debugpy.adapter`; Rust/C/C++ use `gdb -i=dap` with the
  conventional debug binary; any other adapter can be named through
  `KEMORI_DEBUG_ADAPTER`/`KEMORI_DEBUG_ADAPTER_ARGS`. A missing adapter is
  reported as a prerequisite, not a fake success.
- **Built-in snippets with tab stops.** **Insert Snippet…** lists language-owned
  templates for the active file (Rust, Python, Go, JS/TS, C/C++, Java, C#, PHP,
  Ruby, Lua, Shell, Kotlin, Swift, Dart, SQL, Perl and Elixir), expands
  `$1`/`${1:default}`/`$0` placeholders as one undoable edit, and moves through
  the stops with `Tab`/`Shift+Tab`. Escape or explicit cursor navigation leaves
  snippet mode and restores normal indentation.
- **Find/replace can be scoped to a selection.** **Find in Selection** and
  **Replace in Selection** restrict matches to the active contiguous selection;
  the find bar shows `[selection]` and `Alt+S` releases the scope. Rectangular
  selections are refused with a precise message rather than silently searching
  their bounding box. Replacements update the scope's end so repeated replaces
  stay inside the original region.
- **Replace preview before touching the buffer.** `Alt+P` while the replace field
  is focused opens a read-only list of affected lines (`- old` / `+ new`) with
  `↑`/`↓` scrolling. `Enter` or `a` applies every replacement as one undo step;
  `Esc` cancels without editing. Preview respects the active selection scope and
  shows multi-line regex matches on their starting line.
- **Large-file mode.** Files with at least 1,000,000 characters open with syntax
  highlighting, diagnostics, completion, language-server document sync and git
  line markers disabled, so typing and scrolling stay responsive. Editing,
  saving, undo, search and replace still work, and the statusline shows a
  `large file` marker. Smaller files are unchanged.
- **User read-only mode.** **Toggle Read-Only Mode** (`Ctrl+Alt+R`, or the
  palette) marks the active document read-only: typing, pasting, cut, undo,
  format, search/replace, snippets, indentation and save are refused with a
  clear status message while navigation and copying still work. The statusline
  shows `read-only`; toggling the mode off restores editing. This is separate
  from the automatic read-only state of a non-UTF-8 file, which still needs
  **Convert to UTF-8**.
- **Document highlights from the language server.** Moving the cursor asks the
  ready server for `textDocument/documentHighlight` and paints the returned
  ranges with a subtle background that yields to selections and search matches.
  Stale responses are ignored, edits clear the old ranges immediately, and
  servers without the capability simply leave the editor unmarked.
- **Inlay hints from the language server.** Servers that advertise
  `inlayHintProvider` return type/parameter hints for the document; Kemori
  requests them after edits and once a server becomes ready, converts their
  positions with the negotiated encoding, and renders them as dim virtual text
  without changing the buffer. Requests are skipped while the same text version
  is already covered, and large files never request them.
- **Code lens from the language server.** Servers advertising `codeLensProvider`
  return lens titles and commands; titles render as muted virtual text at the end
  of their line, and **Code Lens…** (`Ctrl+Shift+.`) lists them for execution.
  Lenses whose server only supplies data for a later resolve are shown but
  report that resolving is not supported yet.
- **Selection ranges from the language server.** Servers advertising
  `selectionRangeProvider` provide a nested range chain for the caret.
  **Expand Selection** (`Shift+Alt+→`) grows to the next enclosing range and
  **Shrink Selection** (`Shift+Alt+←`) steps back inside it; the cached chain is
  replaced only when the caret moves. Large files skip the request.
- **Call and type hierarchy.** **Incoming Calls…**, **Outgoing Calls…**,
  **Supertypes…** and **Subtypes…** prepare the symbol at the cursor and show
  the server's hierarchy nodes in a jumpable picker, including the defining file
  and line. When preparation returns several symbols, a second picker chooses
  which one to expand.
- **Implementation and type-definition navigation.** **Go to Implementation** and
  **Go to Type Definition** ask the server's `textDocument/implementation` and
  `textDocument/typeDefinition` at the cursor, then reveal the first returned
  location. Missing capabilities and empty results produce precise messages.
- **Document links.** **Document Links…** asks the server for
  `textDocument/documentLink` and lists each target with its tooltip and line.
  File targets open in the editor at the linked position; other targets are
  copied to the clipboard with a status message. Links that require a resolve
  step are skipped. Large files are not special-cased here.
- **Rectangular (column) selection.** `Ctrl+Alt+Shift+←/→/↑/↓` starts or extends
  a column selection and plain `Shift` arrows continue it. Copy joins the
  per-row slices with newlines, typing replaces the same column span on every
  row, and the whole block is one undo step. Short rows contribute only the
  characters they have.
- **Persistent undo across sessions.** Clean files store up to 100 undo steps in
  the per-project session, keyed by an FNV-1a fingerprint of the buffer text.
  Reopening the project restores the stack only when the on-disk bytes still
  match; a file changed outside Kemori starts with a fresh history. Dirty files
  are skipped, so unsaved text is never woven into an on-disk undo stack.
- **C# formatting is wired.** C# uses CSharpier's `format --stdin-path` mode,
  preferring the installed tool and falling back to `dotnet csharpier`. Project
  `.csharpierrc`/ignore files are respected; a missing tool reports the dotnet
  global-tool install hint.
- **PHP formatting is wired.** PHP uses PHP-CS-Fixer's `fix --stdin` mode,
  applying a project `.php-cs-fixer.php` config when present and PSR-12
  otherwise. The provider advertises `Formatting`, and a missing fixer reports
  the Composer install hint.
- **SQL formatting is wired.** SQL uses SQLFluff's `sqlfluff format -` stdin
  mode. A project `.sqlfluff` file supplies its dialect; without one, ANSI is
  used so a standalone query still formats. Missing SQLFluff reports the pip
  install hint.
- **Kotlin formatting is wired.** Kotlin uses ktlint's `--stdin --format`
  mode, with the real file passed as `--stdin-path` so filename and
  `.editorconfig` rules apply. The provider advertises `Formatting`, and a
  missing ktlint reports an actionable install hint.
- **Ruby formatting is wired.** Ruby uses RuboCop's `--stdin` autocorrect mode,
  run from the nearest `.rubocop.yml`/`Gemfile` root so the project's own rules
  apply. The provider advertises `Formatting`, and a missing `rubocop` reports
  the exact `gem install rubocop` hint.
- **DAP attach.** **Debug: Attach to Debugger…** accepts a process id or
  `host:port`, builds an attach plan for the active language's provisioned
  adapter, and sends a DAP `attach` request instead of `launch`. Python, Go,
  JS/TS, C#, PHP and Dart are wired; Java and Ruby report a precise
  not-supported-yet message rather than pretending. Attach is covered by a mock
  DAP test.
- **PHP debugging uses the official Xdebug bridge.** `Tool::PhpDebug` downloads
  the verified `vscode-php-debug-1.40.2` VSIX, extracts its standalone DAP
  adapter, and provisions managed Node.js when needed. The plan launches the
  active PHP file with `xdebug.start_with_request=yes`, `XDEBUG_MODE=debug,develop`
  and a free client port. PHP with Xdebug remains the precise runtime
  prerequisite; a mock DAP test covers initialize → launch → stopped.
- **Format on save.** A new **Format on save** preference (off by default) saves
  the active file, runs the same language-server or built-in formatter as
  **Format Document**, then writes the formatted result again. The first save is
  never lost if formatting fails; a missing formatter is reported and the file
  stays saved. A formatter that is slow or unavailable does not block the editor.
- **Ruby debugging uses rdbg over TCP DAP.** `Tool::Rdbg` installs the pinned
  `debug` gem (`1.11.1`) into Kemori's isolated gem home (Ruby is a
  prerequisite), and the plan runs `rdbg --open=vscode --port <port> <file>`.
  Kemori connects DAP to that local port; a mock TCP DAP test covers
  initialize → launch → stopped.
- **Java debugging runs through JDT LS.** Kemori provisions the pinned
  `java-debug` JDT extension jar (`0.53.1`, SHA-256 verified), passes it to the
  running JDT LS via `initializationOptions.bundles`, resolves the main class
  and runtime classpath with `vscode.java.*` commands, calls
  `vscode.java.startDebugSession`, and connects DAP to the returned port. The
  DAP session owns no child process, so stopping the debugger never kills JDT
  LS. A mock TCP DAP test covers the Java launch path.
- **Dart debugging uses the provisioned Dart SDK.** `Tool::DartDebug` shares the
  existing managed Dart SDK plan, and the debug plan runs `dart debug_adapter`
  over stdio against the active file. A mock DAP test covers initialize → launch
  → stopped.
- **C# debugging is provisioned with netcoredbg.** `Tool::Netcoredbg` downloads
  the verified Samsung netcoredbg archive for the platform, and the debug plan
  runs `netcoredbg --interpreter=vscode` over stdio against the newest
  `bin/Debug/*/<project>.dll`. It explains when the project has not been built.
  A mock DAP test covers initialize → launch → stopped.
- **JS/TS debugging uses the provisioned js-debug DAP bundle.** Kemori downloads
  the official `js-debug-dap-v1.140.0.tar.gz` release (GitHub-published SHA-256),
  provisions its managed Node.js runtime when needed, and writes a
  `js-debug-dap` launcher. The plan asks the adapter for a known local port and
  speaks DAP over TCP; JavaScript and TypeScript files are launched from the
  active file. A mock TCP DAP test covers the initialize → launch → stopped
  path.
- **Go debugging is provisioned with Delve.** `Tool::Dlv` uses the existing
  managed-Go plan to install a pinned
  `github.com/go-delve/delve/cmd/dlv@v1.27.2`, and `dlv dap` is spoken over the
  TCP socket it announces instead of stdio. The plan infers the package
  directory from the active Go file and sends `mode: "debug"`. A mock TCP DAP
  server test drives the full initialize → launch → stopped path.
- **Native debugging prefers LLVM's `lldb-dap`.** Rust, C and C++ plans now
  discover `lldb-dap` (and the older `lldb-vscode` name) before falling back to
  `gdb -i=dap`. Discovery searches the usual PATH, Homebrew/LLVM prefixes,
  versioned `/usr/lib/llvm-*/bin` directories and the Xcode LLDB resources.
  `lldb-dap` is discovery-only for now (its official bundle is a large LLVM
  archive); when neither adapter is present the command names both
  prerequisites. A mock DAP test drives the lldb plan to a stopped event.
- **Python's debug adapter is now provisioned.** `debugpy` is a pinned
  Kemori-managed tool (`debugpy==1.8.22`) with the same isolated
  pipx/uv/virtualenv/`--user` strategies as the Python language server. Debugging
  a Python file prefers the installed `debugpy-adapter` console script and falls
  back to `python3 -m debugpy.adapter` only when it is absent. The tool appears
  as **debug adapter** in Language Setup. A test drives the discovered plan
  through the mock DAP adapter to a stopped event, stack, variables and a watch
  evaluation.
- **Watch expressions, conditions and logpoints.** **Debug: Add Watch…**
  evaluates an expression at every stop; **Debug: Watch Expressions…** lists the
  last values. **Debug: Set Breakpoint Condition…** runs a breakpoint only when
  its expression is true, and **Debug: Set Logpoint…** logs a message without
  stopping the debuggee. Conditional and log breakpoints use a diamond gutter
  marker instead of the solid dot, and travel to the adapter in the
  `setBreakpoints` request.
- **Workspace trust.** Running project tasks, starting the debugger or opening
  a shell now asks once per project root the first time and remembers the answer
  in Kemori's state directory. Editing, search, git and language intelligence
  never require trust; the decision is per-root and stored outside the project.
  `Workspace: Trust This Project` and `Workspace: Revoke Workspace Trust` are in
  the palette.
- **Language-server pull diagnostics.** When a server advertises
  `textDocument/diagnostic`, Kemori requests diagnostics for open documents on
  connect and after changes instead of relying only on pushed reports. When it
  also advertises `workspaceDiagnostics`, refreshing **Problems** requests a
  whole-workspace report; unopened files are converted from the server's
  position encoding and added to the panel, while open documents use the live
  path. The client advertises the pull-diagnostics and workspace-diagnostics
  capabilities during `initialize`.

### Changed
- The bottom panel has four tabs: **Problems**, **Output**, **Terminal** and
  **Debug Console**. `1`–`4` select a tab outside the terminal; while the
  terminal owns the keyboard, `Ctrl+Shift+PageUp`/`PageDown` switch panel tabs
  and `Ctrl+J` returns to the editor.
- The built-in **Problems** scan and language-server workspace diagnostics are
  merged and deduplicated, so a file may be reported by both without a double
  row.

### Dependencies
- Added `libc` as a direct dependency for `openpty`, `TIOCSCTTY` and
  `TIOCSWINSZ`. It was already present transitively, and the integrated
  terminal needs the raw Unix PTY/ioctl layer rather than a process-management
  framework; the dependency decision is recorded in `docs/DECISIONS.md`.

## [3.1.0] - 2026-11-02

### Added
- **Project-wide built-in diagnostics.** Opening **Problems** scans every
  unopened file in the project with its provider's structural diagnostics, up
  to 4,000 files and 2,000 problems, and groups the result with the live
  document and task problems. `r` refreshes the scan; `c` clears task and
  project problems while live document diagnostics stay.
- **Run Active File.** A standalone script in Python, Shell, JavaScript, Ruby,
  PHP, Lua, Perl, Go, Dart or Elixir can be run from the palette with its
  interpreter/toolchain (`python3 file.py`, `go run file.go`, …). It uses the
  same streaming Output tab, stays unavailable for a language without a
  single-file runner, and refuses an unsaved buffer instead of running stale
  code.
- **Project-wide search and replace.** `Ctrl+Shift+F` opens a search form with
  literal or regular-expression matching, case and whole-word toggles,
  include/exclude globs, results grouped by file, and a per-line replacement
  preview. `Alt+Enter` replaces every match atomically, skips open files with
  unsaved edits, names the changed files and reloads clean open buffers
  immediately. An empty replacement asks for a second confirmation before
  deleting matches.
- **Bottom panel.** `Ctrl+J` opens a panel under the editor with three tabs:
  **Problems** (diagnostics across the open files, grouped by file; `Enter`
  jumps to one), **Output** (the last task's captured output; `Enter` jumps to a
  `file:line` reference) and **Debug Console** (messages from the adapter
  preview). `Tab` cycles tabs, `Alt+↑`/`Alt+↓` resize, `c` clears Output or the
  console, and `Esc` returns to the editor. Each tab keeps its own state.

### Changed
- **Task output streams into the bottom panel.** Running a task opens the
  **Output** tab immediately and appends stdout/stderr as it arrives, following
  the tail until the user scrolls up. `x` in the tab cancels the running task,
  `r` reruns the last one, and `h` reopens a completed run from this session's
  history. The final result still separates stdout from stderr. Running a task
  no longer opens a modal view.
- **Task output feeds the Problems tab.** `file:line[:col]` references in the
  last task's output become Problems with a severity inferred from the line;
  `Enter` opens the file and `c` clears the task problems while document
  diagnostics stay visible.
- **The debug-adapter preview writes to the Debug Console.** Its messages are
  kept in the panel instead of only flashing in the statusline.

### Fixed
- **A very short editor viewport keeps the cursor visible.** When a panel leaves
  one or two editor rows, scrolling no longer moves to the end of the file while
  the statusline still reports the cursor's line.

## [3.0.0] - 2026-11-02

### Changed
- **Renamed from Koda to Kemori.** The crate, library and binary are all
  `kemori`; the repository and documentation move to `etokiyra/kemori`. The
  rename removes the name collision with unrelated projects called Koda.
- **State migrates once.** The first Kemori launch copies settings, sessions and
  recents from the old state directory without modifying or deleting it, then
  writes a marker so the import is not repeated.
- **Old environment variables still work for one release.**
  `KEMORI_REMOTE` and `KEMORI_DEBUG_ADAPTER` are preferred; `KODA_REMOTE` and
  `KODA_DEBUG_ADAPTER` fall back for compatibility.
- **The UI, documentation and packaging use the new name.** The welcome
  wordmark, README, AUR/deb/rpm package names, release artifacts and install
  instructions are all Kemori.

### Added
- **Fold-aware editing.** Cursor movement, selection and jumps stay on visible
  rows; edits above a fold shift its header and an edit that swallows a header
  drops it. `Ctrl+K z`/`c`/`o`/`0`/`j` toggle, close, open, fold all and unfold
  all, and a language server's `textDocument/foldingRange` is preferred over
  indentation when it advertises folding.

### Changed
- **Git work runs on the worker.** Diff, hunk apply/revert, push, pull, fetch,
  GitHub repository creation and per-document gutter markers no longer spawn
  `git` on the UI thread; the statusline reports progress and results.
- **Debugging is labelled honestly.** **Start Debugging** is now **Debug Adapter
  Handshake (Preview)**, and its description and status message state that it
  performs a DAP handshake only — execution does not stop on breakpoints yet.
  **Toggle Breakpoint** is described as a visual marker. `docs/LIMITATIONS.md`
  states the real state.
- **The remote runner is no longer presented as remote development.**
  `KEMORI_REMOTE` remains an internal, experimental seam for git status only;
  the file tree, editor, language servers and tasks stay local. The limitation
  is documented instead of advertised.

## [2.1.0] - 2026-10-31

### Added
- **More run-task project kinds.** `Run Task…` now also offers tasks for Maven
  (`pom.xml`), Gradle (`build.gradle`/`build.gradle.kts`), Flutter
  (`pubspec.yaml`) and .NET (`*.csproj`) projects, in addition to Rust, Go,
  Make, Python, Elixir and npm.
- **Git gutter markers.** The editor gutter now shows per-line change markers
  (`+` added, `~` modified, `-` removed) computed from the working tree against
  `HEAD`, refreshing with git status. Untracked files are marked entirely
  added.
- **Next/Previous Hunk.** Two new Git commands (`Next Hunk`, `Previous Hunk`)
  jump the cursor between changed hunks in the active file.
- **Non-UTF-8 files open read-only.** A file that is not valid UTF-8 (but is not
  binary) now opens as read-only lossy text instead of being refused; saving it
  is blocked so the original bytes are never corrupted. Binary and oversized
  files are still refused.
- **Provisioning cache eviction.** The verified download cache now evicts its
  oldest entries once it grows past a soft limit, and a failed download with no
  cached artifact reports that no cache was available.
- **Convert to UTF-8.** A read-only non-UTF-8 file can be converted to UTF-8
  from the command palette so it can then be saved.
- **Language Support view.** A new `Language Support…` command lists every
  language Koda knows with its support class.
- **Set Language.** A new `Set Language…` command overrides the detected
  language for the active file; low-confidence detections nudge toward it.
- **Multi-project workspaces.** Opening a second project now keeps the
  previous one available; `Switch Workspace…` swaps between them, restoring
  each project's open tabs. The LSP server map is now keyed by workspace root,
  so projects never reuse another project's language server.
- **Git push/pull/fetch.** New `Push`, `Pull` and `Fetch` commands.
- **GitHub CLI integration.** `Create GitHub Repository…` uses `gh` to create
  a repo and push, delegating authentication to `gh auth`.
- **File iconography.** The file tree shows a per-extension glyph for files,
  falling back to a neutral dot on monochrome terminals.
- **LSP incremental sync.** `didChange` now sends the smallest changed range
  instead of the whole document when the server supports incremental sync.
- **Mouse support (partial).** Mouse capture is enabled, the wheel scrolls the
  editor, a left click places the cursor, and a drag selects text.
- **Interactive diff hunks.** The diff overlay now supports applying (`a`) or
  reverting (`r`) the current hunk and moving between hunks (`n`/`p`).
- **Breakpoint toggle.** A new `Toggle Breakpoint` command marks lines in the
  editor gutter, a first step toward debug adapter support.
- **Debug adapter scaffold (preview).** A minimal DAP client launches an
  adapter (via `KODA_DEBUG_ADAPTER`), runs initialize/launch/setBreakpoints,
  and reports the result. It does not debug execution; breakpoints are visual
  markers only.
- **Plugin/provider extension point.** `ProviderRegistry::register` (and
  `register_provider`) are documented as the public extension point for new
  language providers.
- **Experimental command runner.** A `CommandRunner` trait backs git status,
  with a local implementation and an SSH-backed one selected via
  `KODA_REMOTE`. Only git status is routed; this is an internal seam, not
  remote development.
- **Code folding.** `Toggle Fold` collapses and expands an indented block at
  the cursor; folded lines are hidden and the gutter shows a fold marker.

## [0.1.3] - 2026-10-29

### Added

- **Terminal colour compatibility.** Koda now detects colour support once at
  startup (`NO_COLOR`, `COLORTERM`, `TERM`) and routes every rendered colour
  through the theme. A 16-colour terminal gets the new bundled **Ansi** theme
  — palette indices only — unless the user already chose a theme; a `NO_COLOR`
  environment suppresses every colour code, keeping emphasis through bold text
  and rendering selections as reversed text so menus, the file tree and the
  editor stay usable.
- **Run Task…** Detects a project's own commands from its markers — `cargo
  build`/`test`, `go build`/`test`, `make`, `python3 -m pytest`, `mix test`
  and every declared `npm` script — and runs the chosen one in the project
  root on the background worker, with a deadline, a bounded output cap and no
  shell. The captured output opens in a panel with the exit verdict; the row
  cursor moves with the arrows and `Enter` jumps to the `file:line[:column]`
  reference under it.

### Fixed

- **The command palette now advertises server-backed commands correctly.**
  `Format Document`, `Rename Symbol` and `Code Actions` are LSP-backed, but the
  palette gated them on each provider's *built-in* capability list, so rename
  and code actions were advertised for Rust and Go alone even though the
  servers for Python, Java, C#, Kotlin, Lua, PHP, Ruby, Swift and others
  provide them too. An attached server that advertises the request now makes
  the command available and the palette shows it.

## [0.1.2] - 2026-10-29

### Added

- **System-wide installation.** A `Makefile` installs to `/usr/local` by default
  and honours `PREFIX`/`DESTDIR`; AUR packages (`koda` and `koda-git`) install
  `/usr/bin/koda`; the release workflow now builds and attaches a `.deb` and an
  `.rpm` (and checksums every artifact). Distro packages make plain `sudo koda`
  work through `secure_path` without an absolute path.
- **Packaging metadata.** `Cargo.toml` carries `[package.metadata.deb]` and
  `[package.metadata.generate-rpm]`; Debian packages depend on `libc6` and
  `libgcc-s1`, and RPM dependencies are generated from the binary.
- **Packaging documentation.** The README installation section documents the
  AUR, `.deb`, `.rpm`, `make install` and `cargo install` paths, including the
  `secure_path` caveat and a warning that running the whole IDE as root is
  discouraged.

### Changed

- **`koda-git` tracks `master` without a pinned version.** Its `pkgver()` uses
  `git describe`; the stable AUR package remains pinned to the release tag.

### Security

- **Root use is documented as exceptional.** The system-wide packages make
  `sudo koda` convenient, but the README now states that it should be used only
  for files that need elevated privileges.

## [0.1.1] - 2026-10-28

### Added

- **crates.io packaging.** Koda is published as the **`koda-ide`** crate because
  `koda` was already taken on crates.io; the installed command is still `koda`,
  so `cargo install koda-ide` is enough. The manifest now carries the
  repository, homepage, docs.rs, keyword and category metadata crates.io needs.
- **Nix language support.** `.nix` files are detected, highlighted (including
  nested block comments, multi-line quoted and `''…''` strings, search paths,
  paths and URIs), diagnosed for unbalanced brackets, outlined by attribute
  binding, completed, hovered and navigated offline. `nil` is the language server
  and `nixfmt` the formatter; Koda installs both from a pinned nixpkgs commit
  with `nix profile` into its own profile. `flake.nix` establishes a Nix
  project, Nix profile binaries are found even when launched from a GUI or
  `sudo`, and **Create a new project** can scaffold a flake.
- **Running under `sudo`.** Koda keeps its state under the root account rather
  than writing root-owned files into the invoking user's home, and an atomic save
  preserves the original file's owner as well as its mode, so editing a user file
  as root does not lock that user out afterwards. The system-wide NixOS profile
  is searched for language tools under `sudo`.
- **In-app Settings.** A keyboard-driven **Settings** screen (opened from the
  command palette) changes a small set of global preferences: theme, line
  numbers, animations, soft wrap, indentation width, spaces vs tabs, inline
  diagnostics and automatic completion. Each change applies immediately, can be
  reset to the defaults, and is remembered across launches. Koda is fully usable
  with no settings file; the choices live in
  `$XDG_STATE_HOME/koda/settings.json` (or `~/.local/state/koda/settings.json`).
  A missing file loads the defaults, and a malformed file is preserved as
  `settings.json.corrupt` rather than overwritten.
- **Three bundled themes.** **Mellow** (the default) is joined by **Midnight**,
  a high-contrast cool dark theme, and **Daylight**, a light theme. All three
  are data-driven from one semantic role set, selectable at runtime, and cover
  every existing surface (editor, statusline, tabs, file tree, palette,
  completion popup, diagnostics, overlays and diffs).
- **One-step project setup.** Opening a project runs a bounded,
  `.gitignore`-aware scan for the languages it contains, then offers a single
  **Project setup** summary. **Set up project** installs every missing managed
  language tool through the existing provisioning path; a language that is
  already ready is never reinstalled, an independent language still succeeds if
  another fails, and a missing system prerequisite is reported rather than
  attempted. **Later** (or `Esc`) dismisses it and leaves built-in editing
  working; **Set Up Project…** in the palette reopens it on demand.
- **Offline, verified provisioning cache.** A successfully verified download is
  stored by its digest under Koda's data directory and reused without
  re-downloading it, and the checksum/release metadata that names it is cached
  by URL as well, so a tool Koda has installed once can be reinstalled or
  repaired with no network. A cached artifact is re-verified before every use, a
  corrupt entry is discarded and redownloaded, and only verified artifacts are
  cached.

### Changed

- **View toggles now persist.** **Toggle Soft Wrap**, **Toggle Inline
  Diagnostics** and **Toggle Animations** update the corresponding preference,
  so the palette and the Settings screen stay in step across launches.
- **Provisioning is atomic and pre-flighted.** Downloads are written to a
  temporary file and renamed into place only after verification; extraction
  stages into a sibling directory and promotes it with a rename, restoring the
  previous installation if promotion fails, so a failed replacement no longer
  destroys a known-good tool. `install()` now refuses a plan whose steps cannot
  all run (or whose disk space is short) before it downloads anything.
- **Provisioning failures are actionable**, distinguishing no network, a proxy
  or TLS failure, a timeout and a checksum mismatch, and naming the tool and
  command that failed.
- **Hex is pinned to an exact version**, and **rebar3 is a Koda-verified
  download** registered through `mix local.rebar` instead of being fetched
  unpinned by Mix. `rustup component add` remains the one documented delegation.
- **Provisioning pre-flight and rollback boundaries** are documented in
  [docs/LIMITATIONS.md](docs/LIMITATIONS.md#provisioning).
- **The language-support contract is now explicit.** A new
  [docs/LANGUAGE-SUPPORT.md](docs/LANGUAGE-SUPPORT.md) defines what "supported"
  means (Complete / Complete-with-a-prerequisite / Offline only /
  Platform-limited), lists each language's offline capabilities, server and
  verification, runtime prerequisite and platform limits, and records the
  known capability-gating inconsistency. A test keeps it in sync with the
  provider registry.

### Fixed

- **Nested Rust block comments highlight correctly.** Because Rust block comments
  nest, `/* a /* b */ c */` now stays a single comment instead of closing at the
  inner `*/`, and the open nesting level is carried across lines.
- **Rust byte strings, byte literals and lifetimes are highlighted.** `b"…"`,
  `b'…'` and `br#"…"#` are one string span (prefix included), and `'a`/`'static`
  read as a lifetime rather than a stray operator followed by plain text.
- **Rust and Go numeric literals scan as one token.** Exponents (`1e-5`), hex
  floats (`0x1p-2`), suffixes (`0xFFu8`, `2.5i`) and digit separators (`1_000`)
  stay with the number, while the range `1..=2` is no longer read as a single
  number.

### Security

- **The download cache never bypasses verification.** Cache identity is the
  expected digest, a cached artifact is re-hashed before use, and a failed or
  mismatched download is never written to the cache.

## [0.1.0] - 2026-10-05

The first release: a zero-configuration terminal IDE that opens a project and
starts working.

### Added

- **Editor.** Rope-backed buffers with multi-document tabs, cursor and
  selection, operation-based undo/redo with a save point, auto-indent and
  bracket auto-pairing, matching-bracket highlighting, grouped multi-cursor
  editing (`Ctrl+D`/`Ctrl+Shift+L`, stacked carets), a kill-ring with `Alt+Y`
  yank-pop, selection-aware indent/outdent, line move/duplicate/delete, and a
  split editor (`Alt+V`, `Alt+O`).
- **Soft wrap.** Display-only wrapping (`Alt+Z`) with visual-row Up/Down
  movement and no effect on positions, undo, diagnostics or cursors.
- **Files and projects.** A welcome home screen with recent projects and files
  and session resume; a lazy, `.gitignore`-aware file tree with an inline fuzzy
  filter; quick open; file create/rename/delete/duplicate/copy; save-all,
  revert, and external-change detection; deterministic, offline project
  scaffolding; and per-project session persistence (open tabs, cursors,
  expanded folders).
- **Search.** Find/replace with case, whole-word and regex options; project-wide
  text search; document, workspace and project-wide symbol navigation.
- **Language intelligence.** Confidence-based, file-first language detection and
  a provider registry. Offline built-in highlighting, structural diagnostics,
  symbols, completion, hover and navigation for **Rust, Go, Python, Shell,
  TypeScript, JavaScript, C, C++, Java, C#, PHP, Kotlin, Lua, SQL, Ruby, Perl,
  Assembly, Dart, Elixir, Swift, HTML, CSS, Markdown, JSON, TOML and YAML**.
  Python, Shell, TypeScript/JavaScript, C/C++, Java, C#, PHP, Kotlin, Lua, Ruby,
  SQL, Perl, Dart, Elixir, Swift, HTML and CSS gain richer features when a
  language server is attached.
- **Language servers.** A JSON-RPC client with lifecycle management: diagnostics,
  completion (automatic and manual), hover, signature help, go-to-definition,
  find-references, document and workspace symbols, rename, code actions and
  formatting, with the built-in providers as offline fallbacks; one server per
  language with an independent restart budget.
- **Provisioning.** **Language Setup…** discovers and installs missing tools.
  Managed, user-local installs for the official Go toolchain (which provides
  `gopls`, `sqls` and `shfmt`), the self-contained clangd bundle, `phpactor.phar`,
  the Dart SDK, a managed Node.js runtime (for the npm-based servers), an
  Eclipse Adoptium JDK (jdtls), a dedicated JDK 21 (Kotlin), a coordinated
  Erlang/OTP + Elixir + ElixirLS stack, the Swift toolchain, and `cpanm` into an
  isolated `local::lib` for Perl's PLS. Every managed download is pinned to an
  exact version and verified against a checksum or signature and fails closed;
  extraction refuses path traversal; large installs check free disk space and
  honour `HTTP(S)_PROXY`; installs are bounded, serialised with an advisory lock,
  and re-probed by launching the real server. **Language Setup** shows a managed
  tool's version and on-disk size and can update or remove it without touching a
  user/system install.
- **Git.** Branch and per-file status, a changed-files list with per-file
  staging, a commit flow, and a read-only unified diff panel.
- **Interface.** The Mellow colourscheme, five animated welcome scenes, the Koda
  familiar, personable empty states, toasts for background work, a
  keyboard-shortcuts cheatsheet generated from the command registry, and the
  command palette.
- **Terminal.** OSC 52 clipboard, bracketed paste and focus reporting, and a
  transparent, terminal-native layout.
- **Distribution.** A tagged release publishes prebuilt archives for Linux
  x86_64 (glibc and musl) and macOS (x86_64 and arm64), each with a SHA-256
  checksum, from the GitHub Releases page. See the
  [install instructions](README.md#-install).

### Changed

- **Relicensed under the Mozilla Public License 2.0** (previously declared
  GPLv3; originally MIT). MPL-2.0 is file-level copyleft.
- Go tooling (`gopls`, `sqls`, `shfmt`, `gofmt`) now comes from Koda's managed
  official Go toolchain, and C/C++, PHP and Assembly no longer need a system
  toolchain for their servers — reducing system prerequisites across the board.
- Language setup is now offered when a project is opened, not only when a file
  is open.
- **Every managed install input is an exact version.** Eclipse JDT moved from the
  floating `-latest` snapshot to a pinned milestone with its published `.sha256`;
  `lua-language-server` and OmniSharp use a pinned tag with a GitHub digest;
  `kotlin-language-server` uses a pinned SHA-256; the .NET SDK is downloaded
  directly and verified by SHA-512 instead of running `dotnet-install.sh`; the
  Rust bootstrap downloads and verifies a pinned `rustup-init` binary instead of
  the moving `sh.rustup.rs` script; the Go toolchain and the Adoptium JDKs are
  pinned by version; and every package-manager install names an exact version.
- **Internal:** `src/app/mod.rs` (8876 lines) was split into focused modules
  under `src/app/` (mechanical extraction, no behaviour change), so every
  production app file is under 800 lines. See
  [DECISIONS.md](docs/DECISIONS.md#splitting-srcappmodrs).

### Fixed

- **LSP positions are converted correctly on non-ASCII lines** in both
  directions, using the server's negotiated encoding (UTF-8/UTF-16/UTF-32).
- **Superseded language-server responses are discarded**; a response is applied
  only when the document is still open at the same version it was requested for.
- **Tag-balance diagnostics ignore raw bodies**, so `<`/`>` inside a
  `<script>`/`<style>` block no longer produce false "never closed" warnings.
- **Checksum and extraction are portable**: hashing falls back through
  `sha256sum`/`sha512sum`, `shasum`, `openssl` and Python, and zip extraction
  falls back through `unzip`, `bsdtar` and Python.
- **Saves are atomic and lossless**, preserving permissions and refusing to
  overwrite an existing destination on rename/copy.

### Security

- **Every managed download is verified and fails closed.** A `Download` step
  cannot be constructed without a SHA-256; GitHub releases, `builds.hex.pm`,
  Adoptium, Node.js, the Dart SDK, the Go toolchain, Microsoft's .NET
  `releases.json`, `rustup-init`, the Swift signature and the pinned `cpanm`
  each verify a checksum, digest or signature. A checksum mismatch or a
  truncated download is rejected and removed before it can become an installed
  tool. Package-manager installs name an exact version and delegate integrity to
  the manager.
- Managed installs run from Koda's own data directory, so a project-local
  `.npmrc` cannot hijack `npm`; downloads and commands are time- and
  output-bounded.
- Downloaded archives are listed before extraction and refused if any entry is
  absolute or contains `..`, failing closed if the listing itself fails.
- Koda never writes to a system-owned location or modifies the user's system
  environment.

> **Known gaps at release.** Two package-manager inputs delegate to their
> manager's trust model: `rustup component add` follows the user's toolchain
> channel, and `mix local.hex`/`local.rebar` fetch Hex's signed archive. There is
> no separate estimate of Swift's unpacked size, and removing a shared managed
> component (Go tools, npm servers, the JDKs) can also remove a sibling Koda
> tool. See [`docs/LIMITATIONS.md`](docs/LIMITATIONS.md#provisioning). Windows is
> not supported.

[3.2.1]: https://github.com/etokiyra/kemori/releases/tag/v3.2.1
[3.2.0]: https://github.com/etokiyra/kemori/releases/tag/v3.2.0
[3.1.0]: https://github.com/etokiyra/kemori/releases/tag/v3.1.0
[3.0.0]: https://github.com/etokiyra/kemori/releases/tag/v3.0.0
[2.1.0]: https://github.com/etokiyra/koda/releases/tag/v2.1.0
[0.1.2]: https://github.com/etokiyra/koda/releases/tag/v0.1.2
[0.1.1]: https://github.com/etokiyra/koda/releases/tag/v0.1.1
[0.1.0]: https://github.com/etokiyra/koda/releases/tag/v0.1.0
