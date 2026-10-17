# 0003. One engine, thin front ends

Status: accepted (2026-10-17)

## Context
Inspired by FFmpeg/libVLC: one strong engine used by many tools. Ibis needs a picker window, a CLI, an AI (MCP) interface, an editor endpoint, and later a browser extension and web build.

## Decision
`crates/ibis-core` contains sources, normalization, search, health rules and key-based output, with no GUI or network dependency. Front ends (`crates/ibis-cli`, `apps/desktop`, future MCP server) only translate between users and the core.

## Consequences
Most behaviour is tested quickly without a window. The core can later compile to WebAssembly for a web version. Logic duplicated in two front ends is a bug: move it to the core.
