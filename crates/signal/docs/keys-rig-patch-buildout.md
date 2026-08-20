# Keys Rig — Patch Buildout (replacing the Gig Performer rig)

**Goal**: every patch in `Worship Gig 3.gig` plays out of the Signal engine, so
the live keys rig stops being Gig Performer + Kontakt + Omnisphere + 20 VST3s
and becomes one `fasttrackstudio --engine` process with explicit CPU/RAM
budgets. That budget control is the whole point — it is what lets the rig run
on light hardware.

Companion docs: `keys-rig-roadmap.md` (the *engine* roadmap — native DSP,
modmatrix), `pack-levels.md`, `pack-distribution.md`, the `signalpack` skill.
This file is the *content* plan: which patches, where their source lives, what
is decoded, and what is still unknown.

---

## 0. The rig we are replacing

Decoded from `Worship Gig 3.gig` (Gig Performer 5, three rackspaces + a global
rackspace). The **global rackspace is the keys rig**; the three rackspaces are
song scenes that borrow it.

### Global rackspace — instruments and their lanes

| GP node | plugin | lanes (MIDI Filter blocks) | per-lane FX |
|---|---|---|---|
| `NI Pianos` | Kontakt 8 | The Grandeur · The Gentleman · The Maverick · The Giant | `EQ: <name>` (Pro-Q 3 each) |
| — | (same Kontakt/Keyscape group) | Felt · Wing | `EQ: Keyscape Felt`, `EQ: Keyscape Wing` |
| `Omni Keys` | Omnisphere | EP 1 · EP 2 | `EQ: EP Vintage`, `EQ: EP Rhodes`, `EP Color Filter` |
| `Omni Pads` | Omnisphere | Pad · Pad Shimmer | `EQ: Omni Pad`, `EQ: Omni Pad Shimmer` |
| `Augmented GRAND PIANO` | Arturia | one lane — preset `Grand Energy` (§4) | `EQ: Arturia Grand` |

Buses: `BUS: Keys`, `BUS: Pads`, `BUS: Shimmer`, `BUS: Delay`, `BUS: Reverb`,
`BUS: Synths`. Sends: `SEND Keys`, `SEND: Pads`, `SEND: Synths`.
Global FX: Decapitator · CLA-2A · Comp Keys (R-Axx) · TAL-Chorus-LX ·
EchoBoy Jr · Saturn 2 · Portal · Raum · ValhallaVintageVerb · ValhallaShimmer ·
Pro-C 2 · Pro-MB · Pro-R 2 · Pro-Q 3 ×12.
MIDI ins: `Keys [1]`, `Arturia [1]`, `Mono Bass [9]`, `Drum Pads [10]`.

### Rackspaces (song scenes) — all three run one `Omni Synths` instance

| rackspace | lanes | notes |
|---|---|---|
| Dulcimer Dance | Dulcimer · Trance (+ `EQ: PHAT Bass`) | the Omnisphere multi holds **Worship PHAT bass**, **Hammered Dolceola**, **Club Europe Plucking Pulsars** |
| Massive Worship | Synth Pluck · Trance · Amb Key | holds **Dissolved Pluck** |
| Gospel Gospel GOSPEL | Dulcimer · Trance | + `Dulcimer Comp` |

> **The gig is fully readable** — see §3.0. `signal_synth::gig::read_gig`
> unwraps every hosted plugin's state, and for the Omnisphere instances hands
> back the `SynthMaster` Multi XML the existing `.prt_omn`/`.mlt_omn` reader
> already parses. `cargo run -p signal-synth --example gig_extract -- omni <file.gig>`
> prints the patch inventory below; `… -- dump <file.gig> <dir>` writes every
> chunk. Nothing here was transcribed off a screen.

---

## 1. The four NI pianos — KSP is already decoded

Source: `/run/media/AudioHaven/Kontakt/Keys/{The Grandeur Library,The Maverick,The Gentleman Library,The Giant}`
Extraction: **already done** for all four —
`/run/media/AudioHaven/Sampled/Keys/<lib>/<instrument>/{script_0..3.ksp, zones.tsv, persistent_0.tsv}`
plus decoded WAVs under `<lib>/wav/`. `The Giant` additionally has a
`The Giant - Cinematic` instrument.

