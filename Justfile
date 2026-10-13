# FastTrackStudio — root workspace recipes
# Run commands: just <recipe-name>

# The session the DAW window opens.
#
# The drum template, not the orchestral stress fixture: it has the
# hierarchy a real desk has (Drums > Drum Kit > Kick > Sum > In/Out/Trig),
# the template's own colours, and it opens with the kick selected — so
# the focus-width rack is on screen before anything is clicked. The
# orchestral fixture is 2000 flat tracks; it exists to be slow, not to
# be looked at.
#
# It is the golden session — built by the Rust builder in
# `dynamic_template::golden_session` and committed under
# features/dynamic-template/fixtures/golden/. `just daw-template`
# regenerates it. Override for a one-off with an argument
# (`just studio-song some.rpp`) or for a session with FTS_DAW_TEMPLATE.
GOLDEN_DIR := "features/dynamic-template/fixtures/golden"
DAW_PROJECT := env("FTS_DAW_TEMPLATE", GOLDEN_DIR / "template.rpp")
DAW_VOCAL := env("FTS_DAW_VOCAL", GOLDEN_DIR / "vocal-fx.rpp")

# List recipes by default
default:
    @just --list

# ── Tailwind (app UI sheet) ──────────────────────────────────────────────
# The single compiled sheet (assets/tailwind-signal.css) is inlined by
# BOTH apps/session/desktop/src/rig_view.rs (signal UI) and
# src/main.rs SessionChrome (session UI) via include_str!; rebuild it
# whenever UI-crate class usage changes. input.css @source globs scan the
# signal/session UI crates + libs/ui + libs/dock.

# Point apps/session/desktop/.lumen-blocks at the lumen-blocks checkout
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
    ln -sfn "$dir" apps/session/desktop/.lumen-blocks

# Build Tailwind CSS (v4)
tailwind: _lumen-link
    cd apps/session/desktop && tailwindcss -i ./input.css -o ./assets/tailwind-signal.css --minify

# Watch Tailwind CSS for changes
tailwind-watch: _lumen-link
    cd apps/session/desktop && tailwindcss -i ./input.css -o ./assets/tailwind-signal.css --watch --minify

# Fail if the committed sheet isn't what the sources produce.
#
# tailwind-signal.css is `include_str!`d by rig_view.rs and main.rs, so it
# has to be committed — which means it can go stale silently when someone
# adds a class and doesn't rebuild. It was stale by ~50 classes when this
# check was written.
tailwind-check: tailwind
    #!/usr/bin/env bash
    set -euo pipefail
    if ! git diff --quiet -- apps/session/desktop/assets/tailwind-signal.css; then
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
# ws://:4040/vox): the session-desktop binary in --engine mode. `rigd`
# kept as an alias for muscle memory.
signal-engine:
    cargo run --release -p session-desktop -- --engine

alias rigd := signal-engine

# Launch the desktop app (Session and/or Charts, per default features) via
# dx serve, so UI edits hot-reload instead of a plain `cargo run`.
desktop:
    cd apps/session/desktop && dx serve --release

# Stage the fts web bundle (the browser remote) for embedding: tailwind →
# dx web build (signal rigs + the session setlist remote) →
# apps/session/desktop/web-dist/, which `cargo build -p session-desktop
# --features embed-web` compiles into the binary (include_dir). On wasm the
# session feature is only the wire surface (session-proto clients +
# session-ui); the player itself runs in the engine. web-dist/ is gitignored.
web-stage: tailwind
    cd apps/session/desktop && dx build --platform web --release --no-default-features --features signal,session
    rm -rf apps/session/desktop/web-dist
    cp -r target/dx/session-desktop/release/web/public apps/session/desktop/web-dist

# Stage the browser keys rig's AudioWorklet bundle (W4 of
# crates/signal/docs/browser-keys-rig.md) into the web bundle:
# a RELEASE wasm build of web-audio-worklet --features keys (KeysWorklet =
# WebRenderer + headless KeysRig), wasm-bindgen'd `--target web`, plus
# the keys-specific processor + worklet polyfill (AudioWorkletGlobalScope
# has no dynamic import(), TextDecoder, crypto, or performance — see
# features/audio/worklet/js/). The page (src/web_keys_rig.rs) expects:
#   /worklet/keys_processor.js
#   /worklet/signal_keys_worklet.js
#   /worklet/signal_keys_worklet_bg.wasm
# wasm-bindgen-cli comes from the dev shell, pinned to the workspace
# wasm-bindgen version (same as task-worklet-wasm).
keys-worklet-wasm out='apps/signal/desktop/web-dist/worklet':
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
    cargo build -p web-audio-worklet --features keys --lib \
        --target wasm32-unknown-unknown --release
    mkdir -p {{out}}
    wasm-bindgen --target web --out-dir {{out}} \
        --out-name signal_keys_worklet \
        target/wasm32-unknown-unknown/release/web_audio_worklet.wasm
    cp features/audio/worklet/js/keys_processor.js {{out}}/keys_processor.js
    cp features/audio/worklet/js/worklet_polyfill.js {{out}}/worklet_polyfill.js
    cp features/audio/worklet/js/keys_decoder_worker.js {{out}}/keys_decoder_worker.js
    cp features/audio/worklet/js/keys_streamer_worker.js {{out}}/keys_streamer_worker.js

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
keys-worklet-wasm-threads out='apps/signal/desktop/web-dist/worklet':
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
    cargo-nightly build -p web-audio-worklet --features keys --lib \
        -Z build-std=std,panic_abort \
        --target wasm32-unknown-unknown --release
    mkdir -p {{out}}
    wasm-bindgen --target web --out-dir {{out}} \
        --out-name signal_keys_worklet \
        target/wasm32-unknown-unknown/release/web_audio_worklet.wasm
    cp features/audio/worklet/js/keys_processor.js {{out}}/keys_processor.js
    cp features/audio/worklet/js/worklet_polyfill.js {{out}}/worklet_polyfill.js
    cp features/audio/worklet/js/keys_decoder_worker.js {{out}}/keys_decoder_worker.js
    cp features/audio/worklet/js/keys_streamer_worker.js {{out}}/keys_streamer_worker.js

# Stage the browser guitar rig (features/rigs/core/docs/browser-guitar-rig.md):
# one wasm module with two entry points — GuitarWorklet on the AudioWorklet
# thread, NamWorker on each NAM Web Worker — plus the processor, the worker,
# the page-side orchestrator and the worklet polyfill (shared with keys).
# The page expects, under {{out}}:
#   guitar_processor.js  nam_worker.js  guitar_rig.js  worklet_polyfill.js
#   signal_guitar_worklet.js  signal_guitar_worklet_bg.wasm
# +simd128: the NAM kernels are what decides how many models fit a quantum.
guitar-worklet-wasm out='apps/signal/desktop/web-dist/worklet/guitar':
    RUSTFLAGS="-C target-feature=+simd128" \
    cargo build -p web-audio-worklet --features guitar --lib \
        --target wasm32-unknown-unknown --release
    mkdir -p {{out}}
    wasm-bindgen --target web --out-dir {{out}} \
        --out-name signal_guitar_worklet \
        target/wasm32-unknown-unknown/release/web_audio_worklet.wasm
    cp features/audio/worklet/js/guitar_processor.js {{out}}/
    cp features/audio/worklet/js/nam_worker.js {{out}}/
    cp features/audio/worklet/js/guitar_rig.js {{out}}/
    cp features/audio/worklet/js/worklet_polyfill.js {{out}}/

# Try the browser guitar rig locally: stage the worklet bundle, export the
# rig (XDG_CONFIG_HOME picks the config), copy the reference DI, and serve
# the harness with the COOP/COEP headers SharedArrayBuffer needs:
#   http://127.0.0.1:8765/  (Start = the demo DI through the chosen patch)
# `node profile.mjs <profile> <patch>` in the dir prints each block's cost.
guitar-web-harness dir='target/guitar-web' port='8765':
    just guitar-worklet-wasm {{dir}}
    cargo run -q -p signal-cli -- rig web-bundle {{dir}}/rig
    cp features/audio/worklet/harness/index.html features/audio/worklet/harness/serve.py features/audio/worklet/harness/profile.mjs {{dir}}/
    cp "${XDG_CONFIG_HOME:-$HOME/.config}/signal/calibration/di-reference.wav" {{dir}}/ || echo "no di-reference.wav — the demo input needs one"
    python3 {{dir}}/serve.py {{port}}

# ONE engine binary serving the whole browser keys rig: stage the web
# bundle + keys worklet (web-stage), then embed it into the release
# binary. Order matters — embed-web include_dir!s web-dist/ at compile
# time, so staging runs first. Then:
#   target/release/session-desktop --engine     (binds 0.0.0.0:4040)
# and open http://<host>:4040/rigs/keys/worship (tailnet-reachable).
keys-web: web-stage
    cargo build --release -p session-desktop --features embed-web

# Playwright end-to-end suite for the browser keys rig (W5): spawns its
# own engine on a scratch port (SIGNAL_ENGINE_ADDR) and proves the rig
# makes SOUND in real chromium. Expects target/release/session-desktop
# to exist — build it with `just keys-web` first. Needs the real pack
# library (or FTS_PACK_LIBRARY pointing at one with the Worship proxies).
keys-web-e2e:
    cd apps/session/desktop/e2e && npm install --no-fund --no-audit && npx playwright test

