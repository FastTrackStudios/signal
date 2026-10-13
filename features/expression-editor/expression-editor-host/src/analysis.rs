//! Pure audio analysis: never calls the DAW or renderer.
const DRUM_HOP: usize = 512;
/// Blend weighted member signals into one detection signal.
///
/// The weights come from [`expression_editor_core::kit::detection_units`]
/// and already sum to 1, so the result sits at the same level however
/// many members fed it — a unit of one trigger and a unit of three mics
/// hand the detector comparable material, and a threshold means the same
/// thing in both.
///
/// Length is the longest member, so a trigger that stops short of the
/// take does not truncate the mics that did not.
pub(crate) fn blend<'a>(members: impl IntoIterator<Item = (&'a [f64], f64)>) -> Vec<f64> {
    let members: Vec<(&[f64], f64)> = members.into_iter().collect();
    let len = members.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
    let mut out = vec![0.0f64; len];
    for (samples, weight) in members {
        for (o, v) in out.iter_mut().zip(samples.iter()) {
            *o += v * weight;
        }
    }
    out
}

// r[impl drums.open.runner]
/// The same document, rebuilt from a cached analysis instead of audio.
///
/// Deliberately shares [`percussion_doc`]'s arithmetic rather than
/// re-deriving it: the two must place a hit at the same frame or a
/// cached open would show the kit subtly out of step with a fresh one,
/// which is the sort of difference nobody notices until an edit lands in
/// the wrong place.
pub(crate) fn percussion_doc_cached(
    cached: &crate::analysis_cache::Analysis,
) -> expression_editor_core::ExpressionDoc {
    let frame_rate = cached.sample_rate / DRUM_HOP as f64;
    let total_frames = cached.samples as f64 / DRUM_HOP as f64;
    build_doc(
        frame_rate,
        total_frames,
        cached.peaks.clone(),
        cached.hits.iter().copied(),
    )
}

/// Assemble the document from what detection produced: the backdrop and
/// the hit times. The only place either path builds notes.
fn build_doc(
    frame_rate: f64,
    total_frames: f64,
    peaks: Vec<f32>,
    hits: impl ExactSizeIterator<Item = (f64, f64)> + Clone,
) -> expression_editor_core::ExpressionDoc {
    use expression_editor_core::rows::SliceBands;
    use expression_editor_core::{ExpressionDoc, Note, NoteId, RowSpace, TimeBase};

    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate }, 0.0, total_frames);
    doc.row_space = RowSpace::Bands(SliceBands::default());
    doc.peaks = peaks;
    let times: Vec<(f64, f64)> = hits.collect();
    for (i, (at, loudness)) in times.iter().enumerate() {
        let start = at * frame_rate;
        // A slice runs to the next hit, so the slices tile the take and
        // can be selected and levelled like notes.
        let end = times
            .get(i + 1)
            .map(|(next, _)| next * frame_rate)
            .unwrap_or(total_frames);
        let mut note = Note::new(NoteId(i as u64 + 1), start, end.max(start + 1.0), 1);
        note.velocity = loudness.clamp(0.0, 1.0);
        note.weight = note.velocity;
        doc.push(note);
    }
    doc
}

pub(crate) fn percussion_doc(
    samples: &[f64],
    sample_rate: f64,
) -> expression_editor_core::ExpressionDoc {
    let (doc, _) = percussion_analysis(samples, sample_rate);
    doc
}

