# Agent and contributor rules

Until 1.0, keep only what prevents real damage ([ADR 0027](docs/adr/0027-fast-until-1-0.md)).
Setup and contribution guidance: [CONTRIBUTING.md](CONTRIBUTING.md). Product rules: README, ROADMAP, architecture and design; accepted ADRs win.

```sh
node scripts/check.mjs --script-tests
cargo test -p ibis-core
cargo test -p ibis-desktop
pnpm -C apps/desktop test
node scripts/pr-lint.mjs --title "fix: explain the change"
```

- Check `git status` first; preserve work you did not make.
- One writer per worktree and set of files; branch from `origin/main`, one branch per pull request. No stacking or pushes to `main`.
- Use a separate Cargo target directory per worktree.
- Add a regression test for each fix; inspect diffs and changed snapshots. Report checks as passed, failed or blocked.
- No real library data, document text, paths or credentials in source, tests, logs or public text. Never publish `.local/` notes.
- UI changes go through `apps/desktop/src/tokens.css`, a shared component in `apps/desktop/src/ui/` or `docs/design.md`; never style one screen on its own. When the maintainer says something looks or behaves wrong, fix it and write the rule into `docs/design.md`.
- Preserve licences and notices; dependencies must pass cargo-deny. Logos and icons come from `brand/`; regenerate with its build script.
- Pull request titles are Conventional Commits (they become the squash commit Release Please reads).
- Sign off commits with a public project or GitHub noreply identity; disclose AI help (`Assisted-by:` or co-author, ADR 0011). Inspect identities and trailers before pushing.
- Risky-area changes (the list in ADR 0027) need one independent review of the final head, by a fresh agent or sub-agent that did not write the change, before merge; fix every P1/P2 first. Other changes need the author's adversarial check.
- Require green "CI result" on the exact head, then squash-merge (auto-merge is fine); never bypass. Revert the merge that causes a real required failure on `main`, then fix separately.
- Until 1.0, replace wrong behaviour; add no compatibility shims or settings migrations.
- Agents have standing engineering and merge authority (ADR 0015). Keep the tracking issue's Now and Queue current.
- Ask the maintainer only about licensing, new network use, repo/organization settings, money/accounts, release contents or actions needing their hands. Never ask again for existing authorization.
