# CSS — Shipped Anchor Values for the IOI-Interpolated Legato Curves

Exact numbers a Rust engine needs to reproduce Cinematic Studio Strings'
legato crossfade / overlap-delay curves **precisely** (no approximation).

- **Source of logic:** `nkx-extract/scratchpad/groups_out/script_1.ksp`
  (line numbers cited per block).
- **Source of numbers:** `nkx-extract/scratchpad/groups_out/persistent_1.tsv`
  — the shipped runtime state the NKI loads over the script's `:=` defaults.
  Every value below is the **shipped** (persistent) value unless it says
  otherwise. A handful of runtime-only vars (`$b0n3s`, `$1fvjk`, `$qcmdq`,
  `$wgvbf`) are *not* in the tsv because they are computed each note, not
  persisted — noted inline.
- Companion: `css-ksp-legato-algorithm.md` (mechanism, variable glossary,
  the two-stage crossfade engine). This file is the numeric appendix to
  that document's §1.3, §1.4, §6, §8. **Do not re-derive the mechanism
  here — this file only pins the numbers.**

The X-axis everywhere is the **IOI** (`$d5ans = $ftvnh`, ms = inter-onset
interval). All interpolation is integer piecewise-linear:

```
out = a0 + (a1 - a0) * (ioi - t0) / (t1 - t0)      for t0 <= ioi < t1
out = a0                                            for ioi < first_bp   (flat)
out = aN                                            for ioi >= last_bp   (flat)
```

The KSP scales by 10000 to keep integer precision
(`5agkf*(l022q*10000/5yyiz)/10000 + base`); a Rust `f32`/`f64` lerp is
equivalent. All of §1–§3, §6 are gated `if ($ocjln = 6)` (Expressive IOI
legato). §4 (OD) runs for every legato transition regardless of `$ocjln`.

---

## 1. `$a3zg3` — XTime (total crossfade time, ms) — lines 18347-18377

Single table (computed **outside** the `$kbqnb` branch, so no soft/hard
split; the interval>12 case is **not** special-cased here — it always
interpolates over IOI).

| IOI breakpoint | var | shipped ms | anchor var | shipped value |
|---|---|---|---|---|
| A | `$3kfur` | 150 | `$igtvr` (< A, flat) | **225** |
| B | `$jxo2x` | 300 | `$sfhq0` (A/B end) | **225** |
| C | `$5e3jr` | 500 | `$je4dz` (B/C end, > C flat) | **225** |

**Net: XTime = flat 225 ms for all IOI.** (All three anchors ship at 225;
the interpolation is a no-op in the shipped state. The breakpoints 150/300/500
would matter only if a preset set the anchors apart.) Script default was 250.

---

## 2. `$igmiu` — Atk-Fade split (%) — lines 18119-18314

Two tables selected by `$kbqnb` (`1` when `IOI > $hx3nl(50)` **and**
`vel > $qm4n3(75)`, else `0`; lines 18114-18118). Computed **alongside**
`$313em` (the CC19 attack-shape value — different anchors, same breakpoints;
included for completeness). Has an **interval>12 fallback**.

### 2a. Soft/normal table (`$kbqnb = 0`) — breakpoints `$jlgbx/$kcnco/$p5rr1`

| IOI bp | var | ms | `$igmiu` anchor | val | `$313em` (CC19) anchor | val |
|---|---|---|---|---|---|---|
| A | `$jlgbx` | 150 | `$bamzk` (< A) | **50** | `$ueewd` | **0** |
| B | `$kcnco` | 300 | `$gxklu` (A/B) | **50** | `$lcreu` | **60** |
| C | `$p5rr1` | 500 | `$4vedg` (B/C, > C) | **50** | `$lodpt` | **0** |

- **`$igmiu` = flat 50 % for all IOI** (all anchors 50).
- `$313em` (attack-shape CC19) = 0 → lerp up to 60 (@300ms) → lerp back to 0
  (@500ms) → flat 0.
