# PTX field offsets discovered via Frida byte-read tracing

**Method**: hook `_$s10Foundation4DataV15_RepresentationOys5UInt8VSicig`
(Foundation `Data.subscript(_:Int) -> UInt8`) in the running converter
via Frida, run on probe fixtures, diff read offsets/values between
baseline and feature-isolated probes.

Each "read" line is `(file_offset, byte_value)`. By generating a probe
fixture with ONE field changed vs baseline and diffing the read
streams, the changed `(offset, value)` pair localizes the feature.

Hook script: `scripts/frida/trace_all_reads.js`.
Mapping helper: `crates/daw-reaper/examples/find_blocks_at.rs`.

## Already-known offsets (verified via Frida)

These match what `dawfile-protools` already has decoded:

| Feature | File offset | Block | Within-block | Roadmap row |
|---------|------------:|-------|-------------:|-------------|
| Track mute (stored bit) | 30546 | `0x1029` @ 0x7744 | +14 (payload +5) | §6 ✓ |
| Track solo | 30303 | `0x102d` @ 0x75b4 | +171 (payload +162) | §6 ✓ |
| Track solo defeat | 38206 | `0x200a` @ 0x9432 | +277 (payload +268) | §6 ✓ |
| Track color | 38044/45 | `0x2015` @ 0x943b | +97/+98 (payload +88) i16 LE | §7 ✓ |

(All previously decoded in the codebase. Confirms the Frida pipeline
is reading the right buffer.)

## NEW offsets — Phase A/B/C wins

### 1. Memory location color (markers) — `0x4826` block

**Was §16 "block not located".** PT12 stores markers as `0x4826`
entries inside the `0x2077` memory-location list (itself inside
`0x2030`). Each marker's color is split into 3 u16 fields:

| Within-block offset | Field | Value example |
|--------------------:|-------|---------------|
| +11 (payload +2) | R component (u16 LE) | 0x00D8 |
| +13 (payload +4) | G component (u16 LE) | 0x006E |
| +15 (payload +6) | B component (u16 LE) | 0x0041 |

Verified by probe `marker_colored` with REAPER color 0xD86E41 — bytes
at +11, +13, +15 read `D8, 6E, 41` (probe captured R + G; B inferred
since the marker_colored probe stopped at G in the trace, but the
block layout is consistent with R/G/B being u16 LE triplet).

### 2. I/O routing block — `0x2602` / `0x2603`

**Was §16 "field semantics unknown".** Reads observed at:

| File offset | Block | Within-block | Notes |
|------------:|-------|-------------:|-------|
| 21482 | `0x2603` @ 0x53d3 | +23 | I/O routing list entry 1 |
| 21500 | `0x2603` @ 0x53d3 | +41 | (18 bytes after previous = entry stride?) |
| 21691 | `0x2603` @ 0x53d3 | +232 | mid-block field |
| 21793 | `0x2603` @ 0x53d3 | +334 | |

Inside the 0x2603 wrapper, `0x2602` entries appear at +10, +28, +49.
The +18 spacing of the first two reads suggests an entry size of 18
bytes for routing entries.

**Action**: probe specific routing setups (`bus_explicit`,
`folder3`, `send` already done — see § Per-probe new reads) to
isolate which bytes encode source/destination/level.

### 3. Per-clip flag byte — `0x1052` payload +18

Reads at file offsets 29755, 29784 land in two `0x1052` entries at
within-block offset +27 (= payload +18, after the 4-byte length
prefix + 10-byte "ProbeTrack" name + 4 bytes).

**Hypothesis**: this is a per-clip / per-region flag byte
(roadmap §10 "Region clip-effect / mute"). The probe didn't toggle
it, but its position is now known.

### 4. Track kind / hierarchy — `0x251a` payload +25

Reads at 549, 642 inside two `0x251a` entries at +25 (= payload +16,
after `[u16][name_len_u32][name_bytes][separator]`). Value `1` in
baseline.

This matches the existing `Track.is_folder` decoder which reads
`0x251a +25`. ✓

## Per-probe new reads

### `marker` (one un-colored marker at t=1.0, name "M")

Compared to baseline, new reads at:

