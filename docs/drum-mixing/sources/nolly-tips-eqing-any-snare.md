---
title: "Nolly Tips: A Practical Guide to EQing any Snare Drum"
speaker: Adam "Nolly" Getgood
channel: GGD
url: https://www.youtube.com/watch?v=FElKSuwA9BM
video_id: FElKSuwA9BM
kind: video-tutorial
genre: modern heavy music (metal, hard rock)
drums: GGD Invasion (Brady Tasmanian Blackwood snare) and GGD Modern & Massive (Ludwig snare), snare top and bottom only
transcript: /run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/FElKSuwA9BM - Nolly Tips： A Practical Guide to EQing any Snare Drum.txt
transcript_kind: YouTube auto-captions (no timestamps, mishearings)
summarised: 2026-11-24
tags: [drums, snare, eq, transient-designer, saturation, compression, metal, sample-library, ear-training]
---

# Nolly — a practical guide to EQing any snare

A tutorial on how to EQ a snare when there's no preset that fits: train the
ear to hear the **front of the hit**, find three problem zones, cut them, and
build the sound back up with a small compression/presence/saturation chain.
He does it twice on raw sample-library snares to show the method works on
different sources. It's the "snare video" he refers back to in the
[Wildfire masterclass](nolly-periphery-wildfire-masterclass.md), whose snare
section is the same chain applied to a finished mix.

**How to read this note.** What Nolly says he does, with his numbers. Items
tagged **[Signal]** or **[I]** are my mapping, not his words. `[caption?]`
marks garbled auto-captions.

## The one-paragraph version

Put a **transient designer on the snare with sustain at zero** so you hear
only the attack, then listen for what leads the sound. There tend to be
three zones: **flappy/clunky ~400–500 Hz**, **clacky ~1 kHz**, and
**harsh/ticky 5 kHz and up**. Cut what leads, add **low-end punch with a
high-Q low shelf**, then a chain of **compression (4–5:1), a presence lift
(~4+ kHz) and saturation**, with the transient designer back in to take off
the last of the blip. **Snare bottom** needs little: a mid cut and a lower
fader. Judge it **in the mix**, over a day if you can.

## The premise: why this is hard

- Every snare is different: drum, head, muffling, drummer, tuning, mic,
  mic position, processing on the way in. A one-size-fits-all EQ is "basically
  impossible", short of using the same samples or recording the same way
  every time.
- So instead of copying a preset, **train your ear** to pick out the key
  frequency areas of the top and bottom mics. He tells viewers they may not
  like his choices; the point is to follow the listening process.
- **Why the attack matters so much in heavy music:** the drum mix has all the
  sustain, fizzle and room detail, then the huge rectangular waveforms of the
  guitars (and bass) go on top and you're left hearing "the tips of the
  icebergs": the brief spike at the front of the snare. The rest gets eaten.
  You can bring tails back with parallel compression or transient designers,
  but the ear still latches onto that first few milliseconds.
- **That's hard to judge** because the sustain, ringing overtones and wire
  fizz keep going and have a different frequency response from the attack,
  and they distract the ear.

## Method

### 1. Strip the sustain
- **Transient designer with sustain pulled to zero** (he uses Native
  Instruments Transient Master) on the snare bus, or on the channel. Now you
  can solo the snare "without distraction" and hear the part you'll actually
  hear in the mix.
- Keep it on while you EQ the top. Switch it off to check you haven't
  damaged the ring.

### 2. Listen for what leads the sound, in three zones
He says he already knows from experience that there tend to be three areas.
Identify how much each is leading the attack:

| Zone | Character | Where |
|---|---|---|
| Low-mid | flappy, clunky | ~400–500 Hz (around ~500) |
| Upper-mid | clacky | ~1 kHz (found ~1.2–1.3 kHz here) |
| Treble | harsh, ticky, plasticky | 5 kHz and up |

- **Vocalise the sound** (he knows it looks silly): is it more of a "tack",
  a "cat" or a "pa"? These differ in the amount of low end:
  - "pa" / plosive: low-end punch. A modern rock/metal snare often has a lot
    of this.
  - "k" / "cat": more upper-mid attack.
  - "tab": very little low end.
