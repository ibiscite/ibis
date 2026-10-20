# 0012. Ship 0.1 as a single process; split out an engine only if measurements require it

Status: accepted (2026-10-18); amends 0004 for releases before the split is justified

## Context
ADR 0004 planned a small always-running engine process plus a separate picker process, to keep the background footprint low. Building the process split, its inter-process protocol and lifecycle before anyone can use the picker delays the first usable release and adds risk, while the expected memory saving has not been measured on the real app.

## Decision
- Releases up to and including 0.1 run as one Tauri process. The picker window is hidden, not destroyed, between uses; the global shortcut and library index live in the same process.
- All behaviour stays in `ibis-core`, so a separate engine process can be introduced later without moving logic.
- The split is reconsidered with measurements of idle memory, CPU wakeups and summon latency on macOS, Windows and Linux (#9). If the single process meets the budgets in `docs/architecture.md`, it stays.

## Consequences
The first usable release comes sooner. Idle memory is higher while the window is hidden (on the order of 100 MiB on macOS, to be measured). The process model is an implementation detail behind `ibis-core`, so changing it later is contained to the desktop crate.
