# 0041 System appearance follows the Linux desktop's dark preference

2026-11-07 (design pass 2, from the maintainer's Linux retest on #1731). On
Linux, Appearance › System follows the desktop's dark preference. Before, Ibis
stayed light under `GTK_THEME=Adwaita:dark`.

- **What counts as dark:**
  - the desktop portal's colour scheme (`org.freedesktop.appearance
    color-scheme` is 1, prefer dark), which GNOME and KDE set;
  - or a GTK theme whose variant or name says dark (`GTK_THEME=Adwaita:dark`,
    `Breeze-Dark`).
- **How:** WebKitGTK takes `prefers-color-scheme` from GTK's prefer-dark
  setting, which neither of those changes. So Ibis reads them once at start,
  off the main thread and within two seconds, and sets the app's theme to dark
  when either says so (`desktop_theme.rs`).
- **Light and Dark win.** Choosing Light or Dark in Appearance still sets
  `color-scheme` on the page, so the desktop only decides System.
- **Not yet:** a change of desktop preference while Ibis runs takes effect at
  the next start. Following it live waits until someone needs it.
