# 0019 Updates

Tauri's updater, run from Rust only. Opt-in, asked once; it sends one HTTPS
request to GitHub for a small feed file, and installs only after the person
agrees. Packages are signed with the Ibis update key, built into the app.
Nightly and stable keys are separate; the stable key is usable only in a
protected environment the maintainer approves. Ibis and Ibis Nightly are two
apps with their own identifier, settings, shortcut and feed. Update checks,
offers and failures never interrupt work: an available update never shows or
focuses the picker; it waits in More, the tray and Settings with one sentence
on what changed. An opt-in "Download quietly, install when I quit" downloads
and verifies in the background and installs only on an explicit quit; asking
first stays the default (maintainer, 2026-10-28).

2026-10-30: PR #1504 review found that an automatic dialog could steal Enter
from Search and install. Automatic offers now wait for empty, idle input; all
native defaults decline, and only deliberate Install activation accepts. Idle
reports carry one-use interaction ownership and cannot revive after hide/reopen.
No input text is sent to the updater. See the native-source audit in
[testing](../testing.md#update-consent-keyboard-safety).

Stable deployment retains the maintainer's one-click GitHub approval.

2026-10-31: Numbered releases are full GitHub releases titled “Ibis X.Y.Z”, tagged
`vX.Y.Z`, with the highest version carrying Latest; this replaces marking every
numbered release pre-release so GitHub shows one Latest and `/releases/latest`
works. Before 1.0, preview status stays in the notes and website. “Ibis Nightly”
uses the rolling `nightly` tag, with its version in the notes; “Ibis update feed”
uses the fixed `stable` tag, whose release's files are replaced. Both are
pre-releases and never Latest.