`Kontakt/Keys/_extracted/` is **empty** — do not go looking there.

### 1.1 What the controls actually are (answered)

All line refs are `Sampled/Keys/The Grandeur Library/The Grandeur/script_0.ksp`;
the other three pianos share the "NI ESSENTIAL PIANOS" script family, so expect
the same structure with different constants.

**COLOR (soft ↔ hard) — *not* a filter sweep. It is a velocity offset into the
sample map, plus a compensating gain trim.** The instrument's own help string
says it "readjusts the sample mapping" (`:169`). The math:

```
; :857-865
select ($Mas_sliToneColor + $COLOR_OFFSET)
  case  0        -> $ColourVolumeBoost := 0
  case -1..-50   -> $ColourVolumeBoost := (vel + 20)   * $ColourVolumeFactor
  case  1..50    -> $ColourVolumeBoost := (-vel + 150) * $ColourVolumeFactor
vel := vel + $Mas_sliToneColor + $COLOR_OFFSET        ; then clamped to 1..127
; :1340
$ColourVolumeFactor := ((($Mas_sliToneColor + $COLOR_OFFSET) * 100) / -50 * 12) / 10
```

The shifted velocity then feeds **three** things, which is why it sounds like
more than a level change:
1. the velocity-layer selection (a different recorded sample),
2. `%VolumeTabelle[vel]` (per-velocity trim),
3. a per-note **LP cutoff**: `set_engine_par($ENGINE_PAR_CUTOFF, %FilterTabelle[vel], $DryToneGroup, $LPSlot, 0)` (`:879`).

Final note gain (`:927`) sums `%VolumeOffset[note] + %VolumeTabelle[vel] +
$KeyRangeVolume + $ColourVolumeBoost + $DynamicRange + %VolumeZone[zone]`.

**SPACE — a convolution reverb with a preset IR menu.** `load_ir_sample(!SpacePaths[$Spa_mnuType] & ".ncw", $IRSlot, 0)` (`:684`, `:727`).
Controls: on/off, Send amount, Pre-delay, Size, IR select (`:177-181`,
automation names at `:508-510`). Nothing exotic — a convolver plus a size/
predelay stage.

**ANATOMY**
- **RESONANCES** — pure bus volume, gated by pedal-down:
  `if ($Mas_sliAnaReso > 0 and $PedalDown = 1)` (`:964`) and
  `set_engine_par($ENGINE_PAR_VOLUME, $Mas_sliAnaReso, -1, -1, $NI_BUS_OFFSET)` (`:1428`).
  i.e. mix level of the sympathetic-resonance sample group. No DSP.
- **DYNAMIC RANGE** — velocity-driven gain compress/expand that keeps every
  velocity sample (`:843-854`):
  `dyn <= 0` → `(vel - 127) * helper`; `dyn > 0` → `(127 - vel) * helper * -1`,
  where `helper = $Mas_sliAnaDyn + $DYN_OFFSET (+ $KK_DYN_OFFSET when $Ana_mnuVelo = 4)`.
- Plus per-group enables: Release, Hammer, Damper, Pedal, String noise,
  Overtones/SSR, Repedal, Half-pedal, Silent key (`:204-218`).

**Shipped defaults** are all in `persistent_0.tsv` (52 vars — `$Mas_sliToneColor 0`,
`$Mas_sliAnaReso 630000`, `$Ana_mnuVelo 4`, the Tone EQ/comp state, etc.). Use
that file as the authoritative "what the patch sounds like out of the box".

### 1.2 Tasks

- [ ] **K1 — Extract the Space IRs.** They live inside the `.nkx` monoliths and
      are addressed by `load_ir_sample` path, *not* by a zone, so the zone-driven
      extractor missed them (Grandeur: 2563 WAVs extracted vs 3358 zones, and no
      IR files anywhere on disk). Teach the extractor to pull NCWs referenced by
      `load_ir_sample`, and recover `!SpaceNames` / `!SpacePaths` (they are not
      in the four `.ksp` dumps — they come from the instrument's string tables,
      so the NKI decoder needs to emit those too).