| File offset | Block | Within-block | Value | Likely meaning |
|------------:|-------|-------------:|------:|----------------|
| 45490 | `0x2077` @ 0xb19a | +24 | 0 | flag/counter |
| 45669 | inside `0x4826` @ 0xb263 | +6 | 38 | marker id? (u16 LE = `26 48`?) |
| 45670 | inside `0x4826` | +7 | 72 (=0x48) | marker id high byte? |
| 45673 | inside `0x4826` | +10 | 147 | timecode-related byte |
| 45675 | inside `0x4826` | +12 | 2 | ? |

### `marker_colored` (marker with color 0xD86E41)

Reads include the R/G bytes mapped above (see § 1). Other reads are
the same as plain marker probe but at slightly shifted file offsets
because the colored variant adds bytes.

### `folder` / `folder3` / `send`

These produce WHOLESALE shifts in file offsets because adding tracks
restructures the file layout. Individual feature bytes need a
different probe strategy (e.g. pre-allocate a fixed track count and
toggle features within).

### `mute_envelope` / `vol_envelope`

Same scan pattern as baseline — envelopes don't trigger extra
subscript reads. The converter probably reads breakpoints via direct
pointer arithmetic (not subscript), so they're invisible to this
hook. To see envelope reads we'd need to instrument the bulk-read
path (e.g. `memcpy` from `Data`).

### `inactive` / `fx_disabled` / `vol` / `pan`

Zero diff vs baseline — likely the probes don't actually mutate the
generated PTX bytes (REAPER builder issue or converter takes a
different code path for these fields).

## Trace on the LotF user session — new offsets

Running the same hook on
`Copy of 02 LORD OF THE FIGHT 1.5.ptx` (3.9 MB, 29 logical tracks)
produces 1,885 byte-reads. Aggregating by block CT and within-block
offset (see `crates/daw-reaper/examples/map_offsets.rs`):

### Newly-observed block CTs (not yet in our parser)

| CT | Read offsets | Count | Likely meaning |
|------|--------------|------:|----------------|
| `0x104f` | +9, +20, +24, +25, +26, +27 | 504 | Per-clip sub-block (lives INSIDE `0x1050` at its payload start). +25/+26 = `FE FF` = i16 LE `-2` (default-color marker), so 0x104f probably stores **clip color** / clip palette. |
| `0x2602` | +10, +31, +33, +35, +36, +47, +48, +50, +51, +52 | 536 | I/O routing entry. +10 is the "active" flag (toggles 0/1 across entries). |
| `0x2628` | +45..+48, +54..+59 | 312 | Audio region inner block. +54..+59 is a **6-byte UID identifying the source audio file** (regions referencing the same file share this UID). |
| `0x262f` | +17, +18, +21 | 112 | Fade definition. +17/+18 likely fade-length encoding bytes (per `pt-fade-encoding.md`). |
| `0x4826` | +7, +8, +11, +13, +15 | 80 | Marker color block — verified above. |
| `0x2077` | +24, ... | 16 | Marker entry list. |
| `0x204d` | +9 | 1 | Unknown — read once early in parse. |

### `0x2628 +54..+59` — **audio file UID** (region → file mapping!)

This is the long-standing §3 / §16 "Block not located: region audio
file index" win. The 6-byte UID identifies the source file. Multiple
regions pointing to the same WAV will share the UID. To build the
region→file mapping:

1. Walk each `0x2629` region; descend to its inner `0x2628`.
2. Read 6 bytes at payload `+54..+59` (block_start `+63..+68`).
3. Group regions by UID. Each unique UID = one source file.
4. Cross-reference UIDs to filenames via the file-list block (whose
   UID layout still needs probing).

This replaces the name-stem heuristic currently in
`parse/regions.rs`.

### `0x1050 +53` — per-clip flag (mute? gain?)

92 reads at +53 inside `0x1050` blocks. Values mostly 0 but TWO are
1 (offsets 390481 and 390537 in LotF). This is a per-clip boolean.

The existing parser knows `0x1050 +46 == 0x01` indicates a fade.
`+53` is 7 bytes later — a separate flag, likely **clip mute** or
**clip gain non-zero** marker (§10 "Region clip-effect / mute" or
"Region clip gain" in the roadmap).

### `0x2602` routing — additional fields

Beyond the +10 active flag:

- `+10`: enable flag (0 = inactive, 1 = active)
- `+33`: another byte flag (varies)
- `+36`: another flag — observed 1 on some entries
- `+47..+52`: cluster of bytes — likely destination ID encoding

### Confirmation of existing decoded offsets on LotF

