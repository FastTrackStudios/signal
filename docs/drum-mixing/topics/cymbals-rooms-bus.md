---
title: Cymbals, rooms and the drum bus — what the sources say
kind: synthesis
scope: drums (overheads, hi-hat, ride, crashes, rooms, drum bus, parallel lanes)
summarised: 2026-11-24
sources:
  - ../sources/nolly-ggd-cymbal-processing.md
  - ../sources/nolly-periphery-wildfire-masterclass.md
  - ../sources/nolly-reviews-your-drum-mixes.md
  - ../sources/colt-capperrune-overhead-trick.md
  - ../sources/eric-valentine-mixing-drums.md
  - ../sources/andrew-scheps-drum-dirt-bus.md
  - ../sources/andrew-scheps-mix-buss-drum-processing.md
  - ../sources/joey-sturgis-how-to-easily-mix-cymbals.md
  - ../sources/svarog-studio-taming-hihats-in-overheads.md
  - ../sources/geargods-cymbal-mixing-secrets.md
  - ../sources/urm-academy-rooms-and-overheads.md
  - ../sources/urm-academy-mixing-secrets-drums.md
  - ../sources/production-expert-clipper-plugins.md
  - ../sources/solitaire-music-crush-room-mics.md
  - ~/.claude/skills/signal-mixing/references/metal-drums.md (Mynett, Putney, Cervini, Sneap)
tags: [drums, overheads, cymbals, hi-hat, ride, rooms, drum-bus, parallel-compression, clipping, synthesis]
---

# Cymbals, rooms and the drum bus

What the sources agree and disagree on, per element. Each claim names its
source note; `metal-drums` means `~/.claude/skills/signal-mixing/references/metal-drums.md`.
**[I]** marks my reading across them. Settings for Unbreakable are in the
[apply plan](../plans/unbreakable-polish.md), not here.

Short names: **Nolly-19** (GGD cymbal masterclass, 2019), **Nolly-25**
(Wildfire, 2025), **Nolly-R** (mix reviews), **Colt**, **Valentine**,
**Scheps-D** (Drum Dirt), **Scheps-M** (mix bus/drums), **Sturgis**,
**Svarog**, **Levi** (GearGods), **URM-V** (rooms/OH video), **URM-A**
(article), **PE** (clippers), **Mynett/Putney/Cervini** (`metal-drums`).

---

## 1. What the overheads are for

| View | Who |
|---|---|
| **Cymbal mics.** The shells' body comes from close mics and rooms; OHs are high-passed hard. | Nolly-19 (HPF 280 Hz), Valentine (lows out: "omnipresent low-mid muck"), Mynett (HPF 550 Hz on metalwork), URM-A ("≤ 500 Hz often fully rolled off" in metal) |
| **Part kit image / snare weight.** Keep some snare in them, even add it. | Colt (dynamic boost of the snare thump in the OHs), URM-V (console drive "for more weight to the snare in the overheads"), Levi ("don't take all the drums out … just a stick hitting a head"), Putney (HPF ~125 Hz, `metal-drums`) |
| **Air only.** The cymbals live in the rooms and close mics. | Scheps-M (so much cymbal bleed elsewhere that the OHs are "a little bit of air") |

