{
  description = "Ibis desktop app";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/02c087f3fe673c9e317f206a627cb6b910336dbe";

  outputs = { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      ibis = pkgs.callPackage ./nix/package.nix { };
    in {
      packages.${system} = { inherit ibis; default = ibis; };
      checks.${system}.package = pkgs.callPackage ./nix/checks.nix { inherit ibis; };
    };
}
