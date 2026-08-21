# FastTrackStudio — root workspace recipes
# Run commands: just <recipe-name>

# List recipes by default
default:
    @just --list

# ── Tailwind (app UI sheet) ──────────────────────────────────────────────
# The single compiled sheet (assets/tailwind-signal.css) is inlined by
# BOTH apps/fasttrackstudio/src/rig_view.rs (signal UI) and
# src/main.rs SessionChrome (session UI) via include_str!; rebuild it
# whenever UI-crate class usage changes. input.css @source globs scan the
# signal/session UI crates + libs/ui + libs/dock.

# Point apps/fasttrackstudio/.lumen-blocks at the lumen-blocks checkout
# cargo actually resolved.
#
# input.css used to reach into $CARGO_HOME with a hardcoded /home/cody path
# and a `lumen-blocks-*/*/` wildcard. That wildcard matched EVERY checkout
# in the cache, not the pinned one — four of them here — so the compiled
# sheet depended on which revisions happened to be lying around, and
# changed on its own as the cache moved. That's what kept this file
# permanently dirty. `cargo metadata` knows the real answer; ask it.
_lumen-link:
    #!/usr/bin/env bash
    set -euo pipefail
    dir=$(cargo metadata --locked --format-version 1 2>/dev/null \
        | python3 -c 'import json,sys,os; print(next(os.path.dirname(p["manifest_path"]) for p in json.load(sys.stdin)["packages"] if p["name"]=="lumen-blocks"))')
    ln -sfn "$dir" apps/fasttrackstudio/.lumen-blocks

# Build Tailwind CSS (v4)
tailwind: _lumen-link
    cd apps/fasttrackstudio && tailwindcss -i ./input.css -o ./assets/tailwind-signal.css --minify

# Watch Tailwind CSS for changes
tailwind-watch: _lumen-link
    cd apps/fasttrackstudio && tailwindcss -i ./input.css -o ./assets/tailwind-signal.css --watch --minify

# Fail if the committed sheet isn't what the sources produce.
#
# tailwind-signal.css is `include_str!`d by rig_view.rs and main.rs, so it
# has to be committed — which means it can go stale silently when someone
# adds a class and doesn't rebuild. It was stale by ~50 classes when this
# check was written.
tailwind-check: tailwind
    #!/usr/bin/env bash
    set -euo pipefail
    if ! git diff --quiet -- apps/fasttrackstudio/assets/tailwind-signal.css; then
        echo "tailwind-signal.css is out of date — run 'just tailwind' and commit the result" >&2
        exit 1
    fi
    echo "tailwind-signal.css is up to date"

# ── Live Rigs (carried from the dissolved signal workspace) ──────────────
# Open a live instrument rig: live input → FX chain (NAM amp / cab / plugins)
# → output, routed through PipeWire via cpal's NATIVE PipeWire backend. Each
# rig's interface / input channel / profile is remembered in
# ~/.config/signal/rigs/<name>.styx.
#
# NOTE: needs `libpipewire` on PKG_CONFIG_PATH for `--features pipewire`.

# The signal engine — the headless rig core (serves the vox router on
# ws://:4040/vox): the fasttrackstudio binary in --engine mode. `rigd`
# kept as an alias for muscle memory.
signal-engine:
    cargo run --release -p fasttrackstudio -- --engine

alias rigd := signal-engine

# Stage the fts web bundle (the browser remote) for embedding: tailwind →
# dx web build (signal rigs + the session setlist remote) →
# apps/fasttrackstudio/web-dist/, which `cargo build -p fasttrackstudio
# --features embed-web` compiles into the binary (include_dir). On wasm the
# session feature is only the wire surface (session-proto clients +
# session-ui); the player itself runs in the engine. web-dist/ is gitignored.
web-stage: tailwind
    cd apps/fasttrackstudio && dx build --platform web --release --no-default-features --features signal,session
    rm -rf apps/fasttrackstudio/web-dist
    cp -r target/dx/fasttrackstudio/release/web/public apps/fasttrackstudio/web-dist
    just keys-worklet-wasm

# Stage the browser keys rig's AudioWorklet bundle (W4 of
# crates/signal/docs/browser-keys-rig.md) into the web bundle:
# a RELEASE wasm build of signal-keys-worklet (KeysWorklet =
# WebRenderer + headless KeysRig), wasm-bindgen'd `--target web`, plus
# the keys-specific processor + worklet polyfill (AudioWorkletGlobalScope
# has no dynamic import(), TextDecoder, crypto, or performance — see
# features/rigs/keys/worklet/). The page (src/web_keys_rig.rs) expects:
#   /worklet/keys_processor.js
#   /worklet/signal_keys_worklet.js
#   /worklet/signal_keys_worklet_bg.wasm
# wasm-bindgen-cli comes from the dev shell, pinned to the workspace
# wasm-bindgen version (same as task-worklet-wasm).
keys-worklet-wasm out='apps/fasttrackstudio/web-dist/worklet':
    # --max-memory: default wasm linear memory caps at 2 GB — resident pack
    # bytes + decoded-PCM budget + app need the full 4 GB address space.
    #
    # +simd128: wasm SIMD (stable, and in every browser we target). The rig
    # renders nine lanes of sampler + FX inside ONE audio thread, so DSP
    # throughput is the headroom that decides whether it plays clean; the
    # per-sample loops (gain, mix, filters, interpolation) are exactly what
    # 128-bit lanes accelerate. Measure `renderLoad()` across this change —
    # it is the number that says whether the render fits the quantum.
    RUSTFLAGS="-C link-arg=--max-memory=4294967296 -C target-feature=+simd128" \
    cargo build -p signal-keys-worklet --lib \
        --target wasm32-unknown-unknown --release
    mkdir -p {{out}}
    wasm-bindgen --target web --out-dir {{out}} \
        --out-name signal_keys_worklet \
        target/wasm32-unknown-unknown/release/signal_keys_worklet.wasm
    cp features/rigs/keys/worklet/keys_processor.js {{out}}/keys_processor.js
    cp features/rigs/keys/worklet/worklet_polyfill.js {{out}}/worklet_polyfill.js
    cp features/rigs/keys/worklet/keys_decoder_worker.js {{out}}/keys_decoder_worker.js
    cp features/rigs/keys/worklet/keys_streamer_worker.js {{out}}/keys_streamer_worker.js

