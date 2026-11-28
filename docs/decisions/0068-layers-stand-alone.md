# 0068 Each layer stands on its own

**Decided** 2026-11-28 with the maintainer's direction that every engine layer
be usable on its own, after Sol and Astra reviewed the split
([#2075](https://github.com/ibiscite/ibis/pull/2075)). It extends
[0054](0054-owners-and-boundaries.md),
[0056](0056-open-layer-for-research-material.md) and
[0065](0065-one-warm-engine-thin-shells.md).

## The decision

1. **One bottom crate for the system calls, so core stays pure.** `ibis-os`
   sits beside `ibis-core`, uses no other Ibis crate, and holds the operating
   system calls that several layers share: whole-file private writes
   (`atomic_file`), bounded filesystem work (`file_io`), contained parser
   panics (`panic_guard`), and for now the `ibis` command's install and the
   toggle receipt. `ibis-core` does no I/O at all; the `core_does_no_io` test
   enforces it with no list of exceptions.
2. **The host owns the transports; the engine owns the methods.** JSON-RPC
   framing, the server and the MCP runtime move from `ibis-engine` into the
   host (`ibis-cli`). The method table and the result budgets stay in the
   engine, so every transport answers the same calls the same way. The agent
   apps setup (`agent_apps`) moves with the MCP face.
3. **The contract is the only promise.** The Rust crates stay unpublished
   (`publish = false`) with no API promise before 1.0. The generated contract
   JSON is the one versioned surface for shells, agents and plugins.

## Consequences

- A new system call goes in `ibis-os` (or the layer that alone needs it),
  never in `ibis-core`.
- A new transport (for example the picker over standard input and output) is
  host code over the engine's method table, not a second dispatcher.
- People building on Ibis use the CLI, MCP or the contract, never the crates.
