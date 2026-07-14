# Signal Domain Model

The signal domain manages guitar/instrument rig presets — from individual FX
blocks up through complete performance profiles and setlists. This page is the
**map**; each level is specced in its own document. All requirement definitions
live in the focused specs below.

## The layers

```
Block → Module → Layer → Engine → Rig    →    Profile → Song → Setlist
└──────────── hierarchy.md ───────────┘         └── profile.md ──┘
                                                  └ song-setlist.md ┘
```

- **[hierarchy.md](hierarchy.md)** — the composition primitives (Block, Module,
  Layer, Engine, Rig) and the saved-artifact form of each (**Preset**). This is
  the *only* routing model: synth/sampler signal chains are built exclusively from
  Blocks and Modules (a Filter is a Block; Lowpass/Highpass/Bandpass/Specialty are
  its block presets).
- **[instrument-engine.md](instrument-engine.md)** — the sound-*generating* side
  (Soundsource → Filter → Amp → FX), which is a Layer expressed in Blocks/Modules.
- **[soundsource.md](soundsource.md)** / **[sampling.md](sampling.md)** — the
  generator inside a Layer and the sampled-instrument data model.
- **[profile.md](profile.md)** — Profiles (patch pools) and Stacks (footswitch
  rotations).
- **[song-setlist.md](song-setlist.md)** — Songs (sections) and Setlists, and the
  switch semantics on recall.
- **[setlist-navigation.md](setlist-navigation.md)** — runtime navigation
  strategies (full-load mute/unmute vs dynamic loading).
- **[browser.md](browser.md)** — the faceted, tag-driven browser over every
  preset kind.
- **[parameter.md](parameter.md)** / **[modulator.md](modulator.md)** /
  **[macro.md](macro.md)** — the cross-cutting control graph every Block
  parameter participates in.

## Backends

The hierarchy is backend-agnostic. Two backends realize it today: the headless
instrument engine (native/WASM/embedded) and REAPER (tracks, Container FX,
`.RTrackTemplate`, `[R]/[E]/[L]/[S]` prefixes — see
[track-roles.md](track-roles.md) and [setlist-navigation.md](setlist-navigation.md)).
The same Presets load into either.
