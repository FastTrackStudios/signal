# Vocal leveling: four lanes on each item, one level for the song

What an engineer does to a vocal before any compressor, done offline and
kept where it can be edited: the noise between lines taken down, the
breaths tucked under, the esses eased, and every line ridden to where
the song wants it.

## Why this shape — what was measured

Unbreakable's lead vocal, its sung level section by section (front
capsule, dBFS): the verses at −39, −37 and −34, the pre-choruses at
−34 to −29, the choruses at −22 to −23, the outro at −16.5 — 27.6 dB from
the quietest section to the loudest. A ride that levels each item
against itself leaves that alone: a verse take is quiet all through, so
it is even already. The level has to be one for the whole track.

A ride read off single 10 ms blocks evens out every syllable — a
compressor. Read off half a second of singing around each block, it
lifts a line and leaves the words inside it their shape.

## Rules

r[level.vocal]
Leveling a vocal track makes four lanes for each item that plays, from
one analysis of the item's audio (a two-channel vocal's front capsule):
a gate, opened a fixed distance above the recording's own noise floor; a
de-breath, tucking under what is not sung and quieter than the song's
level; a ride, bringing each phrase toward one level for the whole
track; and a de-ess, easing each sibilant span. A makeup gain over the
whole track then puts its sung level where its louder lines were sung.
The four sum, in dB, to the item's take volume envelope — what plays,
what the waveform draws, and what rides with the item through every
edit — and are kept with the track, per item, with what made them. On
Unbreakable's lead the sections end within 5 dB of each other, the
verses about 2 dB under the choruses (90 % consistency).
