# Roadmap

Kemori is developed incrementally. This roadmap lists **open** work only; what has
shipped is in [CHANGELOG.md](CHANGELOG.md). It is product-oriented: every item
states the user-facing goal first, then how we will know it is done.

The product direction lives in [docs/PRODUCT.md](docs/PRODUCT.md). Read it
before adding an item here; if an item conflicts with those principles, the
principles win.

The guiding goal:

> Install Kemori. Open a project. Start coding.

```
editor foundation  →  project/language detection  →  language intelligence  →  automatic tooling
      ✅ done                  ✅ done                    ✅ done                 ⏳ now
```

---

## Now

The work that most directly makes Kemori feel like the zero-configuration IDE it
claims to be.

### 1. Trustworthy, unattended provisioning

**Goal.** Installing a language's tooling succeeds without the user's
intervention on any supported platform, or fails with a precise, actionable
reason — and never leaves a half-installed tool behind.

**Done (0.1.x).** A digest-keyed cache reuses verified downloads offline and
never bypasses verification; downloads and extractions are atomic per step and a
failed replacement restores the previous install; `install()` pre-flights the
plan and refuses an unrunnable plan or a short disk before downloading; failures
name the cause and the tool; Hex is pinned and rebar3 is checksum-verified by
Kemori; fake-download tests cover cache hit/miss/corruption, mismatch, failed
download, staging cleanup and failed-promotion restore.

**Remaining.**
- `rustup component add` is kept as the one documented delegation: rustup chooses
  the component for the user's active toolchain and verifies toolchain artifacts
  against its signed manifests; Kemori does not fetch or pin it independently.
  Justified in `docs/DECISIONS.md` and `docs/LIMITATIONS.md`.
- Rollback is per `Extract` step: a plan that installs A and fails on B may
  leave A. Decide whether a whole-plan transaction is worth it.

**Acceptance for the remaining work.** No floating install input without a
documented reason; a multi-step plan leaves no misleading partial state and
reports what was installed; an offline attempt explains the cache/network state.

**Direction.** Pre-flight the whole plan; make the cache the primary artifact
store; treat a multi-step plan as a unit only if the rename-based promotion can
be extended safely.

**Depends on:** nothing. This is the foundation of the whole vision.

### 2. A complete supported-language matrix

**Goal.** Every language Kemori knows has an explicit, honest contract, and every
limitation is deliberate and documented.

**Done (0.1.x).** [docs/LANGUAGE-SUPPORT.md](docs/LANGUAGE-SUPPORT.md) is the
authoritative contract and matrix: offline capabilities, server pin and
verification, runtime prerequisite, platform limits, class and verification
status for every language. PHP and Ruby are decided as Complete-with-a-
prerequisite (no verified portable runtime exists), and the Elixir, Swift,
Assembly and clangd platform limits are scoped. A test fails when a registered
language has no matrix row.

**Remaining.**
- Keep the matrix in sync as servers and platforms change.
- Add format/run/test/debug checks to the contract so "Complete" means the
  development loop works, not only that a server attaches.

**Direction.** The in-app **Language Support…** view now reads the same support
classes as the matrix; keep it generated from one source rather than growing a
second hand-written table.

**Depends on:** item 1 (so gaps can be closed, not just documented).

### 3. In-app Settings and themes

**Goal.** A user can change the handful of things that matter without ever
editing a file.

**Done (0.1.x).** A keyboard-driven **Settings** screen (opened from the
palette) changes theme, line numbers, animations, soft wrap, indentation width,
spaces vs tabs, inline diagnostics and automatic completion. Each change applies
immediately and can be reset to the defaults; a settings file is written on quit
only once something changed, so a fresh install still needs no file. The store
lives under Kemori's state directory, tolerates a missing or malformed file (the
broken file is preserved as `settings.json.corrupt`), and ignores unknown
fields. Four bundled themes — **Mellow** (the default), a high-contrast dark
**Midnight**, a light **Daylight** and the 16-colour **Ansi** fallback — are
data-driven from one role set and selectable at runtime.

**Remaining.**
- **Keybindings are intentionally not user-configurable.** Kemori keeps a
  conventional, keyboard-first keymap; a not-yet-existing arbitrary keybinding
  editor is out of scope by design, not by omission.
- The light theme has deterministic render coverage but has not been viewed on a
  real light terminal (see [LIMITATIONS.md](docs/LIMITATIONS.md)); a 16-colour /
  `NO_COLOR` fallback belongs with item 4.
