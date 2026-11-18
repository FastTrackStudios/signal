# Cinematic Studio Brass: what the spec must express to play like Kontakt

What our sampler spec needs to play the eight CSB section patches the way
Kontakt does: every behaviour the KSP adds on top of the zone map, the
values each instrument ships, and whether the current schema
(`features/sampler/src/spec.rs`) can already say it.

Sources (all read-only):

- `…/Cinematic Studio Brass/_ksp/CSB <inst>/script_1.ksp`: one engine for the
  8 sections (md5 `ece28e53…`, 21 708 lines). Line numbers below refer to
  this file unless a script slot is named.
- `script_0.ksp` (`CSB_T`, the trill-pair detector), `script_2.ksp` (`CSB_W`,
  which waits and then fades a body in, from `EVENT_PAR_2`).
- `persistent_1.tsv` (the shipped values), `groups.json`, and `zones.tsv`
  with its `group` column.
- Full Ensemble (`$sxwuz` 6/7 paths, a different script) is out of scope.

Abbreviations: 2Tb = 2 Trombones, 2Tpt = 2 Trumpets, 4Hn = 4 French Horns
(`CSB 4 Horns`), SBTb = Solo Bass Trombone, SHn = Solo French Horn
(`CSB Solo Horn`), STb = Solo Trombone, STpt = Solo Trumpet, Tuba
(`CSB Solo Tuba`).

Shared conventions:

- **Velocity zone** `$xp1ku` (8475-8483): 1 for ≤ `$eluxs`, 2 for ≤
  `$0uhls`, 3 above. Shipped 64 / 100 on all eight instruments.
- KSP volumes are deci-dB (`-30` = −3.0 dB) unless noted. `change_vol`
  takes milli-dB.
- **Decoder caveat.** `persistent_1.tsv` drops an array's trailing zeros:
  `%nuvn3` is shipped as `1 0`, but its declared size is 6 (680), and
  `%5gm34` has 23 of its 24 entries. Pad every array to its declared size
  before indexing it.

---

## 0. Corrections to the current `cinematic-brass.styx`

These come from the KSP. The current file was built from `CSB_CSW_ENGINE.md`
and the manual.

| Current spec | What Kontakt actually does |
|---|---|
| `legato_engine.zones` delay 0 / 60 / 200 ms ("Time A/B/C") | **CSB has no pre-delay.** The transition fires at note-on: the only `wait()`s in the legato path are 1–100 µs polling waits (13964, 14609, 15424…). "Time A–D" / "Vol A–C" are the release-by-held-time tables (`%ciooq`/`%54rxx`, `%x1kwp`/`%5gm34`, `%e1dvv`/`%fxjmy`; UI at 561-641). See §4. |
| `lt_offset_* (0 83 166)` | The sustain LT offset is `$tuu20 = 0` → `$k2afl/$xem4t/$yds0m` (14867-14875, 11121-11127), and it differs per instrument (§5). `$sdxpr/$wnfmn/$dfyxo` (0/83/166) apply only when `$tuu20 = 1`, and CC58 0-10 always sets `$tuu20 := 0` (17940-17955). |
| "F.wait / FadeIn per instrument" (`$4u2u0`, `$m0nme`) | These are editor knobs showing **one row** of `%k1aqh` (`update_legato_fade_in_ve`, 8234). The real body wait/fade is `%k1aqh[layer*20 + zone]`: it depends on the **dynamic layer** and the velocity zone (§5). |
| `short_note_cc1_map` 0-31 / 32-63 / 64-95 / 96-127 | The script never reads CC1 for the short type. The type comes from CC58 11-30 (17958-17979), the KS-note velocity 0-32 / 33-64 / 65-96 / 97-127 (17111-17124), or the `$f4syh` knob, whose 0-5 / 6-64 / 65-121 / 122-127 ranges are at 18196-18213. Whether CC1 is MIDI-learned to `$f4syh` is stored in the NKI, outside the script: **verify with one Kontakt render** before keeping any CC1 map. |
| `cc1_layers_*` estimates | Use the decoded `CC_VOLUME` CC1 tables per group (`group_curves`, §4). |
| `Marcato rr 2` | Marcato RR is a **global** 0/1 toggle `$d3x3j` added to the velocity index (11021, 14040, 15438): the order is 1, 2, 1, 2 across all keys. The solos ship one file on vel 1-2 (STpt group 99: one zone on vel 1-2). |
| no `section.styx` | No CSB patch has one yet. Every per-instrument table below goes into a CSB generator (a `css_section_spec.py` sibling), not into the shared spec. |
| `library.styx` header "Cinematic Studio Strings — <label>" | Cosmetic: the sync tool's header text. |

---

## 1. Articulation index → groups, CC58, keyswitch notes, availability

**KSP.**

- The CC58 dispatch is at 17938-18113. The mode var is `$ocjln`, and the
  short sub-type is `$k1phr`.
- `$dqcyd` selects muted short (0) or muted long (1). `$5kqeg` is the
  marcato overlay. `$j42b1` is legato on/off.
- The per-mode group selection is `select ($ocjln)` at 8506-9512.
- The group lists are built from each group's start-criteria codes
  (3384-3460): `$aylug` = mic 1..4, `$tylrz` = dynamic 1..5, and a list
  index of `mic*5 + dyn − 1`.

| CC58 | `$ocjln` / sub | Mix groups (name → recordings) | Availability gate |
|---|---|---|---|
| 0-5, 6-10 | 0 Sustain (no Low-Latency mode; both ranges are identical) | `vibsus p/mf/f/ff` → Sus; LT `legato *` → Leg; retrigger `legato zero *` → Legzero; releases `vibsus rel *` → Susrel | `%1dwfj[0]` |
| 11-15 | 1/0 Repetitions | `spiccato` → Repetitions (VeloIDX = band·RR) | `%1dwfj[1]` |
| 16-20 | 1/1 Staccatissimo | `staccatissimo` | `%1dwfj[1]` |
| 21-25 | 1/2 Staccato | `staccato` | `%1dwfj[1]` |
| 26-30 | 1/3 Sfz | `sfz` | `%1dwfj[1]` |
| 31-40 | 2/`$dqcyd=0` muted short | `pizzicato` → **Mutereps** (4Hn: **Stoppedreps**) | `%1dwfj[2]` |
| 41-45 | 7 Rips | `col legno` → Rips | `%1dwfj[7]` |
| 46-50 | 4 Trills | `trills p/mf/ff` → HTrills + WTrills (the half/whole choice is `EVENT_PAR_0` from `CSB_T`); releases `half/whole trills rel` | `%1dwfj[4]` |
| 51-55 | 3 Flutter | `harmonics pp/mp` → Flutter p/ff; `harmonics rel` → Fluttrel | `%1dwfj[3]` |
| 56-60 | 2/`$dqcyd=1` muted long | `tremolo mp/f/fff` → **Mutesus** p/mf/ff (4Hn: **Stoppedsus**); `tremolo rel` → Mutesusrel / Stoppedsusrel | `%1dwfj[2]` |
| 61-65 | 5 Taka (Dbl tongue) | `taka phrase` → Taka (VeloIDX = step + dyn·24) | `%1dwfj[5]` |
| 66-70 / 71-75 | 6 Marcato, overlay off / on | `marcato mod p/mf/f/fff` → Marcato; LT `marc leg *` → Leg (deeper start, +gain); `marcato mod rel *` → Marcrel | `%1dwfj[6]` |
| 76-80 / 81-85 | legato on / off | — | — |

