---
title: Unbreakable — polishing the cymbals, rooms and drum bus
kind: apply-plan
song: Unbreakable (modern hard rock / metal)
start_state: snapshot drum-mix-v1 (Tone v2 on kick/snare; OHs/hat/rooms EQ and balance set)
written: 2026-11-24
research: ../topics/cymbals-rooms-bus.md
tags: [drums, plan, overheads, cymbals, hi-hat, ride, rooms, drum-bus, parallel-compression, clipping]
---

# Unbreakable — the polish plan

An ordered list of moves for an agent to apply in Signal on the live song,
after `drum-mix-v1`. Each move names **the track, the block and its phase,
the values to start from, how to check it, and what to listen for**. Every
move is sourced to the [synthesis](../topics/cymbals-rooms-bus.md) and its
source notes; values tagged **[I]** are my starting points, not anyone's
setting.

**Now** = doable with today's blocks (`eq`, `comp`, `saturate`, `gate`,
`reverb`, `band_pass`). **New** = needs a block being added (keyed dynamic EQ,
hits-keyed nonlinear verb, transient shaper, per-block GR readout). Where a
new block is missing, the plan says what to do in the meantime.

## Rules for every move

- **Snapshot first**: `MIX_SNAPSHOT {name: "before-<move>"}`; save the result
  as a variant where the move is A/B-able.
- **Section windows**: decisions over **CH 1** (crash-heavy, the loudest);
  check **VS 1** after anything that touches the rooms, the hat or the bus.
  Fills: the last bar before CH 1. Use `exact: true` whenever a gate, key or
  trigger is involved. **No whole-song MIX_MEASURE** until the final step.
- **Setting params**: through the saved state (`/tmp/sigstate.py`
  `set_block`, `eq_bands`), not `FX_SET` on a stopped instance (see the
  `drum-mixing` skill's traps). Check it took: MIX_STATE shows the values
  and a LISTEN changes.
- **Gain neutral**: after each move, level-match on the lane's phase make-up
  (`SIGNAL_SET {phase, makeup_db}`) so the A/B is about tone, not level.
  0 LU ± 0.3 over CH 1 on the track's post-fader LUFS.
- **Measurements** (LISTEN figures): `lufs`, `peak`, `rms`, `crest`, and the
  1/3-octave bands (centres 19.7 Hz · 2^(n/3); "the 6.3 kHz band" below
  means the band centred nearest it). "Band change" = post minus pre, both
  normalised to the track's own LUFS, so it reads tone, not level.
- **GR**: `/tmp/gr.py` (`block:<id>:pre` vs `:post` over CH 1) until the
  per-block GR readout lands.
- **Baseline first** (move 0): every later "check" compares to it.

---

## Phase map

| # | Element | Move | Phase | Block | Now / New |
|---|---|---|---|---|---|
| 0 | all | Baseline measurements | — | LISTEN | Now |
| 1 | OHs | HPF up to 220 Hz | Rescue | `eq` | Now |
| 2 | OHs | Find the crash band and the ring | Rescue | LISTEN | Now |
| 3 | OHs + Rooms | One ring notch, mirrored | Rescue | `eq` | Now |
| 4 | OHs | Static crash cut ~6 kHz | Tone | `eq` | Now |
| 5 | OHs | Smash-style parallel comp: snare peaks down, cymbals longer | Tone | `comp` (mix) | Now |
| 6 | OHs | Snare box out / thump in, on snare hits | Rescue (dynamic) | keyed dynamic EQ | **New** |
| 7 | Hi Hat | HPF, LPF, pan | Rescue / Depth | `eq` | Now |
| 8 | OHs ← Hat | Hat ducked in the OHs on hat hits | Relational | keyed dynamic EQ | **New** |
| 9 | Ride | HPF, clang cut, pan, level by section | Rescue / Tone | `eq` | Now |
| 10 | Cymbals | Cymbal-bus parallel comp | Tone | `comp` (mix) | Now |
| 11 | Cymbals | Crash harshness, dynamic, keyed by crash hits | Polish | keyed dynamic EQ | **New** (stopgap: move 4) |
| 12 | Rooms | Room EQ: soft HPF 120 Hz, +1.5 kHz, cymbal shelf | Tone | `eq` | Now |
| 13 | Rooms | Room comp: kick out of the detector | Tone | `comp` (`sc_hpf`) | Now |
| 14 | Rooms | Snare-keyed lift in the rooms | Relational | keyed dynamic EQ / transient shaper | **New** |
| 15 | Rooms | Transient shaper instead of/with the comp (A/B) | Tone | transient shaper | **New** |
| 16 | Drums | Glue: 4:1, 30/100 ms, `sc_hpf` 100 Hz, 2–3 dB | Tone | `comp` | Now |
| 17 | Drums | Saturation after the glue | Tone | `saturate` | Now |
| 18 | Drums | Bus EQ smile | Tone | `eq` | Now |
| 19 | Drums | Clip, then limit | Polish | `saturate` Hard → `comp` 20:1 | Now |
| 20 | Process/Compress/Punch | 20:1 slow-attack parallel (Valentine/Nolly) | Tone | `comp` | Now |
| 21 | Process/Compress/Smash | Crush lane, no cymbals, choruses | Tone | `comp` ×2 + `eq` | Now |
| 22 | Process/Compress/Crunch | Dirt lane (Scheps) | Tone | `saturate` + `eq` | Now |
| 23 | all | Section automation | Balance | faders, lane mutes | Now |
| 24 | Drums | Re-lock −23 LUFS, balance check | Balance | MIX_MEASURE | Now |

Order: cymbals first (they decide how bright the bus may get), then rooms,
then the bus, then the parallel lanes (which feed back into the bus), then
automation. **Don't touch the bus EQ (18) until 4/10 are in** (Nolly-25 cuts
the cymbals first because the bus boost brightens them).

