{
  description = "A Rust-based plugin manager for fish";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-darwin"
        "x86_64-linux"
      ];

      forAllSystems = nixpkgs.lib.genAttrs systems;
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      licenseBySpdx = {
        MIT = nixpkgs.lib.licenses.mit;
      };
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = cargoToml.package.name;
            version = cargoToml.package.version;

            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = with pkgs; [
              cmake
              perl
              pkg-config
            ];

            buildInputs =
              with pkgs;
              [
                zlib
              ]
              ++ lib.optionals stdenv.hostPlatform.isDarwin [
                libiconv
              ];

            meta = with pkgs.lib; {
              description = cargoToml.package.description;
              homepage = cargoToml.package.homepage;
              license =
                licenseBySpdx.${cargoToml.package.license}
                  or (throw "Unsupported Cargo.toml license: ${cargoToml.package.license}");
              mainProgram = cargoToml.package.name;
              platforms = platforms.unix;
            };
          };
        }
      );

      apps = forAllSystems (
        system:
        let
          package = self.packages.${system}.default;
        in
        {
          default = {
            type = "app";
            program = "${package}/bin/${package.meta.mainProgram}";
            meta = package.meta;
          };
        }
      );

      checks = forAllSystems (system: {
        default = self.packages.${system}.default;
      });

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.default ];
            packages = with pkgs; [
              cargo
              clippy
              fish
              rust-analyzer
              rustc
              rustfmt
            ];
          };
        }
      );
    };
}
