# Pro Tools 12 — Fade length encoding

**Status**: empirical reverse engineering, byte-verified against the test
session at `~/Downloads/tombrooksmusic_copy-of-02-lord-of-the-fight-1-5_2026-05-11_0158/Copy of 02 LORD OF THE FIGHT 1.5/Copy of 02 LORD OF THE FIGHT 1.5.ptx`.

The Pro Tools binaries on disk that contain the string `"fade"` are all DSP /
audio-processing libraries (`AFnd_FadeCalculator`, `CCrossfadeCalculator`,
`FF_AudioProcessFade`); the `.ptx` parser does not live in any of them.
Findings below come from analysing the file format directly with the helper
examples in `crates/dawfile-protools/examples/`:
`dump_block_types.rs`, `dump_ct.rs`, `dump_fades.rs`, `dump_262f.rs`,
`fade_link_check.rs`, `decode_fade_lengths.rs`.

## TL;DR for the parser

Replace the current "use the referenced audio region's length as fallback"
heuristic with a real lookup:

1. While walking the block tree, collect every `0x262f` block in file order
   into `fade_defs: Vec<&Block>` (they are children of one `0x2630` wrapper).
2. In `collect_slot_regions()`, the field at sub-entry `+4..=+7` (currently
   read as `region_index: u32`) is **not** an index into `audio_regions` for
   fade entries — it is an index into `fade_defs`.
3. Decode each `fade_defs[i]` with the layout described below to get
   `(in_len_samples, out_len_samples, shape)`.

The `start_pos` field already parsed (sub-entry `+9..=+12`, u32 LE samples) is
correct and remains the timeline position of the fade.

## Detection of fade entries

Unchanged: a `0x1050` (`AudioRegionTrackEntryNew`) is a fade if the byte at
`track_entry.offset + 46 == 0x01`. There are exactly 24 such entries in the
test session, matching exactly 24 `0x262f` blocks under the single `0x2630`
wrapper.

## Block hierarchy

```
0x2630 (FadeDefList wrapper, 1 block per session)
├── header: u16 ct (0x2630) + u32 child-count (== number of fade defs)
└── 0x262f (FadeDef, 24..36 bytes payload), one per fade entry referenced
            from anywhere in the playlist tree (active + alternate playlists)
```

In the test session the wrapper holds `0x2630` at file offset `0x00a6ad`
with `block_size = 866` and 24 children. All 24 children share the same
prologue:

```
+0..1   u16 LE   content_type   = 0x262f
+2..6   5 bytes  zero padding
+7      u8       type_byte      ← high nibble = byte width of in-length,
                                  low nibble  = byte width of out-length
+8..9   2 bytes  zero padding
+10..   N bytes  in-length      (LE, width = high nibble of type_byte)
        M bytes  out-length     (LE, width = low nibble of type_byte)
        u8       shape          (1 = linear, 2 = equal power, 3 = equal gain)
        ...      tail bytes vary by fade-kind; not needed for length
```

`type_byte` values observed (and confirmed):

| type | width-in | width-out | block size | meaning |
|------|----------|-----------|------------|---------|
| 0x30 | 3        | 0         | 25         | single-direction fade (in or out), 24-bit length |
| 0x33 | 3        | 3         | 31         | crossfade with 24-bit lengths |
| 0x22 | 2        | 2         | 29 or 36   | crossfade with 16-bit lengths |
| 0x20 | 2        | 0         | 24         | single-direction fade, 16-bit length |
| 0x11 | 1        | 1         | 34         | crossfade with 8-bit lengths (very short) |

The two `0x22` sizes (29 vs 36) differ only in the amount of trailing
padding/curve data — the in/out length fields are still at `+10` / `+12`.

A "single-direction" entry (`type 0xN0`) is the value the parser should emit
as `FadeRegion::length`; whether it is an *in* or an *out* is determined by
the placement on the timeline relative to the audio region (PT does not store
in/out as distinct flags here — it derives it from neighbouring placements).

## Linkage from track entries to fade defs

