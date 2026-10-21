# Install with Nix

The repository flake targets the desktop app on `x86_64-linux`. macOS, Windows
and other Linux architectures are not offered. Enable Nix's `nix-command` and
`flakes` features before using these commands. Building can take several minutes.

```sh
nix build github:ibiscite/ibis#ibis
nix run github:ibiscite/ibis#ibis
nix profile add github:ibiscite/ibis#ibis
```

The installed program is `ibis-desktop`; the developer CLI `ibis` is not included.
After a profile install, Ibis also appears in your desktop's application menu
when its Nix profile is included in the session's `XDG_DATA_DIRS`.

From a checkout, use `nix build .#ibis` and `nix run .#ibis`. To reproduce an
application revision, replace `github:ibiscite/ibis` with
`github:ibiscite/ibis/COMMIT`, using the full commit ID.

## Which version is this?

The default command selects current `main`, which can contain unreleased work.
It does not select the Stable or Nightly download feed. `nix profile list` shows
the **Locked flake URL**, including the exact installed commit. Ibis shows the
version declared by that source; several development commits can share it.

To choose a numbered release, use `github:ibiscite/ibis/TAG#ibis` once that release
tag contains this flake. Earlier release tags cannot provide a package added
after they were created. A full commit ID works for any revision containing the
flake and stays pinned until you choose another revision.

## NixOS and Home Manager

Add `inputs.ibis.url = "github:ibiscite/ibis";` to your configuration flake. Its
lock file pins the application; Ibis's lock pins its own build tools and libraries.
Pass `inputs` to NixOS through `specialArgs`, or to Home Manager through
`extraSpecialArgs`. Then use this in a NixOS module:

```nix
{ inputs, pkgs, ... }:
{
  environment.systemPackages = [
    inputs.ibis.packages.${pkgs.stdenv.hostPlatform.system}.ibis
  ];
}
```

For Home Manager, use the same package in `home.packages` instead of
`environment.systemPackages`. Update with `nix flake update ibis`, then rebuild
your NixOS or Home Manager configuration. For a profile installation, use
`nix profile list` to find its name, then `nix profile upgrade NAME`.

Nix manages upgrades. This build has no in-app update question or installer:
its package-only Tauri config removes the update key and feed. Existing settings
and libraries remain in their normal locations.

Ibis's **Start Ibis when you log in** uses the public Nix wrapper, preserving
the GTK and tray-library environment. It records that store generation. For an
autostart entry that follows configuration rebuilds, Home Manager can own it:

```nix
{ inputs, pkgs, lib, ... }:
let ibis = inputs.ibis.packages.${pkgs.stdenv.hostPlatform.system}.ibis;
in {
  home.packages = [ ibis ];
  xdg.configFile."autostart/io.github.ibiscite.ibis.desktop".text = ''
    [Desktop Entry]
    Type=Application
    Name=Ibis
    Exec=${lib.getExe ibis} --at-login
    Terminal=false
  '';
}
```

Use one autostart method. When Home Manager owns the entry, manage it in your
configuration instead of toggling it in Ibis. Existing Linux shortcut and tray
limitations still apply; see [compatibility.md](compatibility.md).

## Build and validation

The reusable expression is `nix/package.nix`. It uses the repository's Cargo
and pnpm locks and pinned Nixpkgs
[Tauri](https://github.com/NixOS/nixpkgs/blob/02c087f3fe673c9e317f206a627cb6b910336dbe/doc/hooks/tauri.section.md)
and [pnpm hooks](https://github.com/NixOS/nixpkgs/blob/02c087f3fe673c9e317f206a627cb6b910336dbe/pkgs/build-support/node/fetch-pnpm-deps/default.nix).
pnpm 11.25.0 matches `packageManager`; Rust 1.99.0 satisfies the workspace's 1.90
minimum. Nix invokes its own compiler directly. Dependencies are fetched with
fixed hashes, then Cargo and pnpm work offline inside the build sandbox.

`nix flake check --no-build` evaluates the package. `nix build .#ibis` builds and
runs its desktop Rust tests. `nix flake check` also validates the installed
launcher, desktop entry and referenced icon. The build checks the executable's
embedded launcher and rejects the original updater key/feed. CI separately runs
the existing native smoke test against the installed wrapper with synthetic
data under X11; this does not establish Wayland or hardware-GPU behavior.

The [CI workflow](../.github/workflows/ci.yml) preserves the exact source commit,
derivation, output path, build log and graphical results in its `nix-package-*`
artifact. The job runs on pull requests that change `flake.nix`, `flake.lock`,
`nix/`, `Cargo.lock` or the desktop `package.json`/`pnpm-lock.yaml`, weekly and on
manual runs, so a commit on `main` may have no run of its own; evaluation alone
does not establish a successful build or runtime check. The smoke test checks
behaviour only: its performance budgets are calibrated for debug builds, so the
release package is not held to them.