- `0x1029 +14` mute: 20 reads (= 8 muted + checks)
- `0x102d +N` solo: 20 reads
- `0x200a +N` solo-defeat
- `0x2015 +97/+98` color (i16 LE)
- `0x4826 +11/+13/+15` marker color RGB (this session)

All match the current parser's offsets.

## Routing-examples fixture trace

Running the hook on `routing-examples.ptx` (24 tracks with many
routings) produces 724 reads. The routing block `0x2602` accounts
for 554 of them across 217 distinct entries. Hot offsets:

| Offset | Reads | Observed values | Likely meaning |
|--------|------:|-----------------|----------------|
| +10 | 217 | 0 / 1 | **active flag** (1 = routing entry is in use) |
| +33 | 9 | 0 | secondary flag |
| +35 | 28 | varies | data field |
| +36 | 7 | 0 / 1 | another active-like flag |
| +47..+52 | ~200 cluster | varies | 6-byte block — looks like a destination UID (same pattern as the region source-file UID) |
| +50 | 19 | varies | mid-UID byte |
| +54 | 7 | varies | trailing field |

The `+47..+52` cluster IS shaped like a UID (varying bytes per
entry, consecutive). Hypothesis: it's the destination UID that
points to a specific bus / output / send target. Resolving the UID
to a destination name needs probing of the bus/output list block.

Also confirmed on this trace:
- `0x260e +49 = 66` — a single read; likely a destination index
  for the track output assignment.
- `0x2015` has MULTIPLE i16-LE `FE FF` (default color) reads
  at `+51..+52`, `+54..+55`, `+97..+98` — three separate color
  positions within the same block (probably different states /
  default-vs-set colors).

## Multi-byte ranges captured via `Data.subscript(_:Range<Int>)`

Hooking the range-subscript variant (`_$s10Foundation4DataV15_RepresentationOyACSnySiGcig`)
at PLT stub `base + 0x2690c8` (script:
`scripts/frida/trace_range_reads.js`) reveals **all multi-byte
slice reads** that the single-byte hook misses.

Baseline trace produces 62 unique range reads. Mapping highlights:

| Range size | Within block | Likely meaning |
|------------|--------------|----------------|
| 4 bytes | `0x2637 +9..+12` | u32 — version/sample-rate |
| 8 bytes | `0x2602 +37..+44` | second UID inside routing entry |
| 8 bytes | `0x102d +43..+50` (within `0x2619`) | per-track UID? |
| 10 bytes | `0x2519 +79..+88` | first-track name slice ("ProbeTrack") |
| 10 bytes | `0x1052 +13..+23` | playlist-entry name |
| 18 bytes | `0x2602 +28..+45` (etc.) | routing entry UID + flags |
| 22 bytes | `0x4803 +13..+34` (under `0x204b`) | unknown block sub-field |
| 38 bytes | `0x0003 +9..+47` | session version/product chunk |
| 123 bytes | `0x0003 +9..+131` | larger session metadata |
| 256 bytes | `0x0003 +51..+306` | large session blob |

The range hook **doesn't capture byte values** (returns a Data
slice, not a copy). For value extraction, use the byte-subscript
hook OR read bytes from the buffer post-hoc with known offsets.

**Next-session wiring candidates** (offsets known, semantics need
ground-truth probes):
- `0x102d +41..+46` 6-byte UID (per-track identity?) — also at
  `+53..+58` (mirror).
- `0x2602 +15..+22` 8-byte field (additional routing UID).
- `0x260e +9..+10` u16 — track-output destination index
  (complement to the destination-string at `+45`/`+47`).
- `0x260e +20..+25` 6-byte UID for routing target.
- `0x2637 +9..+12` u32 — some session-level counter.

## Phase D — real-audio clip probes (2026-05-17)

WAV fixture: `/tmp/pt-re/input/clip_probe.wav` (96044 B silent mono 48k/16).
All probes prefix `clip_*` use that source.

### Verified clip offsets (wired in `TrackRegion`)

- `0x104f +9` u8 — **clip mute** (`clip_muted` probe sets to 1).
  Field: `TrackRegion.clip_muted`.
- `0x104f +25..+26` i16 LE — **clip color palette index**.
  `clip_with_wav` → `-2` (default); `clip_colored(0x6e41d8)` → `27`.
  Field: `TrackRegion.clip_color`.

### ✅ Solved — clip slip-offset (source start within source)

