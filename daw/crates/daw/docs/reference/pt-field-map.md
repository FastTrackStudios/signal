# PT block field map — discovered via RPP→PTX probe-and-diff

Method: generate a minimal RPP with ONE feature set (e.g. mute=true on a
single track), convert RPP→PTX via the converter's `--convert` CLI,
plaintext-diff against a `baseline` PTX with the same tracks but no
feature set. Differing bytes = the encoding location.

Probes live in `crates/daw-reaper/examples/rpp_to_ptx_probe.rs`; the
diff tool in `crates/daw-reaper/examples/ptx_plaintext_diff.rs`.

## Verified field locations (2026-05-17)

### The full PT mute model

PT has SEVEN distinct mute-related concepts that interact to produce
the "effective mute" that the converter outputs as REAPER's
`MUTESOLO` field 1:

1. **Stored mute bit** (`0x1029 +5`): set by PT when user clicks
   mute, OR when track is marked inactive/bounced.
2. **Send routing enabled** (`0x260a[0] +8`): cleared when user
   clicks mute; remains set when the track is inactive/bounced.
3. **Folder mute inheritance**: child tracks of a muted folder
   parent are effective-muted via tree walk (the converter does this
   resolution; PT doesn't store it explicitly).
4. **Mute automation envelope** (`<MUTEENV>` in RPP → `0x0002`
   master event stream in PTX): time-varying mute, evaluated at the
   playhead. Format TBD.
5. **Send mute** (per-send `+26` byte in `0x260a`): mutes one send
   slot only, not the whole track.
6. **Clip mute** (TBD location, probably in `0x2629`): mutes a single
   audio clip, not the whole track.
7. **Active/Inactive track flag** (TBD): PT's "Make Inactive" menu —
   inactive tracks are greyed out. REAPER has no exact equivalent;
   probing not yet attempted.

Converter's effective-mute decision (verified by reverse-direction
probes):
```
effective_mute =
    (stored_mute AND NOT send_routed)         # user-clicked mute
    OR folder_parent.effective_mute            # inherited
    OR mute_automation.value_at_playhead       # envelope override (TBD)
```

`parse::mute_resolver` implements the first two terms and achieves
strict equality with the converter's MUTESOLO output on LotF.

### Track mute (bool, stored state)

Stored redundantly across many per-track wrappers — the converter writes
the same bit to all of these:

| Block | Offset | Encoding |
|---|---|---|
| `0x1029` | `+5`   | u8 (0/1) ← what our parser reads |
| `0x260a[1]` | `+26` | u8 (the master-send mute) |
| `0x260d` | `+14`, `+447` | u8 × 2 |
| `0x261b` | `+414`, `+847` | u8 × 2 |
| `0x261c` | `+423`, `+856` | u8 × 2 |
| `0x2624` | `+436`, `+869` | u8 × 2 |

The `+5` byte we previously decoded WAS the mute storage location for
output. The over-mute on LotF is a parsing-direction issue (in real PT
sessions, the +5 bit may also reflect a separate "inactive" /
"bouncedSource" state — the converter cross-references it with mute
automation envelopes when reading).

### Track color (i16 LE, signed)

| Block | Offset | Encoding |
|---|---|---|
| `0x200a` | `+97..+98` | i16 LE |
| `0x200b` | `+106..+107` | i16 LE ← **our parser was reading wrong offset (+163)** |
| `0x2015` | `+88..+89` | i16 LE |

Default value (no color) = `0xfffe` = -2 as i16. Explicit colors are
small positive integers (palette indices). The full byte→RGB lookup
table is in `pt-color-palette-ground-truth.md`.

### Track solo (bool)

| Block | Offset | Encoding |
|---|---|---|
| `0x102d` | `+162` | u8 (0/1) |
| `0x261b` | `+171` | u8 |
| `0x261c` | `+180` | u8 |
| `0x2624` | `+193` | u8 |

### Track solo-defeat (bool)

| Block | Offset | Encoding |
|---|---|---|
| `0x200b` | `+268` | u8 (0/1) ← what parser reads |
| `0x200a` | `+259` | u8 mirror |

Set when the track ignores other tracks' solo state (REAPER's
MUTESOLO field 3). Verified via `t.solo_defeated()` RPP→PTX probe.

### Send routing enabled (bool)

| Block | Offset | Encoding |
|---|---|---|
| `0x260a[0]` | `+8` | u8 (0/1) |

