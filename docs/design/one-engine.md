# One engine

Signal runs inside a DAW the way Session does. A process runs **one
engine**: one `Standalone`, one project, one realtime audio engine on one
device. A rig is **tracks in that project** — a folder with its tracks and
the FX slots that hold its `PluginInstance`s — so any number of rigs play at
once, beside Session's own tracks. Scaling out means **more engine
processes of the same engine**, and the apps connect to as many as they
need (vox over WebSocket or iroh, served by architect's `EngineHost`).

## Where it stands (2026-10-13)

- Every live rig is "a tiny daw project" (`signal_rig_host::RigProject`):
  `RigProject::new` makes its **own** `Standalone`, seeds its own project,
  makes it current and opens its own engine — output-only for keys/sampler,
  duplex for guitar. Signal runs N engines and N device clients.
- `RigProject::on(daw, name)` already builds a rig into a `Standalone` it is
  given (the browser worklet uses it), but it still seeds a project of its
  own and steals "current project".
- Session attaches one duplex engine to its project
  (`Standalone::attach_duplex_engine`). A rig inside a Session track
  (`signal-fx::SignalRig` → `guitar::RigHost` → `sampler::HostedAudio`)
  renders through a **nested** `ProjectRenderer` — an engine in an engine.
- Meters: `Standalone::set_meters(Meters::new(n))` — one bank, indexed by
  track position, sized once. Every rig installs its own bank; tracks added
  after the engine starts are unmetered.
- MIDI: rigs share `signal_rig_host::midi_hub`; Session has no hardware MIDI
  (`LiveMidi for Standalone` is a stub); control surfaces open midir
  themselves.
- Tempo: `signal_rig_host::tempo` is a global BPM beside the transport.

## Target

```
process ── Engine ── Standalone ── project ── tracks
              │                          ├── Session's tracks
              │                          ├── Rig "Guitar" (folder: input, amp, cab, …)
              │                          └── Rig "Keys"   (folder: layers …)
              └── one duplex audio engine (input opens when a track is armed)
              └── one MIDI hub → per-track MIDI input
              └── EngineHost: /vox (ws + iroh), /health, web bundle

app (Session / Signal / phone / browser) ── connects to 1..N engines
```

- **`Engine`** (rig-host today, the engine's own crate later): owns the
  `Standalone`, the project, the audio engine, the transport. One per
  process; passed to rigs, never created by them.
- **`Rig`**: `engine.add_rig("Guitar")` → a folder track; `add_track`,
  `add_fx_slot`, `arm_input` scoped to it; dropping it removes its tracks.
  What `RigProject`/`RigHost` do now, minus owning a daw.
- **Meters** follow the track set and are read **by track** through the
  daw's Peaks / TrackLevels service — no rig-private banks.
- **MIDI** comes through one hub (`features/midi`) into the engine's live
  MIDI, routed to a track by its MIDI input — the same path for Session's
  instrument tracks and the rigs.
- **Tempo** is the transport's.
- **Many engines**: the same binary started again (`--engine`), each its own
  device and project; the app keeps a list of engines and a rig belongs to
  one of them.

## Steps

1. **Meters follow the track set** (daw-standalone): resize the bank when
   tracks are added/removed (off the audio thread, swapped in with the
   snapshot), and read peaks by track guid. Session benefits on its own.
2. **`Engine` + `Rig`** in rig-host: one `Standalone` + project + duplex
   engine with input-on-demand; `Rig` = folder-scoped tracks/slots;
   `RigProject::new` becomes a thin wrapper that makes a private `Engine`
   (so nothing breaks while rigs migrate).
3. **Rigs take an engine**: `GuitarRig`, `KeysRig`, `SamplerRig` (sampler)
   and the drums/ekit/synth backends build into a `Rig` they are given;
   their meters, MIDI and tempo go through the engine.
4. **signal-desktop runs one engine** for all its rigs; the rig services
   (vox) are mounted on it beside the daw facade.
5. **Session hosts rigs the same way**: a rig is tracks in Session's own
   project; `signal-fx`'s nested renderer goes.
6. **MIDI hub → `features/midi`**, `LiveMidi for Standalone` on it (Session
   gets hardware MIDI); control surfaces through `midi`.
7. **Many engines**: an engine list in the app shell; Signal/Session dial
   1..N engines (`EngineAddr` already parses ws/iroh connect strings).
8. **One `rigs` crate**: drums, ekit, synth (+ synth-rig), rig-host and
   rigs-proto first, per-rig features; guitar and keys after measuring
   incremental build time. Rig engines move out of `sampler`.
