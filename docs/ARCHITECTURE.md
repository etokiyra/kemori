# Architecture

The single source of truth for Kemori's module layout and dependency rules.
`AGENTS.md` and `README.md` link here instead of keeping their own copies.

---

## Source tree

```text
src/
├── main.rs                 # CLI entry point (thin)
├── lib.rs                  # module tree
├── app/
│   ├── mod.rs              # App state, constructor, event loop, small helpers
│   ├── background.rs       # applying worker events, external-change polling
│   ├── availability.rs     # command availability and format checks
│   ├── commands.rs         # command dispatch, palette, prompts, quick open
│   ├── completion.rs       # completion popup + hover
│   ├── debug.rs            # debugger session, stack/scopes/variables
│   ├── diagnostics.rs      # diagnostics polling, navigation, list
│   ├── editor.rs           # cursor moves, clipboard, comment, matching bracket
│   ├── files.rs            # save/close/revert and file create/rename/copy/delete
│   ├── git.rs              # diff, changed files, staging, commit
│   ├── keys.rs             # key dispatch (global, editor, tree, overlay, search)
│   ├── language.rs         # navigation, symbols, rename, code actions, formatting
│   ├── lsp.rs              # server lifecycle + response handling
│   ├── mouse.rs            # editor hit-testing, click and drag selection
│   ├── overlay.rs          # picker/prompt/search state + fuzzy matching
│   ├── panel.rs            # bottom panel: problems, output, debug console
│   ├── panes.rs            # split panes and tabs
│   ├── project_search.rs   # project search/replace state, keys and hand-off
│   ├── run.rs              # the main event loop
│   ├── search.rs           # find/replace
│   ├── session.rs          # session persistence, workspace + welcome actions
│   ├── settings.rs         # Settings screen + live application of preferences
│   ├── setup.rs            # tool installation, project setup, tool actions
│   ├── status.rs           # statusline messages and toasts
│   ├── tasks.rs            # task picker and bounded run output
│   ├── terminal_panel.rs   # integrated terminal: keys, sessions, copy/paste
│   ├── tests.rs            # the app-level integration tests
│   ├── trust.rs            # workspace-trust prompt and gate
│   └── view.rs             # tree/diagnostic/wrap/fold toggles and reveal
├── background.rs           # worker pool for detection, LSP, git, formatting, tasks
├── task_run.rs             # one project task: streaming, bounded, cancellable
├── commands/mod.rs         # command registry (ids, titles, shortcuts)
├── debug/
│   ├── mod.rs              # launch plans and DAP types
│   ├── protocol.rs         # Content-Length framing
│   └── session.rs          # adapter process: requests, events, stack/scopes/variables
├── editor/
│   ├── buffer.rs           # rope-backed text buffer
│   ├── document/           # buffer + cursor + selection + undo + highlight cache
│   │   ├── mod.rs          # state, lifecycle, diagnostics and base edits
│   │   ├── cursors.rs      # multiple cursors and edit planning
│   │   ├── typing.rs       # typing, indentation and line operations
│   │   ├── movement.rs     # history, selection and navigation
│   │   ├── highlight.rs    # highlight cache and search helpers
│   │   ├── helpers.rs      # bracket, indentation and word helpers
│   │   └── tests.rs        # document unit tests
│   ├── folding.rs          # visible-row folding, movement and edit remapping
│   ├── history.rs          # undo/redo edits
│   ├── layout.rs           # tab expansion + soft-wrap segments
│   ├── position.rs         # Position / Selection / Cursor
│   └── mod.rs              # Editor (open documents / tabs)
├── filesystem/
│   ├── mod.rs              # fs helpers (sorted reads, atomic writes, walking)
│   └── gitignore.rs        # .gitignore matcher
├── git/mod.rs              # branch + file status via the `git` binary
├── github.rs               # `gh` repository creation (delegated auth)
├── language/
│   ├── id.rs               # LanguageId
│   ├── mod.rs              # LanguageService facade + Capability
│   ├── symbols.rs          # workspace symbol scan
│   ├── format.rs           # formatter pipeline (stdin/stdout tools)
│   ├── tools/              # discovery + trusted provisioning
│   │   ├── mod.rs          # Tool enum, probes, registry facade
│   │   ├── spec.rs         # pinned versions and limits
│   │   ├── plan.rs         # attempt builders and install types
│   │   ├── install.rs      # lock, preflight, step execution
│   │   ├── download.rs     # verified downloads and the cache
│   │   ├── runtimes.rs     # JDK, .NET, Node, Dart runtimes
│   │   ├── remote.rs       # metadata, GPG and GitHub releases
│   │   ├── toolchains.rs   # Go/Swift/Elixir platform toolchains
│   │   ├── archive.rs      # safe extraction and promotion
│   │   ├── hash.rs         # SHA-256/512 helpers
│   │   ├── status.rs       # probe results and portable helpers
│   │   ├── status_types.rs # ToolStatus
│   │   ├── registry.rs     # ToolRegistry and server probing
│   │   ├── discovery.rs    # locating executables
│   │   ├── paths.rs        # managed directories
│   │   ├── env.rs          # launch environments
│   │   └── attempts_more.rs# per-language install attempts
│   ├── setup.rs            # project setup plan (what is missing)
│   ├── support.rs          # support class surfaced in-app
│   ├── data.rs             # shared keyword/builtin data
│   ├── completion.rs       # completion candidate types
│   ├── diagnostics.rs      # Diagnostic types
│   ├── hover.rs            # hover data
│   ├── detection/          # engine, signals, confidence
│   ├── provider/           # LanguageProvider trait + ProviderRegistry
│   ├── lsp/                # JSON-RPC client, lifecycle, result conversion
│   │   ├── mod.rs
│   │   ├── jsonrpc.rs
│   │   └── convert.rs      # the char ↔ LSP position boundary
│   └── <name>/mod.rs       # one provider per language (rust, go, …)
├── privileges.rs           # root/sudo safety boundary helpers
├── process.rs              # wait_captured / wait_streaming: bounded subprocesses
├── project/
│   ├── mod.rs              # Project detection + Workspace
│   ├── create.rs           # deterministic, offline project templates
│   ├── file_tree.rs        # lazily loaded tree
│   └── tasks.rs            # zero-config run/test task detection
├── recent.rs               # recent projects and files for the welcome screen
├── regex.rs                # a small regex engine for search
├── remote.rs               # experimental command-runner seam (git only)
├── search.rs               # project-wide text search
├── session.rs              # per-project session persistence
├── settings.rs             # global user preferences + atomic JSON persistence
├── terminal/
│   ├── mod.rs              # init/restore, OSC 52 clipboard
│   ├── screen.rs           # bounded ANSI screen model for the terminal
│   └── session.rs          # PTY sessions, reader threads and resize
├── trust.rs                # trusted project roots (state directory)
└── ui/
    ├── theme.rs            # bundled themes + semantic roles (single source of truth)
    ├── art.rs              # original ASCII art: mascot, wordmark, frames
    ├── header.rs           # identity bar / breadcrumb
    ├── tabs.rs             # tab pills
    ├── editor/
    │   ├── mod.rs          # gutter, syntax, cursorline, indent guides
    │   └── tests.rs        # scroll and inline-note unit tests
    ├── file_tree.rs        # project sidebar
    ├── panel.rs            # bottom panel: Problems, Output, Debug Console
    ├── project_search.rs   # project search and replace overlay
    ├── status_bar.rs       # the Mellow statusline
    ├── overlay.rs          # palette, quick open, prompts, directory picker
    ├── welcome.rs          # the welcome home screen
    └── mod.rs              # layout + entry point
```