- [x] **K2 — Sample-group classifier.** DONE — `features/rigs/keys/specs/ni-pianos.styx`
      + `signal-sampler --example build_ni_packs`. `zones.tsv` has no
      articulation column, so the group comes from keywords in the sample
      filename. The *prefixes* are library-specific (`GI_PP_SD` / `GI_MAV` /
      `GI_GMF` / `GI_K370`) but the *keywords* are not, so one rule set covers
      all five instruments. Census, all zones classified, none unclaimed:

      | instrument | zones | DryTones | Release | Resonance | SSR | Hammer | noises |
      |---|---|---|---|---|---|---|---|
      | The Grandeur | 3358 | 1584 | 639 | 968 | 64 | 88 | 15 |
      | The Maverick | 3282 | 1587 | 594 | 935 | 66 | 88 | 12 |
      | The Gentleman | 3148 | 1411 | 603 | 968 | 66 | 88 (`_BS_`) | 12 |
      | The Giant | 2003 | 1144 | 350 | 352 | 62 | 88 | 7 |
      | The Giant Cinematic | 610 | 169 | 280 | 99 | 62 | — | — |

      Every zone in all five resolves to a decoded WAV (3358/3358, 3282/3282,
      3148/3148, 2003/2003, 610/610) — the extraction is complete.
- [ ] **K3 — Build the packs.** *(pipeline validated end-to-end; the four
      main pianos are building now.)* Two packs per instrument, Full + Proxy:
      **Piano** (DryTones + Release + Hammer + Damper + Pedal + Stringnoise —
      everything needed to sound a note) and **Resonance** (Resonance + SSR —
      the pedal-down layer, ~30% of the zones and inaudible until a foot goes
      down, so it loads on demand). Output:
      `Signal/Libraries/{Full,Proxy}/Keys/<Library>/<Library> - <Pack>.signalpack`,
      matching the CS two-tree convention rather than the older
      `Libraries/Keys/<Lib>/Packs/` layout.
- [ ] **K3b — The Giant Cinematic pack.** Blocked on K7 — the FX instrument is
      the only one that touches the malformed WAVs.
- [ ] **K7 — Fix 14 malformed WAVs in The Giant's extraction.** Each has a
      `data` chunk 3 bytes past a frame boundary (block align 6), so the decoder
      rejects the whole file: `data 24579 % 6 = 3`. 14 of 9062 across all four
      libraries, **all in The Giant**, and used **only** by `The Giant - Cinematic`
      — the four main pianos are clean. Most are near-empty stubs (`data` = 3,
      15, 27, 39, 57 bytes); two carry real audio (`GI_pianoscratch reso_05.wav`,
      `K370_Plucked_A#4.wav`). Fix in the NCW→WAV writer (truncate the trailing
      partial frame), not by patching the files, or it comes back on re-extract.
- [ ] **K4 — `PianoVoice` control layer.** Implement Color / Dynamic Range /
      Resonances over the sampler as described above. Color and Dynamic Range are
      *velocity-domain* transforms that must run before zone selection, so they
      belong in the note-on path, not as a post-FX block. Resonances is a group
      mix level. Ship `persistent_0.tsv` defaults as the pack's shipped state.
- [ ] **K5 — Space.** Point the existing convolver at the K1 IRs; expose
      on/off + send + predelay + size + IR select.
- [ ] **K6 — Verify against Kontakt.** Same A/B harness discipline as
      `css-ab-harness` / `css-reference-matching`: render the same MIDI through
      real Kontakt and through Signal, compare level ratio and spectral shape at
      Color = -50 / 0 / +50 and Dyn = -200 / 0 / +200.

---

## 2. Keyscape — 10 packs are incomplete, not 1

Packs live in `/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/`
(all built 2026-07-13; sources extracted 2026-05-05).

