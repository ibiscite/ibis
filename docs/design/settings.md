# Settings window and shortcut customization

This page specifies the Settings window, its pages, the global shortcut
recorder and how settings are stored. It sits beside the picker spec in
[design.md](../design.md) and uses the tokens and components in
[system.md](system.md). Where an accepted ADR disagrees, the ADR wins.

Release scope (maintainer, 2026-10-21): a minimal Settings window with
**General, Shortcuts and Libraries** ships in 0.3, and the global shortcut must
be customizable. Citations and Advanced follow with 0.3 styles and 0.4
insertion. Work is tracked in #776 and its sub-issues.

## Summary

1. **Settings is its own window, not a view inside the picker.** It is a normal
   window: it shows in the Dock, taskbar and Alt+Tab, holds focus, and stays open
   while you try a new shortcut in another app. The picker stays a quick,
   light-dismiss panel. The window is created when opened and destroyed when
   closed, so it costs no memory while unused.
2. **Five pages: General, Shortcuts, Libraries, Citations, Advanced.** About
   fifteen visible controls in all. Every control has a working default, and
   every change applies at once, with no Save button. Rare things sit on Advanced.
3. **The picker keeps what you change while writing** (format menu, switching or
   narrowing libraries, Reload, Keep Window Open). **Settings keeps what you set
   once** (shortcut, startup, look, which libraries exist, which styles the format
   menu shows, paste behaviour, privacy, reset and export).
4. **One shortcut recorder, drawn natively on each OS:** an inline field on
   macOS, PowerToys-style key caps and dialog on Windows, GNOME Settings' "Set
   Shortcut" dialog on GNOME. On Wayland, Ibis records nothing and hands the
   shortcut to the desktop's own dialog through the GlobalShortcuts portal.
5. **The recorder never leaves you without a working shortcut.** It registers
   the new keys before releasing the old ones, warns before keys that editors
   type with, and offers "Choose them from a list" for screen-reader and switch
   users.
6. **No new network use.** Any online feature that later appears in Settings
   needs its own ADR and the maintainer's yes.

All three platforms share one content model: the same wording, order and
grouping everywhere. Only the native rendering differs.

## 1. Information architecture

### 1.1 Where a choice lives

| Lives in… | When | Examples |
|---|---|---|
| **Inline in the picker** | Changed while writing, possibly several times a day | Format and style menu, filters, page number |
| **More menu (⌘K / Ctrl+K)** | An occasional action tied to the moment | Keep Window Open (⌘⇧P / Ctrl+Shift+P), switch library, narrow Zotero libraries, Reload, Keyboard shortcuts, **Settings… ⌘,**, Quit |
| **Menu bar or tray menu** | Things the OS expects there, or that must work without the picker | Open Ibis, library line, Reload, Check for Updates, Start at Login, **Settings…**, Report a Problem, Quit |
| **Settings window** | Set once or rarely; needs explanation; or destructive | Everything in 1.2 |
| **Not a setting** | The system owns it, or a good default makes it unnecessary | Accent colour, contrast, reduced motion and transparency, system text scale, watching `.bib` files, the Copied notice position |