- **interval > 12 fallback:** `$igmiu = $rqqqm = 50`, `$313em = $qak4x = 0`
  (lines 18131-18138, applied in every IOI band).

### 2b. Hard/fast table (`$kbqnb = 1`) — breakpoints `$m2pwa/$ljelf/$doano`

| IOI bp | var | ms | `$igmiu` anchor | val | `$313em` (CC19) anchor | val |
|---|---|---|---|---|---|---|
| A | `$m2pwa` | 0 | `$bdiws` (< A) | **50** | `$hajd5` | **127** |
| B | `$ljelf` | 0 | `$dudzw` (A/B) | **50** | `$aplkl` | **127** |
| C | `$doano` | 1 | `$c55bn` (B/C, > C) | **60** | `$scidb` | **100** |

- Breakpoints are **0 / 0 / 1 ms**. Since `$kbqnb=1` requires IOI > 50 ms,
  IOI is **always ≥ the last breakpoint (1)** → the hard table always lands
  in the ">C" flat region: **`$igmiu = 60`, `$313em (CC19) = 100`**, constant.
- **interval > 12 fallback:** `$igmiu = $pforq = 50`, `$313em = $3vz54 = 96`
  (lines 18229-18236).

**Practical summary:** shipped Atk-Fade split is **50 %** soft / **60 %**
hard, effectively constant. The IOI machinery is present but flat in this
preset.

---

## 3. `$x444h` — Node-Vol divisor — lines 18316-18346

Single table (outside `$kbqnb`; no interval>12 special-case). Breakpoints are
the **same** as the `$kbqnb=0` Atk-Fade table (`$jlgbx/$kcnco/$p5rr1`).

| IOI breakpoint | var | ms | anchor var | shipped value |
|---|---|---|---|---|
| A | `$jlgbx` | 150 | `$flfzo` (< A, flat) | **90** |
| B | `$kcnco` | 300 | `$uutz4` (A/B end) | **60** |
| C | `$p5rr1` | 500 | `$owvdm` (B/C end, > C flat) | **60** |

**Net:** `x444h = 90` for IOI < 150 ms → **lerp 90 → 60** over 150–300 ms →
**60** for IOI ≥ 300 ms. (B/C segment 300–500 is 60→60, flat.)

This is the stage-1 fade-in **divisor**: `$mlnoy = $hzl4j*$igmiu/$x444h`.
With `igmiu=50`: at fast IOI `mlnoy = hzl4j*50/90` (stage-1 slightly *longer*
than the 50 % split → gentler), at slow IOI `hzl4j*50/60` (stage-1 much
faster/punchier). Lower divisor = faster stage-1 rise.

---

## 4. `$b0n3s` — Overlap-Delay / prefire (ms) — `legtrans_OD`, lines 6959-7180

`$b0n3s` is runtime-computed (not in tsv). Two mode tables by `$tuu20`
(`1`=Expressive/EX, `0`=Low-latency/LL — **shipped `$tuu20 = 0`**), each with
per-velocity-zone (`$xp1ku`) anchor **rows** over four IOI thresholds A/B/C/D.
Lerp is `anchor_lo + (anchor_hi-anchor_lo)*(ioi-t_lo)/(t_hi-t_lo)`; flat below
A and above D. `wait($b0n3s*1000)` fires only if `$b0n3s > 0`.

### 4a. Expressive (`$tuu20 = 1`) — thresholds A/B/C/D

| threshold | var | shipped ms |
|---|---|---|
| A | `$g45yq` | 200 |
| B | `$bwkdm` | 300 |
| C | `$waq1e` | 800 |
| D | `$whtm2` | 800 |

**Note C == D == 800**, so the C/D segment is degenerate (zero width) — IOI ≥
800 falls straight to the "> D" flat anchor. Effectively three live bands
(<A / A-B / B-C) then flat.

