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

The shipped shortcut backend needs a reachable X11 server with XKB. If that
check fails, the picker explains the problem; open Ibis from the application
launcher. Launching it again brings the running picker forward.

An XWayland connection may pass the X11 check in a Wayland session. That does
not establish shortcut activation while a native Wayland app has focus.
The GlobalShortcuts portal work in [#782](https://github.com/ibiscite/ibis/issues/782)
is scheduled for 0.2 but has not shipped. No native GNOME/KDE Wayland shortcut
test is claimed.

GNOME without a tray extension and some other desktops show no tray icon.
The launcher and picker library controls still work. Use **More > Quit** to
exit; closing the window only hides it. Nightly's start-at-login switch is
also in **More**, and so are the update controls.

### Bind a system shortcut

**Nightly, until the next release:** `ibis toggle` starts Ibis if needed, shows a hidden picker, or hides one (even pinned).
Ordinary launches show the picker. This uses the existing single-instance channel without network ports or extra permissions.
On Wayland it still attempts the in-app shortcut; if registration fails, the picker explains the system binding.

The developer CLI `ibis` is separate from the desktop download. It first finds `ibis-desktop` beside itself;
otherwise it uses PATH on Linux, the installed **Ibis** app on macOS,
or `%LOCALAPPDATA%\Ibis\ibis-desktop.exe`, then `%ProgramFiles%\Ibis\ibis-desktop.exe`, then PATH on Windows. Build the desktop app first when running from source.
`--app` overrides discovery: on macOS use `ibis toggle --app "Ibis Nightly"` or
`ibis toggle --app "/Applications/Ibis Nightly.app"` (passed to `open -n -a`); on Linux/Windows pass the desktop
executable path, including for Nightly or custom installs. Without the CLI, bind `ibis-desktop toggle` directly.
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
  See [Hyprland's binding guide](https://wiki.hypr.land/Configuring/Basics/Binds/).

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
cannot see ([#1129](https://github.com/ibiscite/ibis/issues/1129)). Emacs org-cite, MyST
and natbib key forms are planned for 0.2.

## What the tests establish

`scripts/smoke-native.py` checks a narrow case on each hosted OS: the default
keyboard layout, one display, a synthetic BibTeX file at a path with spaces
and non-Latin characters, Pandoc clipboard text and focus return to an ordinary
editor window. Linux uses AT-SPI/xdotool; Windows uses UI Automation/SendInput;
macOS needs System Events Accessibility and Automation permissions.

Only a completed run with passed `smoke.json` establishes that case. A test
definition or a green build alone does not. These checks do not prove automatic
insertion, all citation formats, screen-reader speech, mixed-DPI displays,
sleep/wake, cloud folders, network drives or every editor. See
[testing](testing.md) for assertions and measurement limits and the
[remaining manual checks](https://github.com/ibiscite/ibis/issues/79#issuecomment-6282579792).