Three-way `clip_slip_{eighth,quarter,half}` probes localized
slip-offset to the **same TCE-clone `0x2628` block** that holds
playrate. Layout when only slip is set (playrate = 1.0):

```
... 01 20 20 33 08 <slip_u16_LE> <length_u16_LE> <pad×4> ...
                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
                two consecutive u16 LE values in samples @ project SR
```

| Probe | slip (s) | len (s) | bytes after `08` | decoded slip | decoded len |
|---|---|---|---|---|---|
| `clip_slip_eighth` | 0.125 | 0.875 | `70 17 10 a4` | 6000 | 42000 |
| `clip_slip_quarter` | 0.25 | 0.75 | `e0 2e a0 8c` | 12000 | 36000 |
| `clip_slip_half` | 0.5 | 0.5 | `c0 5d c0 5d` | 24000 | 24000 |

The byte at `0x173e` of the second `0x2628` payload (relative to
beginning of probe area; varies with source-path length) is the
**slip discriminator**: `0x20` when slip > 0, `0x00` when slip == 0.
The byte after (`0x173f` here) is the direction flag established
earlier: `0x30` (slowdown) / `0x20` (speedup / non-stretched).

**Open question**: this encoding uses u16 for slip+length, but the
playrate-only case uses a wider i64 for the length alone. The two
encodings select on the slip-discriminator byte. Need a combined
`clip_slip_and_playrate` probe to confirm which path the converter
takes when both are set.

### ✅ Solved — playrate encoding via TCE-clone region

Three-way differential probe (`clip_playrate_half/quarter/double` vs
`clip_with_wav`) localized the encoding cleanly. **Playrate is stored
implicitly** as the TCE-clone region's length-in-samples, *not* as a
ratio.

When playrate ≠ 1.0 the converter inserts a **second `0x2628`** block
(plus second `0x2629` name block) representing the time-compressed/
expanded clone of the source region. The clone block layout (relative
to the `0x5A` block start):

```
+0   0x5A 01 00 <size:u32> 28 26      9-byte block header
+9   <pathlen:u32> <utf-8 path bytes>  source path (e.g. "/tmp/.../clip_probe", no extension)
... (variable) ...
... 01 00 <dir_flag:u8> 33 08         field marker; dir_flag = 0x30 (slowdown) or 0x20 (speedup)
... <length_samples:i64 LE>           item length in samples @ project SR
... ff ff ff ff ff ff ff ff fe ff ...
```

Decoded values for the three probes (source = 96044 samples ≈ 1.0s @ 48k):

| Probe | RPP item len | Field `dir_flag` | `length_samples` i64 LE | Decoded |
|---|---|---|---|---|
| `clip_playrate_half` (0.5×) | 2.0s | `0x30` | `00 77 01 00 00 00 00 00` | 96000 |
| `clip_playrate_quarter` (0.25×) | 4.0s | `0x30` | `00 ee 02 00 00 00 00 00` | 192000 |
| `clip_playrate_double` (2.0×) | 0.5s | `0x20` | `c0 5d 00 00 00 00 00 00` | 24000 |

Implied playrate = `source_length_samples / length_samples`. Verified
absolute file offsets in the half probe: `0x1741..0x1748` (= `0x2628`
block-2 payload, after variable-length path string).

Auxiliary observations:

- `0x1052 +83 = 1` is the **TCE-enabled flag** at the inner clip block,
  gating the cross-reference to the second `0x2628`.
- `0x104f +9` and `0x104f +20` are read only when TCE-enabled (both
  values happen to be `0` for these probes — likely an "additional fade
  required" flag and an "alignment mode" flag respectively, not yet
  differentiated).
- All `0x2603 +N` byte shifts are downstream consequences of the
  ~316-byte block insertion, not new fields.

**Wiring status — already in the parser.** Verified via
`cargo run -p daw-reaper --example dump_parsed_regions <file>`. The
existing `parse_three_point` path in `crates/dawfile-protools/src/parse/regions.rs`
already extracts the slip-offset and stretched-length into
`AudioRegion.sample_offset` and `AudioRegion.length`. The clip's
`TrackRegion.region_index` already cross-references the TCE-clone
(second `0x2628`) when one is present. Example:

| Probe | `region_index` | `sample_offset` | `length` |
|---|---|---|---|
| `clip_with_wav` (no TCE) | 0 | 0 | 48000 |
| `clip_slip_quarter` | 1 | **12000** | **36000** |
| `clip_playrate_half` | 1 | 0 | **96000** |