| zone (`$xp1ku`) | < A | A/B end | B/C end | > D | vars |
|---|---|---|---|---|---|
| 1 | **83** | **0** | **0** | **0** | `$kadcz/$nug53/$tfwqt/$xvurx` |
| 2 | **0** | **0** | **0** | **0** | `$lkn4u/$ff0lo/$vl1ol/$qqqvj` |
| 3 | **0** | **0** | **0** | **0** | `$yncub/$gxiln/$ta22b/$3uh30` |

EX shipped: **zone 1 = 83 ms prefire for IOI < 200 ms, lerping to 0 by
300 ms**; zones 2 & 3 = 0 (no prefire on louder/faster attacks). Script
default row 1 was 0/42/83/117 — shipped flattened it to 83/0/0/0.

### 4b. Low-latency (`$tuu20 = 0`, the shipped mode) — thresholds A/B/C/D

| threshold | var | shipped ms |
|---|---|---|
| A | `$deey3` | 75 |
| B | `$fxiox` | 100 |
| C | `$jystg` | 800 |
| D | `$zvaet` | 1100 |

Rows: zone 1 uses `case 1`; zones 2 **and** 3 share `case 2 to 3`.

| zone (`$xp1ku`) | < A | A/B end | B/C end | C/D end | > D | vars |
|---|---|---|---|---|---|---|
| 1 | **77** | **0** | **0** | **0** | **0** | `$nbkqa/$mih5r/$yzpsq/$myv02` |
| 2–3 | **0** | **0** | **0** | **0** | **0** | `$55anl/$umt5l/$cffjr/$yeo2q` |

LL shipped: **zone 1 = 77 ms prefire for IOI < 75 ms, lerping to 0 by
100 ms**; zones 2-3 = 0.

---

## 5. Portamento glide — shipped values

| var | label | shipped | notes |
|---|---|---|---|
| `$upjkh` | glide on OUTgoing voice | **1** (on) | bends old pitch toward new |
| `$ma0b1` | glide on INcoming voice | **1** (on) | bends new pitch up from old |
| `$1mwwo` | BTime (glide duration, ms) | **60** | `$bz0g4 = 60000/$0cqdq = 60` ticks |
| `$ruv02` | Bend depth (base, millicents×? knob units) | **10** | interp'd for interval=1 (§5a) |
| `$i1kki` | Octave (depth-scale by interval) | **10** | 10 = **no** scaling |
| `$0cqdq` | Tick (loop period, µs) | **1000** | |
| `$qcmdq` | Rls-Fade | not in tsv → default **0** | note_off at end of stage-1 |

### 5a. Bend `$ruv02` IOI-interpolation — lines 18378-18500 (interval-dependent)

Breakpoints (shared by all three interval cases): `$ylgac`=75, `$xhiq1`=100,
`$jldrb`=500 ms. Computed **outside** `$kbqnb`. Anchors depend on `$1e5yd`
(= |interval|):

| interval | < A (75) | A/B end (100) | B/C end / >C (500) | vars |
|---|---|---|---|---|
| = 1 | **40** | **10** | **10** | `$acia1/$lck0q/$5ggch` |
| = 2 | **30** | **10** | **0** | `$ntv1r/$vesyx/$2dpwc` |
| > 2 | **20** | **0** | **0** | `$pnuq4/$dsy2m/$hpaga` |

So for a semitone step played fast (IOI < 75 ms) the bend depth is 40 (×1000
= 40000 millicents into `$jyttf`), dropping to the knob value 10 for IOI ≥
100 ms. Wider intervals get progressively less bend. (Note the knob `$ruv02`
= 10 is only the fallback / display value; the interval-1 curve overrides it
per note.)

---

## 6. `$1fvjk` — LT-Offset (transition-sample start, ms)

`$1fvjk` is runtime-computed (not in tsv). There are **two** computations in
`on note` and they feed **different samples**:

- **Computation A (lines 12316-12386)** → feeds the `%ftriy` supplementary
  bow/transition sample (`play_note … 12699`). Adds `$b0n3s` for the
  `$ocjln=0` bases and sets CC 65.
- **Computation B (lines 18648-18726)** → feeds the **main transition voice
  `%jcxqm`** (`play_note … 18742`). Fixed per (kbqnb, interval-band). This is
  the one that positions the actual bow-change sample.

