# Maintaining and releasing Ibis

Follow [AGENTS.md](AGENTS.md) for contributor and autonomous development permissions. Releases, tags and publishing always require an explicit maintainer request.

## Everyday workflow

1. Describe the user outcome and constraints. The implementation handles routine design, tests, documentation and review.
2. Inspect `git status` before editing; preserve existing work. Use separate worktrees for concurrent changes.
3. Make one focused change. Run `cargo test -p ibis-core` while developing.
4. Run `node scripts/check.mjs`, review the diff, and report results and remaining limits.
5. Follow the [autonomous work conditions in AGENTS.md](AGENTS.md#working-autonomously) for commits, pushes, independent review and merging an agent's own pull request.

## Real-library check

When source reading, search or output changes, also test against a **controlled** library, never a personal working library: a synthetic `.bib` file or a dedicated test library in Zotero. Record what was tested in the PR. Do not copy real records into the repository.

## Releases

Versioning, commit conventions and milestone releases are described in `docs/development.md`.

Release Please updates the shared workspace version and the `ibis`, `ibis-core` and `ibis-desktop` entries in the root lockfile together. Its TOML updater tags scalar values with position metadata, so lockfile name filters use `@.name.value` while the replacement selects `.version` ([upstream explanation](https://github.com/googleapis/release-please/issues/2455)). The separate `ibis-fuzz` tooling version stays unchanged. When changing this configuration, rehearse the pinned upstream updater on a temporary checkout with a proposed version, then run `cargo metadata --locked --offline --format-version 1` and the same command with `--manifest-path fuzz/Cargo.toml`. Both must pass; inspect the diff to ensure only the workspace and three release package versions changed. Also run locked metadata on the generated release PR before considering a release.

### Enabling release automation (one time)

1. Organization settings → Actions → General → Workflow permissions: enable "Allow GitHub Actions to create and approve pull requests".
2. Repository settings → Secrets and variables → Actions → Variables: add `RELEASE_PLEASE` with value `enabled`.

Release Please then keeps a release pull request open. Bot-created proposals can require maintainer approval before CI runs. Review the final proposal, then approve or retrigger its validation as appropriate. All required checks and desktop checks must pass on the exact final head before a release can be considered; missing checks are incomplete validation. Do not bypass required checks to merge a release proposal. Merging the release pull request remains an explicit maintainer decision, separate from approving its CI runs.

### Preview builds

`.github/workflows/preview.yml` builds Windows (MSI and setup executable), macOS (Apple silicon and Intel disk images) and Linux (AppImage, .deb, .rpm) packages in CI, with SHA-256 checksums and build-provenance attestations.

- **Dry run:** Actions → Preview builds → Run workflow. The packages are kept as workflow artifacts for 14 days; nothing is published, including when the selected ref is a tag.
- **Pre-release:** after an explicitly authorized release PR merge, Release Please creates the tag and calls the shared package workflow directly with the released SHA and tag. Its `GITHUB_TOKEN` cannot trigger a second tag-push workflow. A maintainer's own `v*` tag push also builds and attaches packages. Major-zero releases are marked as previews from creation; the proposed first version remains `0.1.0`.

Attaching packages preserves the generated changelog and adds the unsigned preview warning and checksum/provenance instructions once.

Checkout uses the released SHA, and packaging stops before building if it differs from the workflow commit: the standard provenance attestation records the workflow's OIDC commit. If a delayed Release Please run discovers a release merged before its own triggering commit, recover by manually running Preview builds **at the existing release tag**. This produces correctly attributed artifacts without publishing. Download that run's `ibis-*` artifacts with `gh run download RUN_ID --pattern 'ibis-*' --dir dist`. Verify every package's SHA-256 against its downloaded `SHA256SUMS` file and its provenance with `gh attestation verify FILE --repo ibiscite/ibis --source-digest TAG_COMMIT_SHA`, using the commit at the existing release tag. After those checks and the offline review below, and only with explicit maintainer approval to publish, attach the packages and checksums with `gh release upload TAG dist/*/* --clobber`. Verify the target tag and release before uploading; keep the preview warning and verification instructions in its notes. Do not delete or move the tag to recover a build.

Before publishing, install a dry-run build on each operating system without a network connection and check: the shortcut (and the message when another app already uses it), hiding and reopening (including the Dock on macOS), choosing a library, copying and pasting a citation into an ordinary editor, Quit, and relaunching with the same library.

### Planned

- Versions follow semantic versioning; `0.x` releases may change behaviour.
- Release notes come from Conventional Commit messages.
- Desktop builds for Windows, macOS and Linux are produced by CI, never on a personal machine. They are signed (Apple Developer ID; SignPath Foundation or Microsoft Store for Windows), with build-provenance attestations and an SBOM.
- Updater and style-bundle signing keys live only in GitHub organization secrets with narrow access; their loss or rotation follows a written procedure before 1.0.
- Distribution: GitHub Releases, Homebrew cask, winget, Flathub/AppImage/.deb/.rpm.