# W13: the SHARED-MEMORY worklet build — wasm threads.
#
# The rig's audio thread must never decode, and copying PCM to it costs a
# memcpy per sample. With shared memory the decoder threads write chunks
# straight into the heap the audio thread reads, which is how the NATIVE
# engine already works (fts-sample's streamer pool). Requirements:
#
#   +atomics,+bulk-memory,+mutable-globals   the thread ABI
#   --shared-memory --import-memory          one memory across instances
#   -Z build-std                             std must be rebuilt with atomics
#                                            (hence nightly — the rest of the
#                                            tree stays on stable 1.94)
#
# The page creates the WebAssembly.Memory and hands it to the worklet and to
# each decoder worker, so all three instantiate over the SAME heap. Serving
# it needs cross-origin isolation (EngineHost::cross_origin_isolated).
#
# Kept as its OWN recipe until it is proven: `keys-worklet-wasm` (single
# threaded) stays the default so a toolchain problem here can never take the
# working rig down with it.
keys-worklet-wasm-threads out='apps/fasttrackstudio/web-dist/worklet':
    # The four TLS symbols are exported EXPLICITLY: wasm-bindgen's threading
    # pass looks up `__wasm_init_tls` (each thread initialises its own TLS
    # block through it), and LLD garbage-collects all of them when no Rust
    # code happens to use `#[thread_local]` — which shows up much later as
    # `failed to prepare module for threading: failed to find
    # __wasm_init_tls`, long after the Rust build succeeded.
    RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals,+simd128 \
        -C link-arg=--shared-memory \
        -C link-arg=--import-memory \
        -C link-arg=--max-memory=4294967296 \
        -C link-arg=--export=__wasm_init_tls \
        -C link-arg=--export=__tls_size \
        -C link-arg=--export=__tls_align \
        -C link-arg=--export=__tls_base" \
    cargo-nightly build -p signal-keys-worklet --lib \
        -Z build-std=std,panic_abort \
        --target wasm32-unknown-unknown --release
    mkdir -p {{out}}
    wasm-bindgen --target web --out-dir {{out}} \
        --out-name signal_keys_worklet \
        target/wasm32-unknown-unknown/release/signal_keys_worklet.wasm
    cp features/rigs/keys/worklet/keys_processor.js {{out}}/keys_processor.js
    cp features/rigs/keys/worklet/worklet_polyfill.js {{out}}/worklet_polyfill.js
    cp features/rigs/keys/worklet/keys_decoder_worker.js {{out}}/keys_decoder_worker.js
    cp features/rigs/keys/worklet/keys_streamer_worker.js {{out}}/keys_streamer_worker.js

# ONE engine binary serving the whole browser keys rig: stage the web
# bundle + keys worklet (web-stage), then embed it into the release
# binary. Order matters — embed-web include_dir!s web-dist/ at compile
# time, so staging runs first. Then:
#   target/release/fasttrackstudio --engine     (binds 0.0.0.0:4040)
# and open http://<host>:4040/rigs/keys/worship (tailnet-reachable).
keys-web: web-stage
    cargo build --release -p fasttrackstudio --features embed-web

# Playwright end-to-end suite for the browser keys rig (W5): spawns its
# own engine on a scratch port (SIGNAL_ENGINE_ADDR) and proves the rig
# makes SOUND in real chromium. Expects target/release/fasttrackstudio
# to exist — build it with `just keys-web` first. Needs the real pack
# library (or FTS_PACK_LIBRARY pointing at one with the Worship proxies).
keys-web-e2e:
    cd apps/fasttrackstudio/e2e && npm install --no-fund --no-audit && npx playwright test

# Build the RELEASE binary (web bundle EMBEDDED) and deploy the ONE
# artifact to ~/.local/lib/fts/fasttrackstudio behind the signal-engine
# systemd user unit. The unit is installed but NOT enabled: the desktop
# app (or `systemctl --user start signal-engine`) is the on/off switch;
# while running, systemd restarts crashes in ~1s; an explicit stop is
# final. If the engine is running during deploy it restarts onto the new
# build, otherwise it stays stopped.
# Logs: `journalctl --user -u signal-engine`.
rig-install: web-stage
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p fasttrackstudio --features embed-web
    install -d ~/.local/lib/fts
    install -m 755 target/release/fasttrackstudio ~/.local/lib/fts/fasttrackstudio.new
    mv -T ~/.local/lib/fts/fasttrackstudio.new ~/.local/lib/fts/fasttrackstudio
    # The pre-consolidation artifacts (signal-engine binary + signal-web
    # bundle) are superseded; leave any existing ones in place until the
    # new unit is confirmed, then clean by hand if desired.
    install -d ~/.config/systemd/user
    install -m 644 apps/fasttrackstudio/systemd/signal-engine.service ~/.config/systemd/user/
    systemctl --user daemon-reload
    systemctl --user try-restart signal-engine
    if systemctl --user is-active --quiet signal-engine; then
        sleep 3
        curl -sf http://127.0.0.1:4040/health >/dev/null && echo "deployed + restarted: health ok"
    else
        echo "deployed (engine stopped — start it from the app or: systemctl --user start signal-engine)"
    fi

