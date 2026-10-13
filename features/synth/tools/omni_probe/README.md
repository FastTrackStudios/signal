# omni_probe — calibrating Signal's Omnisphere import against the real plugin

Renders patch variants through **real Omnisphere** (daw's `omni_render`
VST3 harness, branch `omni-render`) and through **Signal**
(`cargo run -p signal-synth --release --example render_patch`), then compares
spectra. Needs numpy; the measurement data lives on the dev drive at
`$FTS_SAMPLED_ROOT/Synth/Omnisphere-Measurements/` (run the scripts from there).

- `exp.py` — any patch, `TAG.attr=f:0.5` edits; harmonic dB of the held note.
- `fexp.py` — filter probe on `init_part.prt_omn` (JP-8 saw, filter envelope
  depth zeroed): |H| per harmonic = filtered ÷ unfiltered, by FFT
  peak-picking (Omnisphere drifts slightly, so exact-frequency DFTs lie
  above ~2 kHz). `OURS=1` renders Signal too, `JSON=` dumps curves.
- `fit.py` / `fit2.py` — fit each `type1` model (topology, poles, corner
  taper, resonance curve, gain) to `types.json` + `taper.json`.
- `gen.py` — `fit2.json` → the `FILTER_MODELS` table in
  `src/omni_import/model.rs`.
- `regress.sh [type-hex…]` — Signal vs the stored Omnisphere curves, per
  model: resonance 0/0.5/0.9 and knob 0.15–0.75 (RMS dB, < 8 kHz).

Measured laws worth remembering: the effective filter setting is
`freq + 2·(freqN − 0.5)` (same for `res`); in parallel, filter 1 weighs
`1 − bal²` and filter 2 `1 − (1 − bal)²`; wavetables are 64 × 4096.
