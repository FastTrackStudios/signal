# Parity-push session progress — 2026-05-17/18

## Goal

Reach 100% parity with `docs/pt-parity-roadmap.md` (the canonical
read+write feature checklist for `.ptx` files).

## Status: substantial progress on a multi-week goal

A single session can't close 100% of the roadmap — many items are
opaque-blob parser RE (FX plugin params), require new fixtures (real
WAV-source for `clip_muted`/`clip_gain`), or need full block
serializer work (Phase D §15). But this session shipped concrete
movement on the highest-impact items.

## What landed (17 commits on `pt-re-features`, branched off `main`)

### Read-side (parser) — newly decoded

| Feature | Block | Status |
|---|---|---|
| Track solo | `0x102d +162` | ✅ |
| Track solo-defeat | `0x200b +268` (mirror `0x200a +259`) | ✅ |
| Track inactive (derived) | `0x1029 +5 == 1` AND `0x260a[0] +8 == 1` | ✅ |
| Track mute (effective, w/ send routing) | `0x1029 +5` ∧ ¬`0x260a[0] +8` | ✅ |
| Track color | `0x200a +97`, `0x200b +106`, `0x2015 +88` | ✅ |
| Mute automation envelope | `0x260a[1]` (per-track) | ✅ |
| Volume automation envelope | `0x260a[0]` (per-track) | ✅ |

### Write-side (`dawfile-protools::write::native`)

| Feature | Status |
|---|---|
| Single-track field-complete | ✅ name, color, mute, solo, solo-defeat, inactive, vol, pan, mute auto, vol auto |
| Multi-track structural (N × `0x261c`) | ✅ |
| Multi-track parser-visible | ✅ outer-list extension (`0x1015` / `0x1054` / `0x2519`) |
| Multi-track distinct names | ✅ verified up to 5 tracks |
| Per-track color / solo / mute / vol / pan | ✅ scoped to each `0x261c` |

### Parser improvements

- **Format detection.** Parser auto-detects converter-authored PTX
  (11× `0x1029` nested per `0x261c`) vs PT-authored (flat 1-per-track)
  and switches mix-block scoping accordingly. Both LotF user session
  and converter-cloned multi-track files parse correctly.
- **Duplicate-name handling.** `0x251a` walks past consecutive
  duplicates (multi-track interleave) while still stopping on the
  PT-authored 2× full-list copy marker.

### RE infrastructure added

7 new probe / inspection examples:
- `diff_block_counts`, `diff_track_groups[_xfile]` — byte-level diff
- `find_per_track_range`, `find_containing_block`, `find_lists`,
  `find_envtime` — block locators
- `dump_track_group_layout`, `dump_bytes`, `dump_regions`,
  `dump_envelope`, `dump_tracks_parsed`, `count_probe`, `check_state`,
  `check_auto`, `clone_track_mvp` — content inspectors

3 new probe variants added to `rpp_to_ptx_probe`: `one_track_aaa`,
`two_tracks_eq`, `three_tracks`.

### Test counts

- **Before session**: 14 lib + 15 + 14 + 1 + 1 + 3 = 48 tests, 0 ignored.
- **After session**: 18 lib + 15 + 14 + 1 + 1 + 3 = 52 tests, 0 ignored.

All `dawfile-protools` fixtures still byte-identically round-trip
via `RawSession::encrypt()` (the `round_trip_full` integration test).

### Parity matrix update (`docs/feature-parity.md`)

- **PTX → RPP**: 65% → 72% functional
- **RPP → PTX**: 15% → 40% functional

## Remaining roadmap — biggest gaps

The Phase A/B/C/D/E ordering in `pt-parity-roadmap.md` still applies.
Top-impact items left:

### Phase A (correctness)
- Region audio_file_index — current parser explicitly returns 0;
  needs careful byte-RE per region kind (varies with name length,
  region format old vs new). Tracked at §16.

### Phase B (mix fidelity)
- Track input routing — block not located
- Track output for multi-track converter PTX — single-track works
  via `set_track_output` but the baseline emits the empty-variant
  `0x260e` which the writer refuses to convert in-place
- Aux sends count/levels/destinations — partial RE; per-track
  `0x260a` sends decoded structurally but content not parsed
- HW insert routing — block not located
- Per-track FX insert chain — Phase B item but very opaque

### Phase C (production detail)
- Region clip gain (static + dynamic)
- Region clip mute — needs `clip_muted` probe with a real WAV
  fixture (current `item_basic` probe is zero-diff)
- Region pitch / time-stretch / Elastic
- MIDI CC / pitch bend / channel
- Pan automation read — `0x260a[2]` suspected but converter doesn't
  emit it for the `PANENV`/`PANENV2` probe names tried so far

### Phase D (round-trip writer)
- Full block serializer that handles ANY block, not just the
  baseline template
