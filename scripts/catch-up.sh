#!/usr/bin/env bash
# Carry over what the old repos did after studio was cut from them.
#
# studio started from a snapshot of five repos (imported.txt says which
# commit of which branch). Until each is archived, work may still land
# there; this applies each branch's changes since the recorded commit as
# a patch into its directory here, then records the new commit. A hunk
# that no longer applies is left as a .rej file / conflict for a person.
#
# Not carried: each repo's own root Cargo.toml / Cargo.lock (folded into
# studio's root manifest by hand — the diff is printed instead), and the
# files assets.txt keeps in the asset repos.
#
#   FTS=/run/media/Development/fts scripts/catch-up.sh [repo...]
set -euo pipefail
FTS=${FTS:-/run/media/Development/fts}
cd "$(git rev-parse --show-toplevel)"
if [[ -n $(git status --porcelain --untracked-files=no) ]]; then
    echo "commit or stash first" >&2; exit 1
fi
want=("$@")
excludes=()
while read -r _repo from to; do
    [[ -z $_repo || $_repo == \#* ]] && continue
    excludes+=("${to:-$from}")
done < assets.txt
while read -r repo dir branch from; do
    [[ -z $repo || $repo == \#* ]] && continue
    if (( ${#want[@]} )) && [[ ! " ${want[*]} " == *" $repo "* ]]; then continue; fi
    src=$FTS/$repo
    to=$(git -C "$src" rev-parse "$branch")
    if [[ $to == "$from" ]]; then echo "$repo: nothing new"; continue; fi
    # Paths of this repo's files that live in studio-assets, relative to it.
    ex=()
    for f in "${excludes[@]}"; do
        if [[ $dir == . ]]; then ex+=(":(exclude)$f")
        elif [[ $f == "$dir"/* ]]; then ex+=(":(exclude)${f#"$dir"/}"); fi
    done
    pathspec=(. ":(exclude)Cargo.toml" ":(exclude)Cargo.lock" "${ex[@]}")
    [[ $dir == . ]] && pathspec=(. "${ex[@]}" ":(exclude)Cargo.lock")
    echo "$repo: ${from:0:9}..${to:0:9} ($(git -C "$src" rev-list --count "$from..$to") commits)"
    args=(--3way --whitespace=nowarn)
    [[ $dir != . ]] && args+=(--directory="$dir")
    git -C "$src" diff --binary "$from" "$to" -- "${pathspec[@]}" | git apply "${args[@]}" -
    if [[ $dir != . ]] && ! git -C "$src" diff --quiet "$from" "$to" -- Cargo.toml; then
        echo "  $repo's root Cargo.toml changed — fold this into studio's by hand:"
        git -C "$src" diff "$from" "$to" -- Cargo.toml | sed 's/^/    /'
    fi
    sed -i "s|^\($repo \+[^ ]\+ \+[^ ]\+ \+\)$from|\1$to|" imported.txt
done < imported.txt
echo "review with git status / git diff, then commit"