# Build the RELEASE binary (web bundle EMBEDDED) and deploy the ONE
# artifact to ~/.local/lib/fts/session-desktop behind the signal-engine
# systemd user unit. The unit is installed but NOT enabled: the desktop
# app (or `systemctl --user start signal-engine`) is the on/off switch;
# while running, systemd restarts crashes in ~1s; an explicit stop is
# final. If the engine is running during deploy it restarts onto the new
# build, otherwise it stays stopped.
# Logs: `journalctl --user -u signal-engine`.
rig-install: web-stage
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p session-desktop --features embed-web
    install -d ~/.local/lib/fts
    install -m 755 target/release/session-desktop ~/.local/lib/fts/session-desktop.new
    mv -T ~/.local/lib/fts/session-desktop.new ~/.local/lib/fts/session-desktop
    # The pre-consolidation artifacts (signal-engine binary + signal-web
    # bundle) are superseded; leave any existing ones in place until the
    # new unit is confirmed, then clean by hand if desired.
    install -d ~/.config/systemd/user
    install -m 644 apps/session/desktop/systemd/signal-engine.service ~/.config/systemd/user/
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
    ln -sf ~/.local/lib/fts/session-desktop ~/.local/bin/session-desktop
    ln -sf ~/.local/lib/fts/fts ~/.local/bin/fts
    install -d ~/.local/share/icons/hicolor/scalable/apps
    install -m 644 apps/session/desktop/assets/icon.svg \
        ~/.local/share/icons/hicolor/scalable/apps/session-desktop.svg
    install -d ~/.local/share/applications
    sed "s|@BIN@|$HOME/.local/lib/fts/session-desktop|" \
        apps/session/desktop/assets/session-desktop.desktop \
        > ~/.local/share/applications/session-desktop.desktop
    update-desktop-database ~/.local/share/applications 2>/dev/null || true
    gtk-update-icon-cache ~/.local/share/icons/hicolor 2>/dev/null || true
    echo "installed: session-desktop + fts in ~/.local/bin, launcher entry ready"

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
    rm -f ~/.local/bin/session-desktop ~/.local/bin/fts
    rm -rf ~/.local/lib/fts
    rm -f ~/.local/share/applications/session-desktop.desktop
    rm -f ~/.local/share/icons/hicolor/scalable/apps/session-desktop.svg
    update-desktop-database ~/.local/share/applications 2>/dev/null || true
    gtk-update-icon-cache ~/.local/share/icons/hicolor 2>/dev/null || true
    echo "uninstalled (user data in ~/.config/fts and ~/.config/signal kept)"

# Session.app + its macOS .pkg installer (signed with the Developer ID
# identities in the login keychain), built with the host toolchain — see the
# script's header for the knobs (NOTARIZE=1, ADHOC_SIGN=1, MAC_TARGETS=...).
macos-pkg:
    bash apps/session/desktop/ios/package-session-macos.sh

# Build the installer and install it for this user (~/Applications, no
# password), replacing any earlier install.
macos-install: macos-pkg
    installer -pkg "target/Session-$(cargo pkgid -p session-desktop | sed 's/.*[#@]//')-macos.pkg" -target CurrentUserHomeDirectory

# ── Release packaging ────────────────────────────────────────────────────
# Assemble the distributable release artifacts into dist/ (what a
# codeberg release carries, and what fts-installer downloads):
#   session-desktop-v<ver>-x86_64-linux.tar.gz   app + fts CLI + systemd
#       unit + desktop/icon templates + install.sh/uninstall.sh + VERSION
#   fts-installer-x86_64-linux                    standalone installer
#   SHA256SUMS                                    covers both
# Binary copies are patchelf'd to the standard /lib64 loader so they run
# outside the nix shell (target machines still need the shared libs —
# see `ldd` on the packaged binary).
release-package: web-stage
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p session-desktop --features embed-web
    cargo build --release -p fts-cli
    cargo build --release -p fts-installer
    cargo build --release -p fts-extensions
    version="$(cargo pkgid -p session-desktop | sed 's/.*[#@]//')"
    plat=x86_64-linux
    if command -v patchelf >/dev/null; then PATCHELF=(patchelf); else PATCHELF=(nix shell nixpkgs#patchelf -c patchelf); fi
    stage="$(mktemp -d)"; trap 'rm -rf "$stage"' EXIT
    cp target/release/session-desktop target/release/fts "$stage"/
    cp target/release/fts-installer "$stage/fts-installer-$plat"
    for b in session-desktop fts "fts-installer-$plat"; do
        "${PATCHELF[@]}" --set-interpreter /lib64/ld-linux-x86-64.so.2 --remove-rpath "$stage/$b"
        strip "$stage/$b"
    done
    # REAPER extension cdylib: no interpreter to patch (shared lib), just
    # rpath + symbols. install.sh drops it into ~/.config/REAPER/UserPlugins
    # when a REAPER install is present.
    cp target/release/libreaper_fts_extensions.so "$stage/reaper_fts_extensions.so"
    "${PATCHELF[@]}" --remove-rpath "$stage/reaper_fts_extensions.so"
    strip "$stage/reaper_fts_extensions.so"
    cp apps/session/desktop/systemd/signal-engine.service "$stage"/
    cp apps/session/desktop/assets/session-desktop.desktop "$stage"/
    cp apps/session/desktop/assets/icon.svg "$stage/icon.svg"
    install -m 755 apps/installer/scripts/install.sh apps/installer/scripts/uninstall.sh "$stage"/
    printf '%s\n' "$version" > "$stage/VERSION"
    mkdir -p dist
    tarball="session-desktop-v$version-$plat.tar.gz"
    tar -czf "dist/$tarball" -C "$stage" \
        session-desktop fts reaper_fts_extensions.so \
        signal-engine.service session-desktop.desktop \
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
        cargo run -q -p fts-plugin-xtask -- "$cmd" -p "$p-plugin" --release --features export
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
# apps/session/desktop/ios/deploy-macos-plugins.sh, since Apple only accepts
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
    cargo build --release -p session-desktop --features signal-keys-rig
    echo "logging to {{log}}"
    # The engine is a child with inherited stdio, so one tee captures the app
    # AND the engine it spawns — which is where the rig actually lives, and so
    # where anything worth debugging gets logged.
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' \
    RUST_LOG="${RUST_LOG:-info,signal_keys=debug,signal_sampler=debug,vox_core=warn,schema_deser=off}" \
        pw-jack ./target/release/session-desktop --keys 2>&1 | tee "{{log}}"

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

# ── REAPER extension ─────────────────────────────────────────────────────
# `apps/extensions/reaper-fts-extensions` (the production cdylib this
# module's recipes and `release-package` below build) never made it into
# this repo's August 2026 split -- it's signal-domain and lives in the
# signal repo now. `features/adapters/reaper/reaper.just` was never committed here
# either, so `mod reaper` broke every `just` invocation in this repo.
# Disabled until this repo has its own extension to wire up (the
# test-only `session-extension` crate at features/adapters/reaper/session-extension
# has no recipes of its own yet).
#
# mod reaper 'features/adapters/reaper/reaper.just'

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
# thread; see the task repo's apps/session/web/assets/worklet/processor.js).
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
    python3 features/rigs/sampler/signal-sampler/tests/css-ab/score.py \
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

# ── apps/session/web (session.fasttrackstudio.app) ────────────────────────────────
# `apps/session/web/tailwind.css` @sources this crate plus architect-ui, a GIT DEP
# with no stable path on disk — the glob cannot be written literally.
# `cargo metadata` knows where cargo actually resolved it; this recipe asks,
# and symlinks the answer into apps/session/web/.tailwind-src/. Without it the
# classes architect-ui's components use are simply absent from the sheet,
# and the failure is SILENT (a `@source` matching nothing is not an error —
# see keyflow's apps/session/web/tailwind.css, the same pattern this mirrors).
_web-tw-link:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p apps/session/web/.tailwind-src
    dir=$(cargo metadata --format-version 1 2>/dev/null \
        | python3 -c "import json,sys,os;p=json.load(sys.stdin)['packages'];print(next(os.path.dirname(x['manifest_path']) for x in p if x['name']=='architect-ui'))")
    if [ -z "$dir" ] || [ ! -d "$dir" ]; then
        echo "cannot resolve architect-ui — is it in the dependency graph?" >&2
        exit 1
    fi
    ln -sfn "$dir" apps/session/web/.tailwind-src/architect-ui

# Compile the site's Tailwind sheet. Gitignored output; `asset!()` needs it
# at compile time, so this runs before any build of apps/web.
web-tailwind: _web-tw-link
    cd apps/session/web && tailwindcss -i ./tailwind.css -o ./assets/tailwind.css --minify

# Serve apps/session/web with hot reload.
web: web-tailwind
    cd apps/session/web && dx serve --platform web

# Build the shipping web bundle into target/dx/session-web/release/web/public,
# with the guide pre-rendered into it.
#
# `--ssg` builds the app's server as well as its client, runs it, asks it
# for `static_routes` and requests each — which writes the guide's routes
# to disk as finished HTML. Nothing deploys that server.
#
# `--fullstack` because dx decides whether to build a server from the
# CLIENT's features, and `dioxus/fullstack` is on the `server` feature
# alone here (its reqwest would be a second major in the wasm binary —
# see apps/session/web/Cargo.toml). Without it there is no server target and
# `--ssg` silently does nothing.
#
# `--force-sequential` because the pre-render borrows
# `public/index.html` as its page shell and the CLIENT build writes that
# file; in parallel the pages can come out in Dioxus's bare fallback
# shell — no title, no charset, no hydration — with the build still
# reporting success. (dioxus#3518.)
#
# The `rm` because the renderer's cache is `clear_cache(false)` (it must
# be — the cache directory IS the bundle), so a route already in it is
# served rather than re-rendered and a rebuild ships the old html.
web-build: web-tailwind
    rm -rf target/dx/session-web/release/web/public
    cd apps/session/web && dx build --platform web --release \
        --ssg --fullstack --force-sequential

# Type-check apps/session/web against the actual deploy target.
web-check:
    cargo check -p session-web --target wasm32-unknown-unknown

# ── Aliases ──────────────────────────────────────────────────────────────

alias c := check
alias t := test
alias g := guitar

# The workstation on a real song, served: `rsx!` edits hot-reload into the
# running window. SONG: set-in-stone or unbreakable.
# Override the source with EXPRESSION_EDITOR_PRACTICE_ALBUM; TMPDIR controls copies.
ee-practice $SONG="set-in-stone" $FRESH="false":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "$SONG" == "both" ]]; then
        echo 'Open one song per window: just ee-practice set-in-stone / just ee-practice unbreakable' >&2
        exit 2
    fi
    # Reuse the staging by default. It is still a copy — originals stay
    # out of every write path — but it is ONE copy: this used to stage a
    # fresh 5.6 GB per invocation, so an afternoon of opening the window
    # buried the disk and threw away the `.reapeaks` sidecars each time,
    # making every start slow as well. `just ee-practice set-in-stone
    # true` stages a throwaway copy when you want to start from the
    # record as recorded.
    staging=(--cached)
    if [[ "$FRESH" == "true" ]]; then staging=(); fi
    project=$(cargo run -p expression-editor-standalone --example practice -- "${staging[@]}" "$SONG")
    # Served, so `rsx!` edits hot-reload into the running window. `dx`
    # owns argv — it has --cargo-args and --rustc-args but nothing that
    # reaches the app — so the project goes through the environment
    # instead; see `Args::from_env`.
    # Everything the window says also lands in a file, so a warning that
    # scrolls past — or an agent that cannot see your terminal — still has
    # it. `tee` keeps it on screen too.
    mkdir -p target
    EXPRESSION_EDITOR_ARGS="'$project' --drums --size 1600x900" \
        dx serve -p expression-editor-standalone --example workstation \
        --platform desktop --renderer native 2>&1 | tee target/ee-practice.log

