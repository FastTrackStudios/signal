//! The performance panels: the ProgressBar and the transport buttons under
//! the view.
//!
//! Read off the open session rather than the setlist engine for now: the
//! SONG region (lane 0) is the song's span, the SECTIONS regions (lane 1)
//! are its parts — the regions `build_from_chart` stamps. The bar and the
//! buttons are `session-ui`'s, unchanged; what is here is the data and the
//! transport they drive. The same panels in the desktop app and the web
//! demo: only how the reading is polled differs.

use dioxus::prelude::*;

use crate::engine::{Move, Reading, Transport, transport};
use crate::studio::StudioSession;
use session_ui::components::progress::{ProgressSection, SongProgressBar};
use session_ui::components::transport_controls::TransportControlBar;

/// The song as the performance panels (and the navigator) see it.
#[derive(Clone, PartialEq)]
pub(crate) struct Song {
    pub(crate) start: f64,
    pub(crate) end: f64,
    /// Each section's span, in order.
    pub(crate) sections: Vec<(f64, f64)>,
    pub(crate) bar: Vec<ProgressSection>,
}

impl Song {
    pub(crate) fn of(session: &StudioSession) -> Option<Self> {
        let regions = &session.project.sections;
        let (start, end) = regions
            .iter()
            .find(|r| r.lane == 0)
            .map(|r| (r.start, r.end))
            .or_else(|| {
                let start = regions
                    .iter()
                    .map(|r| r.start)
                    .fold(f64::INFINITY, f64::min);
                let end = regions.iter().map(|r| r.end).fold(0.0, f64::max);
                (end > start).then_some((start, end))
            })?;
        let span = (end - start).max(f64::EPSILON);
        let percent = |t: f64| ((t - start) / span * 100.0).clamp(0.0, 100.0);
        let mut parts: Vec<_> = regions
            .iter()
            .filter(|r| r.lane == crate::ruler::SECTIONS_ROW as u32)
            .collect();
        parts.sort_by(|a, b| a.start.total_cmp(&b.start));
        Some(Self {
            start,
            end,
            sections: parts.iter().map(|r| (r.start, r.end)).collect(),
            bar: parts
                .iter()
                .map(|r| ProgressSection {
                    start_percent: percent(r.start),
                    end_percent: percent(r.end),
                    color: r.color.clone().unwrap_or_else(|| "#4b5563".to_owned()),
                    name: r.name.clone(),
                    short_name: r.name.clone(),
                    comment: None,
                })
                .collect(),
        })
    }

    /// The section the play position is in, if any.
    pub(crate) fn current(&self, at: f64) -> Option<usize> {
        self.sections
            .iter()
            .rposition(|(from, _)| *from <= at + 1e-6)
    }

    fn progress(&self, at: f64) -> f64 {
        ((at - self.start) / (self.end - self.start).max(f64::EPSILON) * 100.0).clamp(0.0, 100.0)
    }
}

/// The transport's reading, republished when the position moves a
/// twentieth of a second or a flag changes — enough for a song-wide bar,
/// where a frame's travel is under a pixel.
pub(crate) fn use_reading() -> Signal<Reading> {
    let mut reading = use_signal(Reading::default);
    let mut publish = move || {
        let Some(now) = Transport::shared().map(Transport::reading) else {
            return;
        };
        let was = *reading.peek();
        if (now.at - was.at).abs() > 0.05
            || now.playing != was.playing
            || now.looping != was.looping
            || now.recording != was.recording
        {
            reading.set(now);
        }
    };
    #[cfg(feature = "native")]
    dioxus_native::use_window_event(move |event, _| {
        if matches!(event, winit::event::WindowEvent::RedrawRequested) {
            publish();
        }
    });
    #[cfg(all(feature = "web", not(feature = "native")))]
    use_future(move || async move {
        loop {
            publish();
            gloo_timers::future::TimeoutFuture::new(33).await;
        }
    });
    reading
}