### 2.0 The four this rig needs

| patch | source | pack | ratio | verdict |
|---|---|---|---|---|
| **Vintage Vibe Electric Piano** | 2120 MB, 6055 files | 117 MB | **0.06** | ❌ **broken** — see 2.1 |
| **Rhodes - LA Custom** | 2535 MB | 2520 MB | 0.99 | complete |
| **Double Felt Grand** | 2628 MB | 2624 MB | 1.00 | complete |
| **Wing Upright** | 10042 MB | 10028 MB | 1.00 | complete |

### 2.1 Root cause — the filename convention parser

Keyscape packs are built in **convention mode**: articulation, note and
velocity are parsed out of the filename. Where a patch's articulation token
contains spaces, or the fields are space-separated rather than `_`/`-`
separated, the parse fails and those samples are dropped **silently**.

Vintage Vibe EP is the clean demonstration. Its two filename shapes:

```
RR01_SL01 VVRFstr04_100-100.flac    release — <artic>_<note>-<vel>   → parses
RR01_SL01 VVEP r06_100 v10.flac     body    — space in "VVEP r06",
                                              " v10" velocity form   → dropped
```

The pack holds **4516 entries, every one a release sample, and zero body
samples** (`grep -c VVEP` over the pack = 0). It is a Vintage Vibe with no
Vintage Vibe in it — note-off tails only. That is why 2.1 G became 117 MB, and
why "it sounds thin" would never have been the report; it makes almost no
sound at all.

### 2.2 The sweep — this hits 10 of 43 packs

Pack-bytes ÷ source-bytes across every Keyscape pack. A healthy pack sits at
0.94–1.00; anything below that lost samples.

| patch | source | pack | ratio | note |
|---|---|---|---|---|
| MKS-20 Piano 1 | 276 MB | 3 MB | **0.01** | 1320 files → 15 entries |
| MKS-20 Piano 2 | 315 MB | 4 MB | **0.01** | |
| MKS-20 E Piano 1 | 477 MB | 6 MB | **0.01** | `MKS20 EP 1 r2_…` — spaces in the token |
| MKS-20 E Piano 2 | 563 MB | 6 MB | **0.01** | |
| Vintage Vibe EP | 2120 MB | 117 MB | **0.06** | body missing entirely |
| Simone Celeste | 2286 MB | 770 MB | 0.34 | 4558 → 1520 files, exactly ⅓ |
| Dulcitone | 1336 MB | 661 MB | 0.49 | |
| Rhodes Bass | 923 MB | 457 MB | 0.50 | 3584 → 1792, exactly ½; `rbd rel 109 28` |
| Rhodes - Pre-Piano | 1830 MB | 922 MB | 0.50 | |
| Electric Harpsichord | 850 MB | 493 MB | 0.58 | |

The exact ½ and ⅓ fractions say whole articulations are dropping out, not
scattered files. `Rhodes - Classic` has 1615 space-form filenames and still
rates 0.99, so space-in-filename is a *trigger*, not the whole rule — the
parser needs reading, not guessing at.

Nothing else in the tree is affected: `LA Custom C7 Grand`, `Wing Upright`,
`Wing Tack`, `Wurlitzer 140B`, `Vintage Vibe Vibanet` and the rest all sit at
0.99–1.00.

### 2.3 Tasks

- [x] **KS2 — Diagnose Vintage Vibe EP.** Done, above — and it turned up nine
      more.
- [ ] **KS4 — Fix the convention parser** so a space-bearing articulation token
      and a ` v<NN>` velocity field parse. This is the actual bug; the rebuilds
      depend on it.
- [ ] **KS5 — Make the failure loud.** The Keyscape builder should refuse, or at
      minimum warn hard, when the samples it packed are materially fewer than
      the audio files it was pointed at. Ten packs shipped broken because
      nothing compared those two numbers. `build_ni_packs` now does this for
      undecodable samples (§1.2 K2/K3); the same guard belongs here.
