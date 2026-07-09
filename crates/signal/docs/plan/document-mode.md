# Document mode — lookahead playback for the sampler

**Status: design. 2026-07-02.**

## The idea

Sample libraries with real legato have sampled transitions that take time
(CSS: 333/250/100 ms by velocity zone). Every DAW workflow today compensates
by *shifting MIDI earlier* — negative track delay, or a mirrored
timing-adjusted copy of the track. Both destroy the 1:1 relationship between
the MIDI you see and the score it represents.

Because we control the sampler, we can invert the problem. Give the sampler
the **entire MIDI document of its track ahead of time** (ARA-style) plus the
**host transport**, and it can *anticipate* instead of being compensated:
during deterministic playback it starts each legato transition `delay_ms`
**before** the destination note's tick, so the audible arrival lands exactly
on the grid. No negative delay. No mirror. One quantized, score-faithful
MIDI track drives both engraving and playback.

Two modes, coexisting per engine instance:

| | trigger | patches / timing |
|---|---|---|
| **Live** | incoming MIDI events (today's path, unchanged) | zero-latency: shorts + `low_latency` legato zones, transition fires *after* note-on (current reactive behavior) |
| **Document** | transport playing through known material | full `expressive` legato: transition scheduled `delay_ms` *early* from lookahead; shorts pre-rolled by their `pre_delay_ms` |

## What already exists (survey 2026-07-02)

- **Articulation + legato model is complete.** `signal-sampler/src/spec.rs`:
  `LegatoEngineSpec { expressive, low_latency, portamento }`,
  `LegatoZoneSpec { vel_range, delay_ms }`, `delay_for_velocity()`;
  directional transition zones; `ShortNoteCompensation.pre_delay_ms`
  (documented as "apply a negative track delay" — the hack this design
  retires).
- **Transition player works.** `engine/mod.rs`: `LegatoState::Pending
  { frames_remaining, .. }` → `fire_legato` → `spawn_legato_transition`.
  Today the countdown starts at note-on (reactive-positive). Document mode
  only changes *when the countdown starts*.
- **Deterministic RR** (`engine/rr.rs::set_forced_rr`) — reproducible
  passages already supported.
- **Timeline-ahead precedent.** `fts-signal-controller/src/scene_timer.rs`
  reads a whole track's items via `daw.items(guid)` and matches the playhead
  against them (`read_item_timeline`). Same pattern, note-level.
- **No lookahead, no transport in the audio path.**
  `PluginInstance::process_block(in, out, events)` carries per-block sample
  offsets only; tempo is hard-coded 120 in `node_render`. These are the two
  seams this design adds.
- `docs/sampler-trait-design.md:396` already anticipates "look-ahead legato"
  as a latency source. Document mode is that, made real.

## Design

### 1. The document

Per (engine instance ↔ track): the full note/CC content of the track in
musical time, plus the tempo map to convert to seconds.

```rust
pub struct TrackDocument {
    /// Monotonic version; replaces wholesale on change (documents are small).
    pub version: u64,
    /// Determinism seed — all stochastic choices (RR, jitter) hash from
    /// this. Persisted with the project; see "Determinism" below.
    pub seed: u64,
    pub notes: Vec<DocNote>,     // start_qn, end_qn, chan, pitch, vel
    pub ccs: Vec<DocCc>,         // qn, chan, cc, val
    pub tempo: Vec<TempoPoint>,  // qn, bpm (+ linear flag later)
}
```

The document is **annotated** on ingestion (non-realtime thread) into a
schedule the audio thread consumes:

- articulation per note (keyswitch state at note-on, or channel plan)
- legato edges (different-pitch overlap), re-bow chains (same-pitch abutment)
- per-note transition lead: `legato_engine.expressive.delay_for_velocity(vel)`
- shorts: `short_note_compensation.pre_delay_ms` as pre-roll

This inference already exists, tested against the CSS reference engine, in
`keyflow-orchestra` (`mirror_part` — pure Rust, deps: nothing heavy). Reuse
it: factor its inference stage into an `annotate()` that returns intents
instead of shifted notes. The delay tables live in the sampler spec (styx),
so the annotation asks the spec, not keyflow's profile tables.

Result: `Schedule = Vec<ScheduledEvent>` sorted by time-in-seconds, where
each event is `NoteOn/NoteOff/Cc/LegatoPrefire { at_sec, .. }` and
`LegatoPrefire.at_sec = note.start_sec - delay_ms` — **the inversion**.

### 2. Transport in the audio path

`process_block` gains host time. Two host cases:

- **daw standalone engine** (KeysRig/SamplerRig today): extend
  `PluginEvents` (or a parallel `PluginTransport` arg) with
  `{ playing: bool, playhead_sec: f64, tempo_bpm: f64, loop: Option<(f64,f64)> }`
  snapshotted per block from `TransportShared`. Small daw change; the
  engine currently holds `TransportShared` but never threads it down.
- **REAPER-hosted CLAP** (future `signal-sampler-clap`, same
  `fts_plugin_core` skeleton as `fts-signal-controller`): CLAP delivers
  transport natively in the process context — free.

### 3. The scheduler (audio thread)

Per block: `[playhead, playhead + block/sr)` window against the Schedule.

- Emit every scheduled event whose time falls in the window at its exact
  sample offset (the engine already places by offset).
- `LegatoPrefire` calls `start_legato_transition` with
  `frames_remaining = 0`-equivalent — i.e. fire now, arrival lands on the
  destination tick.
- **Seek/loop/stop**: on discontinuity, kill pending prefires, re-locate the
  schedule cursor, and (like any sampler on seek) start mid-note sustains at
  the right phase or on next boundary (v1: next note boundary).
- **Playback-start invariance (supersedes the old "late start" fallback):**
  starting playback at ANY position P must sound identical to the offline
  render sliced at P — sample for sample. If P lands inside a legato
  transition's window (prefire fired before P, arrival after), the engine
  does not degrade to a fresh attack: it reconstructs the voice the full
  render would have — same transition sample (recomputable because RR is a
  pure hash of the note, not history), started mid-sample at offset
  `P - prefire_frame`, envelopes/crossfades advanced to match. The same
  reconstruction applies to sustains sounding at P and to release tails of
  notes that ended within their ring-out window before P: a bounded
  back-scan (`max_prefire_lead + max_release_tail`) finds every voice alive
  at P and spawns it at its correct internal offset, and CC state is set to
  its interpolated value at P. Consequence: WHERE you press play never
  changes WHAT you hear — realtime from bar 17 == the bounce, at every
  sample.
- **Document edited during playback**: version bump swaps the schedule at
  the next block boundary (same glitch-free swap discipline as patches).

### 4. Live/document arbitration

- Transport stopped → pure live mode (today's behavior, untouched).
- Transport playing + document present → document is authoritative for its
  track; incoming live MIDI on that engine is still allowed (overdub feel)
  but does not double-trigger notes the schedule already played
  (match by pitch+tick window).
- No document → live mode even while playing (plain MIDI-thru today).

### 5. Ingestion / control plane

New vox service in `signal-proto` (CRUD services stay untouched):

```rust
#[vox::service]
trait DocumentService {
    async fn set_document(&self, engine: EngineRef, doc: TrackDocument) -> Result<()>;
    async fn clear_document(&self, engine: EngineRef) -> Result<()>;
}
```

Feeder: the REAPER side (fts-extensions or signal-extension) reuses the
watch/hash loop already written for the mirror module (`fts-extensions
src/mirror.rs`, `mod-mirror`, off by default): tagged track → hash item
content per tick → on change, read notes/CCs in QN + tempo map → push
`set_document`. The sink changes from "write mirror items" to one RPC call.

### 6. Latency reporting

Per `docs/sampler-trait-design.md:396`: in document mode the engine reports
`latency() = 0` (it anticipates rather than delays); live mode likewise 0 —
the *tradeoff* moves from latency to transition authenticity, which is the
correct axis.

## Mode policy: strict live low-latency by default

Kontakt CSS carries a floor of ~60 ms even in its fastest mode, which rules
out live playing. Ours must not. **Mode selection is automatic and strict:**

- **Lookahead mode** — ONLY when the MIDI is known ahead of time: document +
  transport playback in REAPER, or offline render. Full expressive legato,
  scheduled prefires.
- **Strict live mode** — everything else (live MIDI input, no document,
  transport stopped). Zero added latency, no exceptions: fastest attack
  zones, `low_latency` legato tables applied reactively, shorts fire
  immediately with no pre-delay concept. Latency report = 0.

No user toggle needed (though an override can exist): if the engine can see
the future it plays beautifully; if it can't, it plays *now*. This is what
makes sketching effortless — play in on the same patch you'll render with.

## Auto-divisi

Divisi assignment (simultaneous notes ranked top→bottom into monophonic
lines; a held note never loses its line to a re-articulated lower note) is
an algorithm, not data — ported and parity-tested in
`keyflow-orchestra::assign_channels`. The engine plays mono lines
statelessly and doesn't care who assigned them. Two front-ends, one
feature:

- **Explicit channels (import path)**: a document whose notes carry
  meaningful MIDI channels (e.g. keyflow's mxl import) is respected as-is —
  channel = line.
- **Lookahead auto-divisi (document path)**: a document on a single channel
  (or flagged `auto_divisi`) gets the full assignment algorithm in
  `annotate()` — with the entire document visible, the held-note ranking is
  exact, deterministic, and seed-independent (it's pure).
- **Live auto-divisi (realtime path)**: incoming polyphonic MIDI is split
  greedily into lines by the same ranking rule (top note → line 1, held
  notes keep their line). Reactive by nature (no future knowledge),
  deterministic given the same input stream.

  **Live legato gating — the whole point is never having to switch between
  legato and sustain patches while playing.** Live transitions are
  conservative so chords can't confuse the allocator:
  - **Simultaneity gate**: notes arriving within a small window
    (`live_chord_window_ms`, default ~30 ms) are a chord — they fan out to
    separate lines as fresh sustain attacks, never legato.
  - **Interval gate**: a successive note continues an existing line as a
    legato transition only if it is within `live_legato_interval_max`
    semitones of that line's sounding note (default 2 = major 2nd; per-patch
    in the styx spec) *and* the line's previous note overlaps or just
    released. Anything wider is a fresh attack on a free line.
  - Live transitions always use the `low_latency` tables (fast, and the
    small-interval restriction is exactly where fast transitions sound
    right).

  Lookahead mode has **no such gates** — the document knows the actual
  voice-leading, so full-range sampled transitions apply as written. Net
  effect: play triads and stepwise melodies live on one patch, 90% of the
  legato/sustain switching problem gone; the render is where wide sampled
  legato lives.

Line allocation must therefore not be hardwired to "MIDI channel": lines
are engine entities; channel-mapping and auto-divisi are two allocators in
front of them.

## Channels: one engine, many mono lines

In the Kontakt world each divisi/articulation MIDI channel needed its own
sampler instance. Here the engine is ours: **one engine instance per track,
one mono legato line per MIDI channel**. Each channel carries its own
`LegatoState`/line cursor; the sample cache, voice pool, and RR hashing are
shared. A channel is *who is playing* (divisi desk, solo, per-articulation
lane), never a separate plugin. Phase 1's known gap (multi-channel documents
folding into one line and falling back to the reactive path) is closed by
this: the scheduler annotates and prefires per channel.

## Stem-aware output buses (optional)

Downstream mixing splits strings into **Longs** and **Shorts** stems
(e.g. `Strings High Longs` / `Strings High Shorts`). Voices are tagged with
an articulation class at spawn:

- **Longs**: Sustain, Legato, Tremolo, releases of either
- **Shorts**: Short articulations (staccato/staccatissimo/spiccato/pizz)

Routing is a `class → output bus` map on the rig. **Default: every class →
main stereo out** (no behavior change). Flipping the map renders classes
into separate buses (`render_offline_buses` precedent) so stems can be
routed independently and recombined later — sum of split buses must equal
the main-out render bit-exactly (test).

## Determinism (hard requirement)

**Same document + same parameters + same seed → byte-identical audio. No
exceptions.** This is a correctness property, not a preference: it makes
renders reproducible, regressions bisectable, and A/B tests meaningful.

- **Seed** is part of the document/engine state: `TrackDocument.seed: u64`
  (persisted with the project — REAPER side stores it in track ext state so
  it survives save/reload; default derived once from the track GUID, user-
  overridable to re-roll a take).
- **Every stochastic choice is a pure hash, not a counter.** Round-robin
  slot, any sample-start jitter, any humanization:
  `choice = hash(seed, note.start_qn.to_bits(), note.pitch, note.chan, purpose) % n`.
  Two consequences that counters cannot give:
  - **Position independence**: starting playback at bar 17 produces the
    same RR for every note as playing from the top — the choice depends on
    the note, not on how many notes played before it.
  - **Edit stability**: inserting a note changes only that note's choices;
    everything after it keeps its sound (a counter would re-roll the whole
    tail of the piece).
- **Engine plumbing**: the existing `RrCounters`/`set_forced_rr` machinery
  already supports pinning a slot per trigger (survey: "deterministic
  playback of a passage"). Document mode computes the slot in the annotate
  step and pins it via the forced-RR path per scheduled note — the audio
  thread never consults a mutable counter. Live mode keeps the counters
  (a human playing twice is *supposed* to hear rotation).
- **Float determinism**: identical binary, sample rate, and block size
  render identically; the schedule is expressed in absolute frames from the
  document epoch (not relative to transport-start), so block-boundary
  alignment can't shift voice phase between runs. Offline
  `render_offline_document` is the canonical reference; the realtime path
  must match it sample-for-sample at equal sr/block-size, and the phase-1
  A/B harness asserts exactly that.
- **Test**: render the same document twice (and once starting mid-piece) →
  hashes of the output buffers must be equal / consistent per the rules
  above. This test is permanent, not a development aid.

## Deployment target: a self-sufficient CLAP on a REAPER track

The production shape is a **CLAP plugin sitting on a REAPER track** (same
`fts_plugin_core` skeleton as `fts-signal-controller`). The entire algorithm
must function on exactly two inputs — nothing else:

1. **Host transport** from the CLAP process context (playing, song position,
   tempo — sample-accurate, free from REAPER).
2. **The complete MIDI of its own track**, visible forwards AND backwards,
   pulled by the plugin itself through the daw crate (`daw::get()` →
   track/items/notes — the `fts-signal-controller::scene_timer` pattern:
   identify own track, read the timeline ahead of the playhead).

No extension-side push feeder, no DocumentService: the plugin **pulls its
own document**. Off the audio thread it watches its track's items (content
hash, same discipline as the offline path), rebuilds
`TrackDocument`/`Schedule` on change, and swaps atomically at a block
boundary. On the audio thread: transport playing + schedule present →
Lookahead (prefires against the CLAP song position); otherwise StrictLive
on the incoming live MIDI events. Live/document arbitration per the mode
policy above.

## Phases

1. ~~**Schedule + offline proof**~~ **DONE** (`ec08016`): document →
   annotate → schedule → `render_offline_document`, determinism suite,
   Going Home end-to-end.
   1b. ~~**Lines / buses / modes / auto-divisi**~~ **DONE**
   (`cd18003`..`f1d9c69`): per-line mono legato, Longs/Shorts buses,
   automatic StrictLive/Lookahead policy, both divisi allocators.
2. ~~**CLAP shell + realtime transport scheduler**~~ **DONE**:
   `signal-sampler-clap` (fts_plugin_core skeleton) +
   `signal_sampler::document_rt::RealtimeScheduler` — the offline walker
   driven by the CLAP transport, bit-identical to `render_offline_document`
   and block-size invariant (`signal-sampler-clap/tests/host_sim.rs`).
   **Playback-start invariance holds at every sample**: starting/seeking
   to ANY position P — inside a transition window, mid-sustain, inside a
   release tail — is the full render sliced at P, bit-exactly. Voices alive
   across P are reconstructed by bounded deterministic replay
   (`Schedule::reconstruction_start`: replay the activity span containing P
   through the real render path, audio discarded — the only bit-exact
   mechanism, since voice state is per-frame float accumulation). The
   offline `start_frame` render uses the same semantics, so the invariant
   is tested realtime-vs-offline-vs-full-slice. Replay cost is bounded by
   the continuous activity span (audio-thread stall on seek; off-thread
   reconstruction is a future optimization). Tempo policy: schedule frames come
   from the document tempo map; the HOST playhead is trusted (REAPER is the
   tempo authority) and a divergent host tempo logs one warning — the fix
   is rebuilding the document (phase 3's watcher does it automatically).
   Install: `cargo xtask install` (bundles + symlinks
   `Signal Sampler.clap`). Patch: `$SIGNAL_SAMPLER_CLAP_PATCH` (.styx; dev
   default CSS 1st Violins). Document (dev, pending phase 3):
   `$SIGNAL_SAMPLER_CLAP_DOC` JSON, hot-reloaded through the
   `set_document` seam. Live input is dropped while a schedule is playing
   (overdub arbitration comes with phase 3).
3. **Self-sourced document**: own-track identification, item/MIDI read via
   daw crate, change watch + atomic schedule swap, live/document
   arbitration, seed persisted in track ext state.
4. **Stem bus → plugin outputs**: map ArticClass buses onto CLAP output
   ports so REAPER can route Longs/Shorts stems independently (optional,
   default main).

## Non-goals (for now)

- Third-party samplers (Kontakt/CSS): keep using the parked mirror path
  (`fts-extensions` `mod-mirror`, off by default) if ever needed.
- Audio-domain ARA (waveform analysis): this is MIDI-document-only.
