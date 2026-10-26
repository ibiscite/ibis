# Ibis website

## Build

From the repository root, with Node 22.13 or newer:

```sh
node website/build.mjs
python3 -m http.server 8097 --bind 127.0.0.1 --directory website/dist
```

The build copies the shared app tokens and allowlisted brand
exports into `website/dist`. Check with `node --test scripts/website*.test.mjs`.

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
