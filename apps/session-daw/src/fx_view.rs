//! The FX view: the selected track's effects as a row of devices.
//!
//! The mixer's racks show a track's processing stacked over its strip, at
//! a strip's width; this lays the same processors out the way Ableton and
//! Bitwig do — side by side, each a device at an editor's size, the row
//! scrolling sideways. Docked under the arrangement or the mixer, or a
//! view of its own (`shell::FxToggle`). See `docs/fx-view.md`.
//!
//! The devices are the rack's own panels, drawn by the rack's painters
//! and edited through its grips — the settings every rack view shares
//! (`tone::Shared`), so an edit here is an edit on the strip.
//!
//! A lane reads left to right: the track's own mixer strip, its chain
//! grouped by phase (a folded phase is its macro knobs — `macros`), and
//! then, after a gap, the returns it feeds — the reverbs and delays in
//! its folder — each as its strip and its effect. Several lanes are the
//! selected track and the tracks beside it in its folder: Kick, Snare,
//! Toms, Cymbals.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use anyrender::Scene;
use blitz_traits::events::{
    BlitzPointerEvent, BlitzPointerId, BlitzWheelDelta, MouseEventButton, UiEvent,
};
use dioxus::prelude::*;
use vello::kurbo::Affine;

use crate::tone::{Folded, Grip, Panel, Shared, Which};

/// How wide one device is, laid out: an EQ (and the saturation beside
/// it, a frequency picture too) twice what a compressor needs.
const DEVICE_W: f64 = 300.0;

/// The widths of a stage's `units` at a lane's height: as wide as each
/// wants, all shrunk together when the stage would run past
/// [`STAGE_W`] — a stage is seen whole on a 1440p screen.
fn stage_widths(units: &[Which], tone: &crate::tone::Tone, body_h: f64) -> Vec<f64> {
    let wants: Vec<f64> = units.iter().map(|w| unit_w(*w, tone, body_h)).collect();
    let room = GAP.mul_add(
        -crate::num::coord(units.len().saturating_add(1)),
        STAGE_W - EDGE_W - FOLDED_W,
    );
    let total: f64 = wants.iter().sum();
    let shrink = if total > room && total > 0.0 {
        room / total
    } else {
        1.0
    };
    wants.into_iter().map(|w| w * shrink).collect()
}

/// How wide `which` is over `tone` at a lane's height: a frame face as
/// wide as its design is under its header, anything else a device.
fn unit_w(which: Which, tone: &crate::tone::Tone, body_h: f64) -> f64 {
    crate::faces::aspect(which, tone).map_or_else(
        || device_w(which),
        |aspect| (face_h(body_h) * aspect).clamp(DEVICE_W * 0.5, DEVICE_W * 4.0),
    )
}

/// How tall a device's face is in a lane `body_h` tall: what its header
/// leaves. A width taken from the whole height left the face narrower
/// than its header.
fn face_h(body_h: f64) -> f64 {
    (body_h - HEADER_H).max(0.0)
}

/// How wide `which` is as a device.
fn device_w(which: Which) -> f64 {
    match which {
        Which::RescueEq
        | Which::Eq
        | Which::Space
        | Which::PreEq
        | Which::PostEq
        | Which::DecayEq
        | Which::PolishEq
        | Which::Sat => DEVICE_W * 2.0,
        _ => DEVICE_W,
    }
}

/// How tall one lane is, wherever it is: docked, the pane is one lane;
/// full size, as many lanes as fit. One height, so a track's chain is
/// the same picture in both.
pub const LANE_H: f64 = 340.0;

/// The gap between two devices.
const GAP: f64 = 6.0;

/// How far the row stands in from the pane's edges.
const INSET: f64 = 8.0;

/// The FX panel: the selected track's devices, filling where it is put —
/// or, with `lanes` above one (the FX view of its own), that many tracks'
/// chains stacked, from the selected one down, the stack scrolling through
/// the rest.
#[component]
pub fn FxPanel(#[props(default = 1)] lanes: usize) -> Element {
    // Over a song, with the racks' settings: a host without them (a page
    // with no song yet) shows an empty row rather than failing.
    let (Some(closeups), Some(session)) = (
        try_use_context::<crate::closeup::Closeups>(),
        try_use_context::<crate::studio::StudioSession>(),
    ) else {
        return rsx! {};
    };
    // The session's rows: every track a rack has settings for, seeded
    // here as the mixer seeds them, so the FX is whole without it.
    // Every track, not the arrangement's rows: a scene or a fold that
    // hides a track's returns from the timeline must not hide them here,
    // where they are the point.
    let (visible, depths) =
        daw_ui::components::folders::FolderState::default().visible(&session.project.tracks);
    let rows: Vec<(daw_proto::Track, u32)> = visible.into_iter().zip(depths).collect();
    closeups.tone.store.borrow_mut().seed(&rows);
    let tracks = rows;
    let lanes = lanes.max(1);
    let Ok(font) = crate::text::Font::embedded() else {
        return rsx! {};
    };
    #[cfg(feature = "native")]
    {
        let widget = use_hook(|| {
            dioxus_native_dom::CustomWidgetAttr::new(DeviceRow::new(
                Rc::clone(&closeups.tone),
                tracks,
                lanes,
                font,
            ))
        });
        rsx! {
            object {
                style: "position:absolute; left:0; top:0; width:100%; height:100%; background:#0b0c0e;",
                data: widget,
            }
        }
    }
    #[cfg(all(feature = "web", not(feature = "native")))]
    {
        let widget = use_hook(|| {
            crate::web_host::HostedRef(Rc::new(RefCell::new(DeviceRow::new(
                Rc::clone(&closeups.tone),
                tracks,
                lanes,
                font,
            ))))
        });
        rsx! {
            crate::web_host::WidgetCanvas { widget, hidden: false }
        }
    }
    #[cfg(not(any(feature = "native", feature = "web")))]
    {
        let _ = (closeups, tracks, lanes, font);
        rsx! {}
    }
}

use session::mix_phases::MixPhase;

/// How wide one stage may run at most: a whole stage on a 16:9 1440p
/// screen (2560 wide) beside the track's strip — an ultrawide then shows
/// what is beside it (its returns, the next stage).
const STAGE_W: f64 = 2560.0 - 120.0;

/// How tall a device's header is: its name and its bypass.
const HEADER_H: f64 = 18.0;

/// How wide a phase's macros are, open or folded: two knobs across.
const FOLDED_W: f64 = 2.0 * crate::macros::CELL_W + 8.0;

/// How wide a phase with no devices is: a faint edge holding its place.
const BLANK_W: f64 = 80.0;

/// How wide a strip is in the FX: a working strip, with its fader and
/// its name, however narrow or wide it is in the mixer — every lane's
/// and every return's the same.
const STRIP_W: u32 = 84;

/// How wide a card is, for a block frame has no face for.
const CARD_W: f64 = 120.0;

/// How wide a folded rig section is: the list of its blocks.
const SECTION_LIST_W: f64 = 150.0;

/// How wide a phase's edge is: the separator before its devices, and
/// what a press folds it by.
const EDGE_W: f64 = 14.0;

/// How tall the phase's tint is along the top of its devices.
const TINT_H: f64 = 3.0;

/// The space between a track's own chain and the returns it feeds.
const RETURNS_GAP: f64 = 48.0;

/// `track` as its FX strip draws it: [`STRIP_W`] wide. The mixer's own
/// width is the scene's (a return planned wide there was a wide strip
/// here, beside the lane's narrow one).
fn widened(track: &daw_proto::Track) -> daw_proto::Track {
    let mut track = track.clone();
    track.width = Some(STRIP_W);
    track
}

/// What a frame face's edits go to: a rack unit's settings, or a block
/// of the track's rig (which, and the face it wears).
#[derive(Clone)]
enum Target {
    Unit(Which),
    Rig(usize, Box<crate::rig::FaceEntry>),
}

/// What folds in a lane: a mix phase of a channel's chain, a section of
/// a guitar's rig, or the effects on the returns it feeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Group {
    Phase(MixPhase),
    Rig(crate::rig::Section),
    /// The returns a lane feeds: shut, each is its strip alone — the
    /// faders without their effects.
    Returns,
}

impl Group {
    /// Shut until opened: a rig's Input and Pre, which are set once and
    /// left; everything else is open.
    const fn shut_by_default(self) -> bool {
        matches!(
            self,
            Self::Rig(crate::rig::Section::Input | crate::rig::Section::Pre)
        )
    }
}

/// One device on screen: whose, which, where.
#[derive(Clone)]
struct Placed {
    guid: String,
    which: Which,
    panel: Panel,
}

/// One knob on screen: a phase's macro, or a face's control.
#[derive(Clone)]
struct Turned {
    guid: String,
    /// Which knobs it is one of, and which.
    list: &'static [crate::macros::Macro],
    index: usize,
    cell: Panel,
}

/// What the last frame put where, for the pointer to find.
#[derive(Default)]
struct Hits {
    /// Each lane: whose, its top and its height.
    lanes: Vec<(String, f64, f64)>,
    devices: Vec<Placed>,
    knobs: Vec<Turned>,
    /// Each group's edge and tint, by track: a press on one folds it.
    bars: Vec<(String, Group, Panel)>,
    /// Each lane's groups, in order: what a Shift-click folds around.
    groups: Vec<(String, Vec<Group>)>,
    /// The frame faces on screen: their device's key, whose and which,
    /// where — a press on one is the face's.
    faces: Vec<(String, String, Target, Panel)>,
    /// Each device's bypass button: whose, which (a unit, or a rig's
    /// block), and where.
    bypasses: Vec<(String, Target, Panel)>,
    /// The lane being drawn's clip: what scrolls stays right of its strip.
    clip: vello::kurbo::Rect,
    /// Each track strip drawn: whose, and its left and top — a press on
    /// one is the strip's.
    strips: Vec<(String, f64, f64)>,
    /// The faces' pictures, painted after the rest (`paint_scene`): the
    /// device's key, what to paint, where, and the lane's clip.
    pictures: Vec<(String, crate::faces::Picture, Panel, vello::kurbo::Rect)>,
}

