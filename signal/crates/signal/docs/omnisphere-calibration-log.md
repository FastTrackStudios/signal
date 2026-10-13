# Omnisphere Calibration Log

Measured constants and reverse-engineering findings from the A/B harness
(state injection → real Omnisphere 3 on voyager → render → measure).
Companion to `docs/omnisphere-compat-roadmap.md`; the probe driver is
`docs/tools/omni_sweep.py`, the injection tooling `omni_import::state` +
the `omni_state` / `load_plugin` examples.

## Calibrated constants (live in the importer)

| Quantity | Law | Evidence |
|---|---|---|
| Envelope breakpoint time `t` | absolute from note-on, **1.0 = 100 s** | release Δt 0.003 / 0.03 → 0.28 s / 2.92 s decays (−26 dB points of linear 0.3 / 3.0 s ramps, ±5%) |
| Envelope breakpoint level `l` | linear amplitude 0–1 | sustain 0.25 vs 1.0 → ×3.7 RMS |
| Envelope point flags `s` | 14 = normal, 18 = terminal | corpus + round-trip |
| Envelope curve `c` | shapes the segment **starting** at its point; 0.5 = linear; c→0 fast-start `(1−u)^k`; c→1 hold-then-drop `u^k`; `k ≈ 1 + 9.5··|1−2c|` | 5-point sweep on a determinism-verified base; terminal-point c is a no-op |
| Filter cutoff `freq` | **15 Hz × 2^(9.55·v)** | 9-point knee sweep, log-linear fit (keytracking zeroed — it skews the knee) |
| Filter algorithm `type1` | 50-slot enum at 0.02 steps, per-slot measured table (`TYPE1_TABLE`) | 8-band Goertzel fingerprint per slot; pole counts are lower bounds |
| Unison detune `udpth` | **total spread ≈ 185 cents × udpth** (linear ±2%) | spectral partial spacing at 3 points: 189/184/182 ¢/unit |
| `AENVPARAMS`/`FENVPARAMS` | derived UI state — **engine ignores them**; the `AENV`/`FENV` `<p>` breakpoint list renders | full-range attr sweeps changed nothing; breakpoint sweeps tracked exactly |

## State/format facts

- VST3 state chunk: `DAW3` + JUCE wrapper + the SynthMaster Multi XML
  (see `omni_import::state`; byte-identical round-trip).
- The engine **accepts inserted attributes** and canonicalizes (e.g. OSC
  unison attrs migrate into a `UNI` element; `unsOn`/`unsLv` appear on
  OSC). v2-era patches simply lack many attrs — rewriting an absent attr
  is a silent no-op, so probes must insert.
- Engine materializes an `LFO[9]` on round-trip (the corpus "LFO9" mod
  source) and flips `lLFOP` 0 → 999.
- `ARPSEQ2` `VEL` spans ~0..3127 in 2.8-era patches, not 0..127 (arp
  velocity import needs a scale pass).
- Engine default LFO rate = 0.4642; matrix `mute = 0` means ACTIVE.

## Mod matrix / LFO — established but unfinished

- **Factory routes RUN in our host**: muting "RV │ Romeo Leslie" route 3
  (`LFO1 → A tuneFine`, hi 0.5) audibly changes the render.
- **Rate edits on a live route take effect**: 0.4642 → 3.4 Hz,
  0.516 → 6.6 Hz (possible 2× probe ambiguity), 0.54 → 3.8, 0.55 → 8.0.
  Curve NOT fit yet — that patch has 5 other simultaneous routes
  contaminating every measurement.
- **Injecting a new route does NOT activate it**: flipping a row's
  `source` from `off` to a live source (defV/lo/hi/mute correct, verified
  surviving the engine round-trip) produces zero modulation. Activation
  state lives beyond the row attrs; `lLFOP` is suspect #1.
- Next steps: (1) depth-zero Romeo Leslie's other rows (`hi=lo=defV`) —
  do NOT touch their `source` attrs — then re-sweep for a clean rate fit;
  (2) crack activation by diffing a patch saved in the Omnisphere UI with
  N vs N+1 active routes.

## Probe-methodology traps (each cost real hours)

- Unanchored `rate=` regex also rewrites `pulserate=`; `type=` also hits
  the OSC/LFO `type` attrs (a broken OSC type renders SILENCE — the fake
  "silent filter slots"). Anchor with `(?<![a-zA-Z])`, and always print
  rewrite site counts.
- `FILTER key*` (keytracking) skews cutoff sweeps at low notes — zero it.
- Layer detune beating wobbles all amplitude measures — solo voice 1
  (zero later VOICEs' OSC `level`) for deterministic renders.
- "1975 Attempt" carries an intrinsic 3.85 Hz motion (reads 7.6 Hz at 2×)
  that survives every neutralization tried (flSpd/matrix-off/unison-off)
  — bad probe base; prefer a plain factory patch verified against an
  untouched control.
- Autocorrelation on quantized zero-crossing counts aliases (66.67 /
  33.33 / 16.67 Hz artifacts). Prefer Goertzel amplitude tracks at an
  off-peak partial probe, and always A/B against a control render.

## Host-side notes (voyager)

- Build needs `PATH=~/rustc-shim:$PATH` (fakes `rustc +nightly` for
  phon-jit stencils via RUSTC_BOOTSTRAP).
- daw's VST3 host passes a ProcessContext (tempo 120, time advancing);
  `kPlaying` was flipped on in voyager's scratch daw copy during testing —
  it made no difference to Omnisphere modulation and is NOT required.
- `load_plugin --reactivate` cycles deactivate/prepare after
  `--load-state`; also not required for state to take effect.
