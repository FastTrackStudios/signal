+++
title = "Sampler Roadmap"
weight = 20
+++

# Signal Sampler Roadmap

This plan tracks the work needed for Signal's internal sampler, CLI, and TUI
to become a serious Kontakt-style instrument platform.

Scope: `crates/signal-sampler`, `apps/tui`, and sampler-related CLI tooling.
Desktop UI and plugin UI are intentionally out of scope unless called out.

Status legend: 🔲 not started · ⬜ partial · ☑️ usable · ✅ shippable

## Guiding goals

- Playback must be stable under live performance pressure: no stuck notes,
  predictable patch changes, bounded event queues, and useful panic controls.
- The audio callback must avoid blocking and avoid hidden allocation in steady
  state.
- Large libraries must stream or preload predictably without wasting CPU on
  obsolete patch loads.
- Instrument definitions should be expressive enough to model orchestral,
  piano, drum, loop, and hybrid sample libraries without hard-coded special
  cases.
- TUI/CLI must expose enough diagnostics to debug library, preload, MIDI, and
  audio issues without attaching a debugger.

## Current foundation

### ☑️ Audio device and offline player

- `SamplerPlayer::new_offline()` supports no-speaker integration tests.
- `render_offline()` renders through the same bank/event path.
- The live player defaults to the device's native sample rate unless callers
  explicitly request a rate.

### ☑️ Event queue and panic path

- MIDI/audio events are queued off the audio callback.
- Queue drops and pending events are reported through `AudioStatsSnapshot`.
- Musical `all_notes_off()` and hard `panic()` are separate paths.
- TUI Space triggers hard all-sound-off.

### ☑️ Preload cancellation

- `SamplerBank` owns a preload generation token.
- New loads/unloads invalidate older preload jobs.
- Preset preload checks cancellation between engines and within per-sample
  preload loops.

### ☑️ Lock-free cache read snapshot

- `SampleCache::get_loaded()` reads from an atomic snapshot.
- Preload/write paths publish snapshots as decoded samples become available.
- Bulk preload coalesces snapshot publishing to avoid cloning the full cache map
  after every sample.

### ⬜ Voice stealing

- Voice steals are counted.
- Stealing prefers already-releasing voices, otherwise fades the quietest
  stealable voice.
- Engine-level `VoiceConfig.polyphony` is enforced for `.signalengine` and
  preset-contained engines.
- Engine-level `voice_steal` supports default release-first/quietest, oldest,
  quietest, same-note-first, and drop-new policies.
- Zone-mode `group`, `choke_group`, and `off_by` metadata supports basic
  exclusive/choke behavior for DecentSampler-style one-shots.
- Zone-mode `group_polyphony` supports basic per-group voice caps.
- Missing: editable group runtime policies and deeper sustain-aware group
  stealing.

## Tier 1: Live-playback robustness

### ☑️ Split musical release from hard panic

- Keep `all_notes_off()` for musical note release.
- Keep `panic()` for immediate hard kill.
- TUI and CLI should expose both behaviors where useful.

### ☑️ Cancellable preload jobs

- Cancel stale jobs when patches change.
- Keep old jobs from consuming disk/CPU after a new patch is selected.

### ☑️ Audio-thread cache lookup without write contention

- Avoid `RwLock::try_read()` on the audio callback.
- Publish decoded samples through an atomic read snapshot.

### ⬜ Better voice stealing

- Add configurable polyphony:
  - global default
  - per instrument / engine
  - per group
  - per note/choke group
- Steal policy options:
  - release-first
  - oldest
  - quietest
  - same-note first
  - preserve held sustain voices where possible
- Add short steal fade and diagnostics for which policy fired.

### ⬜ Cache miss and missing-sample diagnostics

- Count audio-thread cache misses separately from missing files.
- Track recent cache misses by sample path.
- Track recent sample-map misses by lookup context.
- Surface counts and latest/recent miss details in TUI/CLI diagnostics.
- Missing: richer per-instrument grouping and a dedicated TUI diagnostic list.

### ⬜ Callback allocation audit

- Confirm all steady-state render paths avoid heap allocation.
- Preallocate max expected block buffers.
- Non-stereo callback scratch is pre-sized when a fixed buffer size is known.
- Engine, layer, mic, module, and gain/pan block scratches are pre-sized from
  the expected callback frame count at construction.
- Remaining `resize()` paths are treated as block-size-change fallbacks; they
  now increment resize diagnostics and log the resized block size.
- CLI/TUI audio diagnostics surface resize event counts.

## Tier 2: Kontakt-class playback model

### ⬜ Zones

Required zone fields:

- sample path
- key range
- velocity range
- root key / pitch keycenter
- tune cents
- gain
- pan
- sample start/end
- loop points
- trigger mode
- mic id
- round-robin id
- group id
- group polyphony
- choke group / off-by group ids

