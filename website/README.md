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

Browser checks in `scripts/website-controls.test.mjs` use an existing Playwright
runtime when `IBIS_PLAYWRIGHT_MODULE` is set. `IBIS_CHROMIUM_EXECUTABLE` can select
an existing browser. They serve build files through request interception; no local
server or network access is needed.

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

`build.mjs` generates `sitemap.xml` from the four routes in `content.mjs`;
`robots.txt` advertises it. Section anchors are not separate sitemap URLs. Do not invent
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

1. One shell across four pages: a sidebar index of labels and `@key` links,
   separated from the reading column by a divider line.
2. Serif for reading, sans for UI, mono for keys, commands and code. The
   picker keeps the app's type and tokens.
3. Reserve scarlet for keys, citations and the current item.
4. Separate rows with hairlines; keep the same rhythm and 45rem reading measure
   across pages. Hover follows the sidebar: text darkens, key chips change fill
   and colour, and page arrows nudge. Rings belong only to keyboard focus.
   Download arrows drop into a drawn tray once per hover, inside a fixed icon box.
   Separate the sidebar's groups with a short end-aligned hairline, vertical in the phone bar.
5. Keep content visible: no disclosure toggles, accordions or `<details>`.
6. Hover and press motion uses the app's timings. All motion is off under
   reduced motion. All citation pills hug their text at rest. With a fine hover
   pointer, one shared cycle measures each form and eases the pill's width with
   the site's short timing; neighbours slide with it. Make room before showing
   longer text, centre it, and restore natural sizing on return or pointer leave.
7. Show real Ibis output only, with Copy on every command. Label nightly
   features as nightly.
8. A PR that changes the app's picker updates the demo in the same PR, using
   the shared app tokens scoped by the build and `demo.css`.
9. Keep controls visually slim on touch screens; invisible hit areas provide
   their full tap height. Native selects receive pointer activation directly;
   hit-area overlays belong to buttons, never their labels. At phone width all
   six navigation keys fit, and the picker search and footer each stay on one row. Citation carets stay attached
   to the end of inline chips; outbound marks stay with the link's last word.
10. Focus rings indicate keyboard input, including after script-driven focus
    transfers. Pointer actions keep focus without a ring; the next keyboard action
    restores it. Search examples share the demo's field parts and tokens, and
    empty citations use muted sample text. Follow sentences with paragraph spacing.

The interactive demo keeps the app's bundled sample references and changes only
its own page; it never reads a visitor's library or clipboard. Search, combine,
copy, paste and close/reopen stay keyboard accessible. Without JavaScript the
picker illustration and the rest of each page remain visible.

The shared app shortcut opens the home demo from every page, leaving text entry
alone. The reading order and next-page rows come from `sections` in `content.mjs`.
Escape clears the demo search, then closes the picker; reopening keeps its choices
and citation shelf. `?` outside text entry, or Keyboard shortcuts in the footer,
opens keyboard help; Escape closes it and returns focus.
Scripts' shorthand cycles its examples after three idle seconds, pauses while
hidden or under reduced motion, and stops permanently on focus, input or Try.

### Names
A page's label, its `@key` and its address agree: the key is the label's main word, lowercased (`Features`, `@features`, `/features/`). Labels use the words a newcomer would search for.

### Ready for other languages
- **Words live in the page templates, never in scripts.** A script that shows text reads it from a `data-` attribute next to the element. A whole sentence is one attribute, never stitched from pieces.
- **Layout uses start and end, never left and right** (`margin-inline-start`, `inset-inline-end`, `text-align: end`), so a right-to-left language mirrors the page.
- **No text in images.** Icons come from the app's icon set and always sit beside a word.
- **Translations themselves wait for the first real one:** then pages get a `locales/<lang>/` copy of `pages/` and `content/`, with the shell unchanged.

## Content

`index.html` is the shared shell; `pages/` holds the four page bodies and
`404.html` is the missing-page body in that same shell. The route list in
`content.mjs` supplies navigation and the sitemap. Download detection changes
only the home button's label; all three systems stay visible on Get.

`content/status.json` is the only status list, including nightly work. Check
release notes before moving items: RIS/CSV and the official CSL styles have
shipped; prefix, suffix and Keep My Order shipped in 0.3.2. An empty nightly
feature inventory is stated plainly rather than repeating shipped work.

`content/terminal.txt` contains real plain-output CLI transcripts using
`fixtures/website/sample.bib`. Successful output is stdout; refusals are stderr.
`content/writing-apps.json` holds the four real citation outputs in the writing
example. The CLI regression test runs the transcript commands and verifies these
outputs against that fixture, so an output change requires updating the site:

```sh
CARGO_TARGET_DIR=target cargo test -p ibis --locked --test website_transcripts
```

The typed-search example is a fixed input and its real APA result, not another
parser. `DEFAULT_PICKER_SHORTCUT` in `apps/desktop/src/keys.ts` supplies setup
shortcuts and the demo. No inline scripts or styles are published.
