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
| `Augmented GRAND PIANO` | Arturia | one lane | `EQ: Arturia Grand` |

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

> **Gotcha**: Gig Performer stores plugin state in `PROCESSOR/PROCESSORSTATEZ`
> as base64 of an *obfuscated* blob — not zlib, not raw. So the Omnisphere
> patch names above are **not** greppable out of the .gig. We got the rig map
> from `prop_str_nodeName` (plaintext) only. If we ever want automated import,
> deobfuscating STATEZ is its own task; otherwise read patch names off the
> screen in GP and drive the existing `.prt_omn` reader from the Spectrasonics
> library on disk.

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
- [ ] **K2 — Sample-group classifier per library.** `zones.tsv` has no
      articulation column; the group is encoded in the sample filename and the
      prefix differs per library:
      | library | body | releases | hammer | resonance | misc |
      |---|---|---|---|---|---|
      | Grandeur | `GI_PP_SD_` (2552) | `GI_SD_RELEASE(S)_` (639) | `GI_SD_Hammer_` (88) | `OVERTONE …_SD` | `Pedalnoise`, `Stringnoise`, `DampUp`, `DampOff` |
      | Maverick | `GI_MAV_<note>_` | `GI_MAV_RELEASE_` (594) | `GI_MAV_HAMMER_` (88) | `GI_MAV_RESO_` (935) | mixed `MAV`/`Mav` case — fold case |
      | Gentleman | `GI_GMF_<note>_` | `GI_GMF_Release_` (603) | `GI_PP_BS_` (88) | `GI_GMF_RESO_` (968) | |
      | Giant | `GI_K370_<date>_` | `GI_K370_Release_` (350) | — | `OVERTONE …_K370` | `GI_Tower_Pedalplus` |
      Model it on `features/rigs/orchestra/specs/cs-piano-packs.styx` +
      `signal-sampler/examples/build_cs_packs.rs` (that builder already errors
      on any unclaimed zone — keep that property).
- [ ] **K3 — Build the packs.** One `.signalpack` per piano per group, Full +
      Proxy, into `/run/media/AudioHaven/Signal/Libraries/Keys/<Library>/Packs/`
      (which does not exist yet — today only Keyscape/Omnisphere/Trilian do).
      Sizes to expect: Grandeur 13 G, Maverick 14 G, Gentleman 8.8 G, Giant 7.7 G.
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

## 2. Keyscape — re-extract / rebuild

Packs live in `/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/`
(all built 2026-07-13; sources extracted 2026-05-05).

| patch | source on disk | pack | verdict |
|---|---|---|---|
| **Vintage Vibe Electric Piano** | `Vintage Vibe EP/` 2.1 G, 6056 files | `Vintage Vibe EP.signalpack` **123 MB** | ❌ **broken** — pack is ~6% of the source. Rebuild is mandatory. |
| **Rhodes - LA Custom** | 2.5 G, 6864 files | 2.64 G | size plausible; re-extract to pick up the NCW mid/side fix |
| **Double Felt Grand** | 2.6 G, 1606 files | 2.75 G | size plausible; same |
| **Wing Upright** | `Wing Upright Piano/` 9.9 G, 7057 files | 10.5 G | size plausible; same |

- [ ] **KS1 — Confirm whether the 2026-05-05 Keyscape extraction predates the
      NCW mid/side decode fix.** If it does, re-extract all four (right-channel
      pops otherwise). See the `ncw-midside-decode-bug` note.
- [ ] **KS2 — Diagnose Vintage Vibe EP.** 123 MB from a 2.1 G source is not a
      compression win; the builder dropped zones silently. Find out why before
      rebuilding, and add a size/zone-count sanity assert to the builder so this
      class of failure fails loudly.
- [ ] **KS3 — Rebuild all four packs (Full + Proxy) and re-verify.**
      Reminder from `keyscape-pack-loading`: the keys rig loads `.signalpack`
      only — a re-extract that isn't followed by a pack rebuild never reaches
      the rig.

