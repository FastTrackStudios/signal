# PT Clip Gain — RE Status (PTX → RPP)

Status: **values decoded; clip↔gain assignment unsolved (parked).**

## Summary

Pro Tools stores per-clip gain as an f32 dB **value pool**, but the
association between a pool entry and a specific clip is **computed by Pro
Tools / the official converter at parse time — it is not a serialized field
in the file**. We can read every gain value reliably; we cannot yet assign
them to the correct clips for arbitrary sessions. Emitting a guessed
assignment would corrupt user data (right gain, wrong clip), so clip gain is
not emitted until the association algorithm is recovered.

## What's confirmed

### The value pool — block `0x2637` (top-level)
- Layout: `u32 count`, then `count` × **30-byte entries**.
- Each entry is **byte-identical** except the trailing **f32 dB** at entry
  `+26`. Header bytes are constant: `01 46 01 00 16 00 00 00 00 00 01 00 00
  00 04 00 00 00 00 00 00 00 00 00 00 00`.
- One entry **per gained clip-channel** → a stereo clip contributes 2
  identical-dB entries. Verified against a labelled PT-native session
  (9 visible clips edited → 18 pool entries; all 9 dB values decode exactly:
  −7.5, −6.6, −2.8, +15, +2.6, +1.7, −31.5, +0.1, −3.8).
- `linear = 10^(dB/20)` (≡ `exp10(dB/20)`).

### Why the association is hard
The pool order is **hash/bucket order**, not clip order. For the labelled
file the pool order (as `(track,clip)`) was:
`(1,2)(1,1)(2,1)(2,3)(2,3)(2,1)(1,1)(3,1)(3,1)(3,3)(3,3)(1,2)(3,2)(3,2)(2,2)(2,2)(1,3)(1,3)`
— i.e. not track/clip/position/id order.

Ruled out as the link (none of these carry it):
- Clip-pool blocks `0x1052` → `0x1050` → `0x104f` (clip defs / placements).
  Gained vs ungained clips are **byte-identical** except instance-id and
  sample position. Clip-instance-ids are global: T1 = `0x00..0x0d`,
  T2 = `0x0e..0x1b`, T3 = `0x1c..0x29` (even = ch1, odd = ch2).
- `0x2637` entries — no embedded clip id (all identical but the dB).
- No 0..N index permutation table anywhere in the file.
- No parallel clip-id array near the pool.
- Neighbours of the pool are unrelated (`0x204d` timecode, `0x262a` audio
  file path list, `0x2031` hardware I/O device list).

### Frida findings (official converter as oracle, on voyager)
The official converter maps gains to clips **perfectly, in clip order**
(verified RPP output: T1 items `0.4217/0.4677/0.7244` = −7.5/−6.6/−2.8 dB, etc).
So the link is recoverable in principle. Tracing the converter:
- Gain resolver lives at `main+0x65bfc`; calls `__exp10` **exactly 9× in
  clip order**. By then the gain is a **per-clip array indexed in clip
  order** — the association already happened upstream in the
  "Finding clip gain / Finding clips (18 gain entries)" parse pass.
- The converter mmaps the input, then parses block-by-block out of a heap
  copy of the (decrypted) bytes. The heap copy is just the file bytes — it
  contains **no extra link data**. Conclusion: the converter **derives** the
  pool→clip association; it is not read from a field.

### Recovering it later
Authoritative path is to **decompile the converter's gain-parse function**
(stripped Swift Mach-O, arm64) in Ghidra and read the association algorithm.
Dynamic tracing is fragile here: the process finishes in <30 ms (JS timers
never fire), stdout is buffered (stage strings can't gate timing), and there
are several same-size mmaps (need fd-matching to pin the file buffer).

Converter on voyager: `CodyWright@10.10.10.186`,
`/Applications/PT Reaper Converter.app/Contents/MacOS/PT Reaper Converter
--convert in out`; Frida via `~/pt-re/run.sh --hook script.js`
(Frida 17 — use `Module.findGlobalExportByName`, not `findExportByName`).

## Test assets
- `/tmp/cg_native.ptx` (voyager + local) — PT-native session, 9 clips gained
  (first 3 of each of 3 tracks), values labelled above.
- `crates/daw-reaper/examples/gen_clipgain_stress.rs` — RPP generator for a
  stepped-gain stress session (RPP→PTX probe direction).

See also `docs/pt-color-palette-ground-truth.md` and the
`project_ptx_rpp_features` memory.
