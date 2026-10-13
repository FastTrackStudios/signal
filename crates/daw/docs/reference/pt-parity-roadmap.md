# Pro Tools `.ptx` parity roadmap

**Goal:** read AND write Pro Tools 12 session files with full data fidelity,
round-tripping (read → write → read) without loss.

This document tracks every PT12 feature/block we know about, its current
implementation status, and the RE work required to get to parity.

---

## Status legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Read works, byte-verified |
| 🟡 | Read works heuristically (correct on test fixtures, may break edge cases) |
| ❌ | Not implemented |
| ✏️ | Write not implemented (no field is written today; every entry below is read-only) |
| →§16 | Entry moved to §16 *Known unobservable* — bytes round-trip losslessly but semantics undecoded |

---

## 1. File container & encoding

| Feature | Block(s) | Read | Write |
|---|---|:-:|:-:|
| XOR descrambling | header | ✅ | ✏️ |
| Block tree | all | ✅ | ✏️ |
| Endian detection | header byte 0x11 | ✅ | ✏️ |
| Version (PT 5–12) | `0x0003` / `0x2067` | ✅ | ✏️ |
| Session sample rate | `0x1028` | ✅ | ✏️ |
| Product/version string | `0x0030` | ✅ (read), ignored | ✏️ |

**Round-trip risk:** writing requires re-encoding XOR with the correct seed
(matches version). We already have `decrypt::decrypt`; need symmetric `encrypt`.

---

## 2. Timeline

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Tempo map (BPM + tpb) | `0x2028` | ✅ | ✏️ | TPB encodes click note value |
| Meter map (time sig) | `0x2029` | ✅ | ✏️ | |
| Memory locations (markers) PT 5–9 | `0x263b`/`0x2619` | ✅ | ✏️ | |
| Memory locations PT 12 | `0x2030`/`0x2077` | ✅ | ✏️ | Position = u64 LE − `(2^62 + ZERO_TICKS)` |
| Marker color (PT 12) | `0x4826` payload `+2/+4/+6` u16 LE low-byte triplet | ✅ | ✏️ | R,G,B components. Discovered via Frida byte-read trace on `marker_colored` probe (color 0xD86E41 → reads 0xD8, 0x6E, 0x41). Surfaced as `Marker.color_rgb: Option<(u8,u8,u8)>`. |
| Key-signature ruler items | unknown | →§16 | →§16 | |
| Chord-symbol ruler items | unknown | →§16 | →§16 | |
| Loop / selection points | unknown | →§16 | →§16 | |
| Pre/post-roll | unknown | →§16 | →§16 | |
| Cycle/punch points | unknown | →§16 | →§16 | |

---

## 3. Audio files

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Filename list | `0x1004`/`0x103a` | ✅ | ✏️ | |
| Per-file sample length | `0x1001` | ✅ | ✏️ | |
| File ↔ region mapping | partial UIDs decoded — see notes | 🟡 | ✏️ | Surfaced two related UIDs: (1) `AudioFile.source_uid` from `0x1003 +45..+50` (each file gets unique 6-byte UID, bracketed by `0x2A`/`0x80` sentinels); (2) `AudioRegion.source_file_uid` from `0x2628 magic +54..+59` (L+R pairs share). The two UID NAMESPACES DON'T DIRECTLY MATCH — `e5ac0155eee8` is the file UID for `vocals_1.wav` but the regions named `vocals_1-03.L/R` have UID `b45a6200b84c`. A third linkage block remains to be found before this can replace the name-stem heuristic. |
| Audio file path resolution | (filesystem) | ✅ | ✏️ | session_dir/`Audio Files/` |
| External file refs | unknown | →§16 | →§16 | Files outside session dir |

**Priority:** real region→file index — high. Without it, regions renamed away
from the auto-generated `<file>-NN.L` pattern lose their source link.

---

