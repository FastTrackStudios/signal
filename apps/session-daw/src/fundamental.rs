//! A drum's fundamental, read off its own hits — offline, the whole song
//! at once.
//!
//! The golden template's one-note tracks (`Fund`, the kick's `Sub`) are
//! a parallel band-pass tuned to the drum's low fundamental (Dan
//! Worrall's parallel filters): a drum's fundamental hardly moves, so it
//! can be found once and the filter set to it. A person finds it on an
//! analyser while the drum plays; this finds it the same way, on every
//! hit at once:
//!
//! - **The hits** are the drum's trigger's ([`hits`]): a trigger hears its
//!   own drum and almost nothing else, so the moments the close mics are
//!   read at are the drum's — not the kick bleeding into a floor tom.
//! - **The spectrum** is the close mics' summed as the one-note track
//!   hears them — each at its fader and its polarity (a snare's bottom mic
//!   is flipped, and summed unflipped it cancels the very fundamental
//!   this looks for) — from just after each hit's attack over the ring
//!   that follows ([`WINDOW`]), each hit at its own strength: the band-pass
//!   hears the hard hits louder, and a window that is mostly another
//!   drum's bleed should not count as much as a strike.
//! - **The fundamental** is its strongest peak in the drum's range
//!   ([`Range`]), placed between bins by the peak's shape.

#![cfg(feature = "native")]

use daw::service::{ItemRef, Items as _, ProjectContext, TakeRef, TrackRef, Tracks};
use daw_proto::{AudioAccessors, GetSamplesRequest};

/// The rate the mics are read at.
const RATE: f64 = 48_000.0;

/// The spectrum's length, samples: 0.34 s of ring at 48 kHz, bins of
/// 2.9 Hz — fine enough to tell a 52 Hz kick from a 55.
const WINDOW: usize = 16_384;

/// The rate a trigger is read at to find its hits: a trigger's spike is
/// a few milliseconds wide.
const ONSET_RATE: f64 = 4_000.0;

/// How far under the trigger's loudest a spike still counts as a hit,
/// dB: the strong ones — they ring clearly.
const HIT_UNDER_DB: f64 = 12.0;

/// The least time between two hits, seconds: a spike's own wobble is
/// not a second hit, and a 16th-note triplet at 160 (62 ms) is.
const REFRACTORY: f64 = 0.04;

/// After the hit, the spectrum starts here, seconds: past the stick's
/// click, into the drum's ring.
const AFTER_ATTACK: f64 = 0.005;

/// The most hits read, spread over the song.
const MOST_HITS: usize = 48;

/// Where a drum's fundamental can be, Hz.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub low: f64,
    pub high: f64,
}

/// A kick's fundamental, sub to punch.
pub const KICK: Range = Range {
    low: 35.0,
    high: 110.0,
};
/// A snare's.
pub const SNARE: Range = Range {
    low: 120.0,
    high: 330.0,
};
/// A tom's, floor to rack.
pub const TOM: Range = Range {
    low: 55.0,
    high: 300.0,
};

/// The fundamental of the drum `trigger` triggers, heard on `sources`
/// (its close mics), Hz. `None` when the trigger has no hits, or the
/// mics nothing at them.
#[must_use]
pub fn find(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    trigger: &str,
    sources: &[String],
    range: Range,
) -> Option<f64> {
    let mut reader = Reader::new(daw, project);
    let all = hits(&mut reader, trigger);
    let step = all.len().div_ceil(MOST_HITS).max(1);
    let spectrum = average_spectrum(&mut reader, all.iter().step_by(step).copied(), sources);
    reader.close();
    peak(&spectrum?, range)
}

/// One hit: when, and how hard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Project seconds.
    pub time: f64,
    /// Its peak, dBFS.
    pub level_db: f64,
}

