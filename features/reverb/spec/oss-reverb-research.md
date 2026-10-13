# Open-Source Reverb Design Research

**Generated:** 2026-07-28. Companion to `bigsky-mx-parity.md` — reference
algorithms, portable code, and papers for dialing in the reverb engines and
building the A/B harness (issue #69). Sibling doc: the delay-side survey at
`features/fx/delay/spec/oss-delay-research.md`.

**License triage convention** used throughout:
- **PORTABLE** — MIT/BSD/CC0/BSL/permissive: code may be translated into this
  tree (keep provenance notes at the port site).
- **IDEAS ONLY** — GPL/LGPL/AGPL/unlicensed: clean-room concepts only.
- **FAIR GAME** — papers/manuals/blogs: algorithms freely implementable.

## Executive summary — the twelve highest-value moves

1. **Per-line decay filters done right**: replace the FDN's one-pole/2-band
   damping with Jot's exact per-line shelf (formulas in §Part-2.1) or the
   2024 Välimäki/Prawda/Schlecht **two-stage attenuation filter** (shelf hits
   DC/Nyquist targets exactly, octave GEQ fits the residual, scale all lines
   by `G^(m_i/m_max)`) + the Jot tonal-correction one-zero on the wet out.
   This is the single biggest fidelity jump across Hall/Room/Plate, and it IS
   BigSky's low/high decay control structure (zita's three-band T60 is the
   same idea, portable via the Faust JOS port).
2. **Mod as slow orthogonal matrix rotation** (Schlecht): rotating the FDN
   feedback matrix animates the tail with no decay error and no pitch
   artifacts — the right mechanism for BigSky's Mod knob on Hall/Room class
   engines (keep delay-mod chorusing as the *Classic*-voice flavor).
3. **Hall realism kit**: zita-style in-loop ±0.6 allpasses (density builds
   per pass), three-band T60, Hadamard-vs-Householder A/B, sine-quadrature
   chorus for Concert vs **reverbsc-style per-line random-walk jitter**
   (0.6–1.7 ms drift) for Arena — big-but-not-chorused.
4. **Room realism kit**: Moorer/CloudSeed multitap early-reflection block in
   front (tight 5–15 ms Studio / 20–40 ms Club patterns), Mutable's MIT
   figure-8 tank constants for the small dense late, lower diffusion than you
   think (Valhalla: vocals ring high-coefficient allpasses).
5. **Plate realism kit**: verify our Dattorro core against the paper's exact
   tables (negative −0.70 decay-diffusion, ±0.6 seven-tap output, quadrature
   ~1 Hz ±8-sample mod, allpass interpolation); add **downward-chirp
   dispersion** (highs travel faster in steel) via a chirp-allpass cascade
   and decouple LF decay — the ValhallaPlate lessons; kPlate140/240 (MIT) for
   EMT 140-vs-240 voicing ideas.
6. **MX vs Classic voices, concretely**: MX = 8–12 lines, cubic/allpass
   interpolated reads, random-walk or low-depth quadrature mod, three-band
   T60, full bandwidth. Classic = fewer lines or the Keith Barr single
   allpass-loop topology, 8–10 kHz bandwidth cap, linear/truncated reads
   (interpolation grain), stronger single-sine chorus, Freeverb's +23-sample
   stereo spread. Vintage = bandwidth + interpolation noise, not just EQ.
7. **Spring, calibrated**: the DAFx-11 Gamper/Parker/Välimäki table (M=100
   stretched allpasses a₁≈+0.62, K=fs/2F_C, chirp EQ 183 Hz/B146, g≈−0.8
   NEGATIVE = alternating-polarity drip; C_hf path ≥30 dB down) + run C_lf at
   fs/4 (≈355 mults/sample). Dwell = tanh drive INTO the tank (not loop
   feedback). Multi-spring: different T_D same F_C (Leem) = BigSky's wording.
8. **NonLinear is a tap bank, officially**: the RMX16 manual confirms
   non-recirculating multitap diffuser banks with stepped tap-gain envelopes
   (no detector!) — our envelope-window architecture is right; adopt the
   per-tap rising-LP for Swoosh, preset-stable jitter, soft gate knee; free
   NonLin2 IRs are exact fit targets (a FIR bank's IR is its taps).
9. **Chorale from measured data**: Csound's measured formant tables (F/A/BW
   per vowel per voice type), morph in log-frequency, KEEP the 2.2–3.3 kHz
   singer's-formant cluster, and Ternström's choir numbers (±10–15 c static
   detune + independent 0.5–8 Hz flutter per voice — independence is the
   whole "more singers" illusion).
