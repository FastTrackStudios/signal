# TimeLine MX — behavior reference

Target for delay-dsp machine parity. Sources: TimeLine MX User Manual RevA
+ Strymon official walkthrough video (Pete). This doc is authoritative where
the two disagree; transcript details refine the manual.

## Knob layout & param model

Global knobs: Time, Repeats, Mix, Filter, Grit, Speed, Depth (+ Value/Param
encoders). **Speed/Depth are assignable "Param 1 / Param 2" knobs** — default
to Mod Speed / Mod Depth on most machines, but any menu parameter can be
assigned per preset (dTape defaults: Crinkle→Speed, Wow&Flutter→Depth).

Per-preset (not per-delay): Boost (±3 dB analog), Persist (trails on bypass),
EP Set (expression assignment).

## Common per-delay parameters (every machine)

| Param | Range | Behavior notes |
|---|---|---|
| Mod Speed / Depth | 0–255 | LFO on the delay line |
| Tap Division | ♩, ♪., ♪, trip, 16th, Golden (1.618), Silver (2.414), Free | Free = ignores tap/MIDI clock |
| Pan | per delay | |
| Output Level | | |
| Swell | Off / 0.10–4.0 s | Input-triggered fade-in. **Mix < full: envelope on the wet signal behind dry. Mix = full: envelope on the dry signal INTO the delay** (volume-pedal-swell emulation). Display "7" = 700 ms |
| Duck Sens | 0–18 | envelope-driven wet reduction while playing |
| Duck Release | 0.05–1.00 s | return-to-full time |
| High Pass | Off, 20–900 Hz | post-delay, wet only |
| Infinite hold | footswitch hold | freeze repeats |
| Repeat Dynamics (Digital) | On/Off | "opposite of ducking": high-feedback tails cut off MORE abruptly instead of decaying linearly — level-dependent feedback trim |

Time ranges per machine: dTape/Digital/MultiTap/Spectral/Reverse/Ice/Filter
60–2500 ms; dBucket 80–800; Drum 200–2000; OilCan 200–800; LoFi **2**–2500
(2 ms minimum enables chorus/flange/realtime-lofi use); Reverb: Time =
pre-delay 2–2500, Repeats = decay (40+ s).

## Machines

### dTape — two voices
- **MX voice** (from EC-1): Grit = **Record Level** — drive into the tape,
  continuous saturation, punchy repeats.
- **Classic voice** (TimeLine v1): Grit = **Tape Bias** — bias eats headroom,
  more distortion as bias rises but WITHOUT the record-level punch. Different
  flavor, keep both.
- Filter = Tape Age (max bandwidth → old dull tape). Params: Low Contour
  (full low-end → progressive high-pass, major factor in tape voicing),
  Crinkle (tape damage crackle; tracks Tape Speed), Wow & Flutter (tracks
  Tape Speed), Tape Speed (Fast = higher fidelity/wider head response,
  Normal = warmer).
- High Repeats → pleasing harmonic saturation buildup (self-limiting, never
  harsh runaway).

### dBucket — two voices
- **MX voice** (from Brig), **Classic** (v1).
- **Variable-clock architecture: changing delay time changes read/write clock
  WITHOUT corrupting stored audio** — return to a time and the audio is
  intact. This is the defining dBucket behavior (vs digital crossfade).
- Grit = **Bucket Loss** (per-stage charge-transfer accuracy). Longer delay =
  slower clock = more degradation AND audible aliasing. Loss at min = clean.
- Filter = analog-voiced LP; at max it turns **bandpass/peaking** (perceived
  bandwidth extension trick before cutoff).
- MX voice, no modulation → wet is dual-mono/centered; modulation applies a
  **stereo** spread.

### Digital — four voices
- **24/96** (DIG): full-bandwidth pristine.
- **ADM**: 1-bit highly-oversampled adaptive delta modulation — error grows
  with frequency and amplitude discontinuity → signature **percussive attack
  emphasis** on transients.
- **12-bit**: low-res + companding + filtering → mellow 80s.
- **Classic** (v1). Params: Smear (diffusion), High Pass, Repeat Dynamics.

### Drum (Volante lineage)
- 1 record head, **4 playback heads** on a rotating drum.
- Head edit grid, 3 rows × 4 heads:
  - **Playback**: off / half (−6 dB) / full
  - **Feedback enable**: independent of playback — a head can feed back into
    the input while NOT routed to the output
  - **Pan** per head