/// A lane's strip, recorded once for its track and height.
struct Strip {
    height_bits: u64,
    skin: u64,
    mixer: crate::mcp::Mixer,
}

/// Where one lane is drawn.
#[derive(Clone, Copy)]
struct Lane {
    /// Where what scrolls starts: just right of the track's strip.
    start: f64,
    /// The pane's right edge.
    right: f64,
    top: f64,
    height: f64,
    /// The devices' box, under the tints.
    body_top: f64,
    body_h: f64,
}

impl Lane {
    /// Whether `x`..`x + width` is on screen.
    fn sees(self, x: f64, width: f64) -> bool {
        x + width >= self.start && x <= self.right
    }

    /// A device's panel at `x`.
    const fn device(self, x: f64, width: f64) -> Panel {
        Panel {
            x,
            y: self.body_top,
            width,
            height: self.body_h,
        }
    }
}

/// The chain a track shows in the FX: its rack's, with saturation and
/// modulation at the end of a channel's or a bus's Tone — devices the FX
/// view has room for and the mixer's strips do not (yet).
fn fx_chain(tone: &crate::tone::Tone) -> Vec<Which> {
    // The track's own processors, once its chain is read: what is on it.
    if let Some(units) = &tone.engine_units {
        return units.clone();
    }
    let mut chain = tone
        .panels(crate::tone::panels_for(MixPhase::Tone))
        .to_vec();
    if matches!(
        tone.role,
        crate::tone::Role::Channel | crate::tone::Role::Bus
    ) {
        // A drum gets no modulation unit: nothing on a kit wants one.
        let added: &[Which] = if tone.drum {
            &[Which::Sat]
        } else {
            &[Which::Sat, Which::Mod]
        };
        for &which in added {
            if chain.contains(&which) {
                continue;
            }
            let after = chain
                .iter()
                .rposition(|w| w.phase() == MixPhase::Tone)
                .map_or(chain.len(), |at| at.saturating_add(1));
            chain.insert(after, which);
        }
    }
    chain
}

/// A chain grouped by phase: each phase in the rack's order and its
/// devices — `tone::chain`'s rule for which empty phases still show.
fn grouped(panels: &[Which]) -> Vec<(MixPhase, Vec<Which>)> {
    let level = panels
        .iter()
        .any(|which| matches!(which.phase(), MixPhase::Rescue | MixPhase::Tone));
    crate::tone::RACK_PHASES
        .iter()
        .map(|phase| {
            let units = panels
                .iter()
                .copied()
                .filter(|which| which.phase() == *phase)
                .collect::<Vec<_>>();
            (*phase, units)
        })
        .filter(|(_, units)| level || !units.is_empty())
        .collect()
}

/// Tracks' processors, side by side: one lane, or several stacked.
/// A press on a track strip, until it lets go.
enum StripHold {
    /// A fader or a pan knob, turned from where it was when pressed.
    Turn {
        control: crate::mcp::Control,
        from_y: f64,
        travel: f64,
        was: Box<daw_proto::Track>,
    },
    /// A button: it acts if let go over itself.
    Press {
        guid: String,
        control: Option<crate::mcp::Control>,
        add: bool,
    },
}

struct DeviceRow {
    tone: Rc<Shared>,
    /// Where the strips' edits go: the engine.
    applier: Option<crate::engine::Applier>,
    /// A strip being pressed or turned, by which pointer.
    strip_hold: Option<(BlitzPointerId, StripHold)>,
    /// The session's tracks, kept up with the engine's changes — which is
    /// selected is where the lanes start.
    tracks: Vec<daw_proto::Track>,
    /// How deep each track sits, by guid: who is beside whom.
    depths: std::collections::HashMap<String, u32>,
    watch: Option<crate::engine::Watch>,
    /// Which tracks' FX chains changed in the engine: read again before
    /// they are drawn (`tone::Store::sync_tracks`).
    #[cfg(feature = "native")]
    fx_watch: Option<crate::engine::FxWatch>,
    /// Changes taken off the watch to ask for a frame, applied in it.
    pending: RefCell<Vec<daw_proto::track::TrackEvent>>,
    palette: crate::arrangement::Palette,
    font: crate::text::Font,
    layout: crate::layout::Layout,
    /// How many lanes show at once.
    lanes: usize,
    /// How many tracks past the first the lanes start.
    down: usize,
    /// How far each lane is scrolled sideways, by track.
    scroll: std::collections::HashMap<String, f64>,
    /// Each lane's strip, by track.
    strips: RefCell<std::collections::HashMap<String, Strip>>,
    hits: RefCell<Hits>,
    /// What each track has folded or opened, against its default: per
    /// track, so folding the kick's Tone leaves the snare's alone.
    folds: std::collections::HashMap<(String, Group), bool>,
    hold: Option<(BlitzPointerId, Grip, Placed, (f64, f64))>,
    /// A frame face being played: its key, whose, which, and where.
    playing: Option<(BlitzPointerId, String, String, Target, Panel)>,
    /// A macro being turned, and where the pointer was.
    turning: Option<(BlitzPointerId, Turned, f64)>,
    hover: Option<(String, Which, Grip)>,
    dirty: Cell<bool>,
    /// The effects' own painters, one per face.
    painters: std::rc::Rc<RefCell<crate::faces::Painters>>,
    /// Whether this panel has painted yet — see `Painters::begin_frame`.
    painted: bool,
    /// The rigs the guitar tracks play, by track: what their lanes are.
    rigs: RefCell<std::collections::HashMap<String, crate::rig::Rig>>,
    /// frame's rig faces manifest.
    rig_faces: crate::rig::Faces,
    /// Whether a face is moving: its picture animates, so every frame
    /// is a new one.
    animating: Cell<bool>,
}

impl DeviceRow {
    fn new(
        tone: Rc<Shared>,
        rows: Vec<(daw_proto::Track, u32)>,
        lanes: usize,
        font: crate::text::Font,
    ) -> Self {
        let depths = rows
            .iter()
            .map(|(t, depth)| (t.guid.clone(), *depth))
            .collect();
        // Each track's rig, by its name and its folder's.
        let mut rigs = std::collections::HashMap::new();
        let mut folders: Vec<(u32, String)> = Vec::new();
        for (track, depth) in &rows {
            folders.retain(|(at, _)| at < depth);
            let parent = folders.last().map(|(_, name)| name.as_str());
            if let Some(rig) = crate::rig::for_track(&track.name, parent) {
                rigs.insert(track.guid.clone(), rig);
            }
            if track.is_folder {
                folders.push((*depth, track.name.clone()));
            }
        }
        Self {
            applier: crate::engine::Applier::start(),
            strip_hold: None,
            tone,
            tracks: rows.into_iter().map(|(t, _)| t).collect(),
            depths,
            watch: crate::engine::Watch::start(),
            #[cfg(feature = "native")]
            fx_watch: crate::engine::FxWatch::start(),
            pending: RefCell::new(Vec::new()),
            palette: crate::skin::palette(&crate::arrangement::Palette::from_theme(
                &daw_ui::theming::Theme::dark(),
            )),
            font,
            layout: crate::layout::Layout::from_env(),
            lanes,
            down: 0,
            scroll: std::collections::HashMap::new(),
            strips: RefCell::new(std::collections::HashMap::new()),
            hits: RefCell::new(Hits::default()),
            folds: std::collections::HashMap::new(),
            hold: None,
            playing: None,
            turning: None,
            hover: None,
            dirty: Cell::new(true),
            painters: crate::faces::shared(),
            painted: false,
            rigs: RefCell::new(rigs),
            rig_faces: crate::rig::Faces::load(),
            animating: Cell::new(false),
        }
    }

    /// Whether `group` of track `guid` is folded.
    fn shut(&self, guid: &str, group: Group) -> bool {
        self.folds
            .get(&(guid.to_owned(), group))
            .copied()
            .unwrap_or_else(|| group.shut_by_default())
    }

    /// A press on `group`'s edge on track `guid`: it folds or opens; with
    /// Shift it stays open and the track's others fold; with Alt+Shift,
    /// that on every lane in view.
    fn fold(&mut self, guid: &str, group: Group, shift: bool, alt: bool) {
        // The returns fold alone; with Alt, on every lane in view.
        if group == Group::Returns {
            let shut = !self.shut(guid, group);
            let lanes: Vec<String> = if alt {
                self.hits
                    .borrow()
                    .lanes
                    .iter()
                    .map(|(g, ..)| g.clone())
                    .collect()
            } else {
                vec![guid.to_owned()]
            };
            for lane in lanes {
                self.folds.insert((lane, group), shut);
            }
            return;
        }
        if !shift {
            let now = self.shut(guid, group);
            self.folds.insert((guid.to_owned(), group), !now);
            return;
        }
        let lanes = self.hits.borrow().groups.clone();
        for (lane, groups) in lanes {
            if !alt && lane != guid {
                continue;
            }
            for g in groups {
                self.folds.insert((lane.clone(), g), g != group);
            }
        }
    }

    fn depth(&self, track: &daw_proto::Track) -> u32 {
        self.depths.get(&track.guid).copied().unwrap_or(0)
    }