- Users cannot yet add their own themes (by design for now).

**Direction.** A settings store under Kemori's state directory, a small set of
typed preferences, and a TUI that writes it. The file is an implementation
detail, never a requirement.

**Depends on:** nothing.

### 4. Terminal compatibility

**Goal.** Kemori is readable and fully usable on the terminals and setups people
actually use.

**Done (0.2).** Colour support is detected once at startup from `NO_COLOR`,
`COLORTERM` and `TERM`, and every rendered colour routes through the theme. A
16-colour terminal gets the bundled **Ansi** theme unless the user stored a
theme choice; `NO_COLOR` suppresses every colour code, keeping emphasis through
bold modifiers, and selection highlights fall back to reversed text so menus,
the file tree and text selections stay usable. Theme-level tests cover both
depths.

**Acceptance, remaining.**
- `Ctrl+Shift` chords: when the kitty keyboard protocol is unavailable Kemori
  falls back to the command palette for those commands; the fallback is
  documented in `docs/LIMITATIONS.md`. (Interactive verification under `tmux`
  is still pending.)
- Kemori is legible on a light terminal background: **Daylight** has
  deterministic render coverage but has not been viewed on a real light
  terminal.

**Direction.** Detect capability once at startup and expose a fallback keymap;
route colour through the theme so a fallback theme can be chosen.

### 5. First-run and project setup

**Goal.** The first five minutes explain Kemori and get a project working in one
step.

**Done (0.1.x).** Opening a project runs a bounded, `.gitignore`-aware scan for
its languages, plans each one (ready / needs install / needs a prerequisite /
unavailable), and offers a single **Project setup** summary with a
**Set up project** action and a **Later** dismissal. Selecting it installs every
missing managed tool through the existing provisioning path, one at a time, and
reports installed / ready / failed / needs-attention. Already-ready tooling is
never reinstalled, an independent language still succeeds if another fails, and
the per-language offer is suppressed for languages the project plan covers.
`Set Up Project…` in the palette opens it on demand.

**Remaining.**
- The welcome screen does not yet preview that Kemori will set up tooling.

**Direction.** Reuse language detection and the provisioning queue; batch the
offers per project instead of per language.

**Depends on:** items 1 and 2.

### 6. Packaging polish

**Goal.** Install Kemori without a Rust toolchain, and update it easily.

**Done (0.1.x).** `cargo install kemori` works from crates.io; a `Makefile`
installs to `/usr/local` (or a staged `PREFIX`/`DESTDIR`); AUR packages (`kemori`
and `kemori-git`) install `/usr/bin/kemori`; and the release workflow attaches
`.deb`/`.rpm` packages plus checksummed prebuilt tarballs. The README install
section covers every path and the `sudo`/`secure_path` behaviour.

**Remaining.**
- A one-line installer (`curl … | sh`) installs the right prebuilt binary and
  can update it.
- The README install section stays the single authoritative source.

**Depends on:** the release workflow (shipped in 0.1.0).

### 7. Run and test tasks

**Done (0.2).** `Run Task…` in the palette lists the commands the project's
markers imply — `cargo build`/`test`, `go build`/`test`, `make`,
`python3 -m pytest`, `mix test` and the declared `npm` scripts (lifecycle hooks
skipped) — and runs the chosen one in the project root on the background worker
with a deadline, a bounded output cap and no shell. The captured result stays in
the bottom panel's **Output** tab with the exit verdict. `Up`/`Down` move a row
cursor and `Enter` jumps to the `file:line[:column]` reference on the selected
line, opening the file at that position; the cwd the task ran in anchors relative
paths.

**Remaining.** More project kinds can be added where they have an unambiguous
zero-config command (`pom.xml` → `mvn test`, `pubspec.yaml` → `flutter test`,
`.csproj` → `dotnet build`) — one branch in the detection module each. Task
output is still a completed batch: live streaming, cancel, rerun and history are
part of the core IDE loop below.

**Depends on:** nothing.

### 8. The core IDE loop

**Goal.** Run, test, search and navigate without leaving Kemori.