/// Every hit on `track` — a drum's trigger, ideally: one spike a hit —
/// down to `under_db` under the loudest, in project time.
#[must_use]
pub fn hits_of(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    track: &str,
    under_db: f64,
) -> Vec<Hit> {
    let mut reader = Reader::new(daw, project);
    let out = detect(&mut reader, track, under_db);
    reader.close();
    out
}

/// The rate a track is read at for drum editing's detectors: the
/// transient detector's fast envelope attacks in a millisecond, eight
/// samples of it.
pub const DETECT_RATE: f64 = 8_000.0;

/// A track's audio for the detectors: each playing item's start and its
/// samples at [`DETECT_RATE`], through its volume.
pub type DetectAudio = std::sync::Arc<Vec<(f64, Vec<f32>)>>;

/// What a track's audio was read from: each playing item's place and
/// gain. The same, and the audio read last is still the audio.
type Fingerprint = Vec<(String, u64, u64, u64, u64)>;

/// The audio last read for each track, by what it was read from.
fn audio_cache()
-> &'static std::sync::Mutex<std::collections::HashMap<String, (Fingerprint, DetectAudio)>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, (Fingerprint, DetectAudio)>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// `track`'s audio for the detectors — read once, then kept until its
/// items change, so detecting again with other settings is arithmetic
/// on what is already here: what makes tweaking them live.
#[must_use]
pub fn detect_audio(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    track: &str,
) -> DetectAudio {
    let mut reader = Reader::new(daw, project);
    let items = reader.edited(track);
    let print: Fingerprint = items
        .iter()
        .map(|i| {
            (
                i.guid.clone(),
                i.position.as_seconds().to_bits(),
                i.length.as_seconds().to_bits(),
                i.start_offset.as_seconds().to_bits(),
                i.volume.to_bits(),
            )
        })
        .collect();
    if let Ok(cache) = audio_cache().lock()
        && let Some((was, audio)) = cache.get(track)
        && *was == print
    {
        reader.close();
        return std::sync::Arc::clone(audio);
    }
    let mut out = Vec::with_capacity(items.len());
    for item in &items {
        let frames = (item.length.as_seconds() * DETECT_RATE).ceil();
        let Some(frames) = num_traits::ToPrimitive::to_u32(&frames) else {
            continue;
        };
        let samples: Vec<f32> = reader
            .read(item, 0.0, frames, DETECT_RATE)
            .iter()
            .map(|s| num_traits::ToPrimitive::to_f32(&(s * item.volume)).unwrap_or(0.0))
            .collect();
        out.push((item.position.as_seconds(), samples));
    }
    reader.close();
    let audio: DetectAudio = std::sync::Arc::new(out);
    if let Ok(mut cache) = audio_cache().lock() {
        cache.insert(track.to_owned(), (print, std::sync::Arc::clone(&audio)));
    }
    audio
}

/// `track`'s audio from `from` to `to` (project seconds) as `points`
/// peaks, from the items the song plays.
///
/// Each is the loudest sample in its slice, through the item's volume:
/// what a waveform shows, as numbers — for reading a hit, a ring, a
/// stray by eye.
#[must_use]
pub fn envelope(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    track: &str,
    (from, to): (f64, f64),
    points: usize,
) -> Vec<f64> {
    let mut reader = Reader::new(daw, project);
    let points = points.max(1);
    let span = (to - from).max(1e-6);
    let step = span / f64::from(u32::try_from(points).unwrap_or(u32::MAX));
    // Read finely enough to catch a hit's peak: 8 samples a point, at
    // least 4 kHz.
    let rate = (8.0 / step).max(ONSET_RATE);
    let mut out = vec![0.0_f64; points];
    for item in reader.playing(track) {
        let start = item.position.as_seconds();
        let end = start + item.length.as_seconds();
        let (a, b) = (from.max(start), to.min(end));
        if b <= a {
            continue;
        }
        let frames = ((b - a) * rate).ceil();
        let Some(frames) = num_traits::ToPrimitive::to_u32(&frames) else {
            continue;
        };
        let samples = reader.read(&item, a - start, frames, rate);
        for (k, v) in samples.iter().enumerate() {
            let time = a + f64::from(u32::try_from(k).unwrap_or(u32::MAX)) / rate;
            let at = ((time - from) / step).floor();
            if let Some(slot) = num_traits::ToPrimitive::to_usize(&at).and_then(|i| out.get_mut(i))
            {
                *slot = slot.max((v * item.volume).abs());
            }
        }
    }
    reader.close();
    out
}

