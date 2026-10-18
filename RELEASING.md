# Maintaining and releasing Ibis

Single-maintainer workflow for now. Implement and test by default. Commit, push, tag or release only when the maintainer explicitly asks for that exact action.

## Everyday workflow

1. Describe the user outcome and constraints. The implementation handles routine design, tests, documentation and review.
2. Inspect `git status` before editing; preserve existing work. One writer per checkout; use `git worktree` for concurrent work when asked.
3. Make one focused change. Run `cargo test -p ibis-core` while developing.
4. Run `node scripts/check.mjs`, review the diff, and report results and remaining limits.
5. When asked: commit with sign-off (`git commit -s`), push, and wait for CI on that exact commit.

## Real-library check

When source reading, search or output changes, also test against a **controlled** library, never a personal working library: a synthetic `.bib` file or a dedicated test library in Zotero. Record what was tested in the PR. Do not copy real records into the repository.

## Releases

Versioning, commit conventions and milestone releases are described in `docs/development.md`.

### Enabling release automation (one time)

1. Organization settings → Actions → General → Workflow permissions: enable "Allow GitHub Actions to create and approve pull requests".
2. Repository settings → Secrets and variables → Actions → Variables: add `RELEASE_PLEASE` with value `enabled`.

Release Please then keeps a release pull request open. Pull requests opened by GitHub Actions do not trigger other workflows, so the required checks do not report on them; the release pull request only changes the version and changelog, and a repository administrator merges it using the ruleset bypass. Replacing this with a GitHub App token, so checks run on release pull requests too, is planned before 1.0.

### Preview builds

`.github/workflows/preview.yml` builds Windows (MSI and setup executable), macOS (Apple silicon and Intel disk images) and Linux (AppImage, .deb, .rpm) packages in CI, with SHA-256 checksums and build-provenance attestations.

- **Dry run:** Actions → Preview builds → Run workflow. The packages are kept as workflow artifacts for 14 days; nothing is published.
- **Pre-release:** when the maintainer pushes a tag `v*`, the same packages are published as a GitHub pre-release whose notes say the builds are unsigned and how to verify them.

Before publishing, install a dry-run build on each operating system without a network connection and check: the shortcut (and the message when another app already uses it), hiding and reopening (including the Dock on macOS), choosing a library, copying and pasting a citation into an ordinary editor, Quit, and relaunching with the same library.

### Planned

- Versions follow semantic versioning; `0.x` releases may change behaviour.
- Release notes come from Conventional Commit messages.
- Desktop builds for Windows, macOS and Linux are produced by CI, never on a personal machine. They are signed (Apple Developer ID; SignPath Foundation or Microsoft Store for Windows), with build-provenance attestations and an SBOM.
- Updater and style-bundle signing keys live only in GitHub organization secrets with narrow access; their loss or rotation follows a written procedure before 1.0.
- Distribution: GitHub Releases, Homebrew cask, winget, Flathub/AppImage/.deb/.rpm.
