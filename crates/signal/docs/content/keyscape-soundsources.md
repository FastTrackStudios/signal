# Keyscape Sample Packing & Soundsources

How Keyscape libraries are extracted, packed, and layered — and why the naïve
flat extraction produced "choked" notes. Companion to
[`signalpack-keyscape.md`](signalpack-keyscape.md) (on-disk pack format) and the
`sample-collector` STEAM importer.

## Why `.signalpack`

A `.signalpack` is one file per instrument:

- **Efficient load** — the pack is `mmap`ed; samples decode from the pack body
  on demand, and preload streams incrementally (Kontakt-style) instead of
  `open()`-ing thousands of loose files.
- **Efficient transfer** — one 2.5 GB file moves far faster than 6,800 tiny
  FLACs, and relative sample-index rows mean a pack is relocatable.
- **Layout** — 64-byte header (`SIGPACK\0`, version), an embedded `library.styx`
  spec, a sample index, then one FLAC block per sample (source FLAC bytes
  embedded directly after metadata validation; non-FLAC decoded to PCM).

Built with `signal-cli sampler pack --samples-root <dir> --output <pack>`.

> **Runtime note:** the keys rig currently loads the **raw extracted directory**
> (`PlayerPatch::load` → `SampleMap::scan`), *not* the pack — confirmed by the
> engine log (`keys rig: scanned library … preloaded N samples`). `SampleMap::scan`
> **recurses into subdirectories**, so never leave a backup/`_old` folder inside a
> patch dir — it re-pollutes the map with stale samples.

## The layering problem

A Keyscape patch is **not one sample set** — it is several **soundsources** that
play *together* (blend), plus per-soundsource velocity/round-robin/mic layers.
In the STEAM `.db` manifest each soundsource is a top-level `<DIR>`:

```
<soundsource>/AudioFiles/RR01 lacrm 60 84.wav      ← audio, named by Spectrasonics
<soundsource>/Pitch N-M/…/HitBundle.xml            ← velocity/RR layer map
```

The audio file **names collide across soundsources** — e.g. `RR01 lacrm 60 84.wav`
exists in both the sustain body soundsource *and* the `LACR Mechanical Noise`
soundsource. The original `extract_db` wrote every entry **flat by name**, so:

1. Soundsources **merged** into one articulation (`lacrm`).
2. Same-name files **collided**; the de-duper appended `_2` (the spurious
   `126_2` "layer" we saw was a collision artifact, not a real layer).
3. The short, quiet **mechanical-noise** samples ended up selected as the note
   **body** → notes played a choked mechanical click instead of the sustain.

### Fix — soundsource-aware extraction

`steam::keyscape_retag` appends a soundsource-derived tag to the articulation
token (2nd filename field) so soundsources become **distinct articulations** the
engine keeps separate:

| Soundsource keyword | Role | Article retag |
|---|---|---|
| `… ^` (no keyword) | sustain **body** | *(unchanged)* |
| `… Release …` | release | *(unchanged — already `lacr … rel`)* |
| `Mechanical Noise` | mechanical attack noise | `lacrm` → `lacrm`**`mech`** |
| `Mechanical Noise … Release` | mechanical release | `lacr` → `lacr`**`mechrel`** |
| `Pedal Noise` / `Mechanical Pedal Noise` | pedal noise | *(kept — already `lacrped`/`lacrmechped`)* |

Verified on **Rhodes – LA Custom**: the merged `lacrm` (2985) splits cleanly into
sustain `lacrm` (1577) + `lacrmmech` (1408); the body then resolves to real
sustains at every note/velocity (0 short samples).

### Within-soundsource layers

Inside one soundsource, samples still layer by **round-robin**, **velocity**, and
**blend variant**. The release combo ships three variants that blend *together*
(not round-robins): `rel` (damped tail), `relm` (mechanical click), `relsl`
(string/let-ring), plus `rel_2` at the top velocity. The engine parses the
variant into `SampleKey.direction` and blends every present layer at one shared
RR index (`spawn_layers`). A body `_2` at the hardest velocities is a real second
hit-layer (kept as `direction="2"`).

## Soundsource roles across the library (audit)

Every patch was scanned from its `.db` manifest and each soundsource classified.
Roles seen:

- **body** — the main sustain (42 of 44 patches have one).
- **release** — key-up/damper release blend layers.
- **mech-noise / mech-release** — mechanical noise + its release (Rhodes LA
  Custom, Rhodes Classic, Hohner Pianet T).
- **pedal-felt / pedal-mech** — sustain-pedal noises (LA Custom, Double Felt
  Grand, Wing Upright, Yamaha CP-70B).
- **noise-other** — Vinyl Keyscape's 11 `Record Noise NN` ambience layers.
- **mic / type variants** — `Mono`/`Stereo`, `Fast`/`Slow`, `Tack`/`Tremolo`,
  `Bass`/`Guitar`/`Tutti`, `Celeste`/`Celeste Mute`.