- A visual analyser is a big hint (you can see a bump where the ring is), but
  you can do the same on an analogue-style EQ without one to train the ears
  harder.

### 3. Cut what's leading
- **Cut the flappy zone (~500 Hz)** until it's no longer the leading edge.
  "Not necessarily trying to scoop it all out."
- **Cut the clacky zone (~1.2–1.3 kHz),** then the ticky zone higher up.
- The more you scoop **~900 Hz–1.5 kHz**, the more **woody and less harsh**
  the drum. Some mixers scoop a lot there and it gives a specific character,
  but **it can strip the character of the instrument**. Balance by taste.
- **Option:** make the ~1 kHz cut a **dynamic band**, so it cuts on the initial
  transient then swoops back up to let overtones through.
- The result is a "stark difference", more pleasing, though you lose some ring.

### 4. Put the low-end punch back with a high-Q low shelf
When the drum lacks low-end power ("plosive character"), he uses a move he
likes on snare:
- A **low shelf with the Q pushed up**, so it makes a **bump on either side**
  of the corner frequency. The bump accentuates the **fundamental**, while the
  region just above it is slightly **cut**: "give with one hand and take with
  the other".
- Why not a plain bell boost at ~200 Hz, as often recommended? It brings up
  **muddiness around 300–400 Hz** above it.
- **Don't trust the dial.** In his example the shelf's midpoint is ~270 Hz but
  the bump falls around ~200 Hz, and it's peaking above 6 dB when the gain
  says ~+4.5 dB. Use your eyes on the analyser, mostly your ears.
- Method: start with the gain all the way down, press play, and bring it up
  until you get "a little punch of low end on the fundamental". It may be a
  bit much and need pulling back later.
- He does this on the **snare top**, up front, then more EQ on the bus.

### 5. Snare bottom
- First thing he hears: **mid-range forwardness, flappiness**, probably ~500 Hz
  or a bit higher. **Cut it,** adjusting Q and depth.
- "If it's **captured well** it doesn't need very much": maybe a **little mid
  cut** and it's sorted.
- Then bring the bottom **much further down** relative to the top so the
  balance is right. Judge together with the top.

### 6. Compression, presence and saturation: the trio
All three from one Slate virtual mix rack, which he's used a lot including on
the Periphery record. He says any equivalents will do.
1. **Compressor** (an SSL-style bus comp `[caption?]`): ratio **4 or 5:1**
   and give the snare "quite a lot of compression".