---

## 0. Baseline (Now)

`MIX_SNAPSHOT {name: "before-polish"}`. Then LISTEN, `exact: true`, `png:
true`, over **CH 1** and **VS 1**, on `Drums` (pre-fx and post-fader),
`Drums/Cymbals`, `Drums/Cymbals/OHs`, `Drums/Cymbals/Hi Hat`,
`Drums/Cymbals/Ride`, `Drums/Rooms`. Record `lufs`, `crest`, and the bands at
**100 Hz, 250 Hz, 500 Hz, 1.6 kHz, 2.5 kHz, 3.15 kHz, 5 kHz, 6.3 kHz, 8 kHz,
10 kHz, 12.5 kHz**. Run `/tmp/bal.py` for the balance (snare 1.3 LU over
kick; OHs −10 LU vs kit sum; rooms −7.5 LU).

Also: `Drums` post-fader peak and the master's true peak over CH 1 — the clip
and limit moves (19) are judged against them.

---

## Cymbals

### 1. OHs: high-pass up to 220 Hz — Rescue, `eq`, Now

- **Why:** everyone takes the kick out of the OHs; Nolly-19 goes to 280 Hz,
  Mynett 550 Hz; we're at 150 Hz with the OHs +10 dB, so their low mids are
  loud. Keep some snare body (Levi, Colt) → stop short of the snare's
  fundamental (**251.5 Hz**, measured).