/// The trigger's strong hits, project seconds.
fn hits(reader: &mut Reader<'_>, trigger: &str) -> Vec<f64> {
    detect(reader, trigger, HIT_UNDER_DB)
        .iter()
        .map(|h| h.time)
        .collect()
}

/// The hits on `track` down to `under_db` under its loudest: a spike over
/// the threshold [`REFRACTORY`] after the last, at its peak within that.
fn detect(reader: &mut Reader<'_>, track: &str, under_db: f64) -> Vec<Hit> {
    // Each item's envelope, at the onset rate, then one threshold over
    // the lot: a hit is a hit wherever in the song it is.
    let mut items: Vec<(f64, Vec<f64>)> = Vec::new();
    for item in reader.playing(track) {
        let frames = (item.length.as_seconds() * ONSET_RATE).ceil();
        let Some(frames) = num_traits::ToPrimitive::to_u32(&frames) else {
            continue;
        };
        let samples = reader.read(&item, 0.0, frames, ONSET_RATE);
        items.push((
            item.position.as_seconds(),
            samples.iter().map(|s| (s * item.volume).abs()).collect(),
        ));
    }
    let loudest = items
        .iter()
        .flat_map(|(_, e)| e.iter().copied())
        .fold(0.0_f64, f64::max);
    if loudest <= 0.0 {
        return Vec::new();
    }
    let threshold = loudest * 10_f64.powf(-under_db / 20.0);
    let mut out: Vec<Hit> = Vec::new();
    for (start, envelope) in items {
        let mut last = f64::NEG_INFINITY;
        // The hit still taking its peak, by its place in `out`.
        let mut open: Option<usize> = None;
        let mut peak = 0.0_f64;
        let mut at = 0.0;
        for value in envelope {
            let time = start + at;
            at += 1.0 / ONSET_RATE;
            if time - last < REFRACTORY {
                peak = peak.max(value);
                continue;
            }
            if let Some(hit) = open.take().and_then(|i| out.get_mut(i)) {
                hit.level_db = 20.0 * peak.max(1e-12).log10();
            }
            if value >= threshold {
                open = Some(out.len());
                out.push(Hit {
                    time,
                    level_db: 0.0,
                });
                last = time;
                peak = value;
            }
        }
        if let Some(hit) = open.and_then(|i| out.get_mut(i)) {
            hit.level_db = 20.0 * peak.max(1e-12).log10();
        }
    }
    out
}

