# Synth UI — architecture

The synth rig UI grows into a **full Sampler + Synthesizer** — and becomes the
shared "instrument" front-end for every sound-*generating* rig (Keys, Drums,
Orchestral, Synth); the processing-only rigs (Guitar) keep their own. Two big
surfaces: an **Edit** page (author/inspect signalpacks) and a **Control** page
(the Vital-style synth). Reference: KODA (sampler edit) + Vital (synth control,
see [[control-ui-vital-reference]]).

## Top-level shell

```
┌ Synth ─────────────────────────────────────────── [vol] [meter] ┐
│ BROWSE   PLAY   EDIT   CONTROL   MIX                              │  ← mode bar
├─────────┬────────────────────────────────────────────────────────┤
│ sidebar │  main (per-mode)                                        │
│         │                                                         │
└─────────┴────────────────────────────────────────────────────────┘
```

- **BROWSE** — preset browser (exists: the preset list), grows tags/filter/sort
  (we already put category/instrument/style tags in the packs).
- **PLAY** — perform: keyboard + MIDI monitor (the current trimmed UI).
- **EDIT** — the Sampler edit page (below). Left sidebar = Articulations /
  Variations / Groups.
- **CONTROL** — the synth editor (below). Per-layer A/B/C/D.
- **MIX** — multi-mic + layer mixer (level/pan/mute/solo per mic & per layer).

## EDIT page (Sampler) — KODA-style

Tabs: **MAPPING** · Voice Edit · Mods · Routing · Debug (start with Mapping).
Left sidebar (shared across EDIT tabs):
- **Articulations** (e.g. Sustain, Staccato) — from `ArticulationSpec`.
- **Variations** — sub-variants within an articulation.
- **Groups** — zone groups (the "Sustain" group in the KODA shot).

### MAPPING tab (FIRST DELIVERABLE)
- **Keymap grid**: X = MIDI key 0–127, Y = velocity 0–127. Each **zone** is a
  rectangle spanning its `key_min..key_max` × `vel_min..vel_max`, colored by
  articulation/mic, labelled. **Round-robins** (same key/vel, different
  `rr_index`) render as stacked/striped cells so the RR spread is visible —
  these come from name-import (`parse_sample_stem`) or explicit zones.
- **Piano** along the bottom (reuse `signal_ui::components::Piano`), aligned to
  the grid's X so key columns line up; held/selected keys light.
- **Inspector** (right): properties of the selected zone(s) — file, root_key,
  key/vel range, rr_index, gain_db, tune_cents, mic, articulation, loop
  start/end, trigger_mode. Editable later; read-only first.
- Top bar: zone/velocity mode toggles, MIDI-select, overlap handling (KODA's
  "No Overlap / Fill / …"), auto-map. Read-only first, edit later.

Data: the loaded soundsource's `LibrarySpec.zones` → a wire `SynthMapping`
(zones + mics + articulations + groups). New proto type + a backend method that
returns the mapping for the loaded preset (or a selected soundsource).

## CONTROL page (Synth) — Vital-style

- **Layer selector** A/B/C/D (extensible). Each layer = a full sub-instance:
  source stack → filters → amp → FX, with its own envelopes/LFOs/mod.
- **Oscillator/source**: sample source (with mic mixer) OR wavetable; the
  osc sub-modules (unison/harmonia/FM/ring/…) as we implement them.
- **Filters**: visual **filter-response curve** with draggable cutoff/res;
  animates as the filter env modulates it.
- **Envelopes/LFOs**: interactive **DAHDSR graph** + LFO shape editor (drag
  markers; segment power/curve) — see the Vital reference.
- **Modulation**: drag a source (Env/LFO/Wheel/Velocity/…) onto any knob →
  modulation ring + live meter; a matrix view lists routes. Backed by the
  existing mod engine (`node_render/modmatrix.rs`, source→target→depth).

## Wire model (proto additions, incremental)

- `SynthMapping { zones: Vec<SynthZone>, mics: Vec<SynthMic>, articulations,
  groups }`, `SynthZone { file, key_min/max, vel_min/max, root_key, rr_index,
  gain_db, tune_cents, mic, articulation, group, loop_start/end, trigger_mode }`
  — a faithful projection of `ZoneSpec`.
- `mapping(&self, soundsource: String) -> SynthMapping` service method.
- Later: `SynthLayer`, `SynthModRoute`, envelope/LFO shapes for CONTROL.

## Build order

1. **MAPPING tab** (read-only keymap grid + piano + inspector) ← start now,
   delegated. Needs `SynthMapping` proto + backend expose.
2. Mode shell (BROWSE/PLAY/EDIT/CONTROL/MIX) + EDIT sidebar.
3. CONTROL: layer selector + envelope graph + filter response (Vital components).
4. Modulation drag system.
5. Multi-mic mixer (MIX).
6. Promote the instrument UI to the shared front-end for Keys/Drums/Orchestral.

Rendering: web/wasm remote; interactive graphs = inline **SVG** in rsx; keep
inline-style/Blitz-safe so the same components render in the plugin later.
