{
  lib,
  stdenv,
  rustPlatform,
  cargo-tauri,
  fetchPnpmDeps,
  pnpm_11,
  pnpmConfigHook,
  nodejs_22,
  pkg-config,
  jq,
  cacert,
  wrapGAppsHook3,
  webkitgtk_4_1,
  glib-networking,
  librsvg,
  libayatana-appindicator,
  pnpmDepsHash ? lib.fileContents ./pnpm-deps-hash,
}:
let
  # Match packageManager exactly; Nixpkgs supplies the packaging machinery.
  pnpm = pnpm_11.override {
    version = "11.25.0";
    hash = "sha512-XN6SW08HX3Jetx+64YpC/+eEUkeJ8ZthxzHLhyHsKKruFg4BqNWvT+2ypCzb8wDv4j2zVrDUoXtNY+EfirfJVg==";
    nodejs-slim = nodejs_22;
  };
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "ibis-desktop";
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).workspace.package.version;

  src = lib.cleanSourceWith {
    src = ../.;
    filter = path: type:
      lib.cleanSourceFilter path type
      && !(builtins.elem (baseNameOf path) [ ".local" ".claude" ".env" "target" "node_modules" "dist" "gen" ]);
  };
  cargoLock.lockFile = ../Cargo.lock;
  pnpmDeps = fetchPnpmDeps {
    inherit (finalAttrs) pname version src;
    inherit pnpm;
    sourceRoot = "${finalAttrs.src.name}/apps/desktop";
    fetcherVersion = 4;
    hash = pnpmDepsHash;
  };
  pnpmRoot = "apps/desktop";
  buildAndTestSubdir = "apps/desktop";
  cargoBuildFlags = [ "--locked" ];
  cargoTestFlags = [ "--package" "ibis-desktop" ];
  tauriBuildFlags = [ "--config" (builtins.toJSON (builtins.fromJSON (builtins.readFile ./tauri.conf.json))) ];

  nativeBuildInputs = [ cargo-tauri.hook nodejs_22 pnpm pnpmConfigHook pkg-config jq wrapGAppsHook3 ];
  buildInputs = [ webkitgtk_4_1 glib-networking librsvg libayatana-appindicator ];
  env.IBIS_PACKAGE_LAUNCHER = "${builtins.placeholder "out"}/bin/ibis-desktop";
  # The loopback updater tests still construct a TLS client, which eagerly loads
  # CA roots. The sandbox has no host trust store; use the pinned PEM test bundle.
  preCheck = ''
    export SSL_CERT_FILE="${cacert}/etc/ssl/certs/ca-no-trust-rules-bundle.crt"
  '';
  postBuild = ''
    binary=target/${stdenv.hostPlatform.rust.cargoShortTarget}/release/ibis-desktop
    grep -aFq "$IBIS_PACKAGE_LAUNCHER" "$binary"
    # Verify the compiled app, not only the JSON override passed to Tauri.
    for value in $(jq -r '.plugins.updater | .pubkey, .endpoints[]' apps/desktop/src-tauri/tauri.conf.json); do
      if grep -aFq "$value" "$binary"; then
        echo "Nix build still contains the upstream update key or feed" >&2
        exit 1
      fi
    done
  '';
  postInstall = ''
    substituteInPlace "$out"/share/applications/*.desktop \
      --replace-fail 'Exec=ibis-desktop' "Exec=$out/bin/ibis-desktop"
  '';
  # The tray library is loaded by name, rather than through ELF NEEDED entries.
  preFixup = ''
    gappsWrapperArgs+=(--prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath [ libayatana-appindicator ]}")
  '';

  meta = {
    description = "Find references and copy citations from any app";
    homepage = "https://github.com/ibiscite/ibis";
    license = lib.licenses.agpl3Plus;
    mainProgram = "ibis-desktop";
    platforms = [ "x86_64-linux" ];
  };
})