/// Detect, and hand back both the document and the small part of it
/// worth keeping on disk. See [`crate::analysis_cache`].
pub(crate) fn percussion_analysis(
    samples: &[f64],
    sample_rate: f64,
) -> (
    expression_editor_core::ExpressionDoc,
    crate::analysis_cache::Analysis,
) {
    use expression_editor_audio::{DetectConfig, transients};

    let frame_rate = sample_rate / DRUM_HOP as f64;
    let total_frames = samples.len() as f64 / DRUM_HOP as f64;
    // The backdrop is the whole recording, hits and gaps alike — the
    // only way to see a missed ghost note is to see past the notes.
    let peaks: Vec<f32> = samples
        .chunks(DRUM_HOP)
        .map(|c| c.iter().fold(0.0f64, |m, v| m.max(v.abs())).min(1.0) as f32)
        .collect();
    let hits: Vec<(f64, f64)> = transients(samples, sample_rate, DetectConfig::default())
        .iter()
        .map(|hit| (hit.at, hit.loudness))
        .collect();
    let doc = build_doc(
        frame_rate,
        total_frames,
        peaks.clone(),
        hits.iter().copied(),
    );
    let cacheable = crate::analysis_cache::Analysis {
        sample_rate,
        samples: samples.len() as u64,
        peaks,
        hits,
    };
    (doc, cacheable)
}

/// Ordered, bounded parallel map. Panics propagate instead of silently
/// dropping a microphone from the phase-coherent edit group.
pub(crate) fn map<T: Send, R: Send>(values: Vec<T>, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    let batch = values.len().div_ceil(workers).max(1);
    let mut values = values.into_iter();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        loop {
            let items: Vec<_> = values.by_ref().take(batch).collect();
            if items.is_empty() {
                break;
            }
            let f = &f;
            handles.push(scope.spawn(move || items.into_iter().map(f).collect::<Vec<_>>()));
        }
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("drum analysis worker panicked"))
            .collect()
    })
}

/// Capture has no Send/Sync bound: a caller may hold a thread-affine facade.
/// Analysis can only see the owned, Send capture result.
pub(crate) fn capture_and_analyze<T, C: Send, R: Send>(
    values: Vec<T>,
    capture: impl FnMut(T) -> Option<C>,
    analyze: impl Fn(C) -> R + Sync,
) -> Vec<R> {
    map(values.into_iter().filter_map(capture).collect(), analyze)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_stays_on_owner_thread_while_analysis_uses_only_owned_data() {
        let owner = std::thread::current().id();
        let affine = std::rc::Rc::new(std::cell::Cell::new(0));
        let result = capture_and_analyze(
            (0..17).collect(),
            |i| {
                assert_eq!(std::thread::current().id(), owner);
                affine.set(affine.get() + 1);
                Some(i)
            },
            |i| {
                assert_ne!(std::thread::current().id(), owner);
                i * 2
            },
        );
        assert_eq!(affine.get(), 17);
        assert_eq!(result, (0..17).map(|i| i * 2).collect::<Vec<_>>());
    }

    #[test]
    #[should_panic(expected = "drum analysis worker panicked")]
    fn a_failed_mic_cannot_silently_disappear_from_the_kit() {
        map(vec![1, 2], |i| {
            assert_ne!(i, 2, "failed analysis");
            i
        });
    }
}

#[cfg(test)]
mod cached_doc_tests {
    use super::*;

    /// The cached path must place hits exactly where the live one does.
    /// A frame's difference would put an edit on the wrong side of a
    /// beat, and it would only show up as a detector that "feels off".
    #[test]
    fn a_cached_document_matches_the_one_detection_produced() {
        // A signal with clear transients: silence, spike, silence, spike.
        let rate = 48_000.0;
        let mut samples = vec![0.0f64; 48_000];
        for at in [4_000usize, 20_000, 33_000] {
            for i in 0..400 {
                let decay = 1.0 - (i as f64 / 400.0);
                samples[at + i] = decay * if i % 2 == 0 { 0.9 } else { -0.9 };
            }
        }

        let (live, cacheable) = percussion_analysis(&samples, rate);
        let cached = percussion_doc_cached(&cacheable);

        assert!(!live.notes.is_empty(), "the fixture must produce hits");
        assert_eq!(live.notes.len(), cached.notes.len());
        assert_eq!(live.peaks, cached.peaks);
        for (a, b) in live.notes.iter().zip(cached.notes.iter()) {
            assert_eq!(a.start, b.start, "hit moved");
            assert_eq!(a.end, b.end, "slice moved");
            assert_eq!(a.velocity, b.velocity);
        }
    }
}