- Repeats knob = master feedback scaler over all fb-enabled heads.
- **Spacing**: Even (16th-note spacing), Triplet, **Golden** (each head at
  1.618× the previous head's delay — densest non-overlapping repeats),
  **Silver** (spacing shrinks toward the end — bunched-up cadence).
- Feedback topology note: with head 1 fb ON and even spacing, panning
  collapses to center after the first pass (all heads get signal
  simultaneously); fb from ONLY the last head preserves the stereo pan
  rotation. This emergent behavior must fall out of the topology.
- Filter = high-end (head alignment fidelity), Lo Cut, Grit = distortion,
  wobble mod.

### Oil Can (Adineko/Tel-Ray lineage — "EL Vera")
- Electrostatic charge on rotating oiled disc, **no erase head**: charge
  decays with a time constant while being overwritten.
- **Ghost-echo cadence, even at Repeats = 0**: first echo at record→playback
  head distance; SUBSEQUENT echoes recur at the **disc rotation period**,
  which is LONGER than the first echo → off-kilter, non-grid cadence.
  Long/Short head selects the first-echo distance; the rotation-period echo
  time is identical for both. Both = long+short simultaneously (washes out
  fast with repeats).
- Very low bandwidth: Filter min = more bandwidth than real units (bonus
  range), 12:00 = realistic, max = extremely dark murky bandpass.
- Modulation = non-uniform disc rotation; real units sometimes spring-loaded
  → slow-as-it-fights-the-spring then accelerate wobble character.
- **Grit = rotation randomization / high-frequency speed uncertainty**
  (dirt via time jitter, NOT amplitude saturation).
- Musical use-case: long head, repeats 0, full bandwidth, mix low → sparse
  off-tempo air behind notes.

### MultiTap (expanded from v1 Pattern)
- **8 taps** on one delay line. Per tap: step position, level, pan,
  **per-tap repeats** (feedback contribution), **per-tap filter**
  (9 types: Off, LP gentle, LP peaking, HP, HP peaking, BP, BP peaking,
  Low Shelf, High Shelf) + cutoff, **per-tap modulation** amount.
- Master knobs (Time/Repeats/...) scale the whole pattern relative to the
  per-tap settings.
- **Grid**: 16th (4 subdivisions/beat, DAW-style step notation bar.sub),
  Triplet, or **Off** → free steps 1–256 across a 4-beat pattern
  (256 steps; 65 ≈ beat 2 downbeat).
- **Feedback mode**: `Input` (taps recirculate to the common input) vs
  `Parallel` (8 fully independent delay lines, no interaction, summed).
- **Pattern**: Custom or **Classic 1–16** (v1 TimeLine patterns baked in;
  Classic 1 = simple ping-pong; selecting a classic auto-sets grid/feedback).
- Recipe validation: 2 short hard-panned taps (no fb) + 2 quarter-note taps
  = stereo chorus into delay.

### Spectral (new, granular, frequency-domain)
- Grain scheduler over the delay buffer; **FFT-domain processing with a
  characteristic spectral sound even with all params neutral**; tonality
  evolves through the feedback path.
- **Grain Shape** (envelope): Soft (slow A/R), Swell (slow A, fast R),
  Soft Pluck (fast A, slow R), Pluck (very fast A), **Bounce** (fast A +
  randomized per-grain filter).
- **Spread**: random placement of grains across the delay time (0 = on-time).
- **Direction**: Forward / Reverse / Both (random per grain).
- **Octave**: probability/amount of random per-grain octave-up.
- **Density Sync On**: grains-per-beat ratios (1/1 … 8 per beat, incl.
  off-grid like 2/3). **Off**: free rate 6–250 ms per grain.
- **Stretch**: random per-grain time-stretch (overtly electronic).

### Reverse
- Input-triggered windows (predictable/musical). Smear, High Pass, mod.

### Ice
- Slices delay-buffer audio, plays back pitched. **Interval list**: −1 oct up
  in half-steps, ±50/25 cents micro-tunings, up to +1 oct, +oct&5th, +2 oct.
- Slice: Short/Medium/Long (scales with delay time). Blend: dry↔ice on the
  delay line (max = ice dominates). Regeneration re-shifts each pass
  (octave-up ladder).

### Lo Fi
- Sample Rate 750 Hz–96 kHz (aliasing "ring-mod" buzz), Bit Depth 4–32
  (truncates decay tails at low bits — noise-dominated LSBs), **LoFi Mix**
  (degraded vs clean blend — alternative to taming with Filter), **dVinyl**
  (dynamic = noise only with repeats / static = always-on), **Filter Shape
  bank**: Vintage Amp, Victrola, 70s Clock Radio, Bullhorn, Megaphone,
  Antique Telephone, Cell Phone, Intercom.
- Grit ↑ = more harmonics = MORE aliasing products at low sample rates
  (interaction is the point).
- 2 ms min time + high mix = realtime lofi box / chorus (10–12 ms) /
  flanger (shorter + repeats; max repeats oscillates).

### Filter (absorbs v1 Filter + Trem)
- Synced swept filter on repeats. **LFO shapes: +/-Triangle, +/-Sine,
  +Square, Saw, Ramp, Random(S&H), and Down / Up — Down/Up are
  ATTACK-TRIGGERED one-shot envelope sweeps (down or up in frequency once
  per detected attack, not cyclical)**. ± shapes set sync polarity to
  playing (+ = highest freq at attack, − = lowest).
- Speed: ratio of delay time, 1/32–32/1. Depth: sweep range around the
  Filter knob's center. Q: 0.5–10. Location: pre/post delay line.
- **Tremolo section**: own shape list, speed as delay-time ratio, depth
  0→full amplitude.
- Leslie recipe (validation target): parallel dual — LoFi at 27 ms +
  Doppler mod + Megaphone filter shape = horn; Filter delay at 60 ms,
  triangle LFO at 1/3 ratio, depth on corner freq = drum throb; horn/drum
  levels via the two Mix controls.

### Reverb (bonus machine)
- Time = pre-delay, Repeats = decay (small closet → 40+ s wash).
- Speed/Depth = **tremolo on the wet** (Flint-style). Filter = bandwidth in
  the regeneration. **Grit = distortion both INTO and AFTER the reverb**
  (lofi broken-verb character).

## Dual 1+2 (per preset)

- Routings: **Series (order swappable 1→2 or 2→1), Parallel, Split
  (delay1→L, delay2→R), Split-swapped**.
- Each delay: independent machine, full param set, own tap division
  (incl. Free to opt out of tap tempo).
- Spillover/persist across preset changes; both delays in one preset.

## I/O & platform (Signal rig-level, not delay-dsp)

- Input level instrument/line; IO configs: normal stereo, mono FX loop
  (delay1 → send/return → delay2), stereo FX loop (TRS), wet/dry,
  wet/dry/wet.
- Looper: 5 min stereo, pre/post delay position.
- 300 presets, MIDI CC/PC/clock, expression over any knob.
