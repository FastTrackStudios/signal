# daw on macOS — handoff

The `daw` side of getting FastTrackStudio running on macOS (Apple Silicon,
macOS 27), 2026-09-21. The session-side story — the Blitz studio window,
preparing a multitrack, the guide — is in `session/docs/studio-handoff.md`.

## Pushed (`origin/main`, from branch `macos-compat`)

- Workspace builds and tests on macOS: `midicore-pipewire` is Linux-only.
- REAPER extensions install as `reaper_*.dylib`; REAPER found in
  `/Applications/REAPER.app`; `reaper.just` uses
  `~/Library/Application Support/REAPER`.
- VST3 host: `bundleEntry` gets the plugin's `CFBundleRef`; plugins are
  initialized before any state/parameter call (Pro-Q 4 segfaulted).
  `vst3-host` / `clap-host` imply `audio`. Plugins found in macOS vendor
  folders.
- `play_rpp`: runs in a tokio runtime, transport prompt on stdin,
  `--rate/--buffer/--device/--in/--duplex/--list-devices`; `just play <rpp>`.
- Requested sample rate / buffer size honoured and reported
  (`attach_audio_engine_with_prefs`, `device_caps`, `EngineStats`).
- **CoreAudio duplex backend**: a HAL IOProc on the device (input and output
  in one cycle), private aggregate for two devices
  (`daw-audio-io/src/duplex_coreaudio.rs`, `examples/duplex_probe`);
  `DuplexAudioEngine` / `Standalone::attach_duplex_engine` on macOS.

## Local only — branch `tag-section`, not pushed

| commit | what |
|---|---|
| `a7944c1b` | `SectionType::Tag` (key, name, colour, guide note 94) |
| `f9cc5331` | standalone ruler lanes: 0-based like REAPER's API, flags, default lanes, loader converts the file's 1-based rows |
| `98f09924` | music-convention pins → v0.1.3 (native colours were RGB on mac/Linux); standalone stores RGB from flagged native colours |
| `942076b8` | an xrun is counted and logged, not treated as a dead device |
| `4496af40` | `plugin::render_frame` / `render_cycle` / `track_muted` for plugins that coordinate across tracks |

## Open

- Linux CI for the pushed half ran on the self-hosted runner but was not
  checked to completion; the runner (`airlock`) is currently stopped.
- Low-latency monitoring through a track (a real input into the duplex
  engine) is proven at the backend level only — needs signal's rig on it.
- The duplex/IOProc path does not yet join the device's audio workgroup
  (only matters once DSP runs on worker threads).

## Continued 2026-09-22 (evening)

The work moved to the session app; its handoff is
`session/docs/handoff-live-session.md` (sibling checkout). What landed
here, on `tag-section`, local only:

- `dawfile-reaper::sessionpeaks` — the `.sessionpeaks` waveform cache
  (REAPER's `.reapeaks` bytes, in `Media/Peaks/`); daw-standalone's peak
  store reads and writes through it.
- `daw_standalone::session_file` (feature `session-file`, facade
  `standalone-session-file`) — a live project saved as `.session` and
  loaded back; built-in FX restored by name (`FxFactory::provides`).
- `project_loader::anchor_media` — each project's media anchored to its
  own (absolute) folder, for an engine holding a setlist.
- `decode` pulls `rtrb`; an empty marker GUID is written `""` so a region's
  lane survives the round trip.
