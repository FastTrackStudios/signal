# Track Organization

Canonical FTS project hierarchy for individual song projects and combined setlists.

## FTS Hierarchy

r[organize.hierarchy.structure]

Every FTS project has this canonical structure:

```
Click + Guide/
  Click       — metronome click (MIDI or native click source)
  Loop        — loop/backing track
  Count       — count-in MIDI cues
  Guide       — section name MIDI cues
Keyflow/
  CHORDS      — chord chart MIDI
  LINES       — melodic lines MIDI
  HITS        — rhythmic hits MIDI
TRACKS/
  (content tracks — instruments, vocals, etc.)
Reference/
  Mix         — full song reference (mp3, bounce)
  Stem Split/
    Drums, Bass, Guitar, Vocals, Other, Piano
```

r[organize.hierarchy.always-present]

Click+Guide, Keyflow, and TRACKS folders are always created, even when empty.
When empty, standard placeholder tracks are added (Click/Loop/Count/Guide for
Click+Guide, CHORDS/LINES/HITS for Keyflow).

## Track Classification

r[organize.classify.guide]

Guide tracks are identified by name: Click, Loop, Count, Guide (case-insensitive).

r[organize.classify.keyflow]

Keyflow tracks are identified by name: CHORDS, LINES, HITS (case-insensitive).

r[organize.classify.structural]

Structural folder tracks are stripped and rebuilt:
Click/Guide, Click + Guide, TRACKS, Keyflow, MIDI bus, Reference (case-insensitive,
must be folder parents).

r[organize.classify.reference]

Reference/mix tracks are detected by:
- Track name: contains ".mp3", "mix", "reference", "bounce", "master"
- Item content: source files ending in .mp3 or containing "mix"/"bounce"
- Empty tracks (no items, not folders) — treated as reference placeholders

r[organize.classify.stemsplit]

Stem split tracks are detected by:
- Source file patterns: `(Drums)`, `(Bass)`, `(Vocals)`, `(Guitar)`, `(Piano)`, `(Other)`
- Track name: "stem split" (non-folder)
- Tracks inside an existing Stem Split folder

r[organize.classify.content]

Everything else is classified as content and placed in the TRACKS folder.

## Guide Generation

r[organize.guide.click]

Click generates a continuous MIDI item spanning from the count-in start to the
last region end. Accent note (76) on beat 1, normal note (77) on other beats.

r[organize.guide.count]

Count generates per-section MIDI items for count-in measures:
- Starts at COUNT-IN marker (if present) or one measure before section
- Multi-measure pattern: sparse first measures (beats 1 and halfway),
  full last measure (all beats)
- Each beat gets a distinct MIDI note (C4 + beat_number)

r[organize.guide.section-cue]

Guide generates per-section MIDI items with a single note indicating section type:
- Intro=36(C2), Verse=38(D2), Pre-Chorus=40(E2), Chorus=41(F2)
- Bridge=43(G2), Solo=45(A2), Outro=47(B2), Breakdown=48(C3)
- Generic section=60(C4)

## Ruler Lanes

r[organize.lanes.standard]

FTS standard ruler lane layout (8 lanes):
1. SECTIONS (flag 8 = default region lane)
2. MARKS
3. SONG (flag 4 = default marker lane)
4. START/END
5. KEY
6. MODE
7. CHORDS
8. NOTES

r[organize.lanes.classification]

Markers/regions are classified by name:
- SONGSTART, SONGEND, COUNT-IN → MARKS (lane 2)
- =START, =END, PREROLL, POSTROLL → START/END (lane 4)
- Regions → SECTIONS (lane 1)
- Song-spanning regions on SONG lane are preserved
