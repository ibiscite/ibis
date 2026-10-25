# 0027. Prototype bounded headless citation formatting in Rust

Status: proposed (2026-10-25); amends 0007

## Context

#366 needs the same citation formatter outside the desktop window for the CLI,
MCP and editors (0003). The current five-style adapter already runs without a
DOM, but its worker lifecycle and rich-output policy belong to the web interface.
The [investigation](https://github.com/ibiscite/ibis/issues/366#issuecomment-6393876017)
supports retaining Zotero's pinned citeproc-js. Hayagriva has concrete CSL-JSON
and citation-intent gaps; Citationberg is a parser, not a processor. Neither a
different implementation language nor a successful render proves parity.

## Decision

### Approval boundary

While proposed, this ADR authorizes no implementation or dependency change.
Acceptance, recorded in #1059, authorizes only an optional, test-invoked native
prototype in `ibis-core`, disabled in normal application builds. It does not
replace the production GUI worker, change its five-second watchdog, or add a
CLI command, IPC surface, setting, network endpoint, daemon or persisted format.
The lead reviews the recorded proposal before that prototype starts.

Production adoption needs a later recorded decision after the gates below pass;
this proposal does not pre-approve it. ADRs 0002/0008 and release scope remain
unchanged. A new license exception, network use or release-scope change still
requires the maintainer's reserved decision. No such exception is requested.

### Processor and runtime pin

Keep the five styles, en-US locale and exact processor bytes in
`apps/desktop/vendor/csl/manifest.json`, including Zotero commit
`db41b99fddcc52d38897c59c161b6adaca036afb` and upstream processor source SHA-256
`4e717cb27c1d66024bbaa578604203c178af04e789c6244a722df44d70631466`.
Preserve the manifest's appended `moduleSuffix` for the generated ES module;
the source hash applies before that declared transformation.
Do not change pins or patch the processor while evaluating its host runtime.

Evaluate crates.io `rquickjs = "=0.14.0"`, with `default-features = false` and
only `std`. Lock its core/sys packages to 0.14.0 too. The published archives
identify [rquickjs commit d7ef5ee](https://github.com/DelSkayn/rquickjs/commit/d7ef5eeae702fea24c03643064de454f1c1dd4b0)
and include source matching its [QuickJS-NG gitlink](https://github.com/quickjs-ng/quickjs/commit/0fdea21ff1090084e91dad812b343d92e79ba9d9).
Registry archive SHA-256 values verified in the investigation are:

| Package 0.14.0 | SHA-256 |
| --- | --- |
| [rquickjs](https://github.com/rust-lang/crates.io-index/blob/master/rq/ui/rquickjs) | `a0a22bf72f515cc4189dd7c63d016f6de92a11eee98ddf7dd64794198f6eb34e` |
| [rquickjs-core](https://github.com/rust-lang/crates.io-index/blob/master/rq/ui/rquickjs-core) | `6657979a24d543e5fc66f32cef2f47ca82ce8eb55cec5f7d59a12b7b570bbf24` |
| [rquickjs-sys](https://github.com/rust-lang/crates.io-index/blob/master/rq/ui/rquickjs-sys) | `cee271d0eeba64f0915b846cb7ae02e16faf3dfdffdca91731101d9d30fe3423` |

Reverify the archives and resolved graph before adding the dependency; require
`cargo deny check` without exceptions. The wrapper and QuickJS-NG are MIT;
retain notices under the existing AGPL application license. The C build and
bundled target bindings are part of the qualification; do not silently enable
bindgen, change the Rust minimum or select another runtime to make a target pass.

Do not enable loader, dyn-load, futures, parallel, macro, rust-alloc or companion
Node/web APIs. Use the runtime's default allocator: its documented heap cap is
ineffective with a custom allocator or rust-alloc. Evaluate only pinned adapter
and processor source; pass metadata as data, never interpolated executable text.
Expose only request-owned item/locale lookups and fixed diagnostics. No host
filesystem, module loading, process, clipboard, settings or network access.

### Request ownership and limits

Use the existing normalized CSL-JSON and citation intent, including ordered
repeated references, typed locators, prefix, suffix and author omission. Resolve
desktop records under its existing revision guard before formatting; release
the library lock before evaluation. Generate request-local processor IDs, keep
record identity distinct from citation keys, and clear all request state.

Start with one lazy runtime on its owning worker thread, one active job and at
most one waiting job. Drop cancelled queued previews and report saturation;
never grow an unbounded queue or cache one processor for every official style.
Use a fresh context/processor per experiment first; measure reuse before adding
a cache. No application API or generic interchangeable-processor layer is needed.

These are conservative **prototype ceilings to test**, not measured production
budgets or a guarantee that every valid request fits:

| Resource | Starting ceiling |
| --- | --- |
| Selected CSL-JSON plus citation intent, serialized UTF-8 | 1 MiB; 1,000 citation occurrences |
| Engine-accounted heap | 64 MiB |
| Engine stack | 1 MiB, with additional Rust/C worker-stack headroom |
| Active job | 5 seconds from dispatch, including initialization and fallback passes |
| Each returned representation, canonical or ownership preview | 1 MiB UTF-8 |
| All returned representations together, including preview | 3 MiB |
| Fixed diagnostics per result | 1,001, with bounded position indices |
| Canonical plus preview markup work | 10,000 combined nodes; depth 256 per tree |

Bound before copying across the host boundary, including errors and diagnostic
counts; reject oversized work without truncating citation text. The VM heap cap
does not cap Rust buffers, thread stacks or process RSS. Count serialization and
markup work too; checking annotations alone does not bound canonical output.
The five immutable style assets are preloaded data, separate from request bytes.
Additional styles/locales and custom XML are outside this first prototype.

Run characterization in a parent-owned disposable test process, externally
stopped after ten seconds if it does not return. Sweep heap/stack ceilings to
distinguish resource exhaustion from output mismatches; record failures instead
of increasing limits silently. This external test guard is not a shipping process
architecture or a new background service.

### Timeout, cancellation and recovery

Use a monotonic active-job deadline and the runtime interrupt callback. A caller
can remove a queued job; active cancellation can mark its result obsolete and
request interruption. Never publish an obsolete result. These are cooperative
controls: [native operations can miss interrupt polls](https://github.com/quickjs-ng/quickjs/issues/1753),
and an allocation error may be caught. Do not claim hard interruption, a process
memory bound or equivalence to terminating today's worker.

After timeout, exhaustion or an engine error, discard the runtime once control
returns; do not reuse partially mutated state. Admit no replacement runtime while
the previous worker is still stuck. The external test parent kills a stuck child
and records failed in-process recovery. Production remains blocked unless bounded
recovery is demonstrated for the allowed data and operations, or a separately
reviewed containment change supplies it. Never patch that gap with a leaked
thread, unlimited retries or a silent GUI fallback.

Return fixed failure categories and affected input positions; never log raw
engine exceptions, references, style paths or request contents. Successful
citation text is output data, not diagnostics. Preserve explicit per-item
fallback warnings; a request-wide limit failure publishes no partial result.

### Locale, collation and output parity

The exact QuickJS-NG pin has a known blocker: its `localeCompare` performs NFC
plus scalar comparison and ignores locale/options; locale case conversion is
also not browser-equivalent. The pinned processor uses locale-aware comparison
with base sensitivity, ignored punctuation and numeric ordering. Therefore
plain rquickjs is **not qualified for citation or bibliography sort parity**.
Record failing oracle cases for accents, punctuation, numeric strings, Turkish
case and other scripts before claiming success from the five English examples.
See the [pinned implementation](https://raw.githubusercontent.com/quickjs-ng/quickjs/0fdea21ff1090084e91dad812b343d92e79ba9d9/quickjs.c).

Do not substitute a homemade comparator or pretend that loading CSL locales
provides Intl collation. A maintained collation dependency or another runtime
requires a separately recorded evidence/dependency decision, with data version,
license, footprint and cross-platform deterministic output considered.

Preserve the pinned adapter's standalone updateItems/makeBibliography/citation
path, including its sorting and disambiguation, rather than changing behavior
based on older generic docs. Numeric, ibid and document-dependent output stays
provisional (0007); no document-history API is introduced here. Compare current
worker and native output for all five styles, citation and bibliography, HTML,
text and complete RTF, with Unicode, date ranges, duplicate keys, repeated items,
mixed intent, edited fields, empty/malformed input and fallback/recovery cases.

Keep preview annotations separate: erasing wrappers must reproduce canonical
output; ambiguous ownership keeps the existing aggregate presentation. Engine
HTML remains untrusted. Reuse the current tag/style/block allowlist and escaping
policy, including Unicode-safe complete RTF; do not expose raw engine HTML as
trusted output. A shared window-free implementation of that output policy,
with canonical byte/node/depth bounds, is a production gate. GUI and CLI must
ultimately call that same formatter and policy, with presentation adapters only.

For #19/#20, later resolve requested style provenance, content hashes, the
independent parent and a dependent style's locale override before evaluation;
return unavailable locales honestly so CSL fallback works. Reject missing parents
and cycles explicitly, never accept `[NO_PRINTED_FORM]` as successful resolution.
Preserve 0007's pin/custom/Zotero/update/bundle precedence and attribution.
No network fetch, all-style bundle, custom-style installation or settings-schema
change is authorized by this five-style prototype.

### Qualification and next work

After acceptance, first add bounded synthetic differential fixtures and the
optional native adapter, including known failing collation cases. Keep expected
parity failures explicit: characterization may pass by reporting a known mismatch,
but the parity verdict remains failed, never skipped or represented as support.
Measure cold initialization, warm latency, engine heap, peak process RSS, binary/dependency
delta, cancellation, limit failures and the next healthy request. The Node
baseline in #366 measures another runtime and does not establish these budgets.

Qualify native Linux, macOS and Windows on x86_64 and ARM64 with recorded target,
toolchain and exact head; cross-compilation alone is not a runtime pass.
The [upstream platform table](https://github.com/DelSkayn/rquickjs/blob/d7ef5eeae702fea24c03643064de454f1c1dd4b0/README.md)
calls Windows MSVC experimental and Windows ARM64 untested.
Unavailable targets stay unqualified. Browser/WASM production is outside this
prototype; keep the core's default dependency graph free of the native runtime.
Use existing tests/checks, without a new workflow or broad benchmark harness.

Then record whether collation, recovery, output policy and platform gates justify
production adoption. Public CLI/MCP/editor schemas, permissions and transport
remain separate contract decisions under #43/#789 and 0024. Nothing here adds
the network use, licensing exception or release change those decisions might
otherwise conceal.

## Consequences

The next experiment is buildable and reversible, while the GUI keeps its working
processor and watchdog. A failed runtime evaluation is useful evidence, not a
reason to weaken official-style support or silently change output. The prototype
may be rejected on known collation or recovery limitations; no production runtime
swap, extra abstraction or additional dependency is presumed by this decision.