- **combo** — `Duo Maps Stage`/`Studio` ship **0 audio**; they reference other
  soundsources.

### Collision cases the keyword retag does NOT yet cover

- **Wing Tack Piano** — `^ Mono`, `^ Stereo`, `Tremolo ^ Mono`, `Tremolo ^ Stereo`
  all use the `wup` article → all four collide.
- **Chimeatron** — Vibraharp `Fast` and `Slow` both use `vibe` → collide (Chimes
  `chime` is distinct).
- **Vinyl Keyscape 01** — 11 `Record Noise NN` soundsources.

`Weltmeister Claviset` (`Bs`/`Gtr`/`Tut`) and `Simone Celeste` (`Cel`/`CelMt`)
happen to use distinct article tokens, so they don't collide — but that's luck,
not by design.

## TODO / open work

1. **Generalize `keyscape_retag`** from keyword-matching to a *collision-proof*
   rule: within a patch, if two soundsources would emit the same article token,
   tag each with a sanitized soundsource id (mic/variant/multi-body). Guarantees
   no soundsource ever merges.
2. **Record-noise & arbitrary ambience layers** — give them their own
   articulation + a runtime ambience-blend path.
3. **Duo Maps combos** — resolve soundsource references (the 0-audio patches).
4. **Runtime blending** — the mechanical-noise / pedal / record-noise layers are
   currently *separated but inert*. Blend them under the body as velocity-scaled
   ambience (like the release layers), matching Keyscape's mix
   (release −10 dB, mechanical −20 dB, pedal −20 dB).
5. **`library.styx` regeneration** — declare the new per-soundsource
   articulations + their layer roles so the runtime knows body vs ambience.
6. **Re-pack** each patch after re-extraction so `.signalpack`s match the raw
   library.

## Full per-patch audit

_Generated by `audit_soundsources.py` over the STEAM `.db` manifests. Columns:
soundsource count and role breakdown._

_Covering 44/44 patches._

| Patch | Soundsources | Roles |
|---|---:|---|
| Chimeatron | 6 | body×3, release×3 |
| Classic Toy Piano | 2 | body×1, release×1 |
| Clavichord | 2 | body×1, release×1 |
| Dolceola | 2 | body×1, release×1 |
| Double Felt Grand | 4 | body×1, pedal-felt×1, release×2 |
| Dulcitone | 2 | body×1, release×1 |
| Duo Maps Stage | 0 | (none — combo/reference) |
| Duo Maps Studio | 0 | (none — combo/reference) |
| Electric Harpsichord | 4 | body×2, release×2 |
| Glock Toy Piano | 2 | body×1, release×1 |
| Grand Toy Piano | 2 | body×1, release×1 |
| Harmochord | 2 | body×1, release×1 |
| Hohner Clavinet C | 2 | body×1, release×1 |
| Hohner Pianet N | 2 | body×1, release×1 |
| Hohner Pianet T | 4 | body×1, mech-noise×1, release×2 |
| JD-800 Crystal Rhodes | 1 | body×1 |
| LA Custom C7 Grand | 3 | body×1, pedal-felt×1, release×1 |
| MK-80 Celeste | 1 | body×1 |
| MK-80 Contemporary Rhodes | 1 | body×1 |
| MKS-20 E Piano 1 | 1 | body×1 |
| MKS-20 E Piano 2 | 1 | body×1 |
| MKS-20 Electric Grand | 2 | body×1, release×1 |
| MKS-20 Piano 1 | 1 | body×1 |
| MKS-20 Piano 2 | 1 | body×1 |
| MKS-20 Vibes | 1 | body×1 |
| Rhodes - Classic | 4 | body×1, mech-noise×1, mech-release×1, release×1 |
| Rhodes - LA Custom | 6 | body×1, mech-noise×1, mech-release×1, pedal-felt×1, pedal-mech×1, release×1 |
| Rhodes - Pre-Piano | 2 | body×1, release×1 |
| Rhodes Bass | 2 | body×1, release×1 |
| Saucer Bell Toy Piano | 2 | body×1, release×1 |
| Simone Celeste | 3 | body×2, release×1 |
| Student Mini Grand | 2 | body×1, release×1 |
| Vintage Vibe EP | 2 | body×1, release×1 |
| Vintage Vibe Tine Bass | 2 | body×1, release×1 |
| Vintage Vibe Vibanet | 2 | body×1, release×1 |
| Vinyl Keyscape 01 | 11 | noise×11 |
| Weltmeister Bassett I | 2 | body×1, release×1 |
| Weltmeister Bassett II | 1 | body×1 |
| Weltmeister Claviset | 6 | body×3, release×3 |
| Wing Tack Piano | 4 | body×4 |
| Wing Upright Piano | 8 | body×4, pedal-felt×2, release×2 |
| Wurlitzer 140B | 7 | body×1, mech-noise×1, pedal-felt×2, release×3 |
| Wurlitzer 200A | 2 | body×1, release×1 |
| Yamaha CP-70B | 2 | body×1, pedal-felt×1 |

