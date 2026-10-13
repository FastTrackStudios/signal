# Project-state formats: FabFilter FFBS and Valhalla XML

How FabFilter and Valhalla plugins store state **inside a REAPER project**
(`.RPP`), and how that state maps onto FTS/Signal native blocks.

This is a different source from the existing `.ffp` preset-file importer in
`src/fabfilter/`: that reads preset files on disk, this reads live instance
state out of a project. The two converge on the same target — a Signal
`Block` with native parameters — so a whole mix can be rebuilt with FTS
plugins only.

Decoded from `02 LORD OF THE FIGHT.RPP` (PNG Worship Collective album,
REAPER 7.65/macOS-arm64, 137 tracks). Instance counts in that project:
Pro-Q 4 ×16, Pro-C 2 ×6, Pro-G ×4, Pro-C 3 ×3, Pro-R 2 ×1, Saturn 2 ×1,
ValhallaVintageVerb ×4, ValhallaRoom ×1.

---

## 1. FabFilter — `FFBS` (CLAP state)

FabFilter's CLAP plugins store state in the RPP as a `<STATE>` block of
base64 inside the `<CLAP …>` FX block:

```
<CLAP "CLAP: Pro-Q 4 (FabFilter)" com.FabFilter.Pro-Q.4 ""
  CFG 4 1273 710 ""
  <IN_PINS
  >
  <STATE
    RkZCUwEAAABYAgAA…      ← base64, wrapped
  >
>
```

All the base64 lines concatenate into **one** byte stream (unlike the VST3
three-segment framing used by Omnisphere/Valhalla — see §2).

### Layout

| offset | size | meaning |
|---|---|---|
| 0 | 4 | magic `FFBS` (ASCII) |
| 4 | 4 | u32 LE — format version (`1` throughout) |
| 8 | 4 | u32 LE — `N`, number of float params |
| 12 | 4·N | `N` × f32 LE — the parameter vector |
| 12+4N | … | metadata trailer (§1.2) |

Observed `N` per plugin:

| plugin | 4CC | N floats | trailer bytes |
|---|---|---|---|
| Pro-Q 4 | `FQ4p` | 600 | 324 |
| Pro-C 2 | `FC2p` | 46 | 59 |
| Pro-C 3 | `FC3p` | 100 | 276 |
| Pro-G | `FPGr` | 34 | 96 |
| Pro-R 2 | `FR2p` | 136 | 250 |
| Saturn 2 | `FS2p` | 951 | 75 |

> **Registry fix:** `src/fabfilter/registry.rs` lists Pro-G's signature as
> `FGat`. The shipping Pro-G writes **`FPGr`**. `FGat` is likely the older
> pre-CLAP build.

### 1.1 Pro-Q 4 parameter vector (N = 600)

```
float[0]              global header (1.0 observed)
float[1 + 23*b … ]    band b, b = 0..23  (24 bands × 23 fields = 552)
float[553 … 599]      47 globals (output gain, analyzer, mode, …)
```

Band field map (index within the 23-float band record):

| i | field | encoding |
|---|---|---|
| 0 | `used` | 1.0 = slot in use |
| 1 | `freq` | **log2(Hz)** — `hz = 2^v` (7.589 → 192.5 Hz) |
| 2 | `gain` | dB, signed |
| 3 | `q` | Q factor |
| 4 | `shape` | filter type 0–12 (§1.3) |
| 5 | `slope` | slope/order index |
| 6 | — | 2.0 constant in all observed instances |
| 7 | `placement` | 0 Stereo / 1 L / 2 R / 3 M / 4 S |
| 8 | `dyn_range` | dynamic-EQ range, dB (0 = static band) |
| 9 | `on` | band enabled |
| 10 | `dyn_auto` | auto threshold |
| 11 | — | 1.0 / 0.67 (band-count dependent) |
| 12 | `dyn_attack` | 0–100 |
| 13 | `dyn_release` | 0–100 |
| 16, 17 | dyn/spectral bounds | `6.64, 11.55` used-band vs `3.32, 14.29` default |
| 20 | — | 50.0 constant |

Fields 14, 15, 18, 19, 21, 22 are 0 or near-constant in this project and
are not yet pinned down.

Sanity check — first bands of instance 0 decode to:

```
band0 f=192.5Hz  gain=-1.50dB q=0.500 shape=0(Peak)
band1 f= 60.6Hz  gain=+4.35dB q=0.500 shape=0
band2 f=8233.2Hz gain=+3.80dB q=0.570 shape=0
```

Unused bands sit at the Pro-Q default `1000 Hz / 0 dB / Q 0.5 / Peak`,
which is transparent — so a translator may either skip them (recommended,
detect via the `3.32 / 14.29` marker at fields 16–17) or emit them
harmlessly.

### 1.2 Metadata trailer

After the float vector:

```
FQ4p            4CC plugin signature
03 00 00 00     u32 trailer version
0f 00 00 00     u32 length
"Default Setting"   preset name (length-prefixed, not NUL-terminated)
ff ff ff ff     
01 00 00 00
<len><folder>   preset folder ("T2 (2)", "Toms (2)", "" …)
"CuSV"          key-value section magic
<u32 count>     number of key-value pairs
  <len><KEY> <len><VALUE>   e.g. AUTHOR / "bManic", DESCRIPTION / "…"
```

This is where the human-meaningful names live. From this project:

| plugin | preset | folder | author |
|---|---|---|---|
| Pro-C 3 | `Tom High Sustain` | `T2 (2)` | bManic |
| Pro-G | `Low Tom (Adjust Filters)` | `Toms (2)` | — |
| Pro-R 2 | `Snare Mike 02` | — | bManic |
| Pro-C 2 | `A bit of Control bM` | — | — |

