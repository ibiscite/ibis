# Contributing to Ibis

Problem reports, invented test examples and documentation fixes help.
See [current work](https://github.com/ibiscite/ibis/issues?q=is%3Aopen+label%3Atracking)
and [ROADMAP.md](ROADMAP.md).

## How Ibis is built

Diyanko is the sole maintainer and decides product scope and releases.
AI agents write most code, tests and docs under that direction. Automated
checks and reviews supplement people testing the app. Reports from your
actual writing setup matter; no response time is promised.

Agents may read public issues and draft replies. Outside pull requests need
the maintainer's approval; they do not auto-merge. Describe problems with
invented data and discuss large changes first. See [help](README.md#help-and-feedback)
or [private security reporting](SECURITY.md).

## Setup

Use `rust-toolchain.toml`, Node 22+ and the pnpm version in
`apps/desktop/package.json`:

```sh
pnpm -C apps/desktop install --frozen-lockfile
cargo run -p ibis -- --help
pnpm -C apps/desktop tauri dev
node website/build.mjs
```

Native builds need Xcode Command Line Tools on macOS, or MSVC build tools and
WebView2 on Windows. Debian/Ubuntu needs `build-essential pkg-config
libwebkit2gtk-4.1-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev
libayatana-appindicator3-dev patchelf`. Other systems:
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

## Where things live

[Architecture](docs/architecture.md#crates-and-clients) maps crates and app
modules. `fixtures/` holds invented data, `brand/` generates logos/icons and
`website/` holds the site. [Testing](docs/testing.md) lists checks;
[RELEASING.md](RELEASING.md) describes builds and publication.

## Pull requests

Follow [AGENTS.md](AGENTS.md). A bug fix starts with a failing regression.
Run the focused check; `node scripts/check.mjs --plan` shows the CI classes
and `node scripts/check.mjs` runs local checks for your changes.

Use a Conventional Commit title, at most 72 characters, with lower case after
the colon. Name branches `type/short-description`. Explain the problem,
resulting behaviour, validation and remaining limits. Sign off commits
(`git commit -s`, [DCO](https://developercertificate.org/)) and disclose AI
assistance. Contributions retain their licences; there is no contributor
licence agreement. Ibis is [AGPL-3.0-or-later](LICENSE).
