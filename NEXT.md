# NEXT — the working plan for turning Kemori into a full IDE

**Status snapshot:** 2026-11-07, version 3.2.1 plus unreleased work.
**Verification at the time of writing:** `cargo check --all-targets`, `cargo clippy --all-targets` clean; `cargo test` green (756 library tests, 52 render tests, plus tooling suites with ignored live tests).
**Release status:** `kemori` 3.2.1 is on crates.io, docs.rs and the GitHub `v3.2.1` release. AUR publication remains blocked because AUR account registration is disabled; the packaging sources are ready to refresh when registration reopens.

### What this session completed

- **The integrated terminal is in the bottom panel.** `Ctrl+Shift+T` starts the
  user's `$SHELL` under a real Unix PTY in the project root. The panel has a
  fourth **Terminal** tab: multiple sessions, `Alt+1…9` switching, live resize,
  scrollback (`Shift+PageUp`/`PageDown`, `Ctrl+Home`/`End`), copy/paste
  (`Ctrl+Shift+C`/`Ctrl+Shift+V`) and process-group shutdown. A small ANSI screen
  model handles UTF-8 split across reads, cursor movement, erase, scroll regions,
  alternate-screen programs and OSC titles; colour attributes are deliberately
  ignored so terminal text follows the theme and transparency holds. Keys are
  encoded as a terminal sends them, including modifiers and function keys.
- **Workspace trust gates execution, once per root.** Tasks, the debug-adapter
  preview and the terminal ask before running project code; editing, search,
  git and language intelligence never do. The choice is stored in
  `$XDG_STATE_HOME/kemori/trust.json`, outside the repository, with palette
  commands to trust or revoke.
- **The debugger is real now.** The old handshake preview is replaced by a DAP
  client: `initialize`/`initialized`, breakpoints, `launch`,
  `configurationDone`, stop/continue/exit events, stack frames, scopes and
  variables, stepping (`F5`, `F10`, `F11`, `Shift+F11`) and stop (`Shift+F5`).
  Stack and variable pickers jump to source. Python (`debugpy`) and Rust/C/C++
  (`gdb -i=dap`) plans are inferred; other adapters can be named through
  `KEMORI_DEBUG_ADAPTER`. A shell mock adapter exercises the full stop → stack →
  scopes → variables path in the normal test suite. Adapter provisioning,
  watch/conditional breakpoints and a dedicated variable tree remain.
- **Editor convenience commands.** Palette actions now convert a file to LF or
  CRLF line endings (written on the next save without rewriting the rope, so undo
  stays valid), trim trailing spaces and tabs as one undoable edit, and add a
  missing final newline.
- **Document highlights track the cursor.** When a ready server advertises
  `documentHighlightProvider`, cursor movement and edits send
  `textDocument/documentHighlight`; the response is converted with the
  negotiated position encoding and painted as a subtle background. Older
  responses cannot replace a newer cursor position, edits clear stale ranges,
  and a server without the capability leaves the editor unchanged.
- **Inlay hints are versioned virtual text.** A server advertising
  `inlayHintProvider` is asked for hints over the whole document after an edit
  or handshake. Responses record the buffer version, so arrow keys do not
  re-request unchanged text. Hints render as dim italic text at their character
  position and never enter the buffer or undo history; large files skip them.
- **Code lenses are rendered and runnable.** A server advertising
  `codeLensProvider` supplies lens titles and commands. Titles appear at the end
  of their line, and `Ctrl+Shift+.` opens a picker that runs the selected lens's
  `workspace/executeCommand`. Data-only lenses are listed but reported as
  needing a resolve step Kemori does not implement yet.
- **Selection ranges expand and shrink.** The server's nested ranges are cached
  for the caret; `Shift+Alt+→` grows to the next enclosing range and
  `Shift+Alt+←` steps back. The cache is skipped while a selection exists, so
  repeated expansion uses the chain from the caret where it started.
- **Call and type hierarchies are picker-based.** Four commands prepare the
  symbol at the cursor: Incoming Calls, Outgoing Calls, Supertypes and Subtypes.
  If preparation yields one node the follow-up request is immediate; several
  nodes open a chooser first. Results show the node, its detail and defining
  file/line and jump on selection.
- **Implementation and type definition reuse the definition pipeline.** Both ask
  the server at the cursor and reveal the first location, with precise messages
  for a missing capability or an empty result; they share the same stale-response
  validation as Go to Definition.
- **Document links become a jumpable picker.** The server's links are listed
  with tooltip and line; file targets open at the linked position and other
  targets are copied to the clipboard. Links without a resolved target are
  skipped rather than shown as dead entries.
- **Organize imports and format selection use server requests.** Organize
  Imports sends a code-action request restricted to `source.organizeImports`
  and applies the returned edit or command; Format Selection sends
  `textDocument/rangeFormatting` for the active range and validates the buffer
  version before applying edits.
- **Saves and watched files are reported.** A server that requests `didSave`
  receives it after writes, with text only when `includeText` is requested.
  `client/registerCapability` watchers are stored, matched against workspace
  globs, and fed create/change/delete notifications from saves, file operations
  and external disk changes; unregister requests remove them.
- **Superseded and timed-out requests are cancelled.** The server receives
  `$/cancelRequest` for an older request when a newer one of the same feature
  replaces it, and for requests past their deadline. Diagnostics and commands
  are exempt because several can run at once.
- **Workspace folders and progress are negotiated.** Initialize names the
  project root as a workspace folder; `workspace/workspaceFolders` returns it.
  The client advertises work-done progress and receives server-created tokens;
  begin/report/end updates merge by token and appear in the statusline until the
  token ends.
