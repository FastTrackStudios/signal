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
      # A pinned nightly rustc used ONLY to compile phon-jit's copy-and-patch
      # stencils, which chain via `become` (explicit tail calls — a nightly
      # feature). The rest of the workspace stays on the stable 1.94 pin; the
      # stencils interface with stable-compiled code purely as extracted machine
      # code (symbols + relocations), so there is no ABI mixing. Pinned via the
      # locked rust-overlay input, so it's reproducible. Darwin-only: the native
      # stencil backend only compiles on macOS-aarch64 (elsewhere phon-jit uses
      # the interpreter and never invokes rustc for stencils).
      rustNightly = pkgs.rust-bin.selectLatestNightlyWith (t: t.minimal);
    in
    {
      # Rust toolchain — the FTS-wide pin (same as the dissolved
      # signal/daw/session flakes), with wasm for the web remotes.
      fts.rustToolchain = pkgs.rust-bin.stable."1.94.0".default.override {
        extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        # wasm for the web remotes; on darwin also the iOS targets for the
        # iPhone app (device + simulator), and x86_64-apple-darwin so
        # airlock (Apple Silicon) can cross-compile the Intel desktop/
        # plugin builds without needing real Intel hardware (deploy-macos.sh
        # TARGET=x86_64-apple-darwin, nice-plug-xtask's bundle-universal).
        targets = [ "wasm32-unknown-unknown" ]
          ++ lib.optionals pkgs.stdenv.isDarwin [
            "aarch64-apple-ios"
            "aarch64-apple-ios-sim"
            "x86_64-apple-darwin"
          ];
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
        # No explicit apple-sdk here: the darwin stdenv already carries one,
        # and a second SDK in scope makes the cc wrapper abort with
        # "Multiple conflicting values defined for DEVELOPER_DIR".
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
      // lib.optionalAttrs pkgs.stdenv.isDarwin (
        let
          # iOS cross toolchain: the nix cc wrapper only targets macOS, so
          # the iOS targets link/compile through Xcode's clang via xcrun,
          # with the nix SDK env (SDKROOT/DEVELOPER_DIR) scrubbed.
          iosCC = sdk: pkgs.writeShellScript "ios-clang-${sdk}" ''
            exec /usr/bin/env -u SDKROOT -u DEVELOPER_DIR /usr/bin/xcrun --sdk ${sdk} clang "$@"
          '';
          iosCXX = sdk: pkgs.writeShellScript "ios-clang++-${sdk}" ''
            exec /usr/bin/env -u SDKROOT -u DEVELOPER_DIR /usr/bin/xcrun --sdk ${sdk} clang++ "$@"
          '';
          # The wasm C compiler (ring's build for the web bundle) is clang-18,
          # but the darwin DYLD_LIBRARY_PATH below (for the app's runtime dylibs)
          # forces it to load the stdenv's clang-21 libclang-cpp → symbol
          # mismatch → SIGABRT. Run it with DYLD_LIBRARY_PATH unset so it
          # resolves its OWN libs via rpath.
          wasmCC = pkgs.writeShellScript "wasm-clang-18" ''
            exec /usr/bin/env -u DYLD_LIBRARY_PATH ${pkgs.llvmPackages_18.clang-unwrapped}/bin/clang "$@"
          '';
        in
        {
          DYLD_LIBRARY_PATH = libPath;
          # Override the common wasm CC with the DYLD-clean wrapper (darwin only).
          CC_wasm32_unknown_unknown = "${wasmCC}";
          # phon-jit's build script compiles its `become` tail-call stencils
          # with this pinned nightly rustc (the workspace pin is stable 1.94,
          # which can't parse `become`). See libs/vendor/phon-jit/build.rs.
          PHON_JIT_NIGHTLY_RUSTC = "${rustNightly}/bin/rustc";
          # Rust defaults aarch64-apple-ios to iOS 10, whose runtime lacks
          # `___chkstk_darwin` (it moved into libSystem at iOS 12) — linking
          # a large-stack crate then fails with an undefined symbol. Pin a
          # modern floor for compile + link consistency (the device is iOS
          # 26; 15 is a safe, widely-compatible baseline).
          IPHONEOS_DEPLOYMENT_TARGET = "15.0";
          CARGO_TARGET_AARCH64_APPLE_IOS_LINKER = "${iosCC "iphoneos"}";
          CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER = "${iosCC "iphonesimulator"}";
          CC_aarch64_apple_ios = "${iosCC "iphoneos"}";
          CXX_aarch64_apple_ios = "${iosCXX "iphoneos"}";
          CC_aarch64_apple_ios_sim = "${iosCC "iphonesimulator"}";
          CXX_aarch64_apple_ios_sim = "${iosCXX "iphonesimulator"}";
        }
      );
    };
}
