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
- **Two apps, two feeds.** Nightly builds install as **Ibis Nightly** with their own app identifier, settings and shortcut, so they can sit next to the stable app and never touch its settings. Each app reads only its own update file at a fixed address: `releases/download/nightly/latest.json` for Ibis Nightly, and `releases/download/stable/latest.json` for Ibis, where `stable` is a rolling pre-release that mirrors the newest numbered release (it also gives fixed download links for the stable app). Nightly versions are `X.Y.Z-nightly.YYYYMMDDHHMM`, one patch above the last release, so they always increase.
- Failures (offline, GitHub unavailable, bad signature) never interrupt work: "Check now" says what went wrong; automatic checks stay quiet and try again later.

## Consequences
- One new dependency (`tauri-plugin-updater`, maintained by the Tauri project), a `PRIVACY.md` entry and a README note.
- The maintainer creates the update key once and stores it as an environment secret, with an offline backup. Losing it means testers reinstall once by hand; rotation follows a written procedure before 1.0.
- Testers of today's nightly (installed as "Ibis") move to "Ibis Nightly" once, by hand.
- macOS and Windows still warn on first install until the apps are code-signed; updates after that install without a new download.
- Linux updates cover the AppImage; `.deb` and `.rpm` installs are updated by hand until a package repository exists.
