# signal's recipes: the rigs, the sampler and the plugins, run from a
# signal checkout. They came from Studio's Justfile with the crates
# (studio#510); Studio's keeps the app's (`just guitar`, `just keys`,
# `just signal-engine`), which run the rigs inside Studio.
#
# --release is REQUIRED for real-time audio.

default:
    @just --list

# The plugins here, as .clap + .vst3 (target/bundled/, names from
# bundler.toml). Pass a subset to bundle only those:
#   just plugins-bundle nam
# On macOS nice-plug-xtask's `bundle-universal` builds both
# aarch64-apple-darwin and x86_64-apple-darwin into one bundle.
plugins-bundle plugins="nam signal":
    #!/usr/bin/env bash
    set -euo pipefail
    cmd=bundle
    [ "$(uname)" = "Darwin" ] && cmd=bundle-universal
    for p in {{plugins}}; do
        cargo run -q -p fts-plugin-xtask -- "$cmd" -p "$p-plugin" --release
    done
    ls target/bundled/

# Every visualiser in the rig, in one window, on the design rig's fake
# guitar. Stills of one effect at a time, rendered offline with no window:
# (in a FastTrackStudios/fts-fx checkout)
#   cargo run -p dsp --features delay-ui,delay-viz --example delay_family_sheet -- out/
#   cargo run -p dsp --features reverb-ui,reverb-viz --example reverb_family_sheet -- out/
#   cargo run -p dsp --features modulation-ui,modulation-viz --example engine_sheet -- out/
viz-gallery:
    cargo run --release -p guitar --features ui --example viz_gallery

# Render the rig UI to a PNG — no window, no compositor, no device. Design
# mode is forced, so this can be taken while the rig is playing; it draws
# the VECTOR fallback for any painted panel.
#   just guitar-shot /tmp/rig.png 2560 1440
guitar-shot OUT="rig.png" W="1600" H="1000":
    RIG_SHOT_TAILWIND="${RIG_SHOT_TAILWIND:?the app's sheet, e.g. ../studio/apps/studio/desktop/assets/tailwind-signal.css}" cargo run --release -p guitar --features ui --example rig_shot -- {{OUT}} {{W}} {{H}}

# What the rig costs to run, measured on the real device (silent + ephemeral).
guitar-bench SECONDS="10":
    cargo run --release -p guitar --features sampler/pipewire \
        --example rig_bench -- {{SECONDS}}

# How long a patch switch takes, and where the time goes.
guitar-switch-bench SWITCHES="12":
    cargo run --release -p guitar --features sampler/pipewire \
        --example switch_bench -- {{SWITCHES}}

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

# The guitar rig as a TUI (was `just guitar`)
guitar-tui: (rig "Guitar Rig")

# Open the default drums rig (needs `just rig-setup "Drum Rig" ...` first)
drums: (rig "Drum Rig")

# A terminal surface over the composition-tree presets, no GUI. This was
# `just keys` before the app grew a keys mode.
#
# Nord Stage-style keys TUI — play a preset from a MIDI keyboard
keys-tui preset="Nord Stage" midi="all":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p keys --features pipewire --example keys_tui -- --preset "{{preset}}" --midi "{{midi}}"

# Keys rig integration test: open the rig headless, inject MIDI through the
# ALSA loopback, and assert the rig both saw the events and made sound
# (midi_recent + master_peak). Needs pipewire + the sample libraries; exits
# nonzero on a deaf or silent rig.
keys-test:
    cargo build --release -p keys --example midi_probe
    PIPEWIRE_PROPS='{ application.name = FTS-KeysTest }' ./target/release/examples/midi_probe

# Play the City Grand physically-modeled piano from a MIDI keyboard.
# Voice loads its param table from ~/.config/signal/city-grand/table.json
# (regenerate with `pm sweep` in research/piano-model).
# NOTE: --midi targets ONE port by name (substring). "all" used to be a trap —
# midir allocated an ALSA sequencer queue per port (~24 on this rig), past
# ALSA's ~32-queue limit — but the native PipeWire backend opens ONE node
# however many devices are linked. `just piano <name>` picks a keyboard.
piano midi="KONTROL":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p keys --features pipewire --example keys_tui -- --preset "City Grand" --midi "{{midi}}"

# Play City Wurli — the vendored physically-modeled Wurlitzer 200A (openwurli,
# GPL, personal use) from a MIDI keyboard (TUI). Same ALSA-queue caveat as
# `just piano`: --midi targets ONE port by name, never "all".
wurli midi="KONTROL":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p keys --features pipewire --example keys_tui -- --preset "City Wurli" --midi "{{midi}}"

# Play Cinematic Studio Strings — 1st Violins from a MIDI keyboard (TUI).
strings lib="" midi="all" artic="Leg" mic="Mix":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p sampler --features pipewire --example strings_tui -- {{ if lib != "" { "--lib '" + lib + "'" } else { "" } }} --midi "{{midi}}" --artic "{{artic}}" --mic "{{mic}}"

# Open a saved rig by name (TUI with meters + patch switching).
# --release is REQUIRED for real-time (the vendored NAM C++ core xruns in
# unoptimized builds; the dev profile optimizes deps, release is safest).
rig name:
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p sampler --features pipewire --example guitar_tui -- --rig "{{name}}"

# List audio devices + channel counts (find your interface name)
rig-devices:
    cargo run -p sampler --example guitar_rig -- --list

# Configure + remember a rig's interface / channel / profile, e.g.:
#   just rig-setup "Guitar Rig" --input "Yamaha TF" --channel 3 --profile /path/to.styx
rig-setup name *args:
    cargo run --release -p sampler --example guitar_rig -- --rig "{{name}}" {{args}} --write-config

# ── CSS A/B (orchestral sampling) ────────────────────────────────────────

css_pack := '/run/media/AudioHaven/Signal/Libraries/Proxy/Orchestral/Cinematic Studio Strings/1st Violins/Legato/1st Violins - Legato - Mix.signalpack'

# Score the CSS legato engine against the real Kontakt reference render.
css-ab *ARGS:
    cargo build -p fts-cli --bin fts
    # Score the binary we just BUILT. score.py defaults to ./target/debug/fts,
    # so under a CARGO_TARGET_DIR override it would silently score a stale one.
    python3 features/sampler/tests/css-ab/score.py \
        --fts "${CARGO_TARGET_DIR:-target}/debug/fts" \
        --pack {{quote(css_pack)}} --json scratch/css-ab/score.json {{ARGS}}

# Same, restricted to sections (e.g. `just css-ab-sections S10,S13`).
css-ab-sections SECTIONS:
    just css-ab --sections {{SECTIONS}}

