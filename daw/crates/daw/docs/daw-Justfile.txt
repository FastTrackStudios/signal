# daw workspace — build & symlink recipes
#
# Two REAPER installations:
#   reaper1 = ~/.config/REAPER               (main)
#   reaper2 = ~/.config/FastTrackStudio/Reaper (FTS)
#
# The bridge plugin (.so) goes into UserPlugins/.
# Guest extension binaries go into UserPlugins/fts-extensions/.

reaper1 := env("HOME") / ".config/REAPER"
reaper2 := env("HOME") / ".config/FastTrackStudio/Reaper"

fts_repo := parent_dir(justfile_dir()) / "FastTrackStudio"
daw_target := justfile_dir() / "target/debug"
fts_target := fts_repo / "target/debug"

# Default — show recipes
_default:
    @just --list

# ── Build ────────────────────────────────────────────────────────────────

# Build the daw-bridge REAPER plugin
build-bridge:
    cargo build -p daw-bridge

# Build the guest example extension
build-guest:
    cargo build -p daw-guest-example

# Build everything in the daw workspace
build:
    cargo build

# Check that everything compiles (fast, no codegen)
check:
    cargo check --workspace

# ── Native app ───────────────────────────────────────────────────────────

# Run the native (Blitz/GPU) FastTrackStudio app via the Dioxus CLI with RSX
# hot-reload: the daw-standalone engine rendered through the themed
# DawWorkspace panels (ArrangeView + TCP sidebar over the MixerControlPanel).
# Plain `dx serve` works because daw-native declares an (empty) `native`
# feature dx autodetects the platform from — see apps/daw/native/Cargo.toml.
# The dev shell wires hardware-GPU discovery (see flake.nix); the CI shells
# use lavapipe instead.
#
# (The app itself prefers native Wayland when a compositor socket exists —
# see `prefer_wayland()` in apps/daw/native/src/main.rs. Under XWayland the
# initial tiling resize races blitz's viewport init and the layout sticks at
# 800x600.)
native:
    dx serve -p daw-native

# Same, release-optimized (smoother metering / animation).
native-release:
    dx serve -p daw-native --release

# Plain cargo run, no dx/hot-reload — the upstream blitz way (their justfile
# never uses dx for native; dioxus-native's devtools connect is a no-op when
# the DIOXUS_DEVSERVER_* env vars aren't set). Faster cold start: reuses the
# normal cargo target dir instead of dx's separate profile.
native-run *ARGS:
    cargo run -p daw-native {{ARGS}}

# Run unit tests
test:
    cargo test --workspace

# Install REAPER instance rigs (wrapper scripts, icons, .desktop entries)
setup-rigs *ARGS:
    cargo xtask setup-rigs {{ARGS}}

# Run REAPER integration tests (spawns REAPER headless via fts-test)
integration-test *ARGS:
    cargo xtask reaper-test {{ARGS}}

# Run REAPER integration tests with the GUI visible AND leave REAPER
# open after they finish so you can inspect Actions, menus, dock state,
# etc. Locally (CI unset) the runner uses the real GUI REAPER —
# `--keep-open` stops it from being killed at the end of the run.
#
# Skips loading the FTS guest extensions (sync, dynamic-template,
# session, signal, etc.) so the action_registry surface is the only
# thing being driven. The guests' periodic poll loops slow REAPER's
# main thread down enough to break the per-test 60s timeout.
# To include them, run `cargo xtask reaper-test --keep-open` directly.
#
# Examples:
#   just gui-test                       # run everything
#   just gui-test register_toggle       # filter
#   just gui-test reaper_action_registry
gui-test *ARGS:
    FTS_EXTENSION_WHITELIST=__none__ \
    REAPER_TEST_TIMEOUT_SECS=300 \
    cargo xtask reaper-test --keep-open {{ARGS}}

