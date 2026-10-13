# Item previews: real audio peaks and MIDI previews from both backends

Research for FastTrackStudios/session#25. Every claim below cites a file
in this worktree (branch `daw-ui-vello-arrangement`, `6329258`), the
sibling `../daw` checkout (`/run/media/Development/fts/daw`), or the
REAPER SDK headers vendored in reaper-rs at
`~/.cargo/git/checkouts/reaper-rs-ea00aff97ae2f1e5/3a9e706/main/low/lib/reaper/`
(the revision `../daw/Cargo.lock:10731` pins). Paths under `../daw` are
written `daw:` for brevity.

## Summary

1. Both backends already serve waveform peaks over one RPC, `Peaks::take_peaks` (`daw:crates/daw/proto/src/peak/service.rs:17-23`), and `daw_control::Take::peaks` wraps it (`daw:crates/daw/control/src/items.rs:723-735`) — the session window simply never calls it: items are flat rectangles (`apps/session-daw/src/arrangement.rs:449-468`) and the studio's "waveforms stream in behind them" is a placeholder string with no module behind it (`features/ui/daw-ui/src/studio/mod.rs:388-389`, `studio/project.rs:83-86`).
2. Standalone is the mature side: mmap'd WAV / decoded PCM (`daw:features/standalone/daw-standalone/src/audio_engine/source.rs:24-30`), min/max per block in **take time** (`take_reader.rs:228-270`), a revision-keyed in-memory cache (`peak.rs:60-69`) and a REAPER-compatible `.reapeaks` sidecar mipmap at 160/2400/48000 samples-per-peak (`peak_store.rs`, `daw:features/dawfile/dawfile-reaper/src/reapeaks.rs:11-25`).
3. REAPER's impl is a thin, buggy shim over `PCM_Source_GetPeaks`: it hardcodes 44.1 kHz and 2 channels, reads from source time 0 for the item length (ignoring `start_offset`/`play_rate`), and sizes the buffer for one block when the SDK writes two (max block, then min block) — so its layout does not match the `[min,max]`-pair layout the proto documents and standalone emits (`daw:features/reaper/daw-reaper/src/peak.rs:69-98` vs `reaper_plugin_functions.h:5416`).
4. A MIDI preview reads `Midi::notes` (`daw:crates/daw/proto/src/midi/service.rs:307`) — `MidiNote{pitch, velocity, start_ppq, length_ppq}` in quarter notes from the item start — and maps PPQ to seconds through the tempo map exactly as standalone's renderer does (`daw:features/standalone/daw-standalone/src/audio_engine/render/snapshot.rs:481-492`); REAPER can also render a MIDI take through the peak API itself (`PEAKTRANSFER_MIDI_NOTE_MODE`, `reaper_plugin.h:472-476`).
5. Recommendation: one wire type, `ItemPeaks` — per-channel `(min,max)` pairs in linear −1..1, at a caller-chosen samples-per-peak, in take (item-local) time, quantised to i16 like `.reapeaks` — served by both backends from their existing paths; a folder item folds children per column with `min(min)`/`max(max)`, not a mean (the editor's mean is a drum-mic special case, `features/expression-editor/spec/drum-mode.md:101-107`).

## What the session window draws today

`Arrangement::build` fills one rectangle per item — position to
position+length, inset into the lane, coloured by the item or its track
(`apps/session-daw/src/arrangement.rs:449-468`). No peaks are requested
anywhere in `apps/session-daw` (the only "simulated" signal in the app is
the rack's meter/compressor feed in `apps/session-daw/src/simulate.rs:1-5`,
not item waveforms). The Dioxus studio the app grew out of promises
peaks that "stream in afterwards (`super::waveform`)"
(`features/ui/daw-ui/src/studio/project.rs:83-86`) and shows "waveforms
stream in behind them…" while loading (`studio/mod.rs:388-389`), but
`features/ui/daw-ui/src/studio/` has no `waveform.rs`.

The project snapshot it builds from is `daw_proto::Item` grouped by
track (`studio/project.rs:49-79`). `Item` carries `position`, `length`,
`color`, `fixed_lane` (REAPER 7 comping lanes), `take_count` and
`active_take_index` (`daw:crates/daw/proto/src/item/item.rs:34-36,70,81,85,87`);
the active `Take` carries `play_rate`, `start_offset`, `source_type`
(`Audio | Midi | Empty | Video | Unknown`), `source_sample_rate` and
`source_channels` (`daw:crates/daw/proto/src/item/take.rs:43,53,57,63,65,77-89`).
`source_type` is what decides whether an item gets a waveform or a
note preview.

## REAPER peaks

### What the SDK offers

- `PCM_Source_GetPeaks(src, peakrate, starttime, numchannels,
  numsamplesperchannel, want_extra_type, buf)` — "Gets block of peak
  samples to buf. Note that the peak samples are interleaved, but in
  two or three blocks (maximums, then minimums, then extra). Return
  value has 20 bits of returned sample count, then 4 bits of
  output_mode (0xf00000), then a bit to signify whether extra_type was
  available (0x1000000)." (`reaper_plugin_functions.h:5413-5418`;
  official: https://www.reaper.fm/sdk/reascript/reascripthelp.html#PCM_Source_GetPeaks).
  `GetMediaItemTake_Peaks` is the same call addressed by take
  (`reaper_plugin_functions.h:2027-2030`).
- `PCM_Source_BuildPeaks(src, mode)` — mode 0 = `PeaksBuild_Begin`,
  1 = `PeaksBuild_Run` (returns percent remaining), 2 =
  `PeaksBuild_Finish`; "If PCM_Source_BuildPeaks(src,0) returns zero,
  then no further action is necessary" (`reaper_plugin_functions.h:5363-5368`).
  These are the `PCM_source` virtuals `Peaks_Clear`, `PeaksBuild_Begin/Run/Finish`
  (`reaper_plugin.h:584-587`).
- The transfer struct `PCM_source_peaktransfer_t` (`reaper_plugin.h:458-526`):
  `peakrate` (peaks/second), `numpeak_points`, `nchpeaks`, caller-allocated
  `peaks` (maxima) and optional `peaks_minvals` (minima), and `output_mode`
  ∈ {`PEAKS_MODE`, `WAVEFORM_MODE`, `MIDI_NOTE_MODE`, `MIDI_DRUM_MODE`,
  `MIDI_DRUM_TRIANGLE_MODE`} (`reaper_plugin.h:470-478`). The mode bits
  in the `GetPeaks` return value are this enum — a MIDI source answers
  in note mode.
- Peak files: `GetPeakFileName[Ex|Ex2]` resolves "filename.reapeaks, or a
  hashed filename in another path" (`reaper_plugin_functions.h:2394-2415`);
  `PeakGet_Create(fn, srate, nch)` / `PeakBuild_Create(src, fn, srate, nch)`
  open a cache for reading / building outside a `PCM_source`
  (`reaper_plugin_functions.h:5431-5449`). `REAPER_PeakBuild_Interface::GetPeakInfo`
  notes the builder "won't hit the highest resolution mipmap, just the
  10/sec one or so" (`reaper_plugin.h:992`).
- Source facts a peak request needs: `GetMediaSourceSampleRate` ("MIDI
  source media will return zero"), `GetMediaSourceNumChannels`,
  `GetMediaSourceLength` (`reaper_plugin_functions.h:2131-2158`).

### What daw-reaper exposes

`impl Peaks for Reaper` (`daw:features/reaper/daw-reaper/src/peak.rs:17-102`),
registered as `peak::Service` + `peak::StreamService`
(`daw:features/reaper/daw-reaper/src/services.rs:79,90`). `take_peaks`
resolves item → take → `PCM_source`, then calls the safe wrapper
`pcm_source_get_peaks` (`daw:features/reaper/daw-reaper/src/safe_wrappers/peak.rs:27-48`)
once for the whole item.

### What is wrong or missing on the REAPER side

1. **Sample rate is hardcoded.** `peak_rate = 44100.0 / block_size` and
   `sample_rate: 44100.0` (`peak.rs:69,94`); a 48 kHz session gets
   blocks that are `block_size × 48000/44100` samples long while
   claiming `samples_per_peak: block_size`. Fix: `GetMediaSourceSampleRate`.
2. **Channels are hardcoded to 2** (`peak.rs:70`); a mono source is
   asked for two channels, a 4-channel source loses two. Fix:
   `GetMediaSourceNumChannels`.
3. **Take placement is ignored.** The read starts at source time `0.0`
   for `length` seconds (`peak.rs:63-67,80`); `start_offset` and
   `play_rate` are not applied, so a trimmed or rate-changed take draws
   the wrong stretch of media. Standalone does apply them
   (`take_reader.rs:192-198`).
4. **Buffer layout disagrees with the proto and with standalone.** The
   SDK writes maxima as one block, then minima as a second block
   (`reaper_plugin_functions.h:5416`); daw-reaper allocates
   `num_channels * num_peaks` doubles (`peak.rs:75-76`) — room for one
   block — and returns that as `TakePeakData.peaks`, whose contract is
   per-peak `[ch0_min, ch0_max, ch1_min, ch1_max, …]`
   (`daw:crates/daw/proto/src/peak/types.rs:12-14`). Whether REAPER
   truncates or overruns is not stated in the header; either way the
   result is not what the type documents. The safe allocation per the
   header is `2 × nch × n` (`3 ×` with an extra type), and the mode/count
   bits of the return value must be decoded (the wrapper compares the
   raw return to zero, `peak.rs:87`).
5. **No range or cache.** One call returns the whole take at one block
   size; there is no `(start, end)` window and no build-status path
   (`PCM_Source_BuildPeaks`) for media whose `.reapeaks` does not exist
   yet, so a first draw of a fresh recording can be empty.

`daw_control::Take::peaks` documents that under REAPER the call "serves
from the `.reapeaks` cache when one exists — the same data REAPER's own
arrange view draws" and that "the standalone backend has no peak store
yet and returns an empty frame" (`daw:crates/daw/control/src/items.rs:718-724`).
The second half is stale — see below.

## Standalone peaks

- **Decoded sources.** `AudioSource` is `Memory(DecodedAudio)` (fully
  decoded interleaved f32, compressed formats) or `PcmFile(PcmFile)`
  (memory-mapped WAV via `fts_sample::mapped`, converted per block)
  (`daw:features/standalone/daw-standalone/src/audio_engine/source.rs:20-30`).
  `min_max_block(lo, hi, channel)` walks either directly
  (`source.rs:137-170`). The `decode` feature pulls
  `fts-sample/load,decode-compressed`; `audio` adds cpal
  (`daw-standalone/Cargo.toml:96,102`).
- **Take time.** `TakeReader::peak_block(t0, t1, out)` maps take seconds
  to source frames through `source_time` = `start_offset + t·play_rate`,
  or the stretch-marker map when markers exist (`take_reader.rs:192-205`),
  then writes `out[c*2] = min(≤0)`, `out[c*2+1] = max(≥0)` per channel
  (`take_reader.rs:228-270`). Spec: `r[drums.open.peaks]`
  (`features/expression-editor/spec/drum-mode.md:65-69`).
- **`take_peaks`.** Blocks are `block_size / source_rate` seconds of
  take time; the whole take is emitted as `blocks × channels × 2`
  doubles (`peak.rs:96-111`); missing media → empty `TakePeakData`
  (`peak.rs:73-77`). Results are cached per `(project, take, block)` at
  the project revision (`peak.rs:60-69,132-170`).
- **Peak cache on disk.** With the `reapeaks` feature (`Cargo.toml:119`),
  `PcmFile` sources get a `<media>.reapeaks` sidecar — REAPER's naming
  (`peak_store.rs:35-41`), validated by mtime, channels, rate and
  length (`peak_store.rs:55-70`), built by one PCM scan
  (`ReaPeaks::compute`, `peak_store.rs:101-106`). The format is
  reverse-engineered "byte-exact against a 70-file corpus written by
  REAPER 7.x": `RPKN` magic, nch, 3 levels, rate, source mtime, then
  per level `peak_count × nch × {max: i16, min: i16}` at 160 / 2400 /
  48000 samples-per-peak (`dawfile-reaper/src/reapeaks.rs:8-25,33-48`).
  Coarse zooms fold from the mipmap (`take_reader.rs:250-265`,
  `peak_store.rs:125-142`); zooms finer than 160 samples/peak read PCM.
  `ReaPeaks::columns` already renders "one pair per pixel column,
  REAPER's draw model" (`reapeaks.rs:263-300`).
- Registered as `peak::Service` / `peak::StreamService`
  (`daw:features/standalone/daw-standalone/src/services.rs:61,73`).
  Integration tests cover the sidecar round trip
  (`daw-standalone/tests/reapeaks_sidecar.rs:1-4,123-205`).

So standalone is the reference implementation of the proto contract;
the REAPER side has to be brought up to it, not the other way round.

## MIDI previews

- **Data.** `Midi::notes(location) -> Vec<MidiNote>`
  (`daw:crates/daw/proto/src/midi/service.rs:307`), addressed by
  `MidiTakeLocation{project, item, take}` (`service.rs:24-33`).
  `MidiNote` is `{index, channel, pitch, velocity, start_ppq, length_ppq,
  selected, muted}` with `start_ppq` documented as "quarter notes from
  take start" (`daw:crates/daw/proto/src/midi/note.rs:7-24`).
  `daw_control::MidiEditor::notes()` is the async client wrapper
  (`daw:crates/daw/control/src/midi_editor.rs:22-60`).
- **REAPER backend** reads every note with `MIDI_GetNote`
  (`daw:features/reaper/daw-reaper/src/midi.rs:136-157,269-275`).
- **Standalone backend** answers from `Project::midi_notes[take_guid]`,
  decoded from the RPP at load (`daw:features/standalone/daw-standalone/src/midi.rs:161-172`,
  `project_loader.rs:467-476`).
- **PPQ → seconds.** Take PPQ is relative to the item start and
  tempo-dependent; standalone's renderer converts by
  `item_start_beat = seconds_to_beat(item.position)`, then
  `beat_to_seconds(item_start_beat + ppq)` (`snapshot.rs:481-492`).
  Over the wire the same conversion is `PositionConversion::time_to_quarter_notes`
  / `quarter_notes_to_time` (`daw:crates/daw/proto/src/position_conversion/service.rs:28-34`).
  A preview that wants item-local seconds under a constant tempo can
  use `ppq × 60 / bpm` (the "old formula" the renderer comment names).
- **REAPER's own MIDI thumbnails** go through the peak API: a MIDI
  source's `GetPeakInfo` answers in `PEAKTRANSFER_MIDI_NOTE_MODE` /
  `MIDI_DRUM_MODE` (`reaper_plugin.h:472-476`), signalled in the
  `0xf00000` bits of `GetPeaks`' return. That is what REAPER draws in
  the arrange view; it is not needed here because `notes()` gives the
  structured data and both backends already serve it.

A MIDI preview is therefore a **note list**, not a peak array: `(start_s,
end_s, pitch, velocity, muted)` per note, item-local, drawn as bars in a
pitch range. Folding a folder item's MIDI children is a union of note
lists.

## What the editor already has

- `ExpressionDoc.peaks: Vec<f32>` — "the take's own amplitude, 0..1,
  sampled uniformly across `[start, end]`", drawn as a backdrop
  (`features/expression-editor/expression-editor-core/src/doc.rs:761-769`).
  It is **mono, absolute, max-only**: one value per 512-sample hop,
  `max(|v|)` over the hop (`expression-editor-host/src/analysis.rs:2,96-102`),
  persisted in the `FTSDRUM` analysis cache beside the `.rpp`
  (`analysis_cache.rs:36-49`).
- Its samples come from the audio-accessor RPC, not from peaks: the host
  pulls the whole take through `get_samples` in 65 536-frame chunks at
  the source rate (`expression-editor-host/src/audio_read.rs:98-145`).
  That is the decode-everything path the studio comment warns against
  for opening a session.
- `summed_columns` (`expression-editor-ui/src/stack/waveform.rs:35-62`;
  the ticket's `expression-editor-paint` path does not exist on this
  branch) folds members per viewport column: `member_column` averages
  the bins overlapping the column (`waveform.rs:8-21`), the lane value
  is the **mean across members**, then the lane is normalised so its
  loudest column is 1.0. Spec `r[drums.lanes.summed]` justifies the mean
  because "members are phase-aligned mics of one source"
  (`features/expression-editor/spec/drum-mode.md:101-107`); triggers are
  excluded so their silence does not pull the mean down
  (`drum-mode.md:118-124`).

The mean-and-normalise rule is right for a kick lane and wrong for an
arrangement: a folder item over a bass and a vocal is not two mics of
one source, and normalising per lane hides level differences between
items. The arrangement wants an envelope, not an analysis backdrop.

Note: the ticket cites `docs/spec/session/workflows.md`
(`flow.drums.comping.folder-items`); that file exists on no branch of
this repo (`docs/spec/session/` holds `combined-setlist.md`,
`routing-project.md`, `track-organization.md`). `drums.lanes.summed` is
the only written rule on summing children's peaks.

## Recommendation

Adopt one representation both backends already produce internally and
`.reapeaks` already stores: **per-channel `(min, max)` pairs, linear
−1..1, quantised to i16 (`/32767`), at an explicit `samples_per_peak`
in source samples, indexed in take time (item-local seconds), over a
requested `[start, end)` window** — with a `MidiPreview` sibling for
`SourceType::Midi`. Resolution is chosen by the caller from its zoom
(`samples_per_pixel = source_rate × secs_per_pixel`), and the backend
answers from the coarsest level that still resolves it (standalone:
`ReaPeaks::level_for`, `reapeaks.rs:254-261`; REAPER: `peakrate`).
Standalone serves this today from `TakeReader::peak_block` with the
sidecar behind it; REAPER serves it by fixing the five defects above
(`GetMediaSourceSampleRate`/`NumChannels`, `starttime = start_offset +
start·play_rate`, `2×nch×n` buffer split into max/min blocks, decode
the return bits, and `PCM_Source_BuildPeaks` when the cache is absent)
— no new SDK surface is needed. A folder item is a fold, not a mean:
per column and channel it takes `min` of children's minima and `max` of
children's maxima, each child first resampled onto the folder's column
grid by the same window-max rule `ReaPeaks::columns` uses
(`reapeaks.rs:283-298`); MIDI children contribute a union of note lists;
a child of the other kind is drawn in its own layer. That keeps a folder
item's picture identical to what REAPER draws for the same lanes
(min/max of mipmap windows), stays conservative (never narrower than
any child), and needs no per-lane normalisation. Keep `TakePeakData`
for the drum editor's whole-take reads and add the windowed type beside
it in `daw-proto`:

```rust
// daw-proto/src/peak/types.rs (sketch)

/// A window of waveform peaks for one take, in take time.
#[derive(Clone, Debug, Facet)]
pub struct PeakWindow {
    /// Take-local seconds the window starts at (0 = item start).
    pub start_secs: f64,
    /// Source samples summarised per peak (160 / 2400 / 48000 are the
    /// `.reapeaks` levels; any value is legal, coarser folds).
    pub samples_per_peak: u32,
    /// Source sample rate the ratio is measured in.
    pub sample_rate: u32,
    pub channels: u8,
    /// `count × channels × 2` values, per peak then per channel:
    /// `[ch0.min, ch0.max, ch1.min, ch1.max, …]`, i16 in −32767..32767
    /// (same quantisation and pair order as a `.reapeaks` level).
    pub pairs: Vec<i16>,
}

/// Notes of one MIDI take, in take time — the item preview for
/// `SourceType::Midi`.
#[derive(Clone, Debug, Facet)]
pub struct MidiPreview {
    pub notes: Vec<PreviewNote>,
    /// Lowest / highest pitch present, so a lane can scale itself.
    pub pitch_range: (u8, u8),
}

#[derive(Clone, Copy, Debug, Facet)]
pub struct PreviewNote {
    pub start_secs: f64,
    pub length_secs: f64,
    pub pitch: u8,
    pub velocity: u8,
    pub muted: bool,
}

#[architect::rpc]
pub trait Peaks {
    // …existing track_peak / take_peaks / meters…

    /// Peaks for `[start_secs, start_secs + len_secs)` of a take at the
    /// coarsest ratio ≤ `samples_per_peak` the backend can resolve.
    /// Missing media → empty `pairs`, `channels = 0`.
    fn peak_window(
        &self,
        location: TakeLocation, // {project, item, take} — 4-param cap
        start_secs: f64,
        len_secs: f64,
        samples_per_peak: u32,
    ) -> PeakWindow;

    /// The note preview of a MIDI take, PPQ already mapped to
    /// take-local seconds through the project tempo map.
    fn midi_preview(&self, location: TakeLocation) -> MidiPreview;
}
```

A folder item in the session domain then holds `Vec<child item guid>`
and no peak data of its own; its preview is computed client-side from
the children's `PeakWindow`s (`fold_min_max`) and `MidiPreview`s
(`union`), so the wire carries each take once regardless of how many
folders show it.