# The same workstation in a WRY WebView (dioxus-desktop) instead of Blitz.
#
# The renderer the Session desktop app ships on, so this is where the
# panels get designed: real CSS, devtools, and a DOM that does not mind a
# pane adding and removing nodes. Same project staging as `ee-practice` —
# reused, not re-copied — and everything below the UI is unchanged and
# native: the project loads, the daw facade runs in-process, audio plays.
#
# The drum stack draws as SVG markup here rather than a painted scene —
# a browser engine is fast at exactly those elements and Blitz is not.
# See `expression_editor_ui::stack::markup`.
#
# The window reports its own frame rate to the log a couple of times a
# second (`ui.fps`, `ui.worst_frame_ms`), so a scroll can be measured
# from a terminal instead of a screenshot: `just ee-fps`.
#
# `PROBE=1` turns the dioxus-mcp probe on for `runtime_events`. It is off
# by default because it is not free: it records every `dioxus_core` trace
# event, fields and all, and those fields are whole `VNode` trees —
# 18,000 events a second with the window merely playing back. Turn it on
# to inspect renders, not to measure them.
ee-webview $SONG="set-in-stone" $FRESH="false" $PROBE="0":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "$SONG" == "both" ]]; then
        echo 'Open one song per window: just ee-webview set-in-stone / just ee-webview unbreakable' >&2
        exit 2
    fi
    staging=(--cached)
    if [[ "$FRESH" == "true" ]]; then staging=(); fi
    project=$(cargo run -p expression-editor-standalone --example practice -- "${staging[@]}" "$SONG")
    # Served too, and this is the one where it pays most: a WebView has
    # devtools, so a hot-reloaded `rsx!` edit can be inspected as it
    # lands.
    mkdir -p target
    FTS_DIOXUS_PROBE="$PROBE" \
    RUST_LOG="${RUST_LOG:-warn,expression_editor_ui::frame_meter=info}" \
    ${WEBKIT_DISABLE_COMPOSITING_MODE:+WEBKIT_DISABLE_COMPOSITING_MODE="$WEBKIT_DISABLE_COMPOSITING_MODE"} \
    EXPRESSION_EDITOR_ARGS="'$project' --drums --size 1600x900" \
        dx serve -p expression-editor-standalone --example webview \
        --platform desktop --features webview 2>&1 | tee target/ee-webview.log

# Drive the WebView window on a private display and measure a scroll.
#
# The developer's own window is a native Wayland surface: `grim` is
# refused by the compositor and `xdotool` cannot see it, so there is no
# way to drive or capture it. This gives the window a display of its own
# — Xvfb, a window manager, xdotool input, `import` capture — the same
# shape as `daw::test::VirtualDisplay`, which is how the REAPER panels
# are already tested.
#
# Xvfb is NOT a GPU. Numbers here are for an A/B under a fixed
# environment, not for quoting as what the window does on a screen.
# See scripts/ui-stress/GUIDE.md.
ee-vdisplay $CMD="run" $NAME="run":
    scripts/ui-stress/webview-display.sh "$CMD" "$NAME"

# Stop the private display and anything left running on it.
ee-vdisplay-stop:
    #!/usr/bin/env bash
    pkill -9 -x webview 2>/dev/null || true
    scripts/ui-stress/webview-display.sh stop

# The frame rate the WebView actually presented at, worst frame first.
#
# Reads the `ui.fps` lines the window logs. Scroll for a few seconds,
# then run this: the interesting number is the low end of the range and
# the worst frame, not the mean of a window that spent most of its time
# idle.
ee-fps $LINES="20":
    #!/usr/bin/env bash
    set -euo pipefail
    log=target/ee-webview.log
    if [[ ! -f "$log" ]]; then echo "no $log — run just ee-webview first" >&2; exit 1; fi
    sed -E 's/\x1b\[[0-9;]*[a-zA-Z]//g' "$log" \
      | grep -o 'ui\.fps=[0-9.]*  *ui\.worst_frame_ms=[0-9.]*' \
      | sed -E 's/ui\.fps=([0-9.]*)  *ui\.worst_frame_ms=([0-9.]*)/\1 \2/' \
      | awk '{ printf "%6.1f fps   worst %6.1f ms\n", $1, $2 }' \
      | tail -n "$LINES"

# Prepare self-contained projects without opening a window; prints their paths.
# Reuses the shared staging; pass FRESH=true for a throwaway copy.
ee-practice-prepare $SONG="both" $FRESH="false":
    #!/usr/bin/env bash
    set -euo pipefail
    staging=(--cached)
    if [[ "$FRESH" == "true" ]]; then staging=(); fi
    cargo run -p expression-editor-standalone --example practice -- "${staging[@]}" "$SONG"

# Real-song regression: copy, load, split, undo/redo, save/reopen, verify originals.
ee-practice-test:
    cargo test -p expression-editor-standalone --test practice_real -- --ignored --nocapture --test-threads=1

# Fast iteration loop: the SAME cached song/audio staging every run, fewer
# frames, optionally one phase. Prints the report directory for compare.py.
#
# `ee-stress` is the recipe of record — a fresh copy, all eight phases,
# 120 frames. This one trades that isolation for a minute-long loop: it
# reuses one staging (nothing here writes to it) so the `.reapeaks`
# sidecars stay warm, and it defaults to 40 frames, which is a signal,
# not a result. Confirm anything you intend to report with `ee-stress`.
ee-bench $SONG="set-in-stone" $FRAMES="120" $PHASE="all_panels" $PROFILE="release":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "$SONG" == "both" ]]; then echo 'Run one song per benchmark: set-in-stone or unbreakable' >&2; exit 2; fi
    cargo build -p expression-editor-standalone --example stress --example practice --profile "$PROFILE"
    artifact_root="${CARGO_TARGET_DIR:-target}"
    artifact_profile="$PROFILE"
    if [[ "$PROFILE" == "dev" ]]; then artifact_profile=debug; fi
    project=$("$artifact_root/$artifact_profile/examples/practice" --cached "$SONG")
    report_root=$(mktemp -d -t fts-ui-bench-XXXXXX)
    printf 'Practice project: %s\nReport directory: %s\n' "$project" "$report_root"
    RUST_BACKTRACE=1 FTS_STRESS_FRAMES="$FRAMES" FTS_STRESS_PROFILE="$PROFILE" FTS_STRESS_PHASE="$PHASE" \
      "$artifact_root/$artifact_profile/examples/stress" "$project" --drums --size 1600x900 --out "$report_root" 2>&1 | tee "$report_root/run.log"
    python3 scripts/ui-stress/run.py "$report_root"

# What the last `ee-practice` / `ee-webview` run said, worst first.
#
# The window's own log, summarised: repeated warnings collapsed to one
# line and a count, so a thousand copies of the same message read as one
# fact rather than a wall. Point an agent at this rather than pasting a
# scrollback.
ee-log $WHICH="webview" $LINES="40":
    #!/usr/bin/env bash
    set -euo pipefail
    log="target/ee-$WHICH.log"
    if [[ ! -f "$log" ]]; then echo "no $log — run just ee-$WHICH first" >&2; exit 1; fi
    echo "── $log ($(wc -l < "$log") lines) ──"
    echo
    echo "REPEATED (count, message):"
    # Strip timestamps and ANSI so identical messages actually collapse.
    sed -E 's/\x1b\[[0-9;]*[a-zA-Z]//g; s/^[0-9]{2}:[0-9]{2}:[0-9]{2}//; s/^\s*\[[a-z]+\]\s*//' "$log" \
        | grep -aE "WARN|ERROR|panic" | sort | uniq -c | sort -rn | head -15 || echo "  (none)"
    echo
    echo "LAST $LINES LINES:"
    tail -n "$LINES" "$log"

