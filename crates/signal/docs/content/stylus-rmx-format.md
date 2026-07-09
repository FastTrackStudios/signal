# Stylus RMX — Format + Signal Support Plan

Stylus RMX is a **groove player** — fundamentally different from per-key
sample playback (the model every other library in `Sampled/` uses). Loops
are pre-recorded audio at a fixed BPM, sliced into hits at production time,
and time-stretched at playback to match host tempo. MIDI keys then trigger
either the whole loop or individual slices.

## Source location (full set)

`/mnt/starcommand/Operations/nextcloud-data/data/codywright/files/Resources/Music/Audio Haven/Instrument Libraries/Spectrasonics/SAGE/`:

- `Stylus RMX/Core Library/` — 14 SAGE `.db` files, 7.5 GB
  (RMX Grooves, Kit Modules, Sound Menus, Groove Elements, Example Groove
   Menus, Default, Classic Stylus, Epic Energy, Urban, Swing, Club,
   Utility, Electronic, Breakbeats)
- `SAGE Libraries/EXP Libraries/` — 6 expansion `.db` files
  (Burning Grooves, Liquid Grooves, Metamorphosis, BackBeat, Bonus
   Spectrasonics, Retro Funk)

Total: **20 SAGE container `.db` files** with everything needed.

## Current state on disk

`/run/media/AudioHaven/Sampled/Drum Kits/Stylus RMX/` (13 GB):

```
Stylus RMX/
├─ Core Library/
│  ├─ RMX Grooves/              1,387 .aif    full grooves (4-stem multi-bus)
│  ├─ Kit Modules/              2,641 .aif    kit menus
│  ├─ Classic Stylus/             874 .aif    legacy content
│  ├─ Sound Menus/                176 .aif    drum-type menus
│  ├─ Groove Elements/             47 .aif    individual elements
│  ├─ Utility/                     10 .aif
│  └─ Default/                      1 .aif
└─ Expansions/                  several thousand more
   ├─ Burning Grooves/, Liquid Grooves/, Metamorphosis/, BackBeat/,
   ├─ Bonus Spectrasonics/, Retro Funk/, …
```

8,542 `.aif` files total. Format: stereo 16-bit 44.1 kHz AIFF. Filename
encodes BPM as a numeric prefix:

```
51-Scuba Duba/
  51-Scuba Duba Scuba.aif    ← drums-only stem
  51-Scuba Duba Drums.aif    ← (variant)
  51-Scuba Duba Combo.aif    ← (variant)
  51-Scuba Duba LoFi.aif     ← (variant)
  51-Scuba Duba Beat.aif     ← (variant)
```

A "groove" is the pack-level concept (`51-Scuba Duba`); each pack has
3-6 stem variants. BPM is in the filename, *not* in any AIFF metadata
chunk.

## Slice metadata location (resolved)

The audio AIFFs in the SAGE `.db` files carry **only** `COMM` + `SSND`
chunks — slice positions are NOT in the audio stream itself. Instead each
groove suite has a companion **`data.xml`** sidecar inside the same `.db`
container (sibling FILE entry of the audio blobs).

`sc-import`'s `extract_db` only extracts entries that match audio magic
(SpCA/RIFF/AIFF), so `data.xml` entries are silently skipped. They can be
recovered with a small patch to the extractor (just write all FILE
entries unconditionally; `.xml` and `.prt_rmx` are plaintext anyway).

### `data.xml` schema

One file per groove suite (e.g. ` 51-Scuba Duba/data.xml`, 44 KB).
Plaintext XML.