/// The mics' magnitude spectrum after each hit, summed. `None` when no
/// hit had mic audio under it.
fn average_spectrum(
    reader: &mut Reader<'_>,
    hits: impl Iterator<Item = f64>,
    sources: &[String],
) -> Option<Vec<f64>> {
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(WINDOW);
    let mut spectrum = fft.make_output_vec();
    let mut total = vec![0.0; spectrum.len()];
    let hann: Vec<f64> = (0..WINDOW)
        .map(|i| {
            let phase = std::f64::consts::TAU * f64::from(u32::try_from(i).unwrap_or(0))
                / f64::from(u32::try_from(WINDOW).unwrap_or(1));
            0.5 - 0.5 * phase.cos()
        })
        .collect();
    let seconds = f64::from(u32::try_from(WINDOW).unwrap_or(0)) / RATE;
    let mut counted = 0_u32;
    for hit in hits {
        let from = hit + AFTER_ATTACK;
        let mut sum = vec![0.0; WINDOW];
        let mut heard = false;
        for source in sources {
            let gain = reader.gain(source);
            let Some(item) = reader.playing(source).into_iter().find(|i| {
                let start = i.position.as_seconds();
                start <= from && start + i.length.as_seconds() >= from + seconds
            }) else {
                continue;
            };
            let take_at = from - item.position.as_seconds();
            let samples = reader.read(&item, take_at, u32::try_from(WINDOW).unwrap_or(0), RATE);
            for (s, v) in sum.iter_mut().zip(&samples) {
                *s += v * item.volume * gain;
            }
            heard = true;
        }
        if !heard {
            continue;
        }
        // Its offset off, so the window's own step is no low end.
        let mean = sum.iter().sum::<f64>() / f64::from(u32::try_from(WINDOW).unwrap_or(1));
        let mut input: Vec<f64> = sum.iter().zip(&hann).map(|(s, w)| (s - mean) * w).collect();
        if fft.process(&mut input, &mut spectrum).is_err() {
            continue;
        }
        for (t, c) in total.iter_mut().zip(&spectrum) {
            *t += c.norm();
        }
        counted = counted.saturating_add(1);
    }
    (counted > 0).then_some(total)
}

/// The strongest peak of `spectrum` (bins of `RATE / WINDOW` Hz) in
/// `range` — a bin over both its neighbours — between bins by a parabola
/// through its neighbours' logs.
#[must_use]
pub fn peak(spectrum: &[f64], range: Range) -> Option<f64> {
    let bin = RATE / f64::from(u32::try_from(WINDOW).unwrap_or(1));
    let low = num_traits::ToPrimitive::to_usize(&(range.low / bin).ceil())?.max(1);
    let high = num_traits::ToPrimitive::to_usize(&(range.high / bin).floor())?;
    // A peak: over both its neighbours. A slope running out of the range
    // — rumble rising to its low edge — has none there, and is no note.
    let (at, _) = spectrum
        .iter()
        .enumerate()
        .take(high.saturating_add(1))
        .skip(low)
        .filter(|(i, m)| {
            spectrum.get(i.saturating_sub(1)).is_some_and(|l| *m > l)
                && spectrum.get(i.saturating_add(1)).is_some_and(|r| *m > r)
        })
        .max_by(|a, b| a.1.total_cmp(b.1))?;
    let log = |i: usize| spectrum.get(i).map_or(0.0, |m| m.max(1e-12).ln());
    let (left, centre, right) = (
        log(at.saturating_sub(1)),
        log(at),
        log(at.saturating_add(1)),
    );
    let curve = left - 2.0 * centre + right;
    let offset = if curve.abs() > 1e-12 {
        (0.5 * (left - right) / curve).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    Some((f64::from(u32::try_from(at).ok()?) + offset) * bin)
}

/// The project's items and their takes' audio, an accessor opened per
/// item read and closed together.
struct Reader<'a> {
    daw: &'a daw_standalone::sync::Standalone,
    project: ProjectContext,
    accessors: std::collections::HashMap<String, Option<String>>,
}

impl<'a> Reader<'a> {
    fn new(daw: &'a daw_standalone::sync::Standalone, project: &ProjectContext) -> Self {
        Self {
            daw,
            project: project.clone(),
            accessors: std::collections::HashMap::new(),
        }
    }

    /// The items on `track` the song plays: its playing lane's.
    /// The takes an edit of `track` works on ([`crate::drum_edit::is_edited`]):
    /// what the detectors read, so a hit is found where it is being moved.
    fn edited(&self, track: &str) -> Vec<daw_proto::Item> {
        let Some(t) = Tracks::get(
            self.daw,
            self.project.clone(),
            TrackRef::Guid(track.to_owned()),
        ) else {
            return Vec::new();
        };
        self.daw
            .get_items(self.project.clone(), TrackRef::Guid(track.to_owned()))
            .into_iter()
            .filter(|i| !i.muted && crate::drum_edit::is_edited(&t, i))
            .collect()
    }

