//! The live pass, over the recorded one.
//!
//! Three things cannot be recorded with the strips: the control under
//! the pointer (it changes with the mouse), the meters (they change
//! with the audio), and the play cursor (it changes with the clock).
//! All three are the same shape of problem — something live over
//! something recorded — so they are one pass.
//!
//! Drawing them here costs a handful of shapes a frame. Re-recording
//! the mixer to show a hover would cost the whole mixer, which is the
//! thing the recorded-scene architecture exists to avoid: a hover is
//! the most frequent event a window gets.

use anyrender::PaintScene;
use daw_proto::Track;
use daw_theme_art::geometry::mcp as g;
use daw_theme_art::mixer_controls::Interaction;
use daw_theme_art::paint::tcp as art;
use vello::kurbo::Affine;

use crate::arrangement::Palette;
use crate::mcp::{Control, Mixer};
use crate::pointer::Spot;
use crate::text::Font;

/// The routing widget's state, from the track.
///
/// All three lanes are real now. Sends and receives used to draw as
/// unlit slots whatever the session did, because the counts were not on
/// `Track` and a strip cannot afford to ask the routing service per
/// track — so a session full of sends showed none, which is a lane
/// saying the opposite of the truth rather than saying nothing.
///
/// The widget wants to know WHETHER, not how many: it lights a lane. A
/// strip that drew the number would be drawing a thing that changes
/// under it for reasons it has no other way to see.
#[must_use]
pub const fn routes(track: &Track) -> art::Routing {
    art::Routing {
        parent_send: track.parent_send,
        sends: track.send_count > 0,
        receives: track.receive_count > 0,
    }
}

/// The colours the routing lanes light in.
///
/// The source art's own choices: the output lane is the accent, sends
/// the warn amber, receives the danger red.
#[must_use]
pub fn route_ink(palette: &Palette) -> art::RouteInk {
    art::RouteInk {
        out: crate::tcp::to_theme(palette.accent),
        send: crate::tcp::to_theme(palette.meter_warn),
        recv: crate::tcp::to_theme(palette.meter_danger),
    }
}

/// Everything the live pass needs about the racks.
///
/// Five parameters that travel together and mean nothing apart — the
/// settings a rack is drawn from, the levels and spectra that make one
/// move, the grip the pointer is on, and which panels the phase is
/// showing. Bundled because a function taking sixteen arguments is one
/// nobody can call correctly, not because they are conceptually one
/// thing.
pub struct Racks<'a> {
    pub settings: &'a crate::tone::Store,
    pub history: &'a mut std::collections::HashMap<String, crate::tone::Levels>,
    pub spectra: &'a mut std::collections::HashMap<String, crate::tone::Analyser>,
    /// The one grip the pointer is on, and the strip it is on.
    pub lit: Option<(usize, crate::tone::Grip)>,
    pub panels: &'a [crate::tone::Which],
    /// Which phases are folded shut, and whether that is one answer for
    /// the whole mixer or one per track.
    ///
    /// Synced is the default because the chain is read ACROSS: you fold
    /// Rescue away to compare everyone's Tone, and a mixer where each
    /// strip folded on its own would put a different processor at the
    /// same height on every track. Per-track is the setting for when
    /// you are working one track rather than comparing them.
    pub folded: &'a crate::tone::Fold,
}

impl Racks<'_> {
    /// Which phases are shut on a given track.
    fn folded(&self, guid: &str) -> crate::tone::Folded {
        self.folded.of(guid)
    }
}

impl Racks<'_> {
    /// No racks at all — for the measurement shots, which are of a
    /// mixer at rest.
    #[must_use]
    pub fn none() -> Racks<'static> {
        // Leaked once, for the life of the process, so a `&mut` can be
        // handed out without a caller having to own the maps. This is
        // only reached by the shot path, which runs once.
        static EMPTY_SETTINGS: std::sync::OnceLock<crate::tone::Store> = std::sync::OnceLock::new();
        static EMPTY_FOLD: std::sync::OnceLock<crate::tone::Fold> = std::sync::OnceLock::new();
        Racks {
            folded: EMPTY_FOLD.get_or_init(crate::tone::Fold::default),
            settings: EMPTY_SETTINGS.get_or_init(crate::tone::Store::default),
            history: Box::leak(Box::default()),
            spectra: Box::leak(Box::default()),
            lit: None,
            panels: &[],
        }
    }
}

/// What a track's FX button should say.
///
/// `fx_count` is the only thing the track model knows about a chain, so
/// this answers the only question it can: is there one. Bypass is a
/// property of the chain rather than of the track, and claiming
/// `Active` for a chain that is entirely bypassed would be a lit button
/// over a track doing nothing — which is the failure the empty pill was
/// avoiding by never lighting at all.
#[must_use]
pub const fn chain(track: &Track) -> art::Chain {
    if track.fx_count == 0 {
        art::Chain::Empty
    } else {
        art::Chain::Active
    }
}

