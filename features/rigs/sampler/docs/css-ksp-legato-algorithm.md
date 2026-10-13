# Cinematic Studio Strings — Legato / Playback Algorithm Reference

Reverse-engineered from the decompiled Kontakt KSP performance script
(`nkx-extract/scratchpad/groups_out/script_1.ksp`, ~26 000 lines,
obfuscated identifiers). Companion glue scripts: `script_0.ksp`
(keyswitch → articulation state via `pgs_*`) and `script_2.ksp`.

The goal is a spec a Rust sampler engine can replicate **without** reading
the KSP. Every claim cites `script_1.ksp` line numbers unless noted.
Shipped runtime values come from `persistent_1.tsv` (the values the NKI
actually loads over the script defaults) and are called out as **(shipped)**.

> **How the identifiers were decoded.** Nearly every tunable is a Kontakt
> UI control declared with `declare ui_knob/ui_value_edit/ui_menu $x(...)`
> immediately followed by `set_text($x,"<label>")`. The label is the
> human name. That plus the debug `set_control_par_str(... $CONTROL_PAR_TEXT
> ...)` strings (which spell out `"LT Offset"`, `"OD"`, `"< A"`, `"A/B"`,
> `"EX"`, `"LL"`, `"Velocomp"`, `"rebow"`, etc.) pin down the algorithm.

---

## 0. Big picture

CSS is a **monophonic-per-voice legato** instrument built on a
**double-buffered two-voice crossfade** (a ping-pong of slot `0` and slot
`1`). A new legato note:

1. **waits a fixed/interpolated prefire delay** ("Overlap-Delay", `$b0n3s`)
   before it sounds — this is the famous CSS latency;
2. **plays a dedicated legato-transition sample** into the *other* ping-pong
   slot, **started partway in** (`$1fvjk` = "LT Offset") so the wait is
   compensated and the transition stays rhythmically aligned;
3. **crossfades** the outgoing voice out and the incoming voice in, in a
   `while` loop, optionally with a **two-stage swell** and a
   **portamento pitch glide**;
4. leaves a held **sustain layer** (`%grhcg`, 5 mic positions) sounding
   whose volume tracks the CC1/dynamics fader.

There are **two distinct legato engines**, selected by articulation
(`$ocjln`, §2):

| Engine | `$ocjln` | Crossfade dispatch `$rldy4` | Character |
|---|---|---|---|
| **Standard / Low-latency legato** | `0` | `4` → `select($rldy4) case 4` (line 20163) | OD prefire + **single-stage, fixed-per-velocity-zone** crossfade |
| **Expressive IOI-adaptive legato** | `6` | `1` → `select($rldy4) case 1` (line 20264) | OD prefire + **two-stage swell** crossfade, everything IOI-interpolated (`$a3zg3/$igmiu/$x444h`), optional portamento glide |

`$rldy4` is forced at note time: `if ($ocjln=6) $rldy4:=1 else $rldy4:=4`
(lines 17499-17503). The elaborate `$a3zg3/$igmiu/$x444h/$ruv02`
IOI-interpolation block (lines 18111-18470) is gated **`if ($ocjln=6)`**
(line 18111); the standard legato uses fixed per-zone fade times instead.
This document covers **both**, with the two-stage engine (`case 1`, the
focus of the task) in full.

---

## 1. Variable glossary

`$`=int, `%`=int array, `@`=string, `!`=string array. "Label" = the
`set_text` UI label. Defaults are the script's `:=`; **(shipped)** is
`persistent_1.tsv`.

### 1.1 Mode / state selectors

| Id | Meaning | Evidence |
|---|---|---|
| `$ocjln` | **Articulation / playing mode.** `0`=Sustain/legato (low-lat/expressive submodes via `$tuu20`), `1`=Staccato/Staccatissimo (`$k1phr` variant), `2`/`3`/`4`=short/marcato variants, `5`=Spiccato, `6`=**Expressive IOI legato**, `7`=Pizzicato. Set from the keyswitch CC `$cei4x` value ranges. | decl `script_0.ksp:24`; set `21859-21953`; dispatch `10814`,`21862`… |
| `$sxwuz` | Keyswitch group base (`$ENGINE_PAR_START_CRITERIA_CONTROLLER` of group "KS"); `6`/`7` = special "divisi/ensemble" states that gate many branches. | `210-211` |
| `$tuu20` | **Legato latency mode**: `1`=Expressive (label "Expressive"/"Standard"), `0`=Low-latency. Selects EX vs LL anchor tables in `legtrans_OD` and the LT-offset base. Default `1`; **(shipped 0)**. Overwritten by keyswitch (`21863`=LL for KS 0-5, `21878`=EX for KS 6-10). | decl `2496`; `legtrans_OD` `6962`; `21864/21879` |
| `$jc35m` | **Legato engaged** (master legato on/off). OD prefire + transition only run when `=1`. Default/shipped `1`. | decl `2362`; gate `6960` |
| `$rldy4` | Crossfade-engine menu: `1`="Legato mode A" (two-stage), `4`="Legato mode B" (fixed). Forced per `$ocjln` at note time. | menu `4757-4760`; force `17499-17503`; dispatch `20162` |
| `$xp1ku` | **Velocity zone** 1/2/3 from `$EVENT_VELOCITY` vs `$eluxs`/`$0uhls`. `$4i3zj` = saved copy. | `10786-10797` |
| `$kbqnb` | "Hard/fast" flag inside `$ocjln=6`: `1` when `IOI > $hx3nl AND vel > $qm4n3` → uses the *second* IOI-anchor table (a harder/faster crossfade set). Else `0`. | `18114-18118` |
| `$cztyy` | **Ping-pong slot index** (0/1). `$cztyy` = current voice, `1-$cztyy` = previous (outgoing). Flipped `$cztyy := 1-$cztyy` right before playing the new transition. | init `4722`; flip `18645` |
| `$4pcsa` | Mono/retrigger state: `0` idle, `1` note held, `2` "already in transition" (legato-of-legato). Gates first-note vs transition branch (`if %f4tl5[$cztyy]=0 and $4pcsa<2`, line 17526). | `1206`,`8439-8447`,`20424` |
| `$gfkjw` | "Same note as last" (re-bow) marker; `= $EVENT_NOTE` when repeating a pitch, else `-1`. Selects re-bow sample offsets. | `18097-18101`,`20429-20438` |
| `$3oyab` | "Standard vs Expressive" sub-toggle for `$ocjln=0` (affects `$tuu20` label). | `21874-21880` |
| `$e0ydr`,`$hlypb` | Debug-overlay enables (gate `set_control_par_str` calls). **Ignore for the algorithm** — they only drive on-screen text. | throughout |

### 1.2 Voice / note-tracking arrays

| Id | Meaning | Evidence |
|---|---|---|
| `%jcxqm[0..1]` | **Legato-transition voice** per ping-pong slot (the bow-change sample). Played `play_note(note,$jpvdn,$1fvjk*1000,0)`. Retired via `fade_out(%jcxqm[1-$cztyy],…,1)` + `note_off`. | play `17596`,`18742`; retire `20311`,`20346` |
| `%2ezeo[0..1]` | **Second transition layer** (paired mic / connecting sample), played alongside `%jcxqm` with offset `$wgvbf`; faded/retired in lockstep. | `19118-19123`,`20312-20314` |
| `%grhcg[note + mic*100]` | **Held main sustain voice**, indexed by note **plus mic position ×100** (mics 0-4 → +0/+100/+200/+300/+400). Played `play_note(note,$jabns,0,-1)` (dur `-1` = sustain forever). | play `12943`,`13171`,`15909`,`17764`; loop `20316-20330` |
| `%u1bjb[note + mic*100]` | **Second sustain/dynamic layer** (the other CC1 dynamic crossfade layer), same 5-mic indexing as `%grhcg`. | play `13253`,`15965`; loop `20321-20335` |
| `%ftriy[note]` | **Supplementary transition/bow-noise sample** played when `$aguy2=1` (LT-sample enable), with `$1fvjk` offset; retired with `$tdjzq/$3ivkj/$u0t23` fades. | play `12699`; retire `12165-12197` |
| `%1wcdh[note]` | Voice that inherits the incoming event's pitch-bend / tune; faded then re-armed at transition. | `12290-12308` |
| `%2t4y1[0..1]` | **Note→slot map**: the MIDI note currently held in each ping-pong slot. `abs(%2t4y1[c]-%2t4y1[1-c])` = transition interval; sign = direction. | `17584`,`18730`; interval `20279` |
| `%ptjbp/%ftvfx/%f4tl5[slot]` | Per-slot: velocity / velocity-zone / event-id of the note in that slot. `%f4tl5[$cztyy]=0` ⇒ slot empty ⇒ first note. | `17585-17588` |
| `%zrs2k[slot]` | Accumulated applied tune (millicents) for that slot's voice — used so `change_tune` glides are applied *relative* (delta from last frame). | `20373-20381` |
| `%i35so[note]`,`%icbpd/%ujbzw[note]` | Per-note last-onset timestamps (`$ENGINE_UPTIME`) for the attack-transient envelope and repetition timing. | `10812-10813`,`12717` |
| `%CC[...]` , `$bgt3k` | Live CC table; `$bgt3k` = **dynamics CC value** (CC1/expression, default 127) copied from `%CC[$v0ejk]`. Drives velocity-compression `%fizyd` and dynamic-layer choice. | `4071-4081`,`21838` |

### 1.3 Crossfade timing (the two-stage engine, `$ocjln=6`)

