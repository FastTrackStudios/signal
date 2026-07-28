# BigSky MX — 100% Parity Reference

**Source of truth:** Strymon *BigSky MX User Manual RevB* (83 pages), full-text
extraction of `features/fx/reverb/spec/BigSky_MX_UserManual_RevB.pdf`.
**Generated:** 2026-07-27.
**Companion doc:** the sibling `bigsky-mx-reference.md` holds the earlier
condensed behavior notes and gap analysis; this document is the exhaustive
checklist — every algorithm and parameter of the pedal, with exact value lists
from the manual, so parity can be dialed in one row at a time against
`features/fx/reverb/reverb-dsp` (param surface: `REVERB_PARAMS` ids 0–43 in
`features/fx/signal-fx/src/lib.rs`).

**Status legend**

| Mark | Meaning |
|---|---|
| ✅ | Implemented — behavior and value range reachable through our params |
| ⚠️ | Partial / approximated — behavior exists but not named, not full-range, or fudged |
| ❌ | Missing — not reachable through the param layer |
| N/A | Deliberately out of scope (rig-level equivalent exists by design) |

**Our param id map (REVERB_PARAMS ids 0–43):** 0 mix, 1 decay, 2 size,
3 routing, 4 algo_b, 5 decay_b, 6 mix_b, 7 pan_a, 8 pan_b, 9 trem_rate,
10 trem_depth, 11–16 imp_{decay,tail,attack,stretch,direction,feedback},
17–21 shim_{shift1,shift2,voice2,amount,fb_mode}, 22 mag_ping_pong,
23–28 nl_{chop_rate,chop_depth,gate_speed,late_speed,late_decay,late_level},
29 cloud_ensemble, 30 bloom_harmonics, 31–33 cho_{choir,voice,mod}, 34 voice,
35 hall_mid, 36–37 hall_swell_{rise,type}, 38 size_sel, 39 algorithm,
40 modulation, 41 damping, 42 tone, 43 predelay.

> **Structural gap that colors every table below:** all MX engine-specific
> params are wired to **chain A only** — slot B receives only `algo_b` /
> `decay_b` / `mix_b` / `pan_b` through signal-fx. Two *fully independent* MX
> engines per preset (the pedal's core dual-reverb promise, where every
> Reverb-Specific and Common parameter is per-slot with its own MIDI CC) is
> not reachable via params. Per-row Status marks refer to chain A.

---

## 1. Platform Structure

### 1.1 Signal path & converters (hardware context)

- 24-bit / 96 kHz A/D & D/A, 32-bit floating point processing, 800 MHz tri-core ARM.
- **Analog dry path** — the dry signal is never converted to digital (zero-latency dry).
- Stereo in/out; mono = LEFT only. When stereo output is in use the pedal is
  automatically forced to Buffered Bypass.
- +10 dBu max input; selectable Instrument/Line input sensitivity.
- True Bypass (relay) or Buffered Bypass; Buffered is auto-forced by: Persist,
  Cab Filter, Spillover, KillDry, stereo output.

### 1.2 Front-panel knobs

| Knob | Function (manual, verbatim behavior) | Our param | Status |
|---|---|---|---|
| DECAY | Decay time of the reverberated signal; range varies per reverb type. **Magneto: Delay Time (up to 1500 ms). NonLinear: Time of the nonlinear portion.** | id 1 `decay` (A), id 5 `decay_b` (B) | ⚠️ decay itself ✅ both slots; Magneto/NonLinear time-remap semantics ❌ at the param layer (see engine sections) |
| PRE-DELAY | Time between dry signal and reverb onset, **0 to 1.5 seconds**. **Magneto / NonLinear: Feedback amount.** | id 43 `predelay` (A only) | ⚠️ A only; no B pre-delay; Magneto/NonLinear feedback remap ❌ |
| TONE | High-end content of the reverb; low = darker/warmer, high = bright/crisp; 12 o'clock = balanced. (Bloom: resonant synth-voiced filter — see Bloom.) | id 42 `tone` (A only); id 41 `damping` is a second internal tone control | ⚠️ A only |
| MOD | Modulation depth on the reverberated signal; low = subtle/natural, high = stronger but tasteful. (Engine-specific schemes below.) | id 40 `modulation` (A only) | ⚠️ A only |
| MIX | Balance of analog dry and wet, 100% dry → 100% wet; 50/50 at 3 o'clock. KillDry global mutes dry entirely. | id 0 `mix` (A), id 6 `mix_b` (B) | ✅ per slot; KillDry ❌ (rig-level) |
| PARAM 1 / PARAM 2 | Assignable to PARAM-menu parameters of the current reverb type (see 1.3). | — | ❌ assignable-knob layer absent (params are addressed directly instead) |
| TYPE encoder | Select reverb type; push = dual 1/2 select + routing; hold = save. | id 39 `algorithm` (A), id 4 `algo_b` (B) | ✅ type select per slot; save/preset flow ❌ (rig-level) |
| VALUE encoder | Preset navigation / PARAM menu / GLOBAL menu. | — | N/A (UI concern) |

### 1.3 PARAM 1 / PARAM 2 assignment mechanism

- Each reverb type ships with **factory PARAM 1 & PARAM 2 assignments**.
  The RevB manual enumerates only one example: **Cloud PARAM 1 = Low End**.
  The per-engine factory defaults for the other 11 engines are *not* listed in
  the manual — capture them from hardware when dialing in (the PARAM-menu
  screenshots per engine show menu order, not knob assignment).
- Reassignment: enter PARAM menu → select parameter → push-**hold** VALUE
  while turning PARAM 1 (or 2) → "ASSIGNED".
- Assignments are stored **per reverb type** (not per preset).
- **Not assignable:** 1+2 EXP Setup, 1+2 Copy From, and the Impulse engine's
  Impulse (IR file select) parameter. Assigning any other 1+2 parameter makes
  the knob act on both reverbs simultaneously.
