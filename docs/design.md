# Picker design

This is the agreed starting point for the picker redesign (#223), accepted by the
maintainer on 2026-10-19 after five rounds of prototypes. It describes what the
picker should look like and how its window should behave. It is brought into the
app in small steps (see [Rollout](#rollout)); until a step lands, the app keeps its
current appearance. User-selectable looks may come later; this is the default.

The prototypes were built outside the repository. Everything needed to implement
the design is written down here.

This includes longer-term targets as well as the current picker. Release scope
comes from [ROADMAP.md](../ROADMAP.md); the rollout below separates remaining
work from behavior already implemented. Native acceptance is recorded separately
in [testing.md](testing.md), not established by this specification.

## Seven rules

1. **One anatomy everywhere.** Search, list, detail, citation tray and footer, in
   that order, in every form from a one-line bar to the large settings window.
2. **Native material, Ibis voice.** Glass on macOS, Mica on Windows, flat on
   Linux. Type, spacing, icons and wording stay the same on every platform.
3. **Readable first.** Nothing smaller than 12 px, metadata at 13 px, titles at
   15 px. Secondary text passes 4.5:1 contrast.
4. **Easy to hit.** Every button is at least 32 px; 44 px on touch screens.
5. **Show less, reveal more.** An element appears when it becomes useful and
   steps back when it is not. Nothing is removed, only folded one key away.
6. **Say what happened.** Every action ends in a visible result: copied,
   inserted, or why not. Colour is never the only signal.
7. **Calm motion.** Movement only explains where something came from, and it
   switches off when the system asks for reduced motion.

## Design language

**Type.** The system font on each platform, one scale:

| Role | Size / line | Example |
|---|---|---|
| Search | 22/28 (16 on Windows and Linux, which use a framed text box) | Search references… |
| Title | 15/20, semibold | Sleep spindles predict memory |
| Body | 14/20 | Spindle density predicted recall. |
| Meta | 13/18 | García, Okafor · 2021 |
| Label | 12/16, semibold | Best matches |
| Citation | 13, monospace | `[@garcia2021, p. 4]` |

**Colour roles.** Selection and the main button use the system accent. Ibis
scarlet (`#c23a1c` light, `#ff7a5c` dark) marks only the citation itself: the
text that will be copied, the citation tray and the logo. Status uses Done
(copied, inserted), Look (skipped, needs a look) and Stop (unreachable, failed),
always with words or an icon as well as colour.

**Space and size.** A 4 px grid. Search bar 48–56 px, footer 40 px, icons 20 px
with a 1.5 px stroke. Rows come in three densities: Compact 40 px (one line),
Comfortable 58 px (default, two lines) and Spacious 76 px (three lines). A
Larger text setting scales the whole window by 1.15, not just the letters.

**Motion.** 120 ms for feedback, 200 ms for panels, 280 ms for layout, all with
`cubic-bezier(.2, .8, .2, 1)`. No bounce. With reduced motion, panels fade
instead of sliding. Hover never moves anything.

**Icons.** Ibis's own set: 20 px, 1.5 px stroke, rounded ends, the same on every
platform, so a source or a reference type always looks the same.

**Per platform.** Only these change:

| | macOS | Windows 11 | Linux (GNOME) |
|---|---|---|---|
| Material | Translucent glass | Mica, content on a lighter layer | Flat |
| Window radius | 20 px | 8 px (4 px controls) | 14 px |
| Selected row | Filled accent, white text | Soft grey with an accent pill on the left | Accent tint |
| Search box | Frameless, large | Text box with an underline that turns accent on focus | Framed entry |
| Shortcut labels | ⌘ ⌥ ⇧ | Ctrl Alt Shift | Ctrl Super Shift |
| Copied | Pill near the top of the screen | Notification bottom-right | Toast at the bottom |

High contrast and reduced transparency replace every material with the opaque
system surface, as [native appearance groundwork](https://github.com/ibiscite/ibis/pull/232#issuecomment-6282577029) already does.

## What shows, and when

| Element | Visible | Why |
|---|---|---|
| Search box | Always | The largest element, focused on open. Its placeholder lists what can be searched. |
| Format (for example "Pandoc ▾") | Always | What is pasted depends on it. One word, no icon. |
| More (Ctrl/⌘ K) | Always | The one door to every other action; each shows its shortcut, so the menu is also the cheat sheet. |
| Pin | Pointer over the window, or while on | Rarely needed. Fades in without moving anything. |
| Filter chips | Once added | `#tag`, `y:2019-2022` or `a:name` followed by a space becomes a removable chip. Backspace in an empty box removes the last one. |
| Recently cited, Recently added, With PDF | While browsing | Shown when the search is empty or one is selected; hidden while typing so rows get the room. Tab cycles them. |
| Tag and author suggestions | While typing `#` or `a:` | Take the place of the tabs. Enter or Tab picks the first. |
| Row: title and year | Always | Year at the right edge, in the same place on every row. |
| Row: authors and journal | Comfortable and Spacious | Compact shows the first author ("García et al.") after the title. |
| Row: why it matched | Only when hidden | "In the abstract: …sleep…" or "Tagged: …" when the match is not in the title or authors. |
| PDF or Web label | When a file is attached | Says what is attached ("PDF · 12 pp") and opens the preview. No empty column otherwise. |
| "In 2 libraries" | Only for duplicates | Explains why one reference may differ between sources; it is listed once. |
| Add button | Hovered or selected row | Takes the place of the type icon, so the row never reflows. |
| Citation tray | Once a source is added | Page number per source and the exact text to be copied, in scarlet. |
| Footer tip | One at a time | Tied to what the user is doing, retired once they have done it. Otherwise the footer shows library status. |
| Library status | Words only on trouble | A dot normally, details on hover; a sentence only when something needs the user. |

## Learning without a tutorial

Three layers, each reached from the one before by something on screen:

- **First minute:** type, press Enter, paste. Clicking a row copies it.
- **First week:** → previews the attachment and "Cite page" fills in the page;
  Shift+Enter builds a citation with several sources; Tab shows recent items.
- **Power:** `#tag`, `y:`, `a:` filters; the format menu; Ctrl/⌘ K
  for every action; pin; per-app behaviour.

The format menu offers all seven key formats from [ADR 0016](adr/0016-settings-format.md):
Pandoc, `\autocite`, `\parencite`, `\textcite`, `\cite`, Typst and bare keys.
The five bundled citation styles are separate choices ([ADR 0020](adr/0020-picker-preferences.md)).
Number-key bindings are not implemented or assigned here; the shared key table
for #223 must name each binding before any are advertised.

No pop-up tour and no "advanced mode". Tooltips appear after 600 ms and name the
action and its key. Empty and no-result states say what to try next. Footer tips
are stored locally as "seen" flags only, never as usage data.

## Window behaviour

**Opening.** On the display with the focused window (settings: display with the
pointer, or last position), at the position last used on that display. Focus is
in the search box at once; typing must work without a click. If Ibis was closed
less than a minute ago, the last search returns, selected so typing replaces it
(adjustable up to three minutes; Raycast and PowerToys Command Palette offer
the same setting in their documentation). The kept search lives in memory only
and is never written to disk.
It appears in 120 ms with a short fade. The at-cursor form opens under the text
caret and flips above it near the bottom of the screen.

The focused-window display is a target for rollout step 4. Today the picker uses
the display with the pointer (the nearest display if the pointer is between
displays). If pointer position is unavailable, it keeps its current display or
uses the primary one; it does not yet offer a display-choice setting or save
positions across restarts.

**Moving.** Drag the handle or the empty part of the header; the list never
starts a drag. It snaps to the centre line and the upper third within 18 px,
showing a thin guide; Alt places it freely. Position is remembered per display;
double-clicking the handle re-centres. It cannot be dragged more than half off
screen. If a remembered display is gone, it opens centred.

**Clicking outside.** Ibis hides and the click reaches what was clicked. The
search and selection wait a minute. When pinned, Ibis stays. It never hides while
a page number is being typed, a dialog or file chooser is open, a drag is in
progress or a sign-in is under way.

**Hover.** Moving the mouse selects the row under it; a row sliding under a
still pointer (while typing or scrolling) does not. Hover only tints: no new
columns, no height change. The wheel scrolls without changing the selection.

**Sizes.** Opening the preview widens the window toward the side with more room,
so the list does not jump under the pointer. Resizable from any edge, remembered
per display. On small screens, the preview replaces the list instead of sitting
beside it.

**Giving focus back.** After Enter, Ibis hides and focus returns to the window it
came from. If inserting fails, the citation is on the clipboard and a notice says
why. Shift+Enter adds to the tray and keeps Ibis open. When pinned, Enter inserts
and Ibis stays open.

**Escape** steps back one layer: close a menu or dialog, close the preview,
clear the search and filters, then hide. With sources in the tray, the last step
asks once before discarding them. Escape never unpins.

**Pinned** Ibis floats above the document, keeps its place, and does not take
focus until clicked or summoned. It is the only time Ibis stays on top.

**Never** take keys people type with (Ctrl+Shift+Space is the non-breaking space
in Word and LibreOffice, so Windows and Linux use Ctrl+Shift+Y; Ctrl+Alt types letters on many European layouts; Ctrl+Space
switches input languages), open by itself, appear in the taskbar,
Dock or Alt+Tab (only the settings window does), or animate longer than 280 ms.
The shortcut is ignored while a full-screen app or presentation runs, unless it
is pressed again.

**Platform limits.**

- macOS: a non-activating floating panel, like Spotlight, so the writing app
  stays in front; it opens over full-screen apps in the current Space.
- Windows: an app may come to the front only when the user asked (shortcut or
  tray), which is when Ibis opens. Ibis remembers the window it came from.
  Typing into apps running as administrator is blocked, so Ibis copies instead
  and says why.
- Linux on Wayland: the compositor places windows and pinning on top is not
  available, so Ibis opens centred and "pinned" means "stays open". Pasting into
  other apps is generally not allowed, so Ibis copies and says "Press Ctrl+V".
  A future global shortcut goes through the system's shortcut dialog; the
  preview has no GlobalShortcuts portal integration. Its existing shortcut
  backend requires reachable X11/XWayland with XKB, and otherwise explains
  the limitation while keeping the application launcher available. See
  [Linux preview shortcut capability](compatibility.md#linux-preview-shortcut-capability).
  On X11, Ibis
  can place its window, stay on top when pinned and register its shortcut
  directly, as on Windows.

**Accessibility.** Reduced motion uses fades only. Screen readers hear the
result count and, per row, title, first author, year and why it matched;
"Copied" is announced politely. Every action has a key; focus rings are always
visible (2 px, accent). Right-to-left titles align right within their row.

## Rollout

Each step is one pull request against #223 (or an issue split from it), reviewed
with the states gallery on every platform. Steps 1–3 need no new data or
settings. Settings format 2 is recorded in [ADR 0018](adr/0018-settings-format-2.md);
[ADR 0019](adr/0019-update-checks.md) and
[ADR 0020](adr/0020-picker-preferences.md) add optional update and picker fields.
Density and Larger text are already remembered. Remaining choices kept across
restarts (window position per display, display choice, how long a search is kept,
which tips were seen) need an ADR before new `settings.json` fields are added.

1. **Done (#307).** Design values in `tokens.css`: type scale, densities, radii,
   motion and colour roles, without changing layout. Remaining motion work:
   enforce the 280 ms cap and reduced-motion behavior for every transition.
2. Picker layout: header (search, format, pin), rows, footer with library status
   and More, empty and no-result text.
3. Window behaviour that needs no settings: outside-click hiding, keeping the
   search in memory for a minute, the Escape ladder, and staying on top only
   when pinned. Hide the quick picker from the taskbar, Dock and Alt+Tab; verify
   each platform separately (macOS already uses an accessory panel). Add the
   full-screen/presentation shortcut guard and its second-press override.
   Separately verify the macOS panel opens over full-screen apps in the current
   Space; the panel already has this configuration, which is not evidence that
   every native scenario passes.
4. Density and Larger text are settings under ADR 0020 (#407, #408). Remaining
   work: an ADR for display choice and remembered per-display position, then
   change the opening default from the pointer's display to the focused window's
   display, with the pointer and last-position alternatives.
5. Footer tips, and `#tag`, `y:` and `a:` filters (tags need source support).
6. Recently cited and Recently added (needs a local history that stays on the
   device and, as PRIVACY.md promises, can be turned off and cleared).
7. Citation tray with page numbers (0.3, with #24).
8. Attachments: the PDF or Web label and the preview pane (needs attachment
   data from Zotero).
9. Later: one-line and at-cursor forms, the settings window, and alternative
   looks users can choose; native Wayland GlobalShortcuts portal integration
   and its system shortcut dialog need their own implementation and evidence.
