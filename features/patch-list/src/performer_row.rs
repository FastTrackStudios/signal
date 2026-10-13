//! The performer row: two controls over sends, and no state of its own.
//!
//! While tracking, the things that need doing are about the person
//! playing: their input, their arm, what they hear of themselves. So
//! Record mode groups rows under a performer header, and that header
//! carries their rig and their cue mix.
//!
//! # Why "more of me" is a gang and not a fader
//!
//! There is no "me" bus to turn up. A performer's own tracks reach
//! their cue mix as individual sends, which is what makes the set
//! knowable — and what would make "more of me" fifteen gestures if it
//! were not ganged. The row moves them **together and relatively**, so
//! the balance the engineer set between a performer's own mics survives
//! being asked for more of all of them.
//!
//! # Why the row stores nothing
//!
//! The sends are the state. A level kept on the row would be a second
//! opinion about a number REAPER already has, and the two would part
//! company the first time anyone touched a send directly. The row's
//! only own state is which performer it is.

/// One send into a cue bus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Send {
    /// Linear gain, as REAPER keeps it.
    pub gain: f64,
}

/// What a relative move does to a set of sends.
///
/// Returns the new gain for each, in the order given. A caller writes
/// only what differs — the same diff-and-write the gangs use, for the
/// same reason.
///
/// `by` is a **ratio**, not an offset: doubling a group of sends keeps
/// the quiet one quiet, while adding a fixed amount would close the gap
/// between them and flatten the balance the engineer set.
#[must_use]
pub fn scaled(sends: &[Send], by: f64) -> Vec<Send> {
    sends
        .iter()
        .map(|send| Send {
            gain: (send.gain * by).clamp(0.0, MAX_GAIN),
        })
        .collect()
}

/// The loudest a cue send is allowed to go.
///
/// Cue mixes live in headphones on someone's head, and a runaway gain
/// there is not a bad mix, it is an injury. Four is about +12 dB, which
/// is as much as "more of me" should ever need.
pub const MAX_GAIN: f64 = 4.0;

/// The ratio that moves a set so its **loudest** member lands on
/// `target`, without changing the balance between them.
///
/// Asking by the loudest rather than by the average is deliberate: it
/// is what stops a single quiet send from letting the rest run past the
/// ceiling on the way to an average.
#[must_use]
pub fn ratio_to(sends: &[Send], target: f64) -> f64 {
    let loudest = sends.iter().fold(0.0_f64, |acc, s| acc.max(s.gain));
    if loudest <= 0.0 {
        return 1.0;
    }
    (target / loudest).max(0.0)
}

/// What a performer's row shows.
///
/// Every field is read from somewhere else — the patch list, the sends,
/// the gangs — which is the point. Constructing one is a query, not a
/// place to keep anything.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The only thing the row owns.
    pub performer: String,
    /// The cue bus they listen to, if the plan gives them one.
    pub bus: Option<String>,
    /// Their own tracks' sends into it — the "me" set.
    pub me: Vec<Send>,
    /// The instrument buses' sends into it — the "band" set.
    pub band: Vec<Send>,
}

impl Row {
    /// The loudest of the performer's own sends, which is what the
    /// "me" control reads.
    #[must_use]
    pub fn me_level(&self) -> f64 {
        self.me.iter().fold(0.0_f64, |acc, s| acc.max(s.gain))
    }

    /// The loudest of the band's, which is what the "band" control
    /// reads.
    #[must_use]
    pub fn band_level(&self) -> f64 {
        self.band.iter().fold(0.0_f64, |acc, s| acc.max(s.gain))
    }

    /// A performer with no bus can hear nothing, and the row says so
    /// rather than drawing controls that would write nowhere.
    #[must_use]
    pub const fn is_listening(&self) -> bool {
        self.bus.is_some()
    }
}
