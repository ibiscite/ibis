# Agent and contributor guidelines

These instructions apply to automated coding agents and human contributors alike. Product behaviour is described in `README.md`, `ROADMAP.md` and `docs/architecture.md`; the workflow is in `docs/development.md`; decisions with lasting effect are in `docs/adr/`. Where documents disagree, accepted ADRs take precedence.

Maintainers may keep untracked notes in `.local/`. Read `.local/README.md` if it exists. Never commit, quote or summarize its contents in public text (code, docs, commit messages, issues or pull requests).

## Commands

```sh
node scripts/check.mjs          # formatting, lints, tests and dependency policy (same as CI)
cargo test -p ibis-core         # fast core tests while editing
cargo run -p ibis -- --help     # command line tool
cargo insta review              # review changed snapshots (cargo install cargo-insta)
```

## Architecture rules

- `crates/ibis-core` contains all behaviour that can be tested without a window. It must not depend on Tauri, any GUI toolkit or the network.
- Front ends (CLI, desktop app, future integrations) stay thin. Logic needed by two front ends belongs in the core.
- Sources are read-only. Never write to Zotero, `.bib` files or any user library.
- Record identity is source plus native key, never a citation key alone.
- Nothing fails silently. Skipped data becomes a `LoadWarning`; a failed insertion becomes "copied" with a reason; invalid input is rejected with a clear error rather than guessed.
- No network access without an accepted ADR. Network features are opt-in and listed in `PRIVACY.md`.
- Templates, profiles and styles are data and never execute code.

## Making changes

1. Start every bug fix with a failing test or a new fixture in `fixtures/` that reproduces it.
2. Keep each change small and focused on one outcome, ideally under about 300 changed lines.
3. Prefer maintained libraries to new code and fewer dependencies to more. Dependencies must pass `cargo deny check`.
4. Update documentation and snapshots in the same change, and review snapshot diffs like code.
5. Decisions that are hard to reverse (license, persisted data formats, public CLI or API contracts, network use, security boundaries) require an ADR.
6. Run `node scripts/check.mjs` before reporting work as complete, and report its result accurately.

## Privacy and security

- Never commit or print real reference data, document text, clipboard contents, credentials, personal paths or usernames. Fixtures are synthetic.
- Diagnostics use an allowlist and never include raw errors or payloads that might contain library content.
- Escape every output for its target syntax (LaTeX, Pandoc, Typst, HTML, RTF).
- Local endpoints bind to 127.0.0.1, require a token and reject browser `Origin` headers.

## Version control

- Check `git status` before editing and preserve existing work. Never reset, restore, delete or reformat unrelated changes.
- Do not commit, push, tag, release or publish unless explicitly asked to perform that exact action.
- Write commit messages in the imperative mood using Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`, `ci:`), describing behaviour and validation. Do not include conversation history, tool or agent names, or private context. Sign off commits (`git commit -s`).