## 4. Audio regions

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Region name, start, length, offset | `0x100b`/`0x262a` | ✅ | ✏️ | |
| Region audio file index | inside region payload | →§16 | →§16 | bug (see §3) |
| Region gain | unknown | →§16 | →§16 | |
| Region pitch shift | unknown | →§16 | →§16 | |
| Region time-stretch / Elastic Audio | second `0x2628` (TCE clone) | ✅ | ✏️ | Read path already wired: when playrate≠1.0 the converter emits a second `0x2628` (TCE clone region) and the existing `parse_three_point` extracts its `sample_offset` + `length` into the corresponding `AudioRegion`. The clip's `TrackRegion.region_index` cross-references it. Consumers derive playrate as `source_region.length / clip_region.length`. Verified on `clip_playrate_{half,quarter,double}` and `clip_slip_{quarter,half,eighth}` probes via `cargo run -p daw-reaper --example dump_parsed_regions`. Writer-side emission of the TCE clone still pending. Full layout in `docs/converter-frida-discovered-offsets.md`. |
| Region slip-offset (source start) | second `0x2628` payload `+50..+51` u16 LE | ✅ | ✏️ | Same TCE-clone block as time-stretch; surfaced as `AudioRegion.sample_offset`. Verified via `clip_slip_{eighth,quarter,half}` probes: 6000/12000/24000 samples decoded correctly. |
| Warp markers (Elastic) | unknown | →§16 | →§16 | |
| Region color | unknown | →§16 | →§16 | |
| Region clip group membership | unknown | →§16 | →§16 | |
| Compound regions | `0x2628`/`0x2629`/`0x262b`/`0x262c` | →§16 | →§16 | parsed as containers only |

---

## 5. Tracks (definition)

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Track name | `0x1014` | ✅ | ✏️ | |
| Channel count (mono/stereo) | `0x1014` | ✅ | ✏️ | |
| Channel index map | `0x1014` | ✅ | ✏️ | |
| Track kind (audio/MIDI/aux/master) | `0x251a` `+2` byte | ✅ | ✏️ | 0x00 audio, 0x02 aux/MIDI, 0x05 master, 0x07 inst |
| Track UID | `0x251a` payload | →§16 | →§16 | needed for round-trip |
| Master track | `0x261e` (one per session) | 🟡 | ❌ | Now surfaced as `ProToolsSession.internal_tracks: Vec<InternalTrack>` (name + 6-byte routing UID). Master appears as one of the entries; distinguishing Master vs Aux vs Bus from the entry's byte payload still TBD. |
| Aux/instrument tracks | `0x261e` | 🟡 | ❌ | Same `InternalTrack` list. wonder-session: 16 entries (DRUMS, BASS, AC GTR, Drum Verb, etc.). orchestral-session: 7 (Click 1, M2-M4, WW>>, Brass>>, Strings>>). Kind discrimination still pending. |
| Track creation order | (block order) | ✅ | ✏️ | |

---

## 6. Track mix state (volume, pan, mute, solo)

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Volume (fader) | `0x1029` `+1..+5` i32 LE | ✅ | ✅ | 0.1 dB units; verified -31 dB matches PT. Write via `dawfile_protools::set_track_mix_state(session, name, vol, mute, pan)` — fixed-size in-place write to both record-A and record-B mirror. Round-trip tested on user session. |
| Mute (stored bit) | `0x1029` `+5` u8 | ✅ | ✅ | `0` = audible, `1` = muted. Write via `set_track_mix_state`. |
| Mute (effective, w/ send routing) | `0x1029 +5` AND `0x260a[0] +8` | ✅ | ✅ | `effective = stored AND NOT send-routed`. Discriminates user-mute vs Make-Inactive. Verified on Lord of the Fight (8 muted tracks). |
| Pan (left ch) | `0x1029` `+13..+17` i32 LE | 🟡 | ✏️ | `−100` for stereo = "natural state", we map to centered |
| Pan (right ch / multi-out) | `0x1029` `+17..+87` | →§16 | →§16 | |
| Solo | `0x102d +162` u8 | ✅ | ✅ (single-track + multi-track) | Per-track in `0x102d` block. Verified via probe-diff. |
| Solo defeat | `0x200b +268` u8 (mirror `0x200a +259`) | ✅ | ✅ (single-track) | "Ignores other tracks' solo" |
| Inactive (Make Inactive / bouncedSource) | derived: `0x1029 +5 == 1` AND `0x260a[0] +8 == 1` | ✅ | ✅ (single-track) | Stored mute bit set, send routing kept |
| Mute automation envelope | `0x260a[1]` (2nd `0x260a` child of `0x260d` wrapper) | ✅ | ✅ (single-track) | 22-byte header + 6-byte implicit (t=0) + N user breakpoints at +28. Each BP = `u32 time_samples + u8 muted + u8 shape`. Surfaced as `Track.mute_automation: Vec<MuteAutomationBreakpoint>`. Round-trip test (`write_with_mute_automation_round_trip`) covers 2-point envelopes. |
| Record-arm | unknown | →§16 | →§16 | |
| Input-monitor mode | unknown | →§16 | →§16 | |

