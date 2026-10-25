# 0025. Structured citation input for the engine and CLI

Status: accepted (2026-10-24)

## Context

The engine already supports per-reference locators, prefixes, suffixes and author
omission, but the CLI exposes only pages. Scripts and future front ends need the
same complete citation request, including repeated keys with different locators.
The implementation scope and input contract are recorded in #24 and tracking #1059.

## Decision

- `ibis cite --items` reads one UTF-8 JSON array from stdin, to EOF. It conflicts
  with positional keys and `--page`; existing invocations retain their behavior.
- Each element uses the engine's `CiteItem`: required `key`; optional `locator`
  (`label`, `value`), `prefix`, `suffix` and `suppress_author` (default false).
  Labels are `page`, `chapter`, `section`, `paragraph`, `figure`, `line`, `note`,
  `volume` and `verse`. Order and repeated keys are preserved.
- Unknown or duplicate fields and malformed shapes are rejected. The shared
  reader accepts at most 1 MiB and 1,000 items, with bounded reading even when
  stdin never ends. It performs no file, clipboard, settings or network I/O.
- The existing formatter validates values and escapes each target syntax. An
  empty or whitespace-only locator is an error; unsupported intent produces the
  existing warnings. No custom rendering is added to the CLI.
- `--bib` still checks every requested key. `--format` and `--json` retain their
  existing meaning and output schema. Invalid input produces no partial output;
  request errors exit 1 and argument conflicts use Clap's argument-error status.

## Consequences

One bounded, typed input serves the CLI now and other front ends later. It does
not define persisted state, a daemon, an editor endpoint or CSL rendering in the
CLI. Picker integration remains tracked in #24/#354. No dependency is added.
