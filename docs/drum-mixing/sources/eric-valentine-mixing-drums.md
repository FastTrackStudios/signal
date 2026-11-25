---
title: 'Making Records with Eric Valentine - "Mixing Drums"'
speaker: Eric Valentine
channel: MAKING RECORDS with ERIC VALENTINE
url: https://www.youtube.com/watch?v=iz0Dww6e9QQ
video_id: iz0Dww6e9QQ
kind: video-walkthrough
genre: rock (Greg Holden "Save Yourself"; Tom Petty-ish); Valentine also produced QOTSA, Slash, Good Charlotte
drums: real kit (1930s Slingerland), close mics, overheads, hi-hat, rooms; one kick sample on section downbeats only
transcript: "/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/iz0Dww6e9QQ - Making Records with Eric Valentine - ＂Mixing Drums＂.txt"
transcript_kind: YouTube auto-captions (timestamped)
date: 2019-06-22
duration: "42:56"
also:
  - "Gearspace interview, Aug 2022: https://gearspace.com/board/interviews/1387250-interview-eric-valentine.html (403 to fetch; quoted from search snippets)"
  - "Sound On Sound, Secrets of the Mix Engineers: Eric Valentine (Slash, 2010): https://www.soundonsound.com/techniques/secrets-mix-engineers-eric-valentine"
summarised: 2026-11-24
tags: [drums, drum-bus, distressor, parallel-compression, sidechain-filter, saturation, multiband-distortion, rooms, gated-rooms, overheads, hi-hat]
---

# Eric Valentine — mixing drums

A full walkthrough of a heavy-handed drum mix on a well-recorded kit, almost
no samples. The centre of it is the **drum bus: a pair of Distressors at
20:1 with the detector filtered**, the move Nolly says he borrowed. Valentine
also does a lot of **multiband "tape-like" distortion** per mic and treats the
rooms in three ways at once.

**How to read this note.** What he says, with his numbers, by timestamp.
**[I]** = mine. `[caption?]` = garbled.

## Drum bus — the Distressors (36:21–39:54)

- A hardware insert: a **pair of Distressors** on the whole drum submix.
  "That's a huge part of [the] drums … the whole drums [are] mixed to
  accommodate it." "Pretty heavy-handed."
- **Ratio 20:1.**
- **Detector: "all three of these illuminated — the link, the high-pass and
  the presence."** Reason: "it makes it so the compressors are not overly
  sensitive to compressing the kick drum … if I take those off it just …
  completely annihilate[s] the kick drum in the balance of the drum kit."
  "When I use Distressors on drums [I] pretty much always have all three."
- **Input 6.5**, set "so it's compressing the amount that you want".
- **Attack as slow as possible** — "really using this to manufacture more of
  an attack [on the] drums".
- **Release 2.5**, "will vary based on the tempo": set "so the room swells
  back in at a rate that feels musical … so it's not bringing up too much of
  the cymbals and it's not choking off the drums".
- Output: "whatever, it's not distorting wildly".
- After it: a console-emulation EQ, "overall brightening"; a "tiny bit" of
  API 2500 on the main mix.
- He does **no compression on the individual snare mics**: "all [that] is
  handled by the overall drum [bus] … by the Distressors" (40:31).

**From the Gearspace interview (2022, snippets):** he "almost always" has
"both the HP and the peak engaged on the side chain"; he "pretty much just
sticks with the **10–8 range on the attack**" and plays with "adding just
enough **distortion … before and/or after the Distressor** to help the
compression sound more natural". **From SOS (Slash, 2010):** drums submixed
to a stereo pair through "a pair of Distressors and some NTI EQ", plus "a
pair of 1176 compressors that I mixed underneath to add more density to the
ambience".

**[I] On the Distressor's detector.** "HP" is a ~100 Hz-ish high-pass on
the detector (6 dB/oct); the "band"/presence is a mid-range emphasis in the
detector (around 6 kHz on the hardware) — so the detector hears the snare and
cymbals more, the kick less. Signal's comp has `sc_hpf` but no detector bell.