### 6a. Computation A — `$ocjln = 0` bases (+ OD), lines 12316-12339

`$1fvjk := base + $b0n3s`, then `set_controller(65, …)`.

| mode | zone 1 | zone 2 | zone 3 | CC65 |
|---|---|---|---|---|
| EX (`$tuu20=1`) | `$fjf3c` **0** | `$2p1wl` **83** | `$ywj0r` **177** | `$tdvbm` **127** |
| LL (`$tuu20=0`) | `$ak2j4` **100** | `$ixzi1` **148** | `$cltif` **177** | `$h4cys` **111** |

### 6b. Computation A — `$ocjln = 6` IOI-interp, lines 12341-12372

Breakpoints `$yam53`=100, `$nzsuf`=150, `$5c2um`=500 ms; anchors:

| IOI | < A (100) | A/B end (150) | B/C end / >C (500) | vars |
|---|---|---|---|---|
| `$1fvjk` | **177** | **177** | **117** | `$ggt00/$v0rbb/$5exar` |

→ flat 177 ms below 150 ms IOI, lerp 177→117 over 150–500 ms, flat 117 above.
(Short modes 2/3/4 use `$euzet/$gdfnk/$uhlcw` = 177/177/177.)

### 6c. Computation B — the main `%jcxqm` transition offset, lines 18648-18726

**`$ocjln = 0`** (per-zone, lines 18648-18656; overrides A for `%jcxqm`):
zone1 `$fjftm`=0, zone2 `$uwsuq`=0, zone3 `$252zx`=0.
- interval > 12: `$p3wmm` = **75**.
- re-bow (same note & `$zs1l1=1`): `$knvx2/$zk0vu/$iufx3` = **0 / 0 / 0**.

**`$ocjln = 6`** (kbqnb-split fixed, lines 18704-18726):

| kbqnb | interval = 0 (re-bow) | interval 1–12 | interval > 12 | vars |
|---|---|---|---|---|
| 0 (soft/normal) | **60** | **60** (+`random(0,$yvs44=0)` = +0) | **10** | `$ij3lo / $yp2sq+$yvs44 / $dztsp` |
| 1 (hard/fast) | **20** | **20** (+`random(0,$yamot=0)` = +0) | **0** | `$gnd40 / $po1el+$yamot / $fswjh` |

Both random spans (`$yvs44`, `$yamot`) ship at **0**, so the values are
deterministic. So the shipped Expressive transition sample starts **60 ms**
in for a normal step, **20 ms** in for a hard/fast step, **10/0 ms** for
leaps > 12 st.

---

## 7. Cross-check: derived crossfade timings with shipped values

Using shipped `$a3zg3=225`, `$igmiu=50` (soft) / `60` (hard), `$x444h` = 90
(fast IOI) → 60 (slow IOI), `$0cqdq=1000`, `$qcmdq=0`:

```
$hzl4j = 1000*225 = 225000 µs
soft, fast-IOI:  $mlnoy = 225000*50/90 = 125000 µs (125.0 ms stage-1)
soft, slow-IOI:  $mlnoy = 225000*50/60 = 187500 µs (187.5 ms stage-1)
$rixqv = 225000*(100-50)/100 = 112500 µs (112.5 ms stage-2 swell)
$vlmkl = 225000/1000 = 225 ticks (outgoing note_off)
$qsazz = 225*50/100 = 112 ticks (stage-1)
$wvsi3 = 112500/1000 = 112 ticks (stage-2)
```

---

## 8. Rust wiring notes

Each parameter is a pure piecewise-linear function of (ioi_ms, velocity,
interval). Suggested signatures (all lerp = the integer/float lerp in the
preamble; clamp flat outside the breakpoints):

