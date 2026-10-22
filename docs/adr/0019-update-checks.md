# 0019. Signed in-app updates from GitHub Releases, with separate Stable and Nightly apps

Status: accepted (2026-10-19)

## Context
Testers should get fixes without reinstalling by hand. Builds are already published on GitHub Releases: numbered previews, and a rolling `nightly` pre-release rebuilt after every change to `main` (#284). All `0.x` releases are pre-releases, so GitHub's `releases/latest` link never finds them. ADR 0008 requires every network feature to be off by default, explained once, to state what is sent, and to be listed in `PRIVACY.md`. The maintainer approved update checks on 2026-10-19; they ship in the 0.2 Shared preview milestone.

## Decision
- **Tauri's updater plugin**, run from the Rust process only. The web interface gets no network permission and its content security policy is unchanged.
- **Opt-in, asked once.** On first launch Ibis asks "Check for updates automatically?", saying what is sent. Nothing is checked until the person says yes. Settings can turn it on or off and offer "Check now".
- **What is sent:** an HTTPS request to `github.com` for one small update file, and the package itself when the person agrees to install it. GitHub sees the IP address and the Ibis version in the user agent. No library data, identifier or usage data is sent.
- **Install only with consent.** Ibis shows the new version and its notes, and downloads and installs only after the person agrees, then restarts.
- **Signed updates.** Each update package is signed with an Ibis update key (Tauri's minisign format). The public key is built into the app; a package whose signature does not match is refused. The private key lives only in a GitHub Actions environment that only the publishing jobs can use, with an offline backup kept by the maintainer. On Windows, once stable builds are Authenticode-signed (SignPath Foundation), the update signature is made over the signed installer. The key is separate from Apple and Windows code signing and costs nothing.
- **Two apps, two feeds.** Nightly builds install as **Ibis Nightly** with their own app identifier, settings and shortcut, so they can sit next to the stable app and never touch its settings. Each app reads only its own update file at a fixed address: `releases/download/nightly/latest.json` for Ibis Nightly, and `releases/download/stable/latest.json` for Ibis, where `stable` is a rolling pre-release that mirrors the newest numbered release (it also gives fixed download links for the stable app). Nightly versions are `X.Y.Z-nightly.YYYYMMDDHHMMSS`, one patch above the last release, so they always increase.
- Failures (offline, GitHub unavailable, bad signature) never interrupt work: "Check now" says what went wrong; automatic checks stay quiet and try again later.

## Consequences
- One new dependency (`tauri-plugin-updater`, maintained by the Tauri project), a `PRIVACY.md` entry and a README note.
- Settings gain an optional `update_checks` field (`true` or `false`; absent means not asked yet). It stays within settings format 2: older builds ignore it, as ADR 0016 allows.
- Update entries in `latest.json` name their package type (`darwin-aarch64-app`, `windows-x86_64-nsis`, `linux-x86_64-appimage`, …), so an app installed from a `.deb`, `.rpm` or `.msi` is never offered a different kind of package.
- While macOS builds are only ad-hoc signed, each update is a new code identity to macOS, so permissions such as Accessibility may be asked for again after an update.
- Builds without an update key (development builds, and any build made before the key exists) have no update items and never check.
- The maintainer creates the update key once and stores it as an environment secret, with an offline backup. Losing it means testers reinstall once by hand; rotation follows a written procedure before 1.0.
- Testers of today's nightly (installed as "Ibis") move to "Ibis Nightly" once, by hand.
- macOS and Windows still warn on first install until the apps are code-signed; updates after that install without a new download.
- Linux updates cover the AppImage; `.deb` and `.rpm` installs are updated by hand until a package repository exists.

## Amendment: authenticate the update version (2026-10-22, #921)

- Every manually signed release and nightly package uses the pinned Tauri CLI's
  `signer sign --app-version` with the same version written to `latest.json`.
  Minisign's global signature covers the trusted comment's `version:` field.
  Publication checks that the field exactly matches the build version.
- Both apps set `requireSignedVersion: true`: bytes and trusted comment must
  verify with the built-in key before the updater compares the signed version
  with the advertised version. Missing or mismatched versions are refused.
  Updater 2.14.0 compares parsed semantic versions, normalizing a leading `v`;
  prerelease and build metadata remain part of signed-version equality.
- Offers must increase within the installed app's channel: plain Stable
  versions accept only plain versions; `nightly.*` accepts only `nightly.*`.
  The shared key alone does not authenticate a channel. An older artifact
  advertised as newer fails signature/version verification; an honestly
  advertised downgrade or cross-channel substitution is never offered.
- CLI 2.12.3's signatures remain valid for installed 0.1.x clients without the
  flag (v0.1.1 locks updater 2.14.0 and minisign-verify 0.3.0). Signing and
  enforcement therefore ship together. Existing clients gain enforcement
  after that update; their earlier unversioned signatures are still unsafe
  until then. Endpoints and the signing secret/key stay unchanged.
- Interrupted downloads and partial cached packages fail verification before
  installation; retrying a complete download remains possible. This does not
  promise rollback after an operating-system installer has started; installed
  update/restart acceptance on each platform remains tracked by #493.

Upstream: [CLI 2.12.3 signing](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.3/crates/tauri-cli/src/helpers/updater_signature.rs),
[updater 2.14.0 verification](https://github.com/tauri-apps/plugins-workspace/blob/updater-v2.14.0/plugins/updater/src/updater.rs).