```xml
<LOOPSUITE NAME="51-Scuba Duba"
           AUTHORINFO="Groove Design by Tobias Marberger ©Spectrasonics"
           TEMPO="424c0000"      <!-- IEEE 754 BE float = 51.0 BPM -->
           TICKSPERQUARTER="960">
  <LOOP DRUMKITLOOP="0" START="0" STOP="1839263"
        AUDIOFILENAME="51-Scuba Duba Combo" NAME="51-Scuba Duba Combo">
    <COMBOCHILD NAME="51-Scuba Duba Shake" SLICEBASE="0"/>
    <COMBOCHILD NAME="51-Scuba Duba Scuba" SLICEBASE="32"/>
    <COMBOCHILD NAME="51-Scuba Duba LoFi"  SLICEBASE="86"/>
    <SLICE BEGIN="0"     END="14989"  CLASS="0" MOD="4"/>
    <SLICE BEGIN="14990" END="36988"  CLASS="0" MOD="4"/>
    <SLICE BEGIN="36989" END="57083"  CLASS="0" MOD="4"/>
    <!-- … 273 slices total across the suite … -->
  </LOOP>
  <LOOP AUDIOFILENAME="51-Scuba Duba Beat"  …> <SLICE …/> … </LOOP>
  <LOOP AUDIOFILENAME="51-Scuba Duba LoFi"  …> <SLICE …/> … </LOOP>
  <LOOP AUDIOFILENAME="51-Scuba Duba Scuba" …> <SLICE …/> … </LOOP>
  <LOOP AUDIOFILENAME="51-Scuba Duba Shake" …> <SLICE …/> … </LOOP>
</LOOPSUITE>
```

Each `<LOOP>` is one stem of the suite (one `.aif` file). Slices are in
**original-tempo sample positions**. `MOD` is presumably a slice-class
modifier; `CLASS` defaults to 0 for unmapped slices, increments for
authored kick/snare/etc class assignments.

`<COMBOCHILD>` declares a "Combo" view that reuses the parent loop's
audio but maps slices starting at `SLICEBASE` so different stems can
share the same audio with different slice mappings. The base `LOOP` is
the audio source; combos are virtual slice-routing variants.

### Per-`.db` companion file census

| `.db` (sample) | Audio entries | data.xml | `.prt_rmx` |
|---|---:|---:|---:|
| RMX Grooves       | 1,387 | 194 | 21 |
| Kit Modules       | 2,645 | 67 | 66 |
| Sound Menus       | 176 | (per-suite, ~39 prt_rmx) | |
| (others similar) |  |  | |

`.prt_rmx` files are SynthMaster-style XML patches (we already RE'd this
schema for Omnisphere — `<StylusRMXEngine Version Gain Pan AuxSend…>`).
They're user/factory presets, not slice metadata.

## Architecture for full Stylus RMX support

### New domain concepts

| Concept | Storage | Notes |
|---|---|---|
| **Groove** | `LibrarySpec.grooves: Vec<GrooveSpec>` | One spec entry per stem |
| **Slice** | `GrooveSpec.slices: Vec<SliceMarker>` | Position in samples + optional MIDI note |
| **Tempo** | `GrooveSpec.bpm: f32` | Native loop BPM |
| **Phrase length** | `GrooveSpec.bars: u8` | Often 1/2/4 |

### New `BlockType` variants

Add to `signal-proto/src/block.rs`:

```rust
GrooveBlock        // loop player + tempo-sync + slice triggering
TimeStretchBlock   // generic time-stretching DSP (used by GrooveBlock)
```

### `GrooveSpec` schema

```rust
pub struct GrooveSpec {
    pub file: String,                 // path to .aif/.wav loop
    pub bpm: f32,                     // native tempo
    pub bars: u8,                     // phrase length in bars
    pub time_sig_num: u8,             // 4
    pub time_sig_den: u8,             // 4
    pub slices: Vec<SliceMarker>,     // (sample_offset, midi_note)
    pub tags: Vec<String>,            // genre / mood / "drums" / "perc"
    pub label: String,                // display name
}

pub struct SliceMarker {
    pub sample_offset: u32,           // start in original-tempo samples
    pub midi_note: Option<u8>,        // None = unmapped, just a transient
    pub label: String,                // optional ("kick", "snare 1")
}
```

### Runtime requirements

