# Bluebox — circuit model functional spec

Target behaviour: the **Black Box Analog Design HG-2 plug-in** (VST3 at
`/Library/Audio/Plug-Ins/VST3/Black Box Analog Design HG-2.vst3`). This spec
supersedes the curve-per-knob approach in `bluebox.rs` (see
`hg2-reference.md` for the black-box measurements behind that).

Clean-room status: this document describes **behaviour only** — block
diagram, equations in our own notation, physical constants and our own
closed-form fits. It contains no code, control flow or data tables from the
reference product. Implement from this document plus black-box measurement
(`assets/saturator-models/tools/`); do not consult anything else about the
reference's internals.

Written 2026-10-10. Validated by a reference prototype of exactly these
equations against HG-2 captures (1 kHz level ladders, 32 settings):
small-signal level within 0.1–0.3 dB, H2/H3 typically within 0.5–1.5 dB rms
(details in §9).

## 1. Overview

The model is a voltage-domain simulation of a two-tube amplifier with a
parallel saturation tube, between an input and an output transformer. All
tube stages work on **absolute volts** with a 320 V supply. Everything
between the interpolator and the decimator runs at the **oversampled rate**
`Fo = OS·fs`. Use OS = 4 at fs < 50 kHz, 2 at fs < 100 kHz and 1 above.
Every time constant below is physical (seconds), so it holds at any rate.

```
 in ─┬──────────────── dry (latency-aligned) ──────────────────────────────────────────────┐
     │                                                                                     │
     └► XFMR_in ► ×G_in ► ↑OS ─┬─► LP_alt ► SAT(v) ► H_sat ► ×g_sat ──(+)─► ×(−1) ─┐        │
                               └─────────────────────────────────────┘             │        │
   ┌───────────────────────────────────────────────────────────────────────────────┘        │
   ▼                                                                                        │
  LP_pent ► ×g_pent ►(+)► PLATE1 ─┬► HP_c1 ► LP_tri ► ×g_tri ► LIMIT ►(+)► PLATE2 ─┬► HP_c2 ► ↓OS ► ×G_out ► XFMR_out ─┐
                     ▲−           │                                     ▲−         │                                    │
                     └── Z_cal ◄──┘ (from 320 − Vp1)                    └─ Z_air ◄─┘ (from 320 − Vp2)                   │
                                                                                                                       ▼
                                                                 out = dry + (wet − dry)·mix·power ◄────────────────────┘
```

Polarity: the wet path is **non-inverting** overall and must be sample-aligned
with the dry path. The reference's measured impulse peaks at 0 samples
relative to the input, with +2° phase at 1 kHz at default settings.
Delay the dry path by the oversampler latency.

## 2. Controls and laws

Knob positions `u ∈ [0,1]`; the VST3 shows 0..100.

| control | law |
|---|---|
| Pentode | `g_pent = 0.72 · τ_pent(u)` (drive into stage 1) |
| Triode | `g_tri = 0.90 · τ_S,tri(u)` (drive into stage 2) |
| Saturation In / Saturation | `g_sat = −1.37778 · τ_S,sat(u)` if Sat In, else 0 |
| Saturation Frequency | Low / Flat / High, see §4 |
| Alt Tube | selects the saturation tube voicing and its input RC, see §4 |
| Air In / Air | cathode network of stage 2, see §6 |
| Cal. | Dark / Norm / Bright: cathode network of stage 1, see §5 |
| Density `d ∈ [−100,100]` | `D = 0.251189 + 3.729883·(d+100)/200`; **linear** in the knob (Density 0 → D = 2.1155, i.e. +6.5 dB of drive) |
| Input Gain `i ∈ [−12,12]` dB | `G_in = 10^(i/20) · D` |
| Output | `G_out = 0.175830 · τ_S,out(u) / min(D, 3.16228)` (make-up; Output 50 ≈ unity overall) |
| Mix | `mix = u`; `out = dry + (wet − dry)·mix·power` |
| Power | `power ∈ {0,1}`, ramped like Mix |

The Density law is the main loudness/drive coupling: drive rises with D, and
the output compensates by up to +10 dB.

### 2.1 Taper functions (our fits)

Let `R(u; a,q,r) = uᵃ / (uᵃ + q·(1−u)ʳ)`. Each taper is a blend:

`τ(u) = w·R(u; a₁,q₁,r₁) + (1−w)·R(u; a₂,q₂,r₂)`

| taper | a₁ | q₁ | r₁ | a₂ | q₂ | r₂ | w |
|---|---|---|---|---|---|---|---|
| τ_S,sat (Saturation) | 0.583097 | 48.2374 | 4.42076 | 2.82145 | 1.31445 | 6.83170 | 0.610773 |
| τ_S,tri (Triode) | 0.556154 | 50.2713 | 4.56438 | 2.79456 | 0.922311 | 6.54401 | 0.612542 |
| τ_S,out (Output) | 0.453037 | 55.7602 | 4.36390 | 2.81758 | 0.483429 | 4.43325 | 0.557663 |
| τ_pent (Pentode) | 0.673547 | 2.74453 | 7.49814 | 2.49115 | 25.9128 | 2.94973 | 0.123335 |
| τ_air (Air, see §6) | 0.628234 | 29.3662 | 3.72729 | 8.0 | 0.00558689 | 2.79296 | 0.722472 |