    /// The tracks the lanes can be of: the selected one (or the first)
    /// and those beside it in its folder, in order.
    fn family(&self) -> Vec<usize> {
        let from = self.tracks.iter().position(|t| t.selected).unwrap_or(0);
        let Some(first) = self.tracks.get(from) else {
            return Vec::new();
        };
        let depth = self.depth(first);
        let beside = self
            .tracks
            .iter()
            .enumerate()
            .skip(from)
            .skip(1)
            .take_while(|(_, t)| self.depth(t) >= depth)
            .filter(|(_, t)| self.depth(t) == depth)
            .map(|(i, _)| i);
        std::iter::once(from).chain(beside).collect()
    }

    /// The returns `at` feeds: the reverbs, delays, wideners and shifters
    /// in its folder — the last of each line, not the folders over them.
    fn returns(&self, at: usize) -> Vec<usize> {
        let Some(track) = self.tracks.get(at) else {
            return Vec::new();
        };
        let depth = self.depth(track);
        let store = self.tone.store.borrow();
        let inside = self.tracks.iter().enumerate().skip(at).skip(1);
        let after = self
            .tracks
            .iter()
            .skip(at)
            .skip(2)
            .map(Some)
            .chain(std::iter::once(None));
        inside
            .zip(after)
            .take_while(|((_, t), _)| self.depth(t) > depth)
            .filter(|((_, t), next)| {
                let d = self.depth(t);
                let leaf = next.is_none_or(|next| self.depth(next) <= d);
                leaf && store
                    .get(&t.guid)
                    .and_then(|tone| main_unit(tone.role))
                    .is_some()
            })
            .map(|((i, _), _)| i)
            .collect()
    }

    fn follow(&mut self) {
        let mut events = std::mem::take(&mut *self.pending.borrow_mut());
        if let Some(watch) = &self.watch {
            events.extend(watch.drain());
        }
        for event in &events {
            // The value a fader is being dragged to is the drag's: the
            // engine's echo of an older position would pull it back.
            if let Some(guid) = crate::engine::continuous_for(event)
                && matches!(&self.strip_hold, Some((_, StripHold::Turn { was, .. })) if was.guid == guid)
            {
                continue;
            }
            crate::engine::apply_event(&mut self.tracks, event);
        }
    }

    /// The track strip control at a window point: whose strip, and which
    /// control (`None` on its ground). `None` off every strip.
    fn strip_at(
        &self,
        x: f64,
        y: f64,
    ) -> Option<(daw_proto::Track, Option<crate::mcp::Control>, f64)> {
        let hits = self.hits.borrow();
        let strips = self.strips.borrow();
        hits.strips.iter().find_map(|(guid, left, top)| {
            let mixer = &strips.get(guid)?.mixer;
            let (x0, width, height) = mixer.strip_box(0)?;
            let (cx, cy) = (x - left, y - top);
            if !(x0..x0 + width).contains(&cx) || !(0.0..=height).contains(&cy) {
                return None;
            }
            let track = self.tracks.iter().find(|t| t.guid == *guid)?;
            let control = crate::mcp::control_in(
                mixer,
                0,
                (&self.palette, &self.font),
                &widened(track),
                (cx - x0, cy),
            );
            Some((track.clone(), control, height))
        })
    }

    /// An edit from a strip: shown at once, and sent to the engine.
    fn commit(&mut self, edit: crate::engine::Edit) {
        crate::mixer_panel::predict(&mut self.tracks, &edit);
        if let Some(applier) = &self.applier {
            applier.send(edit);
        }
    }

    /// A press on a track strip: a fader or knob taken hold of, or a
    /// button waiting to be let go of.
    fn press_strip(&mut self, e: &BlitzPointerEvent, x: f64, y: f64) -> bool {
        let Some((track, control, height)) = self.strip_at(x, y) else {
            return false;
        };
        let hold = match control {
            // Locked, faders and knobs stay where they are.
            Some(c) if c.is_continuous() && crate::options::LOCKING.get() => return true,
            Some(c) if c.is_continuous() => StripHold::Turn {
                control: c,
                from_y: y,
                travel: height * 0.4,
                was: Box::new(track),
            },
            control => StripHold::Press {
                guid: track.guid,
                control,
                add: e.mods.shift() || e.mods.meta() || e.mods.ctrl(),
            },
        };
        self.strip_hold = Some((e.id, hold));
        true
    }

    /// A pointer moving with a strip's fader or knob in hand.
    fn turn_strip(&mut self, e: &BlitzPointerEvent, y: f64) -> bool {
        let Some((
            id,
            StripHold::Turn {
                control,
                from_y,
                travel,
                was,
            },
        )) = &self.strip_hold
        else {
            return false;
        };
        if *id != e.id {
            return false;
        }
        let fraction = crate::gesture::drag_fraction(y - from_y, *travel);
        let edit = crate::engine::drag(*control, &was.guid, was, fraction);
        if let Some(edit) = edit {
            self.commit(edit);
        }
        true
    }

    /// A pointer let go of a strip: a button acts if it is let go over
    /// itself; a press on the strip's ground selects its track.
    fn release_strip(&mut self, e: &BlitzPointerEvent) -> bool {
        let Some((_, hold)) = self.strip_hold.take_if(|(id, _)| *id == e.id) else {
            return false;
        };
        let StripHold::Press { guid, control, add } = hold else {
            return true;
        };
        let (x, y) = (f64::from(e.coords.client_x), f64::from(e.coords.client_y));
        let Some((track, up, _)) = self.strip_at(x, y).filter(|(t, ..)| t.guid == guid) else {
            return true;
        };
        if up != control {
            return true;
        }
        let edit = match control {
            Some(control) => crate::engine::click(control, &guid, &track, add),
            None if add => Some(crate::engine::Edit::AddToSelection(guid)),
            None => Some(crate::engine::Edit::Select(guid)),
        };
        if let Some(edit) = edit {
            self.commit(edit);
        }
        true
    }

    /// Whether the engine has said anything since the last frame.
    fn heard(&self) -> bool {
        // An FX changed in the engine: a frame, to read and draw it.
        #[cfg(feature = "native")]
        if let Some(fx) = &self.fx_watch {
            let changed: Vec<String> = fx.drain().collect();
            if !changed.is_empty() {
                let mut store = self.tone.store.borrow_mut();
                for guid in &changed {
                    store.changed(guid);
                }
                return true;
            }
        }
        let Some(watch) = &self.watch else {
            return false;
        };
        let mut pending = self.pending.borrow_mut();
        pending.extend(watch.drain());
        !pending.is_empty()
    }

    fn device_at(&self, x: f64, y: f64) -> Option<(Placed, Grip)> {
        let store = self.tone.store.borrow();
        self.hits.borrow().devices.iter().find_map(|placed| {
            if !placed.panel.contains(x, y) {
                return None;
            }
            let tone = store.get(&placed.guid)?;
            crate::tone::grip_at(&[placed.which], tone, placed.panel, Folded::zoomed(), x, y)
                .map(|grip| (placed.clone(), grip))
        })
    }

    /// The lane under `y`, by its track.
    fn lane_at(&self, y: f64) -> Option<String> {
        self.hits
            .borrow()
            .lanes
            .iter()
            .find(|(_, top, h)| y >= *top && y < top + h)
            .map(|(guid, ..)| guid.clone())
    }

    /// The grip lit on `which` of `guid`: held, or under the pointer.
    fn lit(&self, guid: &str, which: Which) -> Option<Grip> {
        self.hold
            .as_ref()
            .filter(|(_, _, held, _)| held.guid == guid && held.which == which)
            .map(|(_, grip, ..)| *grip)
            .or_else(|| {
                self.hover
                    .as_ref()
                    .filter(|(g, on, _)| g == guid && *on == which)
                    .map(|(.., grip)| *grip)
            })
    }

    fn wheel(&mut self, e: &blitz_traits::events::BlitzWheelEvent) {
        let (dx, dy) = match e.delta {
            BlitzWheelDelta::Lines(x, y) => (x * 40.0, y * 40.0),
            BlitzWheelDelta::Pixels(x, y) => (x, y),
        };
        if dx.abs() > dy.abs() || e.mods.shift() || self.lanes == 1 {
            // The lane under the pointer, along its chain.
            let by = if dx.abs() > dy.abs() { dx } else { dy };
            if let Some(guid) = self.lane_at(f64::from(e.coords.client_y)) {
                let scroll = self.scroll.entry(guid).or_default();
                *scroll = (*scroll - by).max(0.0);
            }
        } else if dy < 0.0 {
            // Down through the tracks, a lane at a time.
            self.down = self.down.saturating_add(1);
        } else {
            self.down = self.down.saturating_sub(1);
        }
    }

    /// Hand what the face at `key` edited to its track's settings.
    fn face_edits(&mut self, key: &str, guid: &str, target: &Target) {
        let edits = {
            let mut painters = self.painters.borrow_mut();
            let Some(surface) = painters.frame_mut(key) else {
                return;
            };
            surface.take_edits()
        };
        if edits.is_empty() {
            return;
        }
        match target {
            Target::Unit(which) => {
                if let Some(tone) = self.tone.store.borrow_mut().edit(guid) {
                    for (addr, value) in &edits {
                        crate::faces::edit(*which, tone, addr, *value);
                    }
                }
                self.tone.edited();
            }
            Target::Rig(at, face) => {
                if let Some(rig) = self.rigs.borrow_mut().get_mut(guid) {
                    for (addr, value) in &edits {
                        rig.edit(*at, &face.ns, addr, *value, face);
                    }
                }
                self.dirty.set(true);
            }
        }
    }