/// Redraw one control in its hover or pressed state.
///
/// `transform` is the mixer's own — the same one the strips are
/// replayed under — so the overlay lands on top of the control it is
/// replacing rather than beside it.
pub fn control(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    mixer: &Mixer,
    track: &Track,
    spot: Spot,
    state: Interaction,
    rack_h: f64,
    transform: Affine,
) {
    let (Some((left, _, _)), Some(strip)) = (mixer.strip_box(spot.row), mixer.strip(spot.row))
    else {
        return;
    };
    // The SAME layout the strip was drawn from — the mixer's own
    // `Strip` for the row. This used to work its own positions out of
    // `Columns`, a `Collapse` and `buttons_top` — three recomputations
    // of what `Strip` already resolves once, and the hover cell drifted
    // away from the resting one every time the column moved.
    let _ = rack_h;
    let at = |control: Control| strip.rect(control).map(|r| (left + r.x0, r.y0));

    // Recorded in content space and replayed under the transform, so
    // the overlay records the same way and rides the same transform.
    let mut scene = anyrender::Scene::new();
    match spot.control {
        Control::Mute | Control::Solo => {
            let (label, on, lit) = if spot.control == Control::Mute {
                ("M", track.muted, crate::tcp::mute_lit(palette))
            } else {
                ("S", track.soloed, crate::tcp::solo_lit(palette))
            };
            if let Some(r) = strip.rect(spot.control) {
                crate::art::place(
                    &mut scene,
                    &art::gutter_button_sized(
                        &palette.chrome,
                        label,
                        on,
                        lit,
                        state,
                        (r.width(), r.height()),
                    ),
                    font,
                    left + r.x0,
                    r.y0,
                );
            }
        }
        Control::Monitor => {
            if let Some((x, y)) = at(Control::Monitor).filter(|_| track.armed) {
                crate::art::place(
                    &mut scene,
                    &art::monitor(
                        &palette.chrome,
                        monitoring(track),
                        monitor_lit(palette, monitoring(track)),
                        state,
                        art::Facing::Up,
                    ),
                    font,
                    x,
                    y,
                );
            }
        }
        Control::RecArm => {
            crate::art::place(
                &mut scene,
                &art::record_arm(
                    &palette.chrome,
                    crate::tcp::lit(palette).rec,
                    track.armed,
                    state,
                    strip.arm(),
                    crate::tcp::to_theme(crate::mcp::strip_ground(palette, track)),
                ),
                font,
                at(Control::RecArm).map_or(left, |(x, _)| x),
                at(Control::RecArm).map_or(0.0, |(_, y)| y),
            );
        }
        Control::Pan => {
            crate::art::place(
                &mut scene,
                &art::pan_knob(
                    &palette.chrome,
                    track.pan.clamp(-1.0, 1.0),
                    crate::tcp::to_theme(palette.pan),
                    state,
                ),
                font,
                at(Control::Pan).map_or(left, |(x, _)| x),
                at(Control::Pan).map_or(0.0, |(_, y)| y),
            );
        }
        Control::Fx => {
            crate::art::place(
                &mut scene,
                &art::fx_pill(
                    &palette.chrome,
                    crate::tcp::lit(palette),
                    chain(track),
                    state,
                ),
                font,
                left + 7.0,
                rack_h + f64::from(g::FX_PILL_TOP),
            );
        }
        // The fader, the name plate and the routing widget have no
        // hover cell in the traced art — REAPER does not light them
        // either. They are still hit-testable and still draggable; they
        // just do not change appearance under the pointer.
        //
        // Nor does the clip latch: it is drawn by the meter, which
        // knows whether it is lit, and a hover cell for it here would
        // be a second opinion about that.
        Control::Volume | Control::Name | Control::Routing | Control::Clip | Control::Folder => {}
    }

    for command in &scene.commands {
        crate::arrangement::submit_command(painter, command, transform);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_ui::studio::RowsRef;

    fn mixer() -> (Mixer, Palette, Font, Vec<Track>) {
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = Font::embedded().expect("the embedded font");
        let tracks: Vec<Track> = (0..4)
            .map(|i| Track {
                guid: format!("t{i}"),
                name: format!("Track {i}"),
                width: Some(133),
                ..Track::default()
            })
            .collect();
        let rows = RowsRef(std::sync::Arc::new(
            tracks.iter().cloned().map(|t| (t, 0)).collect(),
        ));
        let project =
            daw_ui::studio::ProjectRef(std::sync::Arc::new(daw_ui::studio::Project::default()));
        let mixer = Mixer::build(
            &palette,
            &font,
            &project,
            &rows,
            600.0,
            crate::layout::Layout::default(),
            &[],
            false,
            crate::settings::Settings::default(),
            &crate::tone::Store::default(),
        );
        (mixer, palette, font, tracks)
    }

    /// The same fixture, with the Tone rack on.
    fn racked(panels: &[crate::tone::Which]) -> (Mixer, Palette, Font, Vec<Track>) {
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = Font::embedded().expect("the embedded font");
        let tracks: Vec<Track> = (0..4)
            .map(|i| Track {
                guid: format!("t{i}"),
                name: format!("Track {i}"),
                width: Some(133),
                ..Track::default()
            })
            .collect();
        let rows = RowsRef(std::sync::Arc::new(
            tracks.iter().cloned().map(|t| (t, 0)).collect(),
        ));
        let project =
            daw_ui::studio::ProjectRef(std::sync::Arc::new(daw_ui::studio::Project::default()));
        let mut settings = crate::tone::Store::default();
        settings.seed(rows.as_slice());
        let mixer = Mixer::build(
            &palette,
            &font,
            &project,
            &rows,
            900.0,
            crate::layout::Layout::default(),
            panels,
            false,
            crate::settings::Settings::default(),
            &settings,
        );
        (mixer, palette, font, tracks)
    }

    /// The overlay draws SOMETHING for the controls that have a hover
    /// cell — otherwise hovering would do nothing at all and look like
    /// a dead control.
    #[test]
    fn the_lit_controls_draw() {
        let (mixer, palette, font, tracks) = mixer();
        for control in [Control::Mute, Control::Solo, Control::RecArm, Control::Fx] {
            let mut scene = anyrender::Scene::new();
            control_for(&mut scene, &palette, &font, &mixer, &tracks, control);
            assert!(
                !scene.commands.is_empty(),
                "{control:?} drew nothing when hovered"
            );
        }
    }

    /// And nothing for the ones REAPER does not light either. They stay
    /// hit-testable and draggable; they just do not change under the
    /// pointer, and drawing a highlight REAPER has no art for would be
    /// inventing one.
    #[test]
    fn the_unlit_controls_draw_nothing() {
        let (mixer, palette, font, tracks) = mixer();
        for control in [Control::Volume, Control::Name, Control::Routing] {
            let mut scene = anyrender::Scene::new();
            control_for(&mut scene, &palette, &font, &mixer, &tracks, control);
            assert!(
                scene.commands.is_empty(),
                "{control:?} invented a highlight"
            );
        }
    }

    /// The meter is the one control whose value does not come from the
    /// track, so it gets its own check: a level has to change what is
    /// drawn, or the subscription is feeding a picture nobody paints.
    #[test]
    fn a_level_lights_the_meter() {
        let (mixer, palette, font, tracks) = mixer();
        let strip = crate::strip::Strip::new(
            mixer.strip_box(1).expect("a strip").1,
            mixer.strip_box(1).expect("a strip").2,
            mixer.height,
            mixer.rack_h,
            mixer.buttons_top,
        );
        assert!(
            strip.meter_rect().is_some(),
            "this fixture must be wide enough to have a meter at all"
        );

        let map = crate::plan::Rows::of(
            &tracks.iter().cloned().map(|t| (t, 0)).collect::<Vec<_>>(),
            &tracks,
        );
        let draw = |levels: &[daw_proto::TrackLevels]| {
            let mut scene = anyrender::Scene::new();
            controls(
                &mut scene,
                &palette,
                &font,
                &mixer,
                &tracks,
                &map,
                &crate::pointer::Pointer::default(),
                levels,
                &Clips::default(),
                0.0,
                &mut Racks::none(),
                0.0,
                4000.0,
                Affine::IDENTITY,
            );
            scene
        };
        let quiet = draw(&[]);
        let loud = draw(&vec![
            daw_proto::TrackLevels {
                peak_left: 0.9,
                peak_right: 0.9,
                hold_left: 0.9,
                hold_right: 0.9,
            };
            tracks.len()
        ]);
        assert_ne!(quiet, loud, "a signal did not light the meter");
    }

    /// The fader says what it is set to WHILE it is being set.
    ///
    /// The numbers down the column are the meter's, so without this the
    /// one value you are actually changing is the one you have to infer
    /// from where the cap sits. Only while dragging: a permanent
    /// readout on forty strips is forty numbers nobody reads.
    #[test]
    fn the_fader_reads_out_while_it_is_dragged() {
        let (mixer, palette, font, tracks) = mixer();
        let map = crate::plan::Rows::of(
            &tracks.iter().cloned().map(|t| (t, 0)).collect::<Vec<_>>(),
            &tracks,
        );
        let draw = |pointer: &crate::pointer::Pointer| {
            let mut scene = anyrender::Scene::new();
            controls(
                &mut scene,
                &palette,
                &font,
                &mixer,
                &tracks,
                &map,
                pointer,
                &[],
                &Clips::default(),
                0.0,
                &mut Racks::none(),
                0.0,
                4000.0,
                Affine::IDENTITY,
            );
            scene
        };

        let idle = draw(&crate::pointer::Pointer::default());

        let spot = crate::pointer::Spot {
            row: 1,
            control: Control::Volume,
        };
        let mut hovering = crate::pointer::Pointer::default();
        hovering.hover(Some(spot));
        assert_eq!(
            draw(&hovering),
            idle,
            "hovering the fader drew a readout; only a drag should"
        );

        let mut dragging = hovering;
        dragging.press();
        assert_ne!(draw(&dragging), idle, "the drag drew no readout");
    }

    /// A chain lights the FX button. It used to be recorded as empty
    /// and stay empty for the life of the window, which made it the one
    /// control on the strip that could be wrong about the track it was
    /// attached to.
    #[test]
    fn a_chain_lights_the_fx_button() {
        let (mixer, palette, font, tracks) = mixer();
        let map = crate::plan::Rows::of(
            &tracks.iter().cloned().map(|t| (t, 0)).collect::<Vec<_>>(),
            &tracks,
        );
        let draw = |tracks: &[Track]| {
            let mut scene = anyrender::Scene::new();
            controls(
                &mut scene,
                &palette,
                &font,
                &mixer,
                tracks,
                &map,
                &crate::pointer::Pointer::default(),
                &[],
                &Clips::default(),
                0.0,
                &mut Racks::none(),
                0.0,
                4000.0,
                Affine::IDENTITY,
            );
            scene
        };
        let empty = draw(&tracks);
        let mut loaded = tracks.clone();
        for track in &mut loaded {
            track.fx_count = 3;
        }
        assert_ne!(empty, draw(&loaded), "a chain did not light the button");
        assert_eq!(chain(&tracks[0]), art::Chain::Empty);
        assert_eq!(chain(&loaded[0]), art::Chain::Active);
    }

    /// The routing widget says whether the track feeds its parent, and
    /// clicking it toggles that — so the two states have to look
    /// different, or the control is a button with no readout.
    #[test]
    fn the_parent_send_lane_shows_its_state() {
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        let draw = |parent_send: bool| {
            let track = Track {
                parent_send,
                ..Track::default()
            };
            let mut scene = anyrender::Scene::new();
            let drawing = art::routing(
                &palette.chrome,
                art::Axis::Vertical,
                routes(&track),
                route_ink(&palette),
                Interaction::Normal,
            );
            crate::art::place(
                &mut scene,
                &drawing,
                &Font::embedded().expect("the embedded font"),
                0.0,
                0.0,
            );
            scene
        };
        assert_ne!(draw(true), draw(false), "the lane does not show its state");
    }

    /// Levels reach the compressor's display. Without this the
    /// threshold line is a line across an empty box — the display is
    /// the whole reason the threshold moved off a knob.
    #[test]
    fn a_level_history_reaches_the_compressor() {
        // A mixer WITH a rack — the others deliberately have none, and
        // a display cannot be drawn into a panel that is not there.
        let panels = crate::tone::panels_for(session::mix_phases::MixPhase::Tone);
        let (mixer, palette, font, tracks) = racked(panels);
        let mut settings = crate::tone::Store::default();
        settings.seed(&tracks.iter().cloned().map(|t| (t, 0)).collect::<Vec<_>>());
        let map = crate::plan::Rows::of(
            &tracks.iter().cloned().map(|t| (t, 0)).collect::<Vec<_>>(),
            &tracks,
        );
        let draw = |history: &mut std::collections::HashMap<String, crate::tone::Levels>| {
            let mut scene = anyrender::Scene::new();
            controls(
                &mut scene,
                &palette,
                &font,
                &mixer,
                &tracks,
                &map,
                &crate::pointer::Pointer::default(),
                &[],
                &Clips::default(),
                0.0,
                &mut Racks {
                    folded: &crate::tone::Fold::default(),
                    settings: &settings,
                    history,
                    spectra: &mut std::collections::HashMap::new(),
                    lit: None,
                    panels,
                },
                0.0,
                4000.0,
                Affine::IDENTITY,
            );
            scene
        };
        let mut quiet = std::collections::HashMap::new();
        let silent = draw(&mut quiet);

        let mut loud = std::collections::HashMap::new();
        for track in &tracks {
            let entry: &mut crate::tone::Levels = loud.entry(track.guid.clone()).or_default();
            for i in 0..40 {
                entry.push(0.2 + 0.6 * ((i % 7) as f32 / 7.0));
            }
        }
        assert_ne!(silent, draw(&mut loud), "the display stayed empty");
    }

    /// A row that is not there draws nothing rather than panicking —
    /// the pointer can outlive a project reload by a frame.
    #[test]
    fn a_stale_row_is_ignored() {
        let (mixer, palette, font, tracks) = mixer();
        let mut scene = anyrender::Scene::new();
        control(
            &mut scene,
            &palette,
            &font,
            &mixer,
            &tracks[0],
            Spot {
                row: 999,
                control: Control::Mute,
            },
            Interaction::Hover,
            0.0,
            Affine::IDENTITY,
        );
        assert!(scene.commands.is_empty());
    }

    fn control_for(
        scene: &mut anyrender::Scene,
        palette: &Palette,
        font: &Font,
        mixer: &Mixer,
        tracks: &[Track],
        which: Control,
    ) {
        control(
            scene,
            palette,
            font,
            mixer,
            &tracks[1],
            Spot {
                row: 1,
                control: which,
            },
            Interaction::Hover,
            0.0,
            Affine::IDENTITY,
        );
    }
}

/// Every control whose VALUE can change, drawn live for the strips on
/// screen.
///
/// The recorded strip holds what does not move: its ground, its colour
/// bands, its name, its rack, the fader's groove. What moves — the
/// fader's cap and lit travel, the pan knob's pointer, mute, solo, the
/// record arm — is drawn here, from the tracks as they are NOW.
///
/// This is what makes a parameter a reflection of state rather than a
/// picture taken at project open. Re-recording the mixer to show a
/// changed mute would cost the whole mixer for one rectangle; drawing
/// the controls live costs a handful of shapes per VISIBLE strip, which
/// is a few dozen strips however large the session is.
///
/// The division is the same one the profiling found worth making
/// everywhere in this app: record what is expensive and constant,
/// re-emit what is cheap and varying.
pub fn controls(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    mixer: &Mixer,
    tracks: &[Track],
    live: &crate::plan::Rows,
    pointer: &crate::pointer::Pointer,
    levels: &[daw_proto::TrackLevels],
    clipped: &Clips,
    rack_scroll: f64,
    racks: &mut Racks<'_>,
    scroll_x: f64,
    width: f64,
    transform: Affine,
) -> crate::profile::Counts {
    let mut counts = crate::profile::Counts::default();
    let mut scene = anyrender::Scene::new();
    for row in mixer.visible(scroll_x, width) {
        let Some(track) = live.live(tracks, row) else {
            continue;
        };
        let Some((left, strip_w, strip_h)) = mixer.strip_box(row) else {
            continue;
        };
        draw_strip_controls(
            &mut scene,
            palette,
            font,
            track,
            mixer.label(row),
            rack_scroll,
            racks.folded(&track.guid),
            pointer,
            // Levels are indexed by PROJECT track index, not by mixer
            // row: the mixer shows a subset in its own order, and a
            // meter reading another track's level is worse than one
            // reading none.
            usize::try_from(track.index)
                .ok()
                .and_then(|i| levels.get(i))
                .copied(),
            clipped.is(&track.guid),
            racks.history.get_mut(&track.guid),
            racks.spectra.get_mut(&track.guid),
            racks.lit.filter(|(at, _)| *at == row).map(|(_, grip)| grip),
            racks.settings.get(&track.guid),
            racks.panels,
            row,
            left,
            strip_w,
            strip_h,
            mixer.rack_h,
            mixer.buttons_top,
            mixer.height,
            mixer.live,
            mixer.touch,
            mixer.themed(row).is_some(),
        );
        // A theme's strip: its live parts, over its recording.
        if let Some(theme) = mixer.themed(row) {
            let on = |spot: Option<Spot>| spot.filter(|s| s.row == row).map(|s| s.control);
            let fields = crate::skin::fields(
                palette,
                track,
                crate::skin::Shown {
                    level: usize::try_from(track.index)
                        .ok()
                        .and_then(|i| levels.get(i))
                        .copied(),
                    clipped: clipped.is(&track.guid),
                    hover: on(pointer.hovered()),
                    pressed: on(pointer.pressed()),
                    ..mixer.shown(row, font)
                },
            );
            theme.live(
                crate::skin::STRIP,
                (strip_w, strip_h - mixer.rack_h),
                &fields,
                &mut crate::skin::canvas::Host {
                    scene: &mut scene,
                    font,
                },
                Affine::translate((left, mixer.rack_h)),
            );
        }
        // The racks of the strips that are not selected, darkened —
        // over the recording and everything live on it, so the
        // selected one's is the bright one. The rack only: the strip's
        // own controls are the mixer's grey already and stay readable
        // across every track. A setting, and off when the mixer has no
        // selection at all: then there is nothing to be brighter than.
        let dim = crate::layout::dim_unselected();
        if dim > 0.0 && mixer.rack_h > 0.0 && !track.selected && live.any_selected(tracks) {
            scene.fill(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                vello::peniko::Color::from_rgba8(0, 0, 0, 0xff).multiply_alpha(dim),
                None,
                &vello::kurbo::Rect::new(left, 0.0, left + strip_w, mixer.rack_h),
            );
        }
    }
    for command in &scene.commands {
        counts.replayed = counts.replayed.saturating_add(1);
        if crate::arrangement::submit_command(painter, command, transform) {
            counts.submitted = counts.submitted.saturating_add(1);
        }
    }
    counts
}

#[expect(
    clippy::too_many_arguments,
    reason = "one strip's worth of geometry and state; grouping it into a \
              struct would be a struct that exists for this call alone"
)]
fn draw_strip_controls(
    scene: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    track: &Track,
    label: Option<(&str, f32)>,
    scroll: f64,
    folded: crate::tone::Folded,
    pointer: &crate::pointer::Pointer,
    level: Option<daw_proto::TrackLevels>,
    clipped: bool,
    history: Option<&mut crate::tone::Levels>,
    spectrum: Option<&mut crate::tone::Analyser>,
    lit: Option<crate::tone::Grip>,
    settings: Option<&crate::tone::Tone>,
    panels: &[crate::tone::Which],
    row: usize,
    left: f64,
    width: f64,
    height: f64,
    rack_h: f64,
    buttons_top: f64,
    mixer_h: f64,
    live: bool,
    touch: bool,
    // A theme draws the strip's own controls: only the rack is drawn here.
    themed: bool,
) {
    let strip = crate::strip::Strip::laid_out(
        width,
        height,
        mixer_h,
        rack_h,
        buttons_top,
        settings.is_some_and(crate::tone::Tone::wants_column),
        live,
    )
    .touched(touch);
    // Every position comes from the layout, translated by the strip's
    // left edge. Nothing here works out where a control goes.
    let at = |control| strip.rect(control).map(|r| (left + r.x0, r.y0));
    let state = |control| pointer.state(Spot { row, control });

    if !themed {
        if let Some((x, y)) = at(Control::Pan) {
            crate::art::place(
                scene,
                &art::pan_knob(
                    &palette.chrome,
                    track.pan.clamp(-1.0, 1.0),
                    crate::tcp::to_theme(palette.pan),
                    state(Control::Pan),
                ),
                font,
                x,
                y,
            );
        }

        // The FX button, whose state is the track's chain count — a live
        // value like any other, and one that used to be recorded as Empty
        // and stay Empty for the life of the window.
        // Across the strip: a wide pill, the label centred, not the track
        // panel's small one pinned to the left.
        if let Some(r) = strip.rect(Control::Fx) {
            crate::art::place(
                scene,
                &art::fx_pill_wide(
                    &palette.chrome,
                    crate::tcp::lit(palette),
                    chain(track),
                    state(Control::Fx),
                    r.width(),
                ),
                font,
                left + r.x0,
                r.y0,
            );
        }

        // Routing, which used to be recorded — and recorded four pixels
        // above where the hit test looked for it, because the two resolved
        // against different lines. One layout, one position.
        if let Some((x, y)) = at(Control::Routing) {
            crate::art::place(
                scene,
                &art::routing(
                    &palette.chrome,
                    // The mixer stacks the lanes; the track panel sets them
                    // in a row. That is the whole difference between the
                    // theme's two routing images.
                    art::Axis::Vertical,
                    routes(track),
                    route_ink(palette),
                    state(Control::Routing),
                ),
                font,
                x,
                y,
            );
        }

        if let Some((x, y)) = at(Control::RecArm) {
            crate::art::place(
                scene,
                &art::record_arm(
                    &palette.chrome,
                    crate::tcp::lit(palette).rec,
                    track.armed,
                    state(Control::RecArm),
                    strip.arm(),
                    crate::tcp::to_theme(crate::mcp::strip_ground(palette, track)),
                ),
                font,
                x,
                y,
            );
        }

        // Input monitoring, directly OVER the arm, in the band — the two
        // are one decision made twice, what the track records and whether
        // you hear it while it does, so they stack; and over rather than
        // under so the column below the arm is mute, solo, routing with
        // no gap where a lamp is not lit.
        //
        // Only on an armed track: monitoring is a fact about recording,
        // and a lamp for it on a track that is not recording is a lamp
        // with nothing to report.
        if let Some((x, y)) = at(Control::Monitor).filter(|_| track.armed) {
            crate::art::place(
                scene,
                &art::monitor(
                    &palette.chrome,
                    monitoring(track),
                    monitor_lit(palette, monitoring(track)),
                    state(Control::Monitor),
                    art::Facing::Up,
                ),
                font,
                x,
                y,
            );
        }

        for (control, label, on, lit) in [
            (
                Control::Mute,
                "M",
                track.muted,
                crate::tcp::mute_lit(palette),
            ),
            (
                Control::Solo,
                "S",
                track.soloed,
                crate::tcp::solo_lit(palette),
            ),
        ] {
            // At the size the strip gives it: a touchscreen's are bigger.
            let Some(r) = strip.rect(control) else {
                continue;
            };
            crate::art::place(
                scene,
                &art::gutter_button_sized(
                    &palette.chrome,
                    label,
                    on,
                    lit,
                    state(control),
                    (r.width(), r.height()),
                ),
                font,
                left + r.x0,
                r.y0,
            );
        }
    }

    // A rack that is MOVING is drawn whole, live, over its own
    // recording — the panel grounds are opaque, so it covers rather
    // than composites. A spectrum moves with the audio and a grip moves
    // with the hand; either makes the recording stale, and both are
    // rare enough that the rest of the mixer stays in the cheap path.
    //
    // Only where the rack shows something that moves at this size: a
    // panel wide enough for its analyser. A rail-row or curves-only rack
    // stays its recording — redrawn on every publish it cost a whole rack
    // per strip per frame, and a mixer of narrow strips a hundred racks a
    // frame for nothing anyone could see change.
    if let (Some(box_), Some(tone)) = (strip.rack_rect(), settings)
        && (lit.is_some() || (spectrum.is_some() && crate::tone::Rack::at(box_.width()).detailed()))
    {
        // Clipped to the box, because the chain is longer than the box
        // and scrolls inside it: without this the panels above the
        // scroll paint into the rails and the ones below paint over the
        // strip's own controls.
        scene.push_clip_layer(
            Affine::IDENTITY,
            &vello::kurbo::Rect::new(left + box_.x0, box_.y0, left + box_.x1, box_.y1),
        );
        // The scroll is the panel's own `y` — see `tone::layout`. One
        // offset, applied where the rack is placed, so the drawing and
        // the hit test cannot hold different opinions about it.
        let at = crate::tone::Panel::of(box_, left).up(scroll);
        let paint = |meters: &crate::live::Meters| {
            let mut rack = anyrender::Scene::new();
            crate::tone::draw(
                &mut rack,
                palette,
                font,
                tone,
                meters,
                tone.panels(panels),
                at,
                folded,
                lit,
                crate::mcp::rack_ground(palette, track),
            );
            rack
        };
        match spectrum {
            // A lit strip is rebuilt every frame on purpose: the grip
            // under the pointer moves with the hand, and there is only
            // ever one of them.
            Some(analyser) if lit.is_none() => {
                let meters = analyser.meters().clone();
                let cached = analyser.rack(at.width, at.height, || paint(&meters));
                scene.commands.extend_from_slice(&cached.commands);
            }
            Some(analyser) => {
                let meters = analyser.meters().clone();
                scene.commands.extend(paint(&meters).commands);
            }
            None => scene
                .commands
                .extend(paint(&crate::live::Meters::default()).commands),
        }
        scene.pop_layer();
    }

    // The compressor's level history, under the threshold line drawn
    // across it. Live, because it is the one part of a rack that
    // changes with the audio — see `tone::levels`.
    //
    // Clipped to the same box and moved by the same scroll as the rack
    // it belongs to. It is a second pass over the same panels, and a
    // pass that skipped either would draw a compressor's levels where
    // its compressor is not.
    if let (Some(box_), Some(history), Some(tone)) = (strip.rack_rect(), history, settings) {
        scene.push_clip_layer(
            Affine::IDENTITY,
            &vello::kurbo::Rect::new(left + box_.x0, box_.y0, left + box_.x1, box_.y1),
        );
        crate::tone::levels(
            scene,
            tone.panels(panels),
            history,
            // The reduction is what each compressor is DOING, so it is
            // computed from the settings it is doing it with — which is
            // why the whole `Tone` goes in rather than one `Comp`.
            tone,
            crate::tone::Panel::of(box_, left).up(scroll),
            folded,
        );
        scene.pop_layer();
    }

    // The meter, which is the most live thing on the strip: thirty
    // frames a second of it, and the only control here whose value does
    // not come from the track at all.
    //
    // Drawn whole rather than as a lit column over a recorded well —
    // the well is one rounded rectangle, and keeping the two halves in
    // separate passes is how a meter ends up lit past its own edge when
    // the strip resizes.
    // The fader IS the meter: its groove carries the signal, and the
    // cap rides over it as glass so the level reads through the one
    // The track's name, live, because its ink answers to three things
    // that change while the window is open — selection, mute and solo —
    // and re-recording a mixer to dim one name is the trade this
    // architecture exists to refuse. The plate under it is recorded;
    // only the text on it is not.
    if themed {
        return;
    }
    if let (Some((text, size)), Some(plate)) = (label, strip.rect(Control::Name)) {
        name(scene, palette, font, track, text, size, left, plate);
    }

    // place it was always hidden — right where the hand is, which on a
    // loud track is right where the peak is.
    //
    // One column instead of two, which is what makes this fit a narrow
    // strip. It used to be a meter beside a fader, and on an 86-wide
    // strip there was no room for the meter, so there simply was none.
    if strip.has_fader()
        && let Some((x, y)) = at(Control::Volume)
    {
        let travel = strip.travel();
        // Each channel on its own half, not the louder of the two and
        // not their sum: a summed meter cannot tell you that a stereo
        // source has collapsed to one side or that one leg of a pair is
        // dead, and those are two of the things a meter is watched for.
        let peak = level.map_or((0.0, 0.0), |level| {
            (
                crate::engine::meter_fraction(level.peak_left),
                crate::engine::meter_fraction(level.peak_right),
            )
        });
        // Where the signal HAS been. The bar is sampled thirty times a
        // second and cannot show a transient; this is the reading that
        // answers "did that hit".
        let hold = level.map_or((0.0, 0.0), |level| {
            (
                crate::engine::meter_fraction(level.hold_left),
                crate::engine::meter_fraction(level.hold_right),
            )
        });
        // The scale, against the meter and lighting with it.
        if let Some(scale) = strip.scale_rect() {
            crate::art::place(
                scene,
                &art::fader_scale(
                    scale.width(),
                    scale.height(),
                    crate::tcp::to_theme(palette.text_faint),
                    [
                        crate::tcp::to_theme(palette.meter_safe),
                        crate::tcp::to_theme(palette.meter_warn),
                        crate::tcp::to_theme(palette.meter_danger),
                    ],
                    8.0,
                    peak.0.max(peak.1),
                ),
                font,
                left + scale.x0,
                scale.y0,
            );
        }
        crate::art::place(
            scene,
            &art::fader_track(
                &palette.chrome,
                peak,
                hold,
                clipped,
                [
                    crate::tcp::to_theme(palette.meter_safe),
                    crate::tcp::to_theme(palette.meter_warn),
                    crate::tcp::to_theme(palette.meter_danger),
                ],
                strip.columns.fader_w,
                travel,
            ),
            font,
            x,
            y,
        );
        // The cap, centred in the column and narrower than the meter,
        // so a channel shows down each side of it whatever it covers.
        // Where `Strip::cap` puts it, which is also where a finger takes
        // hold of it.
        let Some(cap) = strip.cap(track.volume) else {
            return;
        };
        let (cap_y, cap_h) = (cap.y0 - y, cap.height());
        crate::art::scaled(
            scene,
            &art::fader_cap_through(&palette.chrome, palette.chrome.hardware_mark, CAP_GLASS),
            font,
            left + cap.x0,
            cap.y0,
            cap_h / 53.0,
        );
        // What the fader is SET to, while it is being set. The numbers
        // beside the column are the meter's, so without this the one
        // value you are actually changing is the one you have to infer
        // from where the cap sits. Only while dragging: a permanent
        // readout on forty strips is forty numbers nobody is reading.
        if pointer.state(crate::pointer::Spot {
            row,
            control: Control::Volume,
        }) == daw_theme_art::mixer_controls::Interaction::Pressed
        {
            readout(
                scene,
                palette,
                font,
                track.volume,
                x,
                y,
                strip.columns.fader_w,
                cap_y,
                cap_h,
            );
        }
        // And the level again where the cap crosses it, so the column
        // is continuous rather than interrupted at the one height you
        // are looking at.
        crate::art::place(
            scene,
            &art::fader_through(
                peak,
                [
                    crate::tcp::to_theme(palette.meter_safe),
                    crate::tcp::to_theme(palette.meter_warn),
                    crate::tcp::to_theme(palette.meter_danger),
                ],
                strip.columns.fader_w,
                travel,
                cap_y,
                cap_h,
            ),
            font,
            x,
            y,
        );
    }
}