- **Completion documentation is kept and resolved.** Server items retain their
  detail and documentation, and servers advertising `resolveProvider` are asked
  to resolve the selected item; the popup shows its first documentation line
  while resolved metadata merges back into the list.
- **Search in selection is scoped, not pre-filled.** **Find in Selection** and
  **Replace in Selection** store the contiguous selection as a range and filter
  every match against it; `Alt+S` clears the scope and returns to the whole
  file. Replacement deltas shift the scope end so the next pass stays in range.
  A rectangular selection is refused because its rows are not contiguous text.
- **Replace preview is read-only until confirmed.** `Alt+P` in the replace field
  builds `- old` / `+ new` rows from the current matches without touching the
  buffer; `Enter` or `a` then runs the same replace-all path, so the result is
  one undo step. Grouping matches by line keeps repeated matches on one line
  visible together.
- **Large-file mode keys off character count.** At one million characters the
  document reports `large_file`; syntax highlighting, diagnostics, completion,
  language-server `didOpen`/requests and git line markers are skipped, and a
  server is not started when the only open document of that language is large.
  Editing, saving, undo and search are unchanged, and the statusline says
  `large file` so the reduced feature set is never a silent surprise.
- **Read-only mode is explicit and per-document.** `Ctrl+Alt+R` / **Toggle
  Read-Only Mode** flips a user flag that is separate from the automatic
  non-UTF-8 read-only state. The editor core refuses mutations at its edit
  entry points, and common editing commands report why they did nothing.
  Navigation, selection and copying remain available; the statusline says
  `read-only`.
- **Rectangular selection is keyboard-first in this release.** The column model
  stores an anchor column plus the cursor, so movement, copy and edits share the
  normal document paths instead of a parallel selection type. Per-row edits are
  planned as multi-cursor edits, which makes the whole block one undo step.
  Mouse Alt-drag is not wired yet.
- **Built-in snippets are in.** A language-owned snippet table expands
  `$1`/`${1:default}`/`$0` as one undoable edit; Tab/Shift+Tab move through the
  stops and Escape or navigation leaves snippet mode.
- **Persistent undo survives sessions.** Clean files persist up to 100 undo
  steps in the session store with an FNV-1a content fingerprint; a mismatch or a
  dirty buffer refuses the history instead of corrupting it.
- **C# formatting uses CSharpier.** The provider advertises `Formatting` and
  runs `csharpier format --stdin-path <file>`, falling back to `dotnet csharpier`.
- **PHP formatting uses PHP-CS-Fixer.** The provider advertises `Formatting` and
  pipes the buffer through `fix --stdin`, using a project config or PSR-12.
- **SQL formatting uses SQLFluff.** The provider advertises `Formatting` and
  pipes the buffer through `sqlfluff format -`, using a project `.sqlfluff`
  dialect or ANSI as the standalone default.
- **Kotlin formatting uses ktlint.** The provider advertises `Formatting` and
  runs `ktlint --stdin --format --stdin-path <file>` for editorconfig-aware
  formatting.
- **Ruby formatting uses RuboCop.** The provider advertises `Formatting` and
  pipes the buffer through `rubocop --stdin --autocorrect-all`, resolving the
  project's `.rubocop.yml`/`Gemfile` root.
- **DAP attach.** **Debug: Attach to Debugger…** takes a process id or
  `host:port`, builds an attach plan for the active language's provisioned
  adapter, and sends DAP `attach`; Python, Go, JS/TS, C#, PHP and Dart are wired.
- **PHP debugging uses the official Xdebug bridge.** The verified
  `vscode-php-debug` VSIX is extracted, run through managed Node.js, and launches
  the active file with Xdebug enabled. PHP with Xdebug is the precise prerequisite.
- **Format on save is a real preference.** Settings can save, format with the
  same LSP/built-in formatter, and write the result back; the first save is
  preserved if formatting fails. Tests cover the full save→format→save cycle and
  the default-off case.
- **Ruby debugging uses rdbg.** The pinned `debug` gem is installed into the
  isolated gem home and the plan runs `rdbg --open=vscode --port <port> <file>`
  with DAP over that TCP port.
- **Java debugging runs through JDT LS.** The pinned `java-debug` extension jar
  is provisioned, loaded through `initializationOptions.bundles`, and the debug
  pipeline resolves main class/classpath and calls
  `vscode.java.startDebugSession` before connecting DAP to its port.
- **Dart debugging is provisioned.** `Tool::DartDebug` reuses the managed Dart
  SDK and the plan runs `dart debug_adapter` over stdio against the active file.
- **C# debugging is provisioned.** `Tool::Netcoredbg` downloads the verified
  Samsung archive; the plan runs `--interpreter=vscode` over stdio against the
  newest `bin/Debug/*/<project>.dll` and explains a missing build.
- **JS/TS debugging is provisioned.** `Tool::JsDebug` downloads the official
  `js-debug-dap-v1.140.0` bundle (verified GitHub digest), provisions managed
  Node.js when absent, and writes a `js-debug-dap` launcher. The plan uses a
  known local TCP port and supports JavaScript and TypeScript.
- **Go debugging is provisioned.** `Tool::Dlv` installs the pinned
  `dlv@v1.27.2` through the existing managed-Go plan; `debug::plan_for` uses the
  active file's package directory, `mode: "debug"`, and the TCP DAP transport
  `dlv dap` exposes. The DAP client now speaks both stdio and a TCP socket.