| Id | Label / meaning | Default | Shipped | Evidence |
|---|---|---|---|---|
| `$a3zg3` | **"XTime"** — total crossfade time (ms). IOI-interpolated per zone. | 250 | **225** | knob `5334`; interp `18347-18377` |
| `$igmiu` | **"Atk Fade"** — stage-1/stage-2 split **%** (how much of the fade is the fast stage-1 vs the slow stage-2 swell). IOI-interpolated. | 50 | 50 | knob `5341`; interp `18126-18313` |
| `$x444h` | **"Node Vol"** — crossfade shaping **divisor** for stage-1 fade-in length. IOI-interpolated. | 90 | 90 | knob `5348`; interp `18316-18346` |
| `$0cqdq` | **"Tick"** — the crossfade `while`-loop tick period (µs). Loop `wait($0cqdq)`. | 1000 | 1000 | edit `4693` |
| `$qcmdq` | **"Rls Fade"** — scales the outgoing-voice `note_off` timing (`$vlmkl`). `0` ⇒ note_off at end of stage-1. | 0 | 0 | knob `5355` |
| `$hzl4j` | Derived: `1000*$a3zg3` (crossfade time in µs). | — | — | `20265` |
| `$mlnoy` | Derived stage-1 **fade-in length**: `$hzl4j*$igmiu/$x444h`. | — | — | `20266` |
| `$rixqv` | Derived stage-2 **swell fade-in length**: `$hzl4j*(100-$igmiu)/100`. | — | — | `20267` |
| `$vlmkl` | Derived outgoing **note_off tick count**: `$hzl4j/$0cqdq`, then `*$qcmdq/100` if `$qcmdq>0`. | — | — | `20268`,`20271` |
| `$qsazz` | Derived: `$vlmkl*$igmiu/100` — tick count of the whole loop's stage-1 portion (seeds `$nr244`). | — | — | `20269` |
| `$wvsi3` | Derived stage-2 tick count: `$rixqv/$0cqdq` (or `0` if `$igmiu=100`). | — | — | `20273-20277` |
| `$u44ap` | Loop counter for outgoing note_off (init `$vlmkl`). | — | — | `20337`,`20344` |
| `$nr244` | Loop counter that triggers **stage-2 swell** when it hits 0 (init `$qsazz`, then reloaded with `$wvsi3`). | — | — | `20338`,`20351-20366` |
| `$kxgro` | "Crossfade loop is running" flag (1 during the `while`). | — | — | `20339`,`20403` |

### 1.4 Overlap-Delay (prefire) — `legtrans_OD` (lines 6959-7180)

| Id | Meaning | Evidence |
|---|---|---|
| `$b0n3s` | **OD = Overlap-Delay** (ms). `wait($b0n3s*1000)` before the transition fires. IOI-interpolated between per-velocity-zone anchors; label `@pqxl3` = `"EX …"`/`"LL …"`. | `6959-7180` |
| `$ftvnh` / `$d5ans` | **IOI** (inter-onset interval, ms) = `$e44qy-$q4igo` (now − previous-note time). The interpolation X-axis everywhere. | `12068`,`6961` |
| LL IOI thresholds | `$deey3`=75, `$fxiox`=100, `$jystg`=800, `$zvaet`=1100 (A/B/C/D breakpoints, ms, **shipped**). | `1791`,`6965-6994` |
| EX IOI thresholds | `$g45yq`=200, `$bwkdm`=300, `$waq1e`=800, `$whtm2`=800 (**shipped**). | `7048-7078` |
| EX OD anchors z1 | `$kadcz,$nug53,$tfwqt,$xvurx` (script default `0,42,83,117`; shipped `83,0,0,0`). z2:`$lkn4u,$ff0lo,$vl1ol,$qqqvj`; z3:`$yncub,$gxiln,$ta22b,$3uh30`. | `2216-`,`7051-7162` |
| LL OD anchors z1 | `$nbkqa,$mih5r,$yzpsq,$myv02` (shipped `77,0,0,0`). z2-3:`$55anl,$umt5l,$cffjr,$yeo2q`. | `6968-7039` |

### 1.5 LT-Offset (`$1fvjk`) — transition sample start (lines 12316-12386, 18648-18726)

| Id | Meaning | Evidence |
|---|---|---|
| `$1fvjk` | **"LT Offset"** — how far *into* the transition sample the voice starts (ms; passed as `play_note(...,$1fvjk*1000,...)`). | play `12699`,`17596`,`18742` |
| EX legato bases (`$ocjln=0,$tuu20=1`) | z1 `$fjf3c`, z2 `$2p1wl`, z3 `$ywj0r`; then `$1fvjk := base + $b0n3s`. Also `set_controller(65,$tdvbm)`. | `12318-12327` |
| LL legato bases (`$ocjln=0,$tuu20=0`) | z1 `$ak2j4`, z2 `$ixzi1`, z3 `$cltif`; `$1fvjk := base + $b0n3s`; `set_controller(65,$h4cys)`. | `12328-12338` |
| `$ocjln=6` offsets | IOI-interpolated `< A/A-B/B-C/> C` anchors `$ggt00,$v0rbb,$5exar`; a second per-zone set `$fjftm/$uwsuq/$252zx` (18651), interval>12 `$p3wmm` (18671), re-bow `$knvx2/$zk0vu/$iufx3` (18682). | `12341-12382`,`18648-18726` |
| CC 65 | The **sample-start CC** the transition group's start-modulator reads (`$tdvbm` EX, `$h4cys` LL). Adding `$b0n3s` compensates the prefire so the transition is time-aligned. | `12326`,`12337` |

### 1.6 Volume / dynamics

| Id | Label / meaning | Default | Shipped | Evidence |
|---|---|---|---|---|
| `$3tsb0` | **"Vol."** — connected-sustain volume trim, in centi-dB×10: `-60` ⇒ **−6.0 dB**. Added to the connected note's `$1z3x0`. | −60 | −60 | knob `721`; apply `12753` |
| `$hy4uf` | **"Accent."** — transient accent boost (0-120 → 0-12 dB) added to the outgoing voice at the *start* of a stage-1 crossfade and removed after stage-1. NOT crossfade compensation. | 0 | 0 | knob `5360`; apply `20293-20308`; remove `20383-20418` |
| `$vxi3e` | Destination-sustain **dynamic-layer volume** from CC1 dynamic tables (`%2ng55[mic*6]`, `%j0rnl[mic]`, or `$3jpsu/$gy1gu`). Applied `change_vol(%grhcg[…], $vxi3e*100+$arhiq, 1)`. | — | — | `13040-13045`,`13173`,`15380` |
| `$arhiq` | CC1/velocity-derived dynamics volume delta (centi-dB). e.g. `(127-vel)*%lwgt4[…]/100`. | 0 | — | `11005`,`12773`,`13077` |
| `$1z3x0` | The **connected-note volume delta** actually applied (`change_vol($dtxpw,$1z3x0*100,1)`, line 12093). Built from the attack-transient envelope + `$3tsb0` (zones 1-2) or `$x0jlu` (zone 3). | — | — | `12716-12807` |
| `$4lqhx`,`$ee3a4` | Attack-transient envelope endpoints (centi-dB): "connected note starts quieter, recovers". `$4lqhx`=-60/-30, `$ee3a4`=-30/0. | | | `2428-2434` |
| `$xu41m`,`$c2hkn` | Attack-transient time windows (ms): `$xu41m`=250 (fast decay of the dip), `$c2hkn`=1000/2000 (slow recovery). | | | `2438-2444` |
| `$x0jlu` | **Zone-3** connected-note vol (used instead of `$3tsb0`, so hard attacks skip the −6 dB trim). | 0 | 0 | `1460` |
| `$dzd3m` | Short-note connected vol (−18 dB shipped) for `$ocjln` 2/3/4. | −60 | −180 | `2402` |
| `$eluxs`,`$0uhls` | Velocity-zone splits. Zone1 `0-$eluxs`, zone2 `$eluxs+1..$0uhls`, zone3 `$0uhls+1..127`. | 64 / 85 | **64 / 100** | `4097-4106` |
| `$bgt3k` / `$v0ejk` | Dynamics CC value / CC number (CC1). | 127 | — | `4071-4081`,`21838` |
| `$bduyw` | **Crossfade-the-held-sustain-mics** toggle. When `1`, the crossfade loop also fades `%grhcg`/`%u1bjb` (all 5 mics) out/in during the transition; when `0` the sustain layer is left untouched (its level rides CC1 only). | 1 | **0** | `5226`; `if ($bduyw=1)` `20315`,`20354`,`20386` |
| `$jvqtp` | **"Old out"** — fade-out (ms) of the previous transition voice `$dc22e` when a new legato note arrives. | 1 | **250** | `4955`; `18104-18106` |

### 1.7 Retire / short-transition fade lengths

| Id | Label / meaning | Default | Shipped | Evidence |
|---|---|---|---|---|
| `$fjtlu` | Retire fade (ms) for one velocity range. | 155 | **150** | `2717` |
| `$hbi2j` | Retire fade (ms), range 2. | 180 | **281** | `2722` |
| `$2ebzd` | Retire fade (ms), range 3. | 250 | **281** | `2727` |
| `$tdjzq/$3ivkj/$u0t23` | `%ftriy` supplementary-transition fade-out lengths (ms) per zone. | 20/20/20 | **550/500/500** | `5953-5964`; `12165-12176` |
| Case-4 fixed fades | Per-zone `fade_out`/`fade_in` times for the **standard** legato: `$wgsgw/$g4dbu`(z1), `$kntrn/$c2axk`(z2), `$eqtdi/$mxfsu`(z3), plus interval>12 (`$zyf4u/$33tym`), re-bow (`$k4w4g/$dl3zt`…), short (`$mhmq3/$m0nme`…). | — | — | `20163-20260` |

### 1.8 Portamento / legato-tuning glide (`$ocjln=6`)

