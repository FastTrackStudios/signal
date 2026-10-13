# Spectrum Analyzer — feature spec

Pro-Q 4-grade realtime spectrum analyzer for the FTS-EQ graph. Crate family:

- `spectrum-analyzer` — facade (`dsp` default, `ui` optional).
- `spectrum-analyzer-dsp` — pure analysis engine (no UI deps).
- `spectrum-analyzer-ui` — anyrender painters + Dioxus settings panel.

## Threading model

| Thread | Touches | Work |
|---|---|---|
| Audio (`process`) | `AudioFeed::push_*` | lock-free ring writes only — no FFT, no alloc, no locks |
| UI (`tick`, ~120 Hz) | `Analyzer::tick` / `snapshot` | drain rings → window → FFT → accumulate → smooth → dB → tilt → decay → collisions → publish/subscribe |

Resolution changes rebuild FFT buffers on the UI thread; the audio ring is fixed
at the maximum FFT size (8192 × 4 frames) so the audio thread never reallocates.

## Pro-Q 4 setting → behavior map

| Setting | Values | Implementation |
|---|---|---|
| Resolution | Low/Med/High/Max = 1024/2048/4096/8192 pt | `settings::Resolution::fft_size`; rebuilds the per-slot pipeline |
| Speed | Slow/Med/Fast/V.Fast (≈1.5/0.8/0.4/0.2 s release) | `decayer::SpectrumDecayer::set_decay` one-pole release |
| Tilt | dB/oct around 1 kHz (default 4.5) | `tilter::SpectrumTilter` per-bin `log2(f/1000)·slope` |
| Range | 60/90/120 dB (default 90) | `Range::db`; painter vertical scale (`AnalyzerSnapshot::range_db`) |
| Freeze | hold + build max | `decayer` running-max branch |
| Pre / Post | toggle pre- and post-EQ overlays | `show_pre` / `show_post`; pre+post fed from the plugin's process loop |
| SC / Ext | sidechain or another instance's spectrum | `show_external` + `sharing` registry; `set_external_source(id)` |
| Show Collisions | red highlight where pre & post both stand out | `collision::SpectrumCollision`, painted as red bands |
| Smoothing | octave width (0 = off) | `smoother::SpectrumSmoother` double boxcar |

## Cross-instance sharing

`sharing.rs` holds a process-global registry keyed by a unique `InstanceId`
(allocated in `eq-ui`'s `EqUiState::new`). Each instance publishes its post-EQ
spectrum every tick; any instance can subscribe to another's slot for SC/Ext.
Assumes a single host process (true for DAWs). `list_others` powers a source
picker; `unregister` runs on `Analyzer` drop.

## Integration points

- `apps/eq-plugin` — feeds pre (input) and post (output) mono through `AudioFeed`.
- `crates/eq-ui` — `EqUiState` owns the `Analyzer`; `control_view` ticks it and
  passes `AnalyzerSnapshot` to `EqGraph`; `eq_graph_painter::paint_analyzer`
  draws it (falling back to the legacy `spectrum_db` curve when empty).
- `apps/eq-standalone` — captures live system audio via `cpal` (patch the system
  monitor into the app's input in your patchbay) and feeds the analyzer.

## Attribution

Algorithms (accumulator, decayer, smoother, tilter, collision, sender/transfer)
are reimplemented from a study of ZLEqualizer (AGPLv3). No code is copied.
