# Signal Sampler Implementation Plan

Canonical roadmap: [docs/content/sampler-roadmap.md](../content/sampler-roadmap.md)

This is the working implementation plan for turning Signal's internal sampler,
CLI player, and TUI into a Kontakt-class playback system. The plan is organized
around the features common to serious sampler engines: robust live playback,
expressive zone/group mapping, modulation, streaming, authoring, and diagnostics.

## References To Learn From

- Kontakt: deep zone mapping, groups, articulations, scriptable behavior,
  disk streaming, multi-mic routing, release samples, round robin, keyswitches,
  and library-scale browsing.
- Ableton Sampler/Simpler: practical sample start/loop controls, warp modes,
  modulation, filter/envelope workflow, slicing, and performance-oriented UI.
- DecentSampler: open XML-style instrument definitions with groups, zones,
  bindings, effects, round robin, and broad community library compatibility.
- SFZ/Sforzando ecosystem: opcode-driven mapping for zones, groups, triggers,
  sequence positions, loops, filters, envelopes, and MIDI controls.
- LinuxSampler: disk streaming, large instrument loading, GIG/SFZ concepts,
  MIDI/audio backend separation, and headless sampler architecture.
- Shortcircuit / TAL-Sampler style workflows: fast creative sampling, chopping,
  envelopes, filters, modulation, and immediate hands-on editing.

## Current Foundation

- Offline rendering exists through `SamplerPlayer::new_offline()` and
  `render_offline()`, so tests do not need to open the real audio device.
- Live playback uses a shared bank/player path for CLI, TUI, and tests.
- Event queues are bounded and report pending/dropped events.
- `all_notes_off()` and hard `panic()` are separate behaviors.
- Preload jobs are generation-cancelled when patches change.
- The sample cache has an atomic read snapshot for audio-thread lookups.
- Basic voice stealing, voice caps, choke groups, group polyphony, sample
  start/end, forward loops, alternating loops, reverse playback, one-shot
  triggers, release triggers, gain, and pan are partially implemented.
- Cache byte counts, cache budgets, cache pressure, miss counters, and callback
  scratch resize diagnostics are surfaced in the player statistics.

## Phase 1: Playback Reliability

Goal: make basic live playback hard to break.

- Keep audio callbacks free of blocking locks, file I/O, unbounded work, and
  steady-state heap allocation.
- Finish deterministic cache budget enforcement:
  - global decoded PCM budget
  - per-instrument budget hooks
  - eviction reporting
  - never evict data still owned by active voices
  - unload old-generation cache entries after patch changes
- Expand voice policy:
  - global, engine, group, note, and choke-group polyphony
  - release-first, oldest, quietest, same-note-first, drop-new, and
    sustain-aware stealing
  - short fade on steals
  - diagnostics showing steal counts and policy decisions
- Make missing sample failures actionable:
  - separate missing file, unmapped zone, and cache miss counters
  - track recent sample paths and lookup contexts
  - expose recent failures in CLI and TUI diagnostics
- Add regression tests for panic, all-notes-off, stuck-note prevention, queue
  overflow, preload cancellation, cache pressure, and repeated load/unload.

## Phase 2: Zone And Group Model

Goal: represent real sample libraries without hard-coded special cases.

- Make native zones explicit and editable:
  - sample path
  - key and velocity ranges
  - root key / pitch keycenter
  - transpose and fine tune
  - gain and pan
  - sample start/end
  - loop start/end and loop mode
  - trigger mode
  - mic id
  - round-robin id
  - articulation id
  - group id
  - group polyphony
  - choke/off-by groups
- Add a group layer above zones:
  - group volume, pan, and tune
  - enable conditions
  - group polyphony and voice policy
  - exclusive/choke groups
  - per-group filters, envelopes, and effects
  - per-group round-robin state
  - key, velocity, CC, channel, and articulation conditions
- Support trigger modes:
  - note attack
  - note release / key-up
  - one-shot
  - legato
  - first note
  - repeated note
  - CC threshold
  - pedal down/up
  - aftertouch threshold

## Phase 3: Round Robin, Articulations, And Pedal Behavior

Goal: support expressive instruments like drums, piano, strings, and orchestral
libraries.

- Round robin:
  - cycle
  - random
  - no-repeat random
  - reset by note
  - reset by CC
  - separate state by dynamic layer, mic group, and articulation
- Articulation switching:
  - keyswitches
  - MIDI CC
  - velocity switches
  - program/preset switches
  - MIDI channel switches
  - UI/macro selection
  - latch and momentary behavior
  - visible active articulation in TUI
  - persisted default articulation
- Legato:
  - interval-aware transition sample selection
  - retrigger and non-retrigger modes
  - portamento thresholds
  - pre-delay tables by velocity
  - fallback behavior when transition samples are missing
- Sustain pedal:
  - deferred note-off
  - half-pedal
  - repedal
  - pedal-down body variants
  - pedal noise and release noise layers

