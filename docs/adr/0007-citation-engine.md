# 0007. Citation engine and styles

Status: accepted (2026-10-17)

Evaluated by rendering every official style (2,863 independent, 8,002 dependent) with three citeproc-js builds.

## Decision
- Formatted citations use Zotero's maintained build of citeproc-js, pinned by commit and hash, running in a worker with a watchdog. Not GitHub master (page-range regression in 69 styles), not npm 2.4.63 (≈10× slower setup).
- Ship all official CSL styles and locales (~2 MB compressed). Track versions by content hash plus upstream commit.
- Style precedence: user-pinned version → user custom (`user:` IDs) → user's Zotero styles folder → opt-in signed weekly update bundle → bundled snapshot.
- Key-based outputs (Pandoc, BibLaTeX, LaTeX, Typst) are produced by `ibis-core::cite`, with per-syntax escaping and explicit warnings when a syntax cannot express something.
- Context-dependent styles (numeric, ibid, disambiguation) are labelled provisional outside a document.

## Consequences
A weekly CI job re-renders every style and opens a PR when output changes. Style updates never silently change a user's pinned style.
