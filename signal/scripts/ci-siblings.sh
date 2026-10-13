#!/usr/bin/env bash
# Check out the sibling repos listed in .github/siblings next to this repo
# (`../<dir>`), each at its pinned commit. Reuses a checkout that is already
# there (the self-hosted runner keeps them between runs) and fetches only
# when the commit is missing.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PARENT="$(dirname "$ROOT")"
grep -vE '^\s*(#|$)' "$ROOT/.github/siblings" | while read -r dir repo commit; do
  dest="$PARENT/$dir"
  url="https://github.com/$repo.git"
  if [ ! -d "$dest/.git" ]; then
    rm -rf "$dest"
    git clone --quiet --filter=blob:none "$url" "$dest"
  fi
  git config --global --add safe.directory "$dest" 2>/dev/null || true
  if ! git -C "$dest" cat-file -e "$commit^{commit}" 2>/dev/null; then
    attempt=1
    until git -C "$dest" fetch --quiet origin; do
      [ "$attempt" -ge 5 ] && { echo "fetch $repo failed" >&2; exit 1; }
      attempt=$((attempt + 1)); sleep 15
    done
  fi
  git -C "$dest" checkout --quiet --force "$commit"
  echo "$dir: $(git -C "$dest" rev-parse --short HEAD) ($repo)"
done
