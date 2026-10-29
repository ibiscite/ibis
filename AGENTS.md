# Agent rules

Ibis is one shortcut to cite from your library in any app. Read only what your task needs:
scope in [ROADMAP.md](ROADMAP.md), structure in [docs/architecture.md](docs/architecture.md),
look and behaviour in [docs/design.md](docs/design.md), testing and coverage in
[docs/testing.md](docs/testing.md), lasting decisions in
[docs/decisions.md](docs/decisions.md). Current work and owners: the tracking issue (label `tracking`).

```sh
cargo fmt --all && cargo clippy --all-targets --locked && cargo test --locked
cargo test -p ibis-desktop --locked              # the app's Rust side; plain cargo test skips it
pnpm -C apps/desktop test && pnpm -C apps/desktop typecheck
node scripts/check.mjs --script-tests      # only when scripts/ changed
node scripts/pr-lint.mjs --title "fix: explain the change"
```

## Start here

- Everything needed to resume lives in the repository and on GitHub, never only in a chat, a tool's memory or a local file. Start from the tracking issue: **Plan** gives the order, **Now** what is in flight and who owns it. Run `git fetch origin --prune` and `git status`; keep work you did not make.
- Take the first unclaimed Plan item whose prerequisites are merged. Before editing, read Now and recent claims, check open pull requests for conflicts, and claim paths and mutable-state owners with a draft pull request that says `Closes #N`, or a one-line comment on the tracking issue naming the branch.
- Until 1.0, keep the process light and the code easy to change: the roadmap, the tracking issue and one plain issue per piece of work. Add no labels, automation, templates, boards or process documents unless they save time right away.
- When the maintainer approves a plan or answers a question, record it the same day in the file it changes (`ROADMAP.md`, `docs/` or `docs/decisions.md`) and link it from the tracking issue.

## Parallel work

- One branch and one worktree per pull request, from `origin/main`; no stacking, never push to `main`. Give each worktree its own Cargo target directory.
- One writer per set of paths and mutable-state owners; serialize overlapping work. If yours overlap, finish or review that pull request instead of starting a second.
- Files everyone touches (`ROADMAP.md`, `docs/design.md`, `docs/decisions.md`, `tokens.css`, `App.tsx`) get small edits that merge quickly.
- At most two active implementation PRs repository-wide touching picker, copy, paste, IPC or settings state. Engine work against agreed contracts and docs, research and review can run in parallel. Split a draft over about 1,500 changed lines.

## Building

- Before code, write executable scenarios for success, refusal, failed I/O, delayed A after B and close/reopen; assert the final outcome a person sees. Build the smallest slice meeting the issue's "Done when", including cancellation and stale-result correctness before merge; defer features, not invariants. Delete what you replace. Until 1.0, replace wrong behaviour: no compatibility code or settings migrations.
- Debugging: reproduce each bug in a failing regression test, then fix the cause. After two identical check failures, fix the cause or narrow the reproduction without weakening the required outcome. After three CI rounds on one blocker, cut scope and write the diagnosis on the issue.
- Each async operation has one owner and an immutable identity; stale completion cannot consume newer state. Front ends never reinterpret engine intent.
- UI: keep it simple and on the design system. Changes go through `apps/desktop/src/tokens.css`, a shared component in `apps/desktop/src/ui/` or `docs/design.md`, never one screen on its own. When the maintainer says something looks or behaves wrong, fix it and write the rule into `docs/design.md`. Visual fine-tuning waits until close to 1.0.
- Performance: the budgets in `docs/architecture.md` are limits, not goals. Say on the pull request what a change does to speed and memory when it could affect either. Nothing polls while Ibis is hidden; a check may poll only while its screen is shown.
- Use invented data only. No real library data, document text, private paths or credentials in source, tests, logs or public text. Never publish `.local/`.
- Keep licences and notices; dependencies must pass `cargo deny`. Logos and icons come only from `brand/` (`python3 brand/build.py`).

## Merging

- Pull request titles are Conventional Commits; they become the release notes. Say what changed and how it was checked, and link `Closes #N`.
- Sign off commits (`git commit -s`) with a public project or GitHub noreply identity, and disclose AI help with an `Assisted-by:` or `Co-authored-by:` trailer.
- Review named invariants and changed interactions. Changes to these risky areas need one review of the final head by a session that did not write the change, with every P1 and P2 fixed: the updater, release signing and CI permissions; settings and other files Ibis writes; the clipboard, escaping and pasting; window permissions, native input and the shortcut portal; BibTeX and Zotero reading, citation syntaxes and formatting; untrusted input, network access and security boundaries. Anything else needs your own adversarial check. Findings below P2 go into one 1.0 issue per area, not new issues.
- Merge only with a green "CI result" on the exact head, by squash, never bypassing. Auto-merge sometimes does not fire: if a green pull request is still open, squash-merge it yourself. GitHub runs no pull request checks while a pull request has a merge conflict, so a head with no "CI result" usually needs `origin/main` merged in; never merge a head that has no CI run. If `main` turns red, revert the merge that broke it first, then fix.
- After a merge, update the tracking issue's Now if it changed, then remove your own worktree and local branch for it. Lasting decisions go into `docs/decisions.md`.

2026-10-29: Before final review and CI of a shared contract change, merge current `origin/main`; verify that reviewed head still contains it immediately before squash. Focus local Vitest checks with `pnpm -C apps/desktop exec vitest run FILE --maxWorkers=1` (`pnpm test --` runs the full suite).

## Brief for an implementing agent

```text
Outcome / non-goals:
Owned state / reserved paths / conflicting PRs:
Canonical contract and invariants:
Scenarios first: success; refusal; failed I/O; delayed A after B; close/reopen
Consumer oracle (final observable outcome) / required platform evidence:
Done when / checks:
Reviewer: attack stale ownership, fail-open uncertainty, lost meaning,
private-data propagation and changed interactions.
```

## Asking the maintainer

Agents have standing engineering and merge authority. Ask the maintainer only about licensing, new network use, repository or organization settings, money or accounts, what a release contains, or work needing their hands: once, as a yes/no with your recommendation. Never ask again for something already authorized. Stable releases wait for the maintainer's one-click approval; nightlies publish on their own.
