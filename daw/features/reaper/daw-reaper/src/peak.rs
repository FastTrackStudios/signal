//! `impl Peaks for Reaper` — track meters + take waveform peaks.

use daw_control::lock::LockExt;
use daw_proto::{ItemRef, Peaks, ProjectContext, TakePeakData, TakeRef, TrackPeak, TrackRef};
use reaper_high::Reaper;
use reaper_medium::TakeAttributeKey;

use crate::project_context::find_project_by_guid;
use crate::safe_wrappers::item as item_sw;
use crate::safe_wrappers::peak as peak_sw;
use crate::track::resolve_track_pub;

fn resolve_project(ctx: &ProjectContext) -> Option<reaper_high::Project> {
    match ctx {
        ProjectContext::Current => Some(Reaper::get().current_project()),
        ProjectContext::Project(guid) => find_project_by_guid(guid),
    }
}

impl Peaks for crate::Reaper {
    fn track_peak(&self, project: ProjectContext, track: TrackRef, channel: u32) -> TrackPeak {
        (|| -> Option<TrackPeak> {
            let proj = resolve_project(&project)?;
            let t = resolve_track_pub(&proj, &track)?;
            let raw = t.raw().ok()?;
            let low = Reaper::get().medium_reaper().low();
            let peak_linear = peak_sw::track_get_peak_info(low, raw, channel as i32);
            let peak_hold_db = peak_sw::track_get_peak_hold_db(low, raw, channel as i32, false);
            let peak_db = if peak_linear > 0.0 {
                20.0 * peak_linear.log10()
            } else {
                -150.0
            };
            Some(TrackPeak {
                peak_db,
                peak_hold_db,
            })
        })()
        .unwrap_or_default()
    }

    /// Waveform peaks in take time, the standalone contract: one
    /// `(min ≤ 0, max ≥ 0)` pair per channel per block, blocks of
    /// `block_size` SOURCE samples of take playback time, covering the
    /// item's length. REAPER's `PCM_Source_GetPeaks` reads source time
    /// and answers in its own two-block layout, so the item's placement
    /// (start offset, play rate) is mapped onto the request and the
    /// answer re-interleaved — see [`PeakGeometry`] and
    /// [`sdk_blocks_to_pairs`].
    fn take_peaks(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        block_size: u32,
    ) -> TakePeakData {
        (|| -> Option<TakePeakData> {
            let reaper = Reaper::get();
            let medium = reaper.medium_reaper();
            let low = medium.low();

            let reaper_project_ctx = match &project {
                ProjectContext::Current => reaper_medium::ProjectContext::CurrentProject,
                ProjectContext::Project(guid) => {
                    let proj = find_project_by_guid(guid)?;
                    reaper_medium::ProjectContext::Proj(proj.raw())
                }
            };

            let item_ptr = crate::midi::resolve_item(medium, reaper_project_ctx, &item)?;
            let take_ptr = crate::midi::resolve_take(medium, item_ptr, &take)?;
            let source = item_sw::get_take_source(medium, take_ptr)?;
            // MIDI and empty sources report no rate: no waveform.
            let (rate, channels) = item_sw::get_take_source_format(medium, take_ptr)?;
            let item_medium = item_sw::get_take_item(low, take_ptr)?;
            let length = item_sw::get_item_info_value(
                medium,
                item_medium,
                reaper_medium::ItemAttributeKey::Length,
            );
            let placement = TakePlacement {
                start_offset: item_sw::get_take_info_value(
                    medium,
                    take_ptr,
                    TakeAttributeKey::StartOffs,
                ),
                play_rate: item_sw::get_take_info_value(
                    medium,
                    take_ptr,
                    TakeAttributeKey::PlayRate,
                ),
            };

            let geometry = PeakGeometry::new(rate, block_size, placement, length);
            if geometry.blocks == 0 {
                return Some(TakePeakData {
                    sample_rate: rate,
                    num_channels: channels,
                    peaks: Vec::new(),
                    samples_per_peak: geometry.block,
                });
            }

            // A source whose `.reapeaks` does not exist yet (a fresh
            // recording, an imported file) answers with nothing until
            // the cache is built.
            peak_sw::pcm_source_ensure_peaks(low, source);

            let nch = channels as usize;
            let mut buf = vec![0.0f64; 2 * nch * geometry.blocks];
            let ret = peak_sw::pcm_source_get_peaks(
                low,
                peak_sw::PeakRequest {
                    source,
                    peak_rate: geometry.peak_rate,
                    start_time: geometry.start_time,
                    num_channels: channels as i32,
                    num_samples_per_channel: geometry.blocks as i32,
                },
                &mut buf,
            );
            let peaks = sdk_blocks_to_pairs(&buf, SdkPeaks::decode(ret), geometry.blocks, nch);

            Some(TakePeakData {
                sample_rate: rate,
                num_channels: channels,
                peaks,
                samples_per_peak: geometry.block,
            })
        })()
        .unwrap_or_default()
    }
}

