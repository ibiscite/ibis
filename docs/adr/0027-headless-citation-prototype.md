# 0027. Prototype bounded headless citation formatting in Rust

Status: proposed (2026-10-25); amends 0007

## Context

#366 needs the same citation formatter outside the desktop window for the CLI,
MCP and editors (0003). The current five-style adapter already runs without a
DOM, but its worker lifecycle and rich-output policy belong to the web interface.
The [investigation](https://github.com/ibiscite/ibis/issues/366#issuecomment-6393876017)
supports retaining Zotero's pinned citeproc-js. Bare QuickJS lacks the needed
collation/casing semantics. A [private ICU4X experiment](https://github.com/ibiscite/ibis/issues/366#issuecomment-6394081553)
repaired its observed differences without handwritten sorting. This decision
evaluates that concrete route, not bare QuickJS or production adoption.

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

### Processor, runtime and data pins

Keep the five styles, en-US locale and exact processor bytes in
`apps/desktop/vendor/csl/manifest.json`, including Zotero commit
`db41b99fddcc52d38897c59c161b6adaca036afb` and upstream processor source SHA-256
`4e717cb27c1d66024bbaa578604203c178af04e789c6244a722df44d70631466`.
Preserve the manifest's appended `moduleSuffix` for the generated ES module;
the source hash applies before that declared transformation.
Do not change pins or patch the processor while evaluating its host runtime.

Keep these direct crates optional behind `headless-csl-prototype`, with
default features disabled and only these features enabled:

| Crate | Exact version | Features | License |
| --- | --- | --- | --- |
| rquickjs | =0.14.0 | std | MIT |
| icu_collator | =2.4.0 | compiled_data | Unicode-3.0 |
| icu_casemap | =2.4.0 | compiled_data | Unicode-3.0 |
| icu_locale_core | =2.4.0 | alloc | Unicode-3.0 |
| writeable | =0.6.4 | none | Unicode-3.0 |

Lock rquickjs core/sys packages to 0.14.0 too. The published archives
identify [rquickjs commit d7ef5ee](https://github.com/DelSkayn/rquickjs/commit/d7ef5eeae702fea24c03643064de454f1c1dd4b0)
and include source matching its [QuickJS-NG gitlink](https://github.com/quickjs-ng/quickjs/commit/0fdea21ff1090084e91dad812b343d92e79ba9d9).
ICU component/data archives identify [commit 031d9e8](https://github.com/unicode-org/icu4x/commit/031d9e89f6a3fd26a14491e090729d46094328fd).
Lock `icu_{collator,casemap,normalizer,properties,locale_fallback}_data` to 2.4.0:
the snapshot uses Unicode 18/CLDR 49. Updates require output/data review.
The existing transitive `writeable` utility needs a direct trait import for the
bounded casing writer; its archive identifies [commit a14f2da](https://github.com/unicode-org/icu4x/commit/a14f2dad852be26bad277ae704abf27a15cbfed1).

Before adding dependencies, verify registry archive checksums in `Cargo.lock`
and run `cargo deny --all-features --locked check` without policy exceptions.
Retain third-party notices; the application's AGPL license is unchanged. The
default core/app graph must not enable the prototype; inspect shared transitive
lockfile changes separately rather than silently updating ordinary builds.
The proposed locale-core import omits full `icu_locale` canonicalization/data;
that slimmer graph is unbuilt, not the private experiment's measured footprint.

Do not enable loader, dyn-load, futures, parallel, macro, rust-alloc or companion
Node/web APIs. Use the runtime's default allocator: its documented heap cap is
ineffective with a custom allocator or rust-alloc. Evaluate only pinned adapter
and processor source; pass metadata as data, never interpolated executable text.
Expose only request-owned item/locale lookups, bounded ICU calls and fixed
diagnostics. No host filesystem, module loading, process, clipboard, settings
or network access.

### Request ownership and limits

Use the existing normalized CSL-JSON and citation intent, including ordered
repeated references, typed locators, prefix, suffix and author omission. Resolve
desktop records under its existing revision guard before formatting; release
the library lock before evaluation. Map each distinct resolved record identity
(source plus native key, 0005) to one request-local processor ID. Repeated citation
occurrences reuse that ID with independent locators and citation intent; positions
do not create new works. Distinct records sharing a citekey receive distinct IDs. Clear the
map/state after the request; no new public identity protocol is introduced.

Run one synchronous request in a fresh runtime/context on its owning test thread;
do not introduce a queue, worker pool or processor cache. Only a bounded per-request
collator cache is needed. The first slice is a five-style adapter plus fixtures,
without a public API or generic interchangeable-runtime layer.

These are conservative **prototype ceilings to test**, not measured production
budgets or a guarantee that every valid request fits:

| Resource | Starting ceiling |
| --- | --- |
| Selected CSL-JSON plus citation intent, serialized UTF-8 | 1 MiB; 1,000 records and 1,000 citation occurrences |
| Engine-accounted heap | 64 MiB |
| Engine stack | 1 MiB, with additional Rust/C worker-stack headroom |
| Active job | 5 seconds from dispatch, including initialization and fallback passes |
| Each returned representation, canonical or ownership preview | 1 MiB UTF-8 |
| All returned representations together, including preview | 3 MiB |
| Fixed diagnostics per result | 1,001, with bounded position indices |
| Canonical plus preview markup work | 10,000 combined nodes; depth 256 per tree |
| Each native comparison/casing input and casing output | 1 MiB UTF-8 |
| Native calls / cumulative operand and emitted bytes | 100,000 / 64 MiB per request |
| Cached collators / locale-key bytes | 32 entries / 128 bytes per key |

Bound before copying across the host boundary, including errors and diagnostic
counts; reject oversized work without truncating citation text. The VM heap cap
does not cap Rust buffers, thread stacks or process RSS. Count serialization and
markup work too; checking annotations alone does not bound canonical output.
The five immutable style assets are preloaded data, separate from request bytes.
Additional styles/CSL locale assets and custom XML are outside this first prototype.

Preflight native string sizes before Rust copies. Debit cumulative input/output
credits with checked arithmetic; enforce casing output through a bounded writer,
not a check after `_to_string`. Cache keys include locale and options; a full
cache fails explicitly rather than repeatedly rebuilding collators. Check the
same deadline/cancellation flag before and after every callback. These limits
bound admitted work, not ICU internal allocations or CPU inside one call: Rust
callbacks remain outside VM accounting; no total RSS bound is claimed. Record
native failure/cancellation/unsupported-input flags outside JS; reject the whole
request even if the processor catches the callback exception and returns text.

Run characterization in a parent-owned disposable test process, externally
stopped after ten seconds if it does not return. Sweep heap/stack ceilings to
distinguish resource exhaustion from output mismatches; record failures instead
of increasing limits silently. This external test guard is not a shipping process
architecture or a new background service.

### Timeout, cancellation and recovery

Use a monotonic active-job deadline and the runtime interrupt callback. Reject
pre-cancelled requests; active cancellation marks output obsolete and requests
interruption. Never publish an obsolete result. These are cooperative
controls: [native operations can miss interrupt polls](https://github.com/quickjs-ng/quickjs/issues/1753),
and an allocation error may be caught. Do not claim hard interruption, a process
memory bound or equivalence to terminating today's worker.

After timeout, exhaustion or an engine error, discard the runtime once control
returns; do not reuse partially mutated state. Admit no replacement runtime while
the previous request is still stuck. The external test parent kills a stuck child
and records failed in-process recovery. Production remains blocked unless bounded
recovery is demonstrated for the allowed data and operations, or a separately
reviewed containment change supplies it. Never patch that gap with a leaked
thread, unlimited retries or a silent GUI fallback.

Return fixed failure categories and affected input positions; never log raw
engine exceptions, references, style paths or request contents. Successful
citation text is output data, not diagnostics. Preserve explicit per-item
fallback warnings; a request-wide limit failure publishes no partial result.

### Locale, collation and output parity

Before constructing the processor, replace only its context-local
`String.prototype.localeCompare`, `toLocaleLowerCase` and `toLocaleUpperCase`
with ICU-backed operations. Leave `sys.stringCompare` unset: that global hook
bypasses locale preparation and misses author equality/case conversion paths.
Use maintained collator/case-mapper algorithms, never a handwritten comparator.

Preserve omitted comparison options separately from base sensitivity, numeric
ordering and ignored punctuation; casing uses the first requested locale, not
collator negotiation. Use explicit en-US defaults and prevalidated canonical
tags for the five-style/primitive fixtures. Locale-core parsing is not CLDR alias
canonicalization. Reject unqualified aliases/extensions, locale-list/option shapes,
malformed values and invalid strings with fixed errors. This internal bridge
is not a general ECMA-402/Intl polyfill; broader locale handling needs evidence.

Preserve the pinned adapter's standalone updateItems/makeBibliography/citation
path, including its sorting and disambiguation, rather than changing behavior
based on older generic docs. Numeric, ibid and document-dependent output stays
provisional (0007); no document-history API is introduced here. Compare current
worker and native output for all five styles, citation and bibliography, HTML,
text and complete RTF, with Unicode, date ranges, duplicate keys, repeated items,
mixed intent, edited fields, empty/malformed input and fallback/recovery cases.
For identity fixtures, call pinned citeproc directly with semantic record IDs as
the oracle: repeat one resolved record with different locators, and cite distinct
records sharing a key. Verify citation and bibliography behavior, including work
count/disambiguation. Comparing two position-ID adapters is not an independent
identity oracle; the current GUI cannot select the same record twice.

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

After acceptance, add only the optional adapter, bounded ICU callbacks and
synthetic differential fixtures. Keep expected parity failures explicit:
characterization may pass by reporting a known mismatch,
but the parity verdict remains failed, never skipped or represented as support.
Measure initialization, request latency, heap/RSS, binary/dependency delta,
cancellation, limit failures and the next healthy request. Native private tests
matched Node on 516 results, including 240 raw en-US CSL outputs. They exclude
Ibis fallback/ownership/sanitization/complete RTF and other native platforms.
The single-sample +1.29 MiB binary/+3.55 MiB RSS used full `icu_locale`, rustc
1.99.1 and a 20-second suite deadline, not the five-second job lifecycle.
Node 22.20.0 uses ICU 77.1/CLDR 47/Unicode 16; zero sampled mismatches against it
does not prove shipping-webview or data-version parity. Capture oracle versions.

When code lands, amend `scripts/check.mjs` and existing required Rust/native/MSRV
CI steps in the same change. In addition to default builds, require feature-enabled
`cargo test -p ibis-core --locked --features headless-csl-prototype` and matching
Clippy with `--all-targets -- -D warnings`; the existing Rust 1.90 job must test
that feature too. Keep all-feature cargo-deny. Feature-disabled green tests are
not prototype verification. Use existing jobs/test infrastructure, no new workflow
or benchmark harness; tests must report parity failures, not compile them out.

Qualify native Linux, macOS and Windows on x86_64 and ARM64 with recorded target,
toolchain and exact head; cross-compilation alone is not a runtime pass.
The [upstream platform table](https://github.com/DelSkayn/rquickjs/blob/d7ef5eeae702fea24c03643064de454f1c1dd4b0/README.md)
calls Windows MSVC experimental and Windows ARM64 untested. ICU metadata fits
Rust 1.90 (MSRV 1.88); compile/test the actual resolved graph on 1.90 before merging
code. Qualify the C build/bundled bindings; no silent bindgen or MSRV change.
Unavailable targets stay unqualified. Browser/WASM production is outside this
prototype; keep the core's default dependency graph free of the native runtime.

Then record whether collation, recovery, output policy and platform gates justify
production adoption. Public CLI/MCP/editor schemas, permissions and transport
remain separate contract decisions under #43/#789 and 0024. Nothing here adds
the network use, licensing exception or release change those decisions might
otherwise conceal.

## Consequences

The measured ICU route gives the next experiment a correctness path, while the
GUI keeps its processor and watchdog. Qualification failures remain useful
evidence; they do not justify weakening style support or changing output silently.
Production adoption remains a separate decision after parity, native resource
controls and target gates pass.
