# 0065 One warm engine, thin native shells

**Decided** 2026-11-26 by the maintainer ("be bold and think of the future; do
not let the past cloud judgement"), after:
- two research reports (shell architecture, and what 1.0 must do);
- an outside critique;
- a native macOS spike (SwiftUI picker and Settings › General over
  `ibis serve`).

It supersedes the shell part of [0004](0004-desktop-shell.md). It builds on
[0057](0057-one-method-table.md), [0064](0064-one-citation-one-status-generated-surfaces.md)
and the connection runtime planned for 0.7.

**Amended** 2026-11-26 by the maintainer, after testing the nightly built
from #2052: the Mac app "does not feel native", and the problem is structural.
A web view draws its own copy of every Mac control, so text, controls,
menus, scrolling, focus and window chrome stay imitations that drift with
each macOS release. **The native macOS shell goes ahead now**; it no longer
waits for the measurement below. The Tauri shell over `ibisd` continues on
Windows and Linux and stays on the Mac until the native one reaches parity.
No more effort goes into making the web view imitate macOS.

## Why

- **Every front end loads its own engine.** The app, `ibis`, and each AI
  app's MCP server each open their own library snapshot. At 100k references
  that is 374–433 MiB and 2.6–5.8 s per process
  ([architecture](../architecture.md)). The processes also collide on
  settings and Zotero locks (`SettingsError::Busy`, `StorageInUse`).
  `ibis toggle` polls a receipt file.
- **The web view redraws every macOS control in CSS.** Menus, pop-ups,
  scaling, materials and window behaviour were each measured, rebuilt and
  reviewed by hand. One day took three or more review rounds each on menus,
  scrolling and the Settings window lifetime. AppKit provides all of these.
- **The spike** (579 lines of Swift against about 845 lines of the
  comparable React and CSS):
  - native controls with zero custom metrics;
  - a first engine result 99 ms after spawning `ibis serve`;
  - an engine kept behind a process boundary, which preserves crash
    isolation without bindings.

  macOS has no per-app Dynamic Type; text size follows the system.
- **The current Mac shell fails 0032's memory budget.** The installed nightly
  (0.3.14-nightly.20261126075620, a real library, hidden and idle for four
  hours) measures about 255 MB of physical footprint across its processes,
  against a budget of 200 MiB hidden:

  | Process | Footprint |
  | --- | --- |
  | `ibis-desktop` (engine and shell) | 71 MB |
  | WebKit page, main | 136 MB |
  | WebKit page, second | 22 MB |
  | WebKit GPU | 19 MB |
  | WebKit networking | 7 MB |

  The web view costs about 184 MB, more than twice the engine. This is the
  release-gate failure 0004 names as the reason to change shells.

## The decision

1. **`ibisd`: one warm engine per person.**
   - **What it is:** a per-user daemon, started on demand by any client. It
     owns libraries, settings, recents, citation sessions (the Draft) and
     status.
   - **The contract:** the generated 0057/0064 contract over a same-user
     local socket (a Unix domain socket; a named pipe on Windows) with
     peer-credential checks. No network listener.
   - **Clients:** desktop shells, `ibis`, MCP, editor endpoints, and system
     extensions (Services, Shortcuts, launcher plugins).
   - **Isolation:** untrusted parsers stay in their child processes
     ([0055](0055-agent-layers-full-text-writes-remote.md),
     [0060](0060-one-document-pipeline.md)). A crashed daemon is restarted by
     the next client.
   - **`ibis serve --stdio`** remains for clients that cannot use the socket.
2. **Shells are thin clients.** They render state, own their windows and
   controls, send intents, and hold no rules of their own.
   - **macOS: a native SwiftUI/AppKit shell is the flagship.** It has a
     key-capable `NSPanel` picker, a `Settings` scene with grouped forms and
     a sidebar with search
     (0062), and the menu
     bar item, Services and App Intents. It updates through Sparkle (EdDSA),
     the Mac's standard. It targets macOS 15 and later (amended 2026-11-28,
     [0066](0066-native-shells-use-system-parts.md)). Delivery
     (`ibis-native`: paste, focus and site reads, placement and drag) stays
     **in-process in each signed desktop shell**. The macOS Accessibility
     grant, hiding before posting and 0029's timing proofs belong to the
     app people granted. A native shell links it through a narrow Rust
     interface and never reimplements it in Swift.
   - **The switch is earned by measurement** (superseded 2026-11-26: the native Mac shell goes ahead now; see the amendment above). After `ibisd` is extracted,
     the Tauri shell must meet 0032's hidden-memory budget and its summon
     budget (≤100 ms p95 to typeable) together, on the 10k fixture. Hidden
     Settings and Reader web views are destroyed when closed. A web view
     kept warm costs about 184 MB; one recreated per summon misses the
     latency budget. If both cannot hold together, the native Mac shell goes
     ahead, starting with the picker. Otherwise Tauri stays on the Mac and
     this decision is revised.
   - **The Reader stays an isolated web view** in every shell
     ([0036](0036-reader-trust-boundary.md)): PDF.js receives raw bytes, and
     no native PDF parser runs beside the library. A native shell embeds it
     on demand.
   - **Windows and Linux: native apps follow the Mac** (amended 2026-11-28,
     [0066](0066-native-shells-use-system-parts.md) point 4): WinUI 3 after
     Mac parity, then GTK 4 with libadwaita, each over the same socket. Until
     each reaches parity on the same user journeys, the Tauri app serves that
     system with fixes only, in its own neutral recipe. **It is not converted
     to an `ibisd` client** (amended 2026-11-28): it keeps its engine until a
     native app replaces it on that system, and shares the daemon's files
     (settings, recents, paste approvals) rather than its memory. A Draft,
     paste approvals and delivered-once records live in files the engine
     owns, so a daemon restart loses none of them.
   - **Measurement is the baseline:** whole-process memory, idle wake-ups,
     and summon time to typeable on a 10k and a 100k library, for every
     shell, measured against 0032's budgets. Each native app must beat the
     Tauri app's numbers on its system.
