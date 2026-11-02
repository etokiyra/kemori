# NEXT — the working plan for turning Kemori into a full IDE

**Status snapshot:** 2026-11-02, version 3.0.0, commit `9be6382`.
**Verification at the time of writing:** `cargo check --all-targets`, `cargo clippy --all-targets` clean; `cargo test` green (641 library tests, 51 render tests, plus tooling suites with ignored live tests).
**Release status:** `kemori` 3.0.0 is on crates.io, docs.rs and the GitHub `v3.0.0` release. The AUR packages are prepared and validated locally; pushing them still needs the maintainer's AUR SSH key.

### What this session completed

- **Project-wide search and replace.** `Ctrl+Shift+F` now opens a form with
  literal or regex matching, case/whole-word toggles, include/exclude globs,
  grouped results and a per-line replacement preview. `Alt+Enter` replaces
  atomically on the worker, skips unsaved open files, names every changed file,
  reloads clean open buffers and asks twice before deleting matches.
- **Phase 1's bottom panel is in place.** `Ctrl+J` opens a keyboard-focusable
  panel with **Problems** (diagnostics across open files, grouped by file),
  **Output** (the last task's captured result) and **Debug Console** (adapter
  preview messages). Tabs keep their own state; `Tab` cycles them, `Alt+↑`/`↓`
  resize, `Enter` jumps to a problem or `file:line` reference, `c` clears, and
  `Esc` returns to the editor. Task output no longer uses a modal overlay.
- **The editor's tiny-viewport scroll bug is fixed.** A panel that leaves one or
  two editor rows no longer scrolls to the end of the file while the statusline
  reports the cursor's line. `ui/editor.rs` was split into a directory module
  (`ui/editor/mod.rs` + `ui/editor/tests.rs`), so that file is back under 800
  lines.
- **Phase 0 is complete except the two remaining giant files.** `tools.rs` and
  `document.rs` still need mechanical splits.
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

- Snippet engine; completion documentation and resolve.
- Inlay hints, document highlights, code lens, call/type hierarchy,
  implementation/type definition, selection ranges, folding ranges, document
  links.
- Outline panel and breadcrumbs. "Go to Symbol" is a picker, not a navigable
  outline.
- Correct folding, fold all/unfold all, fold state survives edits.
- Format on save, trim trailing whitespace, final newline.
- Line-ending conversion, persistent undo, read-only toggle.
- Column/rectangular selection, search in selection, replace preview.
- Large-file mode.

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

**Status:** complete except the oversized-file splits.

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
- [ ] **Start splitting oversized files.** One mechanical extraction per
      commit, suite green each time. Start with `language/tools.rs` and
      `editor/document.rs`. *Started: the event loop moved out of
      `src/app/mod.rs` (now under 800), and `ui/editor.rs` became a directory
      module under 800; the two largest files are still untouched.*

**Exit criteria:** no false feature claims; no git subprocess on the UI thread;
folding either correct and tested or removed; 2.1 features have tests.

### Phase 1 — The core IDE loop

**Status:** in progress — bottom panel landed; terminal, live tasks, replace,
workspace-wide problems and trust remain.

**Goal:** a user can run, test, search and navigate without leaving Kemori.

- [x] **Bottom panel** with tabs: Problems, Output, Debug Console. `Ctrl+J`
      focus toggle, `Tab` to switch tabs, `Alt+↑`/`↓` resize, per-tab state,
      scroll/selection, clear, and `Esc` back to the editor. *The Terminal tab
      ships with the integrated terminal below.*
- [ ] **Integrated terminal** via a PTY, rooted at the workspace. Multiple
      sessions, copy/paste, resize, signals. A dependency decision is required;
      record it in `DECISIONS.md`.
- [ ] **Live tasks.** Stream stdout/stderr into Output; cancel, rerun, history.
      Parse `file:line[:col]` references into Problems. Add "Run Active File"
      for standalone scripts.
- [x] **Project search and replace.** Literal/regex, case, whole word,
      include/exclude globs, results grouped by file, replacement preview,
      atomic writes, unsaved-buffer safety and every changed file named.
