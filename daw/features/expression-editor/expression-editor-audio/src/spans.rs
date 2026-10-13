//! Unvoiced spans — where a vocal has no pitch.
//!
//! These are what Vovious calls **sibilants**, and they are the reason
//! amplitude editing has two scopes. A consonant carries no f0, so it
//! is invisible on the pitch track and inaudible to any edit that works
//! on notes — yet it is exactly the thing that needs riding down when a
//! singer's "s" is harsh.
//!
//! The analysis already knows where they are: frames with no detected
//! fundamental. What was missing is promoting that into a span list the
//! UI can hit-test and shade, which is what this does.

/// A run of frames with no detected pitch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    /// First unvoiced frame, inclusive.
    pub start: usize,
    /// Last unvoiced frame, inclusive.
    pub end: usize,
}

impl Span {
    pub fn len(&self) -> usize {
        self.end - self.start + 1
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn contains(&self, frame: usize) -> bool {
        frame >= self.start && frame <= self.end
    }
}

/// Shortest run that counts as a sibilant, in frames.
///
/// A single dropped frame in the middle of a held vowel is a tracker
/// glitch, not a consonant. Without a floor the roll fills with
/// one-frame slivers that cannot be aimed at and mean nothing.
pub const MIN_SPAN_FRAMES: usize = 2;

/// Unvoiced runs in `f0`, where `None` means no pitch was detected.
///
/// Runs shorter than [`MIN_SPAN_FRAMES`] are dropped. Leading and
/// trailing silence is *not* excluded here: the caller knows where the
/// material starts, and this does not.
pub fn unvoiced_spans(f0: &[Option<f64>]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut run: Option<usize> = None;
    for (i, v) in f0.iter().enumerate() {
        match (v, run) {
            (None, None) => run = Some(i),
            (Some(_), Some(start)) => {
                push_span(&mut out, start, i - 1);
                run = None;
            }
            _ => {}
        }
    }
    if let Some(start) = run {
        push_span(&mut out, start, f0.len().saturating_sub(1));
    }
    out
}

fn push_span(out: &mut Vec<Span>, start: usize, end: usize) {
    if end >= start && end - start + 1 >= MIN_SPAN_FRAMES {
        out.push(Span { start, end });
    }
}

/// The unvoiced spans that fall inside `[start, end]`, clipped to it.
///
/// This is what "sibilant scope" means for one note: the consonants
/// *within* its range, not the whole take's. A span straddling the note
/// boundary is clipped rather than dropped, because the half inside the
/// note is still the note's to ride.
pub fn spans_within(spans: &[Span], start: usize, end: usize) -> Vec<Span> {
    spans
        .iter()
        .filter(|s| s.end >= start && s.start <= end)
        .map(|s| Span {
            start: s.start.max(start),
            end: s.end.min(end),
        })
        .collect()
}

/// The span under `frame`, if any.
pub fn span_at(spans: &[Span], frame: usize) -> Option<Span> {
    spans.iter().copied().find(|s| s.contains(frame))
}
