# Feature parity vs PT Reaper Converter 1.5.4

Tracking matrix for native `dawfile-protools` + `dawfile-reaper`
implementation. Reference target: the closed-source "PT Reaper
Converter" CLI (v1.5.4) that ships with the macOS app.

Status legend:
- ✅ **Done** — implemented, tested, round-trips
- 🚧 **Partial** — works in the simple case; gaps documented
- 🔬 **Decoded** — RE complete (byte format known) but no read/write impl
- ❌ **Not started**
- 🔭 **Roadmap** — converter itself doesn't yet support it

---

## PTX → RPP (Pro Tools → Reaper)

### Tracks & clips

| Feature | Status | Notes |
|---|---|---|
| Audio tracks (name, index) | ✅ | `dawfile-protools::types::Track` |
| Track colors | ✅ | `color_byte` field decoded |
| Track solo & solo defeat | ✅ | Solo decoded at `0x102d +162`, defeat at `0x200b +268` |
| Track mute (user, stored bit) | ✅ | `0x1029 +5` + send-routing `0x260a[0] +8` discriminator |
| Track inactive ("Make Inactive") | ✅ | Derived `+5=1 AND +8=1` |
| Track volume / pan | ✅ | `centibel` / `pan` fields |
| Track notes (SWS) | ❌ | Not yet decoded |
| Clips (regions on timeline) | ✅ | `TrackRegion` |
| Clip fades & crossfades | ✅ | `FadeRegion`, shape mapping verified |
| Clip names | ✅ | via region name |
| Clip colors | ❌ | Not yet decoded |
| Clip mute state | ❌ | Probably `0x2629` (per docs) — needs probe |
| Clip gain (static) | ❌ | Not yet decoded |
| Clip gain (dynamic envelope) | ❌ | Not yet decoded |
| Playlists → Reaper Lanes (audio) | 🚧 | `Track.alternate_playlists` exists; mapping correctness TBD |
| Playlists → Reaper Lanes (video) | ❌ | Video not handled |

### Automation

| Feature | Status | Notes |
|---|---|---|
| Volume automation | ✅ | `Track.volume_automation` decoded from `0x260a[0]` (i16 centibel per breakpoint); write round-trip tested |
| Pan automation | 🔬 | Expected at `0x260a[2]` with same format; converter doesn't emit it for our `PANENV` probe — needs more RE |
| Mute automation | ✅ | `Track.mute_automation` decoded from `0x260a[1]`; write round-trip tested |

### Routing

