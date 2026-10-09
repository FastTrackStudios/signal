#!/usr/bin/env bash
# Ship a rig library with the app: merge its entries into the default config
# (examples/ship_library.rs), then copy every capture and cab IR they
# reference into models/ and rewrite the paths to `models/<file>`
# (rig-dir-relative, so they resolve on any machine; see StyxDir::resolve).
#
#   default-config/ship-library.sh [library dir]   (default ~/.config/signal/rig)
#
# The binary embeds what lands here (build.rs lists models/, frozen/ and
# profiles/), and a library that has never had an entry gets it on its next
# start (seed_compositions, seed_songs_and_setlists, seed_profiles).
set -euo pipefail

src=${1:-$HOME/.config/signal/rig}
dst=$(cd "$(dirname "$0")" && pwd)

# Merge: what the shipped files lack is added by name; what they have stays
# (the golden tests hold the shipped Blues rig to its record — rewrite it
# with UPDATE_GOLDEN=1 when a take changes it). Worship is taken whole — it
# is the iPad's and the laptop's default — and so is Blues, its five stacks.
root=$(cd "$dst/../../../.." && pwd)
(cd "$root" && cargo run -q -p signal-guitar --example ship_library -- "$src" "$dst" --take Worship --take Blues)

# Every absolute capture or IR path, copied in flat and rewritten.
mkdir -p "$dst/models"
refs=$(grep -ohE '"/[^"]+\.(nam|wav)"' "$dst"/*.styx "$dst"/profiles/*.styx | tr -d '"' | sort -u)
while IFS= read -r ref; do
  [ -z "$ref" ] && continue
  name=$(basename "$ref")
  if [ ! -f "$ref" ]; then
    echo "missing: $ref" >&2
    exit 1
  fi
  cp "$ref" "$dst/models/$name"
done <<< "$refs"
# Rewrite: any absolute path to a .nam/.wav → models/<file>.
perl -pi -e 's#"/[^"]*/([^"/]+\.(?:nam|wav))"#"models/$1"#g' "$dst"/*.styx "$dst"/profiles/*.styx

# The frozen Cores the presets play.
mkdir -p "$dst/frozen"
[ -d "$src/frozen" ] && cp "$src"/frozen/*.nam "$dst/frozen/" 2>/dev/null || true

echo "shipped $(ls "$dst/models" | wc -l | tr -d ' ') models, $(ls "$dst/profiles" | wc -l | tr -d ' ') profiles ($(du -sh "$dst" | cut -f1))"
