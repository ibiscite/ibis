# Agent rules

Ibis is one shortcut to cite from your library in any app. Read only what your task needs:
scope in [ROADMAP.md](ROADMAP.md), structure in [docs/architecture.md](docs/architecture.md),
look and behaviour in [docs/design.md](docs/design.md), lasting decisions in
[docs/decisions.md](docs/decisions.md). Current work and owners: the tracking issue (label `tracking`).

```sh
cargo fmt --all && cargo clippy --all-targets --locked && cargo test --locked
cargo test -p ibis-desktop --locked              # the app's Rust side; plain cargo test skips it
pnpm -C apps/desktop test && pnpm -C apps/desktop typecheck
node scripts/check.mjs --script-tests      # only when scripts/ changed
node scripts/pr-lint.mjs --title "fix: explain the change"
```

## Working

- Check `git status` first and keep work you did not make. Branch from `origin/main`, one branch per pull request, no stacking, never push to `main`. Give each worktree its own Cargo target directory.
- Claim an issue with a one-line comment on the tracking issue or a draft pull request that says `Closes #N`. One writer per worktree and per set of files.
- Build what the issue's "Done when" names as the smallest working slice; merge it, then harden. Delete what you replace. Until 1.0, replace wrong behaviour: no compatibility code or settings migrations.
- At most two open drafts each; finish them before starting more. Split a draft over about 1,500 changed lines.
- A check that fails twice for the same reason: fix the cause or shrink the test. After three CI rounds on one blocker, cut scope and write the diagnosis on the issue.
- Every fix gets a regression test. Findings below P2 go into one 1.0 issue per area, not new issues.
- Use invented data only. No real library data, document text, private paths or credentials in source, tests, logs or public text. Never publish `.local/`.
- UI changes go through `apps/desktop/src/tokens.css`, a shared component in `apps/desktop/src/ui/` or `docs/design.md`, never one screen on its own. When the maintainer says something looks or behaves wrong, fix it and write the rule into `docs/design.md`.
- Keep licences and notices; dependencies must pass `cargo deny`. Logos and icons come only from `brand/` (`python3 brand/build.py`).

## Merging

- Pull request titles are Conventional Commits; they become the release notes. Say what changed and how it was checked, and link `Closes #N`.
- Sign off commits (`git commit -s`) with a public project or GitHub noreply identity, and disclose AI help with an `Assisted-by:` or `Co-authored-by:` trailer.
- Changes to these risky areas need one review of the final head by a session that did not write the change, with every P1 and P2 fixed: the updater, release signing and CI permissions; settings and other files Ibis writes; the clipboard, escaping and pasting; window permissions, native input and the shortcut portal; BibTeX and Zotero reading, citation syntaxes and formatting; untrusted input, network access and security boundaries. Anything else needs your own adversarial check.
- Merge only with a green "CI result" on the exact head, by squash, never bypassing. Auto-merge sometimes does not fire: if a green pull request is still open, squash-merge it yourself. GitHub runs no pull request checks while a pull request has a merge conflict, so a head with no "CI result" usually needs `origin/main` merged in; never merge a head that has no CI run. If `main` turns red, revert the merge that broke it first, then fix.
- Hand routine pull request work to the Codex crew on the maintainer's Mac with a label, then move on: `codex:babysit` drives the pull request to merge (fixes CI and P1/P2 review findings, squash-merges when green, or comments once and drops the label when blocked); `codex:review` posts one adversarial "Codex review:" comment (it tries to break the change as a user would) and swaps the label to `codex:reviewed`. A `codex:review` counts as the independent review for risky areas. Don't push to a branch while it carries `codex:babysit`.
- After a merge, update the tracking issue's Now if it changed. Lasting decisions go into `docs/decisions.md`.

## Asking the maintainer

Agents have standing engineering and merge authority. Ask the maintainer only about licensing, new network use, repository or organization settings, money or accounts, what a release contains, or work needing their hands: once, as a yes/no with your recommendation. Never ask again for something already authorized. Stable releases wait for the maintainer's one-click approval; nightlies publish on their own.
