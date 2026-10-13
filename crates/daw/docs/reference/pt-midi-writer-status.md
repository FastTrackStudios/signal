# Native PTX MIDI Writer — Status & Resumption Guide

Status as of 2026-05-23 (branch `protools-work`). This documents the effort to
**generate Pro Tools `.ptx` files containing MIDI from scratch** (the inverse of
our PTX→RPP MIDI parsing), so a converted session can be round-tripped back into
Pro Tools with its MIDI intact.

## TL;DR

- The **note/region/placement encoding is solved and Pro Tools accepts it** —
  PT loads our note chunks past every loader validation into "open session".
- The **blocker** is PT's session-index "registry" (`0x0002` block at EOF):
  ~80% decoded and byte-reproducible, but the remaining ~20% (an opaque
  back-reference record stream that includes per-chunk position offsets) could
  not be decoded well enough to **regenerate** for a changed block layout.
- Net: we can reproduce an *unmodified* session's registry byte-for-byte, but
  cannot yet author one for new content. That is the one thing standing between
  here and a working from-scratch writer.

The external "PT Reaper Converter" drops ALL MIDI in the RPP→PTX direction
(`Unified: clips=N fades=N` — audio only), which is *why* a native writer is
needed; it is not an option.

## Why a native writer (not the external converter)

Round-trip: `original.ptx → our protools_to_rpp → RPP (MIDI correct in REAPER)
→ external converter → roundtrip.ptx`. The external converter writes audio but
**no MIDI** into the PTX (verified: 0 midi_regions out, its own log processes
only audio clips). So MIDI must be authored by us.

## File / module map

- `crates/dawfile-protools/src/write/midi.rs` — MIDI block encoders + injector.
  - `ChunkNote { position, duration, note, velocity }` (ticks, 960000/qn).
  - `encode_note_chunk(notes, zero_ticks) -> Vec<u8>` — `MdNLB` note chunk.
  - `MidiTrackInput { notes, name }`, `inject_midi(session, tracks)` — builds
    chunks/regions/placements and replaces the `0x2000`/`0x2634`/`0x1058`
    payloads.
  - Round-trip tested through our own parser (offline).
- `crates/dawfile-protools/src/write/registry.rs` — `0x0002` index
  decode/encode. `encode_registry(session) -> Vec<u8>` is byte-identical to the
  original on all 6 PNG sessions (test `registry_byte_identity`), via lossless
  decode→re-encode (opaque spans copied verbatim — see Limitations).
- `crates/dawfile-protools/examples/reinject_midi.rs` — extracts a session's
  MIDI, re-encodes it via `inject_midi`, writes the `.ptx` (used to make PT
  test files).
- Parser side (the spec the writer must satisfy):
  `crates/dawfile-protools/src/parse/midi.rs`.

## PTX MIDI on-disk format (what we know)

### Block framing
Flat sequence of blocks: `5a XX XX | u32 size@+3 | u16 content_type@+7 |
payload@+9`; block end = `start + 7 + size`. `parse_raw(bytes)` decrypts and
builds the tree; `RawSession::encrypt()` is a byte-perfect inverse (verified
identity — serialization is NOT a source of bugs).

### Note chunks — `0x2000` block
Payload = `u32 chunk_count`, then per chunk an **`MdChun` container**:
`"MdChun" 01 00 <u32 byte_len> <MdNLB chunk>`. PT walks chunks by `byte_len`;
each chunk may carry trailing slack (real sessions: slack up to ~2.2× the data).

`MdNLB` chunk header (23 bytes): `"MdNLB" | u16 ver=3 | u32 field7 |
u32 n_events | u64 zero_ticks`, where:
- `field7 = n_events*47 + 22` (deterministic; verified across all chunks).
- `zero_ticks` carries the **2⁶² baseline**: `0x4000_0000_0000_0000 +
  ZERO_TICKS(0xe8d4a51000) + take_offset`. Top byte must be `0x40`.

Event records: **35 bytes**, in PT's **staggered** layout — note `i`'s onset is
record `i`'s `+27` field, its pitch/vel/dur are in record `i+1`. So N notes are
written as **N+1 records**, `n_events = N+1`. Record layout:
`[+0] note · [+1..9] duration (baseline-2⁶²) · [+9] velocity · [+10] 0x40 ·
[+11..19] & [+19..27] baseline-2⁶² zero · [+27..35] absolute pos = zero_ticks +
chunk-relative pos`. The position field also needs the 2⁶² top byte.
**No explicit terminator** — the staggered decode reads one record past the last
note into the chunk's trailing **slack zeros** (note 0, velocity 0 → dropped by
the emitter). A `0xff` terminator we tried makes PT choke; real sessions use
slack instead. (Self-contained decoding was disproven: it collapses note parity
~240 vs ~10 mismatches; staggered matches the exports.)

### Regions — `0x2633` (in `0x2634` region map)
`0x2633` wraps a `0x2628` (CompoundRegionGroup): name (u32-len-prefixed) at +2,
then a nibble-counted three-point (`cursor::parse_three_point`: high nibbles of
header bytes give the byte-widths of offset/length/start; values LE from +5,
order offset/length/start; MIDI offset = clip_src + 1e12). A u32 chunk index
follows the `0x2628` block. Region map `0x2634` payload = `u32 count` + region
blocks.