# Full install on this machine: everything rig-install does (release
# binary + embedded web UI + systemd unit) PLUS the `fts` CLI, PATH
# symlinks in ~/.local/bin, and desktop integration (launcher entry +
# icon) — FastTrackStudio shows up in the app menu like any other app.
install: rig-install
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p fts-cli
    install -m 755 target/release/fts ~/.local/lib/fts/fts.new
    mv -T ~/.local/lib/fts/fts.new ~/.local/lib/fts/fts
    install -d ~/.local/bin
    ln -sf ~/.local/lib/fts/fasttrackstudio ~/.local/bin/fasttrackstudio
    ln -sf ~/.local/lib/fts/fts ~/.local/bin/fts
    install -d ~/.local/share/icons/hicolor/scalable/apps
    install -m 644 apps/fasttrackstudio/assets/icon.svg \
        ~/.local/share/icons/hicolor/scalable/apps/fasttrackstudio.svg
    install -d ~/.local/share/applications
    sed "s|@BIN@|$HOME/.local/lib/fts/fasttrackstudio|" \
        apps/fasttrackstudio/assets/fasttrackstudio.desktop \
        > ~/.local/share/applications/fasttrackstudio.desktop
    update-desktop-database ~/.local/share/applications 2>/dev/null || true
    gtk-update-icon-cache ~/.local/share/icons/hicolor 2>/dev/null || true
    echo "installed: fasttrackstudio + fts in ~/.local/bin, launcher entry ready"

# Install Patchbay (the PipeWire studio-routing app): release binary in
# ~/.local/lib/fts, `patchbay` on PATH, launcher entry + icon.
# dx web build of the patchbay browser remote → apps/patchbay/web-dist/,
# embedded into fts-patchbay by `--features embed-web` (patchbay-install).
patchbay-web-stage:
    cd apps/patchbay/web && dx build --platform web --release
    rm -rf apps/patchbay/web-dist
    cp -r target/dx/patchbay-web/release/web/public apps/patchbay/web-dist

patchbay-install: patchbay-web-stage
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p fts-patchbay --features embed-web
    install -d ~/.local/lib/fts
    install -m 755 target/release/fts-patchbay ~/.local/lib/fts/patchbay.new
    mv -T ~/.local/lib/fts/patchbay.new ~/.local/lib/fts/patchbay
    install -d ~/.local/bin
    ln -sf ~/.local/lib/fts/patchbay ~/.local/bin/patchbay
    install -d ~/.local/share/icons/hicolor/scalable/apps
    install -m 644 apps/patchbay/assets/icon.svg \
        ~/.local/share/icons/hicolor/scalable/apps/patchbay.svg
    install -d ~/.local/share/applications
    sed "s|@BIN@|$HOME/.local/lib/fts/patchbay|" \
        apps/patchbay/assets/patchbay.desktop \
        > ~/.local/share/applications/patchbay.desktop
    update-desktop-database ~/.local/share/applications 2>/dev/null || true
    gtk-update-icon-cache ~/.local/share/icons/hicolor 2>/dev/null || true
    # KDE keeps its own per-environment menu cache; rebuild it in the
    # session's env (a dev-shell kbuildsycoca updates the wrong cache).
    systemd-run --user --collect kbuildsycoca6 2>/dev/null || kbuildsycoca6 2>/dev/null || true
    echo "installed: Patchbay (run 'patchbay' or launch from the app menu)"

# Remove everything `just install` put on this machine: stop + remove
# the systemd unit, binaries, symlinks, launcher entry, and icon.
# User data is untouched (~/.config/fts, ~/.config/signal — the rig
# config may be a symlink into this repo; never deleted).
uninstall:
    #!/usr/bin/env bash
    set -euo pipefail
    systemctl --user stop signal-engine 2>/dev/null || true
    systemctl --user disable signal-engine 2>/dev/null || true
    rm -f ~/.config/systemd/user/signal-engine.service
    systemctl --user daemon-reload
    rm -f ~/.local/bin/fasttrackstudio ~/.local/bin/fts
    rm -rf ~/.local/lib/fts
    rm -f ~/.local/share/applications/fasttrackstudio.desktop
    rm -f ~/.local/share/icons/hicolor/scalable/apps/fasttrackstudio.svg
    update-desktop-database ~/.local/share/applications 2>/dev/null || true
    gtk-update-icon-cache ~/.local/share/icons/hicolor 2>/dev/null || true
    echo "uninstalled (user data in ~/.config/fts and ~/.config/signal kept)"

