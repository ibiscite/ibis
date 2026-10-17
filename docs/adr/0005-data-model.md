# 0005. Canonical data model: CSL-JSON plus raw source record

Status: accepted (2026-10-17)

## Context
Every CSL processor reads CSL-JSON; Zotero, Better BibTeX and doi.org emit it. BibLaTeX is richer but LaTeX-laden. Citation keys repeat across libraries.

## Decision
Each record has: identity = source + native key; normalized fields mapping to CSL-JSON (1.0.2 variables); the raw source record kept verbatim; optional citation key. Annotations and notes are modelled from the start (for "cite a highlight").

## Consequences
BibLaTeX output can round-trip from the raw record. Search and formatting share one model. Nothing from the source is lost or silently rewritten.
