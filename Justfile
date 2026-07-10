# FastTrackStudio — root workspace recipes
# Run commands: just <recipe-name>

# List recipes by default
default:
    @just --list

# ── Tailwind (signal web remote) ─────────────────────────────────────────
# The compiled sheet is inlined by apps/signal-web (include_str!); rebuild it
# whenever UI-crate class usage changes. input.css @source globs scan the
# signal/session UI crates + libs/fts-ui + libs/dock.

# Build Tailwind CSS (v4)
tailwind:
    cd apps/signal-web && tailwindcss -i ./input.css -o ./assets/tailwind.css --minify

# Watch Tailwind CSS for changes
tailwind-watch:
    cd apps/signal-web && tailwindcss -i ./input.css -o ./assets/tailwind.css --watch --minify

# ── Live Rigs (carried from the dissolved signal workspace) ──────────────
# Open a live instrument rig: live input → FX chain (NAM amp / cab / plugins)
# → output, routed through PipeWire via cpal's NATIVE PipeWire backend. Each
# rig's interface / input channel / profile is remembered in
# ~/.config/signal/rigs/<name>.styx.
#
# NOTE: needs `libpipewire` on PKG_CONFIG_PATH for `--features pipewire`.

# The signal engine — the headless rig core (serves the vox router on
# ws://:4040/vox). `rigd` kept as an alias for muscle memory.
signal-engine:
    cargo run -p signal-engine

alias rigd := signal-engine

# Build the browser remote (tailwind + dx release build) and copy the bundle
# next to the engine binary as target/debug/signal-web, where the engine
# auto-discovers it (env SIGNAL_WEB_DIST > <exe_dir>/signal-web >
# target/dx/signal-web/{release,debug}/web/public). Any device on the LAN
# then gets the UI at http://<host>:4040/.
# Release deploys: copy the same bundle beside the release binary instead
# (target/release/signal-web, or <install_dir>/signal-web next to a shipped
# signal-engine binary).
signal-web-sync: tailwind
    cd apps/signal-web && dx build --platform web --release
    rm -rf target/debug/signal-web
    mkdir -p target/debug
    cp -r target/dx/signal-web/release/web/public target/debug/signal-web

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

# Nord Stage-style keys rig — play a composition-tree preset from a MIDI
# keyboard. --release is REQUIRED for real-time.
keys preset="Nord Stage" midi="all":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p signal-keys --features pipewire --example keys_tui -- --preset "{{preset}}" --midi "{{midi}}"

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
    cargo run -p signal-sampler --example guitar_rig -- --rig "{{name}}" {{args}} --write-config

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

# ── Build ────────────────────────────────────────────────────────────────

# Check the whole workspace compiles
check:
    cargo check --workspace --exclude vox-discover

# Run tests
test:
    cargo test --workspace

# ── Aliases ──────────────────────────────────────────────────────────────

alias c := check
alias t := test
alias g := guitar