/// Which tracks have clipped since anyone last looked.
///
/// By GUID rather than by index, for the reason everything per-track
/// here is: a preset can hide a track and a fold can move one, and a
/// clip latch that followed a row would report the neighbour's fault.
///
/// A latch rather than a reading: a peak-hold decays, which is right
/// for reading a level and wrong for reporting one. The whole value of
/// "this clipped" is that it is still saying so when you look up.
#[derive(Clone, Debug, Default)]
pub struct Clips {
    by_guid: std::collections::HashSet<String>,
}

impl Clips {
    /// Note whatever this frame's levels did.
    ///
    /// Linear 1.0 is 0 dBFS, which is where the sample ran out of
    /// numbers. Anything at or above it is an over.
    pub fn note(&mut self, guid: &str, level: daw_proto::TrackLevels) {
        let over = level.peak_left.max(level.peak_right) >= 1.0;
        if over && !self.by_guid.contains(guid) {
            self.by_guid.insert(guid.to_owned());
        }
    }

    #[must_use]
    pub fn is(&self, guid: &str) -> bool {
        self.by_guid.contains(guid)
    }

    /// Clear one track's latch — what clicking it does.
    ///
    /// Returns whether there was anything to clear, so a click that
    /// lands on an unlit latch can fall through to the fader under it
    /// rather than being swallowed.
    pub fn clear(&mut self, guid: &str) -> bool {
        self.by_guid.remove(guid)
    }

