# 0009. Insertion never fails silently; destination behaviour is data

Status: accepted (2026-10-17)

Edge cases are catalogued in `docs/compatibility.md`.

## Decision
Every insertion ends in exactly one visible state: inserted, or copied with a reason. The insertion state machine captures the target, writes all clipboard formats, restores focus, pastes with a layout-aware shortcut, and restores the previous clipboard only if unchanged. It refuses to inject into secure input, elevated windows and lost targets. Per-app behaviour lives in destination profiles (YAML, schema-validated): built-in, then community, then user overrides; a "that didn't work" action creates a user rule.

## Consequences
New problem apps are fixed with data, not code, and the community compatibility list grows with use. Some environments (e.g. GNOME Wayland without the RemoteDesktop portal) are copy-only by design.
