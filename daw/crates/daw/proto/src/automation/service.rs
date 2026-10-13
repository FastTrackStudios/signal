//! Automation service — envelopes and points.

use super::{AutomationItem, Envelope, EnvelopeLocation, EnvelopePoint, EnvelopeShape};
use crate::DawResult;
use crate::primitives::{AutomationMode, Duration, PositionInSeconds};
use crate::project::ProjectContext;
use crate::track::TrackRef;
use facet::Facet;

/// How an envelope's lane is laid out — see
/// [`Automation::set_lane`].
#[derive(Clone, Copy, Debug, Facet)]
pub struct LaneParams {
    /// A lane of its own, or drawn over the track's lane.
    pub in_own_lane: bool,
    /// The lane's height in pixels. Ignored when `in_own_lane` is
    /// false; clamped up to the theme's minimum by the host.
    pub height: u32,
}

impl LaneParams {
    /// A lane of its own, at `height`.
    pub fn own(height: u32) -> Self {
        Self {
            in_own_lane: true,
            height,
        }
    }

    /// Folded back over the track's lane.
    pub fn overlaid() -> Self {
        Self {
            in_own_lane: false,
            height: 0,
        }
    }
}

/// Parameters for [`Automation::add_automation_item`].
#[derive(Clone, Debug, Facet)]
pub struct AddAutomationItemParams {
    /// `-1` for a fresh pool; an existing id for another instance of
    /// that pool.
    pub pool_id: i32,
    pub position: PositionInSeconds,
    pub length: Duration,
}

impl AddAutomationItemParams {
    /// A new, unpooled item over a range.
    pub fn new(position: PositionInSeconds, length: Duration) -> Self {
        Self {
            pool_id: -1,
            position,
            length,
        }
    }

    /// Another instance of an existing pool — edits propagate between
    /// instances, which is the point of pooling.
    pub fn pooled(pool_id: i32, position: PositionInSeconds, length: Duration) -> Self {
        Self {
            pool_id,
            position,
            length,
        }
    }
}

/// Parameters for [`Automation::set_automation_item_points`].
#[derive(Clone, Debug, Facet)]
pub struct SetAutomationItemPointsParams {
    /// The item, by its index on the envelope.
    pub index: u32,
    /// Its whole curve, in seconds into its source, sorted by time.
    pub points: Vec<EnvelopePoint>,
}

/// Parameters for [`Automation::set_automation_item`].
///
/// Every field is optional: a drag sets position, a resize sets length,
/// a loop toggle sets one flag, and nothing else moves.
#[derive(Clone, Debug, Default, Facet)]
pub struct SetAutomationItemParams {
    pub index: u32,
    pub position: Option<PositionInSeconds>,
    pub length: Option<Duration>,
    pub start_offset: Option<Duration>,
    pub play_rate: Option<f64>,
    pub baseline: Option<f64>,
    pub amplitude: Option<f64>,
    pub loop_source: Option<bool>,
    pub selected: Option<bool>,
}

impl SetAutomationItemParams {
    /// Address an item, changing nothing yet.
    pub fn at(index: u32) -> Self {
        Self {
            index,
            ..Default::default()
        }
    }

    pub fn position(mut self, position: PositionInSeconds) -> Self {
        self.position = Some(position);
        self
    }

    pub fn length(mut self, length: Duration) -> Self {
        self.length = Some(length);
        self
    }
}

/// Parameters for adding an envelope point.
#[derive(Clone, Debug, Facet)]
pub struct AddPointParams {
    pub time: PositionInSeconds,
    /// Normalized value (0.0–1.0).
    pub value: f64,
    pub shape: EnvelopeShape,
}

impl AddPointParams {
    pub fn new(time: PositionInSeconds, value: f64, shape: EnvelopeShape) -> Self {
        Self { time, value, shape }
    }

    pub fn linear(time: PositionInSeconds, value: f64) -> Self {
        Self::new(time, value, EnvelopeShape::Linear)
    }
}

/// Parameters for setting an envelope point.
#[derive(Clone, Debug, Facet)]
pub struct SetPointParams {
    pub index: u32,
    pub time: PositionInSeconds,
    pub value: f64,
    pub shape: EnvelopeShape,
}

/// Time range for envelope operations.
#[derive(Clone, Debug, Facet)]
pub struct TimeRangeParams {
    pub start: PositionInSeconds,
    pub end: PositionInSeconds,
}

