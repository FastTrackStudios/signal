# Compression reference

Read this when choosing or setting a compressor. Pick the *style* first
(from what the sound needs), then the phase it belongs to, then settings.

Source: Tyson (Dinosaur Dog Studio), "The ONLY 4 Ways You Can Use
Compression", Bedroom Producer channel, https://youtu.be/Mn2Xf6JrYhk
(18 min). Transcript from YouTube auto-captions (no human captions):
`/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/Mn2Xf6JrYhk - The ONLY 4 Ways You Can Use Compression.txt`.
Timestamps are caption-paragraph starts, accurate to ~30 s.

**Marking.** `Said:` = the speaker's claim or setting, paraphrased closely
(quotes are verbatim captions). `Ours:` = our interpretation or addition;
the video does not say it. Everything outside a `Said:` line is `Ours:`.

## The model (Said)

- Every note has a **transient** ("peak": the initial impact, the loudest
  part) and a **tail** ("valley": everything else). [t=66](https://youtu.be/Mn2Xf6JrYhk?t=66)
- Compression controls "the relative change between the transient and the
  rest of the note". [t=101](https://youtu.be/Mn2Xf6JrYhk?t=101)
- **Attack and release choose the style**; threshold and ratio only set
  "how intense my compression is". [t=136](https://youtu.be/Mn2Xf6JrYhk?t=136)
- Fast attack = clamp the transient; slow attack = let it through. Fast
  release = let go after the signal drops below threshold, leaving the
  tail mostly alone; slow release = hang on, pulling the tail down.
  [t=171](https://youtu.be/Mn2Xf6JrYhk?t=171) [t=205](https://youtu.be/Mn2Xf6JrYhk?t=205)
- So 2 × 2 = **four styles**, and only four:

| | fast release | slow release |
|---|---|---|
| **fast attack** | Thickness | Consistent (normalizing) |
| **slow attack** | Groovy | Punchy |

All demos ran on a **drum bus**, 3:1, as an insert. Knee, detector
(peak/RMS), compressor model and parallel use are never mentioned.

## 1. Thickness: fast attack, fast release

[t=238](https://youtu.be/Mn2Xf6JrYhk?t=238) · settings [t=270](https://youtu.be/Mn2Xf6JrYhk?t=270)

- Said: "thickness type compression". Turns the transient down, so the tail
  comes up relative to it: "thicker bigger sounding", and perceptually
  louder because the tail is most of the note.
- Said, settings (drum bus): attack **1 ms**, release **20–30 ms**, ratio
  **3:1**, about **5 dB** gain reduction ("for example purposes").
- Said, when right: drums have "more impact"; cymbals (the kit's tails)
  get much louder. He usually pairs it with another style on the bus.
- Ours, failure modes: kick/snare lose their front edge (flat, smeared);
  cymbals and room wash take over; release under ~10 ms on low material
  makes distortion as the compressor tracks the waveform.
- Ours, suits: drum bus, rooms (more sustain and size), toms (more ring),
  snare when it is all crack and no body. Avoid on a kick that already
  lacks click.
- **Phase: Tone** (stylized, insert).

## 2. Groovy: slow attack, fast release

[t=446](https://youtu.be/Mn2Xf6JrYhk?t=446) · settings [t=481](https://youtu.be/Mn2Xf6JrYhk?t=481) / [t=516](https://youtu.be/Mn2Xf6JrYhk?t=516)

- Said: "groovy type compression". The transient passes, the middle of the
  note dips, the end of the tail comes back: "a dip in the middle", "a
  pumping effect". Easier to hear on longer notes (vocal, bass) than kick.
- Said, settings (drum bus): attack **30–50 ms**, release **~20 ms**,
  ratio **3:1**.
- Said, when right: "you don't really want to hear your groovy type
  compression you generally just want to feel it"; more life and movement
  where the track sounded "bland". On drums, cymbals duck just after each
  kick/snare and "swoop back".
- Ours, failure modes: audible pumping or breathing (unless that is the
  creative goal); release too long turns it into Punchy; release not timed
  to the tempo makes the swell land off the groove.
- Ours, suits: drum bus, rooms and overheads (the duck-and-swell breathes
  with the beat), bass, sustained parts that feel static. Set the release
  so the swell finishes before the next hit.
- **Phase: Tone** (stylized, insert).

## 3. Consistent / normalizing: fast attack, slow release

[t=647](https://youtu.be/Mn2Xf6JrYhk?t=647) · settings [t=747](https://youtu.be/Mn2Xf6JrYhk?t=747)

- Said: "normalizing or consistent style compression". Grabs each note
  immediately and holds it for the whole note. "The only style ... not
  really focused on the balance between the transient and the tail";
  works on "macrodynamics" (note-to-note level), not "microdynamics"
  (shape within a note). Loud notes down, quiet notes up after makeup.
- Said, settings (a kick part, uneven strength across a bar): attack
  **1 ms**, release long enough that gain reduction "basically get[s] back
  to zero before the next note comes in"; tune to the busiest section, or
  automate the release per section. Makeup **+3 dB** for ~3 dB reduction.
- Said, when right: "a lot less variation between" notes, "a much more
  consistent take".
- Ours, failure modes: release not back to zero by the next hit, so every
  hit after a loud one is quieter (the unevenness moves, it isn't fixed);
  transients flattened along with level; the part becomes lifeless. If
  the problem is a few stray notes, clip gain or automation is cleaner.
- Ours, suits: uneven kick/snare/tom performances, bass, vocals. Rarely
  the bus or rooms.
- **Phase: Rescue, offline consistency compression (levelling).** Not
  Tone: it repairs the performance; it doesn't style it.

## 4. Punchy: slow attack, slow release

[t=816](https://youtu.be/Mn2Xf6JrYhk?t=816) · settings [t=881](https://youtu.be/Mn2Xf6JrYhk?t=881)

- Said: "Punchy style compression". Transient through, tail held down:
  "more forward", "a bigger impact". One of his favourites.
- Said, settings (drum bus): attack **~50 ms** (30–50 ms is "slow" for
  drums; up to ~100 ms on a vocal, whose transient is slower). Release
  long: he reuses the previous one (caption reads "4 440 milliseconds",
  most likely ~40 or ~440 ms; unclear). On eighth notes it didn't return
  to zero between hits and he kept it, because the leftover gave a little
  groovy movement on quarter notes. Ratio 3:1. Makeup gain (he forgot it
  at first; the A/B didn't count until it was on).
- Said, when right: kick and snare perceptually louder, cymbals quieter;
  "really about that relative difference between the transient and the
  tail".
- Ours, failure modes: attack too slow does nothing; release so long the
  gain never recovers makes it a level squash; spiky, clicky, thin drums
  (the body went with the tail).
- Ours, suits: kick and snare inserts, drum bus, toms that ring too long.
  Too dry for rooms, whose job is the tail.
- **Phase: Tone** (stylized, insert).

## Outside the four (Ours; the video doesn't cover these)

- **Parallel compression → Tone, parallel compression.** Ours: a blend of
  a style, not a fifth one. Heavy settings on a send (often Thickness
  pushed hard: 1 ms / fast, 8:1+, 10–20 dB GR), mixed under the dry signal
  to add body without losing the transients. Delay-compensated, same
  polarity as the dry.
- **Limiting / clipping → Polish.** Ours: peak control at ∞:1 or a hard
  ceiling; not tone shaping.
- **Knee and detector.** Ours: hard knee + peak detector for the
  transient styles (Thickness, Punchy); soft knee + RMS for Consistent.
  Groovy depends mostly on attack.

## Gain neutrality (every chain)

Gain staging happens once at the start; the mix is done on faders. A
compressor must leave the level where it found it, so the fader balance
stays true and every A/B judges tone, not loudness.

- Said: makeup gain about equal to the gain reduction (+3 dB for 3 dB),
  and A/B only after makeup is on ([t=781](https://youtu.be/Mn2Xf6JrYhk?t=781), [t=962](https://youtu.be/Mn2Xf6JrYhk?t=962)).
- Ours: match makeup **by loudness, not by peak or GR meter.** The four
  styles move peaks and average differently: Thickness and Groovy raise
  the average (the tail), so they need less makeup than their GR; Punchy
  keeps peaks and drops the average, so it needs more than its peaks
  suggest. Match short-term LUFS (or RMS) over the same passage, bypassed
  against active, within about 0.5 dB.
- Ours: on a parallel chain, match the summed (dry + wet) bus against the
  dry alone; the blend sets the level.
- Ours: when the compressor has auto-makeup, check it with the meter and
  trim to match.
- Ours: the level-matched A/B is the test of the compressor. If it only
  wins louder, take it out.

## Decision guide (Ours, from the Said settings)

Start at 3:1 and dial threshold for the GR given, then match level.

| I want… | Use | Start at |
|---|---|---|
| bigger, thicker, more sustain/size | Thickness | 1 ms / 20–30 ms, 3:1, ~5 dB GR |
| more life, movement, feel | Groovy | 30–50 ms / ~20 ms, 3:1, 2–4 dB GR; release timed to the groove |
| hits/notes all the same level | Consistent (Rescue, offline) | 1 ms / release back to 0 before the next hit, 3:1, ~3 dB GR |
| more punch, more forward, tighter | Punchy | 30–50 ms drums (~100 ms vocal) / long, recovering by the next hit, 3:1, 3–5 dB GR |
| body under intact transients | Parallel (Tone) | Thickness pushed hard on a send, blend to taste |
| peaks caught for loudness | Limiter/clipper (Polish) | ceiling only |

Drums, typical (Ours): kick → Punchy (Consistent first if uneven); snare →
Punchy, or Thickness if thin; toms → Punchy to shorten, Thickness to
ring; rooms → Thickness or Groovy, often parallel; drum bus → Thickness
+ Groovy or Punchy (he says he usually combines styles on the bus).
