//! The vocal envelope surface: the composite, with the four as
//! togglable overlays, edited in place.
//!
//! One envelope is **active** and receives the drag; the others are
//! visible but inert. Proximity hit-testing is rejected for the same
//! reason #200 rejects it for tracks in a lane: four overlaid curves
//! cross constantly, and an edit landing on whichever happened to be
//! nearer means silently modifying the gate when you meant the ride.
//! "What am I editing" has to be answerable by looking.
//!
//! The ride is active by default, because it is the curve you will edit
//! most.
//!
//! This module holds the state and the geometry — everything assertable
//! without a renderer. The `rsx!` lives with the rest of the canvas.

use level_dsp::envelope::{Contributions, EnvPoint, composite};

/// Which of the four a gesture is aimed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ActiveEnvelope {
    Gate,
    Breath,
    Sibilance,
    /// The broadband "how loud is this bit" curve, and the one edited
    /// most — hence the default.
    #[default]
    Ride,
}

impl ActiveEnvelope {
    pub const ALL: [ActiveEnvelope; 4] = [
        ActiveEnvelope::Gate,
        ActiveEnvelope::Breath,
        ActiveEnvelope::Sibilance,
        ActiveEnvelope::Ride,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ActiveEnvelope::Gate => "Gate",
            ActiveEnvelope::Breath => "Breath",
            ActiveEnvelope::Sibilance => "Sibilance",
            ActiveEnvelope::Ride => "Ride",
        }
    }
}

/// Which overlays are drawn.
///
/// Distinct from bypass: hiding an overlay changes what you *see*,
/// bypassing it changes what you *hear*. Conflating them would mean you
/// could not look at the composite alone without also changing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub gate: bool,
    pub breath: bool,
    pub sibilance: bool,
    pub ride: bool,
    pub composite: bool,
}

impl Default for Shown {
    fn default() -> Self {
        Self {
            gate: true,
            breath: true,
            sibilance: true,
            ride: true,
            composite: true,
        }
    }
}

impl Shown {
    pub fn is_shown(&self, which: ActiveEnvelope) -> bool {
        match which {
            ActiveEnvelope::Gate => self.gate,
            ActiveEnvelope::Breath => self.breath,
            ActiveEnvelope::Sibilance => self.sibilance,
            ActiveEnvelope::Ride => self.ride,
        }
    }

    pub fn toggle(&mut self, which: ActiveEnvelope) {
        let slot = match which {
            ActiveEnvelope::Gate => &mut self.gate,
            ActiveEnvelope::Breath => &mut self.breath,
            ActiveEnvelope::Sibilance => &mut self.sibilance,
            ActiveEnvelope::Ride => &mut self.ride,
        };
        *slot = !*slot;
    }

    /// Show one and hide the others — the "prioritise this" gesture.
    pub fn solo(&mut self, which: ActiveEnvelope) {
        self.gate = which == ActiveEnvelope::Gate;
        self.breath = which == ActiveEnvelope::Breath;
        self.sibilance = which == ActiveEnvelope::Sibilance;
        self.ride = which == ActiveEnvelope::Ride;
    }

    pub fn shown_count(&self) -> usize {
        ActiveEnvelope::ALL
            .iter()
            .filter(|e| self.is_shown(**e))
            .count()
    }
}

/// The surface's state.
#[derive(Clone, Debug, Default)]
pub struct EnvelopePanel {
    pub active: ActiveEnvelope,
    pub shown: Shown,
}

impl EnvelopePanel {
    /// Where a drag goes.
    ///
    /// Always the active envelope, regardless of which curve is nearer
    /// the cursor. The whole point.
    pub fn drag_target(&self) -> ActiveEnvelope {
        self.active
    }

    /// Make one active. Activating an envelope also shows it — editing
    /// something you cannot see is worse than an extra click.
    pub fn activate(&mut self, which: ActiveEnvelope) {
        self.active = which;
        match which {
            ActiveEnvelope::Gate => self.shown.gate = true,
            ActiveEnvelope::Breath => self.shown.breath = true,
            ActiveEnvelope::Sibilance => self.shown.sibilance = true,
            ActiveEnvelope::Ride => self.shown.ride = true,
        }
    }

    /// Step to the next envelope, wrapping.
    pub fn cycle(&mut self) {
        let at = ActiveEnvelope::ALL
            .iter()
            .position(|e| *e == self.active)
            .unwrap_or(0);
        self.activate(ActiveEnvelope::ALL[(at + 1) % ActiveEnvelope::ALL.len()]);
    }
}

/// One curve, ready to draw.
pub struct EnvelopeTrace {
    pub which: Option<ActiveEnvelope>,
    /// `(x, y)` in panel pixels.
    pub points: Vec<(f64, f64)>,
    /// Drawn brighter and taking gestures.
    pub active: bool,
    /// The derived result rather than a contribution.
    pub is_composite: bool,
}

/// Map a dB value into a panel of `height` pixels over `range_db`.
fn y_of(db: f64, height: f64, range_db: f64) -> f64 {
    // 0 dB sits at the top; the range runs downward.
    let clamped = db.clamp(-range_db, 0.0);
    (-clamped / range_db) * height
}

/// Lay out every visible curve for a panel.
///
/// The composite is drawn even when every contribution is hidden — it is
/// what the DAW plays, and a panel showing nothing at all would be
/// indistinguishable from a broken one.
pub fn traces(
    parts: &Contributions,
    panel: &EnvelopePanel,
    width: f64,
    height: f64,
    duration_s: f64,
    range_db: f64,
) -> Vec<EnvelopeTrace> {
    let dur = duration_s.max(1e-6);
    let x_of = |t: f64| (t / dur).clamp(0.0, 1.0) * width;
    let line = |pts: &[EnvPoint]| -> Vec<(f64, f64)> {
        pts.iter()
            .map(|p| (x_of(p.t_s), y_of(p.db, height, range_db)))
            .collect()
    };

    let mut out = Vec::new();
    let mut push = |which: ActiveEnvelope, pts: Vec<(f64, f64)>| {
        if panel.shown.is_shown(which) && !pts.is_empty() {
            out.push(EnvelopeTrace {
                which: Some(which),
                points: pts,
                active: panel.active == which,
                is_composite: false,
            });
        }
    };

    push(ActiveEnvelope::Gate, line(&parts.gate));
    push(ActiveEnvelope::Breath, line(&parts.breath));
    push(
        ActiveEnvelope::Sibilance,
        parts
            .sibilance
            .iter()
            .flat_map(|s| {
                [
                    (x_of(s.from_s), y_of(0.0, height, range_db)),
                    (x_of(s.from_s), y_of(s.db, height, range_db)),
                    (x_of(s.to_s), y_of(s.db, height, range_db)),
                    (x_of(s.to_s), y_of(0.0, height, range_db)),
                ]
            })
            .collect(),
    );
    push(ActiveEnvelope::Ride, line(&parts.ride));

    if panel.shown.composite {
        let c = line(&composite(parts));
        if !c.is_empty() {
            out.push(EnvelopeTrace {
                which: None,
                points: c,
                active: false,
                is_composite: true,
            });
        }
    }
    out
}