| Id | Label / meaning | Default | Shipped | Evidence |
|---|---|---|---|---|
| `$upjkh` | Portamento glide on the **outgoing** voice (bends old pitch toward new). | 0 | **1** | `5386` |
| `$ma0b1` | Portamento glide on the **incoming** voice (bends new pitch up from old). | 0 | **1** | `5394` |
| `$1mwwo` | **"BTime"** — glide duration (ms). `$bz0g4 = $1mwwo*1000/$0cqdq` glide ticks. | 60 | 60 | `5367`; `20283` |
| `$ruv02` | **"Bend"** — glide depth (base millicents). IOI-interpolated when interval=1 (`$acia1/$lck0q…`, 18378-). | 8 | **10** | `5379` |
| `$i1kki` | **"Octave"** — depth scaling by interval size: `$jyttf += (($i1kki-10)*($gdty5-1)*$jyttf+55)/110`. `10`=no scaling. | 10 | 10 | `5374`; `20285-20287` |
| `$jyttf,$kw0xf,$un1ev,$j1oqg` | Glide ramp state: total bend, per-tick step, current outgoing/incoming detune (millicents). | — | — | `20284-20291`,`20370-20381` |
| `$gdty5` | Transition interval in semitones (`abs(%2t4y1[c]-%2t4y1[1-c])`). | — | — | `20279` |
| `$aguy2` | **LT-sample enable** — play the `%ftriy` supplementary transition sample. | 1 | 1 | `1436`; `12312` |

`$foyeb/$g4dbu`, `$v2a3s/$c2axk`, `$rgmo4/$mxfsu` etc. = per-zone
`$EVENT_PAR_2` "start-offset-modulation" packed values for the sustain
voice (mostly display/engine-par plumbing, not core to the fade math).

**UNKNOWN / not fully decoded** (not central to legato): `$foyeb` exact
units, `$takdp`/`$1th03` (divisi/section extras), `%d2gsb`/`%i3hei`/
`%pbnru`/`%ijm3b`/`%qm3zh`/`%nobn3` (per-articulation group-id tables — the
allow_group plumbing), `$s1fr2`/`$eaghu`/`$313em`-as-CC19 (attack-shape
CCs into Kontakt group modulators). These select *which* groups/mics play
but don't change the fade algorithm.

---

## 2. The `on note` handler (lines 10772-20770)

Pseudocode of the decision flow (debug branches omitted):

```
on note:
  if note not in playable range: return
  save note/vel/id
  if $qehof (bypass): ignore, exit

  # --- velocity zone ---
  if vel <= $eluxs:            $xp1ku = 1     # 0..64
  elif vel <= $0uhls:          $xp1ku = 2     # 65..100 (shipped)
  else:                        $xp1ku = 3     # 101..127

  ignore_event(id)             # script owns all playback; nothing plays directly

  # --- articulation dispatch on $ocjln (set by keyswitch CC $cei4x) ---
  if $ocjln in {0,2,3,4,6}:
     disallow_group(ALL); allow only this articulation's groups for the
        enabled mics ($qqwap/$uyrfn/$w40o5/$uy1rv/$ezse2 = 5 mic toggles),
        with a velocity-3 "hard" variant group set (%i3hei) when $x3o0p=1
     ...round-robin selection (SpiccRR1..4 / staccato RR / etc.)...

  # --- FIRST NOTE vs LEGATO TRANSITION ---
  $rldy4 = (if $ocjln==6 then 1 else 4)          # pick crossfade engine
  if %f4tl5[$cztyy]==0 and $4pcsa<2:             # slot empty => FIRST NOTE
     $1fvjk = 0                                  # start transition at sample 0
     %jcxqm[$cztyy] = play_note(note, $jpvdn, 0, 0)
     play sustain layer %grhcg[note+mic*100] (dur -1) for each enabled mic
     $4pcsa = 1
  else:                                          # LEGATO TRANSITION
     $ftvnh = now - prev_onset                   # IOI
     call legtrans_OD                            # WAIT the prefire (§4)
     compute $1fvjk (LT offset, §5) and set CC65 (sample-start)
     if $aguy2: %ftriy[note] = play_note(note,$ymtl2,$1fvjk*1000,0)  # bow sample
     compute connected-note volume $1z3x0 (attack env + $3tsb0 / $x0jlu)
     if $ocjln==6: compute $a3zg3,$igmiu,$x444h,$ruv02 by IOI-interpolation (§6)
     $cztyy = 1 - $cztyy                         # FLIP ping-pong
     record %2t4y1[$cztyy]=note, %ptjbp/%ftvfx/%f4tl5[$cztyy]=...
     %jcxqm[$cztyy] = play_note(note, $jpvdn, $1fvjk*1000, 0)
     %2ezeo[$cztyy] = play_note(..., $wgvbf*1000, 0)   # 2nd transition layer
     play/duck destination sustain %grhcg / %u1bjb (dynamic layer)
     run the CROSSFADE ENGINE (§3)
```

Key points a naive sampler gets wrong:
- **Nothing plays on the raw note event** — `ignore_event` then everything
  is script-driven `play_note` with explicit start offsets and manual fades.
- The **transition sample is a separate sample set** from the sustain, keyed
  by (interval bucket, direction = sign of `%2t4y1` diff, velocity zone,
  round-robin, mic) via the allow_group tables; the script selects the group
  and passes the **start offset** `$1fvjk` and the **attack-shape via CC** (17,
  18, 19, 29, 65) into the group's modulators.
- **Re-bow** (same pitch, `$gfkjw==note`, `$zs1l1=1`) uses its own offset set
  and a shorter transition (18678-18703).
- **Interval > 12 semitones** always uses the fixed ">12" offset/timing and
  skips IOI interpolation (`if $1e5yd>12`, 18669, 18195, 18305).

### Keyswitch → `$ocjln` (CC `$cei4x`, lines 21857-21953)

| CC value | `$ocjln` | Meaning |
|---|---|---|
| 0-5 | 0 | Legato, **Low-latency** (`$tuu20=0`) |
| 6-10 | 0 | Legato, **Expressive/Standard** (`$tuu20=1`, or 0 if `$3oyab`) |
| 11-30 | 1 | Staccatissimo/Staccato (`$k1phr` 0-3) |
| 31-45 | 7 | Pizzicato (`$doko0` 0-2) |
| 46-50 | 4 | short/marcato A |
| 51-55 | 3 | short/marcato B |
| 56-60 | 2 | short/marcato C |
| 61-65 | 5 | Spiccato |
| 66-75 | 6 | **Expressive IOI legato** (`$5kqeg` 0/1) |

(`script_0.ksp:162` mirrors `$ocjln = pgs_get_key_val(PGS_KS,1)` — the
keyswitch state is shared via a PGS key so all script slots agree.)

---

## 3. The two-stage crossfade engine (`select($rldy4) case 1`, lines 20264-20420)

This is the heart of the Expressive legato. Runs **inside the `on note`
callback** (Kontakt runs each callback on its own fiber; `wait()` yields).
Entered after the destination voice `%jcxqm[$cztyy]` has been played and the
OD wait has elapsed.

### 3.1 Derive the timings (20265-20277)

```
$hzl4j = 1000 * $a3zg3                       # total XFade, µs
$mlnoy = $hzl4j * $igmiu / $x444h            # stage-1 fade-in length (µs)
$rixqv = $hzl4j * (100 - $igmiu) / 100       # stage-2 swell fade-in length (µs)
$vlmkl = $hzl4j / $0cqdq                      # outgoing note_off, in ticks
if $qcmdq > 0: $vlmkl = $vlmkl * $qcmdq / 100
$qsazz = $vlmkl * $igmiu / 100                # stage-1 length, in ticks
$wvsi3 = (if $igmiu==100 then 0 else $rixqv / $0cqdq)   # stage-2 length, in ticks
```

Note `$mlnoy` uses **`$x444h` ("Node Vol", 90) as the divisor**, not 100 —
so stage-1 fade-in is *faster* than a linear `$igmiu%` split. This is the
CSS "attack shaping": the destination reaches most of its level quickly,
then swells the rest during stage 2.

### 3.2 Portamento ramp setup (20278-20292)

```
if $upjkh or $ma0b1:
   $gdty5 = |interval|
   if interval==0 or no-glide: $bz0g4 = 0
   else:
      $bz0g4  = $1mwwo*1000 / $0cqdq              # glide ticks (BTime)
      $jyttf  = $ruv02 * 1000                      # total bend, millicents
      if $i1kki != 10 and interval != 1:           # scale by interval size
         $jyttf += (($i1kki-10)*(interval-1)*$jyttf + 55) / 110
      if destination < source (going down): $jyttf = -$jyttf
      $kw0xf  = 1000*$jyttf / $bz0g4               # per-tick step
```

### 3.3 Accent kick-in (20293-20309) — only if `$hy4uf>0`

At the very start, **boost the outgoing** transition voice (and, if
`$bduyw`, the outgoing sustain mics + `%u1bjb`) by `$hy4uf*100` centi-dB,
and set `$2f0hp=1` (accent-applied flag). This adds a transient bump on the
old note as the bow changes. Removed later (§3.6).

### 3.4 Launch the crossfade (20311-20339)

```
fade_out(%jcxqm[1-$cztyy], $hzl4j, stop=1)       # retire OLD transition
fade_out(%2ezeo[1-$cztyy], $hzl4j, 1)            # retire OLD 2nd layer
fade_in (%jcxqm[$cztyy],  $mlnoy)                # stage-1 fade-in NEW
fade_in (%2ezeo[$cztyy],  $mlnoy)
if $bduyw:                                        # (shipped OFF)
   for mic in 0..4:
      fade_out(%grhcg[old+mic*100], $hzl4j, 1)   # retire OLD sustain mics
      fade_out(%u1bjb[old+mic*100], $hzl4j, 1)
      fade_in (%grhcg[new+mic*100], $mlnoy)      # stage-1 fade-in NEW sustain
      fade_in (%u1bjb[new+mic*100], $mlnoy)
$u44ap = $vlmkl        # note_off countdown
$nr244 = $qsazz        # stage-2 trigger countdown
$kxgro = 1
```

