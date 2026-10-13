+++
title = "GGD Modern & Massive 2 — Hand-Pair MIDI Map"
weight = 30
+++

How the GGD Modern & Massive 2 (MM2) drum library is mapped in Signal: the
"hand-pair" shell layout, and how the preset addresses each drum's
articulations. Builds on [Sampler File Formats & Mapping](/sampler-file-formats/).

## Where the map lives

The note→drum assignment is in the plain-text `.signalpreset` `note_routing` —
**not** in the binary `.signalpack`. Packs are audio + zone organization
(velocity / round-robin / mic / articulation); the preset is the sole mapping
authority.

```text
GGD Modern and Massive 2/
├── Packs/      *.signalpack    — decoded audio + embedded LibrarySpec zones
├── Engines/    *.signalengine  — one pack + per-mic layers
└── Presets/    *.signalpreset  — engines + note_routing   ← the map
```

## Two mapping mechanisms

MM2 packs pin each zone to a key — but that key is the pack's *articulation
selector*, not a performance note (e.g. the hats pack puts Closed @42, Tight
@43, Pedal @44, Open @46). The preset uses two mechanisms so it can lay the kit
out freely while the packs stay untouched:

- **Percussion mode** (automatic, from `category "drum-kit"`): the engine plays
  at natural pitch and — for single-articulation packs (kick, each tom) — fires
  on *any* routed note. So an L/R pair is just two notes on the same drum, no
  detune.
- **Per-route articulation** (`NoteRoute.articulation`): for multi-articulation
  packs (snare, hats, ride, crashes, china, splash) ONE shared engine is routed,
  and each note names the articulation to fire. No engine duplication.

Kick and snare have two distinct factory drums in this preset, so L/R use the
two different engines (realistic alternation); toms reuse their single engine.

## Playback

- **One-shot** — drums play to the sample's natural end and ignore note-off, so
  short MIDI gates still ring out (percussion mode, automatic).
- **Hi-hat choke** — the `hats` engine has `choke_group "hats"` (no `choke_on`,
  so monophonic), so every hat hit (open / closed / tight / pedal) silences the
  ringing hat. Kick, snare and toms ring freely (no choke — you can't "close" a
  snare).
- **Cymbal choke** — `ride`, `crash-l`, `crash-r`, `china`, `splash` each set
  `choke_group "<id>" choke_on ( "Choke" )`: crashes ring and overlap, and only
  the **Choke** articulation note (61 / 65 / 71 / 75 / 77) stops the ringing
  cymbal — like grabbing it. Each engine has its own group, so a ride choke
  doesn't silence a crash.

## Reference map — "Metal Monster" preset (C1 = 36)

### Shells (hand pairs)

| Note | | Hand | Engine / articulation |
|---|---|---|---|
| 35 B0 | Kick L | L | `kick-l` (Tama Maple) |
| 36 C1 | Kick R | R | `kick-r` (Vibe Aluminium) |
| 37 C#1 | Snare Cross Stick | — | `snare-a` "Cross Stick" |
| 38 D1 | Snare L | L | `snare-a` "Hit" (Tama Abe) |
| 39 D#1 | Snare Wires Off | — | `snare-a` "Wires Off" |
| 40 E1 | Snare R | R | `snare-b` "Hit" (Yamaha Oak) |
| 41 / 42 | Rack Tom 1 L / R | L/R | `rtom1` |
| 43 / 44 | Rack Tom 2 L / R | L/R | `rtom2` |
| 45 / 46 | Floor Tom 1 L / R | L/R | `ftom1` |
| 47 / 48 | Floor Tom 2 L / R | L/R | `ftom2` |

### Cymbals & hats (single, per-route articulation)

| Note | Piece | Engine / articulation |
|---|---|---|
| 49 C#2 | Hats Tight Tip | `hats` "Tight Tip" |
| 50 D2 | Hats Tight Edge | `hats` "Tight Edge" |
| 51 D#2 | Hats Closed Tip | `hats` "Closed Tip" |
| 52 E2 | Hats Closed Edge | `hats` "Closed Edge" |
| 53 F2 | Hats Open 1 | `hats` "Open 1" |
| 54 F#2 | Hats Open 2 | `hats` "Open 2" |
| 55 G2 | Hats Open 3 | `hats` "Open 3" |
| 56 G#2 | Hats Pedal Chick | `hats` "Pedal Chick" |
| 57 A2 | Hats Pedal Ching | `hats` "Pedal Ching" |
| 60 C3 / 61 C#3 | Crash Left / Choke | `crash-l` "Crash" / "Choke" |
| 64 E3 / 65 F3 | Crash Right / Choke | `crash-r` "Crash" / "Choke" |
| 68 G#3 | Ride Bow | `ride` "Bow" |
| 69 A3 | Ride Bell | `ride` "Bell" |
| 70 A#3 | Ride Crash | `ride` "Crash" |
| 71 B3 | Ride Choke | `ride` "Choke" |
| 74 D4 / 75 D#4 | China Crash / Choke | `china` "Crash" / "Choke" |
| 76 E4 / 77 F4 | Splash Crash / Choke | `splash` "Crash" / "Choke" |

Shells own 35–48; cymbals sit at their chart positions (49+) so nothing
collides. This preserves GGD's discrete "easy-programming" articulations (the
hats Open 1/2/3, Closed Tip/Edge, etc.) — each just lands on a chosen note.

## Known gaps

- **CC-continuous hi-hat** (foot-pedal CC sweeping open→closed) is **not in the
  Signal data** — the extracted GGD source only has 3 discrete open levels
  (Open 1/2/3) + pedal + closed/tight; the continuous CC lives in the encrypted
  Kontakt `.tci`. Approximating it would be a runtime CC→openness crossfade over
  the discrete layers, not a re-import.
- **Crash Far Left** from the GGD chart has no engine in this preset (only two
  crashes), so it's unmapped here.

## Editing checklist

1. Back up the `.signalpreset` first.
2. Styx is line-oriented — each `note`/`targets`/`articulation` on its own line
   inside a multi-line `{ … }` (one-liners fail to parse).
3. Every `note_routing` target must be an `engines` id (ids unique); each
   `articulation` must match a real articulation string in that engine's pack
   (case-insensitive).
4. Validate by loading through `PresetSpec::from_file`.

### Per-preset variation

Engine ids are largely consistent across the ~29 drum presets; some lack the
second kick/snare (those fall back to one engine). "Rooms Demo" carries only
kick/snare/rooms and needs its own handling.