impl TimeRangeParams {
    pub fn new(start: PositionInSeconds, end: PositionInSeconds) -> Self {
        Self { start, end }
    }
}

#[architect::rpc]
pub trait Automation {
    // ── Envelopes ──────────────────────────────────────────────────

    fn envelopes(&self, project: ProjectContext, track: TrackRef) -> Vec<Envelope>;

    fn envelope(&self, project: ProjectContext, location: EnvelopeLocation) -> Option<Envelope>;

    fn set_visible(&self, project: ProjectContext, location: EnvelopeLocation, visible: bool);
    fn set_armed(&self, project: ProjectContext, location: EnvelopeLocation, armed: bool);

    /// Give the envelope a lane of its own, or fold it back over the
    /// track's, and set that lane's height.
    ///
    /// One call rather than two because they are one decision in
    /// REAPER's chunk (`VIS`'s lane flag beside the height) and a UI
    /// that resizes a lane always means "in its own lane, this tall".
    /// `height` below the theme's `envcp_min_height` is clamped by the
    /// host, not refused.
    fn set_lane(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        lane: LaneParams,
    ) -> DawResult<()>;

    // ── Automation items ───────────────────────────────────────────
    //
    // A windowed piece of automation that moves, loops and pools like a
    // media item. Reads first: the arrange has to *draw* them before
    // anything can edit them, and the editing surface (split, glue,
    // un-pool) is deliberately not modelled until a caller needs it.

    /// Every automation item on an envelope, in index order.
    fn automation_items(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
    ) -> Vec<AutomationItem>;

    /// The points *inside* one automation item — its own curve, in
    /// item-relative seconds.
    ///
    /// Separate from [`Automation::points`] because they are separate
    /// data in REAPER: the envelope's underlying points continue under
    /// an item, and an item's curve is its pooled source's.
    fn automation_item_points(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        index: u32,
    ) -> Vec<EnvelopePoint>;

    /// Add an automation item over a time range. Returns its index.
    ///
    /// `pool_id` of `-1` makes a fresh pool; an existing id creates
    /// another *instance* of that pool, which is how REAPER duplicates
    /// automation so edits propagate.
    fn add_automation_item(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        params: AddAutomationItemParams,
    ) -> DawResult<u32>;

    /// Move / resize / re-time one automation item.
    fn set_automation_item(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        params: SetAutomationItemParams,
    ) -> DawResult<()>;

    /// Replace an automation item's curve — its pooled source's, so every
    /// instance of its pool changes with it, which is what pooling is.
    fn set_automation_item_points(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        params: SetAutomationItemPointsParams,
    ) -> DawResult<()>;

    /// Remove an automation item. The envelope's own points are
    /// untouched — the item was a window, not the automation.
    fn delete_automation_item(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        index: u32,
    ) -> DawResult<()>;
    fn set_automation_mode(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        mode: AutomationMode,
    );

    // ── Points ─────────────────────────────────────────────────────

    fn points(&self, project: ProjectContext, location: EnvelopeLocation) -> Vec<EnvelopePoint>;

    fn points_in_range(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        range: TimeRangeParams,
    ) -> Vec<EnvelopePoint>;

    /// Interpolated value at a specific time.
    fn value_at(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        time: PositionInSeconds,
    ) -> f64;

    /// Add a point. Returns the point index.
    /// Mark a parameter's control as touched (Touch/Latch automation
    /// gating). Surfaces call this from fader-touch sensors.
    fn touch_param(&self, project: ProjectContext, location: EnvelopeLocation) -> DawResult<()>;

    /// Release a touched parameter.
    fn release_param(&self, project: ProjectContext, location: EnvelopeLocation) -> DawResult<()>;

    /// Write a parameter value through the automation engine: updates
    /// the static value AND records an envelope point when the mode +
    /// touch state + transport allow (REAPER's touch/latch/write).
    fn write_param(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        value: f64,
    ) -> DawResult<()>;

    fn add_point(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        params: AddPointParams,
    ) -> u32;

    fn delete_point(&self, project: ProjectContext, location: EnvelopeLocation, index: u32);

    fn set_point(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        params: SetPointParams,
    );

    fn delete_points_in_range(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        range: TimeRangeParams,
    );

    // ── Global ─────────────────────────────────────────────────────

    /// Global automation override (`None` if not overridden).
    fn global_automation_override(&self, project: ProjectContext) -> Option<AutomationMode>;

    /// Set global automation override (`None` to clear).
    fn set_global_automation_override(&self, project: ProjectContext, mode: Option<AutomationMode>);
}