The `fade_out(...,1)` means "fade then auto-stop the voice" — but the actual
`note_off` for the outgoing transition is issued explicitly at `$u44ap==0`
below (so its release/ring can be controlled by `$qcmdq`).

### 3.5 The `while` loop (20340-20402)

```
while $u44ap>0 or $nr244>0 or $bz0g4>0:
   dec($u44ap); dec($nr244); dec($bz0g4)

   # (a) retire the outgoing voice
   if $u44ap==0 and %jcxqm[1-$cztyy]!=0:
      note_off(%jcxqm[1-$cztyy]); %u2pkp[1-$cztyy]=0; $u44ap=0

   # (b) STAGE-2 swell: when stage-1 ticks exhausted, start the slow fade-in
   if $nr244==0 and $wvsi3>0:
      fade_in(%jcxqm[$cztyy], $rixqv)
      fade_in(%2ezeo[$cztyy], $rixqv)
      if $bduyw: for mic in 0..4: fade_in(%grhcg[new+mic*100],$rixqv)
                                  fade_in(%u1bjb[new+mic*100],$rixqv)
      $nr244 = $wvsi3      # run for the stage-2 tick count
      $wvsi3 = 0          # (one-shot)

   # (c) portamento glide, per tick
   if $bz0g4 >= 0:
      $un1ev = $jyttf - ($kw0xf*$bz0g4 + 500)/1000      # outgoing detune
      $j1oqg =        - ($kw0xf*$bz0g4 + 500)/1000      # incoming detune
      if $upjkh: change_tune(%jcxqm[1-$cztyy], $un1ev - %zrs2k[1-$cztyy], rel=1)
                 change_tune(%2ezeo[1-$cztyy], ... )
      %zrs2k[1-$cztyy] = $un1ev
      if $ma0b1: change_tune(%jcxqm[$cztyy],  $j1oqg - %zrs2k[$cztyy], rel=1)
                 change_tune(%2ezeo[$cztyy],  ... )
      %zrs2k[$cztyy] = $j1oqg
   else:
      # (d) glide finished -> remove the accent boost from the outgoing voice
      if $hy4uf>0:
         change_vol(%jcxqm[1-$cztyy], -$hy4uf*100, rel=1)
         change_vol(%2ezeo[1-$cztyy], -$hy4uf*100, rel=1)
         if $bduyw: for mic: change_vol(%grhcg[old+mic*100],-$hy4uf*100,1)
                             change_vol(%u1bjb[old+mic*100],-$hy4uf*100,1)
         $2f0hp = 0

   wait($0cqdq)          # tick = "Tick" knob, 1000µs default
$kxgro = 0
```

So the fade is genuinely **two-stage**: a quick equal-ish rise to
`(igmiu/x444h)` of full over `$mlnoy`, then when the stage-1 tick budget
`$nr244` runs out, a **second slow swell** over `$rixqv` to the top. The
outgoing voice is faded over the full `$hzl4j` and hard `note_off`-ed at
`$u44ap`, which (with `$qcmdq=0`) is the end of stage-1.

### 3.6 Accent cleanup after the loop (20404-20419)

```
if $2f0hp==1:     # accent boost never got removed inside the loop
   change_vol(%jcxqm[1-$cztyy], -$hy4uf*100, 1)
   if $bduyw: remove from all sustain mics + %u1bjb
   change_vol(%2ezeo[1-$cztyy], -$hy4uf*100, 1)
```

### 3.7 The single-stage engine (`case 4`, `$ocjln=0`, lines 20163-20260)

For the **standard** legato there is no `while` loop — just paired
`fade_out(old)/fade_in(new)` with **fixed per-velocity-zone times**, chosen
by a cascade of conditions:
- normal transition, same-note (`$gfkjw`): `$wgsgw/$g4dbu` (z1), `$kntrn/$c2axk` (z2), `$eqtdi/$mxfsu` (z3);
- other short modes: `$k4w4g/$dl3zt`, `$m51z0/$bqh3a`, `$r5jeq/$r1bnj`;
- interval > 12: `$zyf4u/$33tym`;
- re-bow / retrigger (`$4pcsa==2`): `$1qyra/$lwnu5`;
- else per-zone `$mhmq3/$m0nme`, `$qkgxg/$niktq`, `$yn5xm/$v3hww`, with a
  `$tkgsb` (divisi) override using `$evjnh`.

Special case: `if $rldy4==1 and $a3zg3==0` (XTime knob at 0) the engine
degenerates to an instant `note_off` of the old voice (20155-20160).

---

## 4. Overlap-Delay / prefire (`legtrans_OD`, 6959-7180)

```
function legtrans_OD:
  if $jc35m != 1: return               # legato off -> no prefire
  $d5ans = $ftvnh                       # IOI
  if $tuu20==0:  table = LL,  thresholds = $deey3,$fxiox,$jystg,$zvaet
  else:          table = EX,  thresholds = $g45yq,$bwkdm,$waq1e,$whtm2
  pick per-velocity-zone anchor row ($xp1ku 1 / 2-or-3 for LL, 1 / 2 / 3 for EX)
  $b0n3s = piecewise-LINEAR interpolate the anchors over the IOI bucket
           (< A -> flat = anchor0; A/B, B/C, C/D -> lerp; > D -> flat = anchor4)
  if $b0n3s > 0: wait($b0n3s * 1000)    # <<< THE CSS LATENCY
```

- `$tuu20` picks **Expressive** vs **Low-latency** anchor tables (debug label
  `"EX"` / `"LL"`, 6967/7050).
- Interpolation is standard: `out = a0 + (a1-a0)*(IOI-t0)/(t1-t0)`.
- Velocity zone selects the anchor *row* (fast notes get less prefire).
- Shipped example (EX, zone 1): flat ~83 ms below the A threshold (200 ms
  IOI). LL zone 1: ~77 ms. The task's "Expressive 333/250/100, Low-latency
  150/100" figures are the **per-zone anchor values in a typical CSS preset**
  — they live in these `$kadcz…`/`$nbkqa…` edits, which are user-editable and
  differ between the script default, the shipped persistent state, and the
  marketing presets. Treat the *mechanism* (IOI-lerp of per-zone anchors,
  gated on Expressive/Low-latency) as authoritative; treat any single number
  as a preset value.

Called at 12152 and 17996, i.e. **before** the transition sample is played,
so the `wait` delays the whole transition.

---

## 5. LT-Offset (`$1fvjk`) — start the transition partway in (12316-12386, 18648-18726)

The transition sample is **not** started at sample 0. Instead:

```
$ocjln==0 (standard legato):
   base = per-zone { EX: $fjf3c/$2p1wl/$ywj0r ; LL: $ak2j4/$ixzi1/$cltif }
   $1fvjk = base + $b0n3s              # add the OD we just waited
   set_controller(65, $tdvbm or $h4cys)   # tell the group's start-modulator

$ocjln==6 (expressive): IOI-interpolated offset (< A/A-B/B-C/> C anchors
   $ggt00/$v0rbb/$5exar), or the interval>12 / re-bow fixed offsets.

play_note(note, vel, $1fvjk*1000 /*µs offset*/, 0)
```

**Why:** because the transition was delayed by `$b0n3s` (OD), starting the
sample `$b0n3s` ms in keeps the *musical* moment of the bow-change aligned to
the beat. This "prefire + skip" pairing is the single most important thing to
replicate. CC 65 is the wire that carries the offset into Kontakt's group
start modulator; a Rust engine just seeks the sample read head to
`$1fvjk` ms.

---

## 6. IOI interpolation of the crossfade parameters (`$ocjln==6`, 18111-18470)

Inside the transition branch, for the expressive engine, `$a3zg3` (XTime),
`$igmiu` (Atk Fade split), `$x444h` (Node Vol divisor) and `$ruv02` (Bend)
are each recomputed from the IOI `$d5ans` using per-IOI-bucket anchors, and
there are **two anchor tables** selected by `$kbqnb` (soft vs hard/fast):

```
$1e5yd = |interval|
$kbqnb = (IOI > $hx3nl and vel > $qm4n3) ? 1 : 0     # hard/fast?

if $1e5yd > 12:   use the fixed ">12" values ($qak4x/$rqqqm or $3vz54/$pforq)
else:             piecewise-lerp:
   # $igmiu (Atk Fade), $313em (attack-shape CC19), interpolated together
   IOI < A     -> flat  ($ueewd/$bamzk         | $hajd5/$bdiws)
   A <= IOI < B -> lerp  (..$lcreu/$gxklu..     | ..$aplkl/$dudzw..)
   B <= IOI < C -> lerp  (..$lodpt/$4vedg..     | ..$scidb/$c55bn..)
   IOI >= C     -> flat  ($lodpt/$4vedg          | $scidb/$c55bn)
   # $x444h (Node Vol): flat/lerp over $jlgbx/$kcnco/$p5rr1 with $flfzo/$uutz4/$owvdm
   # $a3zg3 (XTime):    flat/lerp over $3kfur/$jxo2x/$5e3jr with $igtvr/$sfhq0/$je4dz
   # $ruv02 (Bend, only when interval==1): $acia1/$lck0q/$5ggch over $ylgac/$xhiq1/$jldrb
```

The `$kbqnb=0` table uses anchors `$ueewd/$qak4x/$bamzk/$rqqqm/$lcreu/…`;
the `$kbqnb=1` table uses `$hajd5/$3vz54/$bdiws/$pforq/$aplkl/…`. Both are
piecewise-linear in IOI with breakpoints A/B/C. Shipped anchor values live in
`persistent_1.tsv`; the *structure* is what matters.

Net effect: **slow playing → longer XTime, bigger swell, more prefire, more
bend; fast/hard playing → short XTime, near-instant crossfade, little
prefire.** This IOI-adaptivity is the second thing a naive sampler misses.