- MIDI: PARAM 1 knob = CC 19/20 (R1/R2), PARAM 2 knob = CC 21/22, value 0–127
  (i.e. the knob is a MIDI target independent of what it's assigned to).

**Status: ❌** — no assignable-knob indirection layer exists; our params are
addressed directly by id. Needed only if we want knob-surface parity (e.g. a
hardware controller mapping); DSP parity does not require it.

### 1.4 Footswitches

| Switch | Behavior | Status |
|---|---|---|
| A / B | Engage/bypass the current bank's A/B preset (Footswitch Mode **Preset**, default). In **Dual** mode: A enables/disables Reverb 1, B enables/disables Reverb 2 (B inert on single-reverb presets). | ❌ rig-level (bypass/preset switching lives outside reverb-dsp) |
| A+B | Bank down. B+INFINITE: bank up. | N/A (navigation) |
| INFINITE | Engages Infinite or Freeze per the Inf Mode common parameter; Momentary (default) or Latching per the Inf Latch 1+2 parameter. MIDI: CC 82 (press/release), CC 97 (Infinite Off/On). | ❌ see 1.5 |

### 1.5 Infinite / Freeze

Manual semantics (Inf Mode, per-reverb common parameter — values **Freeze /
Infinite / Off**):

- **Freeze** — play a note/chord, hold the switch: captures and sustains the
  reverb indefinitely; new playing is heard **on top** of the frozen signal
  without being added to the reverb.
- **Infinite** — hold the switch: reverb is applied indefinitely to **all**
  input while held (new input keeps entering the infinite tail).
- **Off** — disables the function.
- **Inf Latch** (1+2 param): Momentary (engaged only while held, default) or
  Latching (toggles on push-release).

**Status: ❌** — implementation has only a boolean freeze; no
Freeze-vs-Infinite-vs-Off three-way, no latch/momentary distinction, and it is
not exposed in `REVERB_PARAMS` at all.

### 1.6 Dual-reverb 1+2 system

Any preset can run two reverbs. Routing options (Dual Mode, selectable via
TYPE encoder or the 1+2 `DUAL` parameter, MIDI CC 99):

| Manual option | CC 99 value | Signal flow | Ours (id 3 `routing`) | Status |
|---|---|---|---|---|
| RV2 Off | 0 | Reverb 1 only | 0 Single | ✅ |
| Parallel | 1 | Input feeds both reverbs independently; outputs summed. Per-reverb Pan applies in stereo. | 3 Parallel | ✅ (enum order differs from CC values — mapping shim needed for MIDI parity) |
| Series 1▶▶2 | 2 | Input → Reverb 1 → Reverb 2 (like two pedals chained) | 1 Series12 | ✅ |
| Series 1◀◀2 | 3 | Input → Reverb 2 → Reverb 1 | 2 Series21 | ✅ |
| Split 1L\|2R | 4 | Reverb 1 mono → LEFT OUT only, Reverb 2 mono → RIGHT OUT only | 4 Split | ✅ |
| Split 1R\|2L | 5 | Same, outputs reversed | 5 SplitSwapped | ✅ |

Per-slot behavior (manual):

- **All individual Reverb Parameters are available and independently editable
  per reverb** — every Common + Reverb-Specific parameter exists twice, with
  distinct MIDI CCs (Reverb 1 CC / Reverb 2 CC columns in §5).
  **Status: ❌ for slot B** — only algo/decay/mix/pan reach chain B.
- Per-slot Pan (Common param) modifies stereo spread; disabled/centered on
  mono out and in Split modes. **Status: ✅** ids 7/8 (auto-center-on-mono ⚠️
  not modeled — rig concern).
- **Copy Settings From** (1+2 param): copy any preset's (00A–149B) Reverb 1
  type + settings into Reverb 2 of the current preset.
  **Status: ✅** `copy_params` in `dual.rs` (preset-library source ❌ rig-level).
- 1+2 Parameters act on both reverbs simultaneously (§4).
- MIX is adjusted per reverb to blend the two.

### 1.7 Presets, spillover, boost, kill dry (platform-level DSP behaviors)

| Feature | Manual behavior | Status |
|---|---|---|
| Presets | 150 banks × A/B = 300 locations; 100 factory presets in 00A–49B; bypass state saved with preset; save via TYPE-hold or MIDI PC while Save screen shown. | ❌ absent — rig-level concern (signal presets/rigs system owns this) |
| Spillover (global) | Wet decay of the current preset "spills" into the next preset. Requires the preset to have been active ≥ 5 seconds (reverb buffer architecture). **Not functional for the Impulse type.** Forces Buffered Bypass. Default Off. | ❌ absent — rig-level (preset-switch crossfade concern) |
| Persist (1+2, per preset) | Reverb trails continue when the effect is **bypassed** (On/Off; On forces Buffered Bypass). Distinct from Spillover (which is across preset changes). | ❌ absent — rig-level bypass concern |
| Boost (1+2, per preset) | Analog output level trim **−3.0 dB to +3.0 dB** (CC 79: 0–60, 30 = 0 dB). Level matching or solo boost. | ❌ absent — rig-level gain staging |
| Dry Signal / KillDry (global) | Normal: dry routed to outputs. KillDry: dry muted so MIX becomes a wet-only effect level (parallel loop / mixer send use). Forces Buffered Bypass. | ❌ absent — rig-level (wet-only routing is a rig mixer decision) |
| Input Level (global) | Instrument (default) / Line sensitivity. | N/A — rig-level gain staging |
| Cab Filter (global) | Off (default) / **Bright** / **Dark** / **Classic**. Bright & Dark are IR-based cab filters (for dark/bright sources respectively); Classic is derived from an analog cab-filter circuit with amp-like EQ + mic'd-speaker roll-off/notching. Forces Buffered Bypass; icon on Home screen. | N/A **by design** — the rig's NAM/cab stage covers this |
| Bypass (global) | True (relay) / Buffered. | N/A — rig-level |
| Expression pedal / EXP Setup | Per-preset heel/toe assignment of any knob(s), any direction; MIDI CC 100 (0 heel – 127 toe) drives the same assignments when MIDI EXP = ON. | ❌ absent — modulation-matrix / rig concern |
| Skipped as pure housekeeping | Active Banks, Home Screen views, Preset Nav, display/LED brightness, MIDI channel/CC/PC/THRU send config, EXP jack modes (Pedal/Bank/Preset/MIDI), MultiSwitch setup, factory reset flows. | — |

### 1.8 Our extensions beyond the pedal (not parity items)

- Wet tremolo on the shared A knob set: id 9 `trem_rate` (0.1–12 Hz), id 10
  `trem_depth` (chain.rs:887–898).
- Continuous `size` (id 2) alongside the pedal-style named `size_sel` (id 38).
- Separate `damping` (id 41) in addition to `tone`.
- Extra algorithms beyond the 12 BigSky engines: Swell, Reflections, Velvet,
  FreeVerb (AlgorithmType indices 10–13; Convolution = Impulse at 14).
- NonLinear Chop split into rate + depth, and explicit `nl_gate_speed`.

---

## 2. Common Reverb Parameters

Four parameters appear in the PARAM menu for **every** reverb type, applied
only to the currently-selected reverb (per-slot, own CCs). Manual note: for
Output Level, Pan, and Inf Mode, the current edited setting **persists when
changing the reverb type**.

| Parameter | Values (manual) | Description (manual, condensed) | MIDI CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Low End | slider | Low-frequency content **and decay profile**; higher = more low-frequency reverberation, impression of larger spaces. Behavior varies per reverb type (see engine tips). | 23 / 24, 0–20 | `low_decay_mult` / `band_crossover_hz` internals | ⚠️ approximated by low-band decay multiplier + crossover; not a first-class param id |
| Output Level | slider | Attenuates the **wet signal of the individual reverb only** — fine-tune the wet/dry balance per slot. | 7 / 8, 0–16 | per-slot `mix` / `mix_b` | ⚠️ approximated by per-slot mix; no dedicated wet-only attenuator distinct from mix |
| Pan | slider, L…R | Pans the wet signal 100% L to 100% R for stereo-spread shaping. Disabled + auto-centered on mono out and in Split L\|R / R\|L routing. | 9 / 10, 0–16 (0 = full left, 8 = center, 16 = full right) | id 7 `pan_a`, id 8 `pan_b` (−1..+1) | ✅ (auto-center-on-mono/split ⚠️ not modeled) |
| Infinite Mode | **Freeze / Infinite / Off** | See §1.5 for exact semantics. | 17 / 18, 0–2 (0 = Freeze, 1 = Infinite, 2 = Off) | boolean freeze only, no param id | ❌ |

---

## 3. Reverb-Specific Parameters — all 12 engines

Engine order below follows the TYPE encoder ring / MIDI Reverb Type values
(CC 1/2, value 0–11). Our AlgorithmType enum order differs (see §1.8) — a
type-value mapping shim is needed for MIDI parity.

> **Voice pairs implementation note (applies to Room/Hall/Plate/Spring/Shimmer):**
> Plate and Spring "MX vs Classic" genuinely map to two implementations
> (`plate.rs` vs `plate_lexicon.rs`; `spring.rs` vs `spring_vintage.rs`, via
> `apply_voice_pairing`, chain.rs:486–509). Hall/Room/Shimmer "Classic" is a
> hand-tuned parameter fudge on the same algorithm (chain.rs:461–478) —
> sparser diffusion, less tail motion, brighter voicing — not a second
> algorithm. Voice select = id 34 `voice` (0 MX / 1 Classic), chain A only.

### 3.1 Room

**Character (manual):** intimate environments from well-tuned studio ambience
to larger nightclub acoustics. Tone knob, Diffusion, and Low End adjust the
damping and scattering effects of room materials, furniture, and people.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Size | **Studio / Club** | Studio: well-tuned intimate studio environment. Club: larger, livelier "nightclub" experience. Character further depends on Voice. | 25 / 26, 0–1 | id 38 `size_sel` (named-size unification via `set_size_index`) | ✅ |
| Diffusion | slider | Softens the early reflections for a thicker, more diffused attack portion; often "felt" more than heard. Higher = more diffusion. | 27 / 28, 0–20 | internal `diffusion` field, no param id | ⚠️ exists internally, not exposed as a param |
| Voice | **MX / Classic** | MX-Studio: rich, dense, smooth, quickly-damped top for realism. MX-Club: lengthened internals, larger space, similar tone. Classic-Studio: lower density + mild high damping, lively/reflective. Classic-Club: lengthened internals of Classic-Studio. | 29 / 30, 0–1 | id 34 `voice` | ⚠️ Classic is a parameter fudge on the same algorithm |

**Behavioral facts ("Using the Room…"):** realistic spaces = minimal
Pre-Delay, Decay 500 ms–2 s; the algorithm remains stable and musical at very
long decays (12 s+ with Mod at 1 o'clock for atmospheres).

**Factory PARAM 1/2:** not enumerated in RevB manual.

### 3.2 Hall

**Character (manual):** diffused reflections and slower-building density.
Concert size is well-balanced/spacious/warm; Arena is huge, enveloping,
booming. Mid parameter gives precise EQ tailoring of the reverberated sound.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Midrange (Mid) | slider | Midrange content of the reverb; up = emphasize mids, down = reduce; halfway = flat. | 35 / 36, 0–20 | id 35 `hall_mid` (−6..+6 dB, chain.rs:531–540) | ✅ |
| Size | **Concert / Arena** | Concert: well-balanced, warm concert-hall auditorium. Arena: acoustics of the largest enclosed venues. Early-reflection buildup and late decay profile change accordingly; also depends on Voice. | 37 / 38, 0–1 | id 38 `size_sel` | ✅ |
| Swell Rise | slider | Rise time of the swell effect; longer swells at higher values; **0 = no swell**. | 39 / 40, 0–22 | id 36 `hall_swell_rise` (chain.rs:814–821) | ✅ |
| Swell Type | **Swell Wet / Swell Dry** | Wet: swells the reverberated signal in behind the dry. Dry: swells the dry signal into the reverb. | 41 / 42, 0–1 (0 = Wet, 1 = Dry) | id 37 `hall_swell_type` | ✅ (ours documents 0 wet / 1 wet+dry — verify the Dry-swell semantic matches "swells the dry signal into the reverb") |
| Voice | **MX / Classic** | MX-Concert: naturally rich, smooth ER buildup, minimal structural resonance. MX-Arena: spacious well-tuned large venue with a prominent late reflection off the back wall. Classic-Concert: even ER profile, generous low damping, soft diffusion. Classic-Arena: mega structure, booming low end, slow buildup to max density. | 43 / 44, 0–1 | id 34 `voice` | ⚠️ Classic = parameter fudge |

**Behavioral facts:** Pre-Delay increases sense of space/separation. Versatile
baseline: Concert, Decay ≈ 3.5 s, Tone noon, Low End centered. Huge spaces:
Arena, 10 s+, more Low End.

**Factory PARAM 1/2:** not enumerated in RevB manual.

### 3.3 Chamber

**Character (manual):** generous, dense, medium-room-sized reverberation with
excellent focus and clarity; the Color options capture the effect of speakers
and mics used in the chamber recording process.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Color | **Neutral / Clear / Smooth / Crisp / Deep** | Neutral: wide-range flat response, natural tone. Clear: reduced low end (avoids mud with bass-heavy sources). Smooth: reduced mid response ("smile" EQ). Crisp: high-passed, very bright. Deep: emphasized mids, vocal qualities. | 45 / 46, 0–4 (0 Neutral, 1 Clear, 2 Smooth, 3 Crisp, 4 Deep) | `room_chamber.rs` reuses `extra_a` as ER level | ❌ no named Color selector / tonal-profile bank |

**Behavioral facts:** positioned between Room and Hall in size; the five
Colors are fixed post-tonality profiles, not a continuous control.

**Factory PARAM 1/2:** not enumerated in RevB manual.

### 3.4 Plate

**Character (manual):** rich, fast-building reverb creating depth without
early-reflection cues to a specific environment; Tone + Low End as the
frequency-shaping tools.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Size | **Small / Large** | Small: representative of a "home project" plate. Large: a traditional studio plate. Character depends on Voice. | 47 / 48, 0–1 | id 38 `size_sel` | ✅ |
| Voice | **MX / Classic** | MX-Small: nicely balanced, smoother top; like an analog plate, **reduced headroom and subtle tube saturation**. MX-Large: like an ideal digital plate — immediate buildup to maximum density, minimal coloration. Classic-Small: splashy, ringy, reduced low end. Classic-Large: lush, smooth, less "zing" than MX. | 49 / 50, 0–1 | id 34 `voice` → `plate.rs` vs `plate_lexicon.rs` | ✅ genuine two-implementation pair (MX saturation nuance ⚠️ unverified) |

**Behavioral facts:** traditional undamped large-plate decay ≈ 5 s; short
1.5 s decays + low Mix for subtle ambience; Tone range = unfiltered
full-bandwidth (max) → warm (noon) → dark/damped (min); Low End deliberately
wide-ranged (plates were routinely post-EQ'd).

**Factory PARAM 1/2:** not enumerated in RevB manual.

### 3.5 Spring

**Character (manual):** the '60s surf / spaghetti-western spring tank — from
warm and mellow to splashy and dripping, via Tone, Mix, Dwell, and the
selectable number of springs.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Dwell | **Clean / Combo / Tube / Overdrive** | Drive of the spring-tank preamp circuit. Clean: cleanest spring tones. Combo: more gain, typical of combo amps with onboard spring. Tube: increased gain **and** harmonic content entering the tank (like turning up an outboard unit's Dwell). Overdrive: expanded preamp gain for maximum trashiness. | 51 / 52, 0–3 (0 Clean, 1 Combo, 2 Tube, 3 Overdrive) | implicit only — `spring.rs:211` maps decay→loop gain | ⚠️ no named 4-stage drive param; no preamp saturation staging |
| Number of Springs | **1 Spring / 2 Springs / 3 Springs** | 1–3 springs in the tank; more springs add complexity from the interaction of each spring's different delay times. | 53 / 54, 0–2 (0 One, 1 Two, 2 Three) | `spring_vintage.rs:288–291` maps `extra_b` → 1–3 springs | ⚠️ implicit (Classic voice only), not a named param |
| Voice | **MX / Classic** | MX: the quintessential spring tank, dripping with authenticity and splashy dynamic response. Classic: lively spring with plenty of bounce and rattle. | 55 / 56, 0–1 | id 34 `voice` → `spring.rs` vs `spring_vintage.rs` | ✅ genuine two-implementation pair |

**Behavioral facts:** combo-amp recipe = 2 Springs + Combo Dwell + Decay
≈ 4.5 s, Tone rolled back; hi-fi recipe = 3 Springs + Clean + ≈ 3 s;
max splash = Tone max + Low End min. **Hot input drives the springs harder**
(input-level-dependent drive). Low End ≤ 50% attenuates lows against
rumble/feedback; > 50% adds richness at Clean/Combo.

**Factory PARAM 1/2:** not enumerated in RevB manual.

### 3.6 Impulse

**Character (manual):** convolution reverb on Impulse Response files —
"mathematically perfect recreations of recorded spaces"; factory IRs included
(140 LONG/MED, 250 LONG, spring tanks, 480 HALL, 960 TAJ / OIL CAN, RMX
REVERSE, plus CC-licensed spaces: Warehouse, Reactor Hall, Maes Howe,
Mausoleum, York Minster, Coral Cave, K10 Spring, St Georges, Chapel, Elveden
Hall); custom IRs imported via Nixie 2 (mono/stereo WAV 16/24-bit 48 kHz,
conformed to stereo 24-bit 48 kHz).

**Reset-on-load rule (manual):** when a new Impulse is loaded, all controls
**except MIX** reset to: DECAY 100%, PRE-DELAY 0, TONE 50%, MOD 0, LOW END
50%, FEEDBACK 0, ATTACK 0, DIRECTION Forward, STRETCH 1.0, TAIL Envelope.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Impulse (select) | Factory folder (+ user folders) | Opens the Impulse file menu; pick an IR. Not PARAM-knob-assignable; **no MIDI CC exists** for it on the pedal either. | — | `load_ir_wav` (custom IR → Convolution engine) | ⚠️ custom IR loading ✅; factory IR library / folder browser ❌ (rig asset concern) |
| Attack | slider | Timing of the onset of the reverberation. | 57 / 58, 0–16 | id 13 `imp_attack` | ✅ (live re-prepare via `ImpulseReshaper` worker, ir/engine.rs + convolution.rs) |
| Stretch | slider | **Re-samples the IR**, changing both inherent decay time and frequency content; low = shorter decay, high = longer. | 59 / 60, 0–16 | id 14 `imp_stretch` (0.25–4.0) | ✅ |
| Tail | **Envelope / Gate** | Tail shaping applied when DECAY < 100% (at 100% no shape is applied). Envelope: decreasing ramp shortens the IR per DECAY. Gate: abruptly truncates the IR per DECAY. | 61 / 62, 0–1 (0 Envelope, 1 Gate) | id 12 `imp_tail` | ✅ |
| Direction | **Forward / Reverse** | Forward: standard decay. Reverse: backward reverb decay heard following the input. | 63 / 64, 0–1 (0 Forward, 1 Reverse) | id 15 `imp_direction` | ✅ |
| Feedback | slider | Wet signal fed back **into the Pre-Delay** for added reflections and ring; effect strongly depends on the Pre-Delay knob position. | 65 / 66, 0–15 | id 16 `imp_feedback` | ✅ (verify the pre-delay-loop topology vs plain wet feedback) |
| (Decay knob) | 0–100% | Scales the IR length via the Tail function. | 3 / 4 | id 11 `imp_decay` (0.01–1.0) | ✅ |

**Behavioral facts:** Spillover is **not functional** for Impulse (pedal
limitation, buffer architecture). Home screen shows IR name instead of decay
ms when Impulse is selected. Reset-on-load defaults not implemented (⚠️ —
policy decision for our preset layer).

**Factory PARAM 1/2:** not enumerated in RevB manual (menu order: Impulse,
Attack, Stretch, Tail, …).

### 3.7 Cloud

**Character (manual):** gorgeously big ambient reverb drawing from late-'70s
techniques; obscures the distinction between reality and fantasy.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Ensemble | slider | **Analyzes the input signal** and generates a harmonically rich pad reminiscent of a string section; min→mid = light/medium ensemble, high = pronounced, lush soundscapes. | 69 / 70, 0–15 | id 29 `cloud_ensemble` (cloud.rs:542–634) | ✅ |
| Diffusion | slider | Adds diffusors **in front of and within** the reverb generator. Min = no diffusion, "grainier" yet mesmerizing transient attacks; up = smoothed and softened. | 67 / 68, 0–20 | internal `diffusion` field, no param id | ⚠️ not exposed as a param |

**Behavioral facts (DSP-revealing):**

- The cascaded input diffusion blocks create an expanded "early" reverb —
  overall reverb time is **longer than the displayed tank decay**, most
  noticeable at low Decay values.
- MOD knob: from min to 2 o'clock, adjusts the amount of modulation
  (**developed by a quadrature oscillator at a frequency harmonious to the
  Cloud generator**) applied to the input diffusor sections. **Past 2 o'clock,
  the quadrature-oscillator frequency itself increases.** Scheme designed for
  high modulation depth without muddying the sustaining tail.

**Factory PARAM 1/2:** PARAM 1 = **Low End** (the manual's one documented
factory assignment). PARAM 2 not enumerated.

### 3.8 Shimmer

**Character (manual):** two tunable voices add pitch-shifted tones to the
reverberated signal; Amount + Feedback span laid-back subtle to full-blown
majestic splendor.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Shift 1 | 28 intervals, octave down → two octaves up (full list below) | First voice interval. | 71 / 72, 0–27 | id 17 `shim_shift1` (continuous semitones −12..+12) | ⚠️ −12..+12 semitone range covers 25 of 28; **missing + Octave + 5th (+19), + 2 Octaves (+24), and the ± 10 Cents detune steps** (continuous field can express ±0.1 semi, but no named detent) |
| Shift 2 | **Off** + the same 28 intervals | Second voice interval; Off if no second voice desired. | 72 / 73, 0–28 | id 18 `shim_shift2` + id 19 `shim_voice2` (0 single / 1 dual = Off switch) | ⚠️ same range gaps as Shift 1; Off ✅ via `shim_voice2` |
| Amount | slider | Level of the Shift 1+2 voices in the reverberated signal, off → maximum. Min = no shimmer at all. | 75 / 76, 0–18 | id 20 `shim_amount` | ✅ |
| Feedback | **Input / Regen / Input+Regen** | Input: shimmer applied at the reverb-core input, non-regenerating. Regen: shimmer applied **within** the reverb core, regenerative — continuously ascending/descending pitches as the reverb decays. Input+Regen: both. | 77 / 78, 0–2 (0 Input, 1 Regeneration, 2 Input + Regeneration) | id 21 `shim_fb_mode` (0 Input / 1 Regenerative / 2 InputPlusRegen) | ✅ |
| Voice | **MX / Classic** | MX: sophisticated clean pitch-shifting, consistent tracking across all intervals, frequency-domain techniques. Classic: lush time-domain shifting with modulated buffers. | 85 / 86, 0–1 | id 34 `voice` | ⚠️ Classic = parameter fudge on the same algorithm, not a time-domain second shifter |

**Shift 1 full interval list (verbatim, CC value order 0–27):**
− Octave, − Major 7, − Minor 7, − Major 6, − Minor 6, − Perfect 5, − Tritone,
− Perfect 4, − Major 3, − Minor 3, − Major 2, − Minor 2, − 10 Cents,
+ 10 Cents, + Minor 2, + Major 2, + Minor 3, + Major 3, + Perfect 4,
+ Tritone, + Perfect 5, + Minor 6, + Major 6, + Minor 7, + Major 7, + Octave,
+ Octave + 5th, + 2 Octaves.
**Shift 2:** value 0 = Off, then the same list shifted by one (1 = − Octave …
28 = + 2 Octaves).
*(Manual typos to be aware of: the CC table prints "8 = Major 3" for − Major 3,
and assigns Shift 1 = CC 71/72 while Shift 2 = CC 72/73 — the CC 72 collision
is an obvious manual error; verify actual CCs on hardware.)*

**Behavioral facts:** MOD knob modulates the shimmer voices **and** the reverb
tank's delay-line lengths with a **4-phase oscillator**. For deep octave-down
shifts, raise Low End so the lower octave comes through. Recipes: +Oct with
+Oct+5th at low Amount = hint of shimmer; −10¢/+10¢ with Feedback + Mod at
min = detuned reverb.

**Factory PARAM 1/2:** not enumerated in RevB manual.

### 3.9 Bloom

**Character (manual):** '90s-style heavily-diffused reverb whose envelope
"blooms"; a bloom-generating section feeds a traditional reverb tank, with a
Feedback parameter around the bloom stage.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Length | slider | Length of the "bloom" portion; higher = longer bloom times. (Decay knob independently controls the **tank** decay.) | 87 / 88, 0–17 | folded into decay/size of the Greyhole-style bloom core (bloom.rs) | ⚠️ no separate bloom-length param distinct from tank decay |
| Feedback | slider | Feedback applied **around the bloom portion** of the reverb. High Length and/or Feedback yield reverbs much longer than the displayed tank decay. | 89 / 90, 0–17 | decay→feedback-gain mapping inside bloom core | ⚠️ no distinct bloom-feedback param |
| Harmonics | slider | **Analyzes the input signal** and generates a harmonically rich pad reminiscent of an analog synth; min = none, mid = light, high = pronounced (synth drones layered on the decay). | 91 / 92, 0–15 | id 30 `bloom_harmonics` (bloom.rs:93–97) | ✅ |

**Behavioral facts (DSP-revealing):**

- MOD knob drives **two independent 16-phase oscillators (32 oscillator
  signals total)**: the first modulates the bloom-generating delay lines, the
  second modulates the tank reverb delay lines.
- TONE knob here is a **unique resonant filter** shaping the top end with
  synth-like voicing (not the standard damping tone). High Feedback + high
  Mod = sweeping resonant harmonics.

**Factory PARAM 1/2:** not enumerated in RevB manual.

### 3.10 Chorale

**Character (manual):** a vocal choir accompanies your music; vowel ranges and
intensities customize the choir; venue varies with Decay; Modulation brings
the choir alive with a multitude of voices.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Vowel | **AAHHOO / AAHH / AAHHOH / OH / OOOHOH / OOO / Random** | Vowel sound(s) the choir sings — traditional singing formants AH, OH, OO and combinations; Random allows any formant. | 93 / 94, 0–6 (in list order) | continuous `vowel_mix` 0 = "ah" → 1 = "oo" over a 4-vowel formant table (chorale.rs `VOWEL_FORMANTS`: ah/ee/oh/oo) | ⚠️ continuous vowel morph exists; the 7 named combination programs and Random are missing |
| Resonance | **Mild / Medium / High** | Intensity of the vowel sound via the vocal-filter **resonance (Q)** values. Mild: subtle vocal quality. Medium: increased intensity. High: most resonant. | 95 / 96, 0–2 (0 Mild, 1 Medium, 2 High) | fixed formant Q | ❌ no Q-intensity selector |
| Choir | slider | **Analyzes the input** and generates a vocal-choir pad; min = none, mid = light single voice, high = pronounced choir. | 103 / 104, 0–15 | id 31 `cho_choir` | ✅ |
| Choir Voice | **Tenor / Baritone** | Pitch range of the voices. Tenor: mid-to-high chorale range. Baritone: low chorale range. | 105 / 106, 0–1 (0 Tenor, 1 Baritone) | id 32 `cho_voice` (documented as 0 Tenor / 1 **Soprano**) | ⚠️ two ranges exist but our second voice is Soprano (up) where the pedal's is Baritone (down) — direction mismatch to resolve |

**Behavioral facts (DSP-revealing):** MOD adds **randomization to the
chorale's pitch and timbre** to create an increasing number of singers with
distinct voices (our id 33 `cho_mod` = per-voice randomization ✅). Vocal
formants are mid-range — at High Resonance a mid-heavy amp may over-reinforce
them (why Resonance staging matters). Tone adds "breath"/high-end
articulation.

**Factory PARAM 1/2:** not enumerated in RevB manual.

### 3.11 Magneto

**Character (manual):** more than a reverb — old-school multi-head tape delay,
slapback, and patterned repeats with **all heads on**; Diffusion smears the
heads, blurring the line between delay and reverb.

**Knob remap (manual, verbatim semantics):**

- **DECAY → DELAY TIME:** sets the delay time **of the last head, up to
  1500 ms**.
- **PRE-DELAY → FEEDBACK:** feedback from the **last head** back to the input
  with Even spacing; with Uneven spacing, feedback is taken from the **last
  two heads**.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Diffusion | slider | Diffusors on the magnetic heads; min = none; up = heads increasingly smeared, reverberated repeats. | 107 / 108, 0–20 | internal progressive per-head diffusion (later heads more diffuse), no param id | ⚠️ hardwired character, not a param |
| Heads | **1 / 2 / 3 / 4 / 6** | Number of "tape machine" heads. Fewer = simpler distinct repeats; more = dense complex patterns and washes. Also changes the Ping Pong L/R pattern. | 109 / 110, 0–4 (0 One, 1 Two, 2 Three, 3 Four, 4 Six) | `const NUM_HEADS: usize = 4` — fixed | ❌ no head-count selector (fixed 4) |
| Spacing | **Even / Uneven** | Even: heads equidistant → equal delay times. Uneven: more complex, less overtly rhythmic effects. Also changes the feedback tap (last head vs last two). | 111 / 112, 0–1 (0 Even, 1 Uneven) | size→head-spacing continuous mapping | ❌ no Even/Uneven mode (and no feedback-tap switch) |
| Ping Pong | **Off / On** | Repeats alternate between Left and Right outs. With 1–2 heads: strict L-R-L. With 3/4/6 heads the pattern alternates differently per head count (e.g. L-R-R). Auto-centered to mono when only LEFT OUT is connected. | 113 / 114, 0–1 | id 22 `mag_ping_pong` (odd heads L, even heads R, hard-panned) | ✅ core behavior (per-head-count pattern variants ⚠️ untested; mono auto-center rig-level) |

**Behavioral facts (DSP-revealing):**

- The **last repeat lands exactly at the displayed Decay (delay-time) value**;
  heads are fractions of it: 300 ms with 3 even heads → repeats at
  100/200/300 ms; 4 heads → 75/150/225/300 ms.
- As Feedback rises, the **EQ response is regenerative** (Tone/Low End inside
  the loop) → evolving soundscapes and ambient washes.
- MOD = **Wow and Flutter generator** (tape-speed modulation, not chorus).
- Tone + Low End are deliberately wide-ranging (tape machines vary bright/
  dark/high-passed/full).

**Ours additionally:** `extra_a` = tape saturation (magneto.rs:135–136),
decay→feedback mapped internally (magneto.rs) — but the **param-layer knob
remap (Decay = time, Pre-Delay = feedback) is not done** (❌): id 1 still
drives "decay" semantics and id 43 stays pre-delay.

**Factory PARAM 1/2:** not enumerated in RevB manual.

### 3.12 NonLinear

**Character (manual):** physics-defying reverb shapes for special effects —
three "backward" shapes, a Gate, tremolo via Chop, plus a separate late-reverb
stage.

**Knob remap (manual, verbatim semantics):**

- **DECAY → TIME:** sets the time of the **nonlinear portion** of the reverb.
- **PRE-DELAY → FEEDBACK:** feedback from the nonlinear portion **back to the
  input** (before it enters the late reverb) → repeating nonlinear shapes as
  the knob rises; max Feedback + Gate = nearly endless "multi-tapped" wash.

| Parameter | Values (verbatim) | Description (manual) | CC (R1/R2, range) | Ours | Status |
|---|---|---|---|---|---|
| Shape | **Swoosh / Reverse / Ramp / Gate / Gauss / Bounce** | Swoosh, Reverse, Ramp: "backward" effects with different slope profiles. Gate: even amplitude profile with abrupt cut-off. Gauss: "bell curve" profile. Bounce: inverted bell. | 115 / 116, 0–5 (in list order) | 4 shapes (Reverse/Gate/Swoosh/Ramp) selected by `extra_a` thresholds (nonlinear.rs:148–153) | ⚠️ 4 of 6 shapes; **Gauss and Bounce missing**; no named selector param |
| Chop | slider | Amplitude modulation on the reverb decay for tremolo effects; min = none, right = increasingly **faster** tremolo patterns. | 117 / 118, 0–17 | ids 23/24 `nl_chop_rate` (0.1–15 Hz) + `nl_chop_depth` | ✅ (richer than the pedal's single knob — one-knob macro needed for CC parity) |
| Diffusion | slider | Diffusors on the nonlinear generator; min = "grainy"; up = smeared and smoothed. High Diffusion + short Decay can sound "metallic". | 119 / 120, 0–20 | internal diffuser feedback tied to `params.diffusion` (nonlinear.rs:163–164), no param id | ⚠️ not exposed |
| Decay | slider | Decay time of the **late reverb** stage. | 121 / 122, 0–17 | id 27 `nl_late_decay` | ✅ |
| Level | slider | Level of the late reverb; min = late reverb **off**. | 123 / 124, 0–18 | id 28 `nl_late_level` | ✅ |
| Mod Speed | slider | LFO speeds for **both** the nonlinear delay-tap lengths and the late reverb's delay lines. | 125 / 126, 0–17 | id 26 `nl_late_speed` (+ id 25 `nl_gate_speed` extension) | ⚠️ late-line speed ✅; unified taps+late speed semantic to verify |

**Behavioral facts (DSP-revealing):** the nonlinear generator **feeds into**
the late reverb (series topology). Gate + short decay + no feedback =
level-independent gated reverb. Swoosh/Reverse at < 100 ms decay + min
Diffusion = slapback. Recipe: Gate + max Feedback, Decay 800 ms, Mod at 10:00.

**Ours additionally:** the Pre-Delay→feedback-around-generator path does not
exist (❌ — no `nl` feedback param, id 43 remains plain pre-delay).

**Factory PARAM 1/2:** not enumerated in RevB manual.

---

## 4. 1+2 Parameters (complete)

Shared parameters at the end of every PARAM menu, applied simultaneously to
both Reverb 1 and Reverb 2 (to Reverb 1 alone when Reverb 2 is Off). Stored
per preset.

| 1+2 Parameter | Values (verbatim) | Description (manual) | MIDI CC | Ours | Status |
|---|---|---|---|---|---|
| Infinite Latch | **Momentary / Latching** | INFINITE footswitch behavior: Momentary = engaged only while held (default); Latching = toggles on push-release. | 98 (0 = Momentary, 1–127 = Latching) | — | ❌ (no infinite subsystem, §1.5) |
| Boost | slider, **−3.0 dB … +3.0 dB** | Preset output level trim, for level matching or solo boost. | 79 (0–60; 0 = −3 dB, 30 = 0 dB, 60 = +3 dB) | — | ❌ rig-level gain staging |
| Persist | **On / Off** | Reverb trails continue when the effect is bypassed (On forces Buffered Bypass). Off = trails silenced immediately on bypass. | 84 (0 = Off, 1–127 = On) | — | ❌ rig-level bypass concern |
| EXP Setup | MIDI On/Off, Heel, Toe | Per-preset heel/toe knob assignments for expression control (multiple knobs, any direction); MIDI EXP On/Off gates CC 100 control. Not PARAM-knob-assignable. | 100 (0 heel – 127 toe) | — | ❌ modulation-matrix concern |
| Dual (Reverb Enable) | **Off / Parallel / Series 1▶▶2 / Series 1◀◀2 / Split 1L\|2R / Split 1R\|2L** | Routing — full semantics in §1.6. | 99 (0–5 in list order) | id 3 `routing` | ✅ (enum-order mapping shim needed) |
| Copy From | **Presets 00A thru 149B** | Copy the selected preset's Reverb 1 type + settings into Reverb 2 of the current preset. Not PARAM-knob-assignable. | — | `copy_params` (dual.rs) | ✅ mechanism; preset-library source ❌ rig-level |

---

## 5. MIDI CC Reference (authoritative full param list)

This is the pedal's complete parameter surface with ranges — the anchor for
our param-id mapping. "Ours" = REVERB_PARAMS id (A-chain unless noted).

### 5.1 Knobs & Common parameters — all reverb types

| Parameter | R1 CC | R2 CC | Values | Ours | Status |
|---|---|---|---|---|---|
| Reverb Type (encoder) | 1 | 2 | 0–11 (engine order §3) | 39 / 4 | ✅ per slot (⚠️ type-value order differs from our enum) |
| Decay knob | 3 | 4 | 0–127 | 1 / 5 | ✅ per slot |
| Pre-Delay knob | 5 | 6 | 0–127 | 43 (A only) | ⚠️ no B |
| Output Level | 7 | 8 | 0–16 | via 0 / 6 (mix) | ⚠️ approximated |
| Pan | 9 | 10 | 0–16 (0 full L, 8 center, 16 full R) | 7 / 8 | ✅ |
| Tone knob | 11 | 12 | 0–127 | 42 (A only) | ⚠️ no B |
| Mod knob | 13 | 14 | 0–127 | 40 (A only) | ⚠️ no B |
| Mix knob | 15 | 16 | 0–127 | 0 / 6 | ✅ |
| Infinite Mode | 17 | 18 | 0–2 (0 Freeze, 1 Infinite, 2 Off) | — | ❌ |
| Param 1 knob | 19 | 20 | 0–127 | — | ❌ (no assignable layer) |
| Param 2 knob | 21 | 22 | 0–127 | — | ❌ |
| Low End | 23 | 24 | 0–20 | low_decay_mult internals | ⚠️ |

### 5.2 1+2 shared

| Parameter | CC | Values | Ours | Status |
|---|---|---|---|---|
| Infinite Latch | 98 | 0 = Momentary, 1–127 = Latching | — | ❌ |
| Boost | 79 | 0–60 (0 = −3 dB, 30 = 0 dB, 60 = +3 dB) | — | ❌ |
| Persist | 84 | 0 = Off, 1–127 = On | — | ❌ |
| Dual Mode | 99 | 0 Off, 1 Parallel, 2 Series 1▶▶2, 3 Series 1◀◀2, 4 Split L\|R, 5 Split R\|L | 3 | ✅ (order shim) |

### 5.3 Reverb-specific

| Parameter | R1 CC | R2 CC | Values | Ours | Status |
|---|---|---|---|---|---|
| Room – Size | 25 | 26 | 0 Studio, 1 Club | 38 | ✅ |
| Room – Diffusion | 27 | 28 | 0–20 | — (internal) | ⚠️ |
| Room – Voice | 29 | 30 | 0 MX, 1 Classic | 34 | ⚠️ fudge |
| Hall – Mid | 35 | 36 | 0–20 | 35 | ✅ |
| Hall – Size | 37 | 38 | 0 Concert, 1 Arena | 38 | ✅ |
| Hall – Swell Rise | 39 | 40 | 0–22 | 36 | ✅ |
| Hall – Swell Type | 41 | 42 | 0 Wet, 1 Dry | 37 | ✅ |
| Hall – Voice | 43 | 44 | 0 MX, 1 Classic | 34 | ⚠️ fudge |
| Chamber – Color | 45 | 46 | 0 Neutral, 1 Clear, 2 Smooth, 3 Crisp, 4 Deep | — | ❌ |
| Plate – Size | 47 | 48 | 0 Small, 1 Large | 38 | ✅ |
| Plate – Voice | 49 | 50 | 0 MX, 1 Classic | 34 | ✅ two impls |
| Spring – Dwell | 51 | 52 | 0 Clean, 1 Combo, 2 Tube, 3 Overdrive | — (implicit) | ⚠️ |
| Spring – Number | 53 | 54 | 0 One, 1 Two, 2 Three Springs | — (implicit, Classic only) | ⚠️ |
| Spring – Voice | 55 | 56 | 0 MX, 1 Classic | 34 | ✅ two impls |
| Impulse – Attack | 57 | 58 | 0–16 | 13 | ✅ |
| Impulse – Stretch | 59 | 60 | 0–16 | 14 | ✅ |
| Impulse – Tail | 61 | 62 | 0 Envelope, 1 Gate | 12 | ✅ |
| Impulse – Direction | 63 | 64 | 0 Forward, 1 Reverse | 15 | ✅ |
| Impulse – Feedback | 65 | 66 | 0–15 | 16 | ✅ |
| Cloud – Diffusion | 67 | 68 | 0–20 | — (internal) | ⚠️ |
| Cloud – Ensemble | 69 | 70 | 0–15 | 29 | ✅ |
| Shimmer – Shift 1 | 71 | 72 | 0–27 (interval list §3.8) | 17 | ⚠️ range gaps |
| Shimmer – Shift 2 | 72* | 73 | 0–28 (Off + list) *(CC 72 collision = manual typo)* | 18 + 19 | ⚠️ range gaps |
| Shimmer – Amount | 75 | 76 | 0–18 | 20 | ✅ |
| Shimmer – Feedback | 77 | 78 | 0 Input, 1 Regeneration, 2 Input + Regeneration | 21 | ✅ |
| Shimmer – Voice | 85 | 86 | 0 MX, 1 Classic | 34 | ⚠️ fudge |
| Bloom – Length | 87 | 88 | 0–17 | — (folded) | ⚠️ |
| Bloom – Feedback | 89 | 90 | 0–17 | — (folded) | ⚠️ |
| Bloom – Harmonics | 91 | 92 | 0–15 | 30 | ✅ |
| Chorale – Vowel | 93 | 94 | 0 AAHHOO, 1 AAHH, 2 AAHHOH, 3 OH, 4 OOOHOH, 5 OOO, 6 Random | — (continuous morph) | ⚠️ |
| Chorale – Resonance | 95 | 96 | 0 Mild, 1 Medium, 2 High | — | ❌ |
| Chorale – Choir | 103 | 104 | 0–15 | 31 | ✅ |
| Chorale – Choir Voice | 105 | 106 | 0 Tenor, 1 Baritone | 32 | ⚠️ ours is Soprano |
| Magneto – Diffusion | 107 | 108 | 0–20 | — (internal) | ⚠️ |
| Magneto – Heads | 109 | 110 | 0 One, 1 Two, 2 Three, 3 Four, 4 Six | — (fixed 4) | ❌ |
| Magneto – Spacing | 111 | 112 | 0 Even, 1 Uneven | — | ❌ |
| Magneto – Ping Pong | 113 | 114 | 0 Off, 1 On | 22 | ✅ |
| Nonlinear – Shape | 115 | 116 | 0 Swoosh, 1 Reverse, 2 Ramp, 3 Gate, 4 Gauss, 5 Bounce | — (extra_a, 4 shapes) | ⚠️/❌ |
| Nonlinear – Chop | 117 | 118 | 0–17 | 23 + 24 | ✅ |
| Nonlinear – Diffusion | 119 | 120 | 0–20 | — (internal) | ⚠️ |
| Nonlinear – Decay | 121 | 122 | 0–17 | 27 | ✅ |
| Nonlinear – Level | 123 | 124 | 0–18 | 28 | ✅ |
| Nonlinear – Mod Speed | 125 | 126 | 0–17 | 26 (+25) | ⚠️ |

### 5.4 Hardware control & other

| Parameter | CC | Values / notes | Status |
|---|---|---|---|
| A Footswitch | 80 | 0 = Press, 127 = Release (momentary-type controller) | ❌ rig-level |
| B Footswitch | 81 | 0 = Press, 127 = Release | ❌ rig-level |
| Infinite Footswitch | 82 | 0 = Press, 127 = Release | ❌ (needs §1.5 first) |
| Value Encoder | 83 | 0 = scroll CCW, 1 = scroll CW | N/A |
| Infinite Off/On | 97 | 0 = Off, 1–127 = On | ❌ |
| Expression Pedal | 100 | 0–127; drives per-preset EXP Setup knob assignments (MIDI EXP must be ON) | ❌ |
| Bypass | 102 | 0 = Bypassed, 1–127 = Engaged | ❌ rig-level |
| MIDI Patch Bank | 0 | 0 = Bank 0 (000A–063B), 1 = Bank 1 (064A–127B), 2 = Bank 2 (128A–149B); followed by PC 0–127 | ❌ rig-level |

---

## 6. Dial-in Queue (per engine, ❌ first)

### Platform-wide (blockers for any full-preset parity)

1. ❌ **Slot B engine params** — route the full MX param set to chain B
   (mirror every A param with a `_b` twin or a slot-select addressing scheme;
   the pedal's R2 CC column is the spec).
2. ❌ **Infinite/Freeze subsystem** — Freeze vs Infinite vs Off three-way
   (Freeze = capture-and-hold, new input bypasses the tank; Infinite = keep
   feeding input), plus Momentary/Latching, exposed as params.
3. ❌ **Param 1/2 assignable layer** — only if hardware-knob-surface parity is
   wanted; DSP parity can skip.
4. ❌ Kill dry, Boost ±3 dB, Persist, Spillover (5 s warm-up rule, not for
   Impulse), presets/banks — rig-level; document rig ownership, don't build
   in reverb-dsp.
5. ⚠️ **Low End** as a first-class param id (today: low_decay_mult +
   band_crossover_hz internals).
6. ⚠️ **Output Level** per slot as a wet-only attenuator distinct from mix.
7. ⚠️ CC/enum order shims: Reverb Type 0–11 order, Dual Mode 0–5 order.
8. N/A Cab Filter (rig NAM/cab covers it — by design).

### Chamber

1. ❌ **Color selector** (Neutral / Clear / Smooth / Crisp / Deep) — five
   fixed tonal profiles (flat / low-cut / mid-cut "smile" / high-passed /
   mid-boost vocal); today `extra_a` is only an ER level.

### Magneto

1. ❌ **Knob remap at the param layer** — Decay = delay time of the last head
   (≤ 1500 ms), Pre-Delay = feedback (last head; last **two** heads when
   Uneven).
2. ❌ **Heads selector 1/2/3/4/6** — currently fixed `NUM_HEADS = 4`.
3. ❌ **Spacing Even/Uneven** — even = equal subdivisions of the delay time;
   uneven = non-rhythmic; also switches the feedback tap.
4. ⚠️ Diffusion as a param (progressive per-head smear exists, hardwired).
5. ⚠️ Ping Pong per-head-count pattern variants (L-R-R etc. at 3/4/6 heads).

### NonLinear

1. ❌ **Gauss and Bounce shapes** (bell / inverted-bell envelopes).
2. ❌ **Pre-Delay = feedback around the nonlinear generator** (repeating
   shapes; Gate + max feedback = endless multi-tap wash).
3. ⚠️ Named Shape selector param (today: `extra_a` thresholds over 4 shapes).
4. ⚠️ Diffusion as a param; ⚠️ single Chop macro (rate+depth) for CC parity;
   ⚠️ Mod Speed unified over taps + late lines.

### Chorale

1. ❌ **Resonance selector** (Mild / Medium / High vocal-filter Q).
2. ⚠️ **Named Vowel programs** (AAHHOO, AAHH, AAHHOH, OH, OOOHOH, OOO,
   Random) — today a continuous ah→oo morph; Random program missing.
3. ⚠️ **Choir Voice = Baritone** (low range) — ours pitches the second voice
   *up* (Soprano); pedal goes down.

### Spring

1. ❌/⚠️ **Dwell drive stages** (Clean / Combo / Tube / Overdrive) as a named
   param with real preamp gain + harmonic staging and input-level-dependent
   drive (today decay→loop-gain only).
2. ⚠️ **Number of Springs** (1/2/3) as a named param on both voices (today
   `extra_b`→1–3 on the Classic/vintage voice only).

### Shimmer

1. ❌ **Interval coverage**: + Octave + 5th (+19 semi), + 2 Octaves (+24), and
   the ± 10 Cents detune steps (named detents) — extend shift range past ±12.
2. ⚠️ Classic voice = real time-domain shifter w/ modulated buffers (today a
   parameter fudge).

### Bloom

1. ⚠️ **Length** (bloom-stage duration) and **Feedback** (around the bloom
   stage) as params distinct from tank decay — the two-stage
   bloom-into-tank topology with independent controls.
2. ⚠️ Two independent 16-phase mod oscillators (bloom lines vs tank lines) and
   the resonant synth-voiced Tone filter — verify against our Greyhole core.

### Room

1. ⚠️ Diffusion as a param id (internal field exists).
2. ⚠️ Classic voice as more than a fudge (lower density, mild high damping) —
   acceptable if A/B against hardware passes.

### Hall

1. ⚠️ Classic voice fudge (even ER profile / booming slow Arena) — A/B check.
2. ⚠️ Verify Swell Dry semantic ("swells the dry signal into the reverb") vs
   our 0 wet / 1 wet+dry implementation.

### Cloud

1. ⚠️ Diffusion as a param id (cascaded input diffusors exist).
2. ⚠️ Mod scheme check: quadrature oscillator on input diffusors, depth to
   2 o'clock then **frequency** beyond — match the two-segment knob law.

### Impulse

1. ⚠️ Factory IR library + folder browsing (rig asset pipeline; custom
   `load_ir_wav` works).
2. ⚠️ Reset-on-load rule (all controls except MIX to documented defaults) —
   preset-layer policy.
3. ⚠️ Feedback topology check: wet fed back **into the pre-delay** line.

### Plate

1. ⚠️ MX-Small "reduced headroom + subtle tube saturation" — verify the
   Lexicon-voice pair covers the saturation nuance. Otherwise complete.