**Keyswitch notes** (`%1dwfj` / `%hwclf`; KS handling at 17092-17179).
These are MIDI numbers; in our naming (C0 = 12), 24 = C1.

| Note | Function | Velocity map |
|---|---|---|
| 24 C1 | Sustain | — |
| 25 C#1 | Muted | ≤ 64 → Mutereps, > 64 → Mutesus |
| 26 D1 | Taka | — |
| 27 D#1 | Trills | — |
| 28 E1 | Flutter | — |
| 29 F1 | Shorts | 0-32 Rep, 33-64 Staccatissimo, 65-96 Staccato, 97-127 Sfz |
| 30 F#1 | Marcato | ≤ 64 overlay off, > 64 on |
| 31 G1 | Rips | — |
| 34 A#1 | Legato | > 64 on, ≤ 64 off |
| 35 B1 | Con sordino **bus FX** (`$ad4ea`, `setConsordino` 6818) | > 64 on |

**Per-instrument availability** (`%1dwfj` index → −1 = absent). It matches
the zone census:

| | 2Tb | 2Tpt | 4Hn | SBTb | SHn | STb | STpt | Tuba |
|---|---|---|---|---|---|---|---|---|
| Trills [4] | – | ✓ | ✓ | – | ✓ | – | ✓ | ✓ |
| Muted [2] | ✓ | ✓ | ✓ (stopped) | ✓ | ✓ | ✓ | ✓ | – |
| Taka [5] | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | – |
| Playable keys `$o5105..$oqjvl` | 40-72 | 56-88 | 40-79 | 40-72 | 40-79 | 40-72 | 56-88 | 38-71 |

Keys outside the playable range never sound: they go to the keyswitch
branch (17063).

**Borrowed zones.** Several Kontakt groups play other recordings, so zone
sync must emit them as `borrowed`. The current `library.styx` files contain
no `borrowed` entries, so this needs checking. Seen:

- 2Tb `marcato mod p` = Sus mf.
- STpt `marcato mod mf` = Marcato mf + Sus f.
- Tuba `marcato mod mf` = Marcato mf + Sus mf.
- Tuba `marcato mod rel mf` = Marcrel mf + Susrel mf.
- 2Tb / STpt `marcato mod rel p` = Susrel mf / –.
- Every `marc leg *` group = Leg layers: 2Tb p = Leg mf; STpt mf = Leg f;
  STpt f = Leg ff.
- SBTb Repetitions band 1 and band 2 are **both** `mp` (VeloIDX 1-14).

**Can the schema say it?**

- Yes, for:
  - CC58: `keyswitch.cc58_map`.
  - Notes: `keyswitch.notes[].vel_map` with `Mutereps` / `Mutesus` /
    `Marcato+@overlay-on`, `@legato-on/off`.
  - Availability: `instrument_filter`.
  - Ranges: `performance.strict_key_ranges` plus `sections[].lowest/highest`
    (set them to the `$o5105..$oqjvl` keys).
- 4Hn stopped mutes: the CC58 token `Mutereps` must resolve to
  `Stoppedreps` on 4Hn. `ArticulationSpec.aliases` may already do this
  (`Mutereps aliases (Stoppedreps)`). **Verify** that the engine resolves
  aliases when it looks up a keyswitch token. If it does not, the generic
  fix is "a keyswitch token resolves through `aliases` per section".
- SBTb duplicated `mp`: `dynamics (mp mp mf f fff)` (positional band → layer)
  works only if the engine indexes bands by position, not by name.
  **Verify.**
- Con sordino bus FX: `performance.sordino_filter` is a placeholder. The bus
  insert's parameters (FX slot 6) are not decoded (see §7).

---

## 2. Short notes: velocity bands, dB, round robins, the `%ktuur` quirk

**Band selection** (8660-9374). This is the same law as CSS `$arhiq`:

1. A band's top threshold is `T[i]`.
2. The velocity index is `VeloIDX = RR + band·RRcount`. The zones encode
   this: for STpt Staccato, pp = vel 1-4, mp = 5-8, f = 9-12, fff = 13-16.
3. The gain is `dB[band] · (T[band] − vel) / (span − 1)`, where `span` is the
   band's width (`UpdateVeloVol` 6556-6696: `%lwgt4 = %qgipe·−100000/(%bcnzg−1)`).
4. The top band uses 127 as its top. Only the first six entries (slot 0,
   `$g25wo = 0`) matter for a section patch.

| Articulation | Thresholds var | dB var | Band count | RR count | RR rule (per key) |
|---|---|---|---|---|---|
| Repetitions | `%g1qri[1..n−1]` | `%bcez1[0..n−1]` | **`%ktuur[0]` = 3/4/5/6** (8682, 8714, 8754, 8802) | `%jmvhj[0]` = 7 | random, not one of the last **2** (8672-8678) |
| Staccatissimo | `%aoywv` | `%ak5pm` | `%ktuur[1]` = 1 → 4 bands (0 → 3) | `%jmvhj[1]` = 5 | not one of the last 2 (9103-9109) |
| Staccato | `%nfxdy` | `%opj4y` | `%ktuur[2]` = 1 → 4 | `%jmvhj[2]` = 4 | not the last 1 (9201-9206) |
| Sfz | `%cjzwd` | `%5u3ic` | `%ktuur[3]` = 0 → 3 | `%jmvhj[3]` = 2 | not the last 1 (= alternate) (9298-9303) |
| Mutereps / Stoppedreps | `%n4f4k` | `%pyntp` | `%ktuur[4]`: 2Tb 1 → 4, others 0 → 3 (8588) | `$v0vlb` = 5 | not the last 1 (8564-8569) |

The `%ktuur` quirk is that CSB stores `%ktuur[0]` as a **band count**, while
the other slots hold a 0/1 "4 bands?" flag (CSS / CSW use a flag in every
slot).

