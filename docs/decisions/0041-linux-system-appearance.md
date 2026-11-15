# 0041 System appearance on Linux

Appearance › System follows the desktop portal's `org.freedesktop.appearance`
`color-scheme` value of 1 or a GTK theme whose name/variant says dark, such as
`Adwaita:dark` or `Breeze-Dark`. WebKitGTK does not derive its prefer-dark
setting from these, so `desktop_theme.rs` reads them off the main thread at
startup, within two seconds, and applies the app theme.

An explicit Light or Dark choice wins. Changes to the desktop preference take
effect at the next start.
