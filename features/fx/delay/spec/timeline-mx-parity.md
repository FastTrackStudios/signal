# TimeLine MX — Full Parity Reference

**Source of truth:** Strymon *TimeLine MX User Manual*, REV A (06/24/2026, 106 pages) —
`features/fx/delay/spec/TimeLineMX_UserManual_RevA.pdf` (full pdftotext extraction).
**Generated:** 2026-07-27.
**Companion doc:** `timeline-mx-reference.md` (sibling file) holds the earlier condensed
behavior notes; this document is the exhaustive machine-by-machine parity checklist.

**Implementation under audit:** `features/fx/delay/delay-dsp` (crate `delay-dsp`) driven
through `DelayEngine` (`engine.rs`), exposed via `DELAY_PARAMS` ids 0–37 in
`features/fx/signal-fx/src/lib.rs`.

**Status legend** — every parameter row carries a `Status`:

| Symbol | Meaning |
|---|---|
| ✅ | Implemented and believed behaviorally equivalent |
| ⚠️ | Partial — exists but wrong range/semantics, or unreachable through the engine/param surface |
| ❌ | Missing entirely |

Verification date of all status calls: 2026-07-27 (code audit of `delay-dsp` + `signal-fx`).

---

## 1. Platform structure

### 1.1 Signal flow

- **Analog dry path**: the dry signal is never converted to digital (zero-latency dry).
  MIX blends 100% dry ↔ 100% wet. `KillDry` (global) mutes the dry path so MIX acts as
  a wet-only effect level (for parallel loops / mixer sends).
- **Wet path**: stereo in → (Looper if `Pre`) → Delay 1 / Delay 2 per the Dual routing →
  (Looper if `Post`) → ±3 dB analog Boost → out.
- **Dual-delay system**: any preset may run two machines simultaneously with routing
  Off / Parallel / Series 1▶▶2 / Series 1◀◀2 / Split 1L|2R / Split 1R|2L (§5).
- 32-bit float processing, 24-bit 96 kHz converters, +10 dBu max input.

| Platform element | Manual behavior | Status | Notes |
|---|---|---|---|
| Analog dry path + MIX 100%dry–100%wet | MIX knob; KillDry option removes dry | ✅ | `mix` = DELAY_PARAMS id 0; KillDry ≈ mix at 1.0 |
| Dual delay 1+2 with 6 routings | See §5 | ✅ | id 17 `routing`: Single/Series12/Series21/Parallel/Split/SplitSwapped |
| Boost ±3 dB per preset (analog, post) | 1+2 param, −3.0…+3.0 dB, default 0 | ❌ | Rig-level concern (preset gain staging), not a delay-dsp feature |
| Looper (5-min 48 kHz stereo, Pre/Post, level, half-speed/reverse/undo) | Full + 1-button modes | ❌ | **Rig-level**: FTS looping lives outside delay-dsp; not a delay parity item |
| Input level Instrument/Line | Global input sensitivity/headroom | ❌ | Rig-level input gain staging |
| True/Buffered bypass | Relay vs buffer; several features force Buffered | n/a | Not meaningful in-DAW |
| Spillover (global, trails across preset changes; needs 5 s buffer warm-up) | Global setting | ❌ | Absent; see also per-preset Persist (§5) |

### 1.2 Front-panel knobs

Seven knobs: **TIME, REPEATS, FILTER, GRIT, SPEED (PARAM 1), DEPTH (PARAM 2), MIX**,
plus **TYPE** and **VALUE** encoders.

| Knob | Generic function | Machine overrides | Status | Notes |
|---|---|---|---|---|
| TIME | Delay Time (range varies per machine, §2.9) | REVERB: Pre-Delay | ✅ / ❌ | id 1 `time` (min just fixed 20→2 ms). Reverb override missing (no Reverb machine) |
| REPEATS | One repeat → self-oscillating regeneration | REVERB: Decay | ✅ / ❌ | id 2 `feedback` |
| FILTER | Shape of the repeats filter | dTAPE: Tape Age; dBUCKET: low-pass cutoff; FILTER machine: LFO center frequency | ⚠️ | ✅ for dTape/dBucket/Digital/LoFi/Filter; **no-op on Drum** (no hicut field); plain LP on Oil Can |
| GRIT | Progressively adds distortion/artifacts | dTAPE: Record Level (MX voice) / Tape Bias (Classic voice); dBUCKET: Bucket Loss; MULTITAP config: PTRN select | ⚠️ | id 29 `grit`. ✅ dTape/dBucket/LoFi (pre-S/H aliasing interaction)/OilCan (rotation jitter); **❌ Drum** (no distortion) |
| SPEED (PARAM 1) | Mod LFO speed (default assignment) | dTAPE: Crinkle; SPECTRAL: Density; MULTITAP config: GRID | ⚠️ | Per-machine specials exist; **generic delay-line Mod Speed missing on Digital/LoFi/Reverse/Ice/Spectral**; not in DELAY_PARAMS |
| DEPTH (PARAM 2) | Mod LFO depth (default assignment) | dTAPE: Wow & Flutter; SPECTRAL: Stretch; MULTITAP config: FDBK | ⚠️ | Same as above |
| MIX | Dry/wet balance | — | ✅ | id 0 |
| PARAM 1/2 assignability | Any PARAM-menu item (except 1+2 EXP Setup and later) may be assigned to SPEED/DEPTH per machine, stored per delay type; assigning a 1+2 param adjusts both delays | ❌ | **Assignment layer absent** — knobs are fixed-function in FTS; the parameter targets themselves are tracked per-row below |

### 1.3 Footswitches, tap tempo, infinite hold

| Element | Manual behavior | Status | Notes |
|---|---|---|---|
| A/B footswitches | Engage/bypass preset; A+B bank down, B+TAP bank up; amber = edited | n/a | Preset navigation is rig-level |
| Footswitch Mode `Dual` | A toggles Delay 1 enable, B toggles Delay 2 enable independently | ⚠️ | Routing id 17 can emulate (Single vs dual modes); no dedicated per-delay enable |
| **Infinite hold** | Press-hold lit A/B footswitch → infinite repeats (delay) / infinite decay (reverb) while held; release to exit | ✅ | id 5 `freeze` (delay). Reverb-side infinite ❌ (no Reverb machine) |
| TAP footswitch | Rhythmic taps set Delay Time; LED blinks tempo; applied to both delays; per-delay TAP Division refines | ✅ | id 6 `tempo_bpm` + ids 7/35/36 tap divisions |
| Remote tap (MIDI CC 93) / MIDI Clock per-preset | Sync Delay Time externally | ⚠️ | Host-tempo sync is the FTS equivalent; per-preset clock-enable flag n/a |
| MS/BPM display | Show time in ms or BPM | n/a | UI concern |

### 1.4 DSP-relevant Global Settings (housekeeping omitted)

| Global | Values | DSP relevance | Status |
|---|---|---|---|
| Bypass | True / Buffered | Trails only possible in Buffered | n/a |
| Input Level | Instrument / Line | Headroom for repeats ("crunchy" clipping) | ❌ rig-level |
| Dry Signal | Normal / KillDry | KillDry mutes dry, MIX = wet level | ✅ via mix=100% |
| Spillover | On / Off | Wet decay spills across preset changes; requires ≥5 s active | ❌ |
| I/O Config | Normal / FX Loop Mono / FX Loop Stereo / Wet Dry / Wet Dry Wet | External wet-loop insert; discrete wet/dry outs | ❌ rig-level routing |
| Looper Location | Pre / Post | Looper before vs after the delays | ❌ rig-level |
| Looper Level | 0–100 | Playback level (default 100 = unity) | ❌ rig-level |