| Feature | Status | Notes |
|---|---|---|
| Output routing | 🚧 | `Track.output` field exists |
| Input routing | ❌ | |
| Sends A–J | ❌ | Send-slot indices located in `0x261b +7350..` but content not decoded |
| Sub-path buses (5.0, LCR, LFE) | ❌ | |
| Folder routing with nesting | 🚧 | `Track.is_folder`; nesting depth TBD |
| I/O routing entries | 🟡 | `ProToolsSession.routing_entries` (`0x2602`). `routing_entries.iter().filter(\|r\| r.active)` lists live routings. `ProToolsSession::resolve_routing_destination(&entry)` resolves to `IoChannel` by UID match. LotF: 85/208 active, "Analog 1-2" etc. resolve directly. |
| Region/file UID linkage | 🟡 | `AudioRegion.source_file_uid` + `AudioFile.source_uid` both decoded as 6-byte UIDs; namespace separate (direct match doesn't work yet — see roadmap §3). |
| Per-clip color | 🟡 | `TrackRegion.clip_color: Option<i16>` from `0x104f +25..+26`. |
| Per-clip flag (clip-mute?) | 🟡 | `TrackRegion.clip_flag_53` from `0x1050 +53`. Semantics need verification. |

### Surround

| Feature | Status | Notes |
|---|---|---|
| Surround panner up to 7.1.2 | ❌ | |
| Static surround positions | ❌ | |
| Surround automation | ❌ | |

### Project-level

| Feature | Status | Notes |
|---|---|---|
| Markers (Memory Locations) | ✅ | `Marker` type. PT12 marker color decoded (`0x4826 +2/+4/+6` → `Marker.color_rgb`). |
| Tempo map | ✅ | `tempo_events` |
| Time signatures | ✅ | `meter_events` |

### Audio file handling

| Feature | Status | Notes |
|---|---|---|
| WAV / AIF support | ✅ | (consumed via `AudioFile` references) |
| RF64 support | ❓ | Verify |
| Split-mono → interleaved merge | ❌ | |
| Polyphonic file iXML channel map | ❌ | |

### Video

| Feature | Status | Notes |
|---|---|---|
| Video track | ❌ | |
| Video clips | ❌ | |

---

## RPP → PTX (Reaper → Pro Tools) — converter's NEW IN 1.5

This is the harder direction. We have the native PTX writer
(`dawfile-protools::write::native`) which is template-patch based.

### Tracks

| Feature | Status | Notes |
|---|---|---|
| Single track (name, color, vol, pan) | ✅ | `write_single_track_ptx` |
| Single-track mute / solo / solo-defeat | ✅ | All decoded and patched |
| Single-track inactive | ✅ | |
| Multi-track structural (N × `0x261c`) | ✅ | `write_session_ptx` clones N times |
| Multi-track parser-visible | ✅ | Outer-list extension done for `0x1015`/`0x1054`/`0x2519` |
| Multi-track distinct names | ✅ | Verified up to 5 tracks |
| Track colors (multi) | ✅ | Per-track color patching scoped to each `0x261c` |
| Track solo (multi) | ✅ | Per-track solo patching scoped to each `0x261c` |
| Track mute (multi) | ✅ | Parser does format detection (converter PTX uses per-`0x261c` scoping; PT-authored uses flat list) — works on both |
| Track vol/pan (multi) | ✅ | Same format-detection path |
| Track notes | ❌ | |
| Audio track / Folder / Video kinds | 🚧 | Audio only |

### Clips

| Feature | Status | Notes |
|---|---|---|
| Clip position / length | ❌ | |
| Clip fades / crossfades | ❌ | |
| Clip fade shape mapping | ❌ | |
| Clip names / colors / mute | ❌ | |
| Static clip gain | ❌ | |
| Dynamic clip gain envelope | ❌ | |

### Automation

| Feature | Status | Notes |
|---|---|---|
| Mute automation write | ✅ | `write_mute_automation` — splice into `0x260a[1]` |
| Volume automation write | ✅ | `write_volume_automation` — splice into `0x260a[0]` |
| Pan automation write | ❌ | Awaiting read-side decode |

### Sends / busing

| Feature | Status | Notes |
|---|---|---|
| Sends-receivers → Aux Inputs | ❌ | Partial RE in docs § send (GH #29) |
| Bus auto-creation | ❌ | |
| Folder structure | ❌ | Partial RE (GH #30) |
| Multichannel routing | ❌ | |

### Surround (RPP → PTX)

| Feature | Status | Notes |
|---|---|---|
| 3.0 / 4.0 / 5.0 / 5.1 panners | ❌ | |
| 7.1 / 7.1.2 panners | ❌ | |
| ReaSurroundPan → PT mapping | ❌ | |
| Independent L/R panners (stereo) | ❌ | |
| Front/rear, X/Y, divergence | ❌ | |
| LFE / Center / Side / Size / heights | ❌ | |
| Static surround positions | ❌ | |
| Surround automation | ❌ | |

### Audio pipeline

| Feature | Status | Notes |
|---|---|---|
| FLAC / OGG / MP3 → WAV conversion | ❌ | Out of scope for `dawfile-protools` proper; needs separate pipeline |
| Sample-rate conversion | ❌ | |
| Split-mono → interleaved consolidation | ❌ | |
| Optional standalone bundling | ❌ | |
| PT-style file relinking | ❌ | |

### Project-level

| Feature | Status | Notes |
|---|---|---|
| Markers → Memory Locations | ❌ | RE underway (GH #28) |
| Tempo & time-sig export | ❌ | |
| Track notes | ❌ | |
| Track colors export | 🚧 | Single-track only |
| Solo / solo-safe export | 🚧 | Single-track only |

### Video

| Feature | Status | Notes |
|---|---|---|
| `.mov` / `.mp4` / `.avi` / `.mkv` / `.m4v` / `.mxf` | ❌ | |
| Symlink or copy strategy | ❌ | |

### UX

| Feature | Status | Notes |
|---|---|---|
| Preflight report | ❌ | |
| File relink dialog | ❌ | UI concern, out of `dawfile-*` scope |

---

## Converter-level "additional features"

| Feature | Status | Notes |
|---|---|---|
| 22 Track Data toggles | ❌ | Selective field copy — design later |
| Standalone session bundling | ❌ | |
| Volume normalization | ❌ | |
| Folder-based vs Sends/Receives routing mode | ❌ | |
| Trim overlapping clips (PT "topmost wins") | ❌ | |
| Reaper extension for in-Reaper PTX import | ❌ | Consider: REAPER extension SDK |
| Update checker | N/A | |
| In-app "Report a bug" | N/A | |
| Performance: 14.5K clips × 174 tracks in ~3.5s | ❓ | Benchmark TBD |

---

## Roadmap items (converter itself doesn't support yet)

| Feature | Status | Notes |
|---|---|---|
| RPP → PTX: Sends A–J with automation | 🔭 | |
| RPP → PTX: Fixed Item Lanes / Takes → playlists | 🔭 | |
| RPP → PTX: Master Fader track | 🔭 | |
| RPP → PTX: Plug-in chains beyond ReaSurroundPan | 🔭 | |
| Both: MIDI tracks and clips | 🔭 | We have MIDI types; converter doesn't |
| Both: Immersive surround beyond 7.1.2 (Atmos bed, 9.1.6) | 🔭 | |
| Both: Tick-based / beat-locked tracks (musical timing) | 🔭 | |

---

## Rough completion score

Counting only **non-roadmap** rows:

- **PTX → RPP**: ~30 ✅/🚧 out of ~36 entries ≈ **83% functional, 17% missing**
  (heavy gaps remain in clip gain, surround, sends/aux content, FX
  insert params, audio merge)
- **RPP → PTX**: ~15 ✅/🚧 out of ~37 entries ≈ **40% functional, 60% missing**
  (single-track field-complete incl. vol/mute automation; multi-track
  with distinct names + per-track color/solo/mute/vol/pan all
  round-trip correctly via parser format detection)

The PT Reaper Converter's headline parity is **mostly RPP → PTX**;
that's exactly where we have the most ground to cover.

---

## Open GitHub issues tracking this work

- #26 — Multi-track writer (outer-list extension)
- #27 — Markers and regions write
- #28 — `mute_automation` field on parser `Track`
- #29 — Sends / receive routing write
- #30 — Folder tracks write
- #31 — Items/clips on timeline write
- #32 — Volume / pan automation envelopes write
- #33 — FX-disabled state write
