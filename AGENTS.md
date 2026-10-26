# Agent and contributor guidelines

These rules apply to coding agents and human contributors alike. **How we work** (issues, labels, milestones, the tracking issue, branches, commits, pull requests, review and merge) is the [contribution standard](docs/development.md#contribution-standard); follow it exactly. Product behaviour is in `README.md`, `ROADMAP.md`, `docs/architecture.md` and `docs/design.md`; testing is in `docs/testing.md`. Lasting decisions are in `docs/adr/`, and accepted ADRs win where documents disagree.

Everything needed to continue is in this repository and on GitHub: these rules, the docs, the issues and the one tracking issue (label `tracking`). Do not rely on notes outside it or on an assistant's memory. `.local/` holds untracked maintainer notes: never commit, quote or summarize them in public text.

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

Under the maintainer's standing authorization (ADR 0015), agents acting for Ibis make the engineering, security, testing, CI and documentation decisions. They branch, commit, push, open pull requests and merge their own when the merge rule is met. They triage issues against the agreed scope, manage labels, milestones, pull requests and the tracking issue, post independent reviews, close completed issues and remove merged branches. They may accept ADRs that record behaviour already agreed in the tracking issue, and decide versions and when to release; only Release Please creates tags, and a stable release publishes only after the maintainer approves its deployment in GitHub (one click). Nightly builds publish on their own. None of this needs another approval.

Ask the maintainer **only** about licensing, new network use, repository or organization settings, anything that costs money or creates accounts, what goes into or out of a release, and anything needing the maintainer's own hands. Ask once, in plain words, with a recommendation and a one-word answer; label the issue `status:needs-maintainer` and list it under "Waiting on the maintainer". Never ask again for authorization already given. Defaults users notice (such as the shortcut) are chosen from evidence and stated in the tracking issue so the maintainer can overrule them; engineering choices are not asked.

The maintainer owns the product and its look. When they say something looks, reads or behaves wrong, fix it and write the rule into `docs/design.md`, `ROADMAP.md` or a check in the same pull request, so it never needs saying twice.

Without this authorization (for example an outside contributor's agent), do not commit, push, tag, release or publish unless your user asks for that exact action.

Anyone (Claude, Codex, another tool or a human) can do any part of the work. Whoever merges is the lead; a reviewer is a session that did not write the change.

## The loop

1. **Pick** the first untaken item in your lane of the tracking issue's Queue, and say in its issue that you took it. A red `main` comes first: when the required "CI result" fails on `main`, revert the merge whose own run first turned it red, then fix it in a new pull request. A failing job that is not required never triggers a revert or a queue hold by itself: record it in the issue CI opened for it, and if it fails twice on `main` for the same reason, treat it as a P2 bug.
2. **Branch** `<type>/<issue>-<short-name>` from `origin/main` in your own worktree; never share a checkout. Start with a failing test or a fixture in `fixtures/`.
3. **Build** one user-visible feature per pull request, with its tests, docs and closely related small fixes; split only when parts can ship on their own. Before opening it, have another model research, attack and try to break the branch, and fix what it finds. Use explicit working directories, inspect the diff before testing, and update docs and snapshots in the same change (review snapshot diffs like code).
4. **Check** with the fast commands and focused tests. The full check is the required "CI result" on the exact head, then the merge queue; run `node scripts/check.mjs` locally only to diagnose a failure or for what CI cannot cover. Report each step as passed, failed or blocked, never "passed" for something that did not run.
5. **Attack your own change once:** hostile, huge or empty input, other scripts and locales, concurrency, each platform, silent failure. P1 = crash, hang, data loss, security; P2 = wrong or silently changed output. Fix P1, P2 and regressions with a test; file P3s as one follow-up issue.
6. **Open** the pull request with the template. Push unfinished work early as a draft so nothing lives on one machine; mark it ready once the check passes.
7. **Review and merge** by the [review rule](docs/development.md#review-and-merge): code needs an independent review of the exact head with every P1 and P2 fixed (filing them does not count); docs-only or tests-only changes need only your recorded step 5 (ADR 0017). Squash-merge when all required checks pass. Never use `--admin` or another bypass.
8. **Close the loop:** check "CI result" is green on `main` and keep the tracking issue's Now and Queue current. Comment there only for decisions and handoffs. Keep other comments under about 150 words; reviews and evidence may be longer. State results plainly, including what did not run.

Reviews follow the [free-first procedure](docs/development.md#free-first-review-tools): never paid plans, credits, overages or automatic bot fixes. The CodeRabbit trial ends **2026-11-06**; the free-only policy continues afterward.

## Pausing and resuming

- **Before you stop**, push every branch; unfinished work goes up as a draft whose body says what is left. Update Now in the tracking issue. Leave nothing that exists only on one machine or in one assistant's memory.
- **To resume**, read this file, the tracking issue and the open pull requests, then `git fetch origin --prune`. Finish open pull requests first. Trust GitHub's current state over any summary.
- **Start prompt:** "Read AGENTS.md and follow it: resume from the tracking issue and work its Queue."

## Limits

- Work in the tracking issue's lanes; lanes never edit the same files, and each has one writer.
- At most two open implementation pull requests per lane, counting those waiting on review or checks. Finish before starting; finish drafts rather than rebuilding them.
- A check that fails twice for the same reason: stop retrying, record the diagnosis in the issue and take the next item.
- Native flakes get one issue and at most one open pull request. Do not grow `scripts/smoke-native.py` unless the issue asks for a new check.
- No stacking: every branch starts from `main`, because stacked squash merges have silently reverted changes. After merging `main` into a branch, `git diff origin/main` must show only your change.
- No new workflows, harnesses, ADRs or dependencies unless the issue asks. Prefer maintained libraries and fewer dependencies. Dependencies pass `cargo deny check`; confirm the exact name on its registry and repository first (AI tools suggest look-alikes).
- Hard-to-reverse decisions (license, persisted formats, public CLI or API contracts, network use, security boundaries) need an ADR.
- When an agent repeats a mistake, add a check to `scripts/check.mjs` or `scripts/pr-lint.mjs` (preferred) or one line here; remove lines that no longer matter.
- Locate unfamiliar files with `rg --files` before reading or invoking them.
- Use a separate Cargo target directory per worktree, or clean `ibis-core`, `ibis-zotero-local` and `ibis-desktop` before reusing one.

## Code, privacy and security

Follow the [architecture principles](docs/architecture.md#principles), including privacy, escaping and endpoints, and each component's contract. No real library data, document text, paths or credentials in code, tests, logs, issues or pull requests.

- A change to the picker, its keys or its styles updates the website demo (`website/demo.js`) in the same pull request.
- UI changes edit `apps/desktop/src/tokens.css`, a shared component in `apps/desktop/src/ui/`, or `docs/design.md`; never style one screen on its own. Fix a visual bug in the rule or component that allowed it, with a check.
- Logos and icons come only from `brand/`: change `settings.json` or `master.svg`, run `python3 brand/build.py`, and never edit generated files by hand.

## Version control safety

- Run `git status` before editing; never reset, restore, delete or reformat work you did not make.
- Never push to `main` or a `claude/*` branch. One branch per pull request.
- For AI-assisted or co-authored maintainer commits, set `git -c user.email=diyanko@ibiscite.org commit -s` explicitly and inspect author, committer and trailers before pushing; only the public project or GitHub noreply identities are allowed.
- Link evidence only from URLs GitHub returned for that commit. Before treating an empty diff as equality, confirm the pathspec matches files.
- Commits follow the [standard](docs/development.md#commits): Conventional Commits, signed off (`git commit -s`), with an `Assisted-by:` trailer or co-author line when AI helped (ADR 0011). Never put conversation history, pause notes or private context in commits, pull requests or issues.
