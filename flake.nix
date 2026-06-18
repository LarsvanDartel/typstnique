{
  description = "Typstnique — a Typst typesetting speed game (full-stack Rust)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        # Stable Rust for building (wasm target for the Leptos frontend).
        # rustfmt is supplied separately (nightly) below so the strict
        # `rustfmt.toml`, which uses unstable options, actually applies.
        rustToolchain = pkgs.rust-bin.stable.latest.minimal.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
          ];
          targets = [ "wasm32-unknown-unknown" ];
        };

        # Nightly rustfmt + cargo-fmt so `cargo fmt` honours the unstable options
        # in rustfmt.toml. Picks the latest nightly that actually ships rustfmt.
        rustfmtNightly = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.rustfmt);
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            rustfmtNightly # nightly `rustfmt`/`cargo fmt` (strict, unstable opts)

            # Build driver for the dual (wasm + server) Leptos build.
            pkgs.cargo-leptos

            # wasm toolchain. NOTE: the `wasm-bindgen` crate version in
            # Cargo.toml must match `wasm-bindgen --version` here, otherwise
            # cargo-leptos errors. Run `wasm-bindgen --version` and align the
            # pin in the root Cargo.toml if they diverge.
            pkgs.wasm-bindgen-cli
            pkgs.binaryen # provides wasm-opt
            pkgs.dart-sass # compiles style/main.scss

            # Leaderboard database tooling.
            pkgs.sqlx-cli
            pkgs.sqlite

            # Misc build deps.
            pkgs.pkg-config
            pkgs.openssl
          ];

          shellHook = ''
            export DATABASE_URL="sqlite:typstnique.db"
            echo "── typstnique dev shell ──────────────────────────────"
            echo "  cargo leptos watch      # run dev server :3000"
            echo "  cargo test -p typst-engine"
            echo "  cargo leptos build --release"
            echo "──────────────────────────────────────────────────────"
          '';
        };
      }
    );
}
