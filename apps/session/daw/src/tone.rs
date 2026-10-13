//! The Tone rack — EQ, compressor and saturation, inside the strip.
//!
//! The mix phase after Balance is Tone, and it is three processors:
//! compression, EQ, saturation. The parallel filters and the parallel
//! compressors are a different phase and are deliberately not here.
//!
//! A channel strip that shows those three is a channel you can mix
//! without opening a plugin window, which is the whole point — a mix is
//! a comparison between tracks, and a comparison you have to make one
//! floating window at a time is not a comparison.
//!
//! # It draws the plugins' own curves
//!
//! Every response in this module comes from the function the plugin's
//! own editor draws with:
//!
//! | panel | curve from |
//! |---|---|
//! | EQ | `eq_ui::eq_graph_response::calculate_combined_response` |
//! | Compressor | `comp_ui::comp_graph_svg::compress_transfer` |
//! | Saturation | `saturate_dsp::preamp::analysis::transfer_curve` |
//!
//! None of the maths is here, and that is the point: a curve in the
//! strip that disagreed with the curve in the plugin window would be
//! worse than no curve at all, because it would be believed. This module
//! samples those functions into polylines and places them in a box.
//!
//! # What it does not do yet
//!
//! Move. The curves depend only on the parameters, so they are recorded
//! into the mixer's scene once and replayed under the transform like
//! everything else. The parts that DO move — gain reduction, the
//! spectrum behind the EQ — are metering, and metering arrives through
//! `daw_ui::widgets::trace` in a live pass over the top, so that a track
//! whose processing was rendered offline still animates.

use anyrender::{PaintScene, Scene};
use delay_dsp::engine::{DelayStyle, Family as DelayFamily};
use eq_ui::eq_graph_interaction::{self as interaction, GraphMapper, Mods};
use eq_ui::eq_graph_model::{EqBand, EqBandShape, StereoMode};
use eq_ui::eq_graph_response::calculate_combined_response;
use fts_audio_ui::axis::{DbAxis, FreqAxis};
use reverb_dsp::algorithm::{AlgorithmType, Family as RoomFamily};
use saturate_dsp::preamp::{Circuit, ClassAPreamp, SideShaper};
use vello::kurbo::{Affine, BezPath, Line, Rect, Shape, Stroke};
use vello::peniko::{Color, Fill};

use crate::arrangement::Palette;
use crate::live::Meters;
use crate::text::Font;

/// The sample rate the curves are drawn against.
///
/// A display constant, not an audio one: the EQ's response at 20 kHz
/// bends with the rate, and a strip that redrew its curve when the
/// device changed would be showing the device rather than the setting.
const DISPLAY_RATE: f64 = 48_000.0;

/// How many points a curve is sampled at.
///
/// One per ~2px of a 200-wide panel. Beyond that the polyline is finer
/// than the pixels and costs recording time for nothing — and this runs
/// once per track at project open, not per frame.
const SAMPLES: usize = 96;

/// The three processors of the Tone phase, for one track.
///
/// Deliberately the plugins' own parameter types rather than a summary
/// of them. A rack that held its own reduced idea of "an EQ" would have
/// to be widened every time a band gained a shape, and would be drawing
/// something the plugin cannot produce.
#[derive(Clone, Debug)]
pub struct Tone {
    /// Rescue.
    pub rescue_eq: Vec<EqBand>,
    pub gate: Gate,
    pub rescue_comp: Comp,
    /// Tone.
    pub eq: Vec<EqBand>,
    pub comp: Comp,
    pub sat: ClassAPreamp,
    /// Which of the plugin's profiles the stage is — an index into
    /// `saturate_profiles::PROFILES`. The selector picks one; `sat` is
    /// what `apply` made of it, plus whatever the grips moved since.
    pub sat_profile: usize,
    /// The quantiser after the stage, which only the digital profiles
    /// switch on. Kept so the curve can draw its steps.
    pub sat_digital: saturate_dsp::digital::DigitalStage,
    /// The saturator's emphasis EQ: six bands before the stage, exactly
    /// inverted after it, so the curve chooses what distorts and not
    /// what comes out (`fx.sat.emphasis`). Bell and shelves only — a
    /// cut has no inverse. At 0 dB a band is idle.
    pub sat_emphasis: [saturate_dsp::emphasis::EmphBand; saturate_dsp::emphasis::BANDS],
    /// Whether the saturator's face shows its emphasis EQ in place of
    /// its picture.
    pub sat_emphasis_view: bool,
    /// Polish.
    pub de_ess: Suppress,
    /// The Polish EQ: the narrow, surgical cuts — the resonances a room
    /// or a body puts in — as bands on the same graph the Tone EQ has,
    /// with the same spectral bands. It was a suppressor of its own,
    /// finding them by comparing the spectrum with its own average;
    /// the EQ's spectral bands do the same job, on a graph you can
    /// already read.
    pub polish_eq: Vec<EqBand>,
    /// Relational.
    pub space: Vec<EqBand>,
    /// Depth.
    pub delay: Echo,
    pub reverb: Room,
    /// The reverb's own controls past the room: its modulation, its tone,
    /// and the cuts on what it returns.
    pub reverb_extra: ReverbExtra,
    /// The machine the reverb runs in, when it is one: it wears that
    /// machine's face whatever the algorithm, the algorithm its program.
    pub reverb_machine: Option<ReverbMachine>,
    /// The advanced Depth units' own EQs: ahead of the effect, and
    /// after it. Inside the one instance — a reverb return does not
    /// carry three plugins, it carries one with an EQ at each end.
    pub pre_eq: Vec<EqBand>,
    pub post_eq: Vec<EqBand>,
    /// The reverb's Decay Rate EQ: bands of decay-TIME multipliers over
    /// frequency, drawn as gain where ±12 dB is ×0.25..×4. The plugin's
    /// `decay_bands`, in the shape the EQ graph already draws.
    pub decay_eq: Vec<EqBand>,
    /// The widener: 0 is mono, 1 is as recorded, 2 is pushed past the
    /// speakers.
    pub wide: f32,
    /// The pitch shifter, in semitones, and how much of it is heard.
    pub pitch: i32,
    pub pitch_mix: f32,
    /// The modulation: which of the six engines, how fast and how deep.
    pub modulation: Wobble,
    /// Which compressor face the Tone compressor wears (an 1176, an
    /// LA-2A…) — see `plugin_faces`.
    pub comp_face: crate::plugin_faces::CompFace,
    /// The track's processors as the engine has them, in its chain's
    /// order — the devices the FX row shows ([`crate::tone_read`]); `None`
    /// until the chain is read (the rack's placeholder chain meanwhile).
    pub engine_units: Option<Vec<Which>>,
    /// Processors on the track the row has no face for, by name and
    /// whether each is on.
    pub engine_others: Vec<(String, bool)>,
    /// A second compressor after the first, in its own face — a kick's
    /// 1176 into an LA-2A. Drawn under the first, the two sharing the
    /// height.
    pub comp_after: Option<(Comp, crate::plugin_faces::CompFace)>,
    /// Each compressor's sidechain EQ: what it listens to, never what
    /// comes out — so any shape, cuts above all. By [`Sidechain`].
    pub sidechain: [Vec<EqBand>; 3],
    /// Whose sidechain EQ is open, filling its compressor's column (both
    /// faces of a stacked pair) in place of the faces.
    pub sc_view: Option<Sidechain>,
    /// A drum: no modulation unit in its chain.
    pub drum: bool,
    /// What this track is FOR, which decides its chain — see [`Role`].
    pub role: Role,
    /// The presets this track keeps, brighter to darker, and which of
    /// them was loaded last. A row of chips at the top of the rack.
    pub presets: Vec<Preset>,
    pub preset: Option<usize>,
    /// Which of the three are switched out.
    pub bypass: Bypass,
    /// How far the EQ graph is zoomed, as an index into the plugin's
    /// own [`eq_ui::eq_graph_model::DB_RANGE_STEPS`].
    ///
    /// An INDEX rather than a number of decibels, because the zoom is a
    /// list of stops the plugin publishes and a rack that invented its
    /// own would disagree with the editor about what "one step out"
    /// means. Per track, like everything else here: a vocal worked at
    /// ±3 and a room mic at ±18 is the normal case, not a special one.
    pub eq_range: i32,
}

/// What a track is for, which decides which units its rack shows.
///
/// Not every track carries every step. A lead vocal is a channel and
/// gets the whole chain; a delay return is ONE delay — a de-esser on
/// the way in, the delay itself, an EQ on the way out — and a reverb
/// return is one reverb with a de-esser, an EQ at each end and its own
/// decay-rate EQ. Decided from where the track sits (the folder it is
/// in) and what it is called, at seed time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// An instrument or a voice: the eleven-unit chain.
    Channel,
    /// A folder. Its level and its mute; the processing is on what it
    /// sums.
    Bus,
    /// A delay return: the advanced delay, one instance.
    Delay,
    /// A reverb return: the advanced reverb, one instance.
    Reverb,
    /// A widener.
    Wide,
    /// A pitch shifter.
    Pitch,
    /// A fundamental: the Fund track under a piece's Sum, and its Sub.
    /// A band-pass tuned to the note, a gate to keep it tight, a
    /// saturator — and nothing else, because the track is one note
    /// blended in under the piece.
    Fund,
    /// A trigger: a spike track for a sampler, with nothing on it yet.
    Trig,
    /// One half of a stereo pair. The pair's folder is the instrument
    /// and carries the processing; a half is almost never processed on
    /// its own, so it gets nothing.
    Half,
    /// A parallel compressor: fed the kit, crushed, blended back under
    /// it. Its compressor first, an EQ to shape what comes back, and
    /// a saturator for the ones that are meant to crunch.
    Parallel,
    /// The dry member of a parallel group: the kit as it is, with an
    /// EQ and nothing that changes its dynamics — it is what the
    /// others are balanced against.
    Dry,
    /// A capture under a guitar's layer — a channel the part was
    /// doubled into, an amp, a mic, the DI — or an arrangement folder
    /// that only holds layers. The part is processed on its layer, so
    /// these carry nothing: see [`guitar_role`].
    Source,
}

impl Role {
    /// Decide a track's role from its name, whether it is a folder,
    /// and the names of the folders above it, nearest last.
    #[must_use]
    pub fn of(name: &str, is_folder: bool, ancestors: &[String]) -> Self {
        if is_folder {
            return Self::Bus;
        }
        let lower = name.to_lowercase();
        // A bus that is a leaf — the three electric-guitar buses, a
        // stem bus with nothing under it yet — is still a bus.
        if lower.ends_with(" bus") || lower.starts_with("gtr ") {
            return Self::Bus;
        }
        if lower.starts_with("wide") || lower.starts_with("widen") {
            return Self::Wide;
        }
        if lower.starts_with("oct") || lower.starts_with("pitch") {
            return Self::Pitch;
        }
        // A piece's own sends and helpers, wherever they sit: its verb
        // is a reverb return, and its fundamental, its sub and its
        // trigger are the one-note tracks.
        if lower == "verb" || lower == "reverb" || lower.starts_with("room sim") {
            return Self::Reverb;
        }
        if lower == "delay" {
            return Self::Delay;
        }
        if lower == "fund" || lower == "sub" {
            return Self::Fund;
        }
        if lower == "dry" {
            return Self::Dry;
        }
        if lower.ends_with("trig") {
            return Self::Trig;
        }
        if is_pair_half(name) {
            return Self::Half;
        }
        for folder in ancestors.iter().rev() {
            let folder = folder.to_lowercase();
            if folder.starts_with("delay") {
                return Self::Delay;
            }
            if folder.starts_with("verb")
                || folder.starts_with("reverb")
                || folder.starts_with("ambience")
                || folder.starts_with("plate")
                || folder.starts_with("hall")
                || folder.starts_with("spring")
            {
                return Self::Reverb;
            }
            // Movement — chorus, flanger — has no face of its own yet;
            // the widener's is the nearest picture, and it stands in.
            if folder.starts_with("mod") || folder.starts_with("chorus") {
                return Self::Wide;
            }
            if folder.starts_with("pitch") {
                return Self::Pitch;
            }
            if folder.starts_with("wide") {
                return Self::Wide;
            }
            if folder.starts_with("compress") {
                return Self::Parallel;
            }
        }
        Self::Channel
    }

    /// The chain this role shows, or `None` for the channel's own.
    #[must_use]
    pub const fn panels(self) -> Option<&'static [Which]> {
        match self {
            Self::Channel => None,
            Self::Bus => Some(&BUS_CHAIN),
            Self::Delay => Some(&DELAY_CHAIN),
            Self::Reverb => Some(&REVERB_CHAIN),
            Self::Wide => Some(&WIDE_CHAIN),
            Self::Pitch => Some(&PITCH_CHAIN),
            Self::Fund => Some(&FUND_CHAIN),
            Self::Trig | Self::Half | Self::Source => Some(&[]),
            Self::Parallel => Some(&PARALLEL_CHAIN),
            Self::Dry => Some(&DRY_CHAIN),
        }
    }
}

/// A fundamental: the gate that keeps it tight, the band-pass tuned to
/// the note, and the saturator that gives it an edge. In the order the
/// rack reads — Rescue, then Tone — which is also the order the signal
/// wants: gate the bleed before the filter rings on it.
pub const FUND_CHAIN: [Which; 3] = [Which::Gate, Which::Eq, Which::Sat];

/// A delay return: the de-esser on the way in, the delay itself with
/// its machine selector and knobs, and an EQ on the way out.
pub const DELAY_CHAIN: [Which; 5] = [
    Which::Presets,
    Which::DeEssIn,
    Which::Delay,
    Which::Knobs,
    Which::PostEq,
];

/// A reverb return: de-esser, an EQ into the space, the space with its
/// selector and knobs, its decay-rate EQ, and an EQ out.
pub const REVERB_CHAIN: [Which; 7] = [
    Which::Presets,
    Which::DeEssIn,
    Which::PreEq,
    Which::Reverb,
    Which::Knobs,
    Which::DecayEq,
    Which::PostEq,
];

/// A bus: what it sums is processed elsewhere.
pub const BUS_CHAIN: [Which; 2] = [Which::Eq, Which::Comp];

/// A parallel compressor's chain: the compressor is the point, then
/// what shapes the return, then what dirties it.
pub const PARALLEL_CHAIN: [Which; 4] = [Which::Presets, Which::Comp, Which::Eq, Which::Sat];

/// The dry member's chain: an EQ, and nothing that touches dynamics.
pub const DRY_CHAIN: [Which; 1] = [Which::Eq];

pub const WIDE_CHAIN: [Which; 2] = [Which::Presets, Which::Wide];
pub const PITCH_CHAIN: [Which; 2] = [Which::Presets, Which::Pitch];

/// A named setting of the whole rack, kept on the track.
///
/// Stored as a whole `Tone` rather than a diff, so loading one is a
/// copy and nothing is left over from the setting before it. The stored
/// tone carries no presets of its own.
#[derive(Clone, Debug)]
pub struct Preset {
    pub name: String,
    pub tone: Box<Tone>,
}

/// The EQ graph's default zoom.
///
/// The plugin's own default (±6 dB), not a second opinion — see
/// `DEFAULT_DB_RANGE` there for why ±3 was too tight to work in.
pub const DEFAULT_EQ_RANGE: i32 = 1;

impl Tone {
    /// The EQ graph's dB range, top to bottom.
    #[must_use]
    pub fn eq_db_range(&self) -> f64 {
        eq_ui::eq_graph_model::db_range_for_index(self.eq_range)
    }

    /// Zoom the graph by `steps`, positive being further out.
    ///
    /// Returns whether it moved, so a caller knows whether to re-record
    /// rather than guessing.
    pub fn zoom_eq(&mut self, steps: i32) -> bool {
        let last = i32::try_from(eq_ui::eq_graph_model::DB_RANGE_STEPS.len())
            .unwrap_or(1)
            .saturating_sub(1);
        let to = self.eq_range.saturating_add(steps).clamp(0, last);
        let moved = to != self.eq_range;
        self.eq_range = to;
        moved
    }

    /// The band set a given EQ panel edits.
    ///
    /// Three panels share one drawing and one interaction model and
    /// differ only in which list they are about — so this is the one
    /// place that mapping lives, and adding a fourth EQ is one arm.
    #[must_use]
    pub fn bands(&mut self, which: Which) -> Option<&mut Vec<EqBand>> {
        match which {
            Which::RescueEq => Some(&mut self.rescue_eq),
            Which::Eq => Some(&mut self.eq),
            Which::Space => Some(&mut self.space),
            Which::PreEq => Some(&mut self.pre_eq),
            Which::PostEq => Some(&mut self.post_eq),
            Which::DecayEq => Some(&mut self.decay_eq),
            Which::PolishEq => Some(&mut self.polish_eq),
            _ => None,
        }
    }

    /// The compressor a given panel edits.
    #[must_use]
    pub const fn compressor(&mut self, which: Which) -> Option<&mut Comp> {
        match which {
            Which::RescueComp => Some(&mut self.rescue_comp),
            Which::Comp => Some(&mut self.comp),
            _ => None,
        }
    }

    /// The chain this track shows, given the chain the window would
    /// show a channel.
    ///
    /// `default` is what the phase filter and the tone toggle decided
    /// for a channel — empty when the rack is off — so an FX return
    /// follows those decisions and then draws its own units instead.
    #[must_use]
    pub fn panels<'a>(&self, default: &'a [Which]) -> &'a [Which] {
        if default.is_empty() {
            return default;
        }
        self.role.panels().unwrap_or(default)
    }

    /// Whether this track wants the focused column layout — see
    /// `strip::Layout`. A channel's chain is taller than the rack's
    /// share and earns the column; a return's is short, and a focused
    /// return stays stacked.
    #[must_use]
    pub fn wants_column(&self) -> bool {
        self.role == Role::Channel
    }

    /// The band set a panel draws, to read.
    #[must_use]
    pub fn bands_ref(&self, which: Which) -> &[EqBand] {
        match which {
            Which::RescueEq => &self.rescue_eq,
            Which::Space => &self.space,
            Which::PreEq => &self.pre_eq,
            Which::PostEq => &self.post_eq,
            Which::DecayEq => &self.decay_eq,
            Which::PolishEq => &self.polish_eq,
            _ => &self.eq,
        }
    }

    /// Load one of this track's presets into every setting the rack
    /// draws, keeping the presets and the role.
    ///
    /// The whole visualiser follows, because the settings ARE the
    /// picture: a preset is not a name on a chip, it is what the chain
    /// becomes when you click it.
    pub fn load_preset(&mut self, index: usize) {
        let Some(preset) = self.presets.get(index) else {
            return;
        };
        let loaded = (*preset.tone).clone();
        let presets = std::mem::take(&mut self.presets);
        let role = self.role;
        // A preset is a sound, not the hardware it runs in.
        let machine = self.reverb_machine;
        *self = loaded;
        self.presets = presets;
        self.role = role;
        self.reverb_machine = machine;
        self.preset = Some(index);
    }

    /// Which machine the saturator reads as: the quantiser if a digital
    /// profile is in, else what the shapers say.
    #[must_use]
    pub fn sat_circuit(&self) -> Circuit {
        if saturate_profiles::PROFILES
            .get(self.sat_profile)
            .is_some_and(|p| p.voicing.digital)
        {
            Circuit::Steps
        } else {
            self.sat.circuit()
        }
    }

    /// Point the stage at a profile, through the plugin's own `apply`,
    /// keeping the drive knob where it was and the mix as it is.
    ///
    /// The drive is carried as a KNOB position rather than a gain: a
    /// fuzz travels further than a tape machine, and carrying the gain
    /// across would land a modest tape drive at the top of the fuzz.
    pub fn set_sat_profile(&mut self, index: usize) {
        let Some(profile) = saturate_profiles::PROFILES.get(index) else {
            return;
        };
        let was_scale = saturate_profiles::PROFILES
            .get(self.sat_profile)
            .map_or(1.0, |p| p.voicing.drive_scale)
            .max(f32::EPSILON);
        let knob = ((self.sat.drive - 1.0) / (15.0 * was_scale)).clamp(0.0, 1.0);
        let mix = self.sat.mix;
        let controls = saturate_profiles::Controls {
            drive: knob,
            mix,
            ..saturate_profiles::Controls::default()
        };
        saturate_profiles::apply(profile, &controls, &mut self.sat, &mut self.sat_digital);
        self.sat.mix = mix;
        self.sat_profile = index;
    }

    /// A click on a circuit chip: the plugin's own rail rule — the
    /// family's first profile, or the next one if you are already in it.
    pub fn choose_sat_family(&mut self, category: usize) {
        let to = saturate_profiles::rail_click_target(self.sat_profile, category);
        self.set_sat_profile(to);
    }

    /// The next profile, wrapping — what a click on the glyph does.
    pub fn cycle_sat(&mut self) {
        let next = self.sat_profile.saturating_add(1);
        let to = if next >= saturate_profiles::PROFILES.len() {
            0
        } else {
            next
        };
        self.set_sat_profile(to);
    }

    /// The wet/dry mix a given panel edits.
    #[must_use]
    pub const fn mix(&mut self, which: Which) -> Option<&mut f32> {
        match which {
            Which::Delay => Some(&mut self.delay.mix),
            Which::Reverb => Some(&mut self.reverb.mix),
            Which::Sat => Some(&mut self.sat.mix),
            _ => None,
        }
    }

    /// The suppressor a given panel edits.
    #[must_use]
    pub const fn suppressor(&mut self, which: Which) -> Option<&mut Suppress> {
        match which {
            Which::DeEss | Which::DeEssIn => Some(&mut self.de_ess),
            _ => None,
        }
    }

    /// Render every reverb tail this rack needs, now, on this thread.
    ///
    /// For a bench or a test that must be deterministic — see
    /// [`crate::live::render_tail_now`].
    pub fn prerender_tails(&self) {
        crate::live::render_tail_now(self.reverb.key());
    }

    /// Every panel with a threshold, as one number.
    ///
    /// The compressors, the gate and the two suppressors all have a red
    /// line you drag, and they all mean "where this starts acting" —
    /// which is why one grip serves them and why the ranges differ.
    #[must_use]
    pub const fn threshold(&self, which: Which) -> Option<(f32, f32, f32)> {
        match which {
            Which::RescueComp => Some((self.rescue_comp.threshold, -60.0, 0.0)),
            Which::Comp => Some((self.comp.threshold, -60.0, 0.0)),
            Which::Gate => Some((self.gate.threshold, -80.0, 0.0)),
            // A suppressor's threshold is how far ABOVE its own average
            // a peak has to stand, so its range is small and positive.
            Which::DeEss | Which::DeEssIn => Some((self.de_ess.threshold, 0.0, 24.0)),
            _ => None,
        }
    }

    /// And setting it, clamped to that panel's own range.
    pub const fn set_threshold(&mut self, which: Which, to: f32) {
        let Some((_, low, high)) = self.threshold(which) else {
            return;
        };
        let to = if to < low {
            low
        } else if to > high {
            high
        } else {
            to
        };
        match which {
            Which::RescueComp => self.rescue_comp.threshold = to,
            Which::Comp => self.comp.threshold = to,
            Which::Gate => self.gate.threshold = to,
            Which::DeEss | Which::DeEssIn => self.de_ess.threshold = to,
            _ => {}
        }
    }

    /// The next stop, wrapping — what a click on the readout does.
    ///
    /// Wrapping rather than stopping because a chip you click is a
    /// cycle: at the last stop the only useful thing left to do is go
    /// back to the first, and a click that does nothing reads as a
    /// broken control.
    pub fn cycle_eq_range(&mut self) {
        let last = i32::try_from(eq_ui::eq_graph_model::DB_RANGE_STEPS.len()).unwrap_or(1);
        self.eq_range = self.eq_range.saturating_add(1).rem_euclid(last.max(1));
    }
}

/// The reverb's controls a room does not have: FTS Reverb's Modulation and
/// Tone, and the low and high cuts on its return (its post-EQ's first and
/// last bands).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReverbExtra {
    /// 0..1.
    pub modulation: f32,
    /// −1 (dark) … 1 (bright); 0 the plugin's middle.
    pub tone: f32,
    /// Hz: 20 is no cut.
    pub low_cut: f32,
    /// Hz: 20 000 is no cut.
    pub high_cut: f32,
}

impl Default for ReverbExtra {
    fn default() -> Self {
        Self {
            modulation: 0.2,
            tone: 0.0,
            low_cut: 20.0,
            high_cut: 20_000.0,
        }
    }
}

/// Which processors are bypassed.
///
/// Per PROCESSOR, not per rack: bypassing is how you check a decision,
/// and the question is almost always "what does this track sound like
/// without the compressor", not "without any of it". A whole-rack
/// switch would make the common comparison the one you cannot make.
///
/// A bitset rather than a field each, because the chain grows: it was
/// three bools, and every processor added meant three more edits in
/// three functions that could disagree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bypass {
    out: u32,
}

impl Bypass {
    /// Wide enough for every [`Which`]: the chain passed sixteen units
    /// when the widener, the pitch shifter and the presets row arrived,
    /// and a `u16` bitset was silently aliasing their bits onto the
    /// rescue EQ, the gate and the rescue compressor — an overflowing
    /// shift in a debug build, a wrapped one in release, so bypassing
    /// the widener switched out the rescue EQ instead.
    const fn bit(which: Which) -> u32 {
        1_u32 << (which as u32)
    }

    #[must_use]
    pub const fn is(self, which: Which) -> bool {
        self.out & Self::bit(which) != 0
    }

    pub const fn toggle(&mut self, which: Which) {
        self.out ^= Self::bit(which);
    }

    /// Switch `which` out (`true`) or in.
    pub const fn set(&mut self, which: Which, out: bool) {
        if out {
            self.out |= Self::bit(which);
        } else {
            self.out &= !Self::bit(which);
        }
    }

    /// Whether anything at all is switched out — for a caller that
    /// wants to say so without asking eleven times.
    #[must_use]
    pub const fn any(self) -> bool {
        self.out != 0
    }
}

/// What a suppressor is set to, in one short line.
fn suppression(set: Suppress) -> String {
    format!("{:.0}dB · {:.0}%", set.threshold, set.depth * 100.0)
}

/// A modulation unit, as its display needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wobble {
    /// Which machine: chorus, flanger, phaser, tremolo, vibrato, rotary.
    pub engine: modulation_ui::viz::Engine,
    /// How fast, in Hz.
    pub rate: f32,
    /// How deep, 0 to 1.
    pub depth: f32,
    /// How much of it is heard.
    pub mix: f32,
}

impl Default for Wobble {
    fn default() -> Self {
        Self {
            engine: modulation_ui::viz::Engine::Chorus,
            rate: 0.8,
            depth: 0.5,
            mix: 0.5,
        }
    }
}

/// A gate, as its display needs it.
///
/// The compressor's mirror: it acts BELOW its threshold rather than
/// above, which is the whole difference and the reason the two cannot
/// share a display without saying which way they work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gate {
    /// Where it opens, in dBFS.
    pub threshold: f32,
    /// How far down it takes what stays shut. Not silence: a gate that
    /// closes completely turns a room into a series of holes, and the
    /// number you actually reach for is how much LESS of it you want.
    pub range: f32,
    /// How fast it opens.
    pub attack: f32,
    /// How long it stays open after the signal drops back.
    pub hold: f32,
    /// And how fast it closes then.
    pub release: f32,
}

impl Default for Gate {
    fn default() -> Self {
        Self {
            threshold: -40.0,
            range: -18.0,
            attack: 1.0,
            hold: 60.0,
            release: 180.0,
        }
    }
}

/// A spectral suppressor, as its display needs it.
///
/// The de-esser and the resonance suppressor are one processor with two
/// jobs: find where the spectrum stands proud of its own average, and
/// pull those places down. The de-esser looks only in the sibilance
/// range and the suppressor looks everywhere, which is a BAND and not a
/// different algorithm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Suppress {
    /// How far above its own average a peak has to stand before it is
    /// treated as a resonance, in dB.
    pub threshold: f32,
    /// How much of what it finds it takes off, 0 to 1.
    pub depth: f32,
    /// How narrow a peak has to be to count — the width of the average
    /// it is compared against, in octaves. Wide, and a whole region
    /// reads as one resonance; narrow, and nothing does.
    pub sharpness: f32,
    /// The range it listens in.
    pub low: f32,
    pub high: f32,
}

impl Suppress {
    /// The de-esser's defaults: sibilance only.
    #[must_use]
    pub const fn sibilance() -> Self {
        Self {
            threshold: 4.0,
            depth: 0.6,
            sharpness: 0.5,
            low: 4_000.0,
            high: 12_000.0,
        }
    }

    /// And the resonance suppressor's: the whole spectrum, gentler.
    #[must_use]
    pub const fn broadband() -> Self {
        Self {
            threshold: 6.0,
            depth: 0.45,
            sharpness: 0.33,
            low: 100.0,
            high: 18_000.0,
        }
    }
}

/// A delay, as its display needs it.
///
/// Time, feedback and mix — the three that decide what the repeats
/// LOOK like, which is what the panel draws. The rest of the plugin's
/// parameters (wow, flutter, drive, duck) change how a repeat sounds
/// rather than where it lands, and a strip-width picture cannot show
/// them without lying about what it is showing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Echo {
    pub time: f32,
    pub feedback: f32,
    pub mix: f32,
    /// How dark the repeats are, 0 bright .. 1 dark.
    pub tone: f32,
    /// How wide, 0 mono .. 1 full.
    pub width: f32,
    /// Which machine makes the repeats. Not a picture of a knob: the
    /// plugin's fourteen styles are fourteen different engines, and
    /// the strip draws the family the style belongs to the way the
    /// plugin's own faces do — a tape softens, a chip smears, a
    /// shimmer climbs.
    pub style: DelayStyle,
    /// Locked to the beat: which division (`crate::beats::DELAY`), or
    /// `None` running free at `time`.
    pub sync: Option<usize>,
}

impl Default for Echo {
    fn default() -> Self {
        Self {
            time: 320.0,
            feedback: 0.38,
            mix: 0.22,
            tone: 0.4,
            width: 0.5,
            style: DelayStyle::Tape,
            sync: None,
        }
    }
}

impl Echo {
    /// The family the style draws as.
    #[must_use]
    pub const fn family(self) -> DelayFamily {
        self.style.family()
    }

    /// Pick a family from the selector strip: its first style, or the
    /// next one along if the current style is already in it — so a
    /// family with several machines is walked by clicking its chip
    /// again.
    pub fn choose_family(&mut self, index: usize) {
        let Some(family) = DelayFamily::ALL.get(index).copied() else {
            return;
        };
        self.style = next_in(family.styles(), self.style);
    }

    /// The next style, wrapping — what a click on the glyph does.
    pub const fn cycle_style(&mut self) {
        let next = self.style.to_index().saturating_add(1);
        self.style = if next >= DelayStyle::COUNT {
            DelayStyle::Tape
        } else {
            DelayStyle::from_index(next)
        };
    }
}

/// A reverb, as its display needs it.
///
/// An impulse, the gap before its tail starts, and the tail — which is
/// `reverb-ui`'s own third centrepiece, "the recorded thing itself: an
/// impulse and its decay envelope". The one picture of a reverb that
/// survives being drawn a hundred and thirty pixels wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Room {
    /// Seconds to −60 dB.
    pub decay: f32,
    /// Milliseconds before the tail begins.
    pub predelay: f32,
    pub mix: f32,
    /// Which space. The tail the strip draws is this algorithm's own
    /// impulse response, rendered — a plate and a spring with the same
    /// decay are different pictures because they are different sounds.
    pub algorithm: AlgorithmType,
    /// The space's size, 0..1, as the algorithm takes it.
    pub size: f32,
    /// High-frequency damping, 0..1.
    pub damping: f32,
    /// How dense the reflections are, 0..1.
    pub diffusion: f32,
    /// Locked to the beat: how long its tail is, a length in beats
    /// (`crate::beats::REVERB`), or `None` running free at `decay`.
    pub sync: Option<usize>,
}

impl Default for Room {
    fn default() -> Self {
        Self {
            decay: 1.8,
            predelay: 24.0,
            mix: 0.18,
            algorithm: AlgorithmType::Room,
            size: 0.5,
            damping: 0.3,
            diffusion: 0.7,
            sync: None,
        }
    }
}

impl Room {
    /// The family the algorithm draws as.
    #[must_use]
    pub const fn family(self) -> RoomFamily {
        self.algorithm.family()
    }

    /// What the tail is rendered from.
    #[must_use]
    pub fn key(self) -> crate::live::RoomKey {
        crate::live::RoomKey::of(self.algorithm, self.decay, self.size, self.damping)
    }

    /// Pick a family from the selector strip — see [`Echo::choose_family`].
    pub fn choose_family(&mut self, index: usize) {
        let Some(family) = RoomFamily::ALL.get(index).copied() else {
            return;
        };
        self.algorithm = next_in(family.algorithms(), self.algorithm);
    }

    /// The next algorithm, wrapping — what a click on the glyph does.
    pub fn cycle_algorithm(&mut self) {
        let next = self.algorithm.index().saturating_add(1);
        self.algorithm = if next >= AlgorithmType::ALL.len() {
            AlgorithmType::Room
        } else {
            AlgorithmType::from_index(next)
        };
    }
}

/// The member of a family a click on its chip lands on: the one after
/// `current` if `current` is in the family, else the first. `current`
/// itself if the family is empty.
fn next_in<T: Copy + PartialEq>(members: impl Iterator<Item = T>, current: T) -> T {
    let members: Vec<T> = members.collect();
    let Some(first) = members.first().copied() else {
        return current;
    };
    members
        .iter()
        .position(|m| *m == current)
        .and_then(|at| members.get(at.saturating_add(1)).copied())
        .unwrap_or(first)
}

/// A compressor, as its display needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Comp {
    /// Where it starts working, in dBFS. The red line across the
    /// visualiser — it is a LEVEL, and a level belongs on the axis the
    /// levels are drawn against rather than on a knob you have to read
    /// a number off.
    pub threshold: f32,
    pub ratio: f32,
    pub knee: f32,
    /// How fast it gets there, in milliseconds.
    pub attack: f32,
    /// And how fast it lets go.
    pub release: f32,
}

impl Default for Comp {
    fn default() -> Self {
        Self {
            threshold: -18.0,
            ratio: 3.0,
            knee: 6.0,
            attack: 10.0,
            release: 120.0,
        }
    }
}

/// The range each of the compressor's knobs travels.
///
/// Stated once because three places need them and they have to agree:
/// the knob's own fraction, the drag that moves it, and the reset that
/// puts it back. Attack and release are logarithmic because a
/// millisecond matters at the fast end and twenty do not at the slow.
impl Comp {
    /// Ratio as a knob fraction, and back.
    #[must_use]
    pub fn ratio_norm(self) -> f64 {
        ((f64::from(self.ratio) - 1.0) / 19.0).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn attack_norm(self) -> f64 {
        log_norm(f64::from(self.attack), 0.1, 200.0)
    }

    #[must_use]
    pub fn release_norm(self) -> f64 {
        log_norm(f64::from(self.release), 5.0, 3_000.0)
    }
}

/// A compressor knob's value as a fraction of its own range.
fn knob_norm(comp: Comp, grip: Grip) -> f64 {
    match grip {
        Grip::Ratio(_) => comp.ratio_norm(),
        Grip::Attack(_) => comp.attack_norm(),
        Grip::Release(_) => comp.release_norm(),
        _ => 0.0,
    }
}

/// And back, clamped to it.
fn set_knob(comp: &mut Comp, grip: Grip, to: f64) {
    let to = to.clamp(0.0, 1.0);
    match grip {
        Grip::Ratio(_) => comp.ratio = f64_to_f32(to.mul_add(19.0, 1.0)),
        Grip::Attack(_) => comp.attack = f64_to_f32(log_denorm(to, 0.1, 200.0)),
        Grip::Release(_) => comp.release = f64_to_f32(log_denorm(to, 5.0, 3_000.0)),
        _ => {}
    }
}

/// A time, short enough for a header.
///
/// Sub-millisecond attacks are real and a "0ms" would be a lie, so the
/// fast end keeps a decimal and everything above ten drops it — which
/// is also where a millisecond stops being a distinction anyone hears.
fn millis(ms: f32) -> String {
    if ms < 10.0 {
        format!("{ms:.1}ms")
    } else {
        format!("{ms:.0}ms")
    }
}

/// A value's position on a logarithmic range, 0..1.
fn log_norm(value: f64, low: f64, high: f64) -> f64 {
    if low <= 0.0 || high <= low {
        return 0.0;
    }
    ((value.max(low).log10() - low.log10()) / (high.log10() - low.log10())).clamp(0.0, 1.0)
}

/// And back.
fn log_denorm(t: f64, low: f64, high: f64) -> f64 {
    if low <= 0.0 || high <= low {
        return low;
    }
    let span = high.log10() - low.log10();
    10.0_f64.powf(t.clamp(0.0, 1.0).mul_add(span, low.log10()))
}

/// What fits in a rack of a given width.
///
/// The same shed-in-order rule as `Squeeze` and `Collapse`: a panel that
/// cannot be read is worse than the space it took, because it still
/// looks like information.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Rack {
    /// The plugin's whole editing surface: its grid, its labels, its
    /// own band nodes. What a FOCUSED strip gets — one track opened
    /// wide enough that the rack stops being a readout you glance at
    /// and becomes the thing you work in.
    ///
    /// The difference is not more of the same drawing. At a glance
    /// width the rack shows what a track's processing IS; at a focus
    /// width it shows you where to put your hands, which needs the
    /// plugin's own calibration rather than the strip's abbreviation
    /// of it.
    Focus,
    /// All three panels, each with its grid and its label.
    Full,
    /// The curves alone, stacked. No grid, no labels — at this width a
    /// gridline is a third of the panel and the label is most of it.
    Curves,
    /// A rail's rack: one thin indicator per unit, live. A gain
    /// reduction bar for a compressor, a light for a gate, heat for the
    /// saturator, a sparkline for an EQ — nothing you can edit, and
    /// nothing you can miss. A rail that showed nothing said the track
    /// carried nothing, which is the one thing a rail must not say.
    Minimal,
    /// Nothing. The strip is a fader and a name, and the rack's height
    /// goes back to the strip.
    Off,
}

impl Rack {
    /// What fits in `width`.
    ///
    /// `Full` needs room for a decade of frequency to still read as a
    /// decade — below about 150 the EQ's three gridlines land on top of
    /// each other. `Curves` needs only enough width for the curve's
    /// shape to survive, which is far less.
    #[must_use]
    pub fn at(width: f64) -> Self {
        if width >= FOCUSED {
            Self::Focus
        } else if width >= LEGIBLE {
            Self::Full
        } else if width >= SHAPE {
            Self::Curves
        } else if width >= MINIMAL {
            Self::Minimal
        } else {
            Self::Off
        }
    }

    /// Whether the rack draws at all.
    #[must_use]
    pub fn on(self) -> bool {
        self != Self::Off
    }

    /// The narrowest width still in this tier.
    ///
    /// What a strip may be shrunk to without changing what it shows —
    /// see `mcp::lending_floor`, which is the only caller and the
    /// reason this exists. Returning the tier's own lower bound rather
    /// than a fixed floor is what keeps a strip from being lent out of
    /// the rack it was drawing.
    #[must_use]
    pub const fn floor(self) -> Option<f64> {
        match self {
            Self::Focus => Some(FOCUSED),
            Self::Full => Some(LEGIBLE),
            Self::Curves => Some(SHAPE),
            Self::Minimal => Some(MINIMAL),
            Self::Off => None,
        }
    }

    /// Whether this tier hands the panel over to the plugin's own
    /// drawing — its grid, its labels, its nodes.
    #[must_use]
    pub const fn editing(self) -> bool {
        matches!(self, Self::Focus)
    }

    /// Whether the panel is detailed: headers, markers, grips.
    ///
    /// Both of the two widest tiers. Written as a predicate rather than
    /// a comparison because the enum is ordered most-capable-first, so
    /// `>= Full` reads as "at least Full" and means the opposite.
    #[must_use]
    pub const fn detailed(self) -> bool {
        matches!(self, Self::Focus | Self::Full)
    }
}

/// The narrowest rack whose EQ panel still reads as a frequency axis.
///
/// Three gridlines — 100, 1k, 10k — and below this they crowd into each
/// other, at which point the panel is a squiggle rather than a decision
/// you can check.
pub const LEGIBLE: f64 = 96.0;

/// The narrowest rack that still draws a curve.
///
/// Below this a curve is a few pixels of wiggle — it reads as ornament
/// rather than as a setting, and ornament in a mixer is worse than
/// space. Below it the rack is indicators — see [`Rack::Minimal`].
///
/// Eighty, so that REAPER's own eighty-six-pixel strip — a bus, a
/// bass, a guitar at its normal width — draws its curves: its rack is
/// eighty-two wide once the insets come off, and at ninety it fell to
/// the rail's indicators, which are for tracks a third that wide.
pub const SHAPE: f64 = 80.0;

/// The narrowest rack that still draws indicators: a rail's own width
/// less its edges.
pub const MINIMAL: f64 = 16.0;

/// The narrowest rack that gives the panel over to the plugin's own
/// editing surface.
///
/// Below this the plugin's grid crowds, its labels collide and its
/// nodes — authored for a graph eight hundred pixels wide — overlap
/// each other. Above it there is room for all three, and the rack stops
/// abbreviating.
///
/// Well under a focus width (618 on a 1440p panel), because a focused
/// strip is not the only way to get here: two of them side by side on
/// an ultrawide, or a mixer of six tracks, land in the same place and
/// should get the same panel.
pub const FOCUSED: f64 = 260.0;

/// The width a strip opens to when you go to WORK on it.
///
/// Above [`LEGIBLE`] rather than at it: the threshold is where the rack
/// stops being illegible, and a control you have deliberately opened
/// should not land on the edge of that.
///
/// One number for two jobs, and they have to be the same number: it is
/// what a selected strip expands to, AND what the template stores for a
/// track that carries its piece's processing. If the stored width were
/// larger, selecting a piece would SHRINK it; if smaller, selecting one
/// would widen it and shove every strip to its right. Either way the
/// mixer moves under you at the moment you click a track, which is
/// exactly when it must not.
///
/// It does not have to be paid for twice. An opened strip BORROWS its
/// extra width from the others rather than adding to the total — see
/// `mcp::widths` — so this is bounded by what the resting layout can
/// afford, not by what the worst selection would cost on top of it.
pub const WORKING: f64 = 133.0;

/// Where the rack goes.
#[derive(Clone, Copy, Debug)]
pub struct Panel {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Panel {
    /// From a rect in strip coordinates, moved to where the strip is.
    ///
    /// The rack's box comes from [`crate::strip::Strip::rack_rect`], in
    /// the strip's own space; the recording is in the mixer's. This is
    /// the one conversion between them, so a grip and a curve cannot
    /// disagree about where the rack starts.
    #[must_use]
    pub fn of(rect: Rect, left: f64) -> Self {
        Self {
            x: rect.x0 + left,
            y: rect.y0,
            width: rect.width(),
            height: rect.height(),
        }
    }

    /// The same box, moved up by a scroll.
    ///
    /// The rack scrolls by moving where its panels START, not by
    /// offsetting each one — see [`layout`]. One move, applied once,
    /// which is what keeps the drawing and the hit test on the same
    /// pixel.
    #[must_use]
    pub fn up(self, by: f64) -> Self {
        Self {
            y: self.y - by,
            ..self
        }
    }

    const fn rect(self) -> Rect {
        Rect::new(self.x, self.y, self.x + self.width, self.y + self.height)
    }

    /// Whether a point is inside.
    #[must_use]
    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// The panel inset by `by` on every side.
    const fn inset(self, by: f64) -> Self {
        Self {
            x: self.x + by,
            y: self.y + by,
            width: self.width - by * 2.0,
            height: self.height - by * 2.0,
        }
    }

    /// `by` off the top, and what is left.
    ///
    /// The header is a strip of the panel rather than an overlay on it:
    /// a value printed over a curve is unreadable exactly when the
    /// curve is interesting, which is the moment you want the number.
    const fn split_top(self, by: f64) -> (Self, Self) {
        let head = Self { height: by, ..self };
        let body = Self {
            y: self.y + by,
            height: self.height - by,
            ..self
        };
        (head, body)
    }
}

/// How tall a panel's header is.
///
/// The type is 7pt and this is the line it sits on plus a pixel of air
/// under it. Small, because it is a readout and not a title — you look
/// at the curve and read the number to confirm what you saw.
const HEAD: f64 = 10.0;

/// The gap between two panels of the rack.
const GAP: f64 = 3.0;

/// How tall a rack of these panels comes to, plus its gaps.
///
/// What the panels ACTUALLY occupy, which is less than the rack they
/// are laid out in — the space below them is held for the processors
/// the other phases bring. A caller that wants to know where the rack's
/// contents end asks this.
#[must_use]
pub fn wanted(panels: &[Which]) -> f64 {
    if panels.is_empty() {
        return 0.0;
    }
    let gaps = GAP * crate::num::coord(panels.len().saturating_sub(1));
    panels.iter().map(|which| which.natural()).sum::<f64>() + gaps
}

/// Which phases are folded shut.
///
/// A phase is the container the chain is read in — you are working the
/// Tone pass, so the four panels above and below it are context you
/// want out of the way — and folding one is how the rack stays legible
/// as it grows past what a strip can hold.
///
/// A bitset over the canonical phase order, for the same reason
/// [`Bypass`] is one: the chain gains phases, and a field each would
/// mean an edit in three places every time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Folded {
    shut: u16,
    /// One panel, filling the box: a close-up (`crate::closeup`), with
    /// no containers and no chain around it.
    zoomed: bool,
}

impl Folded {
    fn bit(phase: session::mix_phases::MixPhase) -> u16 {
        let at = session::mix_phases::MixPhase::ALL
            .iter()
            .position(|p| *p == phase)
            .unwrap_or(0);
        1_u16 << u16::try_from(at).unwrap_or(0)
    }

    #[must_use]
    pub fn is(self, phase: session::mix_phases::MixPhase) -> bool {
        self.shut & Self::bit(phase) != 0
    }

    pub fn toggle(&mut self, phase: session::mix_phases::MixPhase) {
        self.shut ^= Self::bit(phase);
    }

    #[must_use]
    pub const fn any(self) -> bool {
        self.shut != 0
    }

    /// A close-up: the first panel alone, filling the whole box, drawn
    /// and hit and dragged exactly as it is in a strip — only bigger.
    #[must_use]
    pub fn zoomed() -> Self {
        Self {
            zoomed: true,
            ..Self::default()
        }
    }

    /// Whether this is a close-up ([`Folded::zoomed`]).
    #[must_use]
    pub const fn is_zoomed(self) -> bool {
        self.zoomed
    }

    /// Where a mixer opens: Rescue shut, the rest open.
    ///
    /// Rescue is dialled in once, at the start of a mix, and then it
    /// is done — and a pass that is done should not spend the top of
    /// every rack for the rest of the session. Unfold it when you need
    /// it; it stays where you left it.
    #[must_use]
    pub fn rest() -> Self {
        let mut folded = Self::default();
        folded.toggle(session::mix_phases::MixPhase::Rescue);
        folded
    }
}

/// Where the folds live: one answer for the mixer, or one per track.
///
/// Both, because they answer different questions. Synced is the
/// default — the chain is read ACROSS, and a mixer where each strip
/// folded on its own would put a different processor at the same height
/// on every track, which is the one thing a mixer must not do. Per
/// track is for when you are working one track rather than comparing.
#[derive(Clone, Debug, Default)]
pub struct Fold {
    /// The shared answer, used when `synced`.
    every: Folded,
    by_guid: std::collections::HashMap<String, Folded>,
    pub synced: bool,
}

impl Fold {
    /// Where a mixer opens: synced, at [`Folded::rest`].
    #[must_use]
    pub fn rest() -> Self {
        Self {
            every: Folded::rest(),
            ..Self::shared()
        }
    }

    /// Synced, which is the default a mixer wants.
    #[must_use]
    pub fn shared() -> Self {
        Self {
            synced: true,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn of(&self, guid: &str) -> Folded {
        if self.synced {
            self.every
        } else {
            // A track with no answer of its own follows the shared one,
            // which is where the mixer opened — Rescue shut.
            self.by_guid.get(guid).copied().unwrap_or(self.every)
        }
    }

    /// Fold or unfold a phase on a track — or on all of them.
    /// Every rack open at `phase`'s section alone, the others shut — what
    /// moving to a phase shows. A phase with no section of its own
    /// (Balance, Creative, Overview) shuts them all: the faders are the
    /// question.
    pub fn focus(&mut self, phase: session::mix_phases::MixPhase) {
        let mut every = Folded::default();
        for other in session::mix_phases::MixPhase::ALL {
            if other != phase {
                every.toggle(other);
            }
        }
        self.every = every;
        self.by_guid.clear();
    }

    pub fn toggle(&mut self, guid: &str, phase: session::mix_phases::MixPhase) {
        if self.synced {
            self.every.toggle(phase);
        } else {
            self.by_guid
                .entry(guid.to_owned())
                .or_default()
                .toggle(phase);
        }
    }

    /// Switch between the two, carrying the state across.
    ///
    /// Going synced takes the track you were on as the answer for
    /// everyone, rather than resetting: the fold you just made is
    /// almost always the one you want everywhere.
    pub fn sync(&mut self, synced: bool, from: &str) {
        if synced && !self.synced {
            self.every = self.by_guid.get(from).copied().unwrap_or_default();
        }
        self.synced = synced;
    }
}

/// A row of the rack: a phase's header, or one processor under it.
///
/// The two are laid out TOGETHER because they are one column, and a
/// header whose position was worked out separately from the panels it
/// caps would drift from them the first time a phase folded. Drawing
/// and hit testing both walk this.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    /// The container's bar: its name and the chevron that folds it.
    Head(session::mix_phases::MixPhase),
    /// A phase this chain has no units for, kept as a bar so the
    /// phases after it land where they land on every other strip.
    Blank(session::mix_phases::MixPhase),
    Unit(Which),
}

/// The phases a rack is divided into, in signal order.
///
/// Every rack that starts where a channel's does has all five: a
/// phase with no units is a [`Row::Blank`] bar the height of a header,
/// which is what keeps a bus's Tone level with a channel's — a rack
/// that only had the phases it used put every unit at a different
/// height on every strip. A chain that is only the late phases (a
/// return) has nothing to line up with and gets only what it has.
pub const RACK_PHASES: [session::mix_phases::MixPhase; 5] = [
    session::mix_phases::MixPhase::Rescue,
    session::mix_phases::MixPhase::Tone,
    session::mix_phases::MixPhase::Polish,
    session::mix_phases::MixPhase::Relational,
    session::mix_phases::MixPhase::Depth,
];

/// How tall a phase header is.
pub const HEAD_H: f64 = 15.0;

/// Every processor the rack can show, in signal order.
///
/// ALL of them, whatever the phase. The rack used to show a phase's own
/// subset — the EQ alone in Rescue, the compressor and saturator in
/// Polish — which meant changing phase changed which processors
/// existed, and a strip you were reading reorganised itself under you.
///
/// A chain is a chain. You scroll it, and a phase's job is to say which
/// part of it to LOOK at rather than which parts to have. The rail's
/// buttons become focus and collapse once there is a setting for it;
/// until then the whole chain is on screen and the rack scrolls.
pub const ALL_PANELS: [Which; 11] = [
    Which::RescueEq,
    Which::Gate,
    Which::RescueComp,
    Which::Eq,
    Which::Comp,
    Which::Sat,
    Which::DeEss,
    Which::PolishEq,
    Which::Space,
    Which::Delay,
    Which::Reverb,
];

/// Which panels a mix phase asks for.
///
/// Kept as a function because the callers read like the rack is a
/// question about the phase, and one day it will be again — as focus,
/// not as membership.
#[must_use]
pub const fn panels_for(_phase: session::mix_phases::MixPhase) -> &'static [Which] {
    &ALL_PANELS
}

/// Record the rack into `scene`.
///
/// Returns nothing: like every other control in the mixer this is
/// recorded once into the strip's command range and replayed from there.
pub fn record(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    tone: &Tone,
    panels: &[Which],
    panel: Panel,
    folded: Folded,
    under: Option<Color>,
) {
    draw(
        scene,
        palette,
        font,
        tone,
        &Meters::default(),
        panels,
        panel,
        folded,
        None,
        under,
    );
}

/// The same, with one grip lit.
///
/// Only the live pass has a pointer to report, so the recording calls
/// [`record`] and this is what the overlay reaches for. A lit grip is
/// the difference between "there is a handle here" and "this is the
/// handle you will move", which on a curve with four of them is the
/// whole question.
pub fn draw(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    tone: &Tone,
    // `meters` is everything in the rack that moves with the audio —
    // the analyser's bins, the suppressors' reduction, the wet returns.
    // Empty when nothing is playing, which is a rack that can stay in
    // the recording.
    meters: &Meters,
    panels: &[Which],
    panel: Panel,
    folded: Folded,
    lit: Option<Grip>,
    // What to paint under the chain's end, when the rack's box is
    // taller than the chain — the track's colour, if the mixer is set
    // to (see `layout::rack_fill`); nothing, and the panel's
    // ground shows, otherwise. Here rather than in either caller,
    // because the recording and the live pass both draw the rack and
    // whichever one is on top has to paint it.
    under: Option<Color>,
) {
    let rack = Rack::at(panel.width);
    if !rack.on() || panel.height < 24.0 || panels.is_empty() {
        return;
    }
    if let Some(under) = under {
        // From the chain's end down past the box: the caller clips to
        // the box, and the panel may be scrolled up by an amount only
        // the caller knows.
        let top = panel.y + tall(panels, folded);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            under,
            None,
            &Rect::new(
                panel.x,
                top,
                panel.x + panel.width,
                panel.y + panel.height * 2.0,
            ),
        );
    }
    // A rail: the same chain, the same rows at the same heights as the
    // strip beside it — so every compressor sits on one line across
    // the mixer whatever each strip's width — with one indicator drawn
    // in each row instead of a panel. A container is a tick of its
    // colour; a preset row is nothing.
    if rack == Rack::Minimal {
        for (row, at) in chain(panels, panel, folded) {
            let Row::Unit(which) = row else {
                // A blank phase is the same tick, faint: the phase is
                // still there in the order, it just has nothing in it.
                let (phase, tint) = match row {
                    Row::Head(phase) => (phase, phase_tint(phase)),
                    Row::Blank(phase) => (phase, phase_tint(phase).multiply_alpha(BLANK_ALPHA)),
                    Row::Unit(_) => continue,
                };
                let _ = phase;
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    tint,
                    None,
                    &Rect::new(at.x, at.y + 3.0, at.x + 2.0, at.y + at.height - 3.0),
                );
                continue;
            };
            rail_unit(
                scene,
                &Draw {
                    palette,
                    font,
                    tone,
                    meters,
                    rack,
                    folded,
                    lit,
                },
                which,
                at,
            );
        }
        return;
    }

    for (row, at) in chain(panels, panel, folded) {
        let Row::Unit(which) = row else {
            match row {
                Row::Head(phase) => {
                    if !phase_themed(scene, font, phase, at, Some((folded.is(phase), lit))) {
                        container(scene, palette, font, phase, at, folded.is(phase), lit);
                    }
                }
                Row::Blank(phase) => {
                    if !phase_themed(scene, font, phase, at, None) {
                        blank(scene, palette, font, phase, at);
                    }
                }
                Row::Unit(_) => {}
            }
            continue;
        };
        if which == Which::Presets {
            presets(scene, palette, font, tone, at, lit);
            continue;
        }
        let pen = Draw {
            palette,
            font,
            tone,
            meters,
            rack,
            folded,
            lit,
        };
        unit(scene, &pen, which, at);
    }
}

/// What every panel of a rack is drawn from: the palette and face, the
/// track's processing and what its meters say, the tier, which phases
/// are folded, and the grip the pointer has lit.
struct Draw<'a> {
    palette: &'a Palette,
    font: &'a Font,
    tone: &'a Tone,
    meters: &'a Meters,
    rack: Rack,
    folded: Folded,
    lit: Option<Grip>,
}

/// The role a theme draws a processor's panel as: its well, the `%body`
/// where the processor's own display goes, the bypass scrim over that,
/// and the header (the `%glyph`, the name, the value, the way into a
/// close-up) over the scrim.
pub const UNIT: &str = "rack.unit";

/// The role a theme draws a processor's row as on a rail — a strip too
/// narrow for its panel: a faint well, the name where it fits, the
/// `%body` where the host draws its indicator, the bypass scrim over all
/// of it.
pub const RAIL_ROW: &str = "rack.rail";

/// One processor's row on a rail at `at`: the theme's, or the painters'.
fn rail_unit(scene: &mut Scene, pen: &Draw<'_>, which: Which, at: Panel) {
    let Draw {
        palette,
        font,
        tone,
        meters,
        rack,
        ..
    } = *pen;
    let theme = crate::skin::loaded();
    let Some(theme) = theme.as_deref().filter(|t| t.has_role(RAIL_ROW)) else {
        minimal(
            scene,
            palette,
            font,
            tone,
            meters,
            which,
            at.inset(1.0),
            rack,
        );
        if tone.bypass.is(which) {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                palette.tcp_meter_well.multiply_alpha(0.78),
                None,
                &at.rect(),
            );
        }
        return;
    };
    // The name where it fits the header — the painters' rule.
    let name = which.name();
    let fits = font.width(name, 7.0) <= at.width - 6.0;
    let fields = crate::skin::values(&[
        (
            "unit.name",
            theme_runtime::Value::text(if fits { name } else { "" }),
        ),
        (
            "unit.bypassed",
            theme_runtime::Value::Bool(tone.bypass.is(which)),
        ),
    ]);
    theme.record_with(
        RAIL_ROW,
        (at.width, at.height),
        &fields,
        &mut crate::skin::canvas::Host { scene, font },
        Affine::translate((at.x, at.y)),
        &mut |host, slot, rect| {
            if slot == "body" {
                let rect = rect + vello::kurbo::Vec2::new(at.x, at.y);
                minimal_body(
                    host.scene,
                    palette,
                    font,
                    tone,
                    meters,
                    which,
                    Panel {
                        x: rect.x0,
                        y: rect.y0,
                        width: rect.width(),
                        height: rect.height(),
                    },
                );
            }
        },
    );
}

/// One processor's panel at `at`: the theme's, or the painters'.
fn unit(scene: &mut Scene, pen: &Draw<'_>, which: Which, at: Panel) {
    // What the header says is fitted here, because only the host measures
    // text: the name where it fits after the glyph, the value where both
    // do — the painters' header's rule.
    const SIZE: f32 = 7.0;
    let Draw {
        palette,
        font,
        tone,
        rack,
        folded,
        lit,
        ..
    } = *pen;
    let theme = crate::skin::loaded();
    let Some(theme) = theme.as_deref().filter(|t| t.has_role(UNIT)) else {
        unit_painted(scene, pen, which, at);
        return;
    };
    let bypassed = tone.bypass.is(which);
    let mark = which.glyph(tone);
    let expands = rack.detailed() && !folded.is_zoomed();
    let room = at.width - 4.0;
    let left = if mark.is_some() { GLYPH_W } else { 0.0 };
    let name = which.name();
    let name_w = left + font.width(name, SIZE);
    let summary = which.summary(tone, rack);
    let value = if bypassed { "BYPASS" } else { summary.as_str() };
    let name_fits = name_w <= room;
    let value_fits = name_fits
        && name_w + font.width(value, SIZE) + 6.0 <= room - if expands { EXPAND_W } else { 0.0 };
    let fields = crate::skin::values(&[
        (
            "unit.name",
            theme_runtime::Value::text(if name_fits { name } else { "" }),
        ),
        (
            "unit.value",
            theme_runtime::Value::text(if value_fits { value } else { "" }),
        ),
        ("unit.bypassed", theme_runtime::Value::Bool(bypassed)),
        ("unit.glyph", theme_runtime::Value::Bool(mark.is_some())),
        ("unit.expands", theme_runtime::Value::Bool(expands)),
        (
            "unit.zoom_lit",
            theme_runtime::Value::Bool(lit == Some(Grip::Zoom(which))),
        ),
    ]);
    let (name_ink, _) = header_inks(palette, bypassed);
    theme.record_with(
        UNIT,
        (at.width, at.height),
        &fields,
        &mut crate::skin::canvas::Host { scene, font },
        Affine::translate((at.x, at.y)),
        &mut |host, slot, rect| {
            let rect = rect + vello::kurbo::Vec2::new(at.x, at.y);
            match slot {
                "body" => unit_body(
                    host.scene,
                    pen,
                    which,
                    Panel {
                        x: rect.x0,
                        y: rect.y0,
                        width: rect.width(),
                        height: rect.height(),
                    },
                ),
                "glyph" => {
                    if let Some(mark) = mark {
                        glyph(
                            host.scene,
                            name_ink,
                            mark,
                            rect.x0,
                            rect.y0 + f64::from(SIZE) - 0.5,
                        );
                    }
                }
                _ => {}
            }
        },
    );
}

/// The painters' panel: what a theme without one gets.
fn unit_painted(scene: &mut Scene, pen: &Draw<'_>, which: Which, at: Panel) {
    let Draw {
        palette,
        font,
        tone,
        rack,
        folded,
        lit,
        ..
    } = *pen;
    ground(scene, palette, at);
    let inner = at.inset(2.0);
    // The header is taken at EVERY tier the row is tall enough for,
    // and says only the name where the value will not fit. It was
    // skipped at `Curves`, which moved the display up by its height
    // on a strip a few pixels narrower than its neighbour — and
    // the compressors across the mixer stopped sitting on one line.
    let body = body_of(at, rack);
    let head = (body.y > inner.y).then(|| inner.split_top(HEAD).0);
    if body.width > 0.0 && body.height > 0.0 {
        unit_body(scene, pen, which, body);
        // The bypass, over everything the panel just drew.
        //
        // A scrim rather than a badge, and over the WHOLE panel
        // rather than beside it: what you need to know at a glance
        // across a mixer is that this processing is not happening,
        // and a small mark in a corner is exactly the thing a
        // glance misses. Greying the curve out says it once, in the
        // place you are already looking.
        if tone.bypass.is(which) {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                palette.tcp_meter_well.multiply_alpha(0.78),
                None,
                &at.rect(),
            );
        }
        if let Some(head) = head {
            // The way into a close-up, at the header's right end;
            // the value moves over for it.
            let expands = rack.detailed() && !folded.is_zoomed();
            let text = if expands {
                Panel {
                    width: (head.width - EXPAND_W).max(0.0),
                    ..head
                }
            } else {
                head
            };
            header(
                scene,
                palette,
                font,
                which.name(),
                which.glyph(tone),
                &which.summary(tone, rack),
                tone.bypass.is(which),
                text,
            );
            if expands {
                expand_mark(
                    scene,
                    if lit == Some(Grip::Zoom(which)) {
                        palette.text
                    } else {
                        palette.text_faint
                    },
                    head,
                );
            }
        }
    }
}

/// A processor's own display in `body`: its curve, its knobs, its lane
/// of choices — what the panel is FOR, and what a theme leaves to the
/// host (its `%body` slot).
fn unit_body(scene: &mut Scene, pen: &Draw<'_>, which: Which, body: Panel) {
    let Draw {
        palette,
        font,
        tone,
        meters,
        rack,
        lit,
        ..
    } = *pen;
    match which {
        // The three EQs are one drawing over three band sets.
        // What differs is what the bands are FOR, which is the
        // panel's name and not its picture.
        Which::RescueEq
        | Which::Eq
        | Which::Space
        | Which::PreEq
        | Which::PostEq
        | Which::PolishEq => {
            eq(
                scene,
                palette,
                font,
                tone,
                which,
                tone.bands_ref(which),
                &meters.spectrum,
                body,
                rack,
                lit,
                None,
            );
        }
        // Blue, because the vertical axis is not gain: it is
        // how long each band rings, and a graph that looked
        // like the EQ above it would be read as one.
        Which::DecayEq => {
            eq(
                scene,
                palette,
                font,
                tone,
                which,
                &tone.decay_eq,
                &[],
                body,
                rack,
                lit,
                Some(DECAY_INK),
            );
        }
        Which::Knobs => knobs(scene, palette, font, tone, body, rack, lit),
        Which::Wide => wide(scene, palette, tone, body, rack, lit),
        Which::Pitch => pitch(scene, palette, font, tone, body, rack, lit),
        // The modulation editor's own picture, at a strip's width.
        Which::Mod => crate::faces::paint_mod_in(scene, tone, body),
        Which::Presets => {}
        Which::Gate => gate(scene, palette, tone.gate, body, rack, lit),
        Which::RescueComp => {
            comp(scene, palette, font, tone.rescue_comp, body, rack, lit);
        }
        Which::Comp => comp(scene, palette, font, tone.comp, body, rack, lit),
        Which::Sat => {
            sat(
                scene,
                palette,
                tone,
                meters,
                display_of(body, which, rack),
                rack,
                lit,
            );
        }
        Which::DeEss | Which::DeEssIn => {
            suppress(scene, palette, which, tone.de_ess, meters, body, rack, lit);
        }
        Which::Delay => {
            echo(
                scene,
                palette,
                tone.delay,
                meters,
                display_of(body, which, rack),
                rack,
                lit,
            );
        }
        Which::Reverb => {
            room(
                scene,
                palette,
                tone.reverb,
                meters,
                display_of(body, which, rack),
                rack,
                lit,
            );
        }
    }
    if let Some(strip) = lane_of(body, which, rack)
        && matches!(which, Which::Sat | Which::Delay | Which::Reverb)
    {
        selector(scene, palette, font, tone, which, strip, lit);
    }
}

/// Where each panel of the rack lands.
///
/// The one place the rack's vertical division is worked out. Drawing
/// reads it and so does the hit test, which is the same rule
/// [`crate::strip::Strip`] exists for: a grip that is not where its
/// curve is drawn is a grip that moves the wrong thing.
///
/// Every panel keeps its NATURAL height and they stack from `panel.y`,
/// which means the chain is as long as the chain is and runs off the
/// bottom of a box too short for it. It used to scale them down
/// together to fit; that made every processor added shrink every
/// processor already there, until the whole rack was unreadable. A
/// chain is scrolled instead.
///
/// To scroll, hand it a panel whose `y` is already moved up — the
/// scroll is a fact about where the rack is drawn, not a second
/// parameter three callers would have to pass identically. `panel`'s
/// own height is what a caller clips to and what [`scroll_span`]
/// measures against; the layout itself does not read it.
#[must_use]
pub fn layout(panels: &[Which], panel: Panel) -> Vec<(Which, Panel)> {
    units(panels, panel, Folded::default())
}

/// The same, with some phases folded shut.
#[must_use]
pub fn units(panels: &[Which], panel: Panel, folded: Folded) -> Vec<(Which, Panel)> {
    chain(panels, panel, folded)
        .into_iter()
        .filter_map(|(row, at)| match row {
            Row::Unit(which) => Some((which, at)),
            Row::Head(_) | Row::Blank(_) => None,
        })
        .collect()
}

/// The whole column: every phase header and every unit under it.
///
/// One walk, so a header and the panels it caps cannot disagree about
/// where they are — which is the same rule [`crate::strip::Strip`] is
/// built on, applied down instead of across.
#[must_use]
pub fn chain(panels: &[Which], panel: Panel, folded: Folded) -> Vec<(Row, Panel)> {
    let mut out = Vec::with_capacity(panels.len() + RACK_PHASES.len());
    // No chain, no rack: a track that carries nothing gets no bars
    // either, blank or otherwise.
    if panels.is_empty() {
        return out;
    }
    // A close-up: the one panel, the whole box.
    if folded.is_zoomed() {
        out.push((Row::Unit(panels[0]), panel));
        return out;
    }
    let mut y = panel.y;
    let row = |y: f64, height: f64| Panel {
        x: panel.x,
        y,
        width: panel.width,
        height,
    };
    let tier = Rack::at(panel.width);
    // The preset row is in no phase: it caps the chain, and folds with
    // nothing.
    for which in panels
        .iter()
        .copied()
        .filter(|which| *which == Which::Presets)
    {
        out.push((Row::Unit(which), row(y, which.natural())));
        y += which.natural() + GAP;
    }
    // Every phase in order. A phase with units gets a header and the
    // units under it — the chain's ORDER does the grouping, so a
    // container is a run of them. On a chain that starts where a
    // channel's does, a phase without units gets a blank bar the same
    // height, so what follows it sits where it sits on a strip that
    // has the phase. A chain that is only the late phases — a return,
    // which is nothing but Depth — has no channel to line up with,
    // and four bars of nothing over it would say it was missing
    // something it was never going to have.
    let level = panels.iter().any(|which| {
        matches!(
            which.phase(),
            session::mix_phases::MixPhase::Rescue | session::mix_phases::MixPhase::Tone
        )
    });
    for phase in RACK_PHASES {
        let mut units = panels
            .iter()
            .copied()
            .filter(|which| *which != Which::Presets && which.phase() == phase)
            .peekable();
        if units.peek().is_none() {
            if level {
                out.push((Row::Blank(phase), row(y, HEAD_H)));
                y += HEAD_H;
            }
            continue;
        }
        out.push((Row::Head(phase), row(y, HEAD_H)));
        y += HEAD_H;
        if folded.is(phase) {
            continue;
        }
        for which in units {
            let height = which.natural_at(tier);
            out.push((Row::Unit(which), row(y, height)));
            y += height + GAP;
        }
    }
    out
}

/// How far the rack can scroll before the last panel's floor arrives.
///
/// Zero when the chain already fits its box, which is what keeps the
/// gesture inert until there is something below to reach.
#[must_use]
pub fn scroll_span(panels: &[Which], box_height: f64, folded: Folded) -> f64 {
    (tall(panels, folded) - box_height).max(0.0)
}

/// How tall the whole column comes to, headers and folds included.
#[must_use]
pub fn tall(panels: &[Which], folded: Folded) -> f64 {
    let probe = Panel {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };
    chain(panels, probe, folded)
        .last()
        .map_or(0.0, |(_, at)| at.y + at.height)
}

/// The body of a panel — what is left once its header is taken.
///
/// Shared by the drawing and the hit test for the same reason
/// [`layout`] is: the curve is drawn in the body, so a grip has to be
/// measured against the body.
#[must_use]
pub fn body_of(at: Panel, rack: Rack) -> Panel {
    // The header is taken at every tier — the same rows at the same
    // heights across the mixer, so a display's top edge is one line
    // whatever each strip's width — and only when the row can spare
    // it. `rack` used to decide; now it is the caller's tier for the
    // record, and the height decides.
    let _ = rack;
    let inner = at.inset(2.0);
    if inner.height > HEAD * 2.0 {
        inner.split_top(HEAD).1
    } else {
        inner
    }
}

/// One panel of the rack.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Which {
    // ── Rescue: make it usable ───────────────────────────────────────
    /// Surgical cuts. The EQ you reach for before anything else,
    /// because the rest of the chain has to work on what this leaves.
    RescueEq,
    /// What is between the notes — the gate decides how much of the
    /// room, the bleed and the noise survives into everything after it.
    Gate,
    /// The first compressor: catching what is wrong rather than
    /// shaping what is right.
    RescueComp,

    // ── Tone: make it sound like itself ──────────────────────────────
    Eq,
    Comp,
    Sat,
    /// Movement and colour in time — a chorus, flanger or phaser, a
    /// tremolo, vibrato or rotary: the modulation editor's six engines.
    Mod,

    // ── Polish: take the ugly out ────────────────────────────────────
    /// Sibilance, dynamically.
    DeEss,
    /// And every other resonance — the narrow peaks a room or a body
    /// puts in — as surgical bands on the EQ's own graph.
    PolishEq,

    // ── Relational: make it sit with the others ──────────────────────
    /// The EQ that is not about this track: it carves the room another
    /// one needs, which is why it is its own unit and not more bands
    /// on the tone EQ.
    Space,

    // ── Depth: put it somewhere ──────────────────────────────────────
    /// Delay before reverb, which is the order they go in — a repeat
    /// arriving after the tail has started is a repeat you cannot hear
    /// as a repeat.
    Delay,
    Reverb,

    // ── The advanced units' own parts ────────────────────────────────
    /// The de-esser INSIDE an effect: the same detector as the Polish
    /// unit, on the way into the delay or the reverb, so a return
    /// carries one instance and not two. Lives in Depth, because the
    /// return does.
    DeEssIn,
    /// The EQ into an effect — inside the one instance.
    PreEq,
    /// The EQ out of it.
    PostEq,
    /// The reverb's decay-rate EQ, drawn blue: gain here is TIME.
    DecayEq,
    /// A row of the effect's knobs.
    Knobs,
    /// The widener.
    Wide,
    /// The pitch shifter.
    Pitch,
    /// The track's presets, brighter to darker — a row of chips at the
    /// top of the rack, in no phase.
    Presets,
}

impl Which {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RescueEq => "RESCUE EQ",
            Self::Gate => "GATE",
            Self::RescueComp => "RESCUE COMP",
            Self::Eq => "EQ",
            Self::Comp => "COMP",
            Self::Sat => "SAT",
            Self::Mod => "MOD",
            Self::DeEss | Self::DeEssIn => "DE-ESS",
            Self::PolishEq => "POLISH EQ",
            Self::Space => "SPACE",
            Self::Delay => "DELAY",
            Self::Reverb => "REVERB",
            Self::PreEq => "PRE EQ",
            Self::PostEq => "POST EQ",
            Self::DecayEq => "DECAY EQ",
            Self::Knobs | Self::Presets => "",
            Self::Wide => "WIDE",
            Self::Pitch => "PITCH",
        }
    }

    /// Which phase this unit belongs to.
    ///
    /// The chain is ordered BY phase, and the phase is what the rail's
    /// buttons will focus once there is a setting for it. Stated per
    /// unit rather than as a table of ranges, so adding a processor is
    /// one line in one place.
    #[must_use]
    pub const fn phase(self) -> session::mix_phases::MixPhase {
        use session::mix_phases::MixPhase as P;
        match self {
            Self::RescueEq | Self::Gate | Self::RescueComp => P::Rescue,
            Self::Eq | Self::Comp | Self::Sat | Self::Mod => P::Tone,
            Self::DeEss | Self::PolishEq => P::Polish,
            Self::Space => P::Relational,
            // Every part of an FX return is Depth — the return is where
            // a track is put, front to back — including the de-esser on
            // its way in and the widener and the shifter.
            Self::Delay
            | Self::Reverb
            | Self::DeEssIn
            | Self::PreEq
            | Self::PostEq
            | Self::DecayEq
            | Self::Knobs
            | Self::Wide
            | Self::Pitch => P::Depth,
            // No phase: the row sits above the chain. `chain` never
            // opens a container for it.
            Self::Presets => P::Overview,
        }
    }

    /// Whether this unit draws a frequency response.
    ///
    /// Four of them do, on the plugin's own graph — the two EQs, the
    /// de-esser and the resonance suppressor — and they differ in what
    /// the curve MEANS rather than in how it is drawn.
    #[must_use]
    pub const fn is_spectral(self) -> bool {
        matches!(
            self,
            Self::RescueEq
                | Self::Eq
                | Self::Space
                | Self::DeEss
                | Self::DeEssIn
                | Self::PolishEq
                | Self::PreEq
                | Self::PostEq
                | Self::DecayEq
        )
    }

    /// What this panel's settings come to, in one short line.
    ///
    /// A curve says the SHAPE of a decision and a number says the
    /// decision. Both, because they answer different questions: you
    /// scan the curves across a mixer to find the track that is
    /// different, and you read the number to know what to type into the
    /// one you opened.
    fn summary(self, tone: &Tone, rack: Rack) -> String {
        let curve = |bands: &[EqBand]| {
            let live = bands.iter().filter(|b| b.enabled && b.used).count();
            let range = bands
                .iter()
                .filter(|b| b.enabled && b.used)
                .map(|b| b.gain.abs())
                .fold(0.0_f32, f32::max);
            if live == 0 {
                "flat".to_owned()
            } else {
                format!("{live} · {range:.1}dB")
            }
        };
        let squash = |comp: Comp| {
            let head = format!("{:.0}dB · {:.1}:1", comp.threshold, comp.ratio);
            // The times are the envelope's own shape, which is the
            // point of drawing it — but a shape says "fast" and not
            // "three milliseconds", and at a focus width there is room
            // to say both.
            if rack.editing() {
                format!("{head} · {}/{}", millis(comp.attack), millis(comp.release))
            } else {
                head
            }
        };
        match self {
            Self::RescueEq => curve(&tone.rescue_eq),
            Self::Eq => curve(&tone.eq),
            Self::PolishEq => curve(&tone.polish_eq),
            Self::Space => curve(&tone.space),
            Self::PreEq => curve(&tone.pre_eq),
            Self::PostEq => curve(&tone.post_eq),
            // Gain here is time: the deepest band, as a rate.
            Self::DecayEq => {
                let live = tone.decay_eq.iter().filter(|b| b.enabled && b.used).count();
                let peak = tone
                    .decay_eq
                    .iter()
                    .filter(|b| b.enabled && b.used)
                    .map(|b| b.gain)
                    .fold(0.0_f32, |a, g| if g.abs() > a.abs() { g } else { a });
                if live == 0 {
                    "flat".to_owned()
                } else {
                    format!("{live} · ×{:.2}", 10.0_f32.powf(peak / 20.0))
                }
            }
            Self::Knobs | Self::Presets => String::new(),
            Self::Wide => format!("{:.0}%", tone.wide * 100.0),
            Self::Pitch => format!("{:+}st · {:.0}%", tone.pitch, tone.pitch_mix * 100.0),
            Self::Gate => format!("{:.0}dB · {:.0}", tone.gate.threshold, tone.gate.range),
            Self::RescueComp => squash(tone.rescue_comp),
            Self::Comp => squash(tone.comp),
            // The even share is the number an engineer reads a
            // saturator by — it is what separates a valve from a rail
            // — and it is measured, not read off the knob.
            Self::Mod => format!(
                "{:?} · {:.1} Hz",
                tone.modulation.engine, tone.modulation.rate
            ),
            Self::Sat => {
                let even = crate::live::ladder(&tone.sat).even_share() * 100.0;
                format!("x{:.1} · 2nd {even:.0}%", tone.sat.drive)
            }
            Self::DeEss | Self::DeEssIn => suppression(tone.de_ess),
            Self::Delay => {
                let head = format!(
                    "{} · {:.0}%",
                    millis(tone.delay.time),
                    tone.delay.feedback * 100.0
                );
                if rack.editing() {
                    format!("{head} · {}", tone.delay.style.label().to_lowercase())
                } else {
                    head
                }
            }
            Self::Reverb => {
                let head = format!(
                    "{:.1}s · {:.0}%",
                    tone.reverb.decay,
                    tone.reverb.mix * 100.0
                );
                if rack.editing() {
                    format!("{head} · {}", tone.reverb.algorithm.name().to_lowercase())
                } else {
                    head
                }
            }
        }
    }

    /// The glyph that says which MACHINE this unit is, if it has one.
    ///
    /// Three units are a family of machines rather than one: the
    /// saturator is a valve or a rail, the delay is tape or a chip, the
    /// reverb is a hall or a spring. The plugin's own faces put that
    /// first — "you should know which delay you are looking at before
    /// you read a word" — and the strip says it in eight pixels beside
    /// the name.
    #[must_use]
    pub fn glyph(self, tone: &Tone) -> Option<Glyph> {
        match self {
            Self::Sat => Some(Glyph::Circuit(tone.sat_circuit())),
            Self::Delay => Some(Glyph::Delay(tone.delay.family())),
            Self::Reverb => Some(Glyph::Room(tone.reverb.family())),
            _ => None,
        }
    }

    /// Whether this unit's glyph is a switch — clicked to cycle the
    /// machine. The three that are a family of machines.
    #[must_use]
    pub const fn glyph_switches(self) -> bool {
        matches!(self, Self::Sat | Self::Delay | Self::Reverb)
    }

    /// How tall this panel is at a given rack tier.
    ///
    /// The natural height at every tier. A focused rack used to give
    /// the pictures that are time — the delay's repeats, the reverb's
    /// tail — twice the height, and the saturator half again; that
    /// put a focused return's rows at different heights from the same
    /// rows on the return beside it, and the mixer is read across.
    /// Height is a fact about the unit, not about how wide it is.
    #[must_use]
    pub const fn natural_at(self, rack: Rack) -> f64 {
        let _ = rack;
        self.natural()
    }

    /// How tall this panel wants to be, in pixels.
    ///
    /// A HEIGHT, not a share. A share of the panel means a rack that
    /// grows with the window, and these do not need to: a frequency
    /// response is readable at a hundred and seventy pixels and no more
    /// readable at four hundred — it is the same curve with more air
    /// around it. What the extra height is worth something to is the
    /// FADER, which is a ruler and gets more precise the longer it is.
    ///
    /// So the rack asks for what it needs and the strip keeps the rest.
    const fn natural(self) -> f64 {
        match self {
            // Two axes to read, and the panels where extra height buys
            // resolution rather than air: a 3 dB decision and a 12 dB
            // one have to look different.
            Self::RescueEq
            | Self::Eq
            | Self::Space
            | Self::PreEq
            | Self::PostEq
            | Self::PolishEq => 175.0,
            // Time over frequency: the same graph, read as a rate.
            Self::DecayEq => 150.0,
            // A row of five knobs and their legends.
            Self::Knobs => 52.0,
            // A field and an interval: one figure each.
            Self::Wide => 90.0,
            Self::Pitch => 74.0,
            // One row of chips.
            Self::Presets => PRESETS_H,
            // The suppressors are a spectrum and a cut hanging off it —
            // shorter than an EQ, because there is one curve to read
            // rather than a curve against a grid of decisions.
            Self::DeEss | Self::DeEssIn => 130.0,
            // One display, with the envelope drawn into it. Taller than
            // the saturator because the levels in it are read against a
            // threshold, and a threshold you cannot place precisely is
            // a threshold you set by ear twice.
            Self::Comp => 170.0,
            // The rescue compressor catches what is wrong rather than
            // shaping what is right: a coarser decision, a shorter
            // display.
            Self::RescueComp => 120.0,
            // The gate is the same display without the envelope: a line
            // and what falls under it.
            Self::Gate => 120.0,
            // A bent line through a square, and the ladder of what it
            // adds beside it. The ladder wants the height the curve
            // has, no more.
            Self::Sat => 130.0 + SELECTOR + 2.0,
            // Its face is the FX view's; in a rack it is a time picture.
            Self::Mod => 74.0,
            // Time pictures, both. A delay needs width for its taps and
            // no height beyond telling them apart; a reverb's tail is a
            // single falling line.
            // Time pictures, both, with the machine selector under them.
            Self::Delay | Self::Reverb => 100.0 + SELECTOR + 2.0,
        }
    }
}

/// A phase's own colour.
///
/// The same hues the toolbar icons are built with — see
/// `daw/features/reaper/fts-icons/examples/mix.toml`, where each phase
/// states its own. Taken from there rather than chosen again so the
/// container, the rail button and the REAPER toolbar all agree about
/// what colour Tone is.
pub fn phase_tint(phase: session::mix_phases::MixPhase) -> Color {
    use session::mix_phases::MixPhase as P;
    hex(match phase {
        P::Rescue => "#EF4444",
        P::Balance => "#22C55E",
        P::Tone => "#FACC15",
        P::Polish => "#06B6D4",
        P::Relational => "#3B82F6",
        P::Depth => "#8B5CF6",
        P::Creative => "#EC4899",
        P::Overview => "#6366F1",
    })
}

/// The role a theme draws a phase's bar as: its name and colour, and the
/// chevron that folds it — or, for a phase this chain has nothing in,
/// the same bar faint and with nothing to fold (`phase.blank`).
pub const PHASE: &str = "rack.phase";

/// A phase's bar drawn by the theme, when it has one: false, and the
/// painters draw it. `open` is the fold and the lit grip of a real
/// phase; `None` is a blank one.
fn phase_themed(
    scene: &mut Scene,
    font: &Font,
    phase: session::mix_phases::MixPhase,
    at: Panel,
    open: Option<(bool, Option<Grip>)>,
) -> bool {
    let theme = crate::skin::loaded();
    let Some(theme) = theme.as_deref().filter(|t| t.has_role(PHASE)) else {
        return false;
    };
    let [r, g, b, a] = phase_tint(phase).to_rgba8().to_u8_array();
    let fields = crate::skin::values(&[
        (
            "phase.name",
            theme_runtime::Value::text(phase.display_name()),
        ),
        (
            "phase.tint",
            theme_runtime::Value::Color(theme_runtime::Rgba::from_rgba8(r, g, b, a)),
        ),
        ("phase.blank", theme_runtime::Value::Bool(open.is_none())),
        (
            "phase.shut",
            theme_runtime::Value::Bool(open.is_some_and(|(shut, _)| shut)),
        ),
        (
            "phase.held",
            theme_runtime::Value::Bool(
                open.is_some_and(|(_, lit)| lit == Some(Grip::Phase(phase))),
            ),
        ),
    ]);
    theme.record(
        PHASE,
        (at.width, at.height),
        &fields,
        &mut crate::skin::canvas::Host { scene, font },
        Affine::translate((at.x, at.y)),
    )
}

/// A phase's bar at `at` — the theme's (`rack.phase`), or the painters':
/// `open` is the fold and the lit grip of a phase with units, `None` a
/// blank phase's faint bar. What the rack draws over each phase, for
/// anything else that groups a chain by phase (the FX view's devices).
pub fn phase_bar(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    phase: session::mix_phases::MixPhase,
    at: Panel,
    open: Option<(bool, Option<Grip>)>,
) {
    if phase_themed(scene, font, phase, at, open) {
        return;
    }
    match open {
        Some((shut, lit)) => container(scene, palette, font, phase, at, shut, lit),
        None => blank(scene, palette, font, phase, at),
    }
}

/// A phase's container bar: its name, and the chevron that folds it.
///
/// The chain is eleven units long and a strip shows about half of it,
/// so the thing that makes it legible is knowing WHICH PASS you are
/// looking at without counting panels. The bar says it, and folding one
/// puts the passes you are not working out of the way without taking
/// them off the chain.
fn container(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    phase: session::mix_phases::MixPhase,
    at: Panel,
    shut: bool,
    lit: Option<Grip>,
) {
    let held = lit == Some(Grip::Phase(phase));
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        if held {
            palette.tcp_field
        } else {
            palette.tcp_meter_well
        },
        None,
        &at.rect(),
    );
    // The phase's own colour down the left edge — the same hue its
    // button wears in the rail, so the container and the button that
    // will focus it are obviously the same thing.
    let tint = phase_tint(phase);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        tint,
        None,
        &Rect::new(at.x, at.y, at.x + 2.0, at.y + at.height),
    );
    const SIZE: f32 = 8.0;
    crate::tcp::glyphs(
        scene,
        font,
        if held { palette.text } else { palette.text_dim },
        phase.display_name(),
        at.x + 6.0,
        at.y + at.height - 4.0,
        SIZE,
    );
    // The chevron: up when the container is open, down when it is shut,
    // which is the direction a click will move its contents.
    let right = at.x + at.width;
    let (cx, cy) = (right - 9.0, at.y + at.height / 2.0);
    let arm = 3.2;
    let ink = if held {
        palette.text
    } else {
        palette.text_faint
    };
    let tip = if shut { cy + arm * 0.6 } else { cy - arm * 0.6 };
    let base = if shut { cy - arm * 0.6 } else { cy + arm * 0.6 };
    rule_wide(scene, ink, Line::new((cx - arm, base), (cx, tip)), 1.4);
    rule_wide(scene, ink, Line::new((cx, tip), (cx + arm, base)), 1.4);
}

/// How faint a blank phase is drawn against a real one.
const BLANK_ALPHA: f32 = 0.35;

/// A phase this chain has nothing in: the container's bar, faint, with
/// no chevron — there is nothing to fold. It holds the row so the
/// phases under it line up with the strips beside it.
fn blank(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    phase: session::mix_phases::MixPhase,
    at: Panel,
) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.tcp_meter_well.multiply_alpha(0.5),
        None,
        &at.rect(),
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        phase_tint(phase).multiply_alpha(BLANK_ALPHA),
        None,
        &Rect::new(at.x, at.y, at.x + 2.0, at.y + at.height),
    );
    const SIZE: f32 = 8.0;
    crate::tcp::glyphs(
        scene,
        font,
        palette.text_faint.multiply_alpha(BLANK_ALPHA),
        phase.display_name(),
        at.x + 6.0,
        at.y + at.height - 4.0,
        SIZE,
    );
}

/// A panel's well — darker than the strip, so the rack reads as inset
/// rather than as three more controls on the strip's own surface.
fn ground(scene: &mut Scene, palette: &Palette, at: Panel) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.tcp_meter_well,
        None,
        &at.rect(),
    );
}

/// The plugin's own EQ graph, painted into the rack.
///
/// Not a redraw of it — `eq_ui::eq_graph_painter` records into an
/// `anyrender::Scene` under a transform, which is exactly what this
/// rack does, so the strip can hand it a sub-rectangle and get the
/// editor's own grid, curves, fills and per-shape nodes.
///
/// That is the whole point of the plugin's UI being renderer-agnostic:
/// a curve in the strip that disagreed with the curve in the plugin
/// window would be worse than no curve at all, because it would be
/// believed — and the only way to guarantee they agree is for them to
/// be the same code.
///
/// The labels come off, because a strip panel is a hundred and thirty
/// pixels wide and the editor's are authored for eight hundred.
/// The web build has no plugin painter (`eq-ui`'s `graph` is Blitz-only):
/// `false`, and the caller draws the rack's own curve instead.
#[cfg(not(feature = "native"))]
fn eq_from_plugin(
    _scene: &mut Scene,
    _tone: &Tone,
    _bands: &[EqBand],
    _spectrum: &[f32],
    _at: Panel,
    _rack: Rack,
) -> bool {
    false
}

#[cfg(feature = "native")]
fn eq_from_plugin(
    scene: &mut Scene,
    tone: &Tone,
    bands: &[EqBand],
    spectrum: &[f32],
    at: Panel,
    rack: Rack,
) -> bool {
    let state = eq_ui::eq_graph_model::EqGraphRenderState::new();
    state.bands.write().clone_from(&bands.to_vec());
    // The analyser, behind the curves. The painter draws it when the
    // state carries it and skips it when it does not, so a track with
    // no signal gets the same graph it had before this existed.
    if spectrum.len() >= 2 {
        state.spectrum_db.write().extend_from_slice(spectrum);
    }
    {
        let mut config = state.config.write();
        config.db_range = tone.eq_db_range();
        config.min_freq = 20.0;
        config.max_freq = 20_000.0;
        config.sample_rate = DISPLAY_RATE;
        // Two looks, from one painter.
        //
        // At a glance width the rack has already drawn its own ground,
        // its dB ladder and its decades at a size that fits a strip,
        // and the editor's own would cover or crowd them — so the
        // plugin draws only what the rack cannot: the per-band curves,
        // their colours, the filled response. Its nodes come off too,
        // because four of them at their authored size overlap across a
        // hundred and thirty pixels and hide the curve they sit on; the
        // rack draws its own markers instead, sized for a strip.
        //
        // At a focus width there is room for the real thing, so the
        // plugin draws all of it and the rack draws none.
        let editing = rack.editing();
        config.fill_background = false;
        config.show_freq_labels = editing;
        config.show_db_labels = editing;
        // The grid is the rack's at every tier: at Focus its lines are
        // where its figures are (`focus_grid`), which the painter's, at
        // its own fractions of the range, were not.
        config.show_grid = false;
        config.fill_curve = true;
        config.node_scale = if editing { 1.0 } else { 0.0 };
        config.scale = 1.0;
    }
    let Ok(width) = u32::try_from(at.width.max(0.0).round() as i64) else {
        return false;
    };
    let Ok(height) = u32::try_from(at.height.max(0.0).round() as i64) else {
        return false;
    };
    if width < 2 || height < 2 {
        return false;
    }
    eq_ui::eq_graph_painter::paint_eq_graph_scene(
        scene,
        &state,
        Affine::translate((at.x, at.y)),
        width,
        height,
    );
    true
}

/// The EQ's response across the audible band.
#[expect(
    clippy::too_many_arguments,
    reason = "a drawing and everything it needs"
)]
fn eq(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    tone: &Tone,
    which: Which,
    bands: &[EqBand],
    spectrum: &[f32],
    at: Panel,
    rack: Rack,
    lit: Option<Grip>,
    // A colour for the whole graph, for a graph whose vertical axis is
    // not gain. `None` is the EQ's own look: the plugin's painter and
    // its per-band hues.
    tint: Option<Color>,
) {
    let freq = FreqAxis::audible();
    let db = DbAxis::symmetric(tone.eq_db_range());
    let right = at.x + at.width;
    let bottom = at.y + at.height;
    if let Some(tint) = tint {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            tint.multiply_alpha(0.08),
            None,
            &at.rect(),
        );
    }

    // Only at Full: at Focus the plugin draws its own grid and labels,
    // calibrated for a graph you are working in rather than glancing at.
    if rack == Rack::Full {
        for (hz, name) in DECADES {
            let x = freq.freq_to_x(hz, at.x, right);
            rule(scene, palette.grid_beat, Line::new((x, at.y), (x, bottom)));
            // Labelled along the floor, so a bump can be described to
            // someone else without opening the plugin.
            const SIZE: f32 = 6.0;
            let w = font.width(name, SIZE);
            if x - w / 2.0 >= at.x && x + w / 2.0 <= right {
                crate::tcp::glyphs(
                    scene,
                    font,
                    palette.text_faint,
                    name,
                    x - w / 2.0,
                    bottom - 2.0,
                    SIZE,
                );
            }
        }
        // The dB ladder. A strip panel is tall and a gentle EQ uses
        // little of it, so most of the box is empty — and empty space
        // with no marks in it makes a 3 dB decision look the same as a
        // 12 dB one. These are what turn the height into a scale.
        //
        // Stepped by the range the graph is drawn to, by the painter's
        // own law — a ladder that stayed at 6 and 12 would be two lines
        // off the top of a ±3 graph and a smear on a ±30 one.
        let range = tone.eq_db_range();
        let step = ladder_step(range);
        let mut gain = step;
        while gain <= range {
            for at_db in [gain, -gain] {
                let y = db.db_to_y(at_db, at.y, bottom);
                rule(scene, palette.grid_beat, Line::new((at.x, y), (right, y)));
            }
            gain += step;
        }
    }
    if rack.editing() {
        focus_grid(scene, palette, tone, at);
    }
    // Unity, always: without it a boost and a cut look the same.
    let zero = db.db_to_y(0.0, at.y, bottom);
    rule(scene, palette.grid, Line::new((at.x, zero), (right, zero)));

    if bands.is_empty() {
        return;
    }
    // The plugin's own graph, where there is room for it. Its curve, its
    // fill, its per-band colours — the same code the editor paints with,
    // so the strip and the plugin window cannot disagree about what the
    // EQ is doing.
    //
    // The handles are still drawn below, because the editor's nodes are
    // authored for a graph eight hundred pixels wide and this one is a
    // hundred and thirty: at that size a labelled node with a shape
    // glyph is a smudge, where a dot is a position.
    // The analyser at a glance width is a wash, not the painter's
    // analyser: at a hundred and thirty pixels the painter's fill sat
    // over the curve as brightly as the curve itself, and the setting
    // was the thing you could not find. The wash is behind the graph
    // and a fraction of the strength; the focus tier, with room to
    // read both, gets the painter's own.
    let washed = matches!(rack, Rack::Full | Rack::Curves) && spectrum.len() >= 2 && tint.is_none();
    if washed {
        spectrum_wash(scene, spectrum, at);
    }
    // The painter at every tier that has a curve — the curves tier
    // too, which used to fall back to the one total-response line and
    // lost the bands' colours and the composition they show.
    let painted = tint.is_none()
        && rack != Rack::Minimal
        && eq_from_plugin(
            scene,
            tone,
            bands,
            if washed { &[] } else { spectrum },
            at,
            rack,
        );
    if !painted {
        // The fallback: the same response function the plugin's painter
        // uses, as one polyline. What the narrow tier gets, what a
        // graph too small for the plugin's own drawing falls back to,
        // and what a tinted graph always is.
        let points: Vec<(f64, f64)> = (0..SAMPLES)
            .map(|i| {
                let t = crate::num::coord(i) / crate::num::coord(SAMPLES.saturating_sub(1).max(1));
                let hz = freq.norm_to_freq(t);
                let gain = calculate_combined_response(bands, hz, DISPLAY_RATE);
                (
                    freq.freq_to_x(hz, at.x, right),
                    db.db_to_y(gain, at.y, bottom).clamp(at.y, bottom),
                )
            })
            .collect();
        if let Some(tint) = tint {
            // Filled to unity, so a longer band and a shorter one read
            // as areas above and below the line rather than as one
            // wiggle.
            let mut area = BezPath::new();
            area.move_to((at.x, zero));
            for point in points.iter().copied() {
                area.line_to(point);
            }
            area.line_to((right, zero));
            area.close_path();
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                tint.multiply_alpha(0.22),
                None,
                &area,
            );
        }
        curve(scene, tint.unwrap_or(EQ_INK), points.into_iter(), 1.5);
    }
    if rack.editing() {
        focus_labels(scene, palette, font, tone, at);
    }

    // The bands themselves, as handles on the curve.
    //
    // Without them the panel is a line: you can see that something was
    // done and not how many decisions it took or where they sit. A
    // four-band cut and one wide shelf can draw the same curve, and
    // they are not the same setting.
    //
    // Only at `Full`. At `Curves` a handle is three pixels of dot on a
    // curve two pixels wide, which reads as a kink in the line — and at
    // `Focus` the plugin has already drawn its own nodes, which are the
    // ones you came to the focus width for.
    if rack != Rack::Full {
        return;
    }
    for (index, band) in bands.iter().enumerate() {
        if !(band.enabled && band.used) {
            continue;
        }
        let x = freq.freq_to_x(f64::from(band.frequency), at.x, right);
        let y = db.db_to_y(f64::from(band.gain), at.y, bottom);
        if x < at.x || x > right || y < at.y || y > bottom {
            continue;
        }
        // The band's OWN colour, from the plugin's frequency map — the
        // same hue its node takes in the editor and at the focus tier.
        //
        // One accent for every band said "here is a band" and nothing
        // else; a hue that sweeps red to violet across the spectrum
        // says WHICH band, which is how you tell the low shelf from the
        // air band without reading a number. The two tiers now differ
        // in the size of the marker, not in what it means.
        let grown = lit == Some(Grip::Band(which, index));
        let r = if grown { HANDLE + 1.6 } else { HANDLE };
        dot(scene, palette.tcp_meter_well, (x, y), r + 1.0);
        dot(
            scene,
            tint.unwrap_or_else(|| band_color(f64::from(band.frequency))),
            (x, y),
            r,
        );
    }

    // The zoom, last, so nothing draws over the one thing in the panel
    // that says what the rest of it means.
    scale(scene, palette, font, tone, which, at, lit);
}

/// The analyser as a faint wash behind a glance-width EQ.
///
/// The painter's grey, at a quarter of the painter's strength: enough
/// to see where the energy is under the curve, not enough to compete
/// with it.
fn spectrum_wash(scene: &mut Scene, spectrum: &[f32], at: Panel) {
    const ACROSS: usize = 48;
    let bottom = at.y + at.height;
    let full = (20_000.0_f64 / 20.0).log10();
    let points: Vec<(f64, f64)> = (0..ACROSS)
        .map(|i| {
            let t = crate::num::coord(i) / crate::num::coord(ACROSS.saturating_sub(1));
            let hz = 20.0 * 10.0_f64.powf(t * full);
            let db = bin_at(spectrum, hz);
            let level =
                ((db - SUPPRESS_FLOOR_DB) / (SUPPRESS_CEIL_DB - SUPPRESS_FLOOR_DB)).clamp(0.0, 1.0);
            (t.mul_add(at.width, at.x), bottom - level * at.height)
        })
        .collect();
    let grey = Color::from_rgba8(140, 140, 150, 0xff);
    area_under(scene, grey.multiply_alpha(0.07), &points, bottom);
    curve(scene, grey.multiply_alpha(0.22), points.into_iter(), 1.0);
}

/// The ladder's spacing for a given range.
///
/// The painter's own law (`eq_graph_painter`), so the rack's grid and
/// the editor's land on the same decibels.
fn ladder_step(range: f64) -> f64 {
    if range <= 6.0 {
        3.0
    } else if range <= 12.0 {
        6.0
    } else {
        12.0
    }
}

/// The EQ graph's zoom, as a chip at the top of it.
///
/// A curve without its scale is a shape, not a measurement: the same
/// wobble is a surgical half-decibel at ±3 and an inaudible nothing at
/// ±30. This is the label that makes the panel readable at a glance,
/// and the control that changes it — clicked to step out, wheeled
/// either way, double-clicked back to the default.
fn scale(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    tone: &Tone,
    which: Which,
    at: Panel,
    lit: Option<Grip>,
) {
    let chip = scale_chip(at);
    if chip.width < 20.0 || chip.height < 8.0 {
        return;
    }
    let held = lit == Some(Grip::Scale(which));
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        if held {
            palette.tcp_field
        } else {
            palette.tcp_meter_well
        },
        None,
        &vello::kurbo::Rect::new(chip.x, chip.y, chip.x + chip.width, chip.y + chip.height),
    );
    const SIZE: f32 = 8.0;
    let label = format!("±{:.0}", tone.eq_db_range());
    let width = font.width(&label, SIZE);
    crate::tcp::glyphs(
        scene,
        font,
        if held {
            palette.text
        } else {
            palette.text_faint
        },
        &label,
        chip.x + (chip.width - width) / 2.0,
        chip.y + chip.height - 3.0,
        SIZE,
    );
}

/// How big an EQ band's handle is.
///
/// Small enough that four of them on one curve do not merge, large
/// enough to be a target: this is the radius a pointer will have to
/// find once bands are draggable.
const HANDLE: f64 = 2.6;

/// The frequencies a graph with room to be worked in is labelled at.
const FOCUS_MARKS: [(f64, &str); 9] = [
    (50.0, "50"),
    (100.0, "100"),
    (200.0, "200"),
    (500.0, "500"),
    (1_000.0, "1k"),
    (2_000.0, "2k"),
    (5_000.0, "5k"),
    (10_000.0, "10k"),
    (20_000.0, "20k"),
];

/// The gains a worked-in graph is ruled and labelled at: the ladder's
/// steps, halved where it has only its ends, which are the edges.
fn focus_steps(range: f64) -> (f64, impl Iterator<Item = f64>) {
    let step = ladder_step(range);
    let step = if range / step < 2.0 { step / 2.0 } else { step };
    let count = crate::num::index((range / step).floor());
    (
        step,
        (1..=count).flat_map(move |i| {
            let gain = step * crate::num::coord(i);
            [gain, -gain]
        }),
    )
}

/// A worked-in graph's grid: a line at each frequency and gain
/// `focus_labels` names.
fn focus_grid(scene: &mut Scene, palette: &Palette, tone: &Tone, at: Panel) {
    let freq = FreqAxis::audible();
    let db = DbAxis::symmetric(tone.eq_db_range());
    let right = at.x + at.width;
    let bottom = at.y + at.height;
    for (hz, _) in FOCUS_MARKS {
        let x = freq.freq_to_x(hz, at.x, right);
        rule(scene, palette.grid_beat, Line::new((x, at.y), (x, bottom)));
    }
    for gain in focus_steps(tone.eq_db_range()).1 {
        let y = db.db_to_y(gain, at.y, bottom);
        rule(scene, palette.grid_beat, Line::new((at.x, y), (right, y)));
    }
}

/// A worked-in graph's scales: its frequencies along the floor and its
/// gains up the left edge.
///
/// The painter draws the grid at this tier but not the numbers on it —
/// those are its DOM editor's, which a painted graph does not have — so
/// without these a close-up's lines were a ruler with no figures.
fn focus_labels(scene: &mut Scene, palette: &Palette, font: &Font, tone: &Tone, at: Panel) {
    const SIZE: f32 = 7.0;
    let freq = FreqAxis::audible();
    let right = at.x + at.width;
    let bottom = at.y + at.height;
    for (hz, name) in FOCUS_MARKS {
        let x = freq.freq_to_x(hz, at.x, right);
        let w = font.width(name, SIZE);
        let left = (x - w / 2.0).clamp(at.x + 2.0, right - w - 2.0);
        crate::tcp::glyphs(
            scene,
            font,
            palette.text_faint,
            name,
            left,
            bottom - 3.0,
            SIZE,
        );
    }
    let range = tone.eq_db_range();
    let db = DbAxis::symmetric(range);
    let (step, gains) = focus_steps(range);
    for at_db in gains {
        let y = db.db_to_y(at_db, at.y, bottom);
        // Clear of the floor's frequencies and the header above.
        if y > at.y + 10.0 && y < bottom - 12.0 {
            let label = if step < 1.0 {
                format!("{at_db:+.1}")
            } else {
                format!("{at_db:+.0}")
            };
            crate::tcp::glyphs(
                scene,
                font,
                palette.text_faint,
                &label,
                at.x + 3.0,
                y - 2.0,
                SIZE,
            );
        }
    }
}

/// The decades the frequency axis is labelled at.
///
/// Three, not five: at a strip's width the labels are 6pt and four of
/// them collide. 100, 1k and 10k are the ones a tone decision is
/// described in — "take out some 300", "lift the 3k" — and they bracket
/// the two that are not.
const DECADES: [(f64, &str); 3] = [(100.0, "100"), (1_000.0, "1k"), (10_000.0, "10k")];

// The transfer curve is gone from the strip. It said the same thing
// the threshold line and the ratio arrow now say between them — where
// it starts and how hard — in a form you had to read rather than
// reach for. `comp_graph_svg::transfer_curve_path` is still the
// plugin's, and still what a focused editor would draw.

/// The compressor: its ramps, its levels and what it is doing to them.
fn comp(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    comp: Comp,
    at: Panel,
    rack: Rack,
    lit: Option<Grip>,
) {
    let body = at;
    let at = comp_split(body, rack);
    if rack.detailed()
        && let Some(square) = transfer_area(body)
    {
        transfer_curve(scene, palette, font, comp, square, lit);
    }
    let right = at.x + at.width;
    // The comp editor's own axis: 0 dB at the top, −60 at the floor.
    // Taken from the plugin so the threshold line, the ladder and
    // whatever level is drawn over them land on the same numbers it
    // uses.
    let to_y = |db: f64| at.y + comp_ui::comp_graph_svg::db_to_y(db, at.height);

    // The ladder the levels are read against. Without it the threshold
    // is a line at a height rather than a line at a level.
    if rack.detailed() {
        for db in [-12.0, -24.0, -36.0, -48.0] {
            let y = to_y(db);
            rule(scene, palette.grid_beat, Line::new((at.x, y), (right, y)));
        }
    }

    // The threshold: a line ACROSS the display at its own level, which
    // is where a threshold belongs. It was a knob, and a knob makes you
    // read a number and compare it to a meter somewhere else; a line
    // over the levels is the comparison.
    let y = threshold_y(comp, at);
    let held = matches!(lit, Some(Grip::Threshold(_)));
    let red = hex(comp_ui::comp_graph_svg::colors::THRESHOLD);
    rule_wide(
        scene,
        red,
        Line::new((at.x, y), (right, y)),
        if held { 2.5 } else { 1.5 },
    );
    // A tab at the right end while the line is HELD, so the drag has
    // something under it. Not at rest: the whole display is the
    // threshold's target (see `comp_grip`), so there is nothing to
    // aim at — and a knob on the line's end, where the ratio's arrow
    // used to hang, read as the arrow still being there.
    if rack.detailed() {
        if held {
            dot(scene, red, (right - HANDLE - 2.6, y), HANDLE + 1.6);
        }
        // And the settings themselves, as the reduction they produce —
        // outlined over the live one, in the same axes.
        envelope(scene, font, comp, at, lit);
    }
}

/// Where the threshold line sits in a compressor display.
///
/// One function because three things have to agree about it: the line
/// itself, the envelope glyph that meets it, and the hit test that
/// decides which of the two a click belongs to.
fn threshold_y(comp: Comp, at: Panel) -> f64 {
    let y = at.y + comp_ui::comp_graph_svg::db_to_y(f64::from(comp.threshold), at.height);
    y.clamp(at.y, at.y + at.height)
}

/// The compressor's settings, drawn on the display they act in.
///
/// Two strips and an arrow. The RATIO is an arrow down from the
/// threshold line near the left edge, as far as the ratio would take
/// a full-scale signal, with the number at its tip — pulled down for
/// more, because down is the direction the signal goes. The two TIMES
/// run along the floor — attack over release, FAST at the left and
/// SLOW at the right, the default dead centre. Each is a marker on a
/// two-sided scale with the fill running from the tick to the marker,
/// so a departure from the default is a bar in the direction it
/// departed.
fn envelope(scene: &mut Scene, font: &Font, comp: Comp, at: Panel, lit: Option<Grip>) {
    let red = hex(comp_ui::comp_graph_svg::colors::THRESHOLD);
    if let Some(strips) = Strips::of(at) {
        strips.draw(scene, font, comp, red, lit);
    }
    Arrow::of(comp, at).draw(scene, font, comp, red, matches!(lit, Some(Grip::Ratio(_))));
}

/// The column the ratio's strip lives in, at the display's left edge.
const RAIL: f64 = 11.0;

/// A compressor time on its strip: a range with a default in it.
///
/// The strip is two scales joined at the middle — the fast half from
/// the range's floor up to the default, the slow half from the default
/// up to the ceiling, each logarithmic — so the default sits dead
/// centre whatever the range, and a drag left is faster and right is
/// slower by the same feel on either side.
#[derive(Clone, Copy, Debug)]
struct Time {
    ms: f64,
    min: f64,
    default: f64,
    max: f64,
}

impl Time {
    fn attack(ms: f32) -> Self {
        Self {
            ms: f64::from(ms),
            min: 0.1,
            default: f64::from(Comp::default().attack),
            max: 200.0,
        }
    }

    fn release(ms: f32) -> Self {
        Self {
            ms: f64::from(ms),
            min: 5.0,
            default: f64::from(Comp::default().release),
            max: 3_000.0,
        }
    }

    /// The ratio on the same kind of scale: 1:1 to 20:1, the default
    /// in the middle. Not a time, but a range with a default in it is
    /// a range with a default in it.
    fn ratio(ratio: f32) -> Self {
        Self {
            ms: f64::from(ratio),
            min: 1.0,
            default: f64::from(Comp::default().ratio),
            max: 20.0,
        }
    }

    /// Where on the strip, 0 (fastest) to 1 (slowest), 0.5 the default.
    fn place(self) -> f64 {
        let ms = self.ms.clamp(self.min, self.max);
        if ms <= self.default {
            0.5 * log_norm(ms, self.min, self.default)
        } else {
            0.5 + 0.5 * log_norm(ms, self.default, self.max)
        }
    }

    /// The time at a place on the strip.
    fn at(self, place: f64) -> f64 {
        let place = place.clamp(0.0, 1.0);
        if place <= 0.5 {
            log_denorm(place * 2.0, self.min, self.default)
        } else {
            log_denorm((place - 0.5) * 2.0, self.default, self.max)
        }
    }
}

/// The attack and release strips along the display's floor.
///
/// Shared by the drawing and the hit test, for the reason everything
/// in this module is: a marker that is not where its strip is drawn is
/// a marker that moves the wrong time.
#[derive(Clone, Copy, Debug)]
pub struct Strips {
    pub attack: Rect,
    pub release: Rect,
}

/// The ratio's arrow: down from the threshold line, near the left
/// edge, as far as the ratio would take a full-scale signal.
///
/// Back from a strip up the rail: the arrow hangs off the line it
/// acts on, so it is read against the threshold and not against a
/// scale of its own. Near the LEFT edge so a hand aiming at the line
/// — which is the whole display — does not land on it, and clear of
/// where the line's right end used to carry it.
#[derive(Clone, Copy, Debug)]
pub struct Arrow {
    pub x: f64,
    pub top: f64,
    pub tip: f64,
}

/// How far in from the display's left edge the arrow hangs.
const ARROW_X: f64 = 14.0;

/// The number at the arrow's tip, and the words at the strips' ends.
const TINY: f32 = 6.0;

impl Arrow {
    #[must_use]
    pub fn of(comp: Comp, at: Panel) -> Self {
        let top = threshold_y(comp, at);
        let floor = at.y + at.height - STRIPS_H;
        Self {
            x: at.x + ARROW_X,
            top,
            tip: (top + ratio_drop(comp, at.height)).min(floor.max(top)),
        }
    }

    fn draw(self, scene: &mut Scene, font: &Font, comp: Comp, ink: Color, held: bool) {
        let ink = if held { ink } else { ink.multiply_alpha(0.85) };
        rule_wide(
            scene,
            ink,
            Line::new((self.x, self.top), (self.x, self.tip)),
            if held { 2.0 } else { 1.2 },
        );
        let head = 3.0;
        let mut path = BezPath::new();
        path.move_to((self.x - head, self.tip - head * 1.6));
        path.line_to((self.x + head, self.tip - head * 1.6));
        path.line_to((self.x, self.tip));
        path.close_path();
        scene.fill(Fill::NonZero, Affine::IDENTITY, ink, None, &path);
        // The number, tiny, beside the tip: 4:1 is what the arrow says,
        // and the arrow's length says it only roughly.
        let ratio = f64::from(comp.ratio);
        let label = if (ratio - ratio.round()).abs() < 0.05 {
            format!("{ratio:.0}:1")
        } else {
            format!("{ratio:.1}:1")
        };
        crate::tcp::glyphs(
            scene,
            font,
            ink,
            &label,
            self.x + head + 2.0,
            self.tip + 1.0,
            TINY,
        );
    }

    fn holds(self, x: f64, y: f64) -> bool {
        near_segment(
            (x, y),
            (self.x, self.top),
            (self.x, self.tip.max(self.top + 6.0)),
        ) <= GRAB
    }
}

/// How tall one strip is.
const STRIP_H: f64 = 5.0;

/// The letter at a time strip's right end, and the room kept for it.
const STRIP_LABEL_SIZE: f32 = 7.0;
const STRIP_LABEL_W: f64 = 8.0;

/// The row over the strips where FAST and SLOW sit, so the strips
/// keep the whole width for their travel.
const STRIP_WORDS_H: f64 = 7.0;

/// The gap between the two, and under the lower one.
const STRIP_GAP: f64 = 4.0;

/// How tall the pair takes, with their gaps: what the live trace under
/// them is clear of.
pub const STRIPS_H: f64 = STRIP_H * 2.0 + STRIP_GAP * 3.0 + STRIP_WORDS_H;

impl Strips {
    /// The two strips for a display, or `None` when there is no room
    /// for a marker to travel.
    #[must_use]
    pub fn of(at: Panel) -> Option<Self> {
        if at.width < 70.0 || at.height < 40.0 {
            return None;
        }
        // The whole width but the letter at the right: the direction
        // words sit in their own row above, so the travel is as long
        // as the display is wide.
        let left = at.x + 3.0;
        let right = at.x + at.width - 3.0 - STRIP_LABEL_W;
        let floor = at.y + at.height - STRIP_GAP;
        let release = Rect::new(left, floor - STRIP_H, right, floor);
        let attack = Rect::new(
            left,
            release.y0 - STRIP_GAP - STRIP_H,
            right,
            release.y0 - STRIP_GAP,
        );
        Some(Self { attack, release })
    }

    /// Where a time's marker sits on its strip: fast at the left, slow
    /// at the right, the default in the middle.
    fn marker(strip: Rect, time: Time) -> (f64, f64) {
        (
            time.place().mul_add(strip.width(), strip.x0),
            strip.center().y,
        )
    }

    fn draw(self, scene: &mut Scene, font: &Font, comp: Comp, ink: Color, lit: Option<Grip>) {
        // FAST and SLOW once each, in the row over the strips at their
        // two ends: the direction is the same for both, so it is said
        // once, and above rather than beside so the strips keep the
        // width.
        let over = self.attack.y0 - STRIP_GAP + 1.0;
        let words = ink.multiply_alpha(0.6);
        crate::tcp::glyphs(scene, font, words, "FAST", self.attack.x0, over, TINY);
        let slow_w = f64::from(font.width("SLOW", TINY));
        crate::tcp::glyphs(
            scene,
            font,
            words,
            "SLOW",
            self.attack.x1 - slow_w,
            over,
            TINY,
        );
        for (strip, grip, time, label) in [
            (
                self.attack,
                Grip::Attack(Which::Comp),
                Time::attack(comp.attack),
                "A",
            ),
            (
                self.release,
                Grip::Release(Which::Comp),
                Time::release(comp.release),
                "R",
            ),
        ] {
            let held = lit == Some(grip);
            // Its letter at the right end, so the two are told apart
            // without reading the header.
            crate::tcp::glyphs(
                scene,
                font,
                ink.multiply_alpha(if held { 1.0 } else { 0.7 }),
                label,
                strip.x1 + 3.0,
                strip.y1 + 1.0,
                STRIP_LABEL_SIZE,
            );
            // The track, a tick at the default, and the travel filled
            // from the default to the marker — so a departure from the
            // default is a bar in the direction it departed.
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                ink.multiply_alpha(0.28),
                None,
                &strip.to_rounded_rect(2.0),
            );
            let centre = strip.center().x;
            rule(
                scene,
                ink.multiply_alpha(0.6),
                Line::new((centre, strip.y0 - 1.5), (centre, strip.y1 + 1.5)),
            );
            let (x, y) = Self::marker(strip, time);
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                ink.multiply_alpha(if held { 0.8 } else { 0.55 }),
                None,
                &Rect::new(centre.min(x), strip.y0, centre.max(x), strip.y1).to_rounded_rect(2.0),
            );
            dot(scene, ink, (x, y), if held { HANDLE + 1.4 } else { HANDLE });
        }
        let _ = comp;
    }

    /// Which strip a point is on, if any — the whole strip, not just
    /// the marker, so a time can be set by clicking where it should be.
    fn grip_at(self, x: f64, y: f64) -> Option<Grip> {
        let reach = |strip: Rect| {
            x >= strip.x0 - GRAB
                && x <= strip.x1 + GRAB
                && y >= strip.y0 - GRAB / 2.0
                && y <= strip.y1 + GRAB / 2.0
        };
        if reach(self.attack) {
            Some(Grip::Attack(Which::Comp))
        } else if reach(self.release) {
            Some(Grip::Release(Which::Comp))
        } else {
            None
        }
    }
}

// The knobs are gone. Ratio, attack and release were three numbers you
// read and then imagined the effect of on a display two inches away;
// they are one shape on that display now, in its own axes. See
// `envelope`.

/// The gate: its threshold across the level display, the range it
/// takes off below it, and its times as a glyph between the two.
///
/// The compressor's axis, deliberately — same red line you drag, same
/// dB ladder — because the pair is read together down a chain and a
/// level should sit at the same height on both. What is NOT shared is
/// the story. A compressor's is how much; a gate's is open or shut, and
/// its glyph says so: where the compressor's is a dip, the gate's is a
/// TABLE — attack up, hold flat, release down — standing in the band
/// it acts in. The door lane under the display (drawn in the live
/// pass, see [`levels`]) is when it was open.
fn gate(
    scene: &mut Scene,
    palette: &Palette,
    gate: Gate,
    at: Panel,
    rack: Rack,
    lit: Option<Grip>,
) {
    let at = display_of(at, Which::Gate, rack);
    let right = at.x + at.width;
    let bottom = at.y + at.height;
    let to_y = |db: f64| at.y + comp_ui::comp_graph_svg::db_to_y(db, at.height);

    if rack.detailed() {
        for db in [-12.0, -24.0, -36.0, -48.0] {
            let y = to_y(db);
            rule(scene, palette.grid_beat, Line::new((at.x, y), (right, y)));
        }
    }

    let line = to_y(f64::from(gate.threshold)).clamp(at.y, bottom);
    let floor = to_y(f64::from(gate.threshold + gate.range)).clamp(at.y, bottom);
    // What the gate takes off is what lives BELOW the line, down to the
    // range — so that is the band that gets shaded. A compressor shades
    // above.
    // In the gate's own green: the compressor's red is a reduction,
    // and a gate is a door — its lane says "open" in this colour, and
    // so does everything else on the panel.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        GATE_INK.multiply_alpha(0.18),
        None,
        &Rect::new(at.x, line, right, floor),
    );
    let held = matches!(lit, Some(Grip::Threshold(_)));
    let red = GATE_INK;
    rule_wide(
        scene,
        red,
        Line::new((at.x, line), (right, line)),
        if held { 2.5 } else { 1.5 },
    );
    if rack.detailed() {
        let r = if held { HANDLE + 1.6 } else { HANDLE };
        dot(scene, red, (right - r - 1.0, line), r);
        // The range, as a second line: how far down what stays shut
        // goes. A gate that closed completely would put that line on
        // the floor, and the gap between the two IS the setting.
        let range_held = lit == Some(Grip::Range);
        rule_wide(
            scene,
            red.multiply_alpha(0.55),
            Line::new((at.x, floor), (right, floor)),
            if range_held { 2.0 } else { 1.0 },
        );
        if let Some(shape) = GateGlyph::of(gate, at, line, floor) {
            let width = |grip| if lit == Some(grip) { 2.4 } else { 1.4 };
            curve(
                scene,
                red,
                shape.rise().into_iter(),
                width(Grip::Attack(Which::Gate)),
            );
            curve(
                scene,
                red,
                shape.run().into_iter(),
                width(Grip::Hold(Which::Gate)),
            );
            curve(
                scene,
                red,
                shape.fall().into_iter(),
                width(Grip::Release(Which::Gate)),
            );
            let r = |grip| {
                if lit == Some(grip) {
                    HANDLE + 1.4
                } else {
                    HANDLE * 0.8
                }
            };
            dot(scene, red, shape.open, r(Grip::Attack(Which::Gate)));
            dot(scene, red, shape.close, r(Grip::Release(Which::Gate)));
        }
    }
}

/// The gate's ink: green, the door lane's "open" — so a gate is never
/// mistaken for the compressor it sits beside.
const GATE_INK: Color = Color::from_rgba8(0x34, 0xd3, 0x99, 0xff);

/// The gate's times, as a table standing between its two lines.
///
/// The mirror of the compressor's [`Envelope`]: that one hangs from the
/// ceiling down to the threshold, this one stands up from the range
/// floor to the threshold — because a gate OPENS, and up is open. Three
/// grips, each dragged sideways along the time axis they are widths on:
/// the rising edge is the attack, the flat top is the hold, and the
/// falling edge is the release.
#[derive(Clone, Copy, Debug)]
struct GateGlyph {
    /// On the floor, where the signal arrives.
    start: (f64, f64),
    /// Where the attack reaches the threshold line — the door is open.
    open: (f64, f64),
    /// Where the hold ends and the release begins.
    close: (f64, f64),
    /// Back on the floor — shut.
    end: (f64, f64),
}

impl GateGlyph {
    fn of(gate: Gate, at: Panel, line: f64, floor: f64) -> Option<Self> {
        if at.width < 30.0 || at.height < 30.0 {
            return None;
        }
        // Three widths on their own log scales, each up to a quarter
        // of the display, so the glyph stays inside the box at every
        // setting and the run has room to be a run.
        let span = at.width * 0.25;
        let attack = log_norm(f64::from(gate.attack), 0.1, 200.0) * span;
        let hold = log_norm(f64::from(gate.hold), 1.0, 2_000.0).mul_add(span, 6.0);
        let release = log_norm(f64::from(gate.release), 5.0, 3_000.0) * span;
        let left = at.x + RAIL;
        // A table needs a leg: when the range is so small the two
        // lines nearly touch, the glyph stands a little below the
        // threshold anyway, or it would be a line on a line.
        let floor = floor.max(line + 8.0).min(at.y + at.height - 1.0);
        Some(Self {
            start: (left, floor),
            open: (left + attack, line),
            close: (left + attack + hold, line),
            end: (
                (left + attack + hold + release).min(at.x + at.width - 1.0),
                floor,
            ),
        })
    }

    const fn rise(self) -> [(f64, f64); 2] {
        [self.start, self.open]
    }

    const fn run(self) -> [(f64, f64); 2] {
        [self.open, self.close]
    }

    const fn fall(self) -> [(f64, f64); 2] {
        [self.close, self.end]
    }

    /// Which of its edges a point is nearest, if any.
    fn grip_at(self, x: f64, y: f64) -> Option<Grip> {
        [
            (
                Grip::Attack(Which::Gate),
                near_segment((x, y), self.start, self.open),
            ),
            (
                Grip::Hold(Which::Gate),
                near_segment((x, y), self.open, self.close),
            ),
            (
                Grip::Release(Which::Gate),
                near_segment((x, y), self.close, self.end),
            ),
        ]
        .into_iter()
        .filter(|(_, away)| *away <= GRAB)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(grip, _)| grip)
    }
}

/// Distance from a point to a segment — so a ramp is grabbable along
/// its whole length rather than only at its ends.
fn near_segment(point: (f64, f64), from: (f64, f64), to: (f64, f64)) -> f64 {
    let (px, py) = point;
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = dx.hypot(dy);
    if len < f64::EPSILON {
        return (px - from.0).hypot(py - from.1);
    }
    let along = ((px - from.0).mul_add(dx, (py - from.1) * dy) / (len * len)).clamp(0.0, 1.0);
    (px - along.mul_add(dx, from.0)).hypot(py - along.mul_add(dy, from.1))
}

/// How tall the lane under a gate or a de-esser is.
///
/// The door lane and the fire lane: a strip of time along the bottom
/// of the display, drawn in the live pass from the level history. Nine
/// pixels is enough to see a segment and its ramp, and little enough
/// that the display above keeps its ladder.
pub const LANE: f64 = 9.0;

/// How tall the machine selector under a delay or a reverb is.
///
/// One chip per family, the current one lit, and the machine's name
/// beside them. A row rather than a dropdown: across a mixer the
/// question is "which of these is a plate", and a row answers it
/// without a click.
pub const SELECTOR: f64 = 16.0;

/// How wide one chip of the selector is.
pub const CHIP: f64 = 14.0;

/// The part of a body a unit's main display occupies.
///
/// Three units keep a strip along the bottom for a second picture —
/// the gate's door lane, the de-esser's fire lane, the resonance
/// suppressor's comb — and only at a detailed width, because at
/// `Curves` the whole body is the shape and nothing else fits. Stated
/// once because the drawing, the live pass and the hit test all have
/// to agree where the display ends.
#[must_use]
pub const fn display_of(body: Panel, which: Which, rack: Rack) -> Panel {
    if !rack.detailed() {
        return body;
    }
    let keep = match which {
        Which::Gate => LANE + 2.0,
        // The de-esser's strip is OVER its display: the band it
        // watches, then what happened in it.
        Which::DeEss | Which::DeEssIn => BAND_STRIP + 2.0,
        Which::Sat | Which::Delay | Which::Reverb => SELECTOR + 2.0,
        _ => 0.0,
    };
    let over = matches!(which, Which::DeEss | Which::DeEssIn);
    Panel {
        y: if over { body.y + keep } else { body.y },
        height: body.height - keep,
        ..body
    }
}

/// How tall the de-esser's band strip is.
pub const BAND_STRIP: f64 = 18.0;

/// The lane under a unit's display, if it has one at this width.
#[must_use]
pub const fn lane_of(body: Panel, which: Which, rack: Rack) -> Option<Panel> {
    let display = display_of(body, which, rack);
    if display.height >= body.height {
        return None;
    }
    if matches!(which, Which::DeEss | Which::DeEssIn) {
        return Some(Panel {
            y: body.y,
            height: BAND_STRIP,
            ..body
        });
    }
    Some(Panel {
        y: display.y + display.height + 2.0,
        height: body.height - display.height - 2.0,
        ..body
    })
}

/// The de-esser: the band it watches, and what happened in it.
///
/// Two pictures. A strip along the top is the top end of the spectrum
/// — 800 Hz to the ceiling — with the band as a wash between its two
/// edges, so where it is looking is the first thing read. Under it
/// the display is TIME: the band's level as a trace, the reference
/// it is judged against dashed over it, and what came off the top
/// lit in the panel's colour where it came off — drawn in the live
/// pass from the level history (see `lanes`), because it moves with
/// the audio. It was a spectrum with a ribbon hanging off it, which
/// was a compressor's picture drawn sideways and told you neither
/// when it fired nor how hard.
fn suppress(
    scene: &mut Scene,
    palette: &Palette,
    which: Which,
    set: Suppress,
    meters: &Meters,
    at: Panel,
    rack: Rack,
    lit: Option<Grip>,
) {
    let body = at;
    let at = display_of(body, which, rack);
    let ink = DEESS_INK;

    // The ladder the trace is read against.
    if rack.detailed() {
        for db in [-12.0, -24.0] {
            let y = suppress_y(db, at);
            rule(
                scene,
                palette.grid_beat,
                Line::new((at.x, y), (at.x + at.width, y)),
            );
        }
    }

    let Some(strip) = lane_of(body, which, rack) else {
        return;
    };
    let zoom = SuppressZoom::top();
    let strip_bottom = strip.y + strip.height;

    // The top end, faint, so the band is seen against what is there.
    if meters.spectrum.len() >= 4 {
        const ACROSS: usize = 48;
        let points: Vec<(f64, f64)> = (0..ACROSS)
            .map(|i| {
                let t = crate::num::coord(i) / crate::num::coord(ACROSS - 1);
                let db = bin_at(&meters.spectrum, zoom.hz_at(t));
                let share = ((db - SUPPRESS_FLOOR_DB) / (SUPPRESS_CEIL_DB - SUPPRESS_FLOOR_DB))
                    .clamp(0.0, 1.0);
                (
                    t.mul_add(strip.width, strip.x),
                    share.mul_add(-(strip.height - 1.0), strip_bottom),
                )
            })
            .collect();
        area_under(
            scene,
            palette.text_faint.multiply_alpha(0.25),
            &points,
            strip_bottom,
        );
    }

    // The band: a wash between its edges, and the edges themselves.
    let (x_low, x_high) = (
        zoom.x_of(f64::from(set.low), strip),
        zoom.x_of(f64::from(set.high), strip),
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        ink.multiply_alpha(0.2),
        None,
        &Rect::new(x_low.min(x_high), strip.y, x_low.max(x_high), strip_bottom),
    );
    for (hz, side) in [
        (f64::from(set.low), Side::Low),
        (f64::from(set.high), Side::High),
    ] {
        let x = zoom.x_of(hz, strip);
        let held = lit == Some(Grip::Edge(which, side));
        rule_wide(
            scene,
            if held { palette.text } else { ink },
            Line::new((x, strip.y), (x, strip_bottom)),
            if held { 2.0 } else { 1.2 },
        );
    }
    marks(scene, palette, zoom, strip);
}

/// Where a level in the de-esser's band lands on its display.
fn suppress_y(db: f64, at: Panel) -> f64 {
    let t = ((db - SUPPRESS_FLOOR_DB) / (SUPPRESS_CEIL_DB - SUPPRESS_FLOOR_DB)).clamp(0.0, 1.0);
    t.mul_add(-(at.height - 2.0), at.y + at.height - 1.0)
}

/// Where you are on a zoomed axis, since it is no longer the familiar
/// one: a tick at each of [`SUPPRESS_MARKS`] that falls in the window.
fn marks(scene: &mut Scene, palette: &Palette, zoom: SuppressZoom, at: Panel) {
    let bottom = at.y + at.height;
    for (hz, _) in SUPPRESS_MARKS {
        if hz < zoom.low || hz > zoom.high {
            continue;
        }
        let x = zoom.x_of(hz, at);
        if x - 3.0 < at.x || x + 3.0 > at.x + at.width {
            continue;
        }
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette.grid,
            None,
            &Rect::new(x - 0.5, bottom - 3.0, x + 0.5, bottom),
        );
    }
}

/// The ribbon between two polylines of the same length.
fn ribbon(scene: &mut Scene, color: Color, upper: &[(f64, f64)], lower: &[(f64, f64)]) {
    let Some((x, y)) = upper.first().copied() else {
        return;
    };
    let mut taken = BezPath::new();
    taken.move_to((x, y));
    for point in upper.iter().skip(1).copied() {
        taken.line_to(point);
    }
    for point in lower.iter().rev().copied() {
        taken.line_to(point);
    }
    taken.close_path();
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &taken);
}

/// The area under a polyline, down to `floor`.
fn area_under(scene: &mut Scene, color: Color, points: &[(f64, f64)], floor: f64) {
    let (Some((x0, y0)), Some((x1, _))) = (points.first().copied(), points.last().copied()) else {
        return;
    };
    let mut area = BezPath::new();
    area.move_to((x0, floor));
    area.line_to((x0, y0));
    for point in points.iter().skip(1).copied() {
        area.line_to(point);
    }
    area.line_to((x1, floor));
    area.close_path();
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &area);
}

/// A value read out of the analyser's log-spaced bins at a frequency.
///
/// Between two bins rather than snapped to one, or a zoomed view turns
/// into a staircase.
fn bin_at(bins: &[f32], hz: f64) -> f64 {
    let last = crate::num::coord(bins.len().saturating_sub(1).max(1));
    let full = (20_000.0_f64 / 20.0).log10();
    let place = ((hz / 20.0).log10() / full).clamp(0.0, 1.0) * last;
    let low = place.floor();
    let i = crate::num::index(low);
    let a = bins.get(i).copied().unwrap_or(0.0);
    let b = bins.get(i.saturating_add(1)).copied().unwrap_or(a);
    let t = place - low;
    f64::from(a).mul_add(1.0 - t, f64::from(b) * t)
}

/// The window a suppressor's display shows: its band, opened out a
/// third of an octave each side so the shoulders are visible.
///
/// A cut you can see starting is a cut you can tell is in the right
/// place. Shared by the drawing and the hit test, because an edge you
/// grab has to be where the edge is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SuppressZoom {
    pub low: f64,
    pub high: f64,
}

impl SuppressZoom {
    /// The top end: from 800 Hz to the ceiling, where every de-esser's
    /// band is. Fixed, so the band is read as a place on the axis and
    /// the edges move along it, rather than the axis moving under them.
    #[must_use]
    pub const fn top() -> Self {
        Self {
            low: 800.0,
            high: 20_000.0,
        }
    }

    fn decade(self) -> f64 {
        (self.high / self.low).log10().max(f64::EPSILON)
    }

    /// The frequency at `t` across the window.
    #[must_use]
    pub fn hz_at(self, t: f64) -> f64 {
        self.low * 10.0_f64.powf(t * self.decade())
    }

    /// Where a frequency falls across the window, 0..1 inside it.
    #[must_use]
    pub fn place_of(self, hz: f64) -> f64 {
        (hz / self.low).log10() / self.decade()
    }

    /// The same, in a panel's pixels.
    #[must_use]
    pub fn x_of(self, hz: f64, at: Panel) -> f64 {
        self.place_of(hz).clamp(0.0, 1.0).mul_add(at.width, at.x)
    }
}

/// The least a band's two edges may close to: a fifth of an octave.
/// A band narrower than that is a notch, and a notch is the EQ's.
const SUPPRESS_SHOULDER: f64 = 1.26;

/// The frequencies a zoomed suppressor ticks, where they fall inside
/// the band it is showing.
const SUPPRESS_MARKS: [(f64, &str); 6] = [
    (200.0, "200"),
    (1_000.0, "1k"),
    (2_000.0, "2k"),
    (5_000.0, "5k"),
    (10_000.0, "10k"),
    (15_000.0, "15k"),
];

/// The window a suppressor draws its spectrum in.
///
/// The analyser's own range — see `simulate::spectrum`, which clamps to
/// these — so a bin lands where the number says rather than where a
/// second opinion about the scale puts it.
const SUPPRESS_FLOOR_DB: f64 = -40.0;
const SUPPRESS_CEIL_DB: f64 = 16.0;

/// Which edge of a band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Low,
    High,
}

/// The delay: its repeats, on a line — drawn the way its machine makes
/// them.
///
/// The substrate is `delay-ui`'s own centrepiece idea at strip size:
/// time runs left to right over a window wide enough to hold several
/// taps, each one shorter than the last by the feedback, and the dry
/// hit stands at the origin so the first gap IS the delay time. What
/// the FAMILY changes is the shape of one repeat, exactly as the
/// plugin's faces do: a digital delay's are identical bars, a tape's
/// round off and darken, a chip's smear wider each pass, a pitch
/// delay's climb, a rhythmic one's land on a grid, a reversed one's
/// swell into the tap. You should know which delay you are looking at
/// before you read a word.
///
/// Live, the repeats are lit by the wet return: a phrase decaying
/// through the taps brightens each in turn, so the panel plays the
/// delay rather than describing it.
fn echo(
    scene: &mut Scene,
    palette: &Palette,
    echo: Echo,
    meters: &Meters,
    at: Panel,
    rack: Rack,
    lit: Option<Grip>,
) {
    let bottom = at.y + at.height;
    let floor = bottom - 1.0;
    let family = echo.family();
    if rack.detailed() {
        rule(
            scene,
            palette.grid,
            Line::new((at.x, floor), (at.x + at.width, floor)),
        );
        // A rhythmic delay lands on a grid, so the grid is drawn: a
        // beat every delay time, a bar every four.
        if family == DelayFamily::Rhythmic {
            let taps = echo_taps(echo, at);
            for (k, (x, _)) in taps.iter().enumerate() {
                let bar = k % 4 == 0;
                rule(
                    scene,
                    if bar { palette.grid_beat } else { palette.grid },
                    Line::new((*x, at.y), (*x, floor)),
                );
            }
        }
    }
    let wet = DELAY_INK;
    let time_held = lit == Some(Grip::Time);
    let feedback_held = lit == Some(Grip::Feedback);
    let mix = f64::from(echo.mix).clamp(0.0, 1.0);
    let feedback = f64::from(echo.feedback).clamp(0.0, 0.99);
    let mut previous_x = at.x + 3.0;
    for (k, (x, level)) in echo_taps(echo, at).into_iter().enumerate() {
        // The dry hit, then the repeats at the level the feedback
        // leaves them. The mix decides how strongly they are INKED
        // rather than how tall they are: a quiet delay is still a delay
        // with those repeats at those times, and shrinking them would
        // confuse the two settings.
        let height = at.height * 0.9 * level;
        let dry = k == 0;
        // Lit by the signal: the dry hit by the input, each repeat by
        // the wet return that has had k−1 more passes of feedback.
        let glow = if meters.is_empty() {
            0.0
        } else if dry {
            f64::from(meters.sat_peak).clamp(0.0, 1.0)
        } else {
            (f64::from(meters.delay_wet)
                * feedback.powi(i32::try_from(k).unwrap_or(1).saturating_sub(1))
                * 3.0)
                .clamp(0.0, 1.0)
        };
        let base = if dry { 1.0 } else { 0.35 + mix * 0.65 };
        let alpha = crate::mcp::f64_to_f32((base * glow.mul_add(0.3, 0.7)).clamp(0.0, 1.0));
        let ink = if dry {
            palette.text.multiply_alpha(alpha)
        } else {
            wet.multiply_alpha(alpha)
        };
        let held = (dry && time_held) || (!dry && feedback_held) || (k == 1 && time_held);
        let width = if dry || held { 2.0 } else { 1.4 };
        match family {
            _ if dry => rule_wide(
                scene,
                ink,
                Line::new((x, floor), (x, floor - height)),
                width,
            ),
            DelayFamily::Digital | DelayFamily::Rhythmic => {
                rule_wide(
                    scene,
                    ink,
                    Line::new((x, floor), (x, floor - height)),
                    width,
                );
            }
            // Tape: the top rounds off and each pass is darker — the
            // head loses treble every time round.
            DelayFamily::Tape => {
                let dull = crate::mcp::f64_to_f32(
                    crate::num::coord(k).mul_add(-0.12, 1.0).clamp(0.3, 1.0),
                );
                let mut path = BezPath::new();
                path.move_to((x, floor));
                path.line_to((x, floor - height + 2.0));
                path.quad_to((x, floor - height), (x + 2.0, floor - height));
                scene.stroke(
                    &Stroke::new(width).with_caps(vello::kurbo::Cap::Round),
                    Affine::IDENTITY,
                    ink.multiply_alpha(dull),
                    None,
                    &path,
                );
            }
            // A chip: each repeat is wider than the last — the bucket
            // line smears.
            DelayFamily::Analog => {
                let half = crate::num::coord(k).mul_add(0.55, 0.6);
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    ink,
                    None,
                    &Rect::new(x - half, floor - height, x + half, floor),
                );
            }
            // Pitch: each repeat's foot rises by the interval.
            DelayFamily::Pitch => {
                let rise = 3.2 * crate::num::coord(k);
                rule_wide(
                    scene,
                    ink,
                    Line::new((x, floor - rise), (x, floor - rise - height)),
                    width,
                );
                rule(
                    scene,
                    palette.grid_beat,
                    Line::new((x - 2.0, floor - rise), (x + 2.0, floor - rise)),
                );
            }
            // Special: a reversed repeat swells INTO the tap; the rest
            // are a repeat that is no longer one — a soft blob whose
            // height is its level.
            DelayFamily::Special => {
                if echo.style == DelayStyle::Reverse {
                    let mut wedge = BezPath::new();
                    wedge.move_to((previous_x + 2.0, floor));
                    wedge.line_to((x, floor));
                    wedge.line_to((x, floor - height));
                    wedge.close_path();
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        ink.multiply_alpha(0.8),
                        None,
                        &wedge,
                    );
                } else {
                    let rx = crate::num::coord(k).mul_add(0.5, 1.6);
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        ink.multiply_alpha(0.7),
                        None,
                        &vello::kurbo::Ellipse::new(
                            (x, floor - height / 2.0),
                            (rx, height / 2.0),
                            0.0,
                        ),
                    );
                }
            }
        }
        previous_x = x;
    }
}

/// Where the delay's taps land, and how tall each is (0..1).
///
/// The dry hit first, then the repeats at the level the feedback
/// leaves them. Shared by the drawing and the hit test: a tap you grab
/// has to be where the tap is drawn.
#[must_use]
pub fn echo_taps(echo: Echo, at: Panel) -> Vec<(f64, f64)> {
    let left = at.x + 3.0;
    // A window of four and a half taps at the current time, so the
    // picture keeps its shape as the time changes rather than the taps
    // marching off the end. What moves with the time is the SPACING,
    // which is the thing the number means.
    let time = f64::from(echo.time).max(1.0);
    let window = time * 4.5;
    let feedback = f64::from(echo.feedback).clamp(0.0, 0.99);
    let rhythmic = echo.family() == DelayFamily::Rhythmic;
    let mut out = Vec::with_capacity(16);
    let mut level = 1.0_f64;
    let mut when = 0.0_f64;
    for tap in 0..16_usize {
        let x = (when / window).mul_add(at.width - 6.0, left);
        if x > at.x + at.width || level < 0.03 {
            break;
        }
        out.push((x, level));
        let step = if rhythmic {
            PATTERN
                .get(tap.rem_euclid(PATTERN.len()))
                .copied()
                .unwrap_or(1.0)
        } else {
            1.0
        };
        when += time * step;
        level *= feedback;
    }
    out
}

/// Where a rhythmic delay's taps land: on subdivisions rather than
/// evenly, in delay times.
const PATTERN: [f64; 5] = [1.0, 0.5, 0.75, 0.5, 1.25];

/// The reverb: an impulse, the gap before its tail, and the tail — the
/// algorithm's own.
///
/// `reverb-ui`'s third centrepiece — "the recorded thing itself: an
/// impulse and its decay envelope" — which is the only picture of a
/// reverb that survives being a hundred and thirty pixels wide. But
/// the envelope is not an exponential drawn from the decay knob: it is
/// the algorithm's impulse response, rendered by the plugin's own
/// probe (see [`crate::live::tail`]). That is what makes a plate and a
/// spring with the same decay different pictures — a plate is dense
/// from the first millisecond, a spring drips, a bloom rises before it
/// falls — because they are different sounds.
///
/// While the render is on its way the exponential stands in, dashed:
/// a guess that looked like a measurement would be believed.
fn room(
    scene: &mut Scene,
    palette: &Palette,
    room: Room,
    meters: &Meters,
    at: Panel,
    rack: Rack,
    lit: Option<Grip>,
) {
    let bottom = at.y + at.height;
    let floor = bottom - 1.0;
    if rack.detailed() {
        rule(
            scene,
            palette.grid,
            Line::new((at.x, floor), (at.x + at.width, floor)),
        );
    }
    let ink = REVERB_INK;
    let mix = f64::from(room.mix).clamp(0.0, 1.0);
    let geometry = RoomGeometry::of(room, at);

    // The dry impulse.
    rule_wide(
        scene,
        palette.text,
        Line::new((at.x, floor), (at.x, at.height.mul_add(-0.92, floor))),
        2.0,
    );

    // The tail: the rendered envelope, dB below its peak, across the
    // decay window. The mix scales its height — a quiet reverb is a
    // low tail — but never below a quarter, or a reverb at 5% would
    // be a floor line and nothing else.
    let tail = crate::live::tail(room.key());
    let scale = mix.mul_add(0.75, 0.25) * at.height * 0.92;
    let points: Vec<(f64, f64)> = tail
        .envelope
        .iter()
        .enumerate()
        .map(|(i, db)| {
            let t =
                crate::num::coord(i) / crate::num::coord(crate::live::TAIL_BINS.saturating_sub(1));
            let level = (1.0 + f64::from(*db) / 60.0).clamp(0.0, 1.0);
            (
                geometry.start + t * (geometry.end - geometry.start),
                floor - level * scale,
            )
        })
        .collect();
    // Lit by the wet return: a reverb doing nothing is an outline, one
    // washing a chorus is a filled shape.
    if !meters.is_empty()
        && let (Some(first), Some(last)) = (points.first().copied(), points.last().copied())
    {
        let wet = f64::from(meters.reverb_wet).clamp(0.0, 1.0);
        let mut area = BezPath::new();
        area.move_to((first.0, floor));
        for point in points.iter().copied() {
            area.line_to(point);
        }
        area.line_to((last.0, floor));
        area.close_path();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            ink.multiply_alpha(crate::mcp::f64_to_f32(wet.mul_add(0.45, 0.08))),
            None,
            &area,
        );
    }
    let held = matches!(lit, Some(Grip::Decay | Grip::Mix(Which::Reverb)));
    let width = if held { 2.0 } else { 1.4 };
    if tail.exact {
        curve(scene, ink, points.into_iter(), width);
    } else {
        dashed(scene, ink, points.into_iter(), width);
    }
    if rack.detailed() {
        let held = lit == Some(Grip::Predelay);
        rule_wide(
            scene,
            if held {
                palette.text
            } else {
                palette.grid_beat
            },
            Line::new((geometry.start, at.y), (geometry.start, floor)),
            if held { 1.8 } else { 1.0 },
        );
    }
}

/// Where the reverb's tail begins and ends in a panel.
///
/// The window is the decay, so a long reverb fills the panel and a
/// short one does not reach the end — which is the comparison you want
/// across a mixer. Shared by the drawing and the hit test.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoomGeometry {
    /// Where the tail starts: the predelay, along the decay window.
    pub start: f64,
    /// The panel's right edge — the decay reaches it by definition.
    pub end: f64,
}

impl RoomGeometry {
    #[must_use]
    pub fn of(room: Room, at: Panel) -> Self {
        let window = (f64::from(room.decay) * 1000.0).max(1.0);
        let start = (f64::from(room.predelay) / window)
            .clamp(0.0, 0.5)
            .mul_add(at.width, at.x);
        Self {
            start,
            end: at.x + at.width,
        }
    }
}

/// The saturator: its curve, where the signal is on it, and what it
/// is adding.
///
/// A transfer curve alone says drive and nothing else, because every
/// saturator is an S. The model underneath has the two things that
/// separate a valve from a transistor — asymmetry, and the harmonics
/// that produces — and the plugin measures the second with a probe. So
/// the panel answers the three questions a mixer asks of a saturator:
///
/// - **Is it happening?** The curve is lit from the origin out to the
///   signal's peak this frame. A track well under the knee lights the
///   straight part; a slammed one lights the bend.
/// - **What is it adding?** A ladder of seven bars, H2 through H8, from
///   the probe — evens in the phase's yellow, odds in grey. A biased
///   triode draws a yellow ladder, a symmetric rail a grey one, and you
///   can tell them apart from across the room. It breathes with the
///   signal: the probe runs at three levels and the live peak reads
///   between them.
/// - **Which circuit?** The glyph beside the name, from the plugin's
///   own classification.
fn sat(
    scene: &mut Scene,
    palette: &Palette,
    tone: &Tone,
    meters: &Meters,
    at: Panel,
    rack: Rack,
    lit: Option<Grip>,
) {
    let pre = &tone.sat;
    let whole = at;
    let (at, ladder_box) = sat_split(at, rack);
    let right = at.x + at.width;
    let bottom = at.y + at.height;
    let mid_y = at.y + at.height / 2.0;
    let mid_x = at.x + at.width / 2.0;

    // How hard it is saturating: what the stage is adding at the level
    // the signal is at, as the ladder measures it — the sum of the
    // rungs, on a square root so the first decibel of colour shows.
    // With nothing playing it is the full-scale ladder, so a recorded
    // rack still says how hot the setting is.
    let ladder = crate::live::ladder(pre);
    let rungs = if meters.is_empty() {
        ladder.full()
    } else {
        let db = 20.0 * f64::from(meters.sat_peak.max(1e-4)).log10();
        ladder.at(crate::mcp::f64_to_f32(db))
    };
    let heat = (f64::from(rungs.iter().sum::<f32>()) / 0.9)
        .clamp(0.0, 1.0)
        .powf(0.7);
    glow(scene, whole, (mid_x, mid_y), heat);

    if rack.detailed() {
        rule(
            scene,
            palette.grid,
            Line::new((at.x, mid_y), (right, mid_y)),
        );
        rule(
            scene,
            palette.grid,
            Line::new((mid_x, at.y), (mid_x, bottom)),
        );
        // Unity, so the curve's departure from it IS the saturation.
        // Without it a gentle drive and a hard one are both "an S", and
        // the thing you are looking for is how far from straight it
        // has gone.
        rule(
            scene,
            palette.grid_beat,
            Line::new((at.x, bottom), (right, at.y)),
        );
    }

    let mut samples = [(0.0_f32, 0.0_f32); SAMPLES];
    saturate_dsp::preamp::analysis::transfer_curve(pre, &mut samples);
    // A quantiser has no transfer curve of its own — there is no curve
    // that produces an alias — but it does one thing to this one: it
    // turns it into a staircase, and that is the honest picture of it.
    if !tone.sat_digital.is_transparent() {
        let levels = (f64::from(tone.sat_digital.bits.clamp(1.0, 16.0)) - 1.0).exp2();
        for (_, y) in &mut samples {
            *y = crate::mcp::f64_to_f32((f64::from(*y) * levels).round() / levels);
        }
    }
    let place = |(x, y): &(f32, f32)| {
        (
            f64::midpoint(f64::from(*x), 1.0).mul_add(at.width, at.x),
            (mid_y - f64::from(*y) * at.height / 2.0).clamp(at.y, bottom),
        )
    };
    let held = matches!(lit, Some(Grip::Drive | Grip::Bias));
    curve(
        scene,
        SAT_INK,
        samples.iter().map(place),
        if held { 2.2 } else { 1.5 },
    );

    // The lit reach: from the origin out to the signal's peak, both
    // ways. One stroke, and the whole "is it doing anything" answer.
    if !meters.is_empty() {
        let peak = f64::from(meters.sat_peak).clamp(0.0, 1.0);
        let tint = SAT_HOT;
        let reach: Vec<(f64, f64)> = samples
            .iter()
            .filter(|(x, _)| f64::from(x.abs()) <= peak)
            .map(place)
            .collect();
        if reach.len() >= 2 {
            if let (Some(a), Some(b)) = (reach.first().copied(), reach.last().copied()) {
                dot(scene, tint, a, 1.8);
                dot(scene, tint, b, 1.8);
            }
            curve(scene, tint, reach.into_iter(), 2.4);
        }
    }

    // The ladder.
    if let Some(lb) = ladder_box {
        let floor = lb.y + lb.height - 1.0;
        rule(
            scene,
            palette.grid,
            Line::new((lb.x, floor), (lb.x + lb.width, floor)),
        );
        let tint = SAT_INK;
        let n = crate::num::coord(crate::live::RUNGS);
        let gap = 1.2;
        let bar = ((lb.width - 2.0 - gap * (n - 1.0)) / n).max(1.0);
        let tall = lb.height - 4.0;
        for (k, rung) in rungs.iter().enumerate() {
            // Rung k is harmonic k+2, so the evens are the even k. Drawn
            // on a square root so the fourth and sixth are visible
            // beside a second that dwarfs them; the header prints the
            // share as a number.
            let even = k % 2 == 0;
            let h = f64::from(rung.clamp(0.0, 1.0)).sqrt() * tall;
            let x = crate::num::coord(k).mul_add(bar + gap, lb.x + 1.0);
            let ink = if even {
                tint
            } else {
                palette.text_faint.multiply_alpha(0.75)
            };
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                ink,
                None,
                &Rect::new(x, floor - h, x + bar, floor),
            );
        }
        // The tilt, as a wedge under the ladder: which end of the
        // spectrum meets the knee first.
        if rack.detailed() {
            let lean = (f64::from(pre.tilt_db()) / 12.0).clamp(-1.0, 1.0);
            let held = lit == Some(Grip::Tilt);
            rule_wide(
                scene,
                if held {
                    palette.text
                } else {
                    palette.text_faint
                },
                Line::new(
                    (lb.x + 2.0, lean.mul_add(1.5, floor + 3.0)),
                    (lb.x + lb.width - 2.0, lean.mul_add(-1.5, floor + 3.0)),
                ),
                if held { 1.6 } else { 0.9 },
            );
        }
    }
}

/// The saturator's ink: red. Every other unit takes the strip's own
/// colours; this one is the one that makes heat, and it says so.
const SAT_INK: Color = Color::from_rgba8(0xff, 0x4a, 0x3d, 0xff);

/// The lit reach's ink — hotter than the curve, toward white.
const SAT_HOT: Color = Color::from_rgba8(0xff, 0xb3, 0x47, 0xff);

/// The glow's ink — the red of a stage that is working.
const SAT_GLOW: Color = Color::from_rgba8(0xff, 0x2e, 0x0c, 0xff);

/// The saturator's ground, lit from behind by how hard it is working.
///
/// A wash over the panel and a radial glow from the curve's centre,
/// both scaled by `heat` (0..1): a stage adding nothing sits on the
/// rack's own ground, and one at the top of its drive glows like the
/// valve it is modelling. Heat is raised to 0.7 so unity stays cool
/// and the top of the range is where the fire is. One gradient fill — cheap, and recorded with
/// the rest of the panel since heat is a fact about the settings and
/// the level, both of which the rack already rebuilds on.
fn glow(scene: &mut Scene, at: Panel, centre: (f64, f64), heat: f64) {
    if heat <= 0.02 {
        return;
    }
    let alpha = |share: f64| crate::mcp::f64_to_f32((share * heat).clamp(0.0, 1.0));
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        SAT_GLOW.multiply_alpha(alpha(0.22)),
        None,
        &at.rect(),
    );
    let radius = crate::mcp::f64_to_f32(at.width.max(at.height) * 0.7);
    let paint = anyrender::Paint::Gradient(
        vello::peniko::Gradient::new_radial((centre.0, centre.1), radius).with_stops([
            (0.0, SAT_HOT.multiply_alpha(alpha(0.7))),
            (0.35, SAT_GLOW.multiply_alpha(alpha(0.55))),
            (1.0, SAT_GLOW.multiply_alpha(0.0)),
        ]),
    );
    scene.fill(Fill::NonZero, Affine::IDENTITY, &paint, None, &at.rect());
}

/// The saturator's body: the curve's square on the left, the ladder's
/// column on the right — or the whole body for the curve when there
/// is no room for a ladder anyone could read.
#[must_use]
pub const fn sat_split(body: Panel, rack: Rack) -> (Panel, Option<Panel>) {
    if !rack.detailed() || body.width < 80.0 {
        return (body, None);
    }
    let ladder_w = body.width * 0.34;
    let curve = Panel {
        width: body.width - ladder_w - 4.0,
        ..body
    };
    let ladder = Panel {
        x: body.x + body.width - ladder_w,
        width: ladder_w,
        height: body.height - 6.0,
        ..body
    };
    (curve, Some(ladder))
}

/// An open polyline, dashed — for a picture that is a guess rather
/// than a measurement.
fn dashed(scene: &mut Scene, color: Color, points: impl Iterator<Item = (f64, f64)>, width: f64) {
    let mut path = BezPath::new();
    for (i, point) in points.enumerate() {
        if i == 0 {
            path.move_to(point);
        } else {
            path.line_to(point);
        }
    }
    if path.is_empty() {
        return;
    }
    scene.stroke(
        &Stroke::new(width).with_dashes(0.0, [2.5, 2.0]),
        Affine::IDENTITY,
        color,
        None,
        &path,
    );
}

/// The machine a unit is, in eight pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Circuit(Circuit),
    Delay(DelayFamily),
    Room(RoomFamily),
}

/// How wide a header glyph is, with its gap to the name.
pub const GLYPH_W: f64 = 11.0;

/// How wide the expand mark at a header's right end is ([`Grip::Zoom`]).
const EXPAND_W: f64 = 10.0;

/// The expand mark: two corners pointing out, at the right end of a
/// panel's header `head`.
fn expand_mark(scene: &mut Scene, ink: Color, head: Panel) {
    use vello::kurbo::{BezPath, Cap, Join, Stroke};
    let side = (head.height - 3.0).clamp(5.0, 8.0);
    let x1 = head.x + head.width - 1.0;
    let x0 = x1 - side;
    let y0 = head.y + (head.height - side) / 2.0;
    let y1 = y0 + side;
    let arm = side * 0.45;
    let mut path = BezPath::new();
    // Top right corner, and bottom left: out to the edges.
    path.move_to((x1 - arm, y0));
    path.line_to((x1, y0));
    path.line_to((x1, y0 + arm));
    path.move_to((x0, y1 - arm));
    path.line_to((x0, y1));
    path.line_to((x0 + arm, y1));
    let line = Stroke::new(1.1)
        .with_caps(Cap::Round)
        .with_join(Join::Round);
    scene.stroke(&line, Affine::IDENTITY, ink, None, &path);
}

/// Flip a rack switch — the grips that are not values — on `tone`. A
/// fold and a close-up are the caller's: they change what is shown,
/// not the track.
pub fn toggle(tone: &mut Tone, grip: Grip) {
    match grip {
        Grip::Bypass(which) => tone.bypass.toggle(which),
        // Clicking the zoom steps it on; the wheel walks it either way.
        Grip::Scale(_) => tone.cycle_eq_range(),
        // The machine glyph cycles the machine.
        Grip::Family(Which::Sat) => tone.cycle_sat(),
        Grip::Family(Which::Delay) => tone.delay.cycle_style(),
        Grip::Family(Which::Reverb) => tone.reverb.cycle_algorithm(),
        // A chip in the selector strip picks its family.
        Grip::Choose(Which::Sat, i) => tone.choose_sat_family(i),
        Grip::Choose(Which::Delay, i) => tone.delay.choose_family(i),
        Grip::Choose(Which::Reverb, i) => tone.reverb.choose_family(i),
        // A preset chip loads the preset: the whole rack follows.
        Grip::Preset(i) => tone.load_preset(i),
        _ => {}
    }
}

/// Draw a glyph with its left edge at `x`, sitting on `baseline`.
///
/// Paths, never text: a glyph is drawn into every header of every
/// strip, and a glyph run is the most expensive thing the renderer
/// draws. Each is the plugin's own face at the smallest size it still
/// reads: a valve, reels, a chip, an arch, a coil.
fn glyph(scene: &mut Scene, ink: Color, glyph: Glyph, x: f64, baseline: f64) {
    match glyph {
        Glyph::Circuit(circuit) => circuit_glyph(scene, ink, circuit, x, baseline),
        Glyph::Delay(family) => delay_glyph(scene, ink, family, x, baseline),
        Glyph::Room(family) => room_glyph(scene, ink, family, x, baseline),
    }
}

/// The saturator's five circuits, in the plugin's own rail order —
/// which is `saturate_profiles::CATEGORIES`' order, chip for chip.
const CIRCUITS: [Circuit; 5] = [
    Circuit::Valve,
    Circuit::Tape,
    Circuit::Core,
    Circuit::Solid,
    Circuit::Steps,
];

/// The machine selector: one chip per family, the current family lit,
/// and the machine's name beside them.
///
/// The chips are the family glyphs — the same drawings as the header's
/// — so a row of them reads as the set of machines this unit can be,
/// and the lit one as which it is. A family with several machines
/// (three halls, two springs) is walked by clicking its chip again;
/// the name says which member you are on.
fn selector(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    tone: &Tone,
    which: Which,
    at: Panel,
    lit: Option<Grip>,
) {
    const SIZE: f32 = 6.5;
    let (chips, current, name): (Vec<Glyph>, usize, &str) = match which {
        Which::Sat => {
            let profile = saturate_profiles::PROFILES.get(tone.sat_profile);
            (
                CIRCUITS.iter().map(|c| Glyph::Circuit(*c)).collect(),
                profile
                    .and_then(|p| saturate_profiles::category_of(p.id))
                    .map_or(0, |(category, _)| category),
                profile.map_or("", |p| p.name),
            )
        }
        Which::Delay => (
            DelayFamily::ALL.iter().map(|f| Glyph::Delay(*f)).collect(),
            DelayFamily::ALL
                .iter()
                .position(|f| *f == tone.delay.family())
                .unwrap_or(0),
            tone.delay.style.label(),
        ),
        _ => (
            RoomFamily::ALL.iter().map(|f| Glyph::Room(*f)).collect(),
            RoomFamily::ALL
                .iter()
                .position(|f| *f == tone.reverb.family())
                .unwrap_or(0),
            tone.reverb.algorithm.name(),
        ),
    };
    let baseline = at.y + at.height - 4.0;
    let tint = unit_ink(which).unwrap_or_else(|| phase_tint(which.phase()));
    for (i, chip) in chips.iter().enumerate() {
        let x = crate::num::coord(i).mul_add(CHIP, at.x + 2.0);
        let is_current = i == current;
        let hovered = lit == Some(Grip::Choose(which, i));
        let ink = if is_current {
            palette.text
        } else if hovered {
            palette.text_dim
        } else {
            palette.text_faint.multiply_alpha(0.6)
        };
        glyph(scene, ink, *chip, x, baseline);
        // The current family is underlined in the phase's colour: a
        // lit glyph among dim ones says "this one", the bar says it in
        // the colour the rail uses for this pass.
        if is_current {
            rule_wide(
                scene,
                tint,
                Line::new(
                    (x - 1.0, at.y + at.height - 1.0),
                    (x + 9.0, at.y + at.height - 1.0),
                ),
                1.5,
            );
        }
    }
    let chips_w = crate::num::coord(chips.len()).mul_add(CHIP, 2.0);
    let name_w = font.width(name, SIZE);
    if chips_w + name_w + 4.0 <= at.width {
        crate::tcp::glyphs(
            scene,
            font,
            palette.text_dim,
            name,
            at.x + at.width - name_w,
            baseline,
            SIZE,
        );
    }
}

/// The EQ's total response, in the plugin painter's own gold
/// (`eq_graph_painter::paint_combined_curve`), so the curve on a rail
/// and the fallback at the curves tier are the colour the full graph
/// draws it in.
const EQ_INK: Color = Color::from_rgb8(212, 169, 50);

/// The de-esser's ink: a pink. Not the EQ's gold — the two share a
/// panel shape, and a yellow curve over a spectrum read as the EQ's
/// total — and not the compressor's red, the saturator's orange, the
/// gate's green or the returns' blue and violet. Sibilance is a
/// bright, sharp thing; so is the colour.
const DEESS_INK: Color = Color::from_rgba8(0xf4, 0x72, 0xb6, 0xff);

/// The delay's colour: blue. The machine, its repeats, its knobs and
/// its selector, and the delay tracks in the template — one colour
/// for the one thing.
pub const DELAY_INK: Color = Color::from_rgba8(0x3b, 0x82, 0xf6, 0xff);

/// The reverb's colour: purple, likewise.
pub const REVERB_INK: Color = Color::from_rgba8(0x8b, 0x5c, 0xf6, 0xff);

/// The decay-rate EQ's colour: a sky blue — never the EQ's own look, so a
/// graph of time is not read as a graph of gain, and not the delay's
/// blue either.
const DECAY_INK: Color = Color::from_rgba8(0x38, 0xbd, 0xf8, 0xff);

/// The colour a unit is drawn in where it has one of its own.
const fn unit_ink(which: Which) -> Option<Color> {
    match which {
        Which::Sat => Some(SAT_INK),
        Which::Delay => Some(DELAY_INK),
        Which::Reverb => Some(REVERB_INK),
        _ => None,
    }
}

/// The preset row: one chip per preset, brighter to darker, the loaded
/// one lit.
///
/// Each chip carries a swatch that runs from warm white to deep blue
/// down the row — a template's rows run bright and close to dark and
/// far, and the swatch says where a chip sits on that run before you
/// read its name.
fn presets(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    tone: &Tone,
    at: Panel,
    lit: Option<Grip>,
) {
    const SIZE: f32 = 6.5;
    if tone.presets.is_empty() {
        return;
    }
    let count = crate::num::coord(tone.presets.len().max(2).saturating_sub(1));
    for (i, chip) in preset_chips(tone, at) {
        let Some(preset) = tone.presets.get(i) else {
            continue;
        };
        let (left, w) = (chip.x0, chip.width());
        let current = tone.preset == Some(i);
        let hovered = lit == Some(Grip::Preset(i));
        let t = crate::num::coord(i) / count;
        let swatch = swatch_at(t);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            if current {
                palette.tcp_meter_well.multiply_alpha(1.0)
            } else {
                palette.tcp_meter_well.multiply_alpha(0.5)
            },
            None,
            &chip.to_rounded_rect(2.0),
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            swatch,
            None,
            &Rect::new(left + 2.0, at.y + 4.0, left + 8.0, at.y + at.height - 4.0)
                .to_rounded_rect(1.0),
        );
        let ink = if current {
            palette.text
        } else if hovered {
            palette.text_dim
        } else {
            palette.text_faint
        };
        crate::tcp::glyphs(
            scene,
            font,
            ink,
            &preset.name,
            left + 11.0,
            at.y + at.height - 5.0,
            SIZE,
        );
        if current {
            rule_wide(
                scene,
                swatch,
                Line::new(
                    (left + 1.0, at.y + at.height - 1.0),
                    (left + w - 1.0, at.y + at.height - 1.0),
                ),
                1.5,
            );
        }
    }
}

/// Where a chip sits on the bright-to-dark run, as a colour.
fn swatch_at(t: f64) -> Color {
    let bright = (0xff_u8, 0xe6_u8, 0xa0_u8);
    let dark = (0x3b_u8, 0x4a_u8, 0x8a_u8);
    let mix = |a: u8, b: u8| {
        let v = f64::from(a).mul_add(1.0 - t, f64::from(b) * t);
        crate::num::index(v.round()).min(255)
    };
    Color::from_rgba8(
        u8::try_from(mix(bright.0, dark.0)).unwrap_or(0),
        u8::try_from(mix(bright.1, dark.1)).unwrap_or(0),
        u8::try_from(mix(bright.2, dark.2)).unwrap_or(0),
        0xff,
    )
}

/// A row of the effect's knobs.
///
/// Five, evenly across, each an arc from seven o'clock to five with
/// the travel filled in the phase's colour and a pointer on the value.
/// Legends under them at a detailed width. What the plugin's face has
/// under its centrepiece, at strip size.
fn knobs(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    tone: &Tone,
    at: Panel,
    rack: Rack,
    lit: Option<Grip>,
) {
    const SIZE: f32 = 5.5;
    let labels = knob_labels(tone.role);
    let each = at.width / crate::num::coord(KNOBS);
    let r = (each * 0.32).min(at.height * 0.32).max(4.0);
    let tint = match tone.role {
        Role::Reverb => REVERB_INK,
        Role::Delay => DELAY_INK,
        _ => phase_tint(Which::Knobs.phase()),
    };
    for (i, label) in labels.iter().enumerate() {
        let cx = crate::num::coord(i).mul_add(each, at.x + each / 2.0);
        let cy = at.y + r + 3.0;
        let value = knob_value(tone, Which::Knobs, i);
        let held = lit == Some(Grip::Knob(Which::Knobs, i));
        knob_arc(scene, palette, (cx, cy), r, value, tint, held);
        if rack.detailed() {
            let w = font.width(label, SIZE);
            crate::tcp::glyphs(
                scene,
                font,
                palette.text_faint,
                label,
                cx - w / 2.0,
                at.y + at.height - 2.0,
                SIZE,
            );
        }
    }
}

/// One knob: its track, its travel, its pointer. The vocal leveler's
/// stage dials are these too.
pub(crate) fn knob_arc(
    scene: &mut impl PaintScene,
    palette: &Palette,
    centre: (f64, f64),
    r: f64,
    value: f64,
    tint: Color,
    held: bool,
) {
    use std::f64::consts::PI;
    // Seven o'clock to five o'clock, clockwise: 270 degrees of travel.
    let start = PI * 0.75;
    let sweep = PI * 1.5;
    let arc = |from: f64, to: f64| vello::kurbo::Arc::new(centre, (r, r), from, to - from, 0.0);
    scene.stroke(
        &Stroke::new(if held { 2.4 } else { 1.6 }),
        Affine::IDENTITY,
        palette.grid_beat,
        None,
        &arc(start, start + sweep),
    );
    let to = value.clamp(0.0, 1.0).mul_add(sweep, start);
    if value > 0.005 {
        scene.stroke(
            &Stroke::new(if held { 2.4 } else { 1.6 }),
            Affine::IDENTITY,
            tint,
            None,
            &arc(start, to),
        );
    }
    let (px, py) = (to.cos().mul_add(r, centre.0), to.sin().mul_add(r, centre.1));
    scene.stroke(
        &Stroke::new(1.2),
        Affine::IDENTITY,
        if held { palette.text } else { palette.text_dim },
        None,
        &Line::new(centre, (px, py)),
    );
}

/// The widener: the stereo field as a fan, as wide as the setting.
///
/// A semicircle is the whole field; the filled wedge is how much of it
/// the track occupies — a sliver for mono, the speakers at unity, and
/// past them when pushed. The one picture of width that survives a
/// strip.
fn wide(
    scene: &mut Scene,
    palette: &Palette,
    tone: &Tone,
    at: Panel,
    rack: Rack,
    lit: Option<Grip>,
) {
    use std::f64::consts::PI;
    let centre = (at.x + at.width / 2.0, at.y + at.height - 4.0);
    let r = (at.width / 2.0 - 4.0).min(at.height - 8.0).max(6.0);
    let tint = phase_tint(Which::Wide.phase());
    let field = vello::kurbo::Arc::new(centre, (r, r), PI, PI, 0.0);
    scene.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        palette.grid_beat,
        None,
        &field,
    );
    if rack.detailed() {
        // The speakers, at unity.
        for side in [-1.0_f64, 1.0] {
            let angle = PI * side.mul_add(0.25, 0.5);
            let (x, y) = (
                (-angle.cos()).mul_add(r, centre.0),
                (-angle.sin()).mul_add(r, centre.1),
            );
            rule(scene, palette.grid_beat, Line::new(centre, (x, y)));
        }
    }
    // The wedge: half the field per unit of width.
    let half = (f64::from(tone.wide).clamp(0.0, 2.0) * PI / 4.0).min(PI / 2.0);
    let held = lit == Some(Grip::Knob(Which::Wide, 0));
    let mut wedge = BezPath::new();
    wedge.move_to(centre);
    let steps = 24;
    for k in 0..=steps {
        let t = crate::num::coord(k) / crate::num::coord(steps);
        let angle = (t * 2.0).mul_add(half, -PI / 2.0 - half);
        wedge.line_to((
            angle.cos().mul_add(r, centre.0),
            angle.sin().mul_add(r, centre.1),
        ));
    }
    wedge.close_path();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        tint.multiply_alpha(if held { 0.55 } else { 0.35 }),
        None,
        &wedge,
    );
    scene.stroke(
        &Stroke::new(if held { 2.0 } else { 1.2 }),
        Affine::IDENTITY,
        tint,
        None,
        &wedge,
    );
}

/// The pitch shifter: the note, and where it goes.
///
/// Two bars — the input at unity and the shifted copy raised or
/// lowered by the interval — and the interval as a number, because an
/// octave is an octave and a bar's height is not a thing anyone can
/// read to the semitone.
fn pitch(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    tone: &Tone,
    at: Panel,
    rack: Rack,
    lit: Option<Grip>,
) {
    let tint = phase_tint(Which::Pitch.phase());
    let floor = at.y + at.height - 2.0;
    let mid = at.y + at.height / 2.0;
    rule(
        scene,
        palette.grid_beat,
        Line::new((at.x, mid), (at.x + at.width, mid)),
    );
    let held = matches!(lit, Some(Grip::Knob(Which::Pitch, _)));
    // The input.
    let x0 = at.width.mul_add(0.3, at.x);
    rule_wide(scene, palette.text, Line::new((x0, floor), (x0, mid)), 3.0);
    // The shift: up for a positive interval, down for a negative one,
    // a quarter of the panel per octave.
    let shift = f64::from(tone.pitch) / 12.0 * at.height * 0.25;
    let x1 = at.width.mul_add(0.6, at.x);
    let top = (mid - shift).clamp(at.y + 2.0, floor);
    let alpha = crate::mcp::f64_to_f32(f64::from(tone.pitch_mix).mul_add(0.7, 0.3));
    rule_wide(
        scene,
        tint.multiply_alpha(alpha),
        Line::new((x1, floor), (x1, top)),
        if held { 4.0 } else { 3.0 },
    );
    rule(
        scene,
        tint.multiply_alpha(0.6),
        Line::new((x0, mid), (x1, top)),
    );
    if rack.detailed() {
        let text = format!("{:+}", tone.pitch);
        crate::tcp::glyphs(
            scene,
            font,
            palette.text,
            &text,
            x1 + 6.0,
            top.max(at.y + 8.0),
            7.0,
        );
    }
}

/// One unit as an indicator on a rail.
///
/// The unit's own row — the height its panel would have — and a rail
/// wide: one number drawn as a length or a light, and nothing you
/// would try to read a decision off. Live, because a rail is watched
/// for what its track is DOING — a compressor's bar and a gate's light
/// move with the signal; an EQ's sparkline is the one still thing,
/// because it is the one still setting. Reductions hang from the top,
/// the way they do in the full panel; levels and tails stand on the
/// floor.
fn minimal(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    tone: &Tone,
    meters: &Meters,
    which: Which,
    at: Panel,
    rack: Rack,
) {
    // The row's ground, so the rail's chain reads as a chain.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.tcp_meter_well.multiply_alpha(0.5),
        None,
        &at.rect(),
    );
    // The same header row the panel takes, so the indicator starts on
    // the line the display does — with the name where it fits.
    let row = at;
    let at = body_of(row, rack);
    if at.y > row.inset(2.0).y {
        header(
            scene,
            palette,
            font,
            which.name(),
            None,
            "",
            tone.bypass.is(which),
            row.inset(2.0).split_top(HEAD).0,
        );
    }
    minimal_body(scene, palette, font, tone, meters, which, at);
}

/// A rail row's indicator in `at` (its body): what a theme's `rack.rail`
/// leaves to the host.
fn minimal_body(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    tone: &Tone,
    meters: &Meters,
    which: Which,
    at: Panel,
) {
    let right = at.x + at.width;
    let bottom = at.y + at.height;
    let mid = at.y + at.height / 2.0;
    // A length down the row, hanging from the top, as a share of it.
    let bar = |scene: &mut Scene, ink: Color, share: f64| {
        let share = share.clamp(0.0, 1.0);
        if share > 0.01 {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                ink,
                None,
                &Rect::new(
                    at.x + 2.0,
                    at.y,
                    right - 2.0,
                    share.mul_add(at.height, at.y),
                ),
            );
        }
    };
    let peak_db = if meters.is_empty() {
        None
    } else {
        Some(20.0 * f64::from(meters.sat_peak.max(1e-4)).log10())
    };
    match which {
        // The whole graph — the plugin's own fills, nodes and total
        // curve — read, not edited. A rail is thin, not blind: the
        // band colours still say which band is where, and the nodes
        // still say how many decisions there were. Without the
        // analyser: at this width a spectrum behind the curve is a
        // curve you cannot find.
        Which::RescueEq
        | Which::Eq
        | Which::Space
        | Which::PreEq
        | Which::PostEq
        | Which::DecayEq
        | Which::PolishEq => {
            let tint = (which == Which::DecayEq).then_some(DECAY_INK);
            eq(
                scene,
                palette,
                font,
                tone,
                which,
                tone.bands_ref(which),
                &[],
                at,
                Rack::Full,
                None,
                tint,
            );
        }
        // The level against the threshold, and a light for the door.
        Which::Gate => {
            let to_y = |db: f64| at.y + comp_ui::comp_graph_svg::db_to_y(db, at.height);
            let line = to_y(f64::from(tone.gate.threshold)).clamp(at.y, bottom);
            rule(
                scene,
                GATE_INK.multiply_alpha(0.7),
                Line::new((at.x, line), (right, line)),
            );
            if let Some(db) = peak_db {
                let top = to_y(db).clamp(at.y, bottom);
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    hex(comp_ui::comp_graph_svg::colors::GREY).multiply_alpha(0.6),
                    None,
                    &Rect::new(at.x + 2.0, top, right - 2.0, bottom),
                );
            }
            let open = peak_db.is_some_and(|db| db > f64::from(tone.gate.threshold));
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                GATE_INK.multiply_alpha(if open { 0.95 } else { 0.2 }),
                None,
                &Rect::new(at.x + 2.0, at.y + 2.0, right - 2.0, at.y + 8.0).to_rounded_rect(1.5),
            );
        }
        // What the compressor is taking off right now, as a length.
        Which::Comp | Which::RescueComp => {
            let comp = if which == Which::Comp {
                tone.comp
            } else {
                tone.rescue_comp
            };
            let reduction = peak_db.map_or(0.0, |db| {
                let out = comp_ui::comp_graph_svg::compress_transfer(
                    crate::mcp::f64_to_f32(db),
                    comp.threshold,
                    comp.ratio,
                    comp.knee,
                );
                (db - f64::from(out)).max(0.0)
            });
            // The threshold where the panel would draw it, and the
            // reduction hanging from the ceiling in the panel's red.
            let line = (at.y
                + comp_ui::comp_graph_svg::db_to_y(f64::from(comp.threshold), at.height))
            .clamp(at.y, bottom);
            rule(
                scene,
                hex(comp_ui::comp_graph_svg::colors::THRESHOLD).multiply_alpha(0.7),
                Line::new((at.x, line), (right, line)),
            );
            bar(
                scene,
                hex(comp_ui::comp_graph_svg::colors::REDUCTION_EDGE),
                reduction / 60.0,
            );
        }
        // Heat.
        Which::Sat => {
            let ladder = crate::live::ladder(&tone.sat);
            let rungs =
                peak_db.map_or_else(|| ladder.full(), |db| ladder.at(crate::mcp::f64_to_f32(db)));
            let heat = (f64::from(rungs.iter().sum::<f32>()) / 0.9)
                .clamp(0.0, 1.0)
                .powf(0.7);
            glow(scene, at, (at.x + at.width / 2.0, mid), heat);
        }
        Which::DeEss | Which::DeEssIn => {
            bar(scene, DEESS_INK, f64::from(meters.deess_deepest()) / 12.0)
        }
        // The repeats, tiny.
        Which::Delay => {
            for (k, (x, level)) in echo_taps(tone.delay, at).into_iter().enumerate() {
                let ink = if k == 0 { palette.text } else { DELAY_INK };
                rule(
                    scene,
                    ink,
                    Line::new(
                        (x, bottom - 1.0),
                        (x, bottom - 1.0 - level * (at.height * 0.9)),
                    ),
                );
            }
        }
        // The tail, tiny.
        Which::Reverb => {
            let tail = crate::live::tail(tone.reverb.key());
            let geometry = RoomGeometry::of(tone.reverb, at);
            let points = tail.envelope.iter().enumerate().map(|(i, db)| {
                let t = crate::num::coord(i)
                    / crate::num::coord(crate::live::TAIL_BINS.saturating_sub(1));
                let level = (1.0 + f64::from(*db) / 60.0).clamp(0.0, 1.0);
                (
                    t.mul_add(geometry.end - geometry.start, geometry.start),
                    bottom - 1.0 - level * (at.height * 0.9),
                )
            });
            curve(scene, REVERB_INK, points, 1.0);
        }
        // How wide, as a bar out from the middle.
        Which::Wide => {
            let half = (f64::from(tone.wide) / 2.0).clamp(0.0, 1.0) * at.width / 2.0;
            let centre = at.x + at.width / 2.0;
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                phase_tint(Which::Wide.phase()).multiply_alpha(0.8),
                None,
                &Rect::new(centre - half, at.y + 3.0, centre + half, bottom - 3.0),
            );
        }
        // The interval: a mark above or below the line.
        Which::Mod => crate::faces::paint_mod_in(scene, tone, at),
        Which::Pitch => {
            let shift = (f64::from(tone.pitch) / 24.0).clamp(-1.0, 1.0) * (at.height / 2.0 - 2.0);
            rule(
                scene,
                palette.grid_beat,
                Line::new((at.x, mid), (right, mid)),
            );
            rule_wide(
                scene,
                phase_tint(Which::Pitch.phase()),
                Line::new((at.x + 2.0, mid - shift), (right - 2.0, mid - shift)),
                2.0,
            );
        }
        Which::Knobs | Which::Presets => {}
    }
}

/// A stroked glyph path, round-capped.
fn stroke_glyph(scene: &mut Scene, ink: Color, path: &BezPath, width: f64) {
    scene.stroke(
        &Stroke::new(width).with_caps(vello::kurbo::Cap::Round),
        Affine::IDENTITY,
        ink,
        None,
        path,
    );
}

/// The saturator's circuit, in eight pixels.
fn circuit_glyph(scene: &mut Scene, ink: Color, circuit: Circuit, x: f64, baseline: f64) {
    let top = baseline - 7.0;
    let stroke = |scene: &mut Scene, path: &BezPath, w: f64| stroke_glyph(scene, ink, path, w);
    let mut path = BezPath::new();
    match circuit {
        // A valve: a rounded envelope on a base.
        Circuit::Valve => {
            path.move_to((x + 1.0, baseline));
            path.line_to((x + 1.0, top + 3.0));
            path.quad_to((x + 1.0, top), (x + 4.0, top));
            path.quad_to((x + 7.0, top), (x + 7.0, top + 3.0));
            path.line_to((x + 7.0, baseline));
            stroke(scene, &path, 0.9);
        }
        // Tape: two reels.
        Circuit::Tape => reels(scene, ink, x, top),
        // A core: windings.
        Circuit::Core => {
            for i in 0..4 {
                let cx = crate::num::coord(i).mul_add(2.0, x + 1.0);
                path.move_to((cx, baseline));
                path.quad_to((cx, top), (cx + 2.0, top));
            }
            stroke(scene, &path, 0.8);
        }
        // Solid state: the corner.
        Circuit::Solid => {
            path.move_to((x, baseline));
            path.line_to((x + 4.0, top + 1.0));
            path.line_to((x + 8.0, top + 1.0));
            stroke(scene, &path, 1.0);
        }
        // Steps.
        Circuit::Steps => {
            path.move_to((x, baseline));
            for i in 0..3 {
                let sx = crate::num::coord(i).mul_add(2.6, x);
                let sy = crate::num::coord(i.saturating_add(1)).mul_add(-2.2, baseline);
                path.line_to((sx, sy));
                path.line_to((sx + 2.6, sy));
            }
            stroke(scene, &path, 0.9);
        }
    }
}

/// Two reels — tape, whether it is a saturator's or a delay's.
fn reels(scene: &mut Scene, ink: Color, x: f64, top: f64) {
    let mut path = BezPath::new();
    for cx in [x + 2.0, x + 6.5] {
        path.extend(
            vello::kurbo::Circle::new((cx, top + 3.5), 1.8)
                .to_path(0.1)
                .elements()
                .iter()
                .copied(),
        );
    }
    path.move_to((x + 2.0, top + 1.7));
    path.line_to((x + 6.5, top + 1.7));
    stroke_glyph(scene, ink, &path, 0.8);
}

/// The delay's family, in eight pixels.
fn delay_glyph(scene: &mut Scene, ink: Color, family: DelayFamily, x: f64, baseline: f64) {
    let top = baseline - 7.0;
    let stroke = |scene: &mut Scene, path: &BezPath, w: f64| stroke_glyph(scene, ink, path, w);
    let mut path = BezPath::new();
    match family {
        // Digital: three exact ticks.
        DelayFamily::Digital => {
            for i in 0..3 {
                let tx = crate::num::coord(i).mul_add(3.0, x + 1.0);
                path.move_to((tx, baseline));
                path.line_to((tx, top + 1.0));
            }
            stroke(scene, &path, 1.0);
        }
        DelayFamily::Tape => reels(scene, ink, x, top),
        // Analog: a chip with legs.
        DelayFamily::Analog => {
            path.extend(
                Rect::new(x + 1.0, top + 1.5, x + 7.0, baseline - 1.5)
                    .to_path(0.1)
                    .elements()
                    .iter()
                    .copied(),
            );
            for i in 0..3 {
                let lx = crate::num::coord(i).mul_add(2.0, x + 2.0);
                path.move_to((lx, top + 1.5));
                path.line_to((lx, top));
                path.move_to((lx, baseline - 1.5));
                path.line_to((lx, baseline));
            }
            stroke(scene, &path, 0.8);
        }
        // Pitch: a staircase of repeats.
        DelayFamily::Pitch => {
            for i in 0..3 {
                let tx = crate::num::coord(i).mul_add(3.0, x + 1.0);
                let rise = crate::num::coord(i) * 2.0;
                path.move_to((tx, baseline - rise));
                path.line_to((tx, rise.mul_add(-0.5, top + 2.0)));
            }
            stroke(scene, &path, 1.0);
        }
        // Rhythmic: the tap grid.
        DelayFamily::Rhythmic => {
            for (i, j) in [(0, 0), (1, 0), (0, 1), (1, 1), (2, 0)] {
                let cx = crate::num::coord(i).mul_add(3.0, x + 1.5);
                let cy = crate::num::coord(j).mul_add(3.5, top + 2.0);
                path.extend(
                    vello::kurbo::Circle::new((cx, cy), 0.9)
                        .to_path(0.1)
                        .elements()
                        .iter()
                        .copied(),
                );
            }
            scene.fill(Fill::NonZero, Affine::IDENTITY, ink, None, &path);
        }
        // Special: a smear.
        DelayFamily::Special => {
            path.extend(
                vello::kurbo::Ellipse::new((x + 4.0, top + 3.5), (3.5, 2.0), 0.4)
                    .to_path(0.1)
                    .elements()
                    .iter()
                    .copied(),
            );
            stroke(scene, &path, 0.8);
        }
    }
}

/// The reverb's family, in eight pixels.
fn room_glyph(scene: &mut Scene, ink: Color, family: RoomFamily, x: f64, baseline: f64) {
    let top = baseline - 7.0;
    let stroke = |scene: &mut Scene, path: &BezPath, w: f64| stroke_glyph(scene, ink, path, w);
    let mut path = BezPath::new();
    match family {
        // A hall: the arch.
        RoomFamily::Hall => {
            path.move_to((x, baseline));
            path.line_to((x, top + 3.0));
            path.quad_to((x, top), (x + 4.0, top));
            path.quad_to((x + 8.0, top), (x + 8.0, top + 3.0));
            path.line_to((x + 8.0, baseline));
            stroke(scene, &path, 0.9);
        }
        // A plate: the sheet, hung.
        RoomFamily::Plate => {
            path.extend(
                Rect::new(x + 0.5, top + 2.0, x + 7.5, baseline)
                    .to_path(0.1)
                    .elements()
                    .iter()
                    .copied(),
            );
            path.move_to((x + 2.0, top + 2.0));
            path.line_to((x + 2.0, top));
            path.move_to((x + 6.0, top + 2.0));
            path.line_to((x + 6.0, top));
            stroke(scene, &path, 0.8);
        }
        // A room: a box in perspective.
        RoomFamily::Room => {
            path.move_to((x, baseline));
            path.line_to((x + 2.0, top + 2.0));
            path.line_to((x + 6.0, top + 2.0));
            path.line_to((x + 8.0, baseline));
            path.close_path();
            stroke(scene, &path, 0.9);
        }
        // A spring: the coil.
        RoomFamily::Spring => {
            path.move_to((x, baseline - 1.0));
            for i in 0..4 {
                let sx = crate::num::coord(i).mul_add(2.0, x);
                path.line_to((sx + 1.0, top + 1.0));
                path.line_to((sx + 2.0, baseline - 1.0));
            }
            stroke(scene, &path, 0.8);
        }
        // Ambient: the halo.
        RoomFamily::Ambient => {
            path.extend(
                vello::kurbo::Circle::new((x + 4.0, top + 3.5), 3.0)
                    .to_path(0.1)
                    .elements()
                    .iter()
                    .copied(),
            );
            path.extend(
                vello::kurbo::Circle::new((x + 4.0, top + 3.5), 1.0)
                    .to_path(0.1)
                    .elements()
                    .iter()
                    .copied(),
            );
            stroke(scene, &path, 0.8);
        }
        // Random: scattered.
        RoomFamily::Random => {
            for (dx, dy) in [(1.0, 1.0), (5.5, 2.5), (3.0, 5.0), (7.0, 6.0), (2.0, 3.5)] {
                path.extend(
                    vello::kurbo::Circle::new((x + dx, top + dy), 0.8)
                        .to_path(0.1)
                        .elements()
                        .iter()
                        .copied(),
                );
            }
            scene.fill(Fill::NonZero, Affine::IDENTITY, ink, None, &path);
        }
        // Special: the ladder of lamps.
        RoomFamily::Special => {
            path.move_to((x + 1.5, baseline));
            path.line_to((x + 1.5, top));
            path.move_to((x + 6.5, baseline));
            path.line_to((x + 6.5, top));
            for i in 0..3 {
                let ry = crate::num::coord(i).mul_add(2.5, top + 1.0);
                path.move_to((x + 1.5, ry));
                path.line_to((x + 6.5, ry));
            }
            stroke(scene, &path, 0.8);
        }
        // Convolution: the recorded thing.
        RoomFamily::Convolution => {
            path.move_to((x, baseline - 3.0));
            for (i, h) in [7.0, 2.0, 5.0, 3.5, 4.5, 4.0, 3.0].into_iter().enumerate() {
                let sx = x + 1.0 + crate::num::coord(i);
                path.line_to((sx, baseline - h));
                path.line_to((sx + 0.5, (h - 3.0).mul_add(0.5, baseline - 3.0)));
            }
            stroke(scene, &path, 0.7);
        }
    }
}

/// A panel's header: what it is on the left — with the glyph of which
/// machine, where the unit is a family of them — and what it is set
/// to on the right.
///
/// The value is right-aligned so the panels' numbers line up down the
/// rack — which is what lets you compare two strips by running your
/// eye down them rather than reading six numbers.
///
/// The value is dropped rather than elided when the panel is too narrow
/// for both: a truncated "−14d…" is a number you have to open the
/// plugin to check, which is worse than one you know is not shown.
#[expect(
    clippy::too_many_arguments,
    reason = "a drawing and everything it needs"
)]
fn header(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    name: &str,
    mark: Option<Glyph>,
    value: &str,
    bypassed: bool,
    at: Panel,
) {
    const SIZE: f32 = 7.0;
    let baseline = at.y + f64::from(SIZE);
    let (name_ink, value_ink) = header_inks(palette, bypassed);
    let mut left = at.x;
    if let Some(mark) = mark {
        glyph(scene, name_ink, mark, left, baseline - 0.5);
        left += GLYPH_W;
    }
    // A name that will not fit is not drawn: the row is still taken,
    // because the row is what keeps the display under it level with
    // its neighbours, but a rail is not wide enough to be read.
    if (left - at.x) + font.width(name, SIZE) > at.width {
        return;
    }
    crate::tcp::glyphs(scene, font, name_ink, name, left, baseline, SIZE);

    // "BYPASS" replaces the value, because the value is what the
    // processor WOULD do and it is not doing it. Leaving the numbers up
    // would be a panel reporting a setting that is having no effect.
    let value = if bypassed { "BYPASS" } else { value };
    let name_w = font.width(name, SIZE) + (left - at.x);
    let value_w = font.width(value, SIZE);
    if name_w + value_w + 6.0 > at.width {
        return;
    }
    crate::tcp::glyphs(
        scene,
        font,
        value_ink,
        value,
        at.x + at.width - value_w,
        baseline,
        SIZE,
    );
}

/// A header's inks: its name's and its value's.
///
/// The header stays ABOVE the scrim, because it is the one thing on a
/// bypassed panel you still need: which processor this is, and that it
/// is off. Its own ink dims instead.
fn header_inks(palette: &Palette, bypassed: bool) -> (Color, Color) {
    if bypassed {
        (palette.text_faint.multiply_alpha(0.55), palette.text_faint)
    } else {
        (palette.text_faint, palette.text_dim)
    }
}

/// A colour the plugin states, as a colour this window can paint.
///
/// The plugin's UIs are DOMs, so they name colours as `#rrggbb` —
/// `eq_graph_model::freq_to_color` returns one and
/// `comp_graph_svg::colors` is a table of them. Parsing is how a vello
/// host reads the same values rather than keeping a second table that
/// drifts.
fn hex(value: &str) -> Color {
    let digits = value.trim_start_matches('#');
    let channel = |from: usize| {
        digits
            .get(from..from.saturating_add(2))
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
    };
    match (channel(0), channel(2), channel(4)) {
        (Some(r), Some(g), Some(b)) => Color::from_rgba8(r, g, b, 0xff),
        // A table that stopped producing hex is a bug in the plugin,
        // not a reason for the rack to draw nothing: grey is visible
        // and obviously not a decision.
        _ => Color::from_rgba8(0x88, 0x88, 0x88, 0xff),
    }
}

/// A band's colour, from the plugin's own frequency map.
///
/// `eq_graph_model::freq_to_color` sweeps the hue red to violet across
/// the audible range and hands back a hex string, because its first
/// consumer was a DOM. Parsing it here is cheaper than a second colour
/// map that would drift from the editor's — and drifting is exactly
/// what must not happen, since the point of the colour is that a band
/// is the same colour in the strip as it is in the plugin window.
fn band_color(hz: f64) -> Color {
    hex(&eq_ui::eq_graph_model::freq_to_color(hz))
}

/// A filled circle — a handle, or a marker on a curve.
fn dot(scene: &mut Scene, color: Color, at: (f64, f64), r: f64) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color,
        None,
        &vello::kurbo::Circle::new(at, r),
    );
}

/// A line of a given width — the threshold, which has to be findable.
fn rule_wide(scene: &mut Scene, color: Color, line: Line, width: f64) {
    scene.stroke(&Stroke::new(width), Affine::IDENTITY, color, None, &line);
}

/// One hairline.
fn rule(scene: &mut Scene, color: Color, line: Line) {
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, color, None, &line);
}

/// An open polyline through `points`.
fn curve(scene: &mut Scene, color: Color, points: impl Iterator<Item = (f64, f64)>, width: f64) {
    let mut path = BezPath::new();
    for (i, point) in points.enumerate() {
        if i == 0 {
            path.move_to(point);
        } else {
            path.line_to(point);
        }
    }
    if path.is_empty() {
        return;
    }
    scene.stroke(
        &Stroke::new(width).with_caps(vello::kurbo::Cap::Round),
        Affine::IDENTITY,
        color,
        None,
        &path,
    );
}

const fn f64_to_f32(value: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "a dB value in a display range of sixty; f32 holds it exactly"
    )]
    let narrowed = value as f32;
    narrowed
}

/// A plausible Tone chain, until tracks carry their own.
///
/// The rack has nowhere to read real settings from yet. Not because the
/// chain is unreachable — `FxChain::parameters` is right there — but
/// because what comes back is unusable: daw-standalone adds an FX entry
/// by name without loading a binary, so its parameters fall through to
/// the stored path and arrive as `Param 1..N` at 0.5. `bin/chain-probe`
/// demonstrates it against a real session.
///
/// A rack fed `Param 3 = 0.5` would draw a curve that looks like
/// information and is not, which is worse than one that is honestly
/// made up. So this stands in, varied by track index so that a mixer
/// full of racks looks like a mixer full of different decisions rather
/// than one curve repeated twenty times.
#[must_use]
/// A reverb machine: hardware that runs any algorithm as a program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReverbMachine {
    /// The 480-style digital effects system and its remote.
    Digital480,
}

impl ReverbMachine {
    /// The word frame's manifest matches its face by.
    #[must_use]
    pub const fn face(self) -> &'static str {
        match self {
            Self::Digital480 => "480",
        }
    }
}

/// Which compressor a sidechain EQ is for: the Rescue one, the Tone
/// one, or the second of a stacked pair after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sidechain {
    Rescue = 0,
    Main = 1,
    After = 2,
}

impl Sidechain {
    /// The panel this compressor stands in.
    #[must_use]
    pub const fn panel(self) -> Which {
        match self {
            Self::Rescue => Which::RescueComp,
            Self::Main | Self::After => Which::Comp,
        }
    }
}

/// The emphasis bands' starting spread — the plugin's: idle bells at
/// 80 Hz … 10 kHz.
fn emphasis_spread() -> [saturate_dsp::emphasis::EmphBand; saturate_dsp::emphasis::BANDS] {
    [80.0, 250.0, 700.0, 1_800.0, 4_500.0, 10_000.0].map(|freq_hz| {
        saturate_dsp::emphasis::EmphBand {
            freq_hz,
            ..saturate_dsp::emphasis::EmphBand::default()
        }
    })
}

pub fn placeholder(index: usize) -> Tone {
    let voice = Character::of(index);
    // A drift within the voice, so two kicks are not one kick. Small
    // enough that they still read as the same decision made twice.
    let drift = crate::num::coord(index % 5) - 2.0;
    let mut tone = Tone {
        // Rescue is the same surgery on every track, roughly: a
        // high-pass and one cut. It is the pass you make before you
        // know what the track is going to be.
        rescue_eq: vec![
            band(0, 30.0 + drift * 4.0, -18.0, 0.7, EqBandShape::LowCut),
            band(1, 240.0 * 1.1_f64.powf(drift), -3.5, 2.4, EqBandShape::Bell),
        ],
        gate: Gate {
            threshold: f64_to_f32(-42.0 + drift * 3.0),
            ..Gate::default()
        },
        rescue_comp: Comp {
            threshold: f64_to_f32(-8.0 + drift),
            ratio: 2.2,
            ..voice.comp(drift)
        },
        eq: voice.bands(drift),
        comp: voice.comp(drift),
        de_ess: Suppress::sibilance(),
        polish_eq: vec![band(0, 3_200.0, -3.0, 6.0, EqBandShape::Bell)],
        // The relational pass carves rather than shapes: one wide dip
        // where something else lives.
        space: vec![band(
            0,
            900.0 * 1.35_f64.powf(drift),
            -2.5,
            0.9,
            EqBandShape::Bell,
        )],
        delay: Echo {
            time: f64_to_f32(180.0 + 60.0 * (drift + 2.0)),
            style: voice.delay_style(),
            ..Echo::default()
        },
        reverb_machine: None,
        reverb: Room {
            decay: f64_to_f32(1.1 + 0.35 * (drift + 2.0)),
            algorithm: voice.reverb(),
            ..Room::default()
        },
        // Not all one zoom: a vocal worked at ±3 and a room mic at ±18
        // is the normal case, and a desk where every graph is drawn to
        // the same range hides the fact that the range is a choice.
        // Every engine somewhere on the desk, so each picture is seen.
        comp_face: crate::plugin_faces::CompFace::Vca,
        engine_units: None,
        engine_others: Vec::new(),
        reverb_extra: ReverbExtra::default(),
        comp_after: None,
        sidechain: Default::default(),
        sc_view: None,
        drum: false,
        modulation: Wobble {
            engine: modulation_ui::viz::Engine::ALL
                .get(index % modulation_ui::viz::Engine::ALL.len())
                .copied()
                .unwrap_or_default(),
            ..Wobble::default()
        },
        eq_range: match index % 3 {
            0 => DEFAULT_EQ_RANGE,
            1 => DEFAULT_EQ_RANGE.saturating_sub(1),
            _ => DEFAULT_EQ_RANGE.saturating_add(1),
        },
        sat: {
            let mut pre = ClassAPreamp::new(f64_to_f32(DISPLAY_RATE));
            // A single-ended stage: a triode grid above and iron below,
            // biased off centre. `Clean` is wire, and wire into the
            // output clamp draws a hard clipper's flat-ramp-flat — a
            // picture of a limiter, not of saturation. Replaced by the
            // voice's profile below, through the plugin's own `apply`.
            pre.positive = SideShaper::Tube;
            pre.negative = SideShaper::Transformer;
            pre.drive = f64_to_f32(voice.drive() + drift * 0.3);
            pre.q_point = 0.25;
            pre
        },
        sat_profile: 0,
        sat_digital: saturate_dsp::digital::DigitalStage::new(),
        sat_emphasis: emphasis_spread(),
        sat_emphasis_view: false,
        pre_eq: Vec::new(),
        post_eq: Vec::new(),
        decay_eq: Vec::new(),
        wide: 1.0,
        pitch: 0,
        pitch_mix: 0.5,
        role: Role::Channel,
        presets: Vec::new(),
        preset: None,
        bypass: Bypass::default(),
    };
    // Each voice on its own circuit, so a mixer of racks shows the five
    // faces; the drive the voice wants survives the profile.
    let drive = tone.sat.drive;
    tone.set_sat_profile(saturate_profiles::profile_index(voice.sat_profile()).unwrap_or(0));
    tone.sat.drive = drive;
    tone
}

/// A placeholder for a track of a given role.
///
/// An FX return carries its presets — brighter to darker — and opens on
/// the one its name asks for, so a "Long" verb is a long verb before
/// anything is clicked.
#[must_use]
pub fn placeholder_for(role: Role, index: usize, name: &str, ancestors: &[String]) -> Tone {
    let mut tone = placeholder(index);
    tone.role = role;
    tone.comp_face = crate::plugin_faces::CompFace::for_track(name, ancestors);
    tone.drum = crate::plugin_faces::is_drum(name, ancestors);
    // The kick: an 1176 catching the beater, into an LA-2A for the body.
    if crate::plugin_faces::is_named(name, ancestors, "kick") {
        let after = Comp {
            threshold: -20.0,
            ratio: 3.0,
            attack: 10.0,
            release: 300.0,
            ..Comp::default()
        };
        tone.comp_after = Some((after, crate::plugin_faces::CompFace::Opto));
    }
    if role == Role::Fund {
        return fundamental(tone, name, ancestors);
    }
    // The voices' reverbs run in a 480: the classic vocal box.
    if role == Role::Reverb
        && ["vocal", "bgv", "choir"]
            .iter()
            .any(|w| crate::plugin_faces::is_named(name, ancestors, w))
    {
        tone.reverb_machine = Some(ReverbMachine::Digital480);
    }
    // The presets are the slot's — a Short delay offers short delays
    // — and the track opens on the slot's default, which is first.
    let presets = match role {
        Role::Reverb => reverb_presets(name),
        Role::Delay => delay_presets(name),
        Role::Wide => {
            let lower = name.to_lowercase();
            if lower.contains("chorus") || lower.contains("flang") || lower.contains("dimension") {
                mod_presets(name)
            } else {
                wide_presets()
            }
        }
        Role::Pitch => pitch_presets(name),
        Role::Parallel => parallel_presets(name),
        Role::Channel
        | Role::Bus
        | Role::Fund
        | Role::Trig
        | Role::Dry
        | Role::Half
        | Role::Source => Vec::new(),
    };
    if presets.is_empty() {
        return tone;
    }
    tone.presets = presets;
    tone.load_preset(0);
    tone
}

/// A fundamental's chain, tuned to the piece it sits under.
///
/// One band: a band-pass at the note, whose Q says how much of the
/// spectrum around it comes along. The note comes from the piece — a
/// kick's fundamental is in the fifties, a snare's around two hundred,
/// the toms step down — which is read off the folders above the track.
fn fundamental(mut tone: Tone, name: &str, ancestors: &[String]) -> Tone {
    let lower = name.to_lowercase();
    let piece = ancestors
        .iter()
        .rev()
        .map(|a| a.to_lowercase())
        .find(|a| a.starts_with("kick") || a.starts_with("snare") || a.starts_with("tom"));
    let hz = match piece.as_deref() {
        Some(p) if p.starts_with("kick") => 55.0,
        Some(p) if p.starts_with("snare") => 200.0,
        Some(p) if p.starts_with("tom") => {
            // "Tom 1" is the highest; each one down is a whole step or
            // so lower.
            let number = p.chars().filter_map(|c| c.to_digit(10)).next().unwrap_or(1);
            [130.0, 110.0, 92.0, 78.0]
                .get(usize::try_from(number.saturating_sub(1)).unwrap_or(0))
                .copied()
                .unwrap_or(78.0)
        }
        _ => 100.0,
    };
    // A sub is the octave under the note.
    let hz = if lower == "sub" { hz / 2.0 } else { hz };
    let q = 2.2;
    tone.eq = vec![band(0, hz, 0.0, q, EqBandShape::BandPass)];
    tone.gate = Gate {
        threshold: -32.0,
        range: -40.0,
        attack: 0.5,
        hold: 40.0,
        release: 90.0,
    };
    tone.set_sat_profile(saturate_profiles::profile_index("transformer").unwrap_or(0));
    tone.sat.drive = 2.0;
    tone.presets = Vec::new();
    tone.preset = None;
    tone
}

fn preset(name: &str, mut tone: Tone) -> Preset {
    tone.presets = Vec::new();
    tone.preset = None;
    Preset {
        name: name.to_owned(),
        tone: Box::new(tone),
    }
}

/// A high shelf at 6 kHz, which is most of what "darker" means.
fn shelf(gain: f64) -> EqBand {
    band(0, 6_000.0, gain, 0.7, EqBandShape::HighShelf)
}

/// The slot a return fills, read off its name.
///
/// A return is not "a delay": it is THE slap, or THE throw, and the
/// presets it offers are the ones that fill that slot — so cycling
/// them swaps one slap for another slap, never for a throw. A name
/// that says nothing lands on the middle slot.
fn slot_of(name: &str, slots: &'static [&'static str]) -> &'static str {
    let lower = name.to_lowercase();
    slots
        .iter()
        .copied()
        .find(|slot| lower.contains(&slot.to_lowercase()))
        .unwrap_or_else(|| slots.get(slots.len() / 2).copied().unwrap_or(""))
}

/// The delay slots, tightest first.
const DELAY_SLOTS: &[&str] = &["Slap", "Short", "Long", "Throw"];

/// The reverb slots, shortest first.
///
/// `Plate` and `Hall` are the snare's, named for the machine rather than
/// for the length. `Long` stays the middle one — a name that says
/// nothing lands there — so new slots go in pairs around it.
const REVERB_SLOTS: &[&str] = &["Room", "Plate", "Short", "Long", "Hall", "Moment", "Throw"];

/// A delay preset from its numbers.
fn repeat(
    name: &str,
    style: DelayStyle,
    time: f32,
    feedback: f32,
    mix: f32,
    tone: f32,
    width: f32,
    post_eq: Vec<EqBand>,
) -> Preset {
    let mut t = placeholder(1);
    t.role = Role::Delay;
    t.delay = Echo {
        time,
        feedback,
        mix,
        tone,
        width,
        style,
        sync: None,
    };
    t.post_eq = post_eq;
    preset(name, t)
}

/// A reverb preset from its numbers. Every one cuts the lows on the
/// way in; what differs is the space and how it is shaped after.
fn space(
    name: &str,
    algorithm: AlgorithmType,
    decay: f32,
    predelay: f32,
    size: f32,
    damping: f32,
    diffusion: f32,
    mix: f32,
    post_eq: Vec<EqBand>,
    decay_eq: Vec<EqBand>,
) -> Preset {
    let mut t = placeholder(0);
    t.role = Role::Reverb;
    t.pre_eq = vec![band(0, 180.0, -18.0, 0.7, EqBandShape::LowCut)];
    t.reverb = Room {
        algorithm,
        decay,
        predelay,
        damping,
        size,
        diffusion,
        mix,
        sync: None,
    };
    t.post_eq = post_eq;
    t.decay_eq = decay_eq;
    preset(name, t)
}

fn low_cut(hz: f64) -> EqBand {
    band(0, hz, -18.0, 0.7, EqBandShape::LowCut)
}

/// The curated presets for one delay slot, cleanest to most coloured.
///
/// The first is the slot's default — what the track opens on — and
/// the rest are the same job done by a different machine.
fn delay_presets(name: &str) -> Vec<Preset> {
    use DelayStyle as D;
    let lower = name.to_lowercase();
    // The instrument bus's delays, by name.
    if lower.contains("tape") && !lower.contains("throw") {
        return vec![
            repeat(
                "Tape 1/16",
                D::Tape,
                187.0,
                0.3,
                0.25,
                0.5,
                0.6,
                vec![low_cut(200.0), shelf(-6.0)],
            ),
            repeat(
                "Tape 1/8",
                D::Tape,
                375.0,
                0.38,
                0.22,
                0.55,
                0.7,
                vec![low_cut(250.0), shelf(-6.0)],
            ),
            repeat(
                "Dragged 1/8",
                D::Tape,
                395.0,
                0.38,
                0.22,
                0.55,
                0.7,
                vec![low_cut(250.0), shelf(-6.0)],
            ),
            repeat(
                "Dark Tape",
                D::Tape,
                375.0,
                0.45,
                0.2,
                0.8,
                0.6,
                vec![low_cut(300.0), shelf(-10.0)],
            ),
        ];
    }
    if lower.contains("echo boy") || lower.contains("echoboy") {
        return vec![
            repeat(
                "Memory Man",
                D::Bbd,
                375.0,
                0.4,
                0.22,
                0.55,
                0.7,
                vec![
                    low_cut(200.0),
                    band(1, 1_200.0, 3.0, 1.0, EqBandShape::Bell),
                    shelf(-6.0),
                ],
            ),
            repeat(
                "Telephone",
                D::LoFi,
                375.0,
                0.4,
                0.22,
                0.6,
                0.3,
                vec![low_cut(600.0), shelf(-12.0)],
            ),
            repeat(
                "Binson",
                D::OilCan,
                400.0,
                0.45,
                0.2,
                0.6,
                0.7,
                vec![low_cut(250.0), shelf(-6.0)],
            ),
            repeat(
                "Ping-Pong 1/4",
                D::MultiTap,
                750.0,
                0.4,
                0.2,
                0.4,
                1.0,
                vec![low_cut(250.0)],
            ),
            repeat(
                "Motion",
                D::Bbd,
                375.0,
                0.4,
                0.22,
                0.5,
                1.0,
                vec![low_cut(200.0), shelf(-4.0)],
            ),
        ];
    }
    if lower.contains("space echo") {
        return vec![
            repeat(
                "Space Echo",
                D::Reverb,
                340.0,
                0.4,
                0.25,
                0.55,
                0.6,
                vec![low_cut(200.0), shelf(-6.0)],
            ),
            repeat(
                "Space Slap",
                D::Reverb,
                130.0,
                0.2,
                0.25,
                0.5,
                0.5,
                vec![low_cut(200.0), shelf(-4.0)],
            ),
            repeat(
                "Space Spring",
                D::Reverb,
                300.0,
                0.55,
                0.25,
                0.6,
                0.7,
                vec![low_cut(250.0), shelf(-8.0)],
            ),
        ];
    }
    match slot_of(name, DELAY_SLOTS) {
        "Slap" => vec![
            repeat(
                "Tape 95",
                D::Tape,
                95.0,
                0.08,
                0.3,
                0.3,
                0.2,
                vec![shelf(-1.0)],
            ),
            repeat(
                "Slap 15",
                D::Tape,
                15.0,
                0.05,
                0.35,
                0.4,
                0.4,
                vec![shelf(-3.0)],
            ),
            repeat(
                "Slap 30",
                D::Tape,
                30.0,
                0.05,
                0.35,
                0.4,
                0.4,
                vec![shelf(-3.0)],
            ),
            repeat(
                "Crowd",
                D::MultiTap,
                45.0,
                0.2,
                0.3,
                0.5,
                1.0,
                vec![low_cut(300.0), shelf(-6.0)],
            ),
            repeat(
                "Rockabilly",
                D::Tape,
                120.0,
                0.15,
                0.35,
                0.4,
                0.2,
                vec![shelf(-2.0)],
            ),
            repeat("Tight", D::Clean, 70.0, 0.02, 0.25, 0.1, 0.1, vec![]),
            repeat(
                "Drum Slap",
                D::Drum,
                110.0,
                0.1,
                0.3,
                0.35,
                0.6,
                vec![low_cut(150.0)],
            ),
            repeat(
                "Lo-Fi Slap",
                D::LoFi,
                100.0,
                0.12,
                0.3,
                0.6,
                0.3,
                vec![shelf(-4.0)],
            ),
        ],
        "Long" => vec![
            repeat(
                "Tape 1/8",
                D::Tape,
                375.0,
                0.42,
                0.22,
                0.55,
                0.85,
                vec![low_cut(250.0), shelf(-5.0)],
            ),
            repeat(
                "Dotted 8th",
                D::Clean,
                560.0,
                0.38,
                0.2,
                0.3,
                0.9,
                vec![low_cut(250.0), shelf(-3.0)],
            ),
            repeat(
                "Dark BBD",
                D::Bbd,
                375.0,
                0.5,
                0.22,
                0.7,
                0.8,
                vec![low_cut(300.0), shelf(-7.0)],
            ),
            repeat(
                "Oil Can",
                D::OilCan,
                400.0,
                0.45,
                0.2,
                0.6,
                0.7,
                vec![low_cut(300.0), shelf(-6.0)],
            ),
            repeat(
                "Shimmer 1/8",
                D::Shimmer,
                375.0,
                0.5,
                0.18,
                0.4,
                1.0,
                vec![low_cut(400.0)],
            ),
        ],
        "Throw" => vec![
            repeat(
                "BBD Throw",
                D::Bbd,
                750.0,
                0.55,
                0.4,
                0.7,
                1.0,
                vec![
                    low_cut(400.0),
                    band(1, 1_000.0, 6.0, 2.0, EqBandShape::Bell),
                    shelf(-9.0),
                ],
            ),
            repeat(
                "Tape 1/4",
                D::Tape,
                750.0,
                0.6,
                0.4,
                0.75,
                0.9,
                vec![low_cut(400.0), shelf(-8.0)],
            ),
            repeat(
                "Reverse",
                D::Reverse,
                700.0,
                0.4,
                0.4,
                0.5,
                1.0,
                vec![low_cut(300.0), shelf(-4.0)],
            ),
            repeat(
                "Pitch Throw",
                D::Pitch,
                750.0,
                0.5,
                0.35,
                0.45,
                1.0,
                vec![low_cut(400.0), shelf(-5.0)],
            ),
            repeat(
                "Spectral",
                D::Spectral,
                750.0,
                0.6,
                0.35,
                0.5,
                1.0,
                vec![low_cut(500.0)],
            ),
        ],
        _ => vec![
            repeat(
                "Clean 1/16",
                D::Clean,
                187.0,
                0.25,
                0.25,
                0.15,
                0.5,
                vec![low_cut(200.0)],
            ),
            repeat(
                "Tape 1/16",
                D::Tape,
                187.0,
                0.3,
                0.25,
                0.35,
                0.5,
                vec![low_cut(200.0), shelf(-3.0)],
            ),
            repeat(
                "BBD Bounce",
                D::Bbd,
                210.0,
                0.35,
                0.25,
                0.5,
                0.6,
                vec![low_cut(200.0), shelf(-5.0)],
            ),
            repeat(
                "Ping-Pong",
                D::MultiTap,
                187.0,
                0.3,
                0.25,
                0.2,
                1.0,
                vec![low_cut(200.0)],
            ),
            repeat(
                "Filtered",
                D::Filter,
                187.0,
                0.4,
                0.25,
                0.6,
                0.5,
                vec![low_cut(300.0), shelf(-6.0)],
            ),
        ],
    }
}

/// The named rooms a drum mix keeps ready — the bank a mixer picks
/// the band's room out of, short and bright down to long and dark,
/// then the odd ones. Each name is a return in the drum template's
/// Parallel folder, and each carries a few takes on the same idea.
const ROOM_NAMES: &[&str] = &[
    "Room Sim",
    "Wood Room",
    "Music Club",
    "Stadium",
    "RMX 16",
    "Nonlin",
    "Brick Wall",
    // The instrument bus: rooms, plates, halls and springs for
    // everything that is not drums or a voice.
    "Short Room",
    "Slap Room",
    "Early",
    "Fat Plate",
    "Dark Plate",
    "Gold Plate",
    "Large Hall",
    "Vienna",
    "Atmosphere",
    "Big Sky",
    "XL35",
];

/// The curated presets for one reverb slot, plainest to most coloured.
fn reverb_presets(name: &str) -> Vec<Preset> {
    use AlgorithmType as A;
    let dark = |low: f64, high: f64| {
        vec![
            band(0, 300.0, low, 0.7, EqBandShape::LowShelf),
            band(1, 5_000.0, high, 0.7, EqBandShape::HighShelf),
        ]
    };
    let lower = name.to_lowercase();
    if let Some(room) = ROOM_NAMES
        .iter()
        .copied()
        .find(|r| lower.contains(&r.to_lowercase()))
    {
        return match room {
            // A captured room, not a digital one, and treated like a
            // room mic: compressed fast on the way back. Fed off a
            // send so the blend into it is not the drum blend.
            "Room Sim" => vec![
                space(
                    "Sunset Sound",
                    A::Convolution,
                    0.9,
                    0.0,
                    0.5,
                    0.2,
                    1.0,
                    0.3,
                    vec![shelf(1.0)],
                    vec![],
                ),
                space(
                    "Ocean Way",
                    A::Convolution,
                    1.2,
                    0.0,
                    0.7,
                    0.3,
                    1.0,
                    0.3,
                    vec![],
                    vec![],
                ),
                space(
                    "Small Booth",
                    A::Convolution,
                    0.5,
                    0.0,
                    0.3,
                    0.3,
                    1.0,
                    0.3,
                    vec![shelf(-2.0)],
                    vec![],
                ),
                space(
                    "Live Room",
                    A::Room,
                    1.0,
                    4.0,
                    0.6,
                    0.25,
                    0.8,
                    0.3,
                    vec![],
                    vec![],
                ),
            ],
            // Short and bright: sizzle and snap and air around the kit
            // on the uptempo song that has no room for a tail.
            "Wood Room" => vec![
                space(
                    "Wood Room",
                    A::Room,
                    0.7,
                    6.0,
                    0.4,
                    0.2,
                    0.7,
                    0.22,
                    vec![shelf(2.0)],
                    vec![],
                ),
                space(
                    "Ruckus",
                    A::Room,
                    0.8,
                    4.0,
                    0.45,
                    0.1,
                    0.5,
                    0.22,
                    vec![shelf(3.0), band(1, 3_000.0, 2.0, 1.0, EqBandShape::Bell)],
                    vec![],
                ),
                space(
                    "Studio A",
                    A::Hall,
                    0.9,
                    8.0,
                    0.5,
                    0.3,
                    0.8,
                    0.22,
                    vec![],
                    vec![],
                ),
            ],
            // Short and smooth: the 480's music club, low mids and the
            // smoothest decay of the short ones.
            "Music Club" => vec![
                space(
                    "Music Club",
                    A::Room,
                    0.9,
                    10.0,
                    0.5,
                    0.55,
                    0.9,
                    0.22,
                    vec![shelf(-4.0)],
                    vec![],
                ),
                space(
                    "Warm Room",
                    A::Room,
                    1.1,
                    12.0,
                    0.55,
                    0.65,
                    0.9,
                    0.22,
                    vec![shelf(-6.0)],
                    dark(2.0, -6.0),
                ),
                space(
                    "Velvet Room",
                    A::Velvet,
                    1.0,
                    10.0,
                    0.5,
                    0.5,
                    1.0,
                    0.22,
                    vec![shelf(-3.0)],
                    vec![],
                ),
            ],
            // The big ballad: about two seconds, some top rolled off,
            // a little low end — then high-passed on the way back.
            "Stadium" => vec![
                space(
                    "Stadium",
                    A::Hall,
                    2.0,
                    30.0,
                    0.8,
                    0.5,
                    0.85,
                    0.2,
                    vec![low_cut(150.0), shelf(-3.0)],
                    dark(2.0, -4.0),
                ),
                space(
                    "Arena",
                    A::Hall,
                    2.6,
                    40.0,
                    0.9,
                    0.55,
                    0.85,
                    0.2,
                    vec![low_cut(180.0), shelf(-5.0)],
                    dark(3.0, -6.0),
                ),
                space(
                    "Marble Room",
                    A::Bloom,
                    1.8,
                    20.0,
                    0.7,
                    0.45,
                    1.0,
                    0.2,
                    vec![low_cut(120.0)],
                    vec![],
                ),
            ],
            // The eighties snare: dense, a bit of tail, blends with the
            // shorter rooms.
            "RMX 16" => vec![
                space(
                    "Ambience 1.7",
                    A::Plate,
                    1.7,
                    20.0,
                    0.6,
                    0.5,
                    1.0,
                    0.22,
                    vec![low_cut(100.0), shelf(-5.0)],
                    dark(3.0, -6.0),
                ),
                space(
                    "Big Snare",
                    A::Plate,
                    2.2,
                    30.0,
                    0.7,
                    0.45,
                    1.0,
                    0.25,
                    vec![low_cut(120.0), shelf(-3.0)],
                    dark(4.0, -4.0),
                ),
                space(
                    "Room Hall",
                    A::Random,
                    1.5,
                    15.0,
                    0.6,
                    0.5,
                    0.9,
                    0.22,
                    vec![low_cut(100.0)],
                    vec![],
                ),
            ],
            // Phil Collins. Not a lush tail and not supposed to be —
            // which is why even a grainy one sounds right.
            "Nonlin" => vec![
                space(
                    "Nonlin",
                    A::NonLinear,
                    0.9,
                    0.0,
                    0.6,
                    0.3,
                    0.9,
                    0.3,
                    vec![low_cut(120.0)],
                    vec![],
                ),
                space(
                    "Gated",
                    A::NonLinear,
                    0.6,
                    0.0,
                    0.5,
                    0.3,
                    0.9,
                    0.3,
                    vec![low_cut(150.0), shelf(2.0)],
                    vec![],
                ),
                space(
                    "Reverse",
                    A::NonLinear,
                    1.2,
                    0.0,
                    0.7,
                    0.4,
                    0.9,
                    0.3,
                    vec![low_cut(120.0)],
                    vec![],
                ),
            ],
            // Sits an instrument back into an acoustic space without
            // changing its sound: no pre-delay, under a second.
            "Short Room" => vec![
                space(
                    "Sonsig",
                    A::Room,
                    0.8,
                    0.0,
                    0.4,
                    0.3,
                    0.8,
                    0.2,
                    vec![],
                    vec![],
                ),
                space(
                    "Small Studio",
                    A::Room,
                    0.6,
                    0.0,
                    0.3,
                    0.25,
                    0.7,
                    0.2,
                    vec![shelf(1.0)],
                    vec![],
                ),
                space(
                    "D-Verb",
                    A::Hall,
                    0.9,
                    0.0,
                    0.4,
                    0.4,
                    0.4,
                    0.2,
                    vec![shelf(-2.0)],
                    vec![],
                ),
                space(
                    "PCM 60",
                    A::Room,
                    0.7,
                    4.0,
                    0.45,
                    0.35,
                    0.8,
                    0.2,
                    vec![],
                    vec![],
                ),
            ],
            // A slap off the walls, a little different left to right —
            // for live records, in combination with something longer.
            "Slap Room" => vec![
                space(
                    "Slap Room",
                    A::Reflections,
                    0.4,
                    10.0,
                    0.5,
                    0.3,
                    0.4,
                    0.22,
                    vec![],
                    vec![],
                ),
                space(
                    "Wide Slap",
                    A::Reflections,
                    0.5,
                    14.0,
                    0.6,
                    0.3,
                    0.3,
                    0.22,
                    vec![shelf(1.0)],
                    vec![],
                ),
                space(
                    "Tight Slap",
                    A::Reflections,
                    0.3,
                    6.0,
                    0.35,
                    0.3,
                    0.5,
                    0.22,
                    vec![],
                    vec![],
                ),
            ],
            // Early reflections and no tail: a real space that takes
            // up no space, the source pushed back by the distance.
            "Early" => vec![
                space(
                    "Cinematic Near",
                    A::Reflections,
                    0.25,
                    2.0,
                    0.5,
                    0.3,
                    0.6,
                    0.3,
                    vec![],
                    vec![],
                ),
                space(
                    "Cinematic Far",
                    A::Reflections,
                    0.35,
                    12.0,
                    0.8,
                    0.5,
                    0.6,
                    0.3,
                    vec![shelf(-6.0)],
                    vec![],
                ),
                space(
                    "Reflective",
                    A::Reflections,
                    0.3,
                    4.0,
                    0.6,
                    0.1,
                    0.7,
                    0.3,
                    vec![shelf(2.0)],
                    vec![],
                ),
            ],
            // The 480's fat plate: under two seconds, bright and airy,
            // the one every tool bag has.
            "Fat Plate" => vec![
                space(
                    "Fat Plate",
                    A::Plate,
                    1.9,
                    10.0,
                    0.6,
                    0.15,
                    1.0,
                    0.2,
                    vec![shelf(2.0)],
                    vec![],
                ),
                space(
                    "Bright Plate",
                    A::Plate,
                    1.6,
                    8.0,
                    0.5,
                    0.1,
                    1.0,
                    0.2,
                    vec![shelf(3.0)],
                    vec![],
                ),
                space(
                    "Thin Plate",
                    A::Plate,
                    1.4,
                    10.0,
                    0.4,
                    0.15,
                    0.9,
                    0.2,
                    vec![low_cut(300.0), shelf(2.0)],
                    vec![],
                ),
            ],
            // The same decay, decaying darker: for a steel-string
            // acoustic that needs it.
            "Dark Plate" => vec![
                space(
                    "Lustrous",
                    A::Plate,
                    1.9,
                    12.0,
                    0.6,
                    0.55,
                    1.0,
                    0.2,
                    vec![shelf(-4.0)],
                    dark(2.0, -6.0),
                ),
                space(
                    "Warm Plate",
                    A::Plate,
                    2.2,
                    15.0,
                    0.65,
                    0.65,
                    1.0,
                    0.2,
                    vec![shelf(-6.0)],
                    dark(3.0, -9.0),
                ),
                space(
                    "Velvet Plate",
                    A::Velvet,
                    1.8,
                    12.0,
                    0.6,
                    0.5,
                    1.0,
                    0.2,
                    vec![shelf(-3.0)],
                    vec![],
                ),
            ],
            // A different kind of tail — and a decay that can be
            // darkened band by band without getting shorter.
            "Gold Plate" => vec![
                space(
                    "Gold Plate",
                    A::Plate,
                    2.0,
                    10.0,
                    0.7,
                    0.35,
                    1.0,
                    0.2,
                    vec![],
                    dark(0.0, -3.0),
                ),
                space(
                    "Tai Chi",
                    A::Bloom,
                    2.2,
                    10.0,
                    0.7,
                    0.4,
                    1.0,
                    0.2,
                    vec![],
                    dark(0.0, -6.0),
                ),
                space(
                    "Tai Chi Air",
                    A::Bloom,
                    2.2,
                    10.0,
                    0.7,
                    0.2,
                    1.0,
                    0.2,
                    vec![shelf(2.0)],
                    dark(0.0, 3.0),
                ),
            ],
            // The 480's large hall: three seconds, no pre-delay, the
            // whole thing set further away.
            "Large Hall" => vec![
                space(
                    "Large Hall",
                    A::Hall,
                    3.0,
                    0.0,
                    0.8,
                    0.45,
                    0.85,
                    0.18,
                    vec![shelf(-3.0)],
                    dark(2.0, -4.0),
                ),
                space(
                    "Short Hall",
                    A::Hall,
                    2.0,
                    0.0,
                    0.7,
                    0.45,
                    0.85,
                    0.18,
                    vec![shelf(-2.0)],
                    vec![],
                ),
                space(
                    "Distant Hall",
                    A::Hall,
                    3.2,
                    0.0,
                    0.9,
                    0.6,
                    0.85,
                    0.18,
                    vec![
                        shelf(-8.0),
                        band(1, 4_000.0, -6.0, 0.7, EqBandShape::HighShelf),
                    ],
                    dark(3.0, -9.0),
                ),
            ],
            // Brighter, and long: a sustaining wall of warmth under the
            // instrument, ten seconds if it wants to be.
            "Vienna" => vec![
                space(
                    "Vienna Hall",
                    A::Hall,
                    4.0,
                    20.0,
                    0.9,
                    0.35,
                    0.9,
                    0.16,
                    vec![],
                    dark(2.0, 0.0),
                ),
                space(
                    "Vienna Long",
                    A::Hall,
                    10.0,
                    20.0,
                    1.0,
                    0.4,
                    0.9,
                    0.14,
                    vec![shelf(-3.0)],
                    dark(3.0, -3.0),
                ),
                space(
                    "Edgy Hall",
                    A::Cloud,
                    3.5,
                    20.0,
                    0.9,
                    0.2,
                    0.9,
                    0.16,
                    vec![shelf(3.0)],
                    dark(-3.0, 3.0),
                ),
            ],
            // Really long, and a low-pass on the way back: a synth bed
            // under the instrument rather than a room around it.
            "Atmosphere" => vec![
                space(
                    "Atmosphere",
                    A::Cloud,
                    14.0,
                    40.0,
                    1.0,
                    0.5,
                    1.0,
                    0.2,
                    vec![band(1, 3_000.0, -9.0, 0.7, EqBandShape::HighShelf)],
                    dark(0.0, -6.0),
                ),
                space(
                    "Swell Hall",
                    A::Swell,
                    8.0,
                    80.0,
                    1.0,
                    0.5,
                    1.0,
                    0.22,
                    vec![shelf(-4.0)],
                    dark(0.0, -6.0),
                ),
                space(
                    "Long Choir",
                    A::Chorale,
                    9.0,
                    60.0,
                    1.0,
                    0.55,
                    1.0,
                    0.2,
                    vec![shelf(-6.0)],
                    dark(2.0, -6.0),
                ),
            ],
            // A spring that is not a model of one box: any decay, from
            // clean through a gritty combo to driven.
            "Big Sky" => vec![
                space(
                    "Combo Spring",
                    A::Spring,
                    2.0,
                    0.0,
                    0.5,
                    0.4,
                    0.5,
                    0.25,
                    vec![band(0, 2_000.0, 2.0, 1.0, EqBandShape::Bell)],
                    vec![],
                ),
                space(
                    "Clean Spring",
                    A::Spring,
                    2.5,
                    0.0,
                    0.5,
                    0.3,
                    0.5,
                    0.25,
                    vec![],
                    vec![],
                ),
                space(
                    "Dirty Spring",
                    A::Spring,
                    1.6,
                    0.0,
                    0.5,
                    0.5,
                    0.4,
                    0.25,
                    vec![band(0, 1_500.0, 4.0, 1.2, EqBandShape::Bell), shelf(-4.0)],
                    vec![],
                ),
                space(
                    "Long Spring",
                    A::Spring,
                    4.0,
                    0.0,
                    0.6,
                    0.4,
                    0.5,
                    0.22,
                    vec![],
                    vec![],
                ),
            ],
            // The classic tank: the newer model warm and lush, the
            // vintage one shorter and brighter, and 2 kHz for anger.
            _ => vec![
                space(
                    "XL35",
                    A::Spring,
                    2.2,
                    0.0,
                    0.5,
                    0.45,
                    0.5,
                    0.25,
                    vec![],
                    vec![],
                ),
                space(
                    "XL35 Vintage",
                    A::Spring,
                    1.5,
                    0.0,
                    0.4,
                    0.3,
                    0.4,
                    0.25,
                    vec![shelf(2.0)],
                    vec![],
                ),
                space(
                    "XL35 Angry",
                    A::Spring,
                    2.2,
                    0.0,
                    0.5,
                    0.45,
                    0.5,
                    0.25,
                    vec![band(0, 2_000.0, 5.0, 1.5, EqBandShape::Bell)],
                    vec![],
                ),
                space(
                    "Air Spring",
                    A::Spring,
                    2.8,
                    0.0,
                    0.6,
                    0.6,
                    0.4,
                    0.25,
                    vec![band(0, 800.0, 3.0, 0.8, EqBandShape::Bell), shelf(-6.0)],
                    vec![],
                ),
            ],
        };
    }
    if lower.contains("brick") {
        return vec![
            // The 480's brick wall: two hundred and forty milliseconds
            // with the early reflections lopsided, for motion.
            space(
                "Brick Wall",
                A::Reflections,
                0.24,
                0.0,
                0.3,
                0.2,
                0.3,
                0.3,
                vec![],
                vec![],
            ),
            space(
                "Sidewall Slap",
                A::Reflections,
                0.3,
                8.0,
                0.4,
                0.2,
                0.2,
                0.3,
                vec![shelf(2.0)],
                vec![],
            ),
            space(
                "Lopsided",
                A::Reflections,
                0.35,
                12.0,
                0.5,
                0.3,
                0.4,
                0.3,
                vec![],
                vec![],
            ),
        ];
    }
    match slot_of(name, REVERB_SLOTS) {
        // The snare's plate: bright and quick, the crack carried on a
        // sheet of steel rather than put in a room.
        "Plate" => vec![
            space(
                "Snare Plate",
                A::Plate,
                1.2,
                10.0,
                0.45,
                0.2,
                0.9,
                0.2,
                vec![shelf(1.5)],
                vec![],
            ),
            space(
                "Bright Plate",
                A::Plate,
                1.0,
                5.0,
                0.4,
                0.1,
                0.9,
                0.2,
                vec![shelf(3.0)],
                vec![],
            ),
            space(
                "Dark Plate",
                A::Plate,
                1.6,
                15.0,
                0.5,
                0.55,
                0.9,
                0.2,
                vec![shelf(-4.0)],
                dark(-2.0, -6.0),
            ),
        ],
        // The snare's hall: the ballad's backbeat, two seconds of air
        // with the top rolled off so the crack stays in front.
        "Hall" => vec![
            space(
                "Snare Hall",
                A::Hall,
                2.0,
                20.0,
                0.7,
                0.45,
                0.8,
                0.2,
                vec![shelf(-3.0)],
                dark(-3.0, -6.0),
            ),
            space(
                "Small Hall",
                A::Hall,
                1.4,
                12.0,
                0.55,
                0.35,
                0.8,
                0.2,
                vec![],
                vec![],
            ),
            space(
                "Dark Hall",
                A::Hall,
                2.6,
                30.0,
                0.8,
                0.7,
                0.8,
                0.18,
                vec![shelf(-6.0)],
                dark(-6.0, -12.0),
            ),
        ],
        "Room" => vec![
            space(
                "Small Room",
                A::Room,
                0.6,
                8.0,
                0.35,
                0.15,
                0.6,
                0.2,
                vec![shelf(1.5)],
                vec![],
            ),
            space(
                "Wood Room",
                A::Room,
                0.8,
                10.0,
                0.45,
                0.4,
                0.7,
                0.2,
                vec![shelf(-2.0)],
                vec![],
            ),
            space(
                "Reflections",
                A::Reflections,
                0.5,
                4.0,
                0.3,
                0.2,
                0.4,
                0.22,
                vec![],
                vec![],
            ),
            space(
                "Tight Plate",
                A::Plate,
                0.8,
                5.0,
                0.3,
                0.25,
                0.9,
                0.18,
                vec![shelf(1.0)],
                vec![],
            ),
            space(
                "Velvet",
                A::Velvet,
                0.7,
                6.0,
                0.4,
                0.3,
                1.0,
                0.2,
                vec![],
                vec![],
            ),
        ],
        "Long" => vec![
            // A cloud first: the long one is a wash, not a building.
            space(
                "Cloud 4.0",
                A::Cloud,
                4.0,
                50.0,
                0.85,
                0.6,
                0.9,
                0.2,
                vec![shelf(-5.0)],
                dark(-8.0, -8.0),
            ),
            space(
                "Hall 2.6",
                A::Hall,
                2.6,
                40.0,
                0.7,
                0.5,
                0.8,
                0.18,
                vec![shelf(-4.0)],
                dark(-6.0, -6.0),
            ),
            space(
                "Dark Hall",
                A::Hall,
                3.0,
                50.0,
                0.8,
                0.7,
                0.8,
                0.18,
                vec![shelf(-8.0)],
                dark(-6.0, -12.0),
            ),
            space(
                "Magneto",
                A::Magneto,
                2.4,
                30.0,
                0.6,
                0.5,
                0.9,
                0.2,
                vec![shelf(-5.0)],
                dark(-4.0, -6.0),
            ),
            space(
                "Chorale",
                A::Chorale,
                3.2,
                40.0,
                0.75,
                0.45,
                0.9,
                0.18,
                vec![shelf(-4.0)],
                dark(-6.0, -4.0),
            ),
            space(
                "Random Space",
                A::Random,
                2.8,
                35.0,
                0.7,
                0.5,
                0.85,
                0.18,
                vec![shelf(-5.0)],
                dark(-6.0, -6.0),
            ),
        ],
        "Moment" => vec![
            space(
                "Cloud",
                A::Cloud,
                5.5,
                60.0,
                0.9,
                0.65,
                0.9,
                0.25,
                vec![shelf(-7.0)],
                dark(-9.0, -9.0),
            ),
            space(
                "Bloom",
                A::Bloom,
                5.0,
                80.0,
                0.9,
                0.6,
                1.0,
                0.25,
                vec![shelf(-6.0)],
                dark(-9.0, -6.0),
            ),
            space(
                "Swell",
                A::Swell,
                6.0,
                100.0,
                0.95,
                0.6,
                1.0,
                0.25,
                vec![shelf(-6.0)],
                dark(-9.0, -8.0),
            ),
            space(
                "Convolution",
                A::Convolution,
                4.5,
                40.0,
                0.8,
                0.5,
                1.0,
                0.22,
                vec![shelf(-5.0)],
                dark(-6.0, -6.0),
            ),
            space(
                "Velvet Wash",
                A::Velvet,
                5.0,
                60.0,
                0.9,
                0.7,
                1.0,
                0.25,
                vec![shelf(-8.0)],
                dark(-9.0, -12.0),
            ),
        ],
        "Throw" => vec![
            space(
                "Shimmer",
                A::Shimmer,
                8.0,
                90.0,
                1.0,
                0.7,
                1.0,
                0.35,
                vec![shelf(-9.0), band(1, 2_500.0, 3.0, 1.2, EqBandShape::Bell)],
                vec![band(0, 2_000.0, 6.0, 1.0, EqBandShape::Bell)],
            ),
            space(
                "Dark Shimmer",
                A::Shimmer,
                6.0,
                90.0,
                1.0,
                0.85,
                1.0,
                0.3,
                vec![shelf(-12.0)],
                vec![band(0, 1_200.0, 4.0, 1.0, EqBandShape::Bell)],
            ),
            space(
                "Bloom Throw",
                A::Bloom,
                7.0,
                120.0,
                1.0,
                0.6,
                1.0,
                0.35,
                vec![shelf(-7.0)],
                dark(-9.0, -6.0),
            ),
            space(
                "Cloud Throw",
                A::Cloud,
                9.0,
                100.0,
                1.0,
                0.7,
                1.0,
                0.35,
                vec![shelf(-9.0)],
                dark(-12.0, -9.0),
            ),
            space(
                "Chorale Throw",
                A::Chorale,
                7.0,
                80.0,
                1.0,
                0.5,
                1.0,
                0.3,
                vec![shelf(-6.0)],
                dark(-9.0, -4.0),
            ),
        ],
        _ => vec![
            space(
                "Plate 1.4",
                A::Plate,
                1.4,
                20.0,
                0.5,
                0.3,
                0.8,
                0.2,
                vec![shelf(-1.5)],
                vec![],
            ),
            space(
                "Bright Plate",
                A::Plate,
                1.2,
                15.0,
                0.45,
                0.15,
                0.9,
                0.2,
                vec![shelf(2.0)],
                vec![],
            ),
            space(
                "Spring",
                A::Spring,
                1.6,
                10.0,
                0.4,
                0.4,
                0.5,
                0.2,
                vec![shelf(-3.0)],
                vec![],
            ),
            space(
                "Non-Linear",
                A::NonLinear,
                1.0,
                10.0,
                0.5,
                0.3,
                0.9,
                0.22,
                vec![],
                vec![],
            ),
            space(
                "FreeVerb",
                A::FreeVerb,
                1.5,
                20.0,
                0.5,
                0.35,
                0.7,
                0.2,
                vec![shelf(-2.0)],
                vec![],
            ),
        ],
    }
}

/// A parallel compressor from its numbers, with what shapes its return.
fn squash(
    name: &str,
    threshold: f32,
    ratio: f32,
    attack: f32,
    release: f32,
    drive: f32,
    eq: Vec<EqBand>,
) -> Preset {
    let mut t = placeholder(0);
    t.role = Role::Parallel;
    t.comp = Comp {
        threshold,
        ratio,
        attack,
        release,
        ..Comp::default()
    };
    t.eq = eq;
    t.sat.drive = drive;
    preset(name, t)
}

/// The curated presets for one parallel compressor, by what it is for:
/// tight holds the kit together, punch lets the transient through and
/// grabs what follows, smash is the crushed room under everything, and
/// crunch is smash driven into a transformer.
fn parallel_presets(name: &str) -> Vec<Preset> {
    let lower = name.to_lowercase();
    if lower.contains("punch") {
        vec![
            squash(
                "Punch",
                -24.0,
                4.0,
                30.0,
                80.0,
                1.0,
                vec![band(0, 80.0, 2.0, 0.7, EqBandShape::LowShelf)],
            ),
            squash("Slow Grab", -20.0, 6.0, 50.0, 120.0, 1.0, vec![]),
            squash(
                "Snap",
                -26.0,
                3.0,
                20.0,
                60.0,
                1.2,
                vec![band(0, 3_000.0, 2.0, 1.0, EqBandShape::Bell)],
            ),
        ]
    } else if lower.contains("smash") {
        vec![
            squash(
                "Smash",
                -40.0,
                20.0,
                0.5,
                60.0,
                1.0,
                vec![
                    band(0, 100.0, -3.0, 0.7, EqBandShape::LowShelf),
                    band(1, 8_000.0, -3.0, 0.7, EqBandShape::HighShelf),
                ],
            ),
            squash(
                "All Buttons",
                -45.0,
                20.0,
                0.2,
                40.0,
                1.0,
                vec![band(0, 120.0, -4.0, 0.7, EqBandShape::LowShelf)],
            ),
            squash("Pumping", -36.0, 12.0, 1.0, 200.0, 1.0, vec![]),
        ]
    } else if lower.contains("crunch") {
        vec![
            squash(
                "Crunch",
                -36.0,
                10.0,
                1.0,
                80.0,
                3.5,
                vec![
                    band(0, 150.0, -6.0, 0.7, EqBandShape::LowShelf),
                    band(1, 2_500.0, 3.0, 1.0, EqBandShape::Bell),
                ],
            ),
            squash(
                "Transformer",
                -30.0,
                8.0,
                3.0,
                100.0,
                2.5,
                vec![band(0, 120.0, -3.0, 0.7, EqBandShape::LowShelf)],
            ),
            squash(
                "Fuzz",
                -40.0,
                20.0,
                0.3,
                50.0,
                6.0,
                vec![
                    band(0, 200.0, -9.0, 0.7, EqBandShape::LowShelf),
                    band(1, 6_000.0, -6.0, 0.7, EqBandShape::HighShelf),
                ],
            ),
        ]
    } else {
        vec![
            squash("Tight", -22.0, 4.0, 3.0, 100.0, 1.0, vec![]),
            squash("Glue", -18.0, 2.5, 10.0, 200.0, 1.0, vec![]),
            squash(
                "Fast Four",
                -26.0,
                4.0,
                1.0,
                60.0,
                1.0,
                vec![band(0, 60.0, -2.0, 0.7, EqBandShape::LowShelf)],
            ),
        ]
    }
}

fn wide_presets() -> Vec<Preset> {
    let widen = |name: &str, wide: f32| {
        let mut t = placeholder(2);
        t.role = Role::Wide;
        t.wide = wide;
        preset(name, t)
    };
    vec![
        widen("Subtle", 1.25),
        widen("Wide", 1.7),
        widen("Huge", 2.2),
    ]
}

/// The movement returns — chorus, flanger — as the widener until they
/// have a face: what they do to the width is the part the picture
/// can show.
fn mod_presets(name: &str) -> Vec<Preset> {
    let widen = |name: &str, wide: f32| {
        let mut t = placeholder(2);
        t.role = Role::Wide;
        t.wide = wide;
        preset(name, t)
    };
    if name.to_lowercase().contains("flang") {
        vec![widen("Flanger", 1.4), widen("MXR", 1.6), widen("Jet", 2.0)]
    } else {
        vec![
            widen("Dimension D", 1.5),
            widen("Chorus", 1.7),
            widen("Tri-Chorus", 2.0),
        ]
    }
}

/// One list per direction: an up track offers ways up, a down track
/// ways down.
fn pitch_presets(name: &str) -> Vec<Preset> {
    let shift = |name: &str, pitch: i32, mix: f32| {
        let mut t = placeholder(3);
        t.role = Role::Pitch;
        t.pitch = pitch;
        t.pitch_mix = mix;
        preset(name, t)
    };
    let lower = name.to_lowercase();
    if lower.contains('-') || lower.contains("down") || lower.contains("sub") {
        vec![
            shift("Oct-", -12, 0.4),
            shift("Oct- Soft", -12, 0.2),
            shift("Sub", -24, 0.3),
        ]
    } else {
        vec![
            shift("Oct+", 12, 0.35),
            shift("Oct+ Soft", 12, 0.18),
            shift("5th+", 7, 0.25),
        ]
    }
}

/// What a track sounds like, as far as a placeholder can know.
///
/// Taken from the track's INDEX rather than its name, for the same
/// reason [`crate::simulate`] does: it is stable across a rename and
/// works on a session whose tracks are called things this file has
/// never heard of. The point is that a rack of racks reads as a set of
/// decisions — a kick cut at 400 and lifted at 60, a cymbal rolled off
/// at the bottom and opened at the top — rather than one curve
/// repeated with a wobble in it.
///
/// It is still a placeholder. When a chain can be read, this is the
/// shape the real settings arrive in; until then it is what makes the
/// panel worth looking at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Character {
    /// Kicks, floor toms, bass — weight at the bottom and a box to cut.
    Low,
    /// Snares and racks — body, and a crack to find above it.
    Mid,
    /// Cymbals, hats, air — nothing useful below, everything above.
    High,
    /// Rooms and buses — gentle, wide, barely there.
    Broad,
}

impl Character {
    /// Which voice a track gets.
    ///
    /// Hashed rather than `track % 4`, because a period of four lines
    /// up with any regular stride — and the mixer has one: at a width
    /// where the rack is drawn, every eighth strip is on screen, every
    /// one of them ≡ 0 mod 4, and a whole desk of placeholder settings
    /// came out identical.
    const fn of(track: usize) -> Self {
        let mixed = (track ^ (track >> 2)).wrapping_mul(2_654_435_761);
        match mixed % 4 {
            0 => Self::Low,
            1 => Self::Mid,
            2 => Self::High,
            _ => Self::Broad,
        }
    }

    /// The curve this voice usually wants.
    fn bands(self, drift: f64) -> Vec<EqBand> {
        let nudge = |hz: f64| hz * 1.06_f64.powf(drift);
        match self {
            Self::Low => vec![
                band(0, nudge(55.0), 4.0, 0.8, EqBandShape::LowShelf),
                band(1, nudge(380.0), -5.5, 1.6, EqBandShape::Bell),
                band(2, nudge(3_200.0), 3.0, 1.0, EqBandShape::Bell),
                band(3, nudge(9_000.0), -2.0, 0.7, EqBandShape::HighShelf),
            ],
            Self::Mid => vec![
                band(0, nudge(90.0), -4.0, 0.7, EqBandShape::LowShelf),
                band(1, nudge(220.0), 2.5, 1.2, EqBandShape::Bell),
                band(2, nudge(1_100.0), -3.0, 2.0, EqBandShape::Bell),
                band(3, nudge(6_500.0), 4.5, 0.8, EqBandShape::HighShelf),
            ],
            Self::High => vec![
                band(0, nudge(300.0), -7.0, 0.6, EqBandShape::LowShelf),
                band(1, nudge(900.0), -2.5, 1.4, EqBandShape::Bell),
                band(2, nudge(5_000.0), 1.5, 1.1, EqBandShape::Bell),
                band(3, nudge(12_000.0), 4.0, 0.7, EqBandShape::HighShelf),
            ],
            Self::Broad => vec![
                band(0, nudge(70.0), -2.0, 0.7, EqBandShape::LowShelf),
                band(1, nudge(500.0), -1.5, 0.9, EqBandShape::Bell),
                band(2, nudge(2_500.0), 1.0, 0.8, EqBandShape::Bell),
                band(3, nudge(11_000.0), 2.0, 0.7, EqBandShape::HighShelf),
            ],
        }
    }

    /// And the compression. The times are the decision: a kick wants
    /// the transient through and a room wants none of it.
    fn comp(self, drift: f64) -> Comp {
        let (threshold, ratio, attack, release) = match self {
            Self::Low => (-12.0, 4.0, 12.0, 140.0),
            Self::Mid => (-16.0, 5.0, 4.0, 90.0),
            Self::High => (-20.0, 2.5, 1.0, 200.0),
            Self::Broad => (-24.0, 2.0, 25.0, 400.0),
        };
        Comp {
            threshold: f64_to_f32(threshold + drift),
            ratio: f64_to_f32((ratio + drift * 0.3).clamp(1.0, 20.0)),
            knee: 6.0,
            attack: f64_to_f32((attack * 1.15_f64.powf(drift)).clamp(0.1, 200.0)),
            release: f64_to_f32((release * 1.12_f64.powf(drift)).clamp(5.0, 3_000.0)),
        }
    }

    /// Which saturator this voice runs through, by profile id.
    const fn sat_profile(self) -> &'static str {
        match self {
            Self::Low => "transformer",
            Self::Mid => "triode",
            Self::High => "tape",
            Self::Broad => "transistor",
        }
    }

    /// Which delay machine this voice would be sent to. Varied so a
    /// mixer of racks shows the family faces rather than one repeated.
    const fn delay_style(self) -> DelayStyle {
        match self {
            Self::Low => DelayStyle::Clean,
            Self::Mid => DelayStyle::Tape,
            Self::High => DelayStyle::Bbd,
            Self::Broad => DelayStyle::Shimmer,
        }
    }

    /// And which space.
    const fn reverb(self) -> AlgorithmType {
        match self {
            Self::Low => AlgorithmType::Room,
            Self::Mid => AlgorithmType::Plate,
            Self::High => AlgorithmType::Hall,
            Self::Broad => AlgorithmType::Cloud,
        }
    }

    const fn drive(self) -> f64 {
        match self {
            Self::Low => 3.4,
            Self::Mid => 2.6,
            Self::High => 1.6,
            Self::Broad => 2.0,
        }
    }
}

/// Something in the rack you can take hold of.
///
/// Not a control in the strip's sense: these live inside a curve's own
/// axes, so what a drag means depends on which curve it started in. A
/// band moves in frequency AND gain at once, which is the gesture an
/// EQ is actually used with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Grip {
    /// An EQ band: which EQ, and its index in that one's list.
    ///
    /// The chain has three EQs now — rescue, tone and relational — so a
    /// grip that named only an index would move whichever one the
    /// dispatch happened to find first.
    Band(Which, usize),
    /// A threshold — the red line across a display, dragged up and down
    /// the level axis it is drawn on. Both compressors have one, so
    /// does the gate, and so do the two suppressors.
    Threshold(Which),
    /// A compressor's ratio.
    Ratio(Which),
    /// Its attack.
    Attack(Which),
    /// And its release.
    Release(Which),
    /// The gate's hold — the flat top of its table.
    Hold(Which),
    /// The gate's range — the fainter line under its threshold, how far
    /// down what stays shut goes.
    Range,
    /// The saturator's drive.
    Drive,
    /// Its bias — the operating point, dragged sideways so the curve
    /// leans.
    Bias,
    /// Its tilt — which end of the spectrum meets the knee first, the
    /// wedge under the ladder.
    Tilt,
    /// A suppressor's depth — the ribbon, pulled down for more.
    Depth(Which),
    /// A suppressor's sharpness — how wide the reference it compares
    /// against is.
    Sharpness(Which),
    /// One edge of a suppressor's band.
    Edge(Which, Side),
    /// The delay's time — the first repeat, dragged sideways.
    Time,
    /// Its feedback — any later repeat, dragged up and down.
    Feedback,
    /// The reverb's decay — the tail's reach.
    Decay,
    /// Its predelay — the gap rule between the impulse and the tail.
    Predelay,
    /// A wet/dry mix: the delay's, the reverb's, the saturator's.
    Mix(Which),
    /// One chip of a unit's machine selector — clicked to choose that
    /// family. A switch, like the glyph.
    Choose(Which, usize),
    /// One of the track's presets — clicked to load it. A switch.
    Preset(usize),
    /// One knob of a unit's knob row (or the widener's and the pitch
    /// shifter's single figures), dragged up and down.
    Knob(Which, usize),
    /// A unit's machine glyph — clicked to cycle to the next style or
    /// algorithm. A switch, like a bypass: it acts on the click.
    Family(Which),
    /// A panel's header — clicked to switch that processor out.
    ///
    /// The header, because it is the one strip of a panel that is not
    /// a control: everything else in there sets a value, and a bypass
    /// is not a value. It is also the part that stays legible under the
    /// scrim, so the way out is where the way in was.
    Bypass(Which),
    /// A phase's container bar — clicked to fold it shut.
    ///
    /// Not a `Bypass`: folding changes what you can SEE and bypassing
    /// changes what the track sounds like, and a rack where the two
    /// gestures looked alike would be one click away from an unintended
    /// mix decision.
    Phase(session::mix_phases::MixPhase),
    /// An EQ graph's zoom — the chip at the top of the panel saying
    /// what range it is drawn to.
    ///
    /// A zoom is not a parameter: it changes nothing about the sound,
    /// only how much of it you can see. It is here because it is a
    /// thing in the panel you point at, and everything in a panel you
    /// point at has to be something the one hit test can name.
    Scale(Which),
    /// The expand mark at the right of a panel's header: the panel,
    /// full screen (`crate::closeup`). Not a parameter either — it
    /// changes where you are looking, not what you hear.
    Zoom(Which),
}

impl Grip {
    /// Whether this grip is one of the compressor's knobs.
    ///
    /// They share a drag law — a knob is a knob — and differ only in
    /// the range they map onto, which is the one thing each has to say
    /// for itself.
    #[must_use]
    pub const fn is_knob(self) -> bool {
        matches!(self, Self::Ratio(_) | Self::Attack(_) | Self::Release(_))
    }

    /// Whether this grip is a switch rather than a value.
    ///
    /// A switch acts on the CLICK and has no drag; a value does the
    /// opposite. The caller needs to know which before it decides
    /// whether a press is the start of a gesture.
    #[must_use]
    pub const fn is_switch(self) -> bool {
        matches!(
            self,
            Self::Bypass(_)
                | Self::Scale(_)
                | Self::Phase(_)
                | Self::Family(_)
                | Self::Choose(..)
                | Self::Preset(_)
                | Self::Zoom(_)
        )
    }

    /// Which panel this grip lives in.
    #[must_use]
    pub const fn panel(self) -> Which {
        match self {
            Self::Band(which, _)
            | Self::Threshold(which)
            | Self::Ratio(which)
            | Self::Attack(which)
            | Self::Release(which)
            | Self::Hold(which)
            | Self::Depth(which)
            | Self::Sharpness(which)
            | Self::Edge(which, _)
            | Self::Mix(which)
            | Self::Family(which)
            | Self::Choose(which, _)
            | Self::Knob(which, _)
            | Self::Scale(which)
            | Self::Zoom(which)
            | Self::Bypass(which) => which,
            Self::Drive | Self::Bias | Self::Tilt => Which::Sat,
            Self::Range => Which::Gate,
            Self::Preset(_) => Which::Presets,
            Self::Time | Self::Feedback => Which::Delay,
            Self::Decay | Self::Predelay => Which::Reverb,
            // A header belongs to no unit: it caps a run of them.
            Self::Phase(_) => Which::Eq,
        }
    }
}

/// How close a pointer has to be to take hold of something.
///
/// Generous next to [`HANDLE`], because a handle is drawn at the size
/// it reads best and grabbed at the size a hand can hit. Six pixels is
/// about a millimetre at these densities.
const GRAB: f64 = 6.0;

/// A box in panel coordinates.
///
/// Small enough not to want `kurbo::Rect`'s arithmetic, and stated here
/// so the chip's drawing and its hit test are the same four numbers.
#[derive(Clone, Copy, Debug)]
pub struct Chip {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Chip {
    #[must_use]
    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// Where the EQ's zoom readout sits: centred at the top of the graph.
///
/// The top MIDDLE because the corners are taken — the header's name is
/// at one end of the panel and its summary at the other — and because
/// the middle of the top edge is the one part of a response curve that
/// is reliably empty: bands live out along the frequency axis and the
/// curve itself runs through the middle height.
#[must_use]
pub fn scale_chip(body: Panel) -> Chip {
    const WIDTH: f64 = 30.0;
    const HEIGHT: f64 = 12.0;
    Chip {
        x: body.x + (body.width - WIDTH) / 2.0,
        y: body.y + 1.0,
        width: WIDTH.min(body.width),
        height: HEIGHT.min(body.height),
    }
}

/// The EQ panel's axes, as the plugin's own interaction model wants
/// them.
///
/// `GraphMapper` takes ONE padding for both axes because a plugin's
/// graph is a box inside a window; a rack panel is a box at an
/// arbitrary offset inside a strip. So the mapper is built at the
/// origin and the panel's corner is added back — which keeps every
/// conversion the plugin's, and leaves this the only place the two
/// coordinate systems meet.
fn mapper(body: Panel, db_range: f64) -> GraphMapper {
    GraphMapper::new(20.0, 20_000.0, db_range, body.width, body.height, 0.0)
}

/// What is under a point in the rack, if anything.
///
/// `panel` is the rack's whole box in the same coordinates as `x` and
/// `y` — the strip's, not the window's.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "a hit test and everything it reads"
)]
pub fn grip_at(
    panels: &[Which],
    tone: &Tone,
    panel: Panel,
    folded: Folded,
    x: f64,
    y: f64,
) -> Option<Grip> {
    let rack = Rack::at(panel.width);
    // A rail's indicators are read, not touched.
    if !rack.on() || rack == Rack::Minimal {
        return None;
    }
    for (row, at) in chain(panels, panel, folded) {
        // A container bar answers for its whole width: it is fifteen
        // pixels tall and the chevron in it is six, and a fold you have
        // to hit exactly is a fold nobody uses.
        let Row::Unit(which) = row else {
            let Row::Head(phase) = row else { continue };
            if y >= at.y && y < at.y + at.height {
                return Some(Grip::Phase(phase));
            }
            continue;
        };
        let body = body_of(at, rack);
        if body.width <= 0.0 || body.height <= 0.0 {
            continue;
        }
        // The preset row has no header and no body: it is chips.
        if which == Which::Presets {
            if y < at.y || y >= at.y + at.height {
                continue;
            }
            return preset_chip_at(tone, at, x).map(Grip::Preset);
        }
        // The header first: it sits above the body, and a click there
        // is a bypass rather than whatever the body would have done —
        // except on the machine glyph, which cycles the machine.
        if y >= at.y && y < body.y {
            let inner = at.inset(2.0);
            // Wider than the mark: it is ten pixels, and the corner
            // it sits in is the target, so a finger finds it.
            if rack.detailed()
                && !folded.is_zoomed()
                && x >= inner.x + inner.width - EXPAND_W - 14.0
            {
                return Some(Grip::Zoom(which));
            }
            if rack.detailed() && which.glyph_switches() && x >= inner.x && x < inner.x + GLYPH_W {
                return Some(Grip::Family(which));
            }
            return Some(Grip::Bypass(which));
        }
        if y < body.y || y > body.y + body.height {
            continue;
        }
        // A bypassed panel has one control left, and it is the one that
        // brings it back. Grabbing a band you cannot see through the
        // scrim would move a setting with no effect.
        if tone.bypass.is(which) {
            return Some(Grip::Bypass(which));
        }
        // Routed by what the panel IS, not by which one it is: three
        // EQs share a hit test and two compressors share another, and
        // the grip carries the panel so the drag knows which.
        match which {
            // Bands are only grabbable where they are DRAWN, which is
            // only at `Full` — a handle you cannot see is a handle you
            // cannot aim at, and grabbing one by accident moves a
            // setting you did not know was there.
            _ if which.is_spectral() && !rack.detailed() => {}
            Which::RescueEq
            | Which::Eq
            | Which::Space
            | Which::PreEq
            | Which::PostEq
            | Which::DecayEq
            | Which::PolishEq => {
                // The zoom chip first: it is small, it sits over the
                // graph, and a band that happened to be under it would
                // otherwise take every click aimed at it.
                if scale_chip(body).contains(x, y) {
                    return Some(Grip::Scale(which));
                }
                // The plugin's own hit test, not a second one written
                // here: `nearest_band` already decides which of four
                // overlapping bands you meant, and a rack that decided
                // differently from the editor would be two EQs.
                let bands = tone.bands_ref(which);
                if let Some((index, _)) = interaction::nearest_band(
                    bands,
                    mapper(body, tone.eq_db_range()),
                    x - body.x,
                    y - body.y,
                    GRAB,
                ) {
                    return Some(Grip::Band(which, index));
                }
                // And nothing else. The empty graph used to answer for
                // the zoom so a wheel anywhere over it would zoom —
                // but the rack scrolls now, and a panel that swallowed
                // the wheel would be a hole in the scroll the size of
                // an EQ. The chip is the zoom's target; the rest of the
                // graph belongs to the gesture that moves the chain.
            }
            // A suppressor: the band's edges where they are drawn, then
            // the display in three bands of its own — the reference up
            // top is the threshold, the ribbon through the middle is
            // the depth, the floor is the sharpness. Zones rather than
            // curves, because the curves move with the audio and a
            // grip that moved with the audio would be a grip you could
            // not aim at.
            Which::DeEss | Which::DeEssIn => {
                return Some(suppress_grip(tone, which, body, rack, x, y));
            }
            Which::Comp => return Some(comp_grip(tone.comp, which, body, rack, x, y)),
            Which::RescueComp => {
                return Some(comp_grip(tone.rescue_comp, which, body, rack, x, y));
            }
            Which::Gate => return Some(gate_grip(tone.gate, body, rack, x, y)),
            Which::Knobs => return Some(Grip::Knob(which, knob_at(body, x))),
            Which::Wide => return Some(Grip::Knob(which, 0)),
            // The interval on the top half, the mix on the bottom.
            Which::Pitch => {
                let index = usize::from(y >= body.y + body.height / 2.0);
                return Some(Grip::Knob(which, index));
            }
            // Its controls are the FX view's face's.
            Which::Presets | Which::Mod => {}
            // The ladder is the tilt; the curve's centre is the bias;
            // the rest of the curve is the drive.
            Which::Sat => {
                if lane_of(body, which, rack).is_some_and(|strip| strip.contains(x, y)) {
                    return Some(selector_grip(which, body, rack, x));
                }
                let (curve_box, ladder_box) = sat_split(display_of(body, which, rack), rack);
                if ladder_box.is_some_and(|lb| lb.contains(x, y)) {
                    return Some(Grip::Tilt);
                }
                let mid_x = curve_box.x + curve_box.width / 2.0;
                if rack.detailed() && (x - mid_x).abs() <= GRAB * 1.5 {
                    return Some(Grip::Bias);
                }
                return Some(Grip::Drive);
            }
            // The first repeat is the time; any later one is the
            // feedback. Elsewhere on the panel, the time — it is the
            // parameter a delay IS.
            Which::Delay | Which::Reverb
                if lane_of(body, which, rack).is_some_and(|strip| strip.contains(x, y)) =>
            {
                return Some(selector_grip(which, body, rack, x));
            }
            Which::Delay => {
                let body = display_of(body, which, rack);
                let taps = echo_taps(tone.delay, body);
                let nearest = taps
                    .iter()
                    .enumerate()
                    .map(|(k, (tx, _))| (k, (x - tx).abs()))
                    .filter(|(_, away)| *away <= GRAB)
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                return Some(match nearest {
                    Some((k, _)) if k >= 2 => Grip::Feedback,
                    _ => Grip::Time,
                });
            }
            // The gap rule is the predelay, the tail's end is the decay,
            // and the tail's height is the mix.
            Which::Reverb => {
                let body = display_of(body, which, rack);
                let geometry = RoomGeometry::of(tone.reverb, body);
                if rack.detailed() && (x - geometry.start).abs() <= GRAB {
                    return Some(Grip::Predelay);
                }
                if x >= body.width.mul_add(0.6, body.x) {
                    return Some(Grip::Decay);
                }
                return Some(Grip::Mix(which));
            }
        }
    }
    None
}

/// What is under a point in a machine selector strip: the chip, or the
/// bypass past the chips (the strip is part of the panel, and the
/// panel's empty space is the bypass everywhere else).
fn selector_grip(which: Which, body: Panel, rack: Rack, x: f64) -> Grip {
    let strip = lane_of(body, which, rack).unwrap_or(body);
    let chips = match which {
        Which::Sat => CIRCUITS.len(),
        Which::Delay => DelayFamily::ALL.len(),
        _ => RoomFamily::ALL.len(),
    };
    let index = crate::num::index(((x - strip.x - 2.0) / CHIP).floor());
    if index < chips && x >= strip.x + 2.0 {
        Grip::Choose(which, index)
    } else {
        Grip::Bypass(which)
    }
}

/// What is under a point in the gate's panel.
///
/// The glyph's edges first, then the range line, then the threshold —
/// which is everything else, the way a fader's groove is a fader's.
fn gate_grip(gate: Gate, body: Panel, rack: Rack, x: f64, y: f64) -> Grip {
    let display = display_of(body, Which::Gate, rack);
    if !rack.detailed() {
        return Grip::Threshold(Which::Gate);
    }
    let to_y = |db: f64| display.y + comp_ui::comp_graph_svg::db_to_y(db, display.height);
    let bottom = display.y + display.height;
    let line = to_y(f64::from(gate.threshold)).clamp(display.y, bottom);
    let floor = to_y(f64::from(gate.threshold + gate.range)).clamp(display.y, bottom);
    if let Some(shape) = GateGlyph::of(gate, display, line, floor)
        && let Some(grip) = shape.grip_at(x, y)
    {
        return grip;
    }
    // The range line, only where it is clear of the threshold's — two
    // lines a pixel apart are one line, and that one is the threshold.
    if (y - floor).abs() <= GRAB && floor - line > GRAB * 2.0 {
        return Grip::Range;
    }
    Grip::Threshold(Which::Gate)
}

/// What is under a point in a suppressor's panel.
fn suppress_grip(tone: &Tone, which: Which, body: Panel, rack: Rack, x: f64, y: f64) -> Grip {
    let set = tone.de_ess;
    let display = display_of(body, which, rack);
    // The band strip over the display: an edge where one is drawn,
    // else the sharpness — how wide a neighbourhood the band is judged
    // against, which is a fact about the band.
    if rack.detailed()
        && let Some(strip) = lane_of(body, which, rack)
        && y < strip.y + strip.height + 2.0
    {
        let zoom = SuppressZoom::top();
        for (hz, side) in [
            (f64::from(set.low), Side::Low),
            (f64::from(set.high), Side::High),
        ] {
            if (x - zoom.x_of(hz, strip)).abs() <= GRAB {
                return Grip::Edge(which, side);
            }
        }
        return Grip::Sharpness(which);
    }
    // The display: the threshold over most of it — it is the line you
    // are moving — and the depth along the floor.
    if y < display.y + display.height * 2.0 / 3.0 {
        Grip::Threshold(which)
    } else {
        Grip::Depth(which)
    }
}

/// The compressor's display — the whole panel.
///
/// Kept as a function because everything that reads this panel has to
/// agree about it: the drawing, the hit test, the drag's
/// pixels-to-decibels, and the level trace that lands on its axis. A
/// threshold that moved faster than the line under it is a line that is
/// not under your finger.
///
/// It used to return three columns, with the attack and release as
/// ramps in narrow gutters either side. That cost a third of the
/// panel's width, drew time on an axis nobody reads an envelope on, and
/// put the two controls away from the thing they act on — while the
/// display beside them was already showing a reduction over time, which
/// is exactly what they shape. See `envelope`.
///
/// Unless the panel is wide enough to give up a square at its right for
/// the transfer curve (`transfer_area`): then the display is what is
/// left of it.
#[must_use]
pub fn comp_split(body: Panel, _rack: Rack) -> Panel {
    match transfer_area(body) {
        // Stacked: the display is what is under the square.
        Some(square) if square.y + square.height < body.y + body.height - STRIPS_H => {
            let top = square.y + square.height + TRANSFER_GAP;
            Panel {
                y: top,
                height: (body.y + body.height - top).max(0.0),
                ..body
            }
        }
        Some(square) => Panel {
            width: (square.x - TRANSFER_GAP - body.x).max(0.0),
            ..body
        },
        None => body,
    }
}

/// The narrowest compressor panel with room for a transfer curve beside
/// its display — a close-up's, a wide editor's. A strip's is a fraction
/// of it, and there the arrow says what the curve would.
const TRANSFER_MIN_W: f64 = 380.0;
const TRANSFER_MIN_H: f64 = 180.0;
/// The narrowest tall panel that stacks one.
const TRANSFER_MIN_TALL_W: f64 = 260.0;
/// Between the display and the curve.
const TRANSFER_GAP: f64 = 10.0;

/// The transfer curve's square, at the right of a compressor panel wide
/// enough to have one: level in along the bottom, level out up the side,
/// -60 to 0 dB on both. A panel taller than it is wide — a close-up on
/// a phone or a portrait tablet — has it on top instead, and the display
/// under it, since beside it would be a stamp in a corner of a tall
/// empty column.
#[must_use]
pub fn transfer_area(body: Panel) -> Option<Panel> {
    if body.height >= body.width * 1.2 && body.width >= TRANSFER_MIN_TALL_W {
        let side = body.width.min(body.height * 0.5);
        return Some(Panel {
            x: body.x + (body.width - side) / 2.0,
            y: body.y,
            width: side,
            height: side,
        });
    }
    if body.width < TRANSFER_MIN_W || body.height < TRANSFER_MIN_H {
        return None;
    }
    let side = (body.height - STRIPS_H).min(body.width * 0.45).max(0.0);
    Some(Panel {
        x: body.x + body.width - side,
        y: body.y,
        width: side,
        height: side,
    })
}

/// The level out of a compressor for a level in, both in dBFS: straight
/// through below the knee, `1 / ratio` of the rise above it, and the
/// soft knee's curve between. The plugin's own law, so the curve drawn
/// is the reduction heard.
#[must_use]
pub fn transfer(comp: Comp, input: f64) -> f64 {
    let threshold = f64::from(comp.threshold);
    let ratio = f64::from(comp.ratio).max(1.0);
    let knee = f64::from(comp.knee).max(0.0);
    let over = input - threshold;
    if knee > 0.0 && over.abs() <= knee / 2.0 {
        let into = over + knee / 2.0;
        input + (1.0 / ratio - 1.0) * into * into / (2.0 * knee)
    } else if over > 0.0 {
        threshold + over / ratio
    } else {
        input
    }
}

/// The transfer curve in its square: the one-to-one diagonal, the curve
/// bending off it at the threshold, and the threshold's point on it.
fn transfer_curve(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    comp: Comp,
    at: Panel,
    lit: Option<Grip>,
) {
    let red = hex(comp_ui::comp_graph_svg::colors::THRESHOLD);
    let right = at.x + at.width;
    let bottom = at.y + at.height;
    // Level to position: 0 dB at the top and the right, -60 at the floor
    // and the left edge.
    let to_x = |db: f64| at.x + (db + 60.0).clamp(0.0, 60.0) / 60.0 * at.width;
    let to_y = |db: f64| bottom - (db + 60.0).clamp(0.0, 60.0) / 60.0 * at.height;
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.tcp_meter_well.multiply_alpha(0.5),
        None,
        &at.rect(),
    );
    const SIZE: f32 = 6.0;
    for db in [-12.0, -24.0, -36.0, -48.0] {
        rule(
            scene,
            palette.grid_beat,
            Line::new((to_x(db), at.y), (to_x(db), bottom)),
        );
        rule(
            scene,
            palette.grid_beat,
            Line::new((at.x, to_y(db)), (right, to_y(db))),
        );
        let label = format!("{db:.0}");
        let w = font.width(&label, SIZE);
        crate::tcp::glyphs(
            scene,
            font,
            palette.text_faint,
            &label,
            to_x(db) - w / 2.0,
            bottom - 2.0,
            SIZE,
        );
    }
    crate::tcp::glyphs(
        scene,
        font,
        palette.text_faint,
        "IN",
        right - font.width("IN", SIZE) - 3.0,
        bottom - 2.0,
        SIZE,
    );
    crate::tcp::glyphs(
        scene,
        font,
        palette.text_faint,
        "OUT",
        at.x + 3.0,
        at.y + 8.0,
        SIZE,
    );
    // One to one: what the signal would do with no compressor.
    rule(
        scene,
        palette.grid,
        Line::new((at.x, bottom), (right, at.y)),
    );
    // The threshold, where the curve leaves the diagonal.
    let threshold = f64::from(comp.threshold);
    let (tx, ty) = (to_x(threshold), to_y(threshold));
    let held = matches!(lit, Some(Grip::Threshold(_)));
    let faint = red.multiply_alpha(if held { 0.6 } else { 0.3 });
    rule(scene, faint, Line::new((tx, ty), (tx, bottom)));
    rule(scene, faint, Line::new((at.x, ty), (tx, ty)));
    const STEPS: usize = 96;
    let points = (0..=STEPS).map(|i| {
        let input = crate::num::coord(i) / crate::num::coord(STEPS) * 60.0 - 60.0;
        (to_x(input), to_y(transfer(comp, input)))
    });
    let ratio_held = matches!(lit, Some(Grip::Ratio(_)));
    curve(scene, red, points, if ratio_held { 3.0 } else { 2.0 });
    dot(
        scene,
        red,
        (tx, ty),
        if held { HANDLE + 2.4 } else { HANDLE + 1.2 },
    );
    let label = format!("{:.1}:1", comp.ratio);
    let end = (to_x(0.0), to_y(transfer(comp, 0.0)));
    crate::tcp::glyphs(
        scene,
        font,
        red,
        &label,
        end.0 - font.width(&label, SIZE) - 3.0,
        end.1 + 10.0,
        SIZE,
    );
}

/// What a press in the transfer square holds: the curve above the
/// threshold is the ratio — pulled down for more, the way the arrow is —
/// and the rest is the threshold.
fn transfer_grip(comp: Comp, which: Which, at: Panel, x: f64) -> Grip {
    let tx = at.x + (f64::from(comp.threshold) + 60.0).clamp(0.0, 60.0) / 60.0 * at.width;
    if x > tx + 12.0 {
        Grip::Ratio(which)
    } else {
        Grip::Threshold(which)
    }
}

/// How far down the display the ratio's arrow hangs, in pixels.
///
/// The reduction a full-scale signal would take, on the display's own
/// dB axis — which is what the ratio DOES, rather than a length chosen
/// to look proportional. It saturates as the ratio climbs, because the
/// effect does: past about eight to one a harder ratio takes very
/// little more off, and an arrow that kept growing would be claiming
/// otherwise. The number is in the header for the cases where that
/// distinction matters.
#[must_use]
pub fn ratio_drop(comp: Comp, height: f64) -> f64 {
    ratio_reduction(comp) / 60.0 * height
}

/// The same, in decibels — what the ratio takes off a full-scale
/// signal.
///
/// The quantity the glyph's floor sits at, and the one a drag on it
/// moves. Stated on its own because the drag has to invert it.
#[must_use]
pub fn ratio_reduction(comp: Comp) -> f64 {
    let threshold = f64::from(comp.threshold);
    let ratio = f64::from(comp.ratio).max(1.0);
    -threshold * (1.0 - 1.0 / ratio)
}

/// What is under a point in the compressor's panel.
///
/// The envelope's parts first, because they are lines inside the panel
/// the threshold otherwise owns. Everything else is the threshold: it
/// is a line across a display, and a line one pixel tall is not
/// something you aim at — the whole display is its target, the way a
/// fader's groove is a fader's.
fn comp_grip(comp: Comp, which: Which, body: Panel, rack: Rack, x: f64, y: f64) -> Grip {
    let display = comp_split(body, rack);
    if rack.detailed()
        && let Some(square) = transfer_area(body)
        && x >= square.x - TRANSFER_GAP / 2.0
        && y < square.y + square.height + TRANSFER_GAP / 2.0
    {
        return transfer_grip(comp, which, square, x);
    }
    // The envelope's parts first — they are lines inside the display,
    // and the display would otherwise swallow them.
    if rack.detailed() {
        // The strips name the compressor they were drawn for, not the
        // one this panel is.
        if let Some(strips) = Strips::of(display)
            && let Some(grip) = strips.grip_at(x, y)
        {
            return match grip {
                Grip::Attack(_) => Grip::Attack(which),
                _ => Grip::Release(which),
            };
        }
        if Arrow::of(comp, display).holds(x, y) {
            return Grip::Ratio(which);
        }
    }
    // And everything else is the threshold: it is a line across a
    // display, and a line one pixel tall is not something you aim at —
    // the whole display is its target, the way a fader's groove is a
    // fader's.
    Grip::Threshold(which)
}

/// How far a band's gain can go, in decibels.
///
/// The PARAMETER's limit, not the display's — those parted company when
/// the graph learned to zoom. The plugin's gain runs −30..30, and a
/// clamp tied to whatever the graph happened to be showing would mean
/// zooming in silently narrowed what the EQ could do.
pub const EQ_GAIN_LIMIT: f64 = 30.0;

/// Turn the wheel over a grip.
///
/// The EQ's is the plugin's own: unmodified scroll follows the band —
/// a cut filter's meaningful width is its slope, everything else's is
/// Q — and the modifier layers gain, dynamic range, or both on top.
/// None of that is decided here; [`interaction::wheel_target`] decides
/// it, and the rack calls it so that a wheel over a strip and a wheel
/// over the editor do the same thing.
pub fn wheel(tone: &mut Tone, grip: Grip, mods: Mods, delta_y: f64) {
    match grip {
        // The wheel over the row walks the list — the way you audition
        // a slot's presets, one after another, without aiming at chips
        // that may not all be on the row.
        Grip::Preset(_) => {
            let len = tone.presets.len();
            if len == 0 {
                return;
            }
            let at = tone.preset.unwrap_or(0);
            let next = if delta_y > 0.0 {
                at.saturating_add(1).min(len.saturating_sub(1))
            } else {
                at.saturating_sub(1)
            };
            if next != at {
                tone.load_preset(next);
            }
        }
        Grip::Band(which, index) => {
            let Some(band) = tone.bands(which).and_then(|set| set.get_mut(index)) else {
                return;
            };
            let uses_slope = band.shape.uses_slope();
            match interaction::wheel_target(mods, uses_slope) {
                interaction::WheelTarget::Q | interaction::WheelTarget::Slope => {
                    interaction::wheel_band(
                        band,
                        delta_y,
                        uses_slope,
                        interaction::fine_scale(mods),
                    );
                }
                // Dynamic range is not in this rack's model yet — the
                // band type carries it, the curve does not draw it —
                // so a chord that asks for it moves the gain it shares
                // an axis with rather than doing nothing.
                interaction::WheelTarget::Gain
                | interaction::WheelTarget::DynRange
                | interaction::WheelTarget::GainAndRange => {
                    let step = interaction::gain_step(delta_y, mods);
                    let next = f64::from(band.gain) + step;
                    band.gain = interaction::drag_gain_for_shape(
                        band.shape,
                        band.gain,
                        next.clamp(-EQ_GAIN_LIMIT, EQ_GAIN_LIMIT),
                    );
                }
            }
        }
        // A notch of threshold is a dB, and a notch of drive is a
        // tenth — the same relation the two drags have.
        Grip::Threshold(which) => {
            let step = interaction::gain_step(delta_y, mods);
            if let Some((now, _, _)) = tone.threshold(which) {
                tone.set_threshold(which, f64_to_f32(f64::from(now) + step));
            }
        }
        // A switch does not turn, and neither does a container.
        Grip::Bypass(_) | Grip::Phase(_) | Grip::Family(_) | Grip::Choose(..) | Grip::Zoom(_) => {}
        // A notch is a stop, not a fraction of one: the range is a list
        // the plugin publishes and the wheel walks it. Down is further
        // out, which is the direction a wheel zooms out everywhere
        // else.
        Grip::Scale(_) => {
            tone.zoom_eq(if delta_y < 0.0 { 1 } else { -1 });
        }
        // A notch of a knob is a fortieth of its travel, which is about
        // the resolution a hand expects from one — fine enough to place
        // a 3:1 exactly, coarse enough to cross the range.
        Grip::Ratio(which) | Grip::Attack(which) | Grip::Release(which) => {
            let step = interaction::gain_step(delta_y, mods) / 40.0;
            if let Some(comp) = tone.compressor(which) {
                let to = knob_norm(*comp, grip) + step;
                set_knob(comp, grip, to);
            }
        }
        Grip::Drive => {
            let step = interaction::gain_step(delta_y, mods) * 0.1;
            let moved = f64::from(tone.sat.drive) + step;
            tone.sat.drive = f64_to_f32(moved.clamp(0.0, 10.0));
        }
        _ => wheel_more(tone, grip, mods, delta_y),
    }
}

/// The wheel over the grips the newer faces added.
///
/// Split from [`wheel`] by size alone: the law is the same — a notch
/// is about a fiftieth of a control's travel, or a twentieth of a
/// time — and each arm says what its notch is.
fn wheel_more(tone: &mut Tone, grip: Grip, mods: Mods, delta_y: f64) {
    match grip {
        // A notch of bias is a fiftieth of its travel; of tilt, half a
        // decibel — the resolution a hand expects from each.
        Grip::Bias => {
            let step = interaction::gain_step(delta_y, mods) * 0.02;
            tone.sat.q_point = f64_to_f32((f64::from(tone.sat.q_point) + step).clamp(-1.0, 1.0));
        }
        Grip::Tilt => {
            let step = interaction::gain_step(delta_y, mods) * 0.5;
            let to = (f64::from(tone.sat.tilt_db()) + step).clamp(-12.0, 12.0);
            tone.sat.set_tilt_db(f64_to_f32(to));
        }
        // The gate's times share the compressor knobs' law: a notch is
        // a fortieth of the travel.
        Grip::Hold(_) => {
            let step = interaction::gain_step(delta_y, mods) / 40.0;
            let to = log_norm(f64::from(tone.gate.hold), 1.0, 2_000.0) + step;
            tone.gate.hold = f64_to_f32(log_denorm(to.clamp(0.0, 1.0), 1.0, 2_000.0));
        }
        Grip::Range => {
            let step = interaction::gain_step(delta_y, mods);
            tone.gate.range = f64_to_f32((f64::from(tone.gate.range) + step).clamp(-80.0, 0.0));
        }
        // A suppressor's depth in fiftieths, its sharpness likewise,
        // and its band edges by a twelfth of an octave a notch.
        Grip::Depth(which) => {
            let step = interaction::gain_step(delta_y, mods) * 0.02;
            if let Some(set) = tone.suppressor(which) {
                set.depth = f64_to_f32((f64::from(set.depth) + step).clamp(0.0, 1.0));
            }
        }
        Grip::Sharpness(which) => {
            let step = interaction::gain_step(delta_y, mods) * 0.02;
            if let Some(set) = tone.suppressor(which) {
                set.sharpness = f64_to_f32((f64::from(set.sharpness) + step).clamp(0.0, 1.0));
            }
        }
        Grip::Edge(which, side) => {
            let ratio = (interaction::gain_step(delta_y, mods) / 12.0).exp2();
            move_edge(tone, which, side, ratio);
        }
        // Time by a twentieth of itself a notch — a delay is set by
        // ear in proportion, not in milliseconds — and feedback in
        // fiftieths.
        Grip::Time => {
            let ratio = interaction::gain_step(delta_y, mods).mul_add(0.05, 1.0);
            tone.delay.time = f64_to_f32((f64::from(tone.delay.time) * ratio).clamp(1.0, 2_500.0));
        }
        Grip::Feedback => {
            let step = interaction::gain_step(delta_y, mods) * 0.02;
            tone.delay.feedback =
                f64_to_f32((f64::from(tone.delay.feedback) + step).clamp(0.0, 0.99));
        }
        Grip::Decay => {
            let ratio = interaction::gain_step(delta_y, mods).mul_add(0.05, 1.0);
            tone.reverb.decay = f64_to_f32((f64::from(tone.reverb.decay) * ratio).clamp(0.1, 12.0));
        }
        Grip::Predelay => {
            let step = interaction::gain_step(delta_y, mods) * 2.0;
            tone.reverb.predelay =
                f64_to_f32((f64::from(tone.reverb.predelay) + step).clamp(0.0, 250.0));
        }
        Grip::Mix(which) => {
            let step = interaction::gain_step(delta_y, mods) * 0.02;
            if let Some(mix) = tone.mix(which) {
                *mix = f64_to_f32((f64::from(*mix) + step).clamp(0.0, 1.0));
            }
        }
        // A notch of a knob is a fortieth of its travel — the
        // compressor's law; a semitone for the pitch shifter, because
        // an interval is an integer.
        Grip::Knob(Which::Pitch, 0) => {
            let step = if delta_y < 0.0 { 1 } else { -1 };
            tone.pitch = tone.pitch.saturating_add(step).clamp(-24, 24);
        }
        Grip::Knob(which, index) => {
            let step = interaction::gain_step(delta_y, mods) / 40.0;
            let to = knob_value(tone, which, index) + step;
            set_knob_value(tone, which, index, to);
        }
        _ => {}
    }
}

/// Move one edge of a suppressor's band by a ratio, keeping the band
/// at least a third of an octave wide and inside the audible range.
fn move_edge(tone: &mut Tone, which: Which, side: Side, ratio: f64) {
    let Some(set) = tone.suppressor(which) else {
        return;
    };
    let (low, high) = (f64::from(set.low), f64::from(set.high));
    match side {
        Side::Low => set.low = f64_to_f32((low * ratio).clamp(20.0, high / SUPPRESS_SHOULDER)),
        Side::High => {
            set.high = f64_to_f32((high * ratio).clamp(low * SUPPRESS_SHOULDER, 20_000.0))
        }
    }
}

/// What a modified click on a band does, applied.
///
/// [`interaction::dot_action`] resolves the chord; this carries out the
/// two that need no selection model — bypass and shape. Selection is a
/// concept the editor has and a strip does not, so those arms fall
/// through to the plain click the caller was already making.
///
/// Returns whether anything changed, so the caller knows whether to
/// re-record rather than guessing.
pub fn dot_click(tone: &mut Tone, which: Which, index: usize, mods: Mods) -> bool {
    let Some(band) = tone.bands(which).and_then(|set| set.get_mut(index)) else {
        return false;
    };
    match interaction::dot_action(mods) {
        interaction::DotAction::ToggleBypass => {
            band.enabled = !band.enabled;
            true
        }
        interaction::DotAction::CycleShape => {
            band.shape = next_shape(band.shape);
            true
        }
        interaction::DotAction::CycleSlope => {
            if let Some(slope) = band.slope.as_mut() {
                *slope = if *slope >= 10.0 { 1.0 } else { *slope + 1.0 };
                return true;
            }
            false
        }
        // Select, AddToSelection and RangeSelect all need a selection
        // model. A strip's rack has one band under the pointer and no
        // way to show a set, so these are a plain grab.
        interaction::DotAction::Select
        | interaction::DotAction::AddToSelection
        | interaction::DotAction::RangeSelect => false,
    }
}

/// The next shape in the cycle.
///
/// The shapes a strip rack can draw, in the order the editor cycles
/// them. Not every shape the model has — `filter_type_for_position`
/// reaches for cuts and notches when a band is CREATED, and creating
/// one is a gesture this rack does not have yet.
const fn next_shape(shape: EqBandShape) -> EqBandShape {
    match shape {
        EqBandShape::Bell => EqBandShape::LowShelf,
        EqBandShape::LowShelf => EqBandShape::HighShelf,
        EqBandShape::HighShelf => EqBandShape::Notch,
        _ => EqBandShape::Bell,
    }
}

/// Put a grip back where it started.
///
/// Double-clicking a control to default it is the gesture every DAW
/// has, and it is the one that makes a control safe to explore: you can
/// drag something to see what it does knowing the way back is one
/// gesture rather than a memory of the number.
///
/// A band goes FLAT rather than to some authored frequency — its
/// frequency is where you put it and its gain is the decision, so
/// undoing the decision is undoing the gain.
pub fn reset(tone: &mut Tone, grip: Grip) {
    match grip {
        Grip::Band(which, index) => {
            if let Some(band) = tone.bands(which).and_then(|set| set.get_mut(index)) {
                band.gain = 0.0;
            }
        }
        Grip::Threshold(which) => {
            let to = match which {
                Which::Gate => Gate::default().threshold,
                Which::DeEss | Which::DeEssIn => Suppress::sibilance().threshold,
                _ => Comp::default().threshold,
            };
            tone.set_threshold(which, to);
        }
        // Resetting a bypass is switching it back in, which is what
        // the double-click would have done anyway.
        Grip::Bypass(which) => {
            tone.bypass = {
                let mut next = tone.bypass;
                next.toggle(which);
                next
            }
        }
        Grip::Ratio(which) => {
            if let Some(comp) = tone.compressor(which) {
                comp.ratio = Comp::default().ratio;
            }
        }
        Grip::Attack(Which::Gate) => tone.gate.attack = Gate::default().attack,
        Grip::Release(Which::Gate) => tone.gate.release = Gate::default().release,
        Grip::Attack(which) => {
            if let Some(comp) = tone.compressor(which) {
                comp.attack = Comp::default().attack;
            }
        }
        Grip::Release(which) => {
            if let Some(comp) = tone.compressor(which) {
                comp.release = Comp::default().release;
            }
        }
        // Unity: a preamp at drive one is the wire it is modelled on.
        Grip::Drive => tone.sat.drive = 1.0,
        Grip::Bias => tone.sat.q_point = 0.0,
        Grip::Tilt => tone.sat.set_tilt_db(0.0),
        Grip::Hold(_) => tone.gate.hold = Gate::default().hold,
        Grip::Range => tone.gate.range = Gate::default().range,
        Grip::Depth(which) | Grip::Sharpness(which) | Grip::Edge(which, _) => {
            let fresh = match which {
                Which::DeEss | Which::DeEssIn => Suppress::sibilance(),
                _ => Suppress::broadband(),
            };
            if let Some(set) = tone.suppressor(which) {
                match grip {
                    Grip::Depth(_) => set.depth = fresh.depth,
                    Grip::Sharpness(_) => set.sharpness = fresh.sharpness,
                    _ => {
                        set.low = fresh.low;
                        set.high = fresh.high;
                    }
                }
            }
        }
        Grip::Time => tone.delay.time = Echo::default().time,
        Grip::Feedback => tone.delay.feedback = Echo::default().feedback,
        Grip::Decay => tone.reverb.decay = Room::default().decay,
        Grip::Predelay => tone.reverb.predelay = Room::default().predelay,
        Grip::Mix(which) => {
            let fresh = match which {
                Which::Delay => Echo::default().mix,
                Which::Reverb => Room::default().mix,
                _ => 1.0,
            };
            if let Some(mix) = tone.mix(which) {
                *mix = fresh;
            }
        }
        Grip::Scale(_) => tone.eq_range = DEFAULT_EQ_RANGE,
        // A machine is a choice, not a value with a default to go back
        // to; and folding is not a setting on the track, so there is
        // nothing to put back — see `Fold`.
        Grip::Family(_) | Grip::Choose(..) | Grip::Phase(_) | Grip::Preset(_) | Grip::Zoom(_) => {}
        // Back to what the loaded preset had, if one was loaded — a
        // knob's default is the preset's value, not a number.
        Grip::Knob(which, index) => {
            if let Some(preset) = tone.preset.and_then(|i| tone.presets.get(i)) {
                let was = knob_value(&preset.tone, which, index);
                set_knob_value(tone, which, index, was);
            }
        }
    }
}

/// Move a grip by a pixel delta.
///
/// Pixels rather than fractions because these axes are not linear in
/// the same way: a band's frequency is logarithmic and its gain is not,
/// so the conversion has to happen against the panel the drag is in.
#[expect(clippy::too_many_arguments, reason = "a drag and everything it reads")]
pub fn drag(
    tone: &mut Tone,
    grip: Grip,
    panels: &[Which],
    panel: Panel,
    folded: Folded,
    mods: Mods,
    dx: f64,
    dy: f64,
) {
    let rack = Rack::at(panel.width);
    // The grip names its own panel now — see `Grip::panel`. It used to
    // be inferred from the grip's KIND, which stopped working the
    // moment the chain had three EQs and two compressors.
    let Some((_, at)) = units(panels, panel, folded)
        .into_iter()
        .find(|(which, _)| *which == grip.panel())
    else {
        return;
    };
    let body = body_of(at, rack);
    if body.width <= 0.0 || body.height <= 0.0 {
        return;
    }
    match grip {
        Grip::Band(which, index) => {
            let map = mapper(body, tone.eq_db_range());
            let Some(band) = tone.bands(which).and_then(|set| set.get_mut(index)) else {
                return;
            };
            // Where the band is now, in the graph's own coordinates,
            // moved by the pointer's delta.
            let x = (map.freq_to_x(f64::from(band.frequency)) + dx).clamp(0.0, body.width);
            let y = (map.db_to_y(f64::from(band.gain)) + dy).clamp(0.0, body.height);
            // Which axes move is the plugin's decision: Alt pins the
            // gain so you can hunt for where a cut belongs without
            // losing how much of it you had, and Cmd turns the vertical
            // into resonance.
            match interaction::drag_mode(mods, dx, dy) {
                interaction::DragMode::Free => {
                    band.frequency = f64_to_f32(map.x_to_freq(x));
                    band.gain = interaction::drag_gain_for_shape(
                        band.shape,
                        band.gain,
                        map.y_to_db(y).clamp(-EQ_GAIN_LIMIT, EQ_GAIN_LIMIT),
                    );
                }
                interaction::DragMode::FreqOnly => {
                    band.frequency = f64_to_f32(map.x_to_freq(x));
                }
                interaction::DragMode::GainOnly => {
                    band.gain = interaction::drag_gain_for_shape(
                        band.shape,
                        band.gain,
                        map.y_to_db(y).clamp(-EQ_GAIN_LIMIT, EQ_GAIN_LIMIT),
                    );
                }
                interaction::DragMode::Resonance => {
                    // Vertical travel as a scroll: the plugin's own Q
                    // step, so a drag and a wheel move it by the same
                    // law rather than two that nearly agree.
                    interaction::wheel_band(band, dy, false, interaction::fine_scale(mods));
                }
            }
        }
        Grip::Threshold(which) => {
            // Against the DISPLAY's height, not the panel's: the line
            // is drawn on the display, and a threshold that moved
            // against a taller box would run ahead of the line the
            // pointer is holding.
            let display = comp_split(body, rack);
            let Some((now, low, high)) = tone.threshold(which) else {
                return;
            };
            let per_db = display.height / f64::from(high - low);
            let dy = dy * interaction::fine_scale(mods);
            let moved = f64::from(now) - dy / per_db.max(f64::EPSILON);
            tone.set_threshold(which, f64_to_f32(moved));
        }
        // The two times are WIDTHS on the envelope, so they are
        // dragged sideways — time is the horizontal axis on this
        // display and on every other envelope anyone has drawn. Right
        // is longer, for both, because both are durations and neither
        // is the opposite of the other.
        //
        // Against the span the glyph maps them onto, so the edge being
        // moved stays under the finger moving it.
        // The gate's three times are widths on its table, a quarter of
        // the display each — see `GateGlyph::of`.
        Grip::Attack(Which::Gate) | Grip::Hold(Which::Gate) | Grip::Release(Which::Gate) => {
            let display = display_of(body, Which::Gate, rack);
            let dx = dx * interaction::fine_scale(mods);
            let span = (display.width * 0.25).max(1.0);
            let (value, low, high): (&mut f32, f64, f64) = match grip {
                Grip::Attack(_) => (&mut tone.gate.attack, 0.1, 200.0),
                Grip::Hold(_) => (&mut tone.gate.hold, 1.0, 2_000.0),
                _ => (&mut tone.gate.release, 5.0, 3_000.0),
            };
            let moved = (log_norm(f64::from(*value), low, high) + dx / span).clamp(0.0, 1.0);
            *value = f64_to_f32(log_denorm(moved, low, high));
        }
        // Along the strip: left is faster, right is slower, through the
        // strip's own two-sided scale so the marker stays under the
        // finger across the default.
        Grip::Attack(which) | Grip::Release(which) => {
            let display = comp_split(body, rack);
            let dx = dx * interaction::fine_scale(mods);
            let span = Strips::of(display)
                .map_or(display.width * 0.4, |s| s.attack.width())
                .max(1.0);
            if let Some(comp) = tone.compressor(which) {
                let time = if matches!(grip, Grip::Attack(_)) {
                    Time::attack(comp.attack)
                } else {
                    Time::release(comp.release)
                };
                let moved = f64_to_f32(time.at(time.place() + dx / span));
                if matches!(grip, Grip::Attack(_)) {
                    comp.attack = moved;
                } else {
                    comp.release = moved;
                }
            }
        }
        // The floor is pulled DOWN for more, which is the direction the
        // signal goes. Against the display's own dB height, so it stays
        // under the finger — and the ratio keeps moving past the point
        // where the depth stops growing, because the depth saturates
        // and the setting does not.
        // A switch has no drag. Dragging off one is how you change your
        // mind about pressing it, which is the mixer's own rule — and a
        // zoom is a switch between stops.
        Grip::Bypass(_) | Grip::Scale(_) | Grip::Phase(_) | Grip::Zoom(_) => {}
        Grip::Drive => {
            // A quarter of the panel's height is the whole range, so a
            // short drag is a real change — drive is the parameter you
            // nudge, not the one you sweep.
            let per_unit = body.height / 4.0;
            let dy = dy * interaction::fine_scale(mods);
            let moved = f64::from(tone.sat.drive) - dy / per_unit.max(f64::EPSILON);
            tone.sat.drive = f64_to_f32(moved.clamp(0.0, 10.0));
        }
        _ => drag_more(tone, grip, body, rack, mods, dx, dy),
    }
}

/// The drags the newer faces added — split from [`drag`] by size alone.
/// Each arm states the axis its grip moves on and what a panel width or
/// height is worth on it.
fn drag_more(tone: &mut Tone, grip: Grip, body: Panel, rack: Rack, mods: Mods, dx: f64, dy: f64) {
    match grip {
        // Down for more: the arrow points the way the signal goes.
        // Six tenths of the display is the whole range, through the
        // same two-sided scale as the times.
        Grip::Ratio(which) => {
            let display = comp_split(body, rack);
            let dy = dy * interaction::fine_scale(mods);
            let span = (display.height * 0.6).max(1.0);
            if let Some(comp) = tone.compressor(which) {
                let scale = Time::ratio(comp.ratio);
                comp.ratio = f64_to_f32(scale.at(scale.place() + dy / span));
            }
        }

        // The bias leans the curve, so it is dragged the way the curve
        // leans: sideways, half the curve's width for the whole range.
        Grip::Bias => {
            let (curve_box, _) = sat_split(display_of(body, Which::Sat, rack), rack);
            let dx = dx * interaction::fine_scale(mods);
            let per_unit = (curve_box.width / 2.0).max(1.0);
            let moved = f64::from(tone.sat.q_point) + dx / per_unit;
            tone.sat.q_point = f64_to_f32(moved.clamp(-1.0, 1.0));
        }
        // Up is the top: a drag up the ladder tips the emphasis
        // toward the highs. The ladder's height is ±12 dB.
        Grip::Tilt => {
            let (_, ladder_box) = sat_split(display_of(body, Which::Sat, rack), rack);
            let per_db = ladder_box.map_or(body.height, |lb| lb.height) / 24.0;
            let dy = dy * interaction::fine_scale(mods);
            let to =
                (f64::from(tone.sat.tilt_db()) - dy / per_db.max(f64::EPSILON)).clamp(-12.0, 12.0);
            tone.sat.set_tilt_db(f64_to_f32(to));
        }
        // The range line is pulled DOWN for more, against the display's
        // own dB axis, like the ratio's arrow.
        Grip::Range => {
            let display = display_of(body, Which::Gate, rack);
            let per_db = display.height / 60.0;
            let dy = dy * interaction::fine_scale(mods);
            let moved = f64::from(tone.gate.range) - dy / per_db.max(f64::EPSILON);
            tone.gate.range = f64_to_f32(moved.clamp(-80.0, 0.0));
        }
        // The ribbon is pulled down for more: the display's height is
        // the whole range. Sharpness the same way — down is narrower,
        // which is what a tooth is.
        Grip::Depth(which) | Grip::Sharpness(which) => {
            let display = display_of(body, which, rack);
            let dy = dy * interaction::fine_scale(mods);
            let per_unit = display.height.max(1.0);
            if let Some(set) = tone.suppressor(which) {
                let value = if matches!(grip, Grip::Depth(_)) {
                    &mut set.depth
                } else {
                    &mut set.sharpness
                };
                *value = f64_to_f32((f64::from(*value) + dy / per_unit).clamp(0.0, 1.0));
            }
        }
        // An edge moves along the zoomed axis it is drawn on: a panel
        // width is the window's whole span in decades.
        Grip::Edge(which, side) => {
            let display = display_of(body, which, rack);
            let zoom = SuppressZoom::top();
            let dx = dx * interaction::fine_scale(mods);
            let decades = (zoom.high / zoom.low).log10();
            let ratio = 10.0_f64.powf(dx / display.width.max(1.0) * decades);
            move_edge(tone, which, side, ratio);
        }
        // The first repeat's x IS the time: the window is four and a
        // half times, so a pixel is that many milliseconds.
        Grip::Time => {
            let body = display_of(body, Which::Delay, rack);
            let dx = dx * interaction::fine_scale(mods);
            let per_ms = (body.width - 6.0).max(1.0) / (f64::from(tone.delay.time).max(1.0) * 4.5);
            let moved = f64::from(tone.delay.time) + dx / per_ms.max(f64::EPSILON);
            tone.delay.time = f64_to_f32(moved.clamp(1.0, 2_500.0));
        }
        // The repeats' heights are the feedback: the panel's height is
        // the range.
        Grip::Feedback => {
            let body = display_of(body, Which::Delay, rack);
            let dy = dy * interaction::fine_scale(mods);
            let moved = f64::from(tone.delay.feedback) - dy / body.height.max(1.0);
            tone.delay.feedback = f64_to_f32(moved.clamp(0.0, 0.99));
        }
        // The tail reaches the right edge at its decay, so a drag right
        // lengthens it in proportion: a panel width is the decay
        // itself.
        Grip::Decay => {
            let body = display_of(body, Which::Reverb, rack);
            let dx = dx * interaction::fine_scale(mods);
            let ratio = 1.0 + dx / body.width.max(1.0);
            tone.reverb.decay =
                f64_to_f32((f64::from(tone.reverb.decay) * ratio.max(0.2)).clamp(0.1, 12.0));
        }
        // The gap rule moves along the decay window.
        Grip::Predelay => {
            let body = display_of(body, Which::Reverb, rack);
            let dx = dx * interaction::fine_scale(mods);
            let window = f64::from(tone.reverb.decay).max(0.05) * 1000.0;
            let moved = f64::from(tone.reverb.predelay) + dx / body.width.max(1.0) * window;
            tone.reverb.predelay = f64_to_f32(moved.clamp(0.0, 250.0));
        }
        Grip::Mix(which) => {
            let body = display_of(body, which, rack);
            let dy = dy * interaction::fine_scale(mods);
            let per_unit = body.height.max(1.0);
            if let Some(mix) = tone.mix(which) {
                *mix = f64_to_f32((f64::from(*mix) - dy / per_unit).clamp(0.0, 1.0));
            }
        }
        Grip::Knob(which, index) => drag_knob(tone, which, index, mods, dy),
        _ => {}
    }
}

/// A knob's drag: its whole travel is a hundred pixels, up for more.
/// The pitch shifter's interval steps a semitone every six, because an
/// interval is an integer.
fn drag_knob(tone: &mut Tone, which: Which, index: usize, mods: Mods, dy: f64) {
    let dy = dy * interaction::fine_scale(mods);
    if which == Which::Pitch && index == 0 {
        let steps = crate::num::quantise(-dy / 6.0, 1.0);
        tone.pitch = tone.pitch.saturating_add(steps).clamp(-24, 24);
        return;
    }
    let to = knob_value(tone, which, index) - dy / 100.0;
    set_knob_value(tone, which, index, to);
}

/// A knob's value as a fraction of its travel — see [`knob_labels`]
/// for which knob is which.
#[must_use]
pub fn knob_value(tone: &Tone, which: Which, index: usize) -> f64 {
    match (which, tone.role, index) {
        (Which::Knobs, Role::Reverb, 0) => f64::from(tone.reverb.size),
        (Which::Knobs, Role::Reverb, 1) => f64::from(tone.reverb.damping),
        (Which::Knobs, Role::Reverb, 2) => f64::from(tone.reverb.diffusion),
        (Which::Knobs, Role::Reverb, 3) => f64::from(tone.reverb.predelay) / 250.0,
        (Which::Knobs, Role::Reverb, _) => f64::from(tone.reverb.mix),
        (Which::Knobs, _, 0) => log_norm(f64::from(tone.delay.time), 1.0, 2_500.0),
        (Which::Knobs, _, 1) => f64::from(tone.delay.feedback) / 0.99,
        (Which::Knobs, _, 2) => f64::from(tone.delay.tone),
        (Which::Knobs, _, 3) => f64::from(tone.delay.width),
        (Which::Knobs, _, _) => f64::from(tone.delay.mix),
        (Which::Wide, _, _) => f64::from(tone.wide) / 2.0,
        (Which::Pitch, _, 0) => (f64::from(tone.pitch) + 24.0) / 48.0,
        (Which::Pitch, _, _) => f64::from(tone.pitch_mix),
        _ => 0.0,
    }
}

/// And setting it, clamped to the travel.
pub fn set_knob_value(tone: &mut Tone, which: Which, index: usize, to: f64) {
    let to = to.clamp(0.0, 1.0);
    let f = f64_to_f32(to);
    match (which, tone.role, index) {
        (Which::Knobs, Role::Reverb, 0) => tone.reverb.size = f,
        (Which::Knobs, Role::Reverb, 1) => tone.reverb.damping = f,
        (Which::Knobs, Role::Reverb, 2) => tone.reverb.diffusion = f,
        (Which::Knobs, Role::Reverb, 3) => tone.reverb.predelay = f64_to_f32(to * 250.0),
        (Which::Knobs, Role::Reverb, _) => tone.reverb.mix = f,
        (Which::Knobs, _, 0) => tone.delay.time = f64_to_f32(log_denorm(to, 1.0, 2_500.0)),
        (Which::Knobs, _, 1) => tone.delay.feedback = f64_to_f32(to * 0.99),
        (Which::Knobs, _, 2) => tone.delay.tone = f,
        (Which::Knobs, _, 3) => tone.delay.width = f,
        (Which::Knobs, _, _) => tone.delay.mix = f,
        (Which::Wide, _, _) => tone.wide = f64_to_f32(to * 2.0),
        (Which::Pitch, _, 0) => {
            tone.pitch = crate::num::quantise(to.mul_add(48.0, -24.0), 1.0).clamp(-24, 24)
        }
        (Which::Pitch, _, _) => tone.pitch_mix = f,
        _ => {}
    }
}

/// What the knob row's five knobs are, for a role.
#[must_use]
pub const fn knob_labels(role: Role) -> [&'static str; KNOBS] {
    match role {
        Role::Reverb => ["SIZE", "DAMP", "DIFF", "PRE", "MIX"],
        _ => ["TIME", "FDBK", "TONE", "WIDE", "MIX"],
    }
}

/// How many knobs the row holds.
pub const KNOBS: usize = 5;

/// Which knob of the row a point is over.
#[must_use]
pub fn knob_at(body: Panel, x: f64) -> usize {
    let each = body.width / crate::num::coord(KNOBS);
    crate::num::index(((x - body.x) / each.max(1.0)).floor()).min(KNOBS.saturating_sub(1))
}

/// Which preset chip a point is over, if any.
#[must_use]
pub fn preset_chip_at(tone: &Tone, at: Panel, x: f64) -> Option<usize> {
    preset_chips(tone, at)
        .into_iter()
        .find(|(_, chip)| x >= chip.x0 && x < chip.x1)
        .map(|(i, _)| i)
}

/// The chips that fit the row, with the loaded one always among them.
///
/// A row narrower than its list shows a window onto it that starts
/// far enough back for the loaded chip to be the last one in — so the
/// wheel walks the list and the chip you are on never leaves the row.
/// One place for the drawing and the hit test both, for the usual
/// reason.
fn preset_chips(tone: &Tone, at: Panel) -> Vec<(usize, Rect)> {
    let right = at.x + at.width;
    let fits = |first: usize| {
        let mut left = at.x + 2.0;
        let mut out = Vec::new();
        for (i, preset) in tone.presets.iter().enumerate().skip(first) {
            let w = preset_chip_width(&preset.name);
            if left + w > right {
                break;
            }
            out.push((
                i,
                Rect::new(left, at.y + 2.0, left + w, at.y + at.height - 2.0),
            ));
            left += w + 2.0;
        }
        out
    };
    let current = tone.preset.unwrap_or(0);
    let mut first = 0;
    loop {
        let chips = fits(first);
        if chips.iter().any(|(i, _)| *i == current) || first >= current {
            return chips;
        }
        first = first.saturating_add(1);
    }
}

/// A chip's width for a name: the swatch, the text, the air.
fn preset_chip_width(name: &str) -> f64 {
    crate::num::coord(name.len()).mul_add(3.6, 14.0)
}

/// How tall the preset row is.
pub const PRESETS_H: f64 = 18.0;

/// The racks' settings shared between everything that shows them — the
/// mixer's strips, a close-up — with a generation that goes up on every
/// edit, so each can tell its picture is stale.
#[derive(Default)]
pub struct Shared {
    pub store: std::cell::RefCell<Store>,
    generation: std::cell::Cell<u64>,
    /// Where the racks' edits go: the track's FTS processors, in the
    /// engine (`crate::tone_bind`). Started the first time there is one.
    #[cfg(feature = "native")]
    applier: std::cell::OnceCell<Option<crate::engine::Applier>>,
}

impl Shared {
    /// Say the settings moved — and send what moved to the processors
    /// on the tracks ([`Store::flush`]). A store still held by its editor
    /// keeps its changes for the next edit to send.
    pub fn edited(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        #[cfg(feature = "native")]
        {
            let edits = match self.store.try_borrow_mut() {
                Ok(mut store) => store.flush(),
                Err(_) => return,
            };
            if edits.is_empty() {
                return;
            }
            if let Some(applier) = self.applier.get_or_init(crate::engine::Applier::start) {
                for edit in edits {
                    applier.send(edit);
                }
            }
        }
    }

    /// Which edit the settings are at.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.get()
    }
}

/// One FX's wanted parameters (`crate::tone_bind`): its name, and
/// `(index, normalized value)`.
type Stage = (&'static str, Vec<(u32, f64)>);

/// The Tone settings for every track the window has opened.
///
/// Keyed by GUID, not by row: a preset can hide a track and a fold can
/// move one, and a rack that followed a row index would show the
/// neighbour's EQ the moment either happened.
///
/// This is where a real chain's parameters will land. Until then it is
/// seeded from [`placeholder`] — which means a rack is editable NOW,
/// against state the window owns, in exactly the shape the plugin's
/// parameters will take. The binding is the piece that is missing; the
/// UI above it is not waiting on anything.
#[derive(Clone, Debug, Default)]
pub struct Store {
    by_guid: std::collections::HashMap<String, Tone>,
    /// What each track's chain was last given (`crate::tone_bind`), so a
    /// flush sends what changed and only that.
    sent: std::collections::HashMap<String, Vec<Stage>>,
    /// The FX a flush has switched on, by track: a stage's first change
    /// switches it on and sends all of it — the plugin's own defaults are
    /// not the rack's.
    on: std::collections::HashSet<(String, &'static str)>,
    /// Tracks edited since the last flush.
    dirty: std::collections::HashSet<String>,
    /// Tracks whose chain has been read from the engine
    /// ([`Self::sync`]), so it is not read again until it changes.
    read: std::collections::HashSet<String>,
    /// Until when each track's engine changes are the window's own edits
    /// coming back ([`Self::changed`] lets them pass): read again under a
    /// knob being turned, the row would fight the hand turning it.
    own: std::collections::HashMap<String, web_time::Instant>,
}

/// How long after the window's own edit to a track the engine's news of
/// it is taken to be that edit.
const OWN_ECHO: std::time::Duration = std::time::Duration::from_millis(300);

impl Store {
    /// Make sure every track has settings, without disturbing the ones
    /// that already do.
    ///
    /// Called before a record rather than lazily inside one, so that
    /// recording can take `&self` — a `&mut` threaded through the
    /// drawing would put a lock between the mixer and its strips.
    pub fn seed(&mut self, rows: &[(daw_proto::Track, u32)]) {
        // The folders above each track, nearest last: a row's depth
        // says how many of the folders before it still enclose it.
        let mut folders: Vec<(u32, String)> = Vec::new();
        for (index, (track, depth)) in rows.iter().enumerate() {
            folders.retain(|(at, _)| *at < *depth);
            let ancestors: Vec<String> = folders.iter().map(|(_, name)| name.clone()).collect();
            let mut role = Role::of(&track.name, track.is_folder, &ancestors);
            // A stereo pair is one instrument: the folder over an L and
            // an R carries the processing, and the two halves are rails
            // under it. So the folder is a channel, not a bus.
            if track.is_folder && is_pair(rows, index, *depth) {
                role = Role::Channel;
            }
            if let Some(guitar) = guitar_role(rows, index, &ancestors) {
                role = guitar;
            }
            if track.is_folder {
                folders.push((*depth, track.name.clone()));
            }
            self.by_guid
                .entry(track.guid.clone())
                .or_insert_with(|| placeholder_for(role, index, &track.name, &ancestors));
        }
    }

    /// Read each track's chain from the engine that has not been read, or
    /// has changed since ([`Self::changed`]): what the track has is what
    /// shows, not a placeholder.
    #[cfg(feature = "native")]
    pub fn sync_tracks(&mut self, guids: impl IntoIterator<Item = String>) {
        let bpm = crate::open::local_engine()
            .zip(crate::open::current_song())
            .map_or(120.0, |(daw, song)| {
                daw_proto::TempoMap::get_tempo_at(
                    daw,
                    daw::service::ProjectContext::Project(song),
                    0.0,
                )
            });
        for guid in guids {
            if self.read.contains(&guid) || !self.by_guid.contains_key(&guid) {
                continue;
            }
            let chain = crate::tone_read::chain(&guid);
            self.sync(&guid, &chain, bpm);
        }
    }

    /// `chain` (the engine's, on the track `guid`) into its settings: its
    /// processors' values, which are on, which devices it shows. What it
    /// now holds is what the chain was last given, so an edit after sends
    /// only what the edit moves.
    #[cfg(feature = "native")]
    pub fn sync(&mut self, guid: &str, chain: &[crate::tone_read::EngineFx], bpm: f64) {
        let Some(tone) = self.by_guid.get_mut(guid) else {
            return;
        };
        crate::tone_read::apply(tone, chain, bpm);
        tone.engine_units = Some(crate::tone_read::devices(chain));
        tone.engine_others = crate::tone_read::others(chain);
        let stages = crate::tone_bind::stages(tone);
        for (fx, _) in &stages {
            if chain
                .iter()
                .any(|f| f.enabled && f.name.eq_ignore_ascii_case(fx))
            {
                self.on.insert((guid.to_owned(), fx));
            }
        }
        self.sent.insert(guid.to_owned(), stages);
        self.read.insert(guid.to_owned());
    }

    /// The track `guid`'s chain changed in the engine: read it again when
    /// next shown — unless the window is mid-edit on it, whose own edits
    /// these are.
    pub fn changed(&mut self, guid: &str) {
        let echo = self
            .own
            .get(guid)
            .is_some_and(|until| web_time::Instant::now() < *until);
        if !self.dirty.contains(guid) && !echo {
            self.read.remove(guid);
        }
    }

    /// A track's settings, if it has any.
    #[must_use]
    pub fn get(&self, guid: &str) -> Option<&Tone> {
        self.by_guid.get(guid)
    }

    /// The same, to change. Marked for the next [`Self::flush`].
    pub fn edit(&mut self, guid: &str) -> Option<&mut Tone> {
        let tone = self.by_guid.get_mut(guid)?;
        #[cfg(feature = "native")]
        if !self.sent.contains_key(guid) {
            // What the chain has before this edit: the baseline a flush
            // compares against.
            self.sent
                .insert(guid.to_owned(), crate::tone_bind::stages(tone));
        }
        self.dirty.insert(guid.to_owned());
        Some(tone)
    }

    /// The engine edits for what changed since the last flush: each
    /// edited track's chain given the rack's settings
    /// (`crate::tone_bind`). A stage changed for the first time is switched
    /// on and sent whole; after that, only what moved.
    #[cfg(feature = "native")]
    pub fn flush(&mut self) -> Vec<crate::engine::Edit> {
        use crate::engine::Edit;
        let mut edits = Vec::new();
        for guid in std::mem::take(&mut self.dirty) {
            let Some(tone) = self.by_guid.get(&guid) else {
                continue;
            };
            let now = crate::tone_bind::stages(tone);
            let was = self.sent.remove(&guid).unwrap_or_default();
            for (fx, params) in &now {
                let before = was
                    .iter()
                    .find(|(f, _)| f == fx)
                    .map_or(&[][..], |(_, p)| p.as_slice());
                if params.as_slice() == before {
                    continue;
                }
                let first = self.on.insert((guid.clone(), fx));
                let changed = params.iter().filter(|p| first || !before.contains(p));
                edits.extend(changed.map(|(index, value)| {
                    Edit::SetFxParam(guid.clone(), (*fx).to_owned(), *index, *value)
                }));
                if first {
                    edits.push(Edit::SetFxEnabled(guid.clone(), (*fx).to_owned(), true));
                }
            }
            self.own.insert(
                guid.clone(),
                web_time::Instant::now()
                    .checked_add(OWN_ECHO)
                    .unwrap_or_else(web_time::Instant::now),
            );
            self.sent.insert(guid, now);
        }
        edits
    }

    /// No engine to send to (a page): the edits only mark the store.
    #[cfg(not(feature = "native"))]
    pub fn flush(&mut self) -> Vec<crate::engine::Edit> {
        self.dirty.clear();
        Vec::new()
    }

    /// Render every reverb tail in the store, now, on this thread.
    ///
    /// For a bench or a test that must be deterministic — a tail that
    /// arrived between two renders of the same frame would make them
    /// differ.
    pub fn prerender_tails(&self) {
        for tone in self.by_guid.values() {
            tone.prerender_tails();
        }
    }
}

/// The role of a track inside an electric guitar's folder, or `None`
/// for one that is not in one.
///
/// A guitar reads as arrangement → layer → channel → sources: Rhythm
/// over its Main and Octave, each doubled into an L and an R, each of
/// those captured on a DI, a pedalboard and two miked amps. The part is
/// processed on its LAYER — the performance — so the chain goes there,
/// and everything under a layer is a capture with nothing on it.
///
/// - **Arrangement** (directly under the guitar folder): a chain. Rhythm
///   stacks the octave under the main and is processed as one part;
///   Lead is its own layer, doubled into an L and an R. The exception
///   is a Solo folder: its layers are two separate lines (the solo and
///   its harmony), each processed on its own, so the folder over them
///   is only a folder.
/// - **Layer** (under an arrangement): a chain, unless it is one half
///   of the arrangement's own pair (Lead's L and R).
/// - **Deeper** — a channel folder, an amp, a mic, the DI: nothing.
///
/// Keyed on where the track sits under a folder named for the electric
/// guitar, because the strip has the rows and not the taxonomy: the
/// same "Main" or "L" is a lead vocal's layer or a piano's half
/// elsewhere, and the names alone would process the wrong one. And on
/// the depth rather than on what a folder holds, because a scene that
/// hides the sources (Guitar Mixing) or folds a layer (the arrangement)
/// hands this rows without them, and a track's chain is decided once.
#[must_use]
pub fn guitar_role(
    rows: &[(daw_proto::Track, u32)],
    index: usize,
    ancestors: &[String],
) -> Option<Role> {
    let at = ancestors.iter().rposition(|a| is_electric_guitar(a))?;
    // Keys has an Electric of its own (Rhodes, Wurli); it is not a guitar.
    if ancestors
        .iter()
        .take(at)
        .any(|a| a.to_lowercase().starts_with("keys"))
    {
        return None;
    }
    let (track, _) = rows.get(index)?;
    let level = ancestors.len().saturating_sub(at.saturating_add(1));
    let role = match level {
        0 if track.is_folder && track.name.trim().to_lowercase().starts_with("solo") => {
            Role::Source
        }
        0 => Role::Channel,
        1 if !track.is_folder && is_pair_half(&track.name) => Role::Half,
        1 => Role::Channel,
        _ => Role::Source,
    };
    Some(role)
}

/// Whether a folder name is the electric guitar's.
fn is_electric_guitar(name: &str) -> bool {
    matches!(
        name.trim().to_lowercase().as_str(),
        "electric" | "electrics" | "electric guitar" | "electric guitars"
    )
}

/// Whether the folder at `index` holds exactly a left and a right half.
#[must_use]
pub fn is_pair(rows: &[(daw_proto::Track, u32)], index: usize, depth: u32) -> bool {
    let inside: Vec<&str> = rows
        .iter()
        .skip(index.saturating_add(1))
        .take_while(|(_, d)| *d > depth)
        .filter(|(_, d)| *d == depth.saturating_add(1))
        .map(|(t, _)| t.name.as_str())
        .collect();
    inside.len() == 2 && is_pair_half(inside[0]) && is_pair_half(inside[1])
}

/// Whether a track is one half of a stereo pair, by name.
#[must_use]
pub fn is_pair_half(name: &str) -> bool {
    // The scene engine's own answer: the pair-half invariant is enforced
    // there, and a strip that disagreed about what a half IS would rail
    // a track the scene had opened.
    dynamic_template::scenes::is_pair_half(name)
}

fn band(index: usize, frequency: f64, gain: f64, q: f64, shape: EqBandShape) -> EqBand {
    EqBand {
        index,
        used: true,
        enabled: true,
        frequency: f64_to_f32(frequency),
        gain: f64_to_f32(gain),
        q: f64_to_f32(q),
        shape,
        // The cut filters' slope order; `None` leaves the band on its Q,
        // which is what every shape the rack places uses.
        slope: None,
        focus: false,
        stereo_mode: StereoMode::default(),
        name: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The panels are their own height, from the top, and do not
    /// stretch to whatever rack they are given — a frequency response
    /// is readable at a hundred and seventy pixels and no more readable
    /// at four hundred.
    #[test]
    fn the_panels_do_not_stretch_to_fill_the_rack() {
        let tall = Panel {
            x: 0.0,
            y: 0.0,
            width: 133.0,
            height: 900.0,
        };
        let laid = layout(&[Which::Eq, Which::Comp, Which::Sat], tall);
        assert_eq!(laid.len(), 3);
        for (which, at) in &laid {
            assert!(
                (at.height - which.natural()).abs() < 0.01,
                "{which:?} took {} rather than {}",
                at.height,
                which.natural()
            );
        }
        // From the top, under Rescue's blank bar and the phase's own
        // container bar, and the space below is left alone.
        assert!((laid[0].1.y - tall.y - 2.0 * HEAD_H).abs() < f64::EPSILON);
        let used = super::tall(
            &[Which::Eq, Which::Comp, Which::Sat],
            super::Folded::default(),
        );
        assert!(
            used < tall.height,
            "the rack filled everything it was given"
        );
    }

    /// A rack too short for its chain SCROLLS rather than shrinking.
    ///
    /// The panels used to scale down together to fit, which meant every
    /// processor added shrank every processor already there — three
    /// units at two hundred pixels is three unreadable units. Keeping
    /// their natural heights and running off the bottom is what makes
    /// the chain survive growing, and `scroll_span` is how far the box
    /// has to travel to see the rest.
    #[test]
    fn a_short_rack_scrolls_rather_than_shrinking() {
        let panels = [Which::Eq, Which::Comp, Which::Sat];
        let short = Panel {
            x: 0.0,
            y: 0.0,
            width: 133.0,
            height: 200.0,
        };
        let laid = layout(&panels, short);
        assert_eq!(laid.len(), 3, "a panel was dropped");
        for (which, at) in &laid {
            assert!(
                (at.height - which.natural()).abs() < f64::EPSILON,
                "{which:?} was shrunk to {}",
                at.height
            );
        }
        let bottom = laid.last().map_or(0.0, |(_, at)| at.y + at.height);
        assert!(
            bottom > short.height,
            "the chain fitted, so nothing was proved"
        );
        // To the last ROW's floor: the blank bars for the phases this
        // chain lacks come after the last panel, and are scrolled to.
        let floor = super::chain(&panels, short, super::Folded::default())
            .last()
            .map_or(0.0, |(_, at)| at.y + at.height);
        assert!(floor > bottom);
        assert!(
            (super::scroll_span(&panels, short.height, super::Folded::default())
                - (floor - short.height))
                .abs()
                < 0.01,
            "the span does not reach the last row's floor"
        );

        // And a box tall enough to hold the chain does not scroll.
        assert!(super::scroll_span(&panels, 4000.0, super::Folded::default()).abs() < f64::EPSILON);
    }

    /// Scrolling is the panel's own `y`, so the drawing and the hit
    /// test cannot disagree about it — they are handed the same moved
    /// panel rather than each applying an offset.
    #[test]
    fn a_scroll_moves_every_panel_by_the_same_amount() {
        let panels = [Which::Eq, Which::Comp, Which::Sat];
        let box_at = Panel {
            x: 0.0,
            y: 40.0,
            width: 133.0,
            height: 200.0,
        };
        let moved = Panel {
            y: box_at.y - 75.0,
            ..box_at
        };
        for ((_, rest), (_, down)) in layout(&panels, box_at).iter().zip(layout(&panels, moved)) {
            assert!((rest.y - down.y - 75.0).abs() < f64::EPSILON);
            assert!((rest.height - down.height).abs() < f64::EPSILON);
        }
    }

    /// The tiers are ordered, and each threshold is where its tier
    /// starts — written against the constants, because pasted widths go
    /// stale the moment a threshold moves and then test nothing.
    /// The focus tier hands the panel to the plugin, and the tier
    /// below it keeps the rack's own abbreviation. Two looks from one
    /// painter, and the threshold between them is a width.
    #[test]
    fn the_focus_tier_is_the_editing_one() {
        assert!(LEGIBLE < FOCUSED, "the tiers must not overlap");
        assert_eq!(Rack::at(FOCUSED), Rack::Focus);
        assert_eq!(Rack::at(FOCUSED - 0.5), Rack::Full);
        assert!(Rack::at(FOCUSED).editing());
        assert!(!Rack::at(FOCUSED - 0.5).editing());
        // Both are detailed: headers, markers and grips belong to each.
        assert!(Rack::at(FOCUSED).detailed());
        assert!(Rack::at(LEGIBLE).detailed());
        assert!(!Rack::at(SHAPE).detailed());
    }

    /// A focused panel still gives up its header line — the numbers are
    /// what you check the curve against, and they do not stop mattering
    /// because there is more room.
    #[test]
    fn every_detailed_tier_keeps_its_header() {
        let panel = Panel {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 200.0,
        };
        for rack in [Rack::Focus, Rack::Full, Rack::Curves, Rack::Minimal] {
            let body = body_of(panel, rack);
            assert!(
                body.y > panel.inset(2.0).y,
                "{rack:?} did not reserve a header"
            );
        }
        // One height at every tier: that is the point.
        assert!(
            (body_of(panel, Rack::Curves).y - body_of(panel, Rack::Focus).y).abs() < f64::EPSILON
        );
    }

    /// A focused strip may not be lent below the width that made it
    /// focused — the whole point of opening one is that it stays open.
    #[test]
    fn a_focused_rack_will_not_be_lent_away() {
        assert_eq!(Rack::Focus.floor(), Some(FOCUSED));
    }

    #[test]
    fn the_rack_sheds_in_order() {
        assert!(SHAPE < LEGIBLE, "the tiers must not overlap");
        assert_eq!(Rack::at(LEGIBLE), Rack::Full);
        assert_eq!(Rack::at(LEGIBLE - 0.5), Rack::Curves);
        assert_eq!(Rack::at(SHAPE), Rack::Curves);
        assert_eq!(Rack::at(SHAPE - 0.5), Rack::Minimal);
        assert_eq!(Rack::at(MINIMAL), Rack::Minimal);
        assert_eq!(Rack::at(MINIMAL - 0.5), Rack::Off);
        assert!(
            Rack::Full < Rack::Curves && Rack::Curves < Rack::Minimal && Rack::Minimal < Rack::Off
        );
        assert!(
            Rack::at(LEGIBLE).on() && Rack::at(SHAPE - 0.5).on() && !Rack::at(MINIMAL - 0.5).on()
        );
    }

    /// A rack with no room records nothing at all, rather than three
    /// slivers of well with no curve in them.
    #[test]
    fn a_flat_rack_draws_nothing() {
        let mut scene = Scene::new();
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = Font::embedded().expect("the embedded font");
        record(
            &mut scene,
            &palette,
            &font,
            &placeholder(0),
            &[Which::Eq, Which::Comp, Which::Sat],
            Panel {
                x: 0.0,
                y: 0.0,
                width: 300.0,
                height: 8.0,
            },
            super::Folded::default(),
            None,
        );
        assert!(scene.commands.is_empty());
    }

    /// And a rack with room records all three panels.
    #[test]
    fn a_full_rack_draws_three_panels() {
        let mut scene = Scene::new();
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = Font::embedded().expect("the embedded font");
        record(
            &mut scene,
            &palette,
            &font,
            &placeholder(0),
            &[Which::Eq, Which::Comp, Which::Sat],
            Panel {
                x: 0.0,
                y: 0.0,
                width: 300.0,
                height: 180.0,
            },
            super::Folded::default(),
            None,
        );
        // Three grounds, three curves, the rules and the labels.
        assert!(
            scene.commands.len() > 9,
            "a full rack should record more than its three grounds: {}",
            scene.commands.len()
        );
    }

    /// The EQ curve is the plugin's, so a boost must come back as one:
    /// the response at a boosted band's centre is above unity.
    #[test]
    fn the_curve_is_the_plugins_own() {
        let tone = placeholder(0);
        let at_3k = calculate_combined_response(&tone.eq, 3_000.0, DISPLAY_RATE);
        let at_300 = calculate_combined_response(&tone.eq, 300.0, DISPLAY_RATE);
        assert!(at_3k > 0.0, "the 3k bell boosts: {at_3k}");
        assert!(at_300 < 0.0, "the 300 bell cuts: {at_300}");
    }
}

#[cfg(test)]
mod suppress_tests {
    use super::{SUPPRESS_CEIL_DB, SUPPRESS_FLOOR_DB, Suppress, Which};

    /// The analyser speaks DECIBELS, and the suppressor's window has to
    /// be the analyser's own.
    ///
    /// This is the bug that made both suppressors unreadable: the bins
    /// were read as a 0..1 height, so every one of them pinned to the
    /// ceiling or sat on the floor, and the difference between a peak
    /// and its neighbourhood — which is the entire detection — came out
    /// as either nothing or everything.
    #[test]
    fn the_window_is_the_analysers_own() {
        let simulated = crate::simulate::frame(2, 1.25).spectrum;
        for db in &simulated {
            assert!(
                f64::from(*db) >= SUPPRESS_FLOOR_DB && f64::from(*db) <= SUPPRESS_CEIL_DB,
                "the analyser produced {db} dB, outside the window the suppressor draws"
            );
        }
        assert!(SUPPRESS_FLOOR_DB < SUPPRESS_CEIL_DB);
    }

    /// A suppressor acts only inside its band, which is the whole
    /// difference between the de-esser and the broadband one.
    #[test]
    fn the_band_is_what_tells_the_two_apart() {
        let ess = Suppress::sibilance();
        let broad = Suppress::broadband();
        assert!(ess.low > broad.low, "the de-esser should start higher up");
        assert!(
            f64::from(ess.high - ess.low) < f64::from(broad.high - broad.low),
            "the de-esser should be the narrower of the two"
        );
        // Both draw the same picture, and both are spectral.
        assert!(Which::DeEss.is_spectral() && Which::PolishEq.is_spectral());
    }
}

#[cfg(test)]
mod fold_tests {
    use super::{Fold, Folded};
    use session::mix_phases::MixPhase as P;

    /// A fold is per phase and survives being asked about again.
    #[test]
    fn a_phase_folds_and_unfolds() {
        let mut shut = Folded::default();
        assert!(!shut.is(P::Tone));
        shut.toggle(P::Tone);
        assert!(shut.is(P::Tone));
        assert!(!shut.is(P::Rescue), "folding one folded another");
        assert!(shut.any());
        shut.toggle(P::Tone);
        assert!(!shut.any());
    }

    /// Synced, one fold answers for every track — which is what keeps
    /// the chain in register across a mixer.
    #[test]
    fn synced_folds_reach_every_track() {
        let mut fold = Fold::shared();
        fold.toggle("kick", P::Rescue);
        assert!(fold.of("kick").is(P::Rescue));
        assert!(
            fold.of("snare").is(P::Rescue),
            "the fold stayed on one track"
        );
    }

    /// And unsynced, it reaches only the one you folded.
    #[test]
    fn per_track_folds_stay_on_their_track() {
        let mut fold = Fold::default();
        fold.toggle("kick", P::Rescue);
        assert!(fold.of("kick").is(P::Rescue));
        assert!(!fold.of("snare").is(P::Rescue));
    }

    /// Going synced takes the track you were on as the answer for
    /// everyone, rather than throwing the fold away — the fold you just
    /// made is almost always the one you meant.
    #[test]
    fn syncing_adopts_the_track_you_were_on() {
        let mut fold = Fold::default();
        fold.toggle("kick", P::Polish);
        fold.sync(true, "kick");
        assert!(fold.of("snare").is(P::Polish));
        assert!(fold.of("anything at all").is(P::Polish));
    }
}

#[cfg(test)]
mod container_tests {
    use super::{ALL_PANELS, Folded, Grip, HEAD_H, Mods, Panel, Row, Which, chain, tall, units};
    use session::mix_phases::MixPhase as P;

    fn box_at() -> Panel {
        Panel {
            x: 0.0,
            y: 0.0,
            width: 133.0,
            height: 4000.0,
        }
    }

    /// Every phase gets exactly one container, and it sits above the
    /// units it caps.
    #[test]
    fn each_phase_gets_one_header_above_its_units() {
        let rows = chain(&ALL_PANELS, box_at(), Folded::default());
        let mut seen: Vec<P> = Vec::new();
        let mut head: Option<(P, f64)> = None;
        for (row, at) in &rows {
            match row {
                Row::Head(phase) => {
                    assert!(!seen.contains(phase), "{phase:?} got two containers");
                    seen.push(*phase);
                    head = Some((*phase, at.y));
                }
                Row::Unit(which) => {
                    let (phase, y) = head.expect("a unit before any container");
                    assert_eq!(which.phase(), phase, "{which:?} under {phase:?}");
                    assert!(at.y >= y, "{which:?} sat above its own container");
                }
                Row::Blank(phase) => {
                    panic!("the full chain has every phase, but {phase:?} came up blank")
                }
            }
        }
        assert_eq!(seen.len(), 5, "expected one container per phase: {seen:?}");
    }

    /// A return's presets are its slot's: a Short delay offers short
    /// delays, a Throw verb throws, and no list has another slot's
    /// default in it.
    #[test]
    fn a_return_offers_presets_for_its_own_slot() {
        let short = super::placeholder_for(super::Role::Delay, 1, "Short", &[]);
        let throw = super::placeholder_for(super::Role::Delay, 1, "Throw", &[]);
        assert!(short.delay.time < 300.0 && throw.delay.time > 500.0);
        assert!(short.presets.iter().all(|p| p.tone.delay.time < 300.0));
        assert!(throw.presets.iter().all(|p| p.tone.delay.time > 500.0));
        assert_eq!(short.preset, Some(0));
        let room = super::placeholder_for(super::Role::Reverb, 0, "Room", &[]);
        let moment = super::placeholder_for(super::Role::Reverb, 0, "Moment", &[]);
        assert!(room.presets.iter().all(|p| p.tone.reverb.decay < 1.0));
        assert!(moment.presets.iter().all(|p| p.tone.reverb.decay > 4.0));
        // A name that says nothing lands on the middle slot.
        let plain = super::placeholder_for(super::Role::Delay, 1, "Delay", &[]);
        assert!(plain.presets.len() >= 4);
        // The wheel walks the list and stops at its ends; a narrow row
        // keeps the loaded chip on it.
        let mut walk = short.clone();
        for _ in 0..10 {
            super::wheel(&mut walk, Grip::Preset(0), Mods::default(), 1.0);
        }
        assert_eq!(walk.preset, Some(walk.presets.len() - 1));
        let narrow = Panel {
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: super::PRESETS_H,
        };
        let chips = super::preset_chips(&walk, narrow);
        assert!(chips.iter().any(|(i, _)| Some(*i) == walk.preset));
        assert!(chips.len() < walk.presets.len());
        super::wheel(&mut walk, Grip::Preset(0), Mods::default(), -1.0);
        assert_eq!(walk.preset, Some(walk.presets.len() - 2));
    }

    /// A chain without a phase keeps the phase's row as a blank bar, so
    /// the phases after it start where they start on a full chain —
    /// and a chain with nothing in it gets no bars at all.
    #[test]
    fn a_missing_phase_is_a_blank_bar_at_the_same_height() {
        let bus = chain(&super::BUS_CHAIN, box_at(), Folded::rest());
        assert_eq!(
            bus.first().map(|(row, _)| *row),
            Some(Row::Blank(P::Rescue))
        );
        let tone_y = |rows: &[(Row, Panel)]| {
            rows.iter()
                .find(|(row, _)| *row == Row::Head(P::Tone))
                .map(|(_, at)| at.y)
                .expect("a Tone bar")
        };
        let full = chain(&ALL_PANELS, box_at(), Folded::rest());
        assert!((tone_y(&bus) - tone_y(&full)).abs() < f64::EPSILON);
        // Every phase is accounted for, blank or not.
        let bars = bus
            .iter()
            .filter(|(row, _)| matches!(row, Row::Head(_) | Row::Blank(_)))
            .count();
        assert_eq!(bars, super::RACK_PHASES.len());
        assert!(chain(&[], box_at(), Folded::rest()).is_empty());
        // A return is only Depth, and gets only Depth: no bars for the
        // phases a channel has and it never will.
        let delay = chain(&super::DELAY_CHAIN, box_at(), Folded::rest());
        assert!(!delay.iter().any(|(row, _)| matches!(row, Row::Blank(_))));
        assert_eq!(
            delay
                .iter()
                .filter(|(row, _)| matches!(row, Row::Head(_)))
                .count(),
            1
        );
    }

    /// Folding a phase takes its units out of the column and leaves its
    /// container — you have to be able to unfold it.
    #[test]
    fn folding_hides_the_units_and_keeps_the_bar() {
        let mut shut = Folded::default();
        shut.toggle(P::Tone);
        let rows = chain(&ALL_PANELS, box_at(), shut);
        assert!(
            rows.iter().any(|(row, _)| *row == Row::Head(P::Tone)),
            "the container went with its units"
        );
        for which in [Which::Eq, Which::Comp, Which::Sat] {
            assert!(
                !rows.iter().any(|(row, _)| *row == Row::Unit(which)),
                "{which:?} survived the fold"
            );
        }
        // And the rest of the chain is still there.
        assert!(rows.iter().any(|(row, _)| *row == Row::Unit(Which::Delay)));
    }

    /// A fold shortens the column by what it hid, which is what makes
    /// the gesture worth making — the chain gets nearer to fitting.
    #[test]
    fn folding_shortens_the_column() {
        let open = tall(&ALL_PANELS, Folded::default());
        let mut shut = Folded::default();
        shut.toggle(P::Tone);
        let folded = tall(&ALL_PANELS, shut);
        assert!(folded < open, "{folded} was not shorter than {open}");
        // By the units it hid, and not by the container itself.
        let hidden: f64 = [Which::Eq, Which::Comp, Which::Sat]
            .iter()
            .map(|w| w.natural() + super::GAP)
            .sum();
        assert!((open - folded - hidden).abs() < 0.01, "{open} - {folded}");
        assert!(HEAD_H > 0.0);
    }

    /// Folding everything leaves five bars and nothing else.
    #[test]
    fn folding_everything_leaves_the_bars() {
        let mut shut = Folded::default();
        for phase in P::ALL {
            shut.toggle(phase);
        }
        assert!(units(&ALL_PANELS, box_at(), shut).is_empty());
        assert_eq!(chain(&ALL_PANELS, box_at(), shut).len(), 5);
    }
}

#[cfg(test)]
mod phase_tests {
    use super::{Which, panels_for};
    use session::mix_phases::MixPhase as P;

    /// Every phase shows the whole chain.
    ///
    /// It used to show a subset per phase — the EQ alone in Rescue, no
    /// rack at all in Balance — so changing phase changed which
    /// processors EXISTED and a strip reorganised itself under you. A
    /// chain is a chain; a phase's job is to say which part of it to
    /// look at, which is focus rather than membership.
    #[test]
    fn every_phase_shows_the_whole_chain() {
        for phase in P::ALL {
            assert_eq!(
                panels_for(phase),
                super::ALL_PANELS,
                "{phase:?} showed a subset"
            );
        }
    }

    /// The chain is ordered BY phase, and every unit knows which one it
    /// belongs to — which is what the rail's buttons will focus once
    /// there is a setting for it. Out of order, "focus the Tone phase"
    /// would mean scrolling to two places at once.
    #[test]
    fn the_chain_runs_in_phase_order() {
        let order = |phase: P| P::ALL.iter().position(|p| *p == phase);
        let mut last = None;
        for which in super::ALL_PANELS {
            let at = order(which.phase()).expect("a phase in the canonical order");
            if let Some(before) = last {
                assert!(at >= before, "{which:?} is out of phase order");
            }
            last = Some(at);
        }
    }

    /// Every unit that draws a frequency response says so, because they
    /// share one hit test and one graph — and the ones that do not must
    /// not be routed into it.
    #[test]
    fn the_spectral_units_are_the_ones_with_curves() {
        for which in super::ALL_PANELS {
            let spectral = matches!(
                which,
                Which::RescueEq | Which::Eq | Which::Space | Which::DeEss | Which::PolishEq
            );
            assert_eq!(which.is_spectral(), spectral, "{which:?}");
        }
    }

    /// Every phase answers — a `match` that grew a hole would be a
    /// phase button that silently kept the previous rack.
    #[test]
    fn every_phase_answers() {
        for phase in P::ALL {
            let panels = panels_for(phase);
            assert!(!panels.is_empty(), "{phase:?} asked for {panels:?}");
        }
    }
}

#[cfg(test)]
mod grip_tests {
    use super::{Grip, Mods, Panel, Store, Which, drag, grip_at, placeholder};

    const ALL: [Which; 3] = [Which::Eq, Which::Comp, Which::Sat];

    fn rack() -> Panel {
        Panel {
            x: 0.0,
            y: 0.0,
            width: 133.0,
            height: 600.0,
        }
    }

    /// A band is grabbed where it is DRAWN. The layout is shared by the
    /// two, so this asserts they have not drifted apart — which is the
    /// only way a grip can move the wrong band.
    #[test]
    fn a_band_is_grabbed_where_it_is_drawn() {
        let tone = placeholder(0);
        let (which, at) = super::layout(&ALL, rack())
            .into_iter()
            .find(|(which, _)| *which == Which::Eq)
            .expect("an EQ panel");
        assert_eq!(which, Which::Eq);
        let body = super::body_of(at, super::Rack::at(rack().width));
        let freq = fts_audio_ui::axis::FreqAxis::audible();
        let db = fts_audio_ui::axis::DbAxis::symmetric(tone.eq_db_range());
        let band = &tone.eq[2];
        let x = freq.freq_to_x(f64::from(band.frequency), body.x, body.x + body.width);
        let y = db.db_to_y(f64::from(band.gain), body.y, body.y + body.height);
        assert_eq!(
            grip_at(&ALL, &tone, rack(), super::Folded::default(), x, y),
            Some(Grip::Band(Which::Eq, 2))
        );
    }

    /// And a point well away from every band grabs nothing, rather
    /// than the nearest band at any distance or the graph itself.
    ///
    /// It briefly answered `Scale`, so a wheel anywhere over the graph
    /// would zoom. The rack scrolls now, and a panel that swallowed the
    /// wheel would be a hole in the scroll the size of an EQ — so the
    /// empty graph belongs to the gesture that moves the chain, and the
    /// zoom keeps its chip.
    #[test]
    fn empty_space_in_the_eq_grabs_nothing() {
        let tone = placeholder(0);
        // Inside the EQ's own body, far from any band — below Rescue's
        // blank bar and the phase's own container bar, which own the
        // first thirty pixels, and below the unit's header row.
        let y = 2.0 * super::HEAD_H + 14.0;
        assert_eq!(
            grip_at(&ALL, &tone, rack(), super::Folded::default(), 3.0, y),
            None
        );
    }

    /// The zoom chip is at the top middle and wins over whatever is
    /// under it — a band that happened to sit there would otherwise
    /// take every click aimed at the chip.
    #[test]
    fn the_zoom_chip_takes_its_own_clicks() {
        let mut tone = placeholder(0);
        let (_, at) = super::layout(&ALL, rack())
            .into_iter()
            .find(|(which, _)| *which == Which::Eq)
            .expect("an EQ panel");
        let body = super::body_of(at, super::Rack::at(rack().width));
        let chip = super::scale_chip(body);
        assert_eq!(
            grip_at(
                &ALL,
                &tone,
                rack(),
                super::Folded::default(),
                chip.x + chip.width / 2.0,
                chip.y + 2.0
            ),
            Some(Grip::Scale(Which::Eq))
        );
        // It is a switch, so a click acts and a drag does not.
        assert!(Grip::Scale(Which::Eq).is_switch());

        // Clicking cycles the stops and comes back round.
        let start = tone.eq_db_range();
        let steps = eq_ui::eq_graph_model::DB_RANGE_STEPS.len();
        for _ in 0..steps {
            tone.cycle_eq_range();
        }
        assert!((tone.eq_db_range() - start).abs() < f64::EPSILON);
    }

    /// The default is the plugin's own (±6 dB), the wheel walks the
    /// plugin's own stops, and neither end runs away.
    #[test]
    fn the_zoom_walks_the_plugins_stops() {
        let mut tone = placeholder(0);
        assert!(
            (tone.eq_db_range() - eq_ui::eq_graph_model::DEFAULT_DB_RANGE).abs() < f64::EPSILON
        );

        // Down is out, which is the way a wheel zooms out everywhere.
        super::wheel(&mut tone, Grip::Scale(Which::Eq), Mods::default(), -1.0);
        let out = tone.eq_db_range();
        assert!(out > eq_ui::eq_graph_model::DEFAULT_DB_RANGE, "{out}");
        super::wheel(&mut tone, Grip::Scale(Which::Eq), Mods::default(), 1.0);
        assert!(
            (tone.eq_db_range() - eq_ui::eq_graph_model::DEFAULT_DB_RANGE).abs() < f64::EPSILON
        );

        for _ in 0..20 {
            super::wheel(&mut tone, Grip::Scale(Which::Eq), Mods::default(), -1.0);
        }
        let widest = *eq_ui::eq_graph_model::DB_RANGE_STEPS
            .last()
            .expect("a widest stop");
        assert!((tone.eq_db_range() - widest).abs() < f64::EPSILON);
        for _ in 0..20 {
            super::wheel(&mut tone, Grip::Scale(Which::Eq), Mods::default(), 1.0);
        }
        let tightest = eq_ui::eq_graph_model::DB_RANGE_STEPS[0];
        assert!((tone.eq_db_range() - tightest).abs() < f64::EPSILON);

        // And a double-click puts it back where it started.
        super::reset(&mut tone, Grip::Scale(Which::Eq));
        assert!(
            (tone.eq_db_range() - eq_ui::eq_graph_model::DEFAULT_DB_RANGE).abs() < f64::EPSILON
        );
    }

    /// Zooming the graph must not narrow what the EQ can DO. The
    /// display range and the parameter's limit parted company when the
    /// graph learned to zoom, and a clamp tied to the view would mean
    /// zooming in silently capped the gain.
    #[test]
    fn zooming_in_does_not_clamp_the_gain() {
        let mut tone = placeholder(0);
        tone.eq_range = 0;
        for _ in 0..200 {
            super::wheel(&mut tone, Grip::Band(Which::Eq, 0), Mods::default(), 1.0);
        }
        assert!(
            f64::from(tone.eq[0].gain) > eq_ui::eq_graph_model::DB_RANGE_STEPS[0],
            "a ±3 view capped the gain at ±3: {}",
            tone.eq[0].gain
        );
        assert!(f64::from(tone.eq[0].gain) <= super::EQ_GAIN_LIMIT);
    }

    /// Dragging a band up raises its gain and dragging it right raises
    /// its frequency — the two axes it is drawn against.
    #[test]
    fn a_band_follows_the_pointer() {
        let mut tone = placeholder(0);
        let (before_f, before_g) = (tone.eq[1].frequency, tone.eq[1].gain);
        drag(
            &mut tone,
            Grip::Band(Which::Eq, 1),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            12.0,
            -20.0,
        );
        assert!(tone.eq[1].frequency > before_f, "right is higher");
        assert!(tone.eq[1].gain > before_g, "up is more gain");
    }

    /// A band cannot be dragged out of its own axes.
    #[test]
    fn a_band_stays_inside_the_panel() {
        let mut tone = placeholder(0);
        for _ in 0..50 {
            drag(
                &mut tone,
                Grip::Band(Which::Eq, 0),
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                400.0,
                -400.0,
            );
        }
        assert!(tone.eq[0].gain <= super::f64_to_f32(super::EQ_GAIN_LIMIT));
        assert!(tone.eq[0].frequency <= 24_000.0);
        for _ in 0..50 {
            drag(
                &mut tone,
                Grip::Band(Which::Eq, 0),
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                -400.0,
                400.0,
            );
        }
        assert!(tone.eq[0].gain >= -super::f64_to_f32(super::EQ_GAIN_LIMIT));
        assert!(tone.eq[0].frequency > 0.0);
    }

    /// Up is a higher threshold — less compression — which is the way
    /// a fader moves for more level.
    #[test]
    fn dragging_the_threshold_up_compresses_less() {
        let mut tone = placeholder(0);
        let before = tone.comp.threshold;
        drag(
            &mut tone,
            Grip::Threshold(Which::Comp),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            0.0,
            -10.0,
        );
        assert!(tone.comp.threshold > before);
        for _ in 0..200 {
            drag(
                &mut tone,
                Grip::Threshold(Which::Comp),
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                0.0,
                40.0,
            );
        }
        assert!(tone.comp.threshold >= -60.0, "the threshold clamps");
    }

    /// Drive clamps at zero rather than going negative, which would
    /// invert the curve.
    #[test]
    fn drive_stays_positive() {
        let mut tone = placeholder(0);
        for _ in 0..200 {
            drag(
                &mut tone,
                Grip::Drive,
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                0.0,
                40.0,
            );
        }
        assert!(tone.sat.drive >= 0.0);
    }

    /// The store hands a track its own settings and keeps them apart —
    /// keyed by GUID, so a preset hiding a track cannot shuffle them.
    #[test]
    fn the_store_keeps_tracks_apart() {
        let rows: Vec<(daw_proto::Track, u32)> = ["a", "b"]
            .into_iter()
            .map(|guid| {
                (
                    daw_proto::Track {
                        guid: guid.to_owned(),
                        ..daw_proto::Track::default()
                    },
                    0,
                )
            })
            .collect();
        let mut store = Store::default();
        store.seed(&rows);
        store.edit("a").expect("a's settings").comp.threshold = -3.0;
        assert!((store.get("a").expect("a").comp.threshold + 3.0).abs() < f32::EPSILON);
        assert!(store.get("b").expect("b").comp.threshold < -3.0);
        assert!(store.get("nonesuch").is_none());
    }

    /// Seeding twice does not reset what was edited in between — the
    /// rows are re-derived on every fold and every preset.
    #[test]
    fn seeding_again_keeps_edits() {
        let rows: Vec<(daw_proto::Track, u32)> = vec![(
            daw_proto::Track {
                guid: "a".to_owned(),
                ..daw_proto::Track::default()
            },
            0,
        )];
        let mut store = Store::default();
        store.seed(&rows);
        store.edit("a").expect("a").sat.drive = 9.0;
        store.seed(&rows);
        assert!((store.get("a").expect("a").sat.drive - 9.0).abs() < f32::EPSILON);
    }

    /// The golden session's rows, by path, seeded.
    fn golden_store() -> (Store, Vec<(String, String)>) {
        let flats = dynamic_template::golden_session::rpp::flatten(
            &dynamic_template::golden_session::maximal(),
        );
        let rows: Vec<(daw_proto::Track, u32)> = flats
            .iter()
            .map(|flat| {
                (
                    daw_proto::Track {
                        guid: flat.guid.clone(),
                        name: flat.name.clone(),
                        is_folder: flat.is_folder,
                        ..daw_proto::Track::default()
                    },
                    flat.depth,
                )
            })
            .collect();
        let mut store = Store::default();
        store.seed(&rows);
        let paths = flats
            .iter()
            .map(|flat| (flat.path.clone(), flat.guid.clone()))
            .collect();
        (store, paths)
    }

    /// The electric's chain is on its layers — Rhythm, Main, Octave,
    /// Lead, and Solo's Main and Harmony — and nowhere else in it: the
    /// channels, the amps, the mics and the DI carry nothing, and Solo
    /// is only the folder over its two lines.
    #[test]
    fn the_electric_is_processed_on_its_layers() {
        let (store, paths) = golden_store();
        let chain = |path: &str| {
            let guid = &paths
                .iter()
                .find(|(p, _)| p == path)
                .unwrap_or_else(|| panic!("no {path}"))
                .1;
            store
                .get(guid)
                .map(|tone| tone.role.panels() != Some(&[]))
                .unwrap_or_else(|| panic!("{path} was not seeded"))
        };
        for layer in [
            "Electric/Rhythm",
            "Electric/Rhythm/Main",
            "Electric/Rhythm/Octave",
            "Electric/Lead",
            "Electric/Solo/Main",
            "Electric/Solo/Harmony",
        ] {
            assert!(chain(layer), "{layer} lost its chain");
        }
        let mut bare = 0;
        for (path, _) in &paths {
            let Some(rest) = path.strip_prefix("Electric/") else {
                continue;
            };
            if matches!(
                rest,
                "Rhythm" | "Rhythm/Main" | "Rhythm/Octave" | "Lead" | "Solo/Main" | "Solo/Harmony"
            ) {
                continue;
            }
            assert!(!chain(path), "{path} carries a chain");
            bare += 1;
        }
        // Solo, Lead's L and R, and four channels of a folder, two amps
        // and six sources each.
        assert_eq!(bare, 1 + 2 + 4 * 9);
        // The negative control: the same names elsewhere keep theirs —
        // a lead vocal's Main layer, the piano.
        assert!(chain("Vocals/Ron/Main"), "the vocal's Main lost its chain");
        assert!(chain("Keys/Piano"), "the piano lost its chain");
    }
}

#[cfg(test)]
mod tier_tests {
    use super::{Grip, Panel, Which, grip_at, placeholder};

    const ALL: [Which; 3] = [Which::Eq, Which::Comp, Which::Sat];

    fn rack_of(width: f64) -> Panel {
        Panel {
            x: 0.0,
            y: 0.0,
            width,
            height: 600.0,
        }
    }

    /// A handle you cannot see is a handle you cannot aim at. Below the
    /// tier that draws them, the EQ panel grabs nothing — grabbing a
    /// band by accident moves a setting you did not know was there.
    #[test]
    fn bands_are_only_grabbable_where_they_are_drawn() {
        let tone = placeholder(0);
        let wide = rack_of(133.0);
        let narrow = rack_of(super::SHAPE + 1.0);
        assert_eq!(super::Rack::at(narrow.width), super::Rack::Curves);

        // The same band, in both tiers.
        let (_, at) = super::layout(&ALL, wide)
            .into_iter()
            .find(|(which, _)| *which == Which::Eq)
            .expect("an EQ panel");
        let body = super::body_of(at, super::Rack::Full);
        let freq = fts_audio_ui::axis::FreqAxis::audible();
        let db = fts_audio_ui::axis::DbAxis::symmetric(tone.eq_db_range());
        let band = &tone.eq[2];
        let y = db.db_to_y(f64::from(band.gain), body.y, body.y + body.height);

        let x_wide = freq.freq_to_x(f64::from(band.frequency), body.x, body.x + body.width);
        assert_eq!(
            grip_at(&ALL, &tone, wide, super::Folded::default(), x_wide, y),
            Some(Grip::Band(Which::Eq, 2))
        );

        let narrow_body = super::body_of(
            super::layout(&ALL, narrow)
                .into_iter()
                .find(|(which, _)| *which == Which::Eq)
                .expect("an EQ panel")
                .1,
            super::Rack::Curves,
        );
        let x_narrow = freq.freq_to_x(
            f64::from(band.frequency),
            narrow_body.x,
            narrow_body.x + narrow_body.width,
        );
        assert_eq!(
            grip_at(&ALL, &tone, narrow, super::Folded::default(), x_narrow, y),
            None
        );
    }

    /// The compressor and the saturator stay grabbable when the rack
    /// narrows: their curves still show what the drag is doing.
    #[test]
    fn the_single_value_panels_survive_the_narrow_tier() {
        let tone = placeholder(0);
        let narrow = rack_of(super::SHAPE + 1.0);
        let comp = super::layout(&ALL, narrow)
            .into_iter()
            .find(|(which, _)| *which == Which::Comp)
            .expect("a comp panel")
            .1;
        let inside = comp.y + comp.height / 2.0;
        assert_eq!(
            grip_at(
                &ALL,
                &tone,
                narrow,
                super::Folded::default(),
                narrow.width / 2.0,
                inside
            ),
            Some(Grip::Threshold(Which::Comp))
        );
    }

    /// A rail's rack is read, not touched: it grabs nothing.
    #[test]
    fn a_rails_rack_grabs_nothing() {
        let tone = placeholder(0);
        let rail = rack_of(30.0);
        assert_eq!(super::Rack::at(rail.width), super::Rack::Minimal);
        assert_eq!(
            grip_at(&ALL, &tone, rail, super::Folded::default(), 15.0, 300.0),
            None
        );
        assert_eq!(super::Rack::at(10.0), super::Rack::Off);
    }

    /// And it draws something for every unit, so a rail never says its
    /// track carries nothing — more with a signal than without.
    #[test]
    fn a_rails_rack_draws_every_unit() {
        let tone = placeholder(2);
        let palette = crate::arrangement::Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = crate::text::Font::embedded().expect("the embedded font");
        let rail = Panel {
            x: 0.0,
            y: 0.0,
            width: 26.0,
            height: 900.0,
        };
        let rows = super::chain(&super::ALL_PANELS, rail, super::Folded::default());
        // The same rows as the strip beside it: a row per unit and one
        // per container, at the same heights.
        let wide = Panel {
            width: 133.0,
            ..rail
        };
        let full = super::chain(&super::ALL_PANELS, wide, super::Folded::default());
        assert_eq!(
            rows.len(),
            full.len(),
            "a rail's chain is the strip's chain"
        );
        for ((a, at), (b, full_at)) in rows.iter().zip(full.iter()) {
            assert_eq!(a, b);
            assert!(
                (at.y - full_at.y).abs() < f64::EPSILON,
                "{a:?} at {} vs {}",
                at.y,
                full_at.y
            );
        }
        let mut still = anyrender::Scene::new();
        super::draw(
            &mut still,
            &palette,
            &font,
            &tone,
            &crate::live::Meters::default(),
            &super::ALL_PANELS,
            rail,
            super::Folded::default(),
            None,
            None,
        );
        // A track per unit, and an indicator for every unit whose
        // setting shows with no signal — the EQs, the gate's light, the
        // heat, the repeats, the tail. The bars (reduction, fire) wait
        // for a signal.
        assert!(
            still.commands.len() > rows.len() + 6,
            "a track and the still indicators: {}",
            still.commands.len()
        );
        let mut moving = anyrender::Scene::new();
        let meters = crate::simulate::meters(2, 1.25, &tone);
        super::draw(
            &mut moving,
            &palette,
            &font,
            &tone,
            &meters,
            &super::ALL_PANELS,
            rail,
            super::Folded::default(),
            None,
            None,
        );
        assert!(moving.commands.len() >= still.commands.len());
    }
}

#[cfg(test)]
mod reset_tests {
    use super::{Comp, Grip, Mods, Which, drag, placeholder, reset};

    const ALL: [Which; 3] = [Which::Eq, Which::Comp, Which::Sat];

    fn rack() -> super::Panel {
        super::Panel {
            x: 0.0,
            y: 0.0,
            width: 133.0,
            height: 600.0,
        }
    }

    /// Reset undoes a drag, whatever the drag was — which is the whole
    /// promise: you can move something to find out what it does.
    #[test]
    fn reset_undoes_a_drag() {
        let mut tone = placeholder(0);
        let before = tone.eq[1].gain;
        drag(
            &mut tone,
            Grip::Band(Which::Eq, 1),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            0.0,
            -40.0,
        );
        assert!(
            (tone.eq[1].gain - before).abs() > 0.5,
            "the drag moved nothing"
        );
        reset(&mut tone, Grip::Band(Which::Eq, 1));
        assert!(tone.eq[1].gain.abs() < f32::EPSILON, "the band went flat");

        drag(
            &mut tone,
            Grip::Threshold(Which::Comp),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            0.0,
            -30.0,
        );
        reset(&mut tone, Grip::Threshold(Which::Comp));
        assert!(
            (tone.comp.threshold - Comp::default().threshold).abs() < f32::EPSILON,
            "the threshold went back to its default"
        );

        drag(
            &mut tone,
            Grip::Drive,
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            0.0,
            -60.0,
        );
        reset(&mut tone, Grip::Drive);
        assert!(
            (tone.sat.drive - 1.0).abs() < f32::EPSILON,
            "drive is unity"
        );
    }

    /// A band reset keeps its FREQUENCY: where you put it is not the
    /// decision, how much you did there is.
    #[test]
    fn resetting_a_band_keeps_where_it_sits() {
        let mut tone = placeholder(0);
        drag(
            &mut tone,
            Grip::Band(Which::Eq, 2),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            20.0,
            -20.0,
        );
        let moved = tone.eq[2].frequency;
        reset(&mut tone, Grip::Band(Which::Eq, 2));
        assert!((tone.eq[2].frequency - moved).abs() < f32::EPSILON);
    }

    /// A band that is not there is not a panic.
    #[test]
    fn resetting_a_missing_band_is_harmless() {
        let mut tone = placeholder(0);
        reset(&mut tone, Grip::Band(Which::Eq, 99));
    }
}

#[cfg(test)]
mod plugin_interaction_tests {
    use super::{
        EQ_GAIN_LIMIT, Grip, Mods, Panel, Which, dot_click, drag, grip_at, placeholder, wheel,
    };
    use eq_ui::eq_graph_model::EqBandShape;

    const ALL: [Which; 3] = [Which::Eq, Which::Comp, Which::Sat];

    fn rack() -> Panel {
        Panel {
            x: 7.0,
            y: 11.0,
            width: 133.0,
            height: 600.0,
        }
    }

    /// The rack asks the PLUGIN which band you meant, so a rack and an
    /// editor cannot disagree about it. This pins the delegation: the
    /// answer here is `nearest_band`'s answer, offset by the panel.
    #[test]
    fn the_hit_test_is_the_plugins_own() {
        let tone = placeholder(0);
        let (_, at) = super::layout(&ALL, rack())
            .into_iter()
            .find(|(which, _)| *which == Which::Eq)
            .expect("an EQ panel");
        let body = super::body_of(at, super::Rack::at(rack().width));
        let map = super::mapper(body, tone.eq_db_range());
        for index in [0_usize, 2, 3] {
            let band = &tone.eq[index];
            let x = body.x + map.freq_to_x(f64::from(band.frequency));
            let y = body.y + map.db_to_y(f64::from(band.gain));
            assert_eq!(
                grip_at(&ALL, &tone, rack(), super::Folded::default(), x, y),
                Some(Grip::Band(Which::Eq, index)),
                "band {index} was not found where the plugin puts it"
            );
        }
    }

    /// Alt pins the gain: you found the right amount of cut and now you
    /// are hunting for where it belongs. This is the plugin's rule, and
    /// it used to be "free in both axes, always".
    #[test]
    fn alt_moves_a_band_in_frequency_alone() {
        let mut tone = placeholder(0);
        let (before_f, before_g) = (tone.eq[1].frequency, tone.eq[1].gain);
        let alt = Mods::new(true, false, false);
        drag(
            &mut tone,
            Grip::Band(Which::Eq, 1),
            &ALL,
            rack(),
            super::Folded::default(),
            alt,
            15.0,
            -30.0,
        );
        assert!(tone.eq[1].frequency > before_f, "frequency followed");
        assert!(
            (tone.eq[1].gain - before_g).abs() < f32::EPSILON,
            "gain was pinned"
        );
    }

    /// Cmd turns a vertical drag into resonance, which is the one
    /// gesture that changes a parameter the curve shows only indirectly.
    #[test]
    fn cmd_drags_resonance() {
        let mut tone = placeholder(0);
        let before = tone.eq[1].q;
        let cmd = Mods::new(false, false, true);
        drag(
            &mut tone,
            Grip::Band(Which::Eq, 1),
            &ALL,
            rack(),
            super::Folded::default(),
            cmd,
            0.0,
            -20.0,
        );
        assert!((tone.eq[1].q - before).abs() > f32::EPSILON, "q moved");
        assert!(
            tone.eq[1].q > 0.0 && tone.eq[1].q <= 18.0,
            "and stayed sane"
        );
    }

    /// Shift is the fine-tune modifier everywhere, including the two
    /// panels that are not the EQ.
    #[test]
    fn shift_is_fine_everywhere() {
        let coarse = {
            let mut tone = placeholder(0);
            drag(
                &mut tone,
                Grip::Threshold(Which::Comp),
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                0.0,
                -20.0,
            );
            tone.comp.threshold
        };
        let fine = {
            let mut tone = placeholder(0);
            let shift = Mods::new(false, true, false);
            drag(
                &mut tone,
                Grip::Threshold(Which::Comp),
                &ALL,
                rack(),
                super::Folded::default(),
                shift,
                0.0,
                -20.0,
            );
            tone.comp.threshold
        };
        let from = placeholder(0).comp.threshold;
        assert!(
            (fine - from).abs() < (coarse - from).abs(),
            "fine moved {} where coarse moved {}",
            fine - from,
            coarse - from
        );
    }

    /// An unmodified wheel over a bell moves its Q — the plugin's
    /// default, because a bell's meaningful width is its resonance.
    #[test]
    fn the_wheel_follows_the_band() {
        let mut tone = placeholder(0);
        tone.eq[1].shape = EqBandShape::Bell;
        let before = tone.eq[1].q;
        wheel(&mut tone, Grip::Band(Which::Eq, 1), Mods::default(), -1.0);
        assert!(tone.eq[1].q > before, "scrolling up tightened it");
    }

    /// And Cmd+wheel moves the gain instead.
    #[test]
    fn cmd_wheel_moves_the_gain() {
        let mut tone = placeholder(0);
        let before = tone.eq[1].gain;
        wheel(
            &mut tone,
            Grip::Band(Which::Eq, 1),
            Mods::new(false, false, true),
            -1.0,
        );
        assert!(tone.eq[1].gain > before);
        assert!(tone.eq[1].gain <= super::f64_to_f32(EQ_GAIN_LIMIT));
    }

    /// Alt-clicking a band bypasses it, which is the chord the editor
    /// uses and the one that makes A/B-ing a decision possible.
    #[test]
    fn alt_click_bypasses_a_band() {
        let mut tone = placeholder(0);
        assert!(tone.eq[0].enabled);
        assert!(dot_click(
            &mut tone,
            Which::Eq,
            0,
            Mods::new(true, false, false)
        ));
        assert!(!tone.eq[0].enabled);
        assert!(dot_click(
            &mut tone,
            Which::Eq,
            0,
            Mods::new(true, false, false)
        ));
        assert!(tone.eq[0].enabled);
    }

    /// A plain click is not an action — it is the start of a drag, and
    /// has to fall through so the caller can take hold of the band.
    #[test]
    fn a_plain_click_is_not_an_action() {
        let mut tone = placeholder(0);
        assert!(!dot_click(&mut tone, Which::Eq, 0, Mods::default()));
        assert_eq!(tone.eq[0], placeholder(0).eq[0], "nothing changed");
    }

    /// Alt+Cmd cycles the shape, and the cycle comes back round.
    #[test]
    fn alt_cmd_cycles_the_shape() {
        let mut tone = placeholder(0);
        tone.eq[1].shape = EqBandShape::Bell;
        let chord = Mods::new(true, false, true);
        let mut seen = vec![tone.eq[1].shape];
        for _ in 0..4 {
            assert!(dot_click(&mut tone, Which::Eq, 1, chord));
            seen.push(tone.eq[1].shape);
        }
        assert_eq!(seen.first(), seen.last(), "the cycle returned");
        assert!(seen.len() > 2, "and it passed through others");
    }
}

#[cfg(test)]
mod colour_tests {
    use super::band_color;

    /// The marker's colour is the plugin's, not one invented here — a
    /// band has to be the same colour in the strip as it is in the
    /// plugin window, or the colour is telling you two things.
    #[test]
    fn a_band_takes_the_plugins_own_hue() {
        for hz in [40.0, 250.0, 1_000.0, 4_000.0, 16_000.0] {
            let hex = eq_ui::eq_graph_model::freq_to_color(hz);
            let expected = hex.trim_start_matches('#');
            let got = band_color(hz);
            let [r, g, b, _] = got.to_rgba8().to_u8_array();
            assert_eq!(
                format!("{r:02x}{g:02x}{b:02x}"),
                expected,
                "{hz} Hz did not match the plugin"
            );
        }
    }

    /// And it sweeps, so two bands an octave apart are told apart by it.
    #[test]
    fn the_hue_moves_across_the_spectrum() {
        let low = band_color(60.0).to_rgba8().to_u8_array();
        let high = band_color(12_000.0).to_rgba8().to_u8_array();
        assert_ne!(low, high, "the low and the air band look the same");
    }
}

#[cfg(test)]
mod comp_tests {
    use super::{Comp, Grip, Mods, Panel, Rack, Which, drag, grip_at, placeholder, reset};

    const ALL: [Which; 3] = [Which::Eq, Which::Comp, Which::Sat];

    fn rack() -> Panel {
        Panel {
            x: 0.0,
            y: 0.0,
            width: 133.0,
            height: 600.0,
        }
    }

    fn comp_panel() -> Panel {
        let at = super::layout(&ALL, rack())
            .into_iter()
            .find(|(which, _)| *which == Which::Comp)
            .expect("a comp panel")
            .1;
        super::body_of(at, Rack::at(rack().width))
    }

    /// The threshold is a line across the display, so the display is
    /// what you grab — a line one pixel tall is not something you aim
    /// at, the way a fader's groove is the fader.
    #[test]
    fn the_display_grabs_the_threshold() {
        let tone = placeholder(0);
        let body = comp_panel();
        // Mid-height and left of centre: the arrow hangs on the left
        // rail and the strips run along the floor, so the middle of the
        // display is what is being claimed here.
        let inside = body.y + body.height * 0.55;
        assert_eq!(
            grip_at(
                &ALL,
                &tone,
                rack(),
                super::Folded::default(),
                body.x + body.width * 0.3,
                inside
            ),
            Some(Grip::Threshold(Which::Comp))
        );
    }

    /// The settings are grabbed where they are drawn: the attack strip
    /// and the release strip along the floor, the arrow off the line
    /// for the ratio. Away from all three the display is still the
    /// threshold's.
    #[test]
    fn the_glyph_grabs_its_own_parts() {
        let mut tone = placeholder(0);
        tone.comp.threshold = -18.0;
        tone.comp.ratio = 6.0;
        tone.comp.attack = 20.0;
        tone.comp.release = 200.0;
        let display = super::comp_split(comp_panel(), Rack::at(rack().width));
        let level = super::threshold_y(tone.comp, display);
        let strips = super::Strips::of(display).expect("strips to grab");
        for (grip, strip) in [
            (Grip::Attack(Which::Comp), strips.attack),
            (Grip::Release(Which::Comp), strips.release),
        ] {
            let mid = strip.center();
            assert_eq!(
                grip_at(&ALL, &tone, rack(), super::Folded::default(), mid.x, mid.y),
                Some(grip),
                "{grip:?} at {mid:?}"
            );
        }
        // Dragging left on the attack strip is faster, and the default
        // sits in the middle of the strip.
        let was = tone.comp.attack;
        drag(
            &mut tone,
            Grip::Attack(Which::Comp),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            -15.0,
            0.0,
        );
        assert!(
            tone.comp.attack < was,
            "left is fast: {} vs {was}",
            tone.comp.attack
        );
        assert!((super::Time::attack(super::Comp::default().attack).place() - 0.5).abs() < 1e-9);
        assert!((super::Time::release(super::Comp::default().release).place() - 0.5).abs() < 1e-9);
        // The ratio's arrow hangs off the threshold line near the left
        // edge, and is grabbed along its length.
        let arrow = super::Arrow::of(tone.comp, display);
        assert!((arrow.top - level).abs() < f64::EPSILON);
        assert!(arrow.tip > arrow.top);
        assert_eq!(
            grip_at(
                &ALL,
                &tone,
                rack(),
                super::Folded::default(),
                arrow.x,
                (arrow.top + arrow.tip) / 2.0
            ),
            Some(Grip::Ratio(Which::Comp))
        );
        assert!((super::Time::ratio(super::Comp::default().ratio).place() - 0.5).abs() < 1e-9);
        // Right of the arrow and above the time strips is nothing but
        // the display, which belongs to the threshold.
        assert_eq!(
            grip_at(
                &ALL,
                &tone,
                rack(),
                super::Folded::default(),
                arrow.x + 30.0,
                (arrow.top + arrow.tip) / 2.0
            ),
            Some(Grip::Threshold(Which::Comp))
        );
    }

    /// Dragging the threshold DOWN lowers it, because it is drawn on an
    /// axis where down is quieter — the one gesture where following the
    /// pointer is the whole point.
    #[test]
    fn the_threshold_follows_the_pointer_down() {
        let mut tone = placeholder(0);
        let before = tone.comp.threshold;
        drag(
            &mut tone,
            Grip::Threshold(Which::Comp),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            0.0,
            20.0,
        );
        assert!(tone.comp.threshold < before, "down is a lower threshold");
        drag(
            &mut tone,
            Grip::Threshold(Which::Comp),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            0.0,
            -40.0,
        );
        assert!(tone.comp.threshold > before, "and up is a higher one");
    }

    /// Each edge moves its own parameter and nothing else — including
    /// the threshold, which shares the display with all three.
    #[test]
    fn each_edge_moves_one_thing() {
        for grip in [
            Grip::Ratio(Which::Comp),
            Grip::Attack(Which::Comp),
            Grip::Release(Which::Comp),
        ] {
            let mut tone = placeholder(0);
            let was = tone.comp;
            drag(
                &mut tone,
                grip,
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                30.0,
                30.0,
            );
            let now = tone.comp;
            let moved = [
                (now.ratio - was.ratio).abs() > f32::EPSILON,
                (now.attack - was.attack).abs() > f32::EPSILON,
                (now.release - was.release).abs() > f32::EPSILON,
            ];
            assert_eq!(moved.iter().filter(|m| **m).count(), 1, "{grip:?}");
            assert!(
                (now.threshold - was.threshold).abs() < f32::EPSILON,
                "{grip:?} moved the threshold"
            );
        }
    }

    /// Attack and release are logarithmic — a millisecond matters at
    /// the fast end and twenty do not at the slow — so the same drag
    /// moves far less down there.
    #[test]
    fn the_times_are_logarithmic() {
        let step = |from: f32| {
            let mut tone = placeholder(0);
            tone.comp.attack = from;
            drag(
                &mut tone,
                Grip::Attack(Which::Comp),
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                15.0,
                0.0,
            );
            tone.comp.attack - from
        };
        assert!(
            step(1.0) < step(100.0),
            "the fast end moves in smaller steps"
        );
    }

    /// Every control clamps to its own range rather than running away.
    ///
    /// Each is driven in the direction its own control runs: the times
    /// are strips with the slow end at the right, so right is longer;
    /// the floor hangs, so down is harder.
    #[test]
    fn the_controls_clamp() {
        let mut tone = placeholder(0);
        for _ in 0..80 {
            drag(
                &mut tone,
                Grip::Ratio(Which::Comp),
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                0.0,
                60.0,
            );
            drag(
                &mut tone,
                Grip::Attack(Which::Comp),
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                60.0,
                0.0,
            );
            drag(
                &mut tone,
                Grip::Release(Which::Comp),
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                60.0,
                0.0,
            );
        }
        assert!((tone.comp.ratio - 20.0).abs() < 0.01, "{}", tone.comp.ratio);
        assert!(
            (tone.comp.attack - 200.0).abs() < 0.5,
            "{}",
            tone.comp.attack
        );
        assert!(
            (tone.comp.release - 3_000.0).abs() < 5.0,
            "{}",
            tone.comp.release
        );

        let mut back = placeholder(0);
        for _ in 0..80 {
            drag(
                &mut back,
                Grip::Ratio(Which::Comp),
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                0.0,
                -60.0,
            );
            drag(
                &mut back,
                Grip::Attack(Which::Comp),
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                -60.0,
                0.0,
            );
            drag(
                &mut back,
                Grip::Release(Which::Comp),
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                -60.0,
                0.0,
            );
        }
        assert!((back.comp.ratio - 1.0).abs() < 0.01, "{}", back.comp.ratio);
        assert!(
            (back.comp.attack - 0.1).abs() < 0.01,
            "{}",
            back.comp.attack
        );
        assert!(
            (back.comp.release - 5.0).abs() < 0.01,
            "{}",
            back.comp.release
        );
    }

    /// Each control moves the way its own control runs: you pull the
    /// floor DOWN for a harder ratio, because down is more reduction
    /// and that is the direction the signal goes; and you drag either
    /// time to the RIGHT for a longer one, because the strips run fast
    /// to slow with the default in the middle.
    #[test]
    fn each_control_follows_its_own_edge() {
        let was = placeholder(0).comp;

        let mut tone = placeholder(0);
        drag(
            &mut tone,
            Grip::Ratio(Which::Comp),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            0.0,
            20.0,
        );
        assert!(tone.comp.ratio > was.ratio, "down did not harden the ratio");

        let mut tone = placeholder(0);
        drag(
            &mut tone,
            Grip::Attack(Which::Comp),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            20.0,
            0.0,
        );
        assert!(
            tone.comp.attack > was.attack,
            "right did not lengthen the attack"
        );

        let mut tone = placeholder(0);
        drag(
            &mut tone,
            Grip::Release(Which::Comp),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            20.0,
            0.0,
        );
        assert!(
            tone.comp.release > was.release,
            "right did not lengthen the release"
        );
    }

    /// The glyph's depth is how much the ratio takes off a full-scale
    /// signal, so it grows with the ratio and vanishes at unity — where
    /// the compressor is taking nothing off and a drawn reduction would
    /// be claiming otherwise.
    #[test]
    fn the_depth_measures_what_the_ratio_does() {
        let at = |ratio: f32| {
            let mut comp = Comp::default();
            comp.threshold = -20.0;
            comp.ratio = ratio;
            super::ratio_drop(comp, 150.0)
        };
        assert!(at(1.0) < 0.01, "unity drew a reduction");
        assert!(at(4.0) > at(2.0));
        assert!(at(12.0) > at(4.0));
        // And it saturates, which is what the effect does — the header
        // carries the number for the cases where that matters.
        assert!(at(20.0) - at(12.0) < at(4.0) - at(2.0));
    }

    /// The line lands where the pointer put it. One split decides
    /// where the display is, so the pixel the line is drawn at and the
    /// decibel the drag produces cannot disagree — a threshold running
    /// ahead of the line you are holding is the bug this rules out.
    #[test]
    fn the_threshold_tracks_the_pointer() {
        let mut tone = placeholder(0);
        let body = comp_panel();
        let rack_tier = Rack::at(rack().width);
        let display = super::comp_split(body, rack_tier);
        let at_db = |comp: Comp| {
            display.y + comp_ui::comp_graph_svg::db_to_y(f64::from(comp.threshold), display.height)
        };
        let before = at_db(tone.comp);
        let moved = 24.0;
        drag(
            &mut tone,
            Grip::Threshold(Which::Comp),
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            0.0,
            moved,
        );
        let after = at_db(tone.comp);
        assert!(
            (after - before - moved).abs() < 0.5,
            "the line moved {} for a drag of {moved}",
            after - before
        );
    }

    /// And each resets to its own default.
    #[test]
    fn each_knob_resets_to_its_own_default() {
        let mut tone = placeholder(0);
        for grip in [
            Grip::Ratio(Which::Comp),
            Grip::Attack(Which::Comp),
            Grip::Release(Which::Comp),
            Grip::Threshold(Which::Comp),
        ] {
            drag(
                &mut tone,
                grip,
                &ALL,
                rack(),
                super::Folded::default(),
                Mods::default(),
                0.0,
                -25.0,
            );
            reset(&mut tone, grip);
        }
        let default = Comp::default();
        assert!((tone.comp.ratio - default.ratio).abs() < f32::EPSILON);
        assert!((tone.comp.attack - default.attack).abs() < f32::EPSILON);
        assert!((tone.comp.release - default.release).abs() < f32::EPSILON);
        assert!((tone.comp.threshold - default.threshold).abs() < f32::EPSILON);
    }
}

/// A track's analyser, and the rack drawn from it.
///
/// The spectrum is the one part of a rack that moves with the audio,
/// and the plugin paints the whole graph in one call — so a moving
/// spectrum makes the WHOLE rack live, which at sixty strips is four
/// milliseconds a frame to redraw four curves that did not change.
///
/// The engine publishes at about 30 Hz and the window draws at 240, so
/// seven frames in eight are redrawing the same picture. This keeps the
/// picture: the rack is rebuilt when the spectrum is REPLACED, and
/// replayed otherwise, which is the same trade the recorded scene makes
/// one level up.
#[derive(Clone, Debug, Default)]
pub struct Analyser {
    meters: Meters,
    /// The rack as last built, and the box it was built for.
    built: Option<(f64, f64, std::sync::Arc<Scene>)>,
}

impl Analyser {
    /// Replace the meters, which invalidates the picture.
    pub fn set(&mut self, meters: Meters) {
        self.meters = meters;
        self.built = None;
    }

    /// Throw the picture away without touching the bins.
    ///
    /// For the other reason a rack changes: a setting moved. The cache
    /// is keyed on the spectrum because that is what usually changes,
    /// but a knob turned or a processor switched out changes the same
    /// picture — and a rack that waited for the next meter frame to
    /// notice would lag a gesture by a thirtieth of a second.
    pub fn invalidate(&mut self) {
        self.built = None;
    }

    #[must_use]
    pub fn bins(&self) -> &[f32] {
        &self.meters.spectrum
    }

    /// Everything the rack has to show that moves.
    #[must_use]
    pub const fn meters(&self) -> &Meters {
        &self.meters
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.meters.is_empty()
    }

    /// The rack for this box, built once per spectrum.
    ///
    /// `build` is only called on a miss, so the caller may do the full
    /// plugin paint inside it without thinking about how often.
    pub fn rack(
        &mut self,
        width: f64,
        height: f64,
        build: impl FnOnce() -> Scene,
    ) -> std::sync::Arc<Scene> {
        if let Some((w, h, scene)) = &self.built
            && (w - width).abs() < 0.5
            && (h - height).abs() < 0.5
        {
            return std::sync::Arc::clone(scene);
        }
        let built = std::sync::Arc::new(build());
        self.built = Some((width, height, std::sync::Arc::clone(&built)));
        built
    }
}

/// What the compressor's display is showing, over time.
///
/// The threshold is a line across a level axis, which only means
/// anything if there are levels on it. This is the history the line is
/// read against: one input peak per meter frame, newest last.
///
/// A ring in the sense that it drops the oldest rather than growing —
/// a mixer left open for an hour would otherwise hold a hundred
/// thousand floats per track for a display two hundred pixels wide.
#[derive(Clone, Debug, Default)]
pub struct Levels {
    peaks: std::collections::VecDeque<f32>,
    /// The de-esser's deepest cut per frame, in dB.
    fired: std::collections::VecDeque<f32>,
    /// The de-esser's band per frame: the level in it, and the level
    /// around it — the trace, and the line it is judged against.
    ess: std::collections::VecDeque<(f32, f32)>,
    /// The input trace, built once per CHANGE rather than once per
    /// frame.
    ///
    /// The engine publishes levels at about 30 Hz and the window draws
    /// at whatever the display does — 240 here. Rebuilding a
    /// Catmull-Rom through seventy points, as a string, and parsing it
    /// back, on every frame for every strip, was a third of a
    /// millisecond across sixty racks to redraw a line that had not
    /// moved seven frames out of eight.
    ///
    /// Keyed by the box it was built for, because the path is in the
    /// panel's own pixels: a mixer that resizes has to rebuild, and a
    /// mixer that does not never does.
    path: Option<(f64, f64, std::sync::Arc<BezPath>)>,
    /// And the gain reduction, which depends on the COMPRESSOR as well
    /// as on the levels — so it carries the settings it was built for
    /// and rebuilds when a knob moves.
    reduction: Option<(f64, f64, Comp, std::sync::Arc<BezPath>)>,
}

/// How often the level history is published, in hertz.
///
/// The engine's meter rate, which the simulation matches. The
/// reduction's envelope is computed per sample of that history, so it
/// has to know how long a sample is — a 3ms attack and a 33ms sample
/// means the reduction arrives inside one step, and pretending
/// otherwise would draw every compressor as slow.
pub const PUBLISH_HZ: u32 = 30;

/// How many frames of level the display holds.
///
/// Long enough to see a phrase arrive and short enough that the
/// transient you just played is still on the screen.
///
/// Not more: the trace is the one thing in the rack that is rebuilt
/// every frame, and the plugin's `smooth_path` is a Catmull-Rom through
/// every sample — so this number is multiplied by the strip count on
/// every frame the mixer draws. Doubling it cost half a millisecond
/// across sixty racks and bought a smoother line nobody could see,
/// because a strip is a hundred and thirty pixels wide and this is
/// already more samples than it has columns at a glance width.
pub const HISTORY: usize = 72;

impl Levels {
    /// Record one frame's peak.
    ///
    /// A reading equal to the last one is dropped: a silent track
    /// publishes the same zero thirty times a second, and rebuilding
    /// its trace for that is the cost this cache exists to remove.
    pub fn push(&mut self, peak: f32) {
        let peak = peak.clamp(0.0, 1.0);
        if self
            .peaks
            .back()
            .is_some_and(|last| (last - peak).abs() < 1e-4 && self.peaks.len() >= HISTORY)
        {
            return;
        }
        if self.peaks.len() >= HISTORY {
            self.peaks.pop_front();
        }
        self.peaks.push_back(peak);
        self.path = None;
        self.reduction = None;
    }

    /// Record how hard the de-esser fired this frame, in dB.
    ///
    /// Kept in step with the peaks — one entry per frame — so the fire
    /// lane and the level trace above it share a time axis.
    pub fn push_fire(&mut self, db: f32) {
        if self.fired.len() >= HISTORY {
            self.fired.pop_front();
        }
        self.fired.push_back(db.max(0.0));
    }

    /// The de-esser's band this frame: its level, and its surroundings'.
    pub fn push_ess(&mut self, level_db: f32, reference_db: f32) {
        if self.ess.len() >= HISTORY {
            self.ess.pop_front();
        }
        self.ess.push_back((level_db, reference_db));
    }

    /// The peaks, oldest first.
    pub fn peaks(&self) -> impl Iterator<Item = f32> + '_ {
        self.peaks.iter().copied()
    }

    /// The input trace, for a display of this size.
    ///
    /// Built through the plugin's own `smooth_path` — Catmull-Rom, the
    /// shape its editor draws the input wave with — and kept until the
    /// levels or the size change. Up from the bottom, because that is
    /// what a level is.
    fn path(&mut self, width: f64, height: f64) -> Option<std::sync::Arc<BezPath>> {
        if let Some((w, h, path)) = &self.path
            && (w - width).abs() < 0.5
            && (h - height).abs() < 0.5
        {
            return Some(std::sync::Arc::clone(path));
        }
        let d = comp_ui::comp_graph_svg::smooth_path(&self.scaled(), width, height, true, true);
        let built = std::sync::Arc::new(BezPath::from_svg(&d).ok()?);
        self.path = Some((width, height, std::sync::Arc::clone(&built)));
        Some(built)
    }

    /// How much the compressor took off each of those samples, in dB.
    ///
    /// The TARGET comes from the plugin's own `compress_transfer` — the
    /// same function the threshold marker and the ratio arrow are drawn
    /// from, so the amount the display says is being removed is the
    /// amount the maths says. What it does between targets is the
    /// attack and the release.
    ///
    /// Which is the whole reason those two have no control of their
    /// own any more. They were knobs, then they were curves in the
    /// margins, and both were a description of something the panel was
    /// already drawing in the right axes and the right place — a
    /// reduction, over time. A trace that snapped to its target was a
    /// compressor with no attack and no release, and the numbers beside
    /// it were claiming otherwise.
    #[must_use]
    pub fn reduction_db(&self, comp: Comp) -> Vec<f32> {
        // One-pole coefficients, per sample of history. The history is
        // published at the engine's rate, so a sample is about 33ms —
        // which is coarse next to a 3ms attack, and exactly why the
        // coefficient is computed from the interval rather than
        // assumed: a time constant shorter than the frame rate has to
        // arrive within one sample, not over three.
        let step = 1000.0 / f64::from(crate::tone::PUBLISH_HZ);
        let coefficient = |ms: f32| {
            let ms = f64::from(ms).max(0.01);
            crate::mcp::f64_to_f32((-step / ms).exp())
        };
        let (attack, release) = (coefficient(comp.attack), coefficient(comp.release));

        let mut held = 0.0_f32;
        self.peaks
            .iter()
            .map(|&peak| {
                let target = if peak <= 0.0 {
                    0.0
                } else {
                    let db = 20.0 * peak.log10();
                    let out = comp_ui::comp_graph_svg::compress_transfer(
                        db,
                        comp.threshold,
                        comp.ratio,
                        comp.knee,
                    );
                    (db - out).max(0.0)
                };
                // Toward more reduction at the attack rate, back toward
                // none at the release rate. A compressor's asymmetry is
                // the whole of what those two words mean.
                let rate = if target > held { attack } else { release };
                held = target + (held - target) * rate;
                held
            })
            .collect()
    }

    /// The gain-reduction trace, hanging DOWN from the top.
    ///
    /// Down because that is the direction the compressor moves the
    /// signal, and because the input is already coming up from the
    /// floor — two traces growing the same way would be two readings
    /// of one thing rather than a cause and its effect.
    fn reduction_path(
        &mut self,
        comp: Comp,
        width: f64,
        height: f64,
    ) -> Option<std::sync::Arc<BezPath>> {
        if let Some((w, h, was, path)) = &self.reduction
            && (w - width).abs() < 0.5
            && (h - height).abs() < 0.5
            && *was == comp
        {
            return Some(std::sync::Arc::clone(path));
        }
        let gr = comp_ui::comp_graph_svg::scale_gr_wave(&self.reduction_db(comp));
        if gr.iter().all(|v| *v <= f32::EPSILON) {
            return None;
        }
        let d = comp_ui::comp_graph_svg::smooth_path(&gr, width, height, false, true);
        let built = std::sync::Arc::new(BezPath::from_svg(&d).ok()?);
        self.reduction = Some((width, height, comp, std::sync::Arc::clone(&built)));
        Some(built)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.peaks.is_empty()
    }

    /// The history as the plugin's own display scale wants it.
    ///
    /// `scale_input_peak` maps a linear peak through dB, so the trace
    /// is log-scaled the way the compressor's editor draws it — and the
    /// way the threshold line above it is positioned.
    #[must_use]
    pub fn scaled(&self) -> Vec<f32> {
        let raw: Vec<f32> = self.peaks.iter().copied().collect();
        comp_ui::comp_graph_svg::scale_input_wave(&raw)
    }
}

/// Draw the level history into a compressor panel.
///
/// Live, not recorded: this is the one part of the rack that changes
/// thirty times a second, and re-recording a mixer for it would be the
/// whole panel's cost to move one trace. The well, the ladder, the
/// threshold line and the knobs stay in the recording underneath.
///
/// The path is the plugin's — `smooth_path` is what its own editor
/// draws the input wave with, Catmull-Rom through the samples — so the
/// trace has the same shape in a strip as in the plugin window.
pub fn levels(
    scene: &mut Scene,
    panels: &[Which],
    levels: &mut Levels,
    tone: &Tone,
    panel: Panel,
    folded: Folded,
) {
    if levels.is_empty() {
        return;
    }
    let rack = Rack::at(panel.width);
    // A rail's compressor is a bar of what it is taking off now, drawn
    // with the rest of its rack; the history has no room there.
    if !rack.on() || rack == Rack::Minimal {
        return;
    }
    // Both compressors and the gate: all three read a level against a
    // threshold, and all three are unreadable without the level. It
    // used to draw one panel because there used to be one.
    for which in [Which::Gate, Which::RescueComp, Which::Comp] {
        // A bypassed processor is not processing, so it has no display:
        // the scrim over it says so, and a waveform moving under it
        // would say the opposite.
        if tone.bypass.is(which) {
            continue;
        }
        let comp = match which {
            Which::RescueComp => tone.rescue_comp,
            Which::Comp => tone.comp,
            // The gate's trace is the level alone — what it does to the
            // signal is open or shut, not an amount, so there is no
            // reduction curve to draw under it.
            _ => Comp {
                ratio: 1.0,
                ..tone.comp
            },
        };
        one_level(scene, panels, levels, comp, which, panel, folded, rack);
    }
    // The two lanes: when the gate was open, and when the de-esser
    // fired. Both are strips of time under a display, built from the
    // same history the trace above them is, so a segment in the lane
    // is under the hit that opened it.
    lanes(scene, panels, levels, tone, panel, folded, rack);
}

/// The door lane and the fire lane.
fn lanes(
    scene: &mut Scene,
    panels: &[Which],
    levels: &Levels,
    tone: &Tone,
    panel: Panel,
    folded: Folded,
    rack: Rack,
) {
    for (which, at) in units(panels, panel, folded) {
        if !matches!(which, Which::Gate | Which::DeEss | Which::DeEssIn) || tone.bypass.is(which) {
            continue;
        }
        let body = body_of(at, rack);
        if which != Which::Gate {
            ess_trace(scene, levels, tone, display_of(body, which, rack));
            continue;
        }
        let Some(lane) = lane_of(body, which, rack) else {
            continue;
        };
        if lane.width < 2.0 || lane.height < 2.0 {
            continue;
        }
        let per = lane.width / crate::num::coord(HISTORY);
        {
            {
                // Open where the level is over the threshold, and for
                // the hold after it drops back, and ramping shut over
                // the release — the setting drawn where it acts.
                let tint = palette_open();
                let step_ms = 1000.0 / f64::from(PUBLISH_HZ);
                let hold = f64::from(tone.gate.hold) / step_ms;
                let release = f64::from(tone.gate.release) / step_ms;
                let mut open_until = -1.0_f64;
                let mut path = BezPath::new();
                let mut run: Option<f64> = None;
                let count = levels.peaks.len();
                let offset = HISTORY.saturating_sub(count);
                for (i, peak) in levels.peaks().enumerate() {
                    let i_f = crate::num::coord(i);
                    let db = if peak <= 0.0 {
                        -120.0
                    } else {
                        20.0 * f64::from(peak).log10()
                    };
                    if db > f64::from(tone.gate.threshold) {
                        open_until = i_f + hold;
                    }
                    let open = i_f <= open_until;
                    let x = crate::num::coord(i.saturating_add(offset)).mul_add(per, lane.x);
                    match (open, run) {
                        (true, None) => run = Some(x),
                        (false, Some(from)) => {
                            let to = release.min(6.0).mul_add(per, x);
                            path.move_to((from, lane.y + lane.height - 1.0));
                            path.line_to((from + 1.0, lane.y + 1.0));
                            path.line_to((x, lane.y + 1.0));
                            path.line_to((to.min(lane.x + lane.width), lane.y + lane.height - 1.0));
                            path.close_path();
                            run = None;
                        }
                        _ => {}
                    }
                }
                if let Some(from) = run {
                    path.extend(
                        Rect::new(
                            from,
                            lane.y + 1.0,
                            lane.x + lane.width,
                            lane.y + lane.height - 1.0,
                        )
                        .to_path(0.1)
                        .elements()
                        .iter()
                        .copied(),
                    );
                }
                if !path.is_empty() {
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        tint.multiply_alpha(0.85),
                        None,
                        &path,
                    );
                }
            }
        }
    }
}

/// The de-esser's trace: the band's level over time in grey, the
/// reference it is judged against dashed over it, and the bite — what
/// came off the top — lit in the panel's colour between the two.
///
/// Time runs the way the compressor's does, so the two are read the
/// same way across a rack: an "S" is a spike in the band, the dashed
/// line is where the de-esser starts to care, and the bite is what it
/// did about it.
fn ess_trace(scene: &mut Scene, levels: &Levels, tone: &Tone, display: Panel) {
    let count = levels.ess.len();
    if count < 2 || display.width < 2.0 || display.height < 4.0 {
        return;
    }
    let per = display.width / crate::num::coord(HISTORY);
    let offset = HISTORY.saturating_sub(count);
    let x_at = |i: usize| crate::num::coord(i.saturating_add(offset)).mul_add(per, display.x);
    // The cuts are pushed once a frame like the levels are, so the two
    // histories end together: line them up from the end.
    let skew = count.saturating_sub(levels.fired.len());
    let cut_at = |i: usize| {
        i.checked_sub(skew)
            .and_then(|k| levels.fired.get(k))
            .copied()
            .unwrap_or(0.0)
    };
    let input: Vec<(f64, f64)> = levels
        .ess
        .iter()
        .enumerate()
        .map(|(i, (level, _))| (x_at(i), suppress_y(f64::from(*level), display)))
        .collect();
    let output: Vec<(f64, f64)> = levels
        .ess
        .iter()
        .enumerate()
        .map(|(i, (level, _))| (x_at(i), suppress_y(f64::from(level - cut_at(i)), display)))
        .collect();
    let threshold = f64::from(tone.de_ess.threshold);
    let reference = levels
        .ess
        .iter()
        .enumerate()
        .map(|(i, (_, around))| (x_at(i), suppress_y(f64::from(*around) + threshold, display)));
    let grey = Color::from_rgba8(0x9a, 0x9a, 0xa0, 0xff);
    area_under(
        scene,
        grey.multiply_alpha(0.16),
        &input,
        display.y + display.height,
    );
    curve(scene, grey.multiply_alpha(0.55), input.iter().copied(), 1.0);
    if levels.fired.iter().any(|db| *db > 0.05) {
        ribbon(scene, DEESS_INK.multiply_alpha(0.5), &input, &output);
    }
    dashed(scene, DEESS_INK.multiply_alpha(0.8), reference, 0.8);
    curve(scene, DEESS_INK, output.into_iter(), 1.3);
}

/// The colour a gate's open segments are lit in.
///
/// The Balance phase's green: "on", in the same hue the rail says it.
fn palette_open() -> Color {
    phase_tint(session::mix_phases::MixPhase::Balance)
}

/// One processor's level trace.
fn one_level(
    scene: &mut Scene,
    panels: &[Which],
    levels: &mut Levels,
    comp: Comp,
    which: Which,
    panel: Panel,
    folded: Folded,
    rack: Rack,
) {
    let Some((_, at)) = units(panels, panel, folded)
        .into_iter()
        .find(|(at_which, _)| *at_which == which)
    else {
        return;
    };
    let body = body_of(at, rack);
    // The display is the part above the knobs — the same split `comp`
    // makes, so the trace lands on the axis the threshold is on.
    // The display, between the two ramps — the trace has to land on the
    // axis the threshold line is on.
    let body = comp_split(body, rack);
    if body.width < 2.0 || body.height < 2.0 {
        return;
    }
    let at = Affine::translate((body.x, body.y));
    // The signal, in the compressor's own readout grey, up from the
    // floor. Grey because it is the thing being acted ON — it carries
    // no state of its own, and every coloured thing on this panel means
    // something. The plugin's grey rather than one chosen here, so a
    // waveform in a strip and a waveform in the editor are the same
    // shade of not-a-decision.
    if let Some(path) = levels.path(body.width, body.height) {
        // Filled, because what you are reading is how much of the
        // display the signal takes up against the line across it — an
        // outline makes that a comparison of two lines instead. Dim,
        // because the threshold is drawn over it and a solid fill makes
        // the line the thing you cannot see.
        let grey = hex(comp_ui::comp_graph_svg::colors::GREY);
        scene.fill(
            Fill::NonZero,
            at,
            grey.multiply_alpha(0.30),
            None,
            path.as_ref(),
        );
        scene.stroke(
            &Stroke::new(1.0),
            at,
            grey.multiply_alpha(0.9),
            None,
            path.as_ref(),
        );
    }
    // And what the compressor took off, red, hanging down from the top.
    // Down is the direction it moves the signal; red because it is the
    // one thing on this panel that is a REDUCTION, and it shares its
    // colour with the threshold that caused it.
    if let Some(path) = levels.reduction_path(comp, body.width, body.height) {
        let fill = hex(comp_ui::comp_graph_svg::colors::REDUCTION_FILL);
        let edge = hex(comp_ui::comp_graph_svg::colors::REDUCTION_EDGE);
        scene.fill(
            Fill::NonZero,
            at,
            fill.multiply_alpha(0.45),
            None,
            path.as_ref(),
        );
        scene.stroke(&Stroke::new(1.0), at, edge, None, path.as_ref());
    }
}

#[cfg(test)]
mod level_tests {
    use super::{HISTORY, Levels};

    /// The history is bounded: a mixer left open for an hour holds a
    /// display's worth per track, not an hour's.
    #[test]
    fn the_history_is_bounded() {
        let mut levels = Levels::default();
        for i in 0..HISTORY * 4 {
            levels.push((i % 100) as f32 / 100.0);
        }
        assert_eq!(levels.scaled().len(), HISTORY);
    }

    /// Empty is empty — a track that has never played draws nothing
    /// rather than a flat line at the floor, which would read as
    /// silence rather than as no data.
    #[test]
    fn nothing_recorded_is_nothing_drawn() {
        assert!(Levels::default().is_empty());
        assert!(Levels::default().scaled().is_empty());
    }

    /// And the scale is the plugin's, so the trace and the threshold
    /// line above it are on one axis.
    #[test]
    fn the_trace_uses_the_plugins_scale() {
        let mut levels = Levels::default();
        levels.push(1.0);
        levels.push(0.5);
        let got = levels.scaled();
        let want = comp_ui::comp_graph_svg::scale_input_wave(&[1.0, 0.5]);
        assert_eq!(got, want);
    }
}

#[cfg(test)]
mod reduction_tests {
    use super::{Comp, Levels};

    fn hits() -> Levels {
        let mut levels = Levels::default();
        // A loud hit decaying into a quiet floor, twice.
        for _ in 0..2 {
            for i in 0..20_u8 {
                levels.push(0.7 * (-f32::from(i) / 6.0).exp() + 0.004);
            }
        }
        levels
    }

    /// The reduction is what the compressor is taking off, computed
    /// through its own transfer — so a loud sample above the threshold
    /// reduces and a quiet stretch below it recovers.
    ///
    /// Recovers rather than stops: the trace is an envelope follower
    /// now, so it lets go at the release rate instead of snapping to
    /// nothing the instant the signal drops. That is the whole reason
    /// the attack and release are visible at all.
    #[test]
    fn what_crosses_the_threshold_reduces_and_the_rest_recovers() {
        let comp = Comp {
            threshold: -12.0,
            ratio: 4.0,
            knee: 0.0,
            release: 60.0,
            ..Comp::default()
        };
        let gr = hits().reduction_db(comp);
        let peak = gr.iter().copied().fold(0.0_f32, f32::max);
        let quiet = gr.iter().copied().fold(f32::MAX, f32::min);
        assert!(peak > 1.0, "nothing was reduced: {gr:?}");
        assert!(quiet < peak / 4.0, "nothing recovered: {gr:?}");
        assert!(gr.iter().all(|g| *g >= 0.0), "a reduction went negative");
    }

    /// The release is what the recovery takes, so a slower one is still
    /// holding reduction where a faster one has let go. This is the
    /// claim the envelope glyph makes about its lead-out edge.
    #[test]
    fn a_slower_release_holds_the_reduction_longer() {
        let at = |release: f32| {
            let comp = Comp {
                threshold: -12.0,
                ratio: 4.0,
                knee: 0.0,
                release,
                ..Comp::default()
            };
            hits()
                .reduction_db(comp)
                .iter()
                .copied()
                .fold(f32::MAX, f32::min)
        };
        assert!(at(2_000.0) > at(30.0), "{} vs {}", at(2_000.0), at(30.0));
    }

    /// A harder ratio takes more off the same signal. This is the claim
    /// the display makes every time a knob moves, and the one that
    /// would go unnoticed if the trace were computed from anything but
    /// the plugin's own transfer.
    #[test]
    fn a_harder_ratio_reduces_more() {
        let at = |ratio: f32| {
            let comp = Comp {
                threshold: -18.0,
                ratio,
                knee: 0.0,
                ..Comp::default()
            };
            hits()
                .reduction_db(comp)
                .iter()
                .copied()
                .fold(0.0, f32::max)
        };
        assert!(
            at(8.0) > at(2.0),
            "{} was not more than {}",
            at(8.0),
            at(2.0)
        );
    }

    /// A ratio of one is no compressor at all, so nothing hangs from
    /// the top — and the path is `None` rather than a flat nothing,
    /// which is what keeps a silent panel from paying for a trace.
    #[test]
    fn unity_reduces_nothing() {
        let comp = Comp {
            threshold: -30.0,
            ratio: 1.0,
            knee: 0.0,
            ..Comp::default()
        };
        let gr = hits().reduction_db(comp);
        assert!(gr.iter().all(|g| *g < 0.01), "1:1 reduced something");
        assert!(hits().reduction_path(comp, 100.0, 50.0).is_none());
    }

    /// The two traces grow in opposite directions: the signal up from
    /// the floor, the reduction down from the top. Two growing the same
    /// way would be two readings of one thing rather than a cause and
    /// its effect.
    #[test]
    fn the_traces_grow_apart() {
        let comp = Comp {
            threshold: -20.0,
            ratio: 6.0,
            knee: 0.0,
            ..Comp::default()
        };
        let mut levels = hits();
        let (w, h) = (120.0, 60.0);
        let input = levels.path(w, h).expect("an input trace");
        let gr = levels
            .reduction_path(comp, w, h)
            .expect("a reduction trace");
        let mean_y = |path: &vello::kurbo::BezPath| {
            let points: Vec<f64> = path
                .elements()
                .iter()
                .filter_map(|e| match e {
                    vello::kurbo::PathEl::MoveTo(p) | vello::kurbo::PathEl::LineTo(p) => Some(p.y),
                    vello::kurbo::PathEl::CurveTo(_, _, p) => Some(p.y),
                    _ => None,
                })
                .collect();
            points.iter().sum::<f64>() / points.len().max(1) as f64
        };
        assert!(
            mean_y(&input) > mean_y(&gr),
            "the input sat above the reduction"
        );
    }
}

#[cfg(test)]
mod hex_tests {
    use super::hex;

    /// The plugin states its colours as hex because its UIs are DOMs.
    /// This is the one place they become paint, so it has to read them
    /// exactly — a waveform a shade off is a second table that has
    /// already started drifting.
    #[test]
    fn a_plugin_colour_survives_the_trip() {
        for value in [
            comp_ui::comp_graph_svg::colors::GREY,
            comp_ui::comp_graph_svg::colors::REDUCTION_EDGE,
            comp_ui::comp_graph_svg::colors::THRESHOLD,
        ] {
            let [r, g, b, a] = hex(value).to_rgba8().to_u8_array();
            assert_eq!(format!("#{r:02x}{g:02x}{b:02x}"), value);
            assert_eq!(a, 0xff, "a stated colour is opaque; alpha is the caller's");
        }
    }

    /// Anything that is not a colour is visibly not one, rather than
    /// nothing — a rack that drew nothing would look like a rack with
    /// no signal.
    #[test]
    fn a_broken_value_is_still_visible() {
        for bad in ["", "#", "nonesuch", "#12"] {
            let [r, g, b, a] = hex(bad).to_rgba8().to_u8_array();
            assert_eq!((r, g, b, a), (0x88, 0x88, 0x88, 0xff));
        }
    }
}

#[cfg(test)]
mod bypass_tests {
    use super::{Bypass, Grip, Mods, Panel, Rack, Which, drag, grip_at, placeholder, reset};

    const ALL: [Which; 3] = [Which::Eq, Which::Comp, Which::Sat];

    /// Every unit in the chain, so the bitset is checked against the
    /// whole enum rather than the three the drag tests use.
    const EVERY: [Which; 19] = [
        Which::RescueEq,
        Which::Gate,
        Which::RescueComp,
        Which::Eq,
        Which::Comp,
        Which::Sat,
        Which::DeEss,
        Which::PolishEq,
        Which::Space,
        Which::Delay,
        Which::Reverb,
        Which::DeEssIn,
        Which::PreEq,
        Which::PostEq,
        Which::DecayEq,
        Which::Knobs,
        Which::Wide,
        Which::Pitch,
        Which::Presets,
    ];

    /// Switching one unit out switches out that unit and no other.
    ///
    /// The chain passed sixteen units and the bitset was still a `u16`,
    /// so the last three wrapped onto the first three: bypassing the
    /// widener read as bypassing the rescue EQ, and a debug build
    /// panicked on the overflowing shift rather than drawing a frame.
    #[test]
    fn each_unit_has_its_own_bit() {
        for which in EVERY {
            let mut bypass = Bypass::default();
            assert!(!bypass.any());
            bypass.toggle(which);
            assert!(bypass.is(which), "{which:?} did not switch out");
            for other in EVERY {
                if other != which {
                    assert!(!bypass.is(other), "{which:?} also switched out {other:?}");
                }
            }
            bypass.toggle(which);
            assert!(!bypass.any(), "{which:?} did not switch back in");
        }
    }

    fn rack() -> Panel {
        Panel {
            x: 0.0,
            y: 0.0,
            width: 133.0,
            height: 600.0,
        }
    }

    /// Per PROCESSOR, because the question is almost always "what does
    /// this sound like without the compressor", not "without any of it".
    #[test]
    fn each_processor_switches_out_alone() {
        let mut bypass = Bypass::default();
        assert!(!bypass.any());
        bypass.toggle(Which::Comp);
        assert!(bypass.is(Which::Comp));
        assert!(!bypass.is(Which::Eq) && !bypass.is(Which::Sat));
        assert!(bypass.any());
        bypass.toggle(Which::Comp);
        assert!(!bypass.any());
    }

    /// The header is the switch, and it is above the body — so a click
    /// there is a bypass rather than whatever the body would have done.
    #[test]
    fn the_header_is_the_switch() {
        let tone = placeholder(0);
        for which in ALL {
            let at = super::layout(&ALL, rack())
                .into_iter()
                .find(|(w, _)| *w == which)
                .expect("a panel")
                .1;
            // A pixel inside the panel but above its body.
            let y = at.y + 3.0;
            assert_eq!(
                grip_at(
                    &ALL,
                    &tone,
                    rack(),
                    super::Folded::default(),
                    at.x + at.width / 2.0,
                    y
                ),
                Some(Grip::Bypass(which)),
                "{which:?}"
            );
        }
    }

    /// A bypassed panel has ONE control left, and it is the one that
    /// brings it back: grabbing a band through the scrim would move a
    /// setting that is having no effect.
    #[test]
    fn a_bypassed_panel_only_offers_its_way_back() {
        let mut tone = placeholder(0);
        let body = super::body_of(
            super::layout(&ALL, rack())
                .into_iter()
                .find(|(w, _)| *w == Which::Eq)
                .expect("an EQ panel")
                .1,
            Rack::at(rack().width),
        );
        let map = super::mapper(body, tone.eq_db_range());
        let band = &tone.eq[2];
        let x = body.x + map.freq_to_x(f64::from(band.frequency));
        let y = body.y + map.db_to_y(f64::from(band.gain));
        assert_eq!(
            grip_at(&ALL, &tone, rack(), super::Folded::default(), x, y),
            Some(Grip::Band(Which::Eq, 2))
        );

        tone.bypass.toggle(Which::Eq);
        assert_eq!(
            grip_at(&ALL, &tone, rack(), super::Folded::default(), x, y),
            Some(Grip::Bypass(Which::Eq)),
            "a band was still grabbable through the scrim"
        );
    }

    /// A switch has no drag and no turn — dragging off one is how you
    /// change your mind about pressing it.
    #[test]
    fn a_switch_does_not_drag() {
        let mut tone = placeholder(0);
        let was = tone.bypass;
        let grip = Grip::Bypass(Which::Sat);
        assert!(grip.is_switch());
        drag(
            &mut tone,
            grip,
            &ALL,
            rack(),
            super::Folded::default(),
            Mods::default(),
            30.0,
            -30.0,
        );
        assert_eq!(tone.bypass, was, "a drag flipped a switch");
        super::wheel(&mut tone, grip, Mods::default(), -1.0);
        assert_eq!(tone.bypass, was, "a wheel flipped a switch");
    }

    /// And resetting one switches it back — which is what a
    /// double-click would have done anyway.
    #[test]
    fn resetting_a_switch_flips_it() {
        let mut tone = placeholder(0);
        tone.bypass.toggle(Which::Eq);
        reset(&mut tone, Grip::Bypass(Which::Eq));
        assert!(!tone.bypass.is(Which::Eq));
    }
}

#[cfg(test)]
mod character_tests {
    use super::placeholder;

    /// Each track gets its OWN chain, not one curve with a wobble in
    /// it: a rack of racks has to read as a set of decisions.
    #[test]
    fn neighbouring_tracks_get_different_chains() {
        let chains: Vec<_> = (0..4).map(placeholder).collect();
        for (i, a) in chains.iter().enumerate() {
            for b in chains.iter().skip(i + 1) {
                assert!(
                    a.eq.iter().zip(&b.eq).any(|(x, y)| {
                        (x.frequency - y.frequency).abs() > 1.0 || (x.gain - y.gain).abs() > 0.5
                    }),
                    "two chains had the same EQ"
                );
                assert!(
                    (a.comp.attack - b.comp.attack).abs() > 0.1
                        || (a.comp.ratio - b.comp.ratio).abs() > 0.1,
                    "two chains had the same compressor"
                );
            }
        }
    }

    /// And the difference is a decision, not noise: the voice that
    /// lives at the bottom lifts there, and the one that lives at the
    /// top cuts there.
    #[test]
    fn a_low_voice_and_a_high_one_disagree_about_the_bottom() {
        let low = placeholder(0);
        let high = placeholder(2);
        assert!(low.eq[0].gain > 0.0, "the low voice cut its own register");
        assert!(high.eq[0].gain < 0.0, "the high voice kept the mud");
        assert!(
            high.eq[3].gain > low.eq[3].gain,
            "the high voice was darker than the low one"
        );
    }

    /// Nothing arrives bypassed — a rack you have to switch on before
    /// it does anything is a rack that looks broken.
    #[test]
    fn a_new_chain_is_running() {
        assert!(!placeholder(3).bypass.any());
    }
}

#[cfg(test)]
mod face_tests {
    //! The faces the six units grew: each grip is where its picture is.
    use super::{
        ALL_PANELS, Folded, Grip, Mods, Panel, Rack, RoomGeometry, Side, Which, body_of,
        display_of, drag, echo_taps, grip_at, lane_of, placeholder, reset, sat_split, units, wheel,
    };
    use crate::live::Meters;

    fn rack() -> Panel {
        Panel {
            x: 0.0,
            y: 0.0,
            width: 133.0,
            height: 2_400.0,
        }
    }

    fn body(which: Which) -> Panel {
        let (_, at) = units(&ALL_PANELS, rack(), Folded::default())
            .into_iter()
            .find(|(w, _)| *w == which)
            .expect("the unit is in the chain");
        body_of(at, Rack::at(rack().width))
    }

    fn grip(x: f64, y: f64, tone: &super::Tone) -> Option<Grip> {
        grip_at(&ALL_PANELS, tone, rack(), Folded::default(), x, y)
    }

    /// The gate's table: its three edges are its three times, and the
    /// range line is its own grip where it is clear of the threshold.
    #[test]
    fn the_gates_table_is_grabbed_by_its_edges() {
        let tone = placeholder(0);
        let at = display_of(body(Which::Gate), Which::Gate, Rack::Full);
        let to_y = |db: f64| at.y + comp_ui::comp_graph_svg::db_to_y(db, at.height);
        let line = to_y(f64::from(tone.gate.threshold));
        let floor = to_y(f64::from(tone.gate.threshold + tone.gate.range));
        let shape = super::GateGlyph::of(tone.gate, at, line, floor).expect("room for a glyph");
        assert_eq!(
            shape.grip_at(shape.open.0, shape.open.1 + 1.0),
            Some(Grip::Attack(Which::Gate))
        );
        let mid_run = (f64::midpoint(shape.open.0, shape.close.0), shape.open.1);
        assert_eq!(
            shape.grip_at(mid_run.0, mid_run.1),
            Some(Grip::Hold(Which::Gate))
        );
        assert_eq!(
            shape.grip_at(shape.end.0, shape.end.1 - 1.0),
            Some(Grip::Release(Which::Gate))
        );
        // The range line, well away from the glyph.
        assert_eq!(grip(at.x + at.width - 4.0, floor, &tone), Some(Grip::Range));
        // And the threshold everywhere else.
        assert_eq!(
            grip(at.x + at.width - 4.0, at.y + 2.0, &tone),
            Some(Grip::Threshold(Which::Gate))
        );
    }

    /// Dragging the hold right lengthens it; resetting puts it back.
    #[test]
    fn the_hold_is_dragged_sideways() {
        let mut tone = placeholder(0);
        let was = tone.gate.hold;
        drag(
            &mut tone,
            Grip::Hold(Which::Gate),
            &ALL_PANELS,
            rack(),
            Folded::default(),
            Mods::default(),
            20.0,
            0.0,
        );
        assert!(
            tone.gate.hold > was,
            "{} should exceed {was}",
            tone.gate.hold
        );
        reset(&mut tone, Grip::Hold(Which::Gate));
        assert!((tone.gate.hold - super::Gate::default().hold).abs() < f32::EPSILON);
    }

    /// The first repeat is the time; a later one is the feedback; the
    /// glyph cycles the machine.
    #[test]
    fn the_delays_repeats_are_its_grips() {
        let mut tone = placeholder(1);
        let at = display_of(body(Which::Delay), Which::Delay, Rack::Full);
        let taps = echo_taps(tone.delay, at);
        assert!(taps.len() >= 3, "enough repeats to grab: {}", taps.len());
        let y = at.y + at.height / 2.0;
        assert_eq!(grip(taps[1].0, y, &tone), Some(Grip::Time));
        assert_eq!(grip(taps[2].0, y, &tone), Some(Grip::Feedback));
        let was = tone.delay.time;
        drag(
            &mut tone,
            Grip::Time,
            &ALL_PANELS,
            rack(),
            Folded::default(),
            Mods::default(),
            10.0,
            0.0,
        );
        assert!(tone.delay.time > was);
        let fb = tone.delay.feedback;
        drag(
            &mut tone,
            Grip::Feedback,
            &ALL_PANELS,
            rack(),
            Folded::default(),
            Mods::default(),
            0.0,
            -20.0,
        );
        assert!(tone.delay.feedback > fb, "up is more feedback");
        // The spacing follows the time.
        let after = echo_taps(tone.delay, at);
        assert!(
            (after[1].0 - after[0].0 - (taps[1].0 - taps[0].0)).abs() < 1e-6,
            "the window scales with the time, so the first gap holds its share"
        );
        // The header glyph is the family switch.
        let (_, unit) = units(&ALL_PANELS, rack(), Folded::default())
            .into_iter()
            .find(|(w, _)| *w == Which::Delay)
            .expect("a delay");
        assert_eq!(
            grip(unit.x + 4.0, unit.y + 4.0, &tone),
            Some(Grip::Family(Which::Delay))
        );
        let style = tone.delay.style;
        tone.delay.cycle_style();
        assert_ne!(tone.delay.style, style);
        for _ in 0..20 {
            tone.delay.cycle_style();
        }
    }

    /// The reverb: the gap rule is the predelay, the far end the decay,
    /// the rest the mix — and the decay drag lengthens the tail.
    #[test]
    fn the_reverbs_tail_is_its_grips() {
        let mut tone = placeholder(2);
        let at = display_of(body(Which::Reverb), Which::Reverb, Rack::Full);
        let geometry = RoomGeometry::of(tone.reverb, at);
        let y = at.y + at.height / 2.0;
        assert_eq!(grip(geometry.start, y, &tone), Some(Grip::Predelay));
        assert_eq!(grip(at.x + at.width - 5.0, y, &tone), Some(Grip::Decay));
        assert_eq!(
            grip(at.width.mul_add(0.4, at.x), y, &tone),
            Some(Grip::Mix(Which::Reverb))
        );
        let was = tone.reverb.decay;
        drag(
            &mut tone,
            Grip::Decay,
            &ALL_PANELS,
            rack(),
            Folded::default(),
            Mods::default(),
            30.0,
            0.0,
        );
        assert!(tone.reverb.decay > was);
        wheel(&mut tone, Grip::Predelay, Mods::default(), -1.0);
        assert!(tone.reverb.predelay > super::Room::default().predelay - 1.0);
        let algorithm = tone.reverb.algorithm;
        tone.reverb.cycle_algorithm();
        assert_ne!(tone.reverb.algorithm, algorithm);
        for _ in 0..20 {
            tone.reverb.cycle_algorithm();
        }
    }

    /// The selector strip under a delay or a reverb: one chip per
    /// family, and a click on one picks a machine from it — the next
    /// member if you are already there.
    #[test]
    fn the_selector_picks_a_family() {
        let mut tone = placeholder(0);
        let strip = lane_of(body(Which::Delay), Which::Delay, Rack::Full).expect("a selector");
        // Chip 1 is tape.
        let x = super::CHIP.mul_add(1.5, strip.x + 2.0);
        assert_eq!(
            grip(x, strip.y + 4.0, &tone),
            Some(Grip::Choose(Which::Delay, 1))
        );
        assert!(Grip::Choose(Which::Delay, 1).is_switch());
        tone.delay.choose_family(1);
        assert_eq!(tone.delay.family(), delay_dsp::engine::Family::Tape);
        // Rhythmic has three machines: clicking again walks them.
        tone.delay.choose_family(4);
        let first = tone.delay.style;
        tone.delay.choose_family(4);
        assert_ne!(tone.delay.style, first);
        assert_eq!(tone.delay.family(), delay_dsp::engine::Family::Rhythmic);
        // Past the chips is the bypass, not a chip.
        assert_eq!(
            grip(strip.x + strip.width - 2.0, strip.y + 4.0, &tone),
            Some(Grip::Bypass(Which::Delay))
        );
        // And the reverb's picks an algorithm of the family.
        let strip = lane_of(body(Which::Reverb), Which::Reverb, Rack::Full).expect("a selector");
        assert_eq!(
            grip(
                super::CHIP.mul_add(2.5, strip.x + 2.0),
                strip.y + 4.0,
                &tone
            ),
            Some(Grip::Choose(Which::Reverb, 2))
        );
        tone.reverb.choose_family(2);
        assert_eq!(tone.reverb.family(), reverb_dsp::algorithm::Family::Plate);
        tone.reverb.choose_family(99);
        assert_eq!(tone.reverb.family(), reverb_dsp::algorithm::Family::Plate);
    }

    /// The saturator's selector picks a circuit through the plugin's
    /// own rail rule, and the digital circuit draws steps.
    #[test]
    fn the_saturators_selector_picks_a_circuit() {
        let mut tone = placeholder(0);
        let strip = lane_of(body(Which::Sat), Which::Sat, Rack::Full).expect("a selector");
        assert_eq!(
            grip(
                super::CHIP.mul_add(4.5, strip.x + 2.0),
                strip.y + 4.0,
                &tone
            ),
            Some(Grip::Choose(Which::Sat, 4))
        );
        let drive = tone.sat.drive;
        tone.choose_sat_family(4);
        assert_eq!(tone.sat_circuit(), saturate_dsp::preamp::Circuit::Steps);
        assert_eq!(saturate_profiles::PROFILES[tone.sat_profile].name, "Clip");
        tone.choose_sat_family(4);
        assert_eq!(
            saturate_profiles::PROFILES[tone.sat_profile].name,
            "Bitcrush"
        );
        assert!(!tone.sat_digital.is_transparent(), "a bitcrusher quantises");
        tone.choose_sat_family(0);
        assert_eq!(tone.sat_circuit(), saturate_dsp::preamp::Circuit::Valve);
        assert!(tone.sat_digital.is_transparent());
        // The drive knob rides across: a transformer at its drive is a
        // triode at the same knob, not the same gain.
        assert!(tone.sat.drive > 1.0 && (tone.sat.drive - drive).abs() < 8.0);
        tone.cycle_sat();
        assert_eq!(
            saturate_profiles::PROFILES[tone.sat_profile].name,
            "Pentode"
        );
    }

    /// A suppressor: edges where they are drawn, then threshold, depth,
    /// sharpness down the display.
    #[test]
    fn a_suppressor_is_three_bands_and_two_edges() {
        let mut tone = placeholder(3);
        let at = display_of(body(Which::DeEss), Which::DeEss, Rack::Full);
        let strip = lane_of(body(Which::DeEss), Which::DeEss, Rack::Full).expect("a band strip");
        assert!(strip.y < at.y, "the strip is over the display");
        let zoom = super::SuppressZoom::top();
        let low_x = zoom.x_of(f64::from(tone.de_ess.low), strip);
        assert_eq!(
            grip(low_x, strip.y + 5.0, &tone),
            Some(Grip::Edge(Which::DeEss, Side::Low))
        );
        let mid_x = at.x + at.width / 2.0;
        // Between the edges the strip is the sharpness; the display
        // is the threshold, with the depth along its floor.
        assert_eq!(
            grip(strip.x + 3.0, strip.y + 5.0, &tone),
            Some(Grip::Sharpness(Which::DeEss))
        );
        assert_eq!(
            grip(mid_x, at.y + 2.0, &tone),
            Some(Grip::Threshold(Which::DeEss))
        );
        assert_eq!(
            grip(mid_x, at.y + at.height / 2.0, &tone),
            Some(Grip::Threshold(Which::DeEss))
        );
        assert_eq!(
            grip(mid_x, at.y + at.height - 2.0, &tone),
            Some(Grip::Depth(Which::DeEss))
        );
        let was = tone.de_ess.low;
        wheel(
            &mut tone,
            Grip::Edge(Which::DeEss, Side::Low),
            Mods::default(),
            -1.0,
        );
        assert!(tone.de_ess.low > was, "the wheel moves the edge up");
        assert!(tone.de_ess.low < tone.de_ess.high);
        let depth = tone.de_ess.depth;
        drag(
            &mut tone,
            Grip::Depth(Which::DeEss),
            &ALL_PANELS,
            rack(),
            Folded::default(),
            Mods::default(),
            0.0,
            20.0,
        );
        assert!(tone.de_ess.depth > depth, "down is deeper");
    }

    /// The saturator: the ladder is the tilt, the centre the bias, the
    /// curve the drive — and a signal lights the curve.
    #[test]
    fn the_saturators_ladder_and_centre_are_grips() {
        let mut tone = placeholder(0);
        let at = display_of(body(Which::Sat), Which::Sat, Rack::Full);
        let (curve, ladder) = sat_split(at, Rack::Full);
        let ladder = ladder.expect("a ladder at a working width");
        assert_eq!(
            grip(ladder.x + 3.0, ladder.y + 3.0, &tone),
            Some(Grip::Tilt)
        );
        assert_eq!(
            grip(curve.x + curve.width / 2.0, curve.y + 5.0, &tone),
            Some(Grip::Bias)
        );
        assert_eq!(grip(curve.x + 3.0, curve.y + 5.0, &tone), Some(Grip::Drive));
        drag(
            &mut tone,
            Grip::Bias,
            &ALL_PANELS,
            rack(),
            Folded::default(),
            Mods::default(),
            15.0,
            0.0,
        );
        assert!(tone.sat.q_point > 0.25);
        let tilt = tone.sat.tilt_db();
        wheel(&mut tone, Grip::Tilt, Mods::default(), -1.0);
        assert!(tone.sat.tilt_db() > tilt, "up is toward the highs");
        reset(&mut tone, Grip::Tilt);
        assert!(tone.sat.tilt_db().abs() < f32::EPSILON);

        // Drawn with a signal, the rack has more in it than without:
        // the lit reach, the ribbon, the wet fill.
        let palette = crate::arrangement::Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = crate::text::Font::embedded().expect("the embedded font");
        let mut still = anyrender::Scene::new();
        super::draw(
            &mut still,
            &palette,
            &font,
            &tone,
            &Meters::default(),
            &ALL_PANELS,
            rack(),
            Folded::default(),
            None,
            None,
        );
        let mut moving = anyrender::Scene::new();
        let meters = crate::simulate::meters(0, 1.25, &tone);
        super::draw(
            &mut moving,
            &palette,
            &font,
            &tone,
            &meters,
            &ALL_PANELS,
            rack(),
            Folded::default(),
            None,
            None,
        );
        assert!(moving.commands.len() > still.commands.len());
    }

    /// The machines say which they are: three units carry a glyph, and
    /// two of them switch on it.
    #[test]
    fn three_units_carry_a_machine_glyph() {
        let tone = placeholder(0);
        let with: Vec<Which> = ALL_PANELS
            .iter()
            .copied()
            .filter(|w| w.glyph(&tone).is_some())
            .collect();
        assert_eq!(with, vec![Which::Sat, Which::Delay, Which::Reverb]);
        assert!(
            Which::Sat.glyph_switches()
                && Which::Delay.glyph_switches()
                && Which::Reverb.glyph_switches()
        );
        assert!(!Which::Gate.glyph_switches());
    }

    /// The lanes draw from the history: a gate fed hits above its
    /// threshold shows open segments, a de-esser fed fires shows them.
    #[test]
    fn the_lanes_draw_the_history() {
        let tone = placeholder(0);
        let mut levels = super::Levels::default();
        let mut silent = anyrender::Scene::new();
        super::lanes(
            &mut silent,
            &ALL_PANELS,
            &levels,
            &tone,
            rack(),
            Folded::default(),
            Rack::Full,
        );
        for i in 0..40 {
            levels.push(if i % 8 < 3 { 0.5 } else { 0.001 });
            levels.push_fire(if i % 8 == 1 { 6.0 } else { 0.0 });
            levels.push_ess(if i % 8 == 1 { 4.0 } else { -20.0 }, -22.0);
        }
        let mut busy = anyrender::Scene::new();
        super::lanes(
            &mut busy,
            &ALL_PANELS,
            &levels,
            &tone,
            rack(),
            Folded::default(),
            Rack::Full,
        );
        assert!(
            busy.commands.len() >= silent.commands.len() + 2,
            "a door lane and a fire lane"
        );
    }

    /// Every grip names a panel that is in the chain.
    #[test]
    fn every_grip_belongs_to_a_unit() {
        for grip in [
            Grip::Hold(Which::Gate),
            Grip::Range,
            Grip::Bias,
            Grip::Tilt,
            Grip::Depth(Which::DeEss),
            Grip::Sharpness(Which::DeEss),
            Grip::Edge(Which::DeEss, Side::High),
            Grip::Time,
            Grip::Feedback,
            Grip::Decay,
            Grip::Predelay,
            Grip::Mix(Which::Sat),
            Grip::Family(Which::Reverb),
        ] {
            assert!(ALL_PANELS.contains(&grip.panel()), "{grip:?}");
        }
        assert!(Grip::Family(Which::Delay).is_switch());
        assert!(!Grip::Time.is_switch());
    }
}

#[cfg(test)]
mod themed_tests {
    use super::*;

    /// Counts the fills of one colour.
    fn fills_of(scene: &Scene, rgb: [u8; 3]) -> usize {
        scene
            .commands
            .iter()
            .filter(|c| match c {
                anyrender::recording::RenderCommand::Fill(f) => match &f.brush {
                    anyrender::Paint::Solid(c) => c.to_rgba8().to_u8_array()[..3] == rgb,
                    _ => false,
                },
                _ => false,
            })
            .count()
    }

    /// Every tier of the rack, the rail's included, is the theme's when
    /// it has the roles: a theme with a red well puts red in each.
    #[test]
    fn every_tier_of_the_rack_is_drawn_by_the_theme() {
        let src = include_str!("../themes/console/rack.frame");
        let tokens = include_str!("../themes/console/tokens.frame").replace(
            r##"<Var name="rack.well" value="#0a0a0c" />"##,
            r##"<Var name="rack.well" value="#ff0000" />"##,
        );
        let theme = theme_runtime::Theme::from_sources(
            &[("tokens", &tokens), ("rack", src)],
            crate::skin::schema(),
            None,
        )
        .unwrap();
        crate::skin::set(Some(std::sync::Arc::new(theme)));
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = Font::embedded().expect("the embedded face");
        let tone = placeholder(0);
        for width in [60.0, 80.0, 130.0, 300.0] {
            let mut scene = Scene::new();
            record(
                &mut scene,
                &palette,
                &font,
                &tone,
                &[Which::Eq, Which::Comp],
                Panel {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height: 400.0,
                },
                Folded::rest(),
                None,
            );
            assert!(
                fills_of(&scene, [255, 0, 0]) > 0,
                "no theme well at {width} ({:?})",
                Rack::at(width)
            );
        }
        crate::skin::set(None);
    }
}

#[cfg(test)]
mod drum_verb_tests {
    use super::{AlgorithmType as A, Role, placeholder_for};

    /// What a return opens on, as the seed decides it: its role from
    /// its place, its presets from its name.
    fn opens_on(path: &[&str]) -> (Role, Option<A>, Option<String>) {
        let Some((name, folders)) = path.split_last() else {
            return (Role::Channel, None, None);
        };
        let ancestors: Vec<String> = folders.iter().map(|f| (*f).to_owned()).collect();
        let role = Role::of(name, false, &ancestors);
        let tone = placeholder_for(role, 0, name, &ancestors);
        let first = tone.presets.first();
        (
            role,
            first.map(|p| p.tone.reverb.algorithm),
            first.map(|p| p.name.clone()),
        )
    }

    /// Each drum Verb bank is its rooms by name, and each room opens on
    /// its own machine: the kick in a room, the snare on a plate, in a
    /// hall or gated, the toms in a room or gated.
    #[test]
    fn the_drum_verbs_open_on_their_algorithms() {
        for (path, algorithm) in [
            (&["Drum Kit", "Kick", "Verb", "Room"][..], A::Room),
            (&["Drum Kit", "Snare", "Verb", "Plate"][..], A::Plate),
            (&["Drum Kit", "Snare", "Verb", "Hall"][..], A::Hall),
            (&["Drum Kit", "Snare", "Verb", "Nonlin"][..], A::NonLinear),
            (&["Drum Kit", "Toms", "Verb", "Room"][..], A::Room),
            (&["Drum Kit", "Toms", "Verb", "Nonlin"][..], A::NonLinear),
        ] {
            let (role, opened, _) = opens_on(path);
            assert_eq!(role, Role::Reverb, "{path:?} is a reverb return");
            assert_eq!(opened, Some(algorithm), "{path:?} opens on {algorithm:?}");
        }
    }

    /// The new slots do not move the vocal's: its Short, Long and Room
    /// open on what they always did, and a name that says nothing still
    /// lands on the middle slot.
    #[test]
    fn the_vocal_verbs_keep_their_slots() {
        let vocal = |slot: &str| opens_on(&["Vocals", "Vox FX", "Verb", slot]).2;
        assert_eq!(vocal("Short").as_deref(), Some("Plate 1.4"));
        assert_eq!(vocal("Long").as_deref(), Some("Cloud 4.0"));
        assert_eq!(vocal("Room").as_deref(), Some("Small Room"));
        let plain = placeholder_for(Role::Reverb, 0, "Verb", &[]);
        assert_eq!(
            plain.presets.first().map(|p| p.name.as_str()),
            Some("Cloud 4.0")
        );
    }
}