```rust
// §1  XTime — shipped: constant. Keep the general form for preset flexibility.
fn xtime_ms(ioi: f32) -> f32 {
    pwl(ioi, &[(150.0, 225.0), (300.0, 225.0), (500.0, 225.0)])  // → 225 flat
}

// §4  hard/fast selector
fn kbqnb(ioi: f32, vel: u8) -> bool { ioi > 50.0 && vel > 75 }

// §2  Atk-Fade split % (also drives CC19 attack-shape $313em, separate anchors)
fn atk_fade_pct(ioi: f32, vel: u8, interval: i32) -> f32 {
    if interval > 12 { return if kbqnb(ioi, vel) { 50.0 } else { 50.0 }; }
    if kbqnb(ioi, vel) {
        pwl(ioi, &[(0.0, 50.0), (0.0, 50.0), (1.0, 60.0)])   // always → 60
    } else {
        pwl(ioi, &[(150.0, 50.0), (300.0, 50.0), (500.0, 50.0)]) // → 50 flat
    }
}

// §3  Node-Vol divisor (stage-1 fade-in divisor; not interval-split)
fn node_vol_div(ioi: f32) -> f32 {
    pwl(ioi, &[(150.0, 90.0), (300.0, 60.0), (500.0, 60.0)])  // 90 → 60
}

// §4  Overlap-Delay ms. mode: false=LL (shipped), true=EX. zone 1..=3.
fn overlap_delay_ms(ioi: f32, zone: u8, ex_mode: bool) -> f32 {
    if ex_mode {
        let (t, row) = ([200.0,300.0,800.0,800.0], match zone {
            1 => [83.0, 0.0, 0.0, 0.0],
            _ => [0.0,  0.0, 0.0, 0.0],   // zones 2 & 3
        });
        pwl4(ioi, t, row)
    } else {
        let (t, row) = ([75.0,100.0,800.0,1100.0], match zone {
            1 => [77.0, 0.0, 0.0, 0.0],
            _ => [0.0,  0.0, 0.0, 0.0],   // zones 2-3 share a row
        });
        pwl4(ioi, t, row)
    }
}

// §5a  Bend depth (portamento). interval in semitones.
fn bend_depth(ioi: f32, interval: i32) -> f32 {
    let row = match interval {
        1 => [40.0, 10.0, 10.0],
        2 => [30.0, 10.0, 0.0],
        _ => [20.0, 0.0,  0.0],   // > 2 (interval 0 => no glide, handled upstream)
    };
    pwl(ioi, &[(75.0, row[0]), (100.0, row[1]), (500.0, row[2])])
}
// then: total_bend_millicents = bend_depth(..) * 1000; sign = (dst<src)?-1:+1;
//       scale by $i1kki (=10 => none); glide over $1mwwo=60 ms in $0cqdq=1000µs ticks.

// §6  LT-Offset for the main transition voice (%jcxqm), ocjln==6 (Expressive)
fn lt_offset_ms(ioi: f32, vel: u8, interval: i32) -> f32 {
    let hard = kbqnb(ioi, vel);
    match interval {
        0            => if hard { 20.0 } else { 60.0 },   // re-bow
        i if i <= 12 => if hard { 20.0 } else { 60.0 },   // +random(0,0)=0
        _            => if hard { 0.0  } else { 10.0 },   // leap > 12
    }
}
// ocjln==0 legato instead: base(zone) + overlap_delay_ms(..), EX base
//   [0, 83, 177], LL base [100, 148, 177]; plus set CC65 = 127(EX)/111(LL).
```

Where `pwl` / `pwl4` are the flat-ends piecewise-linear helpers from the
preamble (3-breakpoint and 4-breakpoint forms). `velocity` only enters via
`kbqnb` (hard/fast table select) and the OD velocity-zone row; `interval`
(= |semitone step|) selects the Bend and LT-offset rows and the interval>12
fallbacks. IOI (`$d5ans`) is the interpolation X for all of them.

---

## 9. Value provenance table (every anchor, one place)

All values from `persistent_1.tsv` unless marked. "—" = runtime-computed
(not persisted).

