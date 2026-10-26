# Contributing to Ibis

Ibis is free, open source, and maintained by Diyanko, who has final say on the product and releases. Bug reports, synthetic examples, documentation and AI-assisted contributions are welcome. Start with the [tracking issue](https://github.com/ibiscite/ibis/issues?q=is%3Aopen+label%3Atracking) and [roadmap](ROADMAP.md); discuss ideas in [Discussions](https://github.com/ibiscite/ibis/discussions). Report vulnerabilities through [SECURITY.md](SECURITY.md).

## Setup

Use the Rust toolchain in `rust-toolchain.toml`, Node 22 or newer, and the pnpm version in `apps/desktop/package.json`. Install cargo-deny and actionlint for policy/workflow checks. Install the interface dependencies with `pnpm -C apps/desktop install --frozen-lockfile`.

Native builds need Xcode Command Line Tools on macOS; on Windows use the MSVC build tools and WebView2. On Debian/Ubuntu install `build-essential pkg-config libwebkit2gtk-4.1-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libayatana-appindicator3-dev patchelf`. Other platforms: [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

Use one writer per worktree. Start each branch from `origin/main`; do not stack branches. Give each worktree its own Cargo target directory. See [AGENTS.md](AGENTS.md) for authority, privacy and merge rules.

## Commands and testing

```sh
cargo test -p ibis-core                    # parser, search, citations and settings
cargo test -p ibis-desktop                 # native shell; default cargo test skips it
pnpm -C apps/desktop test                  # interface tests
pnpm -C apps/desktop typecheck
pnpm -C apps/desktop lint
pnpm -C apps/desktop build
node scripts/check.mjs --script-tests
node website/build.mjs
cargo run -p ibis -- --help
```

Start fixes with a failing regression test and invented data in `fixtures/`. Test malformed, empty and large inputs and relevant locales/platforms. Review snapshot diffs (`cargo insta review`). Never use real library data, document text, private paths or credentials. Preserve licences and bundled third-party notices when dependencies change.

The required **CI result** on the exact head validates Rust formatting/clippy/tests (including minimum Rust), desktop builds/tests on three OSes, interface checks, citation fidelity and dependency policy. Real Zotero and offline checks exercise native boundaries. UI unit tests cannot prove focus, clipboard or accessibility behaviour in native webviews. Native smoke scripts touch focus and the clipboard: use a disposable session with synthetic data. Optional Pandoc enables the citation fidelity test; CI requires it.

Run `node scripts/check.mjs` to diagnose CI or check tooling changes; it exits 1 for failure and 2 for missing tools. For clean-commit evidence use `--expect-commit FULL_SHA` in an isolated checkout. Prefer focused checks while editing.

## Commits and pull requests

Use a Conventional Commit title: `feat`, `fix`, `perf`, `docs`, `test`, `ci`, `build`, `refactor`, `chore` or `revert`, with an optional lower-case scope and `!`. Keep it at most 72 characters, lower case after the colon, and without a trailing full stop. The squash title becomes release notes through Release Please; see [RELEASING.md](RELEASING.md). Check with `node scripts/pr-lint.mjs --title "fix: keep good entries"`.

Describe what changed, why and how it was checked; link the issue with `Part of #N` or `Closes #N`. Sign off every commit (`git commit -s`, [Developer Certificate of Origin](https://developercertificate.org/)); use a public project or GitHub noreply identity and disclose AI help with an `Assisted-by:` trailer or co-author line ([ADR 0011](docs/adr/0011-ai-contributions.md)). Contributions keep their licences and notices; no contributor licence agreement is required.

Risky changes need independent review of the final head under [ADR 0027](docs/adr/0027-fast-until-1-0.md). Record an adversarial check for other changes. Fix P1/P2 findings, require green checks, and squash-merge (auto-merge once green) without bypasses. The maintainer approves stable publication; only Release Please creates tags.

## Where things live

- `crates/ibis-core`: bounded library readers, normalized references, search, citation output and settings. `crates/ibis-cli`: the command line. `crates/ibis-zotero-local`: Zotero's local API transport.
- `apps/desktop/src-tauri`: native windows, clipboard, library ownership and preferences. `apps/desktop/src`: React UI; shared components in `ui/`, feature state in `features/`. The command registry generates the TypeScript bindings.
- `fixtures/`: invented input and regression examples. Core tests compare parsing, retained warnings and rendered output; desktop tests check snapshot ownership and the Rust/TypeScript contract.
- `brand/`: canonical logos/icons and their generator. `website/`: static site. `scripts/` and `.github/workflows/`: checks, packaging and releases.
- [Architecture](docs/architecture.md), [design](docs/design.md), [compatibility](docs/compatibility.md) and [ADRs](docs/adr/README.md): behaviour and lasting decisions. [TRADEMARKS.md](TRADEMARKS.md): name/logo policy.

The code is AGPL-3.0-or-later; Ibis remains free without ads, telemetry, paid tiers or closed editions. Lasting decisions are public ADRs. As regular contributors join, the maintainer can form a maintainers group and add a second organization owner; any future funding uses a nonprofit fiscal host with public accounts.
