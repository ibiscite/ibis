# Ibis website

## Build

From the repository root, with Node 22.13 or newer:

```sh
node website/build.mjs
python3 -m http.server 8097 --bind 127.0.0.1 --directory website/dist
```

The build copies the shared app tokens, shortcut module and allowlisted brand
exports into `website/dist`. Build and publish only after the page and demo
slices have merged. Check with `node --test scripts/website.test.mjs` and
`node scripts/style-lint.mjs`.

The optional browser regressions in `scripts/website-controls.test.mjs` run when
`IBIS_WEBSITE_URL` points to the loopback preview above; it uses the existing
Playwright runtime (`IBIS_CHROMIUM_EXECUTABLE` can select its browser).

## Publish

Use Cloudflare Workers static assets with its Git integration, on the free plan.
Connect `ibiscite/ibis`, production branch `main`, root directory the repository
root, build command `node website/build.mjs`, deploy command
`npx wrangler deploy`. `wrangler.jsonc` sets the output to `website/dist`;
there is no Worker script. Use Node 22.13 or newer in the build environment.
Cloudflare manages deployment credentials; no GitHub workflow or secrets are
needed. Installers remain on GitHub Releases.

When the reviewed site is ready, the maintainer connects the repository, adds
ibiscite.org to Cloudflare DNS, checks the imported DNS records and replaces
only the nameservers at Hostinger with Cloudflare's assigned pair. Hostinger
remains the registrar. Once the zone is active, add `ibiscite.org` under the
Worker's Settings → Domains & Routes → Custom Domain and enable Always Use
HTTPS in the zone. No path redirects are needed for this single-page site.

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
