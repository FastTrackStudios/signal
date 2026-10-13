# PT Bus Routing — RE Status (PTX → RPP)

Status: **main-output routing readable today; aux sends need real-session RE.**

## Key finding: the RPP→PTX converter is LOSSY for sends

The author-RPP → convert → parse probe methodology (which worked for
automation/folders/colors) **does not work for bus sends**. Verified with
three probes (`gen_bus.rs`):

- `bus_three_plain` — SrcA, SrcB, BusC (no routing).
- `bus_send_one` — `BusC.receive(0)` (BusC ← SrcA).
- `bus_two_senders` — `BusC.receive(0).receive(1)` (BusC ← SrcA, SrcB).

Results: adding a receive promotes BusC out of the audio-track list into an
`internal_track` (output shrinks 76024→75648). But **`send_one` and
`two_senders` are byte-identical except the bus's `routing_uid` and per-track
nonce** — the second sender produces *zero* difference in the source tracks.
Send-slots (`bs+7350+4i`) are all-zero; active `0x2602` routing entries have
`destination_uid = 00…00`. So the converter does **not** encode sender→bus
topology when generating PTX from RPP. (Same class of problem as clip gain:
the converter's PTX output isn't faithful for this feature.)

**Conclusion:** bus-send RE must use **real PT-native sessions**, not
converter output. Target asset: `~/Downloads/Routing Examples.ptx` (a real PT
session built to demo routing; track names describe intent, e.g.
"Name 1 to Name 2", "Name 3 to Aux Group 1").

## What IS readable today

### Main output routing — `0x260e` (TrackRouting), reliable
Each track's `0x260d` wrapper has a `0x260e` child holding a
**length-prefixed destination NAME string at payload `+0x24`**. Parsed into
`Track.output` (`parse/mod.rs` ~482–556, name-keyed `out_by_name` map). On
`Routing Examples.ptx` this correctly yields e.g.
`'Name 1 to Name 2' → output "Name 2 From Name 1"`,
`'Child 2' → output "Aux Group 1"`. Destination may be another track, an
internal bus track, or hardware ("CR", "Analog 1-2").

### Buses — `internal_tracks` (`0x261e`)
A track that is a bus/aux/master lives in `session.internal_tracks` with a
`name` + `routing_uid` (e.g. "Aux Group 1 to Bus 9-10"). Buses are NOT in
`audio_tracks`.

### I/O channels — `0x1021`/`0x1022`
`session.io_channels`: hardware + bus output channels with names and 6-byte
`uid` (`parse/io.rs`). "Bus 1-2".."Bus N", "Analog 1-2", etc.

## What is NOT solved
- **`0x2602` routing entries are unreliable** — `destination_uid`
  (magic+47..52) resolves to mostly-garbage uids on the real session
  (`dump_routings_resolved` → most are `?uid=…`). The active-flag/uid model
  needs re-RE before use.
- **Aux sends** (the 10 per-track send slots, additional to main output) —
  not parsed; converter probe is lossy (above); needs real-session RE.
- **Reaper emission** — `project_import.rs` does not yet translate
  `output`/buses into `AUXRECV`/`MAINSEND`. The RPP builder side is ready:
  `TrackBuilder::receive(src_idx)`, `ReceiveSettings`, AUXRECV serialization.

## Recommended next step
Implement **Part 1** first: translate `Track.output` (when it names an
in-session track or `internal_track`) into a Reaper receive on the
destination + `MAINSEND 0` on the source. This covers main-output bus routing
using only the reliable `0x260e` data. Requires deciding how `internal_tracks`
(buses) are emitted as Reaper tracks in `project_import.rs`. **Part 2** (aux
sends) is a separate real-session RE task.

See `docs/pt-field-map.md` (send section) and the `project_ptx_rpp_features`
memory.
