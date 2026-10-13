# midicore

Ergonomic, backend-agnostic MIDI for FastTrackStudio — one of the two "central
systems" crates (alongside `audiocore`) that will eventually replace the ad-hoc
MIDI plumbing scattered across our crates.

## What it is

- **Typed messages + codec** (`event`) — `MidiEvent` over tight newtypes
  (`Channel` 0–15, `U7` 0–127, `PitchBend` 14-bit) that make illegal values
  unrepresentable, plus round-tripping to/from raw MIDI bytes (note-on velocity-0
  normalizes to note-off, pitch-bend centered at 8192, SysEx framing).
- **Facet-typed I/O types** (`port`) — `PortInfo` / `PortSelector`
  (default / by-id / name-contains / all / virtual) / `TimedEvent` / `MidiIoError`.
- **I/O as `#[architect::rpc]` services** (`service`, feature `vox`) — `MidiPorts`,
  `MidiInput` (`subscribe(selector, tx: Tx<TimedEvent>)`), `MidiOutput`. architect
  + vox emit the async client + dispatcher, so a backend is callable **in-process
  and over the wire** with no hand-written transport: a device in another
  process/machine becomes an `Rx<TimedEvent>` stream.
- **Declarative matching** (`filter`) — `Filter::Notes.and(Filter::OnChannel(..))`.

## Why architect

The backend surface is verb-shaped (list ports, open, send, subscribe), which is
exactly what `#[architect::rpc]` is for. We get the wire codec (facet), the async
client, the dispatcher, and streaming for free — the same plumbing daw's services
use — instead of hand-writing a transport per backend.

## Backend adapters (separate crates, implement the `service` traits)

- `midicore-midir` — hardware I/O over `midir`.
- a daw-backed adapter — bridges `daw-midi-io` (device/all/virtual) into these
  traits, so signal's live rigs move onto midicore.
- `midicore-virtual` — an in-memory test backend.

## Design notes & roadmap

Draws on [`helgoboss-midi`](https://github.com/helgoboss/helgoboss-midi):

- **Raw vs structured split** *(done)* — `raw::RawShortMessage` is a packed,
  `Copy`, allocation-free 3-byte short message with a `ShortMessage` accessor
  trait and lossless conversion to/from the non-SysEx `MidiEvent` subset. SysEx
  stays off the `Copy`/RT path. Pick raw for throughput, structured for matching.
- **Semantic newtypes** *(done)* — distinct `KeyNumber` / `Velocity` /
  `ControllerNumber` / `ControllerValue` / `ProgramNumber` / `Pressure` types
  plus raw `U4` / `U7` / `U14`, each with `new` (clamping, `const`) + `try_new`
  (fallible), so a note number can't be passed where a controller value is.
- **Aggregators** *(planned)* — 14-bit CC and RPN/NRPN scanners that fold
  multi-message sequences into single events (helgoboss's
  `ControlChange14BitMessage` / `ParameterNumberMessage`).
- **Running-status parser** *(planned)* — a streaming `Decoder` that resolves
  running status; today `MidiEvent::decode` handles one framed message.

## Status

Compiles green on the upstream facet 0.50-rc / vox 0.10-rc stack; 5 unit tests
pass. Not yet pushed to codeberg (org over storage quota — pending GC).