---

## 7. Volume / dynamics model

### 7.1 Sustain level = CC1 dynamic layers

The held sustain is two crossfaded layers `%grhcg` (mp/soft) and `%u1bjb`
(f/loud), each in 5 mic copies (`+mic*100`). Their relative volume is set
from the **dynamics CC** (`$bgt3k`, = CC1/expression) via lookup tables
`%2ng55[mic*6]` / `%j0rnl[mic]` → `$vxi3e`, applied as
`change_vol(%grhcg[note+mic*100], $vxi3e*100 + $arhiq, rel=1)`
(13173, 13255, 15380). `$arhiq` is the velocity-derived component
(`(threshold-vel)*weight/100`). CSS's smooth dynamics = **continuous CC1
volume/filter crossfade between two full sustain samplesets**, independent
of the legato crossfade.

### 7.2 The −6 dB connected-sustain trim (`$3tsb0`)

For a **connected** (legato) note in velocity **zones 1 and 2**, the
connected note's volume delta is:

```
$1z3x0 = attack_transient_env + $3tsb0      # $3tsb0 = -60 => -6.0 dB
```
(line 12753). In **zone 3** (hard attack) the trim is replaced by `$x0jlu`
(=0) — i.e. **hard-attacked connected notes are NOT trimmed** (12735-12736).

**When applied:** only on legato *transitions* (`$ocjln==0` connected path,
12715-12770). The **first note** of a phrase goes through the
`%f4tl5[$cztyy]==0` branch (17526) where `$1fvjk=0` and this connected-trim
math is skipped — so the first note is at full level and each *subsequent*
connected note (zones 1-2) is played ~6 dB softer to keep the legato smooth.

### 7.3 Attack-transient envelope (12716-12731)

Within `$xu41m` (250 ms) of the previous onset the connected note is dipped
and recovers over `$c2hkn` (1000-2000 ms), interpolating between `$4lqhx`
and `$ee3a4`. This makes rapid re-articulations progressively quieter/softer
(prevents machine-gunning), on top of the flat `$3tsb0` trim.

### 7.4 Accent (`$hy4uf`) — see §3.3/3.6. Default 0 (off).

### 7.5 Output/makeup gain

Per-mic and master gains are ordinary Kontakt mixer/`set_engine_par`
volumes set in the mixer UI (`show_mixer`, 8094) — not part of the legato
math. `$1z3x0` is applied to the live voice with
`change_vol($dtxpw,$1z3x0*100,1)` (12093).

---

## 8. Timing tables (decoded numbers)

**Velocity zones** (`$eluxs`/`$0uhls`, shipped 64 / 100):
zone1 vel 0-64, zone2 65-100, zone3 101-127.

**IOI thresholds (ms)** for the OD interpolation (shipped):
- Low-latency (`$tuu20=0`): A=75, B=100, C=800, D=1100.
- Expressive (`$tuu20=1`): A=200, B=300, C=800, D=800.

**OD anchor rows** `$b0n3s` (ms) — script defaults show the intended shape
(shipped state is flatter):
- EX zone1: 0 / 42 / 83 / 117 (`$kadcz/$nug53/$tfwqt/$xvurx`).
- LL zone1: `$nbkqa` (77 shipped) …

**Crossfade (Expressive) — knobs:** `$a3zg3` XTime 225 ms (shipped),
`$igmiu` Atk-Fade split 50 %, `$x444h` Node-Vol divisor 90, `$0cqdq` Tick
1000 µs, `$qcmdq` Rls-Fade 0.
- stage-1 fade-in `$mlnoy = 225000*50/90 = 125 000 µs = 125 ms`
- stage-2 swell `$rixqv = 225000*(100-50)/100 = 112 500 µs = 112.5 ms`
- note_off `$vlmkl = 225000/1000 = 225 ticks`; stage-1 `$qsazz = 225*50/100
  = 112` ticks; stage-2 `$wvsi3 = 112500/1000 = 112` ticks.

**LT-Offset:** base (per zone) + `$b0n3s`; e.g. shipped OD ~83 ms means the
transition sample starts ~83 ms + base in.

**Retire fades (ms, shipped):** `$fjtlu`=150, `$hbi2j`=281, `$2ebzd`=281;
supplementary `%ftriy` fades `$tdjzq/$3ivkj/$u0t23` = 550/500/500.

**Connected trim:** `$3tsb0` = −6.0 dB (zones 1-2), `$x0jlu` = 0 dB (zone 3),
`$dzd3m` = −18 dB (short-note modes 2/3/4, shipped).

**Portamento (shipped):** enabled both directions (`$upjkh=1,$ma0b1=1`),
`$1mwwo` BTime 60 ms, `$ruv02` Bend 10, `$i1kki` Octave-scale 10 (none).

---

## 9. What a Rust engine MUST replicate

- **Prefire / Overlap-Delay** before every legato transition: wait
  `$b0n3s` ms, IOI-interpolated between per-velocity-zone anchors, table
  chosen by Expressive vs Low-latency. *This is the CSS "late" feel.*
  (`legtrans_OD`.)
- **Start-offset skip**: play the transition sample seeked in by
  `$1fvjk = base + $b0n3s` ms so the bow-change lands on the beat despite
  the prefire. (`$1fvjk`, CC65.)
- **Ping-pong two-voice architecture**: slot `$cztyy` / `1-$cztyy`; the new
  transition voice fades in while the previous fades out and is
  `note_off`-ed. (`%jcxqm`, `$cztyy`.)
- **Two-stage destination swell** (expressive engine): fast stage-1 rise to
  `igmiu/x444h` of level over `$mlnoy`, then a slow stage-2 swell to full
  over `$rixqv` triggered when the stage-1 tick budget `$nr244` expires.
  (`$mlnoy/$rixqv/$qsazz/$wvsi3`.) Note the **`$x444h` divisor (90, not
  100)** makes stage-1 overshoot the linear split.
- **Single-stage fixed crossfade** (standard engine): per-velocity-zone
  fixed fade-out/fade-in times, no swell. Distinct code path (`case 4`).
- **Velocity zones (3)** drive: which sample groups (soft/hard),
  prefire/offset/fade anchor rows, and the connected-trim exemption.
  (`$xp1ku`, `$eluxs/$0uhls`.)
- **IOI-adaptivity** (expressive): XTime `$a3zg3`, split `$igmiu`, shaping
  `$x444h`, bend `$ruv02` all interpolated from IOI, with a separate
  hard/fast anchor table (`$kbqnb`). Interval > 12 st bypasses interpolation.
- **−6 dB connected-sustain trim** on legato notes in velocity zones 1-2,
  **exempting** the first note of a phrase and hard (zone-3) attacks.
  (`$3tsb0`, `$x0jlu`.)
- **Attack-transient envelope**: rapid re-articulations dipped/softened over
  `$xu41m`/`$c2hkn` between `$4lqhx`/`$ee3a4`. (Prevents machine-gunning.)
- **CC1 continuous dynamics**: two full sustain samplesets (`%grhcg`,
  `%u1bjb`) × 5 mics, volume-crossfaded by the dynamics CC — independent of
  and simultaneous with the legato transition crossfade.
- **Portamento glide** (optional): per-tick `change_tune` ramp on outgoing
  and/or incoming voice over `$1mwwo` ms, depth `$ruv02` scaled by interval
  (`$i1kki`), direction = sign of interval. (`$upjkh/$ma0b1/$jyttf/$kw0xf`.)
- **Accent** (optional, off by default): `$hy4uf` dB transient boost on the
  outgoing voice at bow-change, removed after the glide/stage-1.
- **Re-bow** (repeated pitch) and **interval > 12** are special-cased with
  their own offsets and shorter/fixed transitions.
- **Round-robin + mic routing** via allow_group tables per articulation and
  the 5 mic enables; a Rust engine maps these to (interval bucket,
  direction, velocity zone, RR index, mic) sample selection.

---

## 10. Open / partially-decoded items

- Exact per-preset **OD and IOI-anchor numbers** vary between script default,
  `persistent_1.tsv`, and the shipped GUI presets — decode per instrument
  from persistent state rather than hard-coding.
- The `%d2gsb/%i3hei/%pbnru/%ijm3b/%qm3zh/%nobn3` **group-id tables** map
  (articulation, mic, velocity/RR) → Kontakt group index; the *routing*
  logic is understood, the concrete group numbers are instrument data
  (see `groups.tsv`/`zones.tsv` in the extract).
- Attack-shape CCs 17/18/19/29 feed Kontakt group envelope/filter modulators
  (`$313em`, `$eaghu`, `$ipbym`); they shape the transition sample's
  amp/filter attack but aren't part of the fade *scheduling* math.
- `$ocjln==6` vs `$ocjln==0`: both are "legato" articulations in the CSS
  keyswitch map; which marketing name each corresponds to
  ("Sustain"/"Legato") was not confirmed from the script and is left
  unnamed here to avoid guessing.

---

## 11. Voice lifecycle / note-off (decoded round 2)

Round-2 decode of what STOPS/FADES each voice and at what level each
rings. Same sources: `script_1.ksp` line numbers, shipped numbers from
`persistent_1.tsv`, group data from `groups.json`. Everything below was
re-verified against the file — including a structural (if/else-matched)
trace of the whole `on note` / `on release` nesting.

### 11.0 CORRECTION — the 12xxx region is CHORD mode, not legato

`$ra4sw` is the **"Legato / Chord" mode menu** (`1`=Legato, `0`=Chord;
decl + items 5699-5704). `on note` splits on it:

- `if ($ra4sw=0)` (12063) → **CHORD (polyphonic) path, 12064-17449.**
  Per-note voices `%1wcdh`(body)/`%auysb`(MIDI id)/`%ftriy`(LT), the
  same-note re-strike ("connected") logic at 12144, and the `$3tsb0`
  math all live HERE.
