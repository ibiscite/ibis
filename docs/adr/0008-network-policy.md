# 0008. Offline by default; every network feature opt-in

Status: accepted (2026-10-17)

## Decision
No telemetry ever. All features work offline. Update checks, style updates, metadata lookup and reference verification are each off by default, explained once, state exactly what is sent, and can be turned off. Every flow is listed in `PRIVACY.md`. Tauri capabilities and CSP restrict network access to those endpoints.

## Consequences
No usage data for prioritizing work; we rely on issues and the compatibility list instead.
