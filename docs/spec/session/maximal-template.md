# The Maximal Session

A reference session that covers every base the dynamic template has to
cover, so that a real session — which always has a different shape — can
be checked against it. The Rust builder in
`dynamic_template::golden_session` writes it from the config-derived
template and a fixture song shape; `just daw-template` regenerates the
committed project files under `features/dynamic-template/fixtures/golden/`
and `just daw-scene <slug>` renders each scene of it, with every scene's
picture committed beside them (`just daw-scenes` re-renders the set).

The checklist below is **regenerated from tests**, never hand-ticked: a
box is ticked exactly where a check in
`dynamic_template::golden_session::checklist` walks the built session and
finds the shape, and a test fails when this file disagrees with the
checks.

Drums can be made perfect once: a kit is a kit. Guitars take a different
shape every song, so for them the reference is a checklist rather than a
fixed tree.

## Checklist

### Drums — the kit's pieces and `Process/`, in one folder

- [x] Every piece a folder: **Kick, Snare, Toms, Cymbals, Rooms**, in one
      red family (folder crimson, pieces oxblood → scarlet → terracotta →
      rose → wine).
- [x] Each piece's close mics under a **Sum**; the piece's sends beside
      the Sum, not inside it.
- [x] **Kick**: In, Out, Trig, Sample; Sub; **Verb/** Room.
- [x] **Snare**: Top, Bottom, Trig, Sample, Fund; **Verb/** Plate, Hall, Nonlin.
- [x] **Toms**: per tom T*n*, Trig, Sample, Fund; one shared **Verb/** Room, Nonlin.
- [x] **Cymbals**: OH, Hi-Hat, Ride. **Rooms**: two stereo pairs, Rooms
      and Rooms Far.
- [x] One-note tracks (Fund, Sub) get Gate → band-pass at the fundamental
      → Sat; triggers get nothing.
- [ ] **Process/Compress/**: Dry, Tight, Punch, Smash, Crunch — a balance
      group (`session-daw::balance`): one fader up, the others down by the
      same amount between them; a fader at silence is out.
- [ ] **Process/FX/**: Room Sim (captured rooms, compressed fast on the
      way back) and a **Verb** bank: Wood Room, Music Club, Stadium,
      RMX 16, Nonlin, Brick Wall.
- [x] Drums — the kit and Process in it — reaches the mix by a send to
      **DRUM BUS**.
- [x] **Process** is in the mixer and out of the track panel, all of it.
- [x] Scenes: Tracking, Mixing, Overview (pieces collapsed), Advanced,
      FX.

### Guide and Keyflow — `Guide/`, `Keyflow/`

- [ ] **Guide/** Click, Guide, Shaker at the top of the session, to the
      headphone mixes only (`flow.scenes.guide-folder`).
- [ ] **Keyflow/** KEY, CHORD, LINES, Lyrics, HITS as MIDI items
      (`flow.scenes.keyflow-folder`).

### Percussion — `Percussion/`

- [x] Shaker, Tambourine, Claps → **PERC BUS** under INST BUS
      (`flow.percussion.folder`).

### Bass — `Bass/`

- [x] **Guitar/** DI, Amp — directly under the piece, no Sum.
- [x] **Synth/** Sub, Synth.
- [x] To **BASS BUS** by send.
- [ ] Upright / DI-only variants when a song has them.

### Guitars — `Electric/`, `Acoustic/` (a different shape every song)

- [x] **No Sum folders** anywhere under the guitars: the kit's Sum
      gathers one drum heard several ways, and a guitar's depth is a
      different thing — a double is two performances, an octave layer a
      third, each channel on its own sources.
- [x] The three shapes in one fixture (`flow.guitars.golden`): **Rhythm**
      double-tracked with two octave layers and seven sources a channel,
      **Lead** a DI-only double with no folder under its channels, and
      **Solo** a single track with a Harmony beside it.
- [x] Electrics and acoustics are **separate top-level folders with
      their own buses** — no Guitars folder, no GUITAR BUS.
- [x] Every electric part goes to exactly one of **GTR RHYTHM, GTR LEAD,
      GTR SOLO** by a send — the three live under **ELECTRIC BUS** in the
      bus list, not in the folder. A part moves between them mid-song by
      automating the sends. Stems are the three buses.
- [x] **ELECTRIC BUS sends back into the Electric folder**, which is a
      dead end that meters the electrics after their bus, and is the
      **VCA lead of ELECTRIC BUS** (group 1, with mute and solo): the
      folder's fader is the electrics' fader. No VCA track.
- [x] **Acoustic/** Steel, Nylon, Nashville (the high-strung layer over
      the steel) → **ACOUSTIC BUS**, which returns to the Acoustic folder
      the same way (group 2).
- [ ] **Rhythm**: double-tracked, each channel with seven sources (DI,
      pedalboard, two amps with a 57 and a 121 each), and Main and
      Octave layers doing the same part (`flow.guitars.golden`).
- [ ] **Lead**: a double-tracked DI-only part — L and R tracks, no
      folder under them.
- [ ] **Solo**: one DI track, with a Harmony Solo beside it.
- [ ] Sources start at the default balance
      (`flow.guitars.mixing.source-defaults`): DI muted and centred
      beside other sources, pedalboard muted beside an amp, one amp's
      57 and 121 hard left and right, two amps' mics still hard left and
      right with Amp 1's track 60 % left and Amp 2's 60 % right.

### Keys — `Keys/`, Synths — `Synths/`

- [x] Keys as parts: Piano (a stereo track), Rhodes, Wurli, Organ → **KEYS BUS**.
- [ ] Keys: **Wurli** beside the Rhodes (`flow.keys.parts`).
- [x] Synths by family — SY Arps, SY Pads, SY Leads, SY Chords — with the general synths beside them → **KEYS BUS**.
- [ ] Synths sorted by family — **SY Arps, SY Pads, SY Leads, SY
      Chords** — with two general synth tracks beside them
      (`flow.synths.families`, `flow.synths.golden`).
- [x] Colours, in session order and hue order: electrics blue,
      acoustics seafoam, keys green; synths purple, the synth bass
      included.
- [ ] Clav, strings and pads as the song has them.

### Orchestra — `Orchestra/` (expanded in a later spec)

- [x] **Winds/** Flute, Oboe, Clarinet, Bassoon.
- [ ] **Brass/** Trumpets, Horns, Trombones, Bass Trombone, Tuba.
- [ ] **Strings/** Violin 1, Violin 2, Viola, Cello, Bass.
- [ ] **Orch Percussion/** — present, empty for now.
- [ ] One bus per section under INST BUS (`flow.orchestra.golden`).

### Vocals — `Vocals/`

- [x] Leads Ron (EN/ES/PT), Belen (EN/ES) and Aline (PT), each a mix track with Main and DBL and a source per language; BGV parts; a four-section choir; VOX EN/ES/PT VCAs → **LEAD VOX BUS**.
- [x] BGVs folder → **BGV BUS**; the vocal FX template (`vocal-fx.rpp`)
      folded in.
- [ ] **Three languages** as source tracks under each layer —
      `Vocals / Ron / Main / {EN, ES, PT}` — with a VCA per language over
      every language's sources (`flow.vocals.language`,
      `flow.vocals.golden`).
- [ ] Leads **Ron** (EN, ES, PT), **Belen** (EN, ES), **Aline** (PT), each
      a mix track with Main and DBL under it and a source per language
      under those.
- [ ] BGV parts: Octave Down, Octave Up, Higher Harmony, Lower Harmony,
      Whisper, Bass, Tenor, Alto, Soprano — doubled — and a many-layer
      "Hey!" in `All`.
- [ ] A four-section **Choir** per language.

### Buses — `MIX BUS/`

- [x] `MIX BUS / INST BUS / {DRUM, BASS, ELECTRIC/{GTR RHYTHM, GTR LEAD,
      GTR SOLO}, ACOUSTIC, KEYS}` and `VOX BUS / {LEAD VOX, BGV}` —
      `dynamic_template::buses`' tree without GUITAR BUS, with the three
      electric buses added.
- [x] A bus is a track with no items, unarmed; its chain is EQ → Comp.
- [x] Scene: **Buses** — the tree alone.
- [ ] Monitor buses beside MIX BUS (Click + Guide, Headphones, Talkback,
      Utility).

## Routing rules the reference follows

1. An instrument folder reaches the mix by a **send to its bus**, parent
   send off (`to_bus`).
2. Where a folder's fader must still be the instrument's fader — the
   electrics, the acoustics — its bus sends back into it, the folder is
   a **dead end** that meters that return, and it is the **VCA lead** of
   the bus.
3. A **stereo pair** is one stereo track (`pair()` in the template:
   `NCHAN 2`, a stereo input). A folder over an `L` and an `R` is still
   read as one instrument by the rack (`tone::is_pair`) for sessions
   that arrive that way, with the halves as rails carrying nothing.
4. Buses nest as folders inside the bus they feed; a bus reaches its
   parent by the ordinary folder send.
5. A balance group is a folder named `Compress…`.
