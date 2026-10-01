{
  description = "grpy - discover local concerts from a TUI and add them to Google Calendar";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };
        grpy = rustPlatform.buildRustPackage {
          pname = "grpy";
          version = "0.1.0";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
        };
      in
      {
        packages.default = grpy;

        apps.default = flake-utils.lib.mkApp { drv = grpy; };

        checks.default = grpy;

        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            pkgs.cargo-nextest
            pkgs.cargo-watch
            pkgs.pkg-config
          ];
          RUST_BACKTRACE = "1";
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}
