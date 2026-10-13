# Processor — the DSP, the effects, and the plugins.
#
# `cargo check --workspace` pulls PipeWire in through daw's audio backend and
# does not build on macOS. That is inherited from signal, not new. Build the
# plugins, or the crate you are working on.

fts_plugins := "eq comp saturate delay reverb gate level limiter meter mic modulation pitch trigger tune unison"

# Everything that matters, checked.
check:
    cargo check -p eq-plugin -p comp-plugin -p saturate-plugin -p delay-plugin -p reverb-plugin

# The headless UI suites — the ones that hold the interaction contracts.
test:
    cargo test -p musical-time --features params
    cargo test -p eq-ui -p comp-ui -p saturate-ui -p delay-ui -p reverb-ui --features native


# Bundle every FTS plugin as .clap + .vst3 (target/bundled/, names from
# bundler.toml). Pass a subset to bundle only those:
#   just plugins-bundle "eq comp"
# Debug of a single plugin: cargo run -p fts-plugin-xtask -- bundle -p eq-plugin
#
# On macOS this uses nice-plug-xtask's `bundle-universal` instead of
# `bundle`: it builds both aarch64-apple-darwin and x86_64-apple-darwin and
# lipo's them into one universal .clap/.vst3 per plugin — no custom lipo
# scripting needed. Requires the x86_64-apple-darwin rustc target (added to
# fts.rustToolchain for darwin — nix/modules/toolchain.nix).
plugins-bundle plugins=fts_plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    cmd=bundle
    [ "$(uname)" = "Darwin" ] && cmd=bundle-universal
    for p in {{plugins}}; do
        cargo run -q -p fts-plugin-xtask -- "$cmd" -p "$p-plugin" --release
    done
    ls target/bundled/

# Install the bundled plugins into THIS machine's user plugin dirs, straight
# from target/bundled — no release download, no network. Linux: ~/.clap and
# ~/.vst3; macOS: ~/Library/Audio/Plug-Ins/{CLAP,VST3}. Writes the same
# manifest the release installer does, so `just plugins-uninstall` removes
# exactly this set (and replaces any stale symlink or older copy of the same
# name left over from a previous worktree).
#
# Build + install everything:        just plugins-install
# Iterate on one:                    just plugins-bundle eq && just plugins-install


# Install the bundled plugins into THIS machine's user plugin dirs, straight
# from target/bundled — no release download, no network. Linux: ~/.clap and
# ~/.vst3; macOS: ~/Library/Audio/Plug-Ins/{CLAP,VST3}. Writes the same
# manifest the release installer does, so `just plugins-uninstall` removes
# exactly this set (and replaces any stale symlink or older copy of the same
# name left over from a previous worktree).
#
# Build + install everything:        just plugins-install
# Iterate on one:                    just plugins-bundle eq && just plugins-install
plugins-install: plugins-bundle
    cargo run -q -p fts-installer -- plugins install --from target/bundled

# Symlink the built bundles into this machine's user plugin dirs, so a rebuild
# is live in REAPER (and anything else that scans them) without reinstalling.
#
# `plugins-install` copies; this points at `target/bundled` instead. The
# bundler rewrites each bundle in place at a stable path, so the link stays
# valid across rebuilds — `just plugins-bundle eq` and the next plugin scan
# sees the new binary.
#
#   just plugins-link              # every bundle currently in target/bundled
#   just plugins-bundle eq && ...  # rebuild one; the link already points at it
#
# Idempotent, and deliberately conservative: it replaces its own symlinks but
# refuses to delete a REAL directory of the same name, which would be a copy
# installed by `plugins-install` or a release. Remove those yourself first —
# a plugin folder is not somewhere to be clever with rm -rf.