A consumer can compute `playrate = source_region.length / clip_region.length`
where `source_region` is region 0 (the un-stretched source). The writer
side (PTX emission) still needs to emit the TCE-clone block when slip or
playrate is set — that path doesn't exist yet.

Probes that produced **zero** useful diff vs `clip_with_wav` (converter
discards or REAPER builder doesn't emit):

- `clip_selected`, `clip_at_offset`, `track_selected`, `track_locked`,
  `track_show_mixer`, `clip_named`, `clip_long_name` (clip name inherits
  from region `0x2629`, not stored per-clip).
- `clip_pitch_up_2` / `clip_pitch_up_7` / `clip_pitch_down_3` —
  REAPER `.pitch(semitones)` does not reach the PT export through the
  converter path (identical file size + only hash-region byte changes).
  Pitch shift, if supported by PT, would need a different REAPER source
  signal or a direct PTX-level write.
- `midi_one_note` / `midi_cc1_only` / `midi_cc1_value127` /
  `midi_cc7_volume` — all four produce **identical** block-CT counts and
  identical 13-byte `0x2000` (MdNLB) blocks. The converter emits the
  outer MIDI shell but **drops every MIDI event** (notes and CCs alike).
  Round-tripping MIDI through this converter is not viable; MIDI CC
  decoding will need a real PT-authored session, not converter output.

### Fade probes — partial decode

Block-count diff (`clip_with_wav` vs `clip_fadein` / `clip_fadeout`):

| CT | with_wav | fade present | meaning |
|---|---|---|---|
| `0x104f` | 1 | 2 | extra "fade-region" sub-clip |
| `0x1050` | 1 | 2 | container for extra fade-region |
| `0x262e` | 1 | 0 | "no-fade" track summary marker |
| `0x262f` | 0 | 1 | "has-fade" track summary marker (replaces `0x262e`) |
| `0xc95e` | 1 | 0 | tail config: no-fade variant |
| `0xc9af` | — | 1 (fadein only) | tail config: fade-in variant |
| `0xc9b5` | — | 1 (fadeout only) | tail config: fade-out variant |

Verified per-region fields inside `0x104f` (offsets relative to the
`0x5A` magic byte at the block start):

- **`magic +24` (u8)** — region kind flag.
  - `0x03` = ordinary playable region
  - `0x01` = fade-tail render region
- **`magic +16..+23` (i64 LE samples)** — region's start-offset
  within source. `0` for ordinary regions and for the fade-tail of a
  fade-IN; `48000` (= source end at 48kHz, i.e. the EOF marker for our
  1.0s clip) for the fade-tail of a fade-OUT. Locates *which end* the
  fade attaches to.

These coexist with the previously-decoded fields:
`magic +9` u8 = clip mute; `magic +25..+26` i16 LE = clip color.

### ✅ Confirmed — fade duration + direction + curve inside `0x262f`

This **independently re-verifies** the prior decode in
[`docs/pt-fade-encoding.md`](pt-fade-encoding.md) via a fresh
differential probe. That doc remains the canonical reference (it covers
crossfades and the full type-byte vocabulary `0x30`/`0x33`/`0x22`/`0x20`/
`0x11`). Summary of the cross-check below.

Three-way duration probe (`clip_fadein` 0.25s vs `clip_fadein_long` 0.5s
vs `clip_fadein_xlong` 0.75s) found the duration field cleanly. Each
fade emits a single `0x262f` block (replacing the no-fade `0x262e`) at
session-level whose payload encodes everything.

Block layout (offsets relative to `0x5A` magic):

```
magic +0..+8     9-byte block header (5A 02 00 <size> 00 00 00 2F 26)
magic +9..+13    5 zero bytes
magic +14  u8    direction flag: 0x30 = fade-IN, 0x33 = fade-OUT
magic +15..+16   2 zero bytes
magic +17..+19   u24 LE: fade duration in samples (@ project SR)
magic +20..+22   u24 LE: only set for fade-OUT (duplicate of +17..+19;
                         fadein leaves these as 0x00,0x03,0x00 — the
                         next byte slot is actually the curve)
magic +N   u8    curve type byte: 0x03 = linear fadein, 0x02 = linear
                                  fadeout. Other curves untested.
```

Decoded values for the differential set (project SR = 48000):

| Probe | direction | duration bytes | decoded | curve |
|---|---|---|---|---|
| `clip_fadein` (0.25s) | `0x30` | `e0 2e 00` | 12000 | `0x03` |
| `clip_fadein_long` (0.5s) | `0x30` | `c0 5d 00` | 24000 | `0x03` |
| `clip_fadein_xlong` (0.75s) | `0x30` | `a0 8c 00` | 36000 | `0x03` |
| `clip_fadeout` (0.25s) | `0x33` | `e0 2e 00` (×2) | 12000 | `0x02` |

The fade-tail `0x104f` sub-region (kind = `0x01`) cross-references the
`0x262f` block; the `0x104f` carries only the source-anchor (start
sample within source), and `0x262f` carries the actual fade timing.

Roadmap:

- Wire `TrackRegion.fade_in_samples: Option<u64>` and
  `TrackRegion.fade_out_samples: Option<u64>` from `0x262f` block,
  using direction byte to pick which to populate.
- Wire `TrackRegion.is_fade_region: bool` from `0x104f magic+24 == 0x01`.
- Probe equal-power / S-curve fades to enumerate the curve byte
  vocabulary.

## Next probe ideas

To find more unobservable bytes:

1. **`item_basic` with a real WAV source** — would let us probe
   clip-mute (`0x1050 +53`), clip-gain (`0x104f +?`), clip-color.
2. **Routing-rich probes** (multi-track with explicit `send_to(bus)`)
   to enumerate `0x2602` fields.
3. **Pan probes that ACTUALLY mutate the file** — current `pan` probe
   produces zero diff; investigate whether REAPER builder is missing
   that field or converter ignores it.

## Phase E — internal tracks via real PT fixtures (2026-05-18)

Pivoted from converter-output probes to direct inspection of
PT-authored fixtures under `crates/dawfile-protools/tests/fixtures/`
since the converter discards too much (MIDI events, pitch shift,
edit-group memberships, etc.) to surface those features.

### `0x261e` — internal-track / aux-bus / master-bus / click-track entries

Per-session count = number of non-audio mixer tracks (buses, aux
returns, master, click). Each `0x261e` block carries the track name as
a length-prefixed string at payload offset `+0x1d` (= magic + `0x24`).
Names extracted from fixtures:

| Fixture | count | sample names |
|---|---|---|
| `HeyLady.ptx` | 1 | `Click` (the metronome click track) |
| `studio-session-2.ptx` | 6 | `Click 1`, `DRUMS LR`, `SNAPS`, `GTRS`, `verb`, `Master` |
| `worship-session.ptx` | 7 | `DRUMS`, `EG GTR`, `G Delay`, `Verb`, `HORNS`, `Click 1`, `MixBus` |
| `orchestral-session.ptx` | 7 | `Click 1`, `M2`, `M3`, `M4`, `WW>>`, `Brass>>`, `Strings>>` |
| `wonder-session.ptx` | 16 | `DRUMS`, `Drum Verb`, `PERC`, `BASS`, `AC GTR`, …, `MixBus` |

These are the **session's internal/mixer-only tracks** — not audio
playback tracks. Currently `TrackKind` only models `Audio` and `Midi`;
adding `Aux` / `Bus` / `Master` / `Click` variants would let the parser
expose these.

The single-block `0x2614` (size 13 bytes), and the nested wrappers
`0x2613` ⊂ `0x2615` ⊂ `0x2616` co-occur with each `0x261e` — they
appear to form the per-entry config records (plugin slot, output
routing, signal-flow flags). Concrete byte semantics not yet decoded.

The single-instance container CTs (`0x2611`, `0x2554`, `0x4841`,
`0x2621`) live alongside this list — `0x2621` is huge (~260 KB in
worship-session), almost certainly the wrapper for all the per-aux
config records.

### `0x2077` — Memory Locations (markers + selections + presets)

Found in `worship-session.ptx` × 2. PT's unified "Memory Locations"
feature, storing markers, edit-selections, mixer-snapshots, and
window-configs as a single typed list.

Block payload layout (offsets relative to magic):

```
magic +9    u16 LE   id / version  (observed: 0x0001)
magic +11   u32 LE   flag bitmap   (observed: 0x00000903 — see below)
magic +15   u32 LE   name_length
magic +19   N bytes  name (ASCII, no NUL terminator)
magic +19+N u64 LE   start_position (samples)
magic +27+N u64 LE   end_position (samples; == start for point memlocs)
magic +35+N 8 bytes  pre_roll? (observed `f0 bf ff ff ff ff ff ff`
                     = -1.0 as a double, marking "unset")
...                  further fields: window-config flags, zoom level,
                     selection-region-uids, etc. (not yet decoded)
```

Verified records:

| Probe | name | start | end | kind |
|---|---|---|---|---|
| `worship` 0x2077[0] | `"Location 1"` | 2737115 | 2737115 | point marker |
| `worship` 0x2077[1] | `"move tuba slightly early"` | 8875 | 8875 | point marker |

The `0x00000903` flag bitmap likely combines: `has_name` (bit 0)
+ `has_position` (bit 1) + `is_marker_kind` (bit 8) + `is_point_kind`
(bit 11). Need probes with PT's other memloc kinds (selection,
mixer-snapshot, window-config) to fully decode the bitmap.