    /// And all of them, which is what a desk's "reset peaks" does.
    pub fn clear_all(&mut self) {
        self.by_guid.clear();
    }

    #[must_use]
    pub fn any(&self) -> bool {
        !self.by_guid.is_empty()
    }
}

#[cfg(test)]
mod clip_tests {
    use super::Clips;

    fn level(peak: f32) -> daw_proto::TrackLevels {
        daw_proto::TrackLevels {
            peak_left: peak,
            peak_right: peak * 0.5,
            hold_left: peak,
            hold_right: peak,
        }
    }

    /// A latch, not a reading: it is set by one frame reaching 0 dBFS
    /// and stays set through every quiet frame after it. That is the
    /// whole value of it — a peak-hold decays, and a fault you only see
    /// if you were looking at the right moment is a fault you miss.
    #[test]
    fn a_clip_stays_until_it_is_cleared() {
        let mut clips = Clips::default();
        assert!(!clips.is("kick"));
        clips.note("kick", level(0.9));
        assert!(!clips.is("kick"), "a hot signal is not a clip");

        clips.note("kick", level(1.0));
        assert!(clips.is("kick"), "0 dBFS did not latch");
        for _ in 0..100 {
            clips.note("kick", level(0.01));
        }
        assert!(clips.is("kick"), "the latch decayed");

        assert!(clips.clear("kick"), "clearing reported nothing to clear");
        assert!(!clips.is("kick"));
    }