2. **Presence lift** (the "present" control of Slate's Custom Series EQ):
   mostly a **wide boost around ~4 kHz and a bit**. A zone he wouldn't
   normally reach for, and he likes this one-knob control for that reason:
   it dials in how much forwardness you want.
3. **Saturation** (the "Hollywood" module): "you can use anything, from a
   completely clean clipper to a tape emulation": it's all taking some of the
   transient off the drum. He turns it up until he stops liking the
   character, and finds it still has a "brief blippy" transient left.
4. **Back to the transient designer** on the snare top to cut the attack a
   bit more.
5. **Transient designer on the snare bottom**, too: **cut attack, boost a
   little sustain** to even out the response of the wires.

### 7. In the mix
- **Snare bottom can turn ratty** (~5 kHz) in context. Cut it on the bottom
  channel, then **add the presence back on the bus** with the lift module.
- **It's a balancing phase:** more presence on the close snare vs. the bottom
  fader vs. the bottom's EQ. Pulling the bottom down reveals more of the
  top's attack, and then the presence EQ finishes it.
- **Reverb send from the snare top only,** not the bottom: he likes "less of a
  fizzy character to the snare reverb these days".
- **A second EQ on the snare bus, before the compressor:** a pass over the
  same range to cut a little **~3 kHz clackiness** in context.
- Bring the **snare room trigger** back in at the end to hear it with the room.
- **Time:** for a first song on a record he'd take **a day or so**, listening
  outside the DAW ("while making lunch") and checking over and over that the
  balance of attack against sitting in the mix is right. The snare is one of
  the main things he listens for.

## Second source: Modern & Massive (same method, different answers)

A rock-focused library: a **Ludwig** snare, with a **Josephson condenser on
the top** (the mic he uses on toms on records). Same MIDI, same process.

- Different mic means different cut locations: **a condenser on top has a
  different frequency response.**
- **Top:** the leading character was a **knocky muddiness, a bit below the
  500 Hz he'd cut on the first**, with little of the ticky character and some
  upper-mid harshness. **No low-end emphasis needed**: it already had a lot of
  punch (the condenser, the tuning). "Every recording's different."
- **Bottom:** fat and nice, but not enough cut and needing to be a little more
  upfront. Its **fundamental tone needed nipping back**.
- Saturation **brought out an ugly overtone**: he found and cut it. Still a
  little ticky, so the transient designer on the top again, and he backed off
  the presence control.
- Ends up sounding like the **Modern & Massive snare**: "there's always going
  to be a certain amount of character that's just there in the source track".
- **A/B at the end:** bypassed, all he hears are the clacky frequencies cut
  earlier, which the transient designer made easy to hear.

## Philosophy worth keeping

- **If you don't like the source's character, find another source** rather
  than fight it.
- **One source for the snare tone:** he isn't a fan of layering lots of
  unrelated samples. The tone comes primarily from one, plus maybe one or two
  one-shots for a different character. "Make that sound as good as it can."
- **Don't rely on the same samples or preset every time:** the techniques
  (transient designer, vocalising) get you to unique snare sounds per record
  and help you zero in on what you don't like quicker than trial and error.
- **Programming:** don't program snares at 127. The samples are recorded up
  to very hard hits, and most of the time you get better tone pulling back.
  **~110 velocity** gives a solid rimshot here (about how hard the real
  drummer was hitting).
- **His context chain,** for reference: a Distressor-style compressor on the
  drum bus (the Periphery 4 library video), with the EQ he used to put on the
  mix bus now **only on the drum bus**; then a master bus with a fast-attack,
  fast-release SSL-style comp, a bit of Slate FG-Grey, a bit of FG-Red with the
  mix low, and a limiter. See the Wildfire note for the settings.

## Signal mapping (inference)

**[I]** throughout.

| His move | Signal phase | Note |
|---|---|---|
| Cutting ~500 Hz, ~1–1.3 kHz, 5 kHz+ on the top; mid cut on the bottom | Rescue EQ (corrective) | He's removing what leads the attack, not shaping a style. |
| High-Q low shelf for the fundamental | Tone (tonal EQ) | A parallel-filter-like idea in one band: boost the note, trim just above. Compare with the one-note track in `parallel-filters.md`. |
| Compression at 4–5:1 | Tone (stylised compression) | Heavy, for thickness/punch; not levelling. |
| Presence lift at ~4 kHz | Tone (tonal EQ) | |
| Saturation, then the transient designer to trim what's left | Tone (saturation) | Used to take transient off, not to add colour. |
| Dynamic band on ~1 kHz | Rescue EQ (dynamic) | |
| Snare bottom: cut, lower the fader, cut attack/boost sustain | Rescue + Tone, then the fader | "Mics get Rescue + Tone only." The balance is on the fader. |
| Reverb from the top only | Depth | |
| Cut ~3 kHz on the bus before the compressor | Rescue/Relational | He does it after hearing it in the mix. |

A note on workflow: the transient-designer view is a **monitoring** tool here
(a temporary insert to hear the attack, then turned off), plus a
**processing** step when used to cut the attack after saturation.
It's worth deciding which one a build would offer.

## Open questions

- The name of the bus compressor in the trio, the "Custom Series" EQ and
  "Hollywood" module come from garbled captions (see the Wildfire note's list).
- He gives frequencies by ear (~500, ~1.2–1.3 kHz, ~4 kHz, ~3 kHz) but **no
  gains or Q values**: those are on screen only.
- The shelf example's numbers (~270 Hz midpoint, ~200 Hz bump, +4.5 dB reading,
  >6 dB peak) are specific to that Q and plugin and, by his own statement,
  don't translate.
