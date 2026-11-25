# 0019 Updates

Tauri's updater runs from Rust. Checks read a small HTTPS feed from GitHub;
until 1.0 automatic checks are on by default and can be switched off
([0008](0008-offline-by-default.md)). Packages must verify against the built-in update key; stable and
Nightly have separate keys, app identifiers, settings, shortcuts and feeds.
Stable publishing requires the maintainer's one-click approval.

Update state never opens or focuses the picker. Offers wait for empty, idle
input; native defaults decline, and only deliberate Install activation accepts.
One-use interaction ownership prevents an idle report surviving hide/reopen.
No input text reaches the updater. Offers and failures stay available in More,
the native menu and Settings.

Numbered releases use `vX.Y.Z`, the title “Ibis X.Y.Z” and GitHub's Latest marker
on the highest version. Preview status before 1.0 is stated in the notes and
website. “Ibis Nightly” uses the rolling `nightly` tag; “Ibis update feed” uses
`stable`. Both are prereleases and never Latest.

Windows ships one package, the per-user NSIS `Ibis-Windows-setup.exe`, which
also carries Windows updates. The MSI was dropped (maintainer, 2026-11-14);
Nightly never had one, since Windows Installer versions cannot carry “nightly”.

2026-11-25 (maintainer, #50): numbered releases become immutable on GitHub
(Settings → General → Releases → Enable release immutability), switched on once
the release-hardening series lands. Every file, SBOMs included, is attached
while a release is a draft. The rolling `nightly` and `stable` releases
predate the setting and stay replaceable; they must never be recreated
([RELEASING.md](../../RELEASING.md#immutable-releases)).
