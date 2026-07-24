# CSS — Legato-Transition Attack-Shape CCs (17 / 18 / 19 / 29)

Companion to `css-ksp-legato-algorithm.md` (§3 crossfade, §6 IOI interp,
§10 open items) and `css-ksp-anchor-values.md`. Closes the §10 open item:
"Attack-shape CCs 17/18/19/29 feed Kontakt group envelope/filter
modulators — they shape the transition sample's amp/filter attack but
aren't part of the fade *scheduling* math."

Two sources are cross-referenced:
- the KSP `script_1.ksp` — sends the CC values (`set_controller`);
- the extracted **group modulator data** `groups.json` / `groups.tsv`
  (609 Kontakt groups) — says *what each CC is wired to*. The KSP alone
  cannot answer that; the NKI group mods can, and were extracted here.

All line numbers = `nkx-extract/scratchpad/groups_out/script_1.ksp`.
Shipped values = `persistent_1.tsv`.

---

## 0. Headline

The four CCs are **not** the legato crossfade (that is the KSP-driven
two-stage `fade_in`/`fade_out`, `css-ksp-legato-algorithm.md` §3). They
are a **second, sample-intrinsic amplitude/attack envelope** baked into
the **Kontakt group modulators of the "marcato mod" group set** — which
is the sample set the **Expressive legato transition** (`$ocjln=6`) plays.

Decisive evidence: the CCs are written **only** inside `if ($ocjln=6)`
paths, and the **only groups in the entire 609-group instrument** whose
modulators read CC 17/18/19 are the 20 groups named `"* marcato mod *"`
(5 mics × dynamics mp/mf/f/fff). So by construction the expressive-legato
bow-change sample = a marcato-mod group, and these CCs shape its attack.

**CC 29 is inert** in this extract (see §1) — no group reads it and its
driver is never assigned.

---

## 1. CC → driver → Kontakt modulator → group  (the wiring table)

Modulator column is from `groups.json` (`mods[].source`/`.target`/`.cc`).
`CC_ATTACK→ahdsr_attack` = the amp **AHDSR attack-time** knob;
`CC_TIME1→fm7_level1/level2` = the **level of node 1 / node 2 of the
group's flexible volume envelope** (Kontakt's flex/"fm7" env engine) —
i.e. how high the amp envelope's early breakpoints sit = attack punch.

| CC | KSP driver | Label (set_text / debug) | Kontakt modulator (source→target) | # groups | Groups |
|----|-----------|--------------------------|-----------------------------------|----------|--------|
| **17** | `$s1fr2` | `"NonLeg. CC"` / debug `"NonLeg CC 17"` | `CC_ATTACK → ahdsr_attack` (amp AHDSR **attack time**) | 15 | `* marcato mod {mf,f,fff}` ×5 mics |
| **18** | `$eaghu` | debug `"Env CC 18"` | `CC_TIME1 → fm7_level1` **and** `→ fm7_level2` (flex vol-env node-1 & node-2 **levels**) | 15 | `* marcato mod {mf,f,fff}` ×5 mics |
| **19** | `$313em` | `set_text($313em,"Attack")` / debug `"Attack CC 19"` | `CC_TIME1 → fm7_level1` (flex vol-env node-1 **level**) | 20 | `* marcato mod {mp,mf,f,fff}` ×5 mics |
| **29** | `$ipbym` | debug `"Attack: … CM"` | **none — no group reads CC29** | 0 | — (INERT) |

Modulator rows verified across all 609 groups; the full set of
`(cc, source, target)` combos was enumerated. CC17/18/19 appear
**exclusively** on the marcato-mod groups; no CC targets `filterCutoff`.

### set_controller cite lines

| CC | First-note path (`%f4tl5[$cztyy]=0`, 17526) | Transition path |
|----|---|---|
| 17 | `17558` =127 if vel>`$0yy15`, else `17563` =`$s1fr2` | `18634` =127 (always) |
| 18 | `17569` / `12140` =127 | `18642` =`$eaghu` |
| 19 | `17568` / `12139` =127 | `18638` =`$313em` |
| 29 | — | `12388` / `19863` =`$ipbym` (standard-legato `$ocjln≠6` path) |

`set_controller(29,$ipbym)` sits in the `$ocjln=0` (standard legato)
branch, but `$ipbym` is **declared (`1431`) and never assigned** → stays
0, and no group reads CC29. Dead / disabled feature. Confirmed: only
usages are `1431, 12387-12392, 19862-19864`.

---