**Agreement:** none of them want the kick's low end in the OHs. **Split:** how
much snare the OHs should carry. **[I]** For a sample-based kit whose OHs sit
at −10 LU against the kit sum, the OHs are cymbal mics with *some* snare: the
HPF can come up from 150 Hz toward 200–280 Hz without losing the image, and the
snare in them should be **controlled** (Levi, Nolly-19: compress/limit so the
OH fader doesn't change the snare sound) rather than removed.

## 2. Cymbal harshness

**The frequencies.** Every source puts the abrasive part of crashes in the
upper mids/lower treble, but they disagree on where exactly:

| Range | Who | Move |
|---|---|---|
| **5–7 kHz, −8 dB bell at ~6 kHz** | Nolly-19 | static, "the big one" |
| **~6 kHz cut on the cymbal bus** | Nolly-25 | static, partly to offset the drum bus +5.5 kHz |
| **2–2.5 kHz and ~6 kHz**, spectral-dynamic, slow attack | Colt | dynamic only on crashes/china |
| **~3 kHz** (+500 Hz box) | Sturgis | static, on the cymbal bus |
| **2–4 kHz** | URM-A | static |

**Static vs dynamic.** Colt's case for dynamic: a static 2.5–5 kHz cut closes
the OHs ("the openness lives there") and congests the snare. Nolly moved from
manual notching to **soothe** (Nolly-25); Sturgis uses an algorithmic
softener. **[I]** The trend is **dynamic**: cut only when a crash rings.
Static is fine where the harsh band is not where the "openness" is (Nolly's
6 kHz).

**Notches.** All agree: **one or two**, not ten. Nolly-19: "you could drive
yourself crazy … end up with … a slightly neutered sounding cymbal";
Sturgis: too many notches "sounds like white noise"; Levi: check the
harmonics of a ring (f/2, 2f, 4f). Nolly-19 **mirrors his OH notch on the
rooms** (2,750 Hz in both): the same crash rings in both.

**Order:** EQ/notch → compression → resonance suppression (Sturgis); EQ →
parallel comp → soothe (Nolly-25). Compression *raises* cymbal harshness
(URM-V: "can get [harsh] very easily when compressing really hard"), so the
suppression comes after.

**Keep cymbals out of the crush.** Mynett feeds no metalwork to the smash
bus; Scheps-D EQs the cymbals out of the dirt bus after the distortion;
iZotope (`metal-drums`). Nolly-19 is the exception: a *small* cymbal send
("I don't leave anything out of it, but not anywhere near as much cymbal as
snare").

## 3. Snare in the overheads

Three tools, all agreeing that the snare's **transient** in the OHs is the
problem, not its body:

1. **Compress/limit the OHs** so snare peaks stop sticking out: Nolly-19
   (smash mode, mix < 100 %), Valentine ("pretty extreme limiter … pulled
   forward … longer decays on the crashes"), Levi B (L2 at the snare's peaks),
   URM-A (L1 on overheads).
2. **Duck the OHs on snare hits**: Levi A (sidechain comp, light).
3. **Dynamic EQ keyed by the snare**: Colt (−dB at ~450 Hz box on each hit,
   +dB at the 180–250 Hz thump).

**[I]** In Signal 1 is doable now (comp), 2 and 3 need the keyed dynamic EQ
keyed by the snare's hits — which is cleaner than any of theirs, because the
key is the trigger, not a bleed-laden mic.

## 4. Hi-hat and its relation to the overheads

- **Often the hat close mic is barely used or muted**: Nolly-19 ("turned way
  down"), Scheps-M (not used), Mynett (disabled), Svarog (no output, key only).
- **But in fast metal it gets lost**: Nolly-R (thrash/blast: "just a little
  shimmery sound … bring up the close mics"). **[I]** the hat close mic is a
  *section* control: up in fast parts, down in open, crash-heavy choruses.
- **Too much hat in the OHs:** Svarog keys a high-band compressor on the OHs
  from the hat mic (with a ducker so snare bleed doesn't key it); Gearspace
  posters do the same with a dynamic EQ. Ultimate Metal posters: low-pass the
  close hat if the OHs already carry its top, so the close mic adds only body.
- **Hat EQ:** low end out (Valentine, all), HPF 250–400 Hz (`metal-drums`),
  a 4.5 kHz cut on hat spill in the snare (Mynett).
- **Panning:** a mono spot sits where the cymbal is in the OH image, never
  hard (Nolly-R; audiospectra guide: 15–30 % to the OH side).

## 5. Ride

- The ride "tend[s] not to cut through into the overheads" though loud in the
  room: it needs **more spot mic** than the other cymbals (Nolly-19, Nolly-R
  "a big clangy ride").
- `metal-drums` [I]: HPF 250–400 Hz, −2 dB at 2–3 kHz if it clangs.
- **[I]** Ride and hat spots carry **definition (attack, bell)**, the OHs the
  **wash**; set the spot level per section.

## 6. Rooms

**What they are for — agree:** size, sustain, the snare's explosion, energy.
Nolly-19: "the snare … explosion expanding into the room"; Valentine: "fills
in all the space"; Scheps-M: rooms brought in for pushes so "the drummer is
beating the crap out of the drums".

**Compress or not — disagree:**

| View | Who |
|---|---|
| **Crush, fast/fast, 10 dB+** (1176 style, "all buttons in") | URM-A, Mynett/`metal-drums` (10–20:1 or limiting), URM-V mono room (Kramer PIE limit mode), Valentine's "destroyed" room (tucked way down) |
| **Compress hard, slow attack/fast release** (10:1+, breathes) | Solitaire (weak source) |
| **Compress, then restore the snare with keyed upward expansion** | Nolly-19 |
| **Don't compress — transient designer: attack down, sustain up** (compression makes cymbals as loud as the snare) | Nolly-25, URM-V far room (sustain up) |
| **Gate the rooms on kick/snare** for explosion without wash | Valentine (keyed, key delayed to the room's arrival), SOS Valentine/Slash (room gated on the snare) |

**[I]** These aren't contradictory: each controls the same thing —
**cymbal wash vs shell sustain in the rooms**. Compression raises the wash;
a transient shaper raises sustain without raising cymbal peaks as much;
keyed expansion or gating puts the snare back on top. The common rule: **the
snare must win in the rooms** (Nolly-19, Valentine, Nolly-25 rooms setup:
snare up, kick down in the far rooms).

**Room EQ — agree:**
- **Low end out**: Nolly-19 HPF 120 Hz soft slope ("a ton of kick drum …
  not really necessary"), Nolly-R ("leaving too much low end … swamping the
  mix"), Mynett 185 Hz, Putney 60 Hz (`metal-drums`).
- **Cymbals tamed with a shelf/bell, not a low-pass**: URM-V ("save the meat
  of the cymbals"), Nolly-19 (cut the harsh band, then a glassy sheen on
  top). Mynett is the exception: LPF 2 kHz at 6 dB/oct on natural rooms.
- **Shell aggression ~1.5 kHz**: Nolly-19.
- **The OH notch mirrored**: Nolly-19.

**Kick pumping the rooms:** Nolly-R heard a kick-ducked, "almost gated"
room tail: the compressor saw the kick. **[I]** `sc_hpf` on the room comp.

**By section:** rooms down in verses, up for fills and choruses (Nolly-19,
Scheps-M); a mono room favoured in quiet sections "keeps them pinned down the
middle" (Nolly-19).

## 7. Drum bus — glue

| Setting | Who |
|---|---|
| **SSL-style 4:1, 30 ms, 100 ms/auto, 3–4 dB GR** | Cervini via NTM (`metal-drums`) |
| **2:1, 3–4 dB** (mix bus) | Mynett |
| **Fast-attack bus comp clamping the snare** (3 ms / 100 ms / 4:1 / SC HPF 60 Hz, 3–4 dB on the snare) — on the *instrument* bus | Nolly-25, Nolly-R |
| **Bus comp to glue sampled cymbals to the kit** | Sturgis |

**Agreement:** light (2–4 dB), with the kick filtered out of the detector
where there's a filter. **Split:** slow attack (Cervini: punch through)
vs fast (Nolly: clamp the snare so it *seems* loud without spiking).

## 8. Parallel compression

| Lane | Settings | Feeds | Who |
|---|---|---|---|
| **Distressor 20:1, detector HP + presence, slowest attack, tempo release, in series or 25–40 % blend** | ratio 20, attack slow (Nolly: "8"; generic: < 10 ms, 3–4 ms), release 2.5 (tempo), level-matched | whole kit | Valentine, Nolly-25 |
| **Kick+snare crush, dbx 160 ~4:1, light** | adds length, changes the attack shape | kick, snare | Scheps-M |
| **Smash (33609) 6:1 + limiter, ~18 dB GR**, post +190 Hz / +6.9 kHz shelves | | kick, snare samples, toms; **no cymbals** | Mynett |
| **Generic crush 8:1–∞, fast/fast, 10–20 dB, HPF 100–150 Hz pre** | blend from −∞ | | NTM (`metal-drums`) |
| **Fatso + unlinked Fairchild** from the drum aux | peak limiting, "more fun" | whole kit | Scheps-M |
| **Cymbal parallel, slow release** for length | | cymbals | Newell (Levi) |
| **Heavy parallel on the cymbal bus, ratio 5, ~40 %** | fast attack, fairly fast release | hats, spots, OHs | Nolly-25 |

**Agreement:** the **detector must not see the kick** (Valentine: without HP
it "annihilate[s] the kick drum in the balance"; Nolly-25; NTM; Mynett feeds
kick but the detector question doesn't arise with separate sends). **Level-
match** the blend (Nolly-25). **Cymbals light or out** in the crush (Mynett,
Scheps-D, Nolly-19). **Split:** a *slow* attack to manufacture attack
(Valentine, Nolly-25) vs *fast* crush for density (Mynett, NTM, URM-A rooms).
**[I]** Both, in two lanes: slow = Punch, fast = Smash.

## 9. Saturation and parallel distortion

- **Per mic, "like tape"**: more distortion in the highs than the lows,
  multiband (Valentine). Same multiband plugin on every mic of a piece so
  crossovers sum (Valentine).
- **Around the bus compressor**: distortion before and/or after the
  Distressor "to help the compression sound more natural" (Valentine,
  Gearspace).
- **Parallel dirt bus**: heavy clipping, then a broad mid/cymbal cut, keep
  < 1 kHz and air; return −14.5 dB; choruses only (Scheps-D, Scheps-M).
  **Must be phase coherent** (Scheps-D: SansAmp combs in parallel).
- **On the OHs**: console drive for snare weight (URM-V); tape to roll off
  the top (Colt).

## 10. Clipping and limiting

- **Into a limiter**: Scheps-M (4–5 dB on kick/snare peaks at the mix bus,
  "a big part of how I'm lengthening the kick and the snare"), Nolly-25
  (mixes into Pro-L2, ~6 dB `[caption?]`). Tom bus limiter (Nolly-25).
- **A clipper on the drum bus after comp + EQ, light** (PE: Krantzberg);
  hard clipping punchier than limiting on drums (PE demo), **with
  oversampling** (PE: aliasing).
- **Clip, then limit** (PE: Simpson).
- **[I]** None of them give a drum-bus clip amount; "light touch". Measure:
  clip only the kick/snare peaks (1–3 dB), cymbals untouched.

## 11. Bus EQ

- **+5.5 kHz (< 2 dB), ~12 kHz "weird shelf", +100 Hz (sometimes 60 Hz)** on
  the drum bus (Nolly-25), offset by a ~6 kHz cut on the cymbal bus.
- **+2.5 dB at 100 Hz, +4 dB broad at 10 kHz** on the mix bus (Scheps-M).
- **Overall brightening** after the Distressors (Valentine).
- Post-smash **+190 Hz / +6.9 kHz shelves** (Mynett).
**Agreement:** a smile, after the dynamics. **[I]** It brightens the cymbals
too, which is why the cymbal-bus cut comes first (Nolly-25).

## 12. Automation as polish

Every long-form source automates by section: Nolly-19 (verse rooms down, mono
room in; fill rooms up; bus +1 dB into the downbeat), Scheps-M (rooms, dirt,
snare verb only in pushes/choruses), Valentine (rooms un-gated for fills).
**[I]** "Polished" here means **the kit grows with the arrangement without the
fader moving** (Scheps-M: "the drum kit stays the same level but it just gets
thick underneath").

---

## What none of them settle

- **How much** dynamic cymbal reduction (dB) — always by ear.
- **The Distressor attack in ms** — "slow" on the knob, maybe 3–10 ms
  measured (Nolly-25's own estimate).
- **A drum-bus clip figure.**
- Whether Signal's blocks are phase coherent in parallel (`saturate`
  oversampling, `eq` phase) — check before blending lanes.
