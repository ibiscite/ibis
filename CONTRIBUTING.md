# Contributing to Ibis

Ibis is free, open source, and maintained by Diyanko, who has final say on the product and releases. Bug reports, synthetic examples, documentation and AI-assisted contributions are welcome. See what is being worked on in the [tracking issue](https://github.com/ibiscite/ibis/issues?q=is%3Aopen+label%3Atracking) and the [roadmap](ROADMAP.md); discuss ideas in [Discussions](https://github.com/ibiscite/ibis/discussions). Report vulnerabilities through [SECURITY.md](SECURITY.md). The rules for every change, by people or agents, are in [AGENTS.md](AGENTS.md).

## Setup

Use the Rust toolchain in `rust-toolchain.toml`, Node 22 or newer, and the pnpm version in `apps/desktop/package.json`, then run `pnpm -C apps/desktop install --frozen-lockfile`. Native builds need Xcode Command Line Tools on macOS, or the MSVC build tools and WebView2 on Windows. On Debian/Ubuntu install `build-essential pkg-config libwebkit2gtk-4.1-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libayatana-appindicator3-dev patchelf`; other platforms: [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). `node scripts/check.mjs` runs everything CI runs that your machine can (it exits 2 when a tool is missing).

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

Start a fix with a failing test and invented data. Use a Conventional Commit title (`feat`, `fix`, `perf`, `docs`, `test`, `ci`, `build`, `refactor`, `chore`, `revert`; at most 72 characters, lower case after the colon), sign off every commit (`git commit -s`, [DCO](https://developercertificate.org/)), and say if AI helped. Contributions keep their licences; there is no contributor licence agreement.

The code is AGPL-3.0-or-later; Ibis stays free, without ads, telemetry, paid tiers or closed editions.
