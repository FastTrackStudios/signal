# The dx toolchain trio (fills fts.dx.* and fts.pkgsDx).
#
# Dedicated, current-unstable nixpkgs used ONLY to source `dx`
# (dioxus-cli) at the version the workspace Cargo.lock pins (0.7.9)
# plus binaryen 129 (the wasm-opt dx 0.7.9 expects). The main
# `nixpkgs` (dioxus-flake's pin) carries dioxus-cli 0.7.4 / binaryen
# 126, which dx rejects / SIGABRTs with.
#
# The SAME trio serves the dev shell and the hermetic web bundles —
# previously the shell carried nixpkgs' 0.7.4 and cargo-installed 0.7.9
# into ~/.cargo/bin on every version drift (the "reinstalling dioxus"
# churn). Now dx comes from the store, prebuilt and cached.
{ inputs, ... }:
{
  perSystem = { system, lib, ... }:
    let
      pkgsDx = import inputs.nixpkgs-dx { inherit system; };
    in
    {
      fts.pkgsDx = pkgsDx;
      fts.dx.cli = pkgsDx.dioxus-cli;
      fts.dx.binaryen = pkgsDx.binaryen;

      # wasm-bindgen-cli matching the workspace Cargo.lock's
      # wasm-bindgen (0.2.126) — dx 0.7.9 rejects a mismatch. Built
      # through pkgsDx (its fetchCargoVendor pulls from
      # static.crates.io; the older pin's fetcher 403s).
      fts.dx.wasmBindgen = pkgsDx.rustPlatform.buildRustPackage rec {
        pname = "wasm-bindgen-cli";
        version = "0.2.126";
        src = pkgsDx.fetchCrate {
          inherit pname version;
          hash = "sha256-H6Is3fiZVxZCfOMWK5dWMSrtn50VGv0sfdnsT+cTtyk=";
        };
        cargoHash = "sha256-VucqkXbCi4qtQzY/HrXiDnbSURsagPsdNVMn1Tw3UiY=";
        nativeBuildInputs = [ pkgsDx.pkg-config ];
        buildInputs = lib.optionals pkgsDx.stdenv.isLinux [ pkgsDx.openssl ]
          ++ lib.optionals pkgsDx.stdenv.isDarwin
            (with pkgsDx.darwin.apple_sdk.frameworks; [ Security CoreFoundation ]);
        doCheck = false;
      };
    };
}
