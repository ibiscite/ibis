# 0066 Native shells use the system's own parts

**Decided** 2026-11-28 by the maintainer, after testing the native Mac draft
(#2063): the interface felt "basic, badly emulated". His answers: rebuild
every screen at once, use real macOS controls with system materials
(Spotlight-like) rather than a custom-drawn look, review once at the end, and
plan for a native feel on every system later. It refines the shell part of
[0065](0065-one-warm-engine-thin-shells.md).

## Why

- The first native picker was AppKit drawn by hand: frames computed in
  points, custom buttons, capsules, rules, tokens and selection fills, PDF
  copies of the web icons, a scarlet accent and a 70% tint over the glass.
  It rebuilt the web view's imitation one level down, so it inherited the
  same drift.
- Spotlight, Raycast and Alfred feel native because their parts are the
  system's: the window material, text, symbols, menus, popovers, focus and
  accent all come from the OS, and the app only decides layout and content.
- Every system has one first-party toolkit that owns those parts: SwiftUI and
  AppKit on macOS, WinUI 3 (Windows App SDK) on Windows, GTK 4 with
  libadwaita on GNOME. Cross-platform toolkits that draw their own controls
  (Flutter, Slint, Qt Quick, a web view) reproduce the same imitation problem.

## The decision

1. **A shell is layout over system parts.** It uses the platform's controls,
   materials, symbols, menus, popovers, focus rings, selection colours and
   accent at their own metrics. It draws nothing the system already draws.
   The only custom shapes are the ones the platform has no part for (the
   citation token, the results selection row), and they take their colours
   from the system's semantic roles.
2. **macOS: SwiftUI first, AppKit where SwiftUI has no part.** SwiftUI owns
   the picker's content and Settings (`Form` with the grouped style, sidebar
   lists, `Toggle`, `Picker`, buttons, popovers). AppKit owns the
   non-activating `NSPanel`, its glass (`NSGlassEffectView` on macOS 26,
   `NSVisualEffectView` before), native `NSMenu` pop-ups, text fields that
   need exact focus and caret control, the hot key and the menu bar item.
   Icons are SF Symbols; the bird and the beak stay the brand's.
3. **The engine owns everything a second shell would otherwise copy.** State
   projections, words (the Fluent catalogue, generated per language), status
   and fixes already come from the engine. The command table (names, groups,
   search words, enablement) moves there next, with key bindings declared per
   system, so a Windows or Linux shell renders the same commands.
4. **Other systems, when a gate or reports call for them** (0065): Windows in
   WinUI 3 with Mica and Fluent controls, Linux in GTK 4 and libadwaita
   (gtk4-rs), each a client of `ibisd` with the contract generated for its
   language, and each following this decision. Until then the Tauri shell
   serves them with its own neutral recipe.

## Consequences

- The Mac shell's custom drawing, the picker PDF icon set and the scarlet
  accent are deleted. The accent is the person's system accent.
- Design review on the Mac is the running app, not the web playground; the
  playground stays for the Tauri shell.
- [macOS design guide](../design/macos.md) records the native recipe.
