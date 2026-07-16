# Cinematic Studio Strings — Engine Matching Progress

Goal: make our `signal-sampler` engine play the **Cinematic Studio Strings (CSS)
1st Violins** patch as close to the real Kontakt instrument as the live use case
needs (the `just strings` rig). We drive a comprehensive test MIDI through both
the real CSS (rendered by the user in Kontakt 8, stock default, Mix mic, reverb
0) and our engine, then compare.

## Ground truth (from the real CSS manual)

Pulled the actual CSS Manual PDF + v1.7 release notes. Key specs (also saved to
agent memory as `css-engine-spec`):

- **CC map (defaults):** CC1 = Velocity X-Fade (dynamics for longs; short-TYPE
  selector for shorts), CC2 = Vibrato X-Fade (0 = non-vib), CC5 = Portamento
  Volume, CC11 = Volume, CC58 = Key Switch CC, CC59 = Round Robin Reset.
- **CC58 keyswitch → articulation (verified exact):** 0-5 LowLatencyLegato,
  6-10 ExpressiveLegato, 11-15 Spiccato, 16-20 Staccatissimo, 21-25 Staccato,
  26-30 Sfz, 31-35 Pizzicato, 36-40 Bartok, 41-45 ColLegno, 46-50 Trills,
  51-55 Harmonics, 56-60 Tremolo, 61-65 MeasuredTremolo, 66-70 Marcato(no
  overlay), 71-75 Marcato(overlay), 76-80 LegatoOn, 81-85 LegatoOff,
  86-90 ConSordinoOn, 91-95 ConSordinoOff.
- **Mix mic** = pre-baked stereo blend of all 4 mic distances, single file,
  default; can't combine with other mics. (Our solo-"Mix" is correct.)
- **Reverb default = 0** (dry library).
- **Short notes:** 4 dynamics (pp/p/mf/ff) by VELOCITY, up to 5 RR, a built-in
  60 ms sample-start→peak delay.
