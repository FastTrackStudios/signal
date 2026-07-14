---
name: signalpack
description: "Use when packing raw WAV/FLAC/AIFF samples into a .signalpack (the signal-sampler pack format), authoring or editing a library.styx spec, understanding the pack binary layout, or extracting/mapping sampled instruments (esp. Spectrasonics STEAM .db like Keyscape) into packs. Covers convention-mode (filename parsing) vs authoritative zone-mode mapping, mics/articulations/round-robins, building with the build_pack example, and validating with peek_header/check_pack_resolve."
---

# /signalpack

Pack a folder of raw samples into a single `.signalpack` — the self-contained,
`mmap`-friendly container the `signal-sampler` engine plays. One file per
instrument: a 64-byte header, an embedded `library.styx` spec, a sample index,
and one FLAC block per sample.

## When to use

- Turning a directory of `.wav`/`.flac`/`.aif` into a playable `.signalpack`.
- Writing or fixing a `library.styx` (the spec that maps samples → notes/velocity/
  mics/articulations).
- Understanding the pack binary format or debugging a pack that loads but is silent
  / mis-mapped.
- Extracting a sampled instrument (Spectrasonics STEAM `.db`, Keyscape) into a pack.

## The two ways a pack maps samples → sound

A pack always embeds a `library.styx`. That spec maps playback events to samples in
one of two modes — **pick zone-mode for anything new**:

1. **Zone-mode (authoritative, preferred).** The styx has a `zones (...)` array;
   each zone states its `file`, `key_min/max`, `root_key`, `vel_min/max`,
   `rr_index`, `gain_db`, `tune_cents`, `mic`, `articulation`, `trigger_mode`
   explicitly. The runtime just reads them — no filename parsing, works for any
   naming. This is where the format is going (see
   `crates/signal/docs/content/signalpack-zone-requirements.md`).
2. **Convention-mode (filename parsing, legacy).** The styx has `articulations`
   but no `zones`; the loader scans sample filenames and guesses
   `(articulation, mic, dynamic, note, rr)` via `parse_sample_stem`. Fragile per
   naming scheme (the Keyscape note/mic/release bugs all came from here). Only use
   for libraries whose names already parse cleanly.

> **The styx `articulations` list gates PLAYABILITY.** A sample whose articulation
> id isn't declared (or aliased) loads but never plays. This is the #1 "pack is
> silent" cause. Zone-mode avoids it by making everything explicit.

## Pack raw WAVs → .signalpack (the workflow)

### 1. Lay out the samples
Put the audio files in one directory (flat is fine; the builder reads every audio
file directly under the dir). Keep a `library.styx` in that same dir.

### 2. Write the `library.styx`
See `references/styx.md` for the full grammar and a copy-paste template. Minimum:
`name`, `version`, `vendor`, `sections`, `mics`, `dynamics`, then either `zones`
(preferred) or `articulations` (convention).

### 3. Build the pack
```
cargo run -p signal-sampler --release --example build_pack -- \
    "<samples_root>" "<out.signalpack>"
```
This embeds `<samples_root>/library.styx` verbatim and FLAC-i24-encodes every audio
file into the pack body. It tolerates undecodable source files (warns, skips) and
only fails if nothing packs. Underlying fn: `signal_sampler::engine::cache::create_signal_pack`.

### 4. Validate before shipping
```
# embedded spec + entry count
cargo run -p signal-sampler --release --example peek_header -- "<out.signalpack>"
# every declared articulation resolves to real samples across the keyboard,
# and the engine's default-articulation pick
cargo run -p signal-sampler --release --example check_pack_resolve -- "<out.signalpack>"
```
`check_pack_resolve` PASS = every note resolves; PARTIAL means some notes have no
sample (often fine at the range edges). If the default articulation is a
release/attack layer instead of the body, fix the styx `kind` (see gotchas).

### 5. Load it
`PlayerPatch::from_pack(path)` builds the map from the embedded spec — no
`samples_root` needed. The keys rig auto-discovers `*.signalpack` under
`…/Signal/Libraries/Keys/Keyscape/Packs/` (`rig.rs` picks `from_pack` vs raw `load`
by file extension). See `references/format.md` and
`crates/signal/docs/content/keyscape-soundsources.md`.

## Extracting from a Spectrasonics STEAM `.db` (Keyscape)

The `.db` carries authoritative zones — never parse Keyscape filenames. Pipeline
lives in the `sample-collector` repo (`crates/sc-import`):

- `sc-import steam <db> <out>` — extract the audio (SpCA→FLAC), soundsource-aware.
- `sc-import zonestats <db|dir>` — dump extracted zones (mics, roots, RR, vel) to
  validate the mapping.
- The `.db` gives, per sample: key range (`Pitch N-M` dir), root (`BaseNote`),
  velocity range (`HitVelocity`), RR (`RoundRobinSequenceNum`), gain (`Level`),
  tune (`A440`), and mic (the `<LayerHitStack>` XML basename: `Direct`/`Room`/
  `Stereo Mics`/`Pickup 1`/…).

See `references/keyscape.md` and `crates/signal/docs/content/signalpack-keyscape.md`.

## Gotchas (hard-won)

- **Rebuild the pack after re-extracting/renaming samples.** A stale pack silently
  serves old audio. Convention-mode packs re-parse filenames at load, but a
  zone-mode pack bakes the mapping — rebuild it.
- **Styx gates playability** — undeclared articulations are inert (loaded, silent).
- **`kind @Release` matters** — the default-articulation picker skips
  `@Release`/`@Legato`/mech-ped-aux and plays the first remaining. If a release
  combo is mis-marked `@OneShot`, the patch plays key-off/attack noise instead of
  the body. (`docs/scripts/fix_release_kinds.py` fixes Keyscape styx.)
- **Convention-mode parsing is fragile** — a stray name-number ("E Piano 1")
  becomes the note; `SL01/SL02` mic layers collide if not mapped. These are exactly
  why zone-mode exists.
- **Offline audio-probe examples (`keyscape_probe`) are currently broken**
  (voices: 0) — validate with `check_pack_resolve` (static resolve), not audio.

## Binary format (one-liner)

`SIGPACK\0` magic + version + kind(FLAC-i24) + offsets (64-byte header), then FLAC
blocks, then a text index: `# spec_begin … <library.styx> … # spec_end`, then one
`source\toffset\tbytes\tchannels\tsample_rate\tnum_frames\tsamples` row per sample.
Full layout: `references/format.md` and
`crates/signal/docs/content/sampler-file-formats.md`.

## Tools

| Tool | Purpose |
|---|---|
| `--example build_pack` | raw dir + styx → `.signalpack` |
| `--example peek_header` | dump embedded spec + entry count |
| `--example check_pack_resolve` | static playability + default-articulation check |
| `--example dump_key` / `articulation_of` | how convention-mode parses a filename |
| `sc-import steam` / `zonestats` | Spectrasonics `.db` extract + zone dump |