Shipped values (slot 0). "Thr" lists the thresholds after the implicit 1.
`vel_thresholds` takes the first *bands − 1* of them.

| | Rep bands | Rep thr | Rep dB | Stacc'mo thr / dB | Staccato thr / dB | Sfz thr / dB | Mutereps thr / dB |
|---|---|---|---|---|---|---|---|
| 2Tb | 6 | 55 75 85 105 115 | −6 −7 −10 −4 −6 −9 | 40 75 95 / −6 −6 −6 −8 | 40 75 95 / −12 −6 −6 −8 | 75 105 / −23 −6 −7 | 45 75 110 / −6 −8 −4 −9 |
| 2Tpt | 5 | 65 85 105 115 | −16 −5 −6 −6 −6 | 75 105 115 / −10 −8 −6 −8 | 75 105 115 / −10 −8 −6 −8 | 65 105 / −18 −6 −7 | 42 75 / −6 −10 −8 |
| 4Hn | 4 | 55 85 115 | −6 −9 −8 −11 | 65 95 117 / −6 −6 −6 −8 | 65 85 121 / −12 −14 −6 −6 | 75 105 / −23 −6 −7 | 45 75 / −6 −8 −4 |
| SBTb | 5 | 55 75 95 115 | −6 −5 −8 −7 0 | 50 95 115 / −12 −9 −6 −8 | 50 95 115 / −12 −9 −6 −8 | 95 115 / −23 −6 −7 | 45 75 / −6 −8 −4 |
| SHn | 4 | 65 85 105 | −13 −8 −8 −6 | 65 85 105 / −6 −6 −6 −8 | 55 85 105 / −12 −14 −6 −8 | 75 105 / −23 −6 −7 | 45 75 / −6 −8 −4 |
| STb | 4 | 55 90 110 | −6 −8 −8 −7 | 55 85 105 / −6 −11 −9 −8 | 45 75 105 / −12 −11 −6 −8 | 75 105 / −23 −6 −7 | 65 95 / −6 −8 −16 |
| STpt | 5 | 42 75 85 115 | −6 −9 −6 −8 −11 | 65 95 117 / −6 −6 −6 −8 | 45 75 117 / −6 −14 −6 −6 | 75 105 / −23 −6 −7 | 45 75 / −6 −8 −4 |
| Tuba | 4 | 42 85 115 | −6 −12 −8 −11 | 75 95 115 / −6 −12 −9 −8 | 75 95 115 / −12 −12 −9 −8 | 85 115 / −23 −6 −7 | 65 95 / −6 −8 −16 |

Band → dynamic is the zone order. For example, Rep on 2Tb is
pp mp mf f ff fff, on 2Tpt mp mf f ff fff, and on SBTb mp mp mf f fff.

Shorts also have a release-length knob that a CC sends to the group flex's
third segment time (`set_short_note_CC` 6542-6555):

- CC22 = 75 (Repetitions), CC24 = 75 (Staccatissimo), CC26 = 75 (Staccato),
  CC28 = 75 (Sfz), CC31 = `$wp2ev` = 75 (Mutereps), CC33 = 127 (Rips).
- The base flex for every short group is
  `[8 ms → 1.0, 604 ms hold, 7381 ms → 0]`.

**Can the schema say it?**

- Yes, through a section overlay: `vel_thresholds`, `vel_layer_db`,
  `dynamics`, and `rr` on each short articulation (`ArticulationOverlay`),
  with `dynamics.enable_velocity_layers true` and `apply_short_velvol true`.
  The generator must read `%ktuur[0]` as a count.
- The short release time is `amp_env` (third segment) per articulation.
  The CSS generator's `pizz_release` already maps 75 → 1470 ms from a
  Kontakt measurement for the no-curve case (`Mix pizzicato`, `Mix
  spiccato`). Staccato's CC26 has a curve table `[0.297 0 1 1 0]`, so it
  needs one measurement.
- **New generic mechanism, B1: random round robin with per-key no-repeat
  memory.** Each key draws `random(1, rr)` again until the draw is not among
  that key's last *N* picks. Minimum field: `rr_no_repeat: u8` (0 = the
  engine's rotation; 1 or 2). Today `rr_sequence` is a fixed walk; CSB has
  no sequences.

---

## 3. Rips

**KSP** (case 7, 9436-9512):

- **RR:** `$ladk4 := $g0kc5` (the RR count). With more than one RR it draws
  a random RR that is not the key's last (per key).
- **Gain:** one band (`%ktuur[5] = 0`), so `VeloIDX = RR` and the gain is
  `(127 − vel)·%lwgt4[50]/100` = **`%tccxl[0]` dB · (127 − vel)/126**.
- There is **no dynamic layer and no CC1**. The release-length knob is CC33
  = 127 (full 7381 ms), and the start is CC32 = 127.
- Key → zone: one zone per key range. The zone's `dynamic` field holds the
  rip target pitch (sync artefact), and `rr_index` is the file number
  (`pack_cli.rs` `cs_is_rip`).

| | 2Tb | 2Tpt | 4Hn | SBTb | SHn | STb | STpt | Tuba |
|---|---|---|---|---|---|---|---|---|
| RR (`$g0kc5`) | 2 | 2 | 2 | 2 | **1** | 2 | 2 | **1** |
| `%tccxl[0]` dB at vel 1 | −16 | −12 | −11 | −12 | −12 | −12 | −10 | −12 |

**Can the schema say it?**

- Gain: mostly. Use `vel_thresholds ()` (one band) with `vel_layer_db
  (<tccxl>)`; the law `(top − vel)·d/(span − 1)` with top 127 and span 127
  is exactly the KSP's. Also `dyn_ctrl fixed` and `dynamics ()`. **Verify**
  that the engine applies `vel_layer_db` to a single-band, dynamic-less
  articulation, and that it ignores the zones' `dynamic` tag for Rips. If
  it does not, generic fix: "an articulation with no dynamics matches zones
  by key and RR only".
- RR: B1 with N = 1.

---

## 4. Sustain dynamics, releases, release volume by held time

**CC1 crossfade.** Every `Mix` long, legato and release group has a
`CC_VOLUME` CC1 table, and each instrument ships its own. For example,
`vibsus p` has four distinct tables across the eight instruments. Map them
the way the CSS generator does (`group_curves`), with CSB names:

| Group prefix | Articulation |
|---|---|
| `vibsus` | Sus |
| `legato` | Leg |
| `legato zero` | Legzero |
| `marcato mod` | Marcato |
| `marc leg` | (marcato LT layers) |
| `tremolo` | Mutesus / Stoppedsus |
| `harmonics` | Flutter (p = `pp`, ff = `mp`) |
| `trills` | HTrills + WTrills |

