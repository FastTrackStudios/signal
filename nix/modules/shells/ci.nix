# CI shell — the default shell minus every interactive convenience.
# No shellHook installers (the dx / graphify / tracey cargo-installs
# stalled CI jobs for HOURS compiling from source on cache misses),
# no dioxus-cli / tailwind / editor tooling (CI drives plain cargo),
# no .env sourcing. Toolchain + native headers + the env the build
# scripts need, nothing else. Workflows enter it via `nix develop .#ci`.
{ ... }:
{
  perSystem = { pkgs, lib, config, ... }: {
    devShells.ci = pkgs.mkShell ({
      packages = [ config.fts.rustToolchain pkgs.cargo-nextest ]
      # cargo-rail — release/unify automation (`cargo rail unify --check`
      # as a hygiene gate). NOT used for CI change-selection yet: its
      # plan/affected walk over-selects on this workspace — see
      # nix/modules/cargo-rail.nix. Store-sourced only (never installed
      # from a hook — see the stall note above).
      ++ lib.optionals (config.fts.cargoRail != null) [ config.fts.cargoRail ]
      ++ config.fts.buildInputs
      ++ [ pkgs.pkg-config pkgs.rustPlatform.bindgenHook ];

      # Seeded cargo-home bins (dx for the web-bundle steps) resolve
      # from PATH — never installed from here. APPEND, don't prepend:
      # GitHub-hosted runners ship a rustup stable in ~/.cargo/bin that
      # would otherwise shadow the pinned nix rustc (first symptom:
      # "can't find crate for core" on the wasm target).
      shellHook = ''
        export PATH="$PATH:$HOME/.cargo/bin"
      '';
    }
    // config.fts.shellEnv);
  };
}
