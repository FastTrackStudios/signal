---
name: plugin-measurement
description: Measure a third-party audio plugin well enough to model it — parameter encodings, gain/time behaviour, and saturation curves separated from gain reduction. Use when characterising a compressor, EQ or saturator (FabFilter, UADx, SSL, Pultec, Manley) to recreate its DSP, when capturing preset libraries, or when a measurement looks wrong and you need to know which trap you hit.
---

# Measuring a plugin you intend to model

The tools live in `features/analyzer/signal-analyzer`. Measurements are
archived at `/run/media/AudioHaven/Plugin Analysis`, one directory per
plugin. Read that archive's `README.md` for the capture format.

Plugins that only exist on the Mac are measured on **voyager**
(`ssh voyager`, native CLAP + VST3, no yabridge). See
[running-on-voyager.md](running-on-voyager.md).

The long-form narrative — how the Pro-Q 4 and Pro-C work actually went — is
`crates/signal/import/spec/measuring-a-plugin.md`. This skill is the
operational version.

## The order to do things in

1. **Does it load, and is it authorised?** `load_plugin` prints the name,
   id, parameter count and latency. Then render *something* and check it is
   not silent. An unauthorised plugin answers parameter queries perfectly
   while rendering silence, so a full set of encoding tables can be
   correct and the audio worthless. Check before trusting anything.
2. **What do its parameters mean?** `plugin_params` sweeps each parameter
   and prints the plugin's own display text. Never guess units.
3. **What does it do?** `comp_capture` for gain over time × level ×
   frequency. `saturation_capture` for the nonlinearity.
4. **Cross-check against the manual.** See [manuals.md](manuals.md).

## Measuring gain: `comp_capture`

A pulsing tone — a carrier alternating between two levels — swept across 34
frequencies. Reads back the applied gain per millisecond, so one capture
holds the attack corner, the settled depth, the release shape, and how all
three move with frequency.

```sh
comp_capture --plugin <path> --presets <library dir> --out <dir>      # every factory preset
comp_capture --plugin <path> --sweep "Attack=0..1:14;Release=0..1:14" # a parameter grid
```

