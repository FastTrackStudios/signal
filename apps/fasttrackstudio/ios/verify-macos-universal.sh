#!/usr/bin/env bash
# Confirm a macOS release artifact actually IS the universal (arm64 +
# x86_64) build it claims to be — run this on airlock right after
# deploy-macos.sh / deploy-macos-plugins.sh, or point it at a downloaded
# release asset. Exits non-zero on any hard failure; run with `set -x` or
# check /tmp/fts-verify-*.log for the exec smoke test's captured output.
#
# Usage:
#   verify-macos-universal.sh app [DMG_PATH]        # defaults to the newest
#   verify-macos-universal.sh plugins [ZIP_PATH]    # target/*-macos.{dmg,zip}
#
# What it checks:
#   - codesign --verify (the whole bundle, deep+strict)
#   - app only: spctl assessment (Gatekeeper would accept it) + notarization
#     ticket present (dmg stapled, or an online check succeeds)
#   - every Mach-O file in the bundle reports BOTH arm64 and x86_64 via
#     `lipo -archs` — single-arch files are reported as WARNINGS, not hard
#     failures (some vendored dylib might legitimately not have a per-arch
#     build; see deploy-macos.sh's own per-file lipo warnings)
#   - app only: actually EXECUTES the main binary in both arch personalities
#     (native arm64, and x86_64 via Rosetta if installed) and watches for
#     loader-level failures (bad CPU type, dyld symbol errors, illegal
#     instruction) in the first few seconds — NOT a full functional test,
#     just "does this slice load and start running at all"
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
MODE="${1:-}"
ARTIFACT="${2:-}"

FAILURES=0
WARNINGS=0
fail() {
    echo "  FAIL: $*"
    FAILURES=$((FAILURES + 1))
}
warn() {
    echo "  WARN: $*"
    WARNINGS=$((WARNINGS + 1))
}
pass() {
    echo "  OK: $*"
}

# Walk every regular file under $1 (an .app, .vst3, or .clap bundle
# directory), skip non-Mach-O files (`lipo -archs` fails cleanly on those),
# and require both arm64 and x86_64 slices on the rest. Single-arch Mach-Os
# are WARNINGS (matches deploy-macos.sh's own defensive per-file lipo —
# some vendored binary might genuinely not have gotten merged).
check_universal_tree() {
    local root="$1"
    local checked=0
    while IFS= read -r -d '' f; do
        local archs
        archs="$(lipo -archs "$f" 2>/dev/null)" || continue
        checked=$((checked + 1))
        if echo "$archs" | grep -q 'arm64' && echo "$archs" | grep -q 'x86_64'; then
            :
        else
            warn "not universal (archs: $archs): ${f#"$root"/}"
        fi
    done < <(find "$root" -type f -print0)
    pass "checked $checked Mach-O file(s) under $(basename "$root")"
}