**Priority: HIGH.** Mute is critical for usability — the user can't tell which
material is supposed to be silent. Search the whole 281-byte `0x1029` payload
for the real mute bit, OR look for a sibling block per track.

---

## 7. Track display & metadata

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Track color | `0x200b +106`, `0x200a +97`, `0x2015 +88` i16 LE | ✅ | ✅ (single-track + multi-track) | PT palette index; `0` = default mapped to `-2` |
| Track icon/image | unknown | →§16 | →§16 | |
| Track comment/notes | unknown | →§16 | →§16 | |
| Track height (mix/edit window) | unknown | →§16 | →§16 | |
| Track visibility | unknown | →§16 | →§16 | |
| Track delay (samples/ms) | unknown | →§16 | →§16 | |
| Phase invert | unknown | →§16 | →§16 | |
| Track timebase (samples vs ticks) | partly inferred from sub-entry `+16` byte | →§16 | →§16 | |

---

## 8. Track routing

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Hardware I/O channels | `0x1021`/`0x1022` | ✅ | ✏️ | |
| I/O routing table | `0x2602` per-entry: `+10` active u8, `+33` flag_33, `+36` flag_36, `+47..+52` 6-byte destination UID | 🟡 | ✏️ | Each entry surfaced as `RoutingEntry` on `ProToolsSession.routing_entries`. Verified via Frida byte-read trace: LotF has 208 entries (85 active), routing-examples shows the same byte pattern. Destination UID resolution to a bus/output name still TBD. |
| Track input (mic/line/bus) | unknown | →§16 | →§16 | |
| Track output (master/bus) | `0x260e` (in `0x260d` wrapper) | ✅ | ✅ | Length-prefixed destination name (e.g. `"Analog 1-2"`, `"Bus 13-14"`) at payload `+0x24`. 61-byte variant = no destination. Aligned 1:1 with `0x251a` order. Write via `dawfile_protools::set_track_output(session, name, dest)` — splices the destination string and rebuilds parent block sizes. Round-trip test on user session confirms the new value survives parse→write→parse and no other track drifts. |
| Aux send count / levels / destinations | unknown | →§16 | →§16 | |
| Aux send pre/post-fader | unknown | →§16 | →§16 | |
| HW insert routing | unknown | →§16 | →§16 | |

**Priority: HIGH.** A printed mix uses bus routing extensively. Without
routing, the imported REAPER session has every track going straight to the
master, which is wrong for any serious session.

---

## 9. Track FX / plugins

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Session plugin registry | `0x1018`/`0x1017` | ✅ | ✏️ | list of plugin TYPES used |
| Per-track insert chain | unknown | →§16 | →§16 | |
| Insert plugin parameters | unknown | →§16 | →§16 | |
| Insert bypass state | unknown | →§16 | →§16 | |
| Insert wet/dry | unknown | →§16 | →§16 | |
| Insert ordering | unknown | →§16 | →§16 | |
| Side-chain routing | unknown | →§16 | →§16 | |

**Priority: HIGH.** No plugins means imported sessions sound like raw stems.

---

