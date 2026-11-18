# 0028 Engine first

Capabilities live in `ibis-core`, `ibis-sources`, `ibis-csl` and `ibis-engine`.
The CLI, JSON-RPC, MCP and desktop are clients of typed engine methods; desktop
links in process. The engine stands on its own on every supported system.
Library and citation ownership follow [0052](0052-engine-owns-library-and-citations.md).

One citation request gives one result with typed losses and refusals. Invalid
styles and locales are request errors. Records use opaque library and native
key identities; the same mapper handles running and closed Zotero. In memory,
records keep normalized data Ibis uses. Original-entry exports read the source.

Each writing task captures its writing, libraries and output. Another client's
selection cannot retarget it. A key proven to name a different work refuses
key output. An unreadable bibliography produces a warning. A failed source
refresh retains its coherent snapshot, warning and last successful read time;
uncertain identity or changed selected records still refuse.

The wire contract has its own version (`ibis: 1`), separate from the package
and formatter/style/locale versions. `--contract 1` selects it explicitly;
unsupported versions refuse with the supported list. Requests reject unread
fields. Clients accept additional response fields and treat unknown findings,
losses, refusals and outcomes as needing review. Unknown error codes still fail.
Canonical requests in `fixtures/cases` run through each front end.

Until the stable 1.0 any release may change any part of the contract, and
progress wins over compatibility.
[`contract-compat.mjs`](../../scripts/contract-compat.mjs) compares the
generated contract (`fixtures/engine-contract.schema.json`,
`fixtures/cli-schema.json`, `fixtures/mcp-tools.json` and
`fixtures/cli-surface.json`) with a published release. It reports structural
additions, removals and changes, including annotations and constraints. Earlier
releases provide only the generated CLI help and exit tables for the CLI
comparison. GitHub runs the advisory report in a separate workflow against its
latest published, non-prerelease release. Each numbered release's notes compare
with the previous published release under "Contract changes". The rules for
changing a stable contract are decided at 1.0.

Receipts record ordered opaque identities, citation fingerprints, resolved
output, captured source read times, findings and a canonical content hash. CSL
receipts also identify the processor, bundle digest and actual language. They
archive neither source data nor formatting material.

Strict mode is request-local: any refusal, loss, source warning or failed refresh
returns an error without a citation. Key output also needs a complete current
bibliography check with every key found unambiguously. Caller-supplied records
use the same policy against a request-local snapshot, without opening libraries.
Health and receipt evidence always come from the held citation snapshot.