Language providers: `asm`, `c`, `csharp`, `css`, `dart`, `elixir`, `go`,
`html`, `java`, `json`, `kotlin`, `lua`, `markdown`, `nix`, `perl`, `php`,
`python`, `ruby`, `rust`, `shell`, `sql`, `swift`, `toml`, `web`
(TypeScript/JavaScript), `yaml`.

---

## Dependency direction

- `editor`, `filesystem`, `git`, `commands` **must not** depend on `ui` or `app`.
- `language` depends on nothing UI-related.
- `ui` may read `app` state and editor/language data, but must not contain
  editing logic.
- `app` is the conductor and may use everything.

The editor core must remain language-agnostic.

---

## Adding a language

To add a language:

1. Create `language/<name>/mod.rs` implementing `LanguageProvider`.
2. Register it in `ProviderRegistry::builtin()`.
3. That's it — detection and highlighting work automatically.

Adding a language must never require touching `editor` or `ui`.

---

## Language detection vs. language support

These are separate concerns and must stay separate:

- **Detection** (`language/detection`) consumes lightweight
  `LanguageDescriptor`s and combines signals into a `DetectionResult`
  (`language`, `confidence`, `reasons`). It never imports provider
  implementations.
- **Providers** (`language/provider` + each `language/<name>`) implement the
  `LanguageProvider` trait and *produce* those descriptors via `descriptor()`.

### Detection signals (scored)

| Signal | Weight | Notes |
| --- | --- | --- |
| Special file name | 55 | e.g. `Dockerfile` |
| Shebang | 50 | `#!/usr/bin/env …` |
| Project context | +40 | corroboration only, not a standalone decision |
| File extension | 35 | one signal among many |
| Content hints | 20 each (max 3) | lightweight, no big parser |

File-level signals (name, shebang, extension, content) decide the language.
Project markers provide a **corroborating bonus**: `Cargo.toml` plus a `.rs`
extension yields *high* confidence Rust, but `Cargo.toml` alone never turns
`README.md` into Rust. A project marker says what the project is, not what a
given file is; the workspace tracks project kind separately.

Confidence is derived from the winning score and the margin to the runner-up.
If two languages are nearly tied, confidence is downgraded rather than guessed.