This block lives separately from the simpler `0x4825`/`0x4826` markers
(which the roadmap already lists as ✅). `0x2077` is likely the
authoritative store; `0x4825` may be a denormalized "markers only"
view. The roadmap's `selection-state memlocs` and `zoom-state memlocs`
(both ❌) almost certainly map to other entries in this same list with
different flag-bitmap values.

### `0x4501` / `0x4702` — edit groups + stem mapping

Located via cross-fixture string search for "Group" / "Mix" /
"GROUP" / "Foreign". `orchestral-session.ptx` is a film-post session
with ~40 named groups split across two flat list blocks.

**`0x4501` — edit groups list** (1 per session when groups exist):

Layout from `Bed` example: each entry is
`<u32 LE namelen><utf-8 name><0xFE 0xFF (i16 = -2)>`.

The `FE FF` trailer is the same "no color" sentinel used elsewhere
(clip default color, track default color). Each entry is `4 + namelen
+ 2` bytes; entries are concatenated. The block has a sizable
per-track membership table preceding the name list (the first ~9 KB
of the block before the names start), structure not yet decoded.

Sample entries from orchestral-session (~40 total): `Bed`, `Objects`,
`DX Obj`, `GRP Obj`, `MX Obj`, `FX Obj`, `DZN Obj`, `BG Obj`,
`DIA Group`, `FX Group`, `MX Group`, `GRP Group`, `7.1.2 MIX`,
`DX 712 BED`, `MX 712 BED`, `FX 712 BED`, `Backgrounds`, `Vocals`,
`Music Stem No Vocals`, `Dials`, `DX STEM`, `MX STEM`, `FX STEM`,
`Design`, `DESIGN`, …