**Keep Window Open** (decided in #757): pinning leaves the header. It is a
checkable item in the More menu with ⌘⇧P (Ctrl+Shift+P elsewhere), defined in
`keys.ts`. While pinned, the footer's status area shows a small pin
glyph with the tooltip "Kept open; click to stop", and clicking it unpins. When
not pinned, nothing shows. The header ends with the format selector, aligned to
the content edge. In 0.3, #571/#572 add the one-time "an outside click hides
Ibis" tip, naming the menu item and its shortcut.

Once the Settings window exists, **Density, Larger text and Start at login leave
the More menu.** Start at login stays in the menu bar or tray menu, where
Raycast, Alfred and PowerToys users look for it; the system keeps the real value
(`login_item.rs`).

### 1.2 Pages

Defaults are in **bold**. *Exists*: in `settings.json` or the code today. *New*:
needs an ADR before it adds a field. *Planned*: tied to a roadmap milestone.

**General**

| Setting | Control | Default | Status |
|---|---|---|---|
| Start Ibis when you log in | switch, read from the system each time | **off** until turned on (first run offers it) | exists, not in `settings.json` |
| Light or dark | segmented: **System** / Light / Dark | System | new ([system.md](system.md) §14) |
| Result rows | segmented: Compact / **Comfortable** / Spacious | Comfortable | exists (`density`) |
| Larger text | switch (three steps later, #775) | **off** | exists (`larger_text`) |
| Keep the last search for | pop-up: Don't keep / **1 minute** / 3 minutes | 1 minute | new |
| Show tips in the footer | switch | **on** | new |
| Check for updates automatically | switch, with what is sent written underneath | asked once at first run | exists (`update_checks`) |
| Version · last check · Check Now | info row with a button | | exists (tray) |

Accent colour is never offered (system D6).

**Shortcuts**

| Setting | Control | Default | Status |
|---|---|---|---|
| Open Ibis (global) | shortcut recorder (§2) | **⌃⌘R** macOS, **Ctrl+Shift+Y** Windows and Linux; first run suggests a free one (§2.9) | exists (`shortcut`); recorder new |
| In the picker | the `KEY_TABLE` list, same words as the `?` overlay | the table | read-only until the 0.4 action registry |
| Reset Shortcuts | button, disabled while everything is default | | new |

**Libraries**

| Setting | Control | Default | Status |
|---|---|---|---|
| Search these libraries | one row per source: status line (the footer's words) and an include switch; Add Library…: Zotero library / `.bib` file… / folder of `.bib` files… | the first-run choice | exists for one file or several Zotero libraries; mixed sources need settings format 3 (ADR 0018) |
| Row details (click the row) | Reveal in Finder/Explorer/Files, Reload, Remove (applies at once, 8 s Undo) | | new UI, no new data |
| When Zotero is closed | pop-up: **Read the database** (read-only snapshot) / Don't search Zotero | read | planned 0.3 (#14) |

The group footer says "Ibis only reads your libraries. It never changes them."

**Citations**

| Setting | Control | Default | Status |
|---|---|---|---|
| When you press Enter, copy | one grouped pop-up: *Citation keys* (Pandoc, \autocite, \parencite, \textcite, \cite, Typst, bare keys) and *Styles* (the five bundled; any CSL style from 0.3) | **Pandoc** | exists (`copy_format` + `citation_style`) |
| Preview | a synthetic reference in the chosen format, drawn as the citation shelf draws it ([preview.md](preview.md)) | | new UI |
| Show what will be pasted | switch (also ⌘I / Ctrl+I in the picker; in More until Settings exists) | **on** | exists (`show_paste_preview`, ADR 0020) |
| Format menu | tick and drag: which formats and styles the format menu shows, in order | the seven key formats + five styles | new (`format_menu`), needed once 0.3 adds about 2,600 styles |
| After you choose a reference | pop-up: **Paste into the app** / Copy only | Copy only until 0.4 insertion ships, then Paste | planned 0.4 |
| Put the clipboard back afterwards | switch | **on** | planned 0.4 |

Plain versus rich text is not a setting: both go on the clipboard and the
receiving app chooses.

**Advanced**

| Setting | Control | Default | Status |
|---|---|---|---|
| Privacy and network | one row per network flow, each saying what is sent; mirrors `PRIVACY.md` | all **off** except what the person said yes to | update checks exist; everything else reserved (§6) |
| Remember recently cited references · Clear History | switch + button; clearing applies at once with an 8 s Undo and deletes only after it | **on** once history exists | planned (rollout step 6) |
| Export… / Import… | buttons | | new (§5.3) |
| Reset all settings | button; applies at once with an 8 s Undo and the `.bak` copy | | new |
| Copy diagnostic report | button: allowlisted fields to the clipboard, never uploaded | | optional, matches `diagnostics.rs` |

**Extensions** appears as a sixth page only once the first curated extension
ships.

### 1.3 Progressive disclosure

- Level 1: the rows above, at most about seven per page.
- Level 2, one click: library row details, "Choose them from a list" in the
  recorder, the Windows SettingsExpander (picker keys collapsed by default).
- Level 3: Advanced, then `settings.json` itself. Unknown fields are kept
  (ADR 0022), so hand edits survive.
- Search: ⌘F / Ctrl+F. Windows shows "Find a setting" in the NavigationView;
  GNOME shows the AdwPreferences search button; macOS shows none (five short
  panes), and ⌘F jumps to the first matching row.

## 2. Shortcut customization

### 2.1 Recorder anatomy (`ShortcutRecorder`)

Parts: **label** (the action name) · **key caps** (the current shortcut in
platform order and words: ⌃⌥⇧⌘ on macOS; Ctrl, Alt, Shift, Win or Super
elsewhere) · **change affordance** (the field on macOS, a pencil button on
Windows, the whole row with a chevron on GNOME) · **clear** (× inside the macOS
field; Backspace in the dialog on Windows and GNOME) · **reset** (↺, only when
the value differs from the default; its tooltip names the default) · **message
line** (status icon + bold first sentence + explanation + at most one action) ·
**list fallback** ("Can't press the keys? Choose them from a list").

The field is 32 px high; key caps use the UI font with tabular numerals and are
never scarlet.

| State | What shows | Accessible name or announcement |
|---|---|---|
| Rest | current keys | "Open Ibis shortcut, Control Command R. Button. Press to change." |
| Hover / keyboard focus | native hover; focus ring only for keyboard focus | |
| Recording, empty | macOS: field tinted with the accent at 16 %, "Press keys…"; Windows and GNOME: the dialog opens | polite: "Recording. Press the new shortcut, or Escape to cancel." |
| Recording, partial | live modifiers plus an empty dashed cap | not announced |
| Invalid | the broken rule in words (§2.2); nothing saved | assertive: the sentence |
| Hard conflict | Look status, the reason, the owner if known, a "change it there" action; Save disabled | assertive |
| Soft conflict | Look status, the reason, **Use Anyway** + Choose Another | assertive |
| Saved | Done status: "Saved. Try it now: press ⌃⌥C in any app." Becomes "It works" when Ibis receives the press | polite |
| Cleared | "None". Message: "Open Ibis from the menu bar instead." | polite |
| System-managed (Wayland portal) | the keys the desktop reports + **Change…**, which opens the desktop's dialog | |
| Unavailable | the explanation from `picker.rs` + an "Open from your app launcher" hint | |

Per OS:

- **macOS:** an inline field in the row (the sindresorhus/KeyboardShortcuts,
  Raycast and Alfred pattern). Messages appear inline under the row, never as an
  alert. Reset is a borderless ↺ beside the field.
- **Windows 11:** PowerToys' ShortcutControl. The card shows accent-filled key
  caps and a pencil button that opens a ContentDialog ("Shortcut to open Ibis",
  live key caps, an InfoBar with the rule or conflict, Save / Reset to default /
  Cancel). Save is enabled only for a valid, free combination. After saving, a
  success InfoBar sits in the card's expander.
- **GNOME:** the row shows a GtkShortcutLabel and a chevron and opens the "Set
  Shortcut" dialog of GNOME Settings › Keyboard: Cancel / Set in the header bar,
  a keyboard illustration, "Press Esc to cancel or Backspace to remove the
  shortcut." A conflict replaces the illustration with a warning banner. After
  saving, an AdwToast with **Undo**.

### 2.2 How it records, and the rules

1. **Suspend first.** Recording unregisters the current global shortcut, so
   pressing the old keys records them. Cancel, Escape, closing the window or
   losing focus re-registers it.
2. **Capture natively where the webview cannot see keys** (keys the OS or another
   app already consumed):
   - Windows: a temporary `WH_KEYBOARD_LL` hook while the recorder is active and
     Settings has focus, as PowerToys does; no permission; removed when
     recording ends.
   - X11: an active keyboard grab while recording.
   - macOS: no event tap (it needs Input Monitoring). When modifiers are pressed
     and released with no key in between, show "Nothing arrived. macOS may be
     using that shortcut."
   - Wayland: not recorded by Ibis (§2.6).
3. **Commit** on the first non-modifier key-down. Auto-repeat is ignored;
   releasing all keys with no key pressed keeps recording.
4. **Validity**, always explained in words:
   - At least one of ⌘/Ctrl/Alt(⌥)/Win(Super) plus a key. Shift alone is
     refused. F13–F24 may be used alone; F1–F12 alone get a soft warning.
   - Escape, Tab, Enter, Space alone and arrows alone are refused.
   - Fn/Globe and media keys cannot be recorded.
5. **Swap safely.** Register the new combination, then unregister the old one,
   then write `settings.json` with the locked read-change-save (ADR 0022). If
   registration fails, the old one stays active and a hard conflict shows.
6. **Prove it.** After a save, the next real press of the new shortcut while
   Settings is open turns "Try it now" into "It works" and does not open the
   picker. This catches conflicts no API can list.

### 2.3 Conflict rules

| Source | macOS | Windows 11 | GNOME / Linux |
|---|---|---|---|
| OS-reserved, static list (hard) | ⌘Space, ⌃Space, ⌘Tab, ⌘`, ⌘⇧3/4/5, ⌃↑/↓/←/→, ⌘⌥Esc, ⌃⌘Q, ⌃⌘F, Globe+key | Win+L, D, E, R, I, V, period, Space, Tab, number keys, Win+Shift+S, Ctrl+Alt+Del, Ctrl+Shift+Esc, Alt+Tab, Alt+F4, Ctrl+Esc, F12 | Super alone, Super+A, Super+V, Super+L, Super+Space, Alt+Tab, Alt+F2, Ctrl+Alt+T (Ubuntu), Print |
| Live system bindings (hard) | read `com.apple.symbolichotkeys` (read-only) | `RegisterHotKey` failure ("another app") | gsettings `org.gnome.desktop.wm.keybindings`, `org.gnome.shell.keybindings`, `org.gnome.settings-daemon.plugins.media-keys`; name the action |
| Other apps | not detectable → "It works" test | registration failure | X11 grab failure |
| Keys editors type with (soft, Use Anyway) | ⌥+letter or ⌥⇧+letter alone; ⌥Space | Ctrl+Alt+letter (AltGr); Ctrl+Shift+Space | as Windows; Ctrl+Space (input method) |
| Inside Ibis (hard) | would shadow a picker key (⌘K, ⌘/, ⌘Q, ⌘,) | same with Ctrl | same with Ctrl |

The static lists live in the core as data, one table per OS with a test, like
the Siri and non-breaking-space tests in `picker.rs`. Messages use the Look
status: an icon and words, never colour alone.

### 2.4 Global versus in-picker shortcuts

- **One global shortcut now** (Open Ibis). Per-action global shortcuts arrive
  with the 0.4 action registry, capped and optional, in the same Shortcuts page
  under "From any app".
- **In-picker keys** are read-only until then. From 0.4, editable rows get the
  same recorder scoped to the picker: a modifier is required, bare letters never
  (the search box always has focus). Enter, Shift+Enter, Esc, Tab, the arrows,
  Home and End stay fixed.
- Overrides are sparse (only changed actions are stored), so new defaults reach
  people who never customized.

### 2.5 Keyboard layouts

- **Store the physical key, show the current layout's character.**
  `global-hotkey` already parses "R" as `Code::KeyR`.
- **Labels come from the OS layout:** `UCKeyTranslate` on macOS,
  `ToUnicodeEx`/`MapVirtualKeyEx` on Windows, xkbcommon on Linux.
  `navigator.keyboard.getLayoutMap()` exists only in WebView2.
- **Non-Latin layouts:** label with the Latin letter of the ASCII-capable layout,
  as macOS menus do; the tooltip names both ("the key labelled R / ρ").
- **AZERTY:** the soft-conflict rule covers AltGr; digit keys show the digit.
- In-picker keys match by character on Latin layouts while global keys match by
  position; Settings labels both correctly.
- When the layout changes, labels update and the binding stays.

### 2.6 Wayland and the GlobalShortcuts portal

- Ibis creates a portal session and calls `BindShortcuts` with `open-ibis`, the
  description "Open Ibis" and Ctrl+Shift+Y as the *preferred trigger*. The
  desktop decides. Ibis receives `Activated`, and `ShortcutsChanged` when the
  person rebinds it; version 2 adds `ConfigureShortcuts`.
- **No recorder on Wayland.** The row shows the keys from `ListShortcuts` and a
  **Change…** button that calls `ConfigureShortcuts`; with version 1 it says
  "Change in System Settings" and where.
- The trigger belongs to the desktop and is **never** written to
  `settings.json`.
- Without the portal: today's explanation, and from 0.4 `ibis toggle` with a
  Copy button and "Open Keyboard Settings". On X11, record directly.
- Verify on real sessions before claiming support (KDE Plasma 5.27+,
  xdg-desktop-portal-gnome 48+, Hyprland) and record results in
  [compatibility.md](../compatibility.md). Tracked in #782.

### 2.7 Accessibility

- Tab reaches the recorder; Space or Enter starts recording. **Escape always
  cancels** (WCAG 2.1.2) and is never recordable. Tab without modifiers leaves
  and cancels.
- The recorder is a button with `aria-describedby` on the message line; one live
  region announces state (polite for progress, assertive for problems). The list
  fallback (modifier checkboxes plus a key pop-up, same rules) lets screen-reader
  and switch users set any combination.
- No timeouts; messages never rely on colour alone; targets 32 px or larger.
- The picker's bare `?` works only while focus is outside a text field
  (WCAG 2.1.4); once rebinding exists it is listed and can be turned off.

### 2.8 One key table

- `keys.ts` becomes *defaults + overrides = the resolved table*. The `?` overlay,
  footer hints, More menu labels and the Settings list all read it.
- The `?` overlay ends with "Change shortcuts in Settings ⌘," (opens Settings ›
  Shortcuts).
- Shortcut error notices gain **Change Shortcut…**, which opens Settings ›
  Shortcuts with the recorder already recording.

### 2.9 First-run shortcut

Decided in #756 (maintainer, 2026-10-21: the shortcut "must be customizable"
and "could be empty by default or suggested to first-time users"): **suggest a
shortcut that has been checked to be free, show it, and register it only once it
is visible. Never register one silently.**

- On a fresh install nothing is registered before first run is on screen.
- The suggestion is the OS default if free, otherwise the next free entry in an
  ordered per-OS list kept as data with tests. "Free" means it passes the
  reserved-key table and registers successfully.
- The first-run row shows it in key caps and is true from the moment it
  appears: "Press Ctrl+Shift+Y in any app."
- If nothing is free, the row says Ibis stays reachable from the menu bar or
  tray and the app icon. No error dialog.
- Existing installs keep their shortcut and behaviour.
- 0.3: first run gains **Change…** (this recorder, #778) and **Not now**, which
  stores "no shortcut" (ADR #783). While no shortcut is active, the tray reads
  "Set a Shortcut…".

## 3. The window

**Entry points:** ⌘, (macOS) or Ctrl+, in the picker (a new `KEY_TABLE` row,
`settings`; on macOS also the app menu while Settings is frontmost), "Settings…"
in More and in the menu bar or tray, the error notices' **Change Shortcut…**,
and "More settings…" at the end of first run. Label per OS in `messages.ts`:
"Settings…" on macOS and Windows; GNOME uses its primary-menu wording
("Preferences").

**Why a separate window:** the picker is a non-activating panel that hides on an
outside click, which fights long forms, file choosers and trying a new shortcut
in another app. A separate window matches HIG Settings, Fluent settings pages and
AdwPreferences (Raycast, Alfred and 1Password do the same), shows live changes
beside the picker through one `settings-changed` event, and gets its own
capability file (`capabilities/settings.json`), so settings and recorder
commands are granted to that window only. The cost is a second webview while it
is open; measure it against `scripts/smoke-budgets.json`.

- **macOS:** a titled window, title = the current pane name (system-drawn, 13
  semibold; no title in the content), a toolbar of icon tabs, fixed width
  680 pt, height animating per pane (200 ms; about 520 typical, minimum 440). Not
  resizable or full-screenable. ⌘W closes; Escape does not. Opens on the last
  pane viewed this session (memory only). While open, the activation policy is
  Regular (Dock icon, ⌘Tab, app menu), Accessory again after. The toolbar is
  web-drawn under `titleBarStyle: "Overlay"` until a native NSToolbar is proven
  better side by side.
- **Windows:** native title bar, Mica, 1000×700 default, resizable, minimum
  640×500, Snap layouts allowed. NavigationView collapses to compact below
  1008 px and minimal below 641 px. Page titles 28/36 semibold.
- **GNOME:** header bar with search, an AdwViewSwitcher (icon above label) and a
  close button; 640×600 default, minimum 360×294; the switcher moves to a bottom
  bar when narrow. Web-drawn with `decorations: false`, honouring
  `gtk-decoration-layout`.
- **Single instance:** a second "Settings…" focuses the window and switches to
  the requested pane. Opening Settings hides the picker unless Keep Window Open
  is on. Settings opens on the display with the pointer.

## 4. Components and their native forms

Shared everywhere: wording, order, grouping, icons, colour roles, status words,
a 12 px text minimum. Settings is a standard window, so it uses **native radii
and metrics, not the picker's concentric chain**. Soft depth (branch
`design/depth-prototype`, variant B) carries over as Windows layer cards on Mica
with a 1 px stroke, GNOME flat boxed lists and macOS grouped form boxes, with no
lifted rows.

| Component | Rule | macOS | Windows 11 | GNOME (libadwaita) |
|---|---|---|---|---|
| Page title | | in the title bar | 28/36 semibold | none (the switcher names it) |
| Group | heading 13 semibold, card padding 16, 24 between groups, page side 32 | rounded box radius 12, 0.5 px hairline | BodyStrong 14 header, cards 4 px apart | `.heading`, boxed list radius 12 with the card shadow |
| Row (system D17) | label 14/20, description 13/18 `--muted`, one control at the end; 48 min (56 with description) | pad 9×12, inset hairlines, no icons | SettingsCard min 68, pad 16, 20 px icon, radius 4 | AdwActionRow min 48, pad 8×12, no icons |
| Expandable row | | disclosure row | SettingsExpander (child rows indented 58) | AdwExpanderRow |
| Switch | | NSSwitch | ToggleSwitch with "On/Off" to its left | AdwSwitchRow (whole row toggles) |
| One of a few | segmented | NSSegmentedControl | Toolkit Segmented | AdwToggleGroup |
| One of many | pop-up | NSPopUpButton | ComboBox | AdwComboRow |
| Ordered multi-choice | pick list | checkboxes + drag handle | CheckBox rows + drag | check prefix rows + drag |
| Button | accent for primary only | push button, Title Case | sentence case | Title Case; `suggested-action`, `destructive-action` |
| Destructive | never confirm; 8 s Undo | red label | neutral button | `destructive-action` |
| Key caps | | ⌃⌥⇧⌘ in HIG order, light caps | accent-filled (editable), neutral (fixed) | GtkShortcutLabel |
| Message | Done / Look / Stop, icon and words | inline note under the row | InfoBar | dialog banner; AdwToast with Undo after a save |
| Focus ring | system D8 | 2 px accent, 1 px gap | Fluent double stroke | half-accent mixed toward text, inside |
| Source tile | neutral tile + type glyph | | | Zotero is not drawn in brand red |

Colour roles hold: the system accent is for switches, selection and primary
buttons. Scarlet never appears on a Settings control or the recorder; the
Citations preview uses the shelf's rule from [preview.md](preview.md) (text
colour with a small scarlet quote mark).

White on the default dark macOS accent measures 3.65:1 and on GNOME blue 3.77:1.
Switches and primary buttons keep the platform pair; no scarlet or extra tint is
added to compensate.

### Alignment with the design system

Settled between the two specs and already reflected in [system.md](system.md)
D13 and D17:

- Windows page titles use `--type-title-large` 28/36 (Fluent Title).
- Windows SettingsCard radius is 4 (`ControlCornerRadius`); overlays and
  dialogs stay 8.
- macOS uses a toolbar of icon tabs, 680 wide, height per pane, not a sidebar.
- macOS names the pane in the title bar instead of a title in the content.

Also adopted from system.md: role tokens only; never confirm, offer an 8 s Undo
(D16); the §14 customization scope (light or dark override, footer tips, no
accent override); D8 focus rings; Mica for the Windows Settings window.

## 5. Data model (`settings.json`)

### 5.1 What holds today

Format 2, `schema_version: 2`, with ADR 0022's rules: unknown top-level fields
kept, saves flushed, `.bak` recovery, newer files refused. Writes go through one
locked read-change-save in Rust. Both windows listen for one `settings-changed`
event. The login item stays with the system and never goes into the file.

`show_paste_preview` is the default-on picker preference for "Show what will be
pasted" (ADR 0020). Only `false` is written; hiding output keeps citation chips.

### 5.2 New fields (each needs an ADR first)

| Field (proposed) | Shape | Notes |
|---|---|---|
| `shortcut` (existing) | add `""` meaning "no global shortcut" | ADR #783; an absent field still means the default |
| `keys` | sparse map, action ID → accelerator | 0.4 (ADR #783). Unknown action IDs are kept and ignored, so a Nightly action does not invalidate the file for Stable |
| `format_menu` | ordered list of format and style IDs | 0.3; unknown IDs tolerated per entry |
| `keep_search` | seconds: 0, 60 or 180 | only the duration is stored |
| `paste` | `{ "after_choose": "paste" \| "copy", "restore_clipboard": bool }` | 0.4, with the insertion ADR |
| `history` | bool | PRIVACY.md must list it |
| `libraries` | ordered list of sources with include flags | mixed sources: **settings format 3** with a `v2_to_v3` step (ADR 0018) |

Rule for that ADR: **unknown values inside list or map fields are skipped per
entry.** Only scalar fields with a closed set (density, `copy_format`) keep ADR
0020's whole-file rejection.

### 5.3 Export, import and reset

- **Export** writes the same `settings.json` format. It contains library paths,
  and the dialog says so.
- **Import** validates through the normal reader, shows a one-line summary
  ("Shortcut, 3 libraries, style APA"), keeps the current file as `.bak`, and
  never imports `update_checks` silently: the person is asked again (ADR 0019).
- **Reset** writes defaults at once with an 8 s Undo and the `.bak` copy. It
  does not touch the login item; the toast adds "Start at login is still on"
  with a link to that switch.
- Settings sync through a synced folder (ROADMAP 0.5) is a later ADR.

### 5.4 What needs an ADR

Every new persisted field; settings format 3; the Settings window's own
capabilities (a security boundary); the `keys` contract once extensions declare
shortcuts; any network use, including style downloads.

## 6. Not decided here

Reserved for the maintainer (AGENTS.md): any online feature in Settings
(metadata lookup, verification, the Zotero web library); whether CSL styles are
bundled or fetched (recommendation: bundle, to stay offline by default); any
extensions catalogue.

Product defaults open to the maintainer's override: the shortcut defaults stay
(#133); "Keep the last search" stays at 1 minute; Start at login stays off until
turned on (App Review 2.4.5(iii)); the history switch is on once history exists.

Engineering questions to settle with evidence: whether
`PushSymbolicHotKeyMode` lets ⌘Space reach the recorder without extra
permission; web-drawn versus native macOS toolbar; the minimum portal versions;
the Settings window's memory per OS; reaching Settings when the shortcut is
unavailable and there is no tray (proposed: launching Ibis again opens it);
whether security tools flag the Windows recording hook in signed builds.
