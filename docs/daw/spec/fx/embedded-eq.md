# Embedded EQ — one EQ surface inside other processors

The EQ graph (`features/fx/eq/eq-ui`) is not only the EQ plugin's editor: it
is the house EQ *surface*, embeddable in any processor that shapes a curve
over frequency. Two first consumers: the saturator's emphasis EQ and the
reverb's Post / Decay Rate EQs. Reference for the reverb behaviour:
`features/fx/reverb/spec/pror2-reference.md` (FabFilter Pro-R 2).

## The shared surface

r[fx.embed-eq.one-surface]
An embedded EQ is the same `EqGraph` component and the same `eq-dsp`
filter designs as the EQ plugin — band dots, drag/add/delete/wheel-Q
gestures per [eq-display.md](eq-display.md) and [controls.md](controls.md),
band parameters as host parameters. No processor grows its own band-curve
editor; a gesture learned in the EQ works in every embed.

r[fx.embed-eq.band-params]
An embedded EQ's bands are host parameters with stable, prefixed string ids
(`emph_b{n}_freq` …), appended after the owning plugin's existing params —
never renumbering anything (`fx.stack.params` discipline applies).

## Saturator — emphasis / de-emphasis EQ

r[fx.sat.emphasis]
The saturator carries a 6-band **emphasis EQ**: the EQ curve is applied to
the signal *before* the saturation stage and its exact **mirror** (every
band's gain negated) *after* it. The pair is net-flat for a linear signal —
the EQ chooses *what distorts*, not what the output sounds like (the
generalization of the existing ±tilt pair in `saturate-dsp`). Boost 3 kHz
+6 dB → 3 kHz drives the stage 6 dB harder and comes back down 6 dB after.

r[fx.sat.emphasis.mirror]
The de-emphasis stage is derived from the emphasis bands automatically
(same freq/Q/shape, negated gain) — it is not independently editable, and
the two stay mirrored under every gesture and automation. Cut/notch/pass
shapes with no gain axis are excluded from the emphasis EQ (a low-cut has
no inverse); the embed offers Bell and Shelf shapes only.

r[fx.sat.emphasis.makeup]
Auto-makeup accounts for the emphasis: the level the shaper sees is the
emphasized level, so the makeup calibration includes the emphasis curve's
pink-weighted RMS gain exactly as it includes the tilt's
(`fx.gain-comp.saturate`).

r[fx.sat.emphasis.display]
The saturator's editor shows the emphasis EQ on the shared graph surface,
with the curve drawn in the *drive* colour (it is a drive control, not a
tone control), and the graph's dB scale labelled as drive emphasis.

## Reverb — Post EQ and Decay Rate EQ

r[fx.reverb.post-eq]
The reverb carries a 6-band **Post EQ** on the wet path only (Bell,
Low/High Shelf, Low/High Cut), equalizing the final reverb sound. The dry
path is untouched. Per `fx.gain-comp.eq`, the wet gain is automatically
compensated for the Post EQ curve, so shaping the reverb never rides the
mix.

r[fx.reverb.decay-eq]
The reverb carries a 6-band **Decay Rate EQ**: a curve of decay-time
multipliers over frequency, 25 %–400 % (0 dB-equivalent centre = 100 %),
with Bell / Shelf / Notch shapes. It generalizes the classic low/high
crossover decay: lows at 200 % with highs at 50 % is two shelf bands. The
curve reshapes the *tail* — implemented in the algorithm's feedback path
(per-frequency feedback gain g(f) = g₀^(1/rate(f))), not as a static EQ on
the output.

r[fx.reverb.eq-display]
Both curves live on ONE graph: Decay Rate in its own colour with its scale
(×0.25…×4, log) on the left, Post EQ in the standard curve colour with its
dB scale on the right; the active kind's scale highlights while a band of
that kind is being edited. Clicking/dragging the Post curve edits/creates
Post bands; the Decay curve, Decay bands. Band gestures per
[eq-display.md](eq-display.md).
