{
  lib,
  stdenv,
  rustPlatform,
  cargo-tauri,
  fetchPnpmDeps,
  pnpm_11,
  pnpmConfigHook,
  nodejs_24,
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
  # Nixpkgs' pnpm 11 reads the lockfile apps/desktop/package.json pins.
  pnpm = pnpm_11;
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "ibis-desktop";
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).workspace.package.version;

  src = import ./source.nix { inherit lib; };
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

  nativeBuildInputs = [ cargo-tauri.hook nodejs_24 pnpm pnpmConfigHook pkg-config jq wrapGAppsHook3 ];
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
    platforms = [ "x86_64-linux" "aarch64-linux" ];
  };
})