Omitted as pure housekeeping: MIDI Channel/CC/PC/THRU, Active Banks, Home Screen,
Preset Nav, Tap Footswitch mode, EXP Mode, 1-Button-Looper function, Looper Exit,
Display/LED brightness.

---

## 2. Common Delay Parameters

Available in the PARAM menu for **all** delay types; stored per delay (per machine slot),
independently editable for Delay 1 and Delay 2. Manual note: for TAP Div, Pan, Output
Level, and Swell, the edited setting **persists when changing the delay type**.

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Modulation Speed | 0–255 | Modulation LFO speed applied to the delay repeats | ❌ | Delay-line mod exists only as machine-specific (dTape crinkle/wow, dBucket, MultiTap per-tap, Filter machine LFO). **Missing entirely on Digital / LoFi / Reverse / Ice / Spectral.** Not in DELAY_PARAMS |
| Modulation Depth | 0–255 | Modulation LFO intensity applied to the delay repeats | ❌ | Same as Mod Speed |
| TAP Division | Quarter, Dot Eighth, Eighth, Triplet, Sixteenth, Golden (≈1.62:1), Silver (≈2.41:1), Free | Note/time division of the tap tempo per delay; **Free** "unlocks" the delay for an independent delay time (no longer follows TAP/MIDI clock) | ✅ | ids 7 (`tap_div`), 35/36 (`tap_div_l`/`tap_div_r`) incl. Golden/Silver/Free |
| Pan | Left +8 … Center … Right +8 | Pans the wet signal 100% L↔R. Auto-disabled (forced center) in mono out and in Split L\|R / R\|L routings | ⚠️ | id 37 `pan` exposed for the primary delay; **Delay 2 pan not exposed** (dual surface only has B style/time/feedback/mix) |
| Output Level | 0–100 | Attenuates the individual delay's wet signal only (fine wet/dry trim per delay) | ❌ | Not in engine params; not in DELAY_PARAMS |
| Swell | Off, 0.10 to 4.0 seconds | Fade-in of the selected time applied to the repeats — softer attack | ✅ | id 4 `swell`, incl. the hardware's mix-dependent wet/dry-into-delay behavior |
| Duck Sensitivity | 0–18 | Amount of ducking applied to the delay signal, triggered by the input signal | ⚠️ | Ducking works functionally, but range **not mapped to 0–18** and **not exposed** in DELAY_PARAMS |
| Duck Release | 0.05–1.00 seconds | Release time of the ducking | ⚠️ | Same: functional, unmapped (not 0.05–1.00 s scale), unexposed |
| Smear *(Digital, MultiTap, Reverse, Ice; CC-table common)* | 0–18 | Softens repeat attack while keeping full frequency response; dreamy at high Repeats | ⚠️ | Exists only as a **chain-level** smear, not per-machine; ❌ on Reverse specifically |
| High Pass *(most machines)* | Off–900 Hz | Reduces low-frequency content of the wet signal after the delay | ✅ | id 8 `high_pass` |
| Repeat Dynamics *(Digital)* | Off / On | Non-linear REPEATS reduction — trails taper faster so the next phrase stands out | ✅ | id 9 `repeat_dyn`. Deviation: applied **globally to all machines**, manual scopes it to Digital only |
| Infinite (hold) | footswitch hold / MIDI CC 97 (0=Off, 1–127=On) | Infinite repeats while engaged | ✅ | id 5 `freeze` |

### 2.9 Per-machine Delay Time ranges (Out-of-Tempo-Range table, pg 54)

An out-of-range tap (e.g. >800 ms quarter into dBUCKET) shows the "!" indicator and the
repeats will not sync — each machine clamps to its own range.

| Delay Type | Time Range | Status | Notes |
|---|---|---|---|
| dTAPE | 60 – 2500 ms | ✅ | |
| dBUCKET | 80 – 800 ms | ✅ | True variable-clock: audio survives time changes |
| DIGITAL | 60 – 2500 ms | ✅ | |
| DRUM | 200 – 2000 ms | ✅ | |
| OIL CAN | 200 – 800 ms | ✅ | |
| MULTITAP | 60 – 2500 ms | ✅ | |
| SPECTRAL | 60 – 2500 ms | ✅ | |
| REVERSE | 60 – 2500 ms | ✅ | |
| ICE | 60 – 2500 ms | ✅ | |
| LO FI | 2 – 2500 ms | ✅ | Engine min just fixed 20→2 ms (id 1) |
| FILTER | 60 – 2500 ms | ✅ | |
| REVERB | 2 – 2500 ms Pre-Delay, 40+ seconds Decay | ❌ | Machine not implemented |

---

## 3. Per-machine sections

Machine indices — hardware MIDI TYPE enumeration (CC 1/2, values 0–11):
`0 Spectral, 1 Reverse, 2 Ice, 3 Lo Fi, 4 Filter, 5 Reverb, 6 dTape, 7 dBucket, 8 Digital, 9 Drum, 10 Oil Can, 11 MultiTap`.
FTS `DelayStyle` indices 0–12 interleave two non-MX extras — **Shimmer (4)** and
**Rhythm (7)** — with the MX machines; keep the mapping table in sync when adding Reverb.

---

### 3.1 dTAPE — Status: ✅ substantially complete (`tape_delay.rs`)

*"Immerse yourself in the legendary sound of classic sliding-head tape echo machines
with every nuance intact… Shape the tone with control of low end, wow and flutter, tape
crinkle, and more."*

Knob overrides: FILTER → **Tape Age**, GRIT → **Record Level / Tape Bias**,
SPEED (P1 default) → **Crinkle**, DEPTH (P2 default) → **Wow Flutter**.

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Tape Age (FILTER knob) | 0–127 (knob/CC 11) | Bandwidth of the tape as it ages: minimum = fresh full-bandwidth tape; clockwise = progressively darker repeats | ✅ | id 11 `tape_age` |
| Record Level / Tape Bias (GRIT knob) | 0–127 (knob/CC 13) | **MX voice**: Record Level — gain into the virtual record head; higher = more saturation on repeats. **Classic voice**: Tape Bias — under- to over-biased; higher = reduced echo volume + limited headroom; lower = cleanest echoes, most headroom; 9 o'clock = optimally biased; minimum = under-biased with extra high-frequency response | ✅ | Record-level vs bias-drive distinction modelled; self-limiting saturation present |
| Low Contour | 0–21 | Low-end shaping from full to extreme progressive high-pass; with high repeats a major factor in the overall tape-machine sound | ✅ | |
| Voice | MX, Classic | **MX**: moving-head tape delay with extended Crinkle and Wow & Flutter range; GRIT = Record Level. **Classic**: moving-head tape delay with Tape Bias control; GRIT = Tape Bias | ✅ | id 10 `voice` |
| Crinkle | 0–255 | Amount/severity of tape irregularities — friction, creases, splices, contaminants. Min = fresh clean tape; max = mangled and chewed for years | ✅ | id 12 `crinkle`; **tracks tape speed** as on hardware |
| Wow Flutter | 0–255 | Mechanically-related tape speed fluctuations → natural tape-style modulation. Full CCW = perfectly serviced machine; full CW = machine in need of service | ✅ | |
| *(Tape speed)* | Fast / Normal | (Implementation detail matching hardware time-range voicing) | ✅ | TapeSpeed Fast/Normal implemented |

