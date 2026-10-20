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

Take the first open, untaken item for your role in the **Queue** at the top of the milestone's tracking issue (label `tracking`), and say in its issue that you took it. A red `main` comes first: revert the merge that broke it, then fix it in a new pull request. A pull request closes exactly one issue (`Closes #N`); `status:needs-design` issues need a short plan reviewed before code. Resolve engineering choices from evidence and the recorded scope; ask only about the reserved decisions below. File new findings with `needs:triage`. Labels, titles and milestones follow "Issues, labels and milestones" in `docs/development.md`. Acting for the maintainer, agents manage issues and pull requests: triage findings against the agreed release scope, set milestones, maintain the queue and decisions, post independent reviews, close completed issues and remove merged branches. These routine GitHub updates do not need another approval.

## Working autonomously

The maintainer delegates engineering, security, testing, CI and documentation decisions to agents acting for Ibis (ADR 0015). This is standing authorization to branch, commit, push, open pull requests and merge their own pull requests. Agents may also accept ADRs that record behaviour already agreed in the current milestone's tracking issue; no new approval is needed for those decisions.

Squash-merge pull requests; their Conventional Commit title becomes the commit on `main`.

**Merge only when** all required checks pass on the final commit and an independent review of that exact commit is recorded on the pull request with every P1 and P2 fixed (filing them as issues does not count). Pull requests that change only documentation, or only tests and synthetic fixtures, need no independent review: merge them when the required checks pass and your own adversarial check (step 4 of the loop) is recorded on the pull request (ADR 0017). Keep changes focused and reviewable. Never use `--admin` or another bypass. The maintainer lets agents decide versions and when to publish a release (tracking issue decisions, 2026-10-20); only Release Please creates tags. Every merge to `main` rebuilds the open release pull request and restarts its checks, so while a release is going out (announced as a hold on the tracking issue), nobody merges to `main` until that release pull request has merged and the hold is lifted.

**Review** is done by another session, preferably another model, and posted as a pull request comment starting `Independent review of <full SHA> by <tool and model>`, with verdict, findings and checks run. It covers only that commit (a rebase with an identical tree keeps it). After a small fix for its findings, the reviewer re-checks only the changed lines and records that, naming the new commit; a large fix or a design change needs a full new review.

**Limits:** the tracking issue's Queue is split into lanes that never touch the same files; each lane has one writer with at most two pull requests in progress (drafts) and one level of stacking. Ready pull requests waiting for review don't count, so never wait idle: while one waits, take the next item in your lane or another lane. If five ready pull requests are waiting for review, prioritize completing their independent reviews before starting more work. If a check fails twice for the same reason, stop blind retries, diagnose the failure and record the result in the issue. When an agent makes the same mistake twice, add one short line here or, better, a check in `scripts/check.mjs`, and remove lines that no longer matter. Use `needs:maintainer` only for a reserved decision or something requiring the maintainer's own hands. No new workflows, harnesses, ADRs or dependencies unless the issue asks for them.

Release authority and merge holds follow [Working autonomously](#working-autonomously). Ask the maintainer only about licensing, new network use, repository or organization settings, anything that costs money or creates accounts, what goes into or out of a release, and anything requiring the maintainer's own hands. Ask once, in plain words, with a recommendation and a one-word answer such as yes/no. Do not ask again for authorization already given. Record reserved decisions in the issue and tracking issue.

Choose product defaults that users notice (for example the shortcut) from evidence and state the choice in the current milestone's tracking issue so the maintainer can overrule it. Routine engineering, security, testing, CI and documentation choices are not permission questions.

Without that authorization (for example, an outside contributor's agent), follow "Version control" below.

For meaningful interface changes, include before/after screenshots using synthetic data for the relevant operating systems and light/dark appearances, label the source commit and browser or native capture, and explain any applicable views still missing.

### The loop

1. Take the next queue item; read its issue and the tracking issue.
2. Branch `<type>/<issue>-<short-name>` from `main`, in your own clone or worktree (never share a checkout). Start with a failing test or fixture.
3. Run fast checks while editing (`cargo test -p ibis-core`, `cargo test -p ibis-desktop`, `pnpm -C apps/desktop test`) and `node scripts/check.mjs` on the exact commit before asking for review or merging. Push unfinished work early as a draft pull request (no full check needed), so nothing lives on one machine only, and mark it ready once the check passes. Report each check as passed, failed or blocked, never "passed" for a step that did not run.
4. Review once, adversarially: hostile input, other scripts and locales, huge or empty data, concurrency, platforms, silent failure. P1 = crash, hang, data loss, security; P2 = wrong or silently changed output. Fix P1, P2 and regressions with a test; file P3s as issues.
5. Open the pull request with "How it works" in plain words, for a maintainer who did not write the code. Get the independent review (step 4 is your own review, not that one). Then merge if allowed above, or add `needs:review` and take the next item.
6. After merging, check `main` is green and add one plain line to the tracking issue: what users can now do, what is next.
7. Before stopping or switching machines, push everything and update **Now** in the tracking issue. Nothing needed to continue lives on one machine only.

## Architecture rules

Follow the [architecture principles](docs/architecture.md#principles) and the
contracts documented with each component.

## Making changes

1. Start every bug fix with a failing test or a new fixture in `fixtures/` that reproduces it.
2. Keep each change small and focused on one outcome, ideally under about 300 changed lines.
3. Prefer maintained libraries to new code and fewer dependencies to more. Dependencies must pass `cargo deny check`. Before adding one, confirm on its registry page and repository that it exists under that exact name and is maintained, because AI tools can suggest names that do not exist or belong to look-alike packages.
4. Update documentation and snapshots in the same change, and review snapshot diffs like code.
5. Decisions that are hard to reverse (license, persisted data formats, public CLI or API contracts, network use, security boundaries) require an ADR.
6. Run `node scripts/check.mjs` before reporting work as complete, and report its result accurately.

## Privacy and security

- Follow the [privacy, escaping and endpoint principles](docs/architecture.md#principles).

## Version control

- Mark breaking changes with `!` (for example `feat!:`) or a `BREAKING CHANGE:` footer.
- Check `git status` before editing and preserve existing work. Never reset, restore, delete or reformat unrelated changes.
- Do not commit, push, tag, release or publish unless explicitly asked to perform that exact action, or authorized as described in [Working autonomously](#working-autonomously).
- Never push to a `claude/*` branch; create `<type>/<issue>-<name>` from `main`, one branch per pull request.
- Write commit messages in the imperative mood using Conventional Commits (`feat:`, `fix:`, `perf:`, `docs:`, `test:`, `ci:`, `build:`, `refactor:`, `chore:`, `revert:`), describing behaviour and validation. Do not include conversation history or private context. Commits written with an AI tool end with an `Assisted-by: <tool and model>` trailer (or the tool's own co-author line), as ADR 0011 requires disclosure. Sign off commits (`git commit -s`).
