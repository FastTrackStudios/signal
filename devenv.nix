# signal's dev environment (devenv 2.4): `devenv shell`, or direnv through
# .envrc. devenv-fleet's `rust` module (devenv.yaml) gives the toolchain
# (rust 1.99.1 with wasm32, the rust-toolchain.toml pin), mold,
# cargo-hakari, cargo-rail, prek, just, pkg-config and `fleet-doctor`; its
# `darwin` module the macOS library path. This file adds what the rigs,
# the sampler and the plugins link against: daw's set (audio,
# PipeWire/JACK, the desktop UI), the same set as the apt packages CI
# installs (.github/workflows/checks.yml).
{ pkgs, lib, ... }:
let
  linux = with pkgs; lib.optionals stdenv.isLinux [
    # Audio I/O: cpal's ALSA, JACK and PipeWire backends.
    alsa-lib
    libjack2
    pipewire
    # hidapi's build script pkg-configs libudev (the Komplete Kontrol
    # driver, features/rigs/kontrol).
    udev
    # The desktop UI crates (Dioxus over GTK/WebKit).
    gtk3
    glib
    gdk-pixbuf
    pango
    cairo
    atk
    harfbuzz
    libsoup_3
    webkitgtk_4_1
    libayatana-appindicator
    dbus
    xdotool
    wayland
    libxkbcommon
  ];
  common = with pkgs; [ openssl fontconfig freetype ];
in
{
  packages = common ++ linux ++ (with pkgs; [
    # cargo fetches every git dependency through the git CLI. On macOS this
    # real git shadows the /usr/bin/git xcrun shim.
    git
    openssl.dev
  ])
  ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
    alsa-lib.dev
    pipewire.dev
    # Sets bindgen's clang arguments for crates that run it (pipewire-sys).
    rustPlatform.bindgenHook
  ]);

  env = {
    OPENSSL_DIR = "${pkgs.openssl.dev}";
    OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
  }
  // lib.optionalAttrs pkgs.stdenv.isLinux {
    LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
    # The loader finds what the linker found, so a test binary runs from
    # this shell.
    LD_LIBRARY_PATH = lib.makeLibraryPath (common ++ linux ++ [ pkgs.stdenv.cc.cc.lib ]);
  };

  # kache is not set here: the rust module exports RUSTC_WRAPPER from PATH.

  fleet.banner = ''
    signal dev shell
      cargo check --workspace --all-targets
      cargo test --workspace
  '';
}