Properties: all are monotone, with τ(0)=0 and τ(1)=1. Their accuracy against
the reference's knob law is 0.25–0.55 dB rms. The S-tapers are up to +2.5 dB
hot in the bottom quarter of travel, where gain is −28…−43 dB. A black-box
check of the knob law in the harness is welcome; correct the tapers if
measurement disagrees.

### 2.2 Parameter smoothing

Ramp all continuous gains (G_in, g_sat, g_pent, g_tri, G_out, mix, power) and the
two cathode-network filters (§5, §6) per sample toward their targets, with a
time constant of about 50 ms. Interpolate filter coefficients, not knob
values. Alt Tube is a crossfade between the two saturation voicings, also
ramped. Changing Saturation Frequency crossfades from the old filter to the
new one over 0.15 s. The LP corners (§3, §4) may switch without ramping.

## 3. Input and output transformers (base rate fs)

Each transformer is a core-saturation multiplier, then a DC-blocking high-pass,
then an HF roll-off, then a gain. A leaky flux integrator closes the loop:

```
u   = x · (1 − φ⁸)
y   = HF( HP(u) )
φ  ← clamp( φ·(1 − 1/(fs·1 s)) + y·k_flux/fs , −1, +1 )      # flux, leak τ = 1 s
out = y · g
```

| | HP (1st order) | HF roll-off (2nd order, Q 0.5, magnitude-matched at fs) | k_flux (1/s) | g |
|---|---|---|---|---|
| input | 0.37 Hz | 97 kHz | 83.20 | −3.0876 |
| output | 0.65 Hz | 75 kHz | 5.4785 | 0.25 |

The HF roll-off is an analog response above Nyquist; at 48 kHz it costs only
a few tenths of a dB at 20 kHz. Any design that matches the analog magnitude
in band is fine. The input transformer's gain sets the voltage scale: a
0 dBFS peak arrives at the tubes as about 3.09 V × G_in. The flux term only
matters at very low frequencies and high levels.

## 4. Parallel saturation stage (Fo)

`x` is the interpolated signal.

```
s   ← s + α_alt·(x − s)                           # input RC, α = 1 − exp(−1/(τ·Fo))
t   = (1−A)·[SAT₀(s) − SAT₀(0)] + A·[SAT₁(s) − SAT₁(0)]       # A = Alt crossfade 0..1
h   = H_sat{t}
v   = −( x + g_sat · h )                          # summed with the clean signal, then inverted
```

* τ (input RC): 15.288 µs with Alt off (≈10.4 kHz), 8.9817 µs with Alt on (≈17.7 kHz).
* `H_sat`, a product of first-order sections at Fo:
  * Low: HP 3.379 Hz × LP 1711.3 Hz
  * Flat: HP 3.379 Hz
  * High: HP 5198.6 Hz
* SAT curves (12 V "starved" tube; input in volts):
  `SAT(v) = 12·( w·e^(−σ/λ) + (1−w)·(1 + σ/v₁)^(−q₁) )`, with `σ = softplus_k(v + c)`.

| | λ | c | k | v₁ | q₁ | w |
|---|---|---|---|---|---|---|
| SAT₀ (normal) | 11.7561 | 1.48646 | 3.30665 | 11.8707 | 0.951370 | 0.216030 |
| SAT₁ (Alt Tube) | 7.77859 | 1.49682 | 4.67841 | 1.95671 | 0.381980 | 0.596760 |

`softplus_k(z) = ln(1 + e^(k·z))/k`. Use a numerically safe form:
`max(z,0) + ln(1+e^(−k|z|))/k`.

## 5. Stage 1 — "Pentode" (Fo)

```
p   ← p + α_p·(v − p)        α_p from τ_p = 1.41017 µs − 1.01532 µs·τ_pent(u)    # grid HF limit, ~113…390 kHz
d1  = g_pent · p
e   = 320 − Vp1[n−1]                       # ∝ cathode current through the plate load
Vk1 = Z_cal{ e }                           # cathode voltage
Vp1 = PLATE1( d1 − Vk1 )
```

**Cathode network Z_cal** (first-order, at Fo):
`Z_cal(s) = 0.011 · (1 + s·R·C) / (1 + s·(R + 600)·C)`, with C = 33 nF and
R = 5 kΩ (Dark), 1 kΩ (Norm) or 200 Ω (Bright). Physically: a 600 Ω cathode
resistor bypassed by a series R–C, divided by the plate load.

**PLATE1** (plate volts vs grid-cathode volts on the 320 V load line):
`PLATE1(v) = Vf + (320 − Vf) / (1 + softplus_k(v + c)/v₀)^q`

| Vf | v₀ | q | c | k |
|---|---|---|---|---|
| 9.14207 | 3.76215 | 2.37280 | 0.726697 | 1.61362 |

