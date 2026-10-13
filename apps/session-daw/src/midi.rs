//! The notes inside an item, for drawing it.
//!
//! An item's body should be what the item CONTAINS. Audio has peaks and
//! MIDI has notes, and until now every item in the arrangement was
//! drawn from the same synthetic envelope — a function of the track's
//! index and the time, which never touched the item, the take, or a
//! single sample. That is why a chord track looked like a shaker.
//!
//! Notes arrive the way peaks were always meant to: after the window is
//! already standing. Reading them is a round trip per item, and a
//! session with four hundred MIDI items would spend the whole open
//! doing it — so the window draws what it has, asks for the rest, and
//! redraws as the answers land.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// One note, reduced to what a preview draws.
///
/// Not `MidiNote`: a preview needs where and how high, and carries
/// neither channel nor selection nor mute. Reduced at the edge so the
/// drawing has nothing to decide and the cache stays small — a big
/// session holds a lot of these.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Note {
    /// Where it starts, as a fraction of the item's length.
    ///
    /// A fraction rather than a time, because an item preview is drawn
    /// to the item's width and that is the only thing it is measured
    /// against. It also makes the cache independent of the zoom, so a
    /// scroll never invalidates one.
    pub at: f32,
    /// How long, as the same fraction. Never zero: a note with no
    /// length is a note you cannot see.
    pub len: f32,
    /// MIDI pitch, 0..=127.
    pub pitch: u8,
    /// How hard, 0..=127 — the preview draws it as brightness.
    pub velocity: u8,
}

/// The notes of the items that have been read so far.
///
/// Absent means "not asked yet or still reading", and an empty list
/// means "read, and it has none". The two have to be distinguishable:
/// an item with no notes should stop being asked about, and an item
/// still loading should not be drawn as empty.
/// An audio item's waveform, reduced to what a lane draws: the loudest
/// and the lowest the take reaches across each stretch, every channel
/// folded into one, from the item's own start.
///
/// The take's peaks, not the source's — the engine has already applied
/// the item's slip, play rate and stretch, so point `i` is item time
/// `i * step` and nothing here has to know how it got there.
#[derive(Clone, Debug, PartialEq)]
pub struct Wave {
    /// Seconds each point stands for.
    pub step: f64,
    /// `(max, min)` per point, in −1..1.
    pub points: Vec<(f32, f32)>,
}

/// Audio items whose waveform no longer draws what they play — their
/// take's volume curve was rewritten — to be read again.
static STALE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Say `items`' waveforms are stale: the window reads them again.
pub fn mark_stale(items: impl IntoIterator<Item = String>) {
    if let Ok(mut stale) = STALE.lock() {
        stale.extend(items);
    }
}

/// The items said stale since last asked.
#[must_use]
pub fn take_stale() -> Vec<String> {
    STALE
        .lock()
        .map(|mut s| std::mem::take(&mut *s))
        .unwrap_or_default()
}

/// The volume envelope of `item`'s active take, `(seconds into the item,
/// linear gain)` by time: none when it has none.
#[cfg(feature = "native")]
fn take_volume(
    daw: &daw::standalone::Standalone,
    project: &daw::service::ProjectContext,
    item: &str,
) -> Vec<(f64, f64)> {
    use daw::service::{ItemRef, Takes as _};
    use daw_proto::automation::Automation as _;
    let Some(take) = daw.get_active_take(project.clone(), ItemRef::Guid(item.to_owned())) else {
        return Vec::new();
    };
    let location = daw_proto::EnvelopeLocation {
        track: daw_proto::TrackRef::Index(0),
        envelope: daw_proto::EnvelopeRef::Take {
            item_guid: item.to_owned(),
            take_guid: take.guid,
            kind: daw_proto::TakeEnvelopeKind::Volume,
        },
    };
    daw.points(project.clone(), location)
        .into_iter()
        .map(|p| (p.time.as_seconds(), p.value))
        .collect()
}

