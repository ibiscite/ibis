# Architecture

```
                      ┌─ Desktop picker (web UI in a Tauri shell; one process for now, ADR 0012)
                      ├─ Command line:  ibis search · ibis cite · ibis check · ibis csl
 ENGINE (Rust) ───────┼─ MCP server (read-only) for AI assistants            [planned]
 small, long-running  ├─ Editor endpoint compatible with Better BibTeX CAYW  [planned]
                      └─ Browser extension, launcher integrations, web build  [later]
   │
   ├─ Sources: BibTeX/BibLaTeX · Zotero (open) · RIS, CSL-JSON, Zotero closed [planned]
   ├─ Search index in memory · persistent store                     [planned]
   ├─ Key-based syntaxes in Rust (CSL styles: citeproc-js in the web UI for now)
   ├─ Insertion: state machine + destination profiles              [planned]
   └─ Health rules                                                  [planned]
```

## Principles

- **One engine, thin front ends.** All behaviour that can be tested without a window lives in `crates/ibis-core`; it depends on neither Tauri, a GUI toolkit nor the network. Logic needed by two front ends belongs in the core.
- **Read-only sources.** Never write to Zotero, `.bib` files or any library. Original records are preserved verbatim next to normalized data.
- **Stable identity.** A record is identified by its source plus native key, never a citation key alone.
- **Never fail silently or guess data.** Skipped data becomes a `LoadWarning`; a failed insertion leaves the citation copied with a reason. Reject invalid input with a clear error and warn about missing metadata rather than inventing it.
- **Offline by default.** No telemetry. Network access requires an accepted ADR; every network feature is opt-in and listed in [PRIVACY.md](../PRIVACY.md).
- **Behaviour as data.** Destination profiles, output templates and styles are validated data and never execute code.
- **Escape output.** Escape every output for its target syntax, including LaTeX, Pandoc, Typst, HTML and RTF.
- **Keep private data private.** Never put real reference data, document text, clipboard contents, credentials, personal paths or usernames in tests, logs, diagnostics, issues or commits. Never commit or print these. Fixtures are synthetic; diagnostics use an allowlist rather than raw errors or payloads.
- **Local endpoints.** Bind to `127.0.0.1`, require a token and reject browser `Origin` headers.

## Data model

Each record has an identity (source + native key), normalized fields that map to CSL-JSON, an optional citation key, and the raw source record kept verbatim. Citation keys can repeat across libraries and are never used as identity. See ADR 0005.

## Sources

- **BibTeX/BibLaTeX:** each top-level entry is parsed independently, so a broken entry, duplicate key or undefined `@string` macro produces a warning with a line number while every other entry still loads. Parenthesis-delimited entries, LaTeX accent commands and `crossref` inheritance are supported.
  Expanded and inherited field values allow at most 256 nested brace groups before the upstream content parser runs. Deeper fields are omitted with a field warning while the reference and its other fields remain available. Literal escaped braces do not count. This bounds recursive command parsing on 2 MiB desktop loading threads.
  Unbalanced structural braces omit the field with a warning before parsing or conversion fallback, so value text cannot become other fields. Date components are checked before normalization can consume them; other fields are checked at that same final boundary. Literal escaped braces remain text.
  Each load shares `64 MiB + 128 × input bytes` work credits, using saturating arithmetic.
  Operations cost 32 credits plus conservative byte allowances for processing, copies and diagnostics; cached expansions pay for replay, preserving the separate logical field quotas.
  Exhaustion skips the current and remaining entries with a warning and retains completed references. A limited reload keeps the last good library instead.
  The size-proportional allowance covers the generated 200-record/65,535-byte-note and 20,000-record/ten-field ordinary-library cases, even when loading takes longer. Adversarial valid aliases or inheritance graphs can still exhaust it.
  Clean first-parent inheritance snapshots are shared across references within a 4 MiB cache, including retained field allocations and empty-entry metadata.
  Only complete, warning-free walks qualify; roots still compute their own diagnostics. Replay preserves the per-root evaluation quota, while actual cache metadata work and subsequent field copies consume load credits.
  The desktop watches the containing directory using `notify`, so save-by-rename keeps working. Selected symlinks also watch the resolved target directory and update that watch when the selected link changes. An absent target file in an existing directory is watched by its parent so creating the file recovers automatically. A bounded worker coalesces changes, waits for a quiet interval and requires two identical reads before publishing an indexed replacement. Unreadable, missing or unclosed files retain the last loaded references and show a safe error. Initial opens remain tolerant of damaged entries. A stable empty file replaces the index with an empty library. Writer intent cannot be inferred from a syntactically valid prefix; stability checks bound the observation period rather than guaranteeing that a paused writer has finished. Network filesystems may not deliver notifications; Reload remains available.
