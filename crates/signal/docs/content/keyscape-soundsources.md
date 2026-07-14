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

Build a pack from a raw extraction dir with the in-tree example (the historical
`signal-cli sampler pack` wrapper is not in this checkout):

```
cargo run -p signal-sampler --release --example build_pack -- \
    "<samples_root>" "<out.signalpack>"
```

It embeds `<samples_root>/library.styx` verbatim and FLAC-i24-encodes every audio
file directly under the dir. Validate with `--example check_pack_resolve`.

> **Runtime note:** the keys rig now loads the **`.signalpack`** library
> (`PlayerPatch::from_pack`) from
> `…/Signal/Libraries/Keys/Keyscape/Packs/<Instrument>.signalpack`, falling back
> to the raw extraction (`PlayerPatch::load` → `SampleMap::scan`) when no pack is
> present. The loader in `rig.rs` picks `from_pack` vs `load` by file extension;
> discovery lives in `keys/backend.rs::scan_keyscape` (env `FTS_KEYSCAPE_PACKS` /
> `FTS_KEYSCAPE_ROOT`) and `nord.rs::keyscape_spec`. A pack is self-contained
> (embedded styx + samples), so it reconciles articulation dynamics/RR from its
> own sample index at load. When editing raw dirs, `SampleMap::scan` **recurses
> into subdirectories**, so never leave a backup/`_old` folder inside a patch dir.
>
> **After re-extracting a patch, rebuild its pack** — a stale pack silently
> serves the old samples. (Rhodes LA Custom + Wing Tack were rebuilt when their
> extraction changed; the old packs are archived under
> `scratch-keyscape/stale-packs/`.)

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

### Fix — collision-proof soundsource-aware extraction

`steam::keyscape_ss_tags` makes every soundsource a **distinct articulation** the
engine keeps separate, by a *collision-proof* rule (not a hand-maintained keyword
list):

> **Collision** = two soundsources emit the **same output file stem** — the exact
> condition that makes the extractor's de-duper append `_2` and overwrite/merge
> them on disk.

Detecting by stem (not by leading-alpha article) is what keeps a body and its own
release **together**: `chime` and `chimerel` share the alpha run but never produce
the same file, so they are never separated. It still catches the two real merge
cases:

- **mechanical noise vs body** — both emit `RR01 lacrm 60 84.wav` → collide.
- **mic / type variants** — Wing Tack's `^ Mono` and `^ Stereo` both emit
  `RR01_SL01 wup_100-111.wav` → collide.

Soundsources are grouped into connected collision components; within each, the one
with the **most samples** keeps the article (so `library.styx` stays valid) and
every other gets a sanitized soundsource-derived tag **inserted after the
article's leading-alpha run**: `lacrm` → `lacrm`**`mech`**,
`wup_100-111` → `wup`**`mono`**`_100-111`. Pedal-noise soundsources are excluded
(already distinct; parsed specially as `lacrped` / `lacrmechped`).

Verified lossless on **Rhodes – LA Custom** (6861 files): the merged `lacrm`
(2985) splits into sustain `lacrm` (1577) + `lacrmmech` (1408), and `lacr` (3781)
splits into release `lacr` (3077) + `lacrmechrel` (704); `lacrmsp` (95, a genuine
low-note articulation) is untouched. The body then resolves to real sustains at
every note/velocity (0 short samples).

### Default-articulation / release-`kind` fix

The engine's default-articulation picker (`engine/mod.rs`) plays the first
articulation that is **not** `@Release`/`@Legato` and not a mech/pedal aux layer.
So a patch is only playable if its **body** wins that search — which needs its
**release combo marked `@Release`** in the styx. The external styx generator
mis-classified some releases as `@OneShot` when their names didn't match its
heuristic (`fr`/`mr`/`sr` full/mech/soft-release tokens, not `rel`). The picker
then grabbed a release/attack layer alphabetically ahead of the body — e.g.
Hohner Clavinet C defaulted to `clvcfr` (key-off noise) instead of the body
`clvcr12`: **"just the attack noise, no sustain."**

