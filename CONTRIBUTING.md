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

## Development setup

Follow the [architecture principles](docs/architecture.md#principles) for every
change. Tool versions, platform prerequisites and setup instructions live in
[docs/development.md](docs/development.md#setting-up-a-computer). From the repository
root, run the complete check:

```sh
node scripts/check.mjs
```

The check never edits files. A missing required tool reports an incomplete run;
see [testing.md](docs/testing.md) for focused tests and verification limits.
To start the app, see [Running the desktop app](docs/development.md#running-the-desktop-app).

Snapshot tests live in `crates/*/tests/snapshots/`. When behaviour changes on purpose, run `cargo insta review` and review each change.

## Pull requests

- One cohesive change per pull request; describe the user-visible outcome and how you verified it.
- Sign off each commit to certify the [Developer Certificate of Origin](https://developercertificate.org/): `git commit -s`. No contributor license agreement is required or will ever be required.
- Integration changes (Zotero, citation styles, operating-system behaviour): check current upstream documentation and release notes, and test failures as well as success.
- Dependencies: add one only when it solves a concrete problem better than existing code; it must pass `cargo deny check`. Dependabot updates arrive monthly and are reviewed, not auto-merged.

Report security issues privately as described in [SECURITY.md](SECURITY.md). Everyone participating follows the [Code of Conduct](CODE_OF_CONDUCT.md).
