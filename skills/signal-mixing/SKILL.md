---
name: signal-mixing
description: Use when building, editing or dialling in a mix in Signal (a drum kit first): laying out its tracks and chains, choosing a block and its phase, making phases gain neutral, or setting compression, EQ, saturation, parallel filters or verbs.
---

# Signal mixing

Every mix in Signal is built on one method: a **tree** of tracks, each
track's chain in **phases**, every phase **gain neutral**, the balance
done on the faders. The structure comes first, settings come after.
Studio shows the same phases (`studio/features/session/src/mix_phases.rs`,
by the same slugs), so a chain built here reads the same there.

## Steps

1. **Lay out the tree.** Start from the golden session's shape
   (`studio/features/dynamic-template/src/golden_session/shape.rs`) and the
   song's own session where there is one. Done when every captured track
   (each mic, each trigger) has its lane, every lane is named by its path
   (`Kick/SUM/In`), and every folder, Sum, verb bank and Process lane is in
   place. A drum kit: [`signal_core::kit::drum_kit`](../../features/core/src/kit.rs).
2. **Give each track the chain its role allows** (the table under
   *Roles*). Every block carries its phase (`BlockSpec::phase`), and a
   lane's blocks run in phase order. Done when the build reports no
   "out of the mix's phase order" warning.
3. **Make every phase gain neutral.** Render the song, read what each
   phase did (`Graph::phase_levels`), apply the make-ups
   (`graph::apply_neutral_trims`), and repeat until the trims stop moving
   (three passes; `SIGNAL_KIT_CALIBRATE=3` in `examples/render_kit.rs`,
   or each instance's `Calibrate` switch live). The meters are K-weighted,
   so neutral means 0 LU on a loudness meter.
   Done when the last pass moves no trim more than 0.25 dB and nothing
   hit the ±24 dB cap (a capped phase is a broken block: fix it).
4. **Gain stage, then balance on the faders** (`LaneSpec::gain_db`). Take
   the session's fader levels where it has them. No block does the
   balance's job.
5. **Dial in, phase by phase, in order:** Rescue, then Tone, then Polish,
   Relational, Depth. Re-run step 3 after each phase. A later phase never
   fixes an earlier one: a smeared kick is a Rescue problem however much
   Tone is thrown at it.
6. **Compare** against references only once Tone is right: the separated
   reference stems (`/run/media/AudioHaven/fts-corpus/mix-references/stems/`),
   piece against piece (kick against kick), level-matched.

## Phases

| Phase | Where | What |
|---|---|---|
| **Rescue** | offline, then live | Offline: phase/polarity check, time alignment, de-click/de-clip, tuning, **consistency compression** (levelling). Live: **Rescue EQ** (corrective), the trigger-keyed gate. |
| **Balance** | the faders | Gain staging, then levels. Not an FX group: it has no blocks. |
| **Tone** | live | **Stylized compression** (groove, punch, thickness: never levelling), **tonal EQ** (what it should sound like), **saturation**, **parallel filters**, **parallel compression**. |
| **Polish** | live | Take off what Tone exposed: de-essing, resonance suppression, clipping. |
| **Relational** | live | How tracks sit against each other: relational EQ, separation, sidechain dynamics. |
| **Depth** | live | Panning, reverb, delay, ambience. |
| **Creative** | live | Throws, creative filters, sound design. |

Compression is the easiest to misplace. Attack and release pick the style:
**thickness** (fast/fast), **groove** (slow/fast) and **punch** (slow/slow)
are Tone; **consistency** (fast/slow) is Rescue, offline. Read
[references/compression.md](references/compression.md) before choosing or
setting any compressor.

## Roles

What a track gets is decided by its role, not by what would help it.

| Role | Examples | Chain |
|---|---|---|
| A mic, a sample | `Kick/SUM/In`, `Snare/SUM/Top`, `Toms/T1`, `Cymbals/OHs` | Rescue and Tone only |
| A piece's folder | `Kick`, `Snare` | Rescue (the gate keyed by the piece's trigger), then Tone |
| A Sum | `Kick/SUM` | nothing: it sums, and feeds the piece's Sub and verbs |
| A trigger | `Kick/SUM/Trig` | detection only, muted: it plays the Sample and keys the gates |
| A one-note track | `Kick/Sub`, `Snare/SUM/Fund`, `Toms/T1 Fund` | Tone (parallel filters): one band-pass on the drum's note at 12 dB/oct (wide on a kick, tight on snare and toms) → trigger-keyed gate → saturation, blended under the drum on its fader |
| A verb | `Kick/Verb`, `Snare/VERB/NonLin` | Depth. Stacked in a folder when a piece wants several (the snare's NonLin, Plate, Hall): unmute the ones the song wants |
| A bus | `Drums`, `Drum Kit` | Tone (glue compression), Polish (limiting) at the top |
| Process | `Process/Compress/Punch`, `Process/FX/Verb/Stadium` | fed by the whole kit, **a mute away**: muted until the song asks |

## Gain neutral

A phase is gain neutral when the track sounds as loud with it on as
bypassed: then switching a phase compares sound, not level, and the
faders stay the one place level is set. Signal holds it per phase
(`LaneSpec::trims`) and measures only where the phase's output is
sounding, so a gate's job (taking the bleed away) is not made up for.

- A make-up past ±24 dB is a broken block, not a loud or quiet one.
- A parallel lane (a one-note track, a Process compressor) is gain
  neutral too; its fader is the blend.

## Gotchas

- Macros in a kit work *up* from the preset (`MacroMode::Modulate` from
  0). A `Set` macro at rest pins its targets to their minimum, and a
  whole kit's samples go quiet.
- A sample's level is relative to its hardest hit (the `drum` block
  normalizes each piece at 0 dBFS). Its velocity follows the trigger
  through `dynamics`: a low value plays the library's upper layers.
- Fundamentals for the one-note tracks are guesses until they're
  measured off the hits (`studio/features/engine/engine/src/fundamental.rs`
  does it: trigger hits, the close mics summed at their polarity).
- The `nonlinear` reverb gates its tail on its input level: a quiet
  send never opens it.
- Calibrate at the level the host plays each track. A comped session's
  clip gain can sit 10–25 dB over the raw stems, and a compressor's,
  gate's or saturator's make-up only holds at the level it was measured
  at: give the render tool each track's offset (`SIGNAL_KIT_INPUT_GAINS`,
  from Studio's raw LUFS), or switch on each instance's `Calibrate` while
  the song plays.
- A dead trigger (a tom's at the noise floor) can't key anything. Its
  gate opens on its own input after 10 s without a hit; a drum that is
  played but whose trigger is dead is keyed from its own mic, low-passed
  (`HitDetector::with_low_pass`).

## References

- [references/compression.md](references/compression.md): the four
  compression styles, their phases, settings and gain matching. Read before
  any compressor.
- [references/parallel-filters.md](references/parallel-filters.md): Dan
  Worrall's parallel filter techniques (band-pass, shelf, inverted, delta
  channel), each with its phase. Read before a one-note track or any
  parallel EQ.
- [references/metal-drums.md](references/metal-drums.md): modern metal
  drum settings, sourced (Mynett, Putney, Cervini, Sneap). Read when
  dialling in a metal or hard rock kit.

Transcripts of the source videos are in
`/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/`.
