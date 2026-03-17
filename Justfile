# Justfile - Convenient commands for FastTrackStudio
# Install just: cargo install just
# Run commands: just <recipe-name>

# FTS installation root — override via .env or FTS_HOME env var
fts_home := env("FTS_HOME", env("HOME") / "Music" / "FastTrackStudio")
reaper_dir := fts_home / "Reaper"
reaper_exe := reaper_dir / "FTS-LIVE.app" / "Contents" / "MacOS" / "REAPER"

# Default recipe - show help
_default:
    @just --list

# Run tracey dashboard server
@tracey:
    cargo xtask tracey check

# Generate traceability matrix
@tracey-matrix:
    cargo xtask tracey matrix

# Extract rules from specs
@tracey-rules:
    cargo xtask tracey rules

# Show impact analysis
@tracey-impact:
    cargo xtask tracey impact

# Build spec documentation
@dodeca:
    cargo xtask dodeca build

# Serve spec documentation locally
@dodeca-serve:
    cargo xtask dodeca serve

# Watch and rebuild spec documentation
@dodeca-watch:
    cargo xtask dodeca watch

# Run Rust tests
@test:
    cargo xtask test

# Run all tests (Rust + Playwright WASM integration)
@test-all:
    cargo xtask test
    cargo xtask playwright

# Run native integration tests (spawns test-extension)
@integration:
    cargo xtask integration

# Run WASM integration tests with Playwright
@playwright *args:
    cargo xtask playwright {{ args }}

# Run Playwright tests in UI mode (for debugging)
@playwright-ui:
    cargo xtask playwright --ui

# Install Playwright and run tests
@playwright-install:
    cargo xtask playwright --install

# Build all cells
@build:
    cargo xtask build

# Run DAW standalone cell
@run:
    cargo xtask run

# Quick development workflow: build and test
@dev:
    just build
    just test

# Clean build artifacts
@clean:
    cargo clean
    cd reference/tracey && cargo clean || true
    cd reference/dodeca && cargo clean || true
    cd reference/roam && cargo clean || true

# Full check: build, test, tracey
@check:
    just build
    just test
    just tracey

# Aliases for convenience
alias t := test
alias ta := test-all
alias i := integration
alias b := build
alias r := run
alias dc := dodeca
alias tr := tracey
alias pw := playwright
alias rt := reaper-test

# ============================================================================
# REAPER Extension Development
# ============================================================================

# Run REAPER integration tests (builds extension, spawns REAPER, runs tests)
reaper-test *ARGS:
    cargo xtask reaper-test {{ARGS}}

# Build the REAPER extension
build-extension:
    #!/usr/bin/env bash
    set -euo pipefail

    # Load .env file if it exists
    if [ -f .env ]; then set -a; source .env; set +a; fi

    BUILD_MODE="${BUILD_MODE:-debug}"

    echo "🔧 Building REAPER extension (${BUILD_MODE})..."
    if [[ "$BUILD_MODE" == "release" ]]; then
        cargo build --package reaper-extension --release
    else
        cargo build --package reaper-extension
    fi
    echo "✅ Extension built"

# Create symlink to extension in REAPER's UserPlugins (for development)
link-extension: build-extension
    #!/usr/bin/env bash
    set -euo pipefail

    # Load .env file if it exists
    if [ -f .env ]; then set -a; source .env; set +a; fi

    REAPER_PATH="${REAPER_PATH:-{{reaper_dir}}}"
    EXTENSION_DIR="$REAPER_PATH/UserPlugins"
    BUILD_MODE="${BUILD_MODE:-debug}"
    BUILD_DIR="target/$BUILD_MODE"

    # Create UserPlugins directory if needed
    mkdir -p "$EXTENSION_DIR"

    # Find built extension (macOS = .dylib)
    if [[ -f "$BUILD_DIR/libreaper_fts.dylib" ]]; then
        EXTENSION_FILE="$BUILD_DIR/libreaper_fts.dylib"
        TARGET_NAME="reaper_fts.dylib"
    else
        echo "❌ Error: Extension not found in $BUILD_DIR"
        echo "💡 Expected: libreaper_fts.dylib"
        exit 1
    fi

    # Remove existing symlink/file
    rm -f "$EXTENSION_DIR/$TARGET_NAME"

    # Create symlink with absolute path
    ABS_PATH="$(cd "$(dirname "$EXTENSION_FILE")" && pwd)/$(basename "$EXTENSION_FILE")"
    ln -s "$ABS_PATH" "$EXTENSION_DIR/$TARGET_NAME"

    echo "🔗 Extension symlinked:"
    echo "   Source: $ABS_PATH"
    echo "   Target: $EXTENSION_DIR/$TARGET_NAME"