1. **Time-stretching** — phase-vocoder or rubberband-style. Real-time
   rate-conversion; quality matters for groove playback. Open-source
   options:
   - SoundTouch (LGPL)
   - Rubber Band Library (GPL/commercial dual-licensed)
   - Soundpipe / faust phase vocoder (BSD-style)
   - Roll our own — competent phase vocoder is ~500 lines of DSP.
2. **Slice triggering** — at the audio thread, MIDI note-on for slice key
   K starts playback at `slices[K - base_note].sample_offset`.
3. **Tempo sync** — read host BPM, compute stretch ratio = `host_bpm /
   native_bpm`, drive the time-stretch.
4. **Crossfaded loop** — when a groove ends naturally, optionally
   crossfade back to the start for continuous playback.

### Slice MIDI mapping

Stylus RMX uses a fixed 48-key layout starting at C2 (or C3 depending on
convention) where each subsequent key triggers the next slice. We can
mirror that.

## Mapping plan (no runtime needed yet)

For now, just build a `library.styx` per groove pack listing the loops as
`GrooveSpec` entries with BPM extracted from filename. Slices stay empty.
Once we have a runtime, we either:

- Recover slice metadata via re-extraction, or
- Run transient detection over the existing .aif corpus to populate
  slice arrays.

### Per-pack styx output

Suggested layout: one `library.styx` per groove pack directory (e.g.
`Core Library/RMX Grooves/51-Scuba Duba/library.styx`). The pack lists
its 3-6 stem grooves, each with:

```styx
{
    file       "51-Scuba Duba Scuba.aif"
    bpm        51.0
    bars       4
    time_sig_num 4
    time_sig_den 4
    label      "Scuba"
    tags       (drums)
    slices     ()   // empty until we recover them
}
```

## Recommended order of operations

1. **Patch `sc-import` to dump non-audio entries** — write all `<FILE>`
   entries to disk, not just audio-magic ones. `data.xml` and `.prt_rmx`
   are plaintext; no decryption involved. Cost: ~10 lines in
   `crates/sc-import/src/steam.rs::extract_db`.
2. **Re-extract the 20 RMX `.db` files** with the patched tool. Output:
   ~10 K `.aif` files + per-suite `data.xml` + `.prt_rmx` companions.
3. **Add `GrooveSpec` + `SliceMarker` + `LoopSuite` to
   `signal-sampler::spec`**. Cost: ~80 lines of types + parse test.
4. **Add `Groove` + `TimeStretch` `BlockType`s** to `signal-proto`. Cost:
   2 lines in the `block_types!` macro + entries in `synth_blocks.rs`
   with `todo!()` runtime stubs.
5. **Write a Stylus RMX zonemap script** that, per suite directory:
   - reads `data.xml`
   - parses TEMPO (IEEE float BE), each `<LOOP>` + child `<SLICE>` rows
   - emits a `library.styx` with one `GrooveSpec` per LOOP, populated
     `slices` arrays
6. **Defer to runtime work**: time-stretch DSP, slice MIDI triggering,
   host-tempo sync, COMBOCHILD virtual-stem routing. These gate on
   Tier-1/2 of the `synth-engine-todo.md` roadmap.

## Comparison to existing libraries

| Library type | Playback model | Implemented |
|---|---|---|
| Multisample / drum kit | Per-key sample with vel layers + RR | ✅ |
| Spectrasonics zone-based | Per-(key × vel × RR × mic) zones | ✅ |
| Wavetable | Single-cycle waveform morphing | ☐ (spec only) |
| **Stylus RMX groove** | **Time-stretched loop + slice triggering** | **☐ (this doc)** |

## What NOT to mirror from Stylus RMX

- The "Chaos Designer" — randomized slice rearrangement / pattern
  generator. Useful but a Layer-level feature, not a Block.
- "Edit Mode" — visual slice editor with a piano-roll-like grid. Belongs
  in `signal-ui` if/when we want it.
- Built-in sub-bus routing (Pad / Drums / Music auxes). The existing
  `Engine.fx_sends` covers this concept.
