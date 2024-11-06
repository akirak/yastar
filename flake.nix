{
  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    systems.url = "github:nix-systems/default";

    treefmt-nix.url = "github:numtide/treefmt-nix";

    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
  };

  nixConfig = {
    extra-substituters = [
      "https://akirak.cachix.org"
    ];
    extra-trusted-public-keys = [
      "akirak.cachix.org-1:WJrEMdV1dYyALkOdp/kAECVZ6nAODY5URN05ITFHC+M="
    ];
  };

  outputs =
    inputs@{ nixpkgs, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;

      imports = [
        inputs.treefmt-nix.flakeModule
      ];

      perSystem =
        {
          config,
          system,
          pkgs,
          lib,
          craneLib,
          commonArgs,
          ...
        }:
        {
          _module.args = {
            pkgs = import nixpkgs {
              inherit system;
              overlays = [ inputs.rust-overlay.overlays.default ];
            };
            craneLib = (inputs.crane.mkLib pkgs).overrideToolchain (pkgs: pkgs.rust-bin.stable.latest.default);
            commonArgs = {
              src = lib.cleanSourceWith {
                src = ./.;
                name = "source";
                filter =
                  let
                    isExtraSource = path: _: builtins.match ".+\.(graphql|json)" path != null;
                  in
                  path: type: (isExtraSource path type) || (craneLib.filterCargoSources path type);
              };

              nativeBuildInputs = with pkgs; [
                pkg-config
              ];

              buildInputs = with pkgs; [
                openssl
                duckdb
              ];
              # Add these if you use bitmap backend
              # ++ lib.optionals stdenv.isLinux [
              #   freetype
              #   fontconfig
              # ]
            };
          };

          packages.default = craneLib.buildPackage (
            commonArgs
            // {
              cargoArtifacts = craneLib.buildDepsOnly commonArgs;
            }
          );

          # A self-contained updater for CI where just is not available by
          # default.
          packages.update-data = pkgs.writeShellApplication {
            name = "update-data";
            runtimeInputs = [
              pkgs.just
            ];
            text = ''
              just -f ${./justfile} update
            '';
          };

          devShells.default = craneLib.devShell {
            packages = commonArgs.nativeBuildInputs ++ commonArgs.buildInputs;
          };

          treefmt = {
            projectRootFile = "Cargo.toml";
            programs = {
              actionlint.enable = true;
              nixfmt.enable = true;
              rustfmt.enable = true;
            };
          };
        };
    };
}
