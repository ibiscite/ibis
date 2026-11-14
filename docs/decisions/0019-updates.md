# 0019 Updates

Tauri's updater runs from Rust. Checks are opt-in and read a small HTTPS feed
from GitHub. Packages must verify against the built-in update key; stable and
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
