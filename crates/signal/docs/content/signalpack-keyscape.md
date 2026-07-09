# SignalPack Keyscape Notes

This document captures the current state of the Signal sampler Keyscape work so
it can be picked up in a later session without reconstructing the local context.

## Important Local Paths

Original Spectrasonics/Keyscape source area:

```text
/run/media/starcommand/Resources/Music/Audio Haven/Instrument Libraries/Spectrasonics/STEAM
```

Working sampled-audio extraction area:

```text
/run/media/AudioHaven/Sampled/Keys/Keyscape
```

SignalPack output area:

```text
/run/media/AudioHaven/Signal/Keys/Keyscape
```

The sampled area currently has 43 top-level Keyscape folders. 42 of those have
top-level `.flac`/`.wav` audio files and now have generated `library.styx`
mappings. `Factory` is present but has no direct top-level audio files in the
sampled area.

## Current Packed Libraries

These `.signalpack` files currently exist:

```text
/run/media/AudioHaven/Signal/Keys/Keyscape/Rhodes - LA Custom.signalpack
/run/media/AudioHaven/Signal/Keys/Keyscape/LA Custom C7 Grand.signalpack
/run/media/AudioHaven/Signal/Keys/Keyscape/Wurlitzer 200A.signalpack
/run/media/AudioHaven/Signal/Keys/Keyscape/Wurlitzer 140B.signalpack
```

Approximate current sizes:

```text
576M  Wurlitzer 200A.signalpack
2.5G  Rhodes - LA Custom.signalpack
3.4G  Wurlitzer 140B.signalpack
14G   LA Custom C7 Grand.signalpack
```

The four existing packs were repacked after the relative-path change. Their
sample index rows do not store absolute `/run/media/...` paths.

## SignalPack Format As Implemented

The pack file starts with a 64-byte binary header:

```text
magic:        SIGPACK\0
version:      1
kind:         5  (FLAC i24 PCM payloads)
index offset: u64
index length: u64
sample count: u64
```

The payload area stores one FLAC block per sample. For source `.flac` files the
packer currently embeds the original FLAC bytes directly after validating audio
metadata by decoding the sample. For non-FLAC sources, the packer decodes to PCM
and writes FLAC i24.

The index is plain UTF-8 text at the end of the file. It includes:

```text
# signalpack-index-v1
# spec_path        ...
# spec_format      styx
# spec_begin
...raw embedded .styx text...
# spec_end
# source    offset    bytes    channels    sample_rate    num_frames    samples
...
```

`source` is now relative to the `--samples-root` passed to `signal sampler pack`.
This is important: exports recreate clean names under the export directory
instead of writing `/run/media/...` paths.

Example export behavior:

```text
Record Noise 01_60.flac
```

exports as:

```text
<output-dir>/Record Noise 01_60.wav
```

not:

```text
<output-dir>/run/media/AudioHaven/...
```

## Current CLI Commands

Pack a library:

```bash
cargo run --release -p signal-cli -- sampler pack \
  "/run/media/AudioHaven/Sampled/Keys/Keyscape/Wurlitzer 200A/library.styx" \
  --samples-root "/run/media/AudioHaven/Sampled/Keys/Keyscape/Wurlitzer 200A" \
  --output "/run/media/AudioHaven/Signal/Keys/Keyscape/Wurlitzer 200A.signalpack"
```

Export a pack:

```bash
cargo run --release -p signal-cli -- sampler export \
  "/run/media/AudioHaven/Signal/Keys/Keyscape/Wurlitzer 200A.signalpack" \
  --output-dir /tmp/wurli-export
```

Play a sampled library from CLI:

```bash
cargo run --release -p signal-cli -- sampler play \
  "/run/media/AudioHaven/Sampled/Keys/Keyscape/Wurlitzer 200A/library.styx" \
  --samples-root "/run/media/AudioHaven/Sampled/Keys/Keyscape/Wurlitzer 200A" \
  --sample-rate 48000 \
  --buffer-size 64 \
  --preload
```

