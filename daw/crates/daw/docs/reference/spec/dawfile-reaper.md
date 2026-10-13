# dawfile-reaper Specification

Requirements for the REAPER file format parser, serializer, and project manipulation library.

## RPP Parsing

r[rpp.parse.project]
Parse a complete `.RPP` file into a typed `ReaperProject` struct, extracting version, properties, tracks, items, markers, regions, tempo envelope, and ruler lanes.

r[rpp.parse.chunk-tree]
Parse any RPP text into a generic `RChunk` tree that preserves all tokens losslessly. The tree must support round-trip: parse → stringify → re-parse produces an identical tree.

r[rpp.parse.track]
Parse `<TRACK>` blocks into typed `Track` structs with all fields: name, volume/pan, mute/solo, folder state, items, FX chains, envelopes, receives, hardware outputs.

r[rpp.parse.item]
Parse `<ITEM>` blocks into typed `Item` structs with position, length, fades, takes, source blocks (WAVE, MIDI, MP3, etc.), and play rate settings.

r[rpp.parse.tempo]
Parse `<TEMPOENVEX>` blocks into `TempoTimeEnvelope` with tempo points including position, BPM, shape, time signature encoding, and metronome pattern.

r[rpp.parse.markers]
Parse `MARKER` lines into `MarkerRegionCollection` with separate marker and region lists. Regions are pairs of MARKER lines with the same ID. Support lane assignment (v7.62+).

r[rpp.parse.fx-chain]
Parse `<FXCHAIN>` blocks into typed `FxChain` with plugins (VST2/VST3/AU/JS/CLAP), containers (REAPER 7.0+), bypass state, and parameter envelopes.

r[rpp.parse.lenient]
When strict track parsing fails (e.g., malformed VOLPAN), fall back to lenient parsing that extracts fields it can and skips errors. Must handle both `Content` and `RawLine` block variants from the fast parser.

## RPP Serialization

r[rpp.write.chunk-tree]
Serialize an `RChunk` tree back to RPP text via `stringify_rpp_node` / `write_rpp`. Output must be idempotent: stringify → re-parse → stringify produces identical text.

r[rpp.write.fx-chain]
Serialize `FxChain` → RPP text via `to_rpp_string()`. Must round-trip: parse → serialize → re-parse preserves all plugins, bypass state, and parameters.

## Round-trip Fidelity

r[roundtrip.structural]
After parse → serialize → re-parse, all structural element counts must match: tracks, items, sources, markers, tempo points, GUIDs, fades, volume/pan settings.

r[roundtrip.semantic]
After parse → serialize → re-parse, all typed `ReaperProject` fields must match: track names, IDs, colors, folder structure, item positions/lengths/names/sources, marker names/positions, tempo points.

r[roundtrip.idempotent]
The second serialize pass must produce byte-identical output to the first. Quote normalization is allowed on the first pass (the tokenizer strips unnecessary quotes), but subsequent passes must be stable.

## RPL Parsing

r[rpl.parse]
Parse `.RPL` (REAPER Project List) files where each line is a path to an RPP file. Resolve relative paths against the RPL file's parent directory.

r[rpl.song-name]
Extract clean song names from RPP file paths by stripping the extension and any trailing `[...]` bracketed content (e.g., `"Belief - John Mayer [Battle SP26].RPP"` → `"Belief - John Mayer"`).

## Song Bounds

r[bounds.resolve]
Resolve song bounds from markers using priority chain: PREROLL → COUNT-IN → =START → SONGSTART → first section region → 0.0 for start; POSTROLL → =END → SONGEND → last section region → last marker for end.

r[bounds.content-extent]
Compute the full content extent of a project as the maximum of: item ends (position + length), marker/region endpoints, and tempo envelope point positions.

## RPP Combiner

r[combine.rpl-to-rpp]
Read an RPL file, parse each referenced RPP, determine content extent, and produce a single combined RPP with all songs laid out sequentially on a shared timeline.

r[combine.sequential-layout]
Songs must be laid out sequentially with no overlap. Each song starts at the end of the previous song (plus optional gap). `local_start` = 0 so all content from position 0 is included.

r[combine.gap-measures]
Support gap between songs specified in measures. Gap duration is computed from the **next** song's tempo and time signature using `measures_to_seconds()`.

r[combine.tempo-concat]
Concatenate TEMPOENVEX blocks from all projects with proper time offsets. Strip the original TEMPOENVEX from the first project's header and write a new combined envelope.

r[combine.tempo-boundary]
Insert square-shape (shape=1) tempo boundary points at each song transition. Force the first point of each song to shape=1. Set `DEFSHAPE 1` (square default). This prevents accidental gradual interpolation between songs.

r[combine.tempo-preserve-internal]
Preserve original tempo shapes for internal points within each song. Only boundary points and first-of-song points are forced to square.

r[combine.file-paths]
Resolve all relative `FILE` paths to absolute by joining with the source RPP's parent directory. Handle quoted paths with trailing flags (e.g., `FILE "Media/file.mp3" 1`). No media files should be offline in the combined output.

r[combine.markers]
Copy markers from each source project with time offsets applied. Add SONG-lane regions spanning each song's allocated range. Classify markers into lanes (SECTIONS, MARKS, SONG, START/END).

r[combine.song-folders]
Wrap each song's tracks in a folder track named after the song. The last track in each song closes the folder with `ISBUS 2 -1`.

r[combine.raw-pipeline]
Use the raw text manipulation pipeline (`concatenate_rpp_files_raw`) to preserve ALL data: FX chains, MIDI data, plugin state, envelopes, fades, sends, takes. Only patch `POSITION` and `FILE` lines.

## Track Organization

r[track-ops.wrap-in-folder]
Wrap a `Vec<Track>` inside a new folder track. Compute net folder depth of inner tracks (excluding the last) and set the closing track's indentation to close all open levels plus the new folder.

r[track-ops.group-into-folder]
Extract tracks at given indices from a flat track list, wrap them in a new folder, and insert the folder group at the position of the first extracted track.

r[track-ops.group-by-predicate]
Group tracks matching an arbitrary predicate into a new folder. Support name-based matching via `group_by_names`.

r[track-ops.move-into-existing]
Move tracks into an existing folder by inserting them before the folder's closing track. Scan forward from the folder parent to find the matching close by tracking depth.

r[track-ops.fts-hierarchy]
Organize a flat track list into the canonical FTS project hierarchy: Click+Guide → Keyflow → TRACKS → Reference (with Stem Split sub-folder). Classify tracks by name and content (guide names, keyflow names, stem patterns, mp3/mix detection).

## CLI

r[cli.combine]
`daw combine <input.RPL> [-o output.RPP] [--gap N]` command that combines multiple REAPER projects. Default output path: same directory as input with `.RPP` extension. Print song summary table with positions and durations.

## Beat Position Calculation

r[beat-pos.tempo-changes]
Calculate beat positions with tempo and time signature changes. Accumulate beats across segments between change points, handling measure/beat overflow correctly.

r[beat-pos.float-snap]
Snap near-integer beat fractions (within 1e-9 of an integer) to exact values before the overflow check. Prevents floating-point drift from causing missed beat/measure boundaries.