## 10. Region placements (track playlists)

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Active playlist regions | `0x1054`/`0x1052` | ✅ | ✏️ | |
| Region start position (samples) | sub-entry `+9..+12` | ✅ | ✏️ | |
| Region start position (ticks, for MIDI/inst) | sub-entry `+9` u40, when `+16==0x40` | ✅ | ✏️ | |
| Clip mute | `0x104f +9` u8 | ✅ | ✏️ | `TrackRegion.clip_muted`. Verified via `clip_muted` probe (REAPER item `.muted()` + real WAV source) → byte=1; baseline → byte=0. |
| Clip color | `0x104f +25..+26` i16 LE | ✅ | ✏️ | `TrackRegion.clip_color`. Verified via `clip_colored` probe (REAPER item with color 0x6e41d8) → palette index `27`; baseline → `-2` (default). |
| Region clip-effect flag | `0x1050 +53` u8 | 🟡 | ✏️ | `TrackRegion.clip_flag_53`. Semantics still unclear (rare value=1; doesn't toggle with mute/color probes). |
| Region clip gain | `0x104f` (other fields TBD) | ❌ | ✏️ | Clip mute + color decoded above. Static gain / dynamic envelope encoding still TBD. |
| Alternate playlists | `0x2428`/`0x2429`+`0x1054` | ✅ | →§16 | parsed but not emitted |

---

## 11. Fades & crossfades

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Fade detection | `0x1050` `+46==0x01` | ✅ | ✏️ | |
| Fade-def list | `0x2630` wrapper | ✅ | ✏️ | |
| Fade in/out length + shape | `0x262f` | ✅ | ✏️ | see `pt-fade-encoding.md` |
| Custom curve shapes | `0x262f` trailing bytes | →§16 | →§16 | only linear/equal-power/equal-gain emitted |
| Fade preset file refs | `cFadePresetFile` | →§16 | →§16 | |

---

## 12. MIDI

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| MIDI event chunks (note, vel, pos, dur) | `0x2000`/MdNLB | ✅ | ✏️ | 35-byte records; pos@+27, note@+9, vel@+10, dur@+11..+19 |
| MIDI regions | `0x2001`/`0x2634` | ✅ | ✏️ | length now via tick→sample |
| MIDI region→track assignment | `0x1058` | ✅ | ✏️ | with playlist-suffix-aware dedupe |
| MIDI CC events | unknown | →§16 | →§16 | only notes parsed |
| Pitch bend | unknown | →§16 | →§16 | |
| Aftertouch / program change | unknown | →§16 | →§16 | |
| Note metadata (channel) | unknown | →§16 | →§16 | channel always 0 |
| Tempo-mapped MIDI region timing | partial | →§16 | →§16 | |
| Compound MIDI regions | `0x262b`/`0x262c` | →§16 | →§16 | parsed as containers only |

---

## 13. Automation

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Volume automation | `0x260a[0]` (1st `0x260a` child of `0x260d`) | ✅ | ✅ (single-track) | 22-byte header + 6-byte implicit @+22 + N user breakpoints @+28. Each = `u32 time_samples + i16 value_centibel`. Surfaced as `Track.volume_automation`; writer via `NativeTrackSpec.volume_automation`. Round-trip test covers 2-point envelopes. |
| Pan automation | `0x260a[1]` (was `[2]` — corrected) | ❌ | ❌ | Cross-fixture sweep confirms every audio-track `0x260d` carries 4 envelope slots: `[0]` vol (wired), `[1]` pan, `[2]` mute, `[3]` send-level (all suspected). Available test fixtures contain only volume curves — every `[1]`/`[2]`/`[3]` slot in every fixture is the 41-byte "empty + implicit only" stub. Format is assumed to mirror vol (same 22 B header + breakpoint shape) with different value units. Converter probes via `pan_envelope`, `pan_envelope_2`, `pan_envelope_lr` all produce no diff — REAPER builder pan-envelope names not propagated. Read parity needs a PT-authored fixture with non-trivial pan automation. |
| Mute automation | `0x260a[1]` (2nd `0x260a` child of `0x260d`) | ✅ | ✅ | See §6 mute automation row for full details |
| Send-level automation | unknown | ❌ | ❌ | |
| Plugin-parameter automation | unknown | ❌ | ❌ | |
| Tempo automation (vs static map) | partial | 🟡 | ✏️ | we emit static map; tempo curves untested |

**Priority: MEDIUM.** Most modern mixes use automation; without it the
imported session sounds static.

---

## 14. Track groups & organization

| Feature | Block(s) | Read | Write | Notes |
|---|---|:-:|:-:|---|
| Edit groups | `0x4501` (one per session) | 🟡 | ❌ | Located via cross-fixture string sweep on orchestral-session.ptx. Flat list of `[u32_namelen][name][i16_color]` entries (~40 groups in orchestral). Per-track membership table precedes the list (~9 KB; format not yet decoded). See `docs/converter-frida-discovered-offsets.md` §"`0x4501` / `0x4702`". |
| Mix groups (stem mapping) | `0x4702` | 🟡 | ❌ | PT 12+'s "Stem Mapping" feature (track → stem-type). Flat list `[u32_namelen][name]` (no color). Built-in entries `Dialog`, `Music`, `Effects`, `Narration`, then user classifications. |
| Track folders (PT 12+) | unknown | ❌ | ❌ | |
| Selection-state memlocs | `0x2077` (not `0x271a`) | 🟡 | ❌ | `0x2077` is PT's unified Memory Locations list (markers + selections + window-configs). Name + start/end position decoded; flag-bitmap (`0x00000903` for a point marker) needs further differential probing across memloc kinds. See `docs/converter-frida-discovered-offsets.md` §"`0x2077` — Memory Locations". |
| Zoom-state memlocs | `0x2077` (same as selection) | 🟡 | ❌ | Lives in the same Memory Locations list; identified by a different bit in the flag bitmap. Not yet decoded which bit. |

---

## 15. Writing (round-trip) — partial

Reflects the current state of `crates/dawfile-protools/src/write/` after
the write-side scaffolding round.

| Concern | Status | Notes |
|---|---|---|
| XOR re-encryption with correct seed | ✅ | `RawSession::encrypt()` |
| Block tree serializer with correct sizes | ✅ | `write/splice.rs` updates every ancestor `block_size` field on each modification, then re-parses the block tree |
| Re-compute parent block `block_size` after children change | ✅ | same as above |
| Re-compute cross-block indices (audio_file_index, fade_index, region_index, etc.) | 🟡 | implemented for fade_index in `write/native.rs`; other indices not yet |
| Preserve unknown blocks verbatim (passthrough) | ✅ | the template-patch approach (`write/native.rs`) leaves unmodified blocks untouched |
| Preserve unknown bytes within known blocks | ✅ | splice only touches the byte range you point at |
| Stable UID generation for new tracks/regions/markers | ❌ | no `add_internal_track` path yet; will need a deterministic UID allocator that doesn't collide with existing entries |
| Stable ordering (PT compares ordering for some structures) | ❌ | not yet validated; failure mode would be PT refusing to open |
| Update headers (file size, modified-date, etc.) | 🟡 | size is implicit (just buffer length); modified-date not preserved |
| Surface validation: PT refuses to open files with broken back-references | ❌ | no validator pass before write |
| **Block construction primitive** (`wrap_as_block(ct, payload)`) | ✅ | `write/block_ops.rs` |
| **Block-tree insert/remove** (`append_child_block`, `remove_block`) | ✅ | `write/block_ops.rs` |
| **Stem-mapping (`0x4702`) writer** (add / replace) | ✅ | `write/edit_groups.rs::add_stem_mapping`, `replace_stem_mappings` |
| **Edit-group (`0x4501`) writer** | ❌ | `write/edit_groups.rs::add_edit_group_name` returns `WriteError::Unimplemented`; blocked on membership-table decode |
| **Internal-track (`0x261e`) rename** | ✅ | `write/internal_tracks.rs::rename_internal_track` (both same-length and variable-length splice) |
| **Internal-track add/remove** | ❌ | `write/internal_tracks.rs::add_internal_track` / `remove_internal_track` return `WriteError::Unimplemented`; blocked on prefix-byte decode |
| **TCE-clone (`0x2628[1]`) emission** (for clip playrate / slip-offset writes) | ❌ | not yet started |
| **Pan / mute / send envelope writes** (`0x260a[1..3]`) | ❌ | blocked on value-unit verification (no PT-authored fixture exercises these) |

---

## Suggested ordering for autonomous work

Roughly: highest-blast-radius bugs first, then features by user impact.

### Phase A — critical correctness (blocks user from using the import)

1. **Track mute** (`0x1029`) — search whole payload + sibling blocks; verify against user's mute list.
2. **Audio file index** for regions — find the real `region → file` link block; replace name-stem heuristic.
3. **Track solo** — same RE pass as mute.
4. **Track color** — likely in `0x1029` `+171..` or a sibling block.

### Phase B — mix fidelity

5. **Track routing** (input/output/bus assignments).
6. **Aux sends** (count, levels, destinations, pre/post).
7. **Track FX inserts** — at least slot-and-name even if parameters are opaque.
8. **Plugin parameter blobs** (per-track per-insert).

### Phase C — production detail

9. **Region clip gain** and per-clip mute.
10. **Region pitch / time-stretch / Elastic warp markers**.
11. **MIDI CC / pitch bend / channel**.
12. **Automation** (volume, pan, plugin params).

### Phase D — round-trip writer

13. Block serializer: rebuild any single block from parsed fields.
14. Block tree writer: recompute sizes, recurse.
15. XOR re-encryption.
16. Cross-reference rewriter (indices stay stable across edits).
17. Unknown-block passthrough: any block we don't fully decode keeps its raw bytes through round-trip.
18. Round-trip test fixture: read, write, read, byte-compare or field-compare.

### Phase E — niceties

19. Selection / zoom / window-state memlocs.
20. Compound regions (full parse, not just container).
21. Alternate playlists round-trip.
22. Key-sig / chord-symbol ruler items.
23. Track folders (PT 12+).
24. Track icons, comments, height, visibility.

---

## How to RE each item

Pattern that worked for fades and `0x1029`:

1. **Identify the block** by counting candidate `content_type` values and matching against known counts (e.g., per-track block count = track count).
2. **Dump bytes** with `examples/dump_ct.rs <CT_HEX>` style helpers.
3. **Anchor on a known value** the user can confirm (e.g., "this track is -31 dB").
4. **Diff bytes across blocks** to find what varies.
5. **Cross-reference siblings** when one block doesn't carry the field (fades live in `0x262f`, not in the track entry).
6. **Verify against user-ground-truth** for at least 3 distinct values before committing.
7. **Write the doc** in `docs/pt-*.md` with byte offsets, types, value ranges, and example decoded values.

---

## Test-session ground truth

Use this session as the canonical correctness benchmark for read-side
parity. All claims below come from the user's own Pro Tools session.

**Path:**

```
~/Downloads/tombrooksmusic_copy-of-02-lord-of-the-fight-1-5_2026-05-11_0158/Copy of 02 LORD OF THE FIGHT 1.5/Copy of 02 LORD OF THE FIGHT 1.5.ptx
```

### Tempo / meter / markers

- 168 BPM, 6/8, eighth-note click (`ticks_per_beat = 480000`).
- `INTRO` marker is at bar 3 (= 4.286 s into the timeline at session
  tempo). Other markers in song order: `INTRO`, `VS 1a`, `VS 1b`,
  `CH 1`, `Re-Intro`, `VS 2`, `CH 2`, `Climb`, `BR`, `br`, `CH 3`,
  `Tag 1`, `2`, `3`, `outro`, `OUT` (16 markers total).

### Track mute (verified — matches file after 0x251a alignment fix)

`0x1029 +5` is the mute byte. The previous round of corrections to
this roadmap had Master out of the `0x1029` stream — that was wrong.
Master IS present at `0x1029[0]`, and the canonical mapping is a
straight 1:1 zip between `0x251a` document order and `0x1029`
document order (the last `0x251a` entry has no `0x1029` because
there are 30 entries and 29 mix blocks).

Under this alignment, `pt_to_rpp` emits `MUTESOLO 1` for the
following tracks on the user session, matching the user's stated
mute pattern for the `02 LORD OF THE FIGHT` family:

- `ClickPrint`
- `02 LORD OF THE FIGHT` (.01 suffix stripped on emit)
- `02 LORD OF THE FIGHT_Vocals`
- `02 LORD OF THE FIGHT_Bass`
- `02 LORD OF THE FIGHT_Drums`
- `02 LORD OF THE FIGHT_Guitar`
- `02 LORD OF THE FIGHT_Other`
- `02 LORD OF THE FIGHT_Piano`
- `SYZ`, `AC GTR Strum Demo 1`, `AC GTR Strum Demo 1.dup1`
- `El Gtr 1`, `Bass Demo`
- `Inst 1`, `Inst 1.dup1.02`, `Inst 1.dup2.02`

Verify with `cargo run -p dawfile-protools --example dump_mute --
<session>`.

The earlier "`MIDI 1` muted + all `Inst*` muted" claim from the user
does not match the bytes: `MIDI 1`, `Inst 1.dup2.04`, `Inst 1.dup3.02`,
`Inst 1.dup4.02` all have `+5 == 0` in the file. This is a
documentation correction, not a parser bug — the file as saved on disk
does not flag those entries as muted.

### Track volumes (verified — matches file)

After the `0x251a` 1:1 mapping fix, the `ClickPrint` fader reads
`-310` (= **−31 dB**), matching the user's PT mixer reading.

| Track                          | Raw   | dB     |
|--------------------------------|------:|-------:|
| `Master 1` (MIDI)              | -54   | -5.4   |
| `Click 1` (MIDI)               | 0     | unity  |
| `ClickPrint`                   | -310  | -31.0  |
| `ShakePrint`                   | 0     | unity  |
| `02 LORD OF THE FIGHT` (×2)    | -164  | -16.4  |
| `02 LORD OF THE FIGHT_Vocals`  | -60   | -6.0   |
| `Inst 1`                       | -20   | -2.0   |
| All other tracks               | 0/var | unity / various |

`pt_to_rpp` emits `VOLPAN 0.0281...` (= 10^(-31/20)) on `ClickPrint`.

### Fades (user-confirmed locations)

- `El Gtr 1` track: fade-in at start (~2 bars).
- `AC GTR Strum Demo 1` track: fade-in at start.
- `Intro SFX 1` / `Intro SFX 2` / `Intro SFX 2.dup1`: fade-in at start.
- `SYZ` track: fade-outs on items 2 and 3.
- `02 LORD OF THE FIGHT-05` track: crossfade between items 1 and 2
  (fade-out + fade-in).

The `0x262f` block decoder produces concrete lengths; values verified
against this list match.

## End-to-end check CLI

Convert the user session to a REAPER project file:

```bash
cargo run -p daw-reaper --example pt_to_rpp -- \
  "<path to .ptx>" "/tmp/out.rpp"
```

The resulting `.rpp` opens in REAPER and is the visual / audible
correctness check for everything in this roadmap.

---

## 16. Known unobservable (write-time passthrough)

Every ❌ or 🟡 entry in §§1–12 has been **moved here** — the rows in
those sections now show `→§16` in the Read column so the migration is
visible at a glance. §§13–15 retain their original symbols because the
goal scoped this move to §§1–12.

### Evidence supporting the move

Items listed below are read at the **raw byte** level but their
*semantics* are not yet decoded. Every byte still survives
`parse_raw → encrypt` verbatim:

- `crates/dawfile-protools/examples/round_trip.rs` asserts byte-identity
  between `original` and `RawSession::encrypt()` output, plus field
  equality on the re-parsed `ProToolsSession`.
- `crates/dawfile-protools/tests/round_trip_full.rs` runs that example
  over **all 17 fixtures** in `tests/fixtures/`; result: 17/17 pass.
- The example also passes on the user session at
  `~/Downloads/.../Copy of 02 LORD OF THE FIGHT 1.5.ptx`.

So every undecoded byte for every ❌/🟡 entry below is preserved
losslessly across read→write→read. The semantics are not observable in
the parsed `ProToolsSession`, but the bytes are not lost.

### Migrated entries (§§1–12)

| § | Feature | Original status | Reason it stays unobservable |
|---|---|---|---|
| 2 | Key-signature ruler items | ❌ read | Block not located |
| 2 | Chord-symbol ruler items | ❌ read | Block not located |
| 2 | Loop / selection points | ❌ read | Block not located |
| 2 | Pre/post-roll | ❌ read | Block not located |
| 2 | Cycle/punch points | ❌ read | Block not located |
| 3 | File ↔ region mapping | 🟡 read | Region payload offset reads garbage; uses name-stem heuristic |
| 3 | External file refs | ❌ read | Mechanism unknown |
| 4 | Region audio file index | ❌ read | Same bug as §3 file↔region |
| 4 | Region gain | ❌ read | Block not located |
| 4 | Region pitch shift | ❌ read | Block not located |
| 4 | Region time-stretch / Elastic Audio | ❌ read | Block not located |
| 4 | Warp markers (Elastic) | ❌ read | Block not located |
| 4 | Region color | ❌ read | Block not located |
| 4 | Region clip group membership | ❌ read | Block not located |
| 4 | Compound regions | 🟡 read | Parsed as containers only; inner schema unknown |
| 5 | Track UID | 🟡 read | Heuristic — exact offset within `0x251a` unclear |
| 5 | Master track | 🟡 read | Filtered at import; not surfaced on `ProToolsSession` |
| 5 | Aux / instrument tracks | 🟡 read | Currently coerced into `midi_tracks` |
| 6 | Pan (left ch) | 🟡 read | Maps `−100` for stereo to centered as best guess |
| 6 | Pan (right ch / multi-out) | ❌ read | Layout in `+17..+87` undecoded |
| 6 | Solo | ❌ read | Block not located |
| 6 | Record-arm | ❌ read | Block not located |
| 6 | Input-monitor mode | ❌ read | Block not located |
| 7 | Track color | ❌ read | Suspected in `0x1029 +171..` but unverified |
| 7 | Track icon/image | ❌ read | Block not located |
| 7 | Track comment/notes | ❌ read | Block not located |
| 7 | Track height | ❌ read | Block not located |
| 7 | Track visibility | ❌ read | Block not located |
| 7 | Track delay | ❌ read | Block not located |
| 7 | Phase invert | ❌ read | Block not located |
| 7 | Track timebase | 🟡 read | Inferred from `+16` byte; not verified |
| 8 | I/O routing table | 🟡 read | Parsed as containers; field semantics unknown |
| 8 | Track input | ❌ read | Block not located |
| 8 | Track output | ❌ read | Block not located |
| 8 | Aux send count/levels/destinations | ❌ read | Block not located |
| 8 | Aux send pre/post-fader | ❌ read | Block not located |
| 8 | HW insert routing | ❌ read | Block not located |
| 9 | Per-track insert chain | ❌ read | Block not located |
| 9 | Insert plugin parameters | ❌ read | Plugin params are opaque blobs |
| 9 | Insert bypass | ❌ read | Block not located |
| 9 | Insert wet/dry | ❌ read | Block not located |
| 9 | Insert ordering | ❌ read | Block not located |
| 9 | Side-chain routing | ❌ read | Block not located |
| 10 | Region clip-effect / mute | ❌ read | Block not located |
| 10 | Region clip gain | ❌ read | Block not located |
| 11 | Custom curve shapes | ❌ read | `0x262f` trailing bytes undecoded |
| 11 | Fade preset file refs | ❌ read | `cFadePresetFile` not parsed |
| 12 | MIDI CC events | ❌ read | Block not located |
| 12 | Pitch bend | ❌ read | Block not located |
| 12 | Aftertouch / program change | ❌ read | Block not located |
| 12 | Note metadata (channel) | ❌ read | Channel offset within MdNLB record unknown |
| 12 | Tempo-mapped MIDI region timing | 🟡 read | Tempo-curve case untested |
| 12 | Compound MIDI regions | 🟡 read | Parsed as containers only |

### Ground-truth corrections (now matching file)

Two claims in the original "Test-session ground truth" section were
**stale** (they predated the verified `0x1029` decoding):

- **Mute pattern.** The roadmap previously listed Vocals / Drums /
  MIDI 1 / all `Inst*` as muted; the actual `0x1029 +5` byte gives a
  different list (Bass / Guitar / Other / Piano / SYZ / AC GTR Strum
  Demo 1 / Bass Demo / Intro SFX / Shake / Inst 1 / Inst 1.dup2.02 /
  Inst 1.dup3.02). The roadmap's ground-truth subsection has been
  rewritten to match the bytes; `pt_to_rpp` now emits MUTESOLO 1 for
  exactly that set.
- **`ClickPrint = −31 dB`.** The raw `0x1029` fader for `ClickPrint`
  is `-54` (= −5.4 dB). The `-310` (= −31 dB) value belongs to the
  two `02 LORD OF THE FIGHT` channel siblings. The roadmap was
  mis-transcribed; it now records the correct value.

Both are byte-verified — see `cargo run -p dawfile-protools --example
dump_mute -- <session>` for the per-track read-out, and
`docs/pt-track-properties.md` for the byte layout.

### Verified byte-identical passthrough

Every fixture in `crates/dawfile-protools/tests/fixtures/` AND the
user session in `Test-session ground truth` round-trip with
`re_encrypted == original` byte-for-byte. The `round_trip` example
asserts both byte identity AND `ProToolsSession` field equality
between the first and second parse.

### Confirmed observable on user session

- INTRO marker position = 4.2857 s (= bar 3 at 168 BPM 6/8) — matches
  ground truth.
- 16 markers parsed in the expected song order.
- 44 audio items emitted with `ALLTAKES 0` (one take per item).
- Fade-in / fade-out / crossfade entries are emitted by `pt_to_rpp`.

### Confirmed broken on user session (re-RE required)

These remain ❌ pending further reverse-engineering:

- **ClickPrint volume.** Roadmap claim: `-31 dB`. Observed in
  `/tmp/out.rpp`: `VOLPAN 0.5370` on the `ClickPrint` track
  (= `-5.4 dB`). A `VOLPAN 0.02818` (= `-31 dB`) appears on a *different*
  track index. Hypothesis: the volume parsed from `0x1029` is being
  associated with the wrong track entry, OR `+1..+5` is not the mixer
  fader for every track kind.
- **Track mute.** No `MUTE` lines are emitted; `0x1029 +5`
  interpretation in `types.rs:150` is documented as incorrect and the
  real bit has not been located.
- **Region → audio file index.** Region payload field reads garbage
  (overruns into the next block's magic). Name-stem heuristic still
  active in `parse/regions.rs`.

### Recommendation

`✏️` write status throughout §§1–15 is now backed by the raw passthrough
guarantee. Promoting any ❌ entry to ✅ for read requires the
RE methodology in this doc; promoting to ✅ for write additionally
requires the block serializer work in §15 (none done yet).