# ── Release packaging ────────────────────────────────────────────────────
# Assemble the distributable release artifacts into dist/ (what a
# codeberg release carries, and what fts-installer downloads):
#   fasttrackstudio-v<ver>-x86_64-linux.tar.gz   app + fts CLI + systemd
#       unit + desktop/icon templates + install.sh/uninstall.sh + VERSION
#   fts-installer-x86_64-linux                    standalone installer
#   SHA256SUMS                                    covers both
# Binary copies are patchelf'd to the standard /lib64 loader so they run
# outside the nix shell (target machines still need the shared libs —
# see `ldd` on the packaged binary).
release-package: web-stage
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p fasttrackstudio --features embed-web
    cargo build --release -p fts-cli
    cargo build --release -p fts-installer
    cargo build --release -p fts-extensions
    version="$(cargo pkgid -p fasttrackstudio | sed 's/.*[#@]//')"
    plat=x86_64-linux
    if command -v patchelf >/dev/null; then PATCHELF=(patchelf); else PATCHELF=(nix shell nixpkgs#patchelf -c patchelf); fi
    stage="$(mktemp -d)"; trap 'rm -rf "$stage"' EXIT
    cp target/release/fasttrackstudio target/release/fts "$stage"/
    cp target/release/fts-installer "$stage/fts-installer-$plat"
    for b in fasttrackstudio fts "fts-installer-$plat"; do
        "${PATCHELF[@]}" --set-interpreter /lib64/ld-linux-x86-64.so.2 --remove-rpath "$stage/$b"
        strip "$stage/$b"
    done
    # REAPER extension cdylib: no interpreter to patch (shared lib), just
    # rpath + symbols. install.sh drops it into ~/.config/REAPER/UserPlugins
    # when a REAPER install is present.
    cp target/release/libreaper_fts_extensions.so "$stage/reaper_fts_extensions.so"
    "${PATCHELF[@]}" --remove-rpath "$stage/reaper_fts_extensions.so"
    strip "$stage/reaper_fts_extensions.so"
    cp apps/fasttrackstudio/systemd/signal-engine.service "$stage"/
    cp apps/fasttrackstudio/assets/fasttrackstudio.desktop "$stage"/
    cp apps/fasttrackstudio/assets/icon.svg "$stage/icon.svg"
    install -m 755 apps/installer/scripts/install.sh apps/installer/scripts/uninstall.sh "$stage"/
    printf '%s\n' "$version" > "$stage/VERSION"
    mkdir -p dist
    tarball="fasttrackstudio-v$version-$plat.tar.gz"
    tar -czf "dist/$tarball" -C "$stage" \
        fasttrackstudio fts reaper_fts_extensions.so \
        signal-engine.service fasttrackstudio.desktop \
        icon.svg install.sh uninstall.sh VERSION
    mv "$stage/fts-installer-$plat" dist/
    (cd dist && sha256sum "$tarball" "fts-installer-$plat" > SHA256SUMS)
    echo "packaged:"
    ls -lh "dist/$tarball" "dist/fts-installer-$plat" dist/SHA256SUMS

# Rebuild eq-ui's embedded Tailwind (features/fx/eq/eq-ui/assets/
# tailwind.css) after class changes in eq-ui / architect-ui.
tailwind-eq:
    tailwindcss -i features/fx/eq/eq-ui/tailwind.css -o features/fx/eq/eq-ui/assets/tailwind.css --minify

# Rebuild comp-ui's embedded Tailwind (features/fx/comp/comp-ui/assets/
# tailwind.css) after class changes in comp-ui / architect-ui.
tailwind-comp:
    tailwindcss -i features/fx/comp/comp-ui/tailwind.css -o features/fx/comp/comp-ui/assets/tailwind.css --minify

tailwind-limiter:
    tailwindcss -i features/fx/comp/limiter-ui/tailwind.css -o features/fx/comp/limiter-ui/assets/tailwind.css --minify

# Rebuild trigger-ui's embedded Tailwind (features/fx/trigger/trigger-ui/
# assets/tailwind.css) after class changes in trigger-ui / architect-ui.
tailwind-trigger:
    tailwindcss -i features/fx/trigger/trigger-ui/tailwind.css -o features/fx/trigger/trigger-ui/assets/tailwind.css --minify

# Rasterize the comp editor to PNGs — every profile face, plus the Advanced
# page and a resized panel — so a GUI change can be looked at without opening
# a DAW. Same headless mount the behavioural tests drive, painted through
# blitz + vello_cpu. Shots land in target/gui-shots/comp/ (FTS_SHOTS_DIR
# overrides).
comp-shots:
    cargo test -p comp-ui --features native --test screenshots -- --nocapture

# Same for the EQ: every hardware model's faceplate, painted headless.
# Shots land in target/gui-shots/eq/.
eq-shots:
    cargo test -p eq-ui --features native --test screenshots -- --nocapture

# Every reverb family's panel, painted headless. Shots land in
# target/gui-shots/reverb/.
reverb-shots:
    cargo test -p reverb-ui --features native --test screenshots -- --nocapture

# Every delay family's panel, painted headless. Shots land in
# target/gui-shots/delay/.
delay-shots:
    cargo test -p delay-ui --features native --test screenshots -- --nocapture

# Every saturation circuit's panel, painted headless. Shots land in
# target/gui-shots/saturate/.
saturate-shots:
    cargo test -p saturate-ui --features native --test screenshots -- --nocapture

# Every modulation circuit's panel, painted headless. Shots land in
# target/gui-shots/modulation/.
modulation-shots:
    cargo test -p modulation-ui --features native --test screenshots -- --nocapture

# THE plugin suite — the one list every plugin recipe iterates. A name here
# is `<name>-plugin` as a cargo package and "FTS <Name>" as a bundle (see
# bundler.toml); adding a plugin means touching this line and that file.
fts_plugins := "eq comp reverb delay tune modulation nam level saturate signal guide gate limiter trigger meter pitch unison"

# Bundle every FTS plugin as .clap + .vst3 (target/bundled/, names from
# bundler.toml). Pass a subset to bundle only those:
#   just plugins-bundle "eq comp"
# Debug of a single plugin: cargo run -p fts-plugin-xtask -- bundle -p eq-plugin
#
# On macOS this uses nice-plug-xtask's `bundle-universal` instead of
# `bundle`: it builds both aarch64-apple-darwin and x86_64-apple-darwin and
# lipo's them into one universal .clap/.vst3 per plugin — no custom lipo
# scripting needed. Requires the x86_64-apple-darwin rustc target (added to
# fts.rustToolchain for darwin — nix/modules/toolchain.nix).
plugins-bundle plugins=fts_plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    cmd=bundle
    [ "$(uname)" = "Darwin" ] && cmd=bundle-universal
    for p in {{plugins}}; do
        cargo run -q -p fts-plugin-xtask -- "$cmd" -p "$p-plugin" --release
    done
    ls target/bundled/

# Install the bundled plugins into THIS machine's user plugin dirs, straight
# from target/bundled — no release download, no network. Linux: ~/.clap and
# ~/.vst3; macOS: ~/Library/Audio/Plug-Ins/{CLAP,VST3}. Writes the same
# manifest the release installer does, so `just plugins-uninstall` removes
# exactly this set (and replaces any stale symlink or older copy of the same
# name left over from a previous worktree).
#
# Build + install everything:        just plugins-install
# Iterate on one:                    just plugins-bundle eq && just plugins-install
plugins-install: plugins-bundle
    cargo run -q -p fts-installer -- plugins install --from target/bundled

# Prove every bundle in target/bundled actually LOADS — dlopen it, run its
# entry point, and walk its factory (apps/plugins/verify/). A plugin that
# compiles, links, and exports the right symbol can still fail in a host: a
# missing dependency or a panicking init only shows up at load time, and
# `nm` cannot see either.
#
# Works on both platforms, including their different bundle shapes (Linux
# .clap is a bare shared object, macOS .clap is a directory) and different
# VST3 entry-point names (ModuleEntry vs bundleEntry). On macOS, pass an
# arch to run the universal binaries in one personality:
#   just plugins-verify              # native
#   just plugins-verify x86_64       # the Intel half, under Rosetta
plugins-verify arch="":
    #!/usr/bin/env bash
    set -euo pipefail
    [ -d target/bundled ] || { echo "no target/bundled — run 'just plugins-bundle' first" >&2; exit 1; }
    out="target/plugin-verify"; mkdir -p "$out"
    archflag=""
    [ -n "{{arch}}" ] && archflag="-arch {{arch}}"
    cc $archflag -o "$out/clap_load" apps/plugins/verify/clap_load.c -ldl
    cc $archflag -o "$out/vst3_load" apps/plugins/verify/vst3_load.c -ldl
    # The loadable binary inside a bundle, whatever shape the bundle is.
    binary_in() {
        if [ -d "$1" ]; then find "$1/Contents" -type f -perm -u+x ! -name "*.txt" ! -name "PkgInfo" | head -1
        else echo "$1"; fi
    }
    fail=0
    for b in target/bundled/*.clap target/bundled/*.vst3; do
        [ -e "$b" ] || continue
        case "$b" in *.clap) loader="$out/clap_load";; *) loader="$out/vst3_load";; esac
        bin="$(binary_in "$b")"
        if [ -z "$bin" ]; then printf "%-24s FAIL no binary in bundle\n" "$(basename "$b")"; fail=1; continue; fi
        result="$("$loader" "$bin" 2>&1 | tail -1)"
        printf "%-24s %s\n" "$(basename "$b")" "$result"
        case "$result" in OK*) ;; *) fail=1;; esac
    done
    [ "$fail" = 0 ] || { echo "FAILURES — some bundles do not load" >&2; exit 1; }
    echo "all bundles load"

# Remove every plugin recorded in the install manifest.
plugins-uninstall:
    cargo run -q -p fts-installer -- plugins uninstall

# What the manifest says is installed, and from which version.
plugins-list:
    cargo run -q -p fts-installer -- plugins list

# Package the plugin bundles as a single release tarball in dist/
# (fts-plugins-v<version>-<platform>.tar.gz + SHA256SUMS entry). The macOS
# release artifact is NOT this — it's the signed+notarized .zip built by
# apps/fasttrackstudio/ios/deploy-macos-plugins.sh, since Apple only accepts
# zip/pkg/dmg for notarization.
plugins-package: plugins-bundle
    #!/usr/bin/env bash
    set -euo pipefail
    version="$(cargo pkgid -p eq-plugin | sed 's/.*[#@]//')"
    case "$(uname)-$(uname -m)" in
        Darwin-*)        plat=macos ;;
        Linux-x86_64)    plat=x86_64-linux ;;
        Linux-aarch64)   plat=aarch64-linux ;;
        *) echo "unsupported platform: $(uname)-$(uname -m)" >&2; exit 1 ;;
    esac
    mkdir -p dist
    tarball="fts-plugins-v$version-$plat.tar.gz"
    tar -czf "dist/$tarball" -C target/bundled .
    (cd dist && sha256sum "$tarball" >> SHA256SUMS 2>/dev/null || sha256sum "$tarball" > SHA256SUMS)
    echo "packaged: dist/$tarball"

# Pull the latest upstream NeuralAmpModelerCore into the vendored copy
# (libs/neural-amp-modeler/NeuralAmpModelerCore) and run the crate's
# test suite. The parity tests run every shipped rig model through BOTH
# engines (upstream C++ oracle vs the pure-Rust wasm engine) — a
# divergence or a new unsupported architecture fails loudly and is the
# to-port list for src/pure/. Review the diff before committing.
nam-update:
    #!/usr/bin/env bash
    set -euo pipefail
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    git clone --depth 1 --recurse-submodules --shallow-submodules \
        https://github.com/sdatkinson/NeuralAmpModelerCore "$tmp/core"
    dst="libs/neural-amp-modeler/NeuralAmpModelerCore"
    rsync -a --delete \
        --exclude .git --exclude build --exclude build_inline \
        "$tmp/core/" "$dst/"
    echo "vendored $(git -C "$tmp/core" rev-parse --short HEAD); running parity…"
    cargo test -p neural-amp-modeler

# Symlink the live rig config (~/.config/signal/rig) to the repo's
# in-tree default config, so realtime edits — text editor or the rig's
# own auto-save — are working-tree diffs you commit like any change.
# The previous config dir is moved aside as rig.bak-<date>.
rig-link:
    #!/usr/bin/env bash
    set -euo pipefail
    target="$(pwd)/features/rigs/guitar/default-config"
    rig="$HOME/.config/signal/rig"
    if [ -L "$rig" ]; then echo "already linked: $rig -> $(readlink "$rig")"; exit 0; fi
    if [ -e "$rig" ]; then mv "$rig" "$rig.bak-$(date +%Y%m%d-%H%M%S)"; fi
    mkdir -p "$(dirname "$rig")"
    ln -s "$target" "$rig"
    echo "linked: $rig -> $target"

# Open the default guitar rig (Yamaha TF ch4 → NAM amps)
guitar: (rig "Guitar Rig")

# Open the default drums rig (needs `just rig-setup "Drum Rig" ...` first)
drums: (rig "Drum Rig")

# Built then run separately, under `pw-jack`, for one reason: midir's MIDI
# backend on Linux is JACK-over-pipewire-jack, but the dev shell links the real
# libjack2, so the binary looks for a `jackd` that is not running and hardware
# MIDI silently never attaches. `pw-jack` points it at PipeWire's shim instead.
# Wrapping only the run keeps that LD_LIBRARY_PATH off the build toolchain.
# (The real fix is the flake shipping pipewire.jack rather than libjack2; when
# that lands, drop the wrapper.) --release is REQUIRED for real-time audio.
#
# Open the FTS desktop app straight to the keys rig (Worship profile, loaded)
keys log="/tmp/fts-keys.log":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p fasttrackstudio --features signal-keys-rig
    echo "logging to {{log}}"
    # The engine is a child with inherited stdio, so one tee captures the app
    # AND the engine it spawns — which is where the rig actually lives, and so
    # where anything worth debugging gets logged.
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' \
    RUST_LOG="${RUST_LOG:-info,signal_keys=debug,signal_sampler=debug,vox_core=warn,schema_deser=off}" \
        pw-jack ./target/release/fasttrackstudio --keys 2>&1 | tee "{{log}}"

# A terminal surface over the composition-tree presets, no GUI. This was
# `just keys` before the app grew a keys mode.
#
# Nord Stage-style keys TUI — play a preset from a MIDI keyboard
keys-tui preset="Nord Stage" midi="all":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p signal-keys --features pipewire --example keys_tui -- --preset "{{preset}}" --midi "{{midi}}"

# Keys rig integration test: open the rig headless, inject MIDI through the
# ALSA loopback, and assert the rig both saw the events and made sound
# (midi_recent + master_peak). Needs pipewire + the sample libraries; exits
# nonzero on a deaf or silent rig.
keys-test:
    cargo build --release -p signal-keys --example midi_probe
    PIPEWIRE_PROPS='{ application.name = FTS-KeysTest }' pw-jack ./target/release/examples/midi_probe

# Play the City Grand physically-modeled piano from a MIDI keyboard.
# Voice loads its param table from ~/.config/signal/city-grand/table.json
# (regenerate with `pm sweep` in research/piano-model).
# NOTE: --midi targets ONE port by name (substring). Do NOT use "all" here —
# it allocates an ALSA sequencer queue per port (~24 on this rig) and blows
# past ALSA's ~32-queue limit. `just piano <name>` picks a different keyboard.
piano midi="KONTROL":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p signal-keys --features pipewire --example keys_tui -- --preset "City Grand" --midi "{{midi}}"

# Play City Wurli — the vendored physically-modeled Wurlitzer 200A (openwurli,
# GPL, personal use) from a MIDI keyboard (TUI). Same ALSA-queue caveat as
# `just piano`: --midi targets ONE port by name, never "all".
wurli midi="KONTROL":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p signal-keys --features pipewire --example keys_tui -- --preset "City Wurli" --midi "{{midi}}"

# Play Cinematic Studio Strings — 1st Violins from a MIDI keyboard (TUI).
strings lib="" midi="all" artic="Leg" mic="Mix":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p signal-sampler --features pipewire --example strings_tui -- {{ if lib != "" { "--lib '" + lib + "'" } else { "" } }} --midi "{{midi}}" --artic "{{artic}}" --mic "{{mic}}"

# Open a saved rig by name (TUI with meters + patch switching).
# --release is REQUIRED for real-time (the vendored NAM C++ core xruns in
# unoptimized builds; the dev profile optimizes deps, release is safest).
rig name:
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p signal-sampler --features pipewire --example guitar_tui -- --rig "{{name}}"

# List audio devices + channel counts (find your interface name)
rig-devices:
    cargo run -p signal-sampler --example guitar_rig -- --list

# Configure + remember a rig's interface / channel / profile, e.g.:
#   just rig-setup "Guitar Rig" --input "Yamaha TF" --channel 3 --profile /path/to.styx
rig-setup name *args:
    cargo run --release -p signal-sampler --example guitar_rig -- --rig "{{name}}" {{args}} --write-config

# ── Website (apps/site → fasttrackstudio.app) ───────────────────────────

# Dev server for the website (dioxus, live reload)
site-serve:
    cd apps/site && dx serve --platform web

# Production web build of the website
site-build:
    cd apps/site && dx build --platform web --release

# ── Docs site (apps/docs-site → docs.fasttrackstudio.app) ───────────────
# kf docs (kf-block → SVG pre-render) + dodeca (`ddc`). See
# apps/docs-site/README.md.

# Build the unified docs site → apps/docs-site/output
docs-build:
    apps/docs-site/build.sh

# Docs dev loop — kf-block watcher + ddc live reload on :8080
docs-serve:
    apps/docs-site/serve.sh

# Build + deploy the docs site to fly.io (app: fts-docs)
docs-deploy:
    apps/docs-site/deploy.sh

# ── REAPER extension (apps/extensions/reaper-fts-extensions) ────────────
# The production REAPER extension stack (cdylib reaper_fts_extensions).
# Dev installs symlink the build + live-editable config into REAPER's
# resource dir; release packaging copies the .so into the tarball.
#
# Recipes live in the `reaper` module (reaper.just):
#   just reaper install     build + symlink the .so and config into $REAPER_HOME
#   just reaper build       build the release cdylib
#   just reaper uninstall   remove the symlink
#   just reaper log         tail the live extension log
mod reaper 'features/reaper/reaper.just'

# The UI snapshot regression gate (ui-snapshot) moved to the architect
# repo with architect-ui in the August 2026 split. Run it there:
#   just snapshot-check / snapshot-render <name> / snapshot-update

# REAPER integration tests moved into the `reaper` module:
#   just reaper integration-test        (was: just reaper-integration-test)
#   just reaper integration-test-gui
#   just reaper daw-test                (was: just daw-reaper-test)

# ── Build ────────────────────────────────────────────────────────────────

# Check the whole workspace compiles
check:
    cargo check --workspace

# Run tests. nextest: parallel per-test binaries, much faster than
# `cargo test` on this many crates. It does NOT run doctests — use
# `just test-doc` for those.
test:
    cargo nextest run --workspace

# Doctests only — nextest can't run them (libtest owns doctests).
test-doc:
    cargo test --workspace --doc

# ── Disk / build-time hygiene ────────────────────────────────────────────
# Cargo never garbage-collects target/: every rebuild with a changed
# fingerprint leaves the old artifact behind forever. Measured in this tree:
# 56 stale copies of a single crate, 77 G of `debug/incremental`, ~1 TB of
# target/ across the worktrees. These recipes are the GC cargo doesn't have.

# Reclaim stale artifacts in THIS worktree (keeps anything touched recently).
sweep days="7":
    cargo sweep --time {{days}}
    @du -sh target

# Sweep every worktree — the thing to run when the dev disk fills up.
# Uses `git worktree list` so new worktrees are picked up automatically.
sweep-all days="7":
    #!/usr/bin/env bash
    set -euo pipefail
    before=$(du -sc $(git worktree list --porcelain | awk '/^worktree /{print $2"/target"}') 2>/dev/null | tail -1 | cut -f1)
    for w in $(git worktree list --porcelain | awk '/^worktree /{print $2}'); do
      [ -d "$w/target" ] || continue
      echo "── sweeping $w"
      cargo sweep --time {{days}} "$w" || true
    done
    after=$(du -sc $(git worktree list --porcelain | awk '/^worktree /{print $2"/target"}') 2>/dev/null | tail -1 | cut -f1)
    echo "reclaimed $(( (before - after) / 1024 / 1024 )) GiB"

# Drop incremental-compilation caches everywhere. They are pure cache —
# safe to delete, costs one non-incremental rebuild. Was 77 G in main alone.
sweep-incremental:
    #!/usr/bin/env bash
    set -euo pipefail
    for w in $(git worktree list --porcelain | awk '/^worktree /{print $2}'); do
      rm -rf "$w"/target/*/incremental "$w"/target/incremental 2>/dev/null || true
    done
    echo "incremental caches cleared"

# Where is the disk actually going? Per-worktree target/ sizes, largest first.
disk:
    #!/usr/bin/env bash
    du -sh $(git worktree list --porcelain | awk '/^worktree /{print $2"/target"}') 2>/dev/null | sort -rh

# Why is the build slow? Writes target/cargo-timings/cargo-timing.html —
# a per-crate Gantt chart showing the critical path and link-time tail.
timings *ARGS:
    cargo build --timings {{ARGS}}
    @echo "→ target/cargo-timings/cargo-timing.html"

# ── Knowledge graph (graphify) ───────────────────────────────────────────
# Whole-repo knowledge graph for AI assistants — parses the tree with
# tree-sitter (100% local, no API calls) into graphify-out/ (graph.json +
# GRAPH_REPORT.md + interactive graph.html). graphify is bootstrapped in the
# nix dev shell (see flake.nix shellHook). Output is gitignored + regenerable;
# rebuild after large structural changes. `graph-serve` exposes it over MCP
# (wired into .mcp.json so Claude Code queries it instead of grepping cold).

# Build/refresh the repo knowledge graph (local AST + clustering, no LLM).
# --force so the graph shrinks when .graphifyignore excludes more (vendored
# trees); without it graphify refuses a rebuild that has fewer nodes.
graph:
    graphify update . --force

# Serve the knowledge graph over MCP (stdio) — used by .mcp.json
graph-serve:
    graphify-mcp --transport stdio --graph graphify-out/graph.json

# Rebuild the browser setlist player's AudioWorklet wasm bundle — a small
# RELEASE build of daw-standalone (the render graph that runs ON the audio
# thread; see the task repo's apps/web/assets/worklet/processor.js).
# wasm-bindgen-cli comes from the dev shell, pinned to the workspace
# wasm-bindgen version.
#
# CROSS-REPO since the August 2026 split: the source lives here, the built
# artifact is COMMITTED in the task repo (so plain `dx serve` / CI there
# need no extra step). Pass the path to your task checkout:
#
#   just task-worklet-wasm ../task
#
# Re-run after changing daw-standalone's audio/render/web code, then commit
# the result in the task repo.
task-worklet-wasm task_repo='../task':
    cargo build -p daw-standalone --lib \
        --target wasm32-unknown-unknown --release \
        --no-default-features --features decode,web
    test -d {{task_repo}}/apps/web/assets/worklet \
        || { echo "no worklet dir at {{task_repo}}/apps/web/assets/worklet — pass the task checkout path"; exit 1; }
    wasm-bindgen --target web --out-dir {{task_repo}}/apps/web/assets/worklet \
        --out-name daw_standalone \
        target/wasm32-unknown-unknown/release/daw_standalone.wasm

# ── CSS A/B (orchestral sampling) ────────────────────────────────────────

css_pack := '/run/media/AudioHaven/Signal/Libraries/Proxy/Orchestral/Cinematic Studio Strings/1st Violins/Legato/1st Violins - Legato - Mix.signalpack'

# Score the CSS legato engine against the real Kontakt reference render.
css-ab *ARGS:
    cargo build -p fts-cli --bin fts
    # Score the binary we just BUILT. score.py defaults to ./target/debug/fts,
    # so under a CARGO_TARGET_DIR override it would silently score a stale one.
    python3 features/sampler/signal-sampler/tests/css-ab/score.py \
        --fts "${CARGO_TARGET_DIR:-target}/debug/fts" \
        --pack {{quote(css_pack)}} --json scratch/css-ab/score.json {{ARGS}}

# Same, restricted to sections (e.g. `just css-ab-sections S10,S13`).
css-ab-sections SECTIONS:
    just css-ab --sections {{SECTIONS}}

# ── Expression editor ────────────────────────────────────────────────────

# The expression editor in a window you keep open, with rsx! hot reload.
# Blitz -> Vello -> winit via dioxus-native: the same renderer the VST3
# editor and the REAPER panel use, without the plugin windowing. Pick the
# file to edit inside the window; set EXPRESSION_EDITOR_LIBRARY to point
# the chooser at a folder of material.
#
# ⚠ RESTART IT AFTER A STRUCTURAL rsx! EDIT. Hot reload replaces the
# template in the running app, and dioxus's template diffing cannot
# handle a template whose *node count* changed — adding or removing an
# element, an `if` block, or a component. The next render walks a
# mutation path into a node that is no longer there and you get
#
#     blitz-dom/src/mutator.rs: invalid key        (node_at_path)
#
# which reads like a bug in whatever key you just pressed and is not:
# the same markup mounts fine from a cold start, and the headless
# suite — which has no hot reload — never sees it. Upstream knows the
# class (DioxusLabs/dioxus#3459, #3567); this is its Blitz spelling,
# because blitz-dom holds nodes in a slab and a stale index there says
# "invalid key" rather than "index out of bounds".
#
# Editing *values* — a colour, a size, a string — hot reloads fine. It
# is only shape. To rule it out entirely: `just ee-serve --hot-reload
# false`, or `just ee` for a one-shot window.
ee-serve *ARGS:
    dx serve -p expression-editor-standalone --example serve \
        --platform desktop --renderer native {{ARGS}}

# One-shot window on a file (no hot reload, no chooser).
ee SOURCE="phrase" *ARGS:
    cargo run -p expression-editor-standalone --example editor -- {{SOURCE}} {{ARGS}}

# The workstation: arrangement + TCP over the drum-mode editor, mixer
# down the right, audio out the default output. Defaults to the drum-
# mode reference session; pass any .rpp to open something else.
workstation SOURCE="/run/media/AudioHaven/Project/02 LORD OF THE FIGHT/02 LORD OF THE FIGHT.RPP" *ARGS:
    cargo run --release -p expression-editor-standalone --example workstation -- \
        "{{SOURCE}}" --drums --size 1920x1080 {{ARGS}}

# Repaint the visual-inspection PNGs into target/gui-shots/expression-editor.
#
# These are artefacts for a human to look at, not assertions, so they are
# #[ignore]d and never run in `cargo nextest run` — one of them paints ~49
# scenes through a software rasterizer for minutes, which starved the rest
# of the suite into timing out. Single-threaded on purpose: they are all
# CPU rasterization, so running them in parallel only makes each slower.
ee-shots *ARGS:
    cargo test -p expression-editor-ui --test screenshots \
        -- --ignored --test-threads 1 {{ARGS}}

# ── Aliases ──────────────────────────────────────────────────────────────

alias c := check
alias t := test
alias g := guitar
