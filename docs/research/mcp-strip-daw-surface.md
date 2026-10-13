# What the MCP strip reads from the `daw` crate

Research for [#129](https://github.com/FastTrackStudios/FastTrackStudio/issues/129),
under the wayfinding map [#124](https://github.com/FastTrackStudios/FastTrackStudio/issues/124).

**Question.** A mixer strip needs name, colour, volume, pan, mute, solo, record
arm, input monitoring, record input, FX chain state, sends and receives, and
meter levels. Which `daw` services carry those today, which are missing, and
what does a strip subscribe to for the values that change continuously?

**Method.** Read against the source in this tree — `crates/daw/proto` (the
service traits), `crates/daw/control` (the async facade), and the two backends
`features/reaper/daw-reaper` and `features/standalone/daw-standalone` — plus the
one existing consumer, `apps/fasttrackstudio/src/mixer_view.rs`. Every claim
below cites the file and line it came from.

**Headline.** The strip is in far better shape than the map assumed. Eleven of
the twelve fields are already carried, and **meters already exist as a proper
`#[subscribe]` stream implemented on both backends** — `Peaks::meters`, ~30 Hz,
one frame for the whole mixer, gated on subscriber count. There is no meter
design work to do; there is meter *plumbing* work. The single genuinely missing
read is **record input**, which is write-only today.

---

## The table

`Track` is the bulk read: one `Tracks::all` call returns a `Vec<Track>` carrying
most of a strip in a single round trip.

| Strip field | Carried by | Where |
| --- | --- | --- |
| Name | `Track.name`; write via `Tracks::rename` | `crates/daw/proto/src/track/track.rs:80`, `crates/daw/proto/src/track/service.rs:149` |
| Colour | `Track.color` (`Option<u32>`, `0xRRGGBB`); write via `Tracks::set_color` | `track/track.rs:82`, `track/service.rs:151` |
| Volume | `Track.volume` (normalized, 1.0 = 0 dB); write via `Tracks::set_volume` | `track/track.rs:96`, `track/service.rs:63` |
| Pan | `Track.pan` (−1..1); write via `Tracks::set_pan` | `track/track.rs:98`, `track/service.rs:65` |
| Mute | `Track.muted`; write via `Tracks::set_muted` | `track/track.rs:86`, `track/service.rs:51` |
| Solo | `Track.soloed`; write via `Tracks::set_soloed` (+ `set_solo_exclusive`, `clear_all_solo`) | `track/track.rs:88`, `track/service.rs:53-57` |
| Record arm | `Track.armed`; write via `Tracks::set_armed` | `track/track.rs:90`, `track/service.rs:59` |
| Input monitoring | `Track.input_monitor` (`InputMonitoringMode::{Off,Normal,NotWhenPlaying}`); write via `Tracks::set_input_monitor` | `track/track.rs:106`, `track/track.rs:258`, `track/service.rs:77` |
| **Record input** | **MISSING as a read.** `Tracks::set_record_input` is write-only; `RecordInput` never appears on `Track` and there is no getter. | write-only at `track/service.rs:170`; type at `track/track.rs:273` |
| FX chain state | Counts on `Track.fx_count` / `Track.input_fx_count`; the chain itself via `Effects::list` → `Vec<Fx>` (`name`, `plugin_name`, `enabled`, `offline`, `window_open`, `preset_name`) | `track/track.rs:140-142`, `fx/service.rs:34`, `fx/types.rs:125-146` |
| Sends / receives | `Routing::sends` / `Routing::receives` / `Routing::hardware_outputs` → `Vec<TrackRoute>` (volume, pan, muted, mono, phase, dest name). Parent/master send via `Routing::parent_send_enabled` | `routing/service.rs:42-44`, `routing/service.rs:133`, `routing/route.rs:85-115` |
| Meter levels | `Peaks::meters` — a `#[subscribe]` stream of `MeterFrame` (~30 Hz, whole mixer per frame). Point sampling via `Peaks::track_peak` | `peak/service.rs:31`, `peak/service.rs:12`, `peak/types.rs:24-46` |

Beyond the twelve, `Track` also already carries the things a REAPER-faithful
strip needs and the map has not yet listed: `phase_inverted`, `automation_mode`,
`selected`, `visible_in_mixer`, folder depth / `is_folder` / `parent_guid`, and
the full `TrackGrouping` VCA + gang matrix (`track/track.rs:99-142`,
`track/track.rs:210-229`).

---

## What is MISSING

1. **Record input read.** The only real gap in the twelve. `RecordInput` is a
   fully modelled type (`None` / `Midi { device_id, channel }` / `Audio { channel }`
   / `Raw(i32)`, `track/track.rs:273-291`) and the setter exists
   (`track/service.rs:170`), but nothing reads it back. A strip that shows an
   input selector cannot populate it.
   *Fix:* add `pub record_input: RecordInput` to `Track` and populate it in both
   backends. Putting it on `Track` rather than adding a `Tracks::record_input`
   method keeps the strip to one bulk read — the whole point of `Tracks::all`.

2. **No human-readable input names.** Even with #1, `RecordInput::Audio { channel: 3 }`
   is not a label. There is no service enumerating the host's audio/MIDI input
   channels — `Input` (`input/service.rs`) is the keyboard/mouse interception
   service, unrelated despite the name. A strip's input dropdown needs a
   `Vec<InputChannel { index, name }>` from somewhere; nothing provides it.

3. **No FX-chain change stream on `Effects`.** `Effects` has no `#[subscribe]`
   (`fx/service.rs` — the only subscribes in `daw-proto` are listed below). FX
   changes *do* reach clients, but only through the `EventBus` union stream as
   `DawEvent::Fx(FxStreamEvent)` (`event_bus/event.rs:32`). Same for routing —
   `RoutingEvent` has no per-service stream and rides `DawEvent::Routing`
   (`event_bus/event.rs:45`). That is workable but means a strip wanting live FX
   or send state must subscribe to the whole bus and filter.

4. **`Track` has no `parent_send` field.** `Routing::parent_send_enabled`
   (`routing/service.rs:133`) is a separate per-track call, so building N strips
   costs N extra round trips. `daw-ui`'s `TrackView` already has a `parent_send`
   field (`features/daw-ui/daw-ui/src/panels/model.rs:158`) that the current
   mixer never populates. Folding it onto `Track` would be one line per backend.

5. **Meters are 2 channels and active-project-only in REAPER.** The REAPER poll
   reads channels 0 and 1 only (`features/reaper/daw-reaper/src/peak.rs:200-209`)
   and covers `reaper.current_project()` only (`peak.rs:193`). Fine for a stereo
   strip; a surround or multichannel meter would need the frame widened.

---

## The two continuous cases

### Meters — already solved, use `Peaks::meters`

This is the part the ticket flagged as the interesting shape, and the answer is
that the interesting shape was already chosen and built correctly.

The contract (`peak/service.rs:26-32`, `peak/types.rs:36-46`):

- **Argless `#[subscribe]`**, per the architect idiom. `MeterFrame` carries its
  own `project_guid`; subscribers filter client-side.
- **One frame carries the whole mixer.** `frame.tracks[i]` is project track index
  `i`, the same order `Tracks::all` returns. This is the key wire decision — it
  is *not* one message per track per frame.
- **Linear `0..1`, converted to dB client-side.** `TrackLevels` is 4 × `f32`
  (peak L/R + decaying hold L/R) — 16 bytes per track per frame. A 32-track
  mixer at 30 Hz is ~15 KB/s of payload. That is not a flood.
- **Peak-hold is computed publisher-side** so both backends produce byte-identical
  ballistics — standalone decays per audio block, REAPER decays `0.873` per tick
  to match (`features/reaper/daw-reaper/src/peak.rs:121-125`).

Both backends implement it:

| Backend | Pump | Line |
| --- | --- | --- |
| REAPER | `poll_and_broadcast_meters`, called from the extension's ~30 Hz main-thread timer (`Track_GetPeakInfo` is main-thread-only) | `features/reaper/daw-reaper/src/peak.rs:186` |
| Standalone | `spawn_meter_pump`, a 33 ms loop reading the meter bank's atomics — RT-safe, the audio callback never blocks | `features/standalone/daw-standalone/src/sync/daw.rs:547` |

Two properties make it safe over RPC, and both are already there:

- **Subscriber-count gating.** REAPER's pump returns immediately when nobody is
  listening (`peak.rs:187-190`), so a closed mixer costs zero main-thread work.
  Standalone spawns its pump lazily from the first `meters_hub()` call, so a
  backend used synchronously (offline render, tests) never starts one
  (`sync/daw.rs:541-550`).
- **Backpressure by drop.** The hub is `architect::PubSub`; a slow client lags
  and loses frames rather than stalling the pump. Correct for meters — a stale
  meter frame has no value, so dropping is the *right* failure mode.

**Recommendation: change nothing.** The MCP strip subscribes to
`Daw::meter_events()` (`crates/daw/control/src/lib.rs:382`) for all projects, or
`Project::meter_events()` (`crates/daw/control/src/project.rs:215`) for one, and
indexes `frame.tracks[i]` into per-strip `Signal<f32>`s. `mixer_view.rs:216-247`
is a working reference implementation. Do **not** invent a per-strip meter
subscription — it would be N times the messages for the same bytes, and would
break the index alignment that makes the current frame cheap.

### Fader drag — local optimistic signal, write-through on change

The in-flight value is owned **locally by the UI**, not by the service. The
existing mixer establishes the pattern (`apps/fasttrackstudio/src/mixer_view.rs`):

- `TrackView` holds `fader: Signal<f32>` (`features/daw-ui/daw-ui/src/panels/model.rs:132`).
  The drag mutates that signal; the strip renders from it immediately, so the
  fader is never waiting on a round trip.
- A sibling `TrackSync` component (`mixer_view.rs:88`) holds one `use_effect` per
  control that watches the signal and pushes the change to the engine —
  `h.set_volume(vol)` at `mixer_view.rs:129-136`. The comment at
  `mixer_view.rs:78-86` names this deliberately: the MCP stays a pure view, and
  `TrackSync` is "the one place UI intent becomes engine state."
- `TrackView`s are built once per track-set change inside an effect, specifically
  so the per-track signals are stable across re-renders and "a fader drag isn't
  reset on the next re-render" (`mixer_view.rs:193-206`).

This is the right shape and it works identically in-process and over RPC, because
the UI never blocks on the write. Three refinements the MCP should make:

1. **Throttle the write-through.** Today every signal change spawns an RPC. A
   drag at 60 fps is 60 `set_volume` calls per second per fader — invisible
   in-process, wasteful over a WebSocket. Coalesce to ~30 Hz with a trailing
   edge so the final value always lands.
2. **Suppress echo during the drag.** `TrackEvent::VolumeChanged`
   (`crates/daw/proto/src/track/event.rs:28`) comes back from the backend,
   including from your own write. Applying it mid-drag fights the user's finger.
   The strip needs a "locally owned" flag per control, released shortly after
   the drag ends, during which inbound `VolumeChanged` for that track is ignored.
   The current mixer sidesteps this only because it never subscribes to track
   events at all — it fetches once per song change (`mixer_view.rs:162-191`).
   An MCP that stays in sync with REAPER cannot sidestep it.
3. **Subscribe to `Tracks::events` for everything else.** The current mixer's
   refetch-on-song-change means a rename or colour change in REAPER never reaches
   the strip. `Tracks::events` (`track/service.rs:208`) is the argless
   `#[subscribe]` stream of `TrackStreamEvent { project_guid, event }`
   (`track/event.rs:66`), and `TrackEvent` already covers every strip field that
   changes discretely: `Renamed`, `ColorChanged`, `MuteChanged`, `SoloChanged`,
   `ArmChanged`, `VolumeChanged`, `PanChanged`, `SelectionChanged`,
   `PhaseInvertedChanged`, `InputMonitorChanged`, `Added`/`Removed`/`Moved`
   (`track/event.rs:12-54`).

**The strip's data contract, then, is three subscriptions and one bulk read:**
`Tracks::all` to seed, `Tracks::events` for discrete changes, `Peaks::meters` for
levels, and `EventBus::events` filtered to `Fx` + `Routing` if the strip shows
live FX or send state.

---

## Appendix: every `#[subscribe]` stream in `daw-proto`

The root `CLAUDE.md` says five subscriptions were converted. There are **seven**
today, all argless and all filtered client-side, which is the architect idiom:

| Service | Stream | Line |
| --- | --- | --- |
| `Tracks` | `TrackStreamEvent` | `track/service.rs:208` |
| `TempoMap` | `TempoMapStreamEvent` | `tempo_map/service.rs:76` |
| `Markers` | `MarkerStreamEvent` | `marker/service.rs:53` |
| `Regions` | `RegionStreamEvent` | `region/service.rs:67` |
| `Transport` | `TransportStreamEvent` | `transport/service.rs:93` |
| `EventBus` | `DawEvent` (union of all domains) | `event_bus/service.rs:17` |
| `Peaks` | `MeterFrame` | `peak/service.rs:31` |

`Effects` and `Routing` have **no** stream of their own; their events exist
(`FxStreamEvent`, `RoutingEvent`) but reach clients only via `EventBus`.
