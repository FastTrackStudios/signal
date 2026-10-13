//! Record mode's performance view: recording vocals, with REAPER doing the
//! recording and this — on a tablet, a phone or a page — driving it.
//!
//! - **On top**, the song's bar and, right under it, the section's: the
//!   current section split into its measures, a press on one going there.
//! - **On the right**, the DAW's mixer, with just the five things a singer
//!   needs to hear — Click, Vocal, Drums, Bass, Guitar. Touch mode
//!   ([`crate::touch`]) makes it a finger's: bigger, a fader that moves
//!   only by its cap.
//! - **On the left**, the rating pad (★★★ / ★★ / ★ / ✕): a press marks the
//!   vocal take at the playhead — while he sings, or after, moved there by
//!   the bars ([`crate::take_rating`]).
//! - **When a take ends**, a prompt rates it whole, or offers to go through
//!   it in more detail with the pad.
//! - **At the bottom**, the transport, with Record where Loop is.

use dioxus::prelude::*;
use session_ui::components::{MeasureIndicator, SectionProgressBar};

use crate::compact::{Form, use_form};
use crate::engine::{Move, transport};
use crate::progress::{ProgressBar, Song, TransportButtons, use_reading};
use crate::studio::StudioSession;
use crate::take_rating::{Scope, Stars, miss, star};

const BG: &str = "#0f1012";
const PANEL: &str = "#17181b";
const RULE: &str = "#2a2c31";
const TEXT: &str = "#e5e7eb";
const DIM: &str = "#8b9099";

/// The record view, over the song in context.
#[component]
pub fn RecordView() -> Element {
    let form = use_form();
    let reading = use_reading();
    // Where the take began, while one is being recorded; the take to ask
    // about once it has stopped.
    let mut began = use_signal(|| None::<f64>);
    let mut ask = use_signal(|| None::<f64>);
    let recording = reading().recording;
    use_effect(use_reactive!(|recording| {
        let at = reading.peek().at;
        match (recording, began()) {
            (true, None) => began.set(Some(at)),
            (false, Some(from)) => {
                began.set(None);
                ask.set(Some(from));
            }
            _ => {}
        }
    }));
    let upright = form == Form::Portrait;
    let pick = try_use_context::<PickSong>();
    let body = if upright {
        rsx! {
            div {
                style: "flex:1; min-height:0; display:flex; flex-direction:column; gap:10px; padding:10px;",
                RatingPad { upright }
                div { style: "flex:1; min-height:0; display:flex;", RecordMixer {} }
            }
        }
    } else {
        rsx! {
            div {
                style: "flex:1; min-height:0; display:flex; gap:12px; padding:10px 12px;",
                // The set and the song's sections, for a long song: jump
                // anywhere without scrubbing the bars.
                div {
                    style: "position:relative; flex:none; width:240px; border-radius:16px; overflow:hidden; \
                            background:{PANEL}; border:1px solid {RULE};",
                    crate::navigator::Navigator {
                        on_pick: move |index: usize| {
                            if let Some(PickSong(pick)) = pick {
                                pick.call(index);
                            }
                        },
                    }
                }
                div { style: "flex:none; width:132px; display:flex;", RatingPad { upright } }
                RecordMixer {}
            }
        }
    };
    // Room for the window's traffic lights on a Mac, whose title bar the
    // top row stands in for.
    let lead = if cfg!(target_os = "macos") { 84 } else { 12 };
    let (song_h, section_h) = if upright {
        ("3rem", "2.5rem")
    } else {
        ("4rem", "2.75rem")
    };
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100%; height:100%; box-sizing:border-box; \
                    display:flex; flex-direction:column; background:{BG}; color:{TEXT}; \
                    font-family:system-ui, sans-serif;",
            div {
                style: "flex:none; display:flex; flex-direction:column; gap:6px; padding:8px 12px 0 {lead}px;",
                div {
                    style: "display:flex; gap:8px; align-items:stretch;",
                    RecordMenu { height: song_h.to_owned() }
                    div { style: "flex:1; min-width:0;", ProgressBar { height: song_h.to_owned(), labels: !upright } }
                }
                SectionBar { height: section_h.to_owned() }
            }
            {body}
            div {
                style: "flex:none; padding:0 12px 8px;",
                TransportButtons { compact: upright, height: 56, record: true }
            }
            if let Some(from) = ask() {
                TakePrompt {
                    from,
                    on_detail: move |()| {
                        // Back to where the take began, to go through it.
                        transport(Move::Seek, from);
                        ask.set(None);
                    },
                    on_close: move |()| ask.set(None),
                }
            }
        }
    }
}

