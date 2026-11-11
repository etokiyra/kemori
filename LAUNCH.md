# Launch runbook — Kemori

Working document for the first launch. Delete it when it stops being useful; it
is not part of the product.

---

## The pitch

> **Kemori is a terminal IDE that installs its own language servers.**

Supporting line:

> Open a project. It detects the language, fetches the server from a verified
> source, checks the hash, and configures it. No config file, no plugin
> marketplace, no dotfiles.

Lead with that everywhere. "Zero-config terminal editor" is a claim Helix, micro
and Kakoune also make, and you lose that comparison on maturity. "It sets up its
own toolchain, and verifies what it downloads" is a claim almost nobody makes,
and it is true of Kemori today.

Never lead with "an IDE in your terminal" alone — to anyone who has not used it,
that reads as "worse VS Code".

## Goal

| Metric | Today | 30 days |
| --- | --- | --- |
| GitHub stars | 1 | 50+ |
| crates.io downloads | 49 | 500+ |
| Release downloads | ~6 | 100+ |
| Issues/PRs from strangers | 0 | 3+ |
| People who say they switched | 0 | 1+ |

If one person says "I replaced X with this", the launch worked.

## Ready

- [x] One-line installer, published and tested from the real URL
- [x] Demo GIF in the README, in the repo, on `master`
- [x] README leads with the demo and the differentiator
- [x] Repo description, topics and homepage set
- [x] Three `good first issue`s from documented limitations
- [x] Pinned feedback thread
- [x] Site serves `/kemori/install.sh`

Remaining, and only you can do them:

- [ ] Set the GitHub **social preview** (Settings → Social preview → upload the
      first frame of `docs/images/demo.gif`). It is what appears on every share.
- [ ] Decide the account you will post from. Use your real one. A brand-new
      account posting a project as its first act gets filtered.
- [ ] Have `docs/LIMITATIONS.md` open in a tab. You will need it.

## Schedule

Launch day is **Tuesday 17 November 2026**. 15:00 CET is 09:00 US Eastern, which
is when HN gets its traffic.

| When | What | Where |
| --- | --- | --- |
| Tue 17, 14:45 | Final check, then submit | https://news.ycombinator.com/submit |
| Tue 17, 15:00–18:00 | Stay in the thread | HN |
| Tue 17, 18:30 | Short post + GIF | Mastodon / Bluesky |
| Thu 19, 15:00 | Full post | r/rust |
| Mon 23, 15:00 | Short post + GIF | r/commandline |
| Wed 25 | Link post | lobste.rs (needs an invite) |
| Fri 27 | Submissions | see below |
| Week of 1 Dec | Point release with the launch fixes | GitHub |

One channel per day. Never the same text twice.

## Day 0: the three hours that decide it

1. Submit the repo URL, title `Show HN: Kemori – a terminal IDE that installs its own language servers`.
2. Paste the body below as the first comment immediately, so the thread opens
   with context rather than an empty link.
3. Stay on the page. Reply to everything, fast.
4. Do not edit the title. Do not resubmit if it stalls. Do not ask anyone to
   upvote — HN detects it and it is the one thing that gets a project banned.
5. When the first "this doesn't work on my machine" arrives, reproduce it or
   open an issue in the same reply. Visible responsiveness is worth more than
   the star count.

### Show HN body (first comment)

> I write a lot of code in a terminal, and the part that always annoyed me was
> the setup: install the editor, then install each language server, then wire
> them into a config file, then do the whole thing again on the next machine.
>
> Kemori is my attempt to delete that step. Open a project and it detects the
> languages, downloads the matching server from a verified source, checks the
> hash, and configures it — rust-analyzer, gopls, clangd, a JDK for Java and
> Kotlin, ElixirLS, debugpy and others. A missing server is one prompt, not a
> project. If a language has no server, built-in providers keep diagnostics,
> completion, hover and navigation working offline.
>
> Rust + ratatui, MPL-2.0, Linux and macOS, on crates.io. One line to try:
>
> `curl -fsSL https://kiyra.lol/kemori/install.sh | sh`
>
> Things I would genuinely like feedback on:
>
> - Is auto-downloading toolchains the right default, or should it always ask?
>   It pulls tens of MB per language.
> - What breaks on your projects? The editor is real but young, and
>   `docs/LIMITATIONS.md` is deliberately blunt about what does not work yet.
>
> This is the first time I am posting it anywhere, so criticism is more useful
> to me than praise.