The "send is enabled" flag on the master-send block. PT clears
this when the user clicks mute (so audio doesn't flow out). For
inactive/bounced-source tracks, this stays set (audio still routes
in principle, but the track is silent for other reasons). The
converter uses this to disambiguate user-mute from
inactive-with-stored-mute-bit. See `parse::mute_resolver`.

Cleared on the 12 LotF "over-mute" tracks (SYZ, AC GTR, El Gtr,
Bass Demo, Inst*) but set on the 8 truly-muted ones (ClickPrint
+ LORD family).

### Mute automation envelope (DECODED 2026-05-17)

Stored in **`0x260a[1]`** — the per-track master-send mute block.
Each track's mute automation lives in its own send block. The 6 bytes
per breakpoint cascade size-mirror up the wrapper chain (0x260d,
0x261b, 0x261c, 0x2624).

**Header** (constant 28-byte prefix in 0x260a[1] payload):
```
+0..+3   01 46 01 00          — block tag (constant)
+4       u8                   — payload size byte
+5..+9   00 00 00 00 00       — reserved
+10      u8                   — total point count (1 = no automation)
+11..+13 00 00 00             — reserved
+14..+15 02 00                — envelope type (mute)
+16..+17 u16 LE               — user breakpoint count (= total - 1)
+18..+27 00 ... 00            — reserved
```

**Per breakpoint** (6 bytes each, starting at payload `+28`):
```
+0..+3   u32 LE               — time in samples (at session SR)
+4       u8                   — value: 0 = muted, 1 = un-muted
+5       u8                   — shape/flags (0 = square, default)
```

Note: REAPER's default-at-`t=0` (often val=0 from REAPER side) is
DROPPED by the converter as redundant with PT's implicit
default-unmuted state.

Verified by 4-probe series (1pt, 2pt, 3pt, 4pt MUTEENV):
- 4pt with REAPER points at (0.0,0.0), (1.0,1.0), (2.0,0.0), (3.0,1.0)
- → PT 3 user breakpoints (the t=0 point dropped):
  - bp1 time=0xBB80=48000 (1.0s) val=1 (un-muted)
  - bp2 time=0x17700=96000 (2.0s) val=0 (muted)
  - bp3 time=0x23280=144000 (3.0s) val=1 (un-muted)

Caveat: REAPER builder's 1-breakpoint envelope is ignored by the
converter (no time-varying state → optimized out). Need 2+ points
for any encoding to land.

### Active / Inactive flag (TBD)

Not yet probed. Converter binary contains strings for `Active`,
`Hidden`, `Inactive`, `InactiveH` (Swift small-string), plus a
`states-inactive` JSON option name. The `--options` flag accepts
`{"states-inactive":true}` syntactically but doesn't change output
for an `--convert` invocation (the options likely only affect
`--gen-test` test-generation mode, not normal conversion).

Decoding inactive requires either:
- A PT-authored session with known-inactive tracks (we don't have
  one — LotF's 12 "over-mute" tracks have +5=1 and +8=1, which
  matches "stored mute set but send still routed" — possibly
  inactive but not provable without ground truth).
- Frida RE of `PTXTrackStateSpec.active` reading path in the
  converter binary.

### Track volume (i16 LE, centibel)

| Block | Offset | Encoding |
|---|---|---|
| `0x260a[0]` | `+26..+27` | i16 LE (master-send level) |
| `0x260d` | `+407..+408` | i16 LE mirror |
| `0x261b` | `+807..+808` | i16 LE mirror |
| `0x261c` | `+816..+817` | i16 LE mirror |
| `0x2624` | `+829..+830` | i16 LE mirror |

Default = 0 (= 0 dB). Reaper pan 0.5 → centibel -60 → ~-6 dB.

### Track pan (i16 LE)

| Block | Offset | Encoding |
|---|---|---|
| `0x260a[2]` | `+26..+27` | i16 LE (left-channel pan) |
| `0x260a[16]` | `+26` | u8 (right-channel pan? only 1 byte differs) |
| `0x260c[0]` | `+36..+37` | i16 LE |
| `0x260c[1]` | `+36` | u8 |
| `0x260d` | `+497..+498`, `+1066` | mirror |
| `0x261b` | `+897..+898`, `+1466` | mirror |

Default = `0xff9c` = -100 (= full left, the PT convention even for
stereo tracks where REAPER would call this "balance"). Explicit pan
0.5 → +50.

### Track folder grouping

New block type **`0x200d`** appears ONLY when a track has child tracks
(folder structure). Encoding TBD — needs a "folder with 3 children"
probe to map the children-list format.

## Methodology — extending to next features

For each new feature probe:

1. Add a case to `rpp_to_ptx_probe::build_rpp()` that exercises just
   that one feature on top of `baseline`.
2. Run:
   ```
   cargo run -p daw-reaper --example rpp_to_ptx_probe -- <probe>
   cargo run -p daw-reaper --example ptx_plaintext_diff -- \
     /tmp/probe_baseline.ptx /tmp/probe_<feat>.ptx
   ```
3. Filter to short-run diffs (the long-run diffs are
   UID/timestamp/keystream noise that varies per file generation).
4. The remaining 1-2-byte diffs are the encoding location(s).
5. Add the field to this document.

## Additional probe findings (2026-05-17)

### `marker` (vs baseline)

Marker storage is **entirely inside the `0x0002[0]` container** (6550 bytes,
136 bytes differ between probes). Pattern: pairs of adjacent bytes
change at sparse offsets (+28..+29, +454..+455, +2554..+2555, ...).
This indicates a multi-entry list with i32/i64 position fields where
adjacent bytes represent the high-byte difference between baseline
positions and explicit marker positions.

Marker storage TBD — needs differential probes:
- `marker_two` — 2 markers (to see the per-marker delta)
- `marker_named_long` — marker with a long name (find name field)
- `marker_colored` — explicit color (find color field within marker entry)

### `send` (`Source` + `Dest` with `Dest.receive(0)`)

Send creates a SUBSTANTIAL structural change:
- New `0x200d × 1` (the same folder/child-relationship block)
- `0x1014` count 2→1 (Dest hidden from audio track list — became a bus)
- `0x102d[1]` +13..+16 = name change "Beta"→"Dest", and +28..+35 = 8-byte
  UID/route identifier change
- Many block-size shifts (0x1015, 0x1054, 0x2107, etc.)

This shows the converter MODELS REAPER receives as PT buses. Routing
the second track as a destination promotes it to a bus, removing it
from the audio-track list and creating a `0x200d` linkage.

### `folder3` (Parent + 3 children, vs `folder` Parent + 1 child)

Most counts triple (3 vs 1 children):
- `0x251a` 4→8 (= 2× n_tracks)
- `0x1029`, `0x102d`, `0x200a`, `0x200b`, `0x2015` all scale per-track
- `0x201f[0]` +76..+79 = `05 00 00 00` → `ff ff ff ff` (= 5 → -1)

The `0x201f` u32 field at +76 looks like a **count/sentinel**: when the
folder has 1 child, value is 5; when 3 children, value is -1 (which
typically means "all"). May be a "folder collapse depth" or similar.

### `item_basic`

A track with one `item(0.0, 1.0, |i| i.name("Clip"))` produces an
**IDENTICAL PTX** to baseline (zero diffs). The converter requires the
item to have a real audio source (`source_wave` pointing to an
existing file) to materialize it as a PT clip. Item probes need an
actual file fixture, not just metadata.

### Multi-track template structure (2026-05-17)

Confirmed via 1-track / 2-track / 3-track baselines (all `t.name(...)` only,
no other state). The per-track block counts scale **perfectly linearly**:

| CT       | Per-track | Notes                                    |
|----------|-----------|------------------------------------------|
| `0x1029` | +11       | mix-settings (master + 10 send slots)    |
| `0x102d` | +1        | solo/mute root                           |
| `0x200a` | +1        | aux/color block A                        |
| `0x200b` | +1        | aux/color block B                        |
| `0x2015` | +1        | color block C                            |
| `0x2037` | +16       |                                          |
| `0x2038` | +12       |                                          |
| `0x203b` | +4        |                                          |
| `0x260a` | +71       | sends (1 master vol + 70 send slots)     |
| `0x260c` | +22       | pan mirrors                              |
| `0x260d` | +11       | track wrapper (1 big 1654B + 10× 520B)   |
| `0x2625` | +11       |                                          |
| `0x2626` | +11       |                                          |
| `0x261b` | +1        | **outer per-track wrapper (~7.8 KB)**    |
| `0x261c` | +1        | per-track wrapper sibling                |
| `0x485a` | +11       |                                          |

Outer index blocks gain +1 entry per track (not +1 block): `0x2434`,
`0x4420`, `0x4301`, `0x2619`.

**Multi-track template strategy**: take the per-track block group
(`0x261b` wrapper + adjacent `0x200a`/`0x200b`/`0x2015` color/aux
blocks) from a known-good 1-track baseline, duplicate N times with
patched track name + UID, splice into the session, and extend the
outer index lists by N-1 entries each.

Per-track wrapper byte ranges (verified on `/tmp/baseline_2trk.ptx`,
with `Alpha`/`Beta` names):
- Track 1 `0x261b`: `0x0076ef..0x00955c` (size 7782, payload 7789 B)
- Track 2 `0x261b`: `0x00979b..0x00b607` (size 7781, payload 7788 B)

The 1-byte size difference is the track name length differential
("Alpha"=5, "Beta"=4) — there is NO separate track-index field
embedded in the wrapper size.

#### Pure per-track variation map

Probed with `two_tracks_eq` (`AAA`/`BBB` — equal-length names) and
cross-file compared against `one_track_aaa`. Only 181 bytes differ
between trk1 and trk2 within `two_tracks_eq`, and only **148 bytes**
differ between trk-1-of-1 and trk-1-of-2 — meaning the per-track
wrapper is **structurally self-contained**: there are no "I'm in a
multi-track context" header fields anywhere.

All offsets below are from the `0x261b` BLOCK start (raw file offset),
not from payload start. `bs` = block start.

| Offset | Size | Meaning |
|--------|------|---------|
| `bs+31`   | name_len | Track name bytes (length-prefix lives in a `0x2619` ahead of `bs`) |
| `bs+45`   | 8 | Per-track UID, occurrence 1 (inside `0x260d` big container, payload +4) |
| `bs+61`   | 8 | Per-track UID, occurrence 2 |
| `bs+163`  | 8 | Per-track UID, occurrence 3 |
| `bs+7350` | 1 | Send-slot dest bus index, slot 0 |
| `bs+7354` | 1 | Send-slot dest bus index, slot 1 |
| ...       | ... | (10 slots, stride 4: `bs+7350+4*i`) |
| `bs+7386` | 1 | Send-slot dest bus index, slot 9 |
| `bs+7435` | 148 | Randomized nonce region (per-track, likely cosmetic — only difference between 1-of-1 and 1-of-2 versions of same-named track) |

Track N (1-indexed) gets bus indices `(N-1)*10+1 .. (N-1)*10+10` in
its send-slot list. The first track in a session uses 0x01..0x0a.

Session-trailer block uses a **randomized CT** (`0xc7c9`, `0xeac2`,
`0x0deb` observed across runs) — a per-session unique-id block.

#### Outer index lists (must be extended for parser-visible multi-track)

Parser-level audio track count comes from `0x1014` entries inside
the single top-level `0x1015`. The baseline declares the track as
STEREO (`nch=2`), so a 1-track baseline already parses as 2 audio
track channels. Cloning `0x261c` alone does NOT change
`0x1014`/`0x1015`, so parsed track count is fixed at 2 regardless
of how many `0x261c` clones we splice in.

To produce N parsed PT tracks, each of these outer lists must gain
entries (verified via converter 1-trk vs 2-trk diff):

| Parent list | Per-track addition |
|-------------|--------------------|
| `0x1015`    | +1 × `0x1014` (audio track list entry) |
| `0x1054`    | +2 × `0x1052` (one per channel) |
| `0x2519`    | +2 × `0x251a` (MIDI/event list) |
| `0x2107`    | +1 × `0x210b` |

Each entry has its own internal byte layout (name, channel indices,
refs) requiring per-entry RE before bulk extension. Tracked in GH #26.

Session-trailer block uses a **randomized CT** (`0xc7c9`, `0xeac2`,
`0x0deb` observed across runs) — a per-session unique-id block.

## Probes still needed (next pass)

- `marker_two`, `marker_colored` — to isolate name vs color encoding
- `clip_simple_with_source` — one item with real WAV source
- `clip_muted`, `clip_gain`, `clip_color`
- `vol_env`, `mute_env` — track automation envelopes
- `fade_in`, `fade_out`, `crossfade`
- `bus_explicit` — explicit bus track with multiple senders

## Process for next features

1. Add probe case to `rpp_to_ptx_probe::build_rpp()`
2. Run probe + diff against appropriate baseline
3. Filter to short-run diffs (the long-run ones are file-UID noise)
4. Document field locations in this file
5. Update parser to read those bytes
