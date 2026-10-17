# Development process

This project favours small, verified, reversible steps.

## Flow of work

1. **Issue.** Every change starts from an issue that states the user-visible outcome and acceptance criteria. Issues belong to a milestone (`0.1 Picker`, `0.2 Zotero and styles`, ...).
2. **Branch.** One short-lived branch per issue, named `<type>/<issue>-<short-description>`, e.g. `feat/12-zotero-snapshot-reader`.
3. **Test first.** Add a failing test or fixture that captures the acceptance criteria, then implement.
4. **Check.** `node scripts/check.mjs` must pass locally.
5. **Pull request.** Title in Conventional Commits form (`feat: read Zotero libraries while Zotero is closed`), linked to its issue (`Closes #12`). CI runs on Windows, macOS and Linux.
6. **Merge.** Squash-merge when checks pass. The pull request title becomes the commit on `main`. `main` is protected and always releasable.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `perf:`, `docs:`, `test:`, `refactor:`, `ci:`, `chore:`. Breaking changes use `!` (`feat!:`) or a `BREAKING CHANGE:` footer. Commits are signed off (`git commit -s`).

## Versions and releases

- [Semantic Versioning](https://semver.org/). While the version is `0.x`, features and fixes raise the patch version (`0.1.3`), and completing a roadmap milestone raises the minor version (`0.2.0`), so version numbers match `ROADMAP.md`. A milestone release is requested by adding `Release-As: 0.N.0` to the body of a commit on `main`.
- [release-please](https://github.com/googleapis/release-please) keeps a release pull request open on GitHub. Merging it updates `CHANGELOG.md` and the version in `Cargo.toml`, creates the tag and the GitHub release. Versions are never edited by hand.
- Release builds are produced only by CI from a tag. Preview builds may be published as GitHub pre-releases.
- Data formats that users keep (settings, destination profiles, history) carry a schema version and a tested migration.

## Dependencies

`Cargo.lock` is committed. The Rust toolchain is pinned in `rust-toolchain.toml`. Dependabot proposes grouped updates monthly; each is reviewed and merged only with passing checks. `cargo deny` enforces license compatibility and blocks known vulnerabilities.

## Definition of done

- Acceptance criteria are covered by automated tests (or, for platform behaviour that cannot be automated yet, a recorded manual check in the pull request).
- Documentation, `ROADMAP.md` and snapshots are updated.
- No private data in code, tests, logs or text.
