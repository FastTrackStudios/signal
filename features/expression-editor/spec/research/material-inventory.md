# Demo-project material inventory

Resolves #159. Surveyed 2026-08-09 on THEBATTLESHIP.

**Nothing here is copied into the tree.** Every path below is a location on the
machine. Audio is never committed; the demo project references material by
absolute path, and any fixture that must be committed is either tiny, authored
in-tree, or synthesized.

Paths are given relative to `~/Downloads` unless stated otherwise.

---

## 1. Worship multitracks

Two locations, six songs total, all with a **Click** and a **Guide** track.

### 1a. `Worship MultiTracks/` — five songs, 5.6 GB

Flat per-song folders, one stereo WAV per stem, **44.1 kHz / 16-bit / stereo**
throughout. Naming is `<Song> - <Stem>.wav`.

| Song | Key | Stems | Length | Stem list |
|---|---|---:|---|---|
| God, I'm Just Grateful (Elevation Worship) | D | 19 | 5:04 | Click, Guide, Bass, Synth Bass, Drums 1–2, EG 1–4, Keys, Piano, Organ, Synths, Strings, Loop, BGVS, Choir 1–2 |
| Holy Forever (Bethel Music) | Bb | 13 | 8:23 | Click, Guide, Bass, Synth Bass, Drums, AG, EG 1–2, Keys 1–3, Piano, Loop |
| Thank God I'm Free (Elevation Rhythm) | E | 36 | 5:00 | Click, Guide, Bass, Synth Bass, Drums (Live), EG 1–11, Keys 1–5, Piano 1–2, Piano FX, Arps 1–2, Bells, Loop 1–2, Strings, Synth Lead/Pad/FX, Vox FX, BGVS, Choir |
| Washed (Elevation Rhythm) | B | 18 | 3:43 | Click, Guide, Bass, Drums (Live), AG, EG, Keys 1–5, Piano, Perc 1–2, FX 1–2, Loop, Saxophone |
| Who Else (Gateway Worship) | Ab | 23 | 4:46 | Click, Guide, Bass, Synth Bass, Drums, AG, EG 1–5, Keys 1–8, Piano, Organ, Percussion, Synth FX |

Alongside the song folders:

- `lyrics/` — six timed lyric JSONs (`god-im-just-grateful`, `holy-forever`,
  `praise`, `thank-god-im-free`, `washed`, `who-else`).
- `sync/praise.kf.json` — one keyflow lyric-sync artefact.

### 1b. `Elevation Worship - Praise-20260712T200150Z-2-001/` — the sixth song, 2.0 GB

Structured as delivered by MultiTracks.com: `Elevation Worship - Praise/`
containing a metadata-only folder named `4-4  _  127 BPM  _  A Major`, plus:

- `- MultiTracks/` — **23 stems, 44.1 kHz / 24-bit / stereo**, 4:57:
  `01 Click`, `02 Cue`, `03 Original Track`, `04–05 BGVS`, `06 Choir`,
  `07 Organ`, `08 Keys`, `09 Piano`, `10–11 Electric Bass 1–2`,
  `12 Synth Bass`, `13 Acoustic Guitar`, `14–20 Electric Guitar 1–7`,
  `21 Loop`, `22 Hand Percussion`, `23 Percussion`.
- `- SingleTrack/` — `01 Click`, `02 Cue`, `03 Master`.

Note the naming difference: the Praise delivery calls the guide track **Cue**,
the other five call it **Guide**. Anything that keys off track names for the
demo project must handle both. Bit depth also differs (24-bit here, 16-bit for
the other five), as does the numeric filename prefix convention.

### 1c. `PNG WORSHIP COLLECTIVE SESSION FILES/` — six original songs, ~13.8 GB

This is the richest material on the machine and it is **raw session audio, not
delivered stems** — Pro Tools sessions with real takes, comps and playlists.
All audio is **48 kHz**, mostly 24-bit (some 16-bit prints), mono for close
sources and stereo for prints/refs.