3. **System integration is reuse, not rebuilding.** Each of these is a thin
   socket client:
   - **macOS:** Services, App Intents and Quick Look.
   - **Windows:** a PowerToys or Command Palette extension.
   - **Linux:** KRunner and the GNOME search provider.
   - **Everywhere:** Raycast, Alfred, VS Code and Obsidian plugins.
4. **Documents stay text-first.**
   - Ibis is the best companion to Pandoc, Quarto, Typst, LaTeX and Overleaf.
     Note styles, ibid. and legal citations are delegated to their compilers
     where they already solve them.
   - Word and Google Docs get formatted paste and the existing copy pipeline;
     Ibis builds no live fields or add-ins of its own (ROADMAP › Do not
     build).
   - Mendeley and Citavi arrive through their exports, which Ibis already
     reads.

## The daemon's contract

- **Identity and security:**
  - **Per user:** the socket is in a `0700` directory under
    `$XDG_RUNTIME_DIR` or the user's Library, with the peer's UID checked
    (`SO_PEERCRED`, `getpeereid`).
  - **Windows:** the named pipe uses `FILE_FLAG_FIRST_PIPE_INSTANCE`, a
    current-user-SID DACL, `PIPE_REJECT_REMOTE_CLIENTS` and a peer token
    check.
  - **No network listener.** Another user's session never reaches it.
- **Lifecycle:**
  - The first client starts `ibisd` under an exclusive lock, so racing
    clients join one instance.
  - Its residency follows 0032's idle budget: warm while a shell runs; after
    a measured idle period otherwise.
  - It needs no `systemd --user`, `launchd` agent or service. It lives inside
    the app bundle or package beside `ibis`.
- **Version skew:**
  - Every connection opens with a handshake carrying the protocol version
    and build ID.
  - On a mismatch the newer side asks the daemon to drain and restart.
  - Updates (Sparkle, the Tauri updater, packages) restart it.
- **Crashes:** a crash ends its parser children (process groups or Job
  Objects, as 0029 does). Clients reconnect with backoff. A Draft is held in
  memory only and is lost with the daemon; the shell says so once.
- **Ownership:**
  - `ibisd` owns `settings.json` writes, keeps watching for hand edits, and
    is the only writer of recents and sessions.
  - Revoking paste is synchronous: the daemon tells shells to drop pending
    deliveries before it answers.

## Order

1. Build `ibisd` beside `ibis serve`. The same method table serves both. The
   CLI, MCP and the native apps are clients; the Tauri app shares its files
   instead (amended 2026-11-28).
2. Grow the macOS native shell from the spike to parity (amended
   2026-11-26: now, not after a measurement):
   1. picker;
   2. delivery and paste;
   3. Settings;
   4. Sparkle;
   5. signing.

   One stream (the maintainer, 2026-11-26: "keep it simple and all in one
   stream nightly"): there is no separate native app for testers. Once the
   native picker and paste work, the Mac nightly is the native app, and the
   Tauri Mac build is retired from it; Settings, Sparkle and signing follow in
   later nightlies.
3. Measure every shell against 0032 (memory, wake-ups, summon to typeable).
4. Move paste approvals, the Draft and delivery records into files the
   engine owns, and the editor endpoint out of the Tauri app (amended
   2026-11-28).
5. Build the Windows app in WinUI 3, then the Linux app in GTK 4 with
   libadwaita. Each system drops the Tauri app at parity.
5. Add system extensions as thin clients.
