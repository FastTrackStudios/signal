# FastTrackStudio — root workspace recipes
# Run commands: just <recipe-name>

# List recipes by default
default:
    @just --list

# ── Tailwind (app UI sheet) ──────────────────────────────────────────────
# The single compiled sheet (assets/tailwind-signal.css) is inlined by
# BOTH apps/fasttrackstudio/src/rig_view.rs (signal UI) and
# src/main.rs SessionChrome (session UI) via include_str!; rebuild it
# whenever UI-crate class usage changes. input.css @source globs scan the
# signal/session UI crates + libs/fts-ui + libs/dock.

# Point apps/fasttrackstudio/.lumen-blocks at the lumen-blocks checkout
# cargo actually resolved.
#
# input.css used to reach into $CARGO_HOME with a hardcoded /home/cody path
# and a `lumen-blocks-*/*/` wildcard. That wildcard matched EVERY checkout
# in the cache, not the pinned one — four of them here — so the compiled
# sheet depended on which revisions happened to be lying around, and
# changed on its own as the cache moved. That's what kept this file
# permanently dirty. `cargo metadata` knows the real answer; ask it.
_lumen-link:
    #!/usr/bin/env bash
    set -euo pipefail
    dir=$(cargo metadata --locked --format-version 1 2>/dev/null \
        | python3 -c 'import json,sys,os; print(next(os.path.dirname(p["manifest_path"]) for p in json.load(sys.stdin)["packages"] if p["name"]=="lumen-blocks"))')
    ln -sfn "$dir" apps/fasttrackstudio/.lumen-blocks

# Build Tailwind CSS (v4)
tailwind: _lumen-link
    cd apps/fasttrackstudio && tailwindcss -i ./input.css -o ./assets/tailwind-signal.css --minify

# Watch Tailwind CSS for changes
tailwind-watch: _lumen-link
    cd apps/fasttrackstudio && tailwindcss -i ./input.css -o ./assets/tailwind-signal.css --watch --minify

# Fail if the committed sheet isn't what the sources produce.
#
# tailwind-signal.css is `include_str!`d by rig_view.rs and main.rs, so it
# has to be committed — which means it can go stale silently when someone
# adds a class and doesn't rebuild. It was stale by ~50 classes when this
# check was written.
tailwind-check: tailwind
    #!/usr/bin/env bash
    set -euo pipefail
    if ! git diff --quiet -- apps/fasttrackstudio/assets/tailwind-signal.css; then
        echo "tailwind-signal.css is out of date — run 'just tailwind' and commit the result" >&2
        exit 1
    fi
    echo "tailwind-signal.css is up to date"

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
    cargo run --release -p fasttrackstudio -- --engine

alias rigd := signal-engine

# Stage the fts web bundle (the browser remote) for embedding: tailwind →
# dx web build (signal rigs + the session setlist remote) →
# apps/fasttrackstudio/web-dist/, which `cargo build -p fasttrackstudio
# --features embed-web` compiles into the binary (include_dir). On wasm the
# session feature is only the wire surface (session-proto clients +
# session-ui); the player itself runs in the engine. web-dist/ is gitignored.
web-stage: tailwind
    cd apps/fasttrackstudio && dx build --platform web --release --no-default-features --features signal,session
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

# Full install on this machine: everything rig-install does (release
# binary + embedded web UI + systemd unit) PLUS the `fts` CLI, PATH
# symlinks in ~/.local/bin, and desktop integration (launcher entry +
# icon) — FastTrackStudio shows up in the app menu like any other app.
install: rig-install
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p fts-cli
    install -m 755 target/release/fts ~/.local/lib/fts/fts.new
    mv -T ~/.local/lib/fts/fts.new ~/.local/lib/fts/fts
    install -d ~/.local/bin
    ln -sf ~/.local/lib/fts/fasttrackstudio ~/.local/bin/fasttrackstudio
    ln -sf ~/.local/lib/fts/fts ~/.local/bin/fts
    install -d ~/.local/share/icons/hicolor/scalable/apps
    install -m 644 apps/fasttrackstudio/assets/icon.svg \
        ~/.local/share/icons/hicolor/scalable/apps/fasttrackstudio.svg
    install -d ~/.local/share/applications
    sed "s|@BIN@|$HOME/.local/lib/fts/fasttrackstudio|" \
        apps/fasttrackstudio/assets/fasttrackstudio.desktop \
        > ~/.local/share/applications/fasttrackstudio.desktop
    update-desktop-database ~/.local/share/applications 2>/dev/null || true
    gtk-update-icon-cache ~/.local/share/icons/hicolor 2>/dev/null || true
    echo "installed: fasttrackstudio + fts in ~/.local/bin, launcher entry ready"