- **Native debugging prefers `lldb-dap`.** Rust/C/C++ plans discover
  `lldb-dap`/`lldb-vscode` first and fall back to `gdb -i=dap`; the tool registry
  exposes it as a debug adapter. Discovery covers LLVM prefixes and Xcode, and a
  mock DAP test drives the lldb plan to a stop. `lldb-dap` is discovery-only for
  now (LLVM's official bundle is very large).
- **Python's debug adapter is provisioned.** `ToolPurpose::DebugAdapter` and
  `Tool::Debugpy` are part of the provisioning model, with a pinned
  `debugpy==1.8.22` plan through pipx/uv/Kemori's virtualenv/`--user`. Debugging
  Python prefers the installed `debugpy-adapter` script and falls back to the
  module only when it is absent. A mock DAP test drives the discovered plan
  through stop, stack, variables and a watch.
- **The 800-line rule now holds across the tree.** `editor/document`,
  `language/tools`, `git`, `project/create`, `app/keys`, `app/lsp`,
  `language/lsp`, `language/web`, `ui/overlay` and `app/tests` were split
  mechanically (directory modules, sibling test modules, or textual includes
  where the module's visibility graph made that safest). Every `.rs` file in
  `src/` is now under 800 lines; the largest is 798.
- **LSP pull diagnostics are wired.** A server that advertises
  `textDocument/diagnostic` is asked for diagnostics on connect and after
  changes; a server that also advertises `workspaceDiagnostics` answers a
  whole-workspace request when **Problems** refreshes. Unopened files are
  converted from the negotiated position encoding and merged with the built-in
  scan and live diagnostics, deduplicated. The `initialize` request now
  advertises both client capabilities.
- **`libc` was added as a direct dependency** for the PTY layer, justified in
  `docs/DECISIONS.md` and the changelog.
- **Phase 1 is complete.** Bottom panel, live tasks, task depth, project
  search/replace, built-in workspace problems, the integrated terminal, LSP pull
  diagnostics and workspace trust are all implemented, and Phase 0's
  oversized-file splits are complete.

> Next action: LSP completion snippets and signature-help depth.


- **Released 3.1.0.** crates.io, docs.rs and the GitHub `v3.1.0` release carry
  four tarballs (Linux glibc/musl, macOS x86_64/arm64), `.deb`, `.rpm`,
  per-archive checksums and `SHA256SUMS`; every checksum was verified locally.
  The release workflow now pins `macos-15-intel` for x86_64 and `macos-15` for
  arm64 after a runner-acquisition failure on the shared arm64 pool.
- **Project-wide built-in diagnostics.** **Problems** now scans every unopened
  project file with its provider's structural checks (4,000 files / 2,000
  problems), deduplicates against live document diagnostics, and groups
  everything by file. `r` refreshes the snapshot; `c` clears task and project
  problems while live document problems stay. LSP pull diagnostics remain.
- **Project-wide search and replace.** `Ctrl+Shift+F` now opens a form with
  literal or regex matching, case/whole-word toggles, include/exclude globs,
  grouped results and a per-line replacement preview. `Alt+Enter` replaces
  atomically on the worker, skips unsaved open files, names every changed file,
  reloads clean open buffers and asks twice before deleting matches.
- **Live task output.** A task opens the Output tab at start, streams
  stdout/stderr as it arrives, follows the tail until the user scrolls up,
  cancels with `x`, reruns with `r`, reopens a completed run with `h`, and
  replaces the live view with the final stdout/stderr-separated result. Its
  `file:line` references become Problems, and **Run Active File** runs a
  standalone script through its language's interpreter.
- **Phase 1's bottom panel is in place.** `Ctrl+J` opens a keyboard-focusable
  panel with **Problems** (live document diagnostics plus a built-in scan of
  unopened project files, grouped by file), **Output** (streaming task output)
  and **Debug Console** (adapter preview messages). Tabs keep their own state;
  `Tab` cycles them, `Alt+↑`/`↓` resize, `Enter` jumps to a problem or
  `file:line` reference, `c` clears, and `Esc` returns to the editor. Task
  output no longer uses a modal overlay.
- **The editor's tiny-viewport scroll bug is fixed.** A panel that leaves one or
  two editor rows no longer scrolls to the end of the file while the statusline
  reports the cursor's line. `ui/editor.rs` was split into a directory module
  (`ui/editor/mod.rs` + `ui/editor/tests.rs`), so that file is back under 800
  lines.
- **Phase 0 is complete.** Every source file is under 800 lines, the two named
  giants included. The extractions were mechanical and the suite stayed green.
- **The project was renamed from Koda to Kemori.** The crate, library and
  binary are `kemori`; 3.0.0 is published on crates.io, docs.rs and GitHub. The
  old `koda-ide` crate has a 2.1.1 deprecation shim, old releases are marked
  deprecated, and existing state is imported once.
- **The event loop moved to `src/app/run.rs`,** bringing `src/app/mod.rs` under
  800 lines.
- **AUR remains the one external task:** the packages are ready and validated,
  but the maintainer's AUR SSH key is not registered on this machine.

> Next action: decide the PTY approach (open decision 3) and implement the
> **integrated terminal**, then live task streaming. The
> `tools.rs`/`document.rs` splits are the only Phase 0 item still open.


> Read `AGENTS.md`, `docs/PRODUCT.md`, `docs/ARCHITECTURE.md`,
> `docs/DECISIONS.md`, `docs/LIMITATIONS.md` and
> `docs/LANGUAGE-SUPPORT.md` first. This document is the current execution
> plan. If it disagrees with a product principle, the principle wins. If it
> disagrees with `ROADMAP.md`, reconcile them — do not silently keep two
> roadmaps.

This file exists so a future session can pick up work without re-auditing the
whole repository. Read the "Current reality" and "Phase 0" sections before
touching code.

---

## 0. The goal and the definition of done

Kemori should be a **general, universal terminal IDE** where the most important
languages are *fully* supported and a fresh install can start coding with zero
configuration.

For this plan, a language counts as fully supported only when it has:

- detection and offline editing/highlighting;
- built-in symbols, diagnostics, completion, hover and navigation where meaningful;
- a formatter, or a deliberate documented reason there is none;
- run and test tasks inferred from the project with no config;
- a debug plan (adapter) or a documented reason debugging is unavailable;
- a language server that Kemori provisions, or an explicit prerequisite/limitation;
- lifecycle and recovery (restart, fallback, clean shutdown);
- an honest matrix row and a live test or an explicit "unverified" marker.

The end-to-end acceptance scenario:

> Open a repository in one of the core languages. Kemori detects it, offers one
> setup action, and then the user can edit, format, run, test, debug, search
> the whole project, use git and open a terminal — without creating a config
> file, installing a language server by hand, or knowing which distribution
> they run.

---

## 1. Strengths to preserve

Do not rewrite these; build on them.

- **Rope editor** with operation-based undo, save points, multi-cursor, soft wrap,
  selections, atomic saves, read-only lossy opening of non-UTF-8 files.
- **Language provider abstraction**: the editor core is language-agnostic;
  detection and providers are separate; adding a language is one trait plus
  registration.
- **LSP lifecycle**: one server per language, bounded handshakes, restart
  budget, fallback to built-in intelligence, centralized position conversion.
- **Provisioning**: pinned and verified downloads, managed toolchains, atomic
  per-step installs, digest-keyed offline cache, prerequisites named precisely.
- **Terminal-native UI**: no opaque full-screen background, data-driven themes,
  keyboard-first, command palette, transparent surfaces.
- **Honesty and safety rules** in `AGENTS.md` and `docs/PRODUCT.md`.

---

## 2. Current reality: 2.1.0 is broad but partly shallow

> **Historical context.** This section records the pre-Phase-0 state that
> justified the work. Phase 0 addressed these findings; see the progress log at
> the end of this file. Do not re-open an item here without checking the code.

These are verified findings. They are the reason Phase 0 exists.

### 2.1 Debugging is a handshake, not a debugger

`src/debug.rs` spawns an adapter, performs `initialize` / `launch` /
`setBreakpoints`, then **kills the child** and returns "Debug adapter … initialized".
`Start Debugging` is therefore misleading. Breakpoints are useful as visual
markers, but nothing stops on them.

**Decision needed:** either remove `Start Debugging` from the palette for now,
or rename it to an explicitly experimental "Debug Adapter Handshake (Preview)".
A real DAP client is Phase 2.

### 2.2 "Remote" only covers git status

`KEMORI_REMOTE` is read only in `Request::RefreshGit`
(`src/background.rs` around line 387). The file tree, editor, LSP, tasks,
push/pull/fetch and GitHub creation are local. This is not remote development.

**Decision:** remove it from user-facing claims until remote editing is real.
Do not advertise a feature that half-exists.

### 2.3 Folding hides rows but does not constrain editing

`Document::is_hidden` is only consulted by `ui/editor.rs`. Cursor movement,
selection and typing can enter hidden lines. There is no fold-aware navigation,
no remapping of `folded` header rows when lines are inserted/deleted above them,
no LSP `foldingRange` support, and **no keybinding** — `Toggle Fold` is only in
the palette.

Relevant code:

- `src/editor/document.rs` — `fold_range`, `toggle_fold`, `is_hidden`;
- `src/ui/editor.rs` — hidden-row skip in rendering;
- `src/app/view.rs` — `toggle_fold`;
- `src/app/keys.rs` — no fold key.

**Decision:** make folding correct (Phase 0) or remove it. Do not leave a feature
that can lose the cursor.

### 2.4 Git operations block the UI thread

These run synchronously from `src/app/git.rs`:

- `push`, `pull`, `fetch`;
- `diff` via `show_diff`;
- `apply_patch` via `apply_diff_hunk`;
- GitHub repository creation.

In addition, after every `BackgroundEvent::Git`, `src/app/background.rs` calls
`crate::git::line_changes(&root, &path)` for every open document **on the UI
thread**. That spawns several git processes per document.

This contradicts the stated principle "never block the UI on expensive work".

### 2.5 Multi-project is project switching, not multi-root

`src/app/session.rs` keeps `workspaces: Vec<Workspace>` in memory. Switching:

- saves the current session;
- swaps the workspace;
- clears the editor and LSP map;
- restores the target session.

The list is not persisted, and `poll_lsp` only considers the active workspace
root. There is no unified tree, search, problems list or LSP workspace-folders
model. Rename or complete it in Phase 4.

### 2.6 Project search is read-only and plain

`src/search.rs` does case-insensitive substring search, one match per line,
`.gitignore`-aware and bounded. There is no regex, case toggle, whole-word,
include/exclude globs, grouped results panel, or replace. In-file find has
those options; project search does not.

### 2.7 Documentation has drifted from the code

- `docs/LIMITATIONS.md` still says there is no mouse support, no file icons,
  no folding, no git gutter, no hunk staging, no incremental sync, and that
  non-UTF-8 files are refused. All of those have since changed.
- `docs/ARCHITECTURE.md` does not list `src/app/tasks.rs`, `src/debug.rs`,
  `src/remote.rs`, `src/language/support.rs`, `src/github.rs`.
- `docs/DECISIONS.md` says three bundled themes; the product ships four
  (Mellow, Midnight, Daylight, Ansi).
- `README.md` does not mention folding, git gutter, non-UTF-8 handling,
  multi-project switching, mouse support, incremental sync, the debug scaffold
  or the remote runner.
- `CHANGELOG.md` claims all of 2.1.0; the docs above imply otherwise.

This violates the product principle "Be honest".

### 2.8 The 800-line rule is widely violated

`AGENTS.md` says no source file may exceed 800 lines. Current offenders include:

| File | Lines |
| --- | ---: |
| `src/language/tools.rs` | 7,037 |
| `src/app/tests.rs` | 3,614 |
| `src/editor/document.rs` | 2,984 |
| `src/ui/overlay.rs` | 1,475 |
| `src/language/lsp/mod.rs` | 1,143 |
| `src/app/overlay.rs` | 1,127 |
| `src/language/web/mod.rs` | 1,123 |
| `src/git/mod.rs` | 983 |
| `src/app/commands.rs` | 971 |
| `src/project/create.rs` | 953 |
| `src/app/lsp.rs` | 884 |
| `src/app/keys.rs` | 871 |
| `src/ui/editor.rs` | 830 |

`src/app/mod.rs` is back under the limit (783) after the event loop moved to
`src/app/run.rs`. Some remaining files are tests or predate the rule, but new
work must not grow them. Plan mechanical splits with the suite green after every
commit.

### 2.9 New 2.1 features lack tests

`cargo test` is green, but there are no tests for folding, breakpoints, DAP,
remote execution, gutter markers, hunk apply/revert, mouse behaviour, or the
workspace switcher. The 2.1.0 commit added no new files under `tests/`.

---

## 3. Full-IDE gap map

### Edit and understand

- Completion documentation and resolve.
- Call/type hierarchy, implementation/type definition, document links.
- Outline panel and breadcrumbs. "Go to Symbol" is a picker, not a navigable
  outline.
- Correct folding, fold all/unfold all, fold state survives edits.
- Format on save, trim trailing whitespace, final newline.
- Line-ending conversion, persistent undo, read-only toggle.

### Run, test, debug

- **Integrated terminal** — the largest missing "IDE" feature. PTY-backed
  shell, project cwd, multiple sessions, resize, copy/paste, signals.
- **Live task output** with streaming, cancel, rerun and history. Current tasks
  run to completion and show one bounded batch.
- **Problems panel** across the whole workspace, not only open files.
- **Real DAP debugger**: stack, scopes, variables, watch, stepping, conditional
  and log breakpoints, output, attach/launch.
- **Zero-config run/test/debug inference** for the active file and project kind.

### Search and navigate

- Project-wide replace with preview, include/exclude globs, case/word/regex,
  atomic writes and a clear undo model.
- Results grouped by file in a panel.
- Workspace-wide diagnostics and symbol search that scales.

### Version control

- Branch create/switch/delete UI (helpers exist, commands do not).
- Log/history, blame, stash, amend.
- Per-hunk staging (the diff overlay can apply/revert a hunk but not stage it).
- Merge-conflict resolution.
- All git work asynchronous with progress and cancellation.

### Project and workspace

- Real multi-root workspace: persisted roots, unified tree/search/problems,
  LSP `workspaceFolders`.
- Restore split panes, active pane and a reopen-closed-tab stack.
- **Workspace trust**: tasks, debug adapters and project-specific language
  servers execute project code. Kemori currently has no trust gate.
- First-run "Set up and start" flow that previews what will be installed.

### Language breadth

The matrix has 27 languages. Missing high-demand languages include:

- Zig, R, Scala, Haskell, Julia, Clojure, Erlang;
- Vue, Svelte, Astro;
- Terraform/HCL;
- Dockerfile, Makefile, CMake, XML, INI, `.env`, GraphQL, Protobuf, Solidity,
  diff/patch.

But adding highlighters is not the priority. The core languages already in the
matrix have gaps in formatters, run/test tasks and debugging. Fill those first.

---

## 4. Roadmap

### Phase 0 — Make 2.1 honest and non-blocking

**Status:** complete.

**Goal:** every claim matches the code, no UI-thread subprocess, no cursor can
get lost in a fold.

- [x] **Docs truth pass.** Update `ARCHITECTURE.md`, `DECISIONS.md`,
      `LIMITATIONS.md`, `README.md`, `CHANGELOG.md` and, if direction changes,
      `ROADMAP.md`. Keep `AGENTS.md` rules-only.
- [x] **Gate or remove fake features.** Hide `Start Debugging` behind an
      explicit preview, remove `KEMORI_REMOTE` from claims, and state the real
      debugging/remote status in `LIMITATIONS.md`.
- [x] **Move git work to the worker.** Add background requests for push, pull,
      fetch, diff, apply-patch, GitHub repo creation, and per-document
      `line_changes`. Show progress and allow cancellation. Add tests that the
      UI path does not spawn git. *(Cancellation is still missing and is
      documented in `LIMITATIONS.md`.)*
- [x] **Fix folding or remove it.** Make movement, selection and editing
      fold-aware. Remap fold headers on edits. Add `za`/`zo`/`zc`/`zR`/`zM`
      (or a documented conventional binding). Consume LSP folding ranges.
      Add tests.
- [x] **Add 2.1 regression tests.** Folding, breakpoints, gutter markers, hunk
      apply/revert, non-UTF-8 conversion, workspace switching, incremental
      sync, mouse, icons.
- [x] **Split oversized files.** Every `.rs` file under `src/` is now under 800
      lines. `editor/document`, `language/tools`, `app/keys`, `app/lsp`,
      `language/lsp`, `language/web` and `ui/overlay` were split mechanically;
      `git`, `project/create`, `app/overlay` and `app/tests` had their tests
      extracted. The largest remaining file is 798 lines.

**Exit criteria:** no false feature claims; no git subprocess on the UI thread;
folding either correct and tested or removed; 2.1 features have tests.

### Phase 1 — The core IDE loop

**Status:** complete — bottom panel, live tasks, project search/replace,
workspace problem scanning, the integrated terminal, LSP pull diagnostics and
workspace trust have all landed.

**Goal:** a user can run, test, search and navigate without leaving Kemori.

- [x] **Bottom panel** with tabs: Problems, Output, Debug Console. `Ctrl+J`
      focus toggle, `Tab` to switch tabs, `Alt+↑`/`↓` resize, per-tab state,
      scroll/selection, clear, and `Esc` back to the editor. *The Terminal tab
      ships with the integrated terminal below.*
- [x] **Integrated terminal** via a PTY, rooted at the workspace. Multiple
      sessions, copy/paste, resize, signals. A dependency decision is required;
      record it in `DECISIONS.md`.
      *Shipped: Unix PTY via `libc`, a terminal screen model, four-panel tab,
      session switching, scrollback, process-group shutdown and trust gating.
      Mouse selection and persistent sessions remain documented limitations.*
- [x] **Live task output.** Stream stdout/stderr into Output, follow the tail
      until the user scrolls, cancel with `x`, rerun with `r`, and show the
      final stdout/stderr-separated batch.
- [x] **Task depth.** Session output history (`h`), `file:line` references
      extracted into Problems, and `Run Active File` for standalone scripts
      (refused for a dirty buffer or a language without a single-file runner).
- [x] **Project search and replace.** Literal/regex, case, whole word,
      include/exclude globs, results grouped by file, replacement preview,
      atomic writes, unsaved-buffer safety and every changed file named.
- [x] **Problems panel (built-in).** Workspace scan of unopened files grouped
      by file/severity, combined with live diagnostics and task problems;
      refreshed with `r`.
- [x] **LSP pull diagnostics.** Workspace-wide type diagnostics from a language
      server where it advertises `textDocument/diagnostic`.
      *Shipped: per-document pull on connect/change and `workspace/diagnostic`
      on Problems refresh; unopened files merge into the panel.*
- [x] **Workspace trust and safe mode.** Ask before running project tasks,
      debug adapters or project-specific servers. Store the decision in Kemori's
      state; never require a user-maintained config file.
      *Shipped: tasks, the debug preview and terminal sessions are gated per root
      in `trust.json`. Kemori-provisioned language servers start without a prompt
      because they are not project code.*

**Exit criteria:** open a repo, run a task in the terminal, see output live,
jump to errors, replace across the project, all without config.

### Phase 2 — Language intelligence depth

**Status:** in progress — editor depth, formatter coverage, the DAP debugger,
adapter provisioning, attach and the main LSP intelligence features have landed.
Remaining: completion snippets/signature depth, Swift debugging, Java/Ruby
attach, formatter provisioning and a dedicated variable tree.

**Goal:** the editor and LSP feel like a modern IDE, not a collection of pickers.

- [x] LSP features: highlights, inlay hints, code lens, selection ranges,
      call/type hierarchy, implementation/type definition, document links,
      organize imports and format selection.
      *Shipped: document highlights, type/parameter inlay hints, runnable code
      lenses, Expand/Shrink Selection, incoming/outgoing calls and
      supertype/subtype pickers, implementation and type-definition jumps,
      document links, `source.organizeImports` code actions and
      `textDocument/rangeFormatting`. Stale responses are dropped and edits
      clear old ranges.*
- [x] LSP lifecycle: workspace folders and progress reporting.
      *Shipped: pull diagnostics, `didSave` with optional text, dynamic
      watched-file registration and `workspace/didChangeWatchedFiles`
      notifications, `$/cancelRequest` for superseded or timed-out requests,
      one workspace folder advertised to the server, and `$/progress`
      begin/report/end shown in the statusline.*
- [ ] Completion: snippets and better signature help.
      *Shipped: completion item detail/documentation, `completionItem/resolve`
      for servers that advertise it, and documentation preview in the popup.
      Snippet insertion and richer signature help remain.*
- [x] Editor: snippet engine, format on save, trim trailing whitespace, final
      newline, line-ending conversion, persistent undo, read-only toggle,
      column/rectangular selection, search in selection, replace preview,
      large-file mode.
      *Shipped: snippet engine with built-in language snippets and tab stops,
      rectangular (column) selection with per-row edits, search in selection
      (find and replace, with `[selection]` in the bar), an `Alt+P` replace
      preview that applies on `Enter`/`a`, large-file mode at one million
      characters, user read-only mode via `Ctrl+Alt+R`, format on save
      (Settings), persistent undo across sessions for clean files, trim trailing
      whitespace, final newline and LF/CRLF conversion commands.*
- [x] Formatters: Python (`ruff format`/`black`), Ruby, PHP, Kotlin, Swift,
      Lua, SQL, Java and C# where the server does not already format.
      *Shipped through installed tools: Python (ruff/black), Lua (stylua), Swift
      (swift-format), Java (google-java-format), Ruby (RuboCop), Kotlin (ktlint),
      SQL (SQLFluff), PHP (PHP-CS-Fixer) and C# (CSharpier). Provisioning these
      formatters remains a separate improvement.*
- [x] **Real DAP debugger.** Sessions, breakpoints (conditional/log), stack,
      scopes, variables, watch, stepping, output, attach/launch.
      *Shipped: sessions, conditional and log breakpoints, stack, scopes,
      variables, watch expressions, stepping, output, launch and stop. Attach
      and a dedicated variable tree remain.*
- [ ] **Zero-config adapters** by language/project: Python `debugpy`, Go `dlv`,
      Rust/C/C++/Swift `lldb-dap`/codelldb, JS/TS `js-debug`, Java
      `java-debug`, C# `netcoredbg`, Ruby `rdbg`, PHP Xdebug, Dart VM service.
      Provision pinned/verified adapters where possible; otherwise discover and
      name the prerequisite.
      *Shipped: pinned `debugpy==1.8.22`, `dlv@v1.27.2`, `js-debug-dap-v1.140.0`,
      `netcoredbg-3.2.0-1092`, `java-debug-0.53.1`, Ruby `debug-1.11.1` and
      `php-debug-1.40.2`, the SDK's `dart debug_adapter`, discovered
      `debugpy-adapter` preferred over the module fallback, Rust/C/C++ discovery
      of `lldb-dap` (plus `lldb-vscode`) with `gdb -i=dap` fallback, and TCP DAP
      for `dlv`/js-debug/rdbg. `lldb-dap` itself is not downloaded because
      LLVM's official bundle is very large. Attach is wired for Python, Go,
      JS/TS, C#, PHP and Dart; Java/Ruby attach remain. Swift debug plans are
      still missing.*
- [x] Add a `DebugAdapter` tool purpose to the provisioning model.
      *Shipped: `ToolPurpose::DebugAdapter`, `Tool::Debugpy`, pinned install
      attempts, and `available_debug_adapter`.*

**Exit criteria:** at least Python, Go, Rust, JS/TS and C/C++ can stop on a
breakpoint, step and inspect variables with no launch configuration.

### Phase 3 — Universal language coverage

Add a small number of languages to the **full** contract, not twenty partial
ones.

**Tier 1 languages**

- Zig (`zls`)
- R (`languageserver`)
- Scala (Metals)
- Haskell (HLS)
- Julia (LanguageServer.jl)
- Clojure (clojure-lsp)
- Erlang (`elp`/`erlang_ls`)
- Vue (Vue language server/Volar)
- Svelte (svelte-language-server)
- Terraform/HCL (`terraform-ls`)

**Tier 1 config/file languages**

- Dockerfile, Makefile, CMake, XML, INI, `.env`, GraphQL, Protobuf, Solidity,
  diff/patch.

For each language, the completion checklist is:

- [ ] `LanguageId` variant and `ALL` entry;
- [ ] provider + descriptor (extensions, file names, shebangs, content hints);
- [ ] highlighting and offline features where meaningful;
- [ ] detection tests;
- [ ] formatter (managed or documented prerequisite);
- [ ] project marker / project kind;
- [ ] run and test tasks;
- [ ] scaffolding entry in `CREATABLE` where meaningful;
- [ ] language server in `Tool::ALL` with pinned/verified provisioning or a
      precise prerequisite;
- [ ] support matrix row in `docs/LANGUAGE-SUPPORT.md` and `support.rs`;
- [ ] live handshake test in `tests/optional_servers.rs` or an explicit
      unverified marker;
- [ ] `README.md` supported-language table.

Also extend the support contract: add format/run/test/debug columns or checks
so "Complete" means the development loop works.

### Phase 4 — Workflow and workspace depth

- [ ] Git branch create/switch/delete, log/history, blame, stash, amend,
      per-hunk staging, conflict resolution.
- [ ] Persist multi-root workspaces; unified tree/search/problems; LSP
      workspace folders.
- [ ] Restore split panes, pane focus and a reopen-closed-tab stack.
- [ ] CLI: `kemori file:line`, `--help`, `--version`, `--diff`, `--wait`,
      `--no-lsp`.
- [ ] One-line installer and self-update.

### Phase 5 — Performance and quality

- [ ] Performance budgets: startup, 50 MB file, 100k-file project, frame
      latency.
- [ ] Fuzz providers and the regex engine.
- [ ] Live matrix tests for every server and debug adapter.
- [ ] Interactive verification of the light theme and key chords.
- [ ] Benchmark the hand-written highlighters against tree-sitter, but do not
      migrate without a measured decision (see `DECISIONS.md`).

---

## 5. Suggested next commits

1. `feat(debug): provision pinned DAP adapters and add conditional breakpoints`
2. `feat(language): close Python/Ruby/PHP/Kotlin/Swift/Lua formatter gaps`
3. `feat(editor): format on save and large-file mode`
4. `feat(lsp): inlay hints, document highlights and selection ranges`

The real debugger and the file-size split are done; the next debugger depth is
adapter provisioning and conditional breakpoints.

---

## 6. Working agreement and verification

Every commit must:

- have one clear purpose;
- compile with `cargo check --all-targets`;
- pass `cargo test`;
- pass `cargo clippy --all-targets` with no warnings;
- pass `cargo fmt --check`;
- update `CHANGELOG.md` for user-visible changes;
- update `ROADMAP.md` when direction changes;
- keep `README.md`, `AGENTS.md` and this file in sync.

Commands:

```bash
cargo check --all-targets
cargo test
cargo clippy --all-targets
cargo fmt --check
cargo run -- .
```

New subprocess call sites must use `process::wait_captured` (or
`process::wait_streaming` for live output). New managed downloads must be pinned
and verified. Never paint an opaque full-screen
background. Never use `unwrap()` on user input or filesystem results. Never
add a dependency without listing and justifying it first.

Tests by area:

- editor/buffer: unit tests next to the code;
- detection/providers: unit tests in their modules;
- UI composition: `tests/render.rs` with ratatui `TestBackend`;
- live servers: `tests/optional_servers.rs` and tooling suites, `#[ignore]`d
  and skipping when the tool is absent.

---

## 7. Open decisions

Record the answer in `docs/DECISIONS.md` when decided.

1. **Debugging now or later?** Remove/hide the scaffold immediately, or invest
   in a real DAP client as part of Phase 1?
2. **Remote:** delete `KEMORI_REMOTE` until real remote development exists, or
   keep it as an internal experimental seam?
3. **Terminal dependency:** `portable-pty` or Unix PTY via `nix`/`libc`?
   **Decided:** Unix PTY via `libc` directly (`openpty`, `TIOCSCTTY`,
   `TIOCSWINSZ`, process groups). Windows is out of scope, and the raw layer is
   smaller than a process-management framework. Recorded in `DECISIONS.md`.
4. **Multi-root:** complete the workspace model, or rename the feature to
   "recent projects / project switching" and defer multi-root?
5. **Folding:** fix it fully now, or remove the command until it is correct?
6. **File-size rule:** approve mechanical splits of `tools.rs` and
   `document.rs` before adding features there?
7. **Support contract:** do "Complete" languages now require format, run, test
   and debug, or does the old narrower contract remain until Phase 2?

---

## 8. What not to do

These are deliberate non-goals; do not drift into them while chasing "full IDE".

- Do not recreate VS Code in a terminal or add panel sprawl.
- Do not add a plugin marketplace or general extension platform.
- Do not require a config file, or its absence, for a core feature.
- Do not chase breadth over depth: prefer fewer languages that are actually
  complete.
- Do not replace the user's package manager or system tools.
- Do not add Windows-only paths while Windows remains unsupported.
- Do not bundle a whole toolchain where a smaller verified upstream download
  exists.
- Do not migrate the highlighting engine to tree-sitter without a measured
  comparison and an explicit decision.

---

## 9. Progress log

Keep this table current so the next session knows where to resume.

| Phase | Status | Last commit | Notes |
| --- | --- | --- | --- |
| 0. Docs truth pass | done | 48bf5ec | ARCHITECTURE, DECISIONS, LIMITATIONS, README, ROADMAP, CHANGELOG |
| 0. Gate/remove DAP + remote | done | e1e7eb5 | Debug command is an explicit preview; `KEMORI_REMOTE` is internal |
| 0. Git off the UI thread | done | e1e7eb5 | No cancellation yet; documented |
| 0. Folding correctness | done | e1e7eb5 | Fold-aware movement, edit remapping, `Ctrl+K` chords, LSP ranges |
| 0. 2.1 regression tests | done | 45d099f | Folding, breakpoints, gutter, hunk patch, non-UTF-8, workspace, incremental sync, mouse, icons |
| 0. Oversized-file splits | done | unreleased | Every file under `src/` is under 800 lines; largest is 798 |
| Release / identity | done | dd64f6b | Renamed to Kemori; 3.0.0 on crates.io, docs.rs and GitHub; old `koda-ide` deprecated |
| Release 3.1.0 | done | 36c349e | crates.io, docs.rs, GitHub release with four tarballs, deb/rpm and verified checksums |
| Release 3.2.0 | done | v3.2.0 | Superseded by 3.2.1 after a musl-only test failure; crates.io and GitHub assets remain published |
| Release 3.2.1 | done | v3.2.1 | Fixes the musl netcoredbg test; crates.io, docs.rs and GitHub release |
| AUR publication | blocked | dd64f6b | AUR account registration is disabled; packages remain at 3.0.0 and cannot be pushed until registration reopens |
| 1. Bottom panel | done | 3f1083e | Problems, Output and Debug Console; `Ctrl+J`, tab switching, resize, per-tab state |
| 1. Integrated terminal | done | unreleased | PTY via `libc`, four-tab panel, sessions, scrollback, copy/paste, resize, process-group shutdown |
| 1. Live tasks | done | 03d5362 | Streaming, follow-tail, cancel and rerun |
| 1. Task depth | done | 04845d4 | Session history, references in Problems and Run Active File |
| 1. Problems panel | done | 5b0cafd | Built-in scan of unopened files plus live and task problems |
| 1. LSP pull diagnostics | done | unreleased | Per-document pull plus `workspace/diagnostic` merged into Problems |
| 1. Project search/replace | done | 9be6382 | Form overlay, literals/regex, globs, preview, atomic replace, unsaved-buffer safety |
| 1. Workspace trust | done | unreleased | Per-root `trust.json`; tasks, debug preview and terminal gated |
| 2. LSP depth | partial | unreleased | All LSP feature requests shipped (highlights, hints, lens, selection, hierarchies, implementation/type definition, links, organize imports, range formatting); lifecycle and completion depth remain |
| 2. Editor depth | done | unreleased | Snippets with tab stops, persistent undo, format on save, column selection, search in selection, replace preview, large-file mode, read-only toggle, trim/newline/line-ending commands |
| 2. Formatter gaps | partial | v3.2.0 | All listed languages wired through installed tools; Kemori does not provision the formatters yet |
| 2. Real DAP debugger | partial | v3.2.0 | Sessions, conditional/log breakpoints, watch, stack/scopes/variables, stepping, launch/stop; a dedicated variable tree remains |
| 2. Debug-adapter provisioning | partial | v3.2.0 | `DebugAdapter` purpose; pinned `debugpy`, `dlv`, `js-debug`, `netcoredbg`, `java-debug`, Ruby `debug`, `php-debug`; Dart SDK adapter; lldb-dap discovery; Swift adapter remains |
| 2. DAP attach | partial | v3.2.0 | Prompt + attach plans for Python/Go/JS/TS/C#/PHP/Dart; Java/Ruby attach open |
| 3. Language expansion | not started | — | — |
| 4. Git/workspace depth | not started | — | — |
| 5. Performance/quality | not started | — | — |

When a phase starts, change its status to `in progress` and add the branch or
commit. When it finishes, record the commit and update `CHANGELOG.md`.
