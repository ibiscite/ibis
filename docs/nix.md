# Install with Nix

Enable Nix's `nix-command` and `flakes` features. Packages build from source;
the first build can take a while.

| Package | Program | Systems |
| --- | --- | --- |
| `ibis` (default) | `ibis` command line | `x86_64-linux`, `aarch64-linux`, `x86_64-darwin`, `aarch64-darwin` |
| `ibis-desktop` | `ibis-desktop` app | `x86_64-linux`, `aarch64-linux` |

Run the CLI or add it to your profile:

```sh
nix run github:ibiscite/ibis -- --help
nix profile add github:ibiscite/ibis
```

The default package is the CLI on every supported system. Explicitly selecting
`#ibis` gives the same package, including Bash, Zsh and Fish completions. It
works without the desktop app; see the [CLI reference](cli.md).

On Linux, run or install the desktop separately:

```sh
nix run github:ibiscite/ibis#ibis-desktop
nix profile add github:ibiscite/ibis#ibis-desktop
```

After a profile install, the app appears in your application menu when your
profile's `share` directory is in the session's `XDG_DATA_DIRS`. The wrapper
sets up GTK and the tray library. Existing [Linux shortcut and tray
limits](compatibility.md#linux-shortcuts-and-tray) still apply; a successful
build does not qualify a desktop session or graphics driver.

## Which version is this?

`github:ibiscite/ibis` selects current `main`, including unreleased changes.
It does not follow the Stable or Nightly binary download feed. `nix profile
list` shows the locked application commit; several development commits can
declare the same Ibis version.

To choose a numbered release, replace the URL with
`github:ibiscite/ibis/TAG`, adding `#ibis-desktop` for the app. The tag must
contain this flake: older tags, including those made while Nix support was
removed, cannot provide it. A full commit ID also selects an exact revision:
`github:ibiscite/ibis/COMMIT#ibis`.

Nixpkgs is pinned directly in `flake.nix` to the nixos-26.05 commit
`ac588ef1c914865dad517b337ea0eb9694a4021c`. The restored, outdated
`flake.lock` is omitted: CI runs `nix flake lock` and saves the generated lock
as an artifact. Resolving the input always selects that exact nixpkgs commit,
so it cannot drift with the channel. A consumer's configuration lock pins
Ibis's source commit as well. No `narHash` is guessed or copied from another
revision.

For a profile installation, use `nix profile list` to find its name, then
`nix profile upgrade NAME`. The desktop app's updates come from Nix; this
build strips the upstream updater key and feed. Settings and libraries use
their normal locations.

## NixOS and Home Manager

Add `inputs.ibis.url = "github:ibiscite/ibis";` to your configuration flake.
Pass `inputs` to NixOS through `specialArgs`, or to Home Manager through
`extraSpecialArgs`. In a NixOS module:

```nix
{ inputs, pkgs, lib, ... }:
let ibis = inputs.ibis.packages.${pkgs.stdenv.hostPlatform.system};
in {
  environment.systemPackages = [ ibis.ibis ]
    ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ ibis.ibis-desktop ];
}
```

For Home Manager, use the same expression in `home.packages` instead of
`environment.systemPackages`. Omit the desktop package if you only want the
CLI. Update with `nix flake update ibis`, then rebuild your configuration.

Ibis's **Start Ibis when you log in** records its current store wrapper,
including its GTK and tray environment. To follow configuration rebuilds,
Home Manager can own the Linux autostart entry instead:

```nix
{ inputs, pkgs, lib, ... }:
let desktop = inputs.ibis.packages.${pkgs.stdenv.hostPlatform.system}.ibis-desktop;
in {
  home.packages = [ desktop ];
  xdg.configFile."autostart/com.ibiscite.ibis.desktop".text = ''
    [Desktop Entry]
    Type=Application
    Name=Ibis
    Exec=${lib.getExe desktop} --at-login
    Terminal=false
  '';
}
```

Use one autostart method. When Home Manager owns the entry, manage it in your
configuration instead of toggling it in Ibis.

## Build and CI

From a checkout:

```sh
nix flake lock
nix build .#ibis
./result/bin/ibis --version
nix build .#ibis-desktop  # Linux only
nix flake check --print-build-logs
```

Both packages use `rustPlatform.buildRustPackage` with `Cargo.lock` directly;
registry dependencies need no separately maintained Cargo hash. If a git
dependency is introduced, add only its required `cargoLock.outputHashes`.
The desktop expression in [nix/package.nix](../nix/package.nix) reads the pnpm
version from `apps/desktop/package.json`'s `packageManager`, uses Node 24,
and fetches dependencies from `apps/desktop/pnpm-lock.yaml` before building
offline. Its pnpm tarball integrity must match that version too.

The separate [Nix workflow](../.github/workflows/nix.yml) builds and tests the
CLI and runs `ibis --version` on `ubuntu-latest` and `macos-latest`. On Linux
it also builds the desktop, runs its Rust tests and checks the installed
desktop entry, launcher and icon. The build verifies the embedded store
launcher and rejects the upstream updater key/feed. This is a package check,
not a graphical runtime test. The other two system architectures are
exposed by the flake but are not built by this runner matrix.

The workflow runs for packaging, Cargo lock/manifest and desktop dependency
changes, weekly and on manual dispatch. It has read-only permissions and
reports independently of the required **CI result**. Generated locks and the
desktop build log are saved as artifacts.

## Agents: keep the pnpm hash with the lock

When `apps/desktop/pnpm-lock.yaml` changes, the Nix desktop job may fail with
a fixed-output hash mismatch. Its log prints `specified:` and `got:` for
`ibis-desktop-pnpm-deps`, followed by the exact repair command. Copy the
reported `got:` value:

```sh
node scripts/nix-pnpm-hash.mjs 'sha256-REPLACE_WITH_THE_REPORTED_HASH='
```

The script validates a complete SHA-256 SRI hash before rewriting
`nix/pnpm-deps-hash`. Update that file in the **same pull request** as the
pnpm lock change, then let the Nix job rebuild. No local Nix installation is
needed to apply the fix. To extract the command from a saved build log:

```sh
node scripts/nix-pnpm-hash.mjs --from-log nix-build.log
```

Log inspection only prints the command; it does not change the hash. For
other failures, reproduce the build command printed by CI and fix its first
error. A pnpm tarball or Cargo git-dependency mismatch is a different input;
do not put its hash in `nix/pnpm-deps-hash`.