# Install the Task CLI: `task` on PATH (symlinked from ~/.local/lib/fts).
# Debug build on purpose — the CLI is a network-bound vox client, so
# release opt buys nothing but a much slower rebuild; debug keeps the
# "edit → just task-install → run" loop fast. Re-run after CLI changes.
task-install:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -p task-cli
    install -d ~/.local/lib/fts
    install -m 755 target/debug/task ~/.local/lib/fts/task.new
    mv -T ~/.local/lib/fts/task.new ~/.local/lib/fts/task
    install -d ~/.local/bin
    ln -sf ~/.local/lib/fts/task ~/.local/bin/task
    echo "installed: task in ~/.local/bin — re-run 'just task-install' to update"

# Install Patchbay (the PipeWire studio-routing app): release binary in
# ~/.local/lib/fts, `patchbay` on PATH, launcher entry + icon.
# dx web build of the patchbay browser remote → apps/patchbay/web-dist/,
# embedded into fts-patchbay by `--features embed-web` (patchbay-install).
patchbay-web-stage:
    cd apps/patchbay/web && dx build --platform web --release
    rm -rf apps/patchbay/web-dist
    cp -r target/dx/patchbay-web/release/web/public apps/patchbay/web-dist

patchbay-install: patchbay-web-stage
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p fts-patchbay --features embed-web
    install -d ~/.local/lib/fts
    install -m 755 target/release/fts-patchbay ~/.local/lib/fts/patchbay.new
    mv -T ~/.local/lib/fts/patchbay.new ~/.local/lib/fts/patchbay
    install -d ~/.local/bin
    ln -sf ~/.local/lib/fts/patchbay ~/.local/bin/patchbay
    install -d ~/.local/share/icons/hicolor/scalable/apps
    install -m 644 apps/patchbay/assets/icon.svg \
        ~/.local/share/icons/hicolor/scalable/apps/patchbay.svg
    install -d ~/.local/share/applications
    sed "s|@BIN@|$HOME/.local/lib/fts/patchbay|" \
        apps/patchbay/assets/patchbay.desktop \
        > ~/.local/share/applications/patchbay.desktop
    update-desktop-database ~/.local/share/applications 2>/dev/null || true
    gtk-update-icon-cache ~/.local/share/icons/hicolor 2>/dev/null || true
    # KDE keeps its own per-environment menu cache; rebuild it in the
    # session's env (a dev-shell kbuildsycoca updates the wrong cache).
    systemd-run --user --collect kbuildsycoca6 2>/dev/null || kbuildsycoca6 2>/dev/null || true
    echo "installed: Patchbay (run 'patchbay' or launch from the app menu)"

# Remove everything `just install` put on this machine: stop + remove
# the systemd unit, binaries, symlinks, launcher entry, and icon.
# User data is untouched (~/.config/fts, ~/.config/signal — the rig
# config may be a symlink into this repo; never deleted).
uninstall:
    #!/usr/bin/env bash
    set -euo pipefail
    systemctl --user stop signal-engine 2>/dev/null || true
    systemctl --user disable signal-engine 2>/dev/null || true
    rm -f ~/.config/systemd/user/signal-engine.service
    systemctl --user daemon-reload
    rm -f ~/.local/bin/fasttrackstudio ~/.local/bin/fts
    rm -rf ~/.local/lib/fts
    rm -f ~/.local/share/applications/fasttrackstudio.desktop
    rm -f ~/.local/share/icons/hicolor/scalable/apps/fasttrackstudio.svg
    update-desktop-database ~/.local/share/applications 2>/dev/null || true
    gtk-update-icon-cache ~/.local/share/icons/hicolor 2>/dev/null || true
    echo "uninstalled (user data in ~/.config/fts and ~/.config/signal kept)"

