# The REAPER-facing recipes for the daw repo.
#
# `mod reaper 'features/reaper/reaper.just'` in the root Justfile has
# pointed here since the repo-root cleanup, but the file it names was
# never tracked in git — so the move that claimed to bring it here moved
# nothing, and `just` has refused to load AT ALL since. Not just these
# recipes: a missing module file is a parse error, so every recipe in the
# repo was unreachable. Rewritten from what the root Justfile's own
# comments promise.
#
# The production extension cdylib (reaper_fts_extensions) lives in the
# fts-extensions repo now, after the August 2026 split. What this repo
# builds is `daw-bridge` — the thinner host that the integration harness
# stands up, with nothing else in its UserPlugins so the dev rig's
# extensions cannot interfere with a test.

# Where a dev install puts things. REAPER's resource dir, not its
# install dir — the library goes in UserPlugins beside the user's own.
export REAPER_HOME := env_var_or_default("REAPER_HOME", if os() == "macos" { env_var("HOME") / "Library/Application Support/REAPER" } else { env_var("HOME") / ".config/REAPER" })

# cargo's cdylib suffix, which is also what REAPER scans UserPlugins for.
dylib := if os() == "macos" { "dylib" } else { "so" }

# Release because the host has an audio thread, and a debug build of the
# graph can xrun.

# Build the bridge cdylib
build:
    cargo build --release -p daw-bridge

# A symlink rather than a copy so a rebuild is live: REAPER picks up the
# new library on its next start without this recipe being run again.

# Build and link the bridge into REAPER's UserPlugins
install: build
    mkdir -p "{{REAPER_HOME}}/UserPlugins"
    ln -sf "{{justfile_directory()}}/target/release/libreaper_daw_bridge.{{dylib}}" \
        "{{REAPER_HOME}}/UserPlugins/reaper_daw_bridge.{{dylib}}"
    @echo "linked into {{REAPER_HOME}}/UserPlugins — restart REAPER to load it"

# Unlink the bridge, leaving the build alone
uninstall:
    rm -f "{{REAPER_HOME}}/UserPlugins/reaper_daw_bridge.{{dylib}}"
    @echo "unlinked from {{REAPER_HOME}}/UserPlugins"

# Follow the live extension log.
log:
    tail -f "{{REAPER_HOME}}/fts-daw-reaper.log"

# The rig is isolated from the dev install, so this is a different file
# from `just reaper log`.

# Follow the integration rig's log
test-log:
    tail -f "{{justfile_directory()}}/target/fts-reaper-test/fts-daw-reaper.log"

# The filter matches TEST NAMES, not file names, and a filter matching
# nothing passes with zero tests run — which looks exactly like success.
# Check the count.

# Run the reaper_test suites against a real REAPER, headless
daw-test FILTER="":
    cargo run --release -p daw-reaper-xtask -- {{FILTER}}

# The same with REAPER's window shown, so a test can be watched driving
# the DAW. Held open afterwards so the result can be inspected.

# Run them with REAPER visible, and leave it open
daw-test-gui FILTER="":
    cargo run --release -p daw-reaper-xtask -- --gui --keep-open {{FILTER}}

# Kept because the root Justfile's comments name them. They call the same
# harness; there is no separate runner behind them.

# Alias for daw-test
integration-test FILTER="":
    just reaper daw-test {{FILTER}}

# Alias for daw-test-gui
integration-test-gui FILTER="":
    just reaper daw-test-gui {{FILTER}}