    fn playing(&self, track: &str) -> Vec<daw_proto::Item> {
        let Some(t) = Tracks::get(
            self.daw,
            self.project.clone(),
            TrackRef::Guid(track.to_owned()),
        ) else {
            return Vec::new();
        };
        self.daw
            .get_items(self.project.clone(), TrackRef::Guid(track.to_owned()))
            .into_iter()
            .filter(|i| !i.muted && daw_ui::studio::project::on_playing_lane(&t, i))
            .collect()
    }

    /// What `track`'s fader and polarity make of its audio: what it sends
    /// post-fader.
    fn gain(&self, track: &str) -> f64 {
        Tracks::get(
            self.daw,
            self.project.clone(),
            TrackRef::Guid(track.to_owned()),
        )
        .map_or(1.0, |t| {
            if t.phase_inverted {
                -t.volume
            } else {
                t.volume
            }
        })
    }

    /// `frames` mono frames of `item`'s active take from take time `at`,
    /// at `rate`.
    fn read(&mut self, item: &daw_proto::Item, at: f64, frames: u32, rate: f64) -> Vec<f64> {
        let daw = self.daw;
        let project = self.project.clone();
        let accessor = self
            .accessors
            .entry(item.guid.clone())
            .or_insert_with(|| {
                daw.create_take_accessor(project, ItemRef::Guid(item.guid.clone()), TakeRef::Active)
            })
            .clone();
        let Some(accessor_id) = accessor else {
            return Vec::new();
        };
        self.daw
            .get_samples(GetSamplesRequest {
                accessor_id,
                sample_rate: rate,
                num_channels: 1,
                start_time: at,
                num_samples: frames,
            })
            .samples
    }

    fn close(self) {
        for id in self.accessors.into_values().flatten() {
            self.daw.destroy_accessor(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RATE, Range, WINDOW, peak};

    /// A spectrum with a peak between two bins is read between them; one
    /// outside the range is not read at all.
    #[test]
    fn the_peak_is_read_between_bins() {
        let bin = RATE / f64::from(u32::try_from(WINDOW).unwrap_or(1));
        let mut spectrum = vec![0.01; WINDOW / 2 + 1];
        // 61.5 Hz: bins 20 (58.6) and 21 (61.5) and 22 (64.5).
        spectrum[20] = 0.5;
        spectrum[21] = 1.0;
        spectrum[22] = 0.5;
        let hz = peak(
            &spectrum,
            Range {
                low: 35.0,
                high: 110.0,
            },
        )
        .expect("a peak");
        assert!((hz - 21.0 * bin).abs() < 0.1, "{hz}");
        spectrum[22] = 0.9;
        let leaning = peak(
            &spectrum,
            Range {
                low: 35.0,
                high: 110.0,
            },
        )
        .expect("a peak");
        assert!(leaning > 21.0 * bin && leaning < 22.0 * bin, "{leaning}");
        assert!(
            peak(
                &spectrum,
                Range {
                    low: 120.0,
                    high: 330.0
                }
            )
            .is_none(),
            "flat: no peak"
        );
        // Rumble rising to the range's low edge is a slope, not a note:
        // the real peak above it is read.
        let mut rumble = vec![0.01; WINDOW / 2 + 1];
        for (i, m) in rumble.iter_mut().enumerate().take(14) {
            *m = f64::from(u32::try_from(i).unwrap_or(0)).mul_add(-0.2, 3.0);
        }
        rumble[18] = 0.6;
        rumble[19] = 0.8;
        rumble[20] = 0.6;
        let range = Range {
            low: 35.0,
            high: 110.0,
        };
        let hz = peak(&rumble, range).expect("the note");
        assert!((hz - 19.0 * bin).abs() < 0.1, "{hz}");
    }
}