impl Wave {
    /// This one through a gain curve — `(seconds into it, linear gain)`,
    /// by time, straight between points, the first's before it and the
    /// last's after — as it plays through its take's volume envelope.
    #[must_use]
    pub fn through(mut self, curve: &[(f64, f64)]) -> Self {
        if curve.is_empty() {
            return self;
        }
        let gain_at = |t: f64| {
            let at = curve.partition_point(|(x, _)| *x <= t);
            match (at.checked_sub(1).and_then(|i| curve.get(i)), curve.get(at)) {
                (Some(&(x0, g0)), Some(&(x1, g1))) if x1 > x0 => {
                    ((t - x0) / (x1 - x0)).mul_add(g1 - g0, g0)
                }
                (Some(&(_, g)), _) | (None, Some(&(_, g))) => g,
                (None, None) => 1.0,
            }
        };
        let step = self.step;
        for (k, point) in self.points.iter_mut().enumerate() {
            let t = (crate::num::coord(k) + 0.5) * step;
            let g = num_traits::ToPrimitive::to_f32(&gain_at(t)).unwrap_or(1.0);
            *point = (point.0 * g, point.1 * g);
        }
        self
    }

    /// The part of this one from `from` seconds into it, `length` long —
    /// silence where that reaches outside it. A slice of a take plays the
    /// take's own audio, so its waveform is this, cut where it was.
    #[must_use]
    pub fn slice(&self, from: f64, length: f64) -> Self {
        let step = self.step.max(f64::EPSILON);
        let count = crate::num::index((length / step).ceil());
        let skip = (from / step).round();
        let points = (0..count)
            .map(|k| {
                let at = crate::num::coord(k) + skip;
                if at < 0.0 {
                    (0.0, 0.0)
                } else {
                    self.points
                        .get(crate::num::index(at))
                        .copied()
                        .unwrap_or((0.0, 0.0))
                }
            })
            .collect();
        Self {
            step: self.step,
            points,
        }
    }

    /// A slice `length` long played through stretch `markers` (`(take
    /// time, source time)`, as the engine takes them): each point from
    /// the source time its take time plays, so a decay drawn out over a
    /// gap is drawn drawn out. `origin` is the source time this wave's
    /// first point stands for. Silence outside it, as [`Self::slice`].
    #[must_use]
    pub fn warped(&self, origin: f64, length: f64, markers: &[(f64, f64)]) -> Self {
        let step = self.step.max(f64::EPSILON);
        let count = crate::num::index((length / step).ceil());
        // The source time a take time plays: along the segment holding it
        // (before the first, the first's; past the last, the last's).
        let source = |take: f64| -> f64 {
            let segment = markers
                .windows(2)
                .find(|w| w.get(1).is_some_and(|end| take <= end.0))
                .or_else(|| markers.windows(2).last());
            match segment {
                Some(&[(t0, s0), (t1, s1)]) if t1 > t0 => {
                    (take - t0).mul_add((s1 - s0) / (t1 - t0), s0)
                }
                _ => markers.first().map_or(take, |&(t0, s0)| s0 + (take - t0)),
            }
        };
        let points = (0..count)
            .map(|k| {
                let at = ((source(crate::num::coord(k) * step) - origin) / step).round();
                if at < 0.0 {
                    (0.0, 0.0)
                } else {
                    self.points
                        .get(crate::num::index(at))
                        .copied()
                        .unwrap_or((0.0, 0.0))
                }
            })
            .collect();
        Self {
            step: self.step,
            points,
        }
    }

    /// Fold the engine's per-channel `[min, max, …]` blocks into one
    /// envelope. `None` for a take with no audio to show.
    #[must_use]
    pub fn from_peaks(data: &daw_proto::TakePeakData) -> Option<Self> {
        let channels = usize::try_from(data.num_channels.max(1)).ok()?;
        let stride = channels * 2;
        if data.peaks.len() < stride || data.sample_rate <= 0.0 {
            return None;
        }
        let points = data
            .peaks
            .chunks_exact(stride)
            .map(|block| {
                let (mut max, mut min) = (0.0_f64, 0.0_f64);
                for pair in block.chunks_exact(2) {
                    min = min.min(pair[0]);
                    max = max.max(pair[1]);
                }
                #[expect(clippy::cast_possible_truncation, reason = "a level in −1..1")]
                (max as f32, min as f32)
            })
            .collect();
        Some(Self {
            step: f64::from(data.samples_per_peak.max(1)) / data.sample_rate,
            points,
        })
    }
}

/// One child item under a folded item (a shut folder's row): where it
/// sits, and what it holds — see [`fold_waves`] and [`fold_notes`].
pub struct Child<'a> {
    /// Its position and length, in seconds.
    pub at: f64,
    pub length: f64,
    /// Its waveform, if it is audio and its peaks are read.
    pub wave: Option<&'a Wave>,
    /// Its notes, if it is MIDI and they are read.
    pub notes: Option<&'a [Note]>,
}