    fn press(&mut self, e: &BlitzPointerEvent) -> bool {
        let (x, y) = (f64::from(e.coords.client_x), f64::from(e.coords.client_y));
        if self.press_strip(e, x, y) {
            return true;
        }
        let bypass = self
            .hits
            .borrow()
            .bypasses
            .iter()
            .find(|(.., at)| at.contains(x, y))
            .map(|(guid, target, _)| (guid.clone(), target.clone()));
        if let Some((guid, target)) = bypass {
            match target {
                Target::Unit(which) => {
                    if let Some(tone) = self.tone.store.borrow_mut().edit(&guid) {
                        tone.bypass.toggle(which);
                    }
                    self.tone.edited();
                }
                Target::Rig(at, _) => {
                    if let Some(block) = self
                        .rigs
                        .borrow_mut()
                        .get_mut(&guid)
                        .and_then(|r| r.blocks.get_mut(at))
                    {
                        block.bypassed = !block.bypassed;
                    }
                }
            }
            return true;
        }
        let face = self
            .hits
            .borrow()
            .faces
            .iter()
            .find(|(.., at)| at.contains(x, y))
            .cloned();
        if let Some((key, guid, target, at)) = face {
            if let Some(surface) = self.painters.borrow_mut().frame_mut(&key) {
                surface.pointer_down(x - at.x, y - at.y, face_mods(e));
            }
            self.face_edits(&key, &guid, &target);
            self.playing = Some((e.id, key, guid, target, at));
            return true;
        }
        let (bar, knob) = {
            let hits = self.hits.borrow();
            (
                hits.bars
                    .iter()
                    .find(|(.., bar)| bar.contains(x, y))
                    .map(|(guid, group, _)| (guid.clone(), *group)),
                hits.knobs
                    .iter()
                    .find(|knob| knob.cell.contains(x, y))
                    .cloned(),
            )
        };
        if let Some((guid, group)) = bar {
            self.fold(&guid, group, e.mods.shift(), e.mods.alt());
            return true;
        }
        if let Some(knob) = knob {
            self.turning = Some((e.id, knob, y));
            return true;
        }
        match self.device_at(x, y) {
            Some((placed, grip)) if grip.is_switch() => {
                if let Some(tone) = self.tone.store.borrow_mut().edit(&placed.guid) {
                    crate::tone::toggle(tone, grip);
                }
                self.tone.edited();
                true
            }
            Some((placed, grip)) => {
                self.hold = Some((e.id, grip, placed, (x, y)));
                true
            }
            None => false,
        }
    }

    fn moved(&mut self, e: &BlitzPointerEvent) -> bool {
        let (x, y) = (f64::from(e.coords.client_x), f64::from(e.coords.client_y));
        if self.turn_strip(e, y) {
            return true;
        }
        if let Some((id, key, guid, target, at)) = self.playing.clone()
            && id == e.id
        {
            if let Some(surface) = self.painters.borrow_mut().frame_mut(&key) {
                surface.pointer_move(x - at.x, y - at.y, face_mods(e));
            }
            self.face_edits(&key, &guid, &target);
            return true;
        }
        if let Some((id, knob, last)) = self.turning.clone()
            && id == e.id
        {
            // Up turns it up: a pixel is a two-hundredth of its travel,
            // with Shift a tenth of that.
            let per = if crate::closeup::mods_of(e).shift {
                2000.0
            } else {
                200.0
            };
            if let Some(tone) = self.tone.store.borrow_mut().edit(&knob.guid)
                && let Some(m) = knob.list.get(knob.index)
            {
                (m.set)(tone, (m.get)(tone) + (last - y) / per);
            }
            self.turning = Some((id, knob, y));
            self.tone.edited();
            return true;
        }
        if let Some((id, grip, placed, last)) = self.hold.clone()
            && id == e.id
        {
            if let Some(tone) = self.tone.store.borrow_mut().edit(&placed.guid) {
                crate::tone::drag(
                    tone,
                    grip,
                    &[placed.which],
                    placed.panel,
                    Folded::zoomed(),
                    crate::closeup::mods_of(e),
                    x - last.0,
                    y - last.1,
                );
            }
            self.hold = Some((id, grip, placed, (x, y)));
            self.tone.edited();
            return true;
        }
        if e.is_finger() {
            return false;
        }
        let over = self
            .device_at(x, y)
            .map(|(placed, grip)| (placed.guid, placed.which, grip));
        let lit = over != self.hover;
        self.hover = over;
        lit
    }

    fn event(&mut self, event: &UiEvent) {
        let changed = match event {
            UiEvent::Wheel(e) => {
                self.wheel(e);
                true
            }
            UiEvent::PointerDown(e) if e.button == MouseEventButton::Main => self.press(e),
            UiEvent::PointerMove(e) => self.moved(e),
            UiEvent::PointerUp(e) | UiEvent::PointerCancel(e) => {
                if let Some((_, key, guid, target, _)) =
                    self.playing.take_if(|(id, ..)| *id == e.id)
                {
                    if let Some(surface) = self.painters.borrow_mut().frame_mut(&key) {
                        surface.pointer_up();
                    }
                    self.face_edits(&key, &guid, &target);
                }
                let stripped = self.release_strip(e);
                let turned = self.turning.take_if(|(id, ..)| *id == e.id).is_some();
                self.hold.take_if(|(id, ..)| *id == e.id).is_some() || turned || stripped
            }
            _ => false,
        };
        if changed {
            self.dirty.set(true);
        }
    }

    /// Draw `track`'s mixer strip at `x`, `top`, `height` tall — the
    /// mixer's own, recorded once, its live controls over it. Its width.
    fn strip(
        &self,
        scene: &mut Scene,
        hits: &mut Hits,
        track: &daw_proto::Track,
        x: f64,
        top: f64,
        height: f64,
    ) -> f64 {
        let track = &widened(track);
        let rows = vec![(track.clone(), 0)];
        let mut strips = self.strips.borrow_mut();
        let skin = crate::skin::version();
        let stale = strips
            .get(&track.guid)
            .is_none_or(|s| s.height_bits != height.to_bits() || s.skin != skin);
        if stale {
            let settings = crate::settings::Settings {
                mixing: true,
                ..crate::settings::Settings::default()
            };
            let mixer = crate::mcp::Mixer::build(
                &self.palette,
                &self.font,
                &daw_ui::studio::ProjectRef(std::sync::Arc::new(
                    daw_ui::studio::project::Project::default(),
                )),
                &daw_ui::studio::RowsRef(std::sync::Arc::new(rows.clone())),
                height,
                self.layout,
                // The strip without its rack: the devices are its rack.
                &[],
                false,
                settings,
                &self.tone.store.borrow(),
            );
            strips.insert(
                track.guid.clone(),
                Strip {
                    height_bits: height.to_bits(),
                    skin,
                    mixer,
                },
            );
        }
        let Some(strip) = strips.get(&track.guid) else {
            return 0.0;
        };
        hits.strips.push((track.guid.clone(), x, top));
        let mixer = &strip.mixer;
        let width = mixer.content_width();
        let at = Affine::translate((x, top));
        mixer.replay(scene, 0.0, width, at);
        let tracks = [track.clone()];
        crate::overlay::controls(
            scene,
            &self.palette,
            &self.font,
            mixer,
            &tracks,
            &crate::plan::Rows::of(&rows, &tracks),
            &crate::pointer::Pointer::default(),
            &[],
            &crate::overlay::Clips::default(),
            0.0,
            &mut crate::overlay::Racks::none(),
            0.0,
            width,
            at,
        );
        width
    }

    /// How wide `phase`'s group is, with `units` in it.
    fn group_w(
        &self,
        guid: &str,
        tone: &crate::tone::Tone,
        (phase, units): (MixPhase, &[Which]),
        body_h: f64,
        returns: &[usize],
    ) -> f64 {
        let returns = if phase == MixPhase::Depth {
            returns
        } else {
            &[]
        };
        if units.is_empty() && returns.is_empty() {
            BLANK_W
        } else if self.shut(guid, Group::Phase(phase)) {
            EDGE_W + FOLDED_W
        } else {
            // The macros stay, open or folded: the devices follow them,
            // and in Depth the returns the track feeds.
            EDGE_W
                + FOLDED_W
                + stage_widths(units, tone, body_h)
                    .iter()
                    .map(|w| GAP + w)
                    .sum::<f64>()
                + self.returns_inside_w(returns, body_h, self.shut(guid, Group::Returns))
        }
    }

    /// How wide `returns` are inside the Depth stage: their fold's edge,
    /// then each its strip and (open) its effect.
    fn returns_inside_w(&self, returns: &[usize], body_h: f64, shut: bool) -> f64 {
        if returns.is_empty() {
            return 0.0;
        }
        let store = self.tone.store.borrow();
        GAP + EDGE_W
            + returns
                .iter()
                .filter_map(|r| self.tracks.get(*r))
                .map(|r| {
                    let device = store
                        .get(&r.guid)
                        .and_then(|t| main_unit(t.role).map(|w| ret_device_w(t, w, body_h)))
                        .unwrap_or(DEVICE_W);
                    let device = if shut { 0.0 } else { GAP + device };
                    GAP + self.layout.width_of(widened(r).width) + device
                })
                .sum::<f64>()
    }

    /// How wide `returns` are, laid out after the chain: their fold's
    /// edge, then each its strip and (open) its effect.
    fn returns_w(&self, returns: &[usize], body_h: f64, shut: bool) -> f64 {
        if returns.is_empty() {
            return 0.0;
        }
        let store = self.tone.store.borrow();
        RETURNS_GAP
            + EDGE_W
            + GAP
            + returns
                .iter()
                .filter_map(|r| self.tracks.get(*r))
                .map(|r| {
                    let device = store
                        .get(&r.guid)
                        .and_then(|t| main_unit(t.role).map(|w| ret_device_w(t, w, body_h)))
                        .unwrap_or(DEVICE_W);
                    let device = if shut { 0.0 } else { device + GAP };
                    self.layout.width_of(widened(r).width) + GAP + device
                })
                .sum::<f64>()
    }