**Done (3.x).** A bottom panel (`Ctrl+J`) holds **Problems**, **Output** and
**Debug Console** tabs with their own state. Problems gathers diagnostics across
the open files, groups them by file and jumps to the source; task output streams
live, can be cancelled or rerun, keeps a session history and adds its
`file:line` references to Problems; debug-adapter messages land in their tab
instead of a modal; the panel resizes and leaves the editor readable on a short
terminal. Project-wide search and replace (`Ctrl+Shift+F`) supports literal or
regex queries, case/word toggles, include/exclude globs, grouped results, a
replacement preview, atomic writes that skip unsaved buffers, and immediate
reload of clean open files. The full execution plan and acceptance criteria are
in [NEXT.md](NEXT.md) Phase 1.

**Remaining.**
- An integrated terminal (PTY), with sessions, copy/paste, resize and signals.
- Run Active File for standalone scripts.
- Workspace-wide Problems and LSP pull diagnostics.
- A workspace-trust gate before running project tasks, debug adapters or
  project-specific servers.

**Depends on:** nothing.

---

## Next

Valuable, but not what defines the first impression.

- **Managed-tool lifecycle.** *Goal:* keep provisioned tools current safely.
  *Acceptance:* Update re-resolves the pinned version, an optional auto-update
  preference exists, and Remove never touches a user/system install.
- **Detection confirmation.** *Goal:* resolve genuinely ambiguous files.
  *Acceptance:* a low-confidence file asks once and remembers the answer.
- **Performance budgets.** *Goal:* Kemori stays fast on real files.
  *Acceptance:* budget tests assert startup time and opening a 50 MB file, and
  regressions fail CI.
- **Descriptor-driven provisioning.** *Goal:* add a language without new
  bespoke branches. *Acceptance:* a language's tooling is declared as data; the
  editor, UI and provisioning code need no per-language changes.

---

## Later

Real, but deferred; none of them is required for the core promise.

- **Real multi-root workspace.** Persisted roots with one unified tree, search
  and problems list; today Kemori switches between projects and keeps each
  session, but does not present several roots at once.
- **Interactive diff/merge** for conflicting files.
- **Remote development over SSH.**
- **Debug adapter support.** The current command is an explicit handshake preview
  only (see [docs/LIMITATIONS.md](docs/LIMITATIONS.md#debugging)).
- **Highlighting engine comparison.** Measure the hand-written tokenizers
  against tree-sitter and record a recommendation in
  [docs/DECISIONS.md](docs/DECISIONS.md); do not migrate without an explicit OK.
- **Plugin/provider API.** Only with a design that cannot bloat Kemori; see the
  non-goals.

---

## Very late (end of project)

Deliberately deferred. These are not forgotten work for the next phase; they are
cleanup for after the migration window has clearly closed, potentially at the
very end of the project.

- **Remove legacy Koda compatibility.** Delete the one-time
  `~/.local/state/koda` importer, its `.migrated-from-koda` marker, and the
  `KODA_REMOTE` / `KODA_DEBUG_ADAPTER` fallbacks. *Acceptance:* the 3.x
  migration window is over (not before 4.0), no runtime code reads the old path
  or variables, and the removal is recorded in `CHANGELOG.md`. Historical
  changelog entries, old release notes and immutable crates.io versions stay as
  they are; only the compatibility code goes.
- **Retire the old `koda-ide` deprecation shim** from any future tooling or
  packaging references once its redirect value is gone.

---

## Non-goals

These are out of scope by design, not merely unscheduled:

- Recreating VS Code in a terminal, or a panel-heavy layout.
- A plugin marketplace or a general extension platform.
- Requiring a configuration file — or its absence — for a core feature.
- Supporting every language; breadth over depth.
- Replacing the user's package manager or system tools.
- Windows support, unless the architecture makes it essentially free (it does not).
- Bundling whole toolchains where a smaller, verified upstream download exists.

See [docs/PRODUCT.md](docs/PRODUCT.md#non-goals) for the longer statement.

---

## Completed

Shipped work lives in [CHANGELOG.md](CHANGELOG.md). Highlights: the rope editor
and UI, language detection and built-in providers, the LSP client and
server-backed features, pinned and verified tool provisioning with Language
Setup, git integration, session persistence, CI, and prebuilt release binaries.
2.1 added code folding, git gutter markers and hunk application, read-only
non-UTF-8 opening with explicit conversion, project switching with session
restore, incremental LSP sync, partial mouse support and file icons.
3.0 renamed the project to **Kemori**, fixed the Phase 0 correctness and
responsiveness gaps, and published `kemori` 3.0.0 on crates.io, docs.rs and
GitHub; the old `koda-ide` crate is deprecated.
