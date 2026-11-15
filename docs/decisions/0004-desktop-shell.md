# 0004 Desktop shell

Tauri 2 with a React interface, in one process. The picker is hidden between
uses; the shortcut and library stay in that process.

On Wayland the shortcut uses the XDG GlobalShortcuts portal. People can bind
`ibis toggle` when the portal is unavailable. The desktop owns the trigger:
Ibis neither saves it to `settings.json` nor registers an XWayland grab.
Portal activation tokens stay native and are consumed through GTK's window API.

Tauri stays unless the shell causes a release-gate failure and an alternative
passes that gate for less work than fixing Tauri. The gates cover whole-process
memory, latency, idle use, screen readers, input methods and paste focus.
