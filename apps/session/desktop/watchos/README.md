# FastTrackStudio — watchOS remote

A standalone Apple Watch remote for the Signal guitar rig: the perform
grid's six footswitches on your wrist.

```
5 6        5 = Ambient       6 = Tap Tempo
3 4        3 = Drive         4 = Lead
1 2        1 = Clean         2 = Crunch
```

Tap a switch to press its footswitch stack (activate / rotate the scene);
**hold** (500 ms, same as the web perform grid) fires the counterpart
function shown as the secondary ring — FX toggle, boost cycle, song
prev/next, tuner. Colors mirror the Dioxus web UI's `folder_color`.

## Transports

- **Demo** — no connection; a full in-app mock of the rig (stacks,
  rotation, FX/boost/tempo logic) for showcasing the interaction.
- **Engine (HTTP)** — talks to `fasttrackstudio --engine` over the
  `/watch/v1` HTTP+SSE bridge on the local network. watchOS forbids
  WebSockets outside audio-streaming sessions (Apple TN3135), so the
  watch cannot speak vox's `/vox` WebSocket directly; the bridge is an
  in-process vox client on the engine instead.
- **BLE MIDI** — CoreBluetooth speaking the Bluetooth-LE MIDI GATT
  profile straight to any advertised BLE MIDI device (MidiWrist-style,
  no iPhone proxy): stack presses as Program Change 0–4, functions as
  CC 80–85. Works against any MIDI gear; the rig's MIDI input maps the
  same way.

The vendored vox Swift runtime (`libs/vendor/facet-swift`) is the future
native path — vox over a URLSession link inside an extended-runtime audio
session (the one WebSocket exception watchOS grants), and the full-fat
transport for iOS/macOS Swift clients.

## Generated types

`FTSWatch/Generated/WatchState.generated.swift` mirrors the facet DTOs in
`features/rigs/guitar/proto/src/watch.rs`. Regenerate after a proto change:

```bash
cargo run -p signal-guitar-proto --example gen_watch_swift \
  > apps/fasttrackstudio/watchos/FTSWatch/Generated/WatchState.generated.swift
```

## Building

Needs a Mac with Xcode 26+ and [XcodeGen](https://github.com/yonaskolb/XcodeGen):

```bash
cd apps/fasttrackstudio/watchos
xcodegen generate
xcodebuild -project FTSWatch.xcodeproj -scheme FTSWatch \
  -destination 'generic/platform=watchOS Simulator' build
```

Running on a physical watch needs a signing team: open the project in
Xcode once, set your team on the FTSWatch target, then build to the
paired watch via the iPhone.
