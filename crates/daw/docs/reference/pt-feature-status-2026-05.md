# Pro Tools feature coverage — status against 9-item goal

Snapshot of where each of the 9 goal items stands after this round of
RE work, with concrete blockers identified.

## ✅ Landed this round

### Item 2 (partial) — Folder / hierarchy flag

`Track.is_folder: bool` decoded from `0x251a` payload byte immediately
after the length-prefixed name. `0x01` = participates in folder/stem
grouping, `0x00` = leaf track.

- Surface accuracy: byte set for Master, folder-parents, and stem
  family tracks (LotF's 02 LORD family); cleared for leaf
  audio/aux/inst tracks.
- Disambiguation TBD with `folder-nesting.ptx` fixture.

Full `children: [String]` mapping per `PTXTrackSpec.children` not
implemented — PT stores it in a separate block we haven't located.

## 🟡 Identified, not yet wired through

### Item 8 — Avid Unique IDs (file relink)

`0x1003` (WavMetadata) blocks each contain a 16-byte UUID near offset
+0xb..+0xd of the inner `0x2106` child. Visible in LotF:

  block[01]: `98 b5 ca ce 5e 2b 42 6c 8b 24 d8 8f 84 33 ab d4`
  block[02]: `68 e1 06 74 06 09 45 fa 8b 0e db 1b f7 83 06 f5`

Also visible: per-file creation app string (e.g. "Logic Pro X").

Promote `AudioFile` struct with `fileUUID: [u8; 16]`, `creator_app:
Option<String>`. PT can relink files by UUID even when their filename
or path changes.

## ❌ Blocked on missing fixtures

### Item 1 — Marker colors

Need `colored-markers.ptx` with 8 explicitly-colored markers per the
sample-data doc. LotF has 16 markers but all use PT's default color.

### Item 3 — Clip color / clip mute / clip gain

Need `clip-attributes.ptx`. Candidate location: 6 unidentified bytes
in the `0x2629` (AudioRegionNew) payload after the name field.
Existing fixtures all use default clip attributes.

### Item 4 — Track solo / active / hidden

Need `solo-track-states.ptx` with one track per state. The bytes are
likely in `0x1029` adjacent to `+5` (mute), but LotF has no varied
solo state so we can't disambiguate the 4 boolean positions.

### Item 6 — Mute / volume / pan automation envelopes

Need `mute-automation.ptx`. The static mute (`+5`) is already
decoded. Automation envelopes are stored in a separate content_type
NOT in the 24 PTXBlocks enum cases we've enumerated — meaning the
upstream tool uses a different dispatch path. Need a session with
explicit automation to anchor the block ID.

### Item 7 — Surround pan

Need `surround-pan.ptx` with a 5.1+ track and known pan position.
Upstream tool has `PTXSurroundPanSpec` with x/y/lfe/divergence fields
plus automation. Block ID unknown.

## 🟡 Partial / needs more RE

### Item 5 — Track sends

Each `0x260d` track wrapper contains 2-3 `0x260a` children. Per the
recovered Swift `PTXSendSpec` model, each has: destination bus,
level, pan, pre/post-fader flag, mute, active flag.

We can read fixed-position bytes already: the first `0x260a` at
+26..+30 holds an i16 LE level that mirrors the track fader. But
without more structural mapping (which 0x260a is which send slot,
and where the destination-bus ID lives) the full send model can't
be reconstructed.

`aux-sends.ptx` fixture (priority-2 sample data) would unblock this.

## ❌ Massive — entire next direction

### Item 9 — RPP → PTX writer

We have NONE of this direction. The upstream tool's
`PTXClipWriter.swift`, `PTXPoolWriter.swift`,
`PTXAutomationWriter`, `PTXFadeWriter`, `PTXSurroundPanWriter`,
`PTXSendWriter`, `PTXRoutingWriter` cover the writer surface.

To build this we need:

1. **Block tree serializer** — every block we currently read must
   be writable. Currently only in-place edits work; structural
   insertion/removal doesn't.
2. **Cross-reference rewriter** — region indices, source-file
   indices, fade-def indices, marker numbers must stay valid
   across edits.
3. **XOR re-encryption** — already symmetric for read; needs the
   write-side seed selection.
4. **Unknown-block passthrough** — every byte of every block we
   don't fully understand must survive the read→edit→write cycle
   verbatim. Foundation already in place (`raw_block::encrypt`
   round-trips byte-identical), but doesn't extend through
   structural mutations yet.
5. **Stable UID generation** — when REAPER content creates new
   PT tracks/regions, their UIDs must collide-free with the
   existing PT session's UID space.

Realistic order to ship this:
- Phase A: round-trip identity proof on more fixtures (mostly done).
- Phase B: per-field in-place writers for the ~12 fields we decode
  today (volume, mute, pan, color, output routing done already).
- Phase C: block-add API for adding a single track/region to an
  existing session.
- Phase D: full RPP→PTX from scratch.

Each phase is several days of focused work.

## Summary table

| # | Feature | Status | Blocker |
|---|---|---|---|
| 1 | Marker colors | ❌ | needs `colored-markers.ptx` |
| 2 | Folder nesting | 🟡 flag landed | needs `folder-nesting.ptx` for full children mapping |
| 3 | Clip color / mute / gain | ❌ | needs `clip-attributes.ptx` |
| 4 | Solo / active / hidden | ❌ | needs `solo-track-states.ptx` |
| 5 | Track sends | 🟡 partial | needs `aux-sends.ptx` |
| 6 | Mute / vol / pan automation | ❌ | needs `mute-automation.ptx` |
| 7 | Surround pan | ❌ | needs `surround-pan.ptx` |
| 8 | Avid Unique IDs | 🟡 identified | RE pass to wire into `AudioFile` struct |
| 9 | RPP → PTX writer | ❌ | several-week effort |

6 items blocked on simple PT-authored fixtures (the sample-data doc
lists each one). 2 items need additional RE on bytes we have.
1 item is a major build-out.
