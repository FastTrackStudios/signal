# Modern metal / hard-rock drum mixing — research for Signal's drum mixer

Compiled 2026-11-23. Every claim carries a link. Tags used throughout:

- **[S]** sourced: the number or practice is stated in the linked source.
- **[S2]** secondary: from an editorial or third-party write-up of an engineer's method, not the engineer's own words.
- **[I]** inference: my extrapolation, a typical value, or a mapping onto Signal. Treat as a starting point.
- **[?]** could not verify.

---

## 0. Corrections to the brief (credits)

| Record | Brief said | Verified |
|---|---|---|
| Megadeth — *Megadeth* (2026, "final album") | — | **Produced** by Dave Mustaine & Chris Rakestraw (Teemu Mäntysaari: additional production/engineering). **Mixed by Matt Hyde** (The Hydeaway, Van Nuys). **Mastered by Ted Jensen** (Sterling Sound). Drums: **Dirk Verbeuren**, tracked at **One On One Recording, North Hollywood**; drum tech Thomas "Ozzy" Richens. [S] [Wikipedia](https://en.wikipedia.org/wiki/Megadeth_(album)), [Roppongi Rocks interview](https://www.roppongirocks.com/archives/18186), [Blabbermouth](https://blabbermouth.net/news/megadeth-shares-puppet-parade-single-from-upcoming-self-titled-album), Discogs [1](https://www.discogs.com/release/36295171-Megadeth-Megadeth) / [2](https://www.discogs.com/release/36288400-Megadeth-Megadeth) (Discogs returned 403 to me, so these are taken from search snippets) |
| Megadeth — "We'll Be Back" (*The Sick, the Dying… and the Dead!*, 2022) | "mixed by Chris Collier" | **Not Collier.** **Mixed by Josh Wilbur**, produced/engineered by Mustaine & Rakestraw, mastered by Ted Jensen, drums by Dirk Verbeuren (drum tech Rick West). Drums and bass were tracked at Blackbird, Nashville. [S] [Wikipedia: album](https://en.wikipedia.org/wiki/The_Sick,_the_Dying..._and_the_Dead!), [Wikipedia: song](https://en.wikipedia.org/wiki/We'll_Be_Back), [Metal Archives](https://www.metal-archives.com/albums/Megadeth/We'll_Be_Back/1056668), [Blabbermouth](https://archive.blabbermouth.net/news/why-megadeth-decided-against-recording-new-album-remotely/). I found no credit for Chris Collier on any Megadeth record. His nearest links are Marty Friedman's solo work and Korn/Prong/Metal Church ([Metal Archives](https://www.metal-archives.com/artists/Chris_Collier/557971)). |
| Spiritbox — "Cellar Door" (*The Fear of Fear* EP, 2023) | "verify" | **Produced/engineered by Dan(iel) Braunstein**, co-produced by Mike Stringer, **mixed by Zakk Cervini**. The EP credits drums to **Zev Rosenberg** and also to Mike Stringer on "tracks 2, 3 and 6". "Cellar Door" is track 1, so per the liner notes its drums are Rosenberg's. A Rise Records YouTube credit block reportedly lists Stringer under "Drums" for the song. That reached me only through a search summary, so it is **[?]** unverified. [S] [Wikipedia](https://en.wikipedia.org/wiki/The_Fear_of_Fear), [Shazam](https://www.shazam.com/en-us/song/1705903384/cellar-door), [YouTube](https://m.youtube.com/watch?v=bMlcUC4p1oU) |

Useful link: **Ted Jensen** mastered both Megadeth records and Gojira's *Magma*, whose measured loudness is in §2.9.

---

## 1. What the people involved have said publicly

### Megadeth 2026 — Dirk Verbeuren / Chris Rakestraw (tracking), Matt Hyde (mix)
- Verbeuren said the drum target was **"very organic-sounding but still very crisp and powerful"** and **"as organic and natural as possible"**. His reference was **Nick Menza's tone on *Countdown to Extinction***. He also said *The Sick, the Dying…* was **"a bit too crisp and produced"** and **"I made sure to change that on this one."** [S] [Roppongi Rocks](https://www.roppongirocks.com/archives/18186)
- The drums were tracked at One On One. The same room and sessions produced Toontrack's **Metal Mania EZX** (three kits plus extra snares). Verbeuren does not say whether those samples are on the album. [S] [Roppongi Rocks](https://www.roppongirocks.com/archives/18186), [Toontrack Metal Mania EZX](https://www.toontrack.com/product/metal-mania-ezx/)
- Rakestraw on recording those Metal Mania sessions: **"I barely used any EQ over the entire session full of mics"**. Skipping EQ while tracking leaves the phase untouched, so the sounds take processing well later. **[S2]** This comes from a search snippet of [Toontrack's "Five Questions to Chris Rakestraw"](https://www.toontrack.com/news/five-questions-to-chris-rakestraw/); the page itself only loaded an index for me.
- **Matt Hyde:** I found **no interview** about this mix. **[?]**
- **Takeaway [I]:** the 2026 album's drum sound is a deliberate step away from heavy sample reinforcement: real room, less "produced". For Signal that means a lower sample share, more room, and less parallel crush than the Spiritbox target.

### "We'll Be Back" — Josh Wilbur (mix)
- Wilbur mixed *Dystopia* (2016, Chris Adler on drums) and *The Sick, the Dying…*. He has produced and mixed Lamb of God since *Sacrament*. [S] [Blabbermouth](https://blabbermouth.net/news/megadeth-taps-lamb-of-god-producer-josh-wilbur-for-new-album-mix), [AllMusic](https://www.allmusic.com/artist/josh-wilbur-mn0000942895)
- From his own words on the [URM Podcast ep. 293](https://urm.academy/urmpc293-josh-wilbur/) [S]:
  - He EQs heavily on the way in: "I eqd the Toms. I am pretty heavy with the EQ going in sometimes", mostly by "dip[ping] the mids on the Toms".
  - Repeating those moves at mixdown "starts becoming too drastic".
  - His mix bus is "one compressor, that's it". His self-mastered version is "a two bus compressor and a limiter".
  - On Korn's *The Serenity of Suffering* they "turned it up till it clipped".
- He built the Toontrack **Drums of Destruction EZX** with Chris Adler and did a video series remixing "In Your Words" (Lamb of God) and "The Sin and the Sentence" (Trivium). In it he turns the original drum stems into MIDI with SD3 Tracker and rebuilds the drums with samples. [S] [Toontrack](https://www.toontrack.com/news/mixing-trivium-and-lamb-of-god-song-with-josh-wilbur/page/34/), [YouTube](https://www.youtube.com/watch?v=hzvrPIH99vI), [SOS review](https://www.soundonsound.com/reviews/toontrack-drums-destruction-ezx). The videos give no numeric settings in text form. **[?]**
- A reviewer said *Dystopia*'s drums "lack the sense of physicality". This is opinion. [S] [Renowned for Sound](https://renownedforsound.com/album-review-megadeth-dystopia/)

### Spiritbox — Dan Braunstein (production) and Zakk Cervini (mix)
- Braunstein, on Spiritbox sessions in general (not "Cellar Door" specifically): **"The drums are programmed with Toontrack Superior Drummer 3, and I add more samples using the Steven Slate Trigger."** Kick and snare "may each have eight additional samples, sometimes even more." His session folders include a **"Kick SC"** (kick sidechain) track. **[S2]** These come from search snippets of [AudioTechnology "Recording Spiritbox"](https://www.audiotechnology.com/features/recording-spiritbox); the page returned 403 to me.
- Braunstein sells a one-shot pack, "Drumshotz", described as the studio sounds of Spiritbox, Dayseeker and others. [S] [Drumforge](https://drumforge.com/products/drumshotz-daniel-braunstein). Nail The Mix has a session of his Spiritbox "Holy Roller" mix. [S] [NTM](https://www.nailthemix.com/zakk-cervini)
- Cervini, in his own words: "I use drum and guitar sounds that I've created that no one else has". An assistant makes his drum MIDI. He works minimal and in the box. [S] [URM Podcast ep. 302](https://urm.academy/urmpc302-zakk-cervini/). On Blink-182 he named the **Softube FET Compressor on the drum bus** as one of five crucial plug-ins. [S] [SOS Inside Track: Blink-182](https://www.soundonsound.com/techniques/inside-track-blink-182-california)
- Nail The Mix's write-up of Cervini's approach **[S2]** [NTM](https://www.nailthemix.com/zakk-cervini):
  - He **blends rather than replaces**.
  - Kick: the live kick gives low-mid thump; a processed sample adds attack and sub.
  - Snare: the live snare gives body and wires; a compressed sample adds crack, with "a pronounced 5kHz-ish attack".
  - Drum bus: SSL-style VCA, **4:1, 30 ms attack, 100 ms or Auto release, 3–4 dB GR**.
  - Guitar HPF at 80–120 Hz and a cut at 250–400 Hz leave room for kick and snare.
- Forum speculation, not reliable: he keeps "a high, medium and low snare" and uses 1176-style parallel drum compression. [Gearspace](https://gearspace.com/board/drums-/1442049-architects-bmth-snare.html)

### Other modern-metal engineers
- **Andy Sneap**, from old forum posts [S, his own words, dated]:
  - "I usually totally replace the kick and mix the snare 50 / 50."
  - He puts triggers on every drum for gate sidechains and replacement.
  - He uses little parallel compression ("maybe on snare") and tunes the kick very low.
  - Sources: [mixingtips forum archive](https://mixingtips.forumotion.com/t58-golden-tips-andy-sneap-reveals-some-of-his-techniques), [Scribd compilation](https://www.scribd.com/document/413896548/Andy-Sneap-Recording-Tips). Nail The Mix's profile has no direct quotes. [S2] [NTM](https://www.nailthemix.com/andy-sneap)
- **Will Putney** (Knocked Loose), from a Nail The Mix breakdown [S2] [NTM](https://www.nailthemix.com/putney-knocked-loose-drums-gating-eq):
  - Gates on the shells are **keyed from printed "spike" tracks**, with **0 attack and 500–600 ms hold/release**.
  - HPF: snare/toms ~60 Hz, cymbals/OH ~125 Hz, rooms ~60 Hz.
  - Kick: the real kick is tracked dark. Hardware EQ gives a "significant" boost at **3.2 kHz**, a **10 kHz** high shelf and a **60 Hz** low shelf.
  - Snare: tuned high, with three sample layers (reinforcement, a quiet fat "body" sample, a multi-mic SD3 sample).
  - He keeps separate **"Real Kick / Fake Kick / Real Snare / Fake Snare" buses**, plus a bus that sets how much of each feeds the rooms.
- **Adam "Nolly" Getgood** (Periphery; GGD co-founder):
  - He keeps the acoustic drums and reinforces kick and snare with samples. He uses parallel drum-bus compression with a **sidechain low-cut so the kick doesn't drive it**. [S] [ResearchGate chapter](https://www.researchgate.net/publication/394366502_04_Adam_'Nolly'_Getgood), [CreativeLive Studio Pass](https://www.creativelive.com/class/studio-pass-periphery-with-adam-nolly-getgood-matt-halpern/lessons/mix-bus-compression-saturation)
  - Numbers from a third-party summary of his "Wildfire" mix [S2, unconfirmed] ([nound.io](https://www.nound.io/v?bit=parallel-compression-for-aggressive-drum-punch-vDcNnGjMxrk)): FG-Stress at **20:1**, sidechain HPF, "slow" attack and fast release, **~32 % mix**, then boosts at **5.5 kHz and 12 kHz** and optionally **60 Hz**.
- **Andy Wallace** (Gojira *Fortitude*, Linkin Park):
  - Mario Duplantier said Wallace understood Gojira wanted "a very organic bass and a real snare". [S] [Mixdown](https://mixdownmag.com.au/features/the-reality-of-the-band-is-we-try-to-find-a-harmony-between-us-gojiras-mario-duplantier-on-the-bands-new-album-fortitude/)
  - On Linkin Park he blended one or two kick samples and **checked phase between the mic'd and sampled kick first**. [S] [SOS Inside Track: Linkin Park](https://www.soundonsound.com/techniques/inside-track-linkin-parks-hunting-party)
- **Metallica, *Hardwired*:** samples add "maybe just 10 percent"; the real kick and snare mics carry the sound. Expander/gate and HPF were the key kick plug-ins. [S] [SOS Inside Track: Moth Into Flame](https://www.soundonsound.com/techniques/inside-track-metallica-moth-flame)
- **Not researched in depth:** Trivium (Wilbur mixed *The Sin and the Sentence* [S] [Discogs](https://www.discogs.com/release/11107058-Trivium-The-Sin-And-The-Sentence)), Sylosis and Gojira's *Magma* (mixed by Johann Meyer [S] [Wikipedia](https://en.wikipedia.org/wiki/Magma_(Gojira_album))). I found no numeric drum settings from those engineers. **[?]**

---

## 2. Concrete numeric settings

The most complete published modern-metal drum mix with numbers is **Mark Mynett's "Making Modern Metal: Part 3", Sound On Sound, March 2018** ([link](https://www.soundonsound.com/techniques/making-modern-metal-part-3)). Mynett lectures at Huddersfield and wrote *Metal Music Manual*. Values below marked **(Mynett)** come from it [S].

### 2.1 Kick
| Item | Value | Source |
|---|---|---|
| Port-hole (outside) mic EQ | **HPF 120 Hz**; broad deep cut at **265 Hz** | Mynett [S] (gain of the "deep" cut not given) |
| Attack (inside/beater) mic EQ | broad deep cut at **541 Hz**; wide boost at **7.5 kHz** | Mynett [S] |
| Each kick mic compression | high ratio, medium attack, **4–6 dB GR** | Mynett [S] |
| Kick group | **4–5 dB GR**, medium attack, fast release; broad **−4 dB at 400 Hz** | Mynett [S] |
| Kick sample EQ | cut ~**400 Hz**, lift at **3.8 kHz** | Mynett [S] |
| Click / presence | boost at **3.2 kHz**, high shelf **10 kHz**, low shelf **60 Hz** | Putney [S2] |
| Generic kick compression | attack **10–30 ms**, release **50–150 ms** | [majormixing](https://majormixing.com/parallel-drum-compression/) / [iZotope](https://www.izotope.com/community/blog/mixing-metal-sessions) [S2] |
| Subkick | 1176, high ratio, fast attack/release, aggressive GR, automated per section | Mynett [S] |

### 2.2 Snare
| Item | Value | Source |
|---|---|---|
| Top: hi-hat spill | de-ess at **4.5 kHz** | Mynett [S] |
| Top/bottom gating | "hard-gated" (SSL G2 gate/expander; threshold not given) | Mynett [S] |
| Bottom compression | **5–6 dB GR**, medium attack | Mynett [S] |
| Bottom EQ | **+~2 dB high shelf at 8 kHz** (Pultec) | Mynett [S] |
| Parallel saturation aux | fed by both snare mics, mostly the top; harmonics emphasise upper mids/highs | Mynett [S] (no drive values) |
| Sample "crack" | ~**5 kHz** attack emphasis | Cervini via NTM [S2] |
| Snare/tom HPF | ~**60 Hz** | Putney [S2] |
| Post-parallel "thwack" / boxiness | +**200–400 Hz** / −**400–600 Hz** | [NTM parallel compression](https://www.nailthemix.com/parallel-compression) [S2] |

### 2.3 Toms (Mynett [S]; Tube-Tech CL1B **6:1, medium attack, medium release**)
| Tom | Cut | Low boost | High boost |
|---|---|---|---|
| Floor (lowest) | 534 Hz | **+4.5 dB @ 85 Hz** | **+3.9 dB @ 3.33 kHz** |
| Low-mid | 402 Hz | **+5.2 dB @ 115 Hz** | **+6.1 dB @ 3.85 kHz** |
| Mid | 670 Hz (deep) | 133 Hz | 3.96 kHz |
| High (smallest) | 790 Hz (deep) | 183 Hz | 4.52 kHz |

Toms were **edited, not gated**: "edits are far more accurate than noise gates" (Mynett [S]). Putney gates his toms with spike keys instead (0 ms attack, 500–600 ms hold/release [S2]). Wilbur's tom move is a mid dip [S].

### 2.4 Cymbals / overheads / hi-hat
- Mynett put an **HPF at 550 Hz** on each metalwork mic and disabled the dedicated hi-hat channel, because the other mics caught enough hat [S].
- Putney's OH/cymbal HPF is ~**125 Hz** [S2].
- Keep cymbals **out of** parallel crush buses: they get harsh when crushed (Mynett's parallel feed excluded metalwork [S]; also [iZotope](https://www.izotope.com/community/blog/mixing-metal-sessions) [S2]).

### 2.5 Rooms
- Mynett (Coles 4038 Blumlein pair, compression printed while tracking): **HPF 185 Hz**, **LPF 2 kHz at 6 dB/oct**, hard-panned [S].
- Putney: HPF ~**60 Hz** [S2].
- "Trash"/crushed rooms: **10:1–20:1 or limiting** [S2] ([developdevice](https://developdevice.com/blogs/news/the-ultimate-drum-mixing-cheat-sheet-a-complete-guide-to-a-perfect-sound)). Fast attack and release make them pump; when crushed, cut the cymbal-heavy top [S] ([SOS Ambient Miking](https://www.soundonsound.com/techniques/ambient-miking-room-improvement)).

### 2.6 Parallel compression
| Recipe | Settings | Source |
|---|---|---|
| Mynett "smash" (Neve 33609) | comp **6:1, 100 ms release** (attack fixed) + limiter **2 ms attack / 50 ms recovery**; **~18 dB total GR**; post EQ low shelf +several dB **@ 190 Hz**, high shelf +several dB **@ 6.9 kHz**. Feeds: kick group, **snare samples only**, edited toms; **no metalwork** | [S] |
| NTM generic crush | **8:1 to ∞:1**, fast attack, fast–medium release, **10–20 dB GR**; HPF **100–150 Hz before** the compressor so the kick doesn't pump it; after: +**5–10 kHz**; blend up from −∞ until the kit thickens | [S2] [NTM](https://www.nailthemix.com/parallel-compression) |
| Nolly drum-bus parallel | **20:1**, sidechain HPF, slower attack, fast release, **~32 % mix**; post +5.5 kHz / +12 kHz | [S2, unconfirmed] |
| Moderate | **4:1–10:1**, 0–3 ms attack, release that recovers before the next hit (100–250 ms at 120 BPM), 10–20 dB GR | [S2] ([majormixing](https://majormixing.com/parallel-drum-compression/)) |

### 2.7 Drum bus / mix bus
- Drum bus (Cervini, per NTM): SSL-style, **4:1, 30 ms attack, 100 ms/Auto release, 3–4 dB GR** [S2].
- Mix bus (Mynett): SSL **2:1, 10 ms attack, Auto release, 3–4 dB GR on peaks** [S].
- Wilbur's whole two-bus is "one compressor" [S].

### 2.8 Reverb (Mynett [S])
- **Plate** (EMT 140): **10 ms pre-delay, 850 ms decay**, automated longer when the beat leaves space.
- **Room** (LX480): **15 ms pre-delay, 1.4 s decay**, with **+7.5 dB high shelf at 11 kHz** before the reverb.
- **Sends:** snare samples + snare bottom; toms (less from the floor toms); small amounts of kick samples, metalwork and the parallel bus.
- **Nonlinear/gated:** no metal source gave numbers **[?]**.

### 2.9 Loudness of modern metal masters
| Release / set | Integrated | Notes | Source |
|---|---|---|---|
| Gojira *Magma* (Ted Jensen) | album avg **−9.3 LUFS**, loudest track −6.9 LUFS, DR 7 | 24-bit master | [S] [ProStudioMasters](https://www.prostudiomasters.com/album/page/33408) |
| Mynett's album (CD master) | **~−6.5 LUFS**, PLR ~7.5 | He compares it with Metallica's *Hardwired* and Kreator's *Gods of Violence*; separate −16 LUFS streaming version | [S] [SOS](https://www.soundonsound.com/techniques/making-modern-metal-part-3) |
| 16 "Modern & Industrial Metal" releases | median **−7.0 LUFS** (IQR −7.8 to −6.4) | BS.1770 | [S] [polished.media](https://polished.media/metal-mastering-loudness/) |
| Gojira *Fortitude* | DR 5 (reviewer measurement) | | [S2] [Angry Metal Guy](https://www.angrymetalguy.com/gojira-fortitude-review/) |
| Streaming | Spotify normalises to about −14 LUFS; for masters louder than that, true peak ≤ −2 dBTP | | [S2] [polished.media](https://polished.media/metal-mastering-loudness/) |

I did **not** find measurements for the Megadeth or Spiritbox records **[?]**. Measure them yourself from the purchased files (e.g. with ffmpeg `ebur128`).

---

## 3. Common workflows

1. **Replace vs. blend.** The spectrum runs:
   - Sneap: kick 100 % replaced, snare 50/50 [S].
   - Cervini, Putney, Nolly and Wilbur-era Lamb of God: real drums plus several sample layers [S/S2].
   - Metallica *Hardwired*: ~10 % samples [S].
   - Megadeth 2026: "as organic and natural as possible" [S].
   - Spiritbox/Braunstein: programmed SD3 plus up to 8+ samples per drum [S2].

   Sample-heavy is the modern-metalcore default; the Megadeth 2026 target sits near the organic end.
2. **Separate real and sample paths.** Putney's Real/Fake buses [S2] and Mynett feeding *only snare samples* into the crush bus [S] both process samples and mics separately. Each source then gets its own crush, and you control how much of each excites the rooms and reverbs.
3. **Alignment.** Align spot mics to an anchor: Mynett used the kick port-hole and snare top, with Sound Radix Auto-Align [S]. Phase-align samples to the mics [S]. Check mic vs. sample kick phase before anything else (Wallace [S]). Triggering from MIDI (DRT/Trigger) brings a fixed trigger latency, so align samples by ear or by correlation **[I]**.
4. **Snare top/bottom.** The bottom mic is normally out of polarity with the top, and is often a few samples late too. Flip it, then nudge it for the crispest bite. Rule of thumb: about **1 ft ≈ 1 ms** [S] ([SOS Mix Rescue: Phase](https://www.soundonsound.com/techniques/mix-rescue-phase-relationships), [SOS Multi-mic](https://www.soundonsound.com/techniques/making-multi-mic-recordings-work), [Sonnox](https://sonnox.com/articles/drum-phase-alignment-when-to-nudge-flip-or-leave-alone)).
5. **Kick click vs. body.** Click lives at **3.2–7.5 kHz** (Putney 3.2k, Mynett 3.8k sample / 7.5k attack mic). Body/sub is at **50–80 Hz**, shelf at 60 Hz. The box is **265–541 Hz**: cut it [S].
6. **Toms: edit vs. gate.** Mynett edits by hand [S]. Putney gates with spike keys, 0 attack, 500–600 ms [S2]. Sneap gates aggressively from trigger sidechains [S]. A trigger-keyed gate gives the accuracy of editing without manual work **[I]**.
7. **Parallel crush.** 10–20 dB GR is the norm; Mynett used ~18 dB [S]. Keep cymbals out, filter the kick out of the detector, and blend from silence [S/S2]. The typical Nolly-style mix is around a third [S2].
8. **Sub / fundamental trick.** Gate a sine (or tone) keyed by the kick: about **C2 ≈ 65 Hz**, or the nearest in-key note in **50–80 Hz** ([Music Guy Mixing](https://www.musicguymixing.com/sine-wave-kick-drum/) [S2]; [Tenth Egg](http://www.tenthegg.co.uk/tips/t005/) says 60–100 Hz). Gate release sets the tail. Optionally delay the gate so the beater gets through first. Mix it so it is felt more than heard [S2]. Signal's gated band-pass chain is the same idea applied to the drum signal.

---

## 4. Apply to Signal — per-lane starting settings

Two targets:
- **O = "Organic"** (Megadeth 2026-ish: Verbeuren's stated goal).
- **M = "Modern polished"** (Spiritbox/Cervini/Putney-ish).

Values are starting points. The last column says where each comes from. "Deep" cuts with no published gain are set by me at −6 dB **[I]**.

### 4.1 Kick
| Lane / block | Parameter | Value | Basis |
|---|---|---|---|
| Kick In · EQ | bell cut | 541 Hz, −6 dB, Q 1.0 | freq [S Mynett]; gain/Q [I] |
| | bell boost (click) | 7.5 kHz, +3 dB, Q 0.7 (O) / +5 dB (M) | freq [S Mynett]; gain [I] |
| Kick In · Comp | ratio / atk / rel / GR | 6:1 / 15 ms / 80 ms / 4–6 dB | GR [S Mynett]; ratio/ms [I] within [S2] 10–30 / 50–150 |
| Kick Out · EQ | HPF | 120 Hz (if a sub lane carries the lows); else 30–40 Hz | [S Mynett] / [I] |
| | bell cut | 265 Hz, −6 dB, Q 1.0 | freq [S]; gain [I] |
| Kick Out · Comp | | as Kick In | [S]/[I] |
| Kick section bus · EQ | bell cut | 400 Hz, −4 dB, broad (Q ≈ 0.7) | [S Mynett] |
| | low shelf / presence / air | 60 Hz +2–3 dB; 3.2 kHz +3 dB; high shelf 10 kHz +2 dB | freqs [S2 Putney]; gains [I] |
| Kick section bus · Comp | | 4:1, 20 ms, 60 ms, 4–5 dB GR | GR & medium/fast [S Mynett]; ms [I] |
| Kick sample (GGD M&M2) · EQ | | −3 dB @ 400 Hz; +3 dB @ 3.8 kHz | freqs [S Mynett]; gains [I] |
| Kick sample · blend | sample:mic | **O ≈ 30–40 % sample / M ≈ 60–100 %** (Sneap 100 %) | range [S]; split [I] |
| Kick sample · alignment | | align the sample transient to the Kick In onset, check polarity against Kick Out | [S] practice; method [I] |
| Kick sub chain · gate (trigger key) | thr / atk / hold / rel | key on the trigger; 0 ms attack; optional 2–5 ms open delay; hold 30 ms; release 120–200 ms (shorter for blast beats) | 0 atk [S2 Putney]; delay idea [S2 MGM]; ms [I] |
| Kick sub chain · band-pass | centre / Q | **50–65 Hz** (nearest in-key note), Q 2–4 | [S2] MGM/Tenth Egg; Q [I] |
| Kick sub chain · saturation | drive / mix | light drive, so the 2nd harmonic (100–130 Hz) reads on small speakers | [I] |
| Kick sub chain · level | | −10 to −6 dB under the kick; "felt not heard" | [S2] / [I] |

### 4.2 Snare
| Lane / block | Parameter | Value | Basis |
|---|---|---|---|
| Snare Top · EQ | HPF | 60 Hz | [S2 Putney] |
| | hat spill | 4.5 kHz −2 to −3 dB narrow (Signal has no de-esser) | freq [S Mynett]; static cut [I] |
| | body / boxiness | +2 dB @ 200 Hz (O) ; −3 dB @ 450 Hz if boxy | ranges [S2 NTM]; picks [I] |
| Snare Top · Gate (trigger key) | | 0 ms attack, hold+release 500 ms total (O: range ≥ −15 dB, not a full close) | [S2 Putney]; range [I] |
| Snare Top · Comp | | 4:1, 10 ms (lets the stick attack through), 80 ms, 3–5 dB | Nolly "highlight stick attack" [S]; numbers [I] |
| Snare Bottom · polarity / delay | | **invert**; nudge so it lines up with the top (≈ 0.1–0.5 ms) | [S] SOS/Sonnox; range [I] |
| Snare Bottom · EQ | HPF / high shelf | HPF 150 Hz [I]; **+2 dB shelf @ 8 kHz** | shelf [S Mynett] |
| Snare Bottom · Comp | | 4:1, 15 ms, 80 ms, **5–6 dB GR** | GR [S Mynett]; rest [I] |
| Snare Bottom · Gate | | trigger-keyed, same as top ("hard-gated") | [S Mynett] |
| Snare sample · blend | | **O ≈ 25–40 % / M ≈ 50 %+** (Sneap 50/50; Braunstein multi-layer) | [S]/[S2]; split [I] |
| Snare sample · EQ | | +2–3 dB @ 5 kHz "crack" | [S2 Cervini/NTM] |
| Snare fundamental chain | band-pass | the snare's tuned fundamental, typically **~170–250 Hz**: measure it per kit piece; Q 3 | [I] (no source gave a snare fundamental) |
| | gate | trigger key, 0 ms attack, release 80–150 ms | [I] |

### 4.3 Toms (per tom; gate keyed by trigger)
| Tom | HPF | Cut | Low boost | High boost | Comp | Basis |
|---|---|---|---|---|---|---|
| Floor | 50 Hz | 534 Hz −6 dB | +4.5 dB @ 85 Hz | +3.9 dB @ 3.33 kHz | 6:1, 20 ms, 150 ms, 3–6 dB GR | EQ [S Mynett]; HPF [S2]/[I]; ms [I] ("medium/medium" [S]) |
| Low-mid | 60 Hz | 402 Hz −6 dB | +5.2 dB @ 115 Hz | +6.1 dB @ 3.85 kHz | same | [S]/[I] |
| Mid | 60 Hz | 670 Hz −6 dB | +4 dB @ 133 Hz | +4 dB @ 3.96 kHz | same | freqs [S]; gains [I] |
| High | 60 Hz | 790 Hz −6 dB | +4 dB @ 183 Hz | +4 dB @ 4.52 kHz | same | freqs [S]; gains [I] |
| Gate (all) | — | — | — | — | 0 ms attack, hold 200 ms + release 300–400 ms (≈ Putney's 500–600 total); floor tom longer | [S2]/[I] |
| Fundamental chain | — | — | band-pass at the "low boost" freq above, Q 3 | — | — | [I]: Mynett's boosts are probably close to each tom's fundamental |

### 4.4 Cymbals, rooms
| Lane | Settings | Basis |
|---|---|---|
| OH | HPF **125 Hz (M)** … **550 Hz (O-dark / cymbals-only)**; for Megadeth-organic, where the OHs carry kit image, use 125–200 Hz | [S2 Putney] / [S Mynett]; choice [I] |
| Hi-hat | HPF 250–400 Hz; **often muted** if the OHs carry enough hat | mute [S Mynett]; HPF [I] |
| Ride | HPF 250–400 Hz; −2 dB @ 2–3 kHz if it clangs | [I] |
| Rooms (natural) | HPF **185 Hz**, LPF **2 kHz @ 6 dB/oct**, hard-panned; light comp | [S Mynett] |
| Rooms (crushed / "trash" via the Smash bus or a room lane) | HPF 60–100 Hz, 10:1–20:1, fastest attack, release 50–100 ms, LPF 6–8 kHz | ratios [S2]; HPF [S2 Putney]; rest [I] |
| Rooms level | O: higher (organic target) / M: tucked | [I] from [S] Verbeuren |

### 4.5 Parallel buses
| Bus | Feeds | Compressor | EQ | Mix / level | Basis |
|---|---|---|---|---|---|
| **Tight** (glue) | all shells + some OH | **4:1, 30 ms, 100 ms (or the auto-release equivalent), 3–4 dB GR** | — | 100 % in series on the drum bus, or 50 % parallel | [S2 Cervini/NTM] |
| **Punch** (Nolly-style) | kick, snare, toms, rooms | **20:1**, attack 10–20 ms, release 30–50 ms, 8–12 dB GR | **pre-HPF 100–150 Hz** (Signal's compressor has no sidechain filter, so filter the send and restore the lows with a post shelf); post +2–3 dB @ 5.5 kHz & 12 kHz, optional +2 dB @ 60 Hz | **~30 %** | [S2] Nolly/NTM; ms [I] |
| **Smash** (Mynett 33609) | kick group, **snare samples**, toms; **no cymbals** | **6:1, 100 ms release, fastest attack; then a second stage ∞:1 / 2 ms / 50 ms (limiter)**; **~18 dB total GR** | post low shelf +3–4 dB @ **190 Hz**, high shelf +3–4 dB @ **6.9 kHz** | blend from −∞; typically −12 to −6 dB under dry | [S Mynett]; shelf gains & level [I] |
| **Crunch** (parallel saturation) | snare top/bottom (mostly top), snare samples | preamp saturator, heavy drive; HPF 200 Hz, LPF 10 kHz on the return | emphasise upper mids/highs | low; ride per section | Mynett's Vertigo aux [S]; NTM saturation before/after [S2]; values [I] |

Signal gap to note **[I]**: several recipes rely on a **sidechain HPF** (Nolly, NTM) or a **de-esser** (Mynett's snare). A sidechain-filter option on the compressor would remove the pre-HPF-and-shelf workaround.

### 4.6 Drum bus, sends
| Block | Settings | Basis |
|---|---|---|
| Drum bus comp | 4:1, 30 ms, 100 ms/auto, 3–4 dB GR (O: 2–3 dB) | [S2 Cervini] |
| Drum bus saturation | preamp, gentle (≤ 1 dB peak reduction) | [I] |
| Plate send | **10 ms pre-delay, 850 ms decay**; snare (samples + bottom), toms | [S Mynett] |
| Room send | **15 ms pre-delay, 1.4 s decay, +7.5 dB high shelf @ 11 kHz pre-reverb**; small kick-sample, metalwork and parallel-bus sends | [S Mynett] |
| Nonlinear / gated | snare only, 250–400 ms, pre-delay 0–10 ms, low level, mainly M-target and fills | [I] (no metal source) |
| Headroom target | leave the drum bus peaking around −6 dBFS; the master targets **−9 to −6.5 LUFS** (organic toward −9, Megadeth/Gojira-Jensen territory; modern −7) | LUFS [S] Magma/Mynett/polished; bus peak [I] |

### 4.7 Two preset deltas at a glance [I]
| Control | Organic (Megadeth 2026) | Modern (Spiritbox/Cervini) |
|---|---|---|
| Kick sample share | 30–40 % | 60–100 % |
| Snare sample share | 25–40 % | ≥ 50 %, multi-layer |
| Rooms | natural, prominent | crushed, tucked |
| Smash bus | low | moderate–high |
| Gates | expander-like (range −15 dB) | hard close |
| Click EQ | +3 dB | +5 dB |

---

## 5. Gaps and caveats
- No published interview with **Matt Hyde** (Megadeth 2026) or **Zakk Cervini** about these specific records. Their sections rest on secondary analyses and on what the band and producers said.
- The Discogs and AudioTechnology pages returned 403 to me. Those claims come from search snippets.
- The Nolly numbers (20:1 / 32 %) come from a single third-party summary.
- Sneap's quotes are 15+ years old.
- The Mynett figures come from one album he produced (Damnation's Hammer). They are well documented, but they are not the target records.
- I made no loudness measurements of the Megadeth or Spiritbox records.
- All fundamental-frequency values for the snare/tom sub chains are inference: measure each GGD M&M2 kit piece (FFT of a single hit) and tune to that.
