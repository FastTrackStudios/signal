# signal workspace recipes
# Run commands: just <recipe-name>

# Default: serve the desktop app
default: dx

# ── Desktop App ──────────────────────────────────────────────────────────

# Serve the dioxus desktop app (hot-reload)
dx *args: tailwind
    cd apps/desktop && dx serve {{args}}

# Build the desktop app for release
dx-build: tailwind
    cd apps/desktop && dx build --release --platform desktop

# Build Tailwind CSS (v4)
tailwind:
    cd apps/desktop && tailwindcss -i ./input.css -o ./assets/tailwind.css --minify

# Watch Tailwind CSS for changes (run alongside dx serve)
tailwind-watch:
    cd apps/desktop && tailwindcss -i ./input.css -o ./assets/tailwind.css --watch --minify

# ── Build ────────────────────────────────────────────────────────────────

# Check all crates compile
check:
    cargo check --workspace

# Build all crates
build: tailwind
    cargo build --workspace

# Run tests
test:
    cargo test --workspace

# ── CLI ──────────────────────────────────────────────────────────────────

# Run the signal CLI
cli *args:
    cargo run -p signal-cli -- {{args}}

# Release build
release: tailwind
    cargo build --release

# ── Live Rigs ──────────────────────────────────────────────────────────────
# Open a live instrument rig: live input → FX chain (NAM amp / cab / plugins) →
# output, routed through PipeWire via cpal's JACK backend (`pw-jack`). Patches
# switch instantly. Each rig's interface / input channel / profile is remembered
# in ~/.config/signal/rigs/<name>.styx. Set one up with `just rig-setup`.
#
# NOTE: needs `libjack2` on PKG_CONFIG_PATH — re-enter the nix dev shell
# (`direnv reload`) after the flake change so `--features jack` builds.

# Open the default guitar rig (Yamaha TF ch4 → NAM amps)
guitar: (rig "Guitar Rig")

# Open the default keys rig (needs `just rig-setup "Keys Rig" ...` first)
keys: (rig "Keys Rig")

# Open the default drums rig (needs `just rig-setup "Drum Rig" ...` first)
drums: (rig "Drum Rig")

# Open a saved rig by name (TUI with input/output meters + patch switching).
# --release is REQUIRED for real-time: in debug the vendored NAM C++ core is
# unoptimized (~10-50x slower), so model prewarm at startup crawls (looks hung)
# and processing xruns. (pw-jack comes from the flake dev shell — `direnv reload`.)
#
# PIPEWIRE_LATENCY requests this quantum for the rig's nodes ON DEMAND: the
# interface drops to it only while the rig runs, then idles back to the device
# default — low latency for playing without forcing everyday audio to run hot.
# Tune per-launch, e.g.: just rig "Guitar Rig" 256/48000  (or 64/48000 lower).
rig name latency="128/48000":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' PIPEWIRE_LATENCY={{latency}} pw-jack cargo run --release -p signal-sampler --features jack --example guitar_tui -- --rig "{{name}}"

# List audio devices + channel counts (find your interface name)
rig-devices:
    cargo run -p signal-sampler --example guitar_rig -- --list

# Show each interface port's reported latency (frames) — quick latency snapshot.
rig-latency-ports:
    pw-jack jack_lsp -L

# Measure true round-trip latency. Set up a hardware loopback (e.g. patch a TF
# output to TF input 22), then connect jack_iodelay:out → that output and the
# looped-back input → jack_iodelay:in (in qpwgraph). It prints round-trip ms.
rig-latency:
    pw-jack jack_iodelay

# Configure + remember a rig's interface / channel / profile, e.g.:
#   just rig-setup "Guitar Rig" --input "Yamaha TF" --channel 3 --profile /path/to.styx
rig-setup name *args:
    cargo run -p signal-sampler --example guitar_rig -- --rig "{{name}}" {{args}} --write-config

# ── Aliases ──────────────────────────────────────────────────────────────

alias c := check
alias b := build
alias t := test
alias g := guitar