## 2. Driver values (shipped) + interpolation

### CC17 — `$s1fr2` "NonLeg. CC"  → amp AHDSR attack time
- Declared `ui_value_edit $s1fr2(0,127,1)`, default 64, **shipped 32**
  (`5192-5196`, persistent).
- Gate `$0yy15` = velocity split, declared default 64 (`5186-5187`),
  **shipped 1**.
- Applied `if EVENT_VELOCITY > $0yy15 (=1): CC17=127 else CC17=$s1fr2`
  (`17557-17567`). With the shipped gate of **1**, essentially **every
  real note sends CC17 = 127**; `$s1fr2`=32 only fires at velocity 1.
- In the transition branch CC17 is hard-coded **127** (`18634`).
- **Net: CC17 = 127 for all transitions** → drives `ahdsr_attack` to the
  top of its modulation range.

### CC18 — `$eaghu` "Env"  → flex vol-env node-1 & node-2 levels
Interval- and IOI-bucket dependent (`18495-18632`). Interval test is
`$1e5yd>2` = **≥ minor third** ("min3") vs `≤2` = **< min3**; buckets are
the same IOI break `$wghyc`=150 ms etc. Anchor shipped values:

| condition | driver | shipped |
|---|---|---|
| ≥min3, IOI `< A` | `$lje1u` | **96** |
| < min3, IOI `< A` | `$ea2ac` | **127** |
| ≥min3 `B/C` base | `$zanef` | 52 |
| < min3 `B/C` base | `$54vxm` | 52 |
| ≥min3 `> C` | `$kxxub` | 52 |
| < min3 `> C` | `$srxsw` | 52 |
| interval > 12 | `$bkgr5` | 52 |
| re-bow (all buckets) | `$2ld3s/$5lfj0/$ue2mx/$kydyo/$15cwt/$ndthb` | **105** |
| re-bow, interval > 12 | `$cdv4h` | **127** |

A/B buckets lerp between the `<A` and `B/C` anchors via the standard
`$5agkf*($l022q*10000/$5yyiz)/10000 + base` interpolant (`18515-18530`).
**Reading: small interval + fast note → CC18 ≈ 127 (full-level, punchy
attack); wide interval / slow note → ≈ 52 (softer); re-bow ≈ 105.**

### CC19 — `$313em` "Attack"  → flex vol-env node-1 level
Co-computed with `$igmiu` (the Atk-Fade split) in the §6 IOI-interp
block (`18119-18310`), with two tables selected by `$kbqnb`:

`$kbqnb = 1` (hard/fast) iff `IOI > $hx3nl(=50 ms) AND vel > $qm4n3(=75)`,
else 0 (`18114-18118`).

| bucket | soft `$kbqnb=0` driver → shipped | hard `$kbqnb=1` driver → shipped |
|---|---|---|
| IOI `< A` | `$ueewd` → **0** | `$hajd5` → **127** |
| `A/B` lerp base | `$ueewd`(0) → `$lcreu`(60) | `$hajd5`(127) → `$aplkl`(127) |
| `B/C` lerp base | `$lcreu` → **60** | `$aplkl`(127) → `$scidb`(100) |
| IOI `> C` | `$lodpt` → **0** | `$scidb` → **100** |
| interval > 12 | `$qak4x` → 0 (clamp) | `$3vz54` → 96 |