    /// Clearing an unlit latch reports that there was nothing there, so
    /// the click can fall through to the fader the band sits on — which
    /// is what makes the latch cost the fader no travel.
    #[test]
    fn an_unlit_latch_does_not_swallow_the_click() {
        let mut clips = Clips::default();
        assert!(!clips.clear("kick"));
    }

    /// One track's clip is not another's.
    #[test]
    fn a_latch_belongs_to_its_own_track() {
        let mut clips = Clips::default();
        clips.note("kick", level(1.2));
        clips.note("snare", level(0.3));
        assert!(clips.is("kick"));
        assert!(!clips.is("snare"));
        assert!(clips.any());
        clips.clear_all();
        assert!(!clips.any());
    }
}

/// A track's input-monitoring mode, as the art's own three states.
///
/// Mapped here so the drawing does not have to know what a `daw_proto`
/// track is — the art speaks in what it draws, not in what the engine
/// calls it.
/// The colour the monitor lamp lights in.
///
/// Not the arm's red: the arm already says "recording", and a second
/// red lamp under it read as a second arm. Through is a light grey —
/// a lamp that is on — and the tape-style mode, which is the one you
/// set deliberately, is the one that gets a colour.
fn monitor_lit(palette: &Palette, mode: art::Monitoring) -> daw_theme::Color {
    match mode {
        art::Monitoring::NotWhenPlaying => TAPE,
        art::Monitoring::Off | art::Monitoring::Normal => crate::tcp::to_theme(palette.text),
    }
}

/// The tape-style monitoring colour: an orange, so it is neither the
/// arm's red nor the grey of monitoring straight through.
const TAPE: daw_theme::Color = daw_theme::Color {
    r: 0xf0,
    g: 0x8c,
    b: 0x2e,
    a: 0xff,
};

fn monitoring(track: &Track) -> art::Monitoring {
    use daw_proto::track::InputMonitoringMode as M;
    match track.input_monitor {
        M::Off => art::Monitoring::Off,
        M::Normal => art::Monitoring::Normal,
        M::NotWhenPlaying => art::Monitoring::NotWhenPlaying,
    }
}

/// The track's name on its plate.
///
/// Centred, because a plate is a nameplate and a name pinned to the
/// left of a wide one sits in the corner of an empty box.
///
/// The ink carries three states the name is the biggest target for.
/// Muted dims it towards the plate it sits on — a muted track is one
/// you are not hearing, and it should look like it from across forty
/// strips rather than requiring you to find a ten-pixel button. Soloed
/// takes the solo colour, because a solo is the loudest thing about a
/// session and the name should say so. Selected takes full text and an
/// accent rule along the plate's top edge, which marks the whole strip
/// below it rather than just the label.
fn name(
    scene: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    track: &Track,
    text: &str,
    size: f32,
    left: f64,
    plate: vello::kurbo::Rect,
) {
    if track.selected {
        crate::art::place(
            scene,
            &art::name_selected(
                crate::tcp::to_theme(palette.accent),
                plate.width() - 4.0,
                plate.height(),
            ),
            font,
            left + plate.x0 + 2.0,
            plate.y0,
        );
    }
    // Solo before mute: a track can be both, and what you are hearing
    // is decided by the solo.
    let ink = if track.soloed {
        palette.solo
    } else if track.muted {
        palette.text_faint
    } else if track.selected {
        palette.text
    } else {
        palette.text_dim
    };
    let width = font.width(text, size);
    crate::tcp::glyphs(
        scene,
        font,
        ink,
        text,
        left + plate.x0 + (plate.width() - width) / 2.0,
        plate.y0 + plate.height() / 2.0 + f64::from(size) / 3.0,
        size,
    );
}

/// The fader's own value, in decibels, beside the cap that is setting
/// it.
///
/// Placed against the cap rather than at a fixed height, because the
/// thing it labels moves — and on the side away from the scale, so it
/// cannot be read as one of the meter's marks.
#[expect(
    clippy::too_many_arguments,
    reason = "a placement, and every part of it is one"
)]
fn readout(
    scene: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    volume: f64,
    x: f64,
    y: f64,
    fader_w: f64,
    cap_y: f64,
    cap_h: f64,
) {
    use vello::kurbo::Rect;
    use vello::peniko::Fill;

    let label = volume_label(volume);
    const SIZE: f32 = 9.0;
    let width = font.width(&label, SIZE) + 6.0;
    let height = f64::from(SIZE) + 4.0;
    let left = x + fader_w + 3.0;
    let top = (y + cap_y + cap_h / 2.0 - height / 2.0).max(y);
    scene.fill(
        Fill::NonZero,
        vello::kurbo::Affine::IDENTITY,
        palette.tcp_field,
        None,
        &Rect::new(left, top, left + width, top + height),
    );
    crate::tcp::glyphs(
        scene,
        font,
        palette.text,
        &label,
        left + 3.0,
        top + height - 4.0,
        SIZE,
    );
}

/// The fader's value as its readout prints it.
#[must_use]
pub fn volume_label(volume: f64) -> String {
    let db = art::fader_db(crate::tcp::volume_fraction(volume));
    // Silence has no logarithm and should not be given a number that
    // implies one.
    if volume <= 0.0 {
        "-inf".to_owned()
    } else if db >= 0.0 {
        format!("+{db:.1}")
    } else {
        format!("{db:.1}")
    }
}

