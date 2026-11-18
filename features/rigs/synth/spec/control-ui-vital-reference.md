# Control UI — Vital reference notes

The synth **Control** page (the full-synthesizer UI) is modelled on Vital
(mtytel/vital, cloned to `/run/media/Development/reference-vital`, JUCE/C++ —
reference for *interaction + visual model*, not code). Everything is **visual +
interactive** — the opposite of Omnisphere's opaque knobs. Eventually every
sound-generating rig (Keys / Drums / Orchestral / Synth) runs on this one
"instrument" UI.

## Components to model (Vital → our Dioxus/SVG equivalents)

| Vital source (`src/interface/`) | What to build |
|---|---|
| `editor_components/envelope_editor.{h,cpp}` | **Interactive DAHDSR graph** — draggable markers (delay, attack, hold, decay, sustain, release); dragging *between* two markers bends that segment's **power/curve** (exponent). Hover radius ~12px, grab ~20px; ~98 line points/segment. |
| `editor_components/lfo_editor.{h,cpp}` | **LFO shape editor** — draggable breakpoints + curve, sync/rate, retrigger. |
| `editor_components/filter_response.{h,cpp}` | **Filter response curve** — live magnitude plot vs frequency, cutoff/res handles draggable on the curve itself. |
| `editor_sections/modulation_{manager,matrix}.{h,cpp}`, `editor_components/modulation_{button,meter}` | **Drag-to-modulate system** — drag a source (Env/LFO/etc.) onto any knob; the knob shows a modulation ring + live meter; depth is set by dragging the ring. A matrix view lists all routes. |
| `editor_sections/envelope_section`, `lfo_section` | Section containers wrapping the editors + their sliders. |

## The model

- **Envelope**: DAHDSR, each of attack/decay/release has a **power** (curve
  shape) in addition to time; sustain is a level. Our importer already parses
  ADSR (`env_seconds`, calibrated 0–20 s log) — extend to carry power/curve.
- **Modulation**: any source (Envelope, LFO, Mod Env, mod matrix macro, MIDI:
  Wheel/velocity/aftertouch) → any target param, with signed depth. We already
  have a mod engine (`node_render/modmatrix.rs`) + routes (source → target →
  depth). The UI is a visual layer over that: drop a source on a knob, drag the
  ring for depth, see it move live.
- **Visual filter**: show the response curve; the filter env modulating cutoff
  animates the curve in real time.

## Layering (the big idea)

Each layer **A B C D** (extensible to E F G …; start with Omnisphere's 4) is a
**full sub-instance** of the synth — its own oscillator/source stack, filters,
amp, envelopes, LFOs, FX, and routing. The Control page has a per-layer view +
a layer selector. Layers sum into the Part. This maps onto the existing
`Container` tree (Quadzone → Layer A..D) — each Layer container already holds
its own filters/amp/modulators.

## Multi-mic

Sample sources expose **multiple mic positions** (`MicSpec` in the sampler
spec — e.g. Keyscape Direct/Room/Stereo/Pickup). The Control UI needs a mic
mixer per source (level/pan/mute/solo per mic), and the Mapping page shows which
mic each zone belongs to. This is first-class, not bolted on.

## Rendering

Interactive graphs = **inline SVG** in rsx (`<path>` for curves, draggable
`<circle>`/`<rect>` handles with pointer events). Web/wasm remote can use SVG
freely; keep it Blitz-safe (inline styles) so the same components later render
in the plugin/standalone contexts.
