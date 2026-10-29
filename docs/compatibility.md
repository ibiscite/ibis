# Compatibility and limits

Ibis is a copy-only preview: find a reference, copy its citation, then paste it
yourself. Pasting into the previous app is planned for 0.3 on macOS, then
Windows, leaving the citation on the clipboard; destination profiles come after 1.0. Copying
replaces the clipboard; your clipboard manager may keep a copy.

## Platforms: tested and targeted

The download names describe builds, not a tested minimum operating system.
The oldest supported versions are still being established in
[#371](https://github.com/ibiscite/ibis/issues/371).

| Platform | Build target | Recorded automated coverage |
| --- | --- | --- |
| Windows | x64 desktop Windows | Windows Server 2025 hosted runner (`windows-2025`), including installer and picker smoke checks. This is not a Windows 10 or 11 desktop test. |
| macOS | Apple silicon and Intel | macOS 15 Apple silicon hosted runner, including installed-app and picker smoke checks. Intel is built but not installed or exercised by that job. Nightly's Open at Login uses macOS 13+ APIs; that does not establish a tested minimum for the whole app. |
| Linux | x64; packages built on Ubuntu 22.04 | Ubuntu 24.04 hosted runner under X11 (Xvfb/Openbox): `.deb` picker smoke checks and extracted AppImage startup. This does not verify an Ubuntu 22.04 desktop, Fedora or native Wayland. |

Windows and Linux ARM64 builds are planned. A build succeeding does not prove
keyboard, display, accessibility or installation behavior on other systems.

**Windows 10:** Ibis has no recorded Windows 10 desktop acceptance test.
Windows 10 22H2's regular support ended on October 14, 2025; ESU and LTSC have
separate lifecycles. Microsoft's WebView2 updates for Windows 10 22H2 continue
until at least October 2028, without requiring ESU. Runtime support is not an
Ibis compatibility guarantee or continued support for Windows itself.

**Windows runtime:** Ibis needs Microsoft Edge WebView2 Runtime. The installer
downloads Microsoft's bootstrapper if the runtime is missing, so that setup
needs internet access. An organization can deploy the runtime in advance.
Ibis can then search and copy offline. WebView2's shared Evergreen runtime has
its own Microsoft update service, separate from Ibis's opt-in app updates.

## Installation blocks

On Windows, SmartScreen may offer **More info > Run anyway** for an
unrecognized download. Continue only if you trust the release source.
Smart App Control can block unsigned files without that option and has no
per-app exception. Report the block to Ibis and wait for a trusted, signed
build. On work or school devices, ask IT to review and approve the app under
their policy. Keep security protection enabled; a signed build can still be
blocked by policy or reputation checks.

**AppImage says FUSE is missing:** on Ubuntu 24.04, install `libfuse2t64`
(`sudo apt install libfuse2t64`); Ubuntu 22.04 uses `libfuse2`. Keep FUSE 3
installed. Other distributions use their own FUSE 2 compatibility package.
Alternatively, use the `.deb` from the same Stable or Nightly release page
on Debian/Ubuntu, or run `APPIMAGE_EXTRACT_AND_RUN=1 ./Ibis-Linux.AppImage`.
CI checks that the extracted AppImage starts and stays up; it does not run the
full picker smoke test through that fallback.

## Linux shortcuts and tray

X11 shortcuts need a reachable X11 server with XKB. On Wayland Ibis instead
uses the local XDG GlobalShortcuts portal, even when XWayland is available.
It requests `open-ibis` ("Open Ibis"), preferring Ctrl+Shift+Y. Settings >
Shortcuts shows the trigger reported by `ListShortcuts` and `ShortcutsChanged`.
**Change…** opens `ConfigureShortcuts` on portal version 2; version 1 gives
the desktop settings location. The trigger is never saved by Ibis.

Without a working portal, or after cancellation/session loss, Ibis explains
the launcher and system-binding fallback, including exact Sway and Hyprland
configuration lines. Restart Ibis to retry the portal; there is no hidden
polling or automatic consent retry.
Successful sessions with no usable trigger (including an empty description)
also show the exact binding instructions. The portal session stays active for
activation and configuration; a later reported trigger clears that guidance.

Only these tested sessions are claimed. Candidates and hands-on evidence are
tracked per desktop in [#1387](https://github.com/ibiscite/ibis/issues/1387).

| Desktop/session | Actually tested | Remaining verification |
|---|---|---|
| Openbox/X11, Ubuntu 24.04 hosted runner | Existing Xvfb/Openbox `.deb` picker and extracted AppImage startup smoke | Physical desktop keyboard/focus behavior |
| KDE Plasma Wayland (5.27+ candidate) | No real session tested | Record Plasma and portal versions; consent, activation from a native Wayland app, focus token, trigger changes, v1/v2 settings, restart |
| GNOME Wayland (portal-gnome 48+ candidate) | No real session tested | Record GNOME and portal versions; same checks, including permission refusal |
| Sway / wlr portal | No real session tested | Record versions; absent-portal explanation, copied config binding, toggle from a focused app |
| Hyprland | No real session tested | Record versions and config format; portal capability or exact fallback binding, toggle and focus |

Automated private-bus tests cover the portal protocol, version gating, consent,
activation, desktop trigger changes and owner loss; they do not establish
compositor behavior. Settings tests cover the recorder-free flow and failures.

GNOME without a tray extension and some other desktops show no tray icon.
The launcher and picker library controls still work. Use **More > Quit** to
exit; closing the window only hides it. Nightly's start-at-login switch is
also in **More**, and so are the update controls.

### Bind a system shortcut

**Nightly, until the next release:** `ibis toggle` starts Ibis if needed, shows a hidden picker, or hides one (even pinned).
Ordinary launches show the picker. This uses the existing single-instance channel without network ports or extra permissions.
On Wayland it uses the portal; if that is unavailable, the picker and Settings explain the system binding.

The developer CLI `ibis` is separate from the desktop download. It first finds `ibis-desktop` beside itself;
otherwise it uses PATH on Linux, the installed **Ibis** app on macOS,
or `%LOCALAPPDATA%\Ibis\ibis-desktop.exe`, then `%ProgramFiles%\Ibis\ibis-desktop.exe`, then PATH on Windows. Build the desktop app first when running from source.
`--app` overrides discovery: on macOS use `ibis toggle --app "Ibis Nightly"` or
`ibis toggle --app "/Applications/Ibis Nightly.app"` (passed to `open -n -a`); on Linux/Windows pass the desktop
executable path, including for Nightly or custom installs. Without the CLI, bind
`ibis-desktop toggle` directly (Linux Nightly `.deb`: `ibis-nightly toggle`).
The CLI defaults to Stable; for Linux Nightly use
`ibis toggle --app /usr/bin/ibis-nightly`. If shortcut registration fails, the
Wayland warning names the running app's executable, with its channel as fallback.
For an AppImage, use `"/path/to/Ibis-Linux.AppImage" toggle`. Use an absolute executable path if your desktop has
a different PATH; quote paths containing spaces and launch the same build/channel each time.

Choose an unused combination, for example Ctrl+Shift+Y:

- **GNOME:** Settings > Keyboard > View and Customize Shortcuts > Custom Shortcuts > Add Shortcut.
  Name it **Toggle Ibis**, set Command to `ibis toggle` (or the direct command above), and assign the keys.
  See [GNOME's shortcut guide](https://help.gnome.org/gnome-help/keyboard-shortcuts-set).
- **KDE Plasma:** System Settings > Keyboard > Shortcuts > Add New > Command or Script. Enter the same command,
  name it **Toggle Ibis**, assign the keys and Apply. Older Plasma versions call this **Add Command**.
- **Sway:** add `bindsym --no-repeat Ctrl+Shift+y exec ibis toggle` to `~/.config/sway/config`, then run `swaymsg reload`.
- **Hyprland 0.55+:** add `hl.bind("CTRL + SHIFT + Y", hl.dsp.exec_cmd("ibis toggle"))`
  to `~/.config/hypr/hyprland.lua`. For older hyprlang configurations, add
  `bind = CTRL SHIFT, Y, exec, ibis toggle` to `hyprland.conf`.
  See [Hyprland's binding guide](https://wiki.hypr.land/configuring/core/binds/).

Test from a terminal, then from your shortcut: closed app → visible picker → hidden picker → visible picker.
Native Linux toggle smoke and real GNOME/KDE/Sway/Hyprland focus checks remain follow-ups; argument tests do not establish compositor behavior.

## Picker shortcuts

The defaults are **Command+Shift+Y** on macOS and **Ctrl+Shift+Y** on
Windows/X11. Registration can fail if another app owns the shortcut; check the
picker's shortcut warning. A shortcut setting uses modifiers and one key
joined by `+`; `CommandOrControl` means Cmd on macOS and Ctrl elsewhere.
Invalid settings report a warning and use the default.

Earlier Ibis versions never saved the default shortcut. An unset or `null`
`shortcut` therefore picks up the new default automatically. Every explicitly
stored shortcut is kept, including `Control+Command+R`. To switch to the new
default, remove the `shortcut` setting and restart Ibis.

On Windows and X11 a shortcut letter follows the key labelled with it. On
macOS Ibis moves a letter to the key that types it on the active layout, so it
stays the labelled key on QWERTZ, AZERTY, Dvorak and Colemak, and follows a
layout switch within about two seconds. Command shortcuts use the layout’s
Command layer: Dvorak–QWERTY ⌘ therefore uses QWERTY while Command is held;
custom shortcuts without Command still follow Dvorak. When no key types the
letter (for example on a Cyrillic layout), the shortcut uses the letter's US-keyboard
place and the picker says so. Non-English keyboard coverage remains
incomplete. Avoid Ctrl+Alt combinations on Windows because many layouts use
them as AltGr.

## Writing apps

Ibis copies; you paste. What to copy depends on where you write. This table
comes from how each app accepts pasted text and from Zotero's own guidance; it
is not a test of every app. Reports help: use the "Writing app report" form.

| Where you write | What to copy from Ibis | Notes |
| --- | --- | --- |
| Markdown, Quarto, R Markdown, Obsidian, Zettlr | Pandoc keys (`[@key]`) | The document needs the matching `.bib` or CSL-JSON file when it is built. |
| LaTeX, Overleaf | BibLaTeX or LaTeX keys | Same: the project's `.bib` must contain the key. |
| Typst | Typst keys (`@key`) | The project needs a `.bib` or Hayagriva file. |
| Word, LibreOffice, Google Docs with Zotero citations | Nothing; use Zotero's plugin | Pasted text never joins Zotero's managed bibliography or changes with the style. |
| Word for the web, Pages, Notion, email | A formatted citation | Zotero has no plugin for these. Italics survive where the app accepts HTML. RTF arrives in 0.3 and bibliography entries in every style in 0.4. |
| Chat and plain-text fields | A formatted citation | Plain text drops italics. |

Numeric styles such as IEEE copy the reference entry without its number: the
in-text number depends on every reference your document cites, which one copy
cannot see ([#1129](https://github.com/ibiscite/ibis/issues/1129)). Emacs org-cite and
MyST key forms are planned.

Engine citation requests support Natbib (`"syntax": "natbib"`): normal,
narrative, author-only and year-only forms use its standard commands. Narrative
and author-only prefixes stay before the command and report flattening. A normal
multi-reference cluster retains key order; per-item locators, affixes and author
suppression report unsupported losses because Natbib has only one pair of notes
for the whole cluster. The other forms currently require one reference.
Author-only requests with a locator or suffix are refused: Natbib's numerical
mode silently omits those postnotes, and Ibis cannot know the document's mode.

## What the tests establish

`scripts/smoke-native.py` checks a narrow case on Linux/X11: the default
keyboard layout, one display, a synthetic BibTeX file at a path with spaces
and non-Latin characters, Pandoc clipboard text and focus return to an ordinary
editor window using AT-SPI/xdotool. The desktop build/tests and offline checks
still run on Linux, macOS and Windows; package install/start checks cover all three.

Only a completed run with passed `smoke.json` establishes that case. A test
definition or a green build alone does not. These checks do not prove automatic
insertion, all citation formats, screen-reader speech, mixed-DPI displays,
sleep/wake, cloud folders, network drives or every editor. See
[testing](../CONTRIBUTING.md) for assertions and measurement limits and the
[remaining manual checks](https://github.com/ibiscite/ibis/issues/79#issuecomment-6282579792).