/// How clear the window in the fader cap is.
///
/// Nearly all the way: the ring around it is solid and does not fade at
/// any setting, so the cap keeps its edges however bright the meter
/// behind it — and the middle can therefore afford to be glass rather
/// than a tint. See `paint::fader_cap_through`.
const CAP_GLASS: f64 = 0.88;

// A rack that moves is drawn by `controls`, over its own recording,
// alongside the meters and the level traces — one pass over the visible
// strips rather than a second one for the racks. It used to be its own
// function for the dragged strip alone, which stopped being a special
// case the moment a spectrum could move one too.

/// The track panel's live controls, for the rows on screen.
///
/// The mixer's counterpart is [`controls`]; this is the same division
/// applied to the other panel. The recorded row holds its ground, its
/// rail, its colours and its name; the values — mute, solo, arm, volume
/// and pan — are drawn per frame from the tracks as they are now.
pub fn panel_controls(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    scene: &crate::arrangement::Arrangement,
    rows: &[(Track, u32)],
    tracks: &[Track],
    map: &crate::plan::Rows,
    view: crate::arrangement::Viewport,
    pointer: &crate::pointer::Pointer<crate::pointer::RowSpot>,
    transform: Affine,
) -> crate::profile::Counts {
    let mut counts = crate::profile::Counts::default();
    let mut out = anyrender::Scene::new();
    for index in scene.visible_rows(view) {
        control_row(
            &mut out, palette, font, scene, rows, tracks, map, view, pointer, index,
        );
    }
    for command in &out.commands {
        counts.replayed = counts.replayed.saturating_add(1);
        if crate::arrangement::submit_command(painter, command, transform) {
            counts.submitted = counts.submitted.saturating_add(1);
        }
    }
    counts
}

/// How thick a name field's meter strip is, at most.
const STRIP: f64 = 2.5;

/// Each visible track's level, drawn IN its name field: one thin strip
/// low in the field, under the name, where it crosses nothing. Colour and
/// length only — the mixer's zones (safe, warm, hot, over, at
/// `METER_ZONES`) in flat segments, no scale and no well: at a glance
/// across forty rows, which are playing and which are hot is what an
/// arrangement meter is for. The louder channel is what it shows; the
/// mixer is where a pair is read apart.
///
/// Returns whether any strip is lit, so a caller knows to keep redrawing
/// while levels fall.
#[must_use]
pub fn row_meters(
    painter: &mut impl PaintScene,
    palette: &Palette,
    scene: &crate::arrangement::Arrangement,
    rows: &[(Track, u32)],
    tracks: &[Track],
    map: &crate::plan::Rows,
    view: crate::arrangement::Viewport,
    levels: &[daw_proto::TrackLevels],
    transform: Affine,
) -> bool {
    use crate::row::{Control, Row};
    use daw_theme_art::paint::tcp::METER_ZONES;

    let mut lit = false;
    // The zone boundaries along a strip, as fractions of its length.
    let edges = METER_ZONES.map(|db| daw_theme_art::paint::tcp::meter_norm(db));
    let colors = [
        palette.meter_safe,
        palette.meter_warn,
        palette.meter_danger,
        palette.meter_danger,
    ];
    for index in scene.visible_rows(view) {
        let (Some((track, depth)), Some(live)) = (rows.get(index), map.live(tracks, index)) else {
            continue;
        };
        let Some(level) = usize::try_from(live.index).ok().and_then(|i| levels.get(i)) else {
            continue;
        };
        let Some((top, height)) = scene.row_band(index, view) else {
            continue;
        };
        let row = Row::new(
            top,
            height,
            i32::try_from(*depth).unwrap_or(0),
            track.is_folder,
            scene.tcp,
        );
        let Some(field) = row.rect(Control::Name) else {
            continue;
        };
        let thick = STRIP.min(field.height() / 6.0);
        // In the lower half, a hair inside the field's bottom edge.
        let y0 = (field.y1 - thick - 1.0).max(field.y0 + field.height() / 2.0);
        let peak = level.peak_left.max(level.peak_right);
        let fraction = crate::engine::meter_fraction(peak).clamp(0.0, 1.0);
        if fraction <= 0.0 || peak <= 1e-5 {
            continue;
        }
        lit = true;
        let length = field.width() * fraction;
        let mut from = 0.0;
        for (zone, color) in colors.iter().enumerate() {
            let to = edges
                .get(zone)
                .map_or(length, |edge| (field.width() * edge).min(length));
            if to > from {
                painter.fill(
                    vello::peniko::Fill::NonZero,
                    transform,
                    *color,
                    None,
                    &vello::kurbo::Rect::new(field.x0 + from, y0, field.x0 + to, y0 + thick),
                );
                from = to;
            }
            if from >= length {
                break;
            }
        }
    }
    lit
}

/// Every row's live controls, recorded once with a command range per
/// row.
///
/// Recorded for ALL rows rather than the visible ones, which is the
/// whole point: a recording keyed to what is on screen is thrown away
/// by a scroll, and scrolling is when a window can least afford to
/// rebuild forty rows of vector art. Keyed per row instead, a scroll
/// replays a different span of the same recording.
///
/// What it is NOT keyed by is the pointer. Hover is one control drawn
/// again on top — see `crate::pointer` — so a recording taken at rest
/// stays good while the mouse moves across it.
#[must_use]
pub fn record_controls(
    palette: &Palette,
    font: &Font,
    scene: &crate::arrangement::Arrangement,
    rows: &[(Track, u32)],
    tracks: &[Track],
    map: &crate::plan::Rows,
    view: crate::arrangement::Viewport,
) -> Controls {
    let mut out = anyrender::Scene::new();
    let mut spans = Vec::with_capacity(rows.len());
    let at_rest = crate::pointer::Pointer::default();
    for index in 0..rows.len() {
        let from = command_index(&out);
        control_row(
            &mut out, palette, font, scene, rows, tracks, map, view, &at_rest, index,
        );
        spans.push(from..command_index(&out));
    }
    Controls { scene: out, spans }
}

/// One cut of the live controls.
pub struct Controls {
    scene: anyrender::Scene,
    spans: Vec<core::ops::Range<u32>>,
}

impl Controls {
    /// Replay the rows a viewport can see.
    pub fn replay(
        &self,
        painter: &mut impl PaintScene,
        scene: &crate::arrangement::Arrangement,
        view: crate::arrangement::Viewport,
        transform: Affine,
    ) -> crate::profile::Counts {
        let mut counts = crate::profile::Counts::default();
        for row in scene.visible_rows(view) {
            let Some(span) = self.spans.get(row) else {
                continue;
            };
            let (from, to) = (span.start as usize, span.end as usize);
            let Some(commands) = self.scene.commands.get(from..to) else {
                continue;
            };
            for command in commands {
                counts.replayed = counts.replayed.saturating_add(1);
                if crate::arrangement::submit_command(painter, command, transform) {
                    counts.submitted = counts.submitted.saturating_add(1);
                }
            }
        }
        counts
    }
}

/// How many commands a scene holds, as the index of the next one.
fn command_index(scene: &anyrender::Scene) -> u32 {
    u32::try_from(scene.commands.len()).unwrap_or(u32::MAX)
}

/// One row's controls again, over the recording, so the control under
/// the pointer can show it.
///
/// The recording is taken at rest — see [`record_controls`] — which is
/// what lets it survive a mouse move. The cost of that is that a hover
/// has to be painted on top, and this is it: the whole row, redrawn,
/// with the pointer's verdict applied. One row of forty, on the frames
/// where the pointer is on one at all.
pub fn hovered_row(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    scene: &crate::arrangement::Arrangement,
    rows: &[(Track, u32)],
    tracks: &[Track],
    map: &crate::plan::Rows,
    view: crate::arrangement::Viewport,
    pointer: &crate::pointer::Pointer<crate::pointer::RowSpot>,
    transform: Affine,
) -> crate::profile::Counts {
    let mut counts = crate::profile::Counts::default();
    let Some(row) = pointer.active().map(|(spot, _)| spot.row) else {
        return counts;
    };
    let mut out = anyrender::Scene::new();
    control_row(
        &mut out, palette, font, scene, rows, tracks, map, view, pointer, row,
    );
    for command in &out.commands {
        counts.replayed = counts.replayed.saturating_add(1);
        if crate::arrangement::submit_command(painter, command, transform) {
            counts.submitted = counts.submitted.saturating_add(1);
        }
    }
    counts
}

