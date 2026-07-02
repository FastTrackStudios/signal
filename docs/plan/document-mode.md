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
- **Late start** (transport starts < delay_ms before a legato note): fall
  back to the reactive path for that one note — same sound as live mode,
  degrades gracefully.
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

## Phases

1. **Schedule + offline proof**: `TrackDocument` → annotate → schedule →
   `SamplerRig::render_offline_document()`. A/B against the current
   `render_offline` + negative-delay input: document render must put
   transition arrivals on the grid. (Reuses the spectral A/B harness.)
2. **Transport into daw standalone** `process_block` + realtime scheduler;
   KeysRig plays a document from its own transport.
3. **DocumentService + REAPER feeder** (watch/hash loop → RPC); live/document
   arbitration.
4. **CLAP host path** (signal-sampler as a CLAP inside REAPER, transport from
   host) — pairs with the parked `signal-daw` plan.

## Non-goals (for now)

- Third-party samplers (Kontakt/CSS): keep using the parked mirror path
  (`fts-extensions` `mod-mirror`, off by default) if ever needed.
- Audio-domain ARA (waveform analysis): this is MIDI-document-only.