- **Values:** Low Cut **220 Hz, 24 dB/oct** [I] (Colt: "steeply, right under
  the snare's thump").
- **Check:** OHs' 100 Hz band −6 dB or more vs baseline; the 250 Hz band
  within −1.5 dB. Kit sum (`Drums` pre-fx) 63–100 Hz bands unchanged ±0.5 dB
  (the kick comes from the kick).
- **Listen for:** a tighter kick in the chorus, no thinner snare. If the snare
  thins, back to 180 Hz.

### 2. Find the crash band and the ring — Rescue, LISTEN, Now

- **Why:** Nolly-19: the crash's abrasive spike is "always somewhere between
  about 5 to 7 k"; Colt 2.5–5 kHz; Sturgis 3 kHz. Ours must be measured, not
  guessed.
- **How:** LISTEN `Drums/Cymbals/OHs` over a **crash-only stretch of CH 1**
  vs the same length of **VS 1** (hat, no crashes), both normalised to their
  own LUFS. The crash band = the 1/3-octave band(s) in 2–8 kHz where CH 1
  exceeds VS 1 most. The ring = a band that sticks out ≥ 4 dB above its
  neighbours in CH 1 (Nolly-19's was 2,750 Hz). Then, for the ring only, an
  FFT of a single crash (LISTEN_HITS on the crash hits, if the kit's cymbal
  trigger has them) to pin its frequency within ±2 %.
- **Also check its harmonics** (Levi): f/2, 2f, 4f.
- **Output:** two numbers, `F_crash` (start: 6.3 kHz) and `F_ring` (if any).

### 3. One notch on the ring, mirrored on the rooms — Rescue, `eq`, Now

- **Why:** one or two notches, never more (Nolly-19, Sturgis, Levi); the same
  crash rings in the rooms, so the same notch goes there (Nolly-19).
- **Values:** Notch at `F_ring`, **−6 dB, Q 8** [I], on `Cymbals/OHs` **and**
  `Rooms` (Rescue eq of each). Skip if move 2 found no ring.
- **Check:** the ring band −3 to −5 dB in CH 1 on both tracks; the neighbouring
  bands within ±0.5 dB.
- **Listen for:** a whistle gone from the crash tail; the crash not duller.

### 4. Static crash cut at ~6 kHz — Tone, `eq` on OHs, Now

- **Why:** Nolly-19's "big one" (−8 dB bell at 6 kHz); Nolly-25 still cuts
  ~6 kHz on the cymbal bus. Sample cymbals need less (Sturgis).
- **Values:** Bell at `F_crash` (start **6.3 kHz**), **−4 dB, Q 1.0** [I].
  Keep the existing 10 kHz shelf +1.5 dB above it (air back, as Nolly-19's
  +1.5 dB air control).
- **Check:** OHs' 5–8 kHz bands −2 to −3 dB vs baseline in CH 1, VS 1 hat
  bands (8–12.5 kHz) within −1 dB. Level-match (Tone make-up).
- **Listen for:** crashes stop "spitting"; the chorus can take the OHs louder.
  **Stop when the crashes sound distant** — that's the "neutered" warning.
- **When move 11 exists:** halve this cut (−2 dB) and let the dynamic band do
  the rest, so the OHs stay open between crashes (Colt).

### 5. OH smash-style parallel compression — Tone, `comp` on OHs, Now

- **Why:** Nolly-19 (smash mode, mix < 100 %): tame the snare/tom transients
  in the OHs so their fader doesn't change the snare, and lengthen the
  cymbals; Valentine (fast "extreme limiter": "longer decays on the crashes");
  Levi B (limit the OHs at the snare's peaks before any other compression).
- **Values:** `comp` ratio **8**, attack **0.5 ms**, release **80 ms**,
  `sc_hpf` **150 Hz**, threshold for **6–8 dB GR on snare hits**, `mix`
  **0.4**, lookahead 0 [I].
- **Check:** OHs' `crest` −2 to −3 dB over CH 1 (snare peaks down); OHs' LUFS
  after level-match 0 ± 0.3; the crash decay (LISTEN png) visibly longer.
  `/tmp/bal.py`: snare-vs-kick unchanged ±0.3 LU (the OH snare is part of
  the snare's level).
- **Listen for:** the crash attack smoother, "more of a wash" (Nolly-19); the
  snare no louder when the OH fader goes up. Too much: cymbals pumping on
  snare hits → lower `mix` to 0.25.

### 6. Snare box out, thump in — inside the OHs — Rescue (dynamic), **New**

- **Why:** Colt: a narrow dynamic cut at ~450 Hz (snare box) and a dynamic
  boost at the snare's thump, both only on snare hits, so the OHs keep their
  warmth between hits. Our OHs already have a static 450 Hz −3 dB.
- **Values (keyed dynamic EQ on `Cymbals/OHs`, keyed by the snare's hits):**
  - band 1: bell **450 Hz, Q 4, range −5 dB**, attack 0.5 ms, release 80 ms;
  - band 2: bell **251.5 Hz** (the measured fundamental), **Q 1.4, range
    +3 dB**, attack 1 ms, release 120 ms [I].
  - Then **remove the static 450 Hz −3 dB** cut (it's replaced).
- **Check:** LISTEN_HITS on snare hits over CH 1: OHs' 500 Hz band −3 to −4 dB
  at the hit, +2 dB at 250 Hz; between hits (VS 1, hat only) the 400–500 Hz
  bands within ±0.5 dB of before.
- **Listen for:** a thicker snare with no box when the OHs come up.
- **Meanwhile:** keep the static −3 dB.

### 7. Hi-hat close: HPF, LPF, pan — Rescue / Depth, `eq`, Now

- **Why:** the close hat is often barely used (Nolly-19, Scheps-M, Mynett), but
  ours is +12 dB to match the references (hat −7.6 LU). Its job: **definition
  and body**, while the OHs carry its top (Ultimate Metal posters: low-pass
  the close hat if the OHs carry its top). Valentine: "taking out all the
  low end".
- **Values:** Low Cut **250 → 350 Hz** (24 dB/oct) [I]; High Cut **12 kHz,
  12 dB/oct** [I] — only if the OHs' 10–12.5 kHz in VS 1 already sit within
  3 dB of the hat's; else leave it open. Pan to where the hat sits in the OH
  image, **not hard**: 20–30 % to the hat side (Nolly-R; audiospectra
  15–30 %).
- **Check:** `/tmp/bal.py` hat −7.6 LU ±0.5 (re-trim the fader); the hat's
  snare bleed (LISTEN_HITS on snare hits, hat track) 250–500 Hz down ≥ 3 dB.
- **Listen for:** the hat "energetic", not "a little shimmery sound"
  (Nolly-R), in VS 1 and in fast parts.

### 8. Hat in the OHs, ducked on hat hits — Relational, **New**

- **Why:** Svarog: a high band on the OHs keyed by the hat's hits lowers the
  hat in the OHs without a static cut; keyed by **hits**, not a mic, it needs
  none of his snare-ducker workaround.
- **Values (keyed dynamic EQ on `Cymbals/OHs`, keyed by hi-hat hits):** bell
  at the hat's peak band (measure: VS 1, OHs, 8–12.5 kHz; start **9 kHz**),
  **Q 1.2, range −3 dB**, attack 0.5 ms, release 60 ms [I].
- **Only if needed:** apply when, after 7, the hat is too loud in the OHs —
  i.e. bringing the close hat to −7.6 LU makes the hat as a whole too bright
  (VS 1: `Cymbals` 8–12.5 kHz bands > 3 dB above the reference shape).
- **Check:** VS 1 OHs' 8–12.5 kHz −1.5 to −2.5 dB; CH 1 crash bands
  unchanged ±0.5.
- **Listen for:** a clear, close hat with no fizz cloud in the OHs.

### 9. Ride — Rescue / Tone, `eq`, Now

- **Why:** the ride doesn't cut through the OHs, so it needs more spot
  (Nolly-19, Nolly-R: "a big clangy ride").
- **Values:** Low Cut **200 → 300 Hz** [I]; if it clangs, bell **2.5 kHz, Q
  1.5, −2 dB** (`metal-drums` [I]); pan to the ride's side of the OH image,
  ~30–40 %; fader set per section in move 23.
- **Check:** in the ride sections, ride LUFS ≈ hat's in VS 1 (±2 LU) [I]; the
  ride's bell attack visible over the OHs in a LISTEN png.
- **Listen for:** bell and stick on the ride, not just wash.

### 10. Cymbal-bus parallel compression — Tone, `comp` on `Cymbals`, Now

- **Why:** Nolly-25: heavy parallel on the cymbal bus (fast attack, fairly
  fast release, ratio ~5, ~40 %) "keeps the quiet cymbals audible, adds
  excitement". Sturgis: compression on the cymbal bus evens the shell
  transients that build up when the cymbal mics sum. Our cymbal comps are
  idle.
- **Values:** ratio **5**, attack **1 ms**, release **60 ms**, `sc_hpf`
  **300 Hz** (the detector hears cymbals, not shell lows), threshold for
  **8–10 dB GR**, `mix` **0.35** [I] (Nolly-25: ~40 %).
- **Check:** `Cymbals` crest −1.5 to −2.5 dB over CH 1; LUFS 0 ± 0.3 after
  make-up; VS 1 quiet cymbal hits (LISTEN_HITS on soft crashes/hat) up
  1–2 dB relative to loud ones.
- **Listen for:** cymbals more even and present without being louder.
  **Harshness rises with compression** (URM-V): re-check the 5–8 kHz bands —
  if they're +1 dB vs after move 4, deepen move 4 by 1 dB.

### 11. Crash harshness, dynamic — Polish, keyed dynamic EQ on `Cymbals`, **New**

- **Why:** Colt (spectral dynamics on crashes/china, two bands, slow
  attack so the snare doesn't trigger them), Nolly-25 (soothe after the
  parallel comp), Sturgis (softener after comp). Polish comes after the comp
  that exposed it.
- **Values (keyed by crash/china hits, or self-keyed with a slow attack if
  the cymbal hits aren't available):**
  - band 1: bell **3.15 kHz** (or `F_crash` low), **Q 2, range −3 dB**,
    attack **5 ms**, release **200 ms**;
  - band 2: bell **6.3 kHz** (`F_crash`), **Q 1.5, range −4 dB**, attack
    **5 ms**, release **250 ms** [I].
  - With it in, move 4 drops to −2 dB.
- **Check:** CH 1 OHs/Cymbals 2.5–8 kHz bands at crash hits −2 to −3 dB;
  VS 1 (no crashes) 2.5–8 kHz within ±0.5 dB of move-4-at-−2 dB. Snare hits
  in CH 1 (LISTEN_HITS) show no change in the OHs above 2 kHz.
- **Listen for:** open, bright OHs between crashes, crashes that don't bite.
- **Role note:** a Polish block belongs on a bus/folder (`Cymbals`), not on
  the OH mic. This keeps the signal-mixing roles table intact.

---

## Rooms

### 12. Room EQ — Tone, `eq` on `Rooms`, Now

- **Why:** the common room mistakes are cymbals washing out and too much low
  end "swamping the mix" (Nolly-R); Nolly-19: soft HPF 120 Hz, +1.5 kHz for
  shell aggression, cut the harsh cymbal band, a glassy sheen; cymbals tamed
  with a shelf/bell, not a low-pass (URM-V).
- **Values:** Low Cut **60 → 120 Hz, 12 dB/oct** (soft, Nolly-19); bell
  **1.5 kHz, Q 0.9, +2 dB**; bell **`F_crash`, Q 1.0, −3 dB**; high shelf
  **12 kHz, +1 dB** (sheen) [I]; the ring notch from move 3.
- **Check:** Rooms' 63–100 Hz bands −4 dB or more; Rooms' 6.3 kHz band
  −2 dB; `/tmp/bal.py` rooms −7.5 LU ± 0.5 after level-match.
- **Listen for:** the snare "explosion" in the room up front; less cymbal
  cloud behind it; the kick no boomier.

### 13. Room compression: the kick out of the detector — Tone, `comp`, Now

- **Why:** Nolly-R heard rooms ducked by the kick ("almost gated … around
  the kick drum"); Valentine and Nolly-25 filter the kick out of every drum
  detector. Ours: 4:1, 3/120 ms, ~5 dB, `sc_hpf` off.
- **Values:** keep 4:1, 3 ms, 120 ms; **`sc_hpf` 150 Hz**; re-set the threshold
  for **~5 dB GR** (it will compress less on the kick) [I].
- **Check:** `/tmp/gr.py` on `Rooms` over CH 1: GR on kick-only beats (if the
  song has any) ≤ 2 dB, on snare hits 5–7 dB; Rooms' crest stays ~as at
  drum-mix-v1 (sustain kept).
- **Listen for:** no pumping on double kick; the snare tail breathing.

### 14. Snare-keyed lift in the rooms — Relational, **New**

- **Why:** Nolly-19's "crucial" trick: compress the rooms for thickness,
  then a **snare-keyed upward expander** on the room bus lifts the rooms on
  each snare hit, so "the snare … jump[s] out to the front again".
- **Values (keyed dynamic EQ on `Rooms`, keyed by snare hits, one wide band
  used as a gain lift):** bell **1.2 kHz, Q 0.3** (near-broadband), **range
  +3 dB**, attack **1 ms**, release **180 ms** (≈ a snare's room bloom) [I].
  Alternative: the transient shaper keyed by snare hits, attack +3 dB.
- **Check:** LISTEN_HITS on snare hits, `Rooms`: the snare's peak +2 to
  +3 dB over the hit's first 150 ms; VS 1 hat-only stretches unchanged.
  `/tmp/bal.py` rooms back to −7.5 LU after level-match (it raises the
  rooms' level overall).
- **Listen for:** the "mid-range-y stereo spread" of the snare (Nolly-19).
- **Meanwhile:** a lighter room comp (ratio 3, ~3 dB) so the snare isn't
  flattened, and accept less thickness.

### 15. Rooms: transient shaper vs compression (A/B) — Tone, **New**

- **Why:** Nolly-25 replaced room compressors with transient designers
  (attack down, sustain up): compressing rooms makes the cymbals as loud as
  the snare. URM-V uses one on the far room (sustain up).
- **Values:** transient shaper on `Rooms` **before** the comp: attack
  **−3 dB**, sustain **+4 dB** [I]; then compare variant A (comp only, move 13),
  B (shaper only, comp bypassed), C (shaper + comp at 2–3 dB).
- **Check:** for each variant over CH 1: Rooms' crest (lower = more sustain),
  Rooms' 6.3 kHz band vs the snare's 200–300 Hz in the rooms (the
  cymbal-vs-snare balance in the rooms: higher snare/cymbal ratio wins).
- **Listen for:** the variant where the snare's room tail is biggest and the
  cymbal wash smallest. Save all three variants.

---

## Drum bus

### 16. Glue — Tone, `comp` on `Drums`, Now

- **Why:** Cervini (SSL 4:1, 30 ms, 100 ms/auto, 3–4 dB); Nolly-25/Valentine:
  the kick out of the detector. Sturgis: bus compression is what glues
  sampled cymbals to the kit — ours is a sample kit. Ours now: 2:1, 30/150 ms,
  ~2 dB, no `sc_hpf`.
- **Values:** ratio **4**, attack **30 ms**, release **100 ms**, `sc_hpf`
  **100 Hz**, threshold for **2–3 dB RMS GR** over CH 1 [Cervini numbers;
  2–3 dB for this kit's already-compressed pieces].
- **Check:** `/tmp/gr.py Drums`: 2–3 dB RMS GR, **crest up or unchanged**
  (30 ms lets the attack through: punch, not flattening); `/tmp/bal.py`
  snare 1.3 LU over kick ± 0.3.
- **Listen for:** cymbals and rooms moving with the snare ("the cymbals come
  down a little bit too", Sturgis); no kick pumping.

### 17. Saturation after the glue — Tone, `saturate` on `Drums`, Now

- **Why:** Valentine: distortion "before and/or after the Distressor … to help
  the compression sound more natural". Gentle (`metal-drums`: ≤ 1 dB peak
  reduction).
- **Values:** shape **Tube**, drive **3 dB**, bias **0.1**, `mix` 1, `level`
  −2 to −3 dB to stay neutral [I].
- **Check:** `Drums` peak −0.5 to −1 dB at the same LUFS; 3–8 kHz bands
  ≤ +0.5 dB (if more, the cymbals are fizzing: drop drive to 2 dB or switch
  to Transformer).
- **Listen for:** a denser kit, the snare's crack a little rounder; no cymbal
  grit.

### 18. Bus EQ smile — Tone, `eq` on `Drums`, Now (after 4 and 10)

- **Why:** Nolly-25: +5.5 kHz (< 2 dB), ~12 kHz shelf-ish, +100 Hz on the
  drum bus, after cutting the cymbals; Scheps-M: +2.5 dB at 100 Hz, +4 dB
  broad at 10 kHz on the mix bus; Valentine: overall brightening after the
  Distressors.
- **Values:** bell **100 Hz, Q 0.8, +1.5 dB**; bell **5.5 kHz, Q 0.7,
  +1.5 dB**; high shelf **12 kHz, +1.5 dB** [Nolly-25's places, our gains].
- **Check:** kit sum vs the reference kit shape (`/tmp/refspec.py` data):
  5–12.5 kHz within ±1.5 dB of the reference median after the boost; if
  above, back off the 5.5 kHz bell first. Level-match.
- **Listen for:** snare crack and air, not cymbal hiss. If the crashes bite
  again: the cymbal cut (4/11) is too small, not the bus EQ too big.

### 19. Clip, then limit — Polish, `saturate` Hard → `comp` 20:1, Now

- **Why:** Scheps-M mixes into a limiter doing 4–5 dB on kick/snare peaks
  ("a big part of how I'm lengthening the kick and the snare"); PE: a light
  hard clip after the bus comp and EQ is punchier than a limiter on drums,
  then limit after the clip. The drums are locked at −23 LUFS and the master
  peaks around −6 dBFS, so this is for **peak shape and density, not
  loudness**.
- **Values:**
  - clip: `saturate` shape **Hard**, drive **+2 dB**, `level` **−2 dB**, mix 1
    → 1–2 dB off kick/snare peaks only [I];
  - limit: `comp` ratio **20**, attack **0.1 ms**, release **50 ms**,
    lookahead **2 ms**, `sc_hpf` 20 (off), threshold at the post-clip peak
    −1 dB → ≤ 1 dB GR on the hardest hits [I] (Polish limiting on a bus, as
    the roles table allows).
- **Check:** `Drums` post-fader peak −2 to −3 dB vs baseline at LUFS ±0.2;
  crest −2 to −3 dB; the 5–12 kHz bands **unchanged ±0.3 dB** (cymbals not
  clipped). LISTEN_HITS on snare hits: peak down, the 50–150 ms body
  unchanged.
- **Listen for:** the same snare crack with less spike; if the snare's
  timbre changes (PE: "a noticeable change of timbre in the snare") or the
  cymbals grit, halve the drive.
- **Open:** whether `saturate` oversamples. If it doesn't, a hard clip
  aliases on cymbals: keep drive ≤ 1.5 dB, or use shape **Diode** (softer).

---

## Parallel lanes (`Drums/Process/Compress/*`)

Each lane is muted until wanted. Blend from silence (NTM, Nolly-25) and
**level-match**: with the lane unmuted, pull the `Drums` post-fader back by
what the lane added, so A/B compares density, not level. Each lane's return
is measured as **LU under the dry kit sum** over CH 1.

### 20. Punch — the Valentine/Nolly bus parallel — Tone, Now

- **Why:** Valentine's Distressors (20:1, detector HP + presence, slowest
  attack, tempo release) and Nolly-25's copy (25–40 % blend, level-matched).
  It "manufacture[s] more of an attack".
- **Feeds:** kick, snare, toms, rooms at 0 dB send; **OHs at −10 dB send**
  (Nolly-19: "not anywhere near as much cymbal"); hat and ride **not sent**.
- **Values:** `comp` ratio **20**, attack **25 ms** [I: "slow"; Nolly says the
  knob acts faster than it sounds — try 10 ms too], release **one 8th note at
  the song's tempo** (e.g. 250 ms at 120 BPM; read the tempo map) [I],
  `sc_hpf` **150 Hz** (the detector HP; no presence bell in Signal — the
  higher HPF stands in), threshold for **8–12 dB GR on snare hits**. After it:
  `eq` +2 dB at 5.5 kHz, +1.5 dB 12 kHz shelf (`metal-drums`).
- **Blend:** return at **−8 to −10 LU** under the dry kit [I] (≈ Nolly's 32 %).
- **Check:** `/tmp/gr.py` on the lane; the kit's crest over CH 1 **up** ≥ 0.5
  dB with the lane in (more attack); `/tmp/bal.py` kick vs snare within
  ±0.5 LU (if the kick drops, `sc_hpf` up to 200 Hz).
- **Listen for:** the room swelling back between hits "at a rate that feels
  musical" (Valentine); attack bigger, not smaller. If the cymbals swell,
  release shorter.

### 21. Smash — the crush lane, choruses — Tone, Now

- **Why:** Mynett's 33609 (6:1, 100 ms release, then a limiter; ~18 dB total;
  no cymbals; post +190 Hz / +6.9 kHz shelves); NTM (8:1–∞, fast/fast,
  10–20 dB, HPF before). Scheps-M: such layers make the kit "thick underneath
  everything else" without turning it up.
- **Feeds:** kick, snare (+ Fund), toms, **rooms**; **no OHs, hat or ride**
  (Mynett, Scheps-D).
- **Values:** `eq` Low Cut **120 Hz** before; `comp` 1: ratio **6**, attack
  **0.1 ms**, release **100 ms**, ~10 dB GR; `comp` 2: ratio **20**, attack
  **0.1 ms**, lookahead **2 ms**, release **50 ms**, ~6–8 dB GR; `eq` after:
  low shelf **190 Hz +3 dB**, high shelf **6.9 kHz +3 dB**, high cut
  **10 kHz** (keep cymbal bleed out) [Mynett values; shelf gains I].
- **Blend:** **−12 to −8 LU** under the dry kit; **on in choruses (and the
  last chorus loudest), off in verses** (move 23).
- **Check:** lane total GR 15–18 dB (two `/tmp/gr.py` reads); with the lane
  in, CH 1 kit LUFS +0.5 to +1 LU before level-match, crest −1 dB at most;
  the 5–10 kHz bands of the kit ≤ +0.5 dB (no cymbal fizz).
- **Listen for:** a bigger, denser chorus; the snare's tail and the room
  "bloom". Too much: the kit sounds small and squashed → lower the return,
  not the GR.

### 22. Crunch — the Scheps dirt lane — Tone, Now

- **Why:** Scheps-D/M: heavy parallel distortion, then a broad cut of the
  cymbals it wrecked, keep < 1 kHz plus a little air, return −14.5 dB,
  choruses only: "my drums are badass". Mynett's parallel snare saturation is
  a narrower version.
- **Feeds:** snare and OHs at 0 dB, kick at **−6 dB** (Scheps turned the kick
  down: "ultra low end"), toms at 0, rooms at −6.
- **Values:** `saturate` shape **Hard**, drive **18 dB**, `level` **−12 dB**;
  `eq` after: Low Cut **80 Hz**, bell **4 kHz, Q 0.4, −12 dB** (the broad
  cymbal dip), high shelf **12 kHz +2 dB** ("whatever air … crispiness") [I].
- **Blend:** **−14 to −18 LU** under the dry kit; choruses only.
- **Check:** with the lane in, kit 200 Hz–1 kHz bands +0.5 to +1 dB, 3–8 kHz
  ≤ +0.3 dB. Phase: LISTEN the kit with the lane in at −14 LU vs out, level
  matched: no band dips more than 0.5 dB (no combing; Scheps' SansAmp
  warning).
- **Listen for:** weight and "ringiness"; no audible distortion on its own in
  the mix.
- **Tight** stays for later (the glue is on the bus, move 16); **Dry** stays
  as is.

---

## Automation and the end

### 23. Section automation — Balance, Now

Every long-form source automates by section (Nolly-19, Scheps-M, Valentine).
Starting moves [values from Nolly-19 where given, else I]:

| Where | Move |
|---|---|
| Verses | `Rooms` **−3 dB**; Smash and Crunch **muted**; Punch on; snare's Punch/Smash send −5 dB (Nolly-19) |
| Last bar into a chorus (fills) | `Rooms` **+2 dB** for the fill; `Drums` **+1 dB** for the fill, back down after the downbeat (Nolly-19) |
| Choruses | Smash and Crunch on; ride/hat spots per the cymbal the drummer rides |
| Fast double-kick / blast parts | hat (or ride) close **+2 dB** (Nolly-R: cymbals get lost there) |
| Quiet sections (if any) | favour a mono room if the session has one (Nolly-19); else `Rooms` −4 dB |

- **Check:** VS 1 vs CH 1: kit LUFS difference should follow the
  arrangement (chorus kit ~+1 to +2 LU over verse kit) [I] while the
  **`Drums` fader doesn't move** (Scheps-M).

### 24. Re-lock and balance — Balance, Now

- `/tmp/bal.py` over CH 1: snare 1.3 LU over kick, OHs −10 LU vs kit sum,
  rooms −7.5 LU, hat −7.6 vs references; fix on the faders only.
- **Only now** the whole-song MIX_MEASURE: `Drums` back to **−23.0 ± 0.1
  LUFS** on its fader; check the master's true peak.
- Snapshot **`drum-mix-v2`**.

---

## The top 8

If time is short, these give the most polish, in this order:

1. **Move 2 + 4:** find the crash band, cut it (start −4 dB at 6.3 kHz) — the
   single move Nolly calls make-or-break.
2. **Move 3:** one ring notch on OHs, mirrored on Rooms.
3. **Move 5:** OH parallel smash (8:1, 0.5/80 ms, mix 0.4): snare peaks out
   of the OHs, cymbals longer.
4. **Move 12 + 13:** room EQ (soft HPF 120 Hz, +1.5 kHz, cymbal cut) and the
   room comp's `sc_hpf` 150 Hz.
5. **Move 16:** drum-bus glue 4:1, 30/100 ms, `sc_hpf` 100 Hz, 2–3 dB.
6. **Move 20:** the Punch lane (20:1, slow attack, tempo release, `sc_hpf`
   150 Hz, no hat/ride, −8 to −10 LU).
7. **Move 21:** the Smash lane for choruses (6:1 → 20:1 limiter, no
   cymbals, +190 Hz/+6.9 kHz shelves, −12 to −8 LU).
8. **Move 19:** light hard clip (+2 dB) then a ≤ 1 dB limiter on `Drums`,
   cymbal bands unchanged.

Then, when the new blocks land: **11** (dynamic crash bands), **14**
(snare-keyed room lift), **6** (snare box/thump in the OHs), **8** (hat
in the OHs), **15** (transient-shaped rooms).

## Open questions for the owner / Signal

- Does `saturate` oversample? (Moves 19, 22.)
- Is Signal's `eq` minimum-phase (no pre-ringing; Nolly-R)? Are parallel
  lanes latency-compensated against the dry kit (lookahead in Smash)?
- Does the kit expose **crash/china hits** for keying (move 11) and hi-hat
  hits (move 8)?
- The song's tempo for the Punch release (move 20).
- Roles: moves 6 and 8 put keyed dynamic EQ on the OH *mic* (Rescue/
  Relational). The roles table gives mics Rescue and Tone only: allow
  Relational on OHs, or move them to `Cymbals` (which would also act on the
  hat/ride spots).
