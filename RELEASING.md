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
Nightly omits MSI and RPM. Packages use stable names such as
`Ibis-macOS-Apple-silicon.dmg` and `Ibis-Windows-setup.exe`. CLI archives contain
`ibis`, the licence and notices. Checksums and provenance accompany packages.

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
2. Apply configured [macOS signing](docs/code-signing.md), verify provenance,
   install/start packages and run native checks. Intel macOS is build-only.
3. Stable publishing waits in `updates` for approval. It verifies exact-tag
   checksums/provenance, signs updater archives and publishes the draft.
   Numbered publishing is serialized and does not overwrite public releases.
4. `stable-feed` replaces packages, then `latest.json` on the fixed `stable`
   tag. Older versions cannot replace a newer feed. That tag is not a source ref.

A nightly verifies packages and requires passing push CI for its revision and
current main before replacing downloads. A failure keeps the preceding nightly.
Main CI confirms the full platform matrix. Daily/manual CI adds installed nightly journeys, WebKit,
release-mode/MSRV checks and bounded fuzzing; failures update one issue. Run
`ci.yml` with `workflow_dispatch` on main for qualification before a stable release.
Manual Preview with publication disabled retains artifacts for 14 days. The
daily installed-update check still uses the previous nightly; see [testing](docs/testing.md#native-journeys-and-installed-updates).

## Update keys

Stable and Nightly trust separate public keys. Private keys/passwords belong
in `updates` (`v*`, required approval) and `nightly` (`main`) respectively.
Key-holding jobs restore no shared caches. Signatures authenticate the build
version as well as package bytes. Keep private offline key backups; losing
one requires affected users to reinstall.

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