Releases:

| Group | Articulation |
|---|---|
| `vibsus rel` | Susrel |
| `tremolo rel` | Mutesusrel / Stoppedsusrel |
| `harmonics rel` | Fluttrel |
| `half/whole trills rel` | HTrel / WTrel |
| `marcato mod rel` | Marcrel |

CC2 is meaningless for the bodies. `$fod1v` (`set_sus_subcode` 6514)
chooses vibsus (CC2 `$takdp` > 64, shipped 126) or nonvib groups, and CSB
nonvib groups are empty. The release groups do carry a CC2 table
`[0 −0.169 1 1 0]`, which is about 1.0 at the shipped 126.

**New to brass: CC1 also moves filter cutoff and EQ gains.**

- Groups with `CC_CUTOFF` CC1 → `filterCutoff`:
  - `vibsus mf/f/ff`, `legato mf/f/ff`, `legato zero mf/f/ff`
  - `marcato mod mf/f/fff`, `marc leg f/ff`
  - `tremolo fff`, `harmonics mp`, `vibsus rel f/ff`, `marcato mod rel mf`
- `CC_EQ_FREQ_1` CC1 → `eqGain1/2/3` (and `eqFreq1`) on the ff/fff layers.
- `marcato mod *` also has an **`ENV_FLEX` on `filterCutoff`** with the same
  shape as its volume flex.
- CSS has none of this: `CSS 1st Violins/groups.json` contains 0
  `CC_CUTOFF`.
- `groups.json` does **not** decode the group insert FX (filter type, base
  cutoff, resonance, EQ band freqs and gains). The decoder must be extended
  first.

**Envelopes.**

- vibsus: `ENV_FLEX [4 → 1.0, 1000 hold, 20000 → 0]` plus `ENV_AHDSR`
  (attack 10 s, release 10 s, curve 0.65). CCs set the AHDSR times:
  - Legato **on**: CC80 = 0 (attack 0), CC81 = `$lzs0c` = 75.
  - Legato **off**: CC80 = `$vbta0` (0; **STb 80**), CC81 = 127 − `$af1oz`
    = 80.
  - Trills: CC82/83 (off: 0 / 127 − `$eqk5i`: 71 on 2Tb/SBTb/STb, 80 on
    the rest; on: `$cw0nn` 71/75).
  - Mutes: CC84/85 (80; on 75). Flutter: CC86/87 (80; on 75).
  - All in `set_sus_controls` 6076-6157. CSS measured 198 ms at CC = 30
    (attack) and 580 ms at CC81 = 85 (release); the brass values need the
    same law.
- Releases: flex `[1 → 0.986, 4007 hold, 1250 → 0]` plus AHDSR (attack 250,
  decay 25 s to 0.25, AHD only). CC36 is set to the CC1 value at note-off
  (17664) and drives `fm7_level1/2` through the group's CC1-shaped table,
  so a release's level follows CC1 at note-off.

**Release volume by held time** (on release 17361-17849). The release
plays `wait(1 µs)` after note-off at velocity 1, `change_vol(gflmk)`. The
articulation slot `$se4l4` is 0 Sus, 2 Mutesus, 3 Trills, 4 Flutter,
5 Marcato. Four contexts:

| Context (event par) | Table (times ms / dB) | Extra |
|---|---|---|
| Legato off (`PAR_1 = 1`, set by `CSB_W`) | `%ciooq` / `%54rxx`, 3 points, linear between, flat past C (17382-17436) | **+ `$b5zq1` = −3.0 dB** (17437) |
| Legato first note released alone (`PAR_3 = 1`) | `%ciooq` / `%54rxx` | — |
| Old note of a connection (`PAR_3 = 2`) / retrigger (`PAR_3 = 5`) | `%x1kwp` / `%5gm34`, 4 points (17516-17592) | past D: `%nuvn3[slot] = 1` → **no release** |
| Phrase end (`PAR_3 = 3`) | `%e1dvv` / `%fxjmy` (17599-17640) | — |

Shipped values (dB):

| Slot | Legato off / first note | In-phrase (A B C D) | Phrase end |
|---|---|---|---|
| Sus | 0 / 100 / 500 ms → −6 −6 0 | 0 0 0 500 → −3 −3 −3 −3 (2Tb, 2Tpt, SBTb, STpt); −6 ×4 (SHn, STb, Tuba); **4Hn 0 240 250 500 → −6 0 0 0** | 0 / 100 / 300 → −6 −6 0 |
| Mutes, Trills, Flutter | 0 / 100 / 300 → −6 −6 0 | 0 100 300 400 → −6 −6 0 −6 | same as Sus |
| Marcato | 0 / 60 / 200 → −9 −9 0 (2Tpt 0 / 100 / 200; **SHn 0 / 150 / 1000**) | 0 150 300 500 → −6 −12 −9 0 | 0 / 100 / 300 → −9 −9 0 |
| `%nuvn3[0]` (no Sus in-phrase release past 500 ms) | 1 on 2Tpt, 4Hn, SHn, STb, STpt, Tuba; 0 on 2Tb, SBTb | | |

**Body note-off.**

- Legato-on bodies: `note_off`, so the AHDSR release (CC81).
- LT and detached bodies: `fade_out(by_marks MARK_1)` over `$qxeb4` = 200 ms
  (17240), or `$gch4c` for marcato (2Tb, 4Hn, STpt 200; the rest 150).
- Marcato body (`$cpgu3`, 17853-17883): 100 ms if held < 100 ms; 500 ms if
  held > `$ks1cc` (2Tb 500 ms, the rest 1000 ms); linear between. A
  connected marcato body uses `$a3zg3` instead.

**Detached-note trim.** With legato off, every long except marcato gets
`change_vol($b5zq1)` = **−3 dB** (11028-11043). Marcato gets its velocity
law (`$5n14h` = 0, so 0 dB).

**Can the schema say it?**

- `group_curves`: yes. `amp_env` / `amp_env2`: yes.
  `ArticulationSpec.release_ms_by_held`: yes for the marcato body.
  `performance.release_delay_ms ≈ 0`: yes.
- **New, B4: release level by held time per context.** Per articulation:
  `release_held_db: { detached: [(ms, dB)…], first: […], in_phrase: […],
  phrase_end: […] }`, linear and clamped, plus
  `in_phrase_cutoff_ms: Option<f32>` (beyond it, no release). This
  replaces the single CSS `%ru5pa` table for libraries that set it. Also a
  **`detached_trim_db`** (the −3 dB legato-off body trim), on the legato
  engine or per articulation.