## Phase 4: Loops, Time, And Pitch

Goal: cover both traditional multisampling and modern loop/one-shot workflows.

- Loop modes:
  - forward sustain loop
  - alternating loop
  - reverse loop
  - crossfade loop
  - release loop
  - one-shot loop pads
- Slicing:
  - transient or marker-based slice maps
  - per-slice trigger notes
  - slice choke behavior
  - per-slice start/end and gain
- Time and pitch:
  - high-quality resampling for source/device sample-rate mismatch
  - beat sync
  - time-stretch
  - granular/warp mode
  - reverse playback
  - tempo-aware loop playback

## Phase 5: Streaming And Memory

Goal: make large libraries practical.

- Replace full-RAM-only decoding with disk streaming:
  - attack preload
  - background tail streaming
  - read-ahead
  - per-voice stream cursors
  - stream underrun diagnostics
- Add preload profiles:
  - fast audition
  - performance
  - full preload
  - drum kit priority
  - piano center-out
  - orchestral articulation priority
- Add cache lifecycle controls:
  - generation-based unload after patch changes
  - deterministic eviction
  - LRU or usage-based prioritization
  - per-mic preload priority
  - active-voice protection

## Phase 6: Modulation And DSP

Goal: make the sampler musically useful beyond static playback.

- Modulation sources:
  - AHDSR envelope
  - multi-stage envelope
  - per-zone envelope
  - LFO
  - step sequencer
  - velocity
  - key tracking
  - random
  - aftertouch
  - pitch bend
  - MIDI CC
  - macro knobs
- Modulation targets:
  - volume
  - pan
  - tune
  - sample start
  - loop start/end
  - filter cutoff/Q
  - envelope times
  - send level
  - effect parameters
- DSP and routing:
  - amp envelope
  - pitch envelope
  - filter
  - saturation
  - EQ
  - compressor/limiter
  - transient shaper
  - chorus/phaser/flanger
  - delay
  - reverb/convolution
  - bitcrusher
  - sends, returns, buses, meters, and multi-output rendering

## Phase 7: Authoring And Import

Goal: make libraries buildable and portable.

- Native editor model:
  - zone editing
  - group editing
  - mapping view
  - velocity layers
  - round-robin lanes
  - mic groups
  - articulation map
  - macro definitions
  - module/effect graph
- Auto-mapping:
  - parse note names from filenames
  - parse velocity/dynamic tokens
  - parse round-robin tokens
  - parse mic names
  - detect release samples
  - infer root key
  - distribute key and velocity ranges
  - report unmapped files
- Import priority:
  - SFZ subset
  - DecentSampler-style groups, zones, effects, and bindings
  - SoundFont/SF2 conversion through external tooling or import bridge
  - Kontakt concept compatibility, without encrypted-library import

## Phase 8: CLI, TUI, And Diagnostics

Goal: make the engine understandable without attaching a debugger.

- CLI:
  - live playback
  - offline smoke tests
  - preload controls
  - device selection
  - MIDI logging
  - cache budget reporting and enforcement
  - diagnostic report export
- TUI:
  - pack/block/engine/preset loading
  - preview cancellation
  - panic and all-notes-off controls
  - favorites, recents, and filters
  - active articulation display
  - sustain pedal state
  - cache/miss/voice statistics
  - clear load and playback errors
- Shared diagnostics:
  - cache memory and budget pressure
  - cache miss count
  - missing sample count
  - preload failures
  - active voices by engine/group
  - stolen voices
  - render time and callback overruns
  - queue pending/dropped
  - unresolved routing
  - unsupported modules/effects

## Phase 9: Test System

Goal: test sampler behavior without playing through real speakers.

- Keep offline rendering as the default CI path.
- Use the same `SamplerPlayer`/bank/event API in tests, CLI, and TUI.
- Add fixture packs for:
  - single sample
  - velocity layers
  - round robin
  - release samples
  - choke groups
  - looped samples
  - missing files
  - large preload cancellation
- Add deterministic tests for:
  - note attack and release
  - one-shot notes
  - release triggers
  - pedal defer/release
  - panic hard clear
  - voice stealing policy
  - round-robin order
  - random no-repeat behavior with seeded RNG
  - cache snapshot publishing
  - cache eviction
  - repeated patch load/play/unload
- Add optional backend integration tests later:
  - fake JACK/PipeWire environment
  - virtual MIDI input
  - long-running event queue stress
  - render graph routing tests

## Near-Term Execution Order

1. Finish cache eviction and budget enforcement.
2. Add cache eviction tests and CLI/TUI diagnostics.
3. Formalize native zone/group structs and serialization.
4. Add round-robin state and tests.
5. Add sustain pedal semantics.
6. Add SFZ subset import for mapping validation.
7. Add authoring fixtures and regression packs.
8. Start disk streaming after cache lifecycle behavior is reliable.