/// One row's controls, into `out`.
#[expect(
    clippy::too_many_arguments,
    reason = "the loop body of `panel_controls`, lifted out so a row that draws nothing can return"
)]
fn control_row(
    out: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    scene: &crate::arrangement::Arrangement,
    rows: &[(Track, u32)],
    tracks: &[Track],
    map: &crate::plan::Rows,
    view: crate::arrangement::Viewport,
    pointer: &crate::pointer::Pointer<crate::pointer::RowSpot>,
    index: usize,
) {
    use crate::pointer::RowSpot;
    use crate::row::{Control as C, Indicator, Row};
    use daw_theme_art::geometry::tcp as gt;

    let (Some((track, depth)), Some(live)) = (rows.get(index), map.live(tracks, index)) else {
        return;
    };
    // The band on SCREEN, not in the session: these are drawn
    // under a translate so that the art keeps its own size, which
    // leaves the placing to `row_band`. See it for why.
    let Some((top, height)) = scene.row_band(index, view) else {
        return;
    };
    let row = Row::new(
        top,
        height,
        i32::try_from(*depth).unwrap_or(0),
        track.is_folder,
        scene.tcp,
    )
    .with_lanes(!crate::lanes::speaks_for(tracks, live, scene.lane_tracks(index)).is_empty());
    if row.density == crate::tcp::Density::Bar {
        return;
    }
    // The pointer's verdict on this row's controls. The panel is a
    // recorded scene like the mixer is, so a hover cannot repaint
    // the row — it repaints the ONE control, here, in the same pass
    // that already redraws every live value.
    let look = |control| {
        pointer.state(RowSpot {
            row: index,
            control,
        })
    };

    // The fixed lanes' line, under mute and solo: the lane that plays,
    // between the steps to the one before and after. Over a theme's row
    // as over the built-in one.
    if let Some(band) = row.lane_line() {
        let under = crate::lanes::speaks_for(tracks, live, scene.lane_tracks(index));
        let label = crate::lanes::label_of(&under);
        lane_line(out, palette, font, &label, &row, band, &look);
        lanes_plus(out, palette, &row, crate::lanes::opened(live), &look);
    }

    // A theme's row draws its own live part — every value and the
    // pointer's state — over the recording it made.
    if let Some(theme) = crate::skin::loaded()
        .as_deref()
        .filter(|t| t.has_role(crate::skin::ROW) && !row.tcp.compact)
    {
        let all = [
            C::Phase,
            C::Mute,
            C::Solo,
            C::RecArm,
            C::Volume,
            C::Pan,
            C::Routing,
            C::Fx,
            C::Folder,
            C::Name,
        ];
        let fields = crate::skin::row_fields(
            palette,
            live,
            crate::skin::RowShown {
                depth: i32::try_from(*depth).unwrap_or(0),
                height,
                font: Some(font),
                hover: all.into_iter().find(|c| look(*c) == Interaction::Hover),
                pressed: all.into_iter().find(|c| look(*c) == Interaction::Pressed),
                idle: !row.tcp.recording,
                collapsed: crate::folds::shut(rows, index),
            },
        );
        theme.live(
            crate::skin::ROW,
            (scene.tcp.width(), height),
            &fields,
            &mut crate::skin::canvas::Host { scene: out, font },
            Affine::translate((0.0, top)),
        );
        return;
    }

    // Mute and solo.
    for (control, label, on, lit) in [
        (C::Mute, "M", live.muted, crate::tcp::mute_lit(palette)),
        (C::Solo, "S", live.soloed, crate::tcp::solo_lit(palette)),
    ] {
        let Some(r) = row.rect(control) else { continue };
        // A touchscreen's fills its rect.
        if row.tcp.compact {
            crate::art::place(
                &mut *out,
                &art::gutter_button_sized(
                    &palette.chrome,
                    label,
                    on,
                    lit,
                    look(control),
                    (r.width(), r.height()),
                ),
                font,
                r.x0,
                r.y0,
            );
            continue;
        }
        // Flattened to the rect on a row too short for the full
        // button — the row's shape says how tall, not the art.
        crate::art::squashed(
            &mut *out,
            &art::gutter_button(&palette.chrome, label, on, lit, look(control)),
            font,
            r.x0,
            r.y0,
            1.0,
            (r.height() / crate::tcp::BUTTON.1).min(1.0),
        );
    }

    // The FX button, from the chain the track actually has.
    if let Some(r) = row.rect(C::Fx) {
        crate::art::place(
            &mut *out,
            &art::fx_pill(
                &palette.chrome,
                crate::tcp::lit(palette),
                chain(live),
                look(C::Fx),
            ),
            font,
            r.x0,
            r.y0,
        );
    }

    // Routing, and polarity in the corner below it. Both are live
    // values with their own events — a strip that recorded them was
    // right until the first time anything changed one.
    if let Some(r) = row.rect(C::Routing) {
        crate::art::place(
            &mut *out,
            &art::routing(
                &palette.chrome,
                art::Axis::Horizontal,
                routes(live),
                route_ink(palette),
                look(C::Routing),
            ),
            font,
            r.x0,
            r.y0,
        );
    }
    if let Some(r) = row.rect(C::Phase) {
        crate::art::place(
            &mut *out,
            &art::phase(&palette.chrome, live.phase_inverted, look(C::Phase)),
            font,
            r.x0,
            r.y0,
        );
    }

    // The record arm, on rows tall enough to read one.
    if let Some(r) = row.rect(C::RecArm).filter(|_| row.tcp.compact) {
        // The compact panel's, on its second line: a button like the mute
        // and solo beside it, the record ring on it.
        let lit = crate::tcp::lit(palette).rec;
        crate::art::place(
            &mut *out,
            &art::gutter_button_sized(
                &palette.chrome,
                "",
                false,
                lit,
                look(C::RecArm),
                (r.width(), r.height()),
            ),
            font,
            r.x0,
            r.y0,
        );
        let size = r.width().min(r.height()) * 0.66;
        crate::art::scaled(
            &mut *out,
            &art::record_arm(
                &palette.chrome,
                lit,
                live.armed,
                look(C::RecArm),
                art::Arm::Panel,
                crate::tcp::to_theme(palette.tcp_field),
            ),
            font,
            r.x0 + (r.width() - size) / 2.0,
            r.y0 + (r.height() - size) / 2.0,
            size / 20.0,
        );
    } else if let Some(r) = row.rect(C::RecArm) {
        crate::art::place(
            &mut *out,
            &art::record_arm(
                &palette.chrome,
                crate::tcp::lit(palette).rec,
                live.armed,
                look(C::RecArm),
                art::Arm::Panel,
                crate::tcp::to_theme(palette.tcp_field),
            ),
            font,
            r.x0,
            r.y0,
        );
    }

    // Volume and pan, in whichever form the row is showing — a knob
    // where there is room to turn one, a flattened bar where there
    // is not. Both are the same VALUE; only the shape changes.
    let knob = row.indicator() == Indicator::Knob;
    if let Some(r) = row.rect(C::Volume) {
        let field_h = r.height();
        if knob {
            // Never past the size it was authored — the same rule
            // the rect above measures it by, so the picture and the
            // hit target stay the same shape.
            let scale = (field_h / 22.0).min(1.0);
            crate::art::scaled(
                &mut *out,
                &art::volume_knob(
                    &palette.chrome,
                    crate::tcp::lit(palette).volume,
                    crate::tcp::volume_fraction(live.volume),
                    look(C::Volume),
                    field_h,
                ),
                font,
                r.x0,
                r.y0,
                scale,
            );
        }
    }
    // The name plate has no hover cell in the traced art — REAPER
    // does not light one either — but in this window it is what you
    // double-click to rename, and a control that opens an editor
    // has to say so before you commit to the second click. One rule
    // under the name, in the accent, is the least that reads.
    if look(C::Name) != Interaction::Normal {
        if let Some(r) = row.rect(C::Name) {
            out.fill(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                palette.accent,
                None,
                &vello::kurbo::Rect::new(r.x0, r.y1 - 1.0, r.x1, r.y1),
            );
        }
    }

    if let Some(r) = row.rect(C::Pan) {
        let field_h = r.height();
        if knob {
            let scale = (field_h / 25.0).min(1.0);
            crate::art::scaled(
                &mut *out,
                &art::pan_knob(
                    &palette.chrome,
                    live.pan.clamp(-1.0, 1.0),
                    crate::tcp::to_theme(palette.pan),
                    look(C::Pan),
                ),
                font,
                f64::from(gt::PAN_KNOB_X),
                r.y0 + 25.0_f64.mul_add(-scale, field_h) / 2.0,
                scale,
            );
        }
    }
}

#[cfg(test)]
mod panel_tests {
    use super::*;
    use crate::pointer::{Pointer, RowSpot};
    use crate::row::Control as C;
    use daw_ui::studio::{ProjectRef, RowsRef};

