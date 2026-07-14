# Keyscape / Spectrasonics STEAM `.db` → pack

Keyscape ships each instrument as a STEAM `.db`: a plaintext XML manifest (split at
`</FileSystem>`) followed by an XOR'd binary region of the audio + metadata XML.
**The `.db` carries authoritative zone metadata — never parse Keyscape filenames.**

## `.db` structure

```
<FileSystem>
  <DIR "<soundsource> ^ RR r03">           # a soundsource (body / release / mech / pedal / FX variant)
    <DIR "AudioFiles"> <FILE "…wav" .../> …</DIR>
    <DIR "Pitch 100-100">                  # key range for these samples
      <FILE "HitBundle.xml"/>              # marker only (no LayerHitStack) — SKIP
      <FILE "Direct.xml"/>                 # a MIC layer — <LayerHitStack>
      <FILE "Room.xml"/>  <FILE "Stereo Mics.xml"/> …
```

Each mic layer XML:
```
<LayerHitStack>
  <HitVelocity Minimum="0" Maximum="15">                 # velocity band
    <SampleWaveform RoundRobinSequenceNum="0" BaseNote="100"
        AudioFilePath="../<ss>/AudioFiles/RR01_SL01 ….wav"
        Level="3fca8bdb"    # hex f32 gain  → gain_db = 20·log10(Level)
        A440="3f800000" />  # hex f32 tune  → tune_cents = 1200·log2(A440)
```

So each `<SampleWaveform>` → one zone: `file`, `key_min/max` (Pitch dir), `root`
(BaseNote), `vel_min/max` (HitVelocity), `rr` (RoundRobinSequenceNum), `gain`
(Level), `tune` (A440), `mic` (XML basename).

## Mic layers (the `<LayerHitStack>` XML names)

`Direct`, `Room`, `Stereo Mics`, `Mono Mic`, `Microphone`, `Tube Mic`,
`Mono Overhead`, `Wide Stereo Mics`, `NT5`, `PZM Mic`, `Pickup 1`/`2`,
`Direct Pickup`, `AMP`/`Amp`, `Default Layer`, `Mic`, `149`, `300`, and
`Pedal Down`/`Pedal Up` (these two are CC64 trigger layers, not mics).
`normalize_mic` (in `zonemap.rs`) collapses them to `Main`/`Direct`/`Room`/`Stereo`
and keeps distinctive names verbatim.

## Commands (in the `sample-collector` repo, `crates/sc-import`)

```
sc-import steam     <db|dir> <out>   # extract audio (SpCA→FLAC), soundsource-aware
sc-import zonestats <db|dir>         # dump extracted zones: mics, roots, RR, vel
sc-import zonemap   <db|dir> <out>   # emit a zones styx from .db metadata
```

`steam` applies collision-proof soundsource tagging (`keyscape_ss_tags`) so
same-named samples from different soundsources (body vs mechanical noise, mic
variants) don't merge. Then build the pack with `build_pack` (main repo).

## Library-wide facts (from the 44-patch survey)

- 14 mics; up to 7/patch. 8 body/FX articulation variants (mute/tremolo/vibra/
  wah/suitcase/amp/fast/slow) + release + mechanical + pedal.
- Up to 16 round-robins; 1–152 velocity bands; per-sample tune (26 patches) and
  gain (38). Duo Maps Stage/Studio are 0-audio combos referencing other patches.
- Full requirements + per-patch tables:
  `crates/signal/docs/content/signalpack-zone-requirements.md`.
- Extraction/mapping deep-dive:
  `crates/signal/docs/content/keyscape-soundsources.md`,
  `signalpack-keyscape.md`.
