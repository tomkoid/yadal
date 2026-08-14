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
              "tidlers-0.5.0" = "sha256-yH0hMT3kGIhCe0DkLuME5yZCy7pTsfS9qcJ9YWijHUo=";
              "multitag-0.4.3" = "sha256-FGnUDhs8vGig+S9403LViNauRO9PK7gaO57Df9EZMaY=";
            };
          };

          nativeBuildInputs = [ pkgs.makeBinaryWrapper ];

          postInstall = ''
            wrapProgram $out/bin/yadal \
              --prefix PATH : ${lib.makeBinPath [ pkgs.ffmpeg ]}
          '';

          __structuredAttrs = true;

          meta = {
            description = "Yet another TIDAL Hi-Res audio downloader for the CLI";
            homepage = "https://codeberg.org/tomkoid/yadal";
            license = lib.licenses.gpl3Only;
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
