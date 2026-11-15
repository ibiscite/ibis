{ lib }:

lib.cleanSourceWith {
  src = ../.;
  filter = path: type:
    lib.cleanSourceFilter path type
    && !(builtins.elem (baseNameOf path) [
      ".local" ".claude" ".codex" ".agents" ".env" "target"
      "node_modules" "dist" "dist-canvas" "gen"
    ]);
}