## Overheads (31:31–33:16)

- A **TG-style compressor on a fast, "pretty extreme limiter" setting**:
  "everything is pulled forward all the time … make[s] the cymbals really
  thick … longer decays on the crashes".
- **Lows pulled out of the overheads**: the low end comes from the close mics.
  Low-mid in the overheads is "omnipresent … muck" taking space from the
  guitars; he'd "rather have … bursts of it that punch through".
- Then "very specific" cuts: "little peaky stuff" to make them "more specific
  to … the information that's in those mics".

## Hi-hat (33:16)

- "Taking out all the low end, don't need all that, just want to hear the
  hi-hat." Then a Fairchild-style compressor (NI Supercharger) and a little
  Decapitator crunch.

## Rooms — three treatments (34:32–36:21)

1. **A gated room** — opens only on kick and snare (keyed from duplicated
   close-mic "key" tracks, the key **delayed** ~1150 samples to match the
   room's arrival so you don't hear noise before the hit). "Ends up being
   more like an old Phil Collins record"; un-gated for fills; "makes the kick
   and the snare sound more explosive … [without] cymbal wash going on all
   the time".
2. **A "destroyed" room** — a hardware leveler/limiter (the tracking
   engineer's) "just completely flattened", **turned way down** as "an
   undercurrent … it just fills in all the space and makes everything sound
   really thick". (Name garbled: "level or" `[caption?]`.)
3. **The normal rooms**, compressed at tracking.

## Per-drum moves worth keeping

- **Multiband distortion "like tape"**: Saturn with only the mids/highs bands
  driven, lows clean: "on tape machines the amount of distortion increases
  as the frequencies get higher" (15:46). Multiband to avoid intermodulation
  "splatter". **Same multiband plugin on every mic of a piece** so crossover
  phase shifts match and the mics still sum (10:28, 24:22).
- Kick: inside mic HPF 270 Hz (attack only); outside mic −278 Hz scoop, +5 dB
  at 40 Hz, resonant HPF at 46 Hz; a kick bus compressor "a lot more smacky".
- Snare: +~800 Hz bark; a **duplicate snare track just for lows**, +13 dB at
  140 Hz (input padded); a printed tube-driver distortion layer.
- Gates with ~4 ms look-ahead; gate keys from duplicated close mics.

## Signal mapping (inference)

| His move | Signal phase | Note |
|---|---|---|
| Distressor pair, 20:1, slow attack, tempo release, detector HP + presence | Tone (parallel/glue on `Drums`) | Ours: `comp` ratio 20, attack 20–30 ms, release ~ tempo, `sc_hpf` 120–150 Hz. No detector bell → approximate with higher `sc_hpf`. He runs it **in series** (100 %); Nolly blends 25–40 %. |
| Distortion before/after the bus comp | Tone (saturation around the comp) | `saturate` Tube/Transformer, low drive. |
| OH fast limiter for thick cymbals, long crash decay | Tone | Comp 20:1, fast attack, `mix` < 1 to keep it safe. |
| Gated room keyed by kick/snare, key delayed | Relational / Rescue (keyed gate on a room lane) | Signal's gate is trigger-keyed; the trigger timing vs the room arrival needs the delay he describes. |
| "Destroyed" room, tucked | Tone (a crushed room lane, parallel) | `Process/Compress/Smash` fed from Rooms, or a room duplicate. |
| Multiband tape-like distortion | Tone (saturation) | Signal's `saturate` is broadband: emulate with a parallel band-passed/high-passed lane into drive. |

## Open questions

- His Distressor attack "as slow as possible" (video) vs "10–8 range"
  (Gearspace) vs Nolly's 8: both say slow; on the hardware the knob's ms
  aren't linear.
- The room limiter's name is garbled.
