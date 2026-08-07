{
  description = "Yadal flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        lib = pkgs.lib;
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage (finalAttrs: {
          name = "yadal";

          src = lib.cleanSource ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "tidlers-0.5.0" = "sha256-JbpK4D7m6Egjux5fLhgO9hDNjMVmDc91QL9CwlWqUGc=";
              "multitag-0.4.3" = "sha256-FGnUDhs8vGig+S9403LViNauRO9PK7gaO57Df9EZMaY=";
            };
          };

          buildInputs = with pkgs; [ ffmpeg ];

          meta = {
            description = "Yet another TIDAL track, playlist, album CLI downloader";
            homepage = "https://codeberg.org/tomkoid/yadal";
            license = lib.licenses.gpl3;
            changelog = "https://codeberg.org/tomkoid/yadal/releases";
            maintainers = with lib.maintainers; [ tomkoid ];
            mainProgram = "yadal";
          };
        });

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            ffmpeg
          ];
        };
      }
    );
}
