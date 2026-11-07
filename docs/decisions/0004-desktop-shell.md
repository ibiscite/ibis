# 0004 Desktop shell (with 0012)

Tauri 2 with a React interface, in one process: the picker window is hidden
between uses, and the shortcut and library live in the same process. On
Wayland the shortcut uses the XDG GlobalShortcuts portal, with `ibis toggle`
as a fallback people can bind. The desktop owns the trigger; Ibis never saves
it to `settings.json` or registers an XWayland grab. Portal activation tokens
stay native and are consumed through GTK's safe window API (already linked by
Tauri), never sent to the WebView or stored (2026-10-28, #782).

2026-10-31 (maintainer): reopened before 1.0. Tauri stays unless it fails a release
gate that the shell itself causes (whole-process memory including WebView helpers,
summon and typing latency, idle use, screen readers, input methods, focus for
paste) and an alternative passes that gate for less than fixing the cause in Tauri.