| Song | `.ptx` | Audio files | Size | ARA data present |
|---|---:|---:|---|---|
| 01 ALL THAT I AM | 3 | 39 | 1.1 G | RX Spectral Editor |
| 02 LORD OF THE FIGHT | 1 | 99 | 3.3 G | **Melodyne**, RX Spectral Editor |
| 04 PRESENCE | 2 | 56 | 1.1 G | **Melodyne**, RX Spectral Editor |
| 07 I KNOW A NAME | 1 | 26 | 2.0 G | RX Spectral Editor |
| 09 HANNAH'S SONG | 1 | 23 | 2.6 G | RX Spectral Editor |
| 10 REASON WHY | 1 | 28 | 3.7 G | RX Spectral Editor |

Track families across the sessions: `ClickPrint`, `ShakePrint`, `PGT` (programmed
guide track), real `AC GTR` takes (dozens of numbered playlist files in
LORD OF THE FIGHT), `GTR A/E - …` (12 String, Bridge, Chord, Lower, Strum, Uke,
Ambient, Big Chords, CH Hook, Chords, Drive, Drive MusicMan, Muted Pluck DLY),
`Bass`, `Drums`, `Vocals`, `DemoVox`, `Groove Shaker`, `Intro SFX`, plus
Demucs-style separated stems (`_Bass`, `_Drums`, `_Guitar`, `_Other`, `_Piano`,
`_Vocals`) generated from the reference mixes.

Two caveats:

- The per-song `_Drums` / `_Bass` / `_Vocals` files are **source-separated from a
  mix**, not close mics. They are fine as pitched/unpitched analysis input but
  are not a multitrack drum kit.
- `soxi` cannot read these files (Pro Tools BWF chunk ordering); the `fmt `
  chunk parses fine directly. Whatever loads them must not assume a canonical
  chunk layout.

There are also six MIDI exports at the top of that folder (`01 ALL THAT I AM
MIDI EXPORT.mid` etc.) — see §4.

---

## 2. Orchestral

### 2a. `Colombus Symphony Parsons Orchestral Parts/` — 151 MB, **notation only**

**469 PDFs and no notation source files, no audio.** One `Placeholder.mp3` and
one `.pages` cover document are the only non-PDF files.

Sections and part counts:

| Section | PDFs | Section | PDFs |
|---|---:|---|---:|
| Flutes | 19 | Trumpets | 41 |
| Oboe–English Horn | 22 | Trombones (incl. Tuba) | 27 |
| French Horns | 39 | Timpani | 15 |
| Violin 1 | 16 | Percussion | 35 |
| Violin 2 | 16 | Harp | 14 |
| Viola | 16 | Extras | 15 |
| Cello | 16 | Double Bass | 16 |

