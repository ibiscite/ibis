# 0039 Extending Ibis from outside

Recorded 2026-11-05 on the maintainer's yes.

- **Licence boundary.** The engine crates stay AGPL-3.0-or-later, also when
  another program links them as a library. The published contract (what `ibis
  schema` describes and types generated from it) and thin clients that only talk
  to `ibis`, `ibis serve` or `ibis mcp` are MIT, each file marked with its SPDX
  identifier when it is first published on its own, so integrations carry no AGPL
  obligation (decision 0002).
- **Package names.** The `@ibiscite` organization is reserved on npm. Names on
  crates.io and PyPI are claimed with the first real package published there,
  never with an empty placeholder.
- **Command plugins.** When `foo` is not a built-in command, `ibis foo` runs a
  program named `ibis-foo` found on `PATH` with the remaining arguments, as Git and
  Cargo do. Built-ins always win; Ibis adds no plugin folder, download or registry.
- **The running app answers `ibis`.** While the app runs, `ibis` connects to it
  over a socket (a named pipe on Windows) that only the same user can open, and
  uses its loaded libraries and settings; without the app it reads them itself,
  with the same results (decision 0028). It never listens on a network port. Its
  first version needs the security review in AGENTS.md › Merging.
