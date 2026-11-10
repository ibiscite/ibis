# Design

Ibis follows Apple's macOS Human Interface Guidelines: when in doubt, do what a
stock macOS app does (Spotlight, Finder, System Settings). Every value lives
once in [`apps/desktop/src/tokens.css`](../apps/desktop/src/tokens.css), in three
layers (primitives → semantic roles → component values), and every part lives
once as a shared component in [`apps/desktop/src/ui/`](../apps/desktop/src/ui/);
screens never restyle a part. The design canvas is generated from the app
(`apps/desktop/gallery/canvas.mjs`), so this page names a token or component
instead of repeating its pixels. [Decisions](decisions.md) win over this page.
Tags such as [C12] are cited by code and tests; keep each beside its rule.

This page holds the principles, the model and how the design changes. Each part
has its own page; read only the one your change touches:

| Page | What it holds | Tags |
| --- | --- | --- |
| [Foundations](design/foundations.md) | Tokens, colour, materials, type, spacing, corners, icons, the bird, key caps, motion, accessibility, per platform | C1 C3 C4 C10 C13 C15 C17, D1 D3–D6 D8–D11 D15 |
| [Components](../apps/desktop/src/ui/README.md) | Each shared part in `ui/`, its states, and menus | C9 C21, D7 |
| [Picker](design/picker.md) | Search bar, results, search rules, shelf, footer, keys, window behaviour | C2 C5–C8 C11 |
| [Inspector and Reader](design/inspector-and-reader.md) | Citation details, the side panel and the planned PDF reader | C19 C20 C23 |
| [Copying, pasting and notices](design/delivery.md) | What Enter does, the format rule, writing projects, notices and their words | |
| [First run and choice screens](design/choice-screens.md) | Welcome, no library, empty and failed libraries | C12 |
| [Settings](design/settings.md) | The Settings window and its pages | C18, D17 |
| [Source checks and sheets](design/sources-and-sheets.md) | The project bibliography check, passages (planned), Cited Since Opening | |

## Principles

