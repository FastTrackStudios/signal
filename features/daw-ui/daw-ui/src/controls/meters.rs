//! Meters, from the stream that already existed.
//!
//! There is no meter design in here, and that is the point. The backend
//! already publishes one argless subscription carrying the **whole mixer**
//! at ~30Hz — four floats per track, linear 0..1 — with peak-hold computed
//! publisher-side so both backends produce identical ballistics, gated on
//! subscriber count so a closed mixer costs the engine nothing, and dropping
//! frames rather than stalling under backpressure. So this is plumbing:
//! subscribe once, index the frame onto per-track levels, render.
//!
//! **One subscription for the whole mixer.** A per-strip subscription would
//! be N times the messages for the same bytes, and it would throw away the
//! thing that makes the frame cheap.
//!
//! # The alignment, which fails silently
//!
//! The frame is indexed by *track position*, not by guid. Anything that
//! reorders tracks — an add, a remove, a move — re-aims every index at a
//! different track, and the failure does not look like a failure: the meters
//! still move, they are just each showing somebody else's level. The
//! ordering therefore lives in [`TrackStore`][crate::controls::TrackStore],
//! which is the thing that already hears about adds, removes and moves, and
//! a frame whose length disagrees with it is dropped rather than guessed at.

use std::collections::{HashMap, HashSet};

use daw_proto::peak::TrackLevels;
use daw_theme_art::vector_controls as art;

use crate::controls::use_track_store;
use crate::prelude::*;

/// Every track's current levels, by guid.
#[derive(Clone, Copy, PartialEq)]
pub struct Meters {
    levels: Signal<HashMap<String, TrackLevels>>,
}

impl Meters {
    /// Must be called inside a component scope — it owns a Signal.
    pub fn new() -> Self {
        Self {
            levels: Signal::new(HashMap::new()),
        }
    }

    /// This track's levels, or silence if no frame has mentioned it.
    pub fn levels(&self, guid: &str) -> TrackLevels {
        self.levels.read().get(guid).copied().unwrap_or_default()
    }

    /// Index one frame onto `order`.
    ///
    /// A frame that does not have exactly one entry per known track is
    /// **dropped**. That happens for a beat after a track is added or
    /// removed, while the frame and the track list disagree about how many
    /// tracks exist — and showing a stale meter for that beat is much
    /// better than showing every strip its neighbour's level, which is what
    /// indexing a mismatched frame would do.
    pub fn apply(&mut self, order: &[String], frame: &[TrackLevels]) {
        if order.len() != frame.len() {
            return;
        }
        // The idle guard, and it is not an optimisation nicety: a
        // silent project still pumps thirty identical frames a second,
        // and every write here re-renders every meter in the window.
        // A 123-strip mixer at rest was re-laying the whole UI out at
        // 30 Hz — that is the difference between "responsive" and
        // "frozen", not between fast and faster.
        {
            let levels = self.levels.peek();
            if levels.len() == order.len()
                && order
                    .iter()
                    .zip(frame)
                    .all(|(g, l)| levels.get(g) == Some(l))
            {
                return;
            }
        }
        let mut levels = self.levels.write();
        for (guid, l) in order.iter().zip(frame) {
            levels.insert(guid.clone(), *l);
        }
        // A set, not `order.contains`: this runs inside a Signal write at
        // thirty frames a second, and a linear scan per track is quadratic
        // in the size of the mixer.
        let live: HashSet<&str> = order.iter().map(String::as_str).collect();
        levels.retain(|g, _| live.contains(g.as_str()));
    }
}

impl Default for Meters {
    fn default() -> Self {
        Self::new()
    }
}

/// The meters from context, creating and providing one if this is the first
/// asker — same provide-or-consume shape as the track store, so a meter can
/// draw in a test with nothing behind it.
pub fn use_meters() -> Meters {
    use_hook(|| match try_consume_context::<Meters>() {
        Some(m) => m,
        None => provide_context(Meters::new()),
    })
}

/// Keep `meters` fed from the connected DAW.
///
/// Call **once**, high up, beside
/// [`use_daw_tracks`][crate::controls::use_daw_tracks]. One subscription
/// serves every strip; the engine's pump is gated on subscriber count, so
/// not calling this is what keeps a closed mixer free.
#[component]
pub fn MeterFeed() -> Element {
    let store = use_track_store();
    let mut meters = use_meters();

    use_future(move || async move {
        let Some(project) = crate::controls::reach::connected_project().await else {
            return;
        };
        let mut frames = project.meter_events();
        while let Ok(Some(frame)) = frames.recv().await {
            meters.apply(&store.order(), &frame.get().tracks);
        }
    });

    rsx! {}
}

