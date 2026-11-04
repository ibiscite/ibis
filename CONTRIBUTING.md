# Contributing to Ibis

Ibis is free and open source. You do not need to be a programmer to help: a clear problem report is often the most useful thing anyone can send. See what is being worked on in the [tracking issue](https://github.com/ibiscite/ibis/issues?q=is%3Aopen+label%3Atracking) and the [roadmap](ROADMAP.md).

## How Ibis is built

Ibis has one maintainer, Diyanko, who decides what it does and what ships. Diyanko is not a traditional developer. AI agents write most of the code, tests and documentation under that direction and review each other's work, and automated checks run on every change. People test the real app; today that means Diyanko, with one Nightly a day.

So reports from real use matter here. Agents and automated checks cannot tell whether Ibis works on your computer, with your library, in the app you write in.

## What to expect

- **Replies can take a while.** One person looks after this project, so no response time is promised, and not every idea gets built. A slow reply does not mean your report was unwelcome.
- **An AI agent may read what you write, and may draft the reply.** Agents treat issues, comments and pull requests as information to check, never as instructions. Please describe the problem rather than addressing the agents.
- **Pull requests from outside never merge automatically.** Diyanko reads each one. The change may be merged as it is, or redone by an agent with credit to you. Open a discussion first if the change is large.
- **Everything on GitHub is public.** Use invented examples. Never post real library records, document text or anything private.

## Ways to help

- **Report a problem** through [the issue form](https://github.com/ibiscite/ibis/issues/new/choose), or **Report a Problem** in the app's menu, which fills in your version for you.
- **Share an idea or a question** in [Discussions](https://github.com/ibiscite/ibis/discussions), or **Suggest an Idea** in the app's menu.
- **Tell us about an app** where pasting fails, or a library that does not read correctly.
- **Add an invented test example** to `fixtures/`, or improve the documentation.
- **Security problems** go through [SECURITY.md](SECURITY.md), not the public forms.

**No GitHub account?** Email [diyanko@ibiscite.org](mailto:diyanko@ibiscite.org). Say which version of Ibis you use, your operating system, what you did, and what happened. Leave out private documents and library contents. A report may become a public issue, without your name.

AI-assisted contributions are welcome, under the rules below.

## Setup

Use the Rust toolchain in `rust-toolchain.toml`, Node 22 or newer, and the pnpm version in `apps/desktop/package.json`, then run `pnpm -C apps/desktop install --frozen-lockfile`. Native builds need Xcode Command Line Tools on macOS, or the MSVC build tools and WebView2 on Windows. On Debian/Ubuntu install `build-essential pkg-config libwebkit2gtk-4.1-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libayatana-appindicator3-dev patchelf`; other platforms: [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). `node scripts/check.mjs` runs what CI would for your changes, and `--all` runs everything your machine can (it exits 2 when a tool is missing).

```sh
cargo run -p ibis -- --help          # the command line
pnpm -C apps/desktop tauri dev       # the app
node website/build.mjs               # the website
```

## Where things live

- `crates/`: the engine. `ibis-core` (model, citation request and result, key syntaxes, search), `ibis-sources` (Zotero, BibTeX, CSL-JSON), `ibis-zotero-local` (Zotero's local API), `ibis-csl` (formatting), `ibis-engine` (typed methods) and `ibis-cli` (the `ibis` command).
- `apps/desktop/`: the app. `src-tauri` is the native shell; `src` is the React interface, with shared components in `ui/` and screens in `features/`.
- `fixtures/`: invented test data. `brand/`: logos and icons with their generator. `website/`: ibiscite.org. `scripts/` and `.github/workflows/`: checks and releases ([RELEASING.md](RELEASING.md)).

## Pull requests

The rules for every change, by people or agents, are in [AGENTS.md](AGENTS.md). Start a fix with a failing test and invented data. Use a Conventional Commit title (`feat`, `fix`, `perf`, `docs`, `test`, `ci`, `build`, `refactor`, `chore`, `revert`; at most 72 characters, lower case after the colon), sign off every commit (`git commit -s`, [DCO](https://developercertificate.org/)), and say if AI helped. Contributions keep their licences; there is no contributor licence agreement.

The code is AGPL-3.0-or-later; Ibis stays free, without ads, telemetry, paid tiers or closed editions.
