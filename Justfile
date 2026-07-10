# FastTrackStudio — root workspace recipes
# Run commands: just <recipe-name>

# List recipes by default
default:
    @just --list

# ── Tailwind (signal UI sheet) ───────────────────────────────────────────
# The compiled sheet is inlined by apps/fasttrackstudio/src/rig_view.rs
# (include_str!); rebuild it whenever UI-crate class usage changes.
# input.css @source globs scan the signal/session UI crates + libs/fts-ui
# + libs/dock. NOTE: apps/fasttrackstudio/assets/tailwind.css is the
# SEPARATE session-scoped sheet (asset!()) — this recipe must not touch it.

# Build Tailwind CSS (v4)
tailwind:
    cd apps/fasttrackstudio && tailwindcss -i ./input.css -o ./assets/tailwind-signal.css --minify

# Watch Tailwind CSS for changes
tailwind-watch:
    cd apps/fasttrackstudio && tailwindcss -i ./input.css -o ./assets/tailwind-signal.css --watch --minify

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
    cargo run -p fasttrackstudio -- --engine

alias rigd := signal-engine

# Stage the fts web bundle (the browser remote) for embedding: tailwind →
# dx web build (signal feature only) → apps/fasttrackstudio/web-dist/,
# which `cargo build -p fasttrackstudio --features embed-web` compiles
# into the binary (include_dir). web-dist/ is gitignored.
web-stage: tailwind
    cd apps/fasttrackstudio && dx build --platform web --release --no-default-features --features signal
    rm -rf apps/fasttrackstudio/web-dist
    cp -r target/dx/fasttrackstudio/release/web/public apps/fasttrackstudio/web-dist

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
