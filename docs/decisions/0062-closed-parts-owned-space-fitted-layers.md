# 0062 Closed parts, owned space, fitted layers, and a gate that sees

**Decided** 2026-11-26, from the maintainer's request to fix the front end at
its core rather than screen by screen, after three design audits and the
maintainer's live review of nightly 0.3.14-nightly.20261126075620
([#2014](https://github.com/ibiscite/ibis/issues/2014)).

## Why

[Design](../design.md) already states the right principles: one of each,
fixed slots, nothing jumps, values only from tokens. The defects people meet
break those principles anyway, and they share three causes.

| Symptom people meet | Cause |
| --- | --- |
| General's first divider runs edge to edge; a Settings warning halves the gap above it; the card inside Details sits inside another card | **Space has two owners.** A divider exists only when one row follows another (`.ui-row + .ui-row`). A section gap is the page's gap plus a sibling margin. Pages hand-build cards. |
| The suggested choice's icon is invisible in light mode; an open citation token is two-tone and its focus ring covers half of it | **Parts are open.** Density rules on `:root` and `#results` inside the shared row reach every part that borrows the row's class names. Fifteen sheets restyle `.ui-button`, so the winner depends on specificity. |
| The locator menu in the citation drawer opens with no visible rows; a Settings search jumps to a row that is off screen | **Floating and scrolling have no region.** A menu places itself against a boundary it is never given. A reveal fires once, before the content above it settles. |
| All of these passed the picture gate | **The gate checks proxies.** It measures boxes, not ink, and DOM presence, not visible rows. It grows Settings pages until nothing scrolls. Local pictures come from Chromium while CI uses WebKit. |

The census at decision time: 65 px/rem and 23 em literals outside tokens;
239 direct `--space-N` reads in 42 sheets; 113 `calc()` (at the ceiling);
13 unnamed breakpoints in three units; six ways to draw a shortcut; six row
implementations; two switches; two spinners; 13 `#results` selectors inside
a shared part.

## The decision

### 1. Parts are closed

- **A sheet styles only its own part.** Each stylesheet in `ui/` selects only
  its own class prefix (`.ui-NAME`, `.ui-NAME-*`) and its own state
  attributes. ID selectors and `:root[data-*]` descendants are not used
  inside a part.
- **Context reaches a part only through its knobs.** Density, platform,
  contrast and the window size set custom properties on an ancestor. A part
  reads them; it is never selected from outside.
- **A part changes another only through that part's documented knobs or
  props**, as `--band-control-*` already does. Every state fill of a control
  is set once, in its own sheet.
- **A component renders only its own class names.** Borrowing another part's
  classes to inherit its look is composition by accident. A part that needs
  that look composes the part.
- **One paint owner per surface.** A control with inner buttons (the citation
  token and its ×) paints one fill and one ring, on the whole surface.
- **Both sides are checked.** The ownership test covers stylesheets and the
  class names components render, in `ui/` as well as screens. A knob is a
  custom property the part documents, set on the instance it is meant for.

### 2. Every space has one owner

- **A part has no outer margin.** The space between siblings belongs to their
  container: `Stack` for gaps, `Group` for the inset and the dividers between
  its items, `Section` for its heading and group. Dividers are drawn by the
  container between any two items, so no child can break them.
- **A Group holds rows.** Its items are rows; a note, warning or status
  travels as a prop of the row it is about; a disclosure's body is unpainted
  rows in the same card, never a second card. Gaps are set by an item's role
  (row, heading, card), never by which sibling precedes it.
- **Parts read roles, never steps.** `--space-N` appears only in
  `tokens.css`, where roles such as `--inset`, `--row-pad` and
  `--section-gap` are defined from it.
- **Density is a recipe for each task, not a global multiplier.** Ibis has
  five row recipes:
  - launcher row;
  - menu row;
  - form row (Inspector and drawer);
  - settings row;
  - sheet row.

  Each recipe is one set of tokens, defined together: height, inset, lead
  size, gap, title and detail type, trailing gap and wrap rule. Compact and
  Comfortable switch a recipe's tokens at the frame. They never add
  selectors.
- **One primitive for each concept.** One `Row` with fixed slots (lead,
  label, detail, trailing) under every recipe. Settings declares rows as data
  that `Row` renders, and pages do not hand-place JSX rows. There is one
  `Shortcut` for every displayed key, one `Switch` and one `Progress`.

### 3. What floats fits, and what scrolls is revealed

- **The frame publishes regions.** These are the window's content box, the
  drawer's visible box and a view's scroll box. Every `Floating` takes its
  boundary from the nearest region, never from the viewport or a clipping
  ancestor.
- **Placement is not focus.** Where focus returns when a menu closes never
  limits where the menu may open; the boundary is the region alone.
- **A menu has a minimum.** That minimum is its measured filter and chrome plus
  four whole rows, or all its rows when it has fewer (a filter with two
  matches needs two). It is checked against the region and the actual
  clipping of the layer it portals into.
  - It opens on the side with room for the minimum.
  - Its height is what that side allows, and it scrolls inside.
  - When neither side has room, it opens as a view in the window, the way
    the palettes do.
- **One `reveal(target)` per scroll owner.** It keeps the target inside the
  visible box until layout settles, or until the person scrolls, types or
  moves. Settings search, keyboard focus and search jumps all use it.
  A reveal knows when its view is ready, and a newer jump or the person
  scrolling cancels it.

### 4. The gate checks what people see

- **Real windows.**
  - The canvas renders at the picker's Small, Medium and Large sizes, at
    default and Larger text.
  - Settings renders at its real window height, scrolling. A page is not
    grown to fit.
  - Local pictures use WebKit, as CI does.
- **Ink, not boxes.** Every visible glyph passes contrast against its own
  tile. Words start on their column in LTR and RTL. A control has one fill.
- **Visible, not present.**
  - A floating surface shows at least its minimum of whole, unclipped rows
    inside its region.
  - A focused or revealed element intersects its scroll box once layout
    settles, and is not hidden under a sticky title.
  - Every check is proven by a planted failure: rows covered by an opaque
    layer, focus under a sticky header, a required icon drawn invisible.
  - Minimum-size cases in German and Arabic at 200% text, forced colours, and
    keys pressed in the first frame with normal and reduced motion.
- **Three reference journeys, as canvas scenarios with invented stress
  content.** These are stand-ins for user research, not a substitute for
  watching people use Ibis.
  1. Cite one reference with a long title and many authors.
  2. Cite several with a chapter locator in a Small window with Larger text,
     opening each drawer menu.
  3. Change library, cancel, and search Settings while slow content loads
     above the target.
- **Ratchets.** The census above becomes counted ceilings in `design.test.ts`
  and `ui/ownership.test.ts`. They may only go down, and this series takes
  each to zero or to a written exception.
- **Docs hold rules, not measurements.** A design page names a token or part.
  Where two pages disagree, the code's current behaviour is written down and
  the other page is corrected in the same change.

## How it lands

Each step lands as its own pull request, and every one of them is green when
it lands: a new check arrives with the change that makes it pass, and is shown
red on main in the pull request. The website's picker demo and its copy of the
app tokens change in the same pull request as the picker. Picture baselines come
from CI, as [testing](../testing.md) says.

1. **Closed parts and owned space.** Containers own dividers and gaps;
   ChoiceList and the citation token stop borrowing; Button owns its states;
   ratchets added. The only picture changes are the bugs it fixes.
2. **Gate.** Real window sizes, ink and visible-row checks, the three
   journeys. Red where the symptoms above still exist.
3. **Regions.** Fitted floating and one `reveal`; the drawer menus and the
   Settings search jump turn green.
4. **Recipes.** The five row recipes on one `Row`, and one `Shortcut`.
   Settings rows are declared as data. The picker's footer and shelf move
   onto `Band`.
5. **Website.** The same rules hold in `website/`. Its type steps and list
   primitive live in its own token block. Its content states only what a
   published release contains.

## The maintainer's answers, 2026-11-26

- **Highlighting a result never changes its size.** Nothing jumps, and the
  list stays breathable and easy to read. Every row shows the same thing in
  the same place: the title, then what people need to choose between
  similar works (authors, year, journal or publisher). Highlighting changes
  only the paint. The density decides how much room every row gets, never
  just the highlighted one.
- **Larger Text scales the words and the rows that hold them, everywhere,
  Settings included.** Icons, window insets and control chrome stay at their
  native size, as with macOS text sizes. The description in Appearance says
  exactly that.