- **New, B5: per-layer CC-driven filter/EQ.** Per `GroupCurveSpec`-like
  entry: `{dynamic, cc, target: cutoff|eq_gain1|eq_gain2|eq_gain3|eq_freq1,
  points}`, plus the group's static filter/EQ definition (type, base
  cutoff, Q, band freqs/gains) and an optional flex envelope on cutoff.
  Prerequisite: `nkx-extract` must emit the group insert-FX parameters.
- CC80-87 times: `performance.attack_ms` / `release_ms` exist only per
  instrument. CSB needs them **per articulation and per legato state**: a
  small generic addition, `ArticulationSpec.attack_ms/release_ms` with
  `legato_on` variants, or the Kontakt CC→time law plus the CC values.

---

## 5. Legato transition (sustain, `$ocjln = 0`; 13944-15336, `CSB_W` slot 2)

**First note of a phrase** (13944-14389):

- The body plays from sample start (`$1fvjk := 0`).
- Velocity zone 1: no fade.
- Zones 2 and 3 (`$ksywl` = `$gbfsz` = 1): `fade_in` over `$2krfe` =
  **100 ms**, with no mute wait. The `random(2,4)` variant velocity is a
  no-op: CSB Sus zones span vel 1-4 with one sample start.
- The zone-3 attack voice plays velocity `random(8,9)` in the vibsus groups
  (14366-14385), but **CSB has no zones there** (vibsus = Sus vel 1-4
  only), so it is silent.
- The phrase dip `$4lqhx` / `$ee3a4` = 0 / 0 (15115-15125): no dip.

**Connected note** (`$cztyy` flip, 14539-15336):

1. **No pre-delay.** The new body and the LT start at note-on.
2. **Body offset** `$fjftm/$uwsuq/$252zx` = 0 / 0 / 0. Leap > 12: `$p3wmm` =
   **75 ms**. Retrigger (`$knvx2…`) = 0.
3. **New body** (the next layer slot `o5vyx` = dynamic layer: slot 0 p,
   1 mf, 2 f, 3 ff):
   - It is muted at start (`fade_out(…,1,0)`, 14685).
   - `EVENT_PAR_2 = wait·10000 + fade` from `%k1aqh[slot·20 + zone]` (wait)
     and `[slot·20 + 4 + zone]` (fade) (14708-14721).
   - `CSB_W` (`script_2` `on note`) polls the wait, then
     `fade_in(fade)`.
   - Leap > 12: the wait `[slot·20]` is 0, so the body fades in at once
     over `[slot·20 + 4]` (15306-15317).
   - **High-note override:** if either note is ≥ `$vv4qt`, every layer uses
     wait `$so0i3` / fade `$kk5ih` (14690-14696).
   - Retrigger (same key, CC64 held) uses `%wu5b4` the same way
     (14724-14731).
4. **Old body** `fade_out(%sbsxn[zone])`, where `%sbsxn` =
   (`$1qyra` big = 150, `$mhmq3`, `$qkgxg`, `$yn5xm`) (8452-8455, 15322).
   Retrigger: `%jajrd` = `$wgsgw` / `$kntrn` / `$eqtdi`.
5. **LT** (interval ≤ 12 only, 14800-15243):
   - It plays the lower note at velocity = interval (up) or interval + 12
     (down). On a retrigger it plays the key itself at a random Legzero RR
     `random(1,3)`.
   - Start offset: `$k2afl/$xem4t/$yds0m` by zone.
   - Envelope: the `legato *` group flex
     `[84 → 0.63, 216 → 1, 260 hold, 442, 986, 2012, 342 → 0, …]`. CC67
     moves `level1`/`time1`: CC67 = `$tcfuy` = 25 in legato sustain
     (6150-6152).
   - Level 0 dB (`$x0jlu` unused since `$shybn = 1`).
6. **LT IOI dip** (B3).
   - `$dwujq` from IOI = `$e44qy − $q4igo` (14473-14510): 0 for IOI below
     150 ms; then linear to −3.5 dB at 300, to −3.0 dB at 800, and back to
     0 at 1450 (`$xdpcr/$0crpu/$lgkxr/$fca5r` = 150/300/800/1450,
     `$x3vds/$4jko3` = −35/−30 deci-dB).
   - It is applied to the LT at once (15258-15262) and then recovers
     linearly to 0 over `$3ev2b` = `$pgocp` ms (16862-16961).
   - This is the CSB analogue of the CSW "Time A–D" IOI volume dip.
7. **LT fade-out tied to the body fade-in.** From the body's wait, over its
   fade time, the LT follows a 100-point dB curve `%jygjv` (in dB + 100,
   one per layer, identical everywhere: 0 dB until about 15%, −6 dB at 50%,
   −21 dB at 70%, −65 dB at 90%), then `note_off` (16786-16984).
8. **Previous LT voices** (`%0ou01`):
   - They fade over `$cig1o` (14735-14741): IOI below 150 ms → anchor A;
     above 300 ms → anchor B; linear between (`%0hwcy` is linear).
   - The loud class is zones 2-3 (`$251z0` = 1) with `$h3l3o` → `$4wrso`;
     the soft class is zone 1 with `$w4sb3` → `$elc3z`.
   - The voices before those fade over `$ry4xt` = 250 ms.
9. **Random LT detune** `$buuyh` is on for 2Tpt, 4Hn, SBTb, STb, STpt and
   Tuba, but `$rx2sc = 0`, so it is a **no-op**.

Shipped per-instrument values. `%k1aqh` is shown per layer as
"wait z1/z2/z3 · fade z1/z2/z3"; the big-leap fade is 150 (Tuba 500).

