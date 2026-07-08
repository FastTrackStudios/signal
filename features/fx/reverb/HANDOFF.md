# Reverb work handoff — BigSky MX parity (worktree `fx-dsp`)

State as of 2026-07-08. Continue in the worktree
`/run/media/Development/FastTrackStudio/signal/.claude/worktrees/fx-dsp`
(branch `worktree-fx-dsp`). Authoritative behavior spec:
`features/fx/reverb/spec/bigsky-mx-reference.md` (read it first —
engines, params, gap analysis, pass order).

## Environment gotchas (will bite you if skipped)

- The worktree's root `Cargo.toml` and `xtask/Cargo.toml` carry
  **uncommitted absolute-path rewrites** for external `../` deps
  (relative paths don't resolve from `.claude/worktrees/`). NEVER commit
  or revert these two files' path lines; when committing root Cargo.toml
  changes, stage a copy built from `git show HEAD:Cargo.toml` + your new
  lines (see commit 61d5e98b's technique).
- Commit with `--no-verify` (capn hooks are broken).
- ONE cargo invocation at a time — a wedged `cargo test` holds the build
  lock and silently blocks everything (this stalled pass B for hours;
  `pgrep -f "cargo test"` before starting, kill stale ones).
- audiocore-dsp is a SHARED checkout at ../Plugins/FTS-Audiocore
  (branch latest-blitz-migration) — additive changes only.
- Main checkout is at `/run/media/Development/FastTrackStudio/signal`,
  has its own active session — do not touch it except for the final merge.

## Where things stand

Committed on `worktree-fx-dsp` (not yet merged to main):
- `5b558cbf` — **Pass A done**: `dual.rs` DualReverb (Single / Parallel /
  Series12 / Series21 / Split / SplitSwapped), per-slot `ReverbChain.pan`
  (equal-power, smoothed), wet tremolo (`trem_rate_hz`/`trem_depth`),
  `copy_params`, signal-fx NativeReverb wraps DualReverb (ids: routing 3,
  algo_b 4, decay_b 5, mix_b 6, pan_a 7, pan_b 8, trem_rate 9,
  trem_depth 10). 49 lib tests were green at commit.

Already merged to main earlier (context): all reverb quality passes
(artifact fixes, coefficient smoothing, IR metrics harness in
`tests/ir_metrics.rs`, convolution modulation `ConvolutionModParams` —
motion/LFO/duck/dual-IR-morph), Dattorro plate tap-matrix fix.

## Pass B — code COMPLETE on disk, UNCOMMITTED, verification pending

The "Impulse live params" implementation is fully written in the working
tree (agent finished the code, then its test runs kept stalling on the
build lock — the code was never the problem):

Modified: `reverb-dsp/src/{algorithm.rs, algorithms/convolution.rs,
chain.rs, lib.rs, impulse_params.rs (new), ir/engine.rs, ir/mod.rs,
ir/prepared.rs, ir/transforms.rs}`, `signal-fx/src/lib.rs`,
new test file `reverb-dsp/tests/impulse_params.rs`.

What it implements (per spec Impulse row): `ImpulseParams` on ReverbChain —
Decay % (1–100), Tail Envelope|Gate, Attack, Stretch 0.25–4× (resample),
Direction Fwd/Reverse, Feedback (wet→pre-delay recirculation, runtime DSP);
IR re-preparation off the audio thread via the IrEngine worker pattern with
last-one-wins debounce + synchronous `reprepare_now()` for tests;
defaults-reset-on-IR-load (Mix preserved); signal-fx ids 11–16
(imp_decay/imp_tail/imp_attack/imp_stretch/imp_direction/imp_feedback).

**TODO to land pass B:**
1. `cargo test -p reverb-dsp` (debug), `cargo test -p reverb-dsp --release
   --test ir_metrics`, `cargo check -p signal-fx -p reverb` — fix whatever
   fails (unverified code; expect small issues at most).
2. Confirm the bit-transparency test passes (all ImpulseParams defaults ==
   previous convolution output at 1e-12).
3. Commit (message pattern: see `git log --oneline -8`).

## Pass C — in-algorithm params (not started)

Spec gap items 5, 8, 9. One fork-agent brief exists in session history;
essentials:
- **Shimmer dual shift**: Shift1 + Shift2 independent intervals
  (−oct..+oct each), Amount (both voices' level), Feedback mode enum
  Input / Regenerative / InputPlusRegen (regen = shifter inside the loop
  → octave ladders). Use audiocore `GrainPitchShifter` or pitch-dsp
  (`pitch-dsp` is a workspace dep; WSOLA chosen for delay shimmer,
  measured via Goertzel inharmonicity — copy that test pattern).
- **Magneto**: Ping Pong on/off (taps alternate hard L/R; big width +
  center clarity). Note knob remap semantics live at the param-mapping
  layer: decay=delay time, predelay=feedback (document, don't break
  AlgorithmParams).
- **NonLinear**: Chop (amplitude mod/trem on the decay — rate + depth),
  explicit gate speed param, Late stage (Late Speed / Late Decay /
  Late Level) as separate late-reverb controls.
- Files: `algorithms/{shimmer.rs, magneto.rs, nonlinear.rs}`,
  param plumbing via `algorithm.rs` AlgorithmParams or per-algorithm
  setters (follow how conv_mod/ImpulseParams reached Convolution).
- Tests per feature (ping-pong: alternating L/R energy at tap periods;
  chop: periodic AM on decay; dual shift: two Goertzel peaks).

## Pass D — input-analysis generators (not started)

Spec gap items 4, 6, 7 — the hardest DSP:
- **Cloud Ensemble**: pitch-tracked synthetic string/pad layer blended
  into the reverb input (Cloudburst-style). pitch-dsp has PLL/tracking
  primitives; even a filtered-sawtooth ensemble driven by a pitch
  tracker + slow attack works. Keep Diffusion param independent (both
  coexist on MX).
- **Bloom Harmonics**: harmonic-overtone generator on the trail —
  pitch-dsp `PolyOctave`/POG filter-bank is the right basis (octave +
  fifth partials, level = Harmonics param).
- **Chorale**: Choir level param, Choir Voice (Tenor vs higher range =
  formant/pitch range switch), Mod = per-voice pitch/timbre
  RANDOMIZATION (more mod = more distinct singers — decorrelate the
  existing voices' vibrato + formant centers).
- All three: `// interpretation` comments where the hardware behavior
  is inferred; NaN/stability tests + audibility tests (feature on vs
  off differ measurably), keep `ir_metrics` green (these algorithms are
  exempt from some bounds — check the exemption comments).

## Pass E — voices + polish (not started)

Spec gap items 10, 11 (+13 note):
- **Voice pairs**: `voice: MX|Classic` param mapping to existing variant
  pairs where both heritages exist (plate ↔ plate_lexicon etc.); for
  Hall/Room/Spring/Shimmer treat current as one voice, counterpart =
  re-tuned variant (honest naming, no fake "classic BigSky port" claims).
- **Hall**: Mid control (mid-band cut/boost around ~1 kHz on the wet),
  Swell Rise + Swell Type (wet vs wet+dry) as Hall params (reuse the
  swell algorithm's envelope logic).
- **Size unification**: map Size params onto the existing variant system
  (Hall: Concert/Arena = variant 0/2; Room: Studio/Club; Plate sizes).
- signal-fx: expose new per-algorithm params (continue REVERB_PARAMS ids
  from 16). Cab Filter: skipped deliberately (rig-level NAM/cab exists).

## Final step — merge to main

Mirror the delay merge (commit `41d7a6f5` on main):
1. Everything committed on `worktree-fx-dsp`; full suite green
   (`cargo test -p reverb-dsp` debug + release ir_metrics, `cargo check
   --workspace`).
2. In the MAIN checkout: commit/WIP-commit any dirty overlapping files
   first (check `git -C .../signal status`), then
   `git merge worktree-fx-dsp --no-verify`. Expect `Cargo.lock` conflict
   → `git checkout --ours Cargo.lock && cargo metadata >/dev/null` →
   commit.
3. Verify on main: reverb + delay tests, `cargo check --workspace`.
4. Do NOT merge the worktree-local Cargo.toml/xtask path rewrites
   (they're uncommitted; just never stage them).

## Verification quick-reference

```
cargo test -p reverb-dsp                                  # lib + conv_mod + impulse_params
cargo test -p reverb-dsp --release --test ir_metrics      # voicing bounds (~5 s)
cargo check -p signal-fx -p reverb
cargo run -p reverb-dsp --release --example ir_smoke -- <ir.wav>   # real-IR sanity
# IR library: /run/media/AudioHaven/Signal/IR (~2400 wavs, Bricasti M7 etc.)
```
