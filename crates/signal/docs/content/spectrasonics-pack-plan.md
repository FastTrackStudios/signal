# Spectrasonics Library Packing Plan

Plan for converting Keyscape, Omnisphere, and Trilian into `.signalpack` files,
then sweeping the rest of `/run/media/AudioHaven/Sampled`.

See `signalpack-keyscape.md` for prior Keyscape session notes.

## Source / output paths

- **Encrypted source:** `/run/media/starcommand/Resources/Music/Audio Haven/Instrument Libraries/Spectrasonics/STEAM/`
- **Decrypted samples:** `/run/media/AudioHaven/Sampled/`
- **Final packs:** `/run/media/AudioHaven/Signal/`
- **Decryption tool:** `/home/cody/Development/FastTrackStudio/sample-collector` (`sc-import` for STEAM, `sc-analysis`, `sc-export`)

## Granularity

**One `.signalpack` per source `.db` patch file.** Not per-library, not per-section.
A `.db` is the natural patch unit in Spectrasonics STEAM containers.

## Output layout — Library → Patches + shared resources

```
/run/media/AudioHaven/Signal/<Family>/<Library>/
  <Patch>.signalpack         ← one pack per source .db
  <Patch>.signalpack
  …
  IRs/                       ← shared resources usable by any patch in this library
    <IR set>/
      *.wav
  Wavetables/                ← (future) shared wavetable resources
  Settings/                  ← (future) shared preset/settings blobs
```

Shared resources sit at the Library level because a single IR (cab, plate,
spring) is referenced by many patches. Encoding them once per library avoids
duplication and lets the convolver address them by stable path.

Once SignalPack gains multi-resource container support, `IRs/` and
`Wavetables/` may collapse into per-library `<Library>.resources.signalpack`
or stay as raw trees — TBD by access pattern. For now they live as raw WAV
trees because the convolver isn't built yet.

Counts (Phase 0 audit):

| Suite      | `.db` files | Notes                                     |
|------------|-------------|-------------------------------------------|
| Keyscape   | 55          | + 8 IR `.db` in `Settings Library/Presets/Factory/Effects/Impulse Data/` |
| Omnisphere | 5,526       | + `Wavetables/~BundleArchives.db` (2.7 GB binary container) |
| Trilian    | 61          | bass-only                                 |

## SignalPack format direction

Signal will be both sampler and synth. SignalPack must be a multi-resource
container holding any of:

- **Sample multisamples** (current — FLAC i24)
- **Wavetables** (new — for synth)
- **Impulse responses** (new — recovered from STEAM)
- **Settings/preset blobs** (new)

Single `kind` field in the current 64-byte header gets extended to a per-resource
kind in the index. Engine sampler reads only sample resources for now; synth and
convolver hook in later. Header magic stays `SIGPACK\0`; bump version when
adding multi-kind index.

## Phase 1 — Keyscape (in progress)

### 1a. Pack remaining patches (immediate)

42 `.styx` mappings already authored under `/run/media/AudioHaven/Sampled/Keys/Keyscape/<patch>/library.styx`.
4 packs already produced. 38 remaining.

Command per patch:

```bash
signal sampler pack \
  "/run/media/AudioHaven/Sampled/Keys/Keyscape/<Patch>/library.styx" \
  --samples-root "/run/media/AudioHaven/Sampled/Keys/Keyscape/<Patch>" \
  --output       "/run/media/AudioHaven/Signal/Keys/Keyscape/<Patch>.signalpack"
```

### 1b. Recover Keyscape IRs (next)

8 IR `.db` files (~440 MB):
`Bassman, Boutique, Brit-Vox, Classic Twin, Hiwattage, Innerspace (422 MB),
Rock Stack, Thriftshop Speaker`.

Current `sc-import` parses these as STEAM containers but yields zero audio
entries — IR data lives in a non-FLAC binary block we currently skip.

Action: reverse-engineer the post-XML region (start with `Innerspace.db`, the
largest target). Once format is known:

1. Add IR extractor branch to `sc-import/src/steam.rs`.
2. Define `SAMPLE_IR_FLAC_I24` (or raw float) resource kind in SignalPack.
3. Pack each IR as its own `.signalpack`.

### 1c. Smoke-test in desktop sampler

Load each pack in `signal-desktop`, MIDI-trigger a few notes per articulation,
confirm release samples and round-robin behave per `.styx`.

## Phase 2 — Omnisphere (after Keyscape complete)

Largest suite. Introduces articulations (Sustain / Short / Plucked / OneShot /
Pad / Hit) — first library to exercise `KeyswitchSpec` with CC58 mappings.

Steps:

1. Decrypt audit — verify SpCA key idx 7 (`0x3a2472ab`) across all
   Soundsource + Wavetable subdirs; document any mismatches.
2. Bucket Soundsources into `ArticulationKind` variants by patch-name heuristics.
3. Wavetable extractor — round-trip `~BundleArchives.db` through Omnisphere
   binary branch; define wavetable frame format (single-cycle, frame count,
   cycle len) as a SignalPack resource kind.
4. Multi-articulation `.styx` template — establish standard CC58 ranges
   (e.g. C-1=Sustain, C#-1=Short, D-1=OneShot) for reuse across orchestral.
5. Validate keyswitch firing end-to-end in desktop app.

## Phase 3 — Trilian

Articulation-heavy bass: Sustain, Slide, Hammer, Mute, Harmonic, Slap, Pop.
Trilian uses XOR-WAV (key `f5 a9 f5 ae`), not SpCA-FLAC.

Steps:

1. Confirm `sc-import` path detection picks the Trilian decrypter for all
   patches.
2. Map Soundsource names → `ArticulationKind` (Trilian names already encode
   articulation cleanly).
3. Directional legato — split slide-up vs slide-down.
4. Round-robin clustering via `sc-analysis` velocity/RR detection.
5. Standardize CC58 layout matching Omnisphere conventions; this becomes the
   reusable keyswitch standard for future orchestral libraries.

## Phase 4 — Engine work surfaced by Phase 2/3

Required `signal-sampler` features that Omnisphere/Trilian will exercise first
and orchestral libraries will need later:

- Legato note-distance filter (only fire transition if Δsemitones ≤ N).
- Keyswitch UI binding — surface current articulation in `signal-ui`.
- Release rule expansion: `release_velocity_min`, `release_pedal_required`,
  `release_min_hold_ms`.
- Per-section keyswitch in multi-section libraries.

## Phase 5 — Sweep `/run/media/AudioHaven/Sampled`

After 1–3 mature, generic loop per library:

1. `sc-import` if encrypted, else `sc-capture` if needs MIDI rendering.
2. `sc-analysis` for round-robin / velocity clustering.
3. Author `.styx`.
4. `signal sampler pack`.

Subdirs visible: `Drum Kits`, `FX`, `Keys`, `Orchestral`, `Z - Inbox`. Drums
(OneShot only) are fastest. Orchestral is hardest — do last, with articulation
infrastructure proven by Omnisphere/Trilian.

## Open questions

- IR `.db` binary tail format — known unknown until `Innerspace.db` is RE'd.
- Wavetable container — single mega-pack vs per-wavetable pack? Decide after
  probing `~BundleArchives.db` extraction.
- `.mlt_key/.mlt_omn/.mlt_trl` patch templates — defer until v1 ships unless
  needed for preset parity.
