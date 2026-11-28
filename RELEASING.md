# Maintaining and releasing Ibis

Stable publication needs the maintainer's GitHub deployment approval.
Nightlies publish automatically. Engineering and review authority is in
[AGENTS.md](AGENTS.md#trust-and-authority); future release work is in [ROADMAP.md](ROADMAP.md).

## Releases

Release Please owns versions and `CHANGELOG.md`. A milestone uses `0.N.0`;
fixes/small additions use patch versions. `Release-As: 0.N.0` in a main commit
requests a milestone version. Do not edit versions manually.

| Release | Tag | GitHub status |
| --- | --- | --- |
| Ibis X.Y.Z | `vX.Y.Z` | Numbered release; highest version is Latest. |
| Ibis Nightly | `nightly` | Rolling pre-release. |
| Ibis update feed | `stable` | Fixed-tag pre-release for permanent download URLs. |

Stable and Nightly have separate app IDs and Linux executable/icon names.
Windows ships one NSIS installer, and the same files go to the Microsoft Store
as an MSIX package (see [The Microsoft Store](#the-microsoft-store)). Nightly omits RPM. Packages use stable names
such as `Ibis-macOS-Apple-silicon.dmg` and `Ibis-Windows-setup.exe`. CLI archives
contain `ibis`, `LICENSE`, `NOTICE.md` and `THIRD-PARTY-NOTICES.txt`; every app
package carries the same three files in its `notices` resource folder. Preview's
notices job writes them once from the lockfiles (`scripts/third-party-notices.py`,
with `cargo about` and `pnpm licenses`): every licence and NOTICE file each
dependency ships, and the standard text of its declared licence for the few that
ship none (listed in the job log; npm ones only after review in the script). It
fails when a crate's licence cannot be read or named, or an npm package has no
licence file and no review. The verify job requires them in every package byte
for byte. `cargo deny` remains the licence policy. Checksums
and provenance accompany packages.

## The note people read

For a milestone, put a short user-facing note above generated changes: what
someone can now do, what was fixed and what remains limited. Name only
behaviour demonstrated by that build. `scripts/release-notes.mjs` adds the
download table, verification steps, signing warning and changed CSL styles;
`changed-styles.txt` holds the full style list.

## How a release is built

Release Please maintains `chore: release X.Y.Z`. Prepare the final head against
main and run `bash scripts/land.sh` like any other branch. Merge updates
versions/changelog and tags through the release app token, starting
[Preview](.github/workflows/preview.yml).

1. Build Windows, macOS Apple silicon/Intel and Linux packages. Compare CSL
   output with the preceding release; new failures block numbered publication.
2. Apply configured [macOS signing](docs/code-signing.md) and verify provenance.
   The verify job then opens every package on its own system
   (`scripts/verify-packages.py`): checksums, notices, the version from the tag
   or commit in the package's metadata and executables, the processor, the
   macOS signature (by the pinned certificate) and identity, and the command
   line's `--version`. It first plants bad copies (cut in half, a wrong
   version, notices removed) and fails unless each is refused. Install, publish
   and nightly jobs require it. Packages are then installed and started.
   Intel macOS is verified but not run, and its release-notes row says so.
3. The sbom job attests each final package to its CycloneDX SBOM
   (`scripts/sbom.py`, written by the build on its own system: the Rust
   crates that target links, the embedded citeproc-js, CSL and Zotero files
   and, for the app, the npm packages).
4. Stable publishing waits in `updates` for approval. It verifies exact-tag
   checksums, provenance and SBOM attestations (`scripts/verify-sboms.sh`),
   signs updater archives, attaches packages and SBOMs to the draft and only
   then publishes it. Numbered publishing is serialized and never changes a
   public release.
5. `stable-feed` replaces packages, then `latest.json` on the fixed `stable`
   tag. Older versions cannot replace a newer feed. That tag is not a source ref.

Once a day (02:07 UTC) Preview builds the nightly from main's head if its push
CI passed and the nightly lacks it, then verifies the packages and that CI again
before replacing downloads; otherwise the preceding nightly stays.
Main CI confirms the full platform matrix. Daily/manual CI adds installed nightly journeys, WebKit,
release-mode/MSRV checks and bounded fuzzing; failures update one issue. Run
`ci.yml` with `workflow_dispatch` on main for qualification before a stable release.
Manual Preview with publication disabled retains artifacts for 14 days. The
daily installed-update check still uses the previous nightly; see [testing](docs/testing.md#native-journeys-and-installed-updates).

## The Microsoft Store

Every Windows build also makes `Ibis-Windows.msix` (`scripts/build-msix.ps1`):
the installer's files, the Store images in `apps/desktop/src-tauri/icons/windows`
and [`AppxManifest.xml`](apps/desktop/src-tauri/packaging/msix/AppxManifest.xml).
The build installs a copy signed with a throwaway certificate, runs `ibis
--version` through its alias, starts the app from Start and removes it
(`scripts/test-msix.ps1`). The package is a workflow artifact
(`store-x86_64-pc-windows-msvc`, kept 90 days), never a release asset: it
is unsigned, and the Store signs what it receives. Only a numbered release's
package (product "Ibis") goes to the Store. In Partner Center, open the app,
start an update submission, replace the package under Packages and submit. The
Store copy needs Windows 11, where the folders the manifest excludes from
virtualization are honoured; Windows 10 keeps the installer. It updates through
the Store, so its updater is off; its startup task
and command alias replace the Run key and PATH entry (`src-tauri/src/store.rs`).

The manifest's restricted capabilities need a reason in the submission:
`runFullTrust` because Ibis is a desktop app, and `unvirtualizedResources`
because Ibis shares its settings with its command line and connects AI apps by
editing their settings files in AppData (only those folders are excluded).

The identity in `scripts/build-msix.ps1` must match Partner Center's
Product identity page (package name, publisher and publisher display name).

## Verifying a download

Anyone can check a file from a numbered release (use `refs/heads/main` for
the nightly):

```sh
shasum -a 256 FILE        # compare with SHA256SUMS-*.txt in the same release
gh attestation verify FILE --repo ibiscite/ibis --source-ref refs/tags/TAG \
  --signer-workflow ibiscite/ibis/.github/workflows/preview.yml
gh attestation verify FILE --repo ibiscite/ibis --source-ref refs/tags/TAG \
  --signer-workflow ibiscite/ibis/.github/workflows/preview.yml \
  --predicate-type https://cyclonedx.org/bom
```

The first attestation says the file was built from that tag by this
repository's Preview workflow; the second carries the SBOM, which must equal
the attached `Ibis-<system>.cdx.json` (app packages) or
`ibis-<system>-cli.cdx.json` (command line). Before approving a stable
release, run these three checks on one package per system from the release
candidate's workflow artifacts and note the result on the release PR.

## Immutable releases

Maintainer decision, 2026-11-25 (#50): numbered releases are immutable once
published ([decision 0019](docs/decisions/0019-updates.md)). The repository
setting is **Settings → General → Releases → Enable release immutability**;
it applies only to releases published after it is switched on. Every file a
numbered release carries (packages, checksums, SBOMs, the style list) is
attached while it is a draft, and nothing edits it after publication except
GitHub's Latest marker.

The rolling `nightly` and `stable` releases replace their files and the
nightly tag moves; both were published before the setting, so they stay
mutable. Never delete them: a new one would lock at its first publication and
its tag could not be used again, so the publishing scripts refuse to create
one unless GitHub confirms immutability is off
(`publication-guard.mjs new-rolling`). If one is lost, the channel has to move
off GitHub Releases or to a new tag name, which the app's feed URLs and the
website name; that is a maintainer decision.

## Update keys

Stable and Nightly trust separate public keys. Private keys/passwords belong
in `updates` (`v*`, required approval) and `nightly` (`main`) respectively.
Key-holding jobs restore no shared caches. Signatures authenticate the build
version as well as package bytes. Keep an offline backup of each private key
and its password, in two places, never in the repository.

The app trusts exactly one key, built into it (`tauri.conf.json` for Ibis,
`NIGHTLY_PUBKEY` in `scripts/update-feed.mjs` for Nightly), and
`update-feed.mjs` refuses a feed signed by any key other than the one built
into the commit being published.

**Planned rotation** (for example before a key's holder changes):

1. Generate the new pair offline with `pnpm -C apps/desktop tauri signer generate`
   and back it up as above.
2. Bridge release: a reviewed change puts the new public key into the app,
   while this one release is still signed with the old key, which every
   installed copy trusts. The feed's key check must accept the old key for
   exactly this version; that exception is a code change made, reviewed and
   removed with the rotation, since the scripts allow one key per channel.
3. Wait until the update counts show most people on the bridge release, then
   replace the environment's secret with the new key. Later releases are
   signed with it only. Copies older than the bridge cannot verify them and
   must install the next release by hand; the release notes say so.

**Lost key** (no backup): installed copies can never accept another update.
Generate a new pair, ship it in the next release, and say in the release
notes, the website and a pinned issue that everyone must download
and install that release by hand. Nothing else changes.

**Leaked key**: remove the secret from its environment at once and treat it as
lost. A leaked key alone cannot reach people: the app fetches updates only
from this repository's release feed, which needs write access to change. Check
the release and environment audit logs for unexpected publications, then ship
the new key as for a lost one. A Nightly key never signs Ibis, so a Nightly
leak touches only Nightly.

## One-time setup

The workflows use the `ibis-release` GitHub App, installed only on this
repository with Contents, Pull requests and Issues read/write. Its
`RELEASE_APP_ID` and `RELEASE_APP_PRIVATE_KEY` live in `release` for main.
`RELEASE_PLEASE=enabled` enables the workflow. The app token allows its tags
to start workflows. The signing and update environments have separate access.
These names describe workflow requirements, not an audit of live settings.

## Before approving a stable release

Use invented libraries on each tested system. Check offline launch, shortcut
and conflict handling, hide/reopen, library selection, delivery, Quit and
persisted choices. Preinstall WebView2 for offline Windows installation.
Retain evidence for the exact release build; a build alone is not acceptance.

## Recovering a package build

A tag without its draft stops publication. Rebuild at that existing tag with
publication off, check SHA256SUMS and verify provenance:

```sh
gh attestation verify FILE --repo ibiscite/ibis --source-ref refs/tags/TAG --source-digest TAG_COMMIT_SHA --signer-workflow ibiscite/ibis/.github/workflows/preview.yml
```

Then rerun at the same tag with publication on, or upload verified packages
to its draft. Never move/delete a release tag to recover a build.