/// How a host picks a song of the set (the wide layout's tabs, a phone's
/// navigator): given to the record view, which has neither.
#[derive(Clone, Copy)]
pub struct PickSong(pub Callback<usize>);

/// The record view's menu, in place of the app's top bar: a ☰ beside the
/// song's bar, opening the set's songs to pick from and the way back out
/// of record mode.
#[component]
fn RecordMenu(height: String) -> Element {
    use session::modes::Mode;
    let setlist = try_use_context::<Signal<crate::setlist::Setlist>>();
    let pick = try_use_context::<PickSong>();
    let mode = try_use_context::<Signal<Mode>>();
    let touch = try_use_context::<crate::touch::Touch>();
    let mut open = use_signal(|| false);
    let songs: Vec<(usize, String, String)> = setlist
        .map(|s| {
            s.read()
                .songs
                .iter()
                .enumerate()
                .map(|(i, song)| (i, song.name.clone(), song.color.clone()))
                .collect()
        })
        .unwrap_or_default();
    let at = setlist.map_or(0, |s| s.read().at);
    rsx! {
        div {
            style: "position:relative; flex:none; display:flex; z-index:60;",
            button {
                style: "width:52px; height:{height}; border-radius:12px; border:1px solid {RULE}; \
                        background:{PANEL}; color:{TEXT}; display:flex; align-items:center; \
                        justify-content:center; cursor:pointer;",
                onclick: move |_| open.toggle(),
                lucide_dioxus::Menu { size: 24, color: "currentColor" }
            }
            if open() {
                // A press outside closes it.
                div {
                    style: "position:fixed; top:0; left:0; width:100vw; height:100vh; z-index:61;",
                    onclick: move |_| open.set(false),
                }
                div {
                    style: "position:absolute; top:100%; margin-top:8px; left:0; z-index:62; width:300px; \
                            max-height:70vh; overflow-y:auto; display:flex; flex-direction:column; gap:10px; \
                            padding:10px; border-radius:16px; background:#1c1e22; border:1px solid {RULE}; \
                            box-shadow:0 16px 40px rgba(0,0,0,0.6);",
                    if let Some(mut mode) = mode {
                        div {
                            style: "display:flex; gap:4px; padding:3px; border-radius:12px; background:{PANEL}; border:1px solid {RULE};",
                            for (each, label) in [(Mode::Live, "Live"), (Mode::Record, "Record")] {
                                button {
                                    key: "{label}",
                                    style: {
                                        let on = mode() == each;
                                        let (fg, bg) = if on {
                                            (if each == Mode::Record { "#fca5a5" } else { "#93c5fd" }, "#24272d")
                                        } else {
                                            (DIM, "transparent")
                                        };
                                        format!(
                                            "flex:1; height:40px; border-radius:9px; border:none; background:{bg}; \
                                             color:{fg}; font-family:inherit; font-size:15px; font-weight:650; cursor:pointer;"
                                        )
                                    },
                                    onclick: move |_| {
                                        mode.set(each);
                                        open.set(false);
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                    if let Some(crate::touch::Touch(mut touch)) = touch {
                        button {
                            style: {
                                let (fg, bg) = if touch() { ("#93c5fd", "#24272d") } else { (DIM, PANEL) };
                                format!(
                                    "display:flex; align-items:center; justify-content:space-between; height:44px; \
                                     padding:0 14px; border-radius:12px; border:1px solid {RULE}; background:{bg}; \
                                     color:{fg}; font-family:inherit; font-size:15px; font-weight:650; cursor:pointer;"
                                )
                            },
                            onclick: move |_| touch.toggle(),
                            span { "Touch controls" }
                            span { if touch() { "On" } else { "Off" } }
                        }
                    }
                    span { style: "font-size:11px; font-weight:700; letter-spacing:0.08em; color:{DIM}; padding:2px 4px 0;", "SONGS" }
                    for (i, title, dot) in songs {
                        button {
                            key: "{i}",
                            style: {
                                let bg = if i == at { "#262a31" } else { "transparent" };
                                format!(
                                    "display:flex; align-items:center; gap:10px; min-height:48px; padding:0 12px; \
                                     border:none; border-radius:10px; background:{bg}; color:{TEXT}; \
                                     font-family:inherit; font-size:16px; text-align:left; cursor:pointer;"
                                )
                            },
                            onclick: move |_| {
                                open.set(false);
                                if let Some(PickSong(pick)) = pick {
                                    pick.call(i);
                                }
                            },
                            span { style: "flex:none; width:10px; height:10px; border-radius:5px; background:{dot};" }
                            span { style: "flex:1; min-width:0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{title}" }
                            span { style: "flex:none; font-size:12px; color:{DIM};", "{i + 1}" }
                        }
                    }
                }
            }
        }
    }
}

/// The section's bar: the current section of the song, split into its
/// measures, each filling as it plays; a press on one goes to it.
#[component]
fn SectionBar(height: String) -> Element {
    let session: StudioSession = use_context();
    let song = use_hook(|| Song::of(&session));
    let reading = use_reading();
    let Some(song) = song else {
        return rsx! {};
    };
    let at = reading().at;
    let Some(index) = song
        .current(at)
        .or(Some(0))
        .filter(|i| *i < song.sections.len())
    else {
        return rsx! {};
    };
    let (from, to) = song.sections[index];
    let span = (to - from).max(f64::EPSILON);
    // The measures that start inside the section, from the song's tempo
    // map — or, for a song without one, its tempo in 4/4 from the start.
    let fallback = [daw_ui::studio::project::TempoChange {
        at: 0.0,
        bpm: if reading().bpm > 0.0 {
            reading().bpm
        } else {
            120.0
        },
        beats_per_bar: 4,
        beat_unit: 4,
    }];
    let tempo: &[daw_ui::studio::project::TempoChange] = if session.project.0.tempo.is_empty() {
        &fallback
    } else {
        &session.project.0.tempo
    };
    let starts: Vec<(u32, f64)> = crate::ruler::Timeline::new(tempo)
        .beats(to, 20_000)
        .into_iter()
        .filter(|b| b.beat == 1 && b.at >= from - 1e-6 && b.at < to - 1e-6)
        .map(|b| (b.measure, b.at))
        .collect();
    let measures: Vec<MeasureIndicator> = starts
        .iter()
        .map(|(measure, start)| MeasureIndicator {
            position_percent: (start - from) / span * 100.0,
            measure_number: i32::try_from(*measure).unwrap_or(i32::MAX),
            time_signature: None,
            musical_position: daw_proto::MusicalPosition {
                measure: i32::try_from(*measure).unwrap_or(i32::MAX),
                beat: 1,
                subdivision: 0,
            },
        })
        .collect();
    let mut section = song.bar[index].clone();
    section.start_percent = 0.0;
    section.end_percent = 100.0;
    let progress = ((at - from) / span * 100.0).clamp(0.0, 100.0);
    rsx! {
        div {
            style: "flex:none;",
            SectionProgressBar {
                height: height.clone(),
                progress,
                sections: vec![section],
                measure_indicators: measures,
                on_measure_click: move |position: daw_proto::MusicalPosition| {
                    let found = starts
                        .iter()
                        .find(|(measure, _)| i64::from(*measure) == i64::from(position.measure));
                    if let Some((_, start)) = found {
                        transport(Move::Seek, *start);
                    }
                },
            }
        }
    }
}

/// The rating pad: a ★ and a ✕, marking the vocal take at the playhead —
/// ★ again at the same spot is one more star, up to three.
#[component]
fn RatingPad(upright: bool) -> Element {
    let reading = use_reading();
    let mut stars = use_signal(Stars::default);
    let direction = if upright { "row" } else { "column" };
    let level = stars().level();
    rsx! {
        div {
            style: "flex:1; min-width:0; min-height:0; display:flex; flex-direction:column; gap:8px; \
                    padding:12px; border-radius:16px; background:{PANEL}; border:1px solid {RULE};",
            span { style: "font-size:12px; font-weight:700; letter-spacing:0.06em; color:{DIM}; text-align:center;", "RATE" }
            div {
                style: "flex:1; min-height:0; display:flex; flex-direction:{direction}; gap:8px;",
                StarButton {
                    level,
                    on_press: move |()| {
                        let at = reading.peek().at;
                        stars.write().press(at);
                        star(Scope::Moment(at));
                    },
                }
                MissButton {
                    on_press: move |()| {
                        stars.write().reset();
                        miss(Scope::Moment(reading.peek().at));
                    },
                }
            }
        }
    }
}

/// The ★: how many stars its spot has so far, under it.
#[component]
fn StarButton(level: Option<u8>, on_press: EventHandler<()>) -> Element {
    let shown = level.unwrap_or(0);
    let row: String = (1..=crate::take_rating::MAX_STARS)
        .map(|n| if n <= shown { '★' } else { '☆' })
        .collect();
    rsx! {
        button {
            style: "flex:1; min-width:0; min-height:48px; display:flex; flex-direction:column; align-items:center; \
                    justify-content:center; gap:6px; border-radius:14px; border:1px solid {RULE}; \
                    background:#1f2228; color:#fbbf24; font-family:inherit; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            span { style: "font-size:40px; line-height:1;", "★" }
            span { style: "font-size:13px; letter-spacing:2px; color:#a38a4a;", "{row}" }
        }
    }
}

/// The ✕ (drawn: the glyph is missing from the fonts a phone draws with).
#[component]
fn MissButton(on_press: EventHandler<()>) -> Element {
    rsx! {
        button {
            style: "flex:1; min-width:0; min-height:48px; display:flex; align-items:center; justify-content:center; \
                    border-radius:14px; border:1px solid {RULE}; background:#1f2228; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            lucide_dioxus::X { size: 34, color: "#f87171", stroke_width: 3 }
        }
    }
}

/// The prompt at the end of a take: ★ (again, for more) or ✕ for the whole
/// take, or go through it in more detail.
#[component]
fn TakePrompt(from: f64, on_detail: EventHandler<()>, on_close: EventHandler<()>) -> Element {
    let mut stars = use_signal(Stars::default);
    let level = stars().level();
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100%; height:100%; z-index:50; display:flex; \
                    align-items:center; justify-content:center; background:rgba(0,0,0,0.6);",
            onclick: move |_| on_close.call(()),
            div {
                style: "width:min(92%, 460px); box-sizing:border-box; padding:20px; border-radius:20px; \
                        background:{PANEL}; border:1px solid {RULE}; display:flex; flex-direction:column; gap:14px;",
                onclick: move |e| e.stop_propagation(),
                span { style: "font-size:20px; font-weight:700;", "How was that take?" }
                div {
                    style: "display:flex; gap:10px; height:96px;",
                    StarButton {
                        level,
                        on_press: move |()| {
                            // The whole take: every ★ lands on its start.
                            stars.write().press(0.0);
                            star(Scope::Take(from));
                        },
                    }
                    MissButton {
                        on_press: move |()| {
                            stars.write().reset();
                            miss(Scope::Take(from));
                            on_close.call(());
                        },
                    }
                }
                div {
                    style: "display:flex; gap:10px;",
                    button {
                        style: "flex:1; height:48px; border-radius:12px; border:1px solid {RULE}; background:transparent; \
                                color:{TEXT}; font-family:inherit; font-size:15px; font-weight:600; cursor:pointer;",
                        onclick: move |_| on_detail.call(()),
                        "More detailed"
                    }
                    button {
                        style: "flex:1; height:48px; border-radius:12px; border:none; background:#3aa0ff; \
                                color:#0b0c0e; font-family:inherit; font-size:15px; font-weight:700; cursor:pointer;",
                        onclick: move |_| on_close.call(()),
                        "Done"
                    }
                }
            }
        }
    }
}

/// The five groups a singer's mix is made of, left to right: each matched
/// to the project's top-level track whose name says it.
const GROUPS: [(&str, &[&str]); 5] = [
    ("Click", &["click"]),
    ("Vocal", &["vocal", "vox"]),
    ("Drums", &["drum"]),
    ("Bass", &["bass"]),
    ("Guitar", &["guitar", "gtr"]),
];

/// A group's track, as the mixer shows it.
#[derive(Clone, Debug, PartialEq)]
struct Strip {
    label: &'static str,
    /// The group's top-level track, when the project has one.
    track: Option<daw_proto::Track>,
    /// The track its arm button arms: the group's track itself, or — for a
    /// folder, which REAPER does not record into — the track in it that
    /// records (see [`arm_target`]).
    arm: Option<daw_proto::Track>,
}

/// The mixer: the DAW's own ([`crate::mixer_panel`]), with one strip per
/// group the project has. Which tracks those are is read from the engine
/// now and again, since a song picked is another project; what they are
/// set to arrives as the engine's track events, inside the mixer.
#[component]
fn RecordMixer() -> Element {
    let mut strips = use_signal(Vec::<Strip>::new);
    use_future(move || async move {
        loop {
            if let Some(found) = read_strips().await
                && guids(&found) != guids(&strips.peek())
            {
                strips.set(found);
            }
            architect::platform::sleep(std::time::Duration::from_secs(2)).await;
        }
    });
    let list = strips();
    if list.iter().all(|s| s.track.is_none()) {
        return rsx! {};
    }
    // The room the view has left: the strips grow to fill it
    // (`Links::fill`) rather than leaving a gap beside them.
    rsx! {
        div {
            style: "position:relative; flex:1; min-width:0; height:100%; \
                    border-radius:16px; overflow:hidden; border:1px solid {RULE};",
            GroupMixer { key: "{guids(&list)}", strips: list }
        }
    }
}

/// The groups' strips as the mixer's rows: each group's track, its arm
/// sent to the track under it that records.
#[component]
fn GroupMixer(strips: Vec<Strip>) -> Element {
    use_context_provider(|| {
        let rows = strips
            .iter()
            .filter_map(|s| s.track.clone())
            .map(|track| (track, 0))
            .collect();
        let arm_of = strips
            .iter()
            .filter_map(|s| {
                let (track, arm) = (s.track.as_ref()?, s.arm.as_ref()?);
                (arm.guid != track.guid).then(|| (track.guid.clone(), arm.clone()))
            })
            .collect();
        let mut links = crate::mixer_panel::Links::alone(rows, arm_of);
        links.fill = true;
        links
    });
    #[cfg(feature = "native")]
    return rsx! { crate::mixer_panel::Mixer {} };
    #[cfg(all(feature = "web", not(feature = "native")))]
    return rsx! { crate::mixer_panel::WebMixer { hidden: false } };
    #[cfg(not(any(feature = "native", feature = "web")))]
    rsx! {}
}

/// Which tracks the groups are: what, changed, means another mixer.
fn guids(strips: &[Strip]) -> String {
    strips
        .iter()
        .flat_map(|s| [s.track.as_ref(), s.arm.as_ref()])
        .map(|t| t.map_or("-", |t| t.guid.as_str()))
        .collect::<Vec<_>>()
        .join(",")
}

/// The groups' tracks, from the engine.
async fn read_strips() -> Option<Vec<Strip>> {
    let daw = daw::rpc::Daw::try_get()?;
    let project = daw.current_project().await.ok()?;
    let tracks = project.tracks().all().await.ok()?;
    Some(strips_of(&tracks))
}

/// The five groups, in order: each with its top-level track, when the
/// project has one, and the track its arm button arms.
fn strips_of(tracks: &[daw_proto::Track]) -> Vec<Strip> {
    GROUPS
        .iter()
        .map(|(label, words)| {
            let says = |t: &daw_proto::Track| {
                let name = t.name.to_lowercase();
                words.iter().any(|w| name.contains(w))
            };
            let track = tracks
                .iter()
                .find(|t| t.parent_guid.is_none() && says(t))
                .cloned();
            let arm = track.as_ref().and_then(|t| arm_target(tracks, t, &says));
            Strip { label, track, arm }
        })
        .collect()
}

/// The track a group's arm button arms: a plain track itself; for a folder,
/// the track under it that records — an armed one first, then one whose
/// name says the group, then the first.
fn arm_target(
    tracks: &[daw_proto::Track],
    group: &daw_proto::Track,
    says: &dyn Fn(&daw_proto::Track) -> bool,
) -> Option<daw_proto::Track> {
    if !group.is_folder {
        return Some(group.clone());
    }
    let under = |t: &daw_proto::Track| {
        let mut at = t.parent_guid.as_deref();
        while let Some(guid) = at {
            if guid == group.guid {
                return true;
            }
            at = tracks
                .iter()
                .find(|p| p.guid == guid)
                .and_then(|p| p.parent_guid.as_deref());
        }
        false
    };
    let inside: Vec<&daw_proto::Track> =
        tracks.iter().filter(|t| !t.is_folder && under(t)).collect();
    inside
        .iter()
        .find(|t| t.armed)
        .or_else(|| inside.iter().find(|t| says(t)))
        .or_else(|| inside.first())
        .map(|t| (*t).clone())
}

#[cfg(test)]
mod tests {
    use super::strips_of;

    fn track(guid: &str, name: &str, parent: Option<&str>) -> daw_proto::Track {
        daw_proto::Track {
            guid: guid.into(),
            name: name.into(),
            parent_guid: parent.map(Into::into),
            ..daw_proto::Track::default()
        }
    }

    #[test]
    fn the_mixer_shows_the_five_groups_in_order_by_their_top_level_tracks() {
        let tracks = [
            track("g", "Guitar", None),
            track("v", "Vocal", None),
            track("k", "Kick", Some("d")),
            track("d", "DRUMS", None),
            track("c", "Click", None),
            track("x", "Keys", None),
        ];
        let strips = strips_of(&tracks);
        let labels: Vec<(&str, Option<&str>)> = strips
            .iter()
            .map(|s| (s.label, s.track.as_ref().map(|t| t.guid.as_str())))
            .collect();
        assert_eq!(
            labels,
            [
                ("Click", Some("c")),
                ("Vocal", Some("v")),
                ("Drums", Some("d")),
                ("Bass", None),
                ("Guitar", Some("g"))
            ]
        );
    }
}