Cross-fixture counts of `0x4501`:

| Fixture | 0x4501 | 0x4702 |
|---|---|---|
| `HeyLady.ptx` (no groups) | 0 | 0 |
| `studio-session-2.ptx` | 1 | 1 |
| `orchestral-session.ptx` | 1 | 1 |
| `wonder-session.ptx` | 1 | 0 |
| `worship-session.ptx` | 1 | 1 |

**`0x4702` — stem-mapping / track-classification list** (when present):

Same flat-name-list layout but **without the `FE FF` trailer**. Each
entry is `<u32 LE namelen><utf-8 name>`. orchestral-session starts the
list with PT 12+'s built-in stem types: `Dialog`, `Music`, `Effects`,
`Narration`, then 2-char codes (`DX`, `MX`, `FX`), then user-defined
classifications. This block backs PT 12's "Stem Mapping" feature
(track → stem-type) used for film export.

**Status — neither parsed yet.** Edit-groups parity needs:

1. New `ContentType::EditGroupList = 0x4501` + `StemMappingList = 0x4702`
   enums in `crates/dawfile-protools/src/content_type.rs`.
2. New `EditGroup { name: String, color: Option<i16>, members:
   Vec<TrackId> }` type and `parse_edit_groups()` walking the flat
   list. Member-track mapping lives in the block prefix and still
   needs to be decoded — the leading ~9 KB has clear per-track
   records (`01 01 04 01 ...` 28-byte patterns) but the field
   semantics are speculative without a known-shape probe.
3. `ProToolsSession.edit_groups: Vec<EditGroup>` exposure. **Done**
   in this branch's parser (preliminary heuristic; over-reads on the
   membership-table prefix).