# Two benchmark runs side by side, with their load averages.
ee-bench-compare BASELINE CANDIDATE:
    python3 scripts/ui-stress/compare.py {{BASELINE}} {{CANDIDATE}}

# Headless full-workstation DOM stress on fresh song/audio copies.
ee-stress $SONG="set-in-stone" $FRAMES="120" $PROFILE="dev" $ENFORCE="false":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "$SONG" == "both" ]]; then echo 'Run one song per benchmark: set-in-stone or unbreakable' >&2; exit 2; fi
    cargo build -p expression-editor-standalone --example stress --example practice --profile "$PROFILE"
    artifact_root="${CARGO_TARGET_DIR:-target}"
    artifact_profile="$PROFILE"
    if [[ "$PROFILE" == "dev" ]]; then artifact_profile=debug; fi
    project=$("$artifact_root/$artifact_profile/examples/practice" "$SONG")
    report_root=$(mktemp -d -t fts-ui-stress-XXXXXX)
    options=()
    if [[ "$ENFORCE" == "true" ]]; then options+=(--enforce); fi
    printf 'Practice project: %s\nReport directory: %s\n' "$project" "$report_root"
    RUST_BACKTRACE=1 FTS_STRESS_FRAMES="$FRAMES" FTS_STRESS_PROFILE="$PROFILE" "$artifact_root/$artifact_profile/examples/stress" "$project" --drums --size 1600x900 --out "$report_root" 2>&1 | tee "$report_root/run.log"
    python3 scripts/ui-stress/run.py "$report_root" "${options[@]}"

# The orchestral test fixture: 2,000 tracks, 20,000 items, no media.
#
# The standard load for anything performance-related. Empty MIDI takes,
# so it opens instantly and needs no 5.6GB staging — what it reproduces
# is the SHAPE of a big template, which is what the renderer pays for.
# Confirm anything surprising against a real session before believing it.
daw-fixture TRACKS="2000" ITEMS="20000":
    #!/usr/bin/env bash
    set -euo pipefail
    out="${FTS_DAW_FIXTURE:-/tmp/fts-orchestral.rpp}"
    scripts/ui-stress/make-synthetic-rpp.py {{TRACKS}} {{ITEMS}} > "$out"
    printf 'wrote %s — %s tracks, %s items, %s\n' "$out" \
        "$(grep -c '^  <TRACK' "$out")" "$(grep -c '^    <ITEM' "$out")" \
        "$(du -h "$out" | cut -f1)"

# A session shaped like the dynamic template builds one.
#
# Small and DEEP, where the orchestral fixture is large and flat:
# `Drums > Drum Kit > Kick > SUM > {In, Out, Trig}` is five levels before
# a single audio track. The flat fixture never nests past one, so it
# cannot show whether the panel draws a folder structure at all.
#
# The hierarchy and the names come from features/dynamic-template's own
# group definitions rather than being invented here: the builder walks
# the config-derived template tree with a fixture song shape
# (`dynamic_template::golden_session`), and the result is the golden
# session. This regenerates the committed project files
# (template.rpp, vocal-fx.rpp), the uncommitted synthetic media beside
# them, and the checklist in docs/spec/session/maximal-template.md —
# which is regenerated from the golden-rule checks, never hand-ticked.
# A test fails when any of the committed text is stale.
daw-template:
    cargo run -p dynamic-template --bin golden-session -- "{{GOLDEN_DIR}}"