# Remove extension from REAPER
uninstall-extension:
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -f .env ]; then set -a; source .env; set +a; fi

    REAPER_PATH="${REAPER_PATH:-{{reaper_dir}}}"
    EXTENSION_DIR="$REAPER_PATH/UserPlugins"

    rm -f "$EXTENSION_DIR/reaper_fts.dylib"
    rm -f "$EXTENSION_DIR/libreaper_fts.dylib"

    echo "🗑️  Extension removed from: $EXTENSION_DIR"

# Symlink JSFX effects to REAPER's Effects/FastTrackStudio directory
link-effects:
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -f .env ]; then set -a; source .env; set +a; fi

    REAPER_PATH="${REAPER_PATH:-{{reaper_dir}}}"
    EFFECTS_DIR="$REAPER_PATH/Effects/FastTrackStudio"

    mkdir -p "$EFFECTS_DIR"

    echo "🔗 Linking JSFX effects to $EFFECTS_DIR..."

    for jsfx in effects/*.jsfx; do
        [ -f "$jsfx" ] || continue
        NAME="$(basename "$jsfx")"
        ABS_PATH="$(cd "$(dirname "$jsfx")" && pwd)/$NAME"
        ln -sf "$ABS_PATH" "$EFFECTS_DIR/$NAME"
        echo "  ✅ $NAME"
    done

# Launch REAPER (foreground, shows logs)
launch-reaper:
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -f .env ]; then set -a; source .env; set +a; fi

    REAPER_EXECUTABLE="${REAPER_EXECUTABLE:-{{reaper_exe}}}"

    if [[ ! -f "$REAPER_EXECUTABLE" ]]; then
        echo "❌ REAPER not found: $REAPER_EXECUTABLE"
        exit 1
    fi

    echo "🚀 Launching REAPER..."
    echo "📋 Logs will appear below. Press Ctrl+C to stop."
    echo ""

    # Change to app's Resources directory so REAPER finds its resources
    APP_DIR="$(dirname "$(dirname "$(dirname "$REAPER_EXECUTABLE")")")"
    cd "$APP_DIR/Contents/Resources"
    exec "$REAPER_EXECUTABLE"

# Build cells (session, daw-standalone, gateway-ws)
build-cells:
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -f .env ]; then set -a; source .env; set +a; fi

    BUILD_MODE="${BUILD_MODE:-debug}"

    echo "🔧 Building cells (${BUILD_MODE})..."
    if [[ "$BUILD_MODE" == "release" ]]; then
        cargo build -p session -p daw-reaper -p gateway-ws --release
    else
        cargo build -p session -p daw-reaper -p gateway-ws
    fi
    echo "✅ Cells built"

# Symlink cells to Extensions/FTS2 directory
link-cells: build-cells
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -f .env ]; then set -a; source .env; set +a; fi

    REAPER_EXECUTABLE="${REAPER_EXECUTABLE:-{{reaper_exe}}}"
    BUILD_MODE="${BUILD_MODE:-debug}"
    BUILD_DIR="target/$BUILD_MODE"

    # Calculate path to Extensions/FTS2
    # REAPER_EXECUTABLE: <fts_home>/Reaper/FTS-LIVE.app/Contents/MacOS/REAPER
    # APP_DIR:           <fts_home>/Reaper/FTS-LIVE.app
    # RESOURCE_DIR:      <fts_home>/Reaper (REAPER resource dir)
    # PARENT:            <fts_home>
    # CELLS_DIR:         <fts_home>/Extensions/FTS2
    APP_DIR="$(dirname "$(dirname "$(dirname "$REAPER_EXECUTABLE")")")"
    RESOURCE_DIR="$(dirname "$APP_DIR")"
    PARENT="$(dirname "$RESOURCE_DIR")"
    GRANDPARENT="$(dirname "$PARENT")"
    CELLS_DIR="$GRANDPARENT/Extensions/FTS2"

    echo "📁 Cells directory: $CELLS_DIR"

    # Create Extensions/FTS2 directory
    mkdir -p "$CELLS_DIR"

    # Symlink cells
    echo ""
    echo "🔗 Creating symlinks for cells..."

    for cell in "session" "daw-reaper" "gateway-ws"; do
        SOURCE="$(pwd)/$BUILD_DIR/$cell"
        TARGET="$CELLS_DIR/$cell"

        # Check if source exists
        if [[ ! -f "$SOURCE" ]]; then
            echo "⚠️  Skipping $cell (not built)"
            continue
        fi

        # Remove old symlink if exists
        if [[ -L "$TARGET" ]] || [[ -f "$TARGET" ]]; then
            rm -f "$TARGET"
        fi

        # Create new symlink
        ln -s "$SOURCE" "$TARGET"
        echo "  ✅ $cell -> $TARGET"
    done

# Build, link extension, and launch REAPER for testing
# Note: Cells (session, gateway-ws) are now managed by the fts-control desktop app
test-reaper: link-extension
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -f .env ]; then set -a; source .env; set +a; fi

    REAPER_EXECUTABLE="${REAPER_EXECUTABLE:-{{reaper_exe}}}"

    echo ""
    echo "✅ Extension built and linked"
    echo "📡 Unix socket will be at: /tmp/fts-control.sock"
    echo "💡 Run 'just run-desktop' in another terminal to start the control app"
    echo ""
    echo "🚀 Launching REAPER..."
    echo "📋 Logs will appear below. Press Ctrl+C to stop."
    echo ""

    APP_DIR="$(dirname "$(dirname "$(dirname "$REAPER_EXECUTABLE")")")"
    cd "$APP_DIR/Contents/Resources"
    exec "$REAPER_EXECUTABLE"

# Launch REAPER in the background, wait for socket, run signal integration tests, then quit REAPER.
# Logs from REAPER stream live; test output follows. Use Ctrl+C to abort early.
test-signal-reaper: link-extension
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -f .env ]; then set -a; source .env; set +a; fi

    REAPER_EXECUTABLE="${REAPER_EXECUTABLE:-{{reaper_exe}}}"
    SOCKET_PATH="/tmp/fts-control.sock"
    SOCKET_TIMEOUT=30

    APP_DIR="$(dirname "$(dirname "$(dirname "$REAPER_EXECUTABLE")")")"

    # Clean up any stale socket from a previous run
    rm -f "$SOCKET_PATH"

    echo ""
    echo "🚀 Launching REAPER in background..."
    cd "$APP_DIR/Contents/Resources"
    "$REAPER_EXECUTABLE" &
    REAPER_PID=$!
    echo "   PID: $REAPER_PID"

    # Ensure REAPER is killed when this script exits (Ctrl+C or test failure)
    trap "echo ''; echo '🛑 Stopping REAPER (PID $REAPER_PID)...'; kill $REAPER_PID 2>/dev/null; wait $REAPER_PID 2>/dev/null; rm -f '$SOCKET_PATH'; echo 'Done.'" EXIT

    echo "⏳ Waiting for socket at $SOCKET_PATH (up to ${SOCKET_TIMEOUT}s)..."
    elapsed=0
    while [ ! -S "$SOCKET_PATH" ]; do
        sleep 1
        elapsed=$((elapsed + 1))
        if [ $elapsed -ge $SOCKET_TIMEOUT ]; then
            echo "❌ Timed out waiting for $SOCKET_PATH"
            exit 1
        fi
        echo -n "."
    done
    echo ""
    echo "✅ Socket ready — running signal integration tests"
    echo ""

    # Run the tests (failures exit non-zero, which triggers the EXIT trap cleanly)
    cargo test -p signal --test reaper_preset_loading -- --ignored --nocapture

# Run the fts-control desktop app
run-desktop:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "🖥️  Building and running fts-control desktop..."
    cargo run -p fts-control-desktop

# Show configured REAPER paths
show-reaper-path:
    #!/usr/bin/env bash
    if [ -f .env ]; then set -a; source .env; set +a; fi

    REAPER_PATH="${REAPER_PATH:-{{reaper_dir}}}"
    REAPER_EXECUTABLE="${REAPER_EXECUTABLE:-{{reaper_exe}}}"

    echo "📁 REAPER Path: $REAPER_PATH"
    echo "📁 UserPlugins: $REAPER_PATH/UserPlugins"
    echo "🎹 Executable:  $REAPER_EXECUTABLE"
    echo ""

    if [[ -d "$REAPER_PATH/UserPlugins" ]]; then
        echo "Installed extensions:"
        ls -la "$REAPER_PATH/UserPlugins" | grep -E "\.(so|dll|dylib)$" || echo "  (none)"
    fi
