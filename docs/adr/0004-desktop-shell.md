# 0004. Desktop shell: Tauri 2, engine and picker as separate processes

> Amended by ADR 0012: releases start as a single process; the split follows only if measurements require it.

Status: accepted (2026-10-17). Background-memory targets to be confirmed by measurement of the two-process build.

## Context
Probes compared Electron (~290 MiB bundle, ~448 MiB visible), Tauri (~11 MiB bundle, ~170 MiB visible, ~124 MiB after closing the window because WebKit stays loaded) and Qt (embedded citation engine too slow). Alternatives without good screen-reader support on Linux (Iced, Compose, MAUI) are excluded. Raycast 2.0 (2026) uses the same shape: native host, shared web UI, Rust core.

## Decision
Tauri 2 with a TypeScript/React UI. The always-running part is a small Rust engine process (hotkey, index, insertion). The picker UI is a separate process started on demand and exited when idle. Wayland hotkeys use the XDG GlobalShortcuts portal directly (the Tauri plugin is X11-only), with an `ibis pick` command users can bind as a fallback.

## Consequences
Two processes and an IPC boundary to maintain. Three different system webviews (WebKit, WebView2, WebKitGTK) must be tested, including with screen readers. Revisit if Slint's accessibility matures (re-evaluate in 2027).
