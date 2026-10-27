# 0008. Offline by default; every network feature opt-in

Status: accepted (2026-10-17)

## Decision
No telemetry ever. All features work offline. Update checks, style updates, metadata lookup and reference verification are each off by default, explained once, state exactly what is sent, and can be turned off. Every flow is listed in `PRIVACY.md`. Tauri capabilities and CSP restrict network access to those endpoints.

Requests to Zotero on this computer (`127.0.0.1` only) are local reads, not network features: Zotero's local API and, read-only, Better BibTeX's JSON-RPC methods `user.groups` and `item.citationkey` on the same port, with no proxy and no redirect (maintainer decision, 2026-10-27, #1231). Each is listed in `PRIVACY.md` from the release that makes it.

## Consequences
No usage data for prioritizing work; we rely on issues and the compatibility list instead.