- [ ] **KS1 — Confirm whether the 2026-05-05 extraction predates the NCW
      mid/side decode fix.** If so, re-extract before rebuilding. See the
      `ncw-midside-decode-bug` note.
- [ ] **KS3 — Rebuild.** The four in §2.0 first (that is what the rig needs),
      then the other six. Reminder from `keyscape-pack-loading`: the rig loads
      `.signalpack` only, so a re-extract without a rebuild never reaches it.

---

## 3. Omnisphere — the seven patches

The reader is in good shape: `features/rigs/synth/src/omni_import/` parses
`.prt_omn` (AmberPart XML), `.mlt_omn` multis, and `DAW3` VST3 state chunks
(`omni_import::state`), with `examples/omni_state.rs` as the CLI and
`examples/build_omni_pack.rs` / `build_omni_packs_all.rs` for pack building.
Soundsources are extracted to `/run/media/AudioHaven/Sampled/Keys/Omnisphere/`
(Core Soundsources · Moog Tribute Library · User) and the pack tree exists at
`Signal/Libraries/Keys/Omnisphere/Packs/` — but **contains no `.signalpack`
files yet**. That is the gap.

### 3.0 Reading the gig — solved

`signal_synth::gig` (`features/rigs/synth/src/gig.rs`) recovers every hosted
plugin's state from a `.gig`. It read all 138 processors in Worship Gig 3.

The state is not encrypted, only stacked four deep, and the trap is the
outer/inner base64: JUCE's `MemoryBlock::toBase64Encoding` uses **its own
64-character table starting with `.`**, packs six bits at a time **LSB-first**,
and carries the decoded length as a decimal prefix (`"380876.+mrl6…"`). Hand
that to an RFC 4648 decoder and you get high-entropy noise that reads as
encryption — which is exactly the wrong conclusion.

```text
<PROCESSORSTATEZ>  "<size>.<chars>"   JUCE MemoryBlock base64
  └─ zlib (78 da)                     the "Z" in STATEZ
      └─ "VC2!" <VST3PluginState><IComponent>…   JUCE VST3 wrapper
          └─ "<size>.<chars>"         JUCE base64 again
              └─ the plugin's own chunk
```

For Omnisphere the innermost chunk is the familiar `DAW3` body **minus its
first 8 bytes** — `IComponent::getState` starts at the `999999999` magic — so
`omni_import::state::parse_state` now accepts both spellings and the existing
`.mlt_omn` reader consumes the result unchanged. Verified: all five Omnisphere
instances parse to 8 parts each (`gig_extract`'s `#[ignore]`d
`recovered_multi_parses`, run with `GIG_FILE=…`).

The Kontakt chunk (`hsin`, 967 KB) and the Arturia one
(`22 serialization`, 75 KB) are recovered but not parsed.

### 3.1 The actual patch inventory

Straight out of `gig_extract omni` — note several names differ from memory:
it is **Club *Europa*** (not Europe), **Worship PHAT *Bass***, Dolceola is from
**Keyscape Creative**, and there is a seventh patch nobody mentioned.

| # | patch (exact) | library | GP instance / part | target Signal lane |
|---|---|---|---|---|
| O1 | `KEY │ American Obesity` | Live Keyboardist | Omni Pads · 1 | Pad Engine → `Pad` |
| O2 | `AD │ Gentle Gothics` | Ambient Dreams | Omni Pads · 2 | Pad Engine → `Shimmer` |
| O3 | `Worship PHAT Bass` | **User** | Omni Synths · 1 (all three rackspaces) | Aux Engine |
| O4 | `Hammered Dolceola` | Keyscape Creative | Omni Synths · 2 (Dulcimer Dance) | Aux — `Dulcimer` |
| O5 | `CLUB │ Club Europa Plucking Pulsars` | Club Land | Omni Synths · 5 (all three) | Aux — `Trance` |
| O6 | `AV │ Dissolved Pluck` | Analog Vibes | Omni Synths · 3 (Massive Worship) | Aux — `Synth Pluck` |
| O7 | `MK-80 Rhodes` | Keyscape Library | Omni Synths · 3 (Gospel) | Keys Engine (new) |