/// The ProgressBar panel. A click on a section plays from it.
#[component]
pub fn ProgressBar(
    /// Its height as CSS — slim on a phone (5rem unless given).
    #[props(default)]
    height: Option<String>,
    /// Whether the sections are named on it (not on an upright phone).
    #[props(default = true)]
    labels: bool,
    /// Square-cornered, edge to edge in a bar (a phone's).
    #[props(default)]
    flat: bool,
) -> Element {
    let session: StudioSession = use_context();
    let song = use_hook(|| Song::of(&session));
    let reading = use_reading();
    let Some(song) = song else {
        return rsx! {
            div { style: "color:#8b9099; font-size:12px;", "No song regions in this session." }
        };
    };
    let starts: Vec<f64> = song.sections.iter().map(|(from, _)| *from).collect();
    // Others' pointers over the bar are a time in the song, placed on this
    // bar wherever it is and however wide.
    #[cfg(feature = "native")]
    use_hook({
        let span = (song.start, song.end);
        move || crate::ghosts::register_anchor("progress", std::rc::Rc::new(SongAnchor { span }))
    });
    rsx! {
                SongProgressBar {
            height,
            labels,
            flat,
            progress: song.progress(reading().at),
            sections: song.bar.clone(),
            on_section_click: move |index: usize| {
                if let Some(at) = starts.get(index) {
                    transport(Move::Seek, *at);
                }
            },
        }
    }
}

/// The performance transport: back a section, play/stop, loop (record, in
/// record mode), on a section.
/// `compact` is a small screen's: the four icons alone, `height` pixels tall
/// (44 unless given).
#[component]
pub fn TransportButtons(
    #[props(default)] compact: bool,
    #[props(default)] height: Option<u32>,
    /// Record mode's: Record where Loop is.
    #[props(default)]
    record: bool,
) -> Element {
    let session: StudioSession = use_context();
    let song = use_hook(|| Song::of(&session));
    let reading = use_reading();
    let r = reading();
    let back = song.clone();
    let on = song;
    // Read up front, never behind a condition (`try_use_context` is a
    // hook): picking the song before or after, as the tabs do.
    let pick = try_use_context::<crate::record_view::PickSong>();
    let setlist = try_use_context::<Signal<crate::setlist::Setlist>>();
    let height = height.unwrap_or(if compact { 44 } else { 64 });
    rsx! {
        div {
            style: "height:{height}px; flex:none; overflow:hidden; border-radius:10px;",
            TransportControlBar {
                icons_only: compact,
                is_playing: r.playing,
                is_looping: r.looping,
                is_recording: r.recording,
                is_armed: false,
                show_recording: false,
                record_in_loop: record,
                on_play_pause: move |()| transport(Move::PlayStop, 0.0),
                on_loop_toggle: move |()| transport(Move::ToggleLoop, 0.0),
                on_record_toggle: move |()| transport(Move::ToggleRecord, 0.0),
                on_arm_toggle: move |()| {},
                                // Back: to the start of this section, or — within a second
                // of it — to the one before, which is what a second press
                // of a back button means; at the song's very start, the
                // song before.
                on_back: move |()| {
                    let Some(song) = back.as_ref() else { return };
                    let at = Transport::shared().map_or(0.0, |t| t.read().0);
                    match back_to(song, at) {
                        Step::Seek(to) => transport(Move::Seek, to),
                        Step::Song => step_song(pick, setlist, -1),
                    }
                },
                // Advance: to the next section, and after the last, the
                // next song.
                on_forward: move |()| {
                    let Some(song) = on.as_ref() else { return };
                    let at = Transport::shared().map_or(0.0, |t| t.read().0);
                    match forward_to(song, at) {
                        Step::Seek(to) => transport(Move::Seek, to),
                        Step::Song => step_song(pick, setlist, 1),
                    }
                },
            }
        }
    }
}

/// Where a transport button goes: a place in this song, or the song
/// before or after it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Seek(f64),
    Song,
}