10. **Cloud Ensemble = 48-band vocoder-driven additive resynthesis** (per
    Strymon's own Cloudburst description): per-band envelope followers →
    partials at 2×/3×/4× band centers. No pitch tracking needed. String-machine
    alternative: 3 BBD lines + two 3-phase LFOs at 120° (Solina topology;
    Plaits' string-machine engine is MIT).
11. **Impulse engine**: our reshaper should apply decay/tail/damp as
    **per-partition spectral gains** (zero extra FFTs — linearity) with
    20–100 ms dual-engine crossfades for swaps; Garcia-optimal partitioning +
    time-distributed tail FFTs for the wasm build (zones_convolver MIT is the
    reference; fft-convolver crate is MIT).
12. **A/B harness upgrade**: add EDT, C50/C80, NED-derived mixing time,
    late-tail σ_dB/centroid to ir_metrics; capture the pedal with repeated
    ESS sweeps and use **inter-sweep coherence decay as the Mod metric**
    (modulated tails are not LTI — one sweep is a lie). Knob→metric map in
    Part 2 §9.

---

# Part 1 — Algorithmic reverb cores (Room / Hall / Chamber / Plate / Cloud)


# Open-Source Algorithmic Reverb Research — Reference Report for BigSky MX Parity

Scope: reference algorithms + concrete dial-in techniques per engine for `features/fx/reverb/reverb-dsp` (engines: Room Studio/Club, Hall Concert/Arena, Chamber+Color, Plate Small/Large, Cloud; MX vs Classic voices). License triage per convention: **PORTABLE** (MIT/BSD/CC0/PD), **IDEAS ONLY** (GPL/LGPL/unreleased), **FAIR GAME** (papers/blogs).

---

## 1. CloudSeed / CloudSeedCore — **PORTABLE (MIT)**

- Original repo: [ValdemarOrn/CloudSeed](https://github.com/ValdemarOrn/CloudSeed) (MIT; C# code on `legacy-v1` branch). Active successor: [GhostNoteAudio/CloudSeedCore](https://github.com/GhostNoteAudio/CloudSeedCore) — **MIT**, C++14, the algorithm we derived from is fully open (attribution to Ghost Note Audio required).
- Architecture (from `DSP/ReverbChannel.h` in CloudSeedCore): input HPF/LPF (defaults 20 Hz / 20 kHz) → **modulated pre-delay** → optional **multitap early-reflection delay** (`TapCount`, `TapDecay`, `TapLength`) → optional **allpass diffuser cascade** (`Stages`, per-stage delay + mod amount/rate + feedback) → **12 parallel "late" lines** (`TotalLineCount = 12`, active count 1–12), each line = modulated delay (`LateLineSize` ms ± `LateLineModAmount` at `LateLineModRate`) → per-line feedback gain from T60 (`DB2Gain(delaySamples / lineDecaySamples * -60)`) → optional per-line **post-diffuser** (`LateDiffuseCount` stages, own modulation) → optional per-line **EQ: low shelf, high shelf, lowpass**.
- Stereo strategy: seeded randomization everywhere — `delayLineSeed`, `postDiffusionSeed`, tap seed, diffuser seed — with a **crossSeed** knob: L uses `1 − 0.5·crossSeed`, R uses `0.5·crossSeed` scaling of the random buffer, giving continuously variable L/R decorrelation. Line sum normalized by `1/√lineCount`. `LateMode` toggles tapping lines pre- vs post-diffuser. Output = `dry·in + earlyGain·early + lineGain·lineSum`, gains in dB with ≤−30 dB = mute.
- **What we're likely NOT using yet** (checklist vs our `chain.rs`/`reverb_line.rs`, which already have per-line diffuser, `tap_post_diffuser`, seeds, cross-seed):
  1. The **multitap early-reflection block** between predelay and diffusion (CloudSeed's "early" out is a real ER generator, not just diffusers) — directly useful for Room Studio/Club realism.
  2. **Per-line shelving EQ inside the feedback loop** (low shelf + high shelf + LP per line, with `EqCrossSeed` decorrelating filter settings L/R) — this is the cheapest route to BigSky's per-engine tone stack (Hall mid EQ, Chamber Color).
  3. **Modulated pre-delay** (slow drift on predelay itself — subtle vintage wow, good for Classic voices).
  4. **Line count as a voicing parameter** (BigSky-ish: fewer lines = grainier/vintage, 8–12 = dense modern — a nearly free MX/Classic differentiator).
  5. **Interpolation toggle** — non-interpolated delay reads are a legit "1980s digital" lo-fi color (zipper/grain), i.e. a Classic-voice switch.
- Note: CloudSeed has **no feedback matrix at all** — the 12 lines are fully parallel and only decorrelated by seeds. Our Householder FDN cores are already a step beyond it for halls; CloudSeed's strength is the *seeded-random parallel bank* character (clouds, washes) — keep it for Cloud, prefer matrixed loops for Room/Hall.

## 2. Mutable Instruments reverb (Clouds/Elements/Rings) — **PORTABLE (MIT)**

Source: [pichenettes/eurorack](https://github.com/pichenettes/eurorack), `elements/dsp/fx/reverb.h`, `clouds/dsp/fx/reverb.h`, `rings/dsp/fx/reverb.h` (all MIT). This is the canonical minimal **Griesinger/Dattorro figure-8 tank**, mono-in/stereo-out:

- **Elements/Rings constants** (32 kHz): input APs **150, 214, 319, 527**; tank loop 1: APs **2182, 2690**, delay **4501**; loop 2: APs **2525, 2197**, delay **6312**.
- **Clouds constants** (smaller, brighter): input APs **113, 162, 241, 399**; loop 1 APs **1653, 2038**, delay **3411**; loop 2 APs **1913, 1663**, delay **4782**.
- Coefficients: `kap = diffusion` (default **0.625** — exactly Dattorro's input diffusion 2), `klp = lp` (one-pole damping, default **0.7**), `krt = reverb_time`. Signal flow per loop: read other loop's long delay (modulated) → ×krt → one-pole LP damp → 2 APs → write own delay → output tap. Cross-fed figure-8 = each channel's tail is the other's input → wide, decorrelated stereo from a mono sum.
- **Modulation**: two LFOs — **0.5 Hz on input AP1** (smears attack, "breathing" diffusion) and **0.3 Hz on the loop delay** (chorus in the tail); depths ~tens of samples at 32 kHz. Two slow *sine* LFOs at incommensurate rates is the entire lushness recipe — extremely cheap.
- Mapping: this is the perfect **skeleton for Room Club and Chamber** (small, dense, slightly chorused). Beads' improved reverb is **NOT open source** (never released; see [MOD WIGGLER](https://www.modwiggler.com/forum/viewtopic.php?t=281283)) — ideas only from ear.

## 3. Airwindows (Galactic, Galactic2, Verbity, Chamber, Infinity/2, PocketVerbs, kPlate A–D, kPlate140/240) — **PORTABLE (MIT)**

Repo: [airwindows/airwindows](https://github.com/airwindows/airwindows) — MIT, all plugin sources included. Chris Johnson's reverb toolbox, in his own words:

- **Verbity** ([page](https://www.airwindows.com/verbity/)): **feedforward chain of three banks of four delays each through Householder matrices** — "each bank feeds another bank and only the very last one of three feeds back to the start" (Bricasti's Casey Nord influence). Strictly **dual-mono** (center stays center), no pitch bending. Controls RoomSize/Sustain/Mess; "Wetness" adds wet without attenuating dry until 0.5.
- **Chamber** ([page](https://www.airwindows.com/chamber/)): same 3×4 feedforward-Householder skeleton but delay times descend by the **golden ratio** ("a spiral of delay constants"); tuned against recordings of **giant underground concrete cisterns**; Darkness models air absorption with warm IIRs; at zero feedback it degenerates to "weird stuttery slapback" → continuous tail as Longness rises. Directly relevant to our **Chamber** engine's character target.
- **Galactic** ([page](https://www.airwindows.com/galactic/)): Verbity-derived feedback+feedforward big-space reverb; stereo width via **quadrature pitch-shift "Drift/Detune"** on the two channels; Replace/Brightness/Bigness controls; buffer-size changes audibly warp the tail (a feature). **Galactic2** ([page](https://www.airwindows.com/galactic2/)): one giant **4-wide Householder matrix accessed row-wise by L and column-wise by R** ("crossways") — mono becomes WIDE center content with **no chorusing/pitch shift**; Darken applies in both direct out and the feedback loop so recirculating energy darkens progressively (very natural infinite-decay behavior). That crossways-matrix trick is portable and cheap — an excellent **Cloud/Hall Arena stereo device that keeps mono compatibility**.
- **kPlate series**: kPlateA–D ([kPlateA](https://www.airwindows.com/kplatea/)) are four differently-voiced plates aiming at "the lettered EMT plate on top of Abbey Road"; **kPlate140** ([page](https://www.airwindows.com/kplate140/)) / kPlate240 are the refined pair (EMT 140 steel vs 240 gold-foil characters: "flashier, deeper, fiery" vs "cloudier, understated"). Technique stack: **5×5 Householder feedforward matrices, generated and fitness-tested by the hundred-thousand** (evolutionary matrix search — he literally brute-forces matrix/delay-set combinations and picks by ear/fitness), plate-style delay density, "Pear" filters, and **Bezier undersampling** ([undersampling tag](https://www.airwindows.com/tag/undersampling/)): run the tank at SR/2 or SR/4 and reconstruct with Bezier-curve interpolation instead of linear, which keeps CPU flat at 96/192k and adds a smooth, non-digital top end. See also [kCathedral5](https://www.airwindows.com/kcathedral5/), [kBeyond](https://www.airwindows.com/kbeyond/).
- **Infinity/Infinity2** ([Infinity2](https://www.airwindows.com/infinity2/)): allpass-network **infinite-sustain** reverb: entry allpasses "spread and smooth" input (bypassable in v2), damping on all paths, feedback kill-switch. Blueprint for BigSky-style **freeze/infinite** behavior in Cloud.
- **PocketVerbs**: older nested-allpass multi-mode set (Room/Hall/Plate/Spring/Tube) — mostly superseded by the above; still MIT if we want its Spring/Tube quirks.
- Actionable takeaways: (a) **feedforward Householder banks** (energy in → cascades forward → single global feedback) give dense-but-uncolored tails with very fast density buildup — a different flavor than our recirculating FDN; (b) **fitness-testing generated matrices/delay sets** is an offline tuning strategy we can copy (script the search, audition IRs, bake constants); (c) **undersampled tanks + fancy reconstruction** for cheap high-SR support; (d) **golden-ratio delay laddering** for chamber character.

## 4. Freeverb + reverbsc/bigverb — **PORTABLE (PD / MIT / MIT-or-Unlicense)**

- **Freeverb** (Jezar at Dreampoint, public domain; constants confirmed via [STK's FreeVerb.cpp](https://github.com/thestk/stk/blob/master/src/FreeVerb.cpp), and [JOS PASP](https://ccrma.stanford.edu/~jos/pasp/Freeverb.html)): 8 parallel lowpass-feedback combs **{1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617}** + 4 series allpasses **{225, 341, 441, 556}** with g = **0.5**; right channel = every length + **stereospread 23**; `feedback = roomsize·0.28 + 0.7`, `damp·0.4`, fixed input gain 0.015. Not a realism reference, but the **+23-sample stereo offset** and Schroeder-Moorer comb bank remain the "cheap vintage digital room" sound — a Classic-voice texture.
- **reverbsc/bigverb** (Sean Costello 1999 → Csound `reverbsc` → Soundpipe (MIT) → [sndkit bigverb](https://paulbatchelor.github.io/sndkit/bigverb/), **MIT or Unlicense**, [repo](https://github.com/PaulBatchelor/sndkit); also ported in DaisySP, MIT): **8 parallel delay lines** (~1945–4127 samples @44.1k), even lines fed L, odd fed R; scattering junction: `sum·(2/N)=0.25` fed back to all inputs; output ×0.35; one-pole LP tone (default 10 kHz) in each feedback path. **The signature technique: per-line randomized delay-time jitter** — each line has its own `{drift amount (0.6–1.7 ms), rand freq (~0.9–4 kHz update), RNG seed}`; a 16-bit LCG picks new targets, linearly interpolated between updates, cubic-interpolated fractional reads. This **random-walk modulation** (vs sine) is what makes it huge and non-chorusy — the exact "lush but not detuned" quality wanted for Hall MX and Cloud. This *is* Costello's CCRMA-era algorithm lineage (his later Anderson & Costello 2009 B-format paper adapts these architectures to ambisonics).

## 5. Zita-Rev1 (Fons Adriaensen) — **IDEAS ONLY (GPL)** … but the Faust JOS port is **PORTABLE (STK-4.3/MIT-style)**

Key nuance: the original zita-rev1 C++ is GPL, but `re.zita_rev_fdn` in [faustlibraries reverbs.lib](https://github.com/grame-cncm/faustlibraries/blob/master/reverbs.lib) lives in the **JOS section under the MIT-style STK-4.3 license** — so the algorithm constants below are translatable ([docs](https://faustlibraries.grame.fr/libs/reverbs/), [JOS PASP page](https://ccrma.stanford.edu/~jos/pasp/Zita_Rev1.html)):

- **8×8 FDN, Hadamard feedback matrix** (`ro.hadamard(8)` butterfly — cheaper and smoother-mixing than Householder; worth A/B-ing against our Householder cores for Hall).
- Delay lengths (seconds): **0.153129, 0.210389, 0.127837, 0.256891, 0.174713, 0.192303, 0.125000, 0.219991**.
- A **Schroeder allpass comb in series inside each delay line**, coefficients alternating **±0.6** — diffusion inside the loop rather than only at input (this is the zita signature: density keeps building every pass without a separate diffuser block).
- **Three-band decay**: per-line gain `g = exp(−3·ln10·delay/T60(band))` with a low shelf giving `t60dc` below crossover **f1** (~200 Hz default) and `t60m` for mids, plus a one-pole HF rolloff at **f2** damping the top. This "T60(low)/T60(mid)/HF-cut" triple is exactly BigSky's Hall low/high decay controls — adopt the same per-line band-split gain instead of a single damping pole.
- `zita_rev1_stereo` adds pre-delay and a 2-band output parametric EQ (maps to BigSky Hall's **mid EQ** knob).

## 6. Dragonfly Reverb / Freeverb3 — **IDEAS ONLY (GPL-3)**

[michaelwillis/dragonfly-reverb](https://github.com/michaelwillis/dragonfly-reverb), GPL-3, DSP from Teru Kamogashira's Freeverb3. Algorithm mapping (per the [manuals](https://michaelwillis.github.io/dragonfly-reverb/dragonfly-room-manual.html)/[DeepWiki](https://deepwiki.com/michaelwillis/dragonfly-reverb)):

- **Dragonfly Hall = Freeverb3 "Hibiki"**: **Moorer early-reflection model + modified zita FDN late** — the ER-into-FDN pattern with separate early/late levels and "dry signal path" is the modern hall recipe; note it confirms zita-class FDNs read as *concert hall* when fed sparse Moorer ERs.
- **Dragonfly Room = Freeverb3 "ProG"** = an implementation of **Dattorro's "Progenitor"** (Effect Design follow-up work modeling the **Lexicon 224 concert-hall/room lineage** — Griesinger's asymmetric loop with multiple taps). Worth studying conceptually for our `plate_progenitor.rs` and Room engines: the Progenitor loop is *asymmetric* (different L/R branch lengths, many scattered output taps), which produces the vintage Lexicon "room" spread.
- **Dragonfly Plate = NRev + STRev** variants; **Early = standalone Moorer ER** with selectable room patterns.
- What's worth learning (clean-room): the *product decision* — Hall/Room/Plate as ER-model + late-core pairings with independent early/late sends — mirrors BigSky's engine structure; and Moorer's tap-pattern ER model as the front half of Room realism.

## 7. JPverb / Greyhole (Julian Parker) — **PORTABLE (MIT, in faustlibraries)**

Confirmed in-file: `declare jpverb license "MIT license"` / `declare greyhole license "MIT license"` (author Julian Parker, interface fixes Till Bovermann) in [reverbs.lib](https://github.com/grame-cncm/faustlibraries/blob/master/reverbs.lib) — the SC3-plugins originals (DEIND project) were GPL, but this MIT declaration makes the Faust text the translation source.

- **JPverb** ("lush chorused vintage Lexicon/Alesis" hall): forward diffusion into a feedback loop of modulated delays. Concrete internals from the Faust source: **five cascaded diffuser blocks**, each a 2×2 **rotation (π/4) + allpasses** with delays `10+30i` / `110+30i` ms-scale scaled by **size**; loop delays prime-derived, scaled by size; **feedback `fb = 10^(−3/(t60/total_length))` with `total_length = 1.7·0.1·(size·5/4 − 1/4)`**; **sine modulation `depth + depth·osc(modFreq) + 5` samples** on the loop delays; damping via smoothing filters after the loop sum; independent **low/mid/high T60 multipliers with two crossover frequencies** (again the BigSky Hall decay-EQ pattern). Params: t60 0.1–60 s, damp, size 0.5–5, earlyDiff, modDepth/modFreq (~0.1–10 Hz).
- **Greyhole**: same diffuser as a "mini-reverb" front end, then a **single very long modulated feedback delay** (`de.sdelay` up to 65536 samples with smooth length crossfading), modulation `10 + depth + depth·osc(freq)` with depth up to ~50 samples, feedback knob, plus **pitch/"moth" degradation options** — it's the diffused-echo/wash topology. Direct blueprint for **Cloud** (and BigSky's "Swell"-adjacent washes): big diffusion + one huge modulated loop reads as cloud, not room.

## 8. Dattorro 1997, "Effect Design Part 1" — **FAIR GAME** (read in full from the [CCRMA PDF](https://ccrma.stanford.edu/~dattorro/EffectDesignPart1.pdf))

The plate-class reference, "in the style of Griesinger" (Lexicon). Complete spec @ **Fs = 29761 Hz**:

- Input: predelay → one-pole **bandwidth** LP (0.9995) → 4 series allpass lattices **142, 107 (coef 0.750 = input diffusion 1), 379, 277 (coef 0.625 = input diffusion 2)**.
- **Tank (figure-8, two cross-fed branches)**: branch A = modulated AP **672±EXCURSION** (coef −**0.70**, "decay diffusion 1", *negative* sign deliberately changes IR character) → delay **4453** → damping one-pole (**0.0005**) → ×decay (**0.50**) → AP **1800** (coef **0.50** = "decay diffusion 2", rule: `decay+0.15`, clamp 0.25, 0.50) → delay **3720** → ×decay → into branch B; branch B mirrors with **908±EXCURSION**, **4217**, AP **2656**, delay **3163**.
- **Modulation**: EXCURSION max 16 samples; recommended ~**1 Hz, ±8 samples** peak @29.8 kHz, on the *first* AP pair of each branch, driven by a **quadrature sine pair** (decorrelated L/R); he explicitly advises: ideally modulate *all* tank diffusers at differing rates/depths so each needs less depth; **use allpass interpolation** (linear interp adds unintended damping).
- **Output taps** (all ±0.6, taps *inside* the opposite branch dominate each side — instant wide stereo):
  `yL = 0.6·[4217@266] + 0.6·[4217@2974] − 0.6·[2656@1913] + 0.6·[3163@1996] − 0.6·[4453@1990] − 0.6·[1800@187] − 0.6·[3720@1066]`
  `yR = 0.6·[4453@353] + 0.6·[4453@3627] − 0.6·[1800@1228] + 0.6·[3720@2673] − 0.6·[4217@2111] − 0.6·[2656@335] − 0.6·[3163@121]`
- Practical gems: instantaneous-density-with-exponential-decay **is** the definition of plate class ("recording engineers don't want to wait for density to build"); magnitude-truncate recursive writes to kill limit-cycle "ocean noise"; negative allpass coefficients as a free character change; the whole 22K-word design defies Schroeder's eigentone-density rule by balancing *echo density vs mode density*.
- Griesinger lineage note: the paper credits the topology style to Griesinger (Lexicon); Dattorro's later "Progenitor" (see Freeverb3 ProG) extends the same loop asymmetrically toward the 224 hall/room sound.

## 9. Valhalla DSP blog + Costello papers — **FAIR GAME** (concepts only; valhalladsp.com blocks fetches, WordPress mirror works)

- [Diffusion, allpass delays, and metallic artifacts](https://valhalladsp.wordpress.com/2011/01/21/reverbs-diffusion-allpass-delays-and-metallic-artifacts/): metallic ring = allpasses pass all frequencies *over time* but bunch them; fewer series APs = less colored but sparser (halls); more APs smear but stretch the attack unnaturally; **modulate the longer allpasses, not the short input ones** (short-AP modulation sounds like "water sloshing in a metal pan"); expose Diffusion as a *per-source* control (drums want high, vocals lower).
- [Diffusion, vocals, and real rooms](https://valhalladsp.wordpress.com/2010/06/07/diffusion-vocals-and-real-rooms/): real rooms are naturally diffuse; artificial low-diffusion = "huge empty room"; vocals ring series APs (pulse-train input) — lower diffusion coefficients for vocal presets; slight sibilant grain is an acceptable trade.
- [Schroeder reverbs: the forgotten algorithm](https://valhalladsp.wordpress.com/2009/05/30/schroeder-reverbs-the-forgotten-algorithm/): parallel-combs-into-series-allpasses still useful; output APs raise density to this day.
- ValhallaPlate series ([Introducing](https://valhalladsp.com/2015/11/07/introducing-valhallaplate/), [Chambers vs Plates](https://valhalladsp.com/2015/11/07/chambers-versus-plates/), [Physics & Psychophysics of Plates](https://valhalladsp.com/2015/11/08/the-physics-and-psychophysics-of-plates/), [The Reverb Modes](https://valhalladsp.com/2015/11/08/valhallaplate-the-reverb-modes/)): plates = **instant echo density + dispersion** (speed of sound frequency-dependent in steel: **highs arrive first**, chirped transients → 3D stereo image); **HF decays short, LF decay can be much longer or shorter than mids**; chambers = higher modal density, slower density buildup than plates but faster than halls; ValhallaPlate's 12 modes = 7 true-steel-modal-density + 5 chamber-density, plus hybrids (Aluminum/Copper/Unobtanium = plate dispersion × chamber mode density). Actionable: add a **dispersive element** (cascaded short allpasses with frequency-dependent group delay, or a chirp-tuned AP chain) to Plate's input path for the "true plate" transient; make Plate LF decay independently adjustable.
- [VintageVerb: The MODES](https://valhalladsp.com/2023/02/10/valhallavintageverb-the-modes/) + [ValhallaRoom Dark Room](https://valhalladsp.wordpress.com/2011/06/21/valhallaroom-v1-0-6-introducing-dark-room/) + [Shimmer intro](https://valhalladsp.wordpress.com/2010/08/30/introducing-valhallashimmer/): Concert Hall = time-varying delays *inside* the recursive network → raises perceived modal density + chorusing; some modes use **internal delay randomization** ("random mod") for artifact reduction *without pitch change*; color modes = **1970s** (bandlimited + lossy/noisy interpolation), **1980s** (brighter, same interpolation grunge), **Now** (clean) — i.e. vintage voicing = bandwidth cap + interpolation noise, not just EQ. Plate mode = "early-80s plate algorithm: bright, instantly diffuse, lush chorusing"; Room = "early-80s room: medium diffusion, darker, chorused".
- [Getting Started with Reverb Design, Part 2: The Best Papers](https://valhalladsp.com/2021/09/22/getting-started-with-reverb-design-part-2-the-foundations/) (blocked from fetch but canonical list): Schroeder '62, Moorer '79, Gardner '92 (nested allpasses), Jot '91 (FDN + frequency-dependent decay), Dattorro '97, Griesinger AES papers, Smith (waveguide). Costello's own CCRMA-era output = the reverbsc random-jitter FDN (see §4) and [Anderson & Costello 2009 B-format adaptation](https://www.researchgate.net/publication/344877067_Energy-Preserving_Time-Varying_Schroeder_Allpass_Filters) lineage.
- [AES 2015 slides](https://valhalladsp.wordpress.com/2015/06/19/slides-from-my-aes-reverb-presentation/) (PDF linked in post) — hardware-history survey (Lexicon/Alesis/AMS/EMT140/EMT250).

## 10. Other permissive gems

- **Keith Barr allpass-loop reverb** — now in [reverbs.lib](https://faustlibraries.grame.fr/libs/reverbs/) as `kb_rom_rev1` (adaptation of Barr's own FV-1 `rom_rev1.spn`) — **GPL-3 (that section only — IDEAS ONLY)**, but the *topology* is fully documented: **one big loop of 4 sections**, each = delay → ×feedback gain → allpasses (one **modulated by a 0.5 Hz sine**) → one-pole damping; 2×4 input allpasses; **8 output taps mixed to stereo with gains 1.5/1.2/1.0/0.8**; 32768 Hz base rate. This is the MidiVerb/Alesis/FV-1 "ring" sound — the single most relevant reference for **Classic-voice** springy/vintage character. (Spin Semi forum [history thread](http://www.spinsemi.com/forum/viewtopic.php?t=3) — cert-expired, couldn't fetch.)
- **STK** ([NRev.cpp](https://github.com/thestk/stk/blob/master/src/NRev.cpp), MIT-style STK-4.3): NRev (CCRMA/CLM): 6 combs **{1433, 1601, 1867, 2053, 2251, 2399}** (prime-adjusted, SR-scaled) + 8 allpasses from **{347, 113, 37, 59, 53, 43, 37, 29, 19}**, AP coef **0.7**, single LP, `g = 10^(−3·delay/(T60·SR))`. JCRev similar (Chowning). Dark, church-y; the T60-per-comb formula is the standard.
- **DaisySP** ([electro-smith/DaisySP](https://github.com/electro-smith/DaisySP), MIT): contains a clean C++ `ReverbSc` port — a second PORTABLE reference implementation of §4's jittered FDN.
- **Csound reverbsc** — LGPL Csound, but algorithm identical to sndkit bigverb (MIT/Unlicense) → use sndkit as source.
- **Teensy Audio Library** (MIT): `AudioEffectFreeverb` — nothing beyond Freeverb.
- **VCV Valley "Plateau"** — Dattorro-derived, **GPL-3 → IDEAS ONLY** (but Dattorro's paper itself is the source anyway).
- **Faust `re.dattorro_rev`** — a ready cross-check implementation of §8 under the faustlibraries licenses (verify section header before translating; the JOS/STK-4.3 sections are portable, GRAME sections LGPL).

---

## Engine-by-engine dial-in plan (BigSky mapping)

**Room — Studio / Club** (our `room.rs`/`room_studio.rs`/`room_chamber.rs`)
- Realism lever #1 is the **early field**: add a Moorer-style multitap ER block (CloudSeed's multitap stage, PORTABLE) with distinct tap patterns per variant — Studio = tight, absorptive, ~5–15 ms taps, fast decay, **medium-low diffusion** (Costello: real small rooms read as *less* diffuse in algorithms; vocals prefer lower AP coefficients ~0.4–0.55); Club = longer tap spread (~20–40 ms), a few strong discrete lateral taps, slightly brighter.
- Late: small figure-8 tank at Clouds constants (113/162/241/399 + ~1.6–4.8 k loops, PORTABLE MIT) rather than a big FDN; damping klp ≈ 0.7; keep RT short and let the ER-to-late balance carry "size".
- Classic-voice room = VintageVerb recipe: bandlimit to ~8–10 kHz + noisy/linear interpolation grain.

**Hall — Concert / Arena** (`hall.rs`/`hall_arena.rs`/`hall_cathedral.rs`)
- Adopt **zita-style in-loop allpass combs (±0.6) inside each FDN line** so density builds per-pass — halls want *slow* density buildup (few input diffusers, 1–2 stages, coef ~0.5–0.6), then compounding.
- Replace single-pole damping with the **three-band T60**: `g_line = 10^(−3·delay/(T60(band)·SR))` with low crossover ~200–400 Hz (BigSky "Low Decay"), HF rolloff crossover 1.5–6 kHz — this alone converts "generic FDN" into "concert hall". Mid EQ knob = zita_rev1's output parametric.
- Consider **Hadamard instead of Householder** for the Hall matrix (zita/Faust `ro.hadamard`) — A/B for smoothness.
- Modulation: Concert = **sine chorusing** in the recursive network (VintageVerb Concert Hall / Lexicon 224 lineage; ~0.3–1 Hz, several samples, quadrature pairs); Arena = **random-walk jitter** per line (reverbsc's per-line drift 0.6–1.7 ms, update ~1–4 kHz random targets) for huge-but-unchorused decay + longer pre-delay and sparser ERs. Swell = input envelope, orthogonal.

**Chamber (+ Color)** (`room_chamber.rs`)
- Character target per Valhalla: **higher modal density than plate, slower density buildup, less coloration** — i.e. more/longer parallel lines (8–12) with high in-loop diffusion, minimal input diffusion, near-flat frequency response.
- Airwindows Chamber's **golden-ratio delay ladder** + feedforward Householder banks (MIT, translatable) is a proven "concrete cistern" voice; its Darkness-as-air-absorption (cascaded gentle IIR in the loop) fits BigSky Color. Map our `ChamberColor` profiles to loop-filter shapes (e.g. dark = 1970s bandlimit; gold = mild low-shelf boost + 6 kHz rolloff; bright = flat with HF T60 stretched).

**Plate — Small / Large** (`plate.rs` Dattorro, `plate_lexicon.rs`, `plate_progenitor.rs`)
- We already have the Dattorro core; verify against the exact spec in §8 (esp. the **negative −0.70 decay-diffusion-1 coefficients**, the ±0.6 seven-tap output structure, quadrature ~1 Hz/±8-sample AP modulation, allpass interpolation). Small vs Large = scale tank delays (×0.7 / ×1.3) keeping tap ratios, and retune damping.
- Realism gap vs BigSky/ValhallaPlate: **dispersion** — prepend a chirp allpass cascade (many short first-order APs, coef ~0.5–0.6) so highs lead transients; and **decouple LF decay** (low-shelf gain in the tank so lows ring longer at "dark plate" settings, shorter at "bright").
- MX plate can borrow kPlate ideas (MIT): 5×5 feedforward Householder pre-matrix for instant density, undersampled tank at 96k+.

**Cloud** (`cloud.rs`)
- Two portable blueprints to blend: **CloudSeed's seeded 12-line parallel bank with per-line post-diffusers** (what it was built for) and **Greyhole's diffuser→giant-modulated-loop** (MIT in Faust). Add Galactic2's **crossways-matrix stereo** for wide-mono-compatible output and Infinity-style feedback clamp for freeze. Random-walk jitter (reverbsc) rather than sine keeps it pitch-stable at long RTs; JPverb's sine mod (~0.1–2 Hz, depth to ~10–50 samples) is the "lush chorused" alternative worth exposing.

**MX vs Classic voice differentiation** (concrete, engine-agnostic)
- **MX (dense/smooth/modern)**: full line counts (8–12), interpolated fractional reads (cubic/allpass), random-walk or low-depth quadrature-sine modulation, three-band T60, full bandwidth, per-line EQ decorrelation, Hadamard/optimized matrices, dispersion on plates.
- **Classic (sparser/springier/vintage)**: fewer lines (4–6) or the **Keith Barr single allpass loop** topology (clean-room from the documented structure: 4 loop sections, one modulated AP, taps at 1.5/1.2/1.0/0.8 mix weights); **bandwidth cap 8–10 kHz in + damping high** (1970s/80s color modes); **linear or truncated (non-interpolated) delay reads** for interpolation grain/noise; stronger single-sine chorus (audible pitch undulation); Freeverb's +23-sample stereo spread instead of true decorrelation; optional magnitude-truncation/limit-cycle-adjacent grit (Dattorro §1.3.4 inverted — leave a little in).

## License triage summary

| Source | Status |
|---|---|
| CloudSeed / CloudSeedCore, Mutable eurorack, Airwindows, Freeverb (PD), STK, DaisySP, sndkit bigverb, Faust JOS section (zita_rev_fdn, freeverb, satrev), Faust jpverb/greyhole (declared MIT) | **PORTABLE** |
| zita-rev1 original C++, Dragonfly/Freeverb3, Csound sources, VCV Plateau, Faust `kb_rom_rev1` (GPL-3 section), GRAME-LGPL sections of reverbs.lib | **IDEAS ONLY** |
| Dattorro 1997 (+ Progenitor concept), Valhalla blog corpus, JOS PASP, Moorer/Jot/Gardner/Griesinger papers, Spin Semi forum | **FAIR GAME** |
| MI Beads reverb | unavailable (never open-sourced) |

Sources: [CloudSeed](https://github.com/ValdemarOrn/CloudSeed) · [CloudSeedCore](https://github.com/GhostNoteAudio/CloudSeedCore) · [pichenettes/eurorack](https://github.com/pichenettes/eurorack) · [airwindows/airwindows](https://github.com/airwindows/airwindows) · [kPlateA](https://www.airwindows.com/kplatea/) · [kPlate140](https://www.airwindows.com/kplate140/) · [Galactic](https://www.airwindows.com/galactic/) · [Galactic2](https://www.airwindows.com/galactic2/) · [Verbity](https://www.airwindows.com/verbity/) · [Chamber](https://www.airwindows.com/chamber/) · [Infinity2](https://www.airwindows.com/infinity2/) · [Undersampling](https://www.airwindows.com/tag/undersampling/) · [JOS Freeverb](https://ccrma.stanford.edu/~jos/pasp/Freeverb.html) · [STK FreeVerb](https://github.com/thestk/stk/blob/master/src/FreeVerb.cpp) · [STK NRev](https://github.com/thestk/stk/blob/master/src/NRev.cpp) · [sndkit bigverb](https://paulbatchelor.github.io/sndkit/bigverb/) · [sndkit repo](https://github.com/PaulBatchelor/sndkit) · [JOS Zita-Rev1](https://ccrma.stanford.edu/~jos/pasp/Zita_Rev1.html) · [faustlibraries reverbs docs](https://faustlibraries.grame.fr/libs/reverbs/) · [reverbs.lib source](https://github.com/grame-cncm/faustlibraries/blob/master/reverbs.lib) · [Dattorro Effect Design Pt 1 (PDF)](https://ccrma.stanford.edu/~dattorro/EffectDesignPart1.pdf) · [dragonfly-reverb](https://github.com/michaelwillis/dragonfly-reverb) · [Dragonfly manuals](https://michaelwillis.github.io/dragonfly-reverb/dragonfly-room-manual.html) · [Valhalla: metallic artifacts](https://valhalladsp.wordpress.com/2011/01/21/reverbs-diffusion-allpass-delays-and-metallic-artifacts/) · [Valhalla: diffusion & vocals](https://valhalladsp.wordpress.com/2010/06/07/diffusion-vocals-and-real-rooms/) · [Valhalla: Schroeder](https://valhalladsp.wordpress.com/2009/05/30/schroeder-reverbs-the-forgotten-algorithm/) · [Chambers vs Plates](https://valhalladsp.com/2015/11/07/chambers-versus-plates/) · [Physics of Plates](https://valhalladsp.com/2015/11/08/the-physics-and-psychophysics-of-plates/) · [ValhallaPlate Modes](https://valhalladsp.com/2015/11/08/valhallaplate-the-reverb-modes/) · [VintageVerb Modes](https://valhalladsp.com/2023/02/10/valhallavintageverb-the-modes/) · [Dark Room](https://valhalladsp.wordpress.com/2011/06/21/valhallaroom-v1-0-6-introducing-dark-room/) · [Reverb papers Pt 2](https://valhalladsp.com/2021/09/22/getting-started-with-reverb-design-part-2-the-foundations/) · [AES slides](https://valhalladsp.wordpress.com/2015/06/19/slides-from-my-aes-reverb-presentation/) · [Beads not open-sourced](https://www.modwiggler.com/forum/viewtopic.php?t=281283) · [Spin Semi history thread](http://www.spinsemi.com/forum/viewtopic.php?t=3)

Local files this maps onto: `/run/media/Development/herdr-worktrees/FastTrackStudio/worktree-rc-scratch/features/fx/reverb/reverb-dsp/src/algorithms/{room,room_studio,room_chamber,hall,hall_arena,hall_cathedral,plate,plate_lexicon,plate_progenitor,cloud}.rs`, `primitives/{allpass_diffuser,modulated_delay,reverb_line,householder,hadamard,multitap_delay}.rs`, `chain.rs`.

---

# Part 2 — FDN theory, specialized engines, convolution, metrics

# Reverb DSP Research — BigSky MX Parity (`features/fx/reverb/reverb-dsp`)

**License triage key** — **PORTABLE**: MIT/BSD/CC0/Apache/BSL code we can port. **IDEAS ONLY**: GPL/LGPL/AGPL/undeclared — study, never copy. **FAIR GAME**: papers/manuals/blogs — math and facts freely usable (clean-room implement).

**Codebase anchors** (all under `/run/media/Development/herdr-worktrees/FastTrackStudio/worktree-rc-scratch/features/fx/reverb/reverb-dsp/src/`): `primitives/fdn.rs` (Householder/Hadamard FDN, one-pole damping + 2-band decay split), `primitives/spectral_delay.rs` + `algorithms/spring.rs`/`spring_vintage.rs`, `algorithms/velvet.rs`, `ir/` (engine/transforms/prepared = Impulse), `algorithms/{bloom,cloud,chorale,magneto,nonlinear,shimmer,plate*}.rs`, and the existing metric harness `tests/ir_metrics.rs` (RT60/band-RT60/NED/worst-mode/LR-correlation already present).

---

## 1. FDN theory

### Jot & Chaigne 1991 — the canonical recipe (FAIR GAME; equations open via [JOS PASP](https://ccrma.stanford.edu/~jos/pasp/FDN_Reverberation.html))
1. Design a **lossless prototype** (delays `m_i`, unitary matrix), then insert **delay-proportional absorption**: per-line broadband gain **`g_i = 10^(−3·m_i / (fs·T60))`** — attenuation proportional to delay length gives a uniform dB/s decay on every signal path.
2. **One-pole shelf per line** for frequency-dependent T60: with `R0^Mi = 10^(−3·Mi/(fs·T60(0)))`, `Rπ^Mi = 10^(−3·Mi/(fs·T60(Nyq)))`:
   `p_i = (R0^Mi − Rπ^Mi)/(R0^Mi + Rπ^Mi)`, `g_i = 2·R0^Mi·Rπ^Mi/(R0^Mi + Rπ^Mi)`, `H_i(z) = g_i/(1 − p_i z⁻¹)`. Cheap approximation `R^Mi ≈ 1 − 6.91·Mi/(fs·T60)` valid when `fs·T60 ≫ 7`.
3. **Tonal correction filter** on the wet output: `E(z) = (1 − b z⁻¹)/(1 − b)`, `b = (1−α)/(1+α)`, `α = T60(Nyq)/T60(DC)` — flattens modal energy so decay changes don't recolor. Generalize: `|E(ω)|² ∝ 1/T60(ω)`.

### Accurate multi-band T60: Schlecht & Habets DAFx-17 → Two-Stage filter 2024
- [Schlecht & Habets DAFx-17](https://www.dafx.de/paper-archive/2017/papers/DAFx17_paper_11.pdf) (FAIR GAME): T60 error is **reciprocal** in filter-dB error — near-0 dB targets (long T60 / short line), a 0.2 dB fit error can flip the loop unstable. Fit **T60 error, not dB error**, with GEQ command gains constrained to ±10 dB ("TLSCon": 0% instability in Monte Carlo vs up to 20% for magnitude-LS). Uses Jot's proportional GEQ (self-similar: dB response scales linearly with command gain up to ~10 dB) — full biquad coefficient formulas reproduced in the paper.
- **Current best practice**: Välimäki/Prawda/Schlecht, ["Two-Stage Attenuation Filter"](https://acris.aalto.fi/ws/portalfiles/portal/140876460/ELEC_V_lim_ki_etal_Two-Stage_Attenuation_Filter_for_Artificial_Reverberation_2024.pdf), IEEE SPL 2024 (paper CC-BY FAIR GAME; [companion code](https://github.com/KPrawda/Two_stage_filter) GPL-3 IDEAS ONLY). Stage I = first-order low-shelf that *exactly* hits target gains at DC and Nyquist: `H_I(z) = G_H·[(G·u+√G)+(G·u−√G)z⁻¹]/[(u+√G)+(u−√G)z⁻¹]`, `G = G_L/G_H`, `u = tan(ωc/2)`; Stage II = octave/third-octave GEQ fitting the residual (now inside its accurate ±12 dB range). Design once for the **longest** delay line, scale every other line by exponentiation `G^(m_i/m_max)` (dB gains × m_i/m_max). Validated: ≤14% max T60 error over 1000 measured RIRs where GEQ-only goes unstable.

### Schlecht corpus (all FAIR GAME papers; [fdnToolbox](https://github.com/SebastianJiroSchlecht/fdnToolbox) is **GPL-3 → IDEAS ONLY**, use as verification oracle)
- [Dissertation](https://theses.eurasip.org/wp-content/uploads/sebastian-jiro-schlecht-feedback-delay-networks-in-artificial-reverberation-and-reverberation-enhancement.pdf) — the field map. [On Lossless FDNs](https://arxiv.org/abs/1606.07729): "unilossless" matrices are lossless for *any* delay lengths; unitary (Householder) qualifies — mix delays freely.
- **Time-varying feedback matrices** (JASA 2015 + "Practical considerations" AES 2015): slowly rotating the orthogonal matrix (`A(n) = R(θn)·A`) is the **least artifact-prone tail animation** — no decay-time error (modulated delays) and no stability risk (time-varying allpasses). This is the right "Mod" mechanism for our FDN engines.
- **Delay Feedback Matrices** ([WASPAA 2019](https://www.audiolabs-erlangen.de/resources/2019-WASPAA-DFM-FDN)): matrix entries = gain + short delay, `A(z) = D_m1(z)·U·D_m0(z)` → **2× faster mixing at ~zero cost** (reads land at offsets inside existing lines).
- **Scattering / Velvet Feedback Matrices** ([TASLP 2020, arXiv:1912.08888](https://arxiv.org/abs/1912.08888)): paraunitary FIR matrices `A(z) = D_mK·U_K···U_1·D_m0`; velvet-sparse construction (δ ≈ 1/30 pulses/sample, Hadamard mixing) gives FDN-16 density at N=4–8 cost; T60 error ≤8% for K≤2 (JND ≈5%); attenuation must count the FFM group delay: use `m + τ_L + τ_R` in the gain formula.
- Echo Density and Mixing Time (TASLP 2017): mixing time predictable from delays — shorter mean delay + more lines = earlier mixing.

### Colorless optimization — PORTABLE code
[diff-fdn-colorless](https://github.com/gdalsanto/diff-fdn-colorless) (**MIT**), papers DAFx-23 + ["Optimizing Tiny Colorless FDNs"](https://arxiv.org/abs/2402.11216) (FAIR GAME): gradient-optimize input/output gains and orthogonal matrix of the lossless prototype for spectral flatness + temporal density. Works at N=4. **Offline-optimize per-engine b/c constants and ship them.**

### Design numbers (synthesis; ["Fifty Years of Artificial Reverberation"](https://dl.acm.org/doi/10.1109/TASL.2012.2189567) as map)
- **N**: 16 = smallest "good" FDN; 32 fully dense; N=8–16 + velvet I/O or DFM exceeds FDN-32 density.
- **Delays**: mutually prime sample counts; `Σ m_i ≥ 0.15·T60·fs` (Schroeder mode-density criterion); spread ~1:1.5–1:2.5; healthy sets at 44.1 kHz: FDN16 primes 1721–4397 (39–100 ms), FDN32 primes 839–2237.
- **Matrices beyond Householder**: Hadamard (fast WHT), random orthogonal (best when optimized), circulant (FFT-diagonalizable; Galois circulants maximize echo density), sparse block-circulant (Anderson ICMC 2015 — raises min round-trip by N/4), DFM, VFM.

**→ Actionable for `primitives/fdn.rs`**: replace the Lp1+2-band split with per-line **one-pole shelf (exact Jot formulas)** minimum / **two-stage filter (shelf + octave GEQ, scaled per m_i/m_max)** for parity; add the tonal-correction one-zero on wet out; enforce `Σm_i ≥ 0.15·T60·fs` at max-decay presets; free upgrades in ascending effort: per-entry read offsets (DFM), velvet FIRs on input/output taps (outside loop, decay untouched), slow orthogonal rotation for Mod.

---

## 2. Velvet-noise reverbs (tuning `algorithms/velvet.rs`)

Key papers (all FAIR GAME, most CC-BY): [FVN, Appl. Sci. 2017](https://www.mdpi.com/2076-3417/7/5/483) · [IVN, TASLP 2021](https://ieeexplore.ieee.org/document/9360485/) · [VNFDN, DAFx-20](https://www.dafx.de/paper-archive/2020/proceedings/papers/DAFx2020_paper_23.pdf) · [Dark Velvet Noise, DAFx-22](https://dafx2020.mdw.ac.at/proceedings/papers/DAFx20in22_paper_31.pdf) · [perceptual density study, TASLP 2013](https://ieeexplore.ieee.org/document/6490018/).

- **Density**: ≥2000 pulses/s broadband (the IVN magic number is grid Td=20 @44.1k → **2205/s**); OVN placement (one jittered pulse per grid cell) is audibly the smoothest variant; only OVN beats Gaussian noise at 2000/s (others need 4000). Lowpassed velvet (fc 1.5 kHz) is smooth at just **600/s** → taper density in the darkened late tail.
- **Interleaved VN (the anti-flutter design)**: never recirculate one sequence. **M=4 branches suffice** (MUSHRA: 1 branch = "very annoying" 0, 4 = 82, 5 = 88). Branch = one delay line used twice (sparse FIR taps + feedback comb); lengths `L_i = C_i·(M·Td)` with distinct primes C_i (e.g. {97,101,103,107}×80 → 176–194 ms), each **>110 ms** or repetition is audible. Per-branch decay gain = Jot's formula `g_i = 10^(−3·L_i/(fs·T60))`; frequency-dependent decay = `|H_prot(ω)|^{L_i/fs}` via the two-stage filter (validated combo). Cost: ~40% of FDN-16.
- **Stepless decay trick**: split each period into 3 segments of **25/35/40%** length, attenuate seg 2 by 1/3 and seg 3 by 2/3 of the inter-step dB drop — +2 mults/branch.
- **Fade-in for free**: offset branch starts by multiples of the grid, level-compensate.
- **Dark Velvet Noise**: random-width rectangular pulses (one RRS filter per distinct width, 4 ops, leaky ε=2⁻¹²) ≈ −6 dB/oct rolloff; **widen pulses + thin density over the tail** = near-free progressive darkening.
- **Transient safety** (decorrelator papers, DAFx-17/18): velvet FIRs >10 ms need exponentially decaying pulse gains with per-pulse random gain in [0.5, 2.0]; optimized sequences reach <1 dB ripple.
- **Stereo**: permute branch interleave order per channel → decorrelated outputs free.
- **VNFDN**: split velvet taps half input / half output of an FDN → ×M² echoes; FDN16+VN15 beats FDN32 density at 52% cost saving.

---

## 3. Spring (validates + extends `algorithms/spring.rs`)

Our crate already implements the right lineage (Välimäki/Parker/Abel parametric model). The research adds calibrated constants and the physics:

- **Physics** ([Parker & Bilbao DAFx-09](https://www.dafx.de/paper-archive/2009/papers/paper_84.pdf), FAIR GAME): IR splits at transition frequency **F_C** (≈2–5 kHz; rarely above 5k): below = dominant dispersive **upward** LF chirps with echo period **T_D ≈ 30–60 ms**; above = a faster, ≥30 dB quieter echo train. Formulas: `T_D ≈ 4LR/(√(E/ρ)·r)`, `F_C ≈ √(E/ρ)·3r/(16√5·π·R²)`. Two multi-spring voicing strategies from measured units: same T_D/different F_C (Olson) vs range of T_D/same F_C (Leem — matches BigSky "interaction of the different delay times").
- **Calibrated model** ([Gamper/Parker/Välimäki DAFx-11](https://www.dafx.de/paper-archive/2011/Papers/39_e.pdf), FAIR GAME — reproduces the JAES 2010 structure with a full parameter table, Leem KA-1210): C_lf loop = 40 Hz DC block → **M=100** stretched first-order allpasses, **a₁ ≈ +0.62**, stretch **K = fs/(2·F_C)** (fractional K via allpass interpolator) → chirp EQ (stretched 2nd-order resonator, **F_peak 183 Hz, B 146 Hz**) → randomly modulated multitap delay → **g_lf ≈ −0.8 (negative — alternating-polarity echoes = the drip)**. Optional C_hf: M=200, K=1, a=−0.6, g=−0.77, ≥30 dB down (droppable for Eco). Elliptic LP at F_C on the tap.
- **Efficiency** ([Parker EURASIP 2011, CC-BY](https://asp-eurasipjournals.springeropen.com/articles/10.1155/2011/646134)): group delay `D(ω) = kM(1−a²)/(1+2a·cos(ωk)+a²)` sizes M; chirp-straightening (k→2k, a→−a, M→M/2 + 8th-order L-R crossover at F_C/2); run C_lf at **fs/4** → 1023 → **355 mults/sample** (C_lf alone 600→75).
- **Dwell** (Fender 6G15 lore, [Premier Guitar](https://www.premierguitar.com/pro-advice/on-the-bench/fender-6g15-reverb-unit) + BigSky MX manual verbatim): Dwell = **drive into the tank preamp**, i.e. a tanh saturator **before** the chirp loop, input-level-dependent — not loop feedback. Map Clean/Combo/Tube/Overdrive ≈ 1×/2×/4×+asym/8×. Decay knob decouples from spring length: `g_lf = 10^(−3·T_D/T60)`.
- **MX vs Classic voice**: MX = stronger pre-echo/drip + dynamic saturation, F_C ≈ 4.3 kHz; Classic = louder C_hf "rattle" path, slightly lower F_C, transient-keyed micro-modulation bursts.
- **Open code**: no portable dispersive spring exists. ChowDSP BYOD spring (GPL-3, IDEAS ONLY) confirms nice tricks (nested 2nd-order allpasses double dispersion/stage; tanh inside loop; Householder "reflection network" side path). Faust `re.springreverb` is non-dispersive and license-undeclared → IDEAS ONLY.

---

## 4. Plate

- **Physics** (Bilbao JAES 2007; Arcas & Chaigne Appl. Acoustics 2010; constants cross-checked in [Russo's open thesis](https://projekter.aau.dk/projekter/files/517547034/Master_Thesis_Russo.pdf) — all FAIR GAME): Kirchhoff plate, dispersion **ω = κβ²** → group velocity ∝ √f → **highs arrive first: downward chirps** (opposite of spring). EMT-140: 2 m × 1 m × 0.5 mm SAE-1010 steel (E=2e11, ν=0.3, ρ=7872) → κ ≈ 0.763 m²/s, first arrival ≈1 ms, last ≈30 ms; **modal density constant ≈ 1.31 modes/Hz** (`n(f) = (A/2)·√(ρh/D)`); undamped T60 ≈ 5 s; damping plate (0.5–6 cm gap) dominates 300 Hz–5 kHz. ~600 Bark-capped modes run real-time (Ducceschi & Webb).
- **What separates "plate" from generic FDN**: (1) instant echo density (heavy input diffusion, no ER gap), (2) flat ~1.3 modes/Hz density, (3) T60(f) with long low-mid plateau + thermoelastic HF rolloff + mid radiation dip, (4) **downward-chirp dispersion** — add per-line first-order allpass cascades tuned so LF group delay exceeds HF by up to ~30 ms on first pass (Parker's D(ω) formula sizes it; opposite sign to spring), (5) stereo via disjoint output tap sets, not mirrored networks.
- **Dattorro "Effect Design Part 1" complete constants** (FAIR GAME, [CCRMA PDF](https://ccrma.stanford.edu/~dattorro/EffectDesignPart1.pdf); fs_ref = 29761 Hz): input APs 142/107/379/277 @ 0.75/0.75/0.625/0.625; figure-8 tank L: modAP 672±16 @ **−0.70**, delay 4453, AP 1800 @ +0.50, delay 3720; R: 908±16 @ −0.70, 4217, 2656 @ +0.50, 3163; decay 0.5, damping 0.0005, bandwidth 0.9995; LFO ≈1 Hz, ±8 samples, quadrature; full stereo output tap tables in the paper (left output taps mostly the right branch — that's the decorrelation).
- **Open code**: CloudSeed (**MIT, PORTABLE**) for tank plumbing; Airwindows kPlate140/kPlate240 (**MIT**) — EMT-targeted 5×5 Householder with searched matrices; Faust `re.dattorro_rev`, `re.zita_rev1` (STK-4.3 MIT-style, PORTABLE); Valley Plateau (GPL-3, used only to verify constants).

**→ Our `plate*.rs`**: verify tank constants against the Dattorro table; add the dispersion-allpass seasoning + thermoelastic T60(f) shape to distinguish MX Large (clean ideal plate) from Classic voices (damping/HP/diffusion tweaks per MX manual language).

---

## 5. Gated / NonLinear / Reverse

- **RMX16 truth** ([AMS manual](https://media.uaudio.com/support/manuals/dd/AMS%20RMX16%20Expanded%20Manual.pdf), FAIR GAME): NonLin/Reverse programs are **non-recirculating multitap diffuser banks with stepped tap-gain envelopes** — no detector/gate → level-independent, repeatable, poly-safe. NONLIN 1 shipped two character outputs (discrete vs diffused tap sets). IMAGE P1 panned the burst across stereo over ~0.6 s.
- **Costello's confirmation** ([KVR](https://www.kvraudio.com/forum/viewtopic.php?t=324559), [ÜberMod TAPS](https://valhalladsp.com/2012/01/26/valhallaubermod-the-taps-parameters/)): delay lines + many output taps, gains arranged as fade-in/slice envelopes, diffusors in front, tap randomization >0 to avoid metallic ringing, "a bit of grain is part of the classic sound".
- **BigSky MX manual verbatim mapping**: Shape ∈ {Swoosh, Reverse, Ramp, **Gate** (even profile, abrupt cut), **Gauss** (bell), **Bounce** (**inverted bell** — loud-soft-loud, not bouncing-ball)}; Decay knob = TIME (nonlinear portion length); Pre-Delay knob = **FEEDBACK around the generator**; Diffusion (min = "grainy"); Chop = tremolo over the shaped section; separate **late reverb stage** with own Decay/Level (off at min); Mod modulates tap lengths + late-tank delays — explicit confirmation of the multitap architecture.
- **Envelope LUTs** (x = t/T): Gate `1` w/ half-cosine knee over last 5–10%; Ramp `x`; Reverse `x^p, p≈2–3`; Swoosh = smoothstep + per-tap LP cutoff rising with x; Gauss `exp(−(x−0.5)²/2σ²), σ≈0.15–0.2`; Bounce `1−(1−m)·exp(−(x−0.5)²/2σ²), m≈0.1–0.3`. Bank: 24–32 taps/channel over 100–800 ms, jittered ±10–20% (per preset, not per trigger), disjoint L/R sets, per-tap damping darker toward the tail, soft-limit inside the feedback loop.
- No faithful open RMX16 clone exists; free NonLin2 IRs (Nevo/Tomkin) are exact **fit targets** since a linear FIR bank's IR is its taps. Airwindows Chamber (MIT) is a feedforward Householder bank structurally close to the burst generator.

**→ Our `nonlinear.rs`**: check tap jitter is preset-stable, Bounce = inverted bell, gate knee is soft, and adopt the per-tap rising-LP for Swoosh; consider the IMAGE-style pan sweep as an easter egg.

---

## 6. Chorale (formant/choir)

- **Vowel tables** ([Csound manual Appendix D](https://csound.com/docs/manual/MiscFormants.html), FAIR GAME — measured data): e.g. **Tenor "ah"** F=650/1080/2650/2900/3250 Hz, A=0/−6/−7/−8/−22 dB, BW=80/90/120/130/140 Hz; **Tenor "oh"** 400/800/2600/2800/3000, 0/−10/−12/−12/−26, 70/80/100/130/135; **Tenor "oo"** 350/600/2700/2900/3300, 0/−20/−17/−14/−26, 40/60/100/120/120; **Bass "ah"** 600/1040/2250/2450/2750, 0/−7/−9/−9/−20, 60/70/110/120/130; Bass "oh" 400/750/2400/2600/2900; Bass "oo" 350/600/2400/2675/2950. F1/F2 carry vowel identity; **keep the fixed 2.2–3.3 kHz F3–F5 "singer's formant" cluster** — it's what reads as *sung*.
- **Filter bank**: parallel 2nd-order constant-peak-gain resonators (`R = exp(−π·BW/fs)`), amp column applied directly; **morph in log-frequency** (lerp log2 F, log BW, dB amps per formant index), one-pole smooth 10–30 ms.
- **FOF** (Rodet/CHANT, FAIR GAME; Csound `fof` semantics: rise 3 ms / dur 17 ms / decay 7 ms; decay rate = π·BW; "octaviation" = attenuate odd grains) and **VOSIM** (Kaegi & Tempelaars JAES 1978 — cheapest formant technique; Plaits has a VOSIM engine) — pitch-locked vowel generators: formants don't shift with pitch, unlike pitch-shifting the input.
- **Choir numbers** ([Ternström](https://www.diva-portal.org/smash/get/diva2:1416134/FULLTEXT01.pdf), FAIR GAME): unison spread **25–30 cents**; per-voice static detune ±10–15 c + **independent** 0.5–8 Hz f0 flutter (~3–6 c RMS — the summed AM of independent flutter *is* the choir cue); vibrato 5–6.5 Hz ±10% with randomized onset delay; onset scatter ~20–60 ms.
- **PORTABLE code**: Mutable Plaits speech/two-formants engines (**MIT**), mda TalkBox/Vocoder (**dual MIT/GPL — take MIT branch**). Csound/soundpipe-`fof` (Csound port) = IDEAS ONLY; Faust physmodels formant functions are MIT-style but check per-function headers.

**→ Our `chorale.rs`**: MX manual says Mod "adds randomization to pitch and timbre — an increasing number of singers with distinct voices" — exactly the Ternström independence model; verify our voices have *independent* flutter, bass/tenor octave layering from the tables above, and singer's-formant cluster retention across the vowel morph.

---

## 7. Cloud Ensemble + Bloom harmonics + Shimmer + Magneto

- **Cloudburst Ensemble, publicly documented** ([Strymon](https://www.strymon.net/product/cloudburst/), [SoS review](https://www.soundonsound.com/reviews/strymon-cloudburst-plug), FAIR GAME): "continuously analyzes **48 frequency bands**, generates corresponding **upper harmonic partials** of what it finds in each band"; "more akin to polyphonic additive synthesis". I.e. **channel-vocoder-driven additive resynthesis on a harmonic grid**: per-band envelope followers (slow attack 50–300 ms) → partial oscillators at 2×/3×/4× band centers, amplitude = band env × HF rolloff. Inherently polyphonic with **no pitch estimation at all**. Upgrade path if true chord-locking wanted: [Klapuri iterative multi-f0](https://www.ee.columbia.edu/~dpwe/papers/Klap03-multif0.pdf) (spectral-smoothness subtraction, FAIR GAME).
- **String-machine ensemble** (Haible Solina analysis + [Valhalla ÜberMod post](https://valhalladsp.com/2012/03/09/what-is-ubermod/), FAIR GAME): **3 BBD lines 1.5–5 ms; two 3-phase LFOs (~0.6–0.7 Hz "Chorus" + ~5–6 Hz "Vibrato") at 0°/120°/240°, each line modulated by the sum of its phase pair; wet-only sum; stereo = independent per channel**. The 120° distribution cancels common-mode pitch wobble. **PORTABLE code**: Plaits **string-machine + chord-organ engines (MIT)**, jpcima/ensemble-chorus (**BSL-1.0**), Clouds/Beads granular (MIT).
- **Bloom**: Faust **`re.greyhole` is MIT** (Julian Parker, verified in file header — the sc3-plugins original is GPL; use the Faust version): stereo 4-level nested-allpass diffuser with rotation → loop of two quadrature-modulated short delays + one long size-scaled delay → damping → feedback. Harmonics = POG-style parallel dual-tap delay-line shifters (+12/+19/+24), 15–25 ms windows, 180° taps, equal-power crossfade ([Dattorro Part 2](https://ccrma.stanford.edu/~dattorro/EffectDesignPart2.pdf), Lent 1989 — FAIR GAME), level-enveloped by an input follower.
- **Shimmer** ([Valhalla history posts](https://valhalladsp.com/2010/05/11/enolanois-shimmer-sound-how-it-is-made/), FAIR GAME): loop = shifter(+12) → reverb → **HPF ~200–300 Hz + LPF ~4–8 kHz** → gain ≲0.7; dual mode adds +7 (fifth) or −12; **randomize grain splice timing ±few ms** in the in-loop shifter (Costello's trick: converts comb glitches into noise the reverb absorbs); 50–120 ms pre-delay for the classic swell.
- **Magneto**: RE-201 = 3 equally-spaced heads → **delay ratios 1:2:3** (max ≈600–650 ms on head 3); MX extends to 6 heads (1:…:6, slightly irrationalized) + Diffusion. Wow ≈0.5–1 Hz periodic ≤0.3% speed; flutter ≈5–10 Hz random ≤0.1% (+ tiny 40–100 Hz scrape); global motor-speed scales all taps with slewed repitch. Saturation: [Chowdhury DAFx-19 tape paper](https://ccrma.stanford.edu/~jatin/420/tape/TapeModel_DAFx.pdf) (FAIR GAME — Jiles-Atherton hysteresis ODE + play-head loss filters; the AnalogTapeModel code is GPL-3 IDEAS ONLY); pedal-budget alternative: tanh+bias with NAB-style pre/de-emphasis (+6 dB/oct above ~3 kHz record, inverse playback). **Saturator must sit inside the feedback loop** with a ~100 Hz–5 kHz bandpass per pass so >1 feedback self-oscillates musically.

---

## 8. Impulse (partitioned convolution + live reshaping) — maps to `ir/`

- **Canon** (all FAIR GAME): Gardner 1995 zero-latency doubling scheme (direct FIR head + N,N,2N,2N,4N,4N… uniform-load variant); [García 2002](https://www.angelofarina.it/Public/AES-113/Garcia-PrePrint5660.pdf) — uniform-partitioned OLS with **frequency-domain delay line**: `O_FDL(N) = 4k·log2(2N) + 4T/N`, closed-form optimum **`N_opt ≈ 0.693·T/k`**; optimal multi-FDL >2× cheaper than Gardner doubling; [Wefers thesis (open access PDF)](http://publications.rwth-aachen.de/record/466561/files/466561.pdf) — the definitive reference incl. **K=2B FFT size is generally right** and threaded-tail scheduling.
- **Verified licenses**: [FFTConvolver](https://github.com/HiFi-LoFi/FFTConvolver) **MIT**; its Rust port [fft-convolver](https://github.com/neodsp/fft-convolver) **MIT** (RT-safe contract: process/set_response allocate nothing); [zones_convolver](https://github.com/zones-convolution/zones_convolver) **MIT** — Garcia-optimal NUPC with **time-distributed sub-transforms** (the single-threaded/wasm answer: big FFTs decomposed across process calls, no load spikes); RustFFT/realfft **MIT/Apache-2** with `wasm_simd`. KlangFalter/zita-convolver/jconvolver/TVOLAP = GPL/LGPL, IDEAS ONLY.
- **Live reshaping** (Wefers & Vorländer 2014 frequency-domain crossfading; TVOLAP paper FAIR GAME): (1) **per-partition complex gain** on the stored spectra — scalar-per-partition gain is *exactly* time-segment gain (linearity), so decay/damp/early-late reshaping costs **zero extra FFTs**; smooth g_p with a one-pole τ ≈ 20–50 ms; frequency-dependent tilt per partition gives Damp; (2) **double-engine equal-power crossfade 20–100 ms** for IR swaps/topology changes.
- **WASM**: AudioWorklet is threadless without COOP/COEP SharedArrayBuffer — plan single-threaded: head FDL at the 128-sample worklet quantum, tail 4096–16384 with time-distributed transforms, SIMD128 via `realfft/wasm_simd`.

**→ Our `ir/engine.rs` + `transforms.rs`**: our reshaper worker + per-transform pipeline aligns with (1)+(2); worth auditing that decay/tail/stretch transforms run as per-partition spectral gains where possible rather than full IR re-FFT, and that direction/stretch changes go through the dual-engine crossfade.

---

## 9. Objective metrics for the A/B dial-in harness

- **Normalized echo density** (Abel & Huang 2006, [CCRMA PDF](https://ccrma.stanford.edu/courses/318/mini-courses/rooms/mus318_Abel_Lecture/echo%20density.pdf), FAIR GAME) — exact: Hann-weighted 20–30 ms window, `η(t) = [1/erfc(1/√2)]·Σ w(τ)·1{|h(τ)| > σ}`, `σ = √(Σ w·h²)`, `erfc(1/√2) ≈ 0.3173`; η→1 = Gaussian late field; **mixing time = first η=1 crossing; rise rate = diffusion signature**. (Already in `tests/ir_metrics.rs` — confirm the erfc normalization and Hann weighting match.)
- **EDC family** (ISO 3382, FAIR GAME): Schroeder backward integration with noise-floor truncation (Lundeby) **before** integrating; **T30 fit −5→−35 dB / T20 −5→−25**; **EDT 0→−10 dB** (better tracks *perceived* reverberance — add it, we only have RT60 now); octave-band Butterworth filtering first; `C50/C80 = 10log10(early/late @ 50/80 ms)`; IACC/L-R cross-correlation (±1 ms lag max) early (0–80 ms) vs late as the width metric.
- **Coloration**: σ_dB of late-tail magnitude spectrum + late spectral centroid trajectory; Schlecht's modal-excitation-distribution work if we ever need more ([DAFx-23 colorless](https://www.dafx.de/paper-archive/2023/DAFx23_paper_32.pdf)).
- **Modulation caveat — the important one**: BigSky tails are modulated → not LTI, a single ESS "IR" is a snapshot. Capture **M = 4–8 repeated sweeps**; average energy metrics; **inter-repeat coherence decay over time IS the Mod-knob metric** (depth from coherence, rate from band-envelope flutter spectrum). Compute NED/spectra per sweep and average.
- **Capture** ([Farina ESS](https://www.melaudia.net/zdoc/sweepSine.PDF), FAIR GAME): 20 Hz–20 kHz exponential sweep, 10–20 s, −12 dBFS, 48 kHz, ≥2 s silence past max decay, deconvolve each stereo channel with the amplitude-compensated inverse sweep (distortion lands at negative time — crop), 100% wet, log all knob positions.
- **Tooling licenses (verified)**: pyroomacoustics **MIT** (`experimental.rt60.measure_rt60`), python-acoustics **BSD-3** (archived; maintained successor [acoustic-toolbox](https://github.com/Universite-Gustave-Eiffel/acoustic-toolbox)), librosa ISC, scipy BSD. **No credible Rust room-acoustics crate exists** — our in-repo Rust metrics in `ir_metrics.rs` are the right call; extend from the formulas (no license entanglement).
- **Knob→metric map for dial-in**: Decay→broadband T20; Lo/Hi Damp→RT60(f) tilt + centroid slope (EQ vs damping disambiguation: EQ shifts all t equally, damping shifts RT60(f)); Diffusion→NED rise/mixing time; Pre-Delay→EDC onset; Mod→inter-sweep coherence; width params→early/late L-R correlation. Scalar distance = weighted sum starting at w = (3·RT60(f), 2·NED, 2·EDC, 1·C80, 1·IACC, 1·centroid).

---

## Cross-cutting license summary

**PORTABLE (usable code)**: fft-convolver, FFTConvolver, zones_convolver, RustFFT/realfft, diff-fdn-colorless, CloudSeed, Airwindows (all, incl. kPlate140), Faust `re.greyhole`/`re.jpverb`/`re.dattorro_rev`/`zita_rev1`, Mutable Plaits/Clouds (MIT), mda plug-ins (MIT branch), jpcima/ensemble-chorus (BSL-1.0), pyroomacoustics/python-acoustics/librosa (harness).
**IDEAS ONLY**: fdnToolbox, Two_stage_filter repo, KlangFalter, zita/jconvolver, TVOLAP, BYOD/ChowDSP tape, Valley Plateau, Dragonfly, Surge, OB-Xd, sms-tools, Csound + soundpipe's `fof` module, Faust `re.springreverb` (undeclared).
**FAIR GAME**: every paper/manual/blog cited — including all load-bearing constants above (Jot filter formulas, two-stage shelf, IVN parameters, DAFx-11 spring table, Dattorro tank tables, RMX16/BigSky MX manuals, Csound formant tables, Solina LFO analysis, Garcia/Wefers cost formulas, Abel-Huang NED, Farina ESS).

### Highest-leverage next steps for our crate
1. **Per-line two-stage attenuation filters** in `primitives/fdn.rs` (replaces the 2-band split; biggest fidelity jump across Hall/Room/Plate) + Jot tonal-correction on wet out.
2. **Slow orthogonal-rotation Mod** for FDN engines (artifact-free tail animation, matches BigSky Mod behavior better than delay modulation).
3. **Velvet**: verify ≥2000 pulses/s early / interleave ≥4 prime-length branches >110 ms / 25-35-40% segmented decay; adopt DVN pulse-widening for late-tail darkening.
4. **Impulse**: ensure reshape transforms are per-partition spectral gains + dual-engine crossfade; adopt time-distributed tail FFTs for wasm.
5. **Metrics harness**: add EDT, C80, mixing-time-from-NED, late-tail σ_dB/centroid, and the **multi-sweep coherence** pipeline for pedal captures (the one thing `ir_metrics.rs` structurally lacks, since it currently renders LTI IRs only).