- **Legato:** recorded on the whole-tone grid {A,B,C#,D#,F,G}; transitions are
  whole-step, **source-labelled** ("up_C#" = C#→D#). Latency by velocity zone:
  Expressive 333/250/100 ms (vel 0-64/65-100/101-127); Low Latency 150/100 ms.
- **Portamento:** legato velocity ≤ 20 (default), volume via CC5.
- **Per-articulation attack/release** stored independently.

## Bugs found and fixed

All in `crates/signal-sampler/src/engine/`:

1. **−3 dB stereo center-pan** — `with_pan` applied an equal-power law (0.707 at
   center) meant for mono sources to CSS's already-stereo samples. Now a balance
   law (unity at center for stereo; equal-power kept for mono). `voice.rs`.
2. **Shorts velocity² double-attenuation** — zoned shorts applied a velocity²
   gain on top of dynamic-layer selection (vel20 → −32 dB, near silent). Now
   unity for multi-dynamic shorts (mirrors the non-zoned path's n_dyn rule).
3. **Sustain CC1 floor double-attenuation** — `cc1_expression` floor (−18 dB)
   stacked on already-quiet low-dynamic layers. Lowered to −12 dB to match CSS's
   measured curve; the "jumpy / distinct change at CC1≈90" is gone — the curve is
   now smooth, monotonic, and shape-matches CSS.
4. **+6 dB output makeup** — CSS plays samples ~+6 dB above their raw file level
   (Kontakt instrument output). Added a global makeup so our default level lands
   on CSS's. Sustain now matches CSS within ~2 dB across the CC1 sweep.
5. **Silent release tail** — recorded release samples (NVrel/Vsusrel) were never
   warmed → cache-miss → silent cutoff. `warm_note` now also warms the release
   (and legato/portamento) articulations. Release samples are played at a trimmed
   gain (`RELEASE_GAIN`) so they sit under the note instead of spiking.
6. **Sluggish sustain attack** — we played the sample's slow ~0.8 s natural
   pre-roll; reverted to playing from the sample start with the configured attack
   (the live rig defaults to 20 ms attack / 400 ms release).
7. **Legato was completely silent** — the legato branch required `kind ==
   Legato`, but CSS's Expressive Legato selects the **Sustain** articulation
   ("Nonvib"). Fixed routing so a sustain played legato fires the Leg/NVLeg
   transitions (CSS's model).
8. **Legato doubling ("chorus")** — `fire_legato` played the long-form Leg sample
   (transition + sustained note) AND a separate sustain body → two detuned notes.
   Now the transition carries the held note (loops a stable mid-sustain region),
   no doubling.
9. **Legato pitch a whole step too high** — CSS legato samples are source-
   labelled; we looked them up by destination and pitched by `to - root_key`, so
   C→D came out as D→E. Now we select the source-side sample and apply a pitch
   offset so the transition's END lands on the target, while keeping the voice
   tagged with the target note (so note-off/silence find it).
10. **Note-off click / loop run-off** — a looped voice stopped looping on release
    and ran into the sample's abrupt end (click). Now loops continue while
    releasing so the voice fades smoothly.
11. **Low-latency legato velocity zones inverted** in the styx config (softer
    should be slower). Fixed in `sample-collector/specs/cinematic-strings.styx`.

## New capability

- `SamplerRig::set_forced_rr(id, Some(slot))` — pin RR per trigger (engine →
  block → bank → rig). With CC59 reset (already implemented) this matches CSS's
  own deterministic RR mechanism.

## Current match state

- **Sustain CC1 dynamics:** shape matches CSS, level within ~2 dB; smooth curve.
- **Short articulations:** spectral cosine vs CSS default, mean **0.874**
  (Col Legno/Bartok/Staccatissimo ~0.92; Spiccato ~0.82, the hardest transient —
  its level and envelope match, the residual is inherent transient timbre).
- **Legato:** fires correctly (was silent), single clean voice (no chorus),
  correct pitch both directions, broadband level tracks CSS.
- **Attack/release:** fast attack matching CSS; release tail tracks CSS for the
  first ~0.2 s.

## Known limitations / next steps

- A *sample-exact* (phase-null) match between the two engines is not achievable:
  CSS bakes its amp envelope + output makeup into the render, and round-robins
  are unresolvable from audio (variants are ~3 % apart; the manual says any RR
  sounds fine). We use a **spectral** comparison instead.
- Held legato notes currently lock to one dynamic (no live CC1 swell *during* a
  held legato note). The proper next step is crossfading the transition into the
  full sustain body so CC1/CC2 keep working on held legato notes.
- Spiccato timbre (~0.82) could be pushed closer with finer transient/attack
  alignment.
- CSS's slight attack *bloom* and the deeper release-tail level aren't fully
  replicated.

## Tooling (examples, `crates/signal-sampler/examples/`)

- `gen_css_test_full.rs` — generates the comprehensive ~9.8-min test MIDI (13
  sections: short velocity sweeps, RR exposure, range, pitch-shift probe, sustain
  CC1/CC2 sweeps, attack/release isolation, longs, legato latency/intervals,
  portamento, re-bow). Prints a timestamped manifest.
- `render_css_test.rs` — renders a test MIDI through our engine offline → WAV.
- `spectral_ab.rs` — per-articulation spectral-similarity scorecard vs a CSS
  render (FFT log-band, floored, mean-centered cosine).
- `null_css_rr.rs` / `null_ceiling.rs` — RR null sweeps + the phase-null ceiling
  check that proved phase-nulling isn't viable.
- `rr_cycle.rs` — RR-cycle resolvability analysis.
- `legato_test.rs` / `legato_pitch.rs` — focused legato diagnostics (voice
  counts, level, pitch).
- `strings_selftest.rs` — offline smoke test of the rig.

Workflow: `gen_css_test_full` → user renders through CSS → `render_css_test` for
our version → `spectral_ab` / targeted sox measurements to compare and tune.

## 2026-07-15 — legato defined by the soundpack (issue 020bc328)

All decoded CSS policy moved out of engine constants into the soundpack
spec (`features/rigs/orchestra/specs/cinematic-strings.styx`, now in-repo):
performance{} (makeup, master tune, note-off fades, attack/release),
legato_engine{} (velocity splits, Overlap-Delay + $1fvjk start-offset IOI
curves, retire crossfades, $3tsb0 sustain trim), dynamics.cc1_expression,
per-articulation amp_env (ENV_FLEX) and transition-selection tags
(legato_role / vibrato_pair / sordino_pair). Engine keeps mechanism only;
defaults equal the old constants (refactor verified ≤1 int16 LSB on the
full A/B render; scorecard digit-identical: timbre 0.963, mean|level|
3.4 dB, short-onset 23 ms, MATCH 38/62 onset-gated).

Fixes found by the new tests: (1) inferred Tremolo↔Nonvib CC2 mispairing —
now explicitly disabled via `vibrato_pair ""`; (2) offline prefire leads
were the raw measured arrival − $1fvjk, so outlier lead-in measurements
(>1 s on some down intervals) pulled the whole legato handoff a second
early — now capped at the mode's velocity-zone delay (arrival still lands
ON the tick via start_offset).

New regression tests: schedule-level (document.rs — spec curves drive the
prefire leads; authored curves honored) and audio-level with the real
library (orchestra tests/legato_lookahead.rs — offline pre-rolls + zero
reactive fallbacks + join continuity; StrictLive commits reactively within
the Overlap-Delay bound). Authoring guide:
`features/sampler/signal-sampler/docs/orchestral-pack-authoring.md`.