`Omni Keys` turns out to be pure Keyscape — parts 1–4 are
`Vintage Vibe Electric Piano`, `Rhodes - LA Custom`, `Double Felt Grand`,
`Wing Upright`. That is **exactly** the §2 rebuild list, which confirms the
lane mapping in §0 and means §2 and the `Omni Keys` lane are one job.

`Worship PHAT Bass` is a **User** patch — it exists only in this gig and in the
Spectrasonics user library on the Mac. Back it up before touching anything.

`worship_profile()` (`features/rigs/keys/src/profile.rs:410`) already anticipates
O1: the Pad layer is authored as module A `OB-8 PWM Big Strings` + module B
`Juno 60 Raw Sub` — the two soundsources American Obesity stacks. `Shimmer`,
`Aux A/B/C`, `Organ A/B`, `Drone`, `SFX A/B` are all empty lanes waiting.

- [ ] **O-0 — Build the Omnisphere soundsource packs** the seven patches need
      (`build_omni_pack`). Per `omnisphere-soundsource-packs`, loops live in the
      FLAC `STINFO` tag and `build_omni_pack` bakes them in — do not hand-roll.
- [ ] **O-1..O-7 — one patch at a time**: locate the `.prt_omn` (user patches in
      the Spectrasonics user library, stock ones in the factory library) →
      `patch_to_container` → drop into the right `LayerDef` →
      A/B against the real plugin. Each is its own ticket; O4/O5 (stock) are the
      easiest first exercise of the reader, O1/O2 are the highest-value.
- [ ] **O-7 — Wire the Aux engine's lane names** to the GP ones (`Dulcimer`,
      `Trance`, `Synth Pluck`, `Amb Key`) so the mental model transfers.

---

## 4. Arturia Augmented GRAND PIANO — PARKED

**Not being worked (2026-08-19).** Findings kept because they were paid for; the
lane is out of scope until the rest of the rig plays. The one live takeaway is
that Arturia's IRs, wavetables and one-shots are plain WAV and free to use
elsewhere in the tree.

The gig's Arturia chunk turned out to be the *most* readable of the three:
Arturia's `serialization::archive` is length-prefixed **plain text**, so the
preset reads straight out of `gig_extract dump`:

> **`Grand Energy`** — User bank, author JT Wright, derived from the factory
> preset `Go for It` (Type Piano / Subtype Acoustic Grand). Its own blurb:
> *"Heavily compressed pop piano patch with a bit of chorus on top… then
> there's the organ-like layer that can be easily added with the Morph Macro."*
> All ~1491 parameters follow as named `key value` pairs.

The samples are on voyager, as remembered — `/Library/Arturia/Samples/Augmented GRAND PIANO`
(2.6 G), split into two very different halves:

| what | format | verdict |
|---|---|---|
| **Mapping** — 51 `.sfz` + 13 `.sfzi` under `Factory/SFZ/Real Piano/` | **plain-text SFZ** | fully open. Per-region `sample=`, `pitch_keycenter`, `lokey`/`hikey`, `tune`, velocity groups, per-region envelopes, the release/resonance layers and their CC controls (`cc1300` Release Volume, `cc1301` Release CrossFade, `cc1302` Release Decay). 21 articulations: Felt, Paper, Pure, Bowed, Damped, Deep Resonance, Finger Pluck, Hammer Noises, Ping Pong, Soft Mallet, Stick Attack, Twine, Wood Pick, … |
| **Audio** — 2983 `.arta` | **`PLC2`, proprietary** | 16-byte header (`PLC2` + `0101` + zeros — no sample rate, channel count or frame count in the clear), then incompressible bytes (gzip *expands* them). No `PLC2` symbols exported from `libaugmentedgrandpianoProcessor.dylib`. Reverse-engineering this is an NCW-sized project, not an afternoon. |
| IRs (29), Wavetables (203), one-shot Samples (143) | **plain WAV** | usable today if we ever want them |