    /// `list`'s knobs for `guid` (named `name`), in `cells`, tinted by
    /// `phase`.
    fn draw_knobs(
        &self,
        scene: &mut Scene,
        hits: &mut Hits,
        (guid, name): (&str, &str),
        tone: &crate::tone::Tone,
        (phase, list): (MixPhase, &'static [crate::macros::Macro]),
        cells: Vec<Panel>,
    ) {
        for (index, (knob, cell)) in list.iter().zip(cells).enumerate() {
            let held = self.turning.as_ref().is_some_and(|(_, t, _)| {
                t.guid == guid && std::ptr::eq(t.list, list) && t.index == index
            });
            crate::macros::draw(
                scene,
                &self.palette,
                &self.font,
                &crate::macros::Knob {
                    tone,
                    phase,
                    track: name,
                    knob,
                    cell,
                    held,
                },
            );
            hits.knobs.push(Turned {
                guid: guid.to_owned(),
                list,
                index,
                cell,
            });
        }
    }

    /// A phase's macro knobs, in `well`.
    fn draw_macros(
        &self,
        scene: &mut Scene,
        hits: &mut Hits,
        track: &daw_proto::Track,
        tone: &crate::tone::Tone,
        phase: MixPhase,
        well: Panel,
    ) {
        let list = crate::macros::of(phase);
        let cells = crate::macros::cells(list.len(), well);
        self.draw_knobs(
            scene,
            hits,
            (&track.guid, &track.name),
            tone,
            (phase, list),
            cells,
        );
    }

    /// A device with a face of its own — the effect's painted picture
    /// over a row of its controls — or `false` for one drawn as the
    /// rack draws it.
    fn draw_face(
        &self,
        scene: &mut Scene,
        hits: &mut Hits,
        guid: &str,
        tone: &crate::tone::Tone,
        which: Which,
        panel: Panel,
    ) -> bool {
        let Some(face) = crate::faces::of(which, tone) else {
            return false;
        };
        let key = format!("{guid}/{which:?}");
        if let crate::faces::Picture::Frame(_) = face.picture {
            // A frame face is the whole device, and plays its own
            // controls: the pointer goes to it. With its back under it,
            // the two share the height, front over back.
            let (top, bottom) = match face.under {
                Some(_) => {
                    let half = panel.height / 2.0;
                    (
                        Panel {
                            height: half,
                            ..panel
                        },
                        Some(Panel {
                            y: panel.y + half,
                            height: half,
                            ..panel
                        }),
                    )
                }
                None => (panel, None),
            };
            hits.faces
                .push((key.clone(), guid.to_owned(), Target::Unit(which), top));
            hits.pictures
                .push((key.clone(), face.picture, top, hits.clip));
            if let (Some(under), Some(bottom)) = (face.under, bottom) {
                let back = format!("{key}/back");
                hits.faces
                    .push((back.clone(), guid.to_owned(), Target::Unit(which), bottom));
                hits.pictures
                    .push((back, crate::faces::Picture::Frame(under), bottom, hits.clip));
            }
            return true;
        }
        crate::faces::chrome(scene, &self.palette, &self.font, &face, panel);
        hits.pictures.push((
            key,
            face.picture,
            crate::faces::picture_at(panel),
            hits.clip,
        ));
        let row = crate::faces::controls(panel);
        let cells = crate::macros::cells(face.knobs.len(), row);
        self.draw_knobs(
            scene,
            hits,
            (guid, ""),
            tone,
            (which.phase(), face.knobs),
            cells,
        );
        true
    }

    /// One device: `which` of `guid`'s settings, in `panel`.
    fn draw_device(
        &self,
        scene: &mut Scene,
        hits: &mut Hits,
        guid: &str,
        tone: &crate::tone::Tone,
        which: Which,
        panel: Panel,
    ) {
        // A header on every device: its name, and its bypass.
        let on = !tone.bypass.is(which);
        let panel = self.device_header(
            scene,
            hits,
            (guid, Target::Unit(which)),
            (&crate::plugin_faces::title(which, tone), on),
            panel,
        );
        if self.draw_face(scene, hits, guid, tone, which, panel) {
            return;
        }
        crate::tone::draw(
            scene,
            &self.palette,
            &self.font,
            tone,
            &crate::live::Meters::default(),
            &[which],
            panel,
            Folded::zoomed(),
            self.lit(guid, which),
            None,
        );
        hits.devices.push(Placed {
            guid: guid.to_owned(),
            which,
            panel,
        });
    }

    /// A device's header across the top of `panel`: what it is, and a
    /// bypass button lit while it is on. The rest of `panel`, for the
    /// device itself.
    fn device_header(
        &self,
        scene: &mut Scene,
        hits: &mut Hits,
        (guid, target): (&str, Target),
        (title, on): (&str, bool),
        panel: Panel,
    ) -> Panel {
        let bar =
            vello::kurbo::Rect::new(panel.x, panel.y, panel.x + panel.width, panel.y + HEADER_H);
        anyrender::PaintScene::fill(
            scene,
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            self.palette.row_b,
            None,
            &bar,
        );
        // The bypass: a lamp at the left, lit while the effect is on.
        let lamp = vello::kurbo::Circle::new((panel.x + 10.0, panel.y + HEADER_H / 2.0), 4.0);
        anyrender::PaintScene::fill(
            scene,
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            if on {
                vello::peniko::Color::from_rgb8(34, 197, 94)
            } else {
                self.palette.text_faint
            },
            None,
            &lamp,
        );
        crate::tcp::glyphs(
            scene,
            &self.font,
            if on {
                self.palette.text
            } else {
                self.palette.text_faint
            },
            &self.font.elide(title, 10.0, panel.width - 30.0),
            panel.x + 20.0,
            panel.y + 13.0,
            10.0,
        );
        hits.bypasses.push((
            guid.to_owned(),
            target,
            Panel {
                width: HEADER_H + 8.0,
                height: HEADER_H,
                ..panel
            },
        ));
        Panel {
            y: panel.y + HEADER_H,
            height: (panel.height - HEADER_H).max(0.0),
            ..panel
        }
    }

    /// `track`'s chain, phase by phase, from `x`. Where it ends.
    fn draw_chain(
        &self,
        scene: &mut Scene,
        hits: &mut Hits,
        (track, returns): (&daw_proto::Track, &[usize]),
        lane: Lane,
        mut x: f64,
    ) -> f64 {
        let store = self.tone.store.borrow();
        let Some(tone) = store.get(&track.guid) else {
            return x;
        };
        let groups = grouped(&fx_chain(tone));
        let holds = |phase: MixPhase, units: &[Which]| {
            !units.is_empty() || (phase == MixPhase::Depth && !returns.is_empty())
        };
        hits.groups.push((
            track.guid.clone(),
            groups
                .iter()
                .filter(|(phase, units)| holds(*phase, units))
                .map(|(phase, _)| Group::Phase(*phase))
                .collect(),
        ));
        for (phase, units) in groups {
            let width = self.group_w(&track.guid, tone, (phase, &units), lane.body_h, returns);
            let x0 = x;
            x += width + GAP;
            if !lane.sees(x0, width) {
                continue;
            }
            let shut = self.shut(&track.guid, Group::Phase(phase));
            let group = Panel {
                x: x0,
                y: lane.top + INSET,
                width,
                height: INSET.mul_add(-2.0, lane.height),
            };
            phase_edge(
                scene,
                &self.palette,
                &self.font,
                (phase.display_name(), crate::tone::phase_tint(phase)),
                group,
                holds(phase, &units).then_some(shut),
            );
            if !holds(phase, &units) {
                continue;
            }
            // The edge folds it, and so does its tint.
            hits.bars.push((
                track.guid.clone(),
                Group::Phase(phase),
                Panel {
                    width: EDGE_W,
                    ..group
                },
            ));
            hits.bars.push((
                track.guid.clone(),
                Group::Phase(phase),
                Panel {
                    height: TINT_H + 4.0,
                    ..group
                },
            ));
            let mut at = x0 + EDGE_W;
            // The phase's macros, folded or open, in the same place: a
            // hand on the phase whatever is showing.
            let well = Panel {
                x: at,
                y: lane.body_top + 4.0,
                width: FOLDED_W,
                height: lane.body_h - 4.0,
            };
            if lane.sees(well.x, well.width) {
                self.draw_macros(scene, hits, track, tone, phase, well);
            }
            if shut {
                continue;
            }
            at += FOLDED_W + GAP;
            let widths = stage_widths(&units, tone, lane.body_h);
            for (which, width) in units.iter().copied().zip(widths) {
                let here = at;
                at += width + GAP;
                if lane.sees(here, width) {
                    self.draw_device(
                        scene,
                        hits,
                        &track.guid,
                        tone,
                        which,
                        lane.device(here, width),
                    );
                }
            }
            // Depth: the returns the track feeds, each its strip and
            // its effect — the tracks it takes to mix this one.
            if phase == MixPhase::Depth && !returns.is_empty() {
                self.draw_returns(scene, hits, (&track.guid, returns), lane, at);
            }
        }
        x
    }

    /// How wide block `at` of `rig` is: its face as wide as its design is
    /// at the lane's height, a card where frame has none.
    fn block_w(&self, rig: &crate::rig::Rig, at: usize, body_h: f64) -> f64 {
        rig.blocks
            .get(at)
            .and_then(|block| self.rig_faces.of(rig, block))
            .map_or(CARD_W, |face| {
                (body_h * face.size.0 / face.size.1.max(1.0)).clamp(CARD_W, DEVICE_W * 5.0)
            })
    }

    /// Each section of `rig` with blocks, and those blocks' indices.
    fn sections(rig: &crate::rig::Rig) -> Vec<(crate::rig::Section, Vec<usize>)> {
        crate::rig::Section::ALL
            .iter()
            .map(|section| {
                let blocks = rig
                    .blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| crate::rig::section_of(b) == Some(*section))
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                (*section, blocks)
            })
            .filter(|(_, blocks)| !blocks.is_empty())
            .collect()
    }

