# 0048 Nix installation

Nix provides reproducible source packages maintained in this repository.

- `ibis`, the CLI, is the default on x86_64 and aarch64 Linux and macOS.
  `buildRustPackage` reads `Cargo.lock`; completions come from the built CLI.
- `ibis-desktop` supports both Linux architectures. Nix owns updates: packages
  omit the upstream feed/key and the store wrapper supplies the tray library.
- The flake pins nixpkgs directly to a nixos-26.05 commit. CI generates and
  preserves the matching lock.
- A read-only workflow builds the CLI on Linux/macOS and checks desktop
  packages on Linux, on relevant main pushes, weekly and manually. It is
  independent of [the local merge gate](0049-local-gate.md).
- `nix/pnpm-deps-hash` tracks desktop dependencies. A failed job prints the new
  hash and update command; lock and hash change together.

Commands: [Nix installation](../nix.md).