case "$MODE" in
app)
    [ -n "$ARTIFACT" ] || ARTIFACT="$(ls -t "$ROOT"/target/FastTrackStudio-*-macos.dmg 2>/dev/null | head -1)"
    [ -n "$ARTIFACT" ] && [ -f "$ARTIFACT" ] || { echo "ERROR: no dmg found (pass a path, or build one first)"; exit 1; }
    echo "=== verifying app: $ARTIFACT ==="

    MOUNT="$(mktemp -d)"
    hdiutil attach -nobrowse -quiet -mountpoint "$MOUNT" "$ARTIFACT"
    trap 'hdiutil detach -quiet "$MOUNT" >/dev/null 2>&1 || true' EXIT
    APP="$(find "$MOUNT" -maxdepth 1 -iname '*.app' | head -1)"
    [ -n "$APP" ] || { fail "no .app inside the dmg"; exit 1; }
    echo "  app: $(basename "$APP")"

    # ── codesign / Gatekeeper / notarization ────────────────────────────
    if codesign --verify --deep --strict "$APP" 2>/tmp/fts-verify-codesign.log; then
        pass "codesign --verify --deep --strict"
    else
        fail "codesign verify failed — see /tmp/fts-verify-codesign.log"
    fi

    if spctl --assess --type execute -vv "$APP" 2>/tmp/fts-verify-spctl.log; then
        pass "spctl assessment (Gatekeeper would accept it)"
    else
        fail "spctl assessment failed — see /tmp/fts-verify-spctl.log"
    fi

    if xcrun stapler validate "$ARTIFACT" >/tmp/fts-verify-staple.log 2>&1; then
        pass "notarization ticket stapled to the dmg"
    else
        warn "dmg isn't stapled (or stapler couldn't reach Apple) — see /tmp/fts-verify-staple.log"
    fi

    # ── universal architecture coverage ──────────────────────────────────
    MAIN_EXE="$(find "$APP/Contents/MacOS" -maxdepth 1 -type f | head -1)"
    [ -n "$MAIN_EXE" ] || { fail "no main executable in Contents/MacOS"; exit 1; }
    check_universal_tree "$APP"

    # ── execution smoke test: does each arch slice actually load? ────────
    # `--engine` is the headless mode (no webview/display needed); it does
    # try to open an audio device, but a dyld/loader failure surfaces
    # immediately regardless — we're checking "does this slice execute at
    # all", not full engine functionality.
    TEST_PORT="127.0.0.1:41499"
    smoke_test_arch() {
        local label="$1"; shift
        local log; log="$(mktemp)"
        echo "  running ($label)..."
        SIGNAL_ENGINE_ADDR="$TEST_PORT" "$@" "$MAIN_EXE" --engine >"$log" 2>&1 &
        local pid=$!
        sleep 3
        if kill -0 "$pid" 2>/dev/null; then
            pass "$label: process is alive after 3s"
            kill "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        else
            local code=0
            wait "$pid" 2>/dev/null || code=$?
            if grep -qiE 'bad cpu type|dyld.*symbol not found|illegal instruction|killed: *9|trace/bpt trap' "$log"; then
                fail "$label: loader-level failure (exit $code) — $(grep -iE 'bad cpu type|dyld.*symbol not found|illegal instruction|killed: *9|trace/bpt trap' "$log" | head -1)"
            else
                warn "$label: exited early (exit $code, likely no audio hardware on this machine) — check $log if unexpected"
            fi
        fi
    }
    smoke_test_arch "arm64 (native)"
    if arch -x86_64 /usr/bin/true >/dev/null 2>&1; then
        smoke_test_arch "x86_64 (Rosetta)" arch -x86_64
    else
        warn "Rosetta 2 not installed — skipping x86_64 execution smoke test (run: softwareupdate --install-rosetta --agree-to-license)"
    fi
    ;;

plugins)
    [ -n "$ARTIFACT" ] || ARTIFACT="$(ls -t "$ROOT"/target/fts-plugins-*-macos.zip 2>/dev/null | head -1)"
    [ -n "$ARTIFACT" ] && [ -f "$ARTIFACT" ] || { echo "ERROR: no plugin zip found (pass a path, or build one first)"; exit 1; }
    echo "=== verifying plugin bundle: $ARTIFACT ==="

    STAGE="$(mktemp -d)"
    trap 'rm -rf "$STAGE"' EXIT
    ditto -x -k "$ARTIFACT" "$STAGE"
    shopt -s nullglob
    BUNDLES=("$STAGE"/*.clap "$STAGE"/*.vst3)
    shopt -u nullglob
    [ "${#BUNDLES[@]}" -gt 0 ] || { fail "no .clap/.vst3 bundles found in the zip"; exit 1; }
    echo "  ${#BUNDLES[@]} bundles"

    for b in "${BUNDLES[@]}"; do
        [ -e "$b" ] || continue
        if codesign --verify --deep --strict "$b" 2>/tmp/fts-verify-plugin-codesign.log; then
            pass "codesign: $(basename "$b")"
        else
            fail "codesign failed: $(basename "$b") — see /tmp/fts-verify-plugin-codesign.log"
        fi
        check_universal_tree "$b"
    done
    ;;

*)
    echo "usage: $0 app [DMG_PATH] | plugins [ZIP_PATH]" >&2
    exit 2
    ;;
esac

echo
echo "=== summary: $FAILURES failure(s), $WARNINGS warning(s) ==="
[ "$FAILURES" -eq 0 ] || exit 1