# Re-render every scene's committed picture from the golden session.
#
# The pictures under features/dynamic-template/fixtures/golden/scenes/
# are test fixtures (`apps/session/daw/tests/golden_scenes.rs`): a
# change to what a scene shows is a diff in a PR. Run this after a
# deliberate scene or template change and commit the result.
daw-scenes:
    #!/usr/bin/env bash
    set -euo pipefail
    # Both halves of a scene fixture: the row list (byte-exact, the half
    # that carries the decision) and the picture (structural, because
    # the runner and this box disagree about antialiasing). Refreshing
    # one without the other is how a fixture goes stale.
    export FTS_UPDATE_GOLDEN=1
    cargo test -p dynamic-template --test scene_rows
    cargo test -p session-daw --test golden_scenes
    # The generated scenes' pictures (drum-mixing and its kin) are not
    # compared — the negative control reads two of them, and the app's
    # Mix view is checked against drum-mixing by eye — so the test above
    # does not refresh them. Re-render each one committed, the same way,
    # so they are of the template as it is now rather than as it was.
    cargo build -q --release -p session-daw --bin bench
    for png in "{{GOLDEN_DIR}}"/scenes/*.png; do
        slug=$(basename "$png" .png)
        case "$slug" in
            *-tracking|*-tracking-overview|*-comping|*-editing|*-mixing) ;;
            *) continue ;;
        esac
        project="{{GOLDEN_DIR}}/template.rpp"
        [[ "$slug" == lead-vocal* ]] && project="{{GOLDEN_DIR}}/vocal-fx.rpp"
        FTS_BENCH_SCENE="$slug" FTS_BENCH_MIXER="$png" FTS_BENCH_ROWS="{{GOLDEN_DIR}}/scenes/$slug.rows" \
            FTS_BENCH_SIZE=2560x1440 ./target/release/bench "$project" >/dev/null
        echo "rendered $slug"
    done

# Re-render the folder-item fixtures from the golden session's own peaks.
#
# Both halves, at three zooms, under
# features/dynamic-template/fixtures/golden/folder-items/ — the fold
# (`<slug>.fold`, compared byte for byte) and the picture (`<slug>.png`,
# compared structurally). See `apps/session/daw/tests/folder_items.rs`.
# Run this after a deliberate change to the fold, the colours or the
# fixture media, and commit the result.
daw-folder-items:
    FTS_UPDATE_GOLDEN=1 cargo test -p session-daw --test folder_items

# One sheet of folder items over a project, by hand — the same render the
# fixtures come from, so it is the way to LOOK at a fold.
#
# `just daw-folder-item-sheet /tmp/fi.png "" 0,16` for eight bars.
daw-folder-item-sheet OUT="/tmp/fts-folder-items.png" PROJECT="" WINDOW="" SIZE="1280x480":
    #!/usr/bin/env bash
    set -euo pipefail
    project="{{PROJECT}}"
    if [[ -z "$project" ]]; then
        project="{{GOLDEN_DIR}}/template.rpp"
        [[ -f "$project" ]] || just daw-template
    fi
    cargo build -p session-daw --bin bench 2>&1 | grep -E '^error' -A6 || true
    window="{{WINDOW}}"
    # No window given means the whole project.
    export FTS_BENCH_WINDOW="$window"
    [[ -n "$window" ]] || unset FTS_BENCH_WINDOW
    FTS_BENCH_FOLDER_ITEMS="{{OUT}}" FTS_BENCH_SIZE="{{SIZE}}" \
        ./target/debug/bench "$project" 2>&1 | grep -viE 'vulkan|objects:|WARN|Fontconfig'

# Benchmark the arrangement HEADLESSLY: no window, no surface, no vsync.
#
# Sweeps both axes hard and reports percentiles. This is the number that
# says how much ROOM is left — the windowed build is pinned to whatever
# display it opens on (240fps on the 240Hz panel, 180 on the 180Hz one),
# which answers "does it keep up" and nothing else.
#
# No compositor and no present here, so treat it as the upper bound on
# drawing alone. Runs over ssh, on a headless box, or in CI.
daw-bench PROJECT="" SIZE="5120x1440":
    #!/usr/bin/env bash
    set -euo pipefail
    project="{{PROJECT}}"
    if [[ -z "$project" ]]; then
        project="${FTS_DAW_FIXTURE:-/tmp/fts-orchestral.rpp}"
        [[ -f "$project" ]] || just daw-fixture
    fi
    cargo build --release -p session-daw --bin bench 2>&1 | grep -E '^error' -A6 || true
    echo "load before: $(cut -d' ' -f1-3 /proc/loadavg)"
    FTS_BENCH_SIZE="{{SIZE}}" ./target/release/bench "$project" 2>&1 | grep -viE 'vulkan|objects:|WARN'

# The master workflow checklist (docs/spec/session/workflows.md): which
# flows have an implementation and a test, and which are still open —
# and the gate CI runs (checks.yml, "Flow verification gate"): a
# `flow.*` rule with an r[impl] and no r[verify] fails; open rules
# (neither) are counted, never failing. Same script here and in CI;
# `-v` lists every rule by state. tracey comes from the dev shell
# (nix/modules/tracey.nix). Exemptions, with a reason, go in
# .config/tracey/flow-verify-grandfathered.txt.
daw-flows *ARGS:
    python3 scripts/tracey-flow-gate.py {{ARGS}}

# The studio benchmark: a 5120x1440 arrangement with the expression
# editor docked under it, and a 2560x1440 mixer on a second display,
# both drawn every frame. The verdict is against 240 Hz for the pair.
daw-studio PROJECT="" SIZE="5120x1440" MIXER="2560x1440":
    #!/usr/bin/env bash
    set -euo pipefail
    project="{{PROJECT}}"
    if [[ -z "$project" ]]; then
        project="${FTS_DAW_FIXTURE:-/tmp/fts-orchestral.rpp}"
        [[ -f "$project" ]] || just daw-fixture
    fi
    cargo build --release -p session-daw --bin bench 2>&1 | grep -E '^error' -A6 || true
    echo "load before: $(cut -d' ' -f1-3 /proc/loadavg)"
    FTS_BENCH_STUDIO=1 FTS_BENCH_SIZE="{{SIZE}}" FTS_BENCH_MIXER_SIZE="{{MIXER}}" \
        ./target/release/bench "$project" 2>&1 | grep -viE 'vulkan|objects:|WARN|Fontconfig'

# The arrangement with the editor docked under it, as a PNG.
daw-dock OUT="/tmp/fts-dock.png" PROJECT="" SIZE="2560x1440":
    #!/usr/bin/env bash
    set -euo pipefail
    project="{{PROJECT}}"
    if [[ -z "$project" ]]; then
        project="{{DAW_PROJECT}}"
        [[ -f "$project" ]] || just daw-template
    fi
    cargo build --release -p session-daw --bin bench 2>&1 | grep -E '^error' -A6 || true
    FTS_BENCH_DOCK="{{OUT}}" FTS_BENCH_SIZE="{{SIZE}}" \
        ./target/release/bench "$project" 2>&1 | grep -viE 'vulkan|objects:|WARN|Fontconfig'

# The audio drum workflow: a tracked kit stacked as role lanes with a
# song's worth of hits, as a PNG. BARS sets how much groove.
daw-kit OUT="/tmp/fts-kit.png" SIZE="2560x900" BARS="200":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p session-daw --bin bench 2>&1 | grep -E '^error' -A6 || true
    FTS_BENCH_KIT="{{OUT}}" FTS_BENCH_SIZE="{{SIZE}}" FTS_BENCH_BARS="{{BARS}}" \
        ./target/release/bench /dev/null 2>&1 | grep -viE 'vulkan|objects:|WARN|Fontconfig'

# The expression editor over the demo drum groove, as a PNG — the view
# `e` opens in the window with nothing selected, painted headless.
daw-expression OUT="/tmp/fts-expression.png" SIZE="1600x900":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p session-daw --bin bench 2>&1 | grep -E '^error' -A6 || true
    FTS_BENCH_EXPRESSION="{{OUT}}" FTS_BENCH_SIZE="{{SIZE}}" \
        ./target/release/bench /dev/null 2>&1 | grep -viE 'vulkan|objects:|WARN|Fontconfig'

# The Patch List view's render fixture: the fixture album resolved
# against the fixture room, written where its test reads it. Run it when
# a deliberate change to the view has moved a pixel, and commit the PNG.
daw-patch-list OUT="apps/session/daw/fixtures/patch-list.png" SIZE="1280x1280":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p session-daw --bin bench 2>&1 | grep -E '^error' -A6 || true
    FTS_BENCH_PATCH_LIST="{{OUT}}" FTS_BENCH_SIZE="{{SIZE}}" \
        ./target/release/bench /dev/null 2>&1 | grep -viE 'vulkan|objects:|WARN|Fontconfig'

# The Patch List view's second render fixture: the same fixture album
# with a session override on Cody's DI and a stale banner (#57). Run it
# when a deliberate change has moved a pixel, and commit the PNG.
daw-patch-list-overridden-stale OUT="apps/session/daw/fixtures/patch-list-overridden-stale.png" SIZE="1280x1280":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p session-daw --bin bench 2>&1 | grep -E '^error' -A6 || true
    FTS_BENCH_PATCH_LIST_OVERRIDDEN_STALE="{{OUT}}" FTS_BENCH_SIZE="{{SIZE}}" \
        ./target/release/bench /dev/null 2>&1 | grep -viE 'vulkan|objects:|WARN|Fontconfig'

# One scene of the visual track manager, as a PNG.
#
# `just daw-scene lead-vocal-fx` renders the vocal template with the
# Short delay and the Long verb in focus; the drum scenes render the
# drum template. Scenes are the data table in
# `dynamic_template::scenes::table`: drum-tracking, drum-mixing,
# drum-overview, drum-advanced, drum-fx, buses, guitar-fx, lead-vocal,
# lead-vocal-fx. In the window the number keys recall the scenes of the
# current DAW mode, and a digit past the end clears the scene.
daw-scene SCENE="lead-vocal-fx" OUT="" SIZE="2560x1440":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{SCENE}}" in
        drum-*|guitar-*|buses) project="{{DAW_PROJECT}}" ;;
        *)                     project="{{DAW_VOCAL}}" ;;
    esac
    [[ -f "$project" ]] || just daw-template
    out="{{OUT}}"; [[ -n "$out" ]] || out="/tmp/fts-scene-{{SCENE}}.png"
    cargo build --release -p session-daw --bin bench 2>&1 | grep -E '^error' -A6 || true
    FTS_BENCH_MIXER="$out" FTS_BENCH_SCENE="{{SCENE}}" FTS_BENCH_SIZE="{{SIZE}}" \
        ./target/release/bench "$project" 2>&1 | grep -viE 'vulkan|objects:|WARN'

# Open the studio — the ruler, the panel and the arrangement painted as
# ONE node, in a real window on dioxus-native.
#
#   just studio                    the golden session, drivable by hand
#   just studio animate            running the benchmark's own gestures
#   just studio "" 2560x1440       at another size
#   FPS=1 just studio              with the frame-time graph over it
#
# The app's own panels (DawPanels), with the app's hands: wheel scrolls,
# shift makes it sideways; hold `z` and scroll to zoom time, shift-`z` for
# the rows; `z` and drag is the zoom tool; middle-drag is the hand; `x`
# docks the mixer. `FPS=1` draws what a frame cost — the shell's own
# resolve-encode-present, not the gap between redraws.
#
# `FPS=1` puts a hundred-frame bar graph in the bottom right, with the
# 4.17 ms budget drawn across it, so a hitch is visible as a hitch rather
# than averaged into a number that looks fine.

#
# Presented WITHOUT vsync, and `VSYNC=1` puts it back. Not a default
# chosen for speed: measured on the golden session at 5120x1440, the
# animated gestures went p50 10.1ms with vsync and 6.8ms without, which
# is not the frame getting cheaper — it is the frame having missed a
# deadline and waiting for the next one. A window that waits is a window
# whose readout reports the wait, and the number this is being tuned
# against has to be what a frame COST.
#
# Logs to /tmp/fts-studio.log rather than to the terminal, because a
# window has no terminal and what a run did has to be readable afterwards.
studio MODE="1" SIZE="5120x1440" SCENE="drum-mixing":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --profile release-fast -p session-daw --bin blitz_shot
    # Through `env`, not as a bare `VAR=x` prefix: bash decides what is
    # an assignment BEFORE it expands anything, so `${FPS:+FTS_BLITZ_FPS=1}`
    # in that position becomes a command name and the recipe dies with
    # "command not found".
    env ${FPS:+FTS_BLITZ_FPS=1} \
    FTS_PRESENT="${VSYNC:+vsync}${VSYNC:-immediate}" \
    FTS_BLITZ_WINDOW="{{MODE}}" \
    FTS_BLITZ_SIZE="{{SIZE}}" \
    FTS_BLITZ_SCENE="{{SCENE}}" \
    FTS_BLITZ_LOG=/tmp/fts-studio.log \
    ./target/release-fast/blitz_shot "{{GOLDEN_DIR}}/template.rpp" /tmp/fts-studio.png

# The studio, on the golden session.
#
#   just studio-demo               your hands on it
#   just studio-demo --animate     driving itself
#
# Hands-on by default, because that is what opening it is usually for.
# `--animate` runs the benchmark's own gestures on screen: the same tree,
# the same session and the same numbers as `just studio-bench`, so what
# the table says and what the window feels like are one thing measured
# twice. The corner reads out what each frame cost either way.
studio-demo ANIMATE="" SIZE="5120x1440":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ANIMATE}}" in
      ""|hands|manual) mode=1 ;;
      --animate|animate) mode=animate ;;
      *) echo "usage: just studio-demo [--animate] [SIZE]" >&2; exit 2 ;;
    esac
    just studio "$mode" "{{SIZE}}"

# Drive one gesture headlessly and say what a frame of it costs.
#
#   just studio-bench pan          across the session
#   just studio-bench down         down it — the axis the panel shares
#   just studio-bench zoom-x       in and out, horizontally
#   just studio-bench zoom-y       and vertically
#
# `DUMP=/tmp/frames` writes every frame as a picture, which is the only
# way to see a fault that exists only while something is moving: a still
# rendered at the same place is correct, because what is wrong is the
# state left over from the frame before.
studio-bench GESTURE="pan" SIZE="5120x1440" FRAMES="120" DUMP="":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p session-daw --bin blitz_shot
    env ${DUMP:+FTS_BLITZ_DUMP="{{DUMP}}"} ${FPS:+FTS_BLITZ_FPS=1} \
    FTS_BLITZ_SCENE=drum-mixing \
    FTS_BLITZ_SIZE="{{SIZE}}" \
    FTS_BLITZ_GESTURE="{{GESTURE}}" \
    FTS_BLITZ_FRAMES="{{FRAMES}}" \
    ./target/release/blitz_shot "{{GOLDEN_DIR}}/template.rpp" /tmp/fts-studio.png 2>/dev/null

# ── Driving the studio from a script ──────────────────────────────────
#
#   just drive                     open it on :99
#   just drive-shot /tmp/now.png   photograph that display
#   just drive-stop                close it
#
# Why this exists: on Wayland a session cannot move the pointer or type
# into a window, so the only way to find out whether a click does what
# you think is to ask a person. Three bugs in the panel's input went
# unnoticed exactly that way — a rename field that opened and could not
# be typed into, a name that reverted on Enter, a control that lit on
# the wrong row. All three showed up in the first minute of driving it.
#
# Xvfb has no GPU, so this renders through llvmpipe at some tens of
# milliseconds a frame. It is for finding out what the window DOES, and
# says nothing whatever about what it costs — measure with
# `just studio-bench`.
#
# Notes that cost an hour each, so they are written down:
#
#   * `setsid`, or the window dies with the shell that started it.
#   * Wait for the WINDOW, not for a number of seconds. `xdotool search`
#     returning an id is the only honest ready signal; the process
#     exists long before it has mapped anything.
#   * There is no window manager on :99, so nothing gives a window the
#     keyboard. `xdotool windowfocus` it first and send keys with
#     `key --window $W`, or every keystroke goes nowhere — which looks
#     exactly like a bug in whatever you are testing.
#
#   W=$(DISPLAY=:99 xdotool search --name FastTrackStudio | head -1)
#   DISPLAY=:99 xdotool windowfocus $W
#   DISPLAY=:99 xdotool mousemove 375 230 click 1
#   DISPLAY=:99 xdotool key --window $W Return

# Open the studio on a nested display a script can drive.
drive SIZE="2560x1440" SCENE="drum-mixing" DISPLAY_NUM="99":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --profile release-fast -p session-daw --bin blitz_shot
    just drive-stop "{{DISPLAY_NUM}}"
    # A killed Xvfb leaves its lock behind and the next one refuses to
    # start — silently, as far as anything asking the display is
    # concerned: every xdotool call just says "failed creating new xdo
    # instance". So wait for the old one to actually go, then clear
    # what it left.
    until ! pgrep -f "Xvfb :{{DISPLAY_NUM}} " >/dev/null 2>&1; do sleep 1; done
    rm -f /tmp/.X{{DISPLAY_NUM}}-lock /tmp/.X11-unix/X{{DISPLAY_NUM}}
    setsid Xvfb :{{DISPLAY_NUM}} -screen 0 {{SIZE}}x24 > /tmp/fts-xvfb.log 2>&1 < /dev/null &
    disown || true
    for _ in $(seq 30); do
        DISPLAY=:{{DISPLAY_NUM}} xdotool getdisplaygeometry >/dev/null 2>&1 && break
        sleep 1
    done
    if ! DISPLAY=:{{DISPLAY_NUM}} xdotool getdisplaygeometry >/dev/null 2>&1; then
        echo "Xvfb :{{DISPLAY_NUM}} did not come up — see /tmp/fts-xvfb.log" >&2
        tail -5 /tmp/fts-xvfb.log >&2 || true
        exit 1
    fi
    echo "display :{{DISPLAY_NUM}} up at {{SIZE}}"
    setsid env -u WAYLAND_DISPLAY DISPLAY=:{{DISPLAY_NUM}} \
        FTS_BLITZ_WINDOW=1 \
        FTS_BLITZ_SIZE="{{SIZE}}" \
        FTS_BLITZ_SCENE="{{SCENE}}" \
        FTS_BLITZ_FPS=0 \
        FTS_PRESENT=immediate \
        FTS_BLITZ_LOG=/tmp/fts-drive.log \
        ./target/release-fast/blitz_shot "{{GOLDEN_DIR}}/template.rpp" /tmp/fts-drive.png \
        > /tmp/fts-drive.out 2>&1 < /dev/null &
    disown || true
    # Software rendering opens slowly; a minute is generous and a hang
    # is better reported than waited on forever.
    for _ in $(seq 60); do
        W=$(DISPLAY=:{{DISPLAY_NUM}} xdotool search --name FastTrackStudio 2>/dev/null | head -1 || true)
        [ -n "${W:-}" ] && break
        sleep 1
    done
    if [ -z "${W:-}" ]; then
        echo "no window after 60s — see /tmp/fts-drive.out" >&2
        tail -5 /tmp/fts-drive.out >&2 || true
        exit 1
    fi
    DISPLAY=:{{DISPLAY_NUM}} xdotool windowfocus "$W" || true
    echo "window $W on :{{DISPLAY_NUM}} — logs /tmp/fts-drive.log, stdout /tmp/fts-drive.out"

# Photograph the nested display.
drive-shot OUT="/tmp/fts-drive-shot.png" DISPLAY_NUM="99":
    #!/usr/bin/env bash
    set -euo pipefail
    DISPLAY=:{{DISPLAY_NUM}} import -window root "{{OUT}}"
    echo "{{OUT}}"

# Close the nested display and whatever was on it.
drive-stop DISPLAY_NUM="99":
    #!/usr/bin/env bash
    set -uo pipefail
    pkill -f "release/blitz_shot .*template.rpp /tmp/fts-drive.png" || true
    pkill -f "Xvfb :{{DISPLAY_NUM}} " || true
    exit 0

# Prove the culling draws the same frame as drawing everything.
#
# The bench's headline number comes from NOT drawing what is off screen,
# which is only a speed-up if the skipped part was never visible. This
# renders 720 viewports twice — culled, then complete — and compares the
# buffers byte for byte. Run it after touching the scene index, the
# viewport maths, or anything that records commands. Exits non-zero on a
# mismatch, so it belongs in CI beside the bench.
# Defaults to the GOLDEN SESSION rather than the orchestral fixture:
# byte-identical culling on the reference session is one of the things
# the golden is for (#49). The orchestral fixture is one argument away
# (`just daw-verify /tmp/fts-orchestral.rpp`, or FTS_DAW_FIXTURE).
daw-verify PROJECT="" SIZE="5120x1440":
    #!/usr/bin/env bash
    set -euo pipefail
    project="{{PROJECT}}"
    if [[ -z "$project" ]]; then
        project="${FTS_DAW_FIXTURE:-{{DAW_PROJECT}}}"
        [[ -f "$project" ]] || just daw-template
    fi
    cargo build --release -p session-daw --bin bench 2>&1 | grep -E '^error' -A6 || true
    FTS_BENCH_VERIFY=1 FTS_BENCH_SIZE="{{SIZE}}" ./target/release/bench "$project" 2>&1 \
        | grep -viE 'vulkan|objects:|WARN'

# The same sweep, in a real window, so you can watch it.
#
# Every parameter on every track, moving, measured.
#
# The mixer's controls are drawn live so a mute can change without the
# mixer being re-recorded. This is the frame that says whether that is
# actually cheap: mutes and solos toggling, arms flipping, faders
# sweeping and pans crossing on every visible strip, every frame.
#
# Nothing scrolls — the question is what a STILL mixer costs when
# everything in it is changing, and a scroll would hide that under the
# cost of culling.
daw-animate PROJECT="" SIZE="2560x1440":
    #!/usr/bin/env bash
    set -euo pipefail
    project="{{PROJECT}}"
    if [[ -z "$project" ]]; then
        project="${FTS_DAW_FIXTURE:-/tmp/fts-orchestral.rpp}"
        [[ -f "$project" ]] || just daw-fixture
    fi
    cargo build --release -p session-daw --bin bench 2>&1 | grep -E '^error' -A6 || true
    FTS_BENCH_ANIMATE=1 FTS_BENCH_SIZE="{{SIZE}}" ./target/release/bench "$project" 2>&1 \
        | grep -viE 'vulkan|objects:|WARN'

# The app's Mixer, on the golden session, the way the scene pictures show
# it — opened straight from the fixtures (not organized a second time, so
# nothing is written beside it), in Mix mode, drawn by the fts-dark theme,
# with a simulated signal in the meters and the racks (FTS_SIMULATE) since
# the template has no media. Compare with
# features/dynamic-template/fixtures/golden/scenes/drum-mixing.png.
#
#   just mixer-golden                    at the pictures' 2560x1440
#   SIMULATE=0 just mixer-golden         silent, as the engine has it
#   THEME=console just mixer-golden      apps/session/daw/themes/console/ (a folder of layers)
#   THEME=console:~/my-layers just …     a theme over it: each path over the ones before
mixer-golden SIZE="2560x1440":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --profile release-fast -p session-desktop
    # A theme by name is themes/<name>/ or themes/fts-<name>.frame; anything
    # else is a path. `:` layers them.
    theme=""
    IFS=: read -ra layers <<< "${THEME:-dark}"
    for layer in "${layers[@]}"; do
        if [ -d "apps/session/daw/themes/$layer" ]; then path="$PWD/apps/session-daw/themes/$layer"
        elif [ -f "apps/session/daw/themes/fts-$layer.frame" ]; then path="$PWD/apps/session-daw/themes/fts-$layer.frame"
        else path="${layer/#\~/$HOME}"; fi
        theme="${theme:+$theme:}$path"
    done
    FTS_SESSION_PREPARE=0 \
    FTS_SESSION_MODE=Mix \
    FTS_SESSION_VIEW=mixer \
    FTS_SIMULATE="${SIMULATE:-1}" \
    FTS_THEME="$theme" \
    FTS_WINDOW_SIZE="{{SIZE}}" \
    FTS_SESSION_PROJECT="$PWD/{{GOLDEN_DIR}}/template.rpp" \
    ./target/release-fast/session-desktop

# How far the themed mixer is from the painters' one: regenerates the
# fts-dark theme from the painters' own drawings (skin::export), shoots the
# mixer both ways and diffs them (percentage of pixels that differ, and a
# picture of where). THEME_DIFF_CROP=x0,y0,x1,y1 also writes a blown-up
# side-by-side of one region. OUT is where the pictures go.
theme-parity PROJECT="features/dynamic-template/fixtures/golden/template.rpp" SIZE="1600x900" OUT="/tmp/fts-theme-parity":
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{OUT}}"
    cargo build -q --release -p session-daw --bin bench --bin theme_diff --bin theme_export
    ./target/release/theme_export apps/session/daw/themes/fts-dark.frame
    FTS_BENCH_MIXER="{{OUT}}/native.png" FTS_BENCH_SIZE="{{SIZE}}" ./target/release/bench "{{PROJECT}}" 2>&1 | grep -E 'wrote' || true
    FTS_THEME=apps/session/daw/themes/fts-dark.frame FTS_BENCH_MIXER="{{OUT}}/themed.png" FTS_BENCH_SIZE="{{SIZE}}" ./target/release/bench "{{PROJECT}}" 2>&1 | grep -E 'wrote|theme' || true
    ./target/release/theme_diff "{{OUT}}/native.png" "{{OUT}}/themed.png" "{{OUT}}/diff.png"

# The studio window (Blitz + the painted arrangement) on a real session,
# prepared first: organize it, build the song from its keyflow chart
# (tempo, markers, section regions, Keyflow folder), and generate the
# click and guide — the multitrack's own click/guide stems are kept,
# muted, beside them. Leave CHART empty to open the session as it is.
# The desktop app, as you run it while working on it: `release-fast`
# (release's optimisation, incremental — see Cargo.toml). A one-line edit
# rebuilds in ~15 s instead of ~60 s. `PROJECT` is a `.RPP` (its prepared
# `.session` beside it opens instead, once there is one), a `.session`, or
# a setlist (a folder of songs, or a `.setlist`); `MODE` opens in a mode:
#   just app "../sessions/Worship Set.setlist" "" organize
app PROJECT="" CHART="" MODE="":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --profile release-fast -p session-desktop
    # A bare binary started from a shell is not a foreground app on macOS
    # (it opens behind whatever is in front); wrapped in a bundle and
    # started with `open`, it comes forward like any app.
    bundle="target/release-fast/Session Dev.app"
    mkdir -p "$bundle/Contents/MacOS"
    printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' \
        '<plist version="1.0"><dict>' \
        '<key>CFBundleName</key><string>Session Dev</string>' \
        '<key>CFBundleIdentifier</key><string>app.fasttrackstudio.session.dev</string>' \
        '<key>CFBundleExecutable</key><string>session-desktop</string>' \
        '<key>CFBundlePackageType</key><string>APPL</string>' \
        '<key>NSHighResolutionCapable</key><true/>' \
        '</dict></plist>' > "$bundle/Contents/Info.plist"
    # Everything the app prints, panics with their backtraces included, goes
    # here — `open` would otherwise throw it away.
    # ~/Library/Logs, not target/: `open` cannot write a launched app's
    # output onto an external volume (-10810).
    mkdir -p "$HOME/Library/Logs/Session Dev"
    log="$HOME/Library/Logs/Session Dev/session-dev.log"
    envs=(--env "RUST_LOG=${RUST_LOG:-warn,session_daw=info}" --env RUST_BACKTRACE=1)
    # quote(): a song title can carry an apostrophe (God, I'm Just Grateful).
    project={{quote(PROJECT)}}
    chart={{quote(CHART)}}
    mode={{quote(MODE)}}
    if [[ -n "$project" ]]; then
        # A folder or a .setlist is a set; anything else is one song.
        if [[ -d "$project" && "$project" != *.session ]] || [[ "$project" == *.setlist ]]; then
            envs+=(--env "FTS_SESSION_SETLIST=$(cd "$(dirname "$project")" && pwd)/$(basename "$project")")
        else
            envs+=(--env "FTS_SESSION_PROJECT=$(cd "$(dirname "$project")" && pwd)/$(basename "$project")")
        fi
    fi
    if [[ -n "$chart" ]]; then envs+=(--env "FTS_SESSION_CHART=$chart"); fi
    if [[ -n "$mode" ]]; then envs+=(--env "FTS_SESSION_MODE=$mode"); fi
    # The view to open on (overview, performance, setup), from the caller's env.
    if [[ -n "${FTS_SESSION_VIEW:-}" ]]; then envs+=(--env "FTS_SESSION_VIEW=$FTS_SESSION_VIEW"); fi
    # The lyrics panel's view (audience, performer, confidence), likewise.
    if [[ -n "${FTS_LYRICS_VIEW:-}" ]]; then envs+=(--env "FTS_LYRICS_VIEW=$FTS_LYRICS_VIEW"); fi
    # Collaboration (collab_bar.rs): share on open, or join a ticket / the
    # file a host writes one to. A joining copy is a SECOND window beside
    # the host, so it neither kills the running one nor shares its log.
    for var in FTS_COLLAB_HOST FTS_COLLAB_JOIN FTS_COLLAB_TASK FTS_COLLAB_NAME FTS_COLLAB_TICKET FTS_COLLAB_PUPPET FTS_COLLAB_PUPPET_TRANSPORT FTS_COLLAB_PUPPET_MOUSE FTS_WINDOW_POS FTS_WINDOW_SIZE FTS_SESSION_SONG FTS_AUDIO_MODE FTS_AUDIO_TARGET SESSION_DAW_REAPER FTS_SOCKET FTS_STREAM_SOURCE FTS_SHARE_LINKS FTS_LOAD FTS_TASK_SERVER FTS_TASK_ORG FTS_TASK_TOKEN XDG_DATA_HOME; do
        if [[ -n "${!var:-}" ]]; then envs+=(--env "$var=${!var}"); fi
    done
    # FTS_SECOND_WINDOW: the same, for any second window (`duo-task`).
    if [[ -n "${FTS_COLLAB_JOIN:-}" || -n "${FTS_SECOND_WINDOW:-}" ]]; then
        log="$HOME/Library/Logs/Session Dev/session-dev-joined.log"
        open -n --stdout "$log" --stderr "$log" "${envs[@]}" "$bundle"
        echo "log: $log"
        exit 0
    fi
    pkill -f 'Session Dev.app/Contents/MacOS/session-desktop' || true
    # Let a killed copy go before its bundle is opened again.
    sleep 1
    # A new file, never an overwrite: macOS caches a binary's code signature
    # by file, and launching one rewritten in place fails (-10810).
    rm -f "$bundle/Contents/MacOS/session-desktop"
    cp target/release-fast/session-desktop "$bundle/Contents/MacOS/session-desktop"
    # One signature across rebuilds. The linker's ad-hoc one names the
    # binary by a build hash, so every build is a new app to macOS and it
    # asks again for the removable drive; a real identity and a fixed
    # identifier keep the grant. SESSION_SIGN_IDENTITY overrides the pick.
    identity="${SESSION_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null \
        | awk -F'"' '/Apple Development/ {print $2; exit}')}"
    if [[ -n "$identity" ]]; then
        codesign --force --sign "$identity" --identifier app.fasttrackstudio.session.dev "$bundle" \
            || echo "codesign with '$identity' failed; the build keeps its ad-hoc signature"
    fi
    open -n --stdout "$log" --stderr "$log" "${envs[@]}" "$bundle"
    echo "log: $log"

# Two windows side by side on one song or setlist, collaborating: the left
# one shares it, the right one joins — to see both ends of a session at
# once with your own mouse. `NAME_A`/`NAME_B` name the two people.
#
#   just duo "../sessions/Worship Set.setlist"
#   just duo "../sessions/imported/Washed/Washed.RPP"
duo PROJECT MODE="live" NAME_A="Cody" NAME_B="Alice":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --profile release-fast -p session-desktop
    # The main screen's usable area, in points: below the menu bar, above
    # the Dock — NOT Finder's desktop bounds, which span every monitor.
    read -r X Y W H < <(swift -e 'import AppKit; let s = NSScreen.main!; let f = s.visibleFrame; print(Int(f.origin.x), Int(s.frame.height - f.maxY), Int(f.width), Int(f.height))' 2>/dev/null)
    # Not an even split: the sharer's window is a little narrower, so
    # every panel is a different size on each side — what anchored
    # pointers have to survive. DUO_SPLIT is the left one's share, in %.
    left=$((W * ${DUO_SPLIT:-44} / 100))
    right=$((W - left))
    ticket="${TMPDIR:-/tmp}/fts-session-ticket"
    rm -f "$ticket"
    FTS_COLLAB_HOST=1 FTS_COLLAB_NAME={{quote(NAME_A)}} FTS_SESSION_VIEW="${FTS_SESSION_VIEW:-overview}" \
        FTS_WINDOW_POS="${X},${Y}" FTS_WINDOW_SIZE="${left}x${H}" \
        just app {{quote(PROJECT)}} "" {{quote(MODE)}}
    for _ in $(seq 1 90); do [[ -s "$ticket" ]] && break; sleep 1; done
    [[ -s "$ticket" ]] || { echo "the host did not share (see the log)"; exit 1; }
    # JOIN_SONG=<n> starts the joiner on the set's n-th song.
    FTS_SESSION_SONG="${JOIN_SONG:-}" FTS_COLLAB_JOIN="$ticket" FTS_COLLAB_NAME={{quote(NAME_B)}} FTS_SESSION_VIEW="${FTS_SESSION_VIEW:-overview}" \
        FTS_WINDOW_POS="$((X + left)),${Y}" FTS_WINDOW_SIZE="${right}x${H}" \
        just app {{quote(PROJECT)}} "" {{quote(MODE)}}
    echo "sharing: $(cat "$ticket")"

# Two windows meeting in a set Task keeps (`FTS_COLLAB_TASK`): no host —
# Task keeps the session, the first window on a song seeds its doc, and the
# second takes it from Task. SETLIST is the set's id in the library's org
# (`FTS_TASK_*`), or `share:<live share link>` to join as a guest.
duo-task SETLIST PROJECT MODE="live" NAME_A="Cody" NAME_B="Alice":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --profile release-fast -p session-desktop
    read -r X Y W H < <(swift -e 'import AppKit; let s = NSScreen.main!; let f = s.visibleFrame; print(Int(f.origin.x), Int(s.frame.height - f.maxY), Int(f.width), Int(f.height))' 2>/dev/null)
    left=$((W * ${DUO_SPLIT:-44} / 100))
    right=$((W - left))
    FTS_COLLAB_TASK={{quote(SETLIST)}} FTS_COLLAB_NAME={{quote(NAME_A)}} FTS_SESSION_VIEW="${FTS_SESSION_VIEW:-overview}" \
        FTS_WINDOW_POS="${X},${Y}" FTS_WINDOW_SIZE="${left}x${H}" \
        just app {{quote(PROJECT)}} "" {{quote(MODE)}}
    # The first seeds; give it the moment it takes before the second looks.
    sleep "${DUO_TASK_GAP:-8}"
    FTS_SECOND_WINDOW=1 FTS_SESSION_SONG="${JOIN_SONG:-}" FTS_COLLAB_TASK={{quote(SETLIST)}} FTS_COLLAB_NAME={{quote(NAME_B)}} FTS_SESSION_VIEW="${FTS_SESSION_VIEW:-overview}" \
        FTS_WINDOW_POS="$((X + left)),${Y}" FTS_WINDOW_SIZE="${right}x${H}" \
        just app {{quote(PROJECT)}} "" {{quote(MODE)}}

# Prepare songs once — organize, build from the chart, generate the click
# and guide — and save each as `Song.session` beside its `.RPP`, which the
# app then opens instead. One process per song (the engine is
# process-wide). `just prepare ../sessions/imported/*/*.RPP`
[positional-arguments]
prepare +PROJECTS:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --profile release-fast -p session-daw --bin prepare
    for p in "$@"; do ./target/release-fast/prepare "$p"; done

studio-song PROJECT CHART="" SIZE="2560x1440":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --profile release-fast -p session-daw --bin blitz_shot
    prep=()
    if [[ -n "{{CHART}}" ]]; then
        prep=(FTS_BLITZ_ORGANIZE=1 "FTS_BLITZ_CHART={{CHART}}" FTS_BLITZ_GUIDE=1)
    fi
    env FTS_BLITZ_WINDOW=1 FTS_BLITZ_SIZE="{{SIZE}}" ${prep[@]+"${prep[@]}"} \
        RUST_LOG="${RUST_LOG:-warn,session_daw=info}" \
        ./target/release-fast/blitz_shot "{{PROJECT}}" /tmp/fts-studio.png

# Everything CI runs, in CI's order, with one command.
#
# The workflow (.github/workflows/checks.yml) is the source of truth;
# this mirrors its steps so a green run here means a green run there.
# It stops at the first failure, because CI does too and a later step
# built on a broken one tells you nothing.
#
# `just ci` runs the lot. `just ci nextest` starts from that step, for
# when you have already passed the cheap ones and are iterating on a
# test. Steps in order: lockfile, fmt, flows, tailwind, check, nextest.
# Filter matches TEST NAMES, not file names, and a filter matching
# nothing exits zero having run nothing — check the count.
#
# Stands up an isolated REAPER under target/fts-reaper-test with only
# daw-bridge in its UserPlugins (built from the sibling ../daw), then
# runs the suites that attach the window to it. Not part of `just ci`:
# it needs a licensed REAPER, which the runner does not have.

# Run the REAPER integration suites against a real REAPER
reaper-test FILTER="":
    cargo run -p session-reaper-xtask -- {{FILTER}}

# The same with REAPER's window shown, held open afterwards
reaper-test-gui FILTER="":
    cargo run -p session-reaper-xtask -- --gui --keep-open {{FILTER}}

# Runs exactly what `.github/workflows/checks.yml` runs, in the same
# order. NOT the other two workflows: `session-ios` and `deploy` need a
# Mac, signing keys and upload credentials, which is why they are not a
# thing you run before pushing — and why a green run here says nothing
# about whether either of them is healthy.

ci FROM="lockfile":
    #!/usr/bin/env bash
    set -euo pipefail
    export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
    steps=(lockfile fmt flows tailwind check nextest)
    start=0
    for i in "${!steps[@]}"; do
      [ "${steps[$i]}" = "{{FROM}}" ] && start=$i && break
    done
    run_from() { local want="$1"; local at=0
      for i in "${!steps[@]}"; do [ "${steps[$i]}" = "$want" ] && at=$i; done
      [ "$at" -ge "$start" ]; }

    # Before anything: is there room to build?
    #
    # A full disk does not fail as a full disk. mold dies with a bus
    # error and cargo reports `linking with cc failed`, which reads as a
    # broken toolchain and sends you looking in the wrong place — it
    # cost an hour once. Checked here because this is the command
    # everyone runs before pushing, and a run that cannot finish should
    # say why in the first second rather than the twentieth minute.
    free_gb=$(df -PBG "$CARGO_TARGET_DIR" 2>/dev/null | awk 'NR==2 {gsub(/G/,"",$4); print $4}')
    if [ -n "${free_gb:-}" ] && [ "$free_gb" -lt 25 ]; then
      echo "only ${free_gb}G free on the volume holding $CARGO_TARGET_DIR."
      echo "A build that runs out of room dies in the LINKER, with a bus"
      echo "error that looks like a compiler fault. Reclaim first:"
      echo "  just sweep-incremental   # the big, always-safe win"
      echo "  just sweep               # artifacts older than 7 days"
      echo "  just disk                # where it all went"
      exit 1
    fi

    if run_from lockfile; then
      echo "── lockfile ─────────────────────────────────────────"
      committed="$(mktemp)"; cp Cargo.lock "$committed"
      cargo metadata --format-version 1 > /dev/null
      packages() { sed '/^\[\[patch\.unused\]\]/,$d' "$1"; }
      unused() { awk '/^\[\[patch\.unused\]\]/ { f = 1; next }
                      f && /^name = /    { n = $3 }
                      f && /^version = / { print n "@" $3 }' "$1" | sort; }
      ok=1
      diff -u <(packages "$committed") <(packages Cargo.lock) || ok=0
      diff -u <(unused "$committed") <(unused Cargo.lock) || ok=0
      rm -f "$committed"
      [ "$ok" = 1 ] || { echo "Cargo.lock is out of date — commit the result of cargo metadata"; exit 1; }
      echo "ok"
    fi

    if run_from fmt; then
      echo "── cargo fmt --check ────────────────────────────────"
      cargo fmt --all --check
      echo "ok"
    fi

    if run_from flows; then
      echo "── flow verification gate ───────────────────────────"
      just daw-flows --own-daemon
    fi

    if run_from tailwind; then
      echo "── web tailwind sheet ───────────────────────────────"
      just web-tailwind
      echo "ok"
    fi

    if run_from check; then
      echo "── cargo check --workspace ──────────────────────────"
      cargo check --workspace
      echo "ok"
    fi

    if run_from nextest; then
      echo "── cargo nextest --workspace ────────────────────────"
      cargo nextest run --workspace --no-fail-fast
    fi

    echo
    echo "every CI step passed locally"

# The Session DAW view in a browser (session-daw's web_host), built to
# apps/session/daw-web/dist: cargo → wasm-bindgen → index.html, plus the
# demo session's project and chart under dist/session/. Host toolchain
# (rustup target add wasm32-unknown-unknown; cargo install
# wasm-bindgen-cli --version 0.2.126). Serve with `just web-daw-serve`.
web-daw SESSION="../sessions/Always On Time" RPP="Always On Time.RPP" CHART="Always_on_Time.kf" PROFILE="wasm-release":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -p session-daw-web --target wasm32-unknown-unknown --profile {{PROFILE}}
    out=apps/session/daw-web/dist
    mkdir -p "$out/session"
    dir=$([ "{{PROFILE}}" = "dev" ] && echo debug || echo "{{PROFILE}}")
    wasm-bindgen --target web --no-typescript --out-dir "$out" \
        "target/wasm32-unknown-unknown/$dir/session-daw-web.wasm"
    # 26 MB → 19 MB (7 MB gzipped). Release only: it takes half a minute.
    if [ "{{PROFILE}}" != "dev" ] && command -v wasm-opt >/dev/null; then
        wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
            --enable-sign-ext --enable-mutable-globals --enable-reference-types \
            --enable-multivalue "$out/session-daw-web_bg.wasm" -o "$out/session-daw-web_bg.wasm"
    fi
    cp apps/session/daw-web/www/index.html "$out/"
    cp "{{SESSION}}/{{RPP}}" "$out/session/demo.RPP"
    cp "{{SESSION}}/{{CHART}}" "$out/session/demo.kf"
    ls -lh "$out" "$out/session"

# Serve the built web DAW on http://localhost:8765.
web-daw-serve:
    cd apps/session/daw-web/dist && python3 -m http.server 8765

# Fetch the large binaries kept out of this repository (assets.txt: the
# private studio-assets and rig-models repos) into .assets/<repo>, and link
# each listed file or directory into place. Re-run to update.
assets:
    #!/usr/bin/env bash
    set -euo pipefail
    n=0
    declare -A fetched=()
    while read -r repo from to; do
        [[ -z $repo || $repo == \#* ]] && continue
        to=${to:-$from}
        if [[ -z ${fetched[$repo]:-} ]]; then
            if [[ -d .assets/$repo/.git ]]; then git -C ".assets/$repo" pull --ff-only --quiet
            else git clone --quiet "git@github.com:FastTrackStudios/$repo.git" ".assets/$repo"; fi
            fetched[$repo]=1
        fi
        mkdir -p "$(dirname "$to")"
        ln -sfn "$PWD/.assets/$repo/$from" "$to"
        n=$((n+1))
    done < assets.txt
    echo "linked $n assets from ${#fetched[@]} repos"

# Generate frame's plugin faces (features/faces/generated: every `.frame`, the
# catalogs and rig-faces/faces.json) from examples/plugin-faces. They are
# not kept in git; session-daw and signal read them from here at runtime.
# Deterministic: unchanged sources rewrite every file byte for byte.
faces *ONLY:
    cargo run -q --release -p plugin-faces -- features/faces/generated {{ if ONLY != "" { "--only " + ONLY } else { "" } }}