1. **One anatomy everywhere.** Search, list, shelf and footer, in that order, at every size; layouts fold, never reorder.
2. **One design, macOS first.** Every system uses the macOS design; only correctness follows the system (see [Per platform](design/foundations.md#per-platform)).
3. **Readable first.** Nothing smaller than 12 px; secondary text (helper lines, the footer, a row's byline) passes 4.5:1 on every surface, in light and dark, and with Increase Contrast.
4. **Easy to hit.** Every target is at least 32 px, 44 px under `pointer: coarse`, whatever the drawn size.
5. **Show less, reveal more.** Things appear when useful and fold one key away; no modes, no "advanced" switch.
6. **Say what happened.** Every action ends in a visible result: copied, pasted, or why not. A refused library file is a failed open with a way to choose another; only a successful read can say the library is empty. Colour is never the only signal. Nothing asks "are you sure?"; destructive actions apply at once with an 8 s Undo [D16].
7. **Calm motion.** Movement explains where something came from and stops under Reduce Motion.
8. **No glow, no edge rings.** Selection is a fill, never an outline. Exceptions: picture choices (see **Selection is a fill** in [components](../apps/desktop/src/ui/README.md)) and a dragged library file, which rings the whole content area.
9. **Great defaults, then choice.** Most people never open Settings; no setting exists because a choice was hard.
10. **The bar for anything people see.** A beginner succeeds with no reading, in plain words, without opening Settings; a pro does everything by keyboard, with nothing waiting or confirming.
11. **One next action.** Every screen has one clear next action, and every failure says why and what to do.
12. **People's words, not the engine's** (maintainer, 2026-11-05). Name things by what the person already has: "thesis.bib", "your Zotero library", "cited since opening". Engine words (session, project bibliography, context, scope, source health) never reach the interface, and the command line's human output uses the same words. Where a citation goes is read from the footer alone: the action names the app and format, the status names the library and any linked `.bib`; no other strip of selectors.
13. **One of each, defined once** (maintainer, 2026-11-07, design pass 2). Three corners, one space scale, one raised layer for everything that floats, one scroll bar, one picture for every visual choice; a sheet is drawn with Settings' cards and rows. A part reads these and never names a second value for them; a screen that looks different is a missing part or token, not a local fix.
14. **A control lives where its choice lasts** (maintainer, 2026-11-10). A choice for this citation lives on the citation and is forgotten after the paste; for this app or document, in the picker's header, remembered for that app and showing where it came from; for this person, in Settings; for this computer, in first run and the system's prompts; for scripts, in the command line. How often and how costly a mistake is decides how visible it is: every time is one key and visible, rare is in the command list (More), rare and consequential is in Settings with Undo. What a context cannot do is hidden, and Ibis says why where people look; the engine decides. A remembered value wins over a default and says so; a choice Ibis can infer has no control. Every action has three doors: a visible control with its key, the key, and the command list. Each control names who needs it; a control nobody needs is removed.

## Where Ibis departs from macOS

Anything not listed follows Apple. A new departure is added here in the same change, or it is a bug.

| Where | Apple | Ibis |
| --- | --- | --- |
| Hit areas | Regular controls are 24 pt | Drawn at Apple's size, hit area at least 32 px (menu rows keep 24) |
| Smallest text | 10 and 11 pt captions | Nothing under 12 px |
| Secondary text | 50% black in light | 60% black, so it keeps 4.5:1 on the window's glass without vibrancy behind it |
| Accent | System accent | Follow the Mac by default, or **Ibis scarlet** |
| Citation colour | None | The scarlet beak marks the citation; its text stays in the text colours [C14] |
| Large buttons | Capsules on macOS 26 | Rounded rectangles at every size |
| Search icon | Magnifier | The Ibis mark, as Alfred shows its hat |
| In-window menus | `NSMenu` | Ibis's own, matching native metrics; menu bar, tray and right-click stay native |
| Window size | Content decides | One steady size while open [C10] |
| Key caps | Shortcuts only in menus | Caps in the footer and key lists; Settings writes keys as menus do |
| Focus ring | A translucent keyboard focus colour | Solid accent text, so it keeps 3:1 on every surface |

## The model

[M1] **One window that unfolds** (decision 0038). The picker is a launcher first:
type, see references, press Enter. Everything else unfolds toward the trailing
side of the same window, one region at a time, and folds back with Esc:

| Step | What the person does | What appears |
| --- | --- | --- |
| Search | Types | The list, the shelf with what Enter pastes, the footer's one action |
| Several references | ⇧↵, ⌘-click or a row's + | One shelf segment each, in order; the count names them |
| Details | Clicks a segment, or ⌘I | The **Inspector** beside the list: title, Show as, Page, what will be pasted, the source actions |
| Pages | **Show PDF** (⌘D) in the Inspector | The window takes a reading shape once, taller and wider; the list folds away and the **Reader**'s thumbnails and pages take its place beside the column |
| Choose a page | Clicks a thumbnail (⇧-click a range, ⌘-click another), or ⌘↵ on the page in view | The Page field and the shelf segment show it, "p. 417–419, 423" |
| Deliver | Enter, as the footer says | Pasted or copied, then the window hides |

- **Beginners** see every step as a visible, named control with its key beside
  it: the footer action, the segment, Show PDF, Cite This Page. **Experts** never
  leave the keyboard: ⇧↵ adds, ← → move between segments, ⌘I opens details, ⌘D
  the pages, ⌥↓ ⌥↑ move a page, ⌘↵ cites it, Enter delivers, Esc steps back.
- **Nothing moves under the person.** Unfolding never reorders search, list,
  shelf or footer. Reading is the one change of shape, because a portrait page
  needs height: the list folds away while the PDF is open, and typing in Search
  or Esc brings it back with the steady size.
- **Planned jobs have their place already.** Passages are results with their
  quotation under the title, and choosing one fills its reference and page;
  collections and tags are a search scope pop-up before the query (as Finder's
  search scope); a manuscript check is a sheet from More listing each citation
  with its state. None of these adds a window or a mode.

## Small delights

The approved, final list; no new delights until the next design pass. Each lives only in About, the welcome, the notice slot or existing feedback; never changes results, output or focus; happens once within the motion cap; never loops or plays a sound. Warm, never childish: at most one short sentence, no slang, exclamation marks, emoji or jokes about the person's work. A delight is a person's action answered, or a real moment marked; nothing appears at random. The fun items sit behind Settings › General › **Small surprises** (on by default).

- **Feel:** a pressed shortcut briefly presses its footer cap; holding ⌘ shows every visible action's key; Enter with nothing to paste shakes the action once and says why; ⌘+/⌘− show the size briefly; the menu bar ibis dips its beak after a copy.
- **Saves time:** Services › Search with Ibis; a `.bib` dropped on the menu bar ibis becomes a library; every Undo names its action ("Undo Remove Vale 2021"); ⌥ on About shows the full build; once, after the first paste, a tip that ⇧⌘P keeps Ibis open between citations.
- **Fun:** About's line on the name ("Named for the bird ancient Egyptians linked with Thoth, god of writing and keeper of records."); clicking the About icon dips the bird's beak, the menu bar's gesture, and the line says the next bird fact, one click, one fact, carrying on next time, until you leave the page; the first open greets with the mark; the first, 100th and 1,000th paste say so on the receipt; a library's size is marked once, with the highest milestone reached only ("A good start.", "A solid library.", "A remarkable library."), so a large import is one moment; the day a year after first use, the line reads "One year with Ibis today."
- **Left out:** confetti, sounds, seasonal themes, joke results, a "what's new" screen, alternative app icons.

## Brand and website

[C16] Logos and icons come only from `brand/` ([brand/README.md](../brand/README.md)). The website's rules live in [website/README.md](../website/README.md#design); its demo uses the app's tokens and key-cap labels at every width.

## Coming features

How each user-visible item planned before 1.0 in ROADMAP.md fits the pages above.

| Feature (release) | Fits as |
|---|---|
| Output per app, self-finishing setup (0.3) | [First run](design/choice-screens.md) |
| Paste into the app you came from (0.3) | Enter pastes; the footer says "Paste into Word"; a refusal copies and says why |
| Citation forms, 29 locator types, prefix and suffix, reorder, any style, presets (0.4) | The shelf, more label and control rows in the **Inspector**, the format pop-up and Settings › Citations |
| `ibis://` links, Show in Zotero, Open PDF in Zotero (0.4) | A link opens the picker filled in; the Zotero actions sit in More and the row's menu |
| Settings window (0.4, #776) | [Settings](design/settings.md) |
| Browse, "Selected in Zotero", passages (0.5) | Collections and tags as search filters; a reference's highlights unfold under its row and fill in its page |
| Bibliography, "Cited since opening", project `.bib` (0.4) | A sheet from More; rows say "In refs.bib" or "Missing from refs.bib" |
| `ibis check`, DOI lookup (0.6) | Command line first; DOI lookup as an opt-in no-results fallback |
| A source's PDF, choosing the page there (#1252) | [Reader](design/inspector-and-reader.md#reader-planned-1252): the window widens once, the one planned exception to [C10]; thumbnails and pages sit between the list and the column, and a page is cited by choosing it ([M1]) |

## How the design changes

- Change `tokens.css` or the shared component in `src/ui/`, never one screen; fix a visual bug in the rule or component that allowed it, with a regression test.
- **Screens lay out parts; parts own the look** (maintainer, 2026-11-05): a screen arranges parts from `src/ui/` (display, grid, flex, position, size, overflow) and never declares its own spacing, colour, border or corner. A screen that needs a different look gets a variant of the part. `ui/ownership.test.ts` fails a screen stylesheet that pads, paints or rounds itself; the screens still carrying their own look are counted there and only go down.
- The five parameters at the top of `tokens.css` (spacing unit, control radius, body text size, Ibis red light and dark) are the knobs. The canvas's Parameters board turns them live; `node apps/desktop/canvas/apply-params.mjs --radius=10px …` writes chosen values back (then `python3 brand/build.py` for the reds).
- The canvas boards run the real app live (`pnpm -C apps/desktop build:canvas`, then upload `ibis-canvas.js` and `.css` and point the two constants in `Screen.dc.html` at them; boards link neither, so a refresh changes one file); a missing state gets a scenario in `apps/desktop/canvas/main.tsx`, never a hand-drawn copy. A hand-drawn sketch is only for a direction not yet built; once it is built, the live board replaces the sketch, so each screen has one picture.
- **One stylesheet check** (decision 0035, #1678): `apps/desktop/src/design.test.ts` parses every stylesheet into the CSS object model and holds each rule of these pages as one table entry (corners from the three steps, spacing from the scale, colours and type from tokens, the overlay layer and the scroll bar each set in one place, no one-sided state edges, a Reduce Motion answer for every movement), each with a planted stylesheet it must catch; contrast is computed from the token values. A new rule is one entry there, never a test that reads a stylesheet's text.
- **Rules that can be measured are measured** [C22]: `apps/desktop/canvas/geometry.ts` holds the layout rules as data (the two columns [C3], pop-ups under their button [C21], form rows sharing edges, equal choice cards [C12]) and checks every canvas screen in the Mac's WebKit (`gallery/webkit.swift`, run by `scripts/check.mjs`) and in Chromium (`gallery/canvas.mjs`). A new part joins a rule by adding its selector to the table; a new rule is a few lines there, never a test per screen. A visual bug a rule could have caught gets a rule.
- **Judged from pictures** (maintainer, 2026-11-04): every visible change renders its canvas scenarios (light and dark, plus larger text, right-to-left, Windows key labels and the forced-colours and Increase Contrast captures where it matters) and is judged by a reviewer who did not make it. A small change needs one judge; a batch gets the panel of independent views (macOS, visual design, first-time user, keyboard only, accessibility, Windows and Linux). A pass merges when no view scores any screen lower than before; work continues until every view scores every screen 9 or better. A defect is fixed in the rule that allowed it, so it cannot come back.
- Record maintainer decisions in [decisions](decisions.md) and update the rule on its page in place, deleting the text it replaces. A rule lives on one page; anywhere else links to it.
- Review these pages against Apple's guidelines on each new macOS release.
