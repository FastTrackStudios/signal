# Sample sessions still needed to finish PT format coverage

The Swift type model recovered from PT ↔ Reaper Converter v1.5.4 gives
us the *schema* of every field PT stores (see
`docs/pt-reaper-converter-swift-dump.txt`). What we still need is
**ground-truth example sessions** so we can byte-anchor each field's
on-disk location.

Each entry below names exactly one PT session to author + what to
set, and which roadmap items it unblocks once captured.

## Priority 1 — small, single-purpose fixtures (fast to author)

### `mute-automation.ptx`
**Setup**: 4 audio tracks, each ~10 seconds long. Configure:
- Track 1: no mute, no automation (control)
- Track 2: static M button ON, no automation envelope
- Track 3: no static mute, but with mute automation envelope that toggles ON at 2 s, OFF at 5 s, ON at 7 s
- Track 4: BOTH static mute ON and an automation envelope that flips it OFF at 0 s (this is the case our LotF parser fails on)

Save the session. Capture the matching .aaf export too.

**Unlocks**: items 1 & 6 of the goal (mute / volume / pan automation
envelopes). Once we can read this we'll find the per-track automation
block ID and decode `[(positionSamples: Int, muted: Bool)]`.

### `colored-markers.ptx`
**Setup**: 8 memory location markers spread along the timeline. Color
each one a different palette cell:
- Marker 1 → palette row 0 col 0 (top-left red)
- Marker 2 → palette row 0 col 6 (red)
- Marker 3 → palette row 0 col 10 (yellow)
- Marker 4 → palette row 0 col 13 (green)
- Marker 5 → palette row 0 col 17 (cyan)
- Marker 6 → palette row 0 col 20 (deep blue)
- Marker 7 → palette row 1 col 0 (darker red)
- Marker 8 → palette row 2 col 6 (darkest red)

**Unlocks**: item — marker colors (`PTXMarker.color: UInt8?` inside
`0x2077`). Currently `0x2077` payload reads but the color byte
position is unidentified.

### `solo-track-states.ptx`
**Setup**: 6 audio tracks. Configure their state flags as follows:
- Track 1: nothing set (audible)
- Track 2: muted
- Track 3: soloed
- Track 4: solo-safe enabled
- Track 5: track set to **inactive** (greyed out)
- Track 6: track set to **hidden**

**Unlocks**: item 4 — Solo/Active/Hidden (`PTXTrackStateSpec.solo /
active / hidden / soloSafe: Bool?`). Currently can't isolate because
LotF has no varied state. Likely all four bits live in `0x1029`
adjacent to `+5` (mute), but we need a session where each tracks
sets exactly one bit for unambiguous mapping.

### `clip-attributes.ptx`
**Setup**: 1 audio track, 6 regions on the timeline:
- Region 1: default (no color, no mute, no gain)
- Region 2: clip color set (any custom)
- Region 3: clip mute enabled
- Region 4: clip gain set to **−6.0 dB** static
- Region 5: clip gain envelope (dynamic) — three breakpoints: 0 dB at 0 s, −12 dB at 2 s, 0 dB at 4 s
- Region 6: clip color + clip mute + clip gain combined

**Unlocks**: item 3 — clip color / clip mute / static + dynamic clip
gain (`PTXClipSpec.color`, `.muted`, `.clipGain`,
`PTXClipGainBreakpoint`). All four candidates likely live in
`0x2629` payload bytes we haven't decoded, plus possibly an
auxiliary per-clip block for breakpoints.

### `folder-nesting.ptx`
**Setup**: Folder structure as described:
- Outer folder track: "Drums"
  - Audio track: "Kick"
  - Audio track: "Snare"
  - Inner folder track: "Toms"
    - Audio track: "Tom1"
    - Audio track: "Tom2"
- Independent audio track: "Bass" (not in any folder)

We already have `tests/fixtures/routing-examples.ptx` which has
deeply-nested folders. That fixture is sufficient ground truth.
What's missing is the byte-level decode of which block stores the
parent → child relationships.

**Unlocks**: item 2 — folder nesting (`PTXTrackSpec.children:
[String]`). Already have the test data; need the RE pass.

