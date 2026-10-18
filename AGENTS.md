# Agent and contributor guidelines

These instructions apply to automated coding agents and human contributors alike. Product behaviour is described in `README.md`, `ROADMAP.md` and `docs/architecture.md`; the workflow is in `docs/development.md` and the testing strategy in `docs/testing.md`; decisions with lasting effect are in `docs/adr/`. Where documents disagree, accepted ADRs take precedence.

Everything needed to continue the work is in this repository and on GitHub: these instructions, the docs above, the issues and their milestones, and one tracking issue per milestone (label `tracking`) that records progress, decisions and open questions. Do not depend on notes kept outside the repository or in an assistant's memory. Maintainers may keep untracked private notes in `.local/`; never commit, quote or summarize them in public text.

## Commands

```sh
node scripts/check.mjs          # formatting, lints, tests and dependency policy (same as CI)
cargo test -p ibis-core         # fast core tests while editing
cargo run -p ibis -- --help     # command line tool
cargo insta review              # review changed snapshots (cargo install cargo-insta)
```

## Picking work

Work is tracked in GitHub issues grouped by milestone (see `ROADMAP.md`). Take the lowest-numbered issue labelled `status:ready` in the earliest open milestone, unless a maintainer has asked for something else. Issues labelled `status:needs-design` need a short written plan, reviewed before implementation; `status:blocked` issues wait on another issue or a maintainer decision. Each issue states its acceptance criteria and tests; a pull request closes exactly one issue (`Closes #N`). If requirements are unclear or a decision is hard to reverse, ask in the issue instead of guessing.

## Working autonomously

The maintainer runs AI coding agents on this repository and authorizes them to work without asking for each step. Within that authorization an agent may create branches, commit, push branches, open pull requests, and squash-merge **its own** pull request when:

- every required check and the desktop checks pass on the final commit;
- one independent review of the final diff (by a session or person that did not write it) has no open P1 finding and no regression introduced by the change;
- the change stays within the issue it closes.

Always ask the maintainer first about: releases (tags, the release pull request, publishing builds), licensing, privacy or network use, repository and organization settings, anything that costs money or creates accounts, and product defaults that users notice (what Enter does, the default shortcut, first-run questions, default style). Ask with a short recommendation, in the session when the maintainer is present, otherwise as a comment on the issue with the label `needs:maintainer`, and continue with other work meanwhile. Routine engineering choices are not questions: choose the standard practice and say what you chose in the pull request.

Without that authorization (for example, an outside contributor's agent), follow "Version control" below.

### The loop

1. Pick work as described above and read its issue and the milestone's tracking issue.
2. Branch `<type>/<issue>-<short-name>` from `main`; start with a failing test or fixture.
3. While editing, run the fast checks (`cargo test -p ibis-core`, `cargo test -p ibis-desktop`, `pnpm -C apps/desktop test`). Before pushing, run `node scripts/check.mjs` on the exact commit and push only if it exits 0. Report every check as passed, failed or blocked (could not run), never "passed" for a step that did not run.
4. Review once, adversarially: hostile input, other scripts and locales, huge or empty data, concurrency, platform differences, silent failure. P1 means crash, hang, data loss or a security issue; P2 means wrong or silently changed output. Fix P1s and regressions with a regression test; file other findings as issues rather than growing the pull request.
5. Do not wait idle for CI or a review: continue with the next issue on a branch stacked on the previous one, and rebase it onto `main` after the squash-merge (see `docs/development.md`).
6. After merging, add one line to the milestone's tracking issue: what is now possible, and what is next or blocked.

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
- Do not commit, push, tag, release or publish unless explicitly asked to perform that exact action, or authorized as described in "Working autonomously". Tags, releases and publishing always need an explicit request.
- Write commit messages in the imperative mood using Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`, `ci:`), describing behaviour and validation. Do not include conversation history, tool or agent names, or private context. Sign off commits (`git commit -s`).
