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
        pkgs = import nixpkgs {
          inherit system;
        };
        lib = pkgs.lib;
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage (finalAttrs: {
          name = "yadal";

          src = lib.cleanSource ./.;

          cargoHash = "sha256-XV5crj3ngXtWcsN4IFUoh3qER2iFAdXjiJjg2Z5ogbE=";

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