Pro-G additionally carries a `_PerTrackIndex` key.

### 1.3 Shape codes → FTS

Pro-Q 4's 13 filter types are **already** the exact set FTS-EQ implements
(`eq-dsp::design::FilterType`, and `eq-dsp` carries the binary-derived
`proq4_peak` / `proq4_mzt` pipelines). The shape code maps straight
through:

```
0 Peak   1 Highpass  2 Lowpass  3 Bandpass  4 Notch
5 BandPassVariant    6 FlatTilt 7 LowShelf  8 HighShelf
9 TiltShelf         10 BandShelf 11 Allpass 12 ShelfAlt
```

### 1.4 Pro-Q 4 → `NativeEq`

`signal_fx::NativeEq` is a 24-band Pro-Q-shaped engine with per-band
`used / on / freq / gain / q / shape / slope`, per-band dynamics
(`range / threshold / attack / release / auto / relative`), `placement`,
and `spectral`. The mapping is essentially field-for-field:

| FFBS band field | `NativeEq` param |
|---|---|
| `used` (0) | `b{n}_used` |
| `on` (9) | `b{n}_on` |
| `2^freq` (1) | `b{n}_freq` (Hz, range 10–30000) |
| `gain` (2) | `b{n}_gain` (dB, ±30) |
| `q` (3) | `b{n}_q` (0.025–40) |
| `shape` (4) | `b{n}_shape` |
| `slope` (5) | `b{n}_slope` (EQ_SLOPE_BASE) |
| `placement` (7) | `b{n}_placement` (EQ_PLACEMENT_BASE) |
| `dyn_range` (8) | `b{n}_dyn_range` (EQ_DYN_RANGE_BASE) |
| `dyn_auto` (10) | `b{n}_dyn_auto` |
| `dyn_attack` (12) | `b{n}_dyn_atk` |
| `dyn_release` (13) | `b{n}_dyn_rel` |

Because both sides share the Pro-Q ZPK design pipeline, this is expected
to be a **numerically exact** translation for static bands, not an
approximation.

---

## 2. Valhalla — VST3 chunk with plain XML

