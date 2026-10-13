# Pro Tools 12 — Per-track Volume / Pan / Mute encoding

**Status**: empirical reverse engineering. The Ghidra project on disk only
contains `FFmt_x86_64`, which is the file-format library for AAF / WAV / AIFF
audio media (not the `.ptx` session reader). The actual `.ptx` reader lives in
the main Pro Tools application code path and is not present in the available
binaries. Findings here are derived by:

1. Cross-referencing block layouts across multiple `.ptx` fixtures (test
   sessions in `crates/dawfile-protools/tests/fixtures/` plus the user-supplied
   "Lord of the Fight 1.5" session at
   `~/Downloads/tombrooksmusic_copy-of-02-lord-of-the-fight-1-5_2026-05-11_0158/...`).
2. Pattern-matching observed value ranges against documented Pro Tools UI
   semantics (fader range −∞ … +12.0 dB in 0.1 dB steps; pan −100 … +100;
   mute = bool).
3. The user's domain knowledge that "many tracks are MUTED" in the test
   session — confirmed by the prevalence of the suspected mute byte being set.

## Block hierarchy

For each mixable track Pro Tools 12 emits one wrapper block:

```
0x260d (TrackMix wrapper)              # one per mixable track
└── 0x1029 (TrackMixSettings, 281 B)   # volume / pan / mute live here
```

In the "Lord of the Fight" session there are 30 tracks total (1 Master + 29
audio/aux/MIDI), 30 entries appear in `0x251a` (track-name records under
`0x2519`), and **29** `0x1029` blocks appear (one per mixable channel: the
Master output is excluded because it has no fader-relative-to-input setting).

The order of `0x1029` blocks matches the order of mixable tracks in the
session's track list (skipping the Master).

## `0x1029` payload layout (281 bytes)

The 281-byte payload is **two mirrored 87-byte logical records** followed by a
trailing 107-byte "extended" region. The duplication is consistent with
Pro Tools' "current value" vs "stored snapshot" pattern (saved-state for the
mix-vs-edit window state, or the mixer's pre-automation snapshot).

All multi-byte fields are **little-endian** (the test file has
`is_bigendian = false`, which is the modern PT12 default).

| Offset | Type     | Field                          | Notes |
|-------:|----------|--------------------------------|-------|
| `+0`   | u8       | record marker                  | Always `0x01`. Discriminator/version. |
| `+1`   | i32 LE   | **volume_main** (record A)     | Fader value in **0.1 dB units**. `0` = unity. Range observed: −1554 … +120. PT UI maximum is +12.0 dB → +120; UI minimum is "−INF", stored as a large-negative sentinel (≤ −1440 / −144 dB). |
| `+5`   | u8       | **mute_main** (record A)       | `0` = audible, `1` = muted. Confirmed against user's testimony for "Lord of the Fight". |
| `+6`   | u8[7]    | reserved/padding               | Always 0 in every observed file. |
| `+13`  | i32 LE   | **pan_main** (record A)        | Pan position. Range observed: −100 … +100 with `0` = center. `−100` is a very common default; for stereo tracks this is the **left-channel pan**. The right-channel pan and any surround positions live further into the block (see "Multi-position pans" below). |
| `+17`  | … 70 B   | record-A continuation          | Mostly zero in mono tracks. In a 5.1 / multi-output context this region holds additional pan positions and per-output gain reduction values (e.g. `64 00 00 00` = 100). |
| `+87`  | i32 LE   | volume_main (record B / mirror)| Mirrors `+1` in every observed file. |
| `+91`  | u8       | mute_main (record B / mirror)  | Mirrors `+5` in every observed file. |
| `+103` | i32 LE   | pan field (record B)           | Often `0` even when `+13` is `−100`. This appears to be a **separate** pan slot, not a strict mirror of `+13` — likely the Edit-window pan vs Mix-window pan, or pre-/post-automation. |
| `+171` | u8       | secondary flag                 | `0`/`1`. Set on a subset of tracks; appears to mark "track is in active mix group" or similar (correlates with non-zero `pan_main` in `live-concert-session.ptx`). Not required for import. |
| `+175` | u8       | secondary flag                 | Same value as `+171` in every observed file. |
| `+265` | i32 LE   | constant `0x32` = 50           | Always 50; possibly default send level (50% / −6 dB). |
| `+269` | i32 LE   | constant `0x32` = 50           | Always 50; same as above (second send slot). |

### Volume scale interpretation

Decimal value × 0.1 dB. Examples from `live-concert-session.ptx`:

| Raw  | dB     |
|-----:|-------:|
| `+120` | +12.0  |
| `+50`  | +5.0   |
| `0`    | 0.0 (unity) |
| `−54`  | −5.4   |
| `−164` | −16.4  |
| `−1440`| −144.0 ≈ −∞ |
| `−1554`| −155.4 ≈ −∞ |

Treat any value `≤ −1440` as `-∞ dB` (muted-by-fader).