- **Zotero:** the local API while Zotero is running, or a Better BibTeX export as a `.bib` file. Reading a consistent read-only snapshot of Zotero's database while Zotero is closed is planned for 0.3 (#14). See ADR 0006.

## Search

Keyword search is instant, case- and accent-insensitive, works in every script and reports which fields matched. Exact citation-key and title matches rank first. A pasted DOI, ISBN, PubMed ID or arXiv ID (bare or as a link) finds the records that carry it; if none does, ordinary search runs. Source scopes use exact record source IDs and filter the loaded snapshot before identifier matching, ranking and limits; no source is reopened. An omitted scope searches all loaded sources, while an empty scope searches none. Desktop status counts describe the searchable snapshot by those same IDs, including zero for empty loaded Zotero libraries; a failed switch keeps the retained snapshot’s counts. Meaning-based multilingual search comes after 1.0 and will be combined with, never substituted for, keyword results. See ADR 0010.

## Formatting

Formatted citations and bibliographies use citeproc-js with the official Citation Style Language styles and locales. For now this runs in the desktop web interface (`apps/desktop/src/format/`), not in the core, so the command line cannot format with styles; moving it into the core is planned for 0.5 (#366). Key-based outputs are produced by `ibis_core::cite`, which applies the escaping and multi-reference rules of each syntax and reports what a syntax cannot express. See ADR 0007.

## Insertion

Planned for 0.4; today the picker copies. Every insertion will end in one visible state: inserted, or copied with a reason. See `docs/compatibility.md` and ADR 0009.

## Desktop process model

The desktop app (`apps/desktop`) is a Tauri 2 shell: a React and TypeScript interface in the system web view, and a thin Rust layer that calls `ibis-core`. The web interface has no file system, shell or network plugin permissions. An application command manifest enforces the per-window capabilities (ADR 0024): `main` can call the 24 picker commands; `notice` can call only `copied_notice` and subscribe to events. A second small window shows that "Copied" notice after a clean copy hides the picker; it never takes focus and is left out of the taskbar, Dock and Alt+Tab. Neither capability allows remote origins. The debug gallery shares `main` and its permissions; the boundary does not isolate pages within one window.

Until measurements justify more, the app is a single process that keeps the library index in memory and hides its window between uses (ADR 0012). The original plan of a small background engine plus an on-demand picker process (ADR 0004) remains the option if idle memory or wakeups exceed the budgets in `scripts/smoke-budgets.json`.

Native dialogs share an owned registry in `picker/dialogs.rs`. Each callback
holds its lease through completion, including source changes or nested error
dialogs; only its own lease is released. The file chooser is exclusive, while
message dialogs can overlap. Both blur guards use this registry, which refuses
new owners and keeps blur hiding blocked if its lock or owner counter is unavailable.

Shared interactive web controls live in `apps/desktop/src/ui/` (ADR 0021).
Only that directory imports React Aria; `scripts/ui-imports.mjs` checks static
imports, re-exports and literal dynamic imports/require calls. It reuses Vite's
pinned JavaScript/TypeScript parser; it is a contributor boundary check, not a
security sandbox for computed module names. During
incremental migration, toolbar-menu role overrides in `tokens.css` apply locally
to its popover, preserving the existing More menu and keys dialog roles.

Desktop search replies and each result carry an opaque in-memory index revision,
captured with the index under the library mutex. Scope filtering and request
cancellation preserve it even for empty replies. Per-source status counts still
describe the retained snapshot, including chosen empty Zotero libraries.
A replacement retires that revision; loading,
failed reloads retaining the index, and unchanged Zotero checks keep it. Selected
references and formatted previews retain the revision they came from. Key preview
and copy, CSL retrieval and formatted copy validate it under that same mutex
before preparing output. Key preview performs no clipboard or settings I/O;
clipboard writes follow after unlocking. Stale or mixed choices
are refused with a fixed refresh/reselection explanation, rather than resolving
old visible identities against new metadata. This does not change core/CLI
search types or persisted settings.
