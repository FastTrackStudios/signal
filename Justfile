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
# output, routed through PipeWire via cpal's NATIVE PipeWire backend (no JACK
# shim). daw picks the interface by name and targets it, so PipeWire maps its
# capture channels to the rig in order — the configured input channel just works.
# Patches switch instantly. Each rig's interface / input channel / profile is
# remembered in ~/.config/signal/rigs/<name>.styx; pick them live with `s` in
# the TUI, or seed one with `just rig-setup`.
#
# NOTE: needs `libpipewire` on PKG_CONFIG_PATH — re-enter the nix dev shell
# (`direnv reload`) after the flake change so `--features pipewire` builds.

# Open the default guitar rig (Yamaha TF ch4 → NAM amps)
guitar: (rig "Guitar Rig")

# Open the default keys rig (needs `just rig-setup "Keys Rig" ...` first)
keys: (rig "Keys Rig")

# Open the default drums rig (needs `just rig-setup "Drum Rig" ...` first)
drums: (rig "Drum Rig")

# Open a saved rig by name (TUI with input/output meters + patch switching).
# --release is REQUIRED for real-time: in debug the vendored NAM C++ core is
# unoptimized (~10-50x slower), so model prewarm at startup crawls (looks hung)
# and processing xruns.
#
# PIPEWIRE_PROPS names the graph node FTS-Signal (so it's easy to spot in
# qpwgraph). The rig requests its low-latency quantum natively from its buffer
# size (set in the TUI with `[`/`]`), so no PIPEWIRE_LATENCY / pw-jack wrapper is
# needed — daw drives the interface's quantum on demand while the rig runs.
rig name:
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p signal-sampler --features pipewire --example guitar_tui -- --rig "{{name}}"

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
