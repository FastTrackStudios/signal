# cargo-rail — graph-aware workspace maintenance (unify / change /
# release / split / plan). nixpkgs carries 0.7.0, which predates the
# plan/change/release surface and mis-walks this workspace's graph, so
# we package the upstream release binaries (static musl on Linux — no
# patchelf) and pin by hash.
#
# KNOWN LIMIT (v0.17.3, tested 2026-07-16): `plan`/`affected` computes a
# wrong transitive-impact set on this tree — a leaf patchbay change
# selects ~all 214 members (dependents AND dependencies mixed), so CI
# change-selection stays on the hand-rolled path filter in
# .forgejo/workflows/ci-heavy.yml until upstream fixes the walk.
# `unify`, `change`, and `release` are unaffected.
{ ... }:
{
  perSystem = { pkgs, lib, system, ... }:
    let
      version = "0.17.3";
      assets = {
        x86_64-linux = {
          target = "x86_64-unknown-linux-musl";
          hash = "sha256-OVPS1ltIqJBvhQ26/F/2zBMdiLSAwb7RniJkYgkaIls=";
        };
        aarch64-linux = {
          target = "aarch64-unknown-linux-musl";
          hash = "sha256-cIuLaX36JF5SMDNOqjoWJxZyTbWP+A6CNFMyObiL/8Q=";
        };
        aarch64-darwin = {
          target = "aarch64-apple-darwin";
          hash = "sha256-kkAbnLtvj5mgzg0St4QK8dMtqwFJJc3+VfgUpucSRhs=";
        };
        # no upstream x86_64-darwin asset — fts.cargoRail is null there.
      };
      asset = assets.${system} or null;
      cargo-rail = pkgs.stdenvNoCC.mkDerivation {
        pname = "cargo-rail";
        inherit version;
        src = pkgs.fetchurl {
          url = "https://github.com/loadingalias/cargo-rail/releases/download/v${version}/cargo-rail-${asset.target}.tar.gz";
          inherit (asset) hash;
        };
        sourceRoot = ".";
        dontBuild = true;
        installPhase = ''
          install -Dm755 cargo-rail $out/bin/cargo-rail
        '';
        meta = {
          description = "Cargo-native monorepo control plane: unify, plan/run, change/release, split/sync";
          homepage = "https://github.com/loadingalias/cargo-rail";
          license = lib.licenses.mit;
          mainProgram = "cargo-rail";
        };
      };
    in
    {
      fts.cargoRail = if asset == null then null else cargo-rail;
    };
}
