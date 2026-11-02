# Maintaining and releasing Ibis

Agents prepare releases and choose versions and timing; each stable release publishes only after the maintainer approves its deployment in GitHub ([AGENTS.md](AGENTS.md#asking-the-maintainer)). Nightly builds publish on their own.

## Releases

Versions follow [Semantic Versioning](https://semver.org/). Before 1.0, `0.N.0` ships milestone `0.N`, and `0.N.1`, `0.N.2`, ... add fixes and small features. To request a milestone version, put `Release-As: 0.N.0` in a commit body on `main`. Never edit versions by hand. Data formats have schema versions; before 1.0 they may change without migrations ([decisions](docs/decisions.md#0016-settings-with-0018-0020-0022-0023-and-0027)).

| Entry | Title | Tag | GitHub flag |
| --- | --- | --- | --- |
| Numbered release | **Ibis X.Y.Z** | `vX.Y.Z` | full release; the highest version carries Latest |
| Nightly | **Ibis Nightly** | rolling `nightly` | pre-release, never Latest |
| Update feed | **Ibis update feed** | fixed `stable` | pre-release, never Latest |

Before 1.0, preview status lives in the notes and website; only the rolling nightly and feed carry GitHub's pre-release flag, and the newest numbered release carries Latest.

No other tags. Ibis uses `com.ibiscite.ibis`; Ibis Nightly is a separate app (`com.ibiscite.ibis.nightly`) without the MSI and `.rpm`, whose version formats cannot carry its version. The nightly version is in the first line of its notes, not its title.

Linux packages use separate channel icon names and desktop templates. The Stable
executable remains `ibis-desktop`; Nightly uses `ibis-nightly`, including Tauri's
automatically named icons. Nightly's config removes inherited Stable icon
destinations through Tauri's JSON merge patch before adding its own.

Packages are named `Ibis-<platform>.<extension>` with no version (for example `Ibis-macOS-Apple-silicon.dmg`, `Ibis-Windows-setup.exe`), so download links never change; ARM packages add `-ARM` after the platform. The command line ships beside them as `ibis-<platform>.tar.gz` (`ibis-Windows.zip` on Windows), holding `ibis`, the licence and `NOTICE.md`. Release notes come from Conventional Commits ([commits](CONTRIBUTING.md#pull-requests)); `scripts/release-notes.mjs` adds the download table, the unsigned-install note and verification steps. Link the [code signing policy](docs/code-signing.md) from release notes: updater signatures and operating-system code signing are separate, and the latter is planned for 1.0.

## How a release is built

Release Please keeps a pull request titled `chore: release X.Y.Z`. Merging it updates `CHANGELOG.md` and the versions, then tags `vX.Y.Z` with the `ibis-release` app's token. The tag starts Preview builds (`.github/workflows/preview.yml`):

1. CI builds Windows, macOS (Apple silicon and Intel) and Linux packages with SHA-256 checksums and build-provenance attestations. All four jobs must succeed.
2. An install job installs each package the way a user would (the Intel disk image excepted), runs `scripts/smoke-native.py` with a synthetic library, and uninstalls it.
3. The publish job waits in the `updates` environment until the maintainer approves it. It then verifies every checksum and attestation against the exact tag and commit, signs the update archives with the stable key, uploads to a draft and publishes it. Any failure leaves the draft unpublished; a public release is never overwritten. Numbered publishes queue and run one at a time, so a run waiting for approval holds the queue and must be approved or rejected before a newer one can reach its prompt.
4. The `stable-feed` job replaces the files on the `stable` pre-release: packages first, then `latest.json`, which the app reads. The fixed `stable` tag anchors permanent download URLs; it never moves and is not a source ref. Older versions leave the feed in place. The job can be re-run on its own.

A nightly follows the same build, install and verify steps without approval, signs with the nightly key in the `nightly` environment, and replaces the `nightly` pre-release only if its commit is still the head of `main`. A failure leaves the previous nightly in place. To refresh it by hand, run Preview builds on `main` with publication checked; with it unchecked the run is a dry run whose packages stay as workflow artifacts for 14 days.

Keeping the previous nightly's identity never blocks publication. The separate
daily update qualification job fails when its previous feed and package exist
but identity evidence is missing or unknown, so failure reporting opens an issue.
A known earlier identity skips qualification with a notice.
Published source is read as text, never executed. Current package installation
checks still gate publication.

## Update keys

Two keys in two environments: the stable key in `updates` (only `v*` tags, after approval) and the nightly key in `nightly` (only `main`). Ibis trusts only the stable key and Ibis Nightly only the nightly key (`NIGHTLY_PUBKEY` in `scripts/update-feed.mjs`), so a key used on every push can never sign a stable update. Key-holding jobs restore no shared cache. Each signature names the build version, which the app checks before installing. Losing a key means that channel's users reinstall once by hand; the maintainer keeps an offline backup of each key. A daily `update-e2e` job installs the previous nightly on each platform and checks that it updates itself to the newest.

## One-time setup

- A private GitHub App, `ibis-release`, installed on this repository only, with Contents, Pull requests and Issues read and write. Its ID and private key are the `RELEASE_APP_ID` and `RELEASE_APP_PRIVATE_KEY` secrets of the `release` environment (only `main`). Release Please needs it because pushes made with `GITHUB_TOKEN` start no workflows, so the tag would build nothing. A lost key is replaced in the app's settings.
- The Actions variable `RELEASE_PLEASE` set to `enabled`.
- The `updates` environment (tag rule `v*`, the maintainer as required reviewer) and the `nightly` environment (branch `main`), each holding its key and password.

When changing Release Please's config, rehearse it in a temporary checkout and check that only the workspace version, the four release package entries in `Cargo.lock` and the version in `apps/desktop/package.json` change, and that `cargo metadata --locked --offline --format-version 1` passes for the workspace. TOML name filters use `@.name.value` ([why](https://github.com/googleapis/release-please/issues/2455)).

## Before approving a stable release

Install the release candidate on each operating system without a network connection and check the shortcut (and the message when another app already uses it), hiding and reopening, choosing a library, copying and pasting a citation into an ordinary editor, Quit, and relaunching with the same library. On Windows, install the WebView2 Runtime first; when it is missing, the installer downloads it, so that one step needs a network connection.

Test a change to source reading, search or output against a **controlled** library (a synthetic `.bib` file or a dedicated Zotero test library), never a personal one, and record what was tested. Do not copy real records into the repository.

## Recovering a package build

A tag without a draft release (Release Please failed after tagging) stops before publishing. To recover, run Preview builds **at the existing release tag** with publication off, so provenance names the source actually built. Download the `ibis-*` artifacts, check them against `SHA256SUMS`, and verify provenance:

```sh
gh run download RUN_ID --pattern 'ibis-*' --dir dist
gh attestation verify FILE --repo ibiscite/ibis --source-ref refs/tags/TAG --source-digest TAG_COMMIT_SHA --signer-workflow ibiscite/ibis/.github/workflows/preview.yml
```

Then rerun at the same tag with publication on, or upload the verified packages to the existing draft and publish it. Never move or delete a `v*` tag to recover a build.

## Planned for 1.0

- An SBOM with every release, and immutable stable releases. 1.0 needs no paid code signing; Apple Developer ID and Windows signing wait for money or a free programme ([ROADMAP](ROADMAP.md)).
- A written procedure for losing or rotating an update key.
- Distribution through Homebrew, winget and Flathub as well as GitHub Releases.