Idle point at DC (Cal gain 0.011): Vp1 ≈ 251.6 V, Vk1 ≈ 0.75 V. Small-signal
gain is about −66 bypassed and about −38 unbypassed.

The cathode feedback uses the **previous oversampled sample's** plate voltage.
That one-sample delay is part of the target behaviour; no implicit solve is
needed or wanted.

Then a coupling capacitor: `h1[n] = c₁·(h1[n−1] + Vp1[n] − Vp1[n−1])` with
`c₁ = RC/(RC + 1/Fo)` and RC = 0.09 s.

## 6. Stage 2 — "Triode" (Fo)

```
t2  ← t2 + α_t·(h1 − t2)     α_t from τ = 7.6305 µs (≈20.9 kHz)
d2  = g_tri · t2
e   = 320 − Vp2[n−1]
Vk2 = Z_air{ e }
Vp2 = PLATE2( LIMIT(d2) − Vk2 )
```

Note the order: the grid limiter acts on the drive before the cathode voltage
is subtracted.

**Cathode network Z_air**: `Z_air(s) = 0.0603 · (1 + s·Ra·C) / (1 + s·(Ra + 2200)·C)`, with C = 10 nF.
* Air In: `Ra = 76.8 Ω + (10 kΩ − 76.8 Ω)·(1 − τ_air(u))`, a reverse pot, so turning Air up lowers Ra and lifts the top end.
* Air off: `Z_air = 0.0603` flat (unbypassed 2.2 kΩ cathode).

**LIMIT** (grid conduction):
`LIMIT(v) = v − δ + b·(δ/b)/(1 + (δ/b)ⁿ)^(1/n) + s·δ`, with `δ = softplus_k(v − a)`

| a | b | n | s | k |
|---|---|---|---|---|
| 9.24066 | 4.68081 | 0.834898 | 0.00818434 | 0.851870 |

(identity below ~5 V, soft limit towards ≈12.9 V)

**PLATE2**:
`PLATE2(v) = Vf + softplus_kf( (320 − Vf) − A₁·softplus_k(v + c₁)^g₁ − (softplus_k(v + c₂)/X₂)^g₂ )`

| Vf | kf | k | A₁ | c₁ | g₁ | X₂ | c₂ | g₂ |
|---|---|---|---|---|---|---|---|---|
| 6.02244 | 0.0965555 | 0.30475 | 1.98030 | 20.5530 | 1.52816 | 70.2491 | 74.9767 | 22.4634 |

The second current term models the hard bottoming when the grid goes
positive. Evaluate it as `exp(g₂·ln(σ/X₂))` to avoid overflow.

Idle point (Air off): Vp2 ≈ 208.9 V, Vk2 ≈ 6.70 V. Small-signal gain is about
−12 bypassed and −7 unbypassed.

Then a coupling capacitor: same form as §5 with RC = 0.18 s. Then decimate
and apply ×G_out, the output transformer and the mix.

## 7. Oversampling

Any linear-phase or minimum-latency design is acceptable if it passes to
20 kHz with ≥ 70 dB image/alias rejection. The reference uses about 32
base-rate samples of combined latency and delays the dry path to match.
**Report latency to the host** and delay the dry path so the mix is coherent.

## 8. Initialisation

Start at the settled DC operating point; otherwise the 0.09 s / 0.18 s
coupling HPs and the cathode filters produce a thump. Either:
* solve `Vp = PLATE(−Z(0)·(320 − Vp))` (with LIMIT(0) = 0 for stage 2) by
  bisection, then set the cathode-filter state to its DC steady state and the
  coupling-HP inputs to Vp; or
* run about 1 s of silence at reset.

Reset the flux states and filter states to 0.

## 9. Validation targets (black box)

Harness: `assets/saturator-models/tools/` (`sat.py` render via `daw`'s
`fx_render`, captures in `captures/hg_curves/*.json` with H1..H7 phasors at
1 kHz over −42…+12 dBFS, `hg_compare.py` for a Rust-vs-capture table,
`ab_plugin.py`, `music.py`).

Accuracy reached by a straightforward implementation of this spec (OS = 4, fs = 48 kHz):

| setting group | level err rms | H2 err rms | H3 err rms |
|---|---|---|---|
| Pentode/Triode grid, Density ±50/±100 | ≤ 0.3 dB (Triode 25: ≈ 2 dB, taper) | 0.4–1.2 dB (Triode 25: ≈ 2.5) | 0.2–0.9 dB (Triode 25: ≈ 3.5) |
| Saturation In (Flat/Low/High × Alt off/on × Sat 50/100) | ≤ 0.1 dB | 0.5–1.7 dB | 0.3–3.5 dB (Alt-on worst, near a 3rd-harmonic null) |

Acceptance for the implementation: match or beat these numbers, and keep the
compression knee where the reference has it (−0.8 dB at 0 dBFS, −4 dB at
+6 dBFS and −9.4 dB at +12 dBFS for Pentode 50, Triode 50, Density 0). Also
check Air and Cal with swept sines: shelf corners from §5/§6, and output
level vs the reference within 1 dB.