| | 2Tb | 2Tpt | 4Hn | SBTb | SHn | STb | STpt | Tuba |
|---|---|---|---|---|---|---|---|---|
| layer p | 2100 · 500 | 2100 · 500 | 1600 · 500 | 2100 · 500 | 2100 · 500 | 2100 · 500 | 2100 · 500 | 2100 · 500 |
| layer mf | 2100 · 500 | 2100 · 500 | 2100 · 500 | 2100 · 500 | 2100 · 500 | 2100 · 500 | 2100 · 500 | 2100 · 500 |
| layer f | (no layer) | 2100/2100/1800 · 500 | 1600/1600/200 · 500 | (no layer) | 2100 · 500 | 2100 · 500 | 2100 · 500 | (no layer) |
| layer ff | 1000 · 1000 | 1550/1550/1350 · 500 | 650/650/200 · 1000/1000/500 | 600 · 750 | 1000 · 500 | 650 · 500 | 2000/2000/200 · 500 | 1500 · 500 |
| `$vv4qt` high-note key / wait · fade | 127 (off) | 87 / 500 · 500 | 77 / 500 · 500 | off | 77 / 500 · 500 | off | 87 / 500 · 500 | off |
| retrigger `%wu5b4` ff layer | 650 · 1000 | 1650/1550/1350 · 500 | 670 · 1000 | 600 · 750 | 1000 · 500 | 650 · 500 | 2000 · 500 | 1000 · 500 |
| LT offset z1/z2/z3 (ms) | 42/83/83 | 64/117/117 | 26/117/117 | 42/83/83 | 26/117/117 | 54/83/83 | 83/117/160 | 42/117/117 |
| old body z1/z2/z3 (ms) | 300/200/200 | 250/300/300 | 400/300/300 | 280/200/200 | 300/150/150 | 300/200/200 | 250/300/300 | 280/200/200 |
| retrigger old body z1/z2/z3 | 400/300/300 | 400/300/300 | 400/300/300 | 400/300/300 | 400/300/300 | 400/300/300 | 300/200/200 | 400/300/300 |
| prev-LT retire loud (IOI 150 → 300) | 175 → 200 | 90 → 300 | 250 → 300 | 155 → 200 | 100 → 150 | 175 → 200 | 90 → 300 | 155 → 200 |
| prev-LT retire soft | 200 → 300 | 120 → 250 | 300 → 400 | 250 → 280 | 200 → 300 | 200 → 300 | 120 → 250 | 250 → 280 |
| IOI-dip recovery `$pgocp` (ms) | 407 | 407 | 450 | 407 | 450 | 356 | 294 | 356 |

All other `%wu5b4` layers are 1600-2000 · 500 (2Tb p/mf 1600, f 2000; 4Hn
1630; Tuba mf 1500). The generator should emit them from the array.

The ff layer (slot 3) is the row `CSB_CSW_ENGINE.md` reported as "F.wait"
for most instruments. The other layers mostly wait **2100 ms**: on a
connected note the transition sample carries the new pitch for about 2 s
before the body fades in.

**What replaces the CSS overlap delay:** nothing. The `wait` belongs to the
body, not to the transition (`CSB_W`). `overlap_delay` must be expressible
as zero: today `None` means the CSS defaults, so write all anchors 0, or
add an explicit "none".

**Can the schema say it?**

- Already there in `kontakt_transition`: `big_leap_body_offset_ms` (75),
  `big_leap_old_fade_ms` (150), `big_leap_new_fade_ms` (150 / 500), `lt_env`
  (from the group flex, CC67 = 25), `phrase_dip_db` 0.
  `lt_offset_expressive`: 3 zones (per instrument).
  `first_note_mute_ms (0 0)`, `first_note_fade_ms (100 100)`,
  `first_note_attack` absent. `velocity_splits (64 100)`.
- **New, B2: body handoff table.** In `kontakt_transition`:
  - `body_wait_ms` / `body_fade_ms` become tables indexed by
    `[dynamic layer][zone]`, plus a `big_leap` column: per layer
    `{wait: [z1,z2,z3], fade: [z1,z2,z3], big_wait, big_fade}`.
  - `retrigger_body` as the same table.
  - `high_key: {min_key, wait_ms, fade_ms}`.
  - `old_body_fade_ms: [z1,z2,z3]` and `retrigger_old_body_fade_ms`.
  - `lt_fade_curve_db: Vec<f32>` (100 points, applied over the new body's
    fade window starting at its wait), applied **per layer voice**. The
    layers' LT voices fade on their own clocks, so the engine has to treat
    transition voices per dynamic layer.
- **New, B3: LT IOI dip.**
  `lt_ioi_dip: { curve: IoiCurveSpec (dB anchors), recover_ms }`.
- **New, B8: previous-LT retire by IOI and attack class.**
  `prev_lt_retire: { soft: IoiCurveSpec, loud: IoiCurveSpec,
  loud_from_zone: u8 }` and `older_lt_retire_ms`.
- **B9: overlay plumbing.** `LegatoOverlay` must be able to override
  `kontakt_transition` (or each of the fields above) and
  `lt_offset_expressive`, because every number in the table above is
  per-instrument.

---

## 6. Marcato (`$ocjln = 6`): the velocity split instead of CSS's IOI tables

**KSP:**

- First note: 14009-14378. Legato connection: 15416-15880 and the
  crossfade `case 1` at 16600-16747. Detached: 11013-11106.
- RR: global toggle `$d3x3j` (velocity 1 / 2).
- Attack class: **soft if vel < `$kxbiw` (64), hard otherwise. No IOI
  term.** (CSS hard = vel > 75 and IOI > 50.)
- CC19 (the attack node levels of the `marcato mod` flex, `CC_TIME1:19 →
  fm7_level1/2`):
  - **127** on a first note, a retrigger key or a leap > 12.
  - Otherwise soft **`$313em` = 0**, hard **`$s3dc1` = 90** (14031-14040,
    15440-15452).
  - With legato off: 127 (retrigger key: 90) (11013-11020).
- Body: the next RR. Offset `$yp2sq + random(0,$a4bsp)`, where `$a4bsp` = 0
  (14605). The retrigger key enters at 0.
- Old/new body crossfade (16600-16747): xfade `$a3zg3`, split `$igmiu` = 50
  %, node divisor `$x444h` = 90 (flat), `$qcmdq` = 35, glide **off**
  (`$upjkh` = `$ma0b1` = 0), `$hy4uf` = 0 (no old-body sink step).