# Run UI / panel tests (requires GPU adapter capable of vello's
# storage-buffer compute pipelines — real discrete GPU or the host's
# integrated GPU). The devshell ships mesa lavapipe but that does NOT
# satisfy vello's compute requirements, so headless CI will need
# either a real GPU or vello-cpu/vello-hybrid backend selection.
# Tests gated by FTS_GPU_TESTS=1; without it they skip cleanly.
ui-tests *ARGS:
    FTS_GPU_TESTS=1 cargo test -p daw-reaper-dioxus --tests {{ARGS}}

# Full CI suite: unit tests + REAPER integration tests
ci:
    @echo "=== unit tests ==="
    cargo test --workspace
    @echo ""
    @echo "=== integration tests ==="
    cargo xtask reaper-test
    @echo ""
    @echo "=== all tests passed ==="

# ── Symlink ──────────────────────────────────────────────────────────────

# Symlink the bridge plugin + guest extension into both REAPER installs
link: link-bridge link-guest

# Symlink the daw-bridge .so into both REAPER UserPlugins
link-bridge:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{daw_target}}/libreaper_daw_bridge.so"
    if [[ ! -f "$src" ]]; then
        echo "error: bridge not built — run: just build-bridge"
        exit 1
    fi
    for rdir in "{{reaper1}}" "{{reaper2}}"; do
        dest="$rdir/UserPlugins/reaper_daw_bridge.so"
        mkdir -p "$(dirname "$dest")"
        ln -sf "$src" "$dest"
        echo "linked: $dest -> $src"
    done

# Symlink the daw-guest example into both fts-extensions dirs
link-guest:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{daw_target}}/daw-guest"
    if [[ ! -f "$src" ]]; then
        echo "error: guest not built — run: just build-guest"
        exit 1
    fi
    for rdir in "{{reaper1}}" "{{reaper2}}"; do
        dest="$rdir/UserPlugins/fts-extensions/daw-guest"
        mkdir -p "$(dirname "$dest")"
        ln -sf "$src" "$dest"
        echo "linked: $dest -> $src"
    done

# Symlink FTS extensions (sync, signal, session, etc.) into both fts-extensions dirs
link-extensions:
    #!/usr/bin/env bash
    set -euo pipefail
    # binary-name : package-name
    extensions=(
        "sync:sync-extension"
        "signal:signal-extension"
        "session:session-extension"
        "input:input-extension"
        "keyflow:keyflow-extension"
        "dynamic-template:dynamic-template-extension"
    )
    for entry in "${extensions[@]}"; do
        bin="${entry%%:*}"
        src="{{fts_target}}/$bin"
        if [[ ! -f "$src" ]]; then
            echo "skip: $bin (not built — cargo build -p ${entry##*:})"
            continue
        fi
        for rdir in "{{reaper1}}" "{{reaper2}}"; do
            dest="$rdir/UserPlugins/fts-extensions/$bin"
            mkdir -p "$(dirname "$dest")"
            ln -sf "$src" "$dest"
            echo "linked: $dest"
        done
    done

# Symlink everything (bridge + guest + FTS extensions) into both REAPER installs
link-all: link-bridge link-guest link-extensions

# ── Status ───────────────────────────────────────────────────────────────

# Show current symlink state for both REAPER installs
status:
    #!/usr/bin/env bash
    for rdir in "{{reaper1}}" "{{reaper2}}"; do
        echo "=== $rdir ==="
        echo "-- UserPlugins --"
        ls -la "$rdir/UserPlugins/"*bridge* 2>/dev/null || echo "  (no bridge)"
        echo "-- fts-extensions --"
        ls -la "$rdir/UserPlugins/fts-extensions/" 2>/dev/null || echo "  (none)"
        ignore="$rdir/UserPlugins/fts-extensions/.fts-ignore"
        if [[ -f "$ignore" ]]; then
            echo "-- .fts-ignore --"
            cat "$ignore"
        fi
        echo ""
    done