| var | value | var | value | var | value |
|---|---|---|---|---|---|
| `$a3zg3` | 225 | `$3kfur` | 150 | `$jxo2x` | 300 |
| `$5e3jr` | 500 | `$igtvr` | 225 | `$sfhq0` | 225 |
| `$je4dz` | 225 | `$igmiu` | 50 | `$x444h` | 90 |
| `$jlgbx` | 150 | `$kcnco` | 300 | `$p5rr1` | 500 |
| `$flfzo` | 90 | `$uutz4` | 60 | `$owvdm` | 60 |
| `$bamzk` | 50 | `$gxklu` | 50 | `$4vedg` | 50 |
| `$ueewd` | 0 | `$lcreu` | 60 | `$lodpt` | 0 |
| `$m2pwa` | 0 | `$ljelf` | 0 | `$doano` | 1 |
| `$bdiws` | 50 | `$dudzw` | 50 | `$c55bn` | 60 |
| `$hajd5` | 127 | `$aplkl` | 127 | `$scidb` | 100 |
| `$qak4x` | 0 | `$rqqqm` | 50 | `$3vz54` | 96 |
| `$pforq` | 50 | `$hx3nl` | 50 | `$qm4n3` | 75 |
| `$g45yq` | 200 | `$bwkdm` | 300 | `$waq1e` | 800 |
| `$whtm2` | 800 | `$deey3` | 75 | `$fxiox` | 100 |
| `$jystg` | 800 | `$zvaet` | 1100 | `$kadcz` | 83 |
| `$nug53` | 0 | `$tfwqt` | 0 | `$xvurx` | 0 |
| `$lkn4u` | 0 | `$ff0lo` | 0 | `$vl1ol` | 0 |
| `$qqqvj` | 0 | `$yncub` | 0 | `$gxiln` | 0 |
| `$ta22b` | 0 | `$3uh30` | 0 | `$nbkqa` | 77 |
| `$mih5r` | 0 | `$yzpsq` | 0 | `$myv02` | 0 |
| `$55anl` | 0 | `$umt5l` | 0 | `$cffjr` | 0 |
| `$yeo2q` | 0 | `$upjkh` | 1 | `$ma0b1` | 1 |
| `$1mwwo` | 60 | `$ruv02` | 10 | `$i1kki` | 10 |
| `$0cqdq` | 1000 | `$qcmdq` | — (dflt 0) | `$acia1` | 40 |
| `$lck0q` | 10 | `$5ggch` | 10 | `$ylgac` | 75 |
| `$xhiq1` | 100 | `$jldrb` | 500 | `$ntv1r` | 30 |
| `$vesyx` | 10 | `$2dpwc` | 0 | `$pnuq4` | 20 |
| `$dsy2m` | 0 | `$hpaga` | 0 | `$fjf3c` | 0 |
| `$2p1wl` | 83 | `$ywj0r` | 177 | `$ak2j4` | 100 |
| `$ixzi1` | 148 | `$cltif` | 177 | `$tdvbm` | 127 |
| `$h4cys` | 111 | `$ggt00` | 177 | `$v0rbb` | 177 |
| `$5exar` | 117 | `$yam53` | 100 | `$nzsuf` | 150 |
| `$5c2um` | 500 | `$euzet` | 177 | `$gdfnk` | 177 |
| `$uhlcw` | 177 | `$fjftm` | 0 | `$uwsuq` | 0 |
| `$252zx` | 0 | `$p3wmm` | 75 | `$knvx2` | 0 |
| `$zk0vu` | 0 | `$iufx3` | 0 | `$ij3lo` | 60 |
| `$yp2sq` | 60 | `$yvs44` | 0 | `$dztsp` | 10 |
| `$gnd40` | 20 | `$po1el` | 20 | `$yamot` | 0 |
| `$fswjh` | 0 | `$ghl4d` | 75 | `$ymmth` | 212 |
| `$o1f3o` | 212 | `$1aefj` | 212 | `$b0n3s` | — |
| `$1fvjk` | — | `$wgvbf` | — | `$ocjln` | 0 (loaded) |
| `$tuu20` | 0 | `$aguy2` | 1 | | |