## Mappings Created

Every top-level Keyscape sampled audio folder has a generated `library.styx`.
Generation used filename-derived mappings:

- Known explicit parsers: Rhodes-style `RR01 lacrm 60 96`, LA Custom C7,
  Wurlitzer 200A, Wurlitzer 140B.
- Loose Keyscape fallback parser: derives articulation IDs from sanitized
  filename prefixes and extracts note, dynamic, round-robin, and release hints
  from numeric and textual tokens.

This means the mappings are complete enough for packing/exporting all top-level
audio files, but not all generated articulations are musically curated yet.

## Known Runtime Notes

- Signal CLI playback has been tested with a Komplete Kontrol S88 MIDI input and
  PipeWire/Yamaha output.
- The sampler supports low buffer sizes; 64 frames was tested. 256 frames at
  48 kHz was the original target for reliable low-latency playback.
- Preload was improved with Rayon-based parallel loading and a non-blocking MIDI
  event queue feeding the audio callback.
- Diagnostics currently include render timing, callback interval timing, xruns,
  and MIDI-to-callback timing.
- Sustain pedal is CC64. Note-off behavior was adjusted so notes stop when no
  longer held unless sustain is down, and pedal-up should not cut notes that are
  still physically held.
- Release samples are still an area for musical tuning. The current logic can
  trigger release articulations from the mapping, but Keyscape-like natural
  release behavior likely needs library-specific rules.

## Verification Already Run

Parser tests:

```bash
cargo test -p signal-sampler sample_map::tests -- --nocapture
```

This passed after adding the Keyscape parsers and loose parser.

Small signalpack relative-path check:

```bash
cargo run --release -p signal-cli -- sampler pack \
  "/run/media/AudioHaven/Sampled/Keys/Keyscape/Vinyl Keyscape 01/library.styx" \
  --samples-root "/run/media/AudioHaven/Sampled/Keys/Keyscape/Vinyl Keyscape 01" \
  --output /tmp/vinyl.signalpack

cargo run --release -p signal-cli -- sampler export \
  /tmp/vinyl.signalpack \
  --output-dir /tmp/vinyl-export
```

Export produced clean relative files:

```text
Record Noise 01_60.wav
Record Noise 02_60.wav
...
```

## Possible Improvements

High priority:

- Load/play directly from a flat `.signalpack` file without requiring the
  sampled source folder to be present.
- Store the embedded `.styx` as the primary playback spec for portable packs.
- Add a `signal sampler inspect-pack` command to show pack metadata, embedded
  spec summary, sample count, and whether sources are relative.
- Add a pack validator that decodes a subset or all payloads and checks every
  index row.

Mapping and musical behavior:

- Curate generated Keyscape mappings per instrument. The generated mappings are
  complete for file coverage, but many articulation names are mechanically
  derived.
- Identify main playable articulation automatically for each generated library.
- Add library-specific release-trigger rules. Some release/mechanical sounds
  should only trigger under particular note velocity, note-off velocity, pedal,
  or articulation conditions.
- Add proper pedal noise handling for pianos and EPs.
- Add per-instrument default output gain and velocity curves.

Format and tooling:

- Add a manifest block with library name, pack creation time, source root label,
  content hash, and generator version.
- Add optional compression experiments for non-FLAC/WAV sources, while keeping
  lossless export possible.
- Add random-access editing tooling for future sample library editor workflows.
  The current format is easy to inspect/export but changing one audio payload in
  place is not yet optimized.
- Support exporting to formats other than WAV.

Performance:

- Add automatic buffer-size probing later. The idea was deferred, but the CLI
  already has runtime diagnostics that can support this.
- Avoid opening the audio stream before preload, or suppress expected preload
  underrun diagnostics, so large library preload does not look like runtime
  failure.
- Cache pack index lookup by absolute scanned path after first suffix match.

