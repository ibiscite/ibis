# Architecture

```
                      ┌─ Desktop picker (web UI in a Tauri shell; one process for now, ADR 0012)
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
  Expanded and inherited field values allow at most 256 nested brace groups before the upstream content parser runs. Deeper fields are omitted with a field warning while the reference and its other fields remain available. Literal escaped braces do not count. This bounds recursive command parsing on 2 MiB desktop loading threads.
  Each load shares at most `min(64 MiB + 64 × input bytes, 512 MiB)` work credits.
  Operations cost 32 credits plus conservative byte allowances for processing, copies and diagnostics; cached expansions pay for replay, preserving the separate logical field quotas.
  Exhaustion skips the current and remaining entries with a warning and retains completed references. A limited reload keeps the last good library instead.
  Sufficiently large or wide valid libraries can also reach this limit.
  The desktop watches the containing directory using `notify`, so save-by-rename keeps working. Selected symlinks also watch the resolved target directory and update that watch when the selected link changes. An absent target file in an existing directory is watched by its parent so creating the file recovers automatically. A bounded worker coalesces changes, waits for a quiet interval and requires two identical reads before publishing an indexed replacement. Unreadable, missing or unclosed files retain the last loaded references and show a safe error. Initial opens remain tolerant of damaged entries. A stable empty file replaces the index with an empty library. Writer intent cannot be inferred from a syntactically valid prefix; stability checks bound the observation period rather than guaranteeing that a paused writer has finished. Network filesystems may not deliver notifications; Reload remains available.
- **Zotero:** local API when Zotero is running; otherwise a consistent read-only snapshot of its database, through an adapter tested against each supported Zotero schema; Better BibTeX exports as a third option. See ADR 0006.

## Search

Keyword search is instant, case- and accent-insensitive, works in every script and reports which fields matched. Exact citation-key and title matches rank first. Meaning-based multilingual search comes after 1.0 and will be combined with, never substituted for, keyword results. See ADR 0010.

## Formatting

Formatted citations and bibliographies use citeproc-js with the official Citation Style Language styles and locales. Key-based outputs are produced by `ibis_core::cite`, which applies the escaping and multi-reference rules of each syntax and reports what a syntax cannot express. See ADR 0007.

## Insertion

Every insertion ends in one visible state: inserted, or copied with a reason. See `docs/compatibility.md` and ADR 0009.

## Desktop process model

The desktop app (`apps/desktop`) is a Tauri 2 shell: a React and TypeScript interface in the system web view, and a thin Rust layer that calls `ibis-core`. The web interface has no file system, shell or network permissions; it can only call the app's own commands.

Until measurements justify more, the app is a single process that keeps the library index in memory and hides its window between uses (ADR 0012). The original plan of a small background engine plus an on-demand picker process (ADR 0004) remains the option if idle memory or wakeups exceed the budgets.
