# 0013. Permit NCSA for the isolated fuzzing dependency

Status: accepted (2026-10-17)

## Context

Nightly BibTeX fuzzing (#66) uses cargo-fuzz and libFuzzer. `libfuzzer-sys`
0.4.13 declares `(MIT OR Apache-2.0) AND NCSA`; NCSA is not in the repository's
general dependency allowlist. Licensing decisions require maintainer approval.

## Decision

Permit NCSA only for `libfuzzer-sys` version `0.4.13` through a crate-specific
cargo-deny exception. Keep the fuzz targets in a separate workspace and check
its dependency graph in local verification and CI. The maintainer approved
this scoped exception; it does not extend the general license allowlist.

## Consequences

Ibis remains AGPL-3.0-or-later. This dependency is test infrastructure and is not
part of the application workspace or distributed application builds. A version
upgrade requires reviewing the exception again. Keep upstream license notices.
