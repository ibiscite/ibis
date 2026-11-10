# 0037 One source per fact; everything else is generated, checked or a command

Maintainer, 2026-11-04: Ibis is built mostly by agents for one maintainer, so the
cost that matters is what each change must read, retype and remember. A fact
lives in one place, in a form a program can read; everything that repeats it is
generated from it or checked against it; and every routine step is a command.
Before 1.0, anything that fights this is restructured, with no compatibility kept.

- **Contract outward.** The engine schema generates the app's types, the CLI
  reference, MCP's tool schemas and the golden cases (0035, #1260, #1687).
- **Specs as data.** A table in a design or architecture document that code must
  match (notices, keys, footer states, status words, budgets) becomes a small data
  file the code imports and the document renders or a test compares, as the
  Notices table already is. People change one row, not prose.
- **One catalogue** of words for the command line, MCP and the app (#1675).
- **One seam per platform concern** with a capability report (#1676), so each
  system is tuned in one place.
- **Pictures are the design baseline.** The canvas renders every state; a pull
  request that changes how something looks shows the pictures that changed, and
  judges look only at those.
- **Budgets as data:** one table of limits, each with its check; deterministic
  counts where possible, timings only reported on shared runners.
- **Routine steps as commands:** `scripts/cloud-setup.sh` sets up a session and
  `scripts/land.sh` lands a branch (refuse a conflict with main, check what changed, push, auto-merge).
- **Small documents:** a session reads the one file its task needs (#1690).
