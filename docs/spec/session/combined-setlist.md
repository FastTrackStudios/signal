# Combined Setlist Generation

Merges multiple song RPP projects into a single REAPER project with all songs
on a shared timeline.

## Pipeline

r[combined.pipeline.overview]

The combine pipeline:
1. Collects RPP file paths (from open projects or RPL file)
2. Parses each project to determine bounds and content
3. Concatenates tracks, tempo, and markers with time offsets
4. Organizes into FTS folder hierarchy
5. Writes combined RPP to disk

## Bounds Resolution

r[combined.bounds.resolution]

`resolve_song_bounds` determines each song's start/end from markers:
- Start: earliest of PREROLL, COUNT-IN, =START, SONGSTART, first section region
- End: POSTROLL, =END, SONGEND, last section region (in priority order)

r[combined.bounds.trimming]

When `trim_to_bounds` is enabled:
- Items before `local_start` are removed or trimmed (position shifted, SOFFS updated)
- Items after `local_end` are removed or truncated
- Tempo points outside bounds are filtered

## Track Organization

r[combined.tracks.guide-merge]

Guide tracks (Click, Loop, Count, Guide) are merged across all songs into shared
header tracks. Items from each song are placed at the correct time offset.

r[combined.tracks.keyflow-merge]

Keyflow tracks (CHORDS, LINES, HITS) are merged the same way as guide tracks,
placed in a shared Keyflow folder.

r[combined.tracks.song-folders]

Content tracks appear under `TRACKS/{Song Name}/` folder hierarchy.
Each song's non-guide, non-keyflow tracks are wrapped in a folder.

r[combined.tracks.reference-split]

Reference and Stem Split tracks are separated into a top-level `Reference/` folder
with per-song sub-folders, distinct from the TRACKS folder.

## Tempo Concatenation

r[combined.tempo.envelope]

Tempo envelopes from each song are concatenated with:
- Points offset by `global_start_seconds - local_start`
- Points outside song bounds filtered
- Square shape (1) forced on first point of each song
- Time signature explicitly set on each song's first point

r[combined.tempo.boundary]

A boundary tempo point is inserted at each song's end to freeze the tempo
and prevent interpolation into the gap or next song.

r[combined.tempo.gap]

The gap between songs is measured in the ending song's tempo:
- `gap_measures` measures at the ending song's BPM and time signature
- Uses `measures_to_seconds(gap_measures, bpm, beats_per_measure)`

## Marker Organization

r[combined.markers.lanes]

All markers and regions are classified into FTS ruler lanes:
- Lane 1 (SECTIONS): section regions (Verse, Chorus, Intro, etc.)
- Lane 2 (MARKS): structural markers (SONGSTART, SONGEND, COUNT-IN)
- Lane 3 (SONG): song-spanning regions
- Lane 4 (START/END): bounds markers (=START, =END, PREROLL, POSTROLL)

r[combined.markers.preserve]

When organizing lanes, original marker data (color, GUID, flags, position)
is preserved. Only the lane assignment is modified.

## CLI

r[combined.cli.combine]

`session combine <input.RPL> [--gap N] [--trim]` combines songs offline:
- `--gap N` — gap between songs in measures (default: 2)
- `--trim` — trim to marker bounds (default: true)
- Output: `<input_stem> Combined.RPP`

r[combined.cli.organize]

`session organize <file.RPP> [--guide]` organizes a single project:
- Restructures tracks into FTS hierarchy
- `--guide` — generates Click, Count, Guide MIDI items from regions
- Organizes ruler lanes
- Resolves relative media paths to absolute
