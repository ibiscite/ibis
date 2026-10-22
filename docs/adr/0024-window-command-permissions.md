# 0024. Restrict desktop commands by window

Status: accepted (2026-10-22)

## Context

Without an application command manifest, Tauri checks capabilities for plugin
commands but lets bundled pages call every application command. The Copied
notice's event-only capability therefore did not restrict its Ibis commands.
Issue #520 requests an enforced window boundary.

## Decision

Declare every registered Ibis command in a Tauri `AppManifest`. Grant the
`main` picker window its 24 commands explicitly. Grant the `notice` window
only `copied_notice`, alongside its existing event subscriptions. Neither
capability grants remote origins access. Other windows have no Ibis commands.

Keep the manifest inventory checked against the runtime command registry.
Test the compiled capabilities through Tauri's IPC dispatcher so denied
commands cannot reach an application handler.

## Consequences

New commands require an explicit manifest and capability decision. Generated
permission files are build output, recreated from the checked inventory.
This is a window boundary: the debug gallery reuses `main` and receives the
picker's permissions. Pages sharing a window are not isolated from one another.
The web interface still has no filesystem, shell or network plugin permissions.