## Replies you will need

Answer in one or two sentences, then move on. Long defensive replies lose
threads.

**"Why not Helix/Neovim?"**
> Both are excellent and more mature than Kemori. The difference is what I
> wanted: you install Helix and then install and configure the servers
> yourself. Kemori fetches and verifies them for you. If you already have a
> working Helix setup, you probably do not need this.

**"Another terminal editor?"**
> Fair. It is not trying to be a better editor than the ones that exist — it is
> trying to remove the setup step. That is the whole thesis.

**"Downloading binaries automatically is a security problem."**
> That is the right instinct, so every managed download is checksum- or
> signature-verified before use, versions are pinned, and the trust model is
> documented in SECURITY.md. It never writes to system directories. If you would
> rather not, built-in providers work with no download at all.

**"Windows?"**
> Not yet, and LIMITATIONS.md says so rather than pretending otherwise. The
> provisioning layer assumes a Unix shell.

**"Does it do X?"**
> Check LIMITATIONS.md — if it is not listed as working, assume it does not yet.
> It is short and honest.

**"Is this AI-generated?"**
> I am a solo developer and I used AI tooling while building it, the same way I
> use a compiler. The design decisions, the constraints in AGENTS.md and the
> limitations document are mine.

Then stop typing and go fix something.

## Other channels

**Mastodon / Bluesky**

> I released Kemori 3.2.1 — a terminal IDE that installs its own language
> servers. Open a project, it fetches and verifies rust-analyzer / gopls /
> clangd / debugpy and configures them. No config file, no plugins.
> Rust + ratatui, MPL-2.0, Linux + macOS.
>
> `curl -fsSL https://kiyra.lol/kemori/install.sh | sh`

**r/rust** — title: `Kemori: a terminal IDE in Rust that fetches and verifies its own language servers`. Body: the architecture angle, not the marketing angle. Mention Rust 2024, ratatui, the rope, the LSP client, and the provisioning design. Flair it correctly and read the sidebar's self-promotion rule before posting.

**r/commandline** — one paragraph, the GIF inline, the install line, one question. This sub reads screenshots, not architecture.

**lobste.rs** — title `Kemori: a terminal IDE that installs its own language servers`, tags `editors, rust, linux, release`. Needs an invite; ask in a Rust community rather than buying one.

**Newsletters** (all free, all take submissions; do these after the posts, not before)

- Console.dev — https://console.dev/submit
- Terminal Trove — https://terminaltrove.com (submit via the site; it blocks curl)
- This Week in Rust — https://this-week-in-rust.org (they take PRs to the next issue)

**Awesome lists** — one PR each, no more, and only where Kemori genuinely fits:

- https://github.com/rothgar/awesome-tuis
- https://github.com/ratatui/awesome-ratatui

## The two weeks after

- Reply to every issue and comment. Response speed is the difference between a
  spike and a community.
- Ship **3.3.0** with the fixes the launch surfaced and post it in the same
  threads. This is the proof that the project is alive, and it matters more
  than the launch itself.
- Write one "what I learned launching my terminal IDE" post with the real
  numbers, including the disappointing ones. That post reaches people the
  launch did not.

## Never

- Buy stars or ask friends to upvote. A dead launch is better than a banned
  account.
- Post the same text to five subreddits. Rewrite per community.
- Argue with criticism. Every critical comment is a free bug report.
- Claim anything `docs/LIMITATIONS.md` says is missing. One person who finds out
  the hard way costs you the thread.
- Launch a second project before this one has answered its first ten issues.
