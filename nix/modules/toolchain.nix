# The FTS toolchain + native build environment (fills fts.rustToolchain,
# fts.buildInputs, fts.nativeBuildInputs, fts.shellEnv).
{ ... }:
{
  perSystem = { pkgs, lib, config, ... }:
    let
      libPath = lib.makeLibraryPath (with pkgs;
        # stdenv.cc.cc.lib — libstdc++ for the NAM C++ core (and any other
        # dynamically-linked C++ dep). On NixOS hosts the system profile
        # papered over its absence; on GitHub-hosted (Ubuntu) runners
        # nix-built test binaries failed to load with
        # "libstdc++.so.6: cannot open shared object file".
        [ stdenv.cc.cc.lib fontconfig freetype openssl ]
        ++ lib.optionals pkgs.stdenv.isLinux [
          alsa-lib avahi libjack2 pipewire
          libGL vulkan-loader gtk3 glib
          gdk-pixbuf pango cairo atk
          libx11 libxcb libxkbcommon wayland
          webkitgtk_4_1 libsoup_3 xdotool
        ]
      );
    in
    {
      # Rust toolchain — the FTS-wide pin (same as the dissolved
      # signal/daw/session flakes), with wasm for the web remotes.
      fts.rustToolchain = pkgs.rust-bin.stable."1.94.0".default.override {
        extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        targets = [ "wasm32-unknown-unknown" ];
      };

      fts.buildInputs = (with pkgs; [
        openssl openssl.dev libiconv pkg-config fontconfig freetype cmake python3
      ])
      ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
        alsa-lib alsa-lib.dev
        # libudev — hidapi's build script pkg-configs it (kontrol's raw
        # USB HID access). On the old self-hosted runner a warm target
        # dir meant the build script never re-ran; cold hosted runners
        # exposed the missing dep.
        udev
        # ONNX Runtime for Chatterbox TTS (session-guide/tts). `ort` uses
        # load-dynamic, so it dlopens libonnxruntime.so at runtime via
        # ORT_DYLIB_PATH (below) — never a downloaded binary, which Nix
        # rejects.
        onnxruntime
        # avahi — vox-discover links libavahi-client (mDNS service
        # discovery for the rig remotes); `.dev` carries the headers.
        avahi avahi.dev
        # JACK headers/lib for cpal's `jack` feature (signal-sampler).
        # At runtime, run under `pw-jack` to route through PipeWire.
        libjack2
        # libpipewire headers/lib for cpal's native `pipewire` feature —
        # talks to PipeWire directly (no JACK shim). `.dev` carries the
        # `libpipewire-0.3.pc` + headers that bindgen/pkg-config need.
        pipewire pipewire.dev
        glib gtk3 gdk-pixbuf pango cairo atk harfbuzz
        libsoup_3 webkitgtk_4_1 xdotool
        libx11 libxcursor libxrandr libxi libxcb
        libxkbcommon wayland libGL vulkan-loader
      ])
      ++ lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
        apple-sdk_15
        libiconv
      ]);

      fts.nativeBuildInputs = with pkgs; [
        pkg-config
        rustPlatform.bindgenHook
        tailwindcss_4
      ];

      # Env every dev/CI shell needs — build-script and bindgen paths,
      # the wasm cross toolchain, runtime library paths.
      fts.shellEnv = {
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        OPENSSL_DIR = "${pkgs.openssl.dev}";
        OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
        # Unwrapped clang: the nix cc-wrapper injects hardening flags
        # (-fzero-call-used-regs) unsupported on wasm32 and leaks glibc
        # includes past -nostdlibinc (breaks ring). Builtin headers come
        # from the wrapper's resource-root instead.
        CC_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.clang-unwrapped}/bin/clang";
        # bintools (the wrapper) only exposes unprefixed names (ar, ld…);
        # llvm-ar lives in bintools-unwrapped. The wrapper path stood
        # here before and only "worked" while a warm target/ kept ring's
        # build script from re-running — cold CI builds hit it.
        AR_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.bintools-unwrapped}/bin/llvm-ar";
        CFLAGS_wasm32_unknown_unknown = "-isystem ${pkgs.llvmPackages_18.clang}/resource-root/include";
        RUST_SRC_PATH = "${config.fts.rustToolchain}/lib/rustlib/src/rust/library";
      }
      // lib.optionalAttrs pkgs.stdenv.isLinux {
        LD_LIBRARY_PATH = libPath;
        # Chatterbox TTS: `ort` (load-dynamic) dlopens this exact .so at
        # runtime. Missing/unset → synthesis fails and section cues fall
        # back to the synth chime; the app still runs.
        ORT_DYLIB_PATH = "${pkgs.onnxruntime}/lib/libonnxruntime.so";
        XDG_DATA_DIRS = "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}";
        # WebKitGTK accelerated compositing fails on NixOS (GBM buffer
        # error → white window). Force software rendering.
        # See: https://github.com/NixOS/nixpkgs/issues/32580
        WEBKIT_DISABLE_COMPOSITING_MODE = "1";
      }
      // lib.optionalAttrs pkgs.stdenv.isDarwin {
        DYLD_LIBRARY_PATH = libPath;
      };
    };
}