### `track-notes.ptx`
**Setup**: 3 audio tracks, each with a different note set via the SWS
extension:
- Track 1: note "first track comment"
- Track 2: note "multi\nline\ncomment"
- Track 3: no note (control)

**Unlocks**: track notes (`PTXTrackSpec.notes: String`).

## Priority 2 — composite fixtures (longer to author but cover many features at once)

### `aux-sends.ptx`
**Setup**: 4 audio tracks, each with sends configured:
- Track 1: send to Bus 1 (pre-fader, −6 dB, pan center)
- Track 2: send to Bus 1 (post-fader, 0 dB, pan center) + send to Bus 2 (post, −12 dB, pan L−50)
- Track 3: send to Bus 1 (post, 0 dB) with **bypass enabled**
- Track 4: send to Bus 1 with a **volume automation envelope** on the send level

**Unlocks**: item 5 — track sends (`PTXSendSpec`, `PTXSendStateSpec.mute
/ preFader / active`). Per-track `0x260a` payload structure.

### `surround-pan.ptx`
**Setup**: One 5.1 audio track (or 7.1 if your PT supports). Configure:
- Static surround pan: 30° front-right
- A pan automation envelope sweeping from front-left to rear-right
  over 5 seconds
- LFE level set to −∞ (no LFE)
- Then another version of the file with LFE at 0 dB

**Unlocks**: item 7 — surround pan (`PTXSurroundPanSpec`,
`PTXSurroundPan`). Unknown block.

### `avid-unique-ids.ptx`
**Setup**: Create a session, import 4 distinct audio files. Save.
Then move the audio files to a *different* folder on disk and reopen
the session — note that PT relinks them automatically via Avid Unique
ID (not by filename).

The session file itself is the artifact; capture the .ptx along with
the file-listing showing each audio file's original UUID (visible in
PT's Workspace browser column "Unique ID" if enabled).

**Unlocks**: item 8 — Avid Unique IDs (`PTXAudioFileInfo.fileUUID,
.hash1, .hash2: Data?`).

## Priority 3 — large composite fixture (full coverage)

### `comprehensive-session.ptx`
**Setup**: Single session combining ALL of the above:
- 12 audio tracks across 3 folder groups
- Mute, solo, active, hidden, and color variation per track
- Volume + pan + mute automation envelopes on at least 4 tracks
- Aux sends to 2 buses, with one bus routing into another
- 4 clip-gain-modified regions
- 8 colored markers
- One 5.1 surround track with pan automation
- Track notes on 3 tracks

This single session lets us regression-test every roadmap item at
once and catches interaction bugs.

## Priority 4 — version-spread fixtures (regression baseline)

Same simple session (e.g. 2 audio tracks, 1 marker, 120 BPM) saved
out from **each major PT version** we want to support:

- PT 12.5 (current modern)
- PT 12.8
- PT 2019.x
- PT 2020.x
- PT 2022.x
- PT 2024.x
- PT 2025.x (latest)

This gives us a regression matrix for round-trip testing across
versions — PT's block format evolves slightly across major releases.

## What we already have (for reference)

- `crates/dawfile-protools/tests/fixtures/RegionTest.ptx` — basic
  region structures, PT12
- `crates/dawfile-protools/tests/fixtures/color-testing.ptx` — full
  23 × 3 color palette grid → already unblocked item: track color
- `crates/dawfile-protools/tests/fixtures/routing-examples.ptx` —
  deeply nested folders + named routing → ready to unblock folder
  nesting item once we decode the parent-child block
- `crates/dawfile-protools/tests/fixtures/green-dolphin-street.ptx` +
  backups — real-world jazz session with bus routing
- `crates/dawfile-protools/tests/fixtures/dreamers-circus.ptx` —
  live concert
- 14 upstream `ptformat` test sessions covering PT 5–12 region/track
  basics
- User's "Lord of the Fight" session — large mixed audio + MIDI + Inst

## Asking-tone summary

If/when you have time to author any of the priority-1 fixtures
above, drop them in `~/Downloads/` and tell us the name — we'll
copy into `tests/fixtures/`, run the existing analysis tooling
against them, and have the corresponding parser feature reading
correctly within a turn. Priority 1 fixtures unblock 6 of the 9
roadmap items; priorities 2-3 unblock the rest.