    /// `section`'s blocks as columns: in Time, a kind's two lanes (two
    /// delays, two reverbs) stacked in one column, a lone one over its
    /// own back; elsewhere a block a column. Each entry: the block, and
    /// whether it is its back.
    fn columns(
        &self,
        rig: &crate::rig::Rig,
        section: crate::rig::Section,
        blocks: &[usize],
    ) -> Vec<Vec<(usize, bool)>> {
        if section != crate::rig::Section::Time {
            return blocks.iter().map(|at| vec![(*at, false)]).collect();
        }
        let mut columns = Vec::new();
        for kind in ["delay", "reverb"] {
            let of_kind: Vec<usize> = blocks
                .iter()
                .copied()
                .filter(|at| rig.blocks.get(*at).is_some_and(|b| b.kind == kind))
                .collect();
            for pair in of_kind.chunks(2) {
                match pair {
                    [a, b] => columns.push(vec![(*a, false), (*b, false)]),
                    [a] => {
                        let face = rig.blocks.get(*a).and_then(|b| self.rig_faces.of(rig, b));
                        // A tall layout is the whole column itself.
                        let has_back = face.is_some_and(|f| f.back.is_some() && f.tall.is_none());
                        columns.push(if has_back {
                            vec![(*a, false), (*a, true)]
                        } else {
                            vec![(*a, false)]
                        });
                    }
                    _ => {}
                }
            }
        }
        columns
    }

    /// How wide a column is: its faces at their share of the height.
    fn column_w(&self, rig: &crate::rig::Rig, column: &[(usize, bool)], body_h: f64) -> f64 {
        let share = body_h / crate::num::coord(column.len().max(1));
        column
            .iter()
            .map(|(at, _)| self.block_w(rig, *at, share))
            .fold(0.0, f64::max)
    }

    /// How wide `section` of `guid`'s rig is.
    fn section_w(
        &self,
        guid: &str,
        rig: &crate::rig::Rig,
        section: crate::rig::Section,
        blocks: &[usize],
        body_h: f64,
    ) -> f64 {
        if self.shut(guid, Group::Rig(section)) {
            return EDGE_W + SECTION_LIST_W;
        }
        EDGE_W
            + self
                .columns(rig, section, blocks)
                .iter()
                .map(|column| self.column_w(rig, column, body_h) + GAP)
                .sum::<f64>()
            - GAP
    }

    /// How wide `guid`'s rig is, laid out.
    fn rig_w(&self, guid: &str, body_h: f64) -> f64 {
        let rigs = self.rigs.borrow();
        let Some(rig) = rigs.get(guid) else {
            return 0.0;
        };
        Self::sections(rig)
            .iter()
            .map(|(section, blocks)| self.section_w(guid, rig, *section, blocks, body_h) + GAP)
            .sum()
    }

    /// A folded section: a list of what is in it, lit where it is on.
    fn section_list(
        &self,
        scene: &mut Scene,
        rig: &crate::rig::Rig,
        section: crate::rig::Section,
        blocks: &[usize],
        (x0, lane): (f64, Lane),
    ) {
        let mut y = lane.body_top + 22.0;
        for at in blocks {
            let Some(block) = rig.blocks.get(*at) else {
                continue;
            };
            if y > lane.body_top + lane.body_h - 4.0 {
                break;
            }
            let dot = vello::kurbo::Circle::new((x0 + EDGE_W + 6.0, y - 3.0), 2.5);
            anyrender::PaintScene::fill(
                scene,
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                if block.bypassed {
                    self.palette.text_faint
                } else {
                    section.tint()
                },
                None,
                &dot,
            );
            crate::tcp::glyphs(
                scene,
                &self.font,
                if block.bypassed {
                    self.palette.text_faint
                } else {
                    self.palette.text_dim
                },
                &self.font.elide(&block.name, 9.0, SECTION_LIST_W - 16.0),
                x0 + EDGE_W + 12.0,
                y,
                9.0,
            );
            y += 14.0;
        }
    }

    /// `track`'s rig, section by section, from `x`: each block wearing
    /// its frame face, a folded section a list of what is in it. Where it
    /// ends.
    fn draw_rig(
        &self,
        scene: &mut Scene,
        hits: &mut Hits,
        track: &daw_proto::Track,
        lane: Lane,
        mut x: f64,
    ) -> f64 {
        let rigs = self.rigs.borrow();
        let Some(rig) = rigs.get(&track.guid) else {
            return x;
        };
        let sections = Self::sections(rig);
        hits.groups.push((
            track.guid.clone(),
            sections.iter().map(|(s, _)| Group::Rig(*s)).collect(),
        ));
        for (section, blocks) in sections {
            let width = self.section_w(&track.guid, rig, section, &blocks, lane.body_h);
            let x0 = x;
            x += width + GAP;
            if !lane.sees(x0, width) {
                continue;
            }
            let shut = self.shut(&track.guid, Group::Rig(section));
            let group = Panel {
                x: x0,
                y: lane.top + INSET,
                width,
                height: INSET.mul_add(-2.0, lane.height),
            };
            phase_edge(
                scene,
                &self.palette,
                &self.font,
                (section.name(), section.tint()),
                group,
                Some(shut),
            );
            hits.bars.push((
                track.guid.clone(),
                Group::Rig(section),
                Panel {
                    width: EDGE_W,
                    ..group
                },
            ));
            hits.bars.push((
                track.guid.clone(),
                Group::Rig(section),
                Panel {
                    height: TINT_H + 4.0,
                    ..group
                },
            ));
            if shut {
                self.section_list(scene, rig, section, &blocks, (x0, lane));
                continue;
            }
            self.draw_columns(scene, hits, (track, rig), section, &blocks, (x0, lane));
        }
        x
    }

    /// An open section's blocks, column by column from `x0`: each in its
    /// frame face, a card where frame has none.
    fn draw_columns(
        &self,
        scene: &mut Scene,
        hits: &mut Hits,
        (track, rig): (&daw_proto::Track, &crate::rig::Rig),
        section: crate::rig::Section,
        blocks: &[usize],
        (x0, lane): (f64, Lane),
    ) {
        let mut at_x = x0 + EDGE_W;
        for column in self.columns(rig, section, blocks) {
            let w = self.column_w(rig, &column, lane.body_h);
            let here = at_x;
            at_x += w + GAP;
            if !lane.sees(here, w) {
                continue;
            }
            let share = lane.body_h / crate::num::coord(column.len().max(1));
            for (row, (at, back)) in column.iter().enumerate() {
                let Some(block) = rig.blocks.get(*at) else {
                    continue;
                };
                let panel = Panel {
                    x: here,
                    y: crate::num::coord(row).mul_add(share, lane.body_top),
                    width: w,
                    height: share,
                };
                // The lower of two lanes is drawn dark, as the rig
                // draws the second of a kind.
                let dark = row > 0 && !back;
                match self.rig_faces.of(rig, block) {
                    Some(face) => {
                        let key = format!(
                            "{}/rig/{at}{}",
                            track.guid,
                            if *back { "/back" } else { "" }
                        );
                        hits.faces.push((
                            key.clone(),
                            track.guid.clone(),
                            Target::Rig(*at, Box::new(face.clone())),
                            panel,
                        ));
                        hits.pictures.push((
                            key,
                            crate::faces::Picture::Frame({
                                let mut f = crate::rig::face_of(rig, block, face, (dark, *back));
                                // Alone in a Time column, its tall layout;
                                // one of two, told it is stacked (no More
                                // button: the column is the other lane).
                                if column.len() == 1 && section == crate::rig::Section::Time {
                                    f.design = crate::rig::pick_design(face, false, false, true);
                                } else if column.len() > 1 {
                                    f.values.push((format!("{}/stacked", face.ns), 1.0));
                                }
                                f
                            }),
                            panel,
                            hits.clip,
                        ));
                    }
                    None => self.block_card(scene, block, section, panel),
                }
            }
        }
    }

    /// A block frame has no face for: a card with its name, what is
    /// loaded and whether it is on.
    fn block_card(
        &self,
        scene: &mut Scene,
        block: &crate::rig::Block,
        section: crate::rig::Section,
        at: Panel,
    ) {
        anyrender::PaintScene::fill(
            scene,
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            self.palette.row_a,
            None,
            &vello::kurbo::RoundedRect::new(at.x, at.y, at.x + at.width, at.y + at.height, 4.0),
        );
        let ink = if block.bypassed {
            self.palette.text_faint
        } else {
            self.palette.text
        };
        crate::tcp::glyphs(
            scene,
            &self.font,
            ink,
            &self.font.elide(&block.name, 10.0, at.width - 12.0),
            at.x + 6.0,
            at.y + 16.0,
            10.0,
        );
        crate::tcp::glyphs(
            scene,
            &self.font,
            self.palette.text_faint,
            &self.font.elide(&block.preset, 8.0, at.width - 12.0),
            at.x + 6.0,
            at.y + 30.0,
            8.0,
        );
        let dot = vello::kurbo::Circle::new((at.x + at.width - 10.0, at.y + 11.0), 3.0);
        anyrender::PaintScene::fill(
            scene,
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            if block.bypassed {
                self.palette.text_faint
            } else {
                section.tint()
            },
            None,
            &dot,
        );
    }

