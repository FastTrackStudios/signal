//! `impl AudioAccessors for Standalone` — decode a take's source file, or
//! a track's playing items mixed.
//!
//! REAPER's audio accessor is a handle you pull rendered samples from,
//! at whatever rate and channel count you ask for. Standalone has no
//! render graph to pull from, but it does have the take's source file,
//! and decoding it gives the same thing: the audio that take plays.
//!
//! That is enough for the callers this exists for — the expression
//! editor's audio session, and anything else that wants to *analyse* a
//! take rather than hear it. What it deliberately does not do is apply
//! item FX or track processing: neither is modelled here, and a caller
//! that got FX from REAPER and dry audio from standalone would be
//! comparing two different signals without being told.
//!
//! Accessors are handles rather than values because that is the shape
//! REAPER's API has and the facade follows it. Here a handle owns the
//! decoded buffer, so `get_samples` is a copy rather than a decode and
//! reading a take in chunks costs one decode rather than one per chunk.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use daw_proto::{
    AudioAccessors, AudioSampleData, GetSamplesRequest, ItemRef, ProjectContext, TakeRef, TrackRef,
};

use crate::sync::Standalone;

/// What an accessor reads: one take (its time from the item's start), or a
/// track's items as they play into it (project time).
#[cfg(any(feature = "audio", feature = "decode"))]
enum OpenAccessor {
    Take(crate::take_reader::TakeReader),
    Track(TrackMix),
}

#[cfg(any(feature = "audio", feature = "decode"))]
impl OpenAccessor {
    fn channels(&self) -> u16 {
        match self {
            Self::Take(r) => r.channels(),
            Self::Track(m) => m.channels,
        }
    }

    fn sample_rate(&self) -> u32 {
        match self {
            Self::Take(r) => r.sample_rate(),
            Self::Track(m) => m.sample_rate,
        }
    }

    fn sample(&self, t: f64, channel: usize) -> f32 {
        match self {
            Self::Take(r) => r.sample(t, channel),
            Self::Track(m) => m.sample(t, channel),
        }
    }
}

/// One item of a track, as it plays into the track: where, how long, its
/// gain and fades, and its take's audio — opened the first time a read
/// reaches it (a slip-edited track is thousands of items, and a read of a
/// span touches a few).
#[cfg(any(feature = "audio", feature = "decode"))]
struct Placed {
    at: f64,
    length: f64,
    gain: f32,
    fade_in: f64,
    fade_out: f64,
    guid: String,
    reader: OnceLock<Option<crate::take_reader::TakeReader>>,
}

/// A track's playing items mixed — what they play into the track, before
/// its FX: each on the lane that plays and not muted, at its place, through
/// its item volume and its fades (linear). Sorted by start, with the
/// longest, so a sample looks only at the items near it.
#[cfg(any(feature = "audio", feature = "decode"))]
struct TrackMix {
    daw: Standalone,
    project: ProjectContext,
    items: Vec<Placed>,
    longest: f64,
    channels: u16,
    sample_rate: u32,
}

#[cfg(any(feature = "audio", feature = "decode"))]
impl TrackMix {
    /// Item `p`'s take, opened on first use.
    fn reader<'a>(&self, p: &'a Placed) -> Option<&'a crate::take_reader::TakeReader> {
        p.reader
            .get_or_init(|| {
                crate::take_reader::TakeReader::open_or_decode(
                    &self.daw,
                    self.project.clone(),
                    ItemRef::Guid(p.guid.clone()),
                    TakeRef::Active,
                )
            })
            .as_ref()
    }

    fn sample(&self, t: f64, channel: usize) -> f32 {
        let past = self.items.partition_point(|p| p.at <= t);
        let mut sum = 0.0_f32;
        for p in self.items[..past].iter().rev() {
            if p.at < t - self.longest {
                break;
            }
            let into = t - p.at;
            if into >= p.length {
                continue;
            }
            let mut gain = p.gain;
            if p.fade_in > 0.0 && into < p.fade_in {
                gain *= (into / p.fade_in) as f32;
            }
            let left = p.length - into;
            if p.fade_out > 0.0 && left < p.fade_out {
                gain *= (left / p.fade_out) as f32;
            }
            let Some(reader) = self.reader(p) else {
                continue;
            };
            let ch = channel.min(usize::from(reader.channels().max(1)) - 1);
            sum += reader.sample(into, ch) * gain;
        }
        sum
    }
}

/// The track `track`'s playing items, mixed ([`TrackMix`]). `None` when it
/// has none with audio to read.
#[cfg(any(feature = "audio", feature = "decode"))]
fn open_track(daw: &Standalone, project: ProjectContext, track: TrackRef) -> Option<TrackMix> {
    use daw_proto::Items as _;
    let lanes = daw_proto::Tracks::get(daw, project.clone(), track.clone())?;
    let plays = |lane: Option<u32>| {
        lanes.lane_count <= 1
            || 1_u64
                .checked_shl(lane.unwrap_or(0))
                .is_some_and(|bit| lanes.lane_play_mask & bit != 0)
    };
    let mut items: Vec<Placed> = daw
        .get_items(project.clone(), track)
        .into_iter()
        .filter(|i| !i.muted && plays(i.fixed_lane))
        .map(|i| Placed {
            at: i.position.as_seconds(),
            length: i.length.as_seconds(),
            gain: i.volume as f32,
            fade_in: i.fade_in_length.as_seconds(),
            fade_out: i.fade_out_length.as_seconds(),
            guid: i.guid,
            reader: OnceLock::new(),
        })
        .collect();
    items.sort_by(|a, b| a.at.total_cmp(&b.at));
    let longest = items.iter().map(|p| p.length).fold(0.0, f64::max);
    let mut mix = TrackMix {
        daw: daw.clone(),
        project,
        items,
        longest,
        channels: 1,
        sample_rate: 48_000,
    };
    // Its format: the first item with audio says it. None has any: nothing
    // to read.
    let (channels, rate) = mix
        .items
        .iter()
        .find_map(|p| mix.reader(p).map(|r| (r.channels(), r.sample_rate())))?;
    mix.channels = channels;
    mix.sample_rate = rate;
    Some(mix)
}

