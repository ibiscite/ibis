# 0028. Engine first: the command line is the engine, the app is its client

Status: accepted (2026-10-26); amends 0003, 0005, 0006 and 0007; supersedes the "CSL formatting stays in the web interface" part of 0007

## Context

On 2026-10-26 the maintainer chose one independent Ibis picker that covers what Zotero's citation dialog and Better BibTeX offer for picking, keys, formatting and pasting, then goes further. He asked for an engine usable by people and AI agents with full functionality, and an app built on top of it, in the way Codex and Claude Code put an app over an open engine. Research for the rebuild found that the command line today cannot read Zotero or format with CSL styles, formatting runs only in the web view (so output can differ by platform), library state lives in the desktop crate, and the data model knows only references.

## Decision

- **Engine and front ends.** Every capability lives in Rust crates (`ibis-core`, `ibis-sources`, `ibis-csl`, `ibis-engine`) as typed methods. `ibis` (command line), `ibis serve --stdio` (JSON-RPC 2.0), `ibis mcp` and the desktop app are clients of those methods; the app links them in-process. The command line exposes everything; each other front end exposes an explicit subset, and tests fail when two front ends give different results for the same request.
- **Public contract.** Methods take and return JSON described by a published JSON Schema (`ibis schema`, generated with `schemars`). Outputs use a versioned envelope with stable error codes and the exit codes in `docs/architecture.md`. Before 1.0 the contract may change freely (ADR 0027); from 1.0 it is versioned.
- **One citation request, one result**, as described in `docs/architecture.md`, with typed losses and refusals instead of plausible but wrong text.
- **Formatting in the engine.** Zotero's citeproc-js runs in embedded QuickJS (`rquickjs`) with ICU4X collation. A prototype matched Node byte for byte for five styles, citations and bibliographies, in HTML, text and RTF. Before the web view stops formatting, multilingual fixtures and cross-platform benchmarks must pass; if they do not, a new ADR chooses the formatter before the web view stops formatting.
- **Data model.** Records, attachments, notes, annotations, collections, tags and saved searches, addressed by `library + key`, with opaque library IDs, immutable snapshots and per-object fingerprints. Ibis maps Zotero's raw data to CSL itself so Zotero open and closed give identical results.
- **Read-only.** Sources stay read-only (ADR 0006); Zotero 10's local write API is recorded for a later opt-in ADR.

## Consequences

The command line, agents and the app see the same behaviour, and tests compare them byte for byte. The engine grows by about 5–6 MB for QuickJS, ICU collation data and the bundled styles. A 100-entry bibliography takes seconds in QuickJS rather than under one in V8, so bibliographies run in the background. QuickJS runs style and record data in the main process, so the formatter gets memory and time limits and hostile-input tests. `tauri-specta` is replaced by types generated from the JSON Schema.