Current support is strongest for `.signalpack` zone playback and convention
mapping. Zone-mode playback now honors explicit sample start/end frame windows
and basic reverse playback. Zone trigger mode supports attack/held playback
one-shot playback, and release/key-up trigger zones.
Zone gain and equal-power pan are applied per voice.
Basic forward sustain loop points are supported for zone-mode voices. The
native zone model should become explicit and editable.

### ⬜ Groups and layers

Add a group model above zones:

- group volume/pan/tune
- group enable conditions
- group polyphony
- exclusive/choke groups
- per-group filters/envelopes/effects
- per-group round-robin state
- per-group key/velocity/CC conditions

### ⬜ Triggers

Support trigger modes:

- note attack
- note release
- legato
- first note
- repeated note
- key-up
- CC threshold
- pedal down/up
- aftertouch threshold

Current zone-mode support covers note attack/held playback, one-shot
triggering, release/key-up triggering, explicit pedal-down/pedal-up zones fired
from CC64 threshold crossings, and CC-threshold zones via `trigger_cc` plus
`trigger_value_min/max`. Channel and poly-aftertouch zones use the same value
range fields and fire on threshold entry.

### ⬜ Round robin

Required modes:

- cycle
- random
- no-repeat random
- reset by note
- reset by CC
- per dynamic layer
- per mic group

Current support: filename-convention instruments have per
section/articulation/dynamic cycle counters with CC59 reset. Zone-mode playback
cycles by explicit `rr_index`, keeps multi-mic RR slots aligned even when import
order differs from RR order, and supports `cycle`, `random`, and
`no-repeat-random` through `ZoneSpec.rr_mode`. CC59 resets both convention and
zone-mode counters. Missing: reset-by-note and dynamic/mic-scoped policy
configuration.

### ⬜ Articulation switching

Switch sources:

- keyswitch
- MIDI CC
- velocity
- program/preset
- MIDI channel
- UI/macro selection

Behavior requirements:

- latch vs momentary keyswitches
- visible active articulation
- persisted default articulation
- reset behavior on patch load

### ⬜ Legato

Required features:

- interval-aware transition sample selection
- retrigger vs non-retrigger modes
- portamento thresholds
- pre-delay tables by velocity
- low-latency vs expressive modes
- fallback when transition sample is missing

### ⬜ Sustain pedal

Required features:

- deferred note-off
- half-pedal
- repedal
- pedal-down body sample variants
- pedal noise layers
- pedal-up release noise

Current support: CC64 defers note-off for convention and zone-mode playback,
pedal-up releases only notes that were released while the pedal was held,
pedal-down can swap to a pedal-body articulation, convention libraries can
trigger pedal-down mechanical noise when present, zone libraries can declare
explicit `pedal-down` and `pedal-up` noise layers, repedal restores voices that
are still in their release fade, and half-pedal values use progressively longer
damping release fades with library-authored `linear`, `squared`, or `sqrt`
curves and max-release multipliers.

### ⬜ Looping and slices

Playback modes:

- forward loop
- alternating loop
- reverse loop
- crossfade loop
- sustain loop
- release loop
- beat-synced loop
- slice playback
- one-shot loop pads

Kontakt 8's Leap direction is a useful reference for fast loop/one-shot
performance workflows. Current implementation supports basic forward loop
points, alternating loop, and reverse playback in zone mode; the other
loop/slice modes remain open.

### 🔲 Time and pitch modes

Required modes:

- normal pitched sample playback
- high-quality resampling for source/device sample-rate mismatch
- time-stretch
- beat sync
- granular/warp mode
- reverse playback

Current implementation supports normal pitched playback and basic reverse
zone playback. Higher-quality resampling, stretch, beat sync, and warp remain
open.

## Tier 3: Streaming and memory

### 🔲 Real disk streaming

The current cache decodes samples fully into RAM. Kontakt-scale libraries need:

- attack preload
- background disk streaming for tails
- streaming read-ahead
- per-voice stream cursors
- bounded memory budgets
- stream underrun diagnostics

### ⬜ Memory budgets and eviction

Implement:

- global cache memory budget
- per-instrument budget
- per-mic preload priority
- least-recently-used eviction
- unload old generation data after patch changes
- never evict active voice data

Current support: players can be constructed with a decoded PCM cache budget,
CLI/TUI can opt into budget enforcement, and the cache has deterministic
largest-first eviction that publishes a fresh audio-thread snapshot. Existing
voices keep their `Arc<SampleData>` handles, so playback already in flight is
not cut off by eviction. Remaining work: true LRU/use tracking, per-instrument
and per-mic priorities, automatic old-generation cleanup, and full disk
streaming.

### ☑️ Preload profiles

Profiles:

- fast audition
- performance
- full preload
- drum kit priority
- piano center-out
- orchestral articulation priority

Current support: `PreloadProfile` exposes every profile above, CLI/TUI accept
`--preload-profile`, and background pack/block/engine/preset preload uses the
profile for sample ordering, fast-audition truncation, full preload ordering,
drum-kit engine priority, piano center-out priority, and orchestral section
priority.

## Tier 4: Modulation

### 🔲 Modulation sources

Sources:

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

