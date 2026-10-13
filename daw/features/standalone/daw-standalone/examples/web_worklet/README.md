# daw-standalone — remote project playback in the browser

Host an `.rpp` project + its audio sources on any static-file host
(Nextcloud share link, S3 bucket, Cloudflare R2, GitHub Pages, your
laptop's `python3 -m http.server` — anything that serves bytes over
HTTP). The web demo loads the structure, fetches each source by URL,
decodes it via the browser's `AudioContext`, and plays the full
project through the WASM `ProjectRenderer`.

## Architecture

```
┌─────────────────────────────────────────┐
│ static host (Nextcloud / S3 / nginx…)    │
│   song.rpp                               │
│   audio/kick.wav, audio/snare.wav, …     │
└──────────────────┬──────────────────────┘
                   │ HTTP fetch
                   ▼
┌─────────────────────────────────────────┐
│ main thread (index.html)                 │
│   1. fetch RPP text                      │
│   2. ship to worklet → parse + load      │
│   3. takeSources() → [(take, path), …]   │
│   4. fetch + decodeAudioData each path   │
│   5. attachAudioSource(take, PCM)        │
│   6. postMessage('play')                 │
└──────────────────┬──────────────────────┘
                   │ port.postMessage
                   ▼
┌─────────────────────────────────────────┐
│ audio thread (AudioWorklet)              │
│   WebRenderer (Rust/WASM)                │
│     ├─ Standalone in-memory state        │
│     ├─ ProjectRenderer per quantum       │
│     └─ track / item / send routing       │
└──────────────────┬──────────────────────┘
                   │ stereo PCM
                   ▼
              AudioContext.destination
```

## Build the wasm bundle

```bash
cargo install wasm-pack  # one-time

wasm-pack build crates/daw-standalone \
    --target web \
    --no-default-features \
    --features "web,rpp-project-wasm,decode"
```

Drops `pkg/daw_standalone.js` + `pkg/daw_standalone_bg.wasm` under
`crates/daw-standalone/`. Copy the `pkg/` directory next to
`index.html`.

## Serve

```bash
cd crates/daw-standalone/examples/web_worklet
python3 -m http.server 8080
open http://localhost:8080/
```

`AudioWorklet.addModule` requires a same-origin server — `file://`
won't work.

## Hosting on Nextcloud / S3 / generic HTTP

The web app accepts a **base URL** that gets concatenated with each
source path. Whatever combination produces a working URL is fine —
common shapes:

| Host                 | Base URL                                                                      |
|----------------------|-------------------------------------------------------------------------------|
| Nextcloud share link | `https://cloud.example.com/s/SHARETOKEN/download?path=&files=`                |
| S3 / R2 public bucket | `https://bucket.example/songs/my-project/`                                   |
| GitHub Pages         | `https://user.github.io/repo/songs/my-project/`                               |
| Local dev            | `http://localhost:8080/songs/my-project/`                                     |

For Nextcloud: enable "Allow public download" on the share, then
**files** within the share map directly to `?files=path%2Fto%2Ffile.wav`.

## Native equivalent (same flow, no browser)

The `http-resolver` feature ships a sync `HttpBaseUrlResolver` that
fetches via `ureq`. Drop-in replacement for `ProjectRelativeResolver`
in `play_rpp`:

```rust
use daw_standalone::media_bay::HttpBaseUrlResolver;

daw.media_bay().set_file_resolver(Box::new(
    HttpBaseUrlResolver::new("https://cloud.example/s/TOKEN/download?path=&files=")
));
load_rpp_via_bay(&daw, name, "song.rpp", &rpp_text)?;
```

```bash
cargo run -p daw-standalone --features "rpp-loader,http-resolver" --example play_rpp -- ./local-or-remote.rpp
```

Same end result: project loads, audio decodes, cpal plays.

## Mapping source paths → take GUIDs

The browser flow uses `WebRenderer::take_sources()` which returns a
flat `[take_guid, path, take_guid, path, …]` array. Multiple takes
can reference the same path; the demo deduplicates fetches by
grouping takes per path, then attaches the same decoded PCM to each.

You can also use `pathsToResolve()` for a deduplicated list of paths
only.

## What works today

- Track playback, items + fades, send routing, varispeed, loop
- Source files via any static-host URL
- Tempo + tempo map
- Multi-channel tracks
- Hardware-output routing recorded into the project state (for now
  hardware outs sum to master alongside parent-send tracks)

## What's stub'd

- FX processing (synthetic `Param N` parameters; no actual DSP)
- MIDI playback (notes are stored + queryable but not rendered)
- Automation envelope render (envelopes are queryable but not yet
  applied during mix)

These are tracked as follow-up tasks in the workspace.