Fix: `docs/scripts/fix_release_kinds.py` reads each `.db` manifest, finds the
article-ids produced **exclusively** by `Release`-named soundsources (never by a
body — guards against flipping a body), and rewrites those articulations
`kind @OneShot` → `@Release` in the raw styx. Rebuild the pack afterward
(`build_pack`) so the embedded spec carries the fix. Marking them `@Release` is
doubly correct: the body becomes the default **and** the releases play on
note-off as intended.

Nine patches were mis-marked and corrected — the picker now defaults to the body
for all: Hohner Clavinet C (`clvcr12`), MKS-20 Electric Grand (`mks20egrandr2`,
was `cp70frl`), Hohner Pianet T (`pntsus`), Dolceola (`dlcsus`), Double Felt
Grand, Hohner Pianet N, Vintage Vibe EP/Tine Bass, Wing Upright. (Vinyl Keyscape
01 legitimately defaults to `recordnoise` — it *is* a vinyl-ambience instrument.)
Verify any pack's default with `--example check_pack_resolve`.

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

### Library-wide collision audit (all 44 patches)

Re-checked every `.db` manifest with the stem-collision rule. **Only two patches
had a true on-disk collision** the old flat extraction corrupted:

- **Rhodes – LA Custom** — `lacrm` body vs `lacrm` mechanical-noise (fixed above).
- **Wing Tack Piano** — `^ Mono` / `^ Stereo` emit identical `wup_100-111` stems →
  the smaller variants are now tagged (`wupwingtackpianomono` /
  `…pianostereo`); no more `_2`/`_3`/`_4` dedup junk. (2816 files, 0 collisions.)

Everything else was already correct:

- **Distinct raw articles** — the other mechanical patches never merged because
  Spectrasonics already named them apart: Rhodes Classic `clrmchr`/`clrmchrel`,
  Hohner Pianet T `pianetmechatk`/`pntmechrel`.
- **Body + release** — pairs that share a leading-alpha prefix (`tgp`/`tgprel`,
  `chime`/`chimerel`) never emit the same stem, so they stay together and blend
  as intended.
- **Token-distinguished multi-body** — Chimeatron Vibraharp Fast vs Slow carry a
  `tr` token (`cat vibe …` vs `cat vibe tr …`); Wing Upright's `wup`/`wuptrm`/
  `wupr` variants likewise differ in the raw name. They don't collide on disk;
  whether the **runtime parser** should treat these tokens (`tr` = tremolo) as a
  separate articulation vs a round-robin is a parser question, not an extraction
  one — tracked below.
- **Vinyl Keyscape 01** — 11 `Record Noise NN` ambience soundsources (article
  `noise`); still needs a runtime ambience-blend path (below).

## TODO / open work

1. ✅ **Generalize `keyscape_retag`** to a collision-proof stem rule — done
   (`steam::keyscape_ss_tags`). Library-wide audit confirms it corrects exactly
   the two collided patches (Rhodes LA Custom, Wing Tack) and leaves the other 42
   byte-identical.
2. **Re-extract only what changed** — the collision fix only alters Rhodes LA
   Custom (already deployed) and Wing Tack Piano; other patches extract identically
   under the new collector, so a full 44-patch re-extraction isn't required for
   correctness (only for populating the live library where a patch isn't extracted
   yet).
3. **Multi-body parser tokens** — decide whether the runtime should treat
   variant tokens (`tr`/tremolo, mono/stereo mic, fast/slow) as a distinct
   articulation or a round-robin. Chimeatron / Wing Upright depend on this.
4. **Record-noise & arbitrary ambience layers** — give them their own
   articulation + a runtime ambience-blend path (Vinyl Keyscape's 11 `noise`).
5. **Duo Maps combos** — resolve soundsource references (the 0-audio patches).
6. **Runtime blending** — the mechanical-noise / pedal / record-noise layers are
   currently *separated but inert*. Blend them under the body as velocity-scaled
   ambience (like the release layers), matching Keyscape's mix
   (release −10 dB, mechanical −20 dB, pedal −20 dB).
7. **`library.styx` regeneration** — declare the new per-soundsource
   articulations + their layer roles so the runtime knows body vs ambience.
8. **Re-pack** each patch after re-extraction so `.signalpack`s match the raw
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

