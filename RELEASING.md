# Maintaining and releasing Ibis

Release authority follows [AGENTS.md](AGENTS.md#authority); merge holds follow [Review and merge](docs/development.md#review-and-merge).

## Real-library check

When source reading, search or output changes, also test against a **controlled** library, never a personal working library: a synthetic `.bib` file or a dedicated test library in Zotero. Record what was tested in the PR. Do not copy real records into the repository.

## Releases

Versions follow [Semantic Versioning](https://semver.org/). Before 1.0,
`0.N.0` ships milestone `0.N`; `0.N.1`, `0.N.2`, ... add fixes and small features. Release notes
come from Conventional Commits; commit conventions are in
[docs/development.md](docs/development.md#commits). To request a milestone version, use
`Release-As: 0.N.0` in a commit body on `main`.

The [code signing policy](docs/code-signing.md) records the current unsigned
status and the planned SignPath Foundation process. Link it from release notes;
once accepted and integrated, include the SignPath credit described there.
Updater signatures and operating-system code signing are separate.

Release Please keeps a pull request titled `chore: release X.Y.Z`; merging it
under that authority updates `CHANGELOG.md` and the workspace version,
tags `vX.Y.Z` and publishes **Ibis X.Y.Z** with a download table and changes
grouped by type. Never edit versions by hand. Numbered release builds are
produced only by CI from a tag; previews may be GitHub pre-releases. Data formats
have their own schema versions and tested migrations.

Non-documentation changes on `main` publish
**Ibis Nightly X.Y.Z-nightly.YYYYMMDDHHMMSS** on the rolling `nightly` tag.
The rolling `stable` tag mirrors the newest release so download links never
change. Tags are `vX.Y.Z`, `nightly` and `stable`; no other tags.

Release Please updates the workspace version and the four release package entries
in `Cargo.lock`, and the version in `apps/desktop/package.json` (private, so its
lockfile does not record it); `ibis-fuzz` stays unchanged. When changing its updater config,
rehearse the pinned upstream updater in a temporary checkout, inspect that only
those versions changed, and run both commands below there and on the generated
release PR:

```sh
cargo metadata --locked --offline --format-version 1
cargo metadata --locked --offline --format-version 1 --manifest-path fuzz/Cargo.toml
```

TOML scalar metadata makes the name filters use `@.name.value` while the
replacement selects `.version` ([upstream explanation](https://github.com/googleapis/release-please/issues/2455)).

### Enabling release automation (one time)

1. Organization settings → Actions → General → Workflow permissions: enable "Allow GitHub Actions to create and approve pull requests".
2. Repository settings → Secrets and variables → Actions → Variables: add `RELEASE_PLEASE` with value `enabled`.

Release Please then keeps a release pull request open. Bot-created proposals can require maintainer approval before CI runs. Review the final proposal, then approve or retrigger its validation as appropriate. All required checks and desktop checks must pass on the exact final head before a release can be considered; missing checks are incomplete validation. Do not bypass required checks to merge a release proposal. Approving CI runs does not replace those merge conditions or the release authority linked above.

### Preview builds

`.github/workflows/preview.yml` builds Windows (MSI and setup executable), macOS (Apple silicon and Intel disk images) and Linux (AppImage, .deb, .rpm) packages in CI, with SHA-256 checksums and build-provenance attestations. Packages are named `Ibis-<platform>.<extension>` (for example `Ibis-macOS-Apple-silicon.dmg`, `Ibis-Windows-setup.exe`) with no version, so download links to a release or the nightly never change; the version is in the release title and tag. x64 packages have no architecture in their name and these names never change; ARM packages, when they arrive, add `-ARM` after the platform (`Ibis-Windows-ARM-setup.exe`, `Ibis-Linux-ARM.AppImage`).

- **Dry run:** Actions → Preview builds → Run workflow. The packages are kept as workflow artifacts for 14 days; leave the publication checkbox unchecked. Nothing is published, including when the selected ref is a tag.
- **Pre-release:** after a release PR merge under that authority, Release Please creates a draft release and its tag, then dispatches Preview builds at that tag with publication enabled. Its `GITHUB_TOKEN` cannot trigger a second tag-push workflow, but explicit workflow dispatches can run. A maintainer's own `v*` tag push also builds and verifies packages before publishing a pre-release. Major-zero releases are marked as previews.

- **Nightly:** every push to `main` that changes more than documentation builds the same packages and replaces the `nightly` pre-release ([Releases → nightly](https://github.com/ibiscite/ibis/releases/tag/nightly), titled `Ibis Nightly <version>` with the version and commit at the top of its notes). The `nightly` tag moves to that commit; it never matches `v*`, so it starts no release build and Release Please ignores it. Runs are never cancelled; while one runs, only the newest waiting push is kept, and a run whose commit is no longer the head of `main` does not publish. Publication first checks every checksum and verifies each package's provenance against `refs/heads/main` and that commit; a failed build or check leaves the previous nightly in place. Packages are uploaded before the tag moves and the notes change, so the notes always name the commit that was fully published. To refresh it by hand, run Preview builds on `main` with publication checked. Builds from `main` are the separate app **Ibis Nightly** (identifier `io.github.ibiscite.ibis.nightly`, version `X.Y.Z-nightly.YYYYMMDDHHMMSS` from `scripts/update-feed.mjs`), without the MSI and `.rpm`, whose version formats cannot carry it.
- **Updates (ADR 0019):** the nightly job and the tag release job, in the `updates` environment that only `main` and `v*` tags can use, sign the macOS app archives, the Windows setup executable and the AppImage with the Ibis update key, check that each signature names the key built into the app (the app itself verifies the signature before installing), and upload `latest.json` after the packages. A tag release must name the workspace version; once the release is public, the `stable-feed` job copies its packages and `latest.json` to the rolling `stable` pre-release, which the stable app reads and which gives fixed download links to the newest release. That job can be re-run on its own, and leaves the feed in place for a tag older than the one it names (a backport). Signatures live only in `latest.json`. Once a day, and when Preview builds is run manually on main with "Only rerun the update test", the `update-e2e` job takes the newest nightly and the nightly it replaced from the run that published it (not after every push until [#493](https://github.com/ibiscite/ibis/issues/493) is fixed). It installs the replaced nightly on Linux (the AppImage, under a virtual display), Windows (the setup executable) and Apple silicon macOS (the `.app` from its update archive), starts it with update checks on, presses "Install and Restart" through the system's accessibility interface and checks that the installed app is now the new version and that the new version started; it is skipped, with a notice, when the previous nightly had no feed or a newer commit took over publication. Expired files fail a scheduled run, and a failed scheduled run opens or updates an issue. Losing the key or its password means testers reinstall once by hand; the maintainer keeps an offline backup.

All four package jobs must succeed. Then an install job installs the packages the way a user would and starts the installed app: the Linux `.deb` with `apt-get` (and the AppImage), the Windows setup executable and MSI silently, and the Apple silicon disk image copied to an Applications folder. It runs `scripts/smoke-native.py` against the installed app with a synthetic library, checks the app stays up, and uninstalls it. The Intel disk image is built but not installed. The nightly and pre-release are published only if every install succeeds; screenshots and logs are kept as `install-*` workflow artifacts. Publication checks every downloaded checksum and verifies each package and checksum attestation against the exact tag ref, source commit and preview workflow. Packages upload to a draft; only then is the draft published. A build, verification or upload failure leaves the draft unpublished. An already public release is not overwritten. The release is titled `Ibis X.Y.Z`; `scripts/release-notes.mjs` turns Release Please's generated change list into the notes (download table, unsigned-install note, changes without commit hashes, verification instructions) and leaves notes that already have a download table alone.

### Recovering a package build

Checkout must match the workflow SHA so provenance names the source actually
built. Run Preview builds **at the existing release tag**, with publication off.
Download that run's `ibis-*` artifacts, verify every file against `SHA256SUMS`,
and verify provenance against that tag, its commit and the preview workflow:

```sh
gh run download RUN_ID --pattern 'ibis-*' --dir dist
gh attestation verify FILE --repo ibiscite/ibis --source-ref refs/tags/TAG --source-digest TAG_COMMIT_SHA --signer-workflow ibiscite/ibis/.github/workflows/preview.yml
```

After verification and offline review, follow the release authority linked above:
rerun at the same tag with publication on, or upload verified
packages to the existing draft and publish it. Confirm the target tag and draft,
keep the preview warning and verification instructions, and never move or delete
a `v*` tag to recover a build. Already public releases are not overwritten.

Before publishing, install a dry-run build on each operating system without a network connection and check: the shortcut (and the message when another app already uses it), hiding and reopening (using the menu-bar item on macOS), choosing a library, copying and pasting a citation into an ordinary editor, Quit, and relaunching with the same library. For the Windows offline check, install Microsoft WebView2 Runtime first. When it is missing, the Windows installer downloads Microsoft's bootstrapper to install it; testing that prerequisite setup requires an internet connection. Ibis works offline after setup (see [PRIVACY.md](PRIVACY.md)).

### Planned

- Desktop builds for Windows, macOS and Linux are produced by CI, never on a personal machine. They are signed (Apple Developer ID; SignPath Foundation or Microsoft Store for Windows), with build-provenance attestations and an SBOM.
- Updater and style-bundle signing keys live only in GitHub organization secrets with narrow access; their loss or rotation follows a written procedure before 1.0.
- Distribution: GitHub Releases, Homebrew cask, winget, Flathub/AppImage/.deb/.rpm.
