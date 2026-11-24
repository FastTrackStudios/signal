# Mixing techniques: Dan Worrall's parallel filter series

The catalogue covers five videos, all by Dan Worrall (uploader "Dan Worrall"). Transcripts are in `transcripts/`.

| ID | Title | Length | Captions |
|---|---|---|---|
| RL4KDVFlkUg | F**k Tha Phase Police: Use Parallel EQ | 9:24 | auto-generated (YouTube ASR) |
| le7oVIrt9oU | Parallel Filter Tricks: Highpass and Lowpass Filters | 11:31 | human (en-GB) |
| 4hGBYs10ZAU | Parallel Bandpass Filters | 14:54 | auto-generated (YouTube ASR) |
| O8zIJ0A--lI | Parallel Filters, Pro Level: Flip It And Reverse It | 14:44 | human (en-GB) |
| QyZbIbjvQVE | Parallel Filters, God Tier: The Delta Channel | 10:38 | human (en-GB) |

The table is in teaching order. Each video builds on the one before it.

**Conventions**
- **Says:** text is the speaker's claim, paraphrased closely. Text in quotation marks is verbatim from the transcript. Quotes from auto-captioned videos may have ASR errors; for example, the ASR writes "cue" for "Q".
- **Interpretation:** marks my reading, not his.
- "Unity" means the parallel return and send are at 0 dB, so dry + filtered sum 1:1.
- His tools are FabFilter Pro-Q 3 (zero-latency, natural or linear phase), Plugin Doctor and DDMF Metaplugin for measuring, Reaper and ReaEQ, Pro-G / ReaGate, FabFilter Saturn and Volcano 3. Settings are given as he states them. Where he gives no number, the entry says so.
- **Phase** is the phase in your model. Where a technique could sit in two phases, the entry shows the primary phase first and gives the reason.

---

## 0. Foundations: what a parallel filter sums to

These entries are the theory the techniques rest on. They are not techniques themselves, but the settings in later entries depend on them.