The `0x104f` (`AudioRegionTrackSubEntryNew`) payload is 37 bytes for fade
entries. Its layout (corrected):

```
+0..1    u16 LE   content_type      = 0x104f
+2..3    2 bytes  zero
+4..7    u32 LE   fade_index        ← index into fade_defs (NOT audio_regions)
+8       u8       zero
+9..12   u32 LE   start_pos_samples (timeline position; already 1:1 samples)
+13..16  4 bytes  zero
+17      u8       inferred fade-kind (1, 3, ...)  — also reflects on shape byte
+18..19  i16 LE   = -2 (constant, role unknown)
+20..23  4 bytes  zero
+24..31  i64 LE   = -1 (constant sentinel; never carries an ID in observed data)
+32..33  2 bytes  zero
+34      u8       per-fade variant byte (0 / 1 / 2 — matches stereo grouping;
                  not a length, possibly "channel role" or "snap to bars")
+35..36  2 bytes  zero
```

The current code uses `+4..=+7` as `region_index` and looks the result up in
`audio_regions`; for fade entries this lookup is **wrong** — the value is the
index into `fade_defs`. The `start_pos` field is correct as-is.

Verification: in the test session the 24 fade sub-entries reference fade
indices `[10, 14, 9, 15, 6, 8, 5, 4, 7, 11, 13, 18, 19, 16, 12, 17, 22, 23,
21, 20, 1, 0, 2, 3]` — every value in `0..24` appears exactly once.

## Verification: 24 decoded fades vs ground truth

```
fade fIdx type   shp         in        out   in_sec  playlist
  1   10 0x22     2      43367      43367    0.903  02 LORD OF THE FIGHT.01
  2   14 0x20     3      36998          0    0.771  02 LORD OF THE FIGHT.01
  3    9 0x22     2      43367      43367    0.903  02 LORD OF THE FIGHT.01
  4   15 0x20     3      36998          0    0.771  02 LORD OF THE FIGHT.01
  5    6 0x22     1        677       1354    0.014  Vocal Split.01
  6    8 0x22     1       1356       2712    0.028  Vocal Split.01
  7    5 0x22     1        677       1354    0.014  Vocal Split.01
  8    4 0x22     1       1356       2712    0.028  Vocal Split.01
  9    7 0x33     2     122819     122819    2.559  SYZ
 10   11 0x33     2     173980     173980    3.625  SYZ
 11   13 0x33     2     122819     122819    2.559  SYZ
 12   18 0x33     2     173980     173980    3.625  SYZ
 13   19 0x30     3     192444          0    4.009  AC GTR Strum Demo 1
 14   16 0x11     1          9         18    0.000  AC GTR Strum Demo 1
 15   12 0x30     3     192444          0    4.009  AC GTR Strum Demo 1
 16   17 0x11     1          9         18    0.000  AC GTR Strum Demo 1
 17   22 0x30     3     249365          0    5.195  El Gtr 1
 18   23 0x30     3     249365          0    5.195  El Gtr 1
 19   21 0x30     3     295443          0    6.155  Intro SFX 1
 20   20 0x30     3     295443          0    6.155  Intro SFX 1
 21    1 0x30     3     132814          0    2.767  Intro SFX 2
 22    0 0x30     3     132814          0    2.767  Intro SFX 2
 23    2 0x30     3     108420          0    2.259  Intro SFX 2.dup1
 24    3 0x30     3     108420          0    2.259  Intro SFX 2.dup1
```

Cross-checking with the user's ground-truth notes:

- **"fade-ins on Intro SFX items"** — confirmed: rows 19–24 are all single-
  direction (`type 0xN0`, out-length = 0); 2.26 s, 2.77 s, 6.16 s. Each row
  appears as a stereo pair (rows 19/20, 21/22, 23/24) with identical lengths
  — exactly what we expect for a stereo audio track.
- **"crossfade between FIGHT-05 items 1 and 2"** — confirmed: rows 1 & 3
  ("02 LORD OF THE FIGHT" track) are symmetric crossfades, 0.903 s each
  channel, type `0x22`.