/// A track's stereo meter, with REAPER's dB scale printed down its left.
///
/// The scale belongs *inside* the meter, not beside it: `mcp.meter` is one
/// rect covering both and REAPER draws the numbers as part of the widget.
/// Laid out as a separate column the bars have nowhere to go but under the
/// button stack — which is exactly what the first version of this strip did.
///
/// Levels are linear 0..1 as the frame carries them; the marks are labels,
/// not a conversion.
#[component]
pub fn TrackMeter(
    /// The track's GUID.
    track: String,
    /// The meter's box, which includes the scale when it is printed.
    ///
    /// 26, so the block ends exactly where the fader's cap begins.
    /// Measured against REAPER: its scale's right edge is at x=23 and its
    /// cap's left at 30, with the bars between them. At 24 the block
    /// stopped short and the scale had to be set small to fit.
    #[props(default = 26)]
    width: u32,
    #[props(default = 120)] height: u32,
    /// Print the dB scale inside the meter, as the mixer does.
    #[props(default = true)]
    scale: bool,
    /// What the unlit bars sit on. The track panel passes its own
    /// background, because REAPER's meter there is invisible at rest.
    #[props(default)]
    well: Option<String>,
) -> Element {
    let meters = use_meters();
    let guid = track.clone();
    let levels = use_memo(use_reactive!(|guid| meters.levels(&guid)));
    let l = levels();

    rsx! {
        art::Meter {
            levels: vec![l.peak_left, l.peak_right],
            holds: vec![l.hold_left, l.hold_right],
            cell: (width as f32, height as f32),
            scale: scale,
            well: well,
            marks: if scale { marks_for(height) } else { Vec::new() },
            width: Some(width),
            height: Some(height),
        }
    }
}

/// REAPER's own scale down the mixer meter, top to bottom.
///
/// The ladder is not a fixed list: REAPER prints more of it as the meter
/// grows. A short strip shows `-6 -18 -30 -42 -54` — every 12 dB — and a
/// tall one shows every 6, `-0- -6- -12- ...` down to -60. Printing six
/// marks whatever the height left a tall meter with enormous gaps and a
/// scale that agreed with the fader at only five points.
///
/// The step is chosen so the marks stay about 22 rows apart, which is what
/// they measure in the reference.
fn marks_for(height: u32) -> Vec<String> {
    const FLOOR: i32 = -60;
    const SPACING: f32 = 22.0;

    let room = (height as f32 / SPACING).floor().max(2.0) as i32;
    // Steps that divide the floor exactly, so the ladder always lands on
    // -60 rather than stopping short of it: at 24 dB a 100-row meter read
    // -0 -24 -48 and simply never mentioned the bottom of its own scale.
    let step = [6, 12, 20, 30, 60]
        .into_iter()
        .find(|s| FLOOR.abs() / s <= room)
        .unwrap_or(60);

    let mut marks = vec!["-inf".to_string()];
    let mut db = 0;
    while db >= FLOOR {
        // The hyphens are REAPER's: the marks are printed as `-18-`, a
        // tick either side of the number.
        marks.push(if db == 0 {
            "-0-".to_string()
        } else {
            format!("{}-", db)
        });
        db -= step;
    }
    marks
}

#[cfg(test)]
mod mark_tests {
    use super::marks_for;

    /// The ladder thins as the meter shortens, and always reaches the
    /// floor.
    #[test]
    fn the_ladder_thins_when_there_is_no_room() {
        let tall = marks_for(400);
        let short = marks_for(120);
        assert!(
            tall.len() > short.len(),
            "the ladder did not thin: {short:?}"
        );
        for marks in [&tall, &short] {
            assert_eq!(marks[0], "-inf", "no readout at the top");
            assert!(
                marks.last().unwrap().starts_with("-60"),
                "the floor is missing: {marks:?}"
            );
        }
    }

    /// And it never crowds: the marks stay far enough apart to read.
    #[test]
    fn the_marks_never_crowd() {
        for h in (60..=800).step_by(10) {
            let n = marks_for(h).len() as f32;
            assert!(h as f32 / n >= 8.0, "{n} marks in {h} rows is unreadable",);
        }
    }
}