**Membership decode status (blocked):** worship-session has 4 named
entries (just the built-in stem types) but only 2 occurrences of the
`01 01 04 01` marker in its 0x4501 prefix, spaced 192 bytes apart. So
the marker is not a 1-per-group record and the per-track membership
table layout isn't a simple `N × M` matrix. Without a PT-authored
fixture where the member-track list of each group is externally known,
the prefix can't be reverse-engineered confidently.

### `0x260d` — 4 envelope slots per audio track

Sweep across all PT fixtures shows every audio-track `0x260d` wrapper
contains **exactly four `0x260a` envelope children** (one track in
`green-dolphin-street.ptx` has three — likely Master/Click):

| Slot | Likely role | Status in current parser | Status in test fixtures |
|---|---|---|---|
| `0x260a[0]` | Volume | ✅ wired | populated in 5 fixtures |
| `0x260a[1]` | **Pan** (suspected) | ❌ | empty (41 B) everywhere |
| `0x260a[2]` | **Mute** (suspected) | ❌ | empty everywhere |
| `0x260a[3]` | **Send-level / Pre-fader** (suspected) | ❌ | empty everywhere |

The volume envelope format (22 B header + 6 B implicit @+22 +
`N × (u32 time_samples + i16 value_cB)` breakpoints @+28) is fully
decoded. The other slots are believed to share the same byte layout
with different value units (pan position, mute bool, send level),
but **none of the test fixtures contain a non-empty pan/mute/send
envelope** — they are all 41-byte "empty + implicit only" stubs.

Reading non-empty curves for slots [1]/[2]/[3] cannot be verified
without a PT-authored fixture that exercises pan/mute/send
automation. Wiring `pan_automation`/`mute_automation`/`send_automation`
that simply reuses the volume decoder on `0x260a[1..3]` would be
sound code-wise but the value-unit interpretation would be
**unverified**.

### `0x2064` — plugin factory-preset references

Three blocks in `worship-session`, payload 350 bytes each, each
containing two length-prefixed strings: a volume name (`"Macintosh HD"`)
and a `.tfx` file name (PT's plugin factory-preset format).
Examples:

- `TrLntunrtmRTFact.tfx` — Tr-Lntu-nrtm-RT-Fact = obfuscated AAX plugin code
- `UADxU3C2jccFact.tfx` — UAD-style plugin preset

Probably session-level "plugin presets in use" cache; not the per-track
plugin instance assignments (those live elsewhere). Useful for parity
on plugin-state round-trip.

### Next on this branch

- Probe REAPER aux-return / master-track / instrument-track creation
  via the builder (if exposed) to differential-decode the per-record
  bytes in `0x2614`/`0x2615`/`0x2616`.
- Decode the routing UID inside `0x261e +0x32..+0x37`
  (`85 42 e7 e5 df a2` in worship-session DRUMS) and link it against
  the `0x2602` routing entries already parsed.
- Identify the block CT for **edit groups / mix groups**: ✅ found.
  See "`0x4501` / `0x4702`" below.

## Methodology — extending this

To find more byte-level fields:

1. Build a probe fixture in `crates/daw-reaper/examples/rpp_to_ptx_probe.rs`
   that isolates ONE feature.
2. Convert via `scripts/pt-convert.sh --hook scripts/frida/trace_all_reads.js`.
3. Diff the read log against baseline.
4. Run differing offsets through `crates/daw-reaper/examples/find_blocks_at`
   to identify (CT, within-block offset).
5. Codify the offset in `crates/dawfile-protools/src/parse/` and a
   writer in `crates/dawfile-protools/src/write/`.

## Limitations

- **Only Foundation `Data` subscript reads visible.** Direct pointer
  derefs in scan loops (`*pcVar1 == 0x5A`) bypass this hook. Most
  4-byte numeric reads (vol, pan, time positions) likely fall here.
- **Probe shifts.** Multi-track / folder / send probes shift file
  offsets globally, making feature isolation harder.
- **Module ASLR.** Each run gets a different `module_base`. Always
  compute offsets relative to `base` for hook addresses.

## Files

- `scripts/frida/trace_all_reads.js` — the hook script
- `scripts/frida/trace_block_scan.js` — earlier hook on FUN_100175f6c
- `scripts/frida/trace_data_reads.js` — emit + read combo
- `/tmp/reads_<probe>.log` — captured logs per probe
- `crates/daw-reaper/examples/find_blocks_at.rs` — offset → block resolver