**Reading (matches the reference's "0→60→0 soft, →100 hard"): soft
playing sweeps CC19 up to ~60 mid-IOI then back to 0; hard/fast playing
holds CC19 ~96-127.** Higher CC19 = higher flex-env node-1 level = more
immediate/punchier attack of the transition sample.

### CC29 — `$ipbym`: constant **0**, no consumer. Ignore.

---

## 3. The transition sample's actual amp envelope (group data)

The marcato-mod groups carry a fixed **flex volume envelope** (the shape
CC18/19 modulate the node levels of) plus an **empty-segment AHDSR**
(the attack-time CC17 modulates). From `groups.json` (mp group example,
identical across mics; `groups.tsv` shows the same as the `segments`
column):

Flex **volume** env (primary amp), `f`/`mf` variant — segments as
`[time, flag, level]`, level 0..1:
```
[1,1,0.5] , [559,1,0.505] , [1440,1,0.67] , [484,1,0.5] , [6516,0,0.05]
```
A second flex vol env (mf/f/fff): `[1,·,0.5],[919,·,0.70],[470,·,0.9],[3130,·,0.05],[336,·,0.5]`.
Flex **filterCutoff** env (fixed, see §5):
`[1,·,0.685],[1499,·,0.5],[104,·,0.45],[1000,·,0.63]`.
AHDSR (target=volume): **`segments: []`** — empty in the extract.

> **UNKNOWN — time unit + AHDSR base.** The flex-env first field is the
> per-segment time; the extractor did **not** annotate its unit (Kontakt
> stores flex times as a normalized rate, not ms). If literal ms these
> are slow swells (hundreds of ms), which is inconsistent with a snappy
> bow attack, so the unit is **not confirmed** — do not read these as ms.
> The AHDSR that CC17 drives came out with **empty segments**, so its
> base attack/hold/decay/sustain/release **times are not in the extract**
> (they live only in the live NKI's AHDSR module). Concretely: **the
> transition attack time in milliseconds is NOT decidable from this
> extract.** What *is* decidable is the wiring and the CC drive values
> (§1-2) and the relative direction of modulation.

---

## 4. Does the attack shape depend on velocity / IOI / interval?

Yes — three axes, all via the CC drive values (§2), independent of the
crossfade timing:

- **First note of a phrase** (`%f4tl5[$cztyy]=0`, 17526): CC18=CC19=**127**
  (`12139-12140`, `17568-17569`) → **maximum-punch attack**. Later
  transitions get the interpolated softer values.
- **Velocity**: CC17 gate (`$0yy15`) and the `$kbqnb` hard flag
  (`vel > $qm4n3=75`) push CC19 onto the hard 96-127 table.
- **IOI (speed)**: CC19 and CC18 interpolate over the same IOI buckets as
  §6 — **fast → high attack (punchy), slow → low (soft/rounded)** on the
  hard table; the soft table peaks mid-IOI (~60) and eases to 0 at the
  extremes.
- **Interval**: CC18 splits at **≥ minor third** vs **< min3**
  (`$1e5yd>2`); interval > 12 st takes fixed anchors (52 / 96); re-bow
  (repeated pitch) uses its own 105/127 anchors.

---

## 5. Filter attack (Q5)

**CC17/18 do NOT drive a filter.** No modulator anywhere targets
`filterCutoff` from a CC (verified across all 609 groups). The marcato-mod
groups *do* have a **fixed** flex `filterCutoff` envelope
(`[1,·,0.685],[1499,·,0.5],[104,·,0.45],[1000,·,0.63]`) — it opens
slightly brighter at onset (node level 0.685) then settles to ~0.5-0.63,
giving a brief bow-attack brightness — but it is **not CC-modulated** and
does **not** change with velocity/IOI. Treat it as a static per-sample
tone shaping.

---

## 6. Rust wiring

What a Rust engine should do for the **Expressive** (`$ocjln=6`)
transition voice, on top of the §3 two-stage KSP crossfade:

1. **Keep the §3 crossfade as the transition fade-in.** The audible
   "fade-in shape" of the bow change is the KSP `fade_in(%jcxqm, $mlnoy)`
   two-stage swell (`css-ksp-legato-algorithm.md` §3.1-3.5), *not* these
   CCs. Replicate that first; it is fully decoded.

2. **Layer a per-voice attack shaping on the transition sample** whose
   *strength* follows the CC drive values:
   - an **amp-attack-time control** ← CC17 (= **127** in practice, i.e.
     always at the top of its range — you can treat it as a constant);
   - an **attack-punch / early-envelope-level control** ← CC19
     (`$313em`) and CC18 (`$eaghu`), computed from
     `(velocity, IOI, interval, first-note?, rebow?)` exactly per §2/§4.
   Map "high CC → punchier/faster attack, low CC → softer/rounder", using
   the §2 tables. First note & fast/hard/small-interval ⇒ full punch (127);
   slow/wide-interval ⇒ soft (~52); soft-table mid-IOI ⇒ ~60.

3. **Static filter tilt** (§5): a small fixed downward cutoff settle at
   onset, not CC-driven — optional polish, safe to omit.

4. **CC29: ignore** (inert).

**Honest limit:** the *absolute* attack time in ms and the AHDSR base
segments are **not in the extract** (empty AHDSR; flex-env time unit
unconfirmed). Do **not** hard-code an attack-ms figure from this data.
Either (a) measure the marcato-mod sample onsets directly from the
extracted WAVs, or (b) tune the per-voice attack-punch curve by ear,
driven by the §2 CC values which *are* fully decoded. The **direction and
data-dependence** of the modulation is authoritative; the **millisecond
scale is not.**
