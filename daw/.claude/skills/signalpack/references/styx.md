# `library.styx` — the spec inside a pack

`library.styx` is the map from playback events → samples. It is embedded verbatim
in the pack. Facet-style syntax: `key value`, `( … )` lists, `{ … }` records,
`@Variant` enum tokens, `"quoted"` strings.

Structs: `features/sampler/signal-sampler/src/spec.rs` (`LibrarySpec`, `ZoneSpec`,
`ArticulationKind`, `MicSpec`, `SectionSpec`, `DynamicsSpec`).

## Header + declarations (both modes)

```
name    "Keyscape Wing Tack Piano"
version "1.0"
vendor  Spectrasonics

sections ({
  id          main
  label       "Wing Tack Piano"
  note_grid   ()          # empty = every semitone; else explicit sampled roots
  lowest_note A0
  highest_note C8
})

mics ({
  id    Main             # zone.mic references this id; empty zone.mic = mic-agnostic
  label Main
  kind  blended          # or a single mic position
})

dynamics {
  short_note_controller velocity
}
```

## Zone-mode (preferred) — explicit, no filename parsing

Add a `zones (...)` array. `is_zoned()` is true when it's non-empty. Every field
below exists on `ZoneSpec`:

```
zones (
  {
    file        "RR01_SL01 Celr02_60-15-F.flac"   # pack-relative sample path
    key_min     0                                  # inclusive key range
    key_max     60
    root_key    60                                 # pitch that plays untransposed
    vel_min     0                                  # inclusive velocity band
    vel_max     15
    rr_index    0                                  # 0-based; same key/vel = RR group
    rr_mode     ""                                 # ""/cycle | random | no-repeat-random
    gain_db     4.990
    tune_cents  0.000
    mic         Direct                             # references a mics.id
    articulation main                              # references an articulations.id
    trigger_mode ""                                # ""/attack | release | one-shot |
                                                   #   pedal-down | pedal-up | cc
    trigger_cc   0                                 # e.g. 64 for sustain-pedal zones
  }
  # …one record per sample × mic × RR × velocity band…
)
```

Multi-mic: many zones share `(key, vel, rr, articulation)` and differ only by
`mic` — the engine fires one per active mic. Releases: `trigger_mode release`.
Pedal: `trigger_mode pedal-down`/`pedal-up`, `trigger_cc 64`.

## Convention-mode (legacy) — filename parsing

No `zones`; declare `articulations` and let `parse_sample_stem` read the sample
filenames. The articulation id must equal the parsed token or the samples are
inert.

```
articulations (
  {
    id       lacrm            # must match the filename-derived articulation
    label    "LA Custom body"
    kind     @Sustain         # @Sustain | @Short | @OneShot | @Release | @Legato
    dynamics ("2" "16" "31" "47" "63" "79" "95" "111" "127")   # velocity ceilings
    rr       4                # round-robin count
    dyn_ctrl velocity
  }
  { id lacr label "release" kind @Release dynamics (...) rr 1 dyn_ctrl velocity }
)
```

**`kind` drives the default-articulation picker** (`engine/mod.rs`): it plays the
first articulation that is NOT `@Release`/`@Legato` and not a mech/pedal aux id.
Mark release combos `@Release` or the patch plays key-off noise instead of the
body. Body = `@Sustain` (held) or `@OneShot`/`@Short` (percussive, no note-off cut).

## Zoned styx from a Spectrasonics `.db`

`sc-import zonemap <db> <out>` emits a zones styx from the `.db`'s own metadata
(`zonemap::write_styx`) — authoritative, no parsing. Extended to carry `mic` +
`soundsource` from the `<LayerHitStack>` layers.