    /// The returns `returns`, each its strip and its effect, from `x`.
    fn draw_returns(
        &self,
        scene: &mut Scene,
        hits: &mut Hits,
        (owner, returns): (&str, &[usize]),
        lane: Lane,
        mut x: f64,
    ) {
        if returns.is_empty() {
            return;
        }
        // The fold's edge: a press folds the effects away, the strips
        // stay — the faders alone.
        let shut = self.shut(owner, Group::Returns);
        let edge = Panel {
            x,
            y: lane.top + INSET,
            width: EDGE_W,
            height: INSET.mul_add(-2.0, lane.height),
        };
        if lane.sees(edge.x, edge.width) {
            returns_edge(scene, &self.palette, edge, shut);
        }
        hits.bars.push((owner.to_owned(), Group::Returns, edge));
        x += EDGE_W + GAP;
        let store = self.tone.store.borrow();
        for ret in returns.iter().filter_map(|r| self.tracks.get(*r)) {
            let Some((tone, which)) = store
                .get(&ret.guid)
                .and_then(|t| main_unit(t.role).map(|which| (t, which)))
            else {
                continue;
            };
            let strip_w = self.layout.width_of(widened(ret).width);
            if lane.sees(x, strip_w) {
                self.strip(scene, hits, ret, x, lane.top, lane.height);
            }
            x += strip_w + GAP;
            if shut {
                continue;
            }
            let width = ret_device_w(tone, which, lane.body_h);
            if lane.sees(x, width) {
                self.draw_device(scene, hits, &ret.guid, tone, which, lane.device(x, width));
            }
            x += width + GAP;
        }
    }

    /// One lane: `at`'s strip, its chain, and the returns it feeds.
    fn draw_lane(
        &mut self,
        scene: &mut Scene,
        hits: &mut Hits,
        at: usize,
        top: f64,
        height: f64,
        right: f64,
    ) {
        let Some(track) = self.tracks.get(at).cloned() else {
            return;
        };
        hits.lanes.push((track.guid.clone(), top, height));
        // The track's own strip, standing still while its chain
        // scrolls past it.
        let start = self.strip(scene, hits, &track, 0.0, top, height) + GAP;
        // A phase is an edge and a tint, not a bar over it: the devices
        // get the height.
        let lane = Lane {
            start,
            right,
            top,
            height,
            body_top: top + INSET + TINT_H + 2.0,
            body_h: (INSET.mul_add(-2.0, height) - TINT_H - 2.0).max(0.0),
        };
        let returns = self.returns(at);
        let chain_w = if self.rigs.borrow().contains_key(&track.guid) {
            self.rig_w(&track.guid, lane.body_h)
        } else {
            let store = self.tone.store.borrow();
            store.get(&track.guid).map_or(0.0, |tone| {
                grouped(&fx_chain(tone))
                    .iter()
                    .map(|(p, u)| {
                        self.group_w(&track.guid, tone, (*p, u), lane.body_h, &returns) + GAP
                    })
                    .sum()
            })
        };
        let rig = self.rigs.borrow().contains_key(&track.guid);
        // A rig's returns follow it; a chain's are in its Depth stage.
        let after = if rig {
            self.returns_w(
                &returns,
                lane.body_h,
                self.shut(&track.guid, Group::Returns),
            )
        } else {
            0.0
        };
        let most = (chain_w + after - (right - start) + INSET).max(0.0);
        let scroll = self
            .scroll
            .get(&track.guid)
            .copied()
            .unwrap_or(0.0)
            .min(most);
        self.scroll.insert(track.guid.clone(), scroll);
        // What scrolls is clipped to the right of the strip.
        hits.clip = vello::kurbo::Rect::new(start, top, right, top + height);
        let mut moving = Scene::new();
        if rig {
            let end = self.draw_rig(&mut moving, hits, &track, lane, start - scroll);
            // The returns it feeds, after a breath.
            self.draw_returns(
                &mut moving,
                hits,
                (&track.guid, &returns),
                lane,
                end - GAP + RETURNS_GAP,
            );
        } else {
            self.draw_chain(&mut moving, hits, (&track, &returns), lane, start - scroll);
        }
        let clip = hits.clip;
        anyrender::PaintScene::push_clip_layer(scene, Affine::IDENTITY, &clip);
        anyrender::PaintScene::append_scene(scene, moving, Affine::IDENTITY);
        anyrender::PaintScene::pop_layer(scene);
    }

    fn paint_scene(
        &mut self,
        mut ctx: Option<&mut dyn anyrender::RenderContext>,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        self.follow();
        self.dirty.set(false);
        let scale = scale.max(f64::EPSILON);
        let (w, h) = (f64::from(width) / scale, f64::from(height) / scale);
        let family = self.family();
        // Docked, the one lane is the pane; full size, every lane is
        // `LANE_H`, as many as the view has room for.
        let (lanes, lane_h) = if self.lanes > 1 {
            (crate::num::index(h / LANE_H).max(1), LANE_H)
        } else {
            (1, h)
        };
        // Never past the last of them: the stack stops at the end.
        self.down = self
            .down
            .min(family.len().saturating_sub(lanes.min(family.len())));
        // What the lanes show is what their tracks have: each chain read
        // from the engine before it is drawn — again where it changed.
        #[cfg(feature = "native")]
        {
            // The tracks first: finding a lane's returns reads the store.
            let shown: Vec<String> = family
                .iter()
                .skip(self.down)
                .take(lanes)
                .flat_map(|&at| std::iter::once(at).chain(self.returns(at)))
                .filter_map(|at| self.tracks.get(at).map(|t| t.guid.clone()))
                .collect();
            let mut store = self.tone.store.borrow_mut();
            if let Some(watch) = &self.fx_watch {
                for guid in watch.drain() {
                    store.changed(&guid);
                }
            }
            store.sync_tracks(shown);
        }
        let mut drawn = Scene::new();
        let mut hits = Hits::default();
        for (n, at) in family.into_iter().skip(self.down).take(lanes).enumerate() {
            self.draw_lane(
                &mut drawn,
                &mut hits,
                at,
                crate::num::coord(n) * lane_h,
                lane_h,
                w,
            );
        }
        // The faces' pictures: each its widget's, at device pixels, so a
        // shader is not drawn small and scaled up.
        let pictures = std::mem::take(&mut hits.pictures);
        self.animating.set(ctx.is_some() && !pictures.is_empty());
        let mut painters = self.painters.borrow_mut();
        painters.begin_frame(!self.painted && ctx.is_some());
        self.painted |= ctx.is_some();
        for (key, picture, at, clip) in pictures {
            let size = (
                crate::num::index(at.width * scale)
                    .try_into()
                    .unwrap_or(0_u32),
                crate::num::index(at.height * scale)
                    .try_into()
                    .unwrap_or(0_u32),
            );
            if size.0 < 2 || size.1 < 2 {
                continue;
            }
            let painted = painters.paint(
                ctx.as_mut().map(|c| reborrow(&mut **c)),
                &key,
                &picture,
                (size.0, size.1, scale),
            );
            let place = Affine::scale(1.0 / scale).then_translate((at.x, at.y).into());
            let alpha = painters.opacity(&key);
            if alpha < 1.0 {
                anyrender::PaintScene::push_layer(
                    &mut drawn,
                    vello::peniko::Mix::Normal,
                    alpha,
                    Affine::IDENTITY,
                    &clip,
                    None,
                    None,
                );
            } else {
                anyrender::PaintScene::push_clip_layer(&mut drawn, Affine::IDENTITY, &clip);
            }
            anyrender::PaintScene::append_scene(&mut drawn, painted, place);
            anyrender::PaintScene::pop_layer(&mut drawn);
        }
        // A face still waiting to be got ready draws next frame.
        if painters.waiting() {
            self.animating.set(true);
        }
        drop(painters);
        *self.hits.borrow_mut() = hits;
        let mut out = Scene::new();
        anyrender::PaintScene::append_scene(&mut out, drawn, Affine::scale(scale));
        out
    }
}

/// A phase's edge over `group`: the separator down its left and its
/// tint along the top — the theme's `fx.phase` if it has one. `open` is
/// whether it is folded, `None` for a phase with nothing in it.
fn phase_edge(
    scene: &mut Scene,
    palette: &crate::arrangement::Palette,
    font: &crate::text::Font,
    (name, tint): (&str, vello::peniko::Color),
    group: Panel,
    open: Option<bool>,
) {
    let theme = crate::skin::loaded();
    if let Some(theme) = theme.as_deref().filter(|t| t.has_role("fx.phase")) {
        let [r, g, b, a] = tint.to_rgba8().to_u8_array();
        let fields = crate::skin::values(&[
            ("phase.name", theme_runtime::Value::text(name)),
            (
                "phase.tint",
                theme_runtime::Value::Color(theme_runtime::Rgba::from_rgba8(r, g, b, a)),
            ),
            ("phase.blank", theme_runtime::Value::Bool(open.is_none())),
            ("phase.shut", theme_runtime::Value::Bool(open == Some(true))),
        ]);
        if theme.record(
            "fx.phase",
            (group.width, group.height),
            &fields,
            &mut crate::skin::canvas::Host { scene, font },
            Affine::translate((group.x, group.y)),
        ) {
            return;
        }
    }
    let faint = open.is_none();
    let fill = |scene: &mut Scene, color: vello::peniko::Color, rect: vello::kurbo::Rect| {
        anyrender::PaintScene::fill(
            scene,
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            color,
            None,
            &rect,
        );
    };
    // The separator: a hairline down the middle of the edge.
    let line_x = group.x + EDGE_W / 2.0 - 0.5;
    fill(
        scene,
        palette.divider,
        vello::kurbo::Rect::new(
            line_x,
            group.y + TINT_H + 18.0,
            line_x + 1.0,
            group.y + group.height,
        ),
    );
    // The tint, along the top of what it holds.
    let tint = if faint {
        tint.multiply_alpha(0.3)
    } else {
        tint
    };
    fill(
        scene,
        tint,
        vello::kurbo::Rect::new(
            group.x + EDGE_W,
            group.y,
            group.x + group.width,
            group.y + TINT_H,
        ),
    );
    if faint {
        // An empty phase says which it is, faintly, under its tint.
        crate::tcp::glyphs(
            scene,
            font,
            palette.text_faint,
            name,
            group.x + EDGE_W + 4.0,
            group.y + TINT_H + 12.0,
            9.0,
        );
        return;
    }
    edge_chevron(scene, palette, group, open == Some(true));
    if open == Some(true) {
        // Shut, it says which it is beside its chevron.
        crate::tcp::glyphs(
            scene,
            font,
            palette.text_dim,
            name,
            group.x + EDGE_W + 4.0,
            group.y + TINT_H + 12.0,
            9.0,
        );
    }
}