---

## 3. Omnisphere — the six patches

The reader is in good shape: `features/rigs/synth/src/omni_import/` parses
`.prt_omn` (AmberPart XML), `.mlt_omn` multis, and `DAW3` VST3 state chunks
(`omni_import::state`), with `examples/omni_state.rs` as the CLI and
`examples/build_omni_pack.rs` / `build_omni_packs_all.rs` for pack building.
Soundsources are extracted to `/run/media/AudioHaven/Sampled/Keys/Omnisphere/`
(Core Soundsources · Moog Tribute Library · User) and the pack tree exists at
`Signal/Libraries/Keys/Omnisphere/Packs/` — but **contains no `.signalpack`
files yet**. That is the gap.

| # | patch | GP instance | target Signal lane |
|---|---|---|---|
| O1 | **American Obesity** | Omni Pads | Pad Engine → `Pad` |
| O2 | **Gentle Gothics** | Omni Pads | Pad Engine → `Shimmer` |
| O3 | **Worship PHAT bass** | Omni Synths (Dulcimer Dance) | Aux Engine |
| O4 | **Hammered Dolceola** (stock) | Omni Synths | Aux Engine — "Dulcimer" lane |
| O5 | **Club Europe Plucking Pulsars** (stock) | Omni Synths | Aux Engine — "Trance" lane |
| O6 | **Dissolved Pluck** | Omni Synths (Massive Worship) | Aux Engine |

`worship_profile()` (`features/rigs/keys/src/profile.rs:410`) already anticipates
O1: the Pad layer is authored as module A `OB-8 PWM Big Strings` + module B
`Juno 60 Raw Sub` — the two soundsources American Obesity stacks. `Shimmer`,
`Aux A/B/C`, `Organ A/B`, `Drone`, `SFX A/B` are all empty lanes waiting.

- [ ] **O-0 — Build the Omnisphere soundsource packs** the six patches need
      (`build_omni_pack`). Per `omnisphere-soundsource-packs`, loops live in the
      FLAC `STINFO` tag and `build_omni_pack` bakes them in — do not hand-roll.
- [ ] **O-1..O-6 — one patch at a time**: locate the `.prt_omn` (user patches in
      the Spectrasonics user library, stock ones in the factory library) →
      `patch_to_container` → drop into the right `LayerDef` →
      A/B against the real plugin. Each is its own ticket; O4/O5 (stock) are the
      easiest first exercise of the reader, O1/O2 are the highest-value.
- [ ] **O-7 — Wire the Aux engine's lane names** to the GP ones (`Dulcimer`,
      `Trance`, `Synth Pluck`, `Amb Key`) so the mental model transfers.

---

## 4. Profile wiring

- [ ] **P1** — Extend `worship_profile()` so every lane in §0's table has a
      patch: Keys A/B (already `LA Custom C7 Grand` / `Rhodes - LA Custom`),
      plus lanes for The Grandeur / Gentleman / Maverick / Giant / Felt / Wing,
      EP 1 / EP 2, Pad / Shimmer, and the Aux set.
- [ ] **P2** — Author the stacks (`Spotlight` / `Verse` / `Energy` / `Hooks` /
      `Underscore`) over the full lane set; today several stacks reference lanes
      that hold nothing.
- [ ] **P3** — Recreate the bus topology (Keys / Pads / Shimmer / Delay / Reverb
      sends) as container sends rather than a flat plugin graph.
- [ ] **P4** — Decide what happens to the Arturia Augmented Grand lane. It has no
      sampled source we own; either drop it or find the nearest pack.

---

## 5. The budget — why this is worth doing

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
3. Is deobfuscating Gig Performer's `PROCESSORSTATEZ` worth it? It would let us
   import the whole gig mechanically instead of transcribing it — but only if
   the Omnisphere and Kontakt states inside are then readable by the existing
   readers.