### 🔲 Modulation targets

Targets:

- volume
- pan
- tune
- sample start
- loop start/end
- filter cutoff/Q
- envelope times
- send level
- effect parameters

### 🔲 Mod matrix

Required controls:

- depth
- curve
- smoothing
- bipolar/unipolar mode
- per-voice vs global scope
- source transform/remap

## Tier 5: DSP, routing, and effects

### ⬜ Preset graph

Current preset graph supports engine/module routing, but modules are
pass-through. Required next work:

- pre-bucket resolved edges for faster render
- support module sends/returns
- support master buses
- expose per-bus meters
- support multi-output rendering

### 🔲 Per-voice DSP

- amp envelope
- pitch envelope
- filter
- saturation
- sample-start modulation

### 🔲 Group and bus effects

Effects list:

- EQ
- filter
- compressor
- limiter
- transient shaper
- saturation/distortion
- chorus/phaser/flanger
- delay
- reverb/convolution
- bitcrusher
- amp/cabinet

## Tier 6: Instrument authoring and import

### 🔲 Native editor model

Data model needs to support:

- zone editing
- group editing
- mapping view
- velocity layers
- round-robin lanes
- mic groups
- articulation map
- macro definitions
- module/effect graph

### 🔲 Auto-mapping tools

Tools:

- parse note names from filenames
- parse velocity/dynamic tokens
- parse RR tokens
- parse mic names
- detect release samples
- distribute key ranges
- distribute velocity ranges
- infer root key
- report unmapped files

### ⬜ Import targets

Priority:

1. SFZ subset
2. DecentSampler-style groups/effects/bindings
3. SoundFont/SF2 conversion via external tooling or import bridge
4. Kontakt concept compatibility, not direct encrypted-library import

Initial SFZ opcode subset:

- `sample`
- `lokey`, `hikey`, `key`
- `lovel`, `hivel`
- `pitch_keycenter`
- `tune`, `transpose`
- `volume`, `pan`
- `trigger`
- `seq_position`, `seq_length`
- `group`, `off_by`
- loop opcodes

Current support: `LibrarySpec::from_sfz()` imports a practical SFZ subset into
native zones, including sample paths, key/velocity ranges, root key, tune and
transpose, volume/pan, release/legato/first trigger labels, sequence position
and length as RR metadata, group/off_by, sample offset/end, and loop points.
Remaining targets are DecentSampler effects/bindings, SF2 bridge support, and
broader Kontakt concept mapping.

## Tier 7: TUI and CLI product layer

### ⬜ Browser workflow

Current browser supports search, favorites, preview, and patch loading. Add:

- favorite-only filter
- recents
- search scoring
- vendor/category/instrument facets
- patch metadata preview before load
- load history
- unload/reload commands

### ⬜ Performance controls

Add:

- audition note selector
- preview velocity
- MIDI channel selection
- loaded instrument mute/solo
- panic/all-notes-off distinction
- sustain pedal state display

### ⬜ Diagnostics

Expose:

- cache memory
- cache miss count
- missing sample count
- preload failures
- active voices per engine/group
- stolen voices
- render time
- callback overruns
- queue pending/dropped
- unresolved routing
- unsupported modules/effects
- export diagnostic report

## Tier 8: Testing

### ☑️ Offline no-speaker tests

- `SamplerPlayer::new_offline()`
- queued MIDI/event render path

### ⬜ Playback regression tests

Add tests for:

- note attack/release
- pedal defer/release
- panic clears voices immediately
- voice stealing fade/counter
- legato transition
- round-robin sequence
- no-repeat random
- missing sample behavior
- cache snapshot publishing
- preload cancellation

### 🔲 Integration/stress tests

Add:

- repeated patch load/play/unload
- fake JACK/PipeWire backend or fully offline equivalent
- long-running MIDI event queue stress
- large preset preload cancellation
- render graph routing tests

## References

- Kontakt 8 manual: <https://www.native-instruments.com/ni-tech-manuals/kontakt-manual/en/new-in-kontakt-8.html>
- Kontakt Classic view / Mapping Editor: <https://www.native-instruments.com/ni-tech-manuals/kontakt-manual/en/classic-view>
- Kontakt Building in Kontakt: <https://www.native-instruments.com/en/products/komplete/samplers/kontakt-8/building-in-kontakt/>
- Ableton Sampler manual: <https://www.ableton.com/en/live-manual/11/live-instrument-reference/>
- Ableton multisampling guide: <https://help.ableton.com/hc/en-us/articles/115001318670-How-To-Multisampling-with-Sampler>
- SFZ opcodes: <https://sfzformat.com/opcodes/>
- SFZ sample opcode docs: <https://sfzlab.github.io/sfz-website/documentation/syntax/opcodes/sample/>
- sfizz reference: <https://docs.cycling74.com/reference/sfizz~/>
- sforzando: <https://www.plogue.com/products/sforzando.html>
- DecentSampler developer guide: <https://decentsampler-developers-guide.readthedocs.io/_/downloads/en/stable/pdf/>