### Mute decoding

Concrete check against the user's "Lord of the Fight" session — in that
session the user reported "many tracks are MUTED". The `0x1029.mute_main`
flag is set (`= 1`) for the following tracks, every one of which is a stem
that one would mute when listening to the printed mix:

| Track index | Name                                | mute |
|------------:|--------------------------------------|:----:|
| 02 | `02 LORD OF THE FIGHT.01`            | 1 |
| 05 | `02 LORD OF THE FIGHT_Bass`         | 1 |
| 07 | `02 LORD OF THE FIGHT_Guitar`       | 1 |
| 08 | `02 LORD OF THE FIGHT_Other`        | 1 |
| 09 | `02 LORD OF THE FIGHT_Piano`        | 1 |
| 10 | `SYZ`                                | 1 |
| 11 | `AC GTR Strum Demo 1`                | 1 |
| 12 | `AC GTR Strum Demo 1.dup1`           | 1 |
| 14 | `Bass Demo`                          | 1 |
| 15 | `Intro SFX 1`                        | 1 |
| 16 | `Intro SFX 2`                        | 1 |
| 17 | `Intro SFX 2.dup1`                   | 1 |

Tracks left audible (`mute_main = 0`): `ClickPrint`, `ShakePrint`,
`02 LORD OF THE FIGHT_Vocals`, `02 LORD OF THE FIGHT_Drums`, `Vocal Split.01`,
`El Gtr 1`. That matches what an engineer would do when building a vocal
print over a mix bounce.

### Multi-position pans (5.1 / multi-out)

Tracks routed to multi-channel outputs (Master 5.1, etc.) carry additional pan
positions in the `+17 … +87` region of record A. The values typically appear
as a series of `64 00 00 00` (= 100 = full-throw) markers separated by zero
bytes. For mono / stereo audio import we only need `+13` (and ideally `+103`
for verification).

## Where the data is not

- `0x102d` (30 entries) is **not** the per-track mix-state block. Each `0x102d`
  contains a child `0x2619` and holds *playlist names* like "Master 1",
  "Click 1", "Audio 1", "Audio 1.dup1", together with track-creation order
  and sample-rate references. This is the per-playlist display block, not
  the mixer state.
- `0x251a` (60 entries — really 30 × 2 because the same set is emitted under
  both the master `0x2519` track-list and a duplicate container) holds **track
  name + UID + track kind** (Audio = `0x00`, AUX = `0x02`, Master = `0x05`).
  No volume/pan/mute here.

## Reading the fields in Rust

```rust
/// Volume in 0.1-dB units. 0 = unity; ≤ -1440 is treated as -∞.
pub fn track_volume_centibel(payload: &[u8]) -> i32 {
    i32::from_le_bytes(payload[1..5].try_into().unwrap())
}

/// True if the track is muted on the Mix window.
pub fn track_mute(payload: &[u8]) -> bool {
    payload[5] != 0
}

/// Pan position. -100 = full L, 0 = center, +100 = full R.
/// For stereo tracks this is the left-channel pan; the right-channel pan
/// is encoded later in the record (multi-output area at +17..+87).
pub fn track_pan(payload: &[u8]) -> i32 {
    i32::from_le_bytes(payload[13..17].try_into().unwrap())
}

/// Convert centibel volume to linear gain. Saturates ≤ -144 dB to 0.0.
pub fn volume_to_linear(centibel: i32) -> f32 {
    if centibel <= -1440 { return 0.0; }
    10f32.powf(centibel as f32 / 200.0) // dB = centibel/10, gain = 10^(dB/20)
}

/// Iterate every 0x1029 block in document order; skips the Master, so the
/// Nth `0x1029` corresponds to the Nth mixable track in `0x1015` ordering.
pub fn iter_track_mix<'a>(blocks: &'a [Block], data: &'a [u8])
    -> impl Iterator<Item = (i32, bool, i32)> + 'a
{
    blocks.iter()
        .filter(|b| b.content_type_raw == 0x1029)
        .map(move |b| {
            let p = &data[b.offset + 2 ..];
            (track_volume_centibel(p), track_mute(p), track_pan(p))
        })
}
```

## Open questions / lower-priority

- Exact meaning of bytes at `+171` / `+175` (correlates with byte-pattern
  groupings in `live-concert-session.ptx` but not needed for muted-import
  filtering).
- Whether `+103` (pan record B) is "Edit window pan", "saved snapshot",
  or "automation idle" — needs a session that has been edited in both
  windows to disambiguate. For audible-import filtering only `+5` matters,
  so this is parked.
- The trailing `0x32 0x32` constants at `+265 / +269`: assumed default
  pre/post send level. Untested.

## Repro tool

`cargo run -p dawfile-protools --example dump_track_props -- <session.ptx> 0x1029`
prints the full 281-byte payload plus the decoded `(val_a, flag, val_b)` for
every track in the file, in the same order as the mixer.
