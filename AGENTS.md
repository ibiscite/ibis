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

Take the first open, untaken item for your role in the **Queue** at the top of the milestone's tracking issue (label `tracking`), and say in its issue that you took it. A red `main` comes first. A pull request closes exactly one issue (`Closes #N`); `status:needs-design` issues need a short plan reviewed before code. If something is unclear or hard to reverse, ask in the issue instead of guessing. Don't add issues to milestones or the queue: file findings with the label `needs:triage`.

## Working autonomously

The maintainer runs AI coding agents here (ADR 0015). Implementing agents build; the maintainer, or a reviewing agent acting for the maintainer, orders the queue and reviews. Agents may branch, commit, push and open pull requests.

**Merge your own pull request only if** it is a small (about 300 lines, excluding lockfiles, generated files and snapshots) fix, test, fixture or doc correction that adds no feature and changes no interface, default or persisted format (correcting wrong output is fine); it touches only `crates/`, `apps/desktop/src/`, `apps/desktop/src-tauri/src/`, `fixtures/` or `docs/` (not `docs/adr/`, `docs/development.md` or `docs/testing.md`); all required checks pass; and an independent review is recorded with every P1 and P2 fixed (filing them as issues doesn't count). Otherwise add `needs:review` and move on. Never use `--admin` or another bypass; never create tags.

**Review** is done by another session, preferably another model, and posted as a pull request comment starting `Independent review of <full SHA> by <tool and model>`, with verdict, findings and checks run. It covers only that commit (a rebase with an identical tree keeps it).

**Limits:** at most three open agent pull requests in total and one level of stacking; at the limit, address review feedback or stop and update **Now**. If a check fails twice for the same reason, or you need a decision listed below, comment on the issue, add `needs:maintainer` and take the next item. No new workflows, harnesses, ADRs or dependencies unless the issue asks for them.

Always ask the maintainer first about: releases (tags, the release pull request, publishing builds), licensing, privacy or network use, repository and organization settings, anything that costs money or creates accounts, and product defaults that users notice (what Enter does, the default shortcut, first-run questions, default style). Ask with a short recommendation, in the session when the maintainer is present, otherwise in the issue with `needs:maintainer`. Routine engineering choices are not questions: choose the standard practice and say so in the pull request.

Without that authorization (for example, an outside contributor's agent), follow "Version control" below.

### The loop

1. Take the next queue item; read its issue and the tracking issue.
2. Branch `<type>/<issue>-<short-name>` from `main`, in your own clone or worktree (never share a checkout). Start with a failing test or fixture.
3. Run fast checks while editing (`cargo test -p ibis-core`, `cargo test -p ibis-desktop`, `pnpm -C apps/desktop test`) and `node scripts/check.mjs` on the exact commit before pushing. Report each check as passed, failed or blocked, never "passed" for a step that did not run.
4. Review once, adversarially: hostile input, other scripts and locales, huge or empty data, concurrency, platforms, silent failure. P1 = crash, hang, data loss, security; P2 = wrong or silently changed output. Fix P1, P2 and regressions with a test; file P3s as issues.
5. Open the pull request with "How it works" in plain words, for a maintainer who did not write the code. Get the independent review (step 4 is your own review, not that one). Then merge if allowed above, or add `needs:review` and take the next item.
6. After merging, check `main` is green and add one plain line to the tracking issue: what users can now do, what is next.
7. Before stopping or switching machines, push everything and update **Now** in the tracking issue. Nothing needed to continue lives on one machine only.

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
- Write commit messages in the imperative mood using Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`, `ci:`), describing behaviour and validation. Do not include conversation history or private context. Commits written with an AI tool end with an `Assisted-by: <tool and model>` trailer (or the tool's own co-author line), as ADR 0011 requires disclosure. Sign off commits (`git commit -s`).
