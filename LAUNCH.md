# Launch plan — Kemori

A working document for the first real launch. Delete it whenever it stops being
useful; it is not part of the product.

Current state: **1 star, 49 crates.io downloads, 1–2 downloads per release
asset.** That is the "nobody has been told this exists" number, not a
"the product is bad" number. The first launch is not meant to go viral. It is
meant to move from zero users to the first ten, and from "I built this" to
"other people use this".

---

## 1. The pitch

One sentence, used everywhere:

> **Kemori is a terminal IDE that installs its own language servers.**

Supporting line:

> Open a project. It detects the language, fetches the server from a verified
> source, checks the hash, and configures it. No config file, no plugin
> marketplace, no dotfiles.

That is the differentiator. "Zero-config terminal editor" is a claim Helix,
micro and Kakoune also make, and you lose that comparison on maturity. "It sets
up its own toolchain, verified" is a claim almost nobody makes, and it is true
of Kemori today. Lead with it everywhere.

Never lead with "an IDE in your terminal" alone — it reads as "worse VS Code"
to anyone who has not used it.

## 2. Goal

| Metric | Today | 30 days after launch |
| --- | --- | --- |
| GitHub stars | 1 | 50+ |
| crates.io downloads | 49 | 500+ |
| Release asset downloads | ~6 | 100+ |
| Issues/PRs from strangers | 0 | 3+ |
| People who say they switched | 0 | 1+ |

If one person says "I replaced X with this", the launch worked. Stars are a
side effect.

## 3. Before posting anything

- [ ] Push `scripts/install.sh` to `etokiyra.github.io/site/kemori/install.sh`
      so `https://kiyra.lol/kemori/install.sh` resolves.
- [ ] Test `curl -fsSL https://kiyra.lol/kemori/install.sh | sh` in a clean
      container (`docker run --rm -it debian:12 sh`), with no Rust installed.
- [ ] Commit `docs/images/demo.gif` — the README needs it on `master`.
- [ ] Set the repo website field to `https://kiyra.lol/kemori` (or the site).
- [ ] Upload the first frame of the GIF as the GitHub **social preview**
      (Settings → Social preview). It is what shows on every share.
- [ ] Open 4–6 `good first issue`s so visitors see a way in.
- [ ] Pin one issue: "What would make you switch from your current editor?"
- [ ] Re-read `docs/LIMITATIONS.md` and make sure the README is as honest as it
      is. Overclaiming is the fastest way to lose a Show HN thread.

## 4. Timing

Post on a **Tuesday, Wednesday or Thursday, 08:00–10:00 US Eastern**
(14:00–16:00 CET). Avoid Mondays (backlog) and Fridays (nobody reads).

Suggested schedule, one thing per channel, never cross-posted in the same hour:

| Day | Channel | Why |
| --- | --- | --- |
| Tue | Show HN | Biggest reach, most critical, needs you free for 3 hours |
| Tue | Mastodon / Bluesky | Cheap, no rules, good for the GIF |
| Thu | r/rust | Rust audience cares about the implementation |
| +3 days | r/commandline | Workflow audience, screenshot-driven |
| +5 days | lobste.rs | Needs an invite; small but high-quality traffic |
| +7 days | Newsletters | Console.dev, Terminal Trove, This Week in Rust |
| +10 days | awesome-list PRs | Slow compounding traffic, not a launch |

Do not post to five subreddits on day one. Each sub has regulars who notice,
and a burst of identical posts reads as spam.

## 5. Drafts

### Show HN

**Title**

> Show HN: Kemori – a terminal IDE that installs its own language servers

**Body**

> I write a lot of code in a terminal, and the part that always annoyed me was
> the setup: install the editor, then install each language server, then wire
> them into a config file, then do the whole thing again on the next machine.
>
> Kemori is my attempt to delete that step. Open a project and it detects the
> languages, downloads the matching server from a verified source, checks the
> hash, and configures it — rust-analyzer, gopls, clangd, a JDK for Java and
> Kotlin, ElixirLS, debugpy, and others. A missing server is one prompt, not a
> project. If a language has no server, built-in providers keep diagnostics,
> completion, hover and navigation working offline.
>
> It is Rust + ratatui, MPL-2.0, Linux and macOS, on crates.io, and you can try
> it with one line:
>
> `curl -fsSL https://kiyra.lol/kemori/install.sh | sh`
>
> What I would like feedback on:
>
> - Is auto-downloading toolchains the right default, or should it always ask
>   first? It pulls tens of MB per language.
> - What breaks on your projects? The editor is real but young, and
>   docs/LIMITATIONS.md is deliberately blunt about what does not work.
>
> I am a solo developer and this is the first time I am posting it anywhere, so
> honest criticism is more useful to me than praise.

**Rules for the thread:** be present for at least three hours. Answer every
substantive comment. Never argue; "you're right, that's a bug, filed as #123"
is the strongest possible reply. Do not ask anyone to upvote — HN detects it.

### r/rust

**Title**

> Kemori: a terminal IDE in Rust that fetches and verifies its own language
> servers

**Body**

> I've been building a terminal IDE in Rust for a while and finally released it
> properly. The interesting part (to me) is the language layer: detection,
> providers and the LSP lifecycle are first-class subsystems rather than
> editor + plugin.
>
> Every managed tool is downloaded from a known source and verified before use
> — checksums for prebuilt bundles, signatures for the Swift toolchain,
> pinned versions when delegating to a package manager. Missing tools are
> planned and installed in one step, and everything lives under Kemori's own
> data directory instead of touching the system.
>
> Rust 2024, ratatui, rope-backed editor, MPL-2.0. Linux and macOS.
>
> Repo: … Demo GIF in the README. I'd especially like feedback on the
> provisioning design and anything that makes it painful to package for a
> distro.

Check r/rust's self-promotion rules before posting (they allow project posts,
but read the sidebar; flair it correctly).

### r/commandline

**Title**

> Kemori — a terminal IDE that sets up its own language servers

**Body**

Short. One paragraph, the GIF inline, the install line, one question. This sub
responds to screenshots and workflow, not architecture.

### Mastodon / Bluesky

> I released Kemori 3.2.1 — a terminal IDE that installs its own language
> servers. Open a project, it fetches and verifies rust-analyzer / gopls /
> clangd / debugpy and configures them. No config file, no plugins.
> Rust + ratatui, MPL-2.0, Linux + macOS.
>
> `curl -fsSL https://kiyra.lol/kemori/install.sh | sh`
>
> [GIF]

### lobste.rs

Title: `Kemori: a terminal IDE that installs its own language servers`
Tags: `editors`, `rust`, `linux`, `release`
URL: the repo. Lobste.rs needs an invite; if you do not have one, ask in a Rust
community rather than buying one.

## 6. Afterwards, which is the part most people skip

- Reply to **every** issue and comment for two weeks. Speed of response is the
  difference between a spike and a community.
- Ship a point release within 2–3 weeks containing the fixes the launch
  surfaced. Announce it in the same threads. This is what proves the project is
  alive.
- Write one short "what I learned launching my terminal IDE" post with the real
  numbers, including the disappointing ones. That post will reach people the
  launch did not.
- Add a `## Who uses Kemori` line to the README only when it is true.

## 7. Never do

- Buy stars, or ask friends to upvote. HN and Reddit both detect it, and a
  dead-looking launch is better than a banned account.
- Post the same text to five subreddits. Rewrite per community.
- Argue with criticism. Every critical comment is a free bug report.
- Claim "IDE" features that `docs/LIMITATIONS.md` says are missing. One person
  who finds out the hard way costs you the thread.
- Launch a second project before this one has answered its first ten issues.
