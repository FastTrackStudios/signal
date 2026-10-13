# Devil-Loc Deluxe (plug-in) — "Deluxe" reference

Target: the **Soundtoys Devil-Loc Deluxe** plug-in
(`/Library/Audio/Plug-Ins/VST3/Soundtoys/Devil-Loc_Deluxe.vst3`). Our
recreation is the saturate algorithm **Deluxe** (`saturate-dsp/src/deluxe.rs`).
Goal: the same algorithm with roughly the same results (not a null).
Everything here was measured black-box; tools and captures live outside git in
`/Volumes/dev-drive/assets/saturator-models/`.

Generated 2026-10-09.

## Controls as the VST3 exposes them

| Name | Range / display | Measured law |
|---|---|---|
| Crush | 0.0..10.0 | gain into the VCA, 11-point taper (dB, linear between whole steps), 0 → +33.6 dB |
| Crunch | 0.0..10.0 | gain into the curve after the detector tap, exactly 3.000 dB per unit |
| Darkness | 20000..500 Hz (log) | steady-state 4-pole low-pass *after* the curve, corner bottoming out ≈ 2.4 kHz, then a level drop (−5.5 dB at 500) |
| Mix | 0..100 | equal-power, unity dry |
| Release | Off / On | detector release τ ≈ 177 ms (Off) / 98 ms (On) |

## Structure

`x → A → ×Crush → ×VCA → [detector tap] → ×Crunch → curve → HP 9.45 Hz → Darkness → mix`

- Zero latency, deterministic; internal float32 around a DC operating point
  (IR error floor ≈ 1e-7 absolute — measure IRs with ±1e-3/2e-3 pairs).
- `A` carries the whole linear colour (+5.5 dB at 1 kHz, resonant LF shelf,
  slight HF tilt); `H2/H1²` flat 100 Hz–8 kHz → only the 9.45 Hz coupling HP
  follows the curve.
- Curve: asymmetric soft clip, rails ≈ +0.30 / −0.38, expansive at first
  (H1 rises before it falls); oversampled internally (H5 alias −72 dB).
- Detector on the stereo **sum** (L, −L does not compress), feedback.
  Integrating: a cap charged by a log-domain rectified excess of the
  (≈0.3 ms smoothed) tap level, discharged exponentially — gain reduction in
  dB ∝ cap. Gives ~10 ms overshoot after an onset, ≈15:1 static ratio
  (0.932 dB/dB), and a hold at the GR floor (≈ 49 dB) for 100–400 ms after
  loud bursts. A single impulse barely moves it (charge-rate limited).
- Darkness misleads under impulses (not a fixed filter on an isolated spike);
  steady-state multitone and pink noise agree with each other.

## Known gaps

Burst-onset envelopes (3–15 dB rms), the reference's violent pumping on pink
noise at Crush 10, Release Off's extra slow overshoot decay, and the curve's
oversampling (we run it at base rate).