/// The most points a folded waveform holds, however long its span.
const FOLDED_POINTS: usize = 1 << 16;

/// A shut folder's item, `start..end` seconds, as the envelope of the
/// audio under it: per point, the loudest of every child sounding there.
///
/// At the finest of the children's resolutions, so folding loses nothing
/// a child showed open. `None` where no child under it has a waveform.
#[must_use]
pub fn fold_waves(start: f64, end: f64, children: &[Child<'_>]) -> Option<Wave> {
    let waves: Vec<(f64, &Wave)> = children
        .iter()
        .filter_map(|c| {
            c.wave
                .filter(|w| w.step > 0.0 && !w.points.is_empty())
                .map(|w| (c.at, w))
        })
        .collect();
    let finest = waves
        .iter()
        .map(|(_, w)| w.step)
        .fold(f64::INFINITY, f64::min);
    let span = end - start;
    if !finest.is_finite() || span <= 0.0 {
        return None;
    }
    let count = crate::num::index((span / finest).ceil()).clamp(1, FOLDED_POINTS);
    let step = span / crate::num::coord(count);
    // Each child's points once, into every folded point they overlap: a
    // slip-edited kit is tens of thousands of slices under one folder
    // item, and asking each folded point of every child was billions of
    // steps — ten seconds of a frozen window.
    let mut points = vec![(0.0_f32, 0.0_f32); count];
    let last = count.saturating_sub(1);
    for (at, wave) in &waves {
        for (j, (hi, lo)) in wave.points.iter().enumerate() {
            let begins = crate::num::coord(j).mul_add(wave.step, *at);
            let ends = begins + wave.step;
            if ends <= start || begins >= end {
                continue;
            }
            // A point that only meets a folded point's edge is not in it.
            let first = crate::num::index(((begins - start) / step + EDGE).floor()).min(last);
            let past = crate::num::index(((ends - start) / step - EDGE).ceil())
                .clamp(first.saturating_add(1), count);
            for point in points.iter_mut().take(past).skip(first) {
                *point = (point.0.max(*hi), point.1.min(*lo));
            }
        }
    }
    Some(Wave { step, points })
}

/// How near a folded point's edge, in folded points, a child's point that
/// meets it is taken to only meet it.
const EDGE: f64 = 1e-9;

/// A shut folder's item, `start..end` seconds, as the notes under it: each
/// child's notes placed where they sound and cut to the span, as fractions
/// of it. `None` where no child under it has notes there.
#[must_use]
pub fn fold_notes(start: f64, end: f64, children: &[Child<'_>]) -> Option<Vec<Note>> {
    let span = end - start;
    if span <= 0.0 {
        return None;
    }
    let mut out = Vec::new();
    for child in children {
        for note in child.notes.unwrap_or_default() {
            let from = f64::from(note.at).mul_add(child.length, child.at);
            let to = f64::from(note.len).mul_add(child.length, from);
            if to <= start || from >= end {
                continue;
            }
            let (from, to) = (from.max(start), to.min(end));
            out.push(Note {
                at: crate::num::narrow((from - start) / span),
                // Never zero: a note with no length cannot be seen.
                len: crate::num::narrow((to - from) / span).max(f32::MIN_POSITIVE),
                pitch: note.pitch,
                velocity: note.velocity,
            });
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Samples per point a lane asks the engine for: 5 ms at 48 kHz.
///
/// About two hundred points a second: a drum hit's attack and decay as
/// their own shape at the zooms a kit is edited at, drawn from what is
/// already here rather than read again item by item ([`crate::wave_detail`]
/// takes over only closer than that). At forty a second a hit was a few
/// steps of a staircase.
pub const WAVE_BLOCK: u32 = 256;

/// Samples per point the analyses read (gain staging, the mix chains'
/// levels): what their thresholds were measured at, and four times fewer
/// points to go through than the drawing's.
pub const ANALYSIS_BLOCK: u32 = 1024;

/// How many items' peaks are read at once.
const WAVE_READS: usize = 16;

#[derive(Clone, Default)]
pub struct Previews {
    known: Arc<Mutex<HashMap<String, Vec<Note>>>>,
    /// The audio items' waveforms, by item GUID.
    waves: Arc<Mutex<HashMap<String, Arc<Wave>>>>,
    /// Bumped whenever notes or waveforms arrive — so a picture recorded
    /// before they did (a browser's waveforms come after the song opens)
    /// knows to record itself again.
    generation: Arc<std::sync::atomic::AtomicU64>,
}

impl Previews {
    /// The notes for an item, if they have been read.
    #[must_use]
    pub fn get(&self, guid: &str) -> Option<Vec<Note>> {
        self.known.lock().ok()?.get(guid).cloned()
    }

    /// How many times notes or waveforms have arrived: a holder of a
    /// picture drawn from them records it again when this has moved.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(std::sync::atomic::Ordering::Acquire)
    }

    fn arrived(&self) {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }

    /// Put `wave` in as `guid`'s, as a read would.
    #[cfg(test)]
    pub fn insert_wave(&self, guid: &str, wave: Wave) {
        if let Ok(mut waves) = self.waves.lock() {
            waves.insert(guid.to_owned(), Arc::new(wave));
        }
    }

    /// Put `wave` in as `guid`'s without reading it: a slice of a take
    /// whose waveform is already here (see [`Wave::slice`]).
    pub fn put_waves(&self, waves: impl IntoIterator<Item = (String, Wave)>) {
        if let Ok(mut held) = self.waves.lock() {
            for (guid, wave) in waves {
                held.insert(guid, Arc::new(wave));
            }
        }
        self.arrived();
    }

    /// The waveform for an audio item, if its peaks have been read.
    #[must_use]
    pub fn wave(&self, guid: &str) -> Option<Arc<Wave>> {
        self.waves.lock().ok()?.get(guid).cloned()
    }

    /// Read the peaks of every audio item named, now, on this thread —
    /// [`Self::fill_blocking`]'s twin, for the same reason: a window that
    /// opens on blank lanes and fills them in reads as a broken session.
    ///
    /// With the engine in this process, every item at once across the
    /// waveform pool ([`crate::wave_pool`]); otherwise over the engine's
    /// link, several at a time.
    pub fn fill_waves_blocking(&self, guids: Vec<String>) {
        #[cfg(feature = "native")]
        if self.fill_waves_local(&guids) {
            return;
        }
        let Some(runtime) = crate::open::runtime() else {
            return;
        };
        runtime.block_on(self.fill_waves(guids));
    }

    /// Read every item's peaks from the engine in this process, in
    /// parallel on the waveform pool; `false` with no engine here.
    #[cfg(feature = "native")]
    fn fill_waves_local(&self, guids: &[String]) -> bool {
        use daw::service::{ItemRef, ProjectContext, TakeRef};
        use daw_proto::Peaks as _;
        let (Some(daw), Some(song)) = (crate::open::local_engine(), crate::open::current_song())
        else {
            return false;
        };
        let project = ProjectContext::Project(song);
        let job = crate::jobs::start("Waveforms", guids.len());
        let read = crate::wave_pool::map(guids, |guid| {
            job.advance(1);
            let peaks = daw.take_peaks(
                project.clone(),
                ItemRef::Guid(guid.clone()),
                TakeRef::Active,
                WAVE_BLOCK,
            );
            // Drawn as it plays: through its take's volume envelope (a
            // leveled drum's strokes even, as they sound).
            let curve = take_volume(daw, &project, guid);
            Wave::from_peaks(&peaks).map(|wave| (guid.clone(), wave.through(&curve)))
        });
        if let Ok(mut waves) = self.waves.lock() {
            for (guid, wave) in read.into_iter().flatten() {
                waves.insert(guid, Arc::new(wave));
            }
        }
        self.arrived();
        true
    }

    /// Read the peaks, awaited — the web build's way in.
    pub async fn fill_waves(&self, guids: Vec<String>) {
        let Some(daw) = daw::rpc::Daw::try_get() else {
            return;
        };
        let Ok(project) = daw.current_project().await else {
            return;
        };
        self.fill_waves_in(&project, guids).await;
    }

    /// [`Self::fill_waves`] for `project`, whichever is current.
    pub async fn fill_waves_in(&self, project: &daw_control::Project, guids: Vec<String>) {
        use futures_util::StreamExt as _;
        // Several at once: each is a decode or a peaks-file read the engine
        // answers on its own, and a song of nine hundred items read one at
        // a time held its opening for half a second.
        let mut reads = futures_util::stream::iter(guids)
            .map(|guid| async move { (read_wave(project, &guid).await, guid) })
            .buffer_unordered(WAVE_READS);
        while let Some((wave, guid)) = reads.next().await {
            let Some(wave) = wave else {
                continue;
            };
            if let Ok(mut waves) = self.waves.lock() {
                waves.insert(guid, Arc::new(wave));
            }
        }
        self.arrived();
    }

    /// Read the notes now, on this thread.
    ///
    /// For a renderer rather than a window: something drawing one frame
    /// and exiting has no later to redraw in, so an empty cache would
    /// mean every MIDI item drawn blank — a worse picture than the one
    /// this replaced.
    pub fn fill_blocking(&self, wanted: Vec<(String, f64)>) {
        let Some(runtime) = crate::open::runtime() else {
            return;
        };
        runtime.block_on(self.fill(wanted));
    }

    /// Read the notes, awaited: [`Self::fill_blocking`] where there is no
    /// thread to block (the web build).
    pub async fn fill(&self, wanted: Vec<(String, f64)>) {
        let Some(daw) = daw::rpc::Daw::try_get() else {
            return;
        };
        let Ok(project) = daw.current_project().await else {
            return;
        };
        self.fill_in(&project, wanted).await;
    }

    /// [`Self::fill`] from a given project rather than the current one —
    /// a song a page opens in the background.
    pub async fn fill_in(&self, project: &daw_control::Project, wanted: Vec<(String, f64)>) {
        for (guid, length) in wanted {
            let notes = read(project, &guid, length).await;
            if let Ok(mut known) = self.known.lock() {
                known.insert(guid, notes);
            }
        }
        self.arrived();
    }
}

/// Read one audio item's waveform from its active take's peaks. `None`
/// for an item the engine has no audio for — drawn plain, not faked.
async fn read_wave(project: &daw_control::Project, guid: &str) -> Option<Wave> {
    let item = project.items().by_guid(guid).await.ok()??;
    let data = item.active_take().peaks(WAVE_BLOCK).await.ok()?;
    Wave::from_peaks(&data)
}

/// Read one item's notes, as fractions of its length.
///
/// An empty answer is a real answer — an empty MIDI item is a thing
/// that exists, and drawing it as blank is right.
async fn read(project: &daw_control::Project, guid: &str, length: f64) -> Vec<Note> {
    let Ok(Some(item)) = project.items().by_guid(guid).await else {
        return Vec::new();
    };
    let Ok(notes) = item.active_take().midi().notes().await else {
        return Vec::new();
    };
    // The span the notes actually cover, because a take's PPQ tells us
    // ticks per quarter note and not how many quarter notes the ITEM
    // is. Measuring the notes themselves means the preview fills the
    // block whatever the tempo, which is what it is for: a shape, not a
    // clock.
    let span = notes
        .iter()
        .map(|n| n.start_ppq + n.length_ppq)
        .fold(0.0_f64, f64::max);
    let span = if span > 0.0 { span } else { length.max(1.0) };
    notes
        .iter()
        .map(|note| Note {
            at: (note.start_ppq / span) as f32,
            len: (note.length_ppq / span) as f32,
            pitch: note.pitch,
            velocity: note.velocity,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Note, Previews, Wave};

    /// A wave through its take's volume curve is drawn as it plays: up
    /// where the curve is up, down where it is down, as it was before.
    #[test]
    fn a_wave_is_drawn_through_its_volume_curve() {
        let wave = Wave {
            step: 0.5,
            points: vec![(0.5, -0.5); 4],
        };
        let curve = [(0.9, 1.0), (1.0, 2.0), (1.9, 2.0), (2.0, 0.5)];
        let drawn = wave.clone().through(&curve);
        assert!(
            (drawn.points[0].0 - 0.5).abs() < 1e-6,
            "before the curve: as the first point"
        );
        assert!(
            (drawn.points[2].0 - 1.0).abs() < 1e-6,
            "doubled: {:?}",
            drawn.points
        );
        assert!((drawn.points[2].1 + 1.0).abs() < 1e-6);
        assert_eq!(wave.clone().through(&[]), wave, "no curve, no change");
    }

    /// Absent and empty mean different things.
    ///
    /// An item still being read must not draw as an empty one: a chord
    /// track that flashed blank on every open and then filled in would
    /// read as a bug in the session, not as a load.
    #[test]
    fn nothing_read_yet_is_not_the_same_as_nothing_there() {
        let previews = Previews::default();
        assert_eq!(previews.get("unread"), None);
        previews
            .known
            .lock()
            .unwrap()
            .insert("empty".into(), Vec::new());
        assert_eq!(previews.get("empty"), Some(Vec::new()));
    }

    /// The shape a preview is drawn from: fractions, so the cache
    /// survives a zoom.
    #[test]
    fn a_note_is_a_fraction_of_its_item() {
        let note = Note {
            at: 0.25,
            len: 0.5,
            pitch: 60,
            velocity: 100,
        };
        assert!(note.at + note.len <= 1.0, "a note ran past its item");
    }
}

#[cfg(test)]
mod wave_tests {
    use super::Wave;

    /// Channels fold into one envelope — the loudest of either, the lowest
    /// of either — and a point stands for its block's worth of seconds.
    #[test]
    fn channels_fold_and_the_step_is_the_block() {
        let data = daw_proto::TakePeakData {
            sample_rate: 48_000.0,
            num_channels: 2,
            // [ch0 min, ch0 max, ch1 min, ch1 max] per block
            peaks: vec![-0.2, 0.5, -0.7, 0.1, -0.1, 0.0, -0.3, 0.9],
            samples_per_peak: 480,
        };
        let wave = Wave::from_peaks(&data).expect("a wave");
        assert!((wave.step - 0.01).abs() < 1e-12);
        assert_eq!(wave.points, vec![(0.5, -0.7), (0.9, -0.3)]);
    }

    /// A take with no audio has no wave, not a flat one.
    #[test]
    fn no_peaks_is_no_wave() {
        let data = daw_proto::TakePeakData {
            peaks: Vec::new(),
            ..daw_proto::TakePeakData::default()
        };
        assert_eq!(Wave::from_peaks(&data), None);
    }
}

#[cfg(test)]
mod fold_tests {

    /// A slice drawn out by stretch markers is drawn drawn out: its kept
    /// start point for point, its decay spread to the new length.
    #[test]
    fn a_stretched_slice_draws_its_decay_drawn_out() {
        let points: Vec<(f32, f32)> = (0..100_u8).map(|k| (f32::from(k) / 100.0, 0.0)).collect();
        let wave = Wave { step: 0.01, points };
        // The source from 0.2 s: kept to 0.03 s, then 0.47 s of source over
        // 0.97 s — half speed.
        let markers = [(0.0, 0.2), (0.03, 0.23), (1.0, 0.7)];
        let drawn = wave.warped(0.0, 1.0, &markers);
        assert_eq!(drawn.points.len(), 100);
        assert!(
            (drawn.points[0].0 - 0.20).abs() < 1e-6,
            "starts where the take does"
        );
        assert!(
            (drawn.points[3].0 - 0.23).abs() < 1e-6,
            "the attack as it was"
        );
        assert!(
            (drawn.points[99].0 - 0.70).abs() < 0.01,
            "ends where the source does"
        );
        let unstretched = wave.slice(0.2, 1.0);
        assert!(
            drawn.points[60].0 < unstretched.points[60].0,
            "the decay slowed"
        );
    }

    use super::{Child, Note, Wave, fold_notes, fold_waves};

    /// A slice is its take's waveform from where it was cut, and silence
    /// where it reaches back before the take began.
    #[test]
    fn a_slice_is_cut_from_its_take() {
        let take = Wave {
            step: 0.1,
            points: (0_u8..10).map(|k| (f32::from(k), -f32::from(k))).collect(),
        };
        let mid = take.slice(0.3, 0.3);
        assert_eq!(mid.points, [(3.0, -3.0), (4.0, -4.0), (5.0, -5.0)]);
        let early = take.slice(-0.2, 0.3);
        assert_eq!(early.points, [(0.0, 0.0), (0.0, 0.0), (0.0, -0.0)]);
        let late = take.slice(0.9, 0.3);
        assert_eq!(late.points, [(9.0, -9.0), (0.0, 0.0), (0.0, 0.0)]);
    }

    fn flat(level: f32, points: usize) -> Wave {
        Wave {
            step: 0.1,
            points: vec![(level, -level); points],
        }
    }

    fn child<'a>(
        at: f64,
        length: f64,
        wave: Option<&'a Wave>,
        notes: Option<&'a [Note]>,
    ) -> Child<'a> {
        Child {
            at,
            length,
            wave,
            notes,
        }
    }

    /// A shut folder's item is the loudest of the children sounding at
    /// each moment, placed where each child sits — silent where none is.
    #[test]
    fn folding_is_each_childs_loudest_where_its_points_overlap() {
        // The definition, point by point: every child point that overlaps
        // a folded point's span counts.
        let naive = |start: f64, end: f64, kids: &[(f64, Wave)], count: usize| {
            let step = (end - start) / crate::num::coord(count);
            (0..count)
                .map(|k| {
                    let from = crate::num::coord(k).mul_add(step, start);
                    let to = from + step;
                    kids.iter().fold((0.0_f32, 0.0_f32), |acc, (at, w)| {
                        w.points
                            .iter()
                            .enumerate()
                            .fold(acc, |(mx, mn), (j, (hi, lo))| {
                                let b = crate::num::coord(j).mul_add(w.step, *at);
                                if b < to && b + w.step > from {
                                    (mx.max(*hi), mn.min(*lo))
                                } else {
                                    (mx, mn)
                                }
                            })
                    })
                })
                .collect::<Vec<_>>()
        };
        let wave = |step: f64, n: usize, seed: f32| Wave {
            step,
            points: (0..n)
                .map(|i| {
                    let at = num_traits::ToPrimitive::to_f32(&crate::num::coord(i)).unwrap_or(0.0);
                    let v = ((at + seed) * 0.37).sin().abs();
                    (v, -v * 0.8)
                })
                .collect(),
        };
        let kids = vec![
            (1.003, wave(0.01, 40, 0.0)),
            (1.21, wave(0.01, 7, 3.0)),
            (1.35, wave(0.004, 90, 5.0)),
            (0.9, wave(0.01, 20, 9.0)),
        ];
        let children: Vec<Child<'_>> = kids
            .iter()
            .map(|(at, w)| Child {
                at: *at,
                length: crate::num::coord(w.points.len()) * w.step,
                wave: Some(w),
                notes: None,
            })
            .collect();
        let folded = fold_waves(1.0, 1.8, &children).expect("a fold");
        let want = naive(1.0, 1.8, &kids, folded.points.len());
        assert_eq!(folded.points, want);
    }

    #[test]
    fn a_folded_wave_is_the_loudest_child_where_it_sounds() {
        let quiet = flat(0.2, 10); // 0..1 s
        let loud = flat(0.8, 10); // 2..3 s
        let both = flat(0.5, 30); // 0..3 s
        let wave = fold_waves(
            0.0,
            3.0,
            &[
                child(0.0, 1.0, Some(&quiet), None),
                child(2.0, 1.0, Some(&loud), None),
                child(0.0, 3.0, Some(&both), None),
            ],
        )
        .expect("a wave");
        let at = |secs: f64| wave.points[crate::num::index(secs / wave.step)];
        let near = |got: f32, want: f32| (got - want).abs() < 1e-6;
        assert!(
            near(at(0.45).0, 0.5),
            "the louder of the two over the first second"
        );
        assert!(near(at(1.5).0, 0.5), "only the long child in the middle");
        let (max, min) = at(2.5);
        assert!(
            near(max, 0.8) && near(min, -0.8),
            "the loud one where it sounds"
        );
        assert!(fold_waves(0.0, 1.0, &[child(0.0, 1.0, None, None)]).is_none());
    }

    /// Notes land where they sound in time, as fractions of the folded
    /// span; a note outside it is left out, one across its edge is cut.
    #[test]
    fn folded_notes_land_where_they_sound() {
        let notes = [
            Note {
                at: 0.0,
                len: 0.25,
                pitch: 60,
                velocity: 100,
            },
            Note {
                at: 0.5,
                len: 0.25,
                pitch: 64,
                velocity: 100,
            },
        ];
        // The child is 4 s long at 2 s: notes at 2..3 s and 4..5 s.
        let folded = fold_notes(2.0, 6.0, &[child(2.0, 4.0, None, Some(&notes))]).expect("notes");
        assert_eq!(folded.len(), 2);
        assert!(
            (folded[1].at - 0.5).abs() < 1e-6,
            "4 s is half way through 2..6 s"
        );
        let cut = fold_notes(2.5, 4.0, &[child(2.0, 4.0, None, Some(&notes))]).expect("notes");
        assert_eq!(cut.len(), 1, "the note at 4 s starts at the span's end");
        assert!(
            (cut[0].len - (0.5 / 1.5)).abs() < 1e-6,
            "cut to its half inside"
        );
    }
}