Valhalla plugins are VST3 and use the JUCE three-segment framing (same
shape as Omnisphere in `daw-reaper`'s `plugin_bridge`): each base64 line
group decodes **independently**, then concatenates. Inside is a plain
UTF-8 XML element with **named, already-normalized 0–1 parameters**:

```xml
<ValhallaVintageVerb pluginVersion="4.0.5" presetName="Kick Room"
  Mix="1.0" PreDelay="0.148…" Decay="0.197…" Size="0.600…"
  Attack="0.200…" BassMult="0.553…" BassXover="0.421…"
  HighShelf="0.5" HighFreq="0.5" EarlyDiffusion="1.0" LateDiffusion="1.0"
  ModRate="0.097…" ModDepth="0.515…" HighCut="0.422…" LowCut="0.026…"
  ColorMode="0.666…" ReverbMode="0.458…" mixLock="0"
  uiWidth="935" uiHeight="435"/>
```

No binary decoding needed — locate `<Valhalla…  …/>` in the decoded chunk
and read the attributes.

### 2.1 Parameter sets

**ValhallaVintageVerb 4.0.5** — `Mix, PreDelay, Decay, Size, Attack,
BassMult, BassXover, HighShelf, HighFreq, EarlyDiffusion, LateDiffusion,
ModRate, ModDepth, HighCut, LowCut, ColorMode, ReverbMode`.

**ValhallaRoom 2.0.5** — `mix, predelay, decay, HiCut, earlyLateMix,
lateSize, lateCross, lateModRate, lateModDepth, RTBassMultiply, RTXover,
RTHighMultiply, RTHighXover, earlySize, earlyCross, earlyModRate,
earlyModDepth, earlySend, diffusion, type, space, LoCut`.

Note the **case difference** between the two plugins (`Mix` vs `mix`) —
match attributes case-insensitively.

### 2.2 Enumerated params are quantized 0–1

`ReverbMode`, `ColorMode`, `type`, `space` are enum selectors encoded as
`index / (count - 1)`-style fractions. Observed values decode cleanly:

- `ColorMode` ∈ {0.333…, 0.666…, 1.0} → 3 of 4 modes (1970s/1980s/Now).
- `ReverbMode` at 1/24 steps (0.4583 = 11/24, 0.1666 = 4/24, 0.2916 = 7/24)
  → VintageVerb's mode list.
- `type` = 0.0833… = 1/12 → ValhallaRoom's second algorithm.

A translator must round to the nearest step for the plugin's mode count
rather than treating these as continuous.

### 2.3 Valhalla → `NativeReverb`

`signal_fx::NativeReverb` wraps `reverb::DualReverb` (two chains + BigSky
MX dual routing) and already exposes `decay`, `size`, `mix`, `diffusion`,
`low_end`, plus per-chain `r2_*` variants and `imp_decay_lo/hi` decay-EQ.

Direct mappings:

| Valhalla | `NativeReverb` |
|---|---|
| `Mix` / `mix` | `mix` |
| `Decay` / `decay` | `decay` |
| `Size` / `lateSize` | `size` |
| `EarlyDiffusion`+`LateDiffusion` / `diffusion` | `diffusion` |
| `BassMult` / `RTBassMultiply` | `low_end` |
| `HighCut`, `LowCut` / `HiCut`, `LoCut` | post filters |
| `ReverbMode` / `type` | `AlgorithmType` selection |

Gaps that need FTS-side work are listed in §3.

---

## 3. What the field maps actually came from

Not reverse-engineered by inspection alone — three sources, which agree:

1. **Text presets.** Pro-Q 4, Pro-C 3 and Pro-R 2 ship their `.ffp` presets
   in the **text** INI format, and the `[Parameters]` section lists every
   parameter by name *in the same order as the binary float vector*. That is
   the field map, for free.
2. **The plugin itself.** Queried through `signal-plugin-host`, each plugin
   reports its parameters with ranges and display text.
3. **Real project instances**, checked index by index against both.

Two things source 2 settled that guessing had got wrong:

- Pro-R 2's `Space` is the reverb **time** (0.5 displays as "2.500 sec").
- Pro-R 2's `Predelay` is a **normalized** 0–1 control, not seconds
  (0.0645 displays as "0.645 ms"). No obvious curve reproduces that, so
  `pror2::ProR2::predelay_ms` returns `None` rather than a wrong number,
  pending calibration through the render bridge.

### Corrections to `registry.rs`

| plugin | registry says | actually |
|---|---|---|
| Pro-R 2 | binary, `FRvb` | **text**, `FR2p` |
| Pro-R (1) | `FRvb` | **`FPRr`**, binary, version 4, 85 floats |
| Pro-G | `FGat` | **`FPGr`** |

Pro-R 1 presets live *inside* the Pro-R 2 preset folder (162 of 274) and are
a different, unmapped layout — there are no text Pro-R 1 presets to name
those 85 fields from. `pror2::decode` rejects them explicitly.

### Pro-R 2 layout (136 floats)

```
0..18    globals: Space, Decay Rate, Distance, Brightness, Style, Character,
         Thickness, Stereo Width, Ducking, Mix(%), Lock Mix, Freeze,
         Auto Gate(ms), …, Predelay(normalized), …
19..60   Decay EQ  — 6 bands × 7 (Used, Enabled, Frequency, Rate, Q, Shape,
                     Speakers).  `Rate` is a decay MULTIPLIER, not a gain.
61..114  Post EQ   — 6 bands × 9 (…, Gain, …, Slope, Stereo Placement, …)
115..135 tilts, surround levels, I/O, bypass, analyzer
```

### Pro-Q 4 band record (23 floats, ×24, from float 0)

```
0 Used            1 Enabled       2 Frequency(log2 Hz)  3 Gain(dB)
4 Q               5 Shape(0-12)   6 Slope               7 Stereo Placement
8 Speakers        9 Dynamic Range 10 Dynamics Enabled  11 Dynamics Auto
12 Threshold     13 Attack       14 Release            15 Ext Side Chain
16 SC Filtering  17 SC Low Freq   18 SC High Freq       19 SC Audition
20 Spectral Enabled  21 Spectral Density  22 Solo
```

Globals begin at float 552. **There is no leading header float** — `Band 1
Used` is index 0. Two traps: `Stereo Placement` (7) is *not* `Speakers` (8),
and Pro-Q's placement numbering is Left, Right, **Stereo**, Mid, Side — so
its default of 2 must be translated, not passed through, or every band lands
hard right. `Dynamics Enabled` is set even on untouched bands, so dynamic-EQ
translation must key off `Dynamic Range`, not that flag.

---

## 4. Valhalla algorithm menus — measured, not inferred

Read off the shipping plugins by sweeping the selector and reading back the
display text (`signal-analyzer`'s `reverb_match --enumerate <param> --slots N`).

VintageVerb `ReverbMode`, at exact 24ths (slots 0 and 1 share a label, and
23–24 wrap back to the first):

```
 0,1 Concert Hall   2 Plate            3 Room             4 Chamber
 5 Random Space     6 Chorus Space     7 Ambience         8 Bright Hall
 9 Sanctuary       10 Dirty Hall      11 Dirty Plate     12 Smooth Plate
13 Smooth Room     14 Smooth Random   15 Nonlin          16 Chaotic Chamber
17 Chaotic Hall    18 Chaotic Neutral 19 Cathedral       20 Palace
21 Chamber1979     22 Hall1984
```

ValhallaRoom `type`, at exact 12ths:

```
 0,1 Large Room     2 Medium Room      3 Bright Room      4 Large Chamber
 5 Dark Room        6 Dark Chamber     7 Dark Space       8 Nostromo
 9 Narcissus       10 Sulaco          11 LV-426          12 Dense Room
```

These line up exactly with the factory preset categories: the `Cathedral`
folder is entirely mode 19, `Sanctuary` entirely mode 9, `Ambiences`
entirely mode 7, and `Eric Beam` + `Palace` are all mode 20.

### Chamber and Cathedral were never missing

An earlier read of this said FTS needed new Chamber and Cathedral
algorithms. That was wrong. `reverb-dsp` already ships
`algorithms/room_chamber.rs` and `algorithms/hall_cathedral.rs` as
**variants**: `Room` variant 1 and `Hall` variant 1.

What was actually missing was a way to *select* them — `NativeReverb` exposed
`algorithm`, `size_sel` and `voice`, but never called `ReverbChain::set_variant`,
so those two engines were unreachable from the parameter surface. Fixed by
adding a `variant` parameter (id 58, mirrored as `r2_variant`).

The one genuine algorithm gap left is **Palace** — VintageVerb's largest
cluster at 50 of 214 factory presets. It currently maps to `Hall` variant 2
(`hall_arena`), the nearest large space, not the same one.

**Ordering is load-bearing**: `algorithm` and `variant` both rebuild the
reverb chain, so they must be written *before* any value parameter. Emitting
them last silently reverted everything else to defaults.

---

## 5. Measured parity — what the render bridge found

`signal-analyzer`'s `reverb_match` renders an impulse through the hosted
reference and through `NativeReverb` with the translated parameters, then
compares decay per octave band. Settling the plugin with silent warm-up
blocks before the impulse is required: a reverb clears its delay lines when
it sees an algorithm change, and firing the impulse in that same block gets
it swallowed — which reads as a silent reference and looks exactly like a
translation failure.

Against VintageVerb factory presets:

| preset | mode → FTS | reference RT60 | ours | verdict |
|---|---|---|---|---|
| A Plate | Smooth Plate → Plate | 1.97 s | 1.98 s | matches broadband |
| 84 Small Drums | Hall1984 → Hall | 0.80 s | 1.59 s | **~2.0× long**, uniform across all 8 bands |
| 79 Acoustic Chamber | Chamber1979 → Room/chamber | 2.47 s | ~0.09 s | **~25× short** |

So the `decay` parameter does **not** mean the same reverberation time in
each algorithm. Plate is calibrated; Hall is out by a clean factor of two;
Room's chamber variant is out by more than an order of magnitude. A
per-algorithm decay calibration curve is the first real DSP task this
surfaced.

Even where broadband decay matches, the **frequency-dependent** decay does
not. On "A Plate", per-band ratios ran 1.37–1.46 at 62–250 Hz against
0.96–0.98 at 4–8 kHz: our low end rings ~40% too long because Valhalla's
`BassMult`/`BassXover` shorten the low tail and `NativeReverb` has no
equivalent.

---

## 6. Decay calibration — what was wrong and what was fixed

Sweeping every engine's `decay` control and measuring the RT60 it actually
produces (`signal-analyzer`'s `decay_calibration` example) showed the control
meant something different in each one — and that Room was simply broken.

**Before**

| engine | RT60 at decay 0.05 → 0.95 |
|---|---|
| Room / medium | not measurable below 0.7, max **1.69 s** |
| Room / chamber | stuck at **0.16–0.18 s** across the whole range |
| Room / studio | 0.10 → 0.34 s |
| Hall / concert | 0.71 → 28.9 s |
| Plate / dattorro | 1.22 → 24.3 s |

Hall already used an exact Jot per-line T60 model (`t60 = 0.5·60^decay`, fed
to `Fdn::set_t60`). Room set a raw **feedback gain** (`0.3 + decay·0.65`)
instead. A gain is not a time: it caps the reachable tail at whatever the gain
range allows, and it cannot express a target RT60 at all. A real chamber rings
for ~2.5 s; ours could not exceed 0.18 s.

**Changes made** (`reverb-dsp`)

1. `algorithm::decay_to_t60(decay, min_s, max_s)` — one logarithmic mapping
   every engine routes through, plus `t60_to_decay` as its inverse.
2. Room, Room/Chamber and Room/Studio converted from a feedback gain to
   `Fdn::set_t60`, with ranges 0.15–4 s, 0.25–6 s and 0.08–2 s. They also gained
   the Decay Rate EQ (`set_decay_curve`) they never had.
3. The Hall family refactored onto the same helper — its inline
   `base·ratio^decay` was already this exact form, so behaviour is unchanged
   and the ranges now live in one table (`AlgorithmType::t60_range`).
4. `t60_shelf_targets` reworked: `t60` is now the **midband** time and the
   low/high decay multipliers tilt *around* it, normalized by their geometric
   mean. Previously a 0.5× low multiplier halved the entire measured RT60 —
   asking for 2.5 s while also taming the lows silently gave 1.36 s. Damping
   is deliberately left un-normalized, because absorption really does shorten
   a tail.
5. `ReverbChain::set_decay_seconds` + a `decay_time` parameter on
   `NativeReverb` (id 59). `decay` spans a different number of seconds in
   every engine, so only this can carry "this preset rings for N seconds".
6. A `variant` parameter (id 58) — see §4.

**After**

Every engine with a time model now hits its requested T60 essentially exactly
(measured vs. the range formula, at decay 0.5):

| engine | range | requested | measured |
|---|---|---|---|
| Room / medium | 0.15–4 s | 0.775 s | 0.77 s |
| Room / chamber | 0.25–6 s | 1.22 s | 1.28 s |
| Room / studio | 0.08–2 s | 0.40 s | 0.40 s |
| Hall / concert | 0.5–30 s | 3.873 s | 3.83 s |

Hall previously measured 4.48 s against the same 3.873 s formula. Two further
bugs, both surfaced by giving Room a long tail for the first time:

7. **`Fdn::set_t60` ignored the in-loop allpass.** Per-pass attenuation was
   computed from the delay length alone, but an allpass inside the feedback
   path lengthens every recirculation. Tails ran long — ~1.16× for Hall's 0.6
   coefficient. Fixed by using `delay_samples + loop_ap_len`, and by
   configuring the allpass *before* `set_t60` in each engine.
8. **Room and Room/Chamber had no in-loop diffusion at all** — no
   `set_loop_allpass`, no `set_rotation` (and rotation depth was
   `modulation * 0.2`, so with modulation at its 0 default the tail was never
   animated). Harmless while the tail was capped near 0.18 s; once a chamber
   could sustain for seconds it rang on isolated modes, 36 dB above their
   neighbours. Fixed with in-loop allpasses, a small rotation floor, and —
   the real cause — **longer chamber delay lines**: 251–811 samples scaled by
   a 0.25 default size gave far too few, too widely spaced modes to support a
   multi-second decay. Now 787–2411, roughly half a hall's.

### Closed-loop tuning

`reverb_match --tune` measures the reference's real RT60, asks our engine for
exactly that, and iterates on the residual. The translated `decay_time` is
only an estimate — it comes from the reference control's *displayed* time,
which ignores how `Size` scales the space — so measuring beats predicting.

Against VintageVerb factory presets, tuned:

| preset | mode → FTS | reference | ours | per-band ratios | decay |
|---|---|---|---|---|---|
| 84 Small Drums | Hall1984 → Hall | 0.800 s | 0.801 s | 0.93–1.01 | **pass** |
| 79 Acoustic Chamber | Chamber1979 → Room/chamber | 2.466 s | 2.479 s | 0.89–1.16, 8 k at 1.60 | fail |
| A Plate | Smooth Plate → Plate | 1.979 s | 1.952 s | 0.94–1.35 | fail |

The chamber case ran 0.02–0.08 before any of this work.

### What is still off

- **The decay tilt, not its length.** Broadband decay now matches to ~1%; what
  fails is the *shape*. Our chamber tail stays bright where the reference
  darkens (8 kHz ratio 1.60), and our plate tail stays full in the lows where
  the reference thins (250 Hz ratio 1.35). Both are gap 2 below — we have no
  general frequency-dependent decay control, so we can match a tail's length
  but not its colour over time.
- **Wet level.** Our output sits ~19 dB hotter than the reference on the same
  preset, so the loudness criterion fails even where decay matches. A wet-gain
  normalization question, independent of decay.
- **Spring / vintage is broken**, pre-existing and unrelated: 3.80 s at decay
  0.05, 30.6 s at 0.10, unmeasurable above.

### One existing test was changed

`chain::tests::multi_band_decay_changes_low_energy` drove a 100 Hz sine for
the *whole* buffer and read its last 100 ms, with a comment saying it measured
the tail "after input stops contributing" — but the input never stopped. It
was measuring steady-state LF gain, which the shelf's tonal correction moves
in the opposite direction to decay, and the two cases sat 2% apart either side
of zero. It now drives for 0.5 s of a 3 s buffer and reads the last second, so
it measures an actual decaying tail. The property it asserts still holds and
is directly observable: taming the lows takes the 62 Hz RT60 from 18.5 s to
9.5 s.

---

## 7. Frequency-dependent decay

The tail's *colour over time* — a space whose lows ring twice as long as its
mids is a different room, however well the broadband RT60 matches.

### The DSP was already there

`reverb-dsp` ships a six-band **Decay Rate EQ** (`DecayBand`, `set_decay_curve`):
Bell / Low Shelf / High Shelf curves of decay-**time** multipliers, 0.25×–4×,
realized as per-line biquads inside the FDN feedback path. Every Hall and Room
engine feeds it. Nothing exposed it, so it had never been usable.

Now exposed as `NativeReverb` parameters `dband{1..6}_{shape,freq,rate,q}`
(ids 60–83, mirrored as `r2_*`).

### Valhalla's tone controls are NOT decay multipliers

Measured off the plugin: `BassMult` runs 0.25×–4.00× and `BassXover`
100 Hz–10 kHz; `HighShelf` is linear −24 dB→0 dB and `HighFreq`
100 Hz–20 kHz. `BassMult`'s range is *exactly* a `DecayBand` rate, which makes
a direct mapping look obvious.

It is wrong. On "79 Acoustic Chamber" — `BassMult` 0.0 ("0.25 X") and
`HighShelf` 0.0 (−24 dB) — the reference still decays for 2.47 s, with
measured per-band ratios of only ~0.73 low and ~0.6 high against its own
midband. Translating the displayed multipliers literally gave two stacked
quarter-rate shelves and collapsed our tail from 2.48 s to 0.46 s.

So these controls interact with Valhalla's own damping and geometry in ways
the displayed number does not capture. `signal-import` deliberately emits **no**
decay bands from them (there is a test guarding that decision), and keeps the
measured curves for when a mapping can be grounded.

### Fitting the bands from measurement instead

`reverb_match --tune` now runs two nested loops: an inner one that sets overall
length from the reference's measured RT60, and an outer one that reads the
reference's own per-band ratios and bends our Decay Rate EQ to match. No model
of what the source controls mean is required.

| preset | before | after |
|---|---|---|
| 84 Small Drums (Hall) | 0.053 | 0.053 — **passes**, correctly finds nothing to fix |
| A Plate | 0.506 | 0.305 (converging: 0.506 → 0.354 → 0.305) |
| 79 Acoustic Chamber | 1.361 | 1.361 — no improvement |

### The bug: the curve was being applied twice

A shelf that should have shaped one end of the spectrum shortened the whole
tail. The FDN was innocent — a bare `Fdn` with `set_t60` + `set_decay_curve`
localizes correctly, with or without an in-loop allpass (both now tests). The
fault was one level up.

`ReverbChain::effective_params` collapsed the Decay Rate EQ onto the legacy
`low_decay_mult` / `high_decay_mult` pair for **every algorithm except Hall**,
on the standing assumption that only Hall realized the curve in its FDN. That
assumption went stale the moment `set_decay_curve` was wired into the Room
engines: Room then got the curve twice, once per-frequency and once as a
broadband multiplier on the T60 shelf — and the broadband half dominated.

Fixed by making it a property of the engine rather than a hardcoded name at
the call site (`AlgorithmType::realizes_decay_curve`), so wiring another
engine up means changing one place instead of silently double-applying.

Localization after the fix, chamber, burst decay in seconds:

| band setting | 125 Hz | 1 kHz | 4 kHz |
|---|---|---|---|
| flat | 2.08 | 2.35 | 2.23 |
| low shelf 300 Hz 0.5× | **1.31** | 2.33 | 2.23 |
| high shelf 3 kHz 0.5× | 2.07 | 2.34 | **1.43** |
| bell 100 Hz 0.5× | **1.31** | 2.27 | 2.23 |
| low shelf 20 Hz / high shelf 18 kHz | unchanged | unchanged | unchanged |

### Rate range widened

Fitting a chamber drove both shelves hard against the 0.25× floor and still
could not darken the tail enough. That floor was Pro-R 2's product limit, not
a property of our engine, so `DECAY_RATE_MIN` is now 0.1. Cuts are the safe
direction — `set_decay_curve`'s loop-runaway guard exists to bound boosts.

### Results

`reverb_match --tune`, worst per-band ratio error, VintageVerb factory presets:

| preset | before | after |
|---|---|---|
| 84 Small Drums (Hall) | 0.053 | 0.053 — **passes** |
| A Plate | 0.506 | 0.356 |
| 79 Acoustic Chamber | 1.361 | **0.252** |
| 300 Large Hall | — | 0.224 |

The chamber converges monotonically (1.361 → 0.959 → 0.565 → 0.314 → 0.261 →
0.252) instead of diverging.

### Fitting six bands instead of two shelves

Two shelves at fixed corners could not represent a reference whose decay
*arches* — the chamber peaks at 1–2 kHz and falls away both sides — so the fit
stalled around 0.25 with both shelves pinned at their limit. The tuner now
places all six bands across the measured octaves: a shelf at each end to catch
everything beyond the outermost centre, and bells at 250 / 500 / 1 k / 2 k,
each fitted from the octaves it is responsible for.

Under-relaxing the per-round step was tried and is worse — it merely converges
slower than the round budget allows (the chamber went 0.413 → 0.494, and a
passing room 0.002 → 0.074). The fit does ring a little; keeping the best round
handles that.

The rate floor also moved: fitting a chamber drove shelves hard against the
0.25× limit, which was Pro-R 2's *product* range, not a property of our engine.
`DECAY_RATE_MIN` is now 0.1. Cuts are the safe direction — the loop-runaway
guard exists to bound boosts.

### Two more mapping errors this exposed

**High Cut was mapped to `damping`.** `damping` models absorption: it shortens
high-frequency decay, by up to 6.7× at the limit. Valhalla's High Cut is a
filter on the tail and does *not* shorten decay — "300 Large Hall" measures
2.45 s at 2 kHz and 2.17 s at 8 kHz, essentially flat — yet our mapping made
the top decay so much faster that no `decay_time` below freeze could reach the
reference. It now maps to `tone`, which colours without shortening. Frequency-
dependent decay has a proper home in the Decay Rate EQ now, so nothing is lost.

**"300 Large Hall" is not a hall.** Its `ReverbMode` is slot 5, *Random Space*,
which mapped to Cloud — and Cloud has no decay-time model, so `decay_time` was
a silent no-op and the tuner spun eight rounds saturating at its 60 s ceiling.
That is what motivated the next section.

---

## 8. A Random algorithm

Valhalla's Random Space / Smooth Random / Chaotic family — about 38 factory
presets — had nowhere to land. Folding them onto Cloud meant they could not
even be tuned to the right length.

`reverb-dsp/src/algorithms/random.rs` adds one. What separates it from a hall
is not size or brightness but **motion**: instead of the periodic chorus
detuning a modulated allpass gives, every delay line random-walks its read
position independently (`Fdn::set_jitter`, reverbsc-style), so the tail never
settles into a repeating pattern. Jitter never reaches zero — the engine is
defined by its motion. Diffuse onset (no discrete early reflections), in-loop
allpasses for density, slow feedback-mix rotation on top of the drift, and the
same exact Jot T60 decay Hall and Room use, over a 0.4–25 s range.

Mode routing follows a rule: a mode name carries both a space ("Chamber",
"Hall") and a character ("Chaotic", "Smooth"), and **where it names a space,
the space wins**. So Chaotic Chamber is a chamber and Chaotic Hall is a hall;
only the modes with no space in the name — Random Space, Smooth Random,
Chaotic Neutral, Chorus Space, Dark Space, and Room's Alien-themed modes —
become Random.

## 9. Getting presets to pass

Starting from 3 of 11 sampled factory presets passing the 0.15 per-band bar,
each failure turned out to have a distinct cause. In order of what they
unblocked:

**One decay band per measured octave** (`DECAY_BANDS` 6 → 8). Six bands meant
the outermost two octaves shared a shelf, so an error at 62 Hz could only be
corrected by also moving 125 Hz. The fit stalled around 0.25 with shelves
pinned at their limits.

**A shallower fallback fit** (`DecayFit::T10`, −5 → −15 dB, ×6). A 0.3 s tail
never presents a straight 20 dB of decay, so `reverb_time` returned `None` and
the tuner skipped entirely — several perfectly good short reverbs were simply
declining to be measured. `reverb_time_best_effort` tries T20 then T10, and
still refuses a signal that is not a decay.

**Measured, ordered T60 floors.** A first pass set floors from what the
analyzer could still fit, which put Room's floor *above* Hall's — a room
ringing longer than a hall, which `dual::tests::split_isolates_channels`
rightly rejected. Floors are now ordered by the size of the space.

**Decay-aware engine selection.** The single biggest win. `algorithm_and_variant`
mapped by mode name alone, but a mode names a *character*, not a length, and
Valhalla's library is full of big names on short settings: "PALACE-1982 Room
Mics" wants a 0.29 s tail. Routed to `hall_arena` (floor ~0.8 s) the request
clamped and the render had no fittable decay at all. `signal-import` now reads
`AlgorithmType::t60_range` and falls back to the Room family when the mapped
engine cannot ring that briefly. That preset went from **1.199 (the worst
case) to 0.032**.

**Three length targets, measured the way they are aimed.** The length and the
bands are not independent — whatever length is chosen, the Decay Rate EQ must
lift or cut every band the rest of the way. Aiming at broadband RT60 left that
work lopsided on tilted references. The tuner now tries the reference's
broadband RT60, the geometric mean of its bands, and its *longest* band (which
makes every correction a cut — cuts are the cheap direction, since boosts get
scaled back to keep the feedback loop stable), and keeps whichever fits best.
Each is compared against our own decay measured the same way; mixing them made
the loop chase a number it was not steering.

**The in-loop DC blocker was eating the bottom octave.** A 10 Hz corner costs
only 0.11 dB at 62 Hz, but it sits *inside* the feedback loop, and a 3 s tail
on ~10 ms delay lines recirculates nearly 300 times — some 30 dB of extra
attenuation on the bottom octave. It showed as our 62 Hz band decaying
*shorter* than 125 Hz even with the Decay Rate EQ boosting it as hard as it
could. Now 3 Hz (~5 dB over the same tail), which still blocks the subsonic
offset long feedback paths accumulate.

Under-relaxing the fit was tried and reverted (it converges slower than the
round budget, not more stably), and the length step is now bounded to 8× the
target either way, because on a reference with one measurable octave the ratio
step walked the request into the tens of seconds.

### Result

| preset | mode → FTS | before | now |
|---|---|---|---|
| A Plate | Smooth Plate → Plate | 0.506 | **0.000** |
| SRand-Drum Width | Smooth Random → Random | 0.654 | **0.000** |
| Mueller's Volksbad | — | 0.025 | **0.000** |
| Small RHall | — | 0.216 | **0.001** |
| CH-Bounce Snare | — | 0.392 | **0.005** |
| 300 Large Hall | Random Space → Random | 0.224 | **0.005** |
| Long Dark 70s Snare Room | — | 0.189 | **0.030** |
| PALACE-1982 Room Mics | Palace → Room/studio | 1.199 | **0.032** |
| DH-Beastly Verb | — | 0.297 | 0.179 |
| Small Drum Room | — | 0.206 | 0.206 |
| 79 Vocal Chamber | Chamber1979 → Room/chamber | 0.639 | 0.448 |

**8 of 11 pass**, up from 3, and the ones that pass do so with room to spare.

### The three that do not, and why

They are stuck at identical values across 20 tuning rounds — hard limits, not
slow convergence.

1. **79 Vocal Chamber (0.448)** — the reference is extraordinarily bass-tilted:
   5.25 s at 62 Hz against 3.48 s at 1 kHz, with everything above 2 kHz too
   short to measure. Our fit pins band 1 at the 4.0× ceiling and band 8 at the
   0.1× floor simultaneously. The binding constraint is the **boost** ceiling:
   `set_decay_curve` scales boosts back to keep ≥5 % margin on the loop's base
   attenuation, so a curve needing large boosts cannot be realized however the
   length is chosen. Raising the chamber's T60 ceiling to 12 s did not help,
   which points at the boost guard rather than the range.
2. **Small Drum Room (0.206)** — a 0.171 s reference with exactly **one**
   measurable octave. There is almost nothing to fit, and a single band 21 %
   out is the whole error. This is at the floor of what the decay metric can
   resolve, not a reverb that sounds wrong.
3. **DH-Beastly Verb (0.179)** — marginally over. It regressed slightly (0.121
   → 0.179) when the DC-blocker corner was lowered; that change is right on the
   physics and helps the chamber substantially, so it was kept.

---

## 10. The sweep harness, and keeping the results

`signal-analyzer` ships two examples:

- **`reverb_match`** — one preset: render the reference, tune ours to it,
  report per-band decay.
- **`preset_sweep`** — a whole library, N-way parallel, with a summary and a
  worst-offenders list.

Each preset runs as its own **process**, not a thread: every run hosts a live
plugin instance and a bridged VST3 makes no promises about being driven from
several threads of one process. Verified rather than assumed — `--jobs 1` and
`--jobs 6` produce byte-identical results.

### Speed

The tuner originally ran every candidate to its round limit whatever the
outcome: three length targets × twenty rounds × six length passes is some 360
full-length renders for a match often found in three. With an early exit once
the fit is good enough, a stall detector, and caching the reference's per-band
analysis (`compare_decay_against` — the reference never changes, and
band-splitting is half the cost of a comparison), a preset went from **about
seven minutes to about seven seconds**.

### Persisting the work

Two flags, and both matter for different reasons:

- **`--save-dir`** writes each run's tuned parameters and the measurements
  that justify them, as JSON. This is not a log. The tuned parameter set *is*
  the translated preset — algorithm, variant, `decay_time`, the eight-band
  decay curve, and the tone controls — and re-deriving it costs a full
  plugin-hosted tuning pass.
- **`--reference-cache`** stores each reference render as a WAV and reuses it
  on later runs. A cached reference makes the plugin **entirely unnecessary**:
  `reverb_match` checks the cache before it tries to load anything, so tuning
  continues with no plugin present at all. It also makes a re-run
  *reproducible*, which a live render is not — the reference is re-rendered
  every time, and a plugin with its own modulation does not repeat exactly.

That second point stopped being theoretical: VintageVerb deauthorized itself
mid-session and began passing audio through dry while still reporting correct
state (Mix 100 %, Bypass Off). ValhallaRoom on the identical path was fine,
which is what isolated it to the plugin rather than the host. Every reference
render captured before that point would have been lost, because nothing was
being saved.

The per-criterion result is recorded separately (`decay_passed`,
`loudness_passed`, `all_criteria_passed`) rather than one verdict: a preset
can match the reference's decay to 8 % and still fail overall on level, and a
bare `passed: false` beside a 0.085 decay error reads as a failure the decay
numbers plainly contradict.

### Both libraries, swept and saved

421 presets translated and measured, 423 reference renders cached (~1 GB), at
`/run/media/AudioHaven/Signal/Libraries/Presets/FTS-Reverb/`.

| | VintageVerb | Room |
|---|---|---|
| presets | 212 of 214 | 209 |
| decay pass | 189 / 207 measurable (91.3 %) | 189 / 192 measurable (98.4 %) |
| median worst-band error | **0.019** | **0.019** |
| 90th percentile | 0.110 | 0.038 |
| unmeasurable | 7 | 17 |

Room was the first test of the mapping against that plugin at all — everything
before was tuned against VintageVerb — so 98 % on an unseen plugin is a
reasonable sign the approach generalizes rather than fits.

Worst cases are few and extreme: `Kick-O-Resonator` (2.24), `Vox Plate`
(1.37) on VintageVerb; `GiantCistern` (0.80), `ReleaseTheKraken` (0.63) on
Room.

### Re-tuning from cache is a different problem

The first sweep stopped each fit at 0.10 — a threshold chosen so that a sweep
*hosting a plugin per preset* stayed practical. Both medians consequently sat
just under it, which said more about where the tuner stopped than about how
well it could match.

Re-tuning from cached references removes the plugin entirely, and with it the
wine host, the bridge and the realtime deadline. What is left is arithmetic,
which parallelizes to core count: 30 concurrent jobs held the load average at
24, where six *plugin-hosted* jobs had driven it past 50 and got the sweep
killed. The accuracy target is therefore a flag (`--target-error`) rather than
a constant, and `preset_sweep` does not require `--plugin` at all when a cache
is present.

Re-tuned at 0.02, the **median error fell from 0.085 to 0.019** — the same
presets, four times closer. The pass rate barely moved (95.2 % → 94.7 %,
inside the noise of a couple of borderline cases) because the 0.15 bar was
never what was binding; the quality behind it was.

### The next real gap: wet level

`loudness_passed` is false almost everywhere — our wet output runs roughly
8 dB hot against the reference on the same preset. Decay is matched; level is
not. It is independent of everything above, now recorded in every saved
preset, and is the obvious next thing to fix.

---

## 11. The preset browser

`features/preset-browser/preset-browser` — the library and browsing model the
EQ, Reverb and Compressor editors share. Headless: no Dioxus, so the same
model drives the plugin editors, a TUI, the CLI and the tests, and
`preset-browser-ui` renders it. (This mirrors `signal-browser` / `signal-ui`,
except that pair browses the *collection* — rigs, engines, packs — while this
browses one processor's presets.)

What makes it shareable is the representation: **a preset is a
`Vec<(String, f64)>`**, which is exactly what `NativeEq::set_named` and
`NativeReverb::set_named` already take. One library type serves every
processor without knowing what any of them does.

Behaviour worth stating because it is easy to get subtly wrong:

- Search matches **all words in any order**, case-insensitively, across name,
  category, author, origin and tags — you type what you remember, not what the
  preset author typed, and without knowing which field holds it.
- The selection indexes the **library**, not the visible list, so narrowing a
  filter does not silently re-point it at a different preset.
- Stepping walks the *visible* list, skips what the filter hides, and does not
  wrap — reaching the end of a bank should feel like the end of it.
- Loading is forgiving: one truncated file costs that preset, not the bank,
  and `LoadReport` names what was skipped so a UI can say so rather than
  silently showing a short list. "Empty bank" and "missing bank" are
  distinguished.
- Each preset carries `match_error`, so a browser can be honest that a
  translated library is not uniformly faithful.

`cargo run -p preset-browser --example browse -- <dir> [query]` drives it from
a terminal, which is how the real library was checked:

```
212 presets, 22 categories
189 verified against their reference
search "plate" -> 24 matches
  A Plate                  Chamber       err 0.025  43 params
  DP-Slappy Tom Plate      Dirty Plate   err 0.039  43 params
```

---

## 6. Remaining gaps

Ordered by how much of the preset library they unblock.

1. **Per-algorithm decay calibration.** See §5. Blocks every preset that is
   not a Plate.
2. **Frequency-dependent decay.** VintageVerb `BassMult`/`BassXover`, Room's
   `RTBassMultiply`/`RTXover`/`RTHighMultiply`/`RTHighXover`, and Pro-R 2's
   entire 6-band Decay EQ are all decay multipliers per frequency band.
   `NativeReverb` has `low_end` (single, unitless) and `imp_decay_lo/hi`
   (impulse-only). A general crossover-based decay multiplier is needed.
3. **Pro-R 2 pre-delay curve** — uncalibrated; see §3.
4. **Palace** — no equivalent algorithm.
5. **Early/late split.** Room's `earlyLateMix`, `earlySize`, `earlyCross`,
   `earlySend`, and Pro-R 2's `Distance`, have no FTS surface.
6. **ColorMode** — VintageVerb's 1970s/1980s/Now bandwidth-and-quantization
   character has no FTS analogue at all.
7. **Pro-C 2 vs Pro-C 3.** Pro-C 3's 100 floats are named by its text
   presets, so its map is available; Pro-C 2's 46 are binary-only.
8. **Pro-G (34 floats).** `NativeComp` has no gate mode.
9. **Saturn 2 (951 floats).** Multiband; `NativeSaturate` is single-band.

### Non-gaps (already covered)

- **Trigger 2 → Signal sampler.** The trigger track fires a 14×7″ Ludwig
  Heirloom stainless snare from GGD "Stadium Filler". Both already exist
  natively: `…/GGD Modern and Massive 2/Presets/Stadium Filler.signalpreset`
  and `…/Packs/Snare/14x7'' Ludwig Heirloom Stainless Steel Snare.signalpack`.
  `signal-rigs-drums` loads these directly, and `mm2fx.rs` already maps MM2's
  per-piece mix recipe onto `NativeEq`/`NativeComp`/`NativeReverb`/
  `NativeSaturate`/`NativeTransient`. No new work — just wiring.
- **Chamber / Cathedral.** See §4.
