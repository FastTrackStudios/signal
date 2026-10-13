#!/usr/bin/env bash
# library-import.sh — a setlist into a Task org's song library.
#
#   scripts/library-import.sh "../sessions/Worship Set.setlist" [--originals]
#
# The library is keyflow's (ADR 0004 in task): each song is a `song:`
# document with its chart attached as the default arrangement, the
# setlist is a `songlist` collection of those songs in order, and the
# song's session — the .RPP, the prepared `.session`, the `.lrc`, the
# Ogg proxies and the waveform peaks — is a File Root named `session/<song-slug>`, checkpointed.
# Originals (the WAVs) go up too with --originals; without, a session
# streams from its proxies.
#
# Uses the `task` CLI, already signed in (`task auth login`). `task song
# add` also wants the org's local directory, so TASK_DATA_ROOT must name a
# data root that holds it (for Task's demo: ~/.local/share/task-demo/acme).
# Safe to run again: a song already in the setlist is not added twice,
# and its session files are re-put (unchanged ones are skipped). Env:
#   TASK      the task binary            (default: ../task/target/debug/task)
#   SESSION   the session binary         (default: target/release-fast/session)
#   REFERENCE the reference bouncer      (default: target/release-fast/reference)
#   KIND      the collection kind        (default: songlist — keyflow's)
#
# Title and artist come from the chart's first line (`Washed - Elevation
# Rhythm`), the key from its `#B`. Proxies missing on disk are made first,
# and the song's reference — its mix but the guide, what a page plays until
# the stems are asked for (`Media/Proxies/Reference.ogg`, its index and its
# waveform) — bounced when it has none.
set -euo pipefail

# Every task call gets a deadline and one retry. An upload has hung
# outright on this path (80 minutes on one file, no error on either
# side); the server keeps what landed, and a re-run skips it, so a retry
# picks up where the stuck call stopped.
STEP_TIMEOUT="${STEP_TIMEOUT:-120}"
run_task() {
    local attempt
    for attempt in 1 2; do
        "$TASK" "$@" &
        local pid=$!
        # Its output goes nowhere: inside `$(run_task …)` a watchdog still
        # holding the capture's pipe keeps the substitution open until it
        # wakes, so every captured call would last the whole timeout.
        ( sleep "$STEP_TIMEOUT"; kill -TERM "$pid" 2>/dev/null ) >/dev/null 2>&1 &
        local watchdog=$!
        disown "$watchdog" 2>/dev/null || true
        if wait "$pid"; then
            kill "$watchdog" 2>/dev/null || true
            return 0
        fi
        kill "$watchdog" 2>/dev/null || true
        echo "   task $1 $2: failed or timed out after ${STEP_TIMEOUT}s (attempt $attempt)" >&2
    done
    return 1
}

setlist="${1:?usage: library-import.sh <file.setlist> [--originals]}"
originals="${2:-}"
TASK="${TASK:-../task/target/debug/task}"
SESSION="${SESSION:-target/release-fast/session}"
REFERENCE="${REFERENCE:-target/release-fast/reference}"
KIND="${KIND:-songlist}"
base="$(cd "$(dirname "$setlist")" && pwd)"
collection="$(basename "$setlist" .setlist)"

if ! "$TASK" collection list 2>/dev/null | grep -qF "$collection"; then
    "$TASK" collection create "$collection" --kind "$KIND"
fi

# The song's chart: the one .kf that is not a kept older copy.
chart_of() {
    find "$1" -maxdepth 1 -name '*.kf' | grep -v -e '\.before' -e '\.imported' | head -1
}

while IFS= read -r line; do
    [[ -z "$line" || "$line" == \#* ]] && continue
    folder="$base/$line"
    [[ -d "$folder" ]] || { echo "skip: $line (not a folder)"; continue; }
    name="$(basename "$folder")"
    rpp="$folder/$name.RPP"
    kf="$(chart_of "$folder")"
    head1="$(head -1 "$kf")"
    title="${head1%% - *}"
    artist=""
    [[ "$head1" == *" - "* ]] && artist="${head1#* - }"
    key="$(sed -n 2p "$kf" | grep -o '#[A-Ga-g][#b]*m\?' | tr -d '#' || true)"
    echo "== $title${artist:+ — $artist} (${key:-no key})"

    if [[ ! -d "$folder/Media/Proxies" ]]; then
        "$SESSION" proxies "$rpp" 2>&1 | grep -v SWELL | tail -1
    fi
    if [[ ! -f "$folder/Media/Proxies/Reference.ogg" ]]; then
        "$REFERENCE" "$folder" 2>&1 | grep -v SWELL | tail -1
    fi

    # The slug Task derives from the title (resources' slugify: an
    # apostrophe drops out — "God, I'm" is `god-im` — and any other run of
    # non-alphanumerics is one dash). Already in the setlist, the song is
    # not added again.
    slug="$(printf '%s' "$title" | sed -e "s/['’]//g" | tr '[:upper:]' '[:lower:]' \
        | LC_ALL=C sed -e 's/[^a-z0-9]\{1,\}/-/g' -e 's/^-//' -e 's/-$//')"
    if "$TASK" collection show "$collection" | grep -q "song:$slug\$"; then
        echo "   song:$slug already in $collection"
    else
        added="$("$TASK" song add "$collection" --title "$title" ${artist:+--artist "$artist"} \
            ${key:+--key "$key"} --chart "$kf")"
        echo "$added" | grep -E '^(wrote|added)'
        slug="$(echo "$added" | sed -n 's/^added song:\([^ ]*\) to .*/\1/p')"
        [[ -n "$slug" ]] || { echo "no slug for $title"; exit 1; }
    fi

    root="$(run_task files root ensure "session/$slug" --name "$title" | awk '{print $1}')"
    run_task files put "$root" "$rpp"
    # The chart beside the project, as the song's folder has it: what a
    # player opening the song from its files reads (its `.kf`), besides the
    # copy the library keeps as the song's arrangement.
    run_task files put "$root" "$kf"
    [[ -f "$folder/$name.lrc" ]] && run_task files put "$root" "$folder/$name.lrc"
    [[ -d "$folder/$name.session" ]] && run_task files put "$root" "$folder/$name.session" --to "$name.session"
    run_task files put "$root" "$folder/Media/Proxies" --to Media/Proxies
    # The waveform caches, so a song pulled with only its proxies still
    # draws its waveforms (`session peaks` writes them, keyed by the
    # original's name).
    [[ -d "$folder/Media/Peaks" ]] && run_task files put "$root" "$folder/Media/Peaks" --to Media/Peaks
    if [[ "$originals" == "--originals" ]]; then
        run_task files put "$root" "$folder/Media" --to Media
    fi
    run_task files checkpoint "$root" --message "imported from $line"
done < "$setlist"

echo "== $collection"
"$TASK" collection list | grep -F "$collection"