- [ ] **Problems panel.** Workspace-wide diagnostics grouped by file/severity;
      LSP pull diagnostics where supported. *The tab currently covers the open
      files and groups them by file.*
- [ ] **Workspace trust and safe mode.** Ask before running project tasks,
      debug adapters or project-specific servers. Store the decision in Kemori's
      state; never require a user-maintained config file.

**Exit criteria:** open a repo, run a task in the terminal, see output live,
jump to errors, replace across the project, all without config.

### Phase 2 — Language intelligence depth

**Goal:** the editor and LSP feel like a modern IDE, not a collection of pickers.

- [ ] LSP: inlay hints, document highlights, code lens, folding ranges,
      selection ranges, call/type hierarchy, implementation/type definition,
      document links, organize imports, format selection.
- [ ] LSP lifecycle: `didSave`, watched-file notifications, pull diagnostics,
      workspace folders, progress reporting, `$/cancelRequest`.
- [ ] Completion: snippets, documentation, resolve, better signature help.
- [ ] Editor: snippet engine, format on save, trim trailing whitespace, final
      newline, line-ending conversion, persistent undo, read-only toggle,
      column/rectangular selection, search in selection, replace preview,
      large-file mode.
- [ ] Formatters: Python (`ruff format`/`black`), Ruby, PHP, Kotlin, Swift,
      Lua, SQL, Java and C# where the server does not already format.
- [ ] **Real DAP debugger.** Sessions, breakpoints (conditional/log), stack,
      scopes, variables, watch, stepping, output, attach/launch.
- [ ] **Zero-config adapters** by language/project: Python `debugpy`, Go `dlv`,
      Rust/C/C++/Swift `lldb-dap`/codelldb, JS/TS `js-debug`, Java
      `java-debug`, C# `netcoredbg`, Ruby `rdbg`, PHP Xdebug, Dart VM service.
      Provision pinned/verified adapters where possible; otherwise discover and
      name the prerequisite.
- [ ] Add a `DebugAdapter` tool purpose to the provisioning model.

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

1. `feat(terminal): PTY-backed terminal in the bottom panel`
2. `feat(tasks): stream live output with cancel and rerun`
3. `feat(diagnostics): workspace-wide Problems and pull diagnostics`
4. `feat(app): workspace trust before running project code`

Then the real debugger, then language expansion.

This order is deliberate: add the terminal and live feedback, then make problems
project-wide, then gate project code before the debugger can run it.

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

New subprocess call sites must use `process::wait_captured`. New managed
downloads must be pinned and verified. Never paint an opaque full-screen
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
   Windows is out of scope, so a Unix-first choice is acceptable if documented.
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
| 0. Oversized-file splits | in progress | a5bb9ea | `app/mod.rs` and `ui/editor` split; `tools.rs` and `document.rs` remain |
| Release / identity | done | dd64f6b | Renamed to Kemori; 3.0.0 on crates.io, docs.rs and GitHub; old `koda-ide` deprecated |
| AUR publication | prepared | dd64f6b | Packages built and validated; waiting for the maintainer's AUR SSH key |
| 1. Bottom panel | done | 3f1083e | Problems, Output and Debug Console; `Ctrl+J`, tab switching, resize, per-tab state |
| 1. Problems panel | partial | 3f1083e | Covers open files and groups by file; workspace-wide sweep and pull diagnostics remain |
| 1. Integrated terminal | not started | — | Needs the PTY dependency decision (open decision 3) |
| 1. Live tasks | not started | — | Output is a completed batch in the panel; streaming, cancel and rerun remain |
| 1. Project search/replace | done | 9be6382 | Form overlay, literals/regex, globs, preview, atomic replace, unsaved-buffer safety |
| 1. Workspace trust | not started | — | — |
| 2. LSP depth | not started | — | — |
| 2. Editor depth | not started | — | — |
| 2. Formatter gaps | not started | — | — |
| 2. Real DAP debugger | not started | — | — |
| 3. Language expansion | not started | — | — |
| 4. Git/workspace depth | not started | — | — |
| 5. Performance/quality | not started | — | — |

When a phase starts, change its status to `in progress` and add the branch or
commit. When it finishes, record the commit and update `CHANGELOG.md`.
