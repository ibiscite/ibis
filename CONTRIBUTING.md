# Contributing to Ibis

Code contributions, problem reports, invented test examples and documentation
fixes are welcome.
See [current work](https://github.com/ibiscite/ibis/issues?q=is%3Aopen+label%3Atracking)
and [ROADMAP.md](ROADMAP.md).

## How Ibis is built

AI agents write most code, tests and docs under the maintainer's direction.
Automated checks and reviews supplement people testing the app. Reports from your
actual writing setup matter; no response time is promised.

Agents may read public issues and draft replies. Describe problems with
invented data and discuss large changes first. See [help](README.md#help-and-feedback)
or [private security reporting](SECURITY.md).

## How decisions are made

Diyanko, the maintainer, decides Ibis's scope and releases.
[Decisions](docs/decisions/) and the [roadmap](ROADMAP.md) are public.
Ideas outside Ibis's scope are welcome as separate projects built on the CLI,
`ibis serve` or MCP; see the [engine contract](docs/cli.md#output-for-scripts-and-agents).

## Places to start

You can help without knowing the engine:

- Invented compatibility fixtures for unusual `.bib`, RIS and CSV files.
- Translations when the message catalogue invites a second locale.
- Documentation and website fixes.
- Citation styles: contribute them at [citationstyles.org](https://citationstyles.org/).

## Setup

Use `rust-toolchain.toml`, Node 26 (the current LTS; 22.13 or newer works) and the pnpm version in
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

We review code submitted from another repository and re-apply it on a branch
here. This lets us use this repository's trusted checks, since a pull request
can change the workflow that reports its own result
([decision 0034](docs/decisions/0034-outside-input-is-data.md)). We credit your
work with a `Co-authored-by` trailer and thank you in the release notes.

Follow [AGENTS.md](AGENTS.md). A bug fix starts with a failing regression.
Run the focused check; `node scripts/check.mjs --plan` shows the CI classes
and `node scripts/check.mjs` runs local checks for your changes.

Use a Conventional Commit title, at most 72 characters, with lower case after
the colon. Name branches `type/short-description`. Explain the problem,
resulting behaviour, validation and remaining limits. Sign off commits
(`git commit -s`, [DCO](https://developercertificate.org/)) and disclose AI
assistance. Contributions retain their licences; there is no contributor
licence agreement. Ibis is [AGPL-3.0-or-later](LICENSE).
