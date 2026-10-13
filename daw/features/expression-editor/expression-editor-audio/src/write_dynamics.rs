//! Writing the lanes to the host: one volume envelope, and markers.
//!
//! The sum of the four lanes goes to the take's **volume envelope**,
//! which is the one per-item gain every host already applies — so the
//! result is audible with no plugin in the chain, and stays adjustable
//! in the DAW afterwards.
//!
//! Detected breaths and sibilants also go on as **take markers**,
//! whether or not they are being ducked. A marker is an annotation, not
//! a process: it says "there is a breath here", which is useful to see
//! even when the decision is to leave it alone.

use daw::service::automation::{AddPointParams, Automation, EnvelopeLocation, EnvelopeRef};
use daw::service::{TakeEnvelopeKind, TakeMarkerCreate, Takes, TrackRef};

use crate::dynamics::{Detection, Dynamics};
use crate::lanes::{Lanes, db_to_take_volume, thin};
use crate::session::AudioSession;

/// How much a point may deviate from the line between its neighbours
/// before it is worth keeping.
///
/// Quarter of a dB is below what anyone hears on a gain ride and well
/// above the noise in a detector's output, so the curve keeps its shape
/// and loses its thousands of redundant points.
pub const THIN_TOLERANCE_DB: f64 = 0.25;

/// What a dynamics write did.
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicsWritten {
    /// Envelope points written, after thinning.
    pub points: usize,
    /// Take markers added.
    pub markers: usize,
}

impl AudioSession {
    /// Write the lane sum as the take's volume envelope.
    ///
    /// Replaces whatever was there: the envelope is *derived* from the
    /// lanes, so merging with a previous version would accumulate one
    /// pass on top of another and the gain would creep every time.
    ///
    /// With every lane off, the envelope is cleared rather than written
    /// flat at unity — dead automation on an item is something the user
    /// then has to find and delete.
    pub fn write_dynamics<D>(
        &self,
        daw: &D,
        lanes: &Lanes,
        dynamics: &Dynamics,
        mark: bool,
    ) -> DynamicsWritten
    where
        D: Automation + Takes,
    {
        let location = self.envelope_location();
        let frame_rate = self.analysis().frame_rate.max(1e-9);

        // Clear first, always. Both branches need it: a rewrite must
        // not merge, and a switch-off must leave nothing behind.
        let existing = daw.points(self.location.project.clone(), location.clone());
        for i in (0..existing.len()).rev() {
            daw.delete_point(self.location.project.clone(), location.clone(), i as u32);
        }

        let mut points = 0;
        if let Some(sum) = lanes.sum() {
            for p in thin(&sum, THIN_TOLERANCE_DB) {
                daw.add_point(
                    self.location.project.clone(),
                    location.clone(),
                    AddPointParams::new(
                        daw::service::PositionInSeconds::from_seconds(p.frame as f64 / frame_rate),
                        db_to_take_volume(p.db),
                        daw::service::automation::EnvelopeShape::Linear,
                    ),
                );
                points += 1;
            }
        }

        let mut markers = 0;
        if mark {
            for r in &dynamics.regions {
                let name = r.kind.label();
                // Anchored at the *source* position, so a marker stays
                // on its consonant when the item is trimmed or moved.
                let at = r.start as f64 / frame_rate;
                if daw
                    .add_take_marker(
                        self.location.project.clone(),
                        self.location.item.clone(),
                        self.location.take.clone(),
                        TakeMarkerCreate {
                            name: name.to_string(),
                            source_position_seconds: at,
                            color: marker_colour(r.kind),
                            ..Default::default()
                        },
                    )
                    .is_some()
                {
                    markers += 1;
                }
            }
        }

        DynamicsWritten { points, markers }
    }

    /// Where the take's volume envelope lives.
    pub fn envelope_location(&self) -> EnvelopeLocation {
        EnvelopeLocation {
            // Ignored for take envelopes — the item and take carry the
            // context. Named here only because the struct has the field.
            track: TrackRef::Index(0),
            envelope: EnvelopeRef::Take {
                item_guid: match &self.location.item {
                    daw::service::ItemRef::Guid(g) => g.clone(),
                    _ => String::new(),
                },
                take_guid: String::new(),
                kind: TakeEnvelopeKind::Volume,
            },
        }
    }
}

/// Distinct colours, so the two kinds are told apart at a glance
/// without reading every label.
fn marker_colour(kind: Detection) -> Option<u32> {
    match kind {
        Detection::Breath => Some(0x00_6E_8B_A8),
        Detection::Sibilance => Some(0x00_C2_7A_3A),
    }
}