**Recommendation: don't attack PLC2 — resample the plugin.** Not because the
extraction is hard, but because it would give us the wrong thing. What the rig
needs is `Grand Energy`, and that patch's sound *is* the Augmented engine — two
layers, the Morph macro, chorus, delay, reverb, heavy compression. The `.arta`
files are the dry Felt/Paper/Pure bodies underneath all of that. Extract them
and we would still have to rebuild the engine on top; resample the plugin and
we capture the patch as it is actually heard, which is the only version that
has ever been on stage.

This is also nearly free. `signal-plugin-host`'s `load_plugin` example already
takes `--note`, `--render`, `--secs` and `--load-state`, and §3.0 hands us the
exact `Grand Energy` state chunk out of the gig — so the autosampler is a loop
over notes and velocities around machinery that exists.

- [ ] ~~**A1 — Render `Grand Energy` to a pack.**~~ *(parked)* Load the extracted chunk via
      `--load-state`, sweep note × velocity, capture, build a `.signalpack`.
      **Must run on voyager**: Arturia ships macOS/Windows only, so this needs
      `signal-plugin-host` building on macOS — verify that before scoping the
      rest.
- [ ] ~~**A2 — Decide the sampling grid**~~ *(parked)* (every semitone or every minor third,
      how many velocity layers, tail length). A resampled patch is only as good
      as its grid, and this one has a long compressed tail.
- [ ] **A3 — Teach the sampler to read SFZ.** *(still live — pays off beyond Arturia)* The Arturia mapping is
      a complete, well-formed SFZ description of a 21-articulation piano. Even
      with the audio locked, that is a good reference — and an SFZ importer
      would pay off well beyond Arturia.

---

## 5. Profile wiring

- [ ] **P1** — Extend `worship_profile()` so every lane in §0's table has a
      patch: Keys A/B (already `LA Custom C7 Grand` / `Rhodes - LA Custom`),
      plus lanes for The Grandeur / Gentleman / Maverick / Giant / Felt / Wing,
      EP 1 / EP 2, Pad / Shimmer, and the Aux set.
- [ ] **P2** — Author the stacks (`Spotlight` / `Verse` / `Energy` / `Hooks` /
      `Underscore`) over the full lane set; today several stacks reference lanes
      that hold nothing.
- [ ] **P3** — Recreate the bus topology (Keys / Pads / Shimmer / Delay / Reverb
      sends) as container sends rather than a flat plugin graph.
- [ ] **P4** — Arturia lane is parked (§4). The rig ships without it.

---

## 6. The budget — why this is worth doing

Per-lane RAM is bounded by `engine::budget` (15% of RAM, ≤4 GB — see
`sampler-preload-ram`), and the four NI pianos plus Keyscape's big grands are
43 G + 26 G of source. So the buildout has to answer, per lane:

- [ ] **B1** — Full vs Proxy per lane, per stack. `Spotlight` can afford the Full
      C7; `Underscore` should never page in a grand.
- [ ] **B2** — Preload window vs streaming per group. The resonance and release
      groups are the bulk of the zone count and the least latency-critical.
- [ ] **B3** — Measure. `pack_memory` / `bench_drum_load` / RssAnon, and an xrun
      watch on the live rig. Do not tune this by eye.

---

## Open questions

1. Are the Space IRs recoverable from the `.nkx` at all, or do they live in the
   `.nkr`/`.nkc` resource containers? (K1 blocks K5.)
2. Do the other three pianos' scripts use the same `$COLOR_OFFSET` /
   `$DYN_OFFSET` constants, or per-library ones? Diff `script_0.ksp` across all
   four before generalising `PianoVoice`.
3. ~~Is deobfuscating Gig Performer's `PROCESSORSTATEZ` worth it?~~
   **Answered** — it was never obfuscated, just unusually encoded (§3.0). All
   138 processors decode. The Omnisphere states feed the existing reader
   directly; the Kontakt (`hsin`) and Arturia (`22 serialization`) chunks are
   recovered but not yet parsed, which is a separate piece of work and only
   worth doing if the KSP route (§1) leaves something unanswered.
