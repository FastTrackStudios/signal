# Black Box HG-2 (plug-in) — "Bluebox" reference

Target: the **Black Box Analog Design HG-2 plug-in** (Brainworx / Plugin
Alliance, VST3 at `/Library/Audio/Plug-Ins/VST3/Black Box Analog Design HG-2.vst3`).
Our recreation is the saturate algorithm **Bluebox**. This file records what the
public manual says and what black-box measurement has shown so far.

Generated 2026-10-09.

## Sources

| Tag | Source |
|---|---|
| **[M]** | *Black Box HG-2 Plugin Manual* (Plugin Alliance), local copy `/Applications/Plugin Alliance/Black Box Analog Design HG-2/Black Box Analog Design HG-2 Manual.pdf`, pp. 4–9. |
| **[meas]** | Our captures (`/Volumes/dev-drive/assets/saturator-models/`), tools in its `tools/`. |

## 1. Signal flow per the manual

- Input transformer → **parallel Saturation circuit** (12AX7, two voicings; its
  output is *summed into* the main path and feeds the pentode) → **6U8A pentode**
  → **6U8A triode** (series; "pentode drive also pushes the triode harder") →
  output transformer → passive **Output** attenuator [M p.4, p.6–7].
- Pentode: "even order" harmonics; Triode: "third order" harmonics [M p.6–7].
- **Saturation** (0–10): how much of the parallel saturation signal is fed into
  the main path. **Saturation Frequency** Low / Flat / High selects the band fed
  *into* the saturation circuit. **Alt Tube** switches only the saturation tube
  (more aggressive voicing) [M p.6].
- **Air** (plug-in addition): continuous lift of post-saturation frequencies
  around 10 kHz ("above 10 kHz" [M p.5]); hardware had a fixed 2-step lift [M p.7].
- **Cal.** Dark / Norm / Bright: emulates the hardware's internal HF trim [M p.8].
- **Density** −100..+100 %: raises gain into both tubes equally and compensates
  output gain ("denser signal while preserving loudness") [M p.4, p.8].
- **Input Gain** −12..+12 dB trims all active tube circuits [M p.8].
- **Mix**: dry added back at the output [M p.4].

## 2. Parameters as the VST3 exposes them

| # | Name | Range / display | Notes |
|---|---|---|---|
| 0 | Power | Off/On | |
| 1 | Pentode | 0..100 | gain *into* the stage: 0 = muted (−∞), 50 = unity-ish, 100 = +15.8 dB small-signal [meas] |
| 2 | Triode | 0..100 | 0 = muted, 100 = +5.8 dB small-signal [meas] |
| 3 | Saturation In | Off/On | |
| 4 | Saturation | 0..100 | |
| 5 | Saturation Frequency | Low/Flat/High | stored 0..2 |
| 6 | Alt Tube | Off/On | no effect with Saturation In = Off [meas] |
| 7 | Air In | Off/On | |
| 8 | Air | 0..100 | |
| 9 | Cal. | Dark/Norm/Bright | stored 0..2 |
| 10 | Density | −100..+100 | |
| 11 | Input Gain | −12..+12 dB | |
| 12 | Output | 0..100 | 50 = unity, 100 = +6 dB, 0 = −∞ [meas] |
| 13 | Mix | 0..100 | 0 = exactly dry [meas] |
| 14–17 | Meter Mode, Bank, Bypass×2 | UI only | |

`fx_render` takes continuous values normalised (`Pentode~=0.5`), see
`tools/hgset.py`.

## 3. Measured so far [meas]

- Deterministic (two renders bit-identical); no injected noise. Silence → a
  deterministic sub-Hz settling transient (−106..−170 dBFS by setting).
- Defaults (Pentode 50, Triode 50, all else off): unity ±0.6 dB 20 Hz–16 kHz,
  −1.5 dB at 20 kHz, −10.5 dB at 23 kHz (anti-alias/oversampling); −85 dB
  pre-ringing → linear-phase resampling with host-compensated latency.
- Default distortion: H2 rises 6 dB/6 dB (quadratic), −38 dB at −6 dBFS;
  soft wall at ~0 dBFS (gain −0.8 dB at 0 dBFS, −4.1 dB at +6).
- Density −100 ≈ the default curve 12 dB lower in drive; +100 ≈ +6 dB with
  +1.9 dB net small-signal gain.
