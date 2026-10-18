# Development process

This project favours small, verified, reversible steps.

## Flow of work

1. **Issue.** Every change starts from an issue that states the user-visible outcome and acceptance criteria. Issues belong to a milestone (`0.1 Picker`, `0.2 Zotero and styles`, ...).
2. **Branch.** One short-lived branch per issue, named `<type>/<issue>-<short-description>`, e.g. `feat/12-zotero-snapshot-reader`.
3. **Test first.** Add a failing test or fixture that captures the acceptance criteria, then implement.
4. **Check.** `node scripts/check.mjs` must pass locally. It checks GitHub Actions expressions with actionlint before the Rust and desktop checks, and runs every desktop crate test (`cargo test -p ibis-desktop`), which plain `cargo test` skips because the desktop crate is not a default workspace member. A failing check exits 1; a missing tool reports incomplete (exit 2) on every platform.
5. **Pull request.** Title in Conventional Commits form (`feat: read Zotero libraries while Zotero is closed`), linked to its issue (`Closes #12`). CI runs on Windows, macOS and Linux. A newer push to a pull request cancels its older run; every commit on `main` is tested to completion, and only `main` saves Rust build caches. The description says what users can now do, how it was verified, and what was not verified.
6. **Review.** One independent review of the final diff; P1 findings and regressions are fixed before merging, other findings become issues.
7. **Merge.** Squash-merge when checks pass. The pull request title becomes the commit on `main`. `main` is protected and always releasable; its history is never rewritten.

## Working quickly and safely

CI takes a few minutes per pull request. Most delay comes from waiting, so work is arranged to overlap:

- **Verify the exact commit.** Before pushing, run the full check in a clean checkout of the commit, so uncommitted files cannot make it pass. Reusing one build directory keeps it fast:

  ```sh
  root=$(git rev-parse --show-toplevel); sha=$(git rev-parse HEAD)
  dir="$(mktemp -d)/ibis-verify"   # a new, empty path: never someone else's work
  git worktree add --detach "$dir" "$sha" &&
    (cd "$dir" && CARGO_TARGET_DIR="$root/target/verify" node scripts/check.mjs) &&
    git push
  git worktree remove --force "$dir" 2>/dev/null; git worktree prune
  ```

- **Stack instead of waiting.** Start the next issue on a branch based on the previous one. After the previous pull request is squash-merged, move the branch onto `main` and confirm the content did not change:

  ```sh
  tree=$(git rev-parse next-branch^{tree})
  git fetch origin
  git rebase --onto origin/main previous-branch next-branch
  test "$(git rev-parse next-branch^{tree})" = "$tree" && echo unchanged
  ```

- **Review in parallel.** Run the one independent review of the final diff while CI runs, and save findings in the pull request (fixed, filed as an issue, or rejected with a reason).
- **Independent work in parallel.** Issues that touch different files can be worked on at the same time, each in its own worktree and branch. Two writers never share a checkout. Shared files (`Cargo.toml`, `Cargo.lock`, `apps/desktop/src-tauri/src/main.rs`, `ROADMAP.md`) are changed by one branch at a time.

## Progress reports

Each milestone has a tracking issue (label `tracking`) with a checklist of its issues, decisions taken, and open questions for the maintainer. After each merge, add one line saying what users can now do and what is next. Questions that need the maintainer carry the label `needs:maintainer`.

## Running the desktop app

```sh
cd apps/desktop
pnpm install
pnpm tauri dev
```

Choose a library from the picker or from the menu bar (tray) item. The choice and other preferences are saved in `settings.json` in the app's configuration folder (on macOS `~/Library/Application Support/io.github.ibis-cite.ibis/`):

```json
{ "schema_version": 1, "shortcut": "Control+Command+R", "library": "/path/to/refs.bib", "copy_format": "pandoc" }
```

Every field is optional. Edits made while the app runs are kept, and a file that cannot be read is reported and never overwritten. For development, `IBIS_LIBRARY=/path/to/file.bib` opens a library without saving it.

## Desktop command contract

`apps/desktop/src/bindings.ts` is generated from Rust command signatures and
Serde-aware type definitions. `src-tauri/src/bindings.rs` registers the commands
once for both the runtime handler and Tauri Specta's exporter. After changing a
command or a wire type, regenerate with:

```sh
cargo test -p ibis-desktop bindings::tests::export_contract -- --ignored
```

The normal check script and desktop CI compare a fresh temporary export with the
checked-in file and fail if it is stale; checks never rewrite it. `api.ts` keeps
event adapters, UI labels and the search limit default. Its format-label map is
exhaustive over the generated Rust enum. The core's optional `type-export`
feature enables schema derives only, without Tauri or other GUI dependencies.

Tauri Specta is used because it derives wrappers as well as types, removing the
duplicate command contract without a handwritten schema or generator. Its Tauri
2 support is currently a prerelease, so Specta and Tauri Specta are pinned to
matching exact versions as upstream requires. All added dependencies pass the
existing licence and advisory policy.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `perf:`, `docs:`, `test:`, `refactor:`, `ci:`, `chore:`. Breaking changes use `!` (`feat!:`) or a `BREAKING CHANGE:` footer. Commits are signed off (`git commit -s`).

## Versions and releases

- [Semantic Versioning](https://semver.org/). While the version is `0.x`, features and fixes raise the patch version (`0.1.3`), and completing a roadmap milestone raises the minor version (`0.2.0`), so version numbers match `ROADMAP.md`. A milestone release is requested by adding `Release-As: 0.N.0` to the body of a commit on `main`.
- [release-please](https://github.com/googleapis/release-please) keeps a release pull request open on GitHub. Merging it updates `CHANGELOG.md` and the version in `Cargo.toml`, creates the tag and the GitHub release. Versions are never edited by hand.
- Release builds are produced only by CI from a tag. Preview builds may be published as GitHub pre-releases.
- Data formats that users keep (settings, destination profiles, history) carry a schema version and a tested migration.

## Workflow linting

Install [actionlint](https://github.com/rhysd/actionlint/blob/v1.7.12/docs/install.md) and make it available on `PATH`. Standard options include `brew install actionlint` on macOS, `go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12` with Go installed, or the official release binaries for Linux, macOS and Windows. CI uses the official 1.7.12 Docker image pinned by digest in the required Dependency policy job; updates should review the release and refresh that pin together.

Run `actionlint -shellcheck= -pyflakes=` to validate every workflow's GitHub-specific contexts, expressions and action usage. The full check uses the same flags; optional shellcheck and pyflakes integrations are disabled so installed auxiliary tools do not change the result. Workflow checks read files without editing them.

## Dependencies

`Cargo.lock` is committed. The Rust toolchain is pinned in `rust-toolchain.toml`. Dependabot proposes grouped updates monthly; each is reviewed and merged only with passing checks. `cargo deny` enforces license compatibility and blocks known vulnerabilities.

## Definition of done

- Acceptance criteria are covered by automated tests (or, for platform behaviour that cannot be automated yet, a recorded manual check in the pull request).
- Documentation, `ROADMAP.md` and snapshots are updated.
- No private data in code, tests, logs or text.
