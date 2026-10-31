# Pre-stable architecture review: plan draft (temporary, for review; not to merge)

Source: the maintainer's 2026-10-31 proposal to reopen identity, storage, desktop
framework, the public API, a caller CSL-JSON path, extensibility proofs and
performance gates before 1.0. Checked against origin/main 8c0c81a.

## Verified claims

- `LibraryId` accepts only `^(file|zotero):[0-9a-f]{64}$` (crates/ibis-core/src/contract/model.rs:14).
  Kind is inferred from the prefix (ibis-cli/src/human.rs:65, ibis-cli/src/doctor.rs:254,
  ibis-sources/src/zotero_source.rs:90). Zotero ids are constants
  (`sha256("default:user")`, zotero_source.rs:85), so two Zotero profiles share one id.
  File ids are unsalted SHA-256 of the canonical path (ibis-sources/src/file.rs:159).
- Keyless CSL-JSON records use `id` as their citation key (ibis-sources/src/csl_json.rs:80);
  `1` and `"1"` collapse and the second is dropped as a duplicate (csl_json.rs:97, 198-221);
  CSL-JSON export overwrites `id` with the key (ibis-core render.rs:477).
- Source health: `Engine::finalize` reads global library state after the call
  (ibis-engine/src/service.rs:274); the CLI only changes the exit code and prints a stderr note
  pointing at a later `ibis libraries`; the JSON envelope carries no findings; RPC returns
  only `result.data` (ibis-engine/src/server.rs:82); MCP carries none.
- Responses use `deny_unknown_fields` / `additionalProperties:false` and closed enums; the
  envelope is a fixed `ibis: 1`; `ibis schema` carries no version; only CLI `doctor` reports
  contract version and capabilities; no formatter/style version is reported.
- No inline caller-records path exists; callers write a temp file. That path probes
  `std::fs::metadata` on `file` fields (attachments.rs:164).
- 100k open/refresh is enforced nowhere today: report-only on branch builds and below v0.3
  tags (scripts/preview-budgets.mjs:7-10), while docs/architecture.md:285, :347 and :448
  say it fails nightly. `crates/ibis-engine/tests/source_formatter_performance.rs` is
  release-only and no workflow runs it (preview.yml:249 covers only ibis-core/ibis-sources).
  Summon, typing, idle CPU and whole-app memory have a validator (`release-budgets.mjs`)
  with no collector.
- Storage: only the first slice of the flat generation design exists (~1,400 production
  lines: ibis-sources/src/generation.rs, file/cache.rs, ibis-engine/src/service/file_cache.rs).
  The design rules SQLite out as "a new dependency" (architecture.md:75-77), but bundled
  rusqlite is already in the app (ibis-sources/Cargo.toml:17). Lazy objects, Zotero
  generations and the 100k <3 s / <400 MB target are unbuilt, so comparing now is cheapest.
- Desktop: src-tauri has ~26k Rust lines; ~600 call Tauri APIs. Paste, focus, portal and
  secure-input code (~6.6k) is framework-independent; the React UI is ~7k lines plus ~15k tests.

## Judgement

| Proposal step | Verdict |
|---|---|
| 1 Identity and fidelity | Holds. Do it before more storage work so persisted generations do not bake in prefix-typed ids. |
| 2 Storage comparison | Holds. Time-boxed spike, same records and scorer, identical results before timing. |
| Desktop comparison | Partly. Measure Tauri against the gates a shell affects first; build an Electron/Qt picker only for a framework-caused failure. A full three-way build costs ~2-4 agent-weeks plus maintainer screen-reader/IME sessions. Maintainer chooses. |
| 3 One external API | Holds. Source health with the result; API version separate from binary/formatter/style; strict requests, forward-compatible responses, unknown code = needs review; discovery from existing registry/schema/doctor. |
| 4 Caller CSL-JSON path | Holds, small. Request-local snapshot from existing `load_str` + `Snapshot::new` + `cite_snapshot`. |
| 5 Extensibility proofs | Holds as the evidence for 1.0's "versioned engine contract". No plugin machinery. |
| 6 Executable performance | Holds, and is the cheapest and most urgent: the docs currently overclaim. |

## Sequence (after current P1s #1519, #1520, #1529, #1530)

Engine and CI work only; none of it touches picker, copy, paste, IPC or settings state, so it
runs beside the 0.3 UI lane without using one of the two risky-area PR slots, except where noted.

1. **Make every performance budget a check that runs** (0.3). Can start now: CI and docs only.
   Fix the doc contradiction, run the formatter benchmarks in release in Preview, add the
   collector for `release-budgets.mjs`, one table of budget -> check -> channel -> mode.
2. **Separate source identity from kind and citation keys** (0.3). Risky area (citation
   syntaxes, BibTeX/Zotero reading): independent review. Must merge before any persisted
   generation format beyond the current `.bib` slice.
3. **Spike: flat generation files vs private SQLite store** (0.3), in parallel with 2 on a
   throwaway branch, three-day time-box. Output: results on the issue and a decisions entry.
   The "rebuild library storage" item then builds the winner, on 2's identity model.
4. **Source health with the result, and API compatibility rules** (0.4, with strict mode
   and receipts). After 2.
5. **Render and export caller-supplied CSL-JSON records** (0.4). After 2 and 4.
6. **Prove the contract with real clients** (1.0): Python stdlib stdio client in CI (records,
   render, findings, cancel), a Rust host without manager or desktop, a contributor corpus.
   After 4 and 5.
7. **Qualify the desktop shell against the release gates** (0.3 gates), using 1's collector.
   Alternatives only per the maintainer's choice.

## Done-when summaries

Each issue carries scenarios first (success, refusal, failed I/O, delayed A after B,
close/reopen) and a decision rule; existing code gets no automatic preference (decision 0027).