/// The returns' fold edge, `edge` tall: a hairline down it and its
/// chevron at the top.
fn returns_edge(scene: &mut Scene, palette: &crate::arrangement::Palette, edge: Panel, shut: bool) {
    let line_x = edge.x + EDGE_W / 2.0 - 0.5;
    anyrender::PaintScene::fill(
        scene,
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        palette.divider,
        None,
        &vello::kurbo::Rect::new(
            line_x,
            edge.y + TINT_H + 18.0,
            line_x + 1.0,
            edge.y + edge.height,
        ),
    );
    edge_chevron(scene, palette, edge, shut);
}

/// The chevron that folds a group, at the top of its edge: down open,
/// right shut.
fn edge_chevron(
    scene: &mut Scene,
    palette: &crate::arrangement::Palette,
    group: Panel,
    shut: bool,
) {
    let (cx, cy) = (group.x + EDGE_W / 2.0, group.y + TINT_H + 9.0);
    let mut chevron = vello::kurbo::BezPath::new();
    if shut {
        chevron.move_to((cx - 2.0, cy - 4.0));
        chevron.line_to((cx + 2.0, cy));
        chevron.line_to((cx - 2.0, cy + 4.0));
    } else {
        chevron.move_to((cx - 4.0, cy - 2.0));
        chevron.line_to((cx, cy + 2.0));
        chevron.line_to((cx + 4.0, cy - 2.0));
    }
    anyrender::PaintScene::stroke(
        scene,
        &vello::kurbo::Stroke::new(1.4)
            .with_caps(vello::kurbo::Cap::Round)
            .with_join(vello::kurbo::Join::Round),
        Affine::IDENTITY,
        palette.text_dim,
        None,
        &chevron,
    );
}

/// A frame face's modifiers: Shift fine, Ctrl/Cmd the alternate, as
/// frame's own player has them.
fn face_mods(e: &BlitzPointerEvent) -> frame_live::Modifiers {
    frame_live::Modifiers {
        fine: e.mods.shift(),
        alt: e.mods.ctrl() || e.mods.meta(),
        option: e.mods.alt(),
    }
}

/// `ctx` for as long as it is borrowed here: the shorter-lived trait
/// object a painter takes.
fn reborrow(ctx: &mut dyn anyrender::RenderContext) -> &mut dyn anyrender::RenderContext {
    ctx
}

/// How wide a return's effect is: a frame face as wide as its design is
/// under its header (a 960 × 164 panel is a long one), anything else a
/// device.
fn ret_device_w(tone: &crate::tone::Tone, which: Which, body_h: f64) -> f64 {
    crate::faces::aspect(which, tone).map_or(DEVICE_W, |aspect| {
        (face_h(body_h) * aspect).clamp(DEVICE_W, DEVICE_W * 4.0)
    })
}

/// A return's one effect, by what it is for — `None` for a channel.
const fn main_unit(role: crate::tone::Role) -> Option<Which> {
    use crate::tone::Role;
    match role {
        Role::Reverb => Some(Which::Reverb),
        Role::Delay => Some(Which::Delay),
        Role::Wide => Some(Which::Wide),
        Role::Pitch => Some(Which::Pitch),
        _ => None,
    }
}

/// The FX as it draws for `rows` with `store`'s settings, `lanes` of
/// them, `width` × `height` at 1:1 — for the bench's shot. `folded` are
/// the phases folded, as a press on their edges would leave them, and
/// `returns_shut` the returns' effects folded away.
#[must_use]
pub fn shot(
    rows: Vec<(daw_proto::Track, u32)>,
    store: crate::tone::Store,
    lanes: usize,
    (folded, returns_shut): (&[MixPhase], bool),
    font: crate::text::Font,
    (width, height): (u32, u32),
) -> Scene {
    // The reverbs' tails, rendered before the shot rather than after.
    store.prerender_tails();
    let tone = Rc::new(Shared::default());
    *tone.store.borrow_mut() = store;
    let mut row = DeviceRow::new(tone, rows, lanes.max(1), font);
    row.watch = None;
    let guids: Vec<String> = row.tracks.iter().map(|t| t.guid.clone()).collect();
    for guid in guids {
        for phase in folded {
            row.folds.insert((guid.clone(), Group::Phase(*phase)), true);
        }
        row.folds.insert((guid, Group::Returns), returns_shut);
    }
    row.paint_scene(None, width, height, 1.0)
}

#[cfg(feature = "native")]
impl blitz_dom::node::Widget for DeviceRow {
    fn handle_event(&mut self, event: &UiEvent) {
        self.event(event);
    }

    fn needs_redraw(&self) -> bool {
        self.dirty.get() || self.animating.get() || self.heard()
    }

    fn paint(
        &mut self,
        render_ctx: &mut dyn anyrender::RenderContext,
        _styles: &blitz_dom::node::ComputedStyles,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        self.paint_scene(Some(render_ctx), width, height, scale)
    }
}

#[cfg(feature = "web")]
impl crate::web_host::Hosted for DeviceRow {
    fn paint(&mut self, width: u32, height: u32, scale: f64) -> Scene {
        self.paint_scene(None, width, height, scale)
    }
    fn event(&mut self, event: &UiEvent) {
        DeviceRow::event(self, event);
    }
}

#[cfg(test)]
mod strip_tests {
    use super::{DeviceRow, Shared};
    use crate::mcp::Control;
    use blitz_traits::events::{
        BlitzPointerEvent, BlitzPointerId, Modifiers, MouseEventButton, MouseEventButtons, Point,
        PointerCoords, PointerDetails, UiEvent,
    };
    use std::rc::Rc;

    /// A mouse at a window point (whole pixels).
    fn mouse((x, y): (f64, f64)) -> BlitzPointerEvent {
        let whole = |v: f64| f32::from(u16::try_from(crate::num::index(v)).unwrap_or(0));
        let (x, y) = (whole(x), whole(y));
        BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords: PointerCoords {
                page_x: x,
                page_y: y,
                screen_x: x,
                screen_y: y,
                client_x: x,
                client_y: y,
            },
            button: MouseEventButton::Main,
            buttons: MouseEventButtons::Primary,
            mods: Modifiers::default(),
            details: PointerDetails::default(),
            element: Point { x, y },
            active_pointers: std::sync::Arc::default(),
        }
    }

    /// A row of one lane over two tracks, painted once so its strips are
    /// where a press can find them.
    fn row() -> DeviceRow {
        let tracks: Vec<(daw_proto::Track, u32)> = (0..2)
            .map(|i| {
                (
                    daw_proto::Track {
                        guid: format!("t{i}"),
                        name: format!("Track {i}"),
                        selected: i == 0,
                        volume: 0.5,
                        ..daw_proto::Track::default()
                    },
                    0,
                )
            })
            .collect();
        let font = crate::text::Font::embedded().expect("the embedded font");
        let mut row = DeviceRow::new(Rc::new(Shared::default()), tracks, 1, font);
        row.watch = None;
        let _ = row.paint_scene(
            None,
            1600,
            u32::try_from(crate::num::index(super::LANE_H)).unwrap_or(400),
            1.0,
        );
        row
    }

    /// The window point at the middle of the first lane strip's `control`.
    fn at(row: &DeviceRow, control: Control) -> (f64, f64) {
        let hits = row.hits.borrow();
        let (guid, left, top) = hits.strips.first().expect("a strip drawn");
        let strips = row.strips.borrow();
        let mixer = &strips.get(guid).expect("its recording").mixer;
        let (x0, ..) = mixer.strip_box(0).expect("its box");
        let rect = mixer
            .strip(0)
            .and_then(|s| s.rect(control))
            .expect("the control drawn");
        (left + x0 + rect.center().x, top + rect.center().y)
    }

    fn track<'a>(row: &'a DeviceRow, guid: &str) -> &'a daw_proto::Track {
        row.tracks
            .iter()
            .find(|t| t.guid == guid)
            .expect("the track")
    }

    /// A click on a lane strip's mute mutes its track; a drag up its
    /// fader raises it — the strip in the FX is the mixer's, to use.
    #[test]
    fn a_lane_strip_mutes_and_its_fader_moves() {
        let mut row = row();
        let guid = row.hits.borrow().strips[0].0.clone();
        assert!(!track(&row, &guid).muted);

        let mute = mouse(at(&row, Control::Mute));
        row.event(&UiEvent::PointerDown(mute.clone()));
        row.event(&UiEvent::PointerUp(mute));
        assert!(track(&row, &guid).muted, "the mute is pressed");

        let was = track(&row, &guid).volume;
        let (x, y) = at(&row, Control::Volume);
        row.event(&UiEvent::PointerDown(mouse((x, y))));
        row.event(&UiEvent::PointerMove(mouse((x, y - 40.0))));
        row.event(&UiEvent::PointerUp(mouse((x, y - 40.0))));
        assert!(track(&row, &guid).volume > was, "the fader went up");
    }
}