### Placements — `0x1058` map → `0x1057` groups → `0x1056` → `0x104f`
One `0x1057` per instrument track (kind 0x07/0x01 in `0x2519` order), with a
name then `0x1056` entries; each wraps a `0x104f` sub-entry: region index u32 at
+4, per-placement timeline position as a u40 at +9 (1e12-relative) when the
format byte at +16 is `0x40`.

### The registry — `0x0002` block at EOF (THE BLOCKER)
A session index storing the absolute file offset of every block (and children).
Header: `u32 entry_count | u32 (=1) | u16 (=3)`.
**Structured RC entry (decoded):** `u32 lead (child-group count) | u16
content_type | u32 parent (0xffffffff at top level) | u8 flag | u32 pad(=0) |
u32 refcount | refcount × reference`. References come in two forms: *primary* =
`01 04 00 01 00 | u32 offset | 6 zero bytes` (15 B); *child* = `u16 tag |
u32 offset | 5 bytes` (11 B). All 544 primary offsets land on real block starts.
**Opaque ~20% (NOT decoded):** a stream of shorter back-reference records (tags
seen: `0x4829`, `0x2501`, `0x2056`, plus a long run of bare u32 offsets) whose
framing could not be pinned down; the decoder preserves them as `Record::Raw`.
Crucially, **7 registry offsets point INSIDE the `0x2000` block** = per-chunk
position entries, and these live in the opaque stream. They must be regenerated
to match a new chunk layout, which we cannot yet do.

## The PT loader-validation gauntlet (each fixed, in order discovered)

Testing = inject MIDI into a copy of the original, open in Pro Tools on the
voyager Mac (only way to validate PT acceptance; slow, manual). Errors hit, in
order:
1. **"end of stream"** — `0x2000` payload was bare chunks; needs the `MdChun`
   container framing + leading `u32 count`. Fixed.
2. **"end of stream" again** — changing block sizes shifted later blocks and
   invalidated the registry's absolute offsets. Mitigated by padding replaced
   blocks back to original size (registry-safe) — but see #4/#5.
3. **"size of header doesn't match … MIDI Chunk list"** (partial) — `zero_ticks`
   and position fields were missing the 2⁶² baseline (top byte 0x00 vs 0x40).
   Fixed.
4. **Same error** — the size-keeping padding left dead bytes; PT requires the
   chunk-list to FILL the `0x2000` block. Fixed by absorbing slack into MdChun
   lengths, distributed across chunks (a single huge-slack chunk is rejected).
5. **Got into "open session", then a different choke** — the `0xff` terminator
   record. Removed (use slack zeros like real sessions). **PT now loads the
   chunk list.**
6. **Hangs on "open session"** — the zero-padding still left in `0x2634` /
   `0x1058`; PT walks block children and a zero run reads as a zero-size block →
   infinite loop. **This is where we are.** Padding is a dead end: zeros hang
   PT's walker anywhere. The fix is tight blocks (no padding) + a regenerated
   registry — blocked on the opaque 20%.

## What's committed (branch `protools-work`)

Encoders + each gauntlet fix are individual commits (search "PTX MIDI writer").
Key: `b1336af` registry decode/encode byte-identity. All offline tests pass
(`cargo test -p dawfile-protools` — `midi` and `registry` suites green).

## To resume

Goal: author a registry for a NEW block layout so tight (un-padded) MIDI blocks
can be spliced in. Concretely:
1. **Crack the opaque registry records** in `registry.rs` — the back-reference
   stream (`0x4829`/`0x2501`/`0x2056` + bare-offset run), especially the 7
   per-chunk offset entries inside `0x2000`. Success metric stays offline:
   `encode_registry` must remain byte-identical on unmodified sessions AND
   correctly relocate every offset when block positions change.
2. Then in `inject_midi`: stop padding (`replace_block_payload` should splice at
   natural size), and after splicing call a registry-rewrite that updates all
   stored offsets to the new positions (block-start offsets by displacement; the
   per-chunk offsets to the new chunk positions).
3. Validate in Pro Tools on voyager (`CodyWright@10.10.10.186`, files into the
   session folder so media links resolve; see
   `reference_voyager_official_converter`).

Alternative if the opaque registry stays uncrackable: **in-place note overwrite**
— overwrite the note records inside an existing session's chunks WITHOUT moving
anything (registry untouched → PT accepts). Works for same-session round-trips;
does not generate from scratch.

## Honest limitations

- No PT-openable from-scratch MIDI file has been produced. PT *accepts the note
  chunk encoding* but the full session (with our regions/placements) is not yet
  loadable because of the registry.
- `encode_registry` reproduces unmodified sessions only; it cannot relocate
  offsets for changed layouts (the opaque records are passed through verbatim).
- `inject_midi`'s placement groups don't yet map cleanly through
  `protools_to_rpp` (our own converter reads 0 MIDI items back from an injected
  file) — a separate wiring gap from PT acceptance.
