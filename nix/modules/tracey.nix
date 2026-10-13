# tracey — spec-coverage CLI (bearcove/tracey). Requirements live in
# docs/spec/** as r[<id>] paragraphs, code points at them with
# r[impl <id>] / r[verify <id>], and `tracey query --json …` reports the
# coverage; .config/tracey/config.styx is the config. The CI gate
# (scripts/tracey-flow-gate.py, `just daw-flows`) runs it.
#
# Store-sourced from the upstream release tarball, NOT `cargo install`:
# the crate's build.rs needs node + pnpm (with network) to bundle the
# web dashboard, which is both un-sandboxable and the from-source
# install that used to stall CI for hours (see shells/ci.nix). The
# release binary links only libc/libm/libgcc_s, so autoPatchelf is all
# it takes on NixOS. Not in nixpkgs, no upstream flake.
{ ... }:
{
  perSystem = { pkgs, lib, ... }:
    let
      version = "1.3.0";
      # `nix hash file --sri tracey-<target>.tar.xz` per release asset.
      assets = {
        x86_64-linux = {
          target = "x86_64-unknown-linux-gnu";
          hash = "sha256-+8DCXEQyjMsJcLJQkJX/KUEvpyy7xrADbEzujBKCH0c=";
        };
        aarch64-linux = {
          target = "aarch64-unknown-linux-gnu";
          hash = "sha256-fmTjbT1LXr0j5tv8sWDCitgacZBkGdM2u+uOFG6CYGQ=";
        };
        aarch64-darwin = {
          target = "aarch64-apple-darwin";
          hash = "sha256-NltLMFbFiZAVJrVEAbI2NEMKkl/LEyf1zW9TVoY7INU=";
        };
      };
      asset = assets.${pkgs.stdenv.hostPlatform.system} or null;
      tracey = pkgs.stdenvNoCC.mkDerivation {
        pname = "tracey";
        inherit version;
        src = pkgs.fetchurl {
          url = "https://github.com/bearcove/tracey/releases/download/v${version}/tracey-${asset.target}.tar.xz";
          inherit (asset) hash;
        };
        sourceRoot = "tracey-${asset.target}";
        nativeBuildInputs = lib.optionals pkgs.stdenv.isLinux [ pkgs.autoPatchelfHook ];
        buildInputs = lib.optionals pkgs.stdenv.isLinux [ pkgs.stdenv.cc.cc.lib ];
        dontConfigure = true;
        dontBuild = true;
        installPhase = ''
          runHook preInstall
          install -Dm755 tracey "$out/bin/tracey"
          runHook postInstall
        '';
        meta = {
          description = "Requirement traceability between spec markdown and code";
          homepage = "https://github.com/bearcove/tracey";
          license = with lib.licenses; [ mit asl20 ];
          mainProgram = "tracey";
          platforms = builtins.attrNames assets;
        };
      };
    in
    {
      fts.tracey = if asset == null then null else tracey;
    };
}