# Symlink the built bundles into this machine's user plugin dirs, so a rebuild
# is live in REAPER (and anything else that scans them) without reinstalling.
#
# `plugins-install` copies; this points at `target/bundled` instead. The
# bundler rewrites each bundle in place at a stable path, so the link stays
# valid across rebuilds — `just plugins-bundle eq` and the next plugin scan
# sees the new binary.
#
#   just plugins-link              # every bundle currently in target/bundled
#   just plugins-bundle eq && ...  # rebuild one; the link already points at it
#
# Idempotent, and deliberately conservative: it replaces its own symlinks but
# refuses to delete a REAL directory of the same name, which would be a copy
# installed by `plugins-install` or a release. Remove those yourself first —
# a plugin folder is not somewhere to be clever with rm -rf.
plugins-link:
    #!/usr/bin/env bash
    set -euo pipefail
    src="$(pwd)/target/bundled"
    [ -d "$src" ] || { echo "no target/bundled — run 'just plugins-bundle' first" >&2; exit 1; }
    case "$(uname)" in
        Darwin) clap_dir="$HOME/Library/Audio/Plug-Ins/CLAP"; vst3_dir="$HOME/Library/Audio/Plug-Ins/VST3";;
        *)      clap_dir="$HOME/.clap"; vst3_dir="$HOME/.vst3";;
    esac
    mkdir -p "$clap_dir" "$vst3_dir"
    linked=0; skipped=0
    for bundle in "$src"/*.clap "$src"/*.vst3; do
        [ -e "$bundle" ] || continue
        name="$(basename "$bundle")"
        case "$name" in *.clap) dest_dir="$clap_dir";; *) dest_dir="$vst3_dir";; esac
        dest="$dest_dir/$name"
        if [ -e "$dest" ] && [ ! -L "$dest" ]; then
            printf "  SKIP  %-22s (a real copy is installed at %s)\n" "$name" "$dest_dir"
            skipped=$((skipped+1))
            continue
        fi
        rm -f "$dest"
        ln -s "$bundle" "$dest"
        printf "  link  %-22s -> %s\n" "$name" "$dest_dir"
        linked=$((linked+1))
    done
    echo "linked $linked bundle(s), skipped $skipped"

# Remove the symlinks `plugins-link` made, leaving any real installs alone.


# Remove the symlinks `plugins-link` made, leaving any real installs alone.
plugins-unlink:
    #!/usr/bin/env bash
    set -euo pipefail
    src="$(pwd)/target/bundled"
    case "$(uname)" in
        Darwin) dirs=("$HOME/Library/Audio/Plug-Ins/CLAP" "$HOME/Library/Audio/Plug-Ins/VST3");;
        *)      dirs=("$HOME/.clap" "$HOME/.vst3");;
    esac
    for dir in "${dirs[@]}"; do
        [ -d "$dir" ] || continue
        for link in "$dir"/*.clap "$dir"/*.vst3; do
            [ -L "$link" ] || continue
            target="$(readlink "$link")"
            case "$target" in "$src"/*) rm -f "$link"; echo "  unlink $(basename "$link")";; esac
        done
    done

# Prove every bundle in target/bundled actually LOADS — dlopen it, run its
# entry point, and walk its factory (apps/plugins/verify/). A plugin that
# compiles, links, and exports the right symbol can still fail in a host: a
# missing dependency or a panicking init only shows up at load time, and
# `nm` cannot see either.
#
# Works on both platforms, including their different bundle shapes (Linux
# .clap is a bare shared object, macOS .clap is a directory) and different
# VST3 entry-point names (ModuleEntry vs bundleEntry). On macOS, pass an
# arch to run the universal binaries in one personality:
#   just plugins-verify              # native
#   just plugins-verify x86_64       # the Intel half, under Rosetta

# The models are the private FastTrackStudios/mic-models repository, read from
# $FTS_MIC_MODELS, else ~/.local/share/fts/mic-models. The default mic (LD-47K)
# is built into the plugin, so this is only needed for the other thirty-nine.
# Install or update the FTS Mic models for the plugin
mic-models-install:
    #!/usr/bin/env bash
    set -euo pipefail
    dest="${FTS_MIC_MODELS:-${XDG_DATA_HOME:-$HOME/.local/share}/fts/mic-models}"
    if [ -d "$dest/.git" ]; then
        git -C "$dest" pull --ff-only
    elif [ -e "$dest" ]; then
        echo "$dest exists and is not a mic-models checkout; leaving it alone" >&2
        exit 1
    else
        mkdir -p "$(dirname "$dest")"
        git clone git@github.com:FastTrackStudios/mic-models.git "$dest"
    fi

