//! Continuous controllers, pinned behind the roll.
//!
//! These are **document-level** curves, not the per-note expression
//! lanes. An orchestral part rides CC1 and CC11 across a whole phrase
//! regardless of where the note boundaries fall, so binding them to
//! notes — the way MPE pressure and timbre are bound — would be the
//! wrong model entirely.
//!
//! Pinned lanes draw across the full height of the piano roll at low
//! opacity, so the shape of a swell is visible *while* you edit notes
//! rather than only when you switch to a lane strip. Editing one means
//! entering CC edit mode, where that lane takes the full 0..127 range
//! of the roll and the notes recede.

use crate::doc::Curve;

/// One controller lane.
#[derive(Clone, Debug, PartialEq)]
pub struct CcLane {
    /// Controller number, 0..127.
    pub number: u8,
    /// Display name. Defaults to the standard MIDI name, but an
    /// orchestral library that repurposes a controller can rename it.
    pub name: String,
    /// Index into [`CC_COLORS`].
    pub color: usize,
    /// Drawn behind the roll.
    pub pinned: bool,
    /// Values are normalized 0..1; the wire value is `round(v * 127)`.
    pub curve: Curve,
}

impl CcLane {
    pub fn new(number: u8, name: impl Into<String>, color: usize) -> Self {
        Self {
            number,
            name: name.into(),
            color,
            pinned: false,
            curve: Curve::new(),
        }
    }

    /// The 0..127 value at `t`.
    pub fn value(&self, t: f64) -> u8 {
        (self.curve.sample(t, self.default_value()) * 127.0)
            .round()
            .clamp(0.0, 127.0) as u8
    }

    /// Where a lane rests when nothing is authored.
    ///
    /// Volume-like controllers rest at full, everything else at zero —
    /// a CC11 lane that defaults to silence would mute the part the
    /// moment it is pinned.
    pub fn default_value(&self) -> f64 {
        match self.number {
            7 | 11 => 1.0,
            10 => 0.5,
            _ => 0.0,
        }
    }

    pub fn label(&self) -> String {
        format!("CC{} {}", self.number, self.name)
    }
}

/// Standard name for a controller number.
pub fn standard_name(number: u8) -> &'static str {
    match number {
        1 => "Modulation",
        2 => "Breath",
        4 => "Foot",
        5 => "Portamento",
        7 => "Volume",
        10 => "Pan",
        11 => "Expression",
        64 => "Sustain",
        68 => "Legato",
        _ => "Controller",
    }
}

/// Distinct hues for pinned lanes, chosen to stay legible against the
/// roll at low opacity and to be told apart from the note colours.
pub const CC_COLORS: [&str; 8] = [
    "#38bdf8", // sky
    "#f472b6", // pink
    "#a3e635", // lime
    "#fbbf24", // amber
    "#c084fc", // violet
    "#2dd4bf", // teal
    "#fb7185", // rose
    "#94a3b8", // slate
];

/// The document's controller lanes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CcSet {
    pub lanes: Vec<CcLane>,
}

impl CcSet {
    /// The orchestral default: modulation and expression, both pinned.
    ///
    /// CC1 and CC11 are what a sample library actually rides — CC1 for
    /// dynamics/vibrato intensity, CC11 for volume shaping — so those
    /// are the two worth showing without being asked.
    pub fn orchestral() -> Self {
        let mut cc1 = CcLane::new(1, standard_name(1), 0);
        cc1.pinned = true;
        let mut cc11 = CcLane::new(11, standard_name(11), 1);
        cc11.pinned = true;
        Self {
            lanes: vec![cc1, cc11],
        }
    }

    pub fn get(&self, number: u8) -> Option<&CcLane> {
        self.lanes.iter().find(|l| l.number == number)
    }

    pub fn get_mut(&mut self, number: u8) -> Option<&mut CcLane> {
        self.lanes.iter_mut().find(|l| l.number == number)
    }

    /// Add a lane, or return the existing one's index.
    pub fn ensure(&mut self, number: u8) -> usize {
        if let Some(i) = self.lanes.iter().position(|l| l.number == number) {
            return i;
        }
        let color = self.lanes.len() % CC_COLORS.len();
        self.lanes
            .push(CcLane::new(number, standard_name(number), color));
        self.lanes.len() - 1
    }

    pub fn remove(&mut self, number: u8) -> bool {
        match self.lanes.iter().position(|l| l.number == number) {
            Some(i) => {
                self.lanes.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn toggle_pin(&mut self, number: u8) -> bool {
        match self.get_mut(number) {
            Some(l) => {
                l.pinned = !l.pinned;
                l.pinned
            }
            None => false,
        }
    }

    /// Lanes drawn behind the roll, in declaration order.
    pub fn pinned(&self) -> impl Iterator<Item = &CcLane> {
        self.lanes.iter().filter(|l| l.pinned)
    }

    pub fn pinned_count(&self) -> usize {
        self.lanes.iter().filter(|l| l.pinned).count()
    }

    pub fn color_of(&self, number: u8) -> &'static str {
        self.get(number)
            .map(|l| CC_COLORS[l.color % CC_COLORS.len()])
            .unwrap_or(CC_COLORS[7])
    }
}

/// How pinned lanes are drawn behind the notes.
#[derive(Clone, Copy, Debug, PartialEq, facet::Facet)]
pub struct CcDisplay {
    /// Opacity of a pinned lane that is not being edited. Low on
    /// purpose — several controllers at full strength turn the roll
    /// into a thicket and the notes stop being findable.
    pub background_opacity: f64,
    /// Opacity of the lane currently being edited.
    pub active_opacity: f64,
    /// Fill under the curve, as a fraction of the line opacity. A faint
    /// fill is what makes a swell read as a shape rather than a
    /// squiggle.
    pub fill_ratio: f64,
    /// How far the notes fade while CC edit mode is on.
    pub note_dim: f64,
}

impl Default for CcDisplay {
    fn default() -> Self {
        Self {
            background_opacity: 0.22,
            active_opacity: 0.95,
            fill_ratio: 0.35,
            note_dim: 0.30,
        }
    }
}

/// Map a normalized CC value to a y within a box of height `h`.
///
/// Pinned lanes use the roll's full height so 0..127 spans the visible
/// pitch range — the curve is a shape over time, and giving it the
/// whole canvas is the point of pinning it.
pub fn cc_y(value: f64, h: f64) -> f64 {
    h * (1.0 - value.clamp(0.0, 1.0))
}

/// Inverse of [`cc_y`].
pub fn cc_value(y: f64, h: f64) -> f64 {
    (1.0 - y / h.max(1.0)).clamp(0.0, 1.0)
}