- `else` (17450) → **LEGATO (mono) path, 17450-20628** — first-note
  branch `if (%f4tl5[$cztyy]=0 and $4pcsa<2)` 17526-17959, transition
  branch 17960-20423.

Consequences for earlier sections of this doc:

- **`$3tsb0` (−6.0 dB) is CHORD-MODE ONLY.** Its UI label block is
  literally `"Chord mode SUS"` / `"Vol."` (719-722). It is applied to
  the chord body voice (12284→12289), the chord LT voice
  (`$1z3x0 := $1z3x0+$3tsb0`, 12753), chord sustains (12945, 13004) and
  chord release-sample gain (21222, debug string "ChordVol."). It never
  appears in the mono legato path. §7.2's "connected-sustain trim on
  legato notes" is a chord-mode rule; **do NOT apply −6 dB to legato
  transition voices** (answers Q1's consequence: no).
- §7.2/§7.5's cited line 12093/12753 volume math is the chord path; the
  legato-path equivalents are at 19881-20093 (below).

### 11.1 Q1 — `$dtxpw` is the mono-legato supplementary LT voice

`$dtxpw` (decl 1427) is a **scalar** voice id — the legato-mode twin of
chord-mode's per-note `%ftriy[note]`. One exists at a time.

- **Played** only in the legato **transition** branch, inside
  `if ($aguy2=1)` (19177-20154):
  `$dtxpw := play_note($iikoh,$o1awo,$1fvjk*1000,0)` (19866), then
  `EVENT_PAR_3 := 99` (19867 — its own release callback is a no-op) and
  `MARK_1` (19868 — so note-off `by_marks` fades catch it).
- **What it plays** (19472-19476): upward interval — note
  `$iikoh = %2t4y1[1-$cztyy]` (source note), velocity
  `$o1awo = $1e5yd` (=interval, 1-12); downward —
  `$iikoh = %2t4y1[$cztyy]`, `$o1awo = $1e5yd+12` (13-24). Re-bow:
  note = `$EVENT_NOTE`, vel = RR 30-32 (19779-19785). I.e. the
  **interval-keyed legato-transition sample** — velocity encodes
  interval+direction, groups from the `%4j3ee` allow-table
  (19249-19420, `$ocjln=0`). The main `%jcxqm[new]` voice played
  earlier at 18742 is the **destination body** (sustain groups), played
  muted (`fade_out(...,1,0)` at 18746) and faded in by the crossfade
  engine.
- **`change_vol($dtxpw,$1z3x0*100,1)` (20093) applies the LT-voice trim
  to `$dtxpw` ONLY** — not to `%jcxqm`, not to any sustain, not to the
  "whole event chain". `$1z3x0` here is:
  - `$ocjln=0` (20038-20086): the attack-transient env only —
    `$4lqhx`=−30, `$ee3a4`=0, `$xu41m`=250, `$c2hkn`=2000 (shipped) ⇒
    **−3.0 dB if this note comes ≤250 ms after the previous onset, else
    0 dB**. Zone-3 exemption `$x0jlu` requires `$shybn=0`; shipped
    `$shybn=1`, so zone 3 uses the same env (unlike chord mode's
    unconditional zone-3 exemption at 12750).
  - `$ocjln=6` (19884-20019): "LT Vol" IOI-lerp, breakpoints
    `$14fyb`=150/`$3zzii`=300/`$lvqx4`=500 ms; anchors interval≤2:
    `$ycnsk`=−90→`$nqjls`=0→`$us1ps`=0 (**−9.0 dB fast → 0 dB slow**);
    interval>2: `$0uoxp`=−15→`$i0s2b`=0→`$fbfjs`=0 (−1.5 dB → 0). Plus
    vel-vol `%lwgt4[51]` — computed from knob `$0kd5m` ("LT Vel.Vol.",
    2968-2970), **shipped 0 ⇒ off** (9168-9172, `$vtafn`=0 too).
  - `$ocjln` 2/3/4 (20020-20037): `$1z3x0 := $dzd3m` = **−18.0 dB**,
    plus an unconditional `fade_out($dtxpw,$53oxo*1000,1)` = 1000 ms
    (20104-20105).
- **Re-bow special-case** (interval = 0; 20112-20139): the LT voice is
  started SILENT (`fade_out($dtxpw,1,0)`, 20113) and faded in over
  `$wtxmh`/`$iluqo`/`$thsyv` = **1 / 1 / 50 ms** per velocity zone
  (packed-par variants `$ohdjc`/`$5wzgk`/`$t5nah` all ship 0 → plain
  `fade_in`).

### 11.2 Q2 — the `on release` handler (20771-21759), full decode

Guards: events with `EVENT_PAR_3 = 99` **exit immediately**
(20772-20774) — that filters `%grhcg`/`%u1bjb`/`%ftriy`/`$dtxpw`/
`$nqqly` self-releases. Note range gate 20775 explicitly admits
`$EVENT_NOTE=0` (the `$ntczb` helper, §11.5). `%EVENT_PAR[1]=0` (20786)
= mono/legato events (mono MIDI events keep par1=0 from 10803; chord
sets par1=1 at 12065) → the legato branches; `else` (21021) = the chord
branches (kill `%1wcdh`/`%grhcg`/`%u1bjb`/…/`%ftriy` per note with
plain `note_off`, 21033-21063).

**(a) Held body/sustain at note-off of the CURRENT legato note**
(`$EVENT_ID = %f4tl5[$cztyy]`, 20817-20892), pedal up (`$zs1l1=0`):

1. 2 ms settle (`wait(2000)` µs, 20787-20788), force-finish a running
   crossfade loop (20789-20816: `$u44ap:=0` + immediate
   `note_off(%jcxqm[1-$cztyy])`).
2. Mark bookkeeping: if the other slot voice exists,
   `EVENT_PAR_3 := 3` ("Last") on both the MIDI event and
   `%jcxqm[$cztyy]` (20819-20822).
3. **`note_off(%jcxqm[0])` + `note_off(%jcxqm[1])` (20828-20829) — the
   body voices are HARD note-off'd, NOT script-faded.** The audible
   tail is the group amp-envelope release stage plus the release
   SAMPLE. There is no 690 ms body fade in CSS — a long engine-side
   body fade is exactly the phasy tail the task suspected.
4. **LT voices** (everything `MARK_1` = `$dtxpw` + first-note overlay
   `$nqqly`): `$ocjln≠6` → `fade_out(by_marks($MARK_1),$tukcw*1000,1)`
   with **`$tukcw` = 400 ms shipped** (label "Fade out", 2419-2421;
   fade at 20831). `$ocjln=6` → IOI-lerped fade of the LAST transition
   IOI `$d5ans`: `$fjtlu`=150 → `$hbi2j`=281 → `$2ebzd`=281 ms over
   breakpoints `$ikygg`=75 / `$2b4cs`=150 / `$rzdte`=500 ms
   (20833-20864 — the §1.7 "retire fades" are these same knobs).
5. `%2ezeo` (run-mode voices) faded over `$axgbh` only if `<3000`;
   **shipped `$axgbh`=3000 = sentinel OFF** (20866-20872).
6. Slots cleared (`$cztyy:=0`, `%f4tl5/%jcxqm/%2t4y1 := 0`,
   20874-20880), `$ruzcg := 1` (release-trigger request, 20885).
7. **Mono `%grhcg`/`%u1bjb` (`$ocjln=6` marcato-mod layers, played at
   17764/17847 with dur 0) are NOT touched at note-off** — their FLEX
   amp envelopes decay to zero on their own (e.g. "Main marcato mod mp"
   final segments `[484,1,..],[6516,0,0.05]` ≈ 6.5 s self-decay;
   groups.json idx 174). Only chord mode note-offs them (21038-21047).

Pedal down (`$zs1l1=1`): nothing is stopped; the event is marked
`$iwaqn` (20886-20888) and the whole sequence above is deferred to
CC64-up, which does `note_off(by_marks($iwaqn))` (21801) so the release
callbacks re-run then.

**(b) The transition voice `%jcxqm`** — killed by the same
`note_off(%jcxqm[0/1])` in step 3. For `$ocjln=6` ONLY, the current
transition voice is additionally remembered (`$cpgu3 := %jcxqm[$cztyy]`,
20825) and, after the release sample is played, **faded over `$ftqcv`**
(21723-21751, gated `$ji3vf=1` shipped):

| held time of the note | `$ftqcv` |
|---|---|
| < `$nmukf`=75 ms | `$nhrkt` = **80 ms** |
| 75-175 ms (`$ks1cc`=175) | lerp 80 → 500 ms |
| > 175 ms | `$ur4fp` = **500 ms** |
| event marked par3=2 (note-in-phrase) | `$a3zg3` = 225 ms (21745-21750) |

`$auan0`=0 shipped ⇒ the release SAMPLE itself is never faded
(21752-21754).

**(c) The release-sample trigger** (21130-21758). Requirements:
`$2umbn = %yg1yz[note]` (the note's articulation) ∉ {1,7,5}
(no releases for staccato/pizz/spiccato), `$ruzcg=1`, `$4p5kj=1`
(master enable, shipped 1), then per-articulation enable `$cikng`
(21145-21166; all ship 1). The final go/no-go and gain are decided by
the event's `EVENT_PAR_3`:

| par3 | meaning | fires? | gain `$gflmk` (dB×10) |
|---|---|---|---|
| 1 | "Single" (set on first-note events, 17858) | yes (21258-21317) | `$jljyh` × `%ru5pa[held]`/100; zone 3 additionally requires held ≥ `$5vclc`=0 |
| 3 | "Last" (set at final note-off, 20820/20911) | yes (21338-21364) | `$ofpdo` × `%ru5pa[held]`/100 |
| 2 | "note in phrase" (the `$ntczb` helper, §11.5) | yes if `$qlkx3`=1 (21319-21337) | `$tkzx0` flat (no held-time curve) |
| 5 | "leg0" / re-bow helper | zone 1: no; zones 2/3: only if `$pnsjl`/`$ft4si` > 361 — shipped **−361 ⇒ never** (20992-21011) | (`$ofpdo`×curve + `$pnsjl`/`$ft4si`) |
| 0 / unset | mid-chain legato-passed note | **no** (no branch sets `$ruzcg`; exit at 21401-21403) | — |

Shipped numbers: `$jljyh` ("Min. vol.", 672-673) = **−6.0 dB**,
`$ofpdo` ("MV leg.", 679-680) = **−6.0 dB**, `$tkzx0` per articulation:
legato `$al55n`=**−6.0**, tremolo `$ehjuj`=−3.0, trills `$qtppl`=−3.0,
marcato-mod/expressive `$mig2e`=**−8.0**, harmonics `$phh04`=−3.0 dB.
`$kjwp2` ("Env.R1 vol", added at 21420 for `$ocjln=0`) is only ever
assigned 0 (17533) — effectively 0, UNKNOWN whether another script slot
can set it.

**Held-time scaling `%ru5pa`** (21171-21196 etc.): index =
`held_ms/10 − 1` clamped 0-99 (`$lh2xs`=1000 ⇒ the window is the first
1000 ms), `%ru5pa` shipped = linear 100→1. So the base −6.0 dB is
scaled: **held 10 ms → ×100% = −6.0 dB; held 500 ms → ×~50% = −3.0 dB;
held ≥1 s → ×1% ≈ 0 dB.** Short notes get quiet releases; long notes
get full-level releases.

Mechanics of the trigger: `set_controller(36,$bgt3k)` (21422 — CC36
carries the current dynamics into the release groups' modulator),
velocity forced to 1 (`$zjtcc := 1`, 21464), allow only the release
groups for `$2umbn` (21465-21705: `$ocjln=0` → `%oknf2`+`%v3sej`
tables; 6 → `%h40z2`; 2 → `%4wxpn`; 4 → `%3j1bc`/`%yfch2` by
`%vbpn3[note]`; 3 → `%ubjdd`), then
`$i5yy5 := play_note($bv0ly,1,0,0)` + `change_vol($i5yy5,$gflmk*100,1)`
(21706-21707). The table→family map is built at init from group
start-criteria CC110/CC111 (4440-4545); the criteria values aren't in
the extract, but by name the release families are `vibsus rel *` /
`nonvib rel *` (legato), `marcato mod rel *`, `tremolo rel *`,
`half/whole trills rel *`, `harmonics rel *` (groups.json idx 380-556).

**Ambiguity (flagged, not guessed):** at a chain's final note-off,
par3=3 is set on `%jcxqm[$cztyy]` (20821) *before* its `note_off`
(20828); since `%jcxqm` voices are deliberately NOT marked 99, their
release callback re-enters the handler and the par3=3 fallback
(20985-20990) would fire a second "Last" release. Whether Kontakt
delivers that callback in this ordering is UNVERIFIED — an engine
should fire exactly **one** release sample per real note-off.

### 11.3 Q3 — `%ftriy`/`$dtxpw` lifecycle; first-note anatomy

**The 550/500/500 fades (`$tdjzq/$3ivkj/$u0t23`) are re-bow fades,
triggered at NOTE-ON of a same-pitch re-strike — not at note-off and
not by the crossfade.** Chord mode: 12144 (`%at25y[note]=note` = key
already sounding) → fade the note's old `%oycvi`/`%ftriy` over
550/500/500 ms by velocity zone (12160-12179). Legato mode: re-bow
branch (`$EVENT_NOTE = $gfkjw`) fades the old `$dtxpw` over the same
knobs (19227-19237, plus `%tv4ss` fades `$ve35s`/`$mdsyv` = 0 shipped).

Full retire matrix for the supplementary LT voice (legato mode):

| trigger | fade | evidence |
|---|---|---|
| next legato transition, `$ocjln=0` | `$cig1o` IOI-lerp: LL (shipped): **250 ms → 400 ms** over IOI 150→300 ms (`$muabx`/`$bj430`=250 → `$elc3z`/`$dkiqi`=400, bp `$2oy2u`=150/`$mde3u`=300, curve `%0hwcy` = linear; EX anchors `$w4sb3`/`$kcot0`=400). `$251z0`=1 ⇒ zone 2 uses the zone-3 row. | compute 17998-18069; apply 19180 |
| next transition, `$ocjln=6`, voice still queued (hasn't sounded yet) | IOI-lerp `$whxyw`=200 → `$5zana`=225 → `$1du3v`=281 ms (bp `$uk2da`=150/`$krvzl`=250/`$yrqbj`=500); if already sounding: NOT faded here | 19183-19221 |
| next transition, other articulations | `$4dsk1` = **800 ms** | 19223 |
| re-bow | **550/500/500 ms** by zone | 19227-19237 |
| note-off | MARK_1 fade: **400 ms** (`$ocjln≠6`) / 150→281 ms IOI (`=6`) | 20830-20864 |
| two transitions later (voice from note n−2) | `$eyijx` faded `$jvqtp` = **250 ms** ("Old out") | 19239-19242 |
| never retriggered on a held note | **rings to natural sample end** at its `$1z3x0` trim | — |

So on a held note the LT/transition voice DOES ring out its full sample
— but (i) at the trims of §11.1 (0 dB only for slow, soft `$ocjln=0`
legato; −3/−9/−1.5 dB when fast), and (ii) it IS the bow-change/body
sample for that transition — the destination body `%jcxqm` starts
MUTED (18746) and is faded in by the crossfade engine, so there is no
same-pitch full-gain doubling.

**The FIRST note of a legato phrase has NO transition/LT voice at
all** — `$dtxpw`/`%ftriy` play only in the transition branch
(`$aguy2` block 19177+ / chord re-strike block 12294+). First-note
anatomy (17526-17959, `$ocjln=0`, `$ksywl=1`):

- zone 1: body `%jcxqm := play_note(note,$jpvdn,0,0)` (17596) from the
  `%d2gsb` groups. That's it.
- zone 2 (17915-17935): body is INSTANTLY MUTED (`fade_out(...,1,0)`,
  17916); a separate attack sample `$nqqly := play_note(note,$jabns,
  0,-1)` (17918; RR `%0bvoe`, par3=99, MARK_1, **dur −1** = auto
  note-off at key release) carries the attack; after `$mqqoo` = **40
  ms** the body fades in over `$otlef` = **230 ms** (17931-17934).
- zone 3 (17936-17957): same shape, `$nvwzk` = **40 ms** wait,
  `$yqhyd` = **180 ms** fade-in.

So CSS's "first-note ornament" is a swap (attack sample INSTEAD of the
body for 40 ms, then a 180-230 ms handover), never an addition. An
engine playing a full ornament sample at full gain ON TOP of the
sustain has no CSS counterpart — that is the same-pitch doubling/
phasing. `$nqqly` is retired by: its own dur=−1 auto-release at
note-off (group release env), plus the MARK_1 400 ms fade.

### 11.4 Q4 — static group volumes (groups.json)

**Every named group ships at 0.0 dB.** Checked all 609 groups
(`volume` field, linear → dB):

| family (all 5 mics: Spot1/Spot2/Main/Room/Mix) | vol |
|---|---|
| sustains: `vibsus ppp/p/mf/ff`, `nonvib p/mf/ff` | 0.0 dB |
| transitions: `legato p/mf/ff`, `NVlegato p/mf/ff`, `portamento`, `NVportamento` | 0.0 dB |
| Legzero/first-note: `legato zero p/mf/ff`, `NVlegato zero p/mf/ff` | 0.0 dB |
| `marcato mod mp/mf/f/fff` (the `$ocjln=6` layers) | 0.0 dB |
| `marc port`, `marc leg mf/f/fff` | **+0.04 dB** (1.0043-1.0050 linear — negligible) |
| releases: `vibsus rel *`, `nonvib rel *`, `marcato mod rel *`, `half/whole trills rel *`, `tremolo rel *`, `harmonics rel *`, `marcato vel releases`, `mtrem vel releases` | 0.0 dB |
| shorts, trems, trills, harmonics, runs (`run sim *` etc.) | 0.0 dB |
| 222 unnamed padding groups (3 after each family block, no samples referenced by the script tables) | −6.02 dB (unused) |

**Conclusion: there are NO static group-volume deltas to replicate.**
Transition/retrigger/release groups do NOT sit N dB below sustains.
All level differences come from (i) the script trims of §11.1-11.3,
(ii) the per-group FLEX amp envelopes (present in groups.json
`envelopes[].segments`, e.g. "Main legato mf" `[[80,..],[480,..],
[1002,0,..]]` — segment semantics only partially decoded, UNKNOWN exact
release-tail ms), and (iii) the recorded samples + CC1 crossfade
curves (per-group `mods[]`, which DO differ between sustain and legato
families). If S08/S09 intervals run ~+2.5 dB hot, the cause is engine-
side summing (LT voice + body both at full level, or an un-retired
previous LT voice), not a missing group trim.

### 11.5 Q5 — old-note note-off during legato (the 54 ms case)

When the old key's MIDI note-off arrives after the new note-on:

- `$EVENT_ID = %f4tl5[1-$cztyy]` (20894-20906): **if the crossfade is
  still running** (`$u44ap>0`), the outgoing `%jcxqm[1-$cztyy]` is
  `note_off`'d IMMEDIATELY (its scheduled full-crossfade fade is
  truncated; tail = group release env). If the crossfade already
  finished — **nothing at all**. Either way `$ruzcg` stays 0 ⇒ the
  fall-through trigger gate (21132) fails ⇒ **no release sample from
  the old key's note-off.** Under pedal it is merely marked (20903-4).
- Even after further transitions (event id no longer in either slot),
  a legato-passed note's event has par3 = 1 (first note) or 0
  (mid-chain) — neither has a fallback branch (20979-21017) ⇒ exit.

**BUT the passed note is not release-silent** — its release sample is
fired earlier, by the NEW note's `on note`, via the `$ntczb` helper
(18077-18086): `play_note(0, vel, 0, 1)` — a 1 µs note at MIDI note 0,
`EVENT_PAR_0 = old_note<<7 + zone`, par3 = **2** ("note in phrase";
or **5** if re-bow). Its instant auto-release re-enters `on release`
(note 0 admitted at 20775; par0 unpacked 20779-20781), hits the par3=2
fallback (20979-20980) and plays the **"note in phrase" release for the
OLD note at flat `$tkzx0` = −6.0 dB** (legato; −8.0 dB expressive;
21319-21337), roughly at the moment the transition starts (post-OD).
Re-bow (par3=5) release is shipped OFF (`$pnsjl`/`$ft4si` = −361).
Chord mode has the same helper at 12146-12149.

So: **suppress release samples on the old key's physical note-off for
legato-passed notes; instead fire a fixed −6.0 dB release for the old
note at transition time.**

### 11.6 Engine changes (summary)

1. **Note-off of the last held note**: stop the body voice immediately
   (tail = group release env, sub-second) — kill the 690 ms body fade.
   Fade LT/ornament voices **400 ms** (standard) / **150→281 ms**
   IOI-lerped (expressive). Expressive only: fade the transition voice
   **80→500 ms** scaled by held-time 75→175 ms.
2. **Trigger ONE release sample at note-off**: gain −6.0 dB scaled
   linearly to 0 dB as held time goes 0→1000 ms (`%ru5pa`), vel-1
   layer, release group set per articulation. That sample carries the
   tail — not a long body fade.
3. **Legato-passed notes**: no release sample at their note-off; fire a
   flat **−6.0 dB** "note in phrase" release at the moment the next
   transition starts. None on re-bow.
4. **First note**: NO transition/ornament voice. Zones 2/3 only: mute
   body 40 ms, play attack sample, fade body in 230 ms (z2) / 180 ms
   (z3); retire the attack voice at note-off (release env + 400 ms).
5. **Retire the previous transition voice at every new transition**:
   250→400 ms (IOI 150→300, shipped LL), 550/500/500 ms on re-bow,
   250 ms for the n−2 voice; destination body starts muted and fades in
   via the crossfade engine only.
6. **Transition-voice level**: recorded level (0 dB) is correct for
   slow standard legato; apply −3.0 dB when IOI < 250 ms (standard) or
   −9.0 dB (steps) / −1.5 dB (leaps) lerping to 0 over IOI 150→300 ms
   (expressive). Do NOT apply the −6 dB `$3tsb0` (chord-mode-only).
7. **No group-volume deltas**: all CSS groups sit at 0 dB — remove any
   engine-side static transition/release group trim assumptions.

---

## 12. Re-bows without the pedal (read from source, 2026-08-14)

Read directly out of `script_1.ksp` while chasing the re-bow section of
the parameter test, which was the worst-matching part of the A/B by a
wide margin.

### 12.1 `$gfkjw` is only ever set while the sustain pedal is held

`$gfkjw` is the "previous note" that every re-bow test compares against
(`if ($EVENT_NOTE=$gfkjw ...)`). In the mono-legato path it is assigned
in exactly two places, and both sit under a pedal gate:

```
18096:  if ($ocjln=0 and ($zs1l1=1))
18098:      $gfkjw := $EVENT_NOTE      ← only reachable with the pedal down
18100:      $gfkjw := -1

20425:  if ($zs1l1=1)
20429:      $gfkjw := $EVENT_NOTE
20437:  else
20438:      $gfkjw := -1               ← pedal up: cleared unconditionally
```

It is declared `-1` (5773) and cleared again at 21793. So **with no
pedal `$gfkjw` is always −1 and `$EVENT_NOTE = $gfkjw` is never true.**

Everything the earlier sections describe as re-bow behaviour is
therefore unreachable without CC64 held:

- the Legzero `$1fvjk` offsets (`$knvx2`/`$zk0vu`/`$iufx3`, 18678),
- the 550 / 500 / 500 ms re-bow fades (`$tdjzq`/`$3ivkj`/`$u0t23`, §11.3),
- the same-pitch body crossfade branch (20164).

This is stronger than §11's note that Legzero "requires CC64 held". It
is not that a pedal-less repeat takes a different re-bow path — there is
no re-bow at all.

### 12.2 So what DOES a pedal-less repeat play? A body, and nothing else

It falls through to the ordinary transition machinery, where the
transition-sample selection is guarded (19247) by

```
if ($EVENT_NOTE # $gfkjw and (%2t4y1[$cztyy] # %2t4y1[1-$cztyy]))
```

The second clause requires the two slot notes to DIFFER. On a repeat
they are the same note, so the whole `disallow_group`/`allow_group`
block is skipped and **no transition sample is played**. What remains is
the destination body against the outgoing body.

A pedal-less repeated note in CSS is thus: new body, old body faded
against it, no transition recording, no attack ornament, no Legzero.

### 12.3 What this says about our engine

Our re-attack path (`dispatch.rs`, `from_note == to_note && !cc64_held`)
already fades the old note and triggers a new body, which is the right
shape. Two things do not follow from the source:

- We applied `css_attack_transient_dip_db` (−3 dB inside 250 ms) to the
  re-attack. Its origin `%i35so` is at 12717 — inside the 12xxx CHORD
  region (§11.0), like `$3tsb0` before it. **Corrected:** there IS a
  mono-legato counterpart, at 20038-20090, and it is a different rule —
  see §12.4. (It was also dead code: the dip was read only inside
  `if legato_trim`, which has been false since `$3tsb0` went chord-only.)
- Across five repeats the reference DECAYS about 10 dB while ours
  repeats an almost identical envelope. Body-against-body crossfading is
  the mechanism; the shipped fade times for the pedal-less branch are
  the numbers still to pull.

### 12.4 The mono-legato AB dip (`$1z3x0`, 20038-20090)

The dip we had been reading out of the chord branch does exist in the
mono path, but as a different rule. Structure:

```
if ($ocjln=0)                            ; mono legato
  if ($xp1ku=3 and ($shybn=0))           ; velocity zone 3
      $1z3x0 := $x0jlu                   ; 0 dB shipped, anchor untouched
  else
      if ($ENGINE_UPTIME-$0nind<=$xu41m)             ; A
          $1z3x0 := $4lqhx*100-($ee3a4*100*e/$xu41m)
      else
          if ($ENGINE_UPTIME-$0nind<=$c2hkn)         ; B
              $1z3x0 := $ee3a4*100+(e*(abs($ee3a4)*100/$c2hkn))
          end if                                     ; C: $1z3x0 stale
          $0nind := $ENGINE_UPTIME-$xu41m
      end if
  end if
end if
...
change_vol($dtxpw,$1z3x0*100,1)
```

Shipped persistents (`persistent_1.tsv`): `$4lqhx`=−30, `$ee3a4`=0,
`$xu41m`=250, `$c2hkn`=2000, `$x0jlu`=0. `$4lqhx` is deci-dB, so branch A
is a flat **−3.0 dB** (the `$ee3a4` ramp term vanishes) and branch B is
identically **0 dB**.

The part that matters is the anchor. `$0nind` is planted at the note time
only at a phrase start (17528, gated `%f4tl5[$cztyy]=0 and $4pcsa<2` —
nothing sounding in the slot, polyphony under 2). Branch A does **not**
advance it; B and C push it to `now - $xu41m`, which makes the following
note's elapsed `IOI + 250` — always outside the window. So branch A
cannot fire twice in a row, and after the first over-window note the dip
is off for the rest of the phrase.

Net: **−3 dB on connected notes arriving within 250 ms of the phrase's
first note, 0 dB everywhere else.** Not a per-repeat anti-machine-gun
dip. On the param test this reaches only the first transition of S11 and
S12; every other section is untouched.

### 12.5 `$jvqtp` "Old out" = 250 ms — and what S10 is NOT

`fade_out($eyijx,$jvqtp*1000,1)` at 19239 fades the outgoing body whenever a
new one starts. `$eyijx` is set to `$dtxpw` on the line below, so it always
holds the previous body. The call sits OUTSIDE the interval guard at 19247,
so it runs on a pitch change and a pedal-less repeat alike. Shipped
persistent `$jvqtp` = **250 ms**.

Adopted for the CSS legato re-attack (`CSS_OLD_OUT_MS`). Deliberately not
applied to the general re-strike path, which shares this shape but also
serves live pedal-held playing, where a 250 ms subsume stacks voices.

**Measured: this is not the S10 mechanism.** 90 → 250 ms moves S10 shape
6.00 → 5.99. Per-repeat peak levels over the five repeats (30 ms RMS
envelope, relative to repeat 1):

| repeat | reference | ours   |
|--------|-----------|--------|
| 1      |  +0.00    | +0.00  |
| 2      |  −3.83    | −7.80  |
| 3      | −10.92    | −5.17  |
| 4      | −10.13    | −5.16  |
| 5      | −11.05    | −5.17  |

The reference decays over the first three repeats and then **plateaus about
10.5 dB down**. Ours dips too deep on repeat 2, recovers, and plateaus about
5 dB down — half the reference's attenuation, and the wrong trajectory. A
plateau rather than a continued decay means something keeps re-exciting at a
reduced level; a pure ring-out would keep falling. Whatever produces the
extra ~5 dB and the three-repeat ramp is still unidentified.

Note also the absolute offset: reference repeat 1 peaks at −31.20 dB, ours at
−35.31. That ~4 dB gap is the section's `lvl` ratio and is systematic across
the whole param test (every section reads 1.14-2.03), so it is a separate
issue from the S10 decay shape.
