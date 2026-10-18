# 0010. Search: instant keyword first, meaning-based search added later

Status: proposed (2026-10-17). Model selection awaits quality, memory and latency measurements.

## Decision (proposed)
Keyword search is instant, accent- and case-insensitive, works for every script, and explains why each result matched. Exact key and title matches rank first. 0.x uses an in-memory index; a persistent SQLite FTS5 index follows for large libraries. Meaning-based, cross-language search (after 1.0) uses a small local multilingual model loaded only while the picker is open, fused with keyword results; no cloud.

## Consequences
Keyword results never wait for a model. Model choice will be fixed by measured quality, memory and speed.