- LT (`marc leg *` groups, entered at the zone's `marcato_start` + offset):
  - Offset: soft `$5ibvn`, hard `$cblef` (14878-14882).
  - Level: soft `$b23jt`, hard `$rlbmz`, plus the velocity term `$0kd5m` =
    0 (15128-15134).
  - Previous LT retire: soft `$pprfd`, hard `$orqo1` (15877-15886).
  - The LT voice is not ramped (`$ocjln # 6` guard at 15218).
- Marcato velocity → volume: `$5n14h` = 0, so 0 dB.
- **Overlay** (CC58 71-75, `$5kqeg` = 1 shipped):
  - Plays a Repetitions short with the Repetitions band law (same
    `%g1qri`/`%bcez1`, RR rule B1/2) plus `$uz2rd`.
  - `$uz2rd` = `%wtvg4[0]` = **−361 → skipped** when vel < `$evk4m` (64) on
    a connected note. Otherwise 0 dB: a first note (`PAR_3` already 1), a
    loud connected note, or the retrigger key (14089-14365, 15478-15700).
  - There is no CC1 velocity compression (no `%fizyd` / Velocomp in CSB).
- Marcato legato-overlay voice `%2ezeo` (`$aejdu`) = 0: **off**.
- Release: §4 (Marcato row; body fade by held time; connected = `$a3zg3`).
- The `marcato mod` groups also carry a **cutoff flex** (B5).
- **CC20 → `fm7_time5`** is never sent by the script. Its value at render
  time is Kontakt's default or the host's: measure it, or send a known
  value in the null probe.

| | 2Tb | 2Tpt | 4Hn | SBTb | SHn | STb | STpt | Tuba |
|---|---|---|---|---|---|---|---|---|
| body offset `$yp2sq` (ms) | 0 | 40 | 40 | 40 | 40 | 250 | 0 | 250 |
| xfade `$a3zg3` (ms) | 330 | 120 | 200 | 120 | 250 | 250 | 180 | 250 |
| LT offset soft / hard (ms) | 83 / 177 | 135 / 177 | 117 / 180 | 83 / 177 | 135 / 177 | 83 / 180 | 135 / 212 | 117 / 212 |
| LT dB soft / hard | 0 / −12 | 0 / −12 | 0 / −12 | 0 / −9 | 0 / −12 | 0 / −12 | **−3** / −12 | 0 / −12 |
| prev-LT retire soft / hard (ms) | 200 / 200 | 150 / 250 | 120 / 90 | 200 / 200 | 120 / 200 | 200 / 200 | 180 / 120 | 150 / 150 |
| body note-off: held → fade | 100 → 100, 500 → 500 | 100 → 100, 1000 → 500 | (same) | (same) | (same) | (same) | (same) | (same) |
| LT / detached note-off fade `$gch4c` | 200 | 150 | 200 | 150 | 150 | 150 | 200 | 150 |

**Compared with the strings `marcato_transition`:**

| Strings field | CSB fit |
|---|---|
| `hard_velocity` / `hard_ioi_ms` | 63 / 0 (hard = vel ≥ 64, no IOI). Check the engine's `>` vs `≥`. |
| `xfade_ms`, `split`, `split_big`, `node_divisor` | Flat: `$a3zg3`, (50 50), 50, ((0 90)). |
| `body_offset_ms`, `body_offset_big_ms` | (`$yp2sq` `$yp2sq`) for both. |
| `cc19_soft` / `cc19_other` | ((0 0)) / (90 127 127). |
| `body_env_cc19` | From the flex `[20, 280, 1200, 104, 1000]`, levels 1 and 2 = −1 (= CC19 ÷ 127). |
| `body_env_cc18`, `cc18_*` | Unused (CSB never writes CC18). |
| `glide_*` | 0. |
| `lt_offset_ms`, `lt_db_step` / `lt_db_leap`, `lt_retire_ms` | CSS indexes them by **IOI**; CSB indexes them by **attack class**. They cannot be expressed today without abusing the IOI axis. |
| `in_phrase_release_db` | Flat in CSS; CSB is the held-time table (B4). |
| `overlay` | `{articulation Repetitions, cc1_scale all 100}`, plus the skip rule (B7). |

- **New, B6: attack-class-indexed marcato tables.** Allow
  `lt_offset_ms` / `lt_db_*` / `lt_retire_ms` to be given as
  `{soft: f32, hard: f32}`, or add `lt_offset_ms_by_class: [f32;2]`,
  `lt_db_by_class: [f32;2]`, `lt_retire_ms_by_class: [f32;2]`. The class
  rule is the existing `hard_velocity` / `hard_ioi_ms`.
- **New, B7: overlay skip rule.**
  `OverlaySpec.skip_connected_below_velocity: Option<u8>` (64). An overlay
  that plays only on phrase starts and loud connections.
- `rr_sequence (1 2)` with one library-wide counter (not per note) covers
  the `$d3x3j` toggle. `rr_per_note false`.

---

## 7. Behaviour the strings model does not have

1. **Muted / stopped as articulations, not a mode.** CSB has no
   `@sordino` switch for mutes: Mutereps and Mutesus are separate CC58
   targets (31-40 / 56-60), and the muted KS splits them by velocity. The
   KS note 35 "con sordino" is a **bus insert FX** (`setConsordino`:
   `ENGINE_PAR_EFFECT_BYPASS` on bus slot 6). Its parameters are undecoded,
   and `sordino_filter` stays a placeholder until the bus FX is decoded.
2. **Mutes, trills and flutter legato** (`$rldy4` = 4 crossfade, 16481-16596):
   - Old body fades `$aoogv/$na00h/$ofzvh` = 250 / 150 (Tuba 250) / 150;
     new body fades in `$gjsgz/$gsavh/$xkeuq` = 150 / 50 / 50.
   - Leap > 12: old `$tta5u` 250, new `$k1uuo` 1 ms.
   - Body offset `$ikf4e/$pc5ho/$rcx1d` = 212 ms. Leap `$slwsn` 75.
     Retrigger 212.
   - LT offsets: trills `$xbjei…` 177, mutes `$k4qdw…` 177.
   - LT level: `$w24kn` / `$0md1x` = **−36.1 dB (Tuba −6)**, ramping back
     up to 0 over `$wkeiv` = 1000 ms (`CSB_W` `PAR_0 = 111` path) while
     `fade_out` runs over `$zy4bz` / `$hgqzs` = 1000 ms. The result is a
     faint bump.
   - Flutter gets no LT-level branch and **no transition recordings**, so
     it is a body crossfade only (inferred from the zone census; verify by
     render).
   - This is close to `simple_transition` (`old_fade_ms`, `new_fade_ms`,
     `big_fade_ms`, `lt_*`). It lacks the LT ramp-up (B2's ramp, or a
     `lt_ramp_ms`) and B4 for the in-phrase release.
3. **CC31 "Muted staccato release"** `$wp2ev` = 75 sets the Mutereps flex's
   third segment time (`fm7_time3`), the same law as the CSS pizz release:
   75 → about 1470 ms (measured on CSS). Spec values only: an `amp_env`
   override.
4. **CC19** is CSB's marcato accent depth (§6). CSB never writes CC18.
   **CC29** (`$ipbym`, set at 11162, 14910, 16121) has no persistent value
   and no CC29 mod in any Mix group: ignore it.
5. **Trills** (`CSB_T`, `script_0`):
   - Pair window `$dhhc2` = 25 ms, and the trill plays after the window on
     the lower key at the first velocity. This is `trill_pair { window_ms
     25, delay_ms 25 }`.
   - Quirk: `CSB_T` only listens to keys 36-72 (`$ter3k` / `$oqjvl`
     hard-coded). On 2Tpt / STpt (keys 56-88), a **lone key above 72 plays
     a half-tone trill** (`EVENT_PAR_0 = 0`). Generic:
     `trill_pair.key_range` plus "outside the range a lone key plays
     `half`".
6. **Taka (double-tongue phrase engine)** (9515-10805):
   - `$uoswy` = 10 (manual mode) as shipped. VeloIDX = step + dyn·24, with
     a first step `random(1,3)` ≠ last, middle steps from `%1zo2t`, and a
     last step `random(22,24)`.
   - Steps advance per `$DURATION_SIXTEENTH` (tempo-synced) or the manual
     BPM `$acd3t`, after `$nh2j5` = 30 ms.
   - Level `%ow32p[0]` = −15 dB (2Tpt −9).
   - The dynamic index `%iqtfz` comes from velocity zones; pitch bend
     overrides it in phrase modes (17996-18017).
   - Generic, B10 (low priority): "repeat while held" phrase player
     `{step_note_ms | tempo_division, first_rr: [lo,hi], middle_rr_table,
     last_rr: [lo,hi], level_db}`.
7. **Chord detune with legato off**:
   - `$q4ous` = 1 on 2Tb, 2Tpt, SBTb and STpt: with ≥ 2 keys down, each
     long note is detuned `random(−$yqdot, $yqdot)` cents (5; 4Hn 10, but
     4Hn has it off) (11050-11076).
   - Generic, B11: `random_tune_cents` with a condition
     `when_keys_down >= 2`.
8. **The −3 dB detached trim** and the four release contexts (§4).
9. **CC2.** It only selects vibsus vs (empty) nonvib groups through
   `$takdp`, which is set from the UI or keyswitch, not from incoming CC2.
   Leave it alone; no vibrato dimension.

---

## Implementation order

### (a) Spec values only (no engine change)

1. **CSB generator** (`csb_section_spec.py`, modelled on
   `css_section_spec.py`) writing `_patches/<inst>/section.styx`:
   - Shorts: `vel_thresholds`, `vel_layer_db`, `dynamics`, `rr` (§2),
     reading `%ktuur[0]` as a band count and padding persistent arrays.
   - Rips: `vel_layer_db (<tccxl>)`, `rr`.
   - `group_curves` CC1 for every long, LT and release group, using the CSB
     group-name map in §4.
   - `amp_env` / `amp_env2` from each group's flex / AHDSR. Short release
     times come from CC22 / 24 / 26 / 28 / 31 = 75 (pizz law).
   - Marcato `marcato_transition` overrides that already fit (§6 table).
2. **Fix `cinematic-brass.styx`** (§0):
   - Drop the delay `zones`. Set `overlap_delay` to all-zero anchors.
   - `velocity_splits (64 100)`. `first_note_mute_ms (0 0)`,
     `first_note_fade_ms (100 100)`.
   - Remove the `short_note_cc1_map` until a render settles CC1.
   - `keyswitch.notes` (§1 table). The full `cc58_map`, including 6-10 =
     Sustain.
   - `strict_key_ranges true`, with the section ranges set to the
     `$o5105..$oqjvl` keys.
   - `trill_pair { half HTrills, whole WTrills, window_ms 25, delay_ms 25 }`.
   - Marcato `rr_sequence (1 2)`.
   - `release_ms_by_held` for Marcato. `aliases` for 4Hn Stopped*.
3. Run `zones sync-tsv` again and check that the borrowed groups (§1) are
   emitted.
4. Measure once in Kontakt: instrument gain (`instrument_gain_db`), the
   CC80/81 → attack/release law at 0 / 75 / 80, the Staccato CC26 law, the
   CC20 marcato default, and whether CC1 picks the short type.

### (b) New generic mechanisms (most impact first)

| # | Mechanism | Minimum fields |
|---|---|---|
| B2 | Body handoff table per dynamic layer × velocity zone, with the LT fade-out curve over the body fade window | `kontakt_transition.body: [{wait_ms:[f32;3], fade_ms:[f32;3], big_wait_ms, big_fade_ms}; layers]`, `retrigger_body` (same shape), `high_key: Option<{min_key:u8, wait_ms, fade_ms}>`, `old_body_fade_ms: [f32;3]` (+ `retrigger_old_body_fade_ms`), `lt_fade_curve_db: Vec<f32>` |
| B9 | Section overlay can set any legato-engine field (all of §5 is per instrument) | `LegatoOverlay.kontakt_transition: Option<KontaktTransitionSpec>` (or a field-wise mirror), `lt_offset_expressive: Option<Vec<u32>>` |
| B4 | Release level by held time per context, the in-phrase cutoff, and the detached body trim | `ArticulationSpec.release_held_db: {detached, first, in_phrase, phrase_end: Vec<[f32;2]>}`, `in_phrase_cutoff_ms: Option<f32>`, `detached_trim_db: f32` |
| B1 | Random RR with per-key no-repeat memory | `ArticulationSpec.rr_no_repeat: u8` (1 or 2), counted per key |
| B3 | LT IOI volume dip with linear recovery | `kontakt_transition.lt_ioi_dip: Option<{db: IoiCurveSpec, recover_ms: f32}>` |
| B8 | Previous-LT retire by IOI lerp per attack class | `kontakt_transition.prev_lt_retire: {soft: IoiCurveSpec, loud: IoiCurveSpec, loud_from_zone: u8}`, `older_lt_retire_ms: f32` |
| B6 | Marcato tables indexed by attack class (velocity split) instead of IOI | `MarcatoTransitionSpec.lt_offset_ms_by_class: Option<[f32;2]>`, `lt_db_by_class: Option<[f32;2]>`, `lt_retire_ms_by_class: Option<[f32;2]>` |
| B7 | Overlay skip on soft connected notes | `OverlaySpec.skip_connected_below_velocity: Option<u8>` |
| B5 | Per-layer CC-driven filter / EQ (needs the decoder to emit group insert FX first), plus a flex envelope on cutoff | `group_filters: [{dynamic, kind, base_cutoff_hz, q, eq_bands}]`, `group_curves[].target: volume\|cutoff\|eq_gain{1,2,3}\|eq_freq1`, `cutoff_env: Vec<EnvSegmentSpec>` |
| — | Per-articulation, per-legato-state attack/release times (CC80-87) | `ArticulationSpec.attack_ms / release_ms` (+ `_legato` variants) |
| B11 | Chord detune with legato off | `random_tune_when_keys_down: u8` |
| — | `simple_transition` LT ramp-up for mutes / trills | `SimpleTransitionSpec.lt_ramp_ms: f32` (with `lt_db` as the start level) |
| B12 | Trill-pair key range; lone key outside it plays `half` | `TrillPairSpec.key_range: Option<[u8;2]>` |
| B10 | Taka phrase player (tempo-synced repeat while held) | `phrase: {division \| step_ms, first_rr, middle_rr, last_rr, level_db}` |

After each engine change, run the strings null suite with
`SKIP_KONTAKT=1`. Every new field defaults to "off", so all 50 probes must
stay unchanged.
