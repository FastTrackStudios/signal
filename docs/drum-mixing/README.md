# Drum mixing vault

What people have said about mixing drums: one note per source (a video, an
interview, an article), kept apart from what Signal does with it. The aim is
to have it all in one place so it can be compared, and to feed the
`signal-mixing` skill and the drum rigs.

**Scope:** drums first. Notes on neighbouring things that bear on them (the
bass and the low end it shares with the kick) are kept too, tagged by
`scope` in the frontmatter. A source that describes someone else's method is
marked `secondhand: true`.

## Layout

- `sources/` — one file per source, named `speaker-or-channel-topic.md`.
- `topics/` — syntheses across sources, one per subject.
- `plans/` — apply plans for a song, built from the topics.
- Raw transcripts are **not** kept here (they are other people's text). They
  live in
  `/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/`,
  named `VIDEOID - Title.txt`, and each source note links its transcript in
  its frontmatter.

## A source note

Frontmatter: `title`, `speaker`, `channel`, `url`, `video_id`, `kind`,
`genre`, `drums` (the kit or library), `transcript`, `transcript_kind`,
`summarised`, `tags`.

Body: what they say, per drum piece and per bus, with their numbers as given
and their wording only in short quotes. Use these tags where it matters:

- Plain statements are the speaker's own.
- **[Signal]** / **[I]**: my mapping or inference, not the speaker's.
- `[caption?]`: the transcript is garbled there, check the video.

End with a mapping table onto Signal's phases (Rescue, Tone, Polish,
Relational, Depth) and a list of open questions.

## Sources

| Note | Who | Genre | Covers |
|---|---|---|---|
| [Nolly, Periphery "Wildfire" masterclass](sources/nolly-periphery-wildfire-masterclass.md) | Adam "Nolly" Getgood (GGD) | Progressive metal, sample-based kit | Kick, snare, toms, cymbals/rooms, drum bus, the instrumental bus that ducks the mix on the snare |

| [Nolly Tips: EQing any snare](sources/nolly-tips-eqing-any-snare.md) | Adam "Nolly" Getgood (GGD) | Modern heavy music, sample-based snare | Snare top/bottom: transient-designer listening method, three problem zones, high-Q low shelf, compression/presence/saturation trio, two libraries compared |

| [Colt Capperrune, the overhead trick](sources/colt-capperrune-overhead-trick.md) | Colt Capperrune | Live-feeling band recording | Overheads: spectral dynamics on crashes/china, dynamic cut of snare box (~450 Hz), dynamic boost of snare thump, saturation, all in Pro-Q 4 |

| [Green Light Sound, "Andy Wallace" bass EQ](sources/green-light-sound-andy-wallace-bass-eq.md) | Joe (Green Light Sound), attributing the method to Andy Wallace | Bass; **secondhand** | Not drums: a cranked 1073 low band at the fundamental into an SSL strip. Kept for the kick/bass low end and the boost-then-compress idea |

| [Nolly, GGD cymbal processing](sources/nolly-ggd-cymbal-processing.md) | Adam "Nolly" Getgood (GGD) | Rock/metal, real room (2019) | OHs as cymbal mics: HPF 280 Hz, −8 dB at 6 kHz, 1–2 notches mirrored on rooms, smash comp < 100 %; room bus EQ; snare-keyed upward expansion on the rooms; section automation |

| [Nolly reviews your drum mixes](sources/nolly-reviews-your-drum-mixes.md) | Adam "Nolly" Getgood (Kohle Audio Kult) | Metal, students' mixes of one kit | Failure modes: washy/bass-heavy rooms, kick-pumped rooms, lost hat/ride in fast parts, hard-panned spots, linear-phase EQ, gate range 10–12 dB |

| [Eric Valentine, mixing drums](sources/eric-valentine-mixing-drums.md) | Eric Valentine | Rock, real kit, almost no samples | Drum bus Distressors 20:1, detector HP + presence + link, slowest attack, tempo release; OH fast limiter; gated / destroyed / normal rooms; multiband tape-like distortion |

| [Andrew Scheps, Drum Dirt bus](sources/andrew-scheps-drum-dirt-bus.md) | Andrew Scheps (Puremix) | Rock | Parallel distortion: heavy clip, broad mid/cymbal dip after, return −14.5 dB, choruses only; phase-coherent distortion |

| [Andrew Scheps, mix bus & drums](sources/andrew-scheps-mix-buss-drum-processing.md) | Andrew Scheps (Mix with the Masters) | Rock/pop, stylised kit | Mix-bus limiter 4–5 dB lengthening kick/snare; kick+snare dbx 160 crush; Fatso + unlinked Fairchild parallels; rooms/dirt only in pushes |

| [Joey Sturgis, how to easily mix cymbals](sources/joey-sturgis-how-to-easily-mix-cymbals.md) | Joey Sturgis | Metal/metalcore, sampled and live | Cymbal bus order: HPF, 500 Hz/3 kHz cuts, comp, softener, drum bus comp; few notches |

| [Svarog Studio, taming hi-hats in the OHs](sources/svarog-studio-taming-hihats-in-overheads.md) | Svarog Studio | Metal | OH high band keyed by the hat mic; ducker gate so snare bleed doesn't key it |

| [GearGods, cymbal mixing secrets](sources/geargods-cymbal-mixing-secrets.md) | Eyal Levi, Josh Newell (article) | Metal | Sweep-and-cut with harmonics; snare in the OHs (duck or limit before compressing); slow-release parallel for cymbal length |

| [URM Academy, metal rooms & overheads](sources/urm-academy-rooms-and-overheads.md) | URM Academy | Metal, sampled cymbals | OH console drive, light parallel comp; a job per room (mono crushed, near squashed, far sustain via transient shaper); room cymbal cut, not LPF |

| [URM Academy, Mixing Secrets vol. 3](sources/urm-academy-mixing-secrets-drums.md) | URM blog (article, **secondhand**) | Metal/rock | OH EQ zones (≤500 Hz, 2–4 kHz, 8 kHz+), OH limiting, 1176 room crush 10 dB+ |

| [Production Expert, clipper plugins](sources/production-expert-clipper-plugins.md) | Production Expert (article, **secondhand**) | General | Clipping drums vs limiting, drum-bus clipper after comp+EQ, oversampling, clip → limit |

| [Solitaire Music, crush room mics](sources/solitaire-music-crush-room-mics.md) | Solitaire Music (**weak**) | Rock | Room comp 10:1+, slow attack / fast release |

## Topics and plans

- [Cymbals, rooms and the drum bus](topics/cymbals-rooms-bus.md): what the
  sources above agree and disagree on, per element.
- [Unbreakable: the polish plan](plans/unbreakable-polish.md): ordered moves
  for the song in Signal, with blocks, values, checks and what needs new
  blocks.

Related, outside this vault: `~/.claude/skills/signal-mixing/references/`
(`compression.md`, `parallel-filters.md`, `metal-drums.md`).
