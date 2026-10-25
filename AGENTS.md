# Agent and contributor guidelines

These rules apply to coding agents and human contributors alike. Product behaviour is in `README.md`, `ROADMAP.md` and `docs/architecture.md`. **How we work** (issues, labels, milestones, tracking issues, branches, commits, pull requests, review and merge) is the [contribution standard](docs/development.md#contribution-standard); follow it exactly. Testing is in `docs/testing.md`. Lasting decisions are in `docs/adr/`; where documents disagree, accepted ADRs win.

Everything needed to continue the work is in this repository and on GitHub: these rules, the docs above, the issues and their milestones, and one tracking issue per active milestone (label `tracking`). Do not rely on notes outside it or on an assistant's memory. `.local/` holds untracked maintainer notes: never commit, quote or summarize them in public text.

## Commands

```sh
node scripts/check.mjs          # everything CI checks: formatting, lints, tests, dependency policy
cargo test -p ibis-core         # fast core tests
cargo test -p ibis-desktop      # desktop crate tests (plain cargo test skips them)
pnpm -C apps/desktop test       # interface tests
cargo run -p ibis -- --help     # command line tool
cargo insta review              # review changed snapshots (cargo install cargo-insta)
node scripts/pr-lint.mjs --title "…" --branch "…" --body-file pr.md   # check a pull request before opening it
```

## Authority

The maintainer delegates engineering, security, testing, CI and documentation decisions to agents acting for Ibis (ADR 0015). Under that standing authorization agents branch, commit, push, open pull requests and merge their own pull requests when the merge rule below is met. Acting for the maintainer they also manage issues, labels, milestones and pull requests: triage against the agreed release scope, keep the tracking issue's Queue and Decisions, post independent reviews, close completed issues and remove merged branches. Agents may accept ADRs that record behaviour already agreed in the current tracking issue, and decide versions and when to publish a release; only Release Please creates tags. None of these routine updates needs another approval.

Ask the maintainer **only** about: licensing, new network use, repository or organization settings, anything that costs money or creates accounts, what goes into or out of a release, and anything needing the maintainer's own hands. Ask once, in plain words, with a recommendation and a one-word answer such as yes/no; label the issue `status:needs-maintainer` and list it under "Waiting on the maintainer" in the tracking issue. Do not ask again for authorization already given. Product defaults users notice (for example the shortcut) are chosen from evidence and stated in the tracking issue so the maintainer can overrule them. Engineering choices are resolved from evidence and the recorded scope, not asked.

Without this authorization (for example an outside contributor's agent), do not commit, push, tag, release or publish unless your user asks for that exact action.

## Roles

Any agent (Claude, Codex or another tool) or human can do any part of the work under the same rules. Whoever merges is the lead. A reviewer is a session that did not write the change.

## The loop

1. **Pick.** Take the first untaken item in your lane of the current tracking issue's Queue and say in its issue that you took it. A red `main` comes first: when the required "CI result" check fails on `main`, revert the merge whose own run first turned it red, then fix it in a new pull request. A failing job that is not required never triggers a revert or a queue hold by itself; record it in the issue CI opened for it, and if it fails twice on `main` for the same reason, treat it as a P2 bug.
2. **Branch** `<type>/<issue>-<short-name>` from `origin/main` in your own worktree; never share a checkout. Start with a failing test or a fixture in `fixtures/`.
3. **Build** in small steps, under about 300 changed lines including tests. Use an explicit working directory or absolute paths, and inspect the diff before testing. Update docs and snapshots in the same change; review snapshot diffs like code.
4. **Check** with the fast commands and the focused tests for what you changed. The full check is the required "CI result" on the exact head, then the merge queue's complete validation; run `node scripts/check.mjs` locally only to diagnose a failure or for what CI cannot cover. Report each step as passed, failed or blocked, never "passed" for something that did not run.
5. **Attack your own change once:** hostile, huge or empty input, other scripts and locales, concurrency, each platform, silent failure. P1 = crash, hang, data loss, security; P2 = wrong or silently changed output. Fix P1, P2 and regressions with a test; file P3s as one follow-up issue.
6. **Open** the pull request with the template. Push unfinished work early as a draft so nothing lives on one machine, and mark it ready once the check passes.
7. **Review and merge.** Code needs an independent review of the exact head commit by another session, preferably another model, with every P1 and P2 fixed (filing them as issues does not count). The independent code review comment starts `Independent review of <full SHA> by <tool and model>` and states its verdict and P1/P2 findings. Documentation-only, or tests and synthetic fixtures only, need only your recorded step 5 (ADR 0017). Squash-merge when all required checks pass on that commit. Never use `--admin` or another bypass. When the merge queue is enabled, the release pull request merges like any other; until then, nobody merges to `main` while a release hold is announced on the tracking issue.
8. **Close the loop.** Check "CI result" is green on `main` and keep the tracking issue body current (Now and Queue). Comment on the tracking issue only for decisions and handoffs. Keep other issue and pull request comments under about 150 words; reviews and evidence may be longer. State results plainly, including what did not run.

## Review tools

Follow the [free-first review procedure](docs/development.md#free-first-review-tools) on every change: use free CodeRabbit selectively, keep Codex code/security reviews on demand, and retain the independent exact-head P1/P2 review and required CI. The CodeRabbit trial ends **2026-11-06**; the free-only policy continues afterward. Do not enable paid plans, credits, overages or automatic bot fixes/reviews to retain trial features or bypass a limit. Agents triage findings and report real issues, fixes, verification and uncertainty without waiting for maintainer reminders.

## Pausing and resuming

All state lives on GitHub, so any agent can stop at any point and any other can continue.

- **Before you stop**, push every branch; unfinished work goes up as a draft pull request whose body says what is left. Update Now in the tracking issue body. Leave nothing that only exists on one machine or in one assistant's memory.
- **To resume**, read this file, the tracking issue and the open pull requests, then run `git fetch origin --prune`. Finish open pull requests first: fix red checks, answer reviews, merge. Trust GitHub's current state over any earlier summary.
- **Start prompt** (any agent): "Read AGENTS.md and follow it: resume from the tracking issue and work its Queue."

## Limits

- Work in lanes listed in the tracking issue's Queue; lanes never edit the same files, and each lane has one writer.
- At most two draft pull requests at a time across all agents; a pull request waiting only on review or checks does not count. Finish drafts rather than closing and rebuilding them.
- A check that fails twice for the same reason: stop retrying, record the diagnosis in the pull request's issue and take the next item; a red `main` is still fixed first.
- Native flakes get one issue and at most one open pull request at a time. A pull request must not grow `scripts/smoke-native.py` unless its issue asks for a new check.
- No stacking: every branch starts from `main`, because stacked squash merges have silently reverted changes on `main`. After merging `main` into a branch, check `git diff origin/main` shows only your change.
- No new workflows, harnesses, ADRs or dependencies unless the issue asks for them. Prefer maintained libraries to new code and fewer dependencies to more. Dependencies must pass `cargo deny check`; before adding one, confirm on its registry page and repository that the exact name exists and is maintained (AI tools suggest look-alike names).
- Hard-to-reverse decisions (license, persisted data formats, public CLI or API contracts, network use, security boundaries) need an ADR.
- When an agent repeats a mistake, add a check to `scripts/check.mjs` or `scripts/pr-lint.mjs` (preferred) or one line here; remove lines that no longer matter.
- Before reading or invoking an unfamiliar repository file, locate it with `rg --files`; do not guess its path.
- Use a distinct Cargo target directory per source worktree, or clean `ibis-core`, `ibis-zotero-local` and `ibis-desktop` before reusing one; stale cross-worktree artifacts can hide source changes.

## Code, privacy and security

Follow the [architecture principles](docs/architecture.md#principles), including privacy, escaping and endpoints, and the contract documented with each component. No real library data, document text, paths or credentials in code, tests, logs, issues or pull requests.

The website demo (`website/demo.js`) mirrors the app: a change to the picker, its keys or its styles updates the demo in the same pull request ([design](docs/design.md)).

UI changes edit `apps/desktop/src/tokens.css`, a shared component in `apps/desktop/src/ui/`, or `docs/design.md`; never style one screen on its own. Fix a visual bug in the rule or component that allowed it, with a check.

Logos and icons come only from `brand/`. Change `settings.json` or `master.svg`, run `python3 brand/build.py`, and never edit generated files by hand.

## Version control safety

- Run `git status` before editing; never reset, restore, delete or reformat work you did not make.
- Link evidence only from URLs GitHub returned for that commit. Before treating an empty diff as equality, confirm the pathspec matches files.
- Never push to `main` or a `claude/*` branch. One branch per pull request.
- Commits follow the [standard](docs/development.md#commits): Conventional Commits in the imperative mood, `!` or a `BREAKING CHANGE:` footer for breaking changes, signed off (`git commit -s`), and, when an AI tool helped, an `Assisted-by: <tool and model>` trailer or the tool's co-author line (ADR 0011). Never put conversation history, pause notes or private context in commits, pull requests or issues.