/// Back from `at`: the start of this section, the section before if this
/// one was only just started, and past the song's first section, the song
/// before.
fn back_to(song: &Song, at: f64) -> Step {
    match song.current(at) {
        Some(i) if at - song.sections[i].0 < 1.0 && i > 0 => Step::Seek(song.sections[i - 1].0),
        Some(0) if at - song.sections[0].0 < 1.0 => Step::Song,
        Some(i) => Step::Seek(song.sections[i].0),
        // Before the first section (the count-in): the song's start, and
        // pressed there again, the song before.
        None if at - song.start > 1.0 => Step::Seek(song.start),
        None => Step::Song,
    }
}

/// Forward from `at`: the next section, and after the last, the next song.
fn forward_to(song: &Song, at: f64) -> Step {
    song.sections
        .iter()
        .map(|(from, _)| *from)
        .find(|from| *from > at + 1e-3)
        .map_or(Step::Song, Step::Seek)
}

/// Pick the song `by` places along the set, if there is one there.
fn step_song(
    pick: Option<crate::record_view::PickSong>,
    setlist: Option<Signal<crate::setlist::Setlist>>,
    by: isize,
) {
    let (Some(crate::record_view::PickSong(pick)), Some(setlist)) = (pick, setlist) else {
        return;
    };
    let list = setlist.peek();
    let to = list
        .at
        .checked_add_signed(by)
        .filter(|to| *to < list.songs.len());
    drop(list);
    if let Some(to) = to {
        pick.call(to);
    }
}

/// A pointer over the progress bar, anchored to the song: `u` is a time,
/// in project seconds; `v` how far down the bar.
#[cfg(feature = "native")]
struct SongAnchor {
    span: (f64, f64),
}

#[cfg(feature = "native")]
impl crate::ghosts::Anchor for SongAnchor {
    fn anchor(&self, x: f64, y: f64, (w, h): (f64, f64)) -> Option<(String, f64, f64)> {
        let (start, end) = self.span;
        let at = (x / w.max(1.0)).mul_add(end - start, start);
        Some(("song".to_owned(), at, y / h.max(1.0)))
    }

    fn place(&self, key: &str, u: f64, v: f64, (w, h): (f64, f64)) -> Option<(f64, f64)> {
        let (start, end) = self.span;
        (key == "song").then(|| ((u - start) / (end - start).max(f64::EPSILON) * w, v * h))
    }
}

#[cfg(test)]
mod tests {
    use super::{Song, Step, back_to, forward_to};

    /// A song from 0 to 60: a count-in, then three sections.
    fn song() -> Song {
        Song {
            start: 0.0,
            end: 60.0,
            sections: vec![(4.0, 20.0), (20.0, 40.0), (40.0, 60.0)],
            bar: Vec::new(),
        }
    }

    #[test]
    fn advance_goes_a_section_on_and_after_the_last_to_the_next_song() {
        let song = song();
        assert_eq!(
            forward_to(&song, 0.0),
            Step::Seek(4.0),
            "the count-in to the first section"
        );
        assert_eq!(forward_to(&song, 10.0), Step::Seek(20.0));
        assert_eq!(
            forward_to(&song, 20.0),
            Step::Seek(40.0),
            "from a section's start, the next"
        );
        assert_eq!(
            forward_to(&song, 45.0),
            Step::Song,
            "in the last, the next song"
        );
    }

    #[test]
    fn back_goes_to_this_sections_start_then_the_one_before_then_the_song_before() {
        let song = song();
        assert_eq!(
            back_to(&song, 30.0),
            Step::Seek(20.0),
            "into a section: its start"
        );
        assert_eq!(
            back_to(&song, 20.5),
            Step::Seek(4.0),
            "just after its start: the one before"
        );
        assert_eq!(back_to(&song, 10.0), Step::Seek(4.0));
        assert_eq!(
            back_to(&song, 4.5),
            Step::Song,
            "at the first section's start: the song before"
        );
        assert_eq!(
            back_to(&song, 3.0),
            Step::Seek(0.0),
            "in the count-in: the song's start"
        );
        assert_eq!(
            back_to(&song, 0.2),
            Step::Song,
            "at the song's start: the song before"
        );
    }
}