- Cross-reference rewriter for indices that change after edits
- Unknown-block passthrough (currently guaranteed at the raw-byte
  level via `RawSession::encrypt()` but not at the structural level)

### Phase E (niceties)
- Compound regions, alternate playlists round-trip, key-sig /
  chord-symbol items, track folders, icons/comments/height

## Session 2 (2026-05-18 continued) — Binary RE breakthrough

After hitting the limits of probe-and-diff at the codebase level, we
pivoted to **dynamic binary RE via Frida**. The pipeline:

1. SSH to voyager Mac where the closed-source PT Reaper Converter
   1.5.4 runs.
2. Inject Frida hooks into the running converter, capturing every
   read from the decrypted PTX buffer via `Data.subscript`.
3. Run the converter on probe fixtures and diff byte-reads between
   baseline and feature-isolated probes.
4. Each diff localizes a feature → specific PTX byte.

### Wins via Frida byte-read tracing

| Feature | Block + offset | Status |
|---------|----------------|--------|
| **Marker color (PT 12)** | `0x4826` payload +2/+4/+6 (R/G/B as u16-LE low bytes) | ✅ DECODED + wired in parser |
| **Region source-file UID** | `0x2628` magic +54..+59 (6-byte UID) | 🟡 DECODED + wired; groups L+R pairs correctly |
| **Audio file UID** | `0x1003` magic +45..+50 (between sentinels `0x2A` / `0x80`) | 🟡 DECODED + wired; 24 distinct UIDs on LotF (one per file) |
| Routing block fields | `0x2602` +10 (active flag), +47..+52 (6-byte destination UID), +33/+36/+50/+51/+52 (additional flags) | 📚 Documented; not yet wired |
| Per-clip sub-block | `0x104f` +9/+20/+24/+25/+26/+27 | 📚 Documented; needs probe with real audio to ID fields |
| Per-clip flag | `0x1050` +53 (boolean — mute or gain) | 📚 Documented; needs targeted probe |
| Track-state extra colors | `0x2015` +51..+52, +54..+55 (additional i16-LE color positions) | 📚 Documented; semantics TBD |
| File entry UID (in `0x1003`) | `+45..+50` 6-byte UID (sentinels `2A` at +44, `80` at +51) | 📚 Documented; doesn't directly match region UIDs — separate mapping mechanism |

### Frida pipeline + tools added

- `scripts/frida/trace_all_reads.js` — hooks `Data.subscript` to log
  every byte read, plus 8 RPP emit sites for correlation.
- `scripts/frida/trace_block_scan.js` — hooks `FUN_100175f6c` (the
  universal block-parse helper) to log every CT scan in order.
- `scripts/frida/trace_data_reads.js` — earlier variant.
- `scripts/probe_diff.sh` — end-to-end harness: build probe → trace
  → diff vs baseline → map differing offsets to blocks.
- `crates/daw-reaper/examples/find_blocks_at.rs` — map a file offset
  back to (block CT, within-block offset).
- `crates/daw-reaper/examples/map_offsets.rs` — bulk version, reads
  offsets from stdin.
- `crates/daw-reaper/examples/list_blocks_ct.rs` — list all blocks
  of given content-type in a fixture.
- `crates/daw-reaper/examples/dump_decrypted.rs` — dump decrypted
  PTX bytes for post-processing.
- `crates/daw-reaper/examples/dump_regions_idx.rs` /
  `dump_region_uids.rs` — region-list with per-region UIDs.

### Roadmap status updates

- §2 Marker color (PT 12) — was unaddressed → ✅ DECODED + write-ready
  (writer not yet implemented but offsets known).
- §3 File ↔ region mapping — was 🟡 name-stem heuristic → 🟡
  (improved: 6-byte UID groups L+R pairs; full file resolution
  pending).
- Foundation laid: 5 more §16 entries (clip color, clip mute, routing
  destination, additional 0x2015 colors, file UID) have known
  byte offsets, ready to wire.

### What gates 100% parity now

Less is unknown. Most §16 items that were "block not located" now
have candidate offsets identified by the Frida pipeline. Remaining
work per item:

1. Build a feature-isolating probe (often needs new REAPER builder
   methods, e.g. track notes, clip color, item with real audio).
2. Run `scripts/probe_diff.sh <probe>` to confirm offset.
3. Codify in `parse/` + `write/` modules.
4. Add a round-trip test.

Per-item this is ~30-60 min of work. With ~30 §16 items remaining,
realistic effort: 20-30 hours focused RE.

## Operational notes

- Pre-push hook blocks `git push -u origin pt-re-features` because
  unrelated tests in the `main` worktree don't compile right now
  (`csurf_subscribe_latency.rs`, `event_bus` mod, `daw-control`
  routing/transport methods, etc.). Local branch carries 17 ready
  commits; push after main builds.
- 8 GH issues opened for tracked gaps: #26–#33 in `FastTrackStudios/daw`.
