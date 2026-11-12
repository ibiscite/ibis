# 0045 Nix install

2026-11-11 (maintainer). Nix installation returns after its removal in #1206
on 2026-10-26: source packages give Nix users a reproducible install while
keeping maintenance in the repository and its agents.

- `ibis`, the CLI, is the default on x86_64 and aarch64 Linux and macOS.
  `buildRustPackage` reads `Cargo.lock`; registry dependencies need no Cargo
  hash to maintain. Shell completions come from the built CLI.
- `ibis-desktop` is available on both Linux architectures. Nix owns updates:
  the upstream updater key/feed are removed, the launcher uses its store
  wrapper, and the wrapper supplies the tray library.
- Nixpkgs is pinned to the specified nixos-26.05 commit directly in the flake.
  The obsolete lock is omitted; CI generates and preserves a matching lock.
- A separate read-only workflow builds the CLI on Linux and macOS and the
  desktop with package checks on Linux, on relevant pull requests, weekly
  and by hand. It is not part of the required **CI result**.
- Desktop dependencies keep their hash in `nix/pnpm-deps-hash`. A failed Nix
  job prints the new hash and the exact `scripts/nix-pnpm-hash.mjs` command;
  agents update it in the same pull request as the pnpm lock.

Commands and maintenance steps: [Nix installation](../nix.md).