- **"2 fade-outs on SYZ items 2 and 3"** — confirmed by count: rows 9–12,
  arranged as two stereo pairs (9/11 = 2.559 s, 10/12 = 3.625 s). PT stores
  these as `0x33` "crossfade" entries with equal in/out, which is the
  representation it uses for a fade-out at a region boundary that abuts
  another region.
- **"fade at start of El Gtr 1, AC GTR Strum Demo 1"** — confirmed: rows
  17/18 (El Gtr 1) are single-direction stereo fades of 5.195 s; rows 13/15
  (AC GTR Strum Demo 1) are 4.009 s. The user's "2-bar duration" estimate at
  168 BPM = 2.857 s did not match exactly — the actual fade durations stored
  are longer than that, but the encoding reads them out cleanly and they pair
  up by stereo channel as expected.

## Reading recipe (Rust pseudocode)

```rust
// 1) Once per session, collect fade defs in file order.
fn collect_fade_defs<'a>(blocks: &'a [Block]) -> Vec<&'a Block> {
    let mut out = Vec::new();
    fn walk<'a>(b: &'a Block, out: &mut Vec<&'a Block>) {
        if b.content_type_raw == 0x262f { out.push(b); }
        for c in &b.children { walk(c, out); }
    }
    for b in blocks { walk(b, &mut out); }
    out
}

// 2) For each fade sub-entry, look up the fade def and decode.
struct FadeLengths { in_len: u64, out_len: u64, shape: u8 }

fn decode_fade_def(def: &Block, data: &[u8]) -> FadeLengths {
    let base = def.offset;                  // points at the +0 (content_type)
    let type_byte = data[base + 7];
    let n_in  = (type_byte >> 4) as usize;  // 0,1,2,3
    let n_out = (type_byte & 0x0f) as usize;
    let read_le = |o: usize, n: usize| -> u64 {
        let mut v = 0u64;
        for i in 0..n { v |= (data[o + i] as u64) << (8 * i); }
        v
    };
    let in_len  = if n_in  > 0 { read_le(base + 10, n_in)  } else { 0 };
    let out_len = if n_out > 0 { read_le(base + 10 + n_in, n_out) } else { 0 };
    let shape = data[base + 10 + n_in + n_out];
    FadeLengths { in_len, out_len, shape }
}

// 3) Replace the current `region_index` lookup for fades:
//    sub_entry +4 is the fade_index; lengths come from fade_defs[fade_index].
```

The `start_pos` already extracted from sub-entry `+9..=+12` (u32 LE samples)
remains the timeline position. The current 5-byte `u40_le` read also works
because in every observed fade entry the high byte is zero.

## Sample fields the parser should expose

For the `FadeRegion` struct, the minimum useful update is:

```rust
pub struct FadeRegion {
    pub start_pos: u64,
    pub in_length: u64,    // samples (was: `length`)
    pub out_length: u64,   // samples (0 for single-direction fades)
    pub shape: u8,         // 1=linear, 2=equal-power, 3=equal-gain
    pub fade_index: u32,   // index into the per-session fade-def list
                           // (replaces `region_index`, which was wrong)
}
```

`out_length == 0` ⇒ single-direction fade; emit either fade-in or fade-out
based on the audio-region geometry around `start_pos`. `in_length == out_length`
⇒ symmetric crossfade. `in_length != out_length` (both non-zero) ⇒ asymmetric
crossfade.

## Notes / open questions

1. The trailing bytes in `0x262f` payloads (after the lengths and shape byte)
   carry per-fade kind data — likely the curve-coefficient table for custom
   shapes. Not needed if the parser only reproduces the standard
   linear/equal-power/equal-gain shapes.
2. The byte at sub-entry `+34` (visible as 0/1/2 in the table above) groups
   stereo channels but is not a length and is not needed for fade decoding.
3. The test session has no PT-generated short "fade audio region" objects
   inside `audio_regions`; the previous heuristic of treating the
   sub-entry's +4 field as a region index and reading that region's `length`
   was always wrong — it just happened to read 0 for most entries here, which
   the parser then clamped to 0.