**Usage tips (behavioral):** Low Contour → progressive high-pass on repeats; reduce
REPEATS at minimum Tape Bias to tame regenerative highs.

---

### 3.2 dBUCKET — Status: ✅ best-modelled (`bbd_delay.rs`)

*"Authentic, detailed experience of classic analog bucket-brigade delay types, coveted
for their warm repeats and pleasant signal degradation."*

Knob override: GRIT → **Bucket Loss**. Time range 80–800 ms ("true to the analog
architecture").

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Bucket Loss (GRIT knob) | 0–127 (knob/CC 13) | BBD "chip" loss at each stage — no loss at minimum → full noisy loss at maximum. Softens repeat attack, adds distortion + noise by-products of the charge-transfer process; high repeats morph into hazy sustain | ✅ | id 13 `bucket_loss`; loss is **time-dependent** as on hardware |
| Voice | MX, Classic | **MX**: warm response, full low end. **Classic**: brighter response, mildly rolled-off low end | ✅ | Implemented via BBD stage count |
| FILTER knob behavior | 0–127 | Low-pass cutoff; at max goes bandpass/peaking | ✅ | Verified bandpass/peaking-at-max behavior |
| Modulation (Mod Speed/Depth) | 0–255 each | "Rich and syrupy" BBD modulation; Mod Depth adds lush roundness + wider stereo imaging | ✅ | Mono-till-modulated stereo behavior reproduced |

**Key physical behavior (parity-critical):** variable-clock architecture — audio in the
bucket line **survives delay-time changes** (pitch-bends rather than glitching). ✅.

---

### 3.3 DIGITAL — Status: ❌ voices missing (`clean_delay.rs`)

*"A crystal-clear digital delay that invites endless experimentation… Switch between
distinct digital voicings and enable Repeat Dynamics."*

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Smear | 0–18 | Softens repeat attack, full frequency response retained; higher mix levels stay out of the dry signal's way; dreamy/ethereal at high Repeats | ⚠️ | Chain-level smear only, not the machine param |
| High Pass | Off–900 Hz | Reduces low-frequency content of the wet signal post-delay | ✅ | id 8 |
| Repeat Dynamics | Off, On | Non-linear repeat taper (see §2) | ✅ | id 9 (globally applied) |
| Voice | 24/96, ADM, 12-Bit, Classic | **24/96**: modern, clean, pure with subtle dynamics processing. **ADM**: early-80s adaptive delta modulation — snappy, percussive repeats. **12-Bit**: mid-80s 12-bit conversion — slightly darker, warmer repeats. **Classic**: the original TimeLine digital sound — slightly rounder/fatter with retained clarity | ❌ | **Engine ignores `voice` for Digital.** No 1-bit ADM (percussive attack emphasis), no 12-bit companding, no Classic voicing |
| FILTER knob (voice-dependent) | 0–127 | 24/96 / ADM / 12-Bit voices: progressively reduces high-frequency content. **Classic voice**: unique filter — full bandwidth at min → analog-delay response at noon → tape-delay response at max | ❌ | clean_delay has only time/feedback/hicut/locut/q/decay_tilt; Classic morphing filter absent |
| GRIT knob | 0–127 | "Add GRIT for some dirty digital" | ⚠️ | Generic grit only, no voice interaction |
| Mod Speed / Depth | 0–255 | "Stereo dimension and movement for classic rack-style modulated delay" | ❌ | No delay-line mod on Digital |

---

### 3.4 DRUM — Status: ⚠️ heads grid works, spacing unreachable, filter/grit missing (drum machine in engine)

*"Delivers the beloved mechanical inconsistencies of real drum echo units for unique
warble and soft-clip textures. Enable individual play heads and vary their spacing and
feedback to generate evolving patterns."* Time range 200–2000 ms.

**Head model:** four heads, each with a **Playback** switch (Off / 50% / 100% level), a
**Feedback** switch (Off/On), and an **LR pan** slider (L+5 … Center … R+5, default
Center). TIME sets head 4; heads 1–3 subdivide proportionally per Spacing. FB off →
single repeat from that head; FB on → repeats per the FEEDBACK knob. A head can feed
back **even with its Playback off**. Per-head FB repeats pan L/R individually.

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Head Edit | Graphical: per-head PB Off/50%/100%, FB Off/On, LR pan L+5…R+5 | Individually engages the four playback heads + levels; four feedback heads + pans | ✅ | off/half/full, independent fb enable, pan; fb mono-summed so the pan-rotation topology emerges |
| Spacing | Even, Triplet, Golden, Silver | **Even**: heads at 16th, 8th, dotted 8th, quarter. **Triplet**: 16th triplet, 8th triplet, quarter triplet, quarter. **Golden**: distances per golden ratio ≈1.62:1 — fastest density buildup with multiple heads repeating. **Silver**: silver ratio ≈2.41:1 — repeats "bunch up" toward the head-4 quarter | ⚠️ | Reachable via `DelayEngine::set_drum_spacing` (2026-07-27). Shipped Triplet `[1/6, 1/3, 2/3, 1]` matches the manual verbatim (16th-trip=1/6, 8th-trip=1/3, quarter-trip=2/3, quarter=1) |
| Lo Cut | 0–255 (CC 0–127) | Low-frequency shaping of the echo repeats | ❌ | No field at all |
| FILTER knob | 0–127 | Repeats filter | ❌ | **Knob is a no-op** — drum machine has no hicut field |
| GRIT knob | 0–127 | Soft-clip / distortion textures (machine blurb) | ❌ | No distortion in the drum machine |
| Mod Speed / Depth | 0–255 | Warble | ⚠️ | Menu lists Mod Speed; verify against drum warble implementation during dial-in |

---

### 3.5 OIL CAN — Status: ⚠️ cadence right, filter and modulation character wrong

*"Darker, murkier, and less predictable than traditional echo units. Unique rhythmic
repeats and syrupy modulations shift and morph over time, creating sustained atmospheric
textures."* Time range 200–800 ms.

**Physical model:** record head + two play heads (P1 Short, P2 Long) on a rotating
oil-lubricated disc, **no erase head** — dissipating charge remains as the can rotates,
so repeats occur **even at minimum regeneration** and echoes lack a strong rhythmic
pattern. FEEDBACK adds play-head→record-head regeneration; TIME sets the initial echo
time for **both** heads simultaneously.

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Head Select | Long, Short, Both | Engage the Long or Short play head, or Both for cascading repeats with shifting patterns | ✅ | ROTATION_RATIO 1.45 between heads; same rotation period drives both |
| Ghost echo (no erase head) | — | Repeats at regen = 0 from residual charge | ✅ | Ghost echo at repeats=0 implemented |
| GRIT knob | 0–127 | Distortion/artifacts | ✅ | Implemented as rotation jitter (fits the machine) |
| FILTER knob | 0–127 | Band-limited, murky echo voicing | ❌ | Plain LP 500–8000 Hz; **no dark bandpass at max, no bonus bandwidth at min** |
| Mod Speed / Depth | 0–255 | "Syrupy modulations shift and morph over time" | ❌ | Two hard-coded sine LFOs (0.9 / 6.3 Hz); **no Mod Speed control**, no spring-loaded slow-then-accelerate character |

---

### 3.6 MULTITAP — Status: ⚠️ DSP complete, unreachable through DelayEngine; zero param exposure

*"A variety of multi-tap patterns spanning from rhythmic to ambient… Choose from
templates, patterns, note divisions, and filtering for dimensional and trance-like
results."* 8 taps, 16 classic patterns.

**Config graphical interface — per-tap parameters** (each of Taps 1–8 individually
enable/disable):

| Per-tap param | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| STP (Step) | 1.1 1.2 1.3 1.4 2.1 2.2 2.3 2.4 3.1 3.2 3.3 3.4 4.1 4.2 4.3 4.4 5.1 (Step Guide: notes 1–16 + 5.1; each beat of the current TAP DIV splits into 4 steps) | Delay position per tap. E.g. TAP DIV = quarter: STP 2.1 = quarter-note repeat, 4.1 = half note, 1.3 = eighth note | ✅ DSP | Position per tap implemented |
| RPT (Repeats) | knob range | Repeats per tap — *"only available when the Faders FDBK type is set to PARALLEL or INPUT"* | ✅ DSP | Per-tap repeats implemented |
| FIL (Filter Type) | Low Pass, High Pass, Band Pass, Peak, Shelf EQ | Per-tap filter type; CUT adjusts it | ✅ DSP | Implementation has 9 filter types (superset) |
| CUT (Filter Cut) | 0–255 | Cutoff frequency for the selected FIL type | ✅ DSP | |
| MOD (On/Off) | Off, On | Per-tap modulation enable; SPEED/DEPTH knobs set overall modulation after exiting Config | ✅ DSP | |
| PAN | L+5 … R+5 | Per-tap left-right balance | ✅ DSP | |
| LVL (Level) | 0–255 | Per-tap output volume; MIX sets overall wet/dry after exiting Config | ✅ DSP | |

**Config "Faders" row** (GRIT = PTRN, PARAM 1 = GRID, PARAM 2 = FDBK):

| Param | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| PTRN (Pattern) | Classic 1 – Classic 16 | 16 classic patterns, "simple ping-pong to rhythmic trances"; stereo tap field with both outs, mono-summed when R unplugged | ✅ DSP | `apply_classic`/`set_preset` implement Classic 1–16 |
| GRID | 16ths, Swing 16ths, Triplets, Off | Tap-division spacing grid; choosing a Grid that differs from the pattern alters it (Pattern shows "Custom") | ⚠️ | Grid implemented as 16th / Triplet / Off-256; **Swing 16ths unconfirmed** |
| FDBK (Feedback mode) | 1 BEAT, 2 BEAT, 3 BEAT, 4 BEAT, PARALLEL, INPUT | **n BEAT**: single feedback tap fixed at beat *n* regardless of playback taps. **PARALLEL**: 8 non-interacting parallel delay lines, outputs summed. **INPUT**: all taps feed back to the input per each tap's RPT | ✅ DSP | Input vs Parallel verified; beat modes present |

**Blocking defects:** `engine.rs:607–618` **clobbers taps + feedback_mode on every
update and never syncs grid** — `apply_classic` / `set_preset` / grid are unreachable
through `DelayEngine`. **Zero MultiTap parameters in DELAY_PARAMS.** Machine-level
High Pass (Off–900 Hz) ✅ id 8; Smear (0–18) ⚠️ chain-level; Stereo Spin param appears
in the menu screenshot — verify existence during dial-in.

---

### 3.7 SPECTRAL — Status: ⚠️ granular approximation, no FFT-domain character

*"This granular delay minces the signal into fragments and applies pitch, reverse, time
stretch, and filter effects to create a montage of glitchy patterns and fascinating
panoramas."*

Knob overrides: SPEED (P1) → **Density**, DEPTH (P2) → **Stretch**.

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Grain Shape | Soft, Swell, Soft Pluck, Pluck, Bounce | Attack/envelope/decay "shape" of each grain — textures and dynamics. Soft/Swell = smooth-subtle; Pluck/Bounce = aggressive attacks | ✅ | id 22 `grain_shape`, incl. Bounce |
| High Pass | Off–900 Hz | Wet low-cut post-delay | ✅ | id 8 |
| Spread | 0–20 | Amount of left-right panning — narrow to wide stereo imaging | ✅ | id 26 `spread` |
| Direction | Forward, Reverse, Both | Grain playback direction; Both = random mix of forward and reverse | ✅ | id 23 `direction` |
| Octave | 0–20 | Amount of octave-up pitch shifting applied to the grains | ✅ | id 28 `octave` |
| Density Sync | Off, On | On: grain spacing repeats in sync with Time/Tap Tempo. Off: grain fragments repeat randomly | ⚠️ | Sync exists but semantics differ (below) |
| Density | 1/1–1/32 (synced; 15 steps, CC 0–14) · 250 ms → 6 ms (free) | Synced: duration + amount of grain content, from 1/1 (grain = full repeat) to 1/32 (1/32 of repeat time). Free: 250 ms (lowest) to 6 ms (highest) | ⚠️ / ✅ | Free-rate 6–250 ms ✅ (id 25 `density_ms`). Synced density ⚠️: implemented as **fraction-of-delay-time, not grains-per-beat**; off-grid ratios (e.g. 2/3) unreachable. id 24 `density` |
| Stretch | 0–255 | Grain fragments stretch out as the value increases | ✅ | id 27 `stretch` |

**Character gap:** hardware is FFT/spectral; FTS is **time-domain granular** with a
fixed allpass/shelf approximation (self-flagged `spectral_delay.rs:16–25`), max 8
grain voices. **Usage tips:** Reverse/Both injects reverse repeats; Octave up blends
shimmer; low Density = broad/smooth, high = busy/complex; high REPEATS → dramatic
oscillation of fragments.

---

### 3.8 REVERSE — Status: ❌ weakest machine

*"Play a phrase and hear it echo back in reverse. The reverse process is **synced to
performance** for a consistently repeatable reverse experience."*

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Reverse topology | — | Input-triggered, performance-synced reverse windows | ❌ | FTS is a **free-running two-grain crossfade cycler**, not input-triggered |
| Smear | 0–18 | Softens attack, enhances the "swell" of reversed repeats | ❌ | Not present on this machine |
| High Pass | Off–900 Hz | Wet low-cut | ✅ | id 8 |
| Mod Speed / Depth | 0–255 | In PARAM menu for Reverse | ❌ | No mod on this machine |

**Usage tips:** TIME ≈500 ms + ringing chord = rhythmic effect; expression-on-MIX trick
keeps dry feeding the reverse line while "bypassed" for reverse-tape solos.

---

### 3.9 ICE — Status: ✅ complete (`pitch_delay.rs`)

*"Slices and dices the input signal and plays the pieces back with a selectable interval
shift from anywhere between two octaves up to one octave down."*

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Interval | −Octave, −Maj 7th, −Min 7th, −Maj 6th, −Min 6th, −5th, −Tritone, −4th, −Maj 3rd, −Min 3rd, −Maj 2nd, −Min 2nd, −50 Cents, −25 Cents, +25 Cents, +50 Cents, +Min 2nd, +Maj 2nd, +Min 3rd, +Maj 3rd, +4th, +Tritone, +5th, +Min 6th, +Maj 6th, +Min 7th, +Maj 7th, +Octave, +Oct & 5th, +2 Octs | Pitch interval of the audio slices, octave down to two octaves up. (Manual's own tables disagree on count: the param table omits −Min 2nd, the CC table lists it at 11 and declares 0–27; the full union is the 30-entry list here) | ✅ | id 14 `interval` — full 30-interval menu implemented |
| Slice | Short, Medium, Long | Size of the audio chunks sliced and pitched; **slice sizes scale with the delay time** | ✅ | id 15 `slice`, scaling implemented |
| Blend | 0–20 | Blend of Dry vs Ice signal **on the delay line**. Below half + REPEATS ≈3 o'clock = huge sounds; Ice "floats in" as repeats regenerate | ✅ | id 16 `blend`, applied pre-feedback; regen re-shift ladder implemented (each pass re-shifts) |
| Smear | 0–18 | Softens attack of repeats | ⚠️ | Chain-level only |
| High Pass | Off–900 Hz | Wet low-cut | ✅ | id 8 |
| Mod Speed / Depth | 0–255 | "Add swirl and dimension" | ❌ | No delay-line mod on Ice |

---

### 3.10 LO FI — Status: ✅ near-complete

*"Creatively destruct the signal with bit crushing and sample rate reduction… hand-
crafted filters spanning transistor radios, telephones, and more. Our exclusive dVINYL
algorithm… for realistic vinyl effects."* Time range **2–2500 ms** (min 2 ms = real-time
lo-fi machine at full wet).

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Sample Rate | 750 Hz–96 kHz (CC 0–20, 21 steps) | Delay-line sample rate, low fidelity → high; reduced rate = aliasing artifacts "wreak havoc" | ⚠️ | Implemented + sounds right, but exposed as **divisor 1–64, not absolute Hz** (id 31 `sr_div`) |
| Bit Depth | 4-Bit to 32-Bit (CC 0–20) | Digital bit depth low→high; fuzzy crunchy artifacts as it drops | ✅ | id 30 `bit_depth` (4–32) |
| Lo Fi Mix | 0–20 | Blend of SR/bit-affected signal with full-fidelity signal; max = "full Lo Fi crud". Vinyl artifacts NOT affected by this blend | ✅ | id 32 `lofi_mix` |
| Vinyl | 0–18 | dVINYL: random vinyl dust noise + scratches from a 33⅓ rpm record. **Lower half (dynamic)**: noise only with the repeats. **Upper half (static)**: full-time vinyl noise (intros/outros/bridges) | ✅ | id 33 `vinyl`, dynamic/static halves implemented |
| Filter Shape | Off, Vintage, Victrola, Clock Radio, Bullhorn, Cheerleader, Antiq Telephone, Cell Phone, Intercom (CC 0–8; the param table line-wraps "Vintage Victrola" as one entry — CC table is authoritative: 9 values) | Filters inspired by telephones, Victrolas, AM radios, bullhorns, etc.; processes the Lo Fi + full-fidelity blend and any Vinyl noise | ✅ | id 34 `filter_shape` — 8 shapes + Off; reconcile the Vintage/Victrola split during dial-in |
| Grit interaction | — | GRIT before the sample/hold so distortion aliases | ✅ | Grit-before-S/H ordering implemented |
| Mod Speed / Depth | 0–255 | Chorus/flange/vibrato on the delay line; "96 kHz + 32-bit + Filter Off" = modern digital mod effects | ❌ | No delay-line mod on LoFi |
| High Pass | Off–900 Hz | Wet low-cut | ✅ | id 8 |

---

### 3.11 FILTER — Status: ⚠️ complete surface, SVF lowpass-only, unexposed

*"Morphs the sound with synchronized tremolo and dynamic filtering… Choose from several
LFO waveshapes and filter controls for speed, depth, Q, cutoff, and beyond."* Combines
the original TimeLine's Filter **and** Tremolo delay machines. FILTER knob = **LFO
center frequency** (mid-point of the sweep).

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Filter LFO | +Triangle, −Triangle, +Square, −Square, +Sine, −Sine, Ramp, Saw, Random, Down, Up (11 modes, CC 0–10) | LFO waveform controlling the filter envelope of the delayed signal. '+' shapes: highest frequency synchronous to the input; '−' shapes: lowest frequency synchronous to the input | ⚠️ | All shapes present incl. attack-triggered one-shot Down/Up ✅, but **± polarity is waveform sign only — not attack-sync for cyclic shapes** |
| Filter Speed | 1/32–32/1 (CC 0–34, 35 ratios) | Ratio at which LFO cycles **track the Delay Time** (stays in sync when TIME/TAP changes) | ✅ | Speed as delay-time ratio implemented |
| Filter Depth | 0–18 | Depth/intensity of the filter sweep; FILTER knob sets the sweep mid-point | ✅ | |
| Filter Q | 0.5–10.0 (CC 0–11, 12 steps) | Resonance: low = mild/broad, high = sharp resonant peaks (dramatic sweeps; apparent wet volume rises — trim with MIX) | ✅ | |
| Tremolo LFO | Triangle, Square, Sine, Ramp, Saw (CC 0–4) | LFO waveform for tremolo of the delayed signal | ✅ | Trem section implemented |
| Tremolo Speed | 1/32–32/1 (CC 0–34) | Trem LFO cycle ratio vs Delay Time | ✅ | |
| Tremolo Depth | 0–18 | Trem intensity; max = zero delay volume at the LFO trough | ✅ | |
| High Pass | Off–900 Hz | Wet low-cut (reduces boominess of LFO sweeps + repeats) | ✅ | id 8 |
| Filter placement | — | (Implementation extra: pre/post switch) | ✅ | Not a hardware param |
| Filter topology | — | Sweeping filter type | ⚠️ | **SVF is lowpass-only** — verify bandpass/other coloration vs hardware during dial-in |
| Param exposure | — | — | ❌ | **No Filter-machine params in DELAY_PARAMS** |

**Usage tips:** Sine + slow speed = atmospheric; Random + high speed + high Q =
futuristic; Trem Saw at 1/2–1/4 = "plectrum" chops; Trem Ramp slow = reverse envelope.

---

### 3.12 REVERB — Status: ❌ NOT IMPLEMENTED

*"A new, wide-ranging reverb capable of creating everything from intimate spaces to
larger-than-life sustained washes. Inspired by our Flint pedal, it also offers Tremolo
modulation on the wet signal for enhanced texture."*

No `DelayStyle::Reverb` exists. Full hardware surface, for when it lands:

| Parameter | Values (verbatim) | Description | Status |
|---|---|---|---|
| Pre-Delay (TIME knob) | 2 ms – 2.5 seconds | Time between dry signal and reverb onset; shown on Home screen. Manual recommends TAP DIV = Free (esp. in Dual) so tap/MIDI clock doesn't drive Pre-Delay | ❌ |
| Decay (REPEATS knob) | up to 40+ seconds; approaches **infinite** at max | Duration of the reverberated decay; footswitch-hold = infinite decay while held | ❌ |
| FILTER / GRIT knobs | 0–127 | Standard tone/grit shaping | ❌ |
| Mod Speed / Mod Depth (SPEED/DEPTH) | 0–255 | Tremolo-style modulation on the wet signal; "hypnotic, breathy textures" | ❌ |
| PARAM menu | Mod Speed, Mod Depth, TAP Div, Pan (+ common params) | No reverb-specific PARAM items — Common Parameters cover reverb behaviors (Swell etc.) | ❌ |

**Usage notes:** Swell + low Pre-Delay + high Decay = ambient washes; short Decay +
min Pre-Delay = room "air"; Dual preset combining REVERB with any delay = classic
delay-into-reverb rigs.

---

## 4. 1+2 Parameters (shared, applied to both delays)

Appear at the end of the PARAM menu with the 1+2 symbol; stored per preset. When Delay 2
is Off they apply to Delay 1. Any 1+2 param assigned to a PARAM knob adjusts **both**
delays simultaneously (EXP Setup and later are not assignable).

| Parameter | Values (verbatim) | Description | Status | Notes |
|---|---|---|---|---|
| Boost | −3dB to +3dB (CC 0–60: 0=−3dB, 30=0dB, 60=+3dB; default 0.0dB unity) | Final output level per preset — level matching or solo boost | ❌ | Rig-level (preset gain), not delay-dsp |
| Persist | On, Off (default Off) | Delay trails continue when the effect is bypassed; enabling forces Buffered Bypass. (Global Spillover extends trails across preset changes) | ❌ | Spillover/persist absent |
| Expression Pedal Setup | MIDI On/Off, Heel, Toe | Per-preset assignment of any knob(s) to an expression pedal (EXP jack or MIDI CC 100), with Heel/Toe min/max limits per knob; per-delay focus when assigning | ❌ | Rig-level control-mapping layer (FTS has its own param automation) |
| Dual Delay Enable | Off, Parallel, Series 1▶▶2, Series 1◀◀2, Split 1L\|2R, Split 1R\|2L | **Off**: Delay 2 removed (single delay). **Parallel**: input into each independently, outputs summed (per-delay Pan works in stereo). **Series 1▶▶2**: input→Delay 1→Delay 2 (like two pedals chained). **Series 1◀◀2**: reversed order. **Split 1L\|2R**: Delay 1 mono→LEFT OUT only, Delay 2 mono→RIGHT OUT only. **Split 1R\|2L**: outputs swapped | ✅ | id 17 `routing`: Single / Series12 / Series21 / Parallel / Split / SplitSwapped |
| — Delay 2 parameter surface | all per-delay params independently editable for each delay (type, time, feedback, mix, pan, mod, machine params, TAP Division) | Independent machines + tap divisions per delay | ⚠️ | Independent machines + tap divs ✅ (ids 18–21: `style_b`/`time_b`/`feedback_b`/`mix_b`); **everything else for Delay B unexposed** (no B pan/machine params) |
| MIDI Clock | On, Off (default Off) | Per-preset enable of MIDI Clock sync for Delay 1+2's Delay Time | ⚠️ | Host tempo sync is the FTS analog; no per-preset gate |
| Tap Mode | Preset (default), Global | Preset: saved tempo recalled on preset load. Global: last tapped tempo persists across preset changes | ❌ | Preset/tempo management is rig-level |
| Copy Settings From | Presets 00A thru 149B | Imports another preset's Delay 1 type+settings into Delay 2 of the current preset | n/a | Preset-management convenience |

---

## 5. MIDI CC reference (authoritative full parameter list)

Condensed from manual pp. 95–101. This is the hardware's complete automatable surface —
useful as the canonical enumeration when building the FTS param map.

### 5.1 Knobs & common parameters (per-delay CC pairs)

| Parameter | D1 CC | D2 CC | Range / enum | FTS id | Status |
|---|---|---|---|---|---|
| Delay TYPE Select | 1 | 2 | 0–11: 0 Spectral, 1 Reverse, 2 Ice, 3 Lo Fi, 4 Filter, 5 Reverb, 6 dTape, 7 dBucket, 8 Digital, 9 Drum, 10 Oil Can, 11 MultiTap | 3 `style` / 18 `style_b` | ⚠️ FTS DelayStyle 0–12 interleaves non-MX Shimmer(4)+Rhythm(7); Reverb missing |
| TIME knob | 3 | 4 | 0–127 | 1 / 19 | ✅ |
| REPEATS knob | 5 | 6 | 0–127 | 2 / 20 | ✅ |
| Output Level | 7 | 8 | 0–100 | — | ❌ |
| Pan | 9 | 10 | 0–16 (0 full L, 8 center, 16 full R) | 37 (A only) | ⚠️ |
| FILTER knob | 11 | 12 | 0–127 | (machine-dependent) | ⚠️ per §3 |
| GRIT knob | 13 | 14 | 0–127 | 29 `grit` | ⚠️ per §3 |
| MIX knob | 15 | 16 | 0–127 | 0 / 21 `mix_b` | ✅ |
| TAP Division | 17 | 18 | 0–7: Quarter, Dotted Eighth, Eighth, Triplet, Sixteenth, Golden Ratio, Silver Ratio, Free | 7, 35, 36 | ✅ |
| PARAM 1 knob | 19 | 20 | 0–127 | — | ❌ (no assignment layer) |
| PARAM 2 knob | 21 | 22 | 0–127 | — | ❌ |
| Swell | 23 | 24 | 0–28 (tracks display: Off, 0.10–4.0 s) | 4 | ✅ |
| Duck Release | 25 | 26 | 0–20 (0.05–1.00 s) | — | ⚠️ functional, unmapped/unexposed |
| Duck Sensitivity | 27 | 28 | 0–18 | — | ⚠️ |
| Modulation Speed | 29 | 30 | 0–127 (also Spectral Density 0–14, dTape Crinkle 0–127 — CC reused per machine) | — | ❌ generic |
| Modulation Depth | 33 | 34 | 0–127 (also Spectral Stretch 0–255, dTape Wow&Flutter — CC reused) | — | ❌ generic |
| Smear | 35 | 36 | 0–18 | — | ⚠️ chain-level |
| High Pass | 37 | 38 | 0–19 (0 = Off … 900 Hz) | 8 | ✅ |

### 5.2 Machine-specific CCs

| Machine | Parameter | D1 CC | D2 CC | Range / enum | FTS id | Status |
|---|---|---|---|---|---|---|
| SPECTRAL | Grain Shape | 103 | 104 | 0–4: Soft, Swell, Soft Pluck, Pluck, Bounce | 22 | ✅ |
| | Direction | 107 | 108 | 0–2: Forward, Reverse, Both | 23 | ✅ |
| | Octave | 109 | 110 | 0–20 | 28 | ✅ |
| | Density Sync | 111 | 112 | 0 Off / 1 On | (24 vs 25 selection) | ⚠️ |
| | Density | 29 | 30 | 0–14 (1/1–1/32) | 24 | ⚠️ semantics |
| | Stretch | 33 | 34 | 0–255 | 27 | ✅ |
| ICE | Interval | 53 | 54 | 0–27 (see §3.9 30-interval list; manual table has typos) | 14 | ✅ |
| | Slice | 55 | 56 | 0–2: Short, Medium, Long | 15 | ✅ |
| | Blend | 57 | 58 | 0–20 | 16 | ✅ |
| LO FI | Sample Rate | 59 | 60 | 0–20: 750 Hz–96 kHz | 31 | ⚠️ divisor not Hz |
| | Bit Depth | 61 | 62 | 0–20: 4-Bit–32-Bit | 30 | ✅ |
| | Lo Fi Mix | 63 | 64 | 0–20 | 32 | ✅ |
| | Vinyl | 65 | 66 | 0–18 | 33 | ✅ |
| | Filter Shape | 67 | 68 | 0–8: Off, Vintage, Victrola, Clock Radio, Bullhorn, Cheerleader, Antique Telephone, Cell Phone, Intercom | 34 | ✅ |
| FILTER | Filter LFO | 69 | 70 | 0–10: +Triangle, −Triangle, −Square, +Square, +Sine, −Sine, Ramp, Saw, Random, Down, Up | — | ⚠️ unexposed |
| | Filter Speed | 71 | 72 | 0–34: 1/32–32/1 | — | ⚠️ unexposed |
| | Filter Depth | 73 | 74 | 0–18 | — | ⚠️ unexposed |
| | Filter Q | 75 | 76 | 0–11: 0.5–10.0 | — | ⚠️ unexposed |
| | Trem LFO | 77 | 78 | 0–4: Triangle, Square, Sine, Ramp, Saw | — | ⚠️ unexposed |
| | Trem Speed | 85 | 86 | 0–34: 1/32–32/1 | — | ⚠️ unexposed |
| | Trem Depth | 87 | 88 | 0–18 | — | ⚠️ unexposed |
| dTAPE | Low Contour | 41 | 42 | 0–21 | — | ✅ DSP, unexposed |
| | Voice | 43 | 44 | 0 MX / 1 Classic | 10 | ✅ |
| | Crinkle | 29 | 30 | 0–127 | 12 | ✅ |
| | Wow and Flutter | 33 | 34 | 0–127 | — | ✅ DSP |
| dBUCKET | Voice | 45 | 46 | 0 MX / 1 Classic | 10 | ✅ |
| DIGITAL | Repeat Dynamics | 47 | 48 | 0 Off / 1 On | 9 | ✅ (global) |
| | Voice | 49 | 50 | 0 MX / 1 Classic *(sic — manual CC table lists 2 values; the param table lists 4: 24/96, ADM, 12-Bit, Classic)* | 10 (ignored) | ❌ |
| DRUM | Low Cut | 115 | 116 | 0–127 | — | ❌ |
| | Spacing | 117 | 118 | 0–3: Even, Triplet, Golden Ratio, Silver Ratio | — | ⚠️ unreachable |
| OIL CAN | Head Select | 51 | 52 | 0–2: Long, Short, Both | — | ✅ DSP, unexposed |
| MULTITAP | Grid | 89 | 90 | 0–3: 16th, Swing 16th, Triplet, Off | — | ⚠️ unreachable |
| | Pattern Template | 91 | 92 | 0–15: Classic 1 – Classic 16 | — | ⚠️ unreachable |
| | Feedback Mode | 95 | 96 | 0–5: 1 Beat, 2 Beat, 3 Beat, 4 Beat, Parallel, Input | — | ⚠️ unreachable |

### 5.3 1+2 shared and hardware-control CCs

| Parameter | CC | Range / enum | Status |
|---|---|---|---|
| Boost | 122 | 0–60 (0=−3dB, 30=0dB, 60=+3dB) | ❌ rig-level |
| Persist | 123 | 0=Off, 1–127=On | ❌ |
| Dual Mode | 124 | 0–5: Off, Parallel, Series 1▶▶2, Series 1◀◀2, Split L\|R, Split R\|L | ✅ id 17 |
| Infinite Off/On | 97 | 0=Off, 1–127=On | ✅ id 5 `freeze` |
| Remote TAP Tempo | 93 | any value per tap pulse | ✅ via id 6 `tempo_bpm` |
| Expression Pedal | 100 | 0–127 (drives per-preset EXP knob assignments; MIDI EXP must be On) | ❌ |
| Preset Bypass | 102 | 0=bypassed, 1–127=engaged | n/a rig-level |
| A / B / TAP footswitch | 80 / 81 / 82 | 0=press, 127=release | n/a |
| Value encoder | 83 | 0=CCW, 1=CW | n/a |
| MIDI Patch Bank | 0 | 0–2 (presets 000A–063B / 064A–127B / 128A–149B, + PC 0–127) | n/a |
| Looper: Record/Play/Stop/Fwd-Rev/Speed/Loc/Undo/Redo/Level | 119/120/121/125/126/84/98/99/127 (+ MIDI Notes 0,2,4,16,7,9,21,19,24,23) | see manual pg 96 | ❌ rig-level |

### 5.4 DELAY_PARAMS id map (FTS, for cross-reference)

`0 mix · 1 time (2–2500 ms) · 2 feedback · 3 style · 4 swell · 5 freeze · 6 tempo_bpm ·
7 tap_div · 8 high_pass · 9 repeat_dyn · 10 voice · 11 tape_age · 12 crinkle ·
13 bucket_loss · 14 interval · 15 slice · 16 blend · 17 routing · 18 style_b ·
19 time_b · 20 feedback_b · 21 mix_b · 22 grain_shape · 23 direction · 24 density ·
25 density_ms · 26 spread · 27 stretch · 28 octave · 29 grit · 30 bit_depth ·
31 sr_div · 32 lofi_mix · 33 vinyl · 34 filter_shape · 35 tap_div_l · 36 tap_div_r ·
37 pan`

**Not exposed at all:** any Drum, Oil Can, MultiTap, or Filter-machine parameter;
ducking (sens/release); mod speed/depth; output level; Delay-B pan/machine params.

---

## 6. Dial-in queue

Ordered by severity — ❌ blockers first, then ⚠️ partials, then ✅ verification passes.
"Dial in" = A/B each row of the machine's table against the pedal, one parameter at a
time, at the manual's exact value steps.

### ❌ Missing / broken

1. **REVERB** — machine does not exist (`DelayStyle::Reverb` absent). Needs: Flint-style
   reverb core, Pre-Delay 2 ms–2.5 s on TIME, Decay→infinite on REPEATS, wet-signal
   tremolo via Mod Speed/Depth, footswitch-hold infinite decay, TAP-Div-drives-predelay
   (with Free opt-out), common params (Swell/Duck/Pan/Output Level).
2. **REVERSE** — wrong topology: free-running two-grain crossfader instead of
   input-triggered, performance-synced reverse windows. Also missing: Smear, Mod.
3. **DIGITAL voices** — engine ignores `voice`; implement 24/96 (subtle dynamics), ADM
   (1-bit adaptive delta mod, percussive attack emphasis), 12-Bit (companding), Classic
   (rounder/fatter) + the Classic voice's morphing FILTER knob (full-bw → analog → tape).
4. **DRUM filter + grit** — FILTER knob is a no-op (no hicut field), Lo Cut 0–255
   missing, no distortion/soft-clip. Also unblock Spacing (below).
5. **OIL CAN filter + mod** — replace plain LP 500–8000 with the dark bandpass-at-max /
   bonus-bandwidth-at-min voicing; replace the two hard-coded sine LFOs (0.9/6.3 Hz)
   with a Mod Speed control and the spring-loaded slow-then-accelerate character.
6. **Common Mod layer** — delay-line Mod Speed/Depth (0–255) missing entirely on
   Digital, LoFi, Reverse, Ice, Spectral; nothing exposed in DELAY_PARAMS for any
   machine's mod.
7. **Param1/Param2 assignment layer** — absent (per-machine default + reassignment of
   SPEED/DEPTH to any PARAM item). Decide: implement, or declare FTS param surface the
   equivalent and close.
8. **Boost / Persist / Spillover / EXP Setup / Looper / Input Level** — rig-level;
   track outside delay-dsp (trails-on-bypass = Persist is the one most likely to be
   wanted in-engine).

### ⚠️ Partial / unreachable

9. **MULTITAP plumbing** — DSP is complete (per-tap STP/RPT/FIL/CUT/MOD/PAN/LVL,
   Classic 1–16, grid, Input/Parallel/n-Beat feedback) but `engine.rs:607–618` clobbers
   taps + feedback_mode every update and never syncs grid → `apply_classic`/`set_preset`
   unreachable; zero MultiTap ids in DELAY_PARAMS. Confirm Swing-16th grid exists.
10. **DRUM spacing** — `DrumSpacing` (Even/Triplet/Golden/Silver) unreachable
    (`engine.rs:591` clobbers heads, never calls `set_spacing`); Triplet positions
    flagged wrong (`[1/6, 1/3, 2/3, 1]`) — verify against 16th-trip/8th-trip/
    quarter-trip/quarter by ear.
11. **FILTER machine exposure + topology** — zero params in DELAY_PARAMS; SVF is
    lowpass-only; ± polarity is waveform sign only (should be attack-sync for cyclic
    shapes: '+' = highest freq at input onset, '−' = lowest).
12. **SPECTRAL character** — time-domain granular vs hardware FFT (self-flagged
    `spectral_delay.rs:16–25`); synced Density is fraction-of-delay-time, not
    grains-per-beat, off-grid ratios (2/3) unreachable; 8-voice cap.
13. **Ducking** — map Duck Sens to 0–18 and Duck Release to 0.05–1.00 s; expose both in
    DELAY_PARAMS.
14. **Dual 1+2 surface** — spillover/persist absent; Delay-B exposes only
    style/time/feedback/mix (no B pan, B machine params, B mod); FS-Dual-mode style
    per-delay enable missing.
15. **LO FI sample rate mapping** — expose as absolute Hz (750 Hz–96 kHz, 21 steps)
    instead of divisor 1–64; reconcile Vintage vs Victrola shape split (CC table says 9
    values, param table 8).
16. **Common param exposure** — Output Level (0–100 per delay), Smear per-machine
    (currently chain-level), per-delay Pan for Delay 2.
17. **Repeat Dynamics scoping** — currently global; manual scopes it to Digital.
    Decide keep-as-superset or gate per machine.
18. **DelayStyle enum hygiene** — non-MX extras Shimmer(4) and Rhythm(7) interleaved
    with MX machines; keep a stable MX↔FTS mapping table (see §5.1) for preset import.

### ✅ Verification passes (dial-in to confirm, no known gaps)

19. **dTAPE** — MX/Classic voices (record level vs bias), Tape Age, Low Contour,
    Crinkle-tracks-speed, Wow & Flutter, TapeSpeed, self-limiting saturation.
20. **dBUCKET** — variable clock (time-change survival), time-dependent Bucket Loss,
    filter bandpass/peaking at max, 2 voices via stage count, mono-till-modulated stereo.
21. **ICE** — 30-interval menu, slice scaling with delay time, pre-feedback Blend,
    regen re-shift ladder.
22. **LO FI** — bit 4–32, SR reduction, LoFi Mix, dVinyl dynamic/static halves, 8
    filter shapes, grit-before-S/H aliasing, 2 ms minimum time.
23. **Common** — tap divisions incl. Golden/Silver/Free, Swell mix-dependent behavior,
    High Pass, Freeze/Infinite, per-machine time ranges.

---

## Status update — 2026-07-27 implementation pass

Landed in delay-dsp/signal-fx (see git log for details):

- **REVERB machine** (queue #1): `DelayStyle::Reverb` (index 13) — pre-delay TIME,
  decay REPEATS (0.15 s → 40 s, infinite at max), FILTER = regen bandwidth, GRIT =
  distortion in+out, Mod = wet tremolo. Compact diffuser→Householder-FDN core.
- **REVERSE rebuilt** (#2): the old cycler read at a frozen absolute position (not
  reversed at all); now true −1× reads, onset-synced windows (performance-repeatable),
  Smear (allpass diffusion), Mod. 
- **DIGITAL voices** (#3): 24/96 / ADM (real 1-bit adaptive-delta codec @8×, error
  grows with frequency) / 12-Bit (µ-law compand + darkening) / Classic (rounder +
  morphing FILTER full-bw→analog→tape), all in-loop; plus rack-style Mod.
- **DRUM filter + grit** (#4): record-path head-alignment lowpass + soft-clip drive.
- **OIL CAN filter + mod** (#5): 300–12 kHz travel morphing to resonant low-thinned
  bandpass below ~1.2 kHz, filter moved to record path (murk at Repeats 0); Mod Speed
  control with spring-loaded (slow-then-accelerate) wow phase.
- **Common Mod layer** (#6): LoFi + Ice gained delay-line Mod (Spectral deferred to its
  density rework).
- **Plumbing** (#9/#10): Drum spacing + MultiTap grid/Classic recall are engine-level
  and survive `update()`; Triplet positions confirmed correct against the manual.
- **Exposure** (#13/#16 partial): DELAY_PARAMS ids 38–58 — common Mod, Reverse Smear,
  Digital morph, Duck Sens/Release at spec ranges, Drum spacing/Lo Cut, Oil Can heads,
  Output Level, full Filter-machine surface, MultiTap pattern/fb-mode/grid.

Still open: #7 Param1/2 assignment decision, #11 Filter SVF types + attack-sync
polarity, #12 Spectral FFT character + grains-per-beat density, #14 dual-B machine
params (delay side), #15 LoFi absolute-Hz mapping, #17 Repeat-Dynamics scoping,
#18 enum-order note, and the per-machine A/B dial-in passes (#19–23).

## Status update — 2026-07-27 second pass

- **Spectral**: synced Density is the 15-step 1/1→1/32 menu (off-grid ratios reachable;
  intermediate ratios marked for hardware dial-in), free-mode spacing randomizes ±50%,
  16 grain voices. FFT character remains a documented interpretation. (No Mod on this
  machine — SPEED/DEPTH are Density/Stretch.)
- **Filter machine**: −Square added (11 modes, CC order), ± shapes attack-sync their
  polarity ('+' = highest frequency at the input onset), trem waveform exposed (id 59).
- **Lo-Fi**: Sample Rate = 21-step absolute-Hz menu (id 31), host-rate correct.
- **Decisions**: Repeat Dynamics stays a superset (all machines); Param 1/2 assignment
  is declared equivalent to the FTS param surface (controller-mapping concern).