### 0.1 Parallel bell and shelf EQ is safe; phase shift follows the slope of the curve
- **Phase:** Tone (it governs EQ placed on any parallel channel)
- **Says:** A minimum-phase bell or shelf has no phase shift at its target frequency. "the phase shift is related to the gradient of the frequency response graph rather than the amount of boost or cut". Bells and shelves never get near 180°, so "there's no possible gain setting at which mixing this with the dry signal will result in a notch filter". With dry and wet each at −6 dB, a +6 dB bell at 500 Hz on the wet path gives a normal bell of less than 6 dB in the sum. Linear phase only makes the bell "slightly wider", and it doesn't change the amount of boost. A cut behaves the same way. Shelves in parallel come out "a very slightly higher shelf" (high shelf) or "a tiny bit lower" (low shelf) in minimum phase.
- **Caveats:** HP and LP filters are different. Their phase shift is large near and below the cutoff, and at 180° a parallel sum can null completely. "you do also need to be careful with those filters when dealing with a multi-mic drum recording." He calls the claim that you need linear-phase EQ on multi-mic drums "laughable", and gives as evidence hundreds of kits he mixed on analog EQs and the drum records of the 1970s and 80s.
- **Source:** [Phase Police 03:04](https://youtu.be/RL4KDVFlkUg?t=184), [04:05](https://youtu.be/RL4KDVFlkUg?t=245), [05:05](https://youtu.be/RL4KDVFlkUg?t=305), [01:03](https://youtu.be/RL4KDVFlkUg?t=63), [08:11](https://youtu.be/RL4KDVFlkUg?t=491)

### 0.2 Parallel HP or LP filter = shelf boost; the slope decides whether it is safe
- **Phase:** Tone
- **Says:** With a 12 dB/oct HPF summed with dry at unity, you get roughly a 6 dB high shelf with a small pre-rise dip, "well under 2dB of cut". He says this "often works better than just a straightforward high shelf boost". At 24 dB/oct the dip becomes a sharp cut of nearly 12 dB, which he calls "useless, in practical terms". Steeper than that, "really weird stuff starts to happen". He gives three ways to stay safe:
  1. **Linear phase:** any slope, up to brickwall, gives a clean shelf. The cost is latency and CPU. He uses this option least. He avoids latency where he can because of delayed playback, delayed parameter changes and meters out of sync with the audio.
  2. **6 dB/oct (one pole), zero latency:** "a perfectly smooth high shelf boost", with no Q to set. It matches a conventional 6 dB/oct high shelf placed slightly lower in frequency.
  3. **12 dB/oct with Q below 1 (Pro-Q 3 units):** Q above 1 "there be dragons", meaning a resonant bump in the shelf and a sharp dip below the cutoff. With Q below 1 you get a gentle shelf boost plus a gentle dip a little higher, which is the low-shelf use in 2.3.
- **Settings:** In Pro-Q 3, Q 1 equals about 0.72 in other plugins. "if you're not using Q3 remember to stay below about 0.7". Return gain: unity gives +6 dB, lower gives a subtler boost, above unity gives more than 6 dB.
- **Source:** [HP/LP 01:05](https://youtu.be/le7oVIrt9oU?t=65), [01:43](https://youtu.be/le7oVIrt9oU?t=103), [02:57](https://youtu.be/le7oVIrt9oU?t=177), [04:07](https://youtu.be/le7oVIrt9oU?t=247), [08:17](https://youtu.be/le7oVIrt9oU?t=497), [10:09](https://youtu.be/le7oVIrt9oU?t=609)

### 0.3 Parallel band-pass = bell boost; use 12 dB/oct
- **Phase:** Tone
- **Says:** A 12 dB band-pass summed at unity is "just an eq boost… exactly the kind of bell-shaped boost you'd expect", at "any cue setting". At its centre the filter has unity gain and zero phase shift, so the sum is +6 dB (1+1=2). In zero latency there is some phase shift, but it "just makes the bell a little bit narrower". At 24 dB you get "little dips either side of our boost". He calls that shape potentially useful, but "whenever I've tried it I've always ended up back at the 12 dB setting… don't go any higher unless strange funkiness is what you're after."
- **Source:** [Bandpass 02:05](https://youtu.be/4hGBYs10ZAU?t=125), [03:07](https://youtu.be/4hGBYs10ZAU?t=187); [Flip 00:02](https://youtu.be/O8zIJ0A--lI?t=2)

### 0.4 Inverting the filtered path
- **Phase:** Tone, Relational or Creative, depending on use (see section 4)
- **Says:** A band-pass with polarity flipped, at unity, gives "a hard notch, with infinite attenuation at the cutoff", not a −6 dB cut (1−1=0). Turning the inverted return down morphs the notch into a gentle cut. Turning it above unity flattens the response again (about flat at +6 dB), but that leaves 180° shift at the target, which makes an all-pass. He calls that "probably pointless" and says: "keep the filtered signal at or below unity gain when inverting like this."
  - An inverted notch gives a band-pass. In **linear phase**, an inverted LP gives an exact HP and an inverted HP gives an exact LP, at any slope or Q.
  - In **minimum phase**, an inverted 12 dB LP gives a lumpy HP with a "weird resonant bump". At 24 dB it gives "a vicious 6dB spike", and steeper slopes add notches.
  - **Exception:** "a 1 pole lowpass filter will invert to create a perfect 1 pole highpass filter". With the inverted return turned down, it becomes a low-shelf cut. "Don't underestimate how useful this can be!"
- **Source:** [Flip 00:49](https://youtu.be/O8zIJ0A--lI?t=49), [01:31](https://youtu.be/O8zIJ0A--lI?t=91), [07:14](https://youtu.be/O8zIJ0A--lI?t=434), [07:47](https://youtu.be/O8zIJ0A--lI?t=467), [09:51](https://youtu.be/O8zIJ0A--lI?t=591), [11:14](https://youtu.be/O8zIJ0A--lI?t=674)

### 0.5 Dynamics on a parallel filter channel are mirrored when the path is inverted
- **Phase:** Tone (or Relational when it spans several sources)
- **Says:**
  - On a normal (boosting) parallel band, compression makes that band's boost dynamic. It is parallel compression of an isolated band, so "all the changes to the compression curves that happen when you run a compressor in parallel will be happening here, except only affecting the high frequencies" ([HP/LP 07:39](https://youtu.be/le7oVIrt9oU?t=459)).
  - On a parallel low shelf from a Q<1 12 dB LPF, the boosted sub is compressed, while the dipped upper-bass region "will be moving in the opposite direction… expanded instead of compressed" ([HP/LP 09:32](https://youtu.be/le7oVIrt9oU?t=572)).
  - On an **inverted** path, "we're also inverting the dynamics processing". A compressor becomes a downward expander or gate for that band, and a downward expander becomes gentle downward compression ([Flip 04:02](https://youtu.be/O8zIJ0A--lI?t=242), [06:36](https://youtu.be/O8zIJ0A--lI?t=396)).

---

## 1. Parallel bell and shelf EQ

### 1.1 EQ on an existing parallel compression channel
- **Phase:** Tone
- **What and why (says):** He does not recommend busting signals out to parallel EQ alone, because "a parallel eq doesn't profoundly change its behavior like a parallel compressor does… running just an eq in parallel will merely complicate things needlessly". But "if you've already got a parallel compressor channel running go ahead and eq that channel to your heart's desire". The EQ becomes "subtly dynamic as it will become more significant whenever the compressor gain rides up".
- **Signal flow:** dry track → send to a parallel channel: compressor + bell or shelf EQ → summed with dry. He doesn't state the order of EQ and compressor.
- **Settings:** none stated beyond "a shelf will just give you a shelf and a bell will just give you a bell".
- **Applies to:** any source with a parallel compressor.
- **Caveat:** HP and LP filters on that channel are not safe in the same way (0.1).
- **Source:** [Phase Police 07:09](https://youtu.be/RL4KDVFlkUg?t=429)

---

## 2. Parallel high-pass and low-pass (shelving) channels

### 2.1 Shared parallel high shelf for a group of parts (stacked vocals)
- **Phase:** Tone
- **What and why (says):** One parallel HPF replaces "half a dozen or a dozen individual EQ bands" when many channels need similar top-end lift. Unlike EQ on a subgroup, you can still set the brightness of each part with its send level, and "you can do this from the mixer window without having to open and close individual EQs".
- **Settings:** 6 dB/oct one-pole HPF, zero latency. He does not state the cutoff. Sends and return at unity give a +6 dB shelf. Turn the return down for less, or above unity for more than 6 dB.
- **Signal flow:** aux sends from each vocal → a return channel with only the HPF → summed in the mix. Parallel.
- **Applies to:** stacked harmony vocals in his demo. **Interpretation:** any group of like parts.
- **Source:** [HP/LP 05:13](https://youtu.be/le7oVIrt9oU?t=313), [05:44](https://youtu.be/le7oVIrt9oU?t=344), [06:36](https://youtu.be/le7oVIrt9oU?t=396)

### 2.2 Saturated or compressed parallel high band (exciter, dynamic high shelf)
- **Phase:** Tone
- **Says:** If you saturate after the HPF on the return, you are "only saturating the high frequencies that we're adding, so the result is more like an enhancer or exciter, generating extra high frequency harmonics as well as boosting those already present". If you compress after it instead, you get "a dynamic high frequency shelf". This differs from a dynamic EQ or multiband compressor on a subgroup because it is parallel compression of the band (0.5).
- **Signal flow:** sends → HPF → saturator and/or compressor → return. Parallel, with processing after the filter.
- **Settings:** no saturation or compression values stated.
- **Source:** [HP/LP 07:09](https://youtu.be/le7oVIrt9oU?t=429), [07:39](https://youtu.be/le7oVIrt9oU?t=459)

### 2.3 Parallel low shelf with dip, a one-band "Pultec trick", shared by drums and bass
- **Phase:** Tone
- **What and why (says):** A 12 dB/oct LPF with Q below 1 run in parallel gives a gentle low-shelf boost with a gentle dip a little higher. That is "kind of reminiscent of the Pultec low end trick, except we're achieving it with just one parallel band instead of two". It runs on an aux, not the master or a subgroup, so "any track that needs extra fat bottom end" can feed it. Optional saturation gives "thicker, richer low end as well as more low end". Optional compression makes it "a dynamic low shelf": the boosted sub compresses while the dipped upper bass expands, which "might help to preserve some punchy thump in the kick drum, while still clearing out clutter".
- **Settings:** 12 dB/oct LPF, Q below 1 in Pro-Q 3 (below about 0.7 elsewhere). He does not state the cutoff.
- **Signal flow:** aux sends from drums and two bass parts → LPF → optional saturation → optional compressor → return.
- **Applies to:** drums, bass.
- **Caveat:** Q above 1 gives a resonant bump and a sharp dip. Use the Q conversion for non-FabFilter EQs.
- **Source:** [HP/LP 08:17](https://youtu.be/le7oVIrt9oU?t=497), [08:57](https://youtu.be/le7oVIrt9oU?t=537), [09:32](https://youtu.be/le7oVIrt9oU?t=572), [10:09](https://youtu.be/le7oVIrt9oU?t=609)

---

## 3. Parallel band-pass channels (the drum technique)

> Note: you described this as a "one-note" track. Worrall doesn't use that term in these videos; he says "parallel bandpass filter tuned to the snare fundamental" and "parallel filter channel for every drum".

### 3.1 Shared upper-mid band for all drum shells, on a fader
- **Phase:** Tone
- **What and why (says):** A band-pass at about 2.5 kHz fed by all drum shells: three kick mics, three snare mics and all the toms. With it, "now I can control the crucial upper mid-range region of my drums with a fader". You can saturate it ("add more sizzle") and compress it "to shape the upper mid-range transients and stop it ever getting too extreme". He says this is "just a colorful dynamic eq boost", which also suits non-drum parts, and is "only subtly different to a colorful dynamic eq applied to the drum buss."
- **Settings:** BP about 2.5 kHz at 12 dB/oct. He does not state the Q, saturation or compressor values.
- **Signal flow:** sends from every shell mic → BP → saturation → compression → return.
- **Source:** [Bandpass 03:07](https://youtu.be/4hGBYs10ZAU?t=187), [04:08](https://youtu.be/4hGBYs10ZAU?t=248)

### 3.2 Snare fundamental: parallel band-pass instead of a narrow EQ boost
- **Phase:** Tone. He frames it as rescuing a weak recording, but by your model it is a parallel filter, so it goes in Tone.
- **What and why (says):** The usual symptom of a bad drum recording is "thin and weak sounding drums". That can come from mic placement, phase cancellation between mics, poor tuning or the room. His "more technical diagnosis would probably be not enough low fundamental". He disagrees with "you can't polish a turd": it depends on diagnosing the problem correctly. A drum's fundamental "generally doesn't change much", so it is easy to find on an analyser.
  - **Why not an insert EQ:** a narrow, large boost rings, which lengthens the partial and is useful here. But "getting a satisfying extra resonance means too much boost and a drum that sounds tubby and boxy."
  - **Why parallel:** with the snare mics sent to a band-pass tuned to the fundamental, "I can control the ringing by adjusting the q of the filter and I can also independently control how much of that ringing I add."
- **Settings:** BP tuned to the snare's low fundamental, found on an analyser. He does not state a frequency. It is narrow, picking "out just that one partial". He sets the level ballpark with the plugin's gain, then fine-tunes with the fader in context. His stem-mastering snare fix was "literally just this… no other processing whatsoever."
- **Signal flow:** snare close mics → send → BP → return.
- **Source:** [Bandpass 04:08](https://youtu.be/4hGBYs10ZAU?t=248), [05:09](https://youtu.be/4hGBYs10ZAU?t=309), [06:09](https://youtu.be/4hGBYs10ZAU?t=369), [07:09](https://youtu.be/4hGBYs10ZAU?t=429)

### 3.3 Over-resonant band-pass, then a gate or expander (the gated "one-note" layer)
- **Phase:** Tone
- **What and why (says):** "sometimes I like to really lean into that ringing with the tightest possible cue [Q] setting but then tighten it up again with a gate or expander after it. This can give a really solid and consistent fat bottom end for every snare hit."
- **Settings:** tightest Q, gate or expander after the BP. He does not state a threshold, range or attack. Toms get a slower gate release than the snare, and are "more likely" to get compression too "in case the toms seem to vary in tone too much."
- **Signal flow:** drum mics → send → BP (tight) → gate or expander → optional compressor → return.
- **Applies to:** snare and toms. Each drum needs its own channel because each has its own fundamental, "but often it's just one tom out of the three that wasn't tuned well."
- **Source:** [Bandpass 08:22](https://youtu.be/4hGBYs10ZAU?t=502)

### 3.4 Kick sub: parallel band-pass in place of a sub-kick mic, with auto gain for Q sweeps
- **Phase:** Tone
- **What and why (says):** He no longer bothers with an NS10 sub-kick mic "since I discovered parallel band pass filters for the kick drum". It is easier, saves an input when tracking, and "works better in my opinion". With Pro-Q 3's output auto gain on, he can sweep Q from wide to narrow "without having to adjust the gain or the fader level each time". Wide gives "deep low frequency extension" like a sub-kick mic. Narrow makes the fundamental ring, and a gate then tightens it into "a lovely solid consistent note for every hit".
- **Settings:** BP on the kick's low fundamental (frequency not stated). **Before** enabling auto gain, drop the output gain "by quite a lot", because Pro-Q 3's auto gain "gets rather carried away when you use a bandpass filter like this". Gate optional at narrow Q.
- **Signal flow:** kick mics → send → BP (auto gain) → optional gate → return.
- **Source:** [Bandpass 09:25](https://youtu.be/4hGBYs10ZAU?t=565), [10:26](https://youtu.be/4hGBYs10ZAU?t=626)

### 3.5 VCA over all parallel drum filter channels
- **Phase:** Balance (a fader move over Tone channels)
- **Says:** A VCA controls kick BP, snare BP, three tom BPs and the 2.5 kHz shells band together. Of the difference he says "that's not subtle is it", and it is greater still on drums that are not "well-tuned expertly played… in a nice big studio".
- **Source:** [Bandpass 10:26](https://youtu.be/4hGBYs10ZAU?t=626), [11:29](https://youtu.be/4hGBYs10ZAU?t=689)

### 3.6 Kick into an 808-style bass via a maxed-Q band-pass and foldback saturation
- **Phase:** Creative
- **Says:** Max the Q, "lose the gate or maybe add hold and release to keep it open for longer", and the kick becomes an 808-style bass. He distorts it with Saturn's foldback wave-shaper, "so that the dynamics of the parts sound almost like filter sweeping effects", then compresses it.
- **Caveat (says):** Linear-phase band-passes allow steeper slopes without the response "funkiness". But with audible ringing, half the ringing moves before the hit and "fades up into each hit a bit like a reverse reverb effect". That can suit abstract work, though it needed the maximum latency setting at such a low frequency. "you definitely want to avoid linear phase if you're just trying to achieve a fat round drum sound."
- **Source:** [Bandpass 11:29](https://youtu.be/4hGBYs10ZAU?t=689), [12:32](https://youtu.be/4hGBYs10ZAU?t=752), [13:32](https://youtu.be/4hGBYs10ZAU?t=812)

### 3.7 Mentioned only: synthesise a missing fundamental, or retune a drum to the key
- **Phase:** Tone (or Rescue for the tuning aspect)
- **Says:** If a drum is "so poorly tuned [that] it doesn't really have a proper low fundamental… just pick a frequency and synthesize one". You may also be able to "change the fundamental frequency of the drum so it better fits the key". He doesn't demonstrate either and defers them to a possible future video.
- **Source:** [Bandpass 13:32](https://youtu.be/4hGBYs10ZAU?t=812)

---

## 4. Inverted (polarity-flipped) parallel filters

### 4.1 Inverted band-pass + compressor = band gate (drum box-region declutter)
- **Phase:** Tone. **Interpretation:** it functions as a dynamic corrective cut, so it overlaps with the Rescue EQ intent, but it is a parallel filter with stylistic dynamics.
- **What and why (says):** DMX drum samples (from NI Battery) "could definitely use a cut in the midrange, around the 5 or 600 Hz mark". All drum channels are sent to a polarity-flipped BP. At unity that is a hard notch, "a bit too extreme", and turned down it becomes a gentle cut. With the return back at unity and a compressor added after the BP, the compressor's gain reduction turns the notch into a gentle cut. The band now moves opposite to the compressor, so you get "a downward expander… more like a hard gate for that frequency". He says it "retain[s] some of the vintage midrange thump… while still tightening up and de-cluttering that boxy region". The subtler version turns the return down, so the response moves between a gentler and a harder cut, which is gentle downward expansion.
- **Settings:** BP about 500 Hz (he says "5 or 600 Hz"; Q not stated), polarity flipped in Pro-Q 3's output section. The compressor needs a threshold control and manual makeup gain at unity, so it returns to exactly unity when fully released. He picks "something that can eat the transients effortlessly". No ratio, attack or release stated.
- **Compressor-choice caveat (says):** Avoid fixed-threshold designs with input and output gain, such as T-Racks Comprexxor and 1176 emulations, because "this makes it really awkward to figure out where unity gain is". Avoid Waves H-Comp too, "because you can't turn off the automatic gain compensation".
- **Signal flow:** all drum channels → sends → BP (inverted) → compressor → return at unity.
- **Applies to:** drums. He also mentions "acoustic guitar, which can sound thin without some lower midrange, but can quickly clutter up that region".
- **Source:** [Flip 02:10](https://youtu.be/O8zIJ0A--lI?t=130), [03:07](https://youtu.be/O8zIJ0A--lI?t=187), [04:02](https://youtu.be/O8zIJ0A--lI?t=242), [04:47](https://youtu.be/O8zIJ0A--lI?t=287), [05:38](https://youtu.be/O8zIJ0A--lI?t=338)

### 4.2 Inverted broad band-pass + expander = shared dynamic low-mid scoop for the busy sections
- **Phase:** Relational. It is a cross-source dynamic cut where "the parts you send to this channel will all start to interact with one another in that frequency band". Tone is a secondary fit.
- **What and why (says):** In the part of the song "where everything is in together… the midrange starts to build up". Here that is around 300 Hz. You send every subgroup with content there (everything except drums) to an inverted broad BP. The return is turned down "a good deal" so that it makes a gentle scoop, not a notch. A downward expander follows, and because the path is inverted it acts as gentle downward compression: the cut is gentler when there's less energy in the region. With the channel out, "the low mids get all congested and boxy sounding".
- **Settings:** BP about 300 Hz, "nice broad gentle". Expander ratio "around 2:1 or perhaps a touch higher". Return well below unity. No threshold or time constants stated.
- **Signal flow:** sends from subgroups (not individual tracks) → BP (inverted) → downward expander → return.
- **Source:** [Flip 05:38](https://youtu.be/O8zIJ0A--lI?t=338), [06:36](https://youtu.be/O8zIJ0A--lI?t=396), [07:14](https://youtu.be/O8zIJ0A--lI?t=434)

### 4.3 Subtractive crossover (band = full − filtered)
- **Phase:** none in the mix-phase sense. It is a utility used inside Tone or Polish multiband work. **Interpretation:** this is my assignment.
- **Says:** "no need to calculate a separate high pass filter when you can just subtract the lowpass filter from the full range signal". The bands are "guaranteed to mix back together to create exactly the original signal". In linear phase the inverted LP is an exact HP. A minimum-phase 12 or 24 dB version still recombines perfectly, but "the lumpiness… will start to re-emerge whenever there are gain differences between the bands, and that resonant bump might cause unwanted behaviour in dynamics processors". He says the method is shown in an earlier Reaper-specific video and doesn't repeat it here.
- **Source:** [Flip 08:23](https://youtu.be/O8zIJ0A--lI?t=503), [10:35](https://youtu.be/O8zIJ0A--lI?t=635)

### 4.4 Inverted steep linear-phase notch = send-based hard band-pass effect, automated by fader
- **Phase:** Creative (creative filtering)
- **Says:** Set a notch "as steep as Pro-Q3 can make it, in linear phase… with the polarity flipped". Any channel you send to it is then hard band-passed, "without them all necessarily having to be routed to the same sub group". He sends everything except the deep bass. Automating the filter channel's level brings it in and out, and "the transition between hard filtering and flat is just gradually decreasing shelving EQ… very natural and smooth sounding". As an aside, he notes he's "safe to break the 'no reverb on bass' rule, which I like to do at every opportunity."
- **Caveat:** It needs linear phase for an exact inversion (0.4), which adds latency.
- **Source:** [Flip 08:54](https://youtu.be/O8zIJ0A--lI?t=534), [09:51](https://youtu.be/O8zIJ0A--lI?t=591)

### 4.5 Inverted one-pole LPF on bass, gated by a kick sidechain (kick/bass separation)
- **Phase:** Relational (sidechain dynamics, separation)
- **What and why (says):** A deep bass competes with the kick: "every time the kick drum hits the low end of the mix gets muddy and confused", and the kick sounds "flappy and weak". Send the bass to a parallel 6 dB/oct one-pole LPF with polarity flipped, in zero latency ("no linear phase required"). That makes the bass gently high-passed. The kick now thumps and the mix is tighter, but "the bass isn't deep enough any more!". So add a gate to the filter channel, sidechained from the kick. The LP signal is then subtracted only while the kick hits, and the bass returns to flat as soon as the gate closes.
- **Settings:** 1-pole 6 dB/oct LPF, inverted, zero latency. Cutoff not stated. The gate's closed state should be fully off: ReaGate's infinite attenuation is "about the only situation where [it] is actually optimal", because the bass "return[s] to perfectly flat whenever the gate is closed". Pro-G's 100 dB maximum range is "plenty". No threshold, attack or release stated.
- **Signal flow:** bass → send → LPF (inverted) → gate (sidechain key: kick) → return.
- **Source:** [Flip 11:48](https://youtu.be/O8zIJ0A--lI?t=708), [12:25](https://youtu.be/O8zIJ0A--lI?t=745), [12:56](https://youtu.be/O8zIJ0A--lI?t=776), [13:36](https://youtu.be/O8zIJ0A--lI?t=816)

---

## 5. The delta channel: one return that boosts one source and cuts others

### 5.1 Polarity-flipped sends to a shared channel: A/B swap between two parts
- **Phase:** Creative
- **Says:** A synth (Pigments) and a guitar playing the same chords, both glitched with Audio Damage Automaton, are "overwhelming and confusing" together. The synth's post-fader send to an unprocessed parallel channel makes it 6 dB louder, so he drops the synth fader to compensate. The guitar's send has its polarity flipped, so the guitar nulls. Flipping the polarity of the return swaps them: synth nulls, guitar +6 dB, and you compensate the guitar too. Automating the return's polarity gives "an AB switching setup and rapidly glitch between the two parts".
- **Caveat:** It needs a polarity flip per send. That is native in Reaper, and "most DAWs don't allow you to flip the polarity for a send".
- **Source:** [Delta 01:11](https://youtu.be/QyZbIbjvQVE?t=71), [01:50](https://youtu.be/QyZbIbjvQVE?t=110)

### 5.2 All-pass on the delta channel: notch on one part, band-pass on the other, swept together
- **Phase:** Creative (sound design)
- **Says:** A 2-pole all-pass rotates phase 180° at its frequency. Summed with dry, that gives a notch; with polarity flipped, it gives a band-pass. Put on the channel from 5.1, the result is "a notch filter for the synth, and simultaneously a bandpass filter for the guitar at the same frequency". You can flip which part gets which, adjust width for both, and sweep, automate or modulate the frequency. He used Volcano 3 (Clean style, "none of the saturation special sauce") for its modulation, but says ReaEQ gives "much the same result".
- **Caveat (says):** "it leans more to the sound design side… its not a transparent way to fit two parts together in a mix". For that, he points to his separate "Super Separator" video, which is not in this set.
- **Source:** [Delta 02:37](https://youtu.be/QyZbIbjvQVE?t=157), [03:08](https://youtu.be/QyZbIbjvQVE?t=188), [03:38](https://youtu.be/QyZbIbjvQVE?t=218), [04:13](https://youtu.be/QyZbIbjvQVE?t=253), [04:48](https://youtu.be/QyZbIbjvQVE?t=288)

### 5.3 Delta band-pass: one fader for the snare-fundamental boost and the matching cut in maskers
- **Phase:** Relational (relational EQ against masking)
- **What and why (says):** After fattening the bass, he notices the snare's low end has gone. It isn't thin soloed, so this is masking. The conventional fix boosts the snare fundamental in close, overhead and room mics and cuts the same frequency on bass, keys and guitar. Even top-down, with one boost on the drum bus and cuts on subgroups, "you still have to balance a boost against a corresponding cut", and "your mouse only does one at a time." The delta channel needs "one parameter which dialed in the boost and the cut at the same time".
- **Settings:**
  - Parallel return with a BP tuned to the snare's low fundamental, "not… quite as tight and narrow" as in 3.2, because "I don't want audible ringing in this case". Frequency not stated.
  - Send from the **drum bus** at normal polarity gives +6 dB at the fundamental ("probably too much").
  - Sends from the **bass subgroup** (including its parallel distortion) and the "Everything Else" group are polarity-flipped, which turns their boost into a notch. "I'll usually turn the send down a few dB as well."
  - The return fader then sets the boost and the cut together.
- **Signal flow:** drum bus (+) and masker subgroups (−, a few dB down) → sends → BP → return.
- **Naming (says):** "I call it delta, meaning difference, and if there's a bandpass filter on it I'll include the frequency it's tuned to."
- **Source:** [Delta 05:20](https://youtu.be/QyZbIbjvQVE?t=320), [06:10](https://youtu.be/QyZbIbjvQVE?t=370), [06:42](https://youtu.be/QyZbIbjvQVE?t=402), [07:15](https://youtu.be/QyZbIbjvQVE?t=435), [07:45](https://youtu.be/QyZbIbjvQVE?t=465), [10:02](https://youtu.be/QyZbIbjvQVE?t=602)

### 5.4 Delta band-pass + downward expander: the boost and cut react to the snare and to congestion
- **Phase:** Relational (sidechain-like dynamics without a sidechain)
- **What and why (says):** A downward expander after the BP on the delta channel gives "expansion for the low fundamental of the snare drum, boosting it harder whenever it hits… and we also simultaneously have compression at that frequency for the rest of the mix, ducking that region down harder whenever the snare hits." The two effects also interact: more congestion in the band raises the expander's input, which means "more boost for the snare and more cut for everything else". When the region is clear, "both the boost and the cut will relax together."
- **Settings:** ratio "quite low and gentle". Threshold set "so the signal never quite reaches it, and rides the expansion all the time". "pretty fast attack and some lookahead so I can catch all the transients." No numbers given.
- **Signal flow:** as 5.3, with the expander after the BP on the return.
- **Source:** [Delta 08:39](https://youtu.be/QyZbIbjvQVE?t=519), [09:15](https://youtu.be/QyZbIbjvQVE?t=555)

---

## 6. Principles stated in passing

### 6.1 Fix harsh resonances with a channel EQ cut, not with parallel filters
- **Phase:** Rescue (live Rescue EQ)
- **Says:** "no-one is ever going to try to tame a harsh resonance by sweeping a parallel filter around on another channel. If I've got a source that's burning my eyebrows off at 2KHz, I'm going to fix that with a conventional EQ cut on the channel before I move on to the fun creative stuff". **Interpretation:** this matches your Rescue-before-Tone order.
- **Source:** [HP/LP 02:20](https://youtu.be/le7oVIrt9oU?t=140)

### 6.2 Set the level in the plugin first, then trim on the fader in context
- **Phase:** Balance
- **Says:** "I like to get in the right ballpark using the plug-in gain then fine-tune it with the fader in context with the mix".
- **Source:** [Bandpass 07:09](https://youtu.be/4hGBYs10ZAU?t=429)

### 6.3 Compensate the +6 dB when a source is fed to an unprocessed parallel path
- **Phase:** Balance
- **Says:** An unprocessed parallel copy makes the source 6 dB louder, "so I'll drop the fader to compensate, and as it's a post fader send I don't need to touch the parallel channel." In the measurements he turned "both dry and wet signals down by 6 db so we start with a flat line at unity".
- **Source:** [Delta 01:11](https://youtu.be/QyZbIbjvQVE?t=71); [Phase Police 04:05](https://youtu.be/RL4KDVFlkUg?t=245)

---

## 7. Phase index

| Phase | Entries |
|---|---|
| Rescue | 6.1 (also 3.2's diagnosis and 3.7's retuning are framed as fixes) |
| Balance | 3.5, 6.2, 6.3 |
| Tone | 0.1–0.5, 1.1, 2.1, 2.2, 2.3, 3.1, 3.2, 3.3, 3.4, 3.7, 4.1 |
| Polish | none directly. 4.3 (subtractive crossover) is a building block for multiband polish. |
| Relational | 4.2, 4.5, 5.3, 5.4 |
| Depth | none. The only mention is the "no reverb on bass" aside in 4.4. |
| Creative | 3.6, 4.4, 5.1, 5.2 |

---

## 8. Gain-neutral processing and the ways to use a compressor

### Gain-neutral processing (keeping bypassed and active loudness equal)
None of the videos uses the term or sets out a general loudness-matching method. These are the related points they do give:
- **Unity reference for summed parallel paths.** Dry + identical wet at unity is +6 dB. Compensate on the source fader (post-fader send), or drop both paths by 6 dB ([Delta 01:11](https://youtu.be/QyZbIbjvQVE?t=71), [Phase Police 04:05](https://youtu.be/RL4KDVFlkUg?t=245)). Parallel band-pass and HP/LP tricks are built so that the filter is "invisible" except in its band: unity at the centre gives exactly +6 dB there and nothing elsewhere ([Flip 00:02](https://youtu.be/O8zIJ0A--lI?t=2)).
- **The compressor must return to exact unity at rest.** On an inverted path, "I really want to get back to exactly unity gain when the compressor fully releases". That rules out fixed-threshold input/output designs (Comprexxor, 1176 emulations) and forced auto-gain designs (H-Comp). Use "a threshold control and the option to manually control the makeup gain" ([Flip 03:07](https://youtu.be/O8zIJ0A--lI?t=187), [04:02](https://youtu.be/O8zIJ0A--lI?t=242)). **Interpretation:** this is the opposite of loudness-matched gain compensation. He wants the processor's *idle* gain exact, because the null depends on it.
- **The gate's closed state must be exactly flat.** On the kick-keyed inverted LP, infinite attenuation (ReaGate) means "we return to perfectly flat whenever the gate is closed" ([Flip 12:56](https://youtu.be/O8zIJ0A--lI?t=776)).
- **Auto gain while sweeping Q.** Pro-Q 3's output auto gain keeps the level steady while you change Q on the kick band-pass, but you must lower the output gain a lot first, or it over-compensates on band-pass filters ([Bandpass 09:25](https://youtu.be/4hGBYs10ZAU?t=565)).
- **Parallel filter level as an A/B control.** In 3.5, 4.2 and 5.3 he judges the effect by bringing the return in and out against the original balance. He does not say he level-matches when doing this.

### Ways to use a compressor
**No video gives a taxonomy.** There is no "four ways to use a compressor" or similar list in these five transcripts. These are the compression roles they show:
- Parallel compression in general: "the output of the compressor is blended with the original signal to create more subtle and complex compression effects" ([HP/LP 00:00](https://youtu.be/le7oVIrt9oU?t=0)).
- Band-limited parallel compression, a dynamic high or low shelf ([HP/LP 07:39](https://youtu.be/le7oVIrt9oU?t=459), [09:32](https://youtu.be/le7oVIrt9oU?t=572)).
- Transient shaping of a frequency band: compressing the 2.5 kHz band "to shape the upper mid-range transients and stop it ever getting too extreme" ([Bandpass 04:08](https://youtu.be/4hGBYs10ZAU?t=248)).
- Consistency: compression on tom filter channels "in case the toms seem to vary in tone too much" ([Bandpass 08:22](https://youtu.be/4hGBYs10ZAU?t=502)).
- Compression on an inverted path, which becomes a band expander or gate ([Flip 04:02](https://youtu.be/O8zIJ0A--lI?t=242)). The reverse also holds: an expander on an inverted path becomes downward compression ([Flip 06:36](https://youtu.be/O8zIJ0A--lI?t=396)).
- EQ on a parallel compression channel makes the EQ dynamic ([Phase Police 07:09](https://youtu.be/RL4KDVFlkUg?t=429)).