    /// Four rows tall enough that every control has somewhere to be —
    /// the narrow tiers drop mute and solo on purpose, and a test that
    /// used them would be asserting they are missing.
    fn panel() -> (
        crate::arrangement::Arrangement,
        Palette,
        Font,
        Vec<Track>,
        RowsRef,
    ) {
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = Font::embedded().expect("the embedded font");
        let tracks: Vec<Track> = (0..4)
            .map(|i| Track {
                guid: format!("t{i}"),
                name: format!("Track {i}"),
                height: Some(90),
                ..Track::default()
            })
            .collect();
        let rows = RowsRef(std::sync::Arc::new(
            tracks.iter().cloned().map(|t| (t, 0)).collect(),
        ));
        let project = ProjectRef(std::sync::Arc::new(daw_ui::studio::Project::default()));
        let scene = crate::arrangement::Arrangement::build(
            &palette,
            &font,
            &project,
            &rows,
            crate::layout::Layout::default(),
            &crate::midi::Previews::default(),
            crate::tcp::Tcp::FULL,
        );
        (scene, palette, font, tracks, rows)
    }

    fn drawn(
        scene: &crate::arrangement::Arrangement,
        palette: &Palette,
        font: &Font,
        rows: &RowsRef,
        tracks: &[Track],
        pointer: &Pointer<RowSpot>,
    ) -> anyrender::Scene {
        let view = crate::arrangement::Viewport {
            scroll_x: 0.0,
            scroll_y: 0.0,
            pps: 1.0,
            zoom_y: 1.0,
            width: 1600.0,
            height: 900.0,
            panel_w: crate::arrangement::TCP_WIDTH,
        };
        let mut out = anyrender::Scene::new();
        panel_controls(
            &mut out,
            palette,
            font,
            scene,
            rows.as_slice(),
            tracks,
            &crate::plan::Rows::of(rows.as_slice(), tracks),
            view,
            pointer,
            Affine::IDENTITY,
        );
        out
    }

    /// A playing track lights the strips in its name field; a silent
    /// session draws none and says so, so the widget stops redrawing.
    #[test]
    fn a_level_lights_the_name_field_meter() {
        let (scene, palette, _font, mut tracks, _) = panel();
        for (i, track) in tracks.iter_mut().enumerate() {
            track.index = u32::try_from(i).expect("small");
        }
        let rows = RowsRef(std::sync::Arc::new(
            tracks.iter().cloned().map(|t| (t, 0)).collect(),
        ));
        let view = crate::arrangement::Viewport {
            scroll_x: 0.0,
            panel_w: crate::arrangement::TCP_WIDTH,
            scroll_y: 0.0,
            pps: 1.0,
            zoom_y: 1.0,
            width: 1600.0,
            height: 900.0,
        };
        let map = crate::plan::Rows::of(rows.as_slice(), &tracks);
        let draw = |levels: &[daw_proto::TrackLevels]| {
            let mut out = anyrender::Scene::new();
            let lit = row_meters(
                &mut out,
                &palette,
                &scene,
                rows.as_slice(),
                &tracks,
                &map,
                view,
                levels,
                Affine::IDENTITY,
            );
            (lit, out)
        };
        let silent = vec![daw_proto::TrackLevels::default(); tracks.len()];
        let (lit, quiet) = draw(&silent);
        assert!(!lit && quiet.commands.is_empty(), "silence draws nothing");

        let mut playing = silent.clone();
        playing[1] = daw_proto::TrackLevels {
            peak_left: 0.9,
            peak_right: 0.05,
            hold_left: 0.9,
            hold_right: 0.05,
        };
        let (lit, loud) = draw(&playing);
        assert!(lit, "a level lights the meter");
        // One strip, crossing every zone at this level.
        assert!(loud.commands.len() >= 3, "{} fills", loud.commands.len());
    }

    /// The point of the whole pass: hovering a panel control has to
    /// change what is drawn. The panel is a RECORDED scene, so if the
    /// overlay does not redraw the control there is nothing else that
    /// can, and the control is simply dead under the pointer.
    #[test]
    fn hovering_a_panel_control_changes_the_picture() {
        let (scene, palette, font, tracks, rows) = panel();
        let rest = Pointer::default();
        let at_rest = drawn(&scene, &palette, &font, &rows, &tracks, &rest);
        for control in [C::Mute, C::Solo, C::RecArm, C::Volume, C::Pan, C::Name] {
            let mut pointer = Pointer::default();
            pointer.hover(Some(RowSpot { row: 1, control }));
            let hovered = drawn(&scene, &palette, &font, &rows, &tracks, &pointer);
            assert_ne!(
                at_rest, hovered,
                "{control:?} drew the same thing hovered as at rest"
            );
        }
    }

    /// And pressing it changes it again — otherwise a button that is
    /// held down looks exactly like one the pointer is merely near.
    #[test]
    fn pressing_differs_from_hovering() {
        let (scene, palette, font, tracks, rows) = panel();
        let mut pointer = Pointer::default();
        pointer.hover(Some(RowSpot {
            row: 1,
            control: C::Mute,
        }));
        let hovered = drawn(&scene, &palette, &font, &rows, &tracks, &pointer);
        pointer.press();
        let pressed = drawn(&scene, &palette, &font, &rows, &tracks, &pointer);
        assert_ne!(hovered, pressed, "a held mute looks like an idle one");
    }

    /// A hover on one row leaves the others alone, so the pass really
    /// is redrawing ONE control rather than restyling the panel.
    #[test]
    fn only_the_hovered_row_changes() {
        let (scene, palette, font, tracks, rows) = panel();
        let mut a = Pointer::default();
        a.hover(Some(RowSpot {
            row: 1,
            control: C::Mute,
        }));
        let mut b = Pointer::default();
        b.hover(Some(RowSpot {
            row: 2,
            control: C::Mute,
        }));
        assert_ne!(
            drawn(&scene, &palette, &font, &rows, &tracks, &a),
            drawn(&scene, &palette, &font, &rows, &tracks, &b)
        );
    }
}

/// The `+` beside the lane line ([`crate::row::Control::LaneShow`]): a
/// well with a plus while the track shows the lane that plays, a minus
/// while it shows them all — lit under the pointer, accented open.
fn lanes_plus(
    out: &mut anyrender::Scene,
    palette: &Palette,
    row: &crate::row::Row,
    open: bool,
    look: &dyn Fn(crate::row::Control) -> Interaction,
) {
    use crate::row::Control as C;
    use vello::kurbo::{Rect, RoundedRect};
    use vello::peniko::Fill;
    /// Half a stroke's length, and half its weight.
    const ARM: f64 = 3.0;
    const HALF: f64 = 0.6;
    let Some(r) = row.rect(C::LaneShow) else {
        return;
    };
    let well = if open {
        palette.accent.multiply_alpha(0.35)
    } else {
        palette.divider.multiply_alpha(0.5)
    };
    out.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        well,
        None,
        &RoundedRect::from_rect(r, 3.0),
    );
    let ink = match look(C::LaneShow) {
        Interaction::Normal if !open => palette.text_dim,
        _ => palette.text,
    };
    let (cx, cy) = (r.center().x, r.center().y);
    out.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        ink,
        None,
        &Rect::new(cx - ARM, cy - HALF, cx + ARM, cy + HALF),
    );
    if !open {
        out.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            ink,
            None,
            &Rect::new(cx - HALF, cy - ARM, cx + HALF, cy + ARM),
        );
    }
}

/// The lane line ([`crate::row::Row::lane_line`]): a well, the steps at
/// each end (lit under the pointer), the playing lane's name between.
fn lane_line(
    out: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    label: &str,
    row: &crate::row::Row,
    band: vello::kurbo::Rect,
    look: &dyn Fn(crate::row::Control) -> Interaction,
) {
    use crate::row::Control as C;
    use vello::kurbo::{BezPath, RoundedRect};
    use vello::peniko::Fill;
    /// The name's type size.
    const SIZE: f32 = 9.0;
    out.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.divider.multiply_alpha(0.5),
        None,
        &RoundedRect::from_rect(band, 3.0),
    );
    for (control, forward) in [(C::LanePrev, false), (C::LaneNext, true)] {
        let Some(r) = row.rect(control) else { continue };
        let ink = match look(control) {
            Interaction::Normal => palette.text_dim,
            _ => palette.text,
        };
        let (cx, cy) = (r.center().x, r.center().y);
        let (w, h) = (2.5, 3.5);
        let mut arrow = BezPath::new();
        if forward {
            arrow.move_to((cx - w, cy - h));
            arrow.line_to((cx + w, cy));
            arrow.line_to((cx - w, cy + h));
        } else {
            arrow.move_to((cx + w, cy - h));
            arrow.line_to((cx - w, cy));
            arrow.line_to((cx + w, cy + h));
        }
        arrow.close_path();
        out.fill(Fill::NonZero, Affine::IDENTITY, ink, None, &arrow);
    }
    let (Some(prev), Some(next)) = (row.rect(C::LanePrev), row.rect(C::LaneNext)) else {
        return;
    };
    let room = (next.x0 - prev.x1).max(0.0);
    let label = font.elide(label, SIZE, room);
    let width = font.width(&label, SIZE);
    crate::tcp::glyphs(
        out,
        font,
        palette.text,
        &label,
        prev.x1 + (room - width).max(0.0) / 2.0,
        f64::from(SIZE).mul_add(0.35, band.center().y),
        SIZE,
    );
}
