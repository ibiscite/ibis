# Ibis website

## Build

From the repository root, with Node 22.13 or newer:

```sh
node website/build.mjs
python3 -m http.server 8097 --bind 127.0.0.1 --directory website/dist
```

The build copies the shared app tokens and allowlisted brand
exports into `website/dist`. It also generates `picker-tokens.css` from that
same file, replacing only `:root` with the picture's `[data-app-tokens]` scope.
Every declaration and conditional stays in order, including dark, contrast and
notice rules; the page's prose colours cannot override the app pictures.
There is no maintained second palette. Check with
`node --test scripts/website*.test.mjs`.

The optional sidebar browser check in `scripts/website-controls.test.mjs` uses an
existing Playwright runtime when `IBIS_WEBSITE_URL` points to the loopback preview.
`IBIS_CHROMIUM_EXECUTABLE` can select an existing browser.

## Publish

Use Cloudflare Workers static assets with its Git integration, on the free plan.
Connect `ibiscite/ibis`, production branch `main`, root directory the repository
root, build command `node website/build.mjs`, deploy command
`npx wrangler deploy`. `wrangler.jsonc` sets the output to `website/dist`;
there is no Worker script. Use Node 22.13 or newer in the build environment.
Cloudflare manages deployment credentials; no GitHub workflow or secrets are
needed. Installers remain on GitHub Releases.

The site is served at ibiscite.org through the Worker's custom domain, with
Always Use HTTPS on. Domain and DNS settings are the maintainer's.

`_headers` applies a self-only page CSP and revalidation to unversioned files;
brand assets cache for one hour. SVGs have a separate sandboxed policy allowing
their embedded animation styles, and workers.dev preview links send
`X-Robots-Tag: noindex`. Missing paths get `404.html` with a 404 status. Pull
requests get a preview link from Cloudflare Worker Previews (deploy command
`npx wrangler preview`, `previews` block in `wrangler.jsonc`); the workers.dev
production address stays off. No external fonts, scripts or analytics load.
Check live HTTPS, CSP, caching, missing-path 404s, downloads and both themes.
For rollback, choose the previous reviewed deployment in Cloudflare; restore
nameservers at the registrar only if abandoning Cloudflare DNS.

