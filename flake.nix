{
  description = "A flake for Pyde";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    treefmt.url = "github:numtide/treefmt-nix";
    systems.url = "github:nix-systems/default";
  };
  outputs =
    {
      self,
      nixpkgs,
      treefmt,
      systems,
    }:
    let
      forAllSystems =
        f: nixpkgs.lib.genAttrs (import systems) (system: f nixpkgs.legacyPackages.${system});
    in
    {
      formatter = forAllSystems (
        pkgs:
        (treefmt.lib.evalModule pkgs {
          projectRootFile = "flake.nix";
          programs.nixfmt.enable = true;
          programs.leptosfmt.enable = true;
          programs.rustfmt.enable = true;
          # Both format `*.rs`: the `view!` blocks first, then the rest.
          settings.formatter.leptosfmt.priority = 1;
          settings.formatter.rustfmt.priority = 2;
          programs.taplo.enable = true;
          programs.prettier.enable = true;
        }).config.build.wrapper
      );
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rust-analyzer
            (trunk.overrideAttrs (oldAttrs: {
              nativeBuildInputs = (oldAttrs.nativeBuildInputs) ++ [ pkgs.pkg-config ];
              buildInputs = (oldAttrs.buildInputs) ++ [ pkgs.zstd ];
            }))
            lld
          ];
        };
      });
    };
}