/// Without a decoder there is no audio to read; the table holds nothing.
#[cfg(not(any(feature = "audio", feature = "decode")))]
struct OpenAccessor;

/// Open accessors, keyed by the handle handed out.
///
/// Process-global rather than per-`Standalone`, because `Standalone` is
/// a cloneable handle to shared state and an accessor created through
/// one clone has to be readable through another — the same way a
/// REAPER accessor is valid wherever the API is.
fn table() -> &'static Mutex<HashMap<String, Arc<OpenAccessor>>> {
    static TABLE: OnceLock<Mutex<HashMap<String, Arc<OpenAccessor>>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_id() -> String {
    static N: AtomicU64 = AtomicU64::new(1);
    format!("sa-acc-{}", N.fetch_add(1, Ordering::Relaxed))
}

#[cfg(any(feature = "audio", feature = "decode"))]
fn open(
    daw: &Standalone,
    project: ProjectContext,
    item: ItemRef,
    take: TakeRef,
) -> Option<OpenAccessor> {
    crate::take_reader::TakeReader::open_or_decode(daw, project, item, take).map(OpenAccessor::Take)
}

#[cfg(not(any(feature = "audio", feature = "decode")))]
fn open(
    _daw: &Standalone,
    _project: ProjectContext,
    _item: ItemRef,
    _take: TakeRef,
) -> Option<OpenAccessor> {
    // Without the decoder there is no way to read a source file, and
    // returning silence would be worse than declining: a caller would
    // analyse it and report a take with no notes.
    None
}

impl AudioAccessors for Standalone {
    /// The track's playing items mixed, before its FX ([`TrackMix`]): read
    /// in project time. `None` for a track with no audio to read, rather
    /// than a silent buffer, so a caller finds out here instead of
    /// concluding the track is empty.
    #[cfg(any(feature = "audio", feature = "decode"))]
    fn create_track_accessor(&self, project: ProjectContext, track: TrackRef) -> Option<String> {
        let open = OpenAccessor::Track(open_track(self, project, track)?);
        let id = next_id();
        table().lock().ok()?.insert(id.clone(), Arc::new(open));
        Some(id)
    }

    #[cfg(not(any(feature = "audio", feature = "decode")))]
    fn create_track_accessor(&self, _project: ProjectContext, _track: TrackRef) -> Option<String> {
        None
    }

    fn create_take_accessor(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> Option<String> {
        let open = open(self, project, item, take)?;
        let id = next_id();
        table().lock().ok()?.insert(id.clone(), Arc::new(open));
        Some(id)
    }

    /// Always false: standalone's sources are files on disk, and this
    /// exists for REAPER's case of an item being re-rendered underneath
    /// an open accessor.
    fn has_state_changed(&self, _accessor_id: &str) -> bool {
        false
    }

    // r[impl drums.open.accessor-placement]
    #[cfg(any(feature = "audio", feature = "decode"))]
    fn get_samples(&self, request: GetSamplesRequest) -> AudioSampleData {
        let Some(acc) = table()
            .lock()
            .ok()
            .and_then(|t| t.get(&request.accessor_id).cloned())
        else {
            return AudioSampleData::default();
        };
        let src_channels = acc.channels() as u32;
        let src_rate = acc.sample_rate() as f64;

        // A zero in either field means "whatever you have", which is
        // how a caller discovers the format before reading in earnest.
        let out_channels = if request.num_channels == 0 {
            src_channels
        } else {
            request.num_channels
        };
        let rate = if request.sample_rate <= 0.0 {
            src_rate
        } else {
            request.sample_rate
        };
        let want = request.num_samples as usize;
        if want == 0 || out_channels == 0 {
            return AudioSampleData {
                samples: Vec::new(),
                sample_rate: src_rate,
                num_channels: src_channels,
                num_samples: 0,
            };
        }

        // Each output frame is a take time; the reader maps it through
        // placement and markers to a source frame and interpolates. A
        // request at a foreign rate is therefore resampled linearly —
        // good enough for analysis; a caller that wants the exact
        // samples reads at the source rate the probe reported.
        let src_ch = src_channels.max(1) as usize;
        let mut samples = Vec::with_capacity(want * out_channels as usize);
        for i in 0..want {
            let t = request.start_time + i as f64 / rate;
            for ch in 0..out_channels as usize {
                // Fewer source channels than asked for: repeat the last
                // one, so a mono file read as stereo is centred rather
                // than half-silent.
                let sc = ch.min(src_ch - 1);
                samples.push(acc.sample(t, sc) as f64);
            }
        }

        AudioSampleData {
            samples,
            sample_rate: rate,
            num_channels: out_channels,
            num_samples: want as u32,
        }
    }

    #[cfg(not(any(feature = "audio", feature = "decode")))]
    fn get_samples(&self, _request: GetSamplesRequest) -> AudioSampleData {
        AudioSampleData::default()
    }

    fn destroy_accessor(&self, accessor_id: &str) {
        if let Ok(mut t) = table().lock() {
            t.remove(accessor_id);
        }
    }
}
