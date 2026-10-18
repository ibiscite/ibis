# 0014. Share one lockfile with fuzz infrastructure

Status: accepted (2026-10-18; amends the workspace layout in ADR 0013)

## Context

The isolated fuzz workspace introduced in #66 duplicated the dependency lock for
`ibis-core`. After #3 added file notifications, its stale lock rejected locked
metadata, while an unlocked cargo-deny check silently updated that lock during
read-only verification. cargo-fuzz 0.13.1 supports fuzz targets as
members of an existing workspace.

## Decision

Make `fuzz` a root workspace member and use only the root `Cargo.lock`. Keep it
outside `default-members`, so normal core/CLI checks and application builds do
not compile fuzz targets or link libFuzzer. Only explicit cargo-fuzz commands
use nightly instrumentation; the application toolchain remains pinned.

Run one root dependency-policy check with `--locked`, locally and in CI. The
approved NCSA exception remains limited to `libfuzzer-sys` version `0.4.13`,
which the fuzz manifest pins exactly. This amends only ADR 0013's separate
workspace requirement; its licensing decision and upstream notice obligations
remain unchanged.

## Consequences

Core dependency changes update one dependency lock. Root workspace metadata and
formatting include the fuzz member. Default tests and clippy still select only
the core and CLI, while explicit desktop checks stay separate. Fuzz-only packages
appear in the shared lock and policy graph, but are not application dependencies
or part of distributed application builds.
