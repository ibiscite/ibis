# 0032 Release gates

A numbered release needs the complete search → choose → page → paste/copy
journey with VoiceOver, NVDA and Chinese, Japanese and Korean input methods.
Orca joins for 1.0. Translation uses the shared catalogue; German and one
right-to-left language are planned for 0.4.

Limits apply at 10,000 realistic generated references, including abstracts,
long author lists and mixed scripts:

| Outcome | Limit |
| --- | --- |
| Shortcut to typeable picker | p95 ≤ 100 ms |
| Keystroke to results | p95 ≤ 50 ms; engine search ≤ 20 ms |
| Warm formatted preview | p95 ≤ 100 ms |
| First preview / first results after launch | ≤ 1 s |
| Citable after a source edit | About 2 s, without interrupting search |
| Hidden whole-process memory, webview included | ≤ 200 MiB |
| Hidden idle | ≤ 0.2% of one core and ≤ 1 wakeup/s |
| Picker JavaScript and CSS | ≤ 400 KiB gzipped |

Release machines enforce timing and resource limits; shared runners report
measurements with insufficient margin. A 50,000-record nightly check reports
against the same limits as a nonblocking stretch goal. Current checks are in
[architecture](../architecture.md#performance-checks).

Each finished milestone becomes a numbered release with the maintainer's
approval. Distributions use that release; Nightly carries daily testing.
1.0 adds no features: people qualify the planned work, then the maintainer
promotes it. macOS uses a stable self-signed identity and Windows installers
are unsigned. Paid signing remains a 1.0 decision.
