# Architecture

```
                      ┌─ Picker window (web UI, started on demand, exits when idle)
                      ├─ Command line:  ibis search · ibis cite · ibis check · ibis csl
 ENGINE (Rust) ───────┼─ MCP server (read-only) for AI assistants            [planned]
 small, long-running  ├─ Editor endpoint compatible with Better BibTeX CAYW  [planned]
                      └─ Browser extension, launcher integrations, web build  [later]
   │
   ├─ Sources: BibTeX/BibLaTeX · Zotero · RIS · CSL-JSON · others later
   ├─ Store and search index
   ├─ Formatting: citeproc-js with official CSL styles; key-based syntaxes in Rust
   ├─ Insertion: state machine + destination profiles
   └─ Health rules
```

## Principles

- **One engine, thin front ends.** All behaviour that can be tested without a window lives in `crates/ibis-core`.
- **Read-only sources.** Libraries are never modified. Original records are preserved verbatim next to normalized data.
- **Never fail silently.** Every skipped record and every failed insertion is reported with a reason.
- **Offline by default.** No telemetry; every network feature is opt-in (see `PRIVACY.md`).
- **Behaviour as data.** Per-application insertion rules, output templates and styles are data validated by schemas, so most compatibility fixes need no code.

## Data model

Each record has an identity (source + native key), normalized fields that map to CSL-JSON, an optional citation key, and the raw source record kept verbatim. Citation keys can repeat across libraries and are never used as identity. See ADR 0005.

## Sources

- **BibTeX/BibLaTeX:** each top-level entry is parsed independently, so a broken entry, duplicate key or undefined `@string` macro produces a warning with a line number while every other entry still loads. Parenthesis-delimited entries, LaTeX accent commands and `crossref` inheritance are supported.
- **Zotero:** local API when Zotero is running; otherwise a consistent read-only snapshot of its database, through an adapter tested against each supported Zotero schema; Better BibTeX exports as a third option. See ADR 0006.

## Search

Keyword search is instant, case- and accent-insensitive, works in every script and reports which fields matched. Exact citation-key and title matches rank first. Meaning-based multilingual search is added in 0.4 and is combined with, never substituted for, keyword results. See ADR 0010.

## Formatting

Formatted citations and bibliographies use citeproc-js with the official Citation Style Language styles and locales. Key-based outputs are produced by `ibis_core::cite`, which applies the escaping and multi-reference rules of each syntax and reports what a syntax cannot express. See ADR 0007.

## Insertion

Every insertion ends in one visible state: inserted, or copied with a reason. See `docs/compatibility.md` and ADR 0009.

## Desktop process model

A small Rust engine process owns the global shortcut, index and insertion. The picker interface runs in a separate process that is started when needed and exits when idle, so the background footprint stays small. See ADR 0004.