Full symphonic instrumentation, roughly 13–15 charts per section (an Alan
Parsons Project programme: DAMNED IF I DO, DON'T LET IT SHOW, DR. TARR &
PROFESSOR FETHER, DON'T ANSWER ME, GAMES PEOPLE PLAY, OLD AND WISE, SILENCE AND
I, SIRIUS–EYE IN THE SKY, STANDING ON HIGHER GROUND, TIME, WHAT GOES UP, plus
tacet/placeholder pages for THE RAVEN, TO ONE IN PARADISE, THE VOICE, PRIME
TIME, WOULDN'T WANNA BE LIKE YOU).

**This is engraving-effort material (#78), not expression-editor material.** It
is raster/vector print output with no machine-readable notes and no recordings.
Nothing in it can drive an editor mode.

### 2b. `crates/keyflow/examples/png-project-charts/` — seven MusicXML charts

MusicXML 4.0 partwise, exported from Finale v27.4 for Mac, alongside `.musx`
sources, PDFs and rendered PNGs.

| Chart | Measures | `<harmony>` | `<note>` | Clefs |
|---|---:|---:|---:|---|
| 01 ALL THAT I AM (VS2 in C) | 69 | 130 | 0 | F |
| 01 ALL THAT I AM (VS2 in G) | 70 | 131 | 0 | F |
| 02 LORD OF THE FIGHT | 88 | 110 | 0 | F |
| 04 PRESENCE | 51 | 82 | 0 | F, G |
| 07 I KNOW A NAME | 123 | 94 | 0 | F |
| 09 HANNAH'S SONG | 88 | 139 | 0 | F, G |
| 10 REASON WHY | 86 | 77 | 0 | G |

**Critically: zero `<note>` elements in any of them.** These are *chord charts* —
harmony symbols, rests, rehearsal words and dynamics on a single generic part
("MusicXML Part" / ARIA Player). There is no melody, no per-instrument staff and
no instrumentation to speak of. They carry structure and harmony, nothing
playable.

They do, however, **correspond one-for-one with the PNG session audio in §1c**
(same six songs, same numbering), and two `.kf` keyflow charts sit next to them
(`02 LORD OF THE FIGHT Master RS.kf`, `04 PRESENCE Master RS.kf`, plus
`extra/Dwelling Place.kf` and `extra/Life Giving Water.kf`). So: **notation +
audio for the same songs**, but the notation is harmonic/structural only.

---

## 3. Guitar Pro

**Yes — one real transcription exists, and it is good.**

`Companion Pass Guitar Solo Transcription-1764176290729/Companion Pass Guitar Solo.gp`
(29 KB, plus a 750 KB PDF render).

- Guitar Pro **7.6.0** container format (`.gp` = zip with `Content/score.gpif`).
- One track, `Electric Guitar`, pitched, standard tuning (`40 45 50 55 59 64`).
- **162 bars, 716 beats, 568 notes.**
- Technique density is exactly what scenario 4 needs to exercise:
  **40 bend properties, 71 slides, 143 vibrato markings, 3 harmonics**, 6 tempo
  automations.

Beyond that, a 600-file corpus exists but is all synthetic test data:

- `~/reference/MuseScore/src/importexport/guitarpro/tests/data/` and
  `.../guitarbendimporter_data/` — 122 `.gp`, 78 `.gpx`, 41 `.gp5`, 25 `.gp4`,
  9 `.gp3`. One file per feature (`bend.gp*`, `dive_*.gp`, `palm-mute.gp*`,
  `slide-*.gp*`, `tap-slap-pop.gp*`, `volta.gp*`, …), across every format
  generation.
- A duplicate of the same corpus lives under `~/FastTrackStudio-Legacy/libs/reference/sheet-music/musescore/`.

That corpus is **GPL-3 MuseScore test data**. It is read-only reference for
checking a parser against known-good cases; it is not to be vendored, and per
the map's clean-room rules no MuseScore parsing code enters this tree either.

**Verdict: scenario 4 does not need anything sourced.** `Companion Pass Guitar
Solo.gp` is a real, technique-dense, single-guitar file in the newest format.

---

## 4. MIDI

### 4a. Plain MIDI — plentiful

The six `.mid` exports in `PNG WORSHIP COLLECTIVE SESSION FILES/`, all format 1,
9600 PPQ:

| File | Tracks | Notes | Note channels | CCs |
|---|---:|---:|---|---|
| 01 ALL THAT I AM MIDI EXPORT.mid | 5 | 874 | 16 | 64 |
| 02 LORD OF THE FIGHT MIDI.mid | 10 | 4537 | 1, 16 | 64 |
| 04 PRESENCE MIDI EXPORT.mid | 2 | 701 | 16 | 64 |
| 07 I KNOW A NAME MIDI EXPORT.mid | 2 | 1783 | 16 | 64 |
| 09 HANNAH'S SONG MIDI EXPORT.mid | 12 | 3405 | 1, 16 | 64 |
| 10 REASON WHY MIDI EXPORT.mid | 4 | 848 | 1, 16 | 64 |

`09 HANNAH'S SONG` is the most useful: named tracks `Drums`, `Drums.dup1–3`,
`Bass Guitar`, `Acoustic Guitar`, `Keyscape`, `Kontakt`, `Drumz` — a real
multi-instrument arrangement with a named drum part, so it serves **both** MIDI
mode and Drums mode (named kit lanes) without any authoring.

Also `~/Downloads/Going Home MIDI.mid` (3 tracks, 1694 notes, channel 1).

### 4b. MPE — essentially absent

A full scan of **12,186 `.mid`/`.midi` files** across `~` and
`/run/media/AudioHaven` (notes on ≥4 non-global channels *and* pitch bend on ≥4
of those channels) found exactly **three** MPE-shaped files, all of them vendor
demo content inside an archived Ableton Live 12 **Trial** installation on a
rescue drive:

```
/run/media/AudioHaven/INBOX/old-samsung-rescue/bottles/bottles/Testing/drive_c/
  ProgramData/Ableton/Live 12 Trial/Resources/Max/resources/help/msp/MPE_Lead.mid
  ProgramData/Ableton/Live 12 Trial/Resources/Max/resources/help/msp/MPE_Pad.mid
  ProgramData/Ableton/Live 12 Trial/Resources/Max/examples/resources/prefab_mpe.mid
```

`MPE_Pad.mid` is the strongest of the three: notes, pitch bend **and** CC74 on
14 member channels. None of them emits an MPE Configuration Message (RPN 6), so
zone setup is implicit — which is itself a realistic parser case.

These are Ableton-shipped files under an expired trial. They are usable as a
local, uncommitted sanity input while building the parser; they are **not**
fixtures. Nothing else on the machine is MPE. **Scenario 3 needs its MPE
material authored** — see §6.

---

## 5. Multitrack drums

`RomanStyx_SevenFeel/` — **36 WAV files, 44.1 kHz / 24-bit, 29.5 s, 130 BPM.**
A Cambridge-MT "Mixing Secrets" raw multitrack excerpt from Roman Styx's
*7 Feel*. `Readme.txt`: *"provided for educational purposes only … should not be
used for any commercial purpose without the express permission of the copyright
holders."*

Close-miked kit, one file per source — precisely the shape Drums/Percussive mode
wants:

```
01 KickIn (mono)      02 KickOut (mono)     03 SnareUp (mono)
04 SnareDown (mono)   05 SnareSample (st)   06 HiHat (mono)
07 Tom1 (mono)        08 Tom2 (mono)        09 Overheads (st)
10 DrumsRoomMono      11 DrumsRoomStereo    12 Claps (st)   13 Shaker (st)
```

Plus, in the same session: `16 Bass` (mono), `17–20 AcousticGtr` incl. **DI**
and double-tracks, `21–28 ElecGtr 1–5` incl. double-tracks, `29–30 Synth 1–2`,
`31–32 LeadVox` (+ double), `33–36 BackingVox 1–2` (+ doubles), `14–15 SFX`.

One 30-second excerpt is short for a demo but ideal for tests: it loads fast, it
is bleed-realistic, and the KickIn/KickOut and SnareUp/SnareDown pairs make the
multi-mic alignment case real rather than hypothetical.

The map calls for "a real **open** multitrack drum dataset". Cambridge-MT is
free-to-download and educational-use, not open-licensed. It is fine for local
development and screenshots; if a drum corpus ever needs to be redistributed or
checked into CI, that is a separate sourcing question.

---

## 6. Coverage by mode

Modes as defined in `expression-editor-core/src/mode.rs`.

| Mode | Real material | Source | Gap |
|---|---|---|---|
| **MIDI** | ✅ strong | 6 PNG session MIDI exports (874–4537 notes); `09 HANNAH'S SONG` has named per-instrument tracks | none |
| **MPE** | ❌ **none usable** | 3 Ableton-trial demo files, vendor content, no MPE Config Message | **must be authored** |
| **Drums** | ✅ strong | `09 HANNAH'S SONG MIDI EXPORT.mid` (`Drums`, `Drums.dup1–3`, `Drumz`) for the MIDI side; `RomanStyx_SevenFeel` close mics for the audio side | licence is educational-use, not open |
| **Guitar** | ✅ strong | `Companion Pass Guitar Solo.gp` — GP 7.6, 162 bars, 568 notes, 40 bends, 71 slides, 143 vibrato | none |
| **Vocals** | ✅ strong | PNG sessions' real `Vocals`/`DemoVox` takes (48 k/24-bit) — two songs already carry Melodyne ARA data; `RomanStyx 31 LeadVox`; six worship `Guide`/`Cue` tracks; timed lyric JSON for six songs in `Worship MultiTracks/lyrics/` | syllable timings exist only as lyric JSON, not per-note |
| **Audio** (pitched) | ✅ strong | `RomanStyx 16 Bass`, `18 AcousticGtrDI` (clean DI, ideal for pitch tracking); worship `Bass`/`Piano`/`AG` stems; PNG `AC GTR` raw takes | none |
| **Percussive** (unpitched) | ✅ strong | `RomanStyx` kick/snare/hat/tom/OH/room; worship `Loop`, `Perc 1–2`, `Percussion`, `Hand Percussion`, `Drums (Live)`; `Groove Shaker`, `ShakePrint` | none |

**Six of seven modes are covered by real material. MPE is the only hole.**

### What MPE needs

Nothing on this machine is a usable MPE fixture. Options, cheapest first:

1. **Synthesize it in-tree.** Author a small MPE `.mid` programmatically — a
   handful of overlapping notes across member channels with per-note bend,
   channel pressure and CC74 contours, plus a proper RPN 6 MPE Configuration
   Message. Deterministic, tiny, committable, and it can be *generated* by a
   test rather than stored. This is the recommendation.
2. **Capture it.** The S88 MK3 is not MPE. No MPE controller is on hand, so this
   would mean sourcing hardware.
3. **Download a permissively-licensed MPE example.** Possible, but a synthesized
   fixture is better for tests anyway because we control exactly which edge
   cases (zone setup, pitch-bend range, channel reuse, note stealing) appear.

The Ableton trial files stay useful as an unpaired real-world sanity check
during parser development — read locally, never copied in.

**Resolved (#177): option 1, at `expression-editor-daw::fixture`.** The fixture is
generated, not stored — thirteen voices, an eight-note chord that fills the whole
member zone under a line that takes the released channels back, per-note bend,
channel pressure and CC74 on every note, an RPN 6 Configuration Message declaring
a lower zone of eight members, and RPN 0 stating the 48-semitone per-note bend
range that `expression-editor-reaper` loads takes with (now one constant,
`expression_editor_daw::DEFAULT_BEND_RANGE`, so the two cannot drift).
`fixture::parse_config` reads the zone and range back out of a CC stream, which is
the check that catches a file meaning 2 where the reader assumes 48.
`tests/mpe_fixture.rs` asserts the properties the found files lacked.

### Second-order notes

- **Orchestral material drives no editor mode.** The Colombus PDFs are for #78;
  the seven MusicXML charts are chord charts with zero notes. Where they help is
  as *structure* for the demo project: they align one-to-one with the PNG
  sessions, so section markers and harmony can come from the chart while the
  audio comes from the session.
- **The demo project should be built on the PNG sessions**, not the delivered
  worship multitracks: real takes, real vocals with prior Melodyne passes, plus
  matching MIDI, chord charts and `.kf` files for the same six songs. The
  MultiTracks.com deliveries are better as a wide-track-count stress case (36
  stems on Thank God I'm Free) and for Click/Guide behaviour.
- **Two sample-rate families**: 44.1 k (worship deliveries, RomanStyx) and 48 k
  (PNG sessions). Whatever the demo project loads must handle both, and the
  quantize/align tools must not assume a single rate across a project.
- **Bit depths seen**: 16-bit (5 worship songs, some PNG prints), 24-bit (Praise,
  RomanStyx, most PNG session audio). No float files found.