# ── Release packaging ────────────────────────────────────────────────────
# Assemble the distributable release artifacts into dist/ (what a
# codeberg release carries, and what fts-installer downloads):
#   fasttrackstudio-v<ver>-x86_64-linux.tar.gz   app + fts CLI + systemd
#       unit + desktop/icon templates + install.sh/uninstall.sh + VERSION
#   fts-installer-x86_64-linux                    standalone installer
#   SHA256SUMS                                    covers both
# Binary copies are patchelf'd to the standard /lib64 loader so they run
# outside the nix shell (target machines still need the shared libs —
# see `ldd` on the packaged binary).
release-package: web-stage
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p fasttrackstudio --features embed-web
    cargo build --release -p fts-cli
    cargo build --release -p fts-installer
    cargo build --release -p fts-extensions
    version="$(cargo pkgid -p fasttrackstudio | sed 's/.*[#@]//')"
    plat=x86_64-linux
    if command -v patchelf >/dev/null; then PATCHELF=(patchelf); else PATCHELF=(nix shell nixpkgs#patchelf -c patchelf); fi
    stage="$(mktemp -d)"; trap 'rm -rf "$stage"' EXIT
    cp target/release/fasttrackstudio target/release/fts "$stage"/
    cp target/release/fts-installer "$stage/fts-installer-$plat"
    for b in fasttrackstudio fts "fts-installer-$plat"; do
        "${PATCHELF[@]}" --set-interpreter /lib64/ld-linux-x86-64.so.2 --remove-rpath "$stage/$b"
        strip "$stage/$b"
    done
    # REAPER extension cdylib: no interpreter to patch (shared lib), just
    # rpath + symbols. install.sh drops it into ~/.config/REAPER/UserPlugins
    # when a REAPER install is present.
    cp target/release/libreaper_fts_extensions.so "$stage/reaper_fts_extensions.so"
    "${PATCHELF[@]}" --remove-rpath "$stage/reaper_fts_extensions.so"
    strip "$stage/reaper_fts_extensions.so"
    cp apps/fasttrackstudio/systemd/signal-engine.service "$stage"/
    cp apps/fasttrackstudio/assets/fasttrackstudio.desktop "$stage"/
    cp apps/fasttrackstudio/assets/icon.svg "$stage/icon.svg"
    install -m 755 apps/installer/scripts/install.sh apps/installer/scripts/uninstall.sh "$stage"/
    printf '%s\n' "$version" > "$stage/VERSION"
    mkdir -p dist
    tarball="fasttrackstudio-v$version-$plat.tar.gz"
    tar -czf "dist/$tarball" -C "$stage" \
        fasttrackstudio fts reaper_fts_extensions.so \
        signal-engine.service fasttrackstudio.desktop \
        icon.svg install.sh uninstall.sh VERSION
    mv "$stage/fts-installer-$plat" dist/
    (cd dist && sha256sum "$tarball" "fts-installer-$plat" > SHA256SUMS)
    echo "packaged:"
    ls -lh "dist/$tarball" "dist/fts-installer-$plat" dist/SHA256SUMS

# Rebuild eq-ui's embedded Tailwind (features/fx/eq/eq-ui/assets/
# tailwind.css) after class changes in eq-ui / fts-ui.
tailwind-eq:
    tailwindcss -i features/fx/eq/eq-ui/tailwind.css -o features/fx/eq/eq-ui/assets/tailwind.css --minify

# Rebuild comp-ui's embedded Tailwind (features/fx/comp/comp-ui/assets/
# tailwind.css) after class changes in comp-ui / fts-ui.
tailwind-comp:
    tailwindcss -i features/fx/comp/comp-ui/tailwind.css -o features/fx/comp/comp-ui/assets/tailwind.css --minify

tailwind-limiter:
    tailwindcss -i features/fx/comp/limiter-ui/tailwind.css -o features/fx/comp/limiter-ui/assets/tailwind.css --minify

# Rebuild trigger-ui's embedded Tailwind (features/fx/trigger/trigger-ui/
# assets/tailwind.css) after class changes in trigger-ui / fts-ui.
tailwind-trigger:
    tailwindcss -i features/fx/trigger/trigger-ui/tailwind.css -o features/fx/trigger/trigger-ui/assets/tailwind.css --minify

# Rasterize the comp editor to PNGs — every profile face, plus the Advanced
# page and a resized panel — so a GUI change can be looked at without opening
# a DAW. Same headless mount the behavioural tests drive, painted through
# blitz + vello_cpu. Shots land in target/gui-shots/comp/ (FTS_SHOTS_DIR
# overrides).
comp-shots:
    cargo test -p comp-ui --features native --test screenshots -- --nocapture

# Same for the EQ: every hardware model's faceplate, painted headless.
# Shots land in target/gui-shots/eq/.
eq-shots:
    cargo test -p eq-ui --features native --test screenshots -- --nocapture

# Every reverb family's panel, painted headless. Shots land in
# target/gui-shots/reverb/.
reverb-shots:
    cargo test -p reverb-ui --features native --test screenshots -- --nocapture

# Every delay family's panel, painted headless. Shots land in
# target/gui-shots/delay/.
delay-shots:
    cargo test -p delay-ui --features native --test screenshots -- --nocapture

# Every saturation circuit's panel, painted headless. Shots land in
# target/gui-shots/saturate/.
saturate-shots:
    cargo test -p saturate-ui --features native --test screenshots -- --nocapture

# Every modulation circuit's panel, painted headless. Shots land in
# target/gui-shots/modulation/.
modulation-shots:
    cargo test -p modulation-ui --features native --test screenshots -- --nocapture

# Bundle every FTS plugin as .clap + .vst3 (target/bundled/, names from
# bundler.toml). Debug of a single plugin: cargo run -p fts-plugin-xtask
# -- bundle -p eq-plugin
#
# On macOS this uses nice-plug-xtask's `bundle-universal` instead of
# `bundle`: it builds both aarch64-apple-darwin and x86_64-apple-darwin and
# lipo's them into one universal .clap/.vst3 per plugin — no custom lipo
# scripting needed. Requires the x86_64-apple-darwin rustc target (added to
# fts.rustToolchain for darwin — nix/modules/toolchain.nix).
plugins-bundle:
    #!/usr/bin/env bash
    set -euo pipefail
    cmd=bundle
    [ "$(uname)" = "Darwin" ] && cmd=bundle-universal
    for p in eq comp reverb delay tune modulation nam level saturate signal guide gate limiter trigger meter pitch unison; do
        cargo run -q -p fts-plugin-xtask -- "$cmd" -p "$p-plugin" --release
    done
    ls target/bundled/

# Package the plugin bundles as a single release tarball in dist/
# (fts-plugins-v<version>-x86_64-linux.tar.gz + SHA256SUMS entry).
plugins-package: plugins-bundle
    #!/usr/bin/env bash
    set -euo pipefail
    version="$(cargo pkgid -p eq-plugin | sed 's/.*[#@]//')"
    plat=x86_64-linux
    mkdir -p dist
    tarball="fts-plugins-v$version-$plat.tar.gz"
    tar -czf "dist/$tarball" -C target/bundled .
    (cd dist && sha256sum "$tarball" >> SHA256SUMS 2>/dev/null || sha256sum "$tarball" > SHA256SUMS)
    echo "packaged: dist/$tarball"

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

# Play the City Grand physically-modeled piano from a MIDI keyboard.
# Voice loads its param table from ~/.config/signal/city-grand/table.json
# (regenerate with `pm sweep` in research/piano-model).
# NOTE: --midi targets ONE port by name (substring). Do NOT use "all" here —
# it allocates an ALSA sequencer queue per port (~24 on this rig) and blows
# past ALSA's ~32-queue limit. `just piano <name>` picks a different keyboard.
piano midi="KONTROL":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p signal-keys --features pipewire --example keys_tui -- --preset "City Grand" --midi "{{midi}}"

# Play City Wurli — the vendored physically-modeled Wurlitzer 200A (openwurli,
# GPL, personal use) from a MIDI keyboard (TUI). Same ALSA-queue caveat as
# `just piano`: --midi targets ONE port by name, never "all".
wurli midi="KONTROL":
    PIPEWIRE_PROPS='{ application.name = FTS-Signal }' cargo run --release -p signal-keys --features pipewire --example keys_tui -- --preset "City Wurli" --midi "{{midi}}"

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
    cargo run --release -p signal-sampler --example guitar_rig -- --rig "{{name}}" {{args}} --write-config

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

# ── REAPER extension (apps/extensions/reaper-fts-extensions) ────────────
# The production REAPER extension stack (cdylib reaper_fts_extensions).
# Dev installs symlink the build + live-editable config into REAPER's
# resource dir; release packaging copies the .so into the tarball.
#
# Recipes live in the `reaper` module (reaper.just):
#   just reaper install     build + symlink the .so and config into $REAPER_HOME
#   just reaper build       build the release cdylib
#   just reaper uninstall   remove the symlink
#   just reaper log         tail the live extension log
mod reaper

# The UI snapshot regression gate (ui-snapshot) moved to the architect
# repo with architect-ui in the August 2026 split. Run it there:
#   just snapshot-check / snapshot-render <name> / snapshot-update

# REAPER integration tests moved into the `reaper` module:
#   just reaper integration-test        (was: just reaper-integration-test)
#   just reaper integration-test-gui
#   just reaper daw-test                (was: just daw-reaper-test)

# ── Build ────────────────────────────────────────────────────────────────

# Check the whole workspace compiles
check:
    cargo check --workspace --exclude vox-discover

# Run tests. nextest: parallel per-test binaries, much faster than
# `cargo test` on this many crates. It does NOT run doctests — use
# `just test-doc` for those.
test:
    cargo nextest run --workspace

# Doctests only — nextest can't run them (libtest owns doctests).
test-doc:
    cargo test --workspace --doc

# ── Disk / build-time hygiene ────────────────────────────────────────────
# Cargo never garbage-collects target/: every rebuild with a changed
# fingerprint leaves the old artifact behind forever. Measured in this tree:
# 56 stale copies of a single crate, 77 G of `debug/incremental`, ~1 TB of
# target/ across the worktrees. These recipes are the GC cargo doesn't have.

# Reclaim stale artifacts in THIS worktree (keeps anything touched recently).
sweep days="7":
    cargo sweep --time {{days}}
    @du -sh target

# Sweep every worktree — the thing to run when the dev disk fills up.
# Uses `git worktree list` so new worktrees are picked up automatically.
sweep-all days="7":
    #!/usr/bin/env bash
    set -euo pipefail
    before=$(du -sc $(git worktree list --porcelain | awk '/^worktree /{print $2"/target"}') 2>/dev/null | tail -1 | cut -f1)
    for w in $(git worktree list --porcelain | awk '/^worktree /{print $2}'); do
      [ -d "$w/target" ] || continue
      echo "── sweeping $w"
      cargo sweep --time {{days}} "$w" || true
    done
    after=$(du -sc $(git worktree list --porcelain | awk '/^worktree /{print $2"/target"}') 2>/dev/null | tail -1 | cut -f1)
    echo "reclaimed $(( (before - after) / 1024 / 1024 )) GiB"

# Drop incremental-compilation caches everywhere. They are pure cache —
# safe to delete, costs one non-incremental rebuild. Was 77 G in main alone.
sweep-incremental:
    #!/usr/bin/env bash
    set -euo pipefail
    for w in $(git worktree list --porcelain | awk '/^worktree /{print $2}'); do
      rm -rf "$w"/target/*/incremental "$w"/target/incremental 2>/dev/null || true
    done
    echo "incremental caches cleared"

# Where is the disk actually going? Per-worktree target/ sizes, largest first.
disk:
    #!/usr/bin/env bash
    du -sh $(git worktree list --porcelain | awk '/^worktree /{print $2"/target"}') 2>/dev/null | sort -rh

# sccache hit rate. Hits come from rebuilding a path you previously built
# (e.g. after `just sweep`); it does NOT dedupe across worktrees.
cache-stats:
    sccache --show-stats

# Why is the build slow? Writes target/cargo-timings/cargo-timing.html —
# a per-crate Gantt chart showing the critical path and link-time tail.
timings *ARGS:
    cargo build --timings {{ARGS}}
    @echo "→ target/cargo-timings/cargo-timing.html"

# ── Knowledge graph (graphify) ───────────────────────────────────────────
# Whole-repo knowledge graph for AI assistants — parses the tree with
# tree-sitter (100% local, no API calls) into graphify-out/ (graph.json +
# GRAPH_REPORT.md + interactive graph.html). graphify is bootstrapped in the
# nix dev shell (see flake.nix shellHook). Output is gitignored + regenerable;
# rebuild after large structural changes. `graph-serve` exposes it over MCP
# (wired into .mcp.json so Claude Code queries it instead of grepping cold).

# Build/refresh the repo knowledge graph (local AST + clustering, no LLM).
# --force so the graph shrinks when .graphifyignore excludes more (vendored
# trees); without it graphify refuses a rebuild that has fewer nodes.
graph:
    graphify update . --force

# Serve the knowledge graph over MCP (stdio) — used by .mcp.json
graph-serve:
    graphify-mcp --transport stdio --graph graphify-out/graph.json

# Rebuild the browser setlist player's AudioWorklet wasm bundle — a small
# RELEASE build of daw-standalone (the render graph that runs ON the audio
# thread; see the task repo's apps/web/assets/worklet/processor.js).
# wasm-bindgen-cli comes from the dev shell, pinned to the workspace
# wasm-bindgen version.
#
# CROSS-REPO since the August 2026 split: the source lives here, the built
# artifact is COMMITTED in the task repo (so plain `dx serve` / CI there
# need no extra step). Pass the path to your task checkout:
#
#   just task-worklet-wasm ../task
#
# Re-run after changing daw-standalone's audio/render/web code, then commit
# the result in the task repo.
task-worklet-wasm task_repo='../task':
    cargo build -p daw-standalone --lib \
        --target wasm32-unknown-unknown --release \
        --no-default-features --features decode,web
    test -d {{task_repo}}/apps/web/assets/worklet \
        || { echo "no worklet dir at {{task_repo}}/apps/web/assets/worklet — pass the task checkout path"; exit 1; }
    wasm-bindgen --target web --out-dir {{task_repo}}/apps/web/assets/worklet \
        --out-name daw_standalone \
        target/wasm32-unknown-unknown/release/daw_standalone.wasm

# ── CSS A/B (orchestral sampling) ────────────────────────────────────────

css_pack := '/run/media/AudioHaven/Signal/Libraries/Proxy/Orchestral/Cinematic Studio Strings/1st Violins/Legato/1st Violins - Legato - Mix.signalpack'

# Score the CSS legato engine against the real Kontakt reference render.
css-ab *ARGS:
    cargo build -p fts-cli --bin fts
    # Score the binary we just BUILT. score.py defaults to ./target/debug/fts,
    # so under a CARGO_TARGET_DIR override it would silently score a stale one.
    python3 features/sampler/signal-sampler/tests/css-ab/score.py \
        --fts "${CARGO_TARGET_DIR:-target}/debug/fts" \
        --pack {{quote(css_pack)}} --json scratch/css-ab/score.json {{ARGS}}

# Same, restricted to sections (e.g. `just css-ab-sections S10,S13`).
css-ab-sections SECTIONS:
    just css-ab --sections {{SECTIONS}}

# ── Aliases ──────────────────────────────────────────────────────────────

alias c := check
alias t := test
alias g := guitar
