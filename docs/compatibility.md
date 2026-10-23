# Compatibility and limits

Ibis is a copy-only preview: find a reference, copy its citation, then paste it
yourself. Automatic insertion, destination profiles, clipboard restoration and
input simulation are planned, not shipped. Copying replaces the clipboard;
your clipboard manager may keep a copy.

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
test is claimed. `ibis toggle` and portal-based insertion are also unshipped.

The #782 portal client awaits picker integration. Desktop triggers are not
persisted. Real GNOME/KDE sessions remain unverified; unit/fake-portal tests do
not establish compositor activation or focus behavior.

GNOME without a tray extension and some other desktops show no tray icon.
The launcher and picker library controls still work. Use **More > Quit** to
exit; closing the window only hides it. Nightly's start-at-login switch is
also in **More**. Update controls require the tray menu.

## Picker shortcuts

The defaults are **Control+Command+R** on macOS and **Ctrl+Shift+Y** on
Windows/X11. Registration can fail if another app owns the shortcut; check the
picker's shortcut warning. A shortcut setting uses modifiers and one key
joined by `+`; `CommandOrControl` means Cmd on macOS and Ctrl elsewhere.
Invalid settings report a warning and use the default.

On Windows and X11 the default follows the key labelled Y. On macOS it refers
to the physical US-keyboard R position; the label does not follow the active
layout (#196). Non-English keyboard coverage remains incomplete. Avoid
Ctrl+Alt combinations on Windows because many layouts use them as AltGr.

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
[testing](testing.md) for assertions and measurement limits, the
[coverage ledger](https://github.com/ibiscite/ibis/issues/330#issuecomment-6282580538)
and the [remaining manual checks](https://github.com/ibiscite/ibis/issues/79#issuecomment-6282579792).
