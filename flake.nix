{
  description = "Typstnique — a Typst typesetting speed game (full-stack Rust)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      git-hooks,
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

        # Pre-commit checks, managed by Nix. Run automatically on `git commit`
        # (the dev shell installs the hook), or all at once with
        # `pre-commit run --all-files` / `nix flake check`.
        preCommitCheck = git-hooks.lib.${system}.run {
          src = ./.;
          hooks = {
            # Rust formatting with the strict (nightly) rustfmt above.
            rustfmt = {
              enable = true;
              packageOverrides = {
                cargo = rustToolchain;
                rustfmt = rustfmtNightly;
              };
            };
            # Nix formatting (RFC style). Pin the package to `pkgs.nixfmt`
            # directly: `pkgs.nixfmt-rfc-style` is now a deprecated alias for it
            # and pulling it in emits an evaluation warning.
            nixfmt-rfc-style = {
              enable = true;
              package = pkgs.nixfmt;
            };
            # Enforce the .editorconfig (skip lock files and the vendored JS).
            editorconfig-checker = {
              enable = true;
              excludes = [
                "\\.lock$"
                "texnique_problems\\.js$"
              ];
            };
            # Generic hygiene.
            trim-trailing-whitespace.enable = true;
            end-of-file-fixer.enable = true;
            check-merge-conflicts.enable = true;
            check-added-large-files.enable = true;
          };
        };
      in
      {
        checks.pre-commit-check = preCommitCheck;

        devShells.default = pkgs.mkShell {
          buildInputs = preCommitCheck.enabledPackages;
          shellHook = ''
            ${preCommitCheck.shellHook}
            export DATABASE_URL="sqlite:typstnique.db"
            echo "── typstnique dev shell ──────────────────────────────"
            echo "  cargo leptos watch      # run dev server :3000"
            echo "  cargo fmt               # strict (nightly) formatting"
            echo "  pre-commit run --all    # run all pre-commit checks"
            echo "──────────────────────────────────────────────────────"
          '';
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
        };
      }
    );
}
