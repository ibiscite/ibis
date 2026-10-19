# Contributing to Ibis

Keep every change simple, deterministic, safe and understandable to a non-technical user. Thank you for helping.

## Ways to help, easiest first

1. **Report a problem.** Use an issue template. Say which app, operating system and output format you used. Share only invented examples, never real library records, document text or screenshots of private work.
2. **Report an app where inserting or pasting fails.** The "app compatibility" template asks exactly what we need.
3. **Add data, not code.** Destination app profiles, output templates, translations and health-check rules are data files checked by CI. (These folders arrive in later releases; see `ROADMAP.md`.)
4. **Add a fixture.** A tiny synthetic `.bib` (or other) file that reproduces a bug, in `fixtures/`, is often the most useful contribution of all. See `fixtures/README.md`.
5. **Change code.** Small, focused pull requests with tests.

## AI-assisted contributions

Welcome, under the same bar as any other change:

- Say in the pull request that AI helped and which tool.
- Keep it small and focused on one outcome.
- Include a test or fixture that fails without the change.
- All automated checks must pass. A maintainer (assisted by AI review) still decides. Agents run by the maintainer follow ADR 0015 and `AGENTS.md`.
- You are responsible for the change: you must be able to say what it does and why. Do not paste private data into AI tools while working on Ibis.

A good prompt to start from: *"Read AGENTS.md and CONTRIBUTING.md in this repository. I want Ibis to <outcome>. Propose the smallest change with a test, then implement it and run `node scripts/check.mjs`."*

## Invariants every change keeps

- Sources are read-only. Ibis never writes to Zotero, `.bib` files or any library.
- Offline by default; no telemetry. Any network feature is opt-in, explained, and recorded in `PRIVACY.md` and an ADR.
- Nothing fails silently: skipped data is reported, failed insertion falls back to copying with a reason.
- Never invent metadata. Warn about missing data instead.
- Outputs are escaped for their syntax. Templates and profiles never execute code.
- No private data in tests, logs, diagnostics, issues or commits.

## Development setup

You need [Rust](https://rustup.rs/) (the version in `rust-toolchain.toml` is selected automatically), [Node.js](https://nodejs.org/) 22 or newer and [pnpm](https://pnpm.io/) 11.25.0, pinned in `apps/desktop/package.json` (`npm install --global pnpm@11.25.0`). Rust also needs the platform's C and C++ build tools, and the desktop app needs WebKitGTK on Linux and WebView2 on Windows 10. Step-by-step setup for macOS, Linux and Windows is in "Setting up a computer" in [docs/development.md](docs/development.md#setting-up-a-computer).

```sh
git clone https://github.com/ibiscite/ibis
cd ibis
node scripts/check.mjs
```

To run the desktop app, see "Running the desktop app" in [docs/development.md](docs/development.md).

`node scripts/check.mjs` runs the checks CI runs on every change: formatting, lints, tests (including the desktop app's) and the dependency policy. CI also builds the desktop app and compares rendering with Pandoc. It needs `cargo-deny` (`cargo install --locked cargo-deny`); without it the script reports the run as incomplete rather than passed. It never edits files; run `cargo fmt` to format.

Snapshot tests live in `crates/*/tests/snapshots/`. When behaviour changes on purpose, run `cargo insta review` and review each change.

## Pull requests

- One cohesive change per pull request; describe the user-visible outcome and how you verified it.
- Sign off each commit to certify the [Developer Certificate of Origin](https://developercertificate.org/): `git commit -s`. No contributor license agreement is required or will ever be required.
- Integration changes (Zotero, citation styles, operating-system behaviour): check current upstream documentation and release notes, and test failures as well as success.
- Dependencies: add one only when it solves a concrete problem better than existing code; it must pass `cargo deny check`. Dependabot updates arrive monthly and are reviewed, not auto-merged.

Report security issues privately as described in [SECURITY.md](SECURITY.md). Everyone participating follows the [Code of Conduct](CODE_OF_CONDUCT.md).
