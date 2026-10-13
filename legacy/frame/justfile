# frame — run inside `nix develop` (the flake provides the GUI + Vulkan libs).

# The architect tag the workspace is pinned to (keep in sync with Cargo.toml).
architect_tag := "v0.9.4"

default:
    @just --list

check:
    cargo build --workspace --all-targets --all-features

test:
    cargo test --workspace --all-features

# The face library: every plugin face rendered with the GPU effects stage to
# OUT/<set>/<face>.webp, indexed by kind in OUT/index.json (placeholders
# marked). `just faces-gallery /tmp/gallery --only saturate` for a few.
faces-gallery OUT *ARGS:
    cargo build --release -p plugin-faces -p frame-cli --features frame-cli/anyrender --bin plugin-faces --bin render_surface
    ./target/release/plugin-faces examples/plugins
    python3 scripts/face_gallery.py {{OUT}} {{ARGS}}

# Figma parity: import → render → pixel + SVG diff against Figma's own output.
parity *ARGS="--all":
    cargo xtask parity_test {{ARGS}}

# New feature from architect's zero-ceremony templates (proto + memory backend).
# Afterwards add both crates to [workspace] members.
scaffold-feature name:
    #!/usr/bin/env bash
    set -euo pipefail
    src="${XDG_CACHE_HOME:-$HOME/.cache}/frame/architect-{{architect_tag}}"
    if [ ! -d "$src" ]; then
        git clone -q --depth 1 --branch {{architect_tag}} https://github.com/FastTrackStudios/architect "$src"
    fi
    cargo run -q --manifest-path "$src/xtask/Cargo.toml" -- feature new {{name}}
