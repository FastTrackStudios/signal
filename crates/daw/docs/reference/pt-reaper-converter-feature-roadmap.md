# Feature parity roadmap (vs PT ↔ Reaper Converter v1.5.4)

Mapping the upstream tool's complete feature set to our current state
in `dawfile-protools` + `daw-reaper`. Source-of-truth field/struct
names come from the Swift type dump in
`docs/pt-reaper-converter-swift-dump.txt`.

## Status legend

- ✅ implemented (read AND write)
- 📖 read-only
- 🟡 read partial / unverified
- ❌ not implemented

## PTX → RPP (Pro Tools → Reaper)

| Feature | Swift source-of-truth | Our state | Block(s) |
|---|---|---|---|
| Tracks (name, type, format) | `PTXTrackSpec.name/type/format` | 📖 | `0x1014`, `0x251a` |
| Track output routing | `PTXTrackSpec.output` | ✅ | `0x260e` |
| Track input routing | `PTXTrackSpec.input` | ❌ | likely `0x260e` second UID |
| Track color | `PTXTrackSpec.color: UInt8?` | ✅ | `0x200b +163` |
| Track active flag | `PTXTrackStateSpec.active` | ❌ | candidate bytes in `0x1029` near +5 |
| Track hidden flag | `PTXTrackStateSpec.hidden` | ❌ | candidate bytes in `0x1029` near +5 |
| Track solo | `PTXTrackStateSpec.solo` | ❌ | candidate bytes in `0x1029` near +5 |
| Track soloSafe | `PTXTrackStateSpec.soloSafe` | ❌ | candidate bytes in `0x1029` near +5 |
| Track static mute | `PTXTrackStateSpec.mute` | 📖 (correct as +5 byte) | `0x1029 +5` |
| Track mute automation | `PTXTrackSpec.mute: [(pos, raw)]?` | ❌ | not in PTXBlocks enum — different dispatch |
| Track volume automation | `PTXTrackSpec.volume: [(pos, raw)]?` | ❌ | same |
| Track pan automation | `PTXTrackSpec.pan: [(pos, raw)]?` | ❌ | same |
| Track sends | `PTXTrackSpec.sends: [String: PTXSendSpec]` | ❌ | likely `0x260a` or `0x260c` per-send |
| Track folder nesting | `PTXTrackSpec.children: [String]` | 🟡 (`Track.is_folder` flag from `0x251a` payload byte after name; children list not yet found) | `0x251a` +name_end |
| Track notes | `PTXTrackSpec.notes: String` | ❌ | not yet found |
| Track playlists (alternates) | `PTXTrackSpec.playlists: [PTXPlaylistSpec]` | 📖 (parsed, not emitted) | `0x2428`, `0x2429` |
| Clips: position, length, fades, crossfade | `PTXClipSpec.start/end + .fades` | ✅ (partial — curve types) | `0x1050`, `0x262f` |
| Clip name | `PTXClipSpec.name: String?` | ✅ | `0x2629` |
| Clip color | `PTXClipSpec.color: UInt8?` | ❌ | candidate within `0x2629` |
| Clip mute | `PTXClipSpec.muted: Bool` | ❌ | candidate within `0x1050` (track entry) or `0x2629` |
| Static clip gain | `PTXClipSpec.clipGain: PTXClipGain?` | ❌ | `0x4403` (currently zero on most fixtures) |
| Dynamic clip-gain envelope | `[PTXClipGainBreakpoint]` | ❌ | unknown |
| Fades (in/out + curve) | `PTXFadeSpec` | 📖 | `0x262f` |
| Crossfade detection | `PTXFadeType.crossfade` | ✅ | derived from `0x1050 +46==0x01` |
| Fade curve shape (standard/equalPower/equalGain) | `PTXFadeCurve` enum | 🟡 (linear only) | `0x262f` trailing bytes |
| Audio file references | `PTXAudioFileInfo.filename` | ✅ | `0x1004`/`0x103a` |
| Audio file UUID (Avid Unique ID) | `PTXAudioFileInfo.fileUUID/hash1/hash2` | 🟡 (16-byte UUID located inside `0x1003 → 0x2106` child; not yet wired into `AudioFile`) | `0x1003`/`0x2106` |
| Region → audio-file index | `PTXRegion.sfIdx: Int` | 🟡 (name-stem heuristic) | inside `0x2629` payload |
| Bus + parent bus | `PTXBus` + `parentBusId: Int?` | ❌ | likely `0x2602`/`0x2603` |
| Bus is physical output | `PTXBus.isPhysicalOutput` | ❌ | same block |
| Markers | `PTXMarker` | ✅ | `0x2030`/`0x2077` |
| Marker color | `PTXMarker.color: UInt8?` | ❌ | within `0x2077` payload |
| Tempo map | `PTXTempoEntry` | ✅ | `0x2028` |
| Meter map | `PTXMeterEntry` | ✅ | `0x2029` |
| Surround pan (up to 7.1.2) | `PTXSurroundPanSpec` | ❌ | unknown block |
| Surround pan automation | (same) | ❌ | unknown |
| Sub-path buses (5.0, LCR, LFE) | `PTXSubPathSpec` | ❌ | unknown |
| Split-mono → interleaved merge | output-side feature | ❌ | not applicable to parser |
| Polyphonic file channel mapping (iXML) | `PTXTrack.TrackBlockMapping.channelIndex` | 🟡 (we read channel index but don't map iXML) | `0x1014` |
| WAV / AIF / RF64 support | input-side feature | 🟡 (WAV only currently) | filesystem |

## RPP → PTX (Reaper → Pro Tools)

We have NONE of this direction. The writer crate has only in-place
mutation of individual fields (color, output, mix state). Building a
.ptx from scratch requires:
- Block tree serializer with size recomputation
- Cross-reference rewriter (region indices, source-file indices,
  fade-def indices)
- XOR encryptor for the new content
- Unknown-block passthrough for fields we don't yet decode

These are all in `crates/dawfile-protools/src/write/` skeleton but
not implemented.

## Concrete next-step priorities

Ordered by user-impact-per-effort, with the exact byte / block target
to investigate:

1. **Marker colors** — 1-byte field inside `0x2077` payload. Easy
   target since we already decode every other marker field.

2. **Folder nesting (`PTXTrackSpec.children`)** — required for
   correct REAPER track hierarchy. Probably emitted as a parent-
   pointer or per-folder list near `0x251a`.

3. **Clip color / clip mute / clip gain** — within `0x2629` (audio
   region) payload bytes we haven't decoded.

4. **`PTXTrackStateSpec.solo / active / hidden`** — bytes in
   `0x1029` adjacent to mute. Currently can't isolate because the
   user's session has no varied solo state. Need a fixture session
   with known solo/inactive tracks.

5. **Track sends (`PTXSendSpec`)** — `0x260a` is the strongest
   candidate; multiple per `0x260d` track wrapper. Decoding sends
   requires identifying destination-bus reference and the static
   level/pan.

6. **Mute / volume / pan automation envelopes** — separate dispatch
   path, NOT in PTXBlocks enum. Likely a per-track auxiliary block
   that's only present when automation exists. May require finding
   a PT session with explicit automation to anchor.

7. **Surround pan + sub-path buses** — needed for 5.1+ sessions.
   Block type unknown.

8. **Avid Unique IDs (`PTXAudioFileInfo.fileUUID/hash1/hash2`)** —
   would enable correct file relink in PT without filename matching.

9. **RPP → PTX direction** — the entire writer side. Largest single
   effort; depends on every read field above being decoded first.
