# Strymon BigSky (CLASSIC) — reverb machine reference

Target: the **classic BigSky pedal** as emulated by the **Strymon BigSky plug-in
(VST3)** — not the BigSky MX. Companion docs: `bigsky-mx-reference.md`,
`bigsky-mx-parity.md` (MX manual, exhaustively). This file records what the
classic sources say, with emphasis on what is **new or more precise** than the
MX docs, and on facts that bear on our measured laws for Cloud.

Generated 2026-09-30.

## Sources

| Tag | Source |
|---|---|
| **[C]** | Strymon *BigSky User Manual* Rev D (05.24.19), 25 pp — https://www.strymon.net/manuals/BigSky_UserManual_RevD.pdf (linked from https://www.strymon.net/support/bigsky/). Page refs = printed "pg N". |
| **[P]** | Strymon *BigSky Plug-In User Manual* Rev A (10.18.2022), 48 pp — https://www.strymon.net/manuals/BigSky_Plugin_UserManual_RevA.pdf. **The doc for the exact product we are measuring.** |
| **[MX]** | *BigSky MX User Manual* RevB (Strymon's, not kept in this repo — from strymon.net/support), printed page numbers. |
| **[CB]** | Strymon *cloudburst User Manual* Rev D — https://www.strymon.net/manuals/cloudburst_UserManual_RevD.pdf (Cloudburst = expanded BigSky Cloud algorithm). |
| **[AMB]** | "Strymon – The Cloudburst Interview" (Pete Celi, Dean Miller), Amber Technology blog — https://blog.ambertech.com.au/amberblog/strymon-cloudburst-interview |
| **[PROD]** | Strymon BigSky product copy — https://www.strymon.net/product/bigsky/ and dealer reproduction https://www.guitareffectspedals.com/Strymon-BigSky-Multidimensional-Reverb-p1263.html |
| **[PG]** | Premier Guitar review — https://www.premierguitar.com/gear/reviews/strymon-big-sky-review |

Not reachable: the Gearspace thread "BigSky Cloud Machine Emulation" (HTTP 403).
The KVR and strymon.net Pete Celi interviews contain no algorithm detail.

---

## 1. Platform facts (classic pedal + plug-in)

### 1.1 Machines

Classic has **12 machines**: Room, Hall, Plate, Spring, **Swell**, Bloom, Cloud,
Chorale, Shimmer, Magneto, Nonlinear, **Reflections** [C pg 4, 8–19; P pg 22].
MX dropped Swell (folded into Hall's Swell Rise/Type) and Reflections, and added
Chamber and Impulse [MX].

### 1.2 Hardware / DSP

- 366 MHz SIMD SHARC, 2.4 GFLOPS peak, 32-bit float; 24-bit/96 kHz converters
  [PROD]. (MX: 800 MHz tri-core ARM.)
- Strymon says BigSky combines feedback-delay networks, allpass-delay-filter
  loops, Schroeder reverb sections and multi-tapped delay lines, plus new
  in-house elements [PROD dealer copy].
- Max input +8 dBu (classic) [C pg 24], versus +10 dBu on MX. S/N 115 dB
  typical (50 % wet), 120 dB at 100 % dry [PROD].
- **Analog dry path.** The dry signal is never digitized, and the mix happens in
  analog [C pg 24; PROD].
- Plug-in: runs at 44.1–192 kHz [P pg 3]. Header INPUT and OUTPUT trims are
  each **±36 dB**, with green/red signal/overload LEDs [P pg 11]. BYPASS passes
  only the dry signal [P pg 12].

### 1.3 Decay display = tank RT60. Per-machine ranges

[C pg 4] states that the displayed Decay is **RT60** (the time to fall 60 dB).
[P pg 18] has a typo here ("drop to 0dB").

| Machine | Pedal range [C pg 4] | Plug-in range [P] |
|---|---|---|
| Hall, Plate, Swell, Bloom, Chorale, Shimmer | 500 ms – "+20.00 s" | 500 ms – 20 s |
| Room | 200 ms – "+20.00 s" | 200 ms – 20 s |
| Spring | 800 ms – 10.00 s | 800 ms – 10 s |
| **Cloud** | **1.00 s – "+50.00 s"** | **1 s – 50 s** [P pg 23] |
| Magneto (delay time of last head) | 200 ms – 1.5 s (manual prints "1.50mS", a typo) | 200 ms – 1.5 s [P pg 29] |
| Nonlinear (time of nonlinear part) | **50 ms** – 2.00 s | **500 ms** – 2 s [P pg 31] (conflicts with the pedal value and with its own tip about "<100 ms" slapback [P pg 33]; treat 50 ms as likely correct) |
| Reflections (room size) | 133 ms – 400 ms | 133 ms – 400 ms [P pg 35] |

The leading "+" on the top values ("+20.00S", "+50.00S") suggests the display
saturates and the very top of the knob may go longer than the label. This is
not stated; verify by measurement.

**Cloud caveat:** the cascaded input diffusion makes the **overall** reverb
longer than the displayed tank RT60, most noticeably at low Decay settings
[C pg 14; P pg 24; MX pg 34]. So a measured RT60 near Decay = 1 s will read
longer than 1 s by design.

### 1.4 Knob semantics and plug-in value ranges

In the plug-in, every machine shows **PRE-DELAY 0–127, TONE 0–127, MOD 0–127,
MIX 0–127, LOW END −10…+10**. Decay is shown in time units [P pg 23–47].

| Control | Documented behaviour |
|---|---|
| PRE-DELAY | **0 to 1.5 s** [C pg 2; P pg 18]. This matches our ~1500 ms at 127. The curve is not documented. In Magneto and Nonlinear this knob is Feedback instead. With MIDI Clock ON, clock drives Pre-Delay (Decay on Magneto/Nonlinear) [C pg 7]. |
| TONE | "Adjusts the high end content of the reverb"; low = darker/warmer, high = bright/crisp; 12:00 = "nicely balanced" top end [C pg 2; P pg 19]. Plate tip: Tone runs from **unfiltered full bandwidth (max)** through warm (noon) to dark/damped (min) [C pg 10]. Exceptions: Bloom Tone is a **resonant, synth-voiced filter** [C pg 13]. Reflections Tone is relabelled **DAMPING** (absorptive surfaces) [P pg 34]. |
| MOD | Generic: low settings modulate the delay lines lightly, high settings add stronger modulation [C pg 2]. **The scheme is machine-specific** (§2). |
| LOW END | Classic MIDI range **0–20** for every machine [C pg 23]; the plug-in shows **−10…+10** with 0 = centre [P]. Wording differs by machine: Room/Hall/Reflections say "low frequency content **and decay profile**" (i.e. low-band decay time, in-loop). Cloud/Bloom/Swell/Shimmer/Spring/Plate/Nonlinear say "low frequency content" only [C pg 8–19]. **The "−/+" glyph on Room and Reflections is printed reversed ("+ ‖ −")** [C pg 8, 19]; Hall prints "− ‖ +" [C pg 9]. That is probably a layout artifact; verify knob direction on those two. |
| MIX | "100% dry at minimum to 100% wet at maximum. **50/50 mix occurs when the Mix knob is set to 3:00**" [C pg 2; P pg 19]. On a 7:00–5:00 pot, 3:00 is ≈ 80 % of travel, ≈ **102 / 127**. Mix runs in the **analog** domain [PROD]. See §1.5 for what this means for the mix law. |
| PARAM 1/2 | Assignable to any menu parameter of the current machine; example given: DIFUSN on Cloud [C pg 2, 4]. |

### 1.5 Mix law, Kill Dry, send use (refines "MIX = wet level")

- Pedal global **Dry Signal: NORMAL / KILL**. KILL mutes the dry signal so that
  MIX acts as an effect (wet) level [C pg 21].
- Plug-in: there is **no kill-dry switch**. Instead, a **lock icon** on MIX
  holds the mix value across preset changes. Strymon gives this for send use
  "when its output will normally be 100% wet" [P pg 19].
- **Best documented hint about the curve shape:** Cloudburst, which shares the
  Cloud lineage and the same "50/50 at 3:00" wording [CB pg 4], has a
  *Digital* dry mode that "allows the MIX knob to **dial out the dry signal when
  turned past the 3 o'clock position**" [CB pg 10]. Read together with
  "50/50 at 3:00", this suggests a Strymon mix law where:
  - **dry stays at (or near) unity from 0 up to ~3:00 (≈102/127)**, then fades
    to zero at max;
  - **wet rises from 0 to unity over 0 → ~3:00**.

  This fits our measurement that plug-in MIX behaves as a **wet level** over
  most of its travel. **Measure dry level at MIX = 102…127** to confirm the
  dry fade-out segment.

### 1.6 Hold: Infinite / Freeze, Persist, Spillover, Boost

- **Hold** per preset [C pg 7]:
  - **INFNTE**: reverb sustains forever, and each new note keeps **adding** to it.
  - **FREEZE**: sustains forever; new notes play **on top** without entering the
    reverb.
  - Classic has **no "Off"** option; MX added one. MIDI CC 70 selects
    Freeze/Infinite (0–1), CC 97 is the press/hold switch [C pg 23].
- Plug-in: an INFINITE/FREEZE selector plus a **HOLD** button that holds the
  current audio input to the reverb [P pg 20].
- **Persist** (per preset): trails continue after bypass; forces analog
  buffered bypass [C pg 7].
- **Spillover** (global): trails carry into the next preset; the preset must
  have been active for ≥ 5 s, because of the reverb buffer architecture
  [C pg 21].
- **Boost** ±3 dB per preset, analog (CC 23, 0–60) [C pg 7, 23].
- Classic has **no** per-reverb Output Level, Pan or dual-reverb routing
  (all MX-only).

---

## 2. Modulation schemes per machine (the most useful new detail)

The classic and plug-in manuals spell out the Mod knob much more precisely than
the MX manual. For Room and Hall the MX manual is vaguer: it says only "subtle
movement at low settings or more overt…" [MX pg 23].

| Machine | What MOD does |
|---|---|
| **Cloud** | **Min → 2:00:** sets the **amount** of modulation applied to the **input diffusor sections**. The modulation comes from a **quadrature oscillator** (sin/cos pair) "at a frequency harmonious to the Cloud generator". **Past 2:00:** the **frequency** of the quadrature oscillators rises. The aim is heavy modulation "without muddying up the sustaining reverberation tail", which implies the **tank itself is lightly modulated or not at all** [C pg 14; P pg 24; MX pg 34]. 2:00 ≈ 70 % of travel ≈ **89 / 127**. The MX manual uses identical wording, so this behaviour is the same on classic. |
| Room | **First half** of travel: **random** modulation of the reverb's delay lengths. **Second half:** in addition, modulation is applied to the **input of the tank** [C pg 8; P pg 37]. |
| Hall | Same two-stage scheme as Room. **Mod at 12:00 gives the maximum random delay-length modulation** [C pg 9; P pg 39]. |
| Plate | A "special LFO" modulates the delay lengths, giving lush modulation "without warble" [C pg 10]. |
| Swell | **4-phase** modulation of the reverb's delay lines [C pg 12]. |
| Bloom | Two independent **16-phase** oscillators (32 signals in total). One modulates the bloom-generator delay lines, the other the tank delay lines [C pg 13]. |
| Shimmer | A **4-phase** oscillator modulates the shimmer voices **and** the tank delay-line lengths [C pg 16]. |
| Chorale | Randomizes the choir's **pitch and timbre**, so more Mod gives more distinct singers [C pg 15]. |
| Magneto | Mod is a **Wow & Flutter** generator. The plug-in labels the knob WOW AND FLUTTER [C pg 17; P pg 29–30]. |
| Nonlinear | Mod varies the nonlinear-generator **tap lengths** and the late reverb's delay lines. A separate **Mod Speed** parameter (0–17) sets the LFO speed for both [C pg 18]. |
| Reflections | Mod modulates the **Pre-Delay time**, giving a randomized chorus against the dry signal. The plug-in labels it PRE-DELAY MOD [C pg 19; P pg 34]. |

---

## 3. Cloud — in detail

**Description.** A big ambient reverb that "draws from techniques developed in
the late '70s" [C pg 14].

**What "late '70s" means (Strymon, Pete Celi) [AMB]:** it refers to the
structure of **interconnected loops of all-pass filters and delays** developed
by **David Griesinger at Lexicon** in the 1970s. Cloud is **not** modelled on any
particular Lexicon unit. In other words, it is a Griesinger/Lexicon-style
figure-8 or ring tank with allpasses embedded in the recirculating loop (the
Dattorro "plate"-class topology). This is architecturally close to CloudSeed's
late-line-with-allpass-diffusers design.

**Structure, per the manual [C pg 14; P pg 23–24]:**

1. **Cascaded input diffusion blocks** in front of the tank build an expanded
   "early" reverb. Overall decay therefore exceeds the displayed tank RT60,
   especially at low Decay.
2. **Diffusion** places diffusors **in front of *and within*** the reverb
   generator. One parameter drives both the input-diffuser and in-tank allpass
   amounts.
   - **At minimum there is no diffusion effect**, and transients sound "grainy"
     (discrete echoes and taps audible).
   - Raising Diffusion smooths and softens the sound.
   - Range: CC 37, 0–20 (21 steps) [C pg 23].
3. **Modulation** targets the input diffusors through a quadrature LFO (§2).
4. **Low End** adjusts low-frequency *content* only (CC 38, 0–20; plug-in
   −10…+10). It does **not** say "decay profile" here, unlike Room and Hall.
   That is consistent with our measured **static 1st-order HPF** (≈550 Hz →
   84 Hz). It also suggests the control is an output or input filter, not an
   in-loop low-band decay multiplier.
5. **Decay** = tank RT60, 1–50 s (§1.3).
6. **Tone** has only the generic description. Our measured static LPF
   (~930 Hz → 8 kHz) is consistent with it. Whether 127 is truly unfiltered
   (the Plate tip says max = full bandwidth) is **not stated for Cloud**; the
   8 kHz ceiling may be built into the algorithm.

**Factory knob assignment.** The classic manual uses DIFUSN on Cloud as its
PARAM-knob example [C pg 4]. MX documents PARAM 1 = Low End.

**Classic vs MX Cloud.** The MX menu is Ensemble, Diffusion, Output Level, Pan
(plus the common parameters). The tips text is word-for-word identical to
classic [MX pg 34].

- **Ensemble does not exist on classic**, so the plug-in has no Ensemble.
  Ensemble came from Cloudburst; Dean Miller says it was designed separately as
  a harmonic-content enhancer, cheap enough to run alongside the reverb, and it
  does not track pitch [AMB].
- Classic's line "turn any modest guitar or synth sound into a gorgeous
  ensemble" [C pg 14] refers to the diffusion + modulation character, **not**
  to the MX Ensemble generator.
- Cloudburst's version was modified to cover **small and short** reverbs as
  well as huge ones [AMB]. Its Decay goes "from very short to over 50 seconds"
  [CB pg 3], whereas classic Cloud has a 1 s floor.

---

## 4. Other machines (classic; parameters and ranges)

MIDI value ranges are from [C pg 23] and give the number of steps. Low End is
0–20 on every machine (plug-in −10…+10).

### Room [C pg 8; P pg 36–37]

- **Parameters:** Low End (CC 61); **Size** Studio/Club (CC 59); **Diffusion**
  0–20 (CC 58) softens the early reflections, thickening the attack ("felt more
  than heard").
- **Low End:** turning it up lets more lows reverberate, giving longer apparent
  decay and a "sparsely furnished" room.
- **Size:** the early reflections and decay profile change with it.
- **Mod:** two-stage (§2).
- **Tips:** realistic use = minimum Pre-Delay with Decay 0.5–2 s. Atmospheric =
  Pre-Delay noon, Decay ≥ 12 s, Mod 1:00.
- **vs MX:** MX adds a Voice param. MX "Classic-Studio" (lower density, mild
  HF damping, lively and reflective) is the original [MX pg 23].

### Hall [C pg 9; P pg 38–39]

- **Parameters:** Low End (CC 39); **Mid** 0–20 (CC 42; halfway = flat; plug-in
  "MID EQ", 0 = flat, i.e. bipolar); **Size** Concert/Arena (CC 40).
- **Low End:** more lows reverberating, "fewer bass traps".
- **Mod:** noon = maximum random delay modulation.
- **Tips:**
  - Versatile setting: Concert, Decay ≈ 3.5 s, Tone noon, Low End centred.
  - Mega-structure: Arena, Decay ≥ 10 s, more Low End, Mix below noon.
- **vs MX:** MX adds Swell Rise/Type (from the classic Swell machine) and Voice.
  MX "Classic-Concert" = even early-reflection profile, generous low damping,
  soft diffusion. "Classic-Arena" = booming lows, slow buildup to maximum
  density [MX pg 25].

### Plate [C pg 10; P pg 40–41]

- **Parameters:** Low End (CC 69); **Size** Small (1½′×2¼′) / Large (4′×6′)
  (CC 68).
- **Low End:** wide range. Low = light, airy, won't colour the dry signal.
- **Tone:** max = unfiltered.
- **Size character:** Large is lush, smooth and transparent. Small is splashy
  and ringy with less low end.
- **Tips:** an undamped large plate decays in ≈ 5 s.
- **vs MX:** MX "Classic" voice = the original [MX pg 27].

### Spring [C pg 11; P pg 42–43]

- **Parameters:** Low End (CC 64); **Dwell** Clean/Combo/Tube/Overdrive
  (CC 63, 0–3); **# Springs** 1/2/3 (CC 62).
- **Dwell:** a preamp-drive model. **Hot input drives the spring harder**
  (level-dependent).
- **Low End:** the lower half reproduces the heavy LF cut of real spring
  circuits.
- **vs MX:** MX "Classic" voice = "lively spring with plenty of bounce and
  rattle" [MX].

### Swell (classic only) [C pg 12; P pg 44–45]

- **Parameters:** Low End (CC 65); **Rise Time** (CC 66, 0–22, 23 steps) with
  these exact values in seconds: **0.08, 0.10, 0.12, 0.14, 0.17, 0.20, 0.25,
  0.30, 0.35, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90, 1.00, 1.20, 1.40, 1.70, 2.00,
  2.50, 3.00, 4.00**; **Mode** SWLWET/SWLDRY (CC 67).
- **Mode:** Wet swells the reverb in behind the dry signal, like a volume pedal
  on the wet. Dry swells the dry signal into the reverb.
- **Mod:** 4-phase delay-line modulation.
- **vs MX:** MX Hall's Swell Rise also has 23 steps (0–22), but there 0 = no
  swell [MX].

### Bloom [C pg 13; P pg 46–47]

- **Structure:** a bloom-generating section (more diffusion blocks, '90s style)
  feeds a traditional tank.
- **Parameters:** **Length** 0–17 (CC 32) sets the bloom length. **Feedback**
  0–17 (CC 30) feeds back around the bloom section. Low End (CC 31).
- **Decay** sets the tank only. High Length or high Feedback makes the reverb
  much longer than the displayed decay.
- **Tone:** a resonant synth-voiced filter.
- **Mod:** 2 × 16-phase oscillators. High Feedback plus high Mod gives sweeping
  resonant harmonics.
- **vs MX:** MX adds Harmonics.

### Chorale [C pg 15; P pg 25–26]

- **Vowel** (CC 33, 0–6): **AAHHOO, AAHH, OOOHOH(?), AAHHOH, OOOO, OH,
  RANDOM**. Some names are column-scrambled in the text extraction; the
  formants are AH/OH/OO and two-formant combinations, and RANDOM picks any
  formant.
- **Resonance** (CC 34): Mild/Medium/High = formant filter Q.
- **Tone** adds "breath" and articulation. Mod = pitch/timbre randomization.
- Decay sets the size of the venue.
- **vs MX:** MX adds Choir, Choir Voice, etc.
- No Low End CC is listed for Chorale on classic.

### Shimmer [C pg 16; P pg 27–28]

- **Shift 1** (CC 25, 0–27, 28 values): −Oct, −M7, −m7, −M6, −m6, −P5, −TT,
  −P4, −M3, −m3, −M2, −m2, −10 cents, +10 cents, +m2, +M2, +m3, +M3, +P4, +TT,
  +P5, +m6, +M6, +m7, +M7, +Oct, +Oct&5th, +2 Oct.
- **Shift 2** (CC 26, 0–28): Off + the same list.
- **Amount** 0–18 (CC 27): level of the shifted voices, Off → full.
- **Mode** Input / Regen / In+Reg (CC 28).
- Low End (CC 24).
- **Character:** the voices are **created from the reverberated signal itself**
  [C pg 16; P pg 27].
- **Mod:** 4-phase, applied to the voices and the tank.
- **vs MX:** MX renames Mode → Feedback and adds Voice. MX's "Classic" shimmer
  voice = **time-domain, modulated-buffer pitch shifting** [MX pg 36]. That
  describes the classic/plug-in shifter (MX's own voice is frequency-domain).

### Magneto [C pg 17; P pg 29–30]

- **Knobs:** Decay = delay time of the **last** head (200 ms–1.5 s).
  Pre-Delay = Feedback.
- **Feedback source:** the last head with Even spacing; **the last two heads
  with Uneven** spacing.
- **Heads** **3 / 4 / 6** (CC 57, 0–2; MX adds 1 and 2, plus Ping Pong).
  Example: Even spacing with 3 heads at 300 ms gives taps at 100/200/300 ms.
- **Spacing:** Even/Uneven (CC 54).
- **Diffusion** 0–20 (CC 56) smears the heads into reverb.
- **Low End** (CC 55): low values roll off the bottom like tape machines.
- **Tone/EQ:** with feedback the EQ is regenerative (in the loop).
- **Mod** = wow and flutter.

### Nonlinear [C pg 18; P pg 31–33]

- **Knobs:** Decay = Time of the nonlinear part. Pre-Delay = Feedback around
  the nonlinear generator **before** the late reverb, giving repeating shapes.
- **Shape** (CC 46, 0–5):
  - Swoosh, Reverse, Ramp: "backwards" shapes with different slopes;
  - Gate: even amplitude, abrupt cut-off;
  - Gauss: bell curve;
  - Bounce: anti-bell.
- **Diffusion** (CC 45): 0 = grainy. High Diffusion with short Time sounds
  metallic.
- **Late Decay** 0–17 (CC 47); **Late Level** 0–18 (CC 48; minimum = late
  reverb off); **Mod Speed** 0–17 (CC 43); Low End (CC 44).
- **Tips:** Gate with short Time and no feedback is a level-independent gated
  reverb. Maximum feedback with Gate gives an endless multi-tap wash.
- **vs MX:** MX adds Chop (tremolo).

### Reflections (classic only) [C pg 19; P pg 34–35]

- **Model:** "psycho-acoustically accurate" small-space reverb computing **250
  reflections** from the source position in the chosen room shape.
- **Knob remaps:**
  - Decay = **room size**: 100 → 1000 sq ft (10×10 to 31×31 square, or 8×13 to
    24×39 rect/oblong), displayed as 133–400 ms;
  - Tone = DAMPING (absorptive surfaces);
  - Mod = pre-delay modulation (chorus against the dry signal).
- **Location Y** (CC 50, 0–6): front = dry dominates; back = reflections arrive
  at levels similar to the direct sound.
- **Location X** (CC 49, 0–6): **the dry signal is panned through the stereo
  analog buffers**, i.e. this machine alters the dry path.
- **Shape** (CC 51): Square, Rectangle (1.618:1, short and wide), Oblong
  (1:1.618, long and narrow).
- **Low End:** low-frequency decay profile.
- **Tips:** natural setting = Mix 12:00, Pre-Delay 0.

---

## 5. Common per-preset params, classic (MIDI) [C pg 7, 23]

| Param | CC | Range |
|---|---|---|
| Type | 19 | 0–11 |
| Decay | 17 | 0–127 |
| Pre-Delay | 18 | 0–127 |
| Mix | 15 | 0–127 |
| Tone | 3 | 0–127 |
| Mod | 14 | 0–127 |
| Param 1 | 9 | 0–127 |
| Param 2 | 16 | 0–127 |
| Boost | 23 | 0–60 (±3 dB) |
| Persist | 22 | 0–1 |
| Freeze/Infinite | 70 | 0–1 |
| Expression | 60 | 0–1 |
| MIDI Clock | 71 | 0–1 |
| Hold switch | 97 | 0/127 |
| Bypass | 102 | 0/127 |

Knob CCs are 0–127, which matches the plug-in's 0–127 displays. The **Decay
knob is 0–127** too, mapped onto the per-machine time range above (the law is
not documented).

---

## 6. Implications for our Cloud match (summary)

1. **Topology.**
   - Use a Griesinger-style loop of delays with **embedded allpasses**, fed by
     a **cascade of input allpass diffusers**.
   - **Diffusion** should scale *both* the input-diffuser and the in-loop
     allpass gains. At 0, both should be off, leaving a grainy, tappy attack.
2. **Mod.**
   - Use a quadrature (2-phase) LFO on the **input diffusers**, with little or
     no tank modulation.
   - Depth rises over 0 → ~89/127 at a fixed base rate. Above that, depth is
     held and the **rate rises**.
   - The base rate is tied to the generator's delay structure ("harmonious").
3. **Decay** (1–50 s) is the tank RT60. The measured total decay will exceed it
   because of the early cascade, so calibrate the tank RT60, not the
   broadband tail.
4. **Low End** is a static content filter on Cloud, not a decay profile. This
   supports the measured 1st-order HPF.
5. **Tone** is a static high-cut. Whether the top of its range is truly
   unfiltered is undocumented for Cloud.
6. **Mix.**
   - Our "wet level" model needs a check near the top of the range:
     Strymon's documented law puts 50/50 at 3:00 (≈102/127), and Cloudburst
     says the dry is dialed out only *past* 3:00.
   - The plug-in has no kill-dry; the MIX lock is the send-workflow aid.
7. **No Ensemble on classic.** Do not add an Ensemble layer when matching the
   plug-in.
