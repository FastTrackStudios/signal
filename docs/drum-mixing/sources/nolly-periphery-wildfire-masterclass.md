---
title: Nolly — Mixing Masterclass, Periphery "Wildfire"
speaker: Adam "Nolly" Getgood
channel: GGD
url: https://www.youtube.com/watch?v=6MphgmN-uGM
video_id: 6MphgmN-uGM
kind: video-walkthrough
genre: modern metal / progressive metal
drums: GGD P5 Matt Halpern Signature library (sample-based, Kontakt)
transcript: /run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/6MphgmN-uGM - Adam ＂Nolly＂ Getgood Mixing Masterclass ｜ Periphery ＂Wildfire＂.txt
transcript_kind: YouTube auto-captions (no timestamps, mishearings)
summarised: 2026-11-24
tags: [drums, kick, snare, toms, cymbals, rooms, drum-bus, metal, sample-library]
---

# Nolly — Periphery "Wildfire" mixing masterclass

Nolly mixes the first song of Periphery's *Periphery V* with the drums
rebuilt from the GGD P5 sample library, to show how to get the record's sound
from the library. The first third is drums; the rest covers bass, guitars,
synths, vocals and the buses. This note is the drums in full and the rest
only where it touches them (see [Beyond the drums](#beyond-the-drums)).

**How to read this note.** Everything below is what Nolly says he does, not a
rule. Numbers are as he gave them. Where the auto-captions are garbled, the
number is flagged `[caption?]`. Items tagged **[Signal]** are my mapping onto
Signal's phases (`signal-mixing` skill), not his words.

See also: [Nolly Tips: EQing any snare](nolly-tips-eqing-any-snare.md), the
snare tutorial he refers back to here.

## The one-paragraph version

Light processing on the drums, with the **kick** the most sculpted piece.
Per piece: a **gate** that shapes the envelope rather than cleans bleed, a
**subtractive EQ** (clear the zone just above each drum's fundamental, trim
the unpleasant mids), a **tonal lift** up high, then **saturation** and a
**compressor**. The sound is finished in **parallel compression on the drum
bus** (a Distressor with its detector filtered), a **bit of EQ on the bus**,
and **transient designers on the rooms** in place of compressors. Judge the
close mics **in the mix, not solo**, because the overheads and rooms carry
the rest. The **instrumental bus** compressor also ducks the whole mix on
each snare hit.

## Setup and sounds

- **Library state:** the "Wildfire" preset (the record's cymbal and
  Ratom selection and snare tuning), with GGD's built-in "Turbo" processing
  **off**, so all the processing is in the DAW.
- **Fader starting point:** mostly flat at unity. Snare bottom trimmed back.
  In the **far room mics**, snare raised and kick lowered. Reason: kick gets
  boomy in room tracks and the snare needs to cut through.
- **Routing out of the sampler** (Logic): kick in/out to one channel; snare
  top and bottom each on their own channel; a channel per tom; hi-hat alone;
  other cymbals as a "spot" channel; overheads; mono room; rooms 1+2 on one
  fader for the stereo room.
- **Programming:** don't program everything at full velocity. Kicks sit around
  **99–102** (roughly 100). Tom hits are about 100 at the hardest, with the
  second hit of a fill a bit softer so fills flutter instead of sounding rigid.
- **On how hard drummers kick:** hitting hard with the hands doesn't mean
  kicking with full strength, and he thinks the drummer sounds better for not
  kicking at full force.

## Kick

**Source:** the library's kick, a damped (Evans UV EMAD-style) head. Its
flatter presence region makes a "blank canvas" to sculpt, less clicky and
slappy than an undamped head.

1. **Gate (kept on a sample on purpose).** Not for bleed: it changes the
   envelope (how long the low end blooms) and gets closer to the record.
   His usual kick preset: no attack, hold or knee; **release ≈ 350 ms**; a bit
   of look-ahead; **range 50 dB**; **ratio 10:1**. It mostly trims sustain
   and head flap. **[Signal]** Rescue, the trigger-keyed gate.
2. **EQ, in this order:**
   - **Cut between ~100 and 200 Hz.** The energy just above the fundamental
     gives a tubby, muddy character and gets in the way of the bass. Cutting
     it "uncovers the fundamental".
   - **Big shelf at ~1 kHz.** In his experience ~1 kHz is the dividing line
     between unpleasant mids and the useful presence mids of a kick.
   - **Move the attack energy to ~3 kHz, away from 5–7 kHz.** The aim was a
     meaty, not-too-clicky kick. Moving it takes out click and gives a wider
     slap.
   - **Find and notch the ring** that the high shelf exposes (a head
     resonance he didn't like).
   - **Boost around 800 Hz** for extra meaty slap. He traces this to
     reference one-shots with more in that region.
   - Method note: he tries to make the top and bottom peaks on the analyser
     look similar, then says the ear decides.
3. **Without compression it sounds wrong.** The EQ'd kick alone has the wrong
   attack character until the compressor is in, so don't judge the EQ before
   the compression.
4. **Shimmer band (a "Revival" plugin** `[caption?]`) up high: a very high
   band, above the click, that adds a lovely top without adding pointiness.
   "A little goes a long way."
5. **Saturation, quite heavy** (a "Hollywood" model in a Slate-style rack).
   Adds dense harmonics so the low end thickens **without piling up ~200 Hz
   energy that makes it boxy**, and splats the attack so it's less pokey.
6. **Optical-style compressor** after the saturator: **max ~3 dB GR**, high
   ratio, speed to taste, and a **Tone knob** for how aggressive the kick is
   (a lot here). He says he loves optos on kick for what they do to the
   attack. It focuses the attack from the EQ into "a really short little
   impulse at the front". This is the piece that makes the EQ work.

**Order in the chain:** gate → EQ → shimmer → saturation → opto compressor.

## Snare

**Source:** Matt's signature snare (Evans heavyweight dry head). Real record
mics: **Neumann KMS 105** on top (he calls it a "Godlike snare top mic"),
**Shure Beta 87A** on the bottom. The library's snare needs little work.

**Snare top**
- **Saturate with a Neve-style preamp emulation**, letting it go into the red
  on hits. It trims a little transient, smooths hit-to-hit level variation
  and he likes the sound. A dynamic mic would take heavier handling.
- **Cut around ~550 Hz** to sound like a contemporary metal snare. Otherwise
  he'd leave a snare this balanced alone. He doesn't cut higher up.

**Snare bottom**
- Cut an "o/u"-vowel-like note, and **lift brightness around 5–6 kHz**: a
  condenser gives air but no presence bump, unlike a dynamic. (His record
  setting was a bit higher than where he landed now: "my taste has shifted
  downwards".)

**Snare bus**
- **Gate**, also for the envelope rather than for bleed. A preset he's used
  since 2019: no attack or hold, look-ahead on, **release ≈ 184 ms**,
  **range only 13 dB**, **ratio ≈ 1.7:1**, with the **sidechain filtered**
  (top and bottom cut, to ignore crosstalk).
  Set so all hard hits are above the knee and **ghost notes fall below it**.
  Reason: it exaggerates the hard/ghost difference, which the parallel
  compression later would otherwise flatten.
- **EQ from scratch** using a transient designer with sustain pulled down to
  hear only the attack: suppress the mids ("woody, hollow" for a metal drum),
  nip the low fundamental so it's present but not "boxy / noty", find a spot
  up high to brighten, **low-pass the extreme top**.
- **Compressor** (an FG-401-style): default settings, **4:1**, **second
  slowest attack, second fastest release**, threshold down until **about
  7–10 dB of reduction on hard hits**, make-up to match.
- **Presence lift around ~4.5 kHz** from an EQ's "present" control. It's a
  region he usually doesn't touch on other EQs.
- **Saturation** to trim the transient so it sits without being pointy.

**Snare philosophy**
- The drummer wanted **rimshots to sound like rimshots**: the stick attack,
  not the "fat, pillowy" modern snare. So he used the presence lift and
  **didn't scoop 900 Hz–1 kHz** heavily. Scooping that region sounds modern
  but makes drums sound alike and takes the organic stick-on-drum character.
- **Judge in the mix.** Soloed, the bright snare sounded unnecessary and a
  bit like sandpaper. In context he turned it up much further than the solo
  suggested. The close mic is only part of the sound: overheads and rooms
  supply mids, punch and carry, so he can push the upper mids on the close
  mic hard without it sounding fake.

**Snare reverb**
- Send from the **top mic only**, never the bottom, and **before it is
  brightened**.
- On the record he **duplicated the snare top to a new channel with no
  processing at all** (no gate, cymbal bleed included) into a **100% wet**
  reverb, "almost like another room channel". Un-brightened, it has no zingy
  fizz.
- Reverb is a modified Chromaverb **"80s Drums"** preset: high-frequency
  damping, plus a broad **low cut around 200 Hz** and a **bell cut around
  200 Hz** to remove mud. "Tubular, not muddy."

## Toms

Three toms, low-tuned, coated heads (Evans UV2), Josephson e22S on the real
kit.

- **EQ on each tom; on the tom bus only a limiter** (Waves L1) to catch the
  floor-tom peaks and raise the level. Some saturation from it is fine,
  "but I wouldn't want to go much harder than that".
- **Use the sustain-down transient designer** to hear the attack. Found a
  "flappy" attack (cut around **~1 kHz**) and wheezy mids from overtones
  (scooped a bit more here than on the record, where he left more for an
  organic sound).
- **The note sticks out:** a high-pass wasn't enough without cutting all the
  low end, so he used a **cut at the note plus a dynamic cut** to stop it
  blooming while keeping it present.
- **Floor toms, like the kick:** cut the "mound" **directly above the
  fundamental**. On one floor tom the first overtone was almost louder than
  the fundamental, and it clouded clarity. Dropping it gave a lot of clarity.
- Then bring up the better attack ("airy, plasticky slap").
- **Check all toms playing together** that the attacks are consistent.
- **No compression on toms,** usually: it raises bleed (he says it's a bleed
  thing), and more attack makes toms thin while a fast attack makes them
  bloated. "Some styles of music that's great." Raw usually works for him.
- **Tom reverb:** on a busy fill the pre-delay sounded crazy in isolation, so
  he would **automate the reverb down** in dense passages.

## Cymbals, overheads, rooms

**Cymbals bus** (hats, spots and overheads together)
- **High-pass, and a cut around ~6 kHz** to soften the harsh upper-mid /
  lower-treble. Partly compensates for the **+5.5 kHz boost** on the drum bus
  EQ after it.
- **Heavy parallel compression**, FabFilter Pro-C2 in classic mode: very fast
  attack, fairly fast release, **ratio ≈ 5**, squashed hard but blended at
  **~40%**. This keeps the quiet cymbals audible, adds excitement and makes
  the shells livelier.
- **Then soothe** (dynamic resonance suppression), set more conservatively
  than his old default "Nolly cymbal smooth" preset. It replaces the old
  habit of manually notching ringing frequencies. Use the **Delta** button to
  hear what it removes. Subtle, but "I just love it".

**Rooms**
- Rooms are where the sound lives for him: "what I am all about".
- Stereo pair of AKG C414s plus a ribbon in the middle, all up on the
  mezzanine behind the kit. The condensers give splash and brightness; the
  ribbon gives a "solid, meaty" centre and seems to push the cymbals down.
- **Transient designers instead of compressors on the rooms:** bring up
  **sustain**, cut **attack**. More transparent and it keeps the kit's
  natural balance. Compressing the rooms makes cymbals as loud as the snare,
  which makes the channel far less usable.
- A **reverb send from the rooms** for a bigger, lusher sound.
- **Mono room (in front of the kick):** not used on the record or here; a
  cool sound (resonant side of the kick) he doesn't reach for. He did use it
  for the count-in stick clicks, which were an overdub.

## Drum bus

- **Parallel compression directly on the bus, no separate parallel bus**
  (he corrects people who still cite his old way: a Softube FET compressor on
  a parallel bus with different sends from each channel and lots of snare).
- **Slate Distressor model, settings borrowed from Eric Valentine's video:**
  - **Ratio 20:1**, with the **detector chain filtered** (a high-pass plus a
    mid bump or band-pass), so the compressor doesn't see the full range. He
    thinks this is why it sounds so good: the kick doesn't drive the bus too
    hard and the snare triggers more of it, without needing a separate bus
    with different sends.
  - **Attack 8 (of 10), release ≈ 2.5**. He says he suspects Valentine has
    the attack slowest and release fastest and doesn't know why his differ.
  - **Mix 25–40%** (32% here), **level-matched** so on/off doesn't change the
    perceived level.
  - Matching it with a generic compressor: at 20:1 he'd need a **soft knee**
    (so it's not a hard limiting knee), and an attack **under 10 ms, nearer
    3–4 ms**, because the 8 on the knob seems to act faster than it sounds.
    Hard to replicate: "the way it accelerates in and out can be so unique".
- **EQ on the drum bus only** (these moves used to live on his whole
  instrumental bus; he moved them because drums were what benefited):
  - **~12 kHz band** on a Slate-style custom EQ, "a weird shelf" between a
    bell and a shelf.
  - **~5.5 kHz boost** (under 2 dB) on a Neve-style EQ that sounds "heavy
    handed" and brash.
  - **~100 Hz bump.** Sometimes ~60 Hz as well; not needed on this one.
- The result is "very heavily modelled off the sound of the actual record"
  and "pretty light on processing".

## The transient-designer trick

Used on kick, snare, toms and snare bus: **pull the sustain all the way
down** to hear only the attack, then EQ the attack.
- Also useful to **compare against someone else's processed one-shot**. Dial
  the attack to match by ear with the sustain out of the way. You won't have
  the same source, tune or overtones, so chasing the whole sound fails, but if
  the attack is right "you get a lot of that vibe" while keeping your drum's
  own overtones and sustain.

## The buses that shape the drums

(From his master and instrumental chain. These touch the drums most.)

- **Instrumental bus** (everything except vocals): Slate **FG-Grey**
  (an SSL-style bus compressor) at the same setting for ~10 years: **attack
  3 ms, release 100 ms, ratio 4:1**, **sidechain high-pass at 60 Hz**,
  threshold to taste. **The job is ducking the snare:** the fast attack knocks
  down the snare and, because everything is on the bus, the whole mix at that
  instant. The snare cuts through "as though it was really loud" without
  jumping out harshly. **Aim: ~3–4 dB GR on the snare.** The 60 Hz sidechain
  filter keeps the kick from pumping the mix (without it, the kick would
  trigger more reduction than the snare).
- **Gain-staging check:** about **1 dB of reduction on the kick** and
  **3–4 dB on the snare** means the levels going in are right.
- Followed by **FG-Red** (Focusrite Red-style), set to pump aggressively,
  then **blended low** for a bit of life and energy.
- **Master:** Gullfoss (recover and tame ≈ 20%, slight bright tilt, bracketed
  off above ~12 kHz), Slate virtual tape (½" 2-track, 456 tape, 30 ips, unity,
  normal bias; thickens the kick and pushes the mids), then FabFilter Pro-L2
  (modern, +9 dB, about 6 dB GR `[caption?]`). **He mixes into the limiter**
  so he can judge how the transients should sound after mastering, and
  bypasses only the limiter when sending to the mastering engineer.
- **"Top-down" mixing:** his purest definition is levelling everything with
  nothing on the master and only then EQ'ing the whole. He doesn't do that: he
  has a fixed preset chain on every mix, and works from there.

## Philosophy worth keeping

- **Enhance the good elements, cut the unwanted overtones** of the close
  mics: don't rebuild the drum.
- **Lean into the artist's tone.** The drummer's rimshot preference, the
  guitarist's tones, the singer's voice quality: keep each member's
  character. "Allow their voice to come through as much as possible."
- **Don't overcorrect.** (Said about vocals, but he applies it everywhere.)
  Wiping out what you don't like ends up not sounding like the source. His
  move: a **narrow cut (sometimes dynamic) plus a wide boost in the same area**.
- **Put the stages in separate EQs** during revisions so each move can be
  bypassed and reversed on its own, instead of one merged curve.
- **Shared reverb/ambience through a vocal-bus compressor** gives a ducking
  interplay. Sometimes you'd keep them apart.

## Beyond the drums

Short, for context.

- **Bass** (direct, tracked with Neural Parallax): tracking consistency matters
  most (a consistent DI means less compression needed); compress the low band
  below ~170 Hz, don't distort too much; boost **~1–1.5 kHz** for the clank
  and **~900 Hz** to bring the bass forward; a **Trackspacer sidechained to
  the kick** ducks the bass slightly (ratio/amount "1:1, 12%" `[caption?]`),
  **below ~80 Hz down to 20 Hz**, to stop buildup, which he says is useful moving from long
  sustained notes to double-kick sections.
- **Guitars:** used the band's own tones and processing (an IR, an EQ curve,
  multiband compression on palm mutes, a touch of widening).
- **Synth/FX bus:** a **mid-channel-only cut around 300 Hz** to clear the
  centre for kick, bass, snare and vocal, "a huge win".
- **Vocals:** heavily pre-processed by the singer; Nolly adds EQ, a dynamic
  de-harsh (soothe), spiff for transients and consonants, a de-esser, M/S
  1176-style bus compression, and keeps the reverb and delay tracks through
  the same bus compressor. A compressor with +10 dB at 8 kHz in the sidechain
  acts as a compressor and a de-esser in one.

## Signal mapping (inference)

How his moves line up with Signal's phases. **[I]** throughout.

| His move | Signal phase | Note |
|---|---|---|
| Kick and snare gates (envelope, not bleed; wide range 50 dB / narrow 13 dB) | Rescue (trigger-keyed gate) | Matches the kit's gate role: on a drum the gate shapes the envelope. |
| EQ above the fundamental, snare/tom mid cuts, 5–6 kHz bottom lift, kick shelf | Tone (tonal EQ); Rescue EQ for the ring/notch | The "find and notch the ring" move is corrective, the rest tonal. |
| Saturation then opto compressor on kick; 4:1 snare comp (7–10 dB) | Tone (saturation, stylised compression) | Opto into the attack = punch/groove, not levelling. His kick opto ≤3 dB. |
| Distressor on the bus, 20:1, filtered detector, 25–40% | Tone (parallel compression) on the bus | The detector filter is the interesting bit: parallel compression without separate send levels. |
| soothe on cymbals; spiff, de-esser | Polish | |
| Tom bus limiter | Polish (limiting) | He uses it for level as well as peaks. |
| Snare/tom/room reverb; mono room | Depth | Snare verb fed from an unprocessed duplicate of the top mic. |
| FG-Grey ducking the mix on the snare | Relational (sidechain dynamics) / bus | Mix-wide, not drum-only. |
| Kick-triggered Trackspacer on the bass | Relational | |
| Room transient designers (sustain up, attack down) | Tone | An alternative to compression on the rooms. |

## Open questions

- Which of his "Slate" plugin names the captions garbled needs checking against
  the video before copying a setting: FG-Grey, FG-Red, FG-401, Distressor,
  "Hollywood" saturation, "Revival", "custom series" EQ and "custom opto".
- The master limiter target ("around −10 ls") and reduction ("60b" → ~6 dB)
  come from garbled captions.
- Several numbers (snare gate 184 ms, ratio 1.72:1) are as he read them off the
  screen. Video-only values such as EQ gains and Q aren't in the transcript:
  he points at the graph rather than saying the numbers.