`--sweep` resolves axes **by name** against the plugin, so the same spec
works across versions where raw ids do not (Pro-C 2's attack is id 5, Pro-C
3's is id 7). Scenarios are labelled from the plugin's display text.

## Measuring saturation: `saturation_capture`

Three measurements, and they answer different questions:

| what | module | answers |
|---|---|---|
| steady tones at many levels | `harmonics` | THD, the harmonic series, even vs odd balance |
| exponential swept sine | `swept_sine` | linear response **and** one impulse response per harmonic order, in one pass (Farina) |
| settled-sine transfer curve | `transfer_curve` | the static input→output curve, with gain divided out |

Even and odd order are kept apart deliberately: odd (3rd, 5th) comes from
symmetric compression and reads as hardness, even (2nd, 4th) from asymmetry
and reads as warmth. Two devices with identical THD and opposite balance
sound nothing alike.

### Separating saturation from gain reduction

They live on different time scales, and that is what separates them.
`g(t)` moves at attack/release speed; saturation acts within a sample. So
drive a **settled sine**: once the envelope stops moving `g` is constant,
but the waveform still traverses `−A…+A` every cycle. Plot each output
sample against its input and the scatter collapses onto one curve. Divide
out the small-signal slope and what remains is the saturation alone, as a
unity-gain waveshaper.

Whether that decomposition is *legitimate* is a property of the device, so
measure it rather than assume it — `transfer_curve::agreement` compares
shapes across operating points.

**What the UADx units actually showed.** At a fixed drive, varying stimulus
level, the LA-2A's shape held to −30 dB — stable. Across *drive* settings
it collapsed. So the saturation is not one fixed curve but a **family of
curves indexed by the drive control**, with gain separable at each point.
That is physically right for a FET or opto unit, where the nonlinearity is
inside the gain element, and it is implementable as a 2-D waveshaper
(drive × input amplitude) with gain reduction as a separate stage.

### Never hand-pick the drive control — scan for it

`param_scan` sweeps every front-panel control and reports, per parameter:
whether it is a **selector** (and every discrete state, with the stored value
that selects each), how much it moves the **distortion**, and how much it
moves the **gain**. Then it ranks them.

This exists because hand-picking was wrong on three of sixteen units in the
first fleet run, and wrong *silently*: a control that sits after the gain
element is a clean trim, so sweeping it yields a flawless set of identical
measurements and a capture that looks completely successful. The LA-3A's
`Gain` spans 1.05x where its `Peak Reduction` spans 15,000x; the dbx 160's
`Gain` moved the distortion by exactly nothing.

It also finds axes a person would not think to look for. On the Distressor,
`Input` ranks first at 1390x — but `Output` comes second at **13.6x**, so the
output stage saturates too and is a second drive axis, not a trim. And it
enumerates the selectors properly: `Ratio` has eight states including `NUKE`,
`Audio` has six including `Dist 2`, `Dist 3` and their HP variants. Sampling
those at even steps would land twice on the same ratio and miss NUKE
entirely.

```sh
param_scan --plugin <path> --first 8 --out scan.json
```

`--first N` takes N in **enumeration order**; `--ids lo-hi` selects by
parameter **id**. They are different, and confusing them is easy: on UADx
the first eight parameters carry ids 48..55, so selecting *positions* 48-55
lands on `MIDI CC 0|36` and friends, which sweep cleanly and mean nothing.

Two traps inside the scan itself, both already fixed but worth knowing if you
write something similar. Restore each parameter to its **default** between
sweeps, not to `param_value()` — that reports where the parameter is *now*,
which after a sweep is wherever the sweep left it; doing that to `Bypass`
first left the plugin bypassed for the whole rest of the scan and produced a
perfectly clean table describing nothing. And a handful of controls top a
THD-span ranking without driving anything: `Bypass`/`Power` take the unit
from whatever it does to nothing at all, and `Mix` is worse — a dry/wet
blend passes the input through untouched at 0%, so its span comes out in the
hundreds of thousands and it beat the real drive control by four orders of
magnitude on every unit in the first scan. All are excluded from the
ranking.

### From scan to plan to capture

The scan says what each control does; `make-plan.py` decides what to measure
from it, and `run-plan.py` executes that. The split is deliberate — the plan
is judgement, so it can be regenerated or hand-edited without re-measuring.

```sh
./scripts/scan-fleet.sh                 # what does every control do
./scripts/capture-planned.sh            # plan each unit from its scan, run it
./scripts/capture-planned.sh distressor # or one unit
```

A plan has a **drive** axis (top-ranked continuous control, THD-spaced), a
**second** axis where one moves distortion by more than 3x, and every
**mode** — each discrete control with more than two states — enumerated
state by state.

Modes are measured one at a time with everything else at default, never as a
cartesian product. The product is not affordable and answers a question
nobody asked: the Distressor alone is Ratio(8) x Detector(8) x Audio(6)
against a drive axis and a level axis, some 55,000 renders. One axis at a
time is 24 jobs and under two minutes, and what a model wants from a mode is
how that mode *changes* the curve.

Both levels resume: a job whose `saturation.json` exists is skipped, so an
interrupted fleet picks up where it stopped.

**Modes are where the interesting behaviour hides.** On the Distressor:

| mode | peak THD | even | odd |
|---|---|---|---|
| Audio = Norm | 0.25% | -52.9 | -58.9 |
| Audio = Dist 2 | 0.77% | **-42.4** | -58.9 |
| Audio = Dist 3 | 0.80% | -51.8 | **-42.5** |
| Ratio = 1:1 | **19.6%** | -29.9 | -14.3 |

`Dist 2` adds second-harmonic content and `Dist 3` adds third — the names are
literal, and the measurement recovers exactly that. And `Ratio = 1:1`
distorts eighty times harder than `NUKE`, because with no gain reduction
pulling the level back the input stage is driven far harder. A capture that
only swept the drive control at default settings would show none of this.

### Driving the unit

A waveshaper is only as good as its coverage, and knob position is a poor
proxy for it. The Fairchild 660 sits under 0.03% THD for two thirds of its
Input range, then climbs to 0.5% in the last third — a linear sweep spends
most of its renders on the same nearly-clean curve.

`--drive-param <name>` probes the range and then picks settings spaced
**geometrically in THD**, so the captures span the quietest and loudest
distortion the unit can produce, which is what a shaper needs pinned.
`--drive-spacing linear` opts out.

```sh
saturation_capture --plugin <path> --out <dir> \
    --drive-param Input --drive-steps 8 --freqs 1000 --levels -12
```

Drive controls as measured (not guessed) — `Gain` is often the wrong answer:

| unit | drive | THD span |
|---|---|---|
| 1176 (all revs) | `Input` | 0.002%..1.6% |
| LA-2 / LA-2A Gray / Silver | `Gain` | 0.003%..34% |
| LA-3A | **`Peak Reduction`** (not `Gain`) | 0.0002%..3.2% |
| Fairchild 660 | `Input` | 0.0001%..7.6% |
| dbx 160 | **`Thresh`** (not `Gain`) | 0.0000%..0.46% |
| Distressor | `Input`, and `Output` as a second axis | 0.0003%..0.35% / 13.6x |
| Capitol | `L Input` (stereo unit, per-channel names) | 0.003%..15.6% |
| UA 176 | `Input` | 0.005%..14.1% |
| SSL Bus Comp 2 | none found | flat — no modelled saturation |

The Fairchild 670 is the 660 in stereo; measure the mono unit. Driving only
the 670's L channel gave a 2.2x span against the 660's 14x, which is a
measurement artefact and not a difference between the units.

## The traps, in the order they have cost the most

Every one of these was found by measurement after passing review by eye.

**Comparing against something noise-limited.** This has bitten three times
in different disguises. `agreement` normalises by the reference's residual
energy, so a reference that is barely distorting makes everything look
non-separable for reasons about measurement rather than the device. Always
compare against the *best-measured* condition, and always report the
residual magnitude next to the agreement so a reader can tell "these
differ" from "there was nothing to compare".

**A silent render is not a curve.** A unit turned fully down outputs noise
at −240 dB; dividing that by a gain of ~1e-12 manufactures a shape with
enormous apparent structure, which then wins any "most bend" comparison it
is entered into. `TransferCurve::is_usable()` rejects these — no real gain,
or a residual larger than the signal itself.

**Comparing whole curves instead of the bend.** A normalised transfer curve
is 99.9% straight line on a device at 0.1% THD, so comparing whole curves
reports better than −60 dB agreement between any two nearly-linear devices.
That is a fact about arithmetic. Compare `residual()`.

**Auto Gain, and anything else not being swept.** Pro-C's Auto Gain
defaults to *on* and adds makeup that moves with threshold and ratio, so a
threshold sweep measures compression plus automatic makeup — a curve that
looks entirely reasonable and is the wrong one. `--set "Auto Gain=0"`, and
everything pinned is recorded in the metadata. A wide default knee is the
same trap in miniature.

**Detector ripple posing as saturation.** At low frequencies a fast
detector tracks *within* the cycle, and that ripple is spectrally identical
to second-order distortion. They separate by behaviour: real saturation is
instantaneous and does not move when the time constants do.
`--release-param` measures at two release settings so the part that moves
can be told from the part that does not.

**Peak-bin amplitude reading.** A tone rarely lands on a bin centre; 1 kHz
at 48 kHz is bin 682.67 of a 32k FFT, and reading the peak bin measured a
−12 dBFS sine as −12.63 dBFS. Sum the energy in the main lobe instead.

**Bin centres as x-coordinates.** A periodic tone visits very few distinct
amplitudes — 1 kHz at 48 kHz is exactly 48 samples per cycle, so 48 values
however long the render — and none need land near a centre. Average the
actual inputs per bin; assuming centres biased a known 0.25 gain to 0.261.

**Harmonics above Nyquist.** A 9 kHz fundamental has no real 3rd harmonic
at 48 kHz. Report the noise floor there, never whatever aliased into the
bin.

**Plugins that declare more ports than the host provides.** Fixed in
`daw v0.0.5`, but the shape recurs: a plugin indexes the buffer list it was
promised. Pro-C declares a side-chain input, and against a one-port list it
read past the end — SIGSEGV on macOS, silence through yabridge. If a plugin
crashes or renders silence, check `audio_port_count()` before anything else.

**`rsync -a` preserves mtimes**, so cargo can decide a synced source is
older than its artifact and skip the rebuild — you then measure stale code.
`touch` the sources after syncing to voyager.

## Analog EQs — a flat EQ is idle, not clean

The scan probes from a unit's default state, and a passive EQ's default is
*flat*. Flat, it passes audio through its amplifier without asking anything
of it, so it measures clean and every control shows a 1.0x THD span. Nothing
in a scan can find what the unit does under load, because the saturation is
not on any one control — it is in the unit being **worked**.

    Hitsville EQ, 1 kHz, flat                    0.00000% THD
    Hitsville EQ, 7 bands + Gain at +8 dB       30.90%    THD, odd-dominant

That is the entire difference between "this plugin has no modelled
saturation" and "we measured it asleep". Both Pultecs, both Hitsville EQs
and both Massive Passives read clean until engaged.

**A saturation *mode* has to be switched on, not just fed harder.** This is
the same trap one level up, and it is easy to miss because the capture looks
complete either way. Pro-Q 4 defaults to `Character = Clean`, which is
bit-transparent under 30 dB of boost, so a level sweep that pins the band
boost but leaves Character alone measures 0.00018% at every level — a
perfectly tidy flat line describing the unit not saturating. Pin the mode on
in `engage` *and* keep it in `engage_axes`, so the level sweep runs
saturating while all the modes still get compared:

    Pro-Q 4, band 1 +30 dB, Character = Warm
      -36 dBFS -> gain +30.35 dB, THD  8.69%, odd -33.4
        0 dBFS -> gain  +1.51 dB, THD 42.23%, odd  -7.5

The output ceilings around +1.5 dBFS from -12 upward: a soft clipper, and
the shape a waveshaper is fitted to. With Character left at Clean, every one
of those rows reads 0.00018%.

`custom-plans.json` says how to put a unit under load. Entries are resolved
against the scan's real parameter list, and anything that fails to resolve is
**reported, not silently skipped** — the first version guessed prefixes
`L4-`..`L7-` for the Hitsville Mastering, which has only `L1-`..`L3-`, and
the warning is what caught it.

    engage              controls to pin so the unit is working
    engage_axes         then swept one at a time: how hard a band is
                        pushed IS the drive on a passive EQ
    engage_prefix_max   pin every control with this prefix to maximum
    engage_suffix_max   ...or suffix, e.g. every "*Gain" band
    engage_prefix_on    turn every "<prefix>...Enable" on

An engaged plan measures a **flat baseline beside** the engaged sweep, so
the difference the engage state makes is visible rather than assumed.

Match entries on the plugin's *reported* name, allowing a prefix. UADx
appends a category to several — the archive directory is `UADx Pultec
EQP-1A` while the plugin reports `UADx Pultec EQP-1A EQ` — and an exact
lookup silently found nothing for every Pultec and both Massive Passives,
so the overrides were written and never applied.

Also check `--first N` covers the real controls. The Hitsville Mastering
needs 24 and the Massive Passive 40; at 12 and 16 the later bands were never
scanned, so no override could name them.

## Analog EQs

The same tools apply, with the emphasis moved: for a Pultec or a Manley the
saturation *is* the reason to use it, so `saturation_capture` leads and the
frequency response is measured with `eq_match` / `eq_sweep`. Sweep the
drive control the same way; on a passive EQ the drive is usually the output
amplifier's gain, and the boost/cut controls change how hard it is hit.
Measure the response at several drive settings rather than one — the point
of the unit is that they interact.