/// How a take-time peak request lands on REAPER's source-time peak API.
///
/// Peaks are blocks of `block` source samples of take PLAYBACK time (a
/// block spans the same wall time whatever the play rate does to the
/// source underneath — standalone's `take_reader::peak_block`), so a
/// block is `block / rate` seconds of take time and `play_rate` times
/// that of source time. `PCM_Source_GetPeaks` wants peaks per source
/// second, starting at a source time: `rate / (block × play_rate)`
/// from the take's start offset.
/// Where a take sits in its source: the seconds into the media the item
/// starts at (`D_STARTOFFS`) and how fast it plays through it
/// (`D_PLAYRATE`). The pair every placement-aware read needs, and the
/// pair the old shim ignored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TakePlacement {
    pub start_offset: f64,
    pub play_rate: f64,
}

impl TakePlacement {
    /// Playing straight through the source from its head.
    pub const NONE: Self = Self {
        start_offset: 0.0,
        play_rate: 1.0,
    };

    /// REAPER stores a non-positive rate for "unset"; it plays at 1.0.
    fn rate(self) -> f64 {
        if self.play_rate > 0.0 {
            self.play_rate
        } else {
            1.0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeakGeometry {
    /// `block_size`, at least 1.
    pub block: u32,
    /// Blocks covering the item's length in take time.
    pub blocks: usize,
    /// Peaks per SOURCE second for the SDK.
    pub peak_rate: f64,
    /// Source time the first block reads from — the take's start offset.
    pub start_time: f64,
}

impl PeakGeometry {
    pub fn new(rate: f64, block_size: u32, placement: TakePlacement, length: f64) -> Self {
        let block = block_size.max(1);
        let block_secs = block as f64 / rate;
        let blocks = if rate > 0.0 && length > 0.0 {
            (length / block_secs).ceil() as usize
        } else {
            0
        };
        Self {
            block,
            blocks,
            peak_rate: rate / (block as f64 * placement.rate()),
            start_time: placement.start_offset,
        }
    }
}

/// What `PCM_Source_GetPeaks` answered in: peak pairs, raw samples
/// (zoomed past the cache), or a MIDI note picture that is not a
/// waveform at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdkPeakMode {
    /// `PEAKTRANSFER_PEAKS_MODE` — maxima block then minima block.
    Peaks,
    /// `PEAKTRANSFER_WAVEFORM_MODE` — one block of signed samples.
    Waveform,
    /// The `MIDI_NOTE` / `MIDI_DRUM` / `MIDI_DRUM_TRIANGLE` modes.
    Midi,
}

/// The decoded `PCM_Source_GetPeaks` return word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SdkPeaks {
    /// Peaks actually written per channel (≤ requested; fewer at the
    /// end of the source).
    pub count: usize,
    pub mode: SdkPeakMode,
}

impl SdkPeaks {
    /// "Return value has 20 bits of returned sample count, then 4 bits
    /// of output_mode (0xf00000), then a bit to signify whether
    /// extra_type was available (0x1000000)."
    pub fn decode(ret: i32) -> Self {
        let word = ret.max(0) as u32;
        let mode = match (word & 0xf0_0000) >> 20 {
            0 => SdkPeakMode::Peaks,
            1 => SdkPeakMode::Waveform,
            _ => SdkPeakMode::Midi,
        };
        Self {
            count: (word & 0x0f_ffff) as usize,
            mode,
        }
    }
}

/// Re-interleave the SDK's answer into the proto's layout.
///
/// The SDK buffer holds `requested × nch` maxima (`buf[p × nch + c]`),
/// then `requested × nch` minima at the same stride — both output
/// pointers are fixed at the REQUESTED count before the source writes,
/// so a short answer leaves the minima block where it was. The result
/// is `[ch0_min, ch0_max, ch1_min, ch1_max, …]` per peak for all
/// `requested` peaks, silence past `answer.count`, with the standalone
/// clamp: min never above zero, max never below it.
pub fn sdk_blocks_to_pairs(
    buf: &[f64],
    answer: SdkPeaks,
    requested: usize,
    nch: usize,
) -> Vec<f64> {
    // Nothing to draw is an EMPTY frame, not a frame of silence — a
    // source whose peak cache is still building must not draw as a
    // flat line (standalone's empty-on-missing-media contract).
    if answer.mode == SdkPeakMode::Midi || nch == 0 || answer.count == 0 {
        return Vec::new();
    }
    let count = answer.count.min(requested);
    let stride = nch * 2;
    let mut out = vec![0.0f64; requested * stride];
    let (maxima, minima) = if answer.mode == SdkPeakMode::Waveform {
        // One block of signed samples — each is its own bound, split
        // by sign below.
        (buf, None)
    } else {
        let (maxima, rest) = buf.split_at(buf.len().min(requested * nch));
        (maxima, Some(rest))
    };
    for p in 0..count {
        for c in 0..nch {
            let i = p * nch + c;
            let max = maxima.get(i).copied().unwrap_or(0.0);
            let min = match minima {
                Some(minima) => minima.get(i).copied().unwrap_or(0.0),
                None => max,
            };
            out[p * stride + c * 2] = min.min(0.0);
            out[p * stride + c * 2 + 1] = max.max(0.0);
        }
    }
    out
}

// ── Streaming: ~30 Hz meter frames into DawEventHub ──────────────────
//
// Meter frames stream from the central hub, fed by
// [`poll_and_broadcast_meters`] — called from the extension's ~30 Hz
// timer callback on the REAPER main thread (the same pump that drives
// `poll_and_broadcast_transport`; REAPER API calls are main-thread-only,
// so no spawned task can do this). Same wire semantics as
// daw-standalone's meter pump: one [`daw_proto::MeterFrame`] per tick
// for the active project, `tracks[i]` = project track index `i` (the
// order `Tracks::all` returns), linear `0..1` peak + decaying hold.

impl daw_proto::PeaksStreamSource for crate::Reaper {
    fn meters_hub(&self) -> &architect::PubSub<daw_proto::MeterFrame> {
        crate::event_hub::hub().meters_hub()
    }
}

/// Peak-hold decay applied per ~30 Hz poll tick. Standalone decays its
/// hold per audio block (`HOLD_DECAY = 0.96` at ≈100 blocks/s, i.e.
/// ×0.0169/s); `0.873^30 ≈ 0.0169` gives the same ~half-second fall at
/// the poll rate, so holds behave identically across backends.
pub const HOLD_DECAY_PER_TICK: f32 = 0.873;

/// Per-track decaying peak-hold state for one project's meter stream.
///
/// REAPER's own `Track_GetPeakHoldDB` hold has host-defined
/// latch/reset semantics; computing the hold here from the raw block
/// peaks keeps the wire contract byte-identical to daw-standalone's
/// `TrackMeter` (hold = `max(prev_hold * decay, peak)`, never below
/// the instantaneous peak). Pure — unit-testable without REAPER.
#[derive(Default)]
pub struct MeterHoldState {
    /// Decaying (L, R) hold per track index.
    holds: Vec<(f32, f32)>,
}

impl MeterHoldState {
    /// Fold one tick of per-track `(left, right)` block peaks into the
    /// decaying holds and return the assembled per-track levels, in
    /// input order. Resizes to the input's track count (tracks added
    /// since the last tick start from silence; removed tracks drop
    /// their hold state).
    pub fn frame_levels(
        &mut self,
        peaks: &[(f32, f32)],
        hold_decay: f32,
    ) -> Vec<daw_proto::TrackLevels> {
        self.holds.resize(peaks.len(), (0.0, 0.0));
        self.holds
            .iter_mut()
            .zip(peaks)
            .map(|(hold, &(left, right))| {
                hold.0 = (hold.0 * hold_decay).max(left);
                hold.1 = (hold.1 * hold_decay).max(right);
                daw_proto::TrackLevels {
                    peak_left: left,
                    peak_right: right,
                    hold_left: hold.0,
                    hold_right: hold.1,
                }
            })
            .collect()
    }
}

/// Hold state per project GUID, so tab switches don't cross-pollute
/// holds and each project's meters resume where they left off.
static METER_HOLDS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, MeterHoldState>>,
> = std::sync::OnceLock::new();

fn meter_holds() -> &'static std::sync::Mutex<std::collections::HashMap<String, MeterHoldState>> {
    METER_HOLDS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Poll REAPER track peaks for the ACTIVE project and publish one
/// [`daw_proto::MeterFrame`] through the [`crate::event_hub`] meters
/// hub. Skips all work while the stream has no subscribers.
///
/// **MUST be called from the REAPER main thread** — typically the
/// extension's ~30 Hz timer callback (`Track_GetPeakInfo` and project
/// enumeration are main-thread-only).
pub fn poll_and_broadcast_meters() {
    let hub = crate::event_hub::hub();
    if hub.meters_subscriber_count() == 0 {
        return;
    }

    let reaper = Reaper::get();
    let project = reaper.current_project();
    let project_guid = crate::project_context::project_guid(&project);
    let low = reaper.medium_reaper().low();

    // Post-fader block peaks, linear, in project track order — the
    // same order `Tracks::all` returns, so `tracks[i]` on the frame is
    // project track index `i` (standalone's contract).
    let peaks: Vec<(f32, f32)> = project
        .tracks()
        .map(|t| match t.raw() {
            Ok(raw) => (
                peak_sw::track_get_peak_info(low, raw, 0).max(0.0) as f32,
                peak_sw::track_get_peak_info(low, raw, 1).max(0.0) as f32,
            ),
            Err(_) => (0.0, 0.0),
        })
        .collect();

    let tracks = {
        let mut holds = meter_holds().lock_recoverable("peak::meter_holds");
        holds
            .entry(project_guid.clone())
            .or_default()
            .frame_levels(&peaks, HOLD_DECAY_PER_TICK)
    };

    hub.publish_meter_frame(daw_proto::MeterFrame {
        project_guid,
        tracks,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The SDK writes `PCM_Source_GetPeaks` output as two blocks — every
    /// channel's maxima for every peak, then every channel's minima —
    /// while the proto (and standalone) emit one `(min, max)` pair per
    /// channel per peak. A synthetic SDK-shaped buffer proves the
    /// re-interleave without REAPER.
    #[test]
    fn sdk_two_block_layout_becomes_min_max_pairs() {
        // 3 peaks × 2 channels, requested = returned = 3.
        // maxima block: p0(L,R) p1(L,R) p2(L,R); minima block: same order.
        let buf = [
            0.5, 0.1, 0.6, 0.2, 0.7, 0.3, // maxima
            -0.4, -0.05, -0.3, -0.15, -0.2, -0.25, // minima
        ];
        let got = sdk_blocks_to_pairs(
            &buf,
            SdkPeaks {
                count: 3,
                mode: SdkPeakMode::Peaks,
            },
            3,
            2,
        );
        assert_eq!(
            got,
            vec![
                -0.4, 0.5, -0.05, 0.1, -0.3, 0.6, -0.15, 0.2, -0.2, 0.7, -0.25, 0.3
            ]
        );
    }

    /// Fewer peaks returned than requested: the minima block still
    /// starts at the REQUESTED offset (the SDK sets both output
    /// pointers before it knows how many it will write), and the tail
    /// pads with silence so the frame keeps covering the whole item.
    #[test]
    fn short_return_reads_minima_at_requested_offset_and_pads() {
        let buf = [
            0.5, 0.6, 0.0, 0.0, // maxima, 4 requested, 2 returned
            -0.4, -0.3, 0.0, 0.0, // minima
        ];
        let got = sdk_blocks_to_pairs(
            &buf,
            SdkPeaks {
                count: 2,
                mode: SdkPeakMode::Peaks,
            },
            4,
            1,
        );
        assert_eq!(got, vec![-0.4, 0.5, -0.3, 0.6, 0.0, 0.0, 0.0, 0.0]);
    }

    /// Zoomed past the peak cache the SDK answers in waveform mode —
    /// one block of plain samples, no minima — which folds to a pair
    /// with the signed sample on its side of zero, like standalone's
    /// `min(≤0) / max(≥0)` contract.
    #[test]
    fn waveform_mode_splits_samples_by_sign() {
        let buf = [0.25, -0.5, 0.0];
        let got = sdk_blocks_to_pairs(
            &buf,
            SdkPeaks {
                count: 3,
                mode: SdkPeakMode::Waveform,
            },
            3,
            1,
        );
        assert_eq!(got, vec![0.0, 0.25, -0.5, 0.0, 0.0, 0.0]);
    }

    /// A MIDI source answers in a note mode: no waveform, empty frame.
    #[test]
    fn midi_modes_have_no_waveform() {
        let buf = [60.0, 62.0];
        let got = sdk_blocks_to_pairs(
            &buf,
            SdkPeaks {
                count: 2,
                mode: SdkPeakMode::Midi,
            },
            2,
            1,
        );
        assert!(got.is_empty());
    }

    /// The return word packs count (20 bits) | mode (4 bits) | extra flag.
    #[test]
    fn return_word_decodes_count_and_mode() {
        assert_eq!(
            SdkPeaks::decode(0),
            SdkPeaks {
                count: 0,
                mode: SdkPeakMode::Peaks
            }
        );
        assert_eq!(
            SdkPeaks::decode(1234),
            SdkPeaks {
                count: 1234,
                mode: SdkPeakMode::Peaks
            }
        );
        assert_eq!(
            SdkPeaks::decode((1 << 20) | 7),
            SdkPeaks {
                count: 7,
                mode: SdkPeakMode::Waveform
            }
        );
        assert_eq!(SdkPeaks::decode((2 << 20) | 9).mode, SdkPeakMode::Midi);
        assert_eq!(SdkPeaks::decode((4 << 20) | 9).mode, SdkPeakMode::Midi);
        // The extra-type bit does not leak into the count.
        assert_eq!(SdkPeaks::decode(0x100_0000 | 5).count, 5);
    }

    /// Block geometry: peaks are in take time at the source rate, so a
    /// block is `block / rate` seconds of take time and `play_rate` times
    /// that in source time — the SDK's peak rate is in source seconds.
    #[test]
    fn geometry_maps_take_blocks_onto_source_time() {
        let placement = TakePlacement {
            start_offset: 0.1,
            play_rate: 2.0,
        };
        let g = PeakGeometry::new(48_000.0, 1200, placement, 14.0 * 1200.0 / 48_000.0 - 1e-6);
        assert_eq!(g.blocks, 14);
        assert!((g.peak_rate - 20.0).abs() < 1e-9); // 48000 / (1200 × 2)
        assert!((g.start_time - 0.1).abs() < 1e-12);

        // A non-positive play rate is REAPER's "1.0"; an empty item has no blocks.
        let unset = TakePlacement {
            play_rate: 0.0,
            ..TakePlacement::NONE
        };
        assert!(
            (PeakGeometry::new(44_100.0, 2400, unset, 1.0).peak_rate - 44_100.0 / 2400.0).abs()
                < 1e-9
        );
        assert_eq!(
            PeakGeometry::new(44_100.0, 2400, TakePlacement::NONE, 0.0).blocks,
            0
        );
    }

    /// Frames assemble from a fake peak source in input (project) order.
    #[test]
    fn frame_assembly_preserves_track_order() {
        let mut state = MeterHoldState::default();
        let levels = state.frame_levels(&[(0.1, 0.2), (0.3, 0.4), (0.5, 0.6)], 0.9);
        assert_eq!(levels.len(), 3);
        assert_eq!((levels[0].peak_left, levels[0].peak_right), (0.1, 0.2));
        assert_eq!((levels[1].peak_left, levels[1].peak_right), (0.3, 0.4));
        assert_eq!((levels[2].peak_left, levels[2].peak_right), (0.5, 0.6));
    }

    /// Hold latches to the peak, decays on quieter ticks, and never
    /// falls below the instantaneous peak — standalone `TrackMeter`
    /// semantics.
    #[test]
    fn hold_latches_decays_and_stays_above_peak() {
        let mut state = MeterHoldState::default();

        let first = state.frame_levels(&[(0.8, 0.4)], 0.5);
        assert_eq!(first[0].hold_left, 0.8); // first tick latches up
        assert_eq!(first[0].hold_right, 0.4);

        // Quieter tick: peak follows, hold decays (0.8 * 0.5 = 0.4).
        let second = state.frame_levels(&[(0.1, 0.1)], 0.5);
        assert_eq!(second[0].peak_left, 0.1);
        assert!((second[0].hold_left - 0.4).abs() < 1e-6);
        assert!(second[0].hold_left >= second[0].peak_left);
        assert!(second[0].hold_right >= second[0].peak_right);

        // Louder transient re-latches.
        let third = state.frame_levels(&[(0.9, 0.9)], 0.5);
        assert_eq!(third[0].hold_left, 0.9);
    }

    /// Track count changes are followed: new tracks start silent, and
    /// shrinking drops trailing hold state.
    #[test]
    fn resizes_with_track_count() {
        let mut state = MeterHoldState::default();
        state.frame_levels(&[(0.9, 0.9)], 0.9);

        let grown = state.frame_levels(&[(0.9, 0.9), (0.0, 0.0)], 0.9);
        assert_eq!(grown.len(), 2);
        assert_eq!(grown[1].hold_left, 0.0); // new track has no stale hold

        let shrunk = state.frame_levels(&[(0.1, 0.1)], 0.9);
        assert_eq!(shrunk.len(), 1);
    }
}