References: [Git builds](https://developers.cloudflare.com/workers/ci-cd/builds/configuration/),
[static headers](https://developers.cloudflare.com/workers/static-assets/headers/).

## Search discovery and privacy

`sitemap.xml` lists the canonical homepage only; `robots.txt` advertises it.
Section anchors are part of that page, not separate sitemap URLs. Do not invent
last-modified dates or include deployment-preview addresses. HTML microdata
identifies the WebSite and SoftwareApplication without executable scripts,
ratings, or a weaker CSP. It does not promise search-result enhancements.

Use Google Search Console and Bing Webmaster Tools with DNS ownership
verification and submit `https://ibiscite.org/sitemap.xml` once deployed.
Keep verification records in DNS, not account credentials in this repository.
These tools report search-engine discovery and search performance; they add no
visitor script to Ibis. A Search Console import also grants Bing access to Google
search data; independent DNS verification avoids that account link. Do not add
Google Analytics, Tag Manager or Microsoft Clarity. Hosting request statistics
remain in Cloudflare; Web Analytics/beacon injection stays off. No visitor
cookies, cross-site identifiers or new app data flows are introduced.

After publication, verify the sitemap and robots URLs return 200, the homepage
has its canonical URL and microdata, and preview deployments remain excluded
from indexing. Search engines decide when and whether to index the site.

## Design

One calm, content-height page with a structural margin and `@key` links. It
imports the app's `tokens.css` through the public build, so there is no second
token source or motion scale; brand colours, the website roles included, come
from `brand/build.py`, and website CSS only composes the page. Build and
Cloudflare publication instructions live only in `website/README.md`; account
access, DNS records, credentials, backups and `.local/` never enter the public
build.

**Rhythm.** One vertical scale: 1.1 rem between paragraphs, 1.25 rem between
blocks (lists and link rows), about 5 rem between sections on desktop
and 4 rem on phones. The hero-to-first-section gap equals the section gap;
headings and setup steps follow the same scale, and platform groups get two
large steps when all three are shown. Content sets heights: ordinary scrolling,
no full-screen sections or scroll snapping.

**Chip.** One `@key` component for navigation, section margins and inline
links, with one font size and padding (links add touch space). At rest, scarlet
text on translucent scarlet; hover, a thin inset outline; current, filled
scarlet with contrasting text. Links press in slightly, and arriving at a
section briefly turns its heading scarlet. Keyboard focus is a separate ring
drawn outside, so it never crosses text (inside only in the sidebar and link
rows, where an edge would clip it). Dragging in the sidebar never
selects page text. No generic badges.

**Motion.** App tokens only: 120 ms feedback, 200 ms hand-overs, ease-out, no
bounce, for chip states, button press, the download-arrow nudge and the theme
thumb. Sections may reveal once from 12 px below; the logo draws once, can
redraw from Home and never loops. Quiet delights: a hovered page chip cites
itself in turn as `[@key]`, `\cite{key}` and `@key`; the Star button's star
fills and grows once on hover; the Why beak pecks once as it appears; the
writing cursor blinks; on touch a pressed row gives slightly; text selection is
a soft scarlet tint; the console greets the curious with one line pointing to
the source. Reduced motion turns off movement, transitions, arrival flashes and
smooth scrolling, and holds the chip's first form.

**Layout.**

- **Desktop, 62 rem and up** (tablet landscape too): a sticky sidebar near the
  top, each muted label right-aligned before its chip; the current label is in
  text colour with a filled chip, and a followed link stays current until the
  reader scrolls, even at the page end. The theme switch sits at the sidebar's
  foot, quieter until hovered or focused. The structural margin is a neutral
  hairline; scarlet marks only chips, links and citations.
- **Tablet portrait, 40–62 rem:** one centred reading column, chips centred
  beneath the logo, no empty second column.
- **Phone, below 40 rem:** "Early preview" on the logo's row, then a sticky,
  sideways-scrolling chip bar with an edge fade. When the current section
  changes, only the bar scrolls to keep its chip visible, never the page. Link
  rows stack title and note with the arrow on the right.
- Below desktop the theme switch is in the footer. The footer holds the
  open-source note and essential links; its divider stays within the reading
  column and its rule meets the margin rule.

**Type and colour.** System serif for prose, sample writing and headings that
introduce content; platform sans for small print, labels, action-row titles,
controls, functional headings and download names; monospace for citation keys
and output. One type role stays consistent across the page; no remote fonts.
Neutral greys have zero chroma: light background/surface/line `#FFF` /
`#F5F5F5` / `#E5E5E5`, text/muted `#171717` / `#525252`; dark `#111` /
`#1C1C1C` / `#2E2E2E`, text/muted `#F2F2F2` / `#B5B5B5`. The primary button
keeps its hue on hover and only softens toward the background. Page buttons
share one slim 2.25 rem height with
an invisible 44 px touch target. Text links rest with a soft underline that
turns solid on hover. Forced colours use system roles. Appearance follows
System until a local choice is made before paint: a small light/dark toggle
with a quiet System reset, no dropdown or hover-only menu.

**Words.** The README is the source for public words; the headline also matches
`brand/settings.json` (a test checks the built site). Feature and roadmap
claims come from [ROADMAP.md](../ROADMAP.md) and [Coming features](../docs/design.md#coming-features): a few
major highlights without version numbers, internal names such as "Insertion"
explained for a new visitor, and nothing planned shown as shipped. Short,
direct sentences, the maintainer's "I" for personal experience, plain
invitations to share a workflow or report a problem; no popularity claims,
promised dates, accent-bar callouts or disclosure controls. "Early preview",
linking to the public release list so an unpublished draft cannot create a broken link, sits near the logo, and limitations come before downloads. Copying
and pasting keys is described first, with APA, Chicago, Harvard, IEEE and MLA
after the example; setup stays short. Each section after the first opens with
one larger lead sentence. Why opens with the beak quote mark and is signed with
the maker's `@key` chip, linking to their GitHub profile. The page closes with
one line and two buttons: Star on GitHub (filled) and Share Ibis (outlined,
system share glyph), which opens the browser share sheet or copies the link; no
third-party share buttons, star counts or badges. The logo aligns with the text
beside it by its ink, not its export clear space.

**Demo.** Keep a visible picker example in the public build, including without JavaScript. Its core behavior must match at least the latest released app. The current bounded example demonstrates key-citation search, selection, combining, copy and in-page paste; it does not duplicate the engine’s locator parser or formatted-citation policy. It mirrors the shipped app: a change to what it shows (the picker,
its keys, styles or paste) updates it in the same pull request, and it grows as
the app gains features worth showing. It uses invented references, says Ibis
does not build the document bibliography, follows **Interaction** and takes its
default shortcuts from `keys.ts`; the app's default shortcut (⇧⌘Y, or
Ctrl+Shift+Y) opens and focuses it from anywhere on the page. Its selected row
is the system accent, never scarlet; without a known desktop platform (phones)
it uses the macOS accent pair, because WebKit's AccentColorText is black. The
result list keeps one height while filtering and the paste slot keeps the
sentence's line height, so the page never jumps. An underlined blank marks the
paste position; after copying it becomes a keyboard-accessible dashed scarlet
"Paste" chip with the platform paste shortcut, then the copied key lands there,
and copying again keeps the old citation until the next paste. The picture is
the current 720 × 480 picker: the canonical themed gradient Ibis search mark
(24 px, centred on the tile column), one-line shelf, library status
and a filled Copy action. At smaller widths its controls reflow within the same
height; only results scroll. Below the narrow container breakpoint the query
keeps its own row and Clear and Format wrap below it, including at 200% text.
The footer stays on one row: the library status shortens first, then actions
show key caps while retaining their accessible names. Keys stay visible on
touch. The shelf reserves the whole Copied capsule; its citation ellipsizes,
and the paired beaks share the remaining space in very narrow pictures.
Both beaks mirror in RTL; the brand logo stays unflipped and keys stay LTR.
It follows the page's system appearance and any
explicit local appearance choice. Confirm each action once, in one place: the
shelf's Copied capsule, never a second notice for the same copy. Copy feedback
adds the closing beak and Copied capsule. The copy step says “Press Enter. Ibis
copies the key. Paste into the sentence below.” The saved copied value survives
later searches and format changes until the next copy. The picker stays visible
after copying and pasting so visitors can try another reference; Escape still
closes and reopens it.

Use the existing three references in
`apps/desktop/src-tauri/resources/samples.csljson` for the demo, first-run
samples and help screenshots. The website tests compare every displayed
reference field with that file. The footer status `Sample references · 3` marks
the library once; website rows show plain titles with no extra label. The website
demo copies only inside its own page, so its citations carry no [Sample] suffix,
including the static example and accessible labels. The desktop app keeps its
` [Sample]` citation suffix. The shared practice task is:

Search for “lantern”, choose the lantern gardens reference, then press Enter.

For #1375, the combined device session still needs genuine desktop help screenshots
of first-run **Try sample references**, that practice task in the empty search,
the lantern search result and preview, and the copy/paste receipt and destination
with ` [Sample]` visible. Use this bundled set on macOS, Windows and Linux;
capture the actual platform outcome. No help screenshots are stored here and
none were regenerated in the sandbox. Do not substitute rendered mockups.

Beside the introductory download button and each platform’s downloads, show:
“No account, no uploads, no telemetry; sources read without changes.” Link
to `PRIVACY.md`; keep the sentence wrapping naturally on small screens.
This describes Ibis’s data flows, with the runtime, clipboard, updates and
agent caveats in the policy; it does not promise that the operating system
or another app never uses the network.

**Behaviour.** Offer the visitor's platform first and keep the others
reachable; phones and tablets see all platforms. Downloads keep the fixed
GitHub `stable` URLs. Explanation, setup, downloads and navigation work without
JavaScript, and every action works by keyboard and touch without hover.
On-page links stay in the page and GitHub links open in the same tab. A missing
page is the same quiet page with the logo, one line and a way home; neither it
nor preview links are indexed.
