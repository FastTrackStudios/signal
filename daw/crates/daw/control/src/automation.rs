//! Automation handles for envelopes

use std::sync::Arc;

use crate::Result;
use crate::{DawClients, Error};
use daw_proto::{
    ProjectContext,
    automation::{
        AddAutomationItemParams, AddPointParams, AutomationItem, Envelope, EnvelopeLocation,
        EnvelopePoint, EnvelopeRef, EnvelopeShape, EnvelopeType, LaneParams,
        SetAutomationItemParams, SetPointParams, TakeEnvelopeKind, TimeRangeParams,
    },
    primitives::{AutomationMode, Duration, PositionInSeconds},
    track::TrackRef,
};

/// Envelopes accessor for a track
#[derive(Clone)]
pub struct Envelopes {
    track_guid: String,
    project_id: String,
    clients: Arc<DawClients>,
}

impl Envelopes {
    /// Create a new envelopes handle
    pub(crate) fn new(track_guid: String, project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            track_guid,
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create track reference
    fn track_ref(&self) -> TrackRef {
        TrackRef::Guid(self.track_guid.clone())
    }

    /// Get all envelopes for this track
    pub async fn all(&self) -> Result<Vec<Envelope>> {
        let envelopes = self
            .clients
            .automation
            .envelopes(self.context(), self.track_ref())
            .await?;
        Ok(envelopes)
    }

    /// Get envelope by type
    pub async fn by_type(&self, envelope_type: EnvelopeType) -> Result<Option<EnvelopeHandle>> {
        let location = EnvelopeLocation::new(self.track_ref(), EnvelopeRef::Type(envelope_type));
        let envelope = self
            .clients
            .automation
            .envelope(self.context(), location)
            .await?;

        Ok(envelope.map(|_| {
            EnvelopeHandle::new(
                self.track_guid.clone(),
                EnvelopeRef::Type(envelope_type),
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }

    /// Get volume envelope
    pub fn volume(&self) -> EnvelopeHandle {
        EnvelopeHandle::new(
            self.track_guid.clone(),
            EnvelopeRef::Type(EnvelopeType::Volume),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    /// The envelope `reference` names — there or not yet: a first point
    /// written to it makes it.
    pub fn of(&self, reference: EnvelopeRef) -> EnvelopeHandle {
        EnvelopeHandle::new(
            self.track_guid.clone(),
            reference,
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    /// Get pan envelope
    pub fn pan(&self) -> EnvelopeHandle {
        EnvelopeHandle::new(
            self.track_guid.clone(),
            EnvelopeRef::Type(EnvelopeType::Pan),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    /// Get FX parameter envelope
    pub fn fx_param(&self, fx_guid: &str, param_index: u32) -> EnvelopeHandle {
        EnvelopeHandle::new(
            self.track_guid.clone(),
            EnvelopeRef::FxParam {
                fx_guid: fx_guid.to_string(),
                param_index,
            },
            self.project_id.clone(),
            self.clients.clone(),
        )
    }
}

/// A take's own envelopes.
///
/// Separate from [`Envelopes`], which is track-scoped, because a take
/// envelope has no track: the item and take carry the whole context and
/// `EnvelopeLocation.track` is documented as ignored for them. Reaching
/// them through a track handle would mean asking a caller to supply a
/// value that is then discarded.
#[derive(Clone)]
pub struct TakeEnvelopes {
    item_guid: String,
    take_guid: String,
    project_id: String,
    clients: Arc<DawClients>,
}

impl TakeEnvelopes {
    pub(crate) fn new(
        item_guid: String,
        take_guid: String,
        project_id: String,
        clients: Arc<DawClients>,
    ) -> Self {
        Self {
            item_guid,
            take_guid,
            project_id,
            clients,
        }
    }

    /// A take envelope of the given kind.
    pub fn of_kind(&self, kind: TakeEnvelopeKind) -> EnvelopeHandle {
        EnvelopeHandle::new(
            // Ignored downstream, and passed only because the location
            // struct has the field.
            String::new(),
            EnvelopeRef::Take {
                item_guid: self.item_guid.clone(),
                take_guid: self.take_guid.clone(),
                kind,
            },
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    /// Take volume — the per-item gain, and where a dynamics pass
    /// writes.
    pub fn volume(&self) -> EnvelopeHandle {
        self.of_kind(TakeEnvelopeKind::Volume)
    }

    pub fn pan(&self) -> EnvelopeHandle {
        self.of_kind(TakeEnvelopeKind::Pan)
    }

    pub fn mute(&self) -> EnvelopeHandle {
        self.of_kind(TakeEnvelopeKind::Mute)
    }

    pub fn pitch(&self) -> EnvelopeHandle {
        self.of_kind(TakeEnvelopeKind::Pitch)
    }
}

impl std::fmt::Debug for TakeEnvelopes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TakeEnvelopes")
            .field("item_guid", &self.item_guid)
            .field("take_guid", &self.take_guid)
            .finish()
    }
}

impl std::fmt::Debug for Envelopes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Envelopes")
            .field("track_guid", &self.track_guid)
            .field("project_id", &self.project_id)
            .finish()
    }
}

// =============================================================================
// EnvelopeHandle
// =============================================================================

/// Handle to a single automation envelope
#[derive(Clone)]
pub struct EnvelopeHandle {
    track_guid: String,
    envelope_ref: EnvelopeRef,
    project_id: String,
    clients: Arc<DawClients>,
}

impl EnvelopeHandle {
    /// Create a new envelope handle
    pub(crate) fn new(
        track_guid: String,
        envelope_ref: EnvelopeRef,
        project_id: String,
        clients: Arc<DawClients>,
    ) -> Self {
        Self {
            track_guid,
            envelope_ref,
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create envelope location
    fn location(&self) -> EnvelopeLocation {
        EnvelopeLocation::new(
            TrackRef::Guid(self.track_guid.clone()),
            self.envelope_ref.clone(),
        )
    }

    // =========================================================================
    // Info
    // =========================================================================

    /// Get full envelope state
    pub async fn info(&self) -> Result<Envelope> {
        self.clients
            .automation
            .envelope(self.context(), self.location())
            .await?
            .ok_or_else(|| Error::Other("Envelope not found".to_string()))
    }

    // =========================================================================
    // State
    // =========================================================================

    /// Mark this parameter's control as touched (Touch/Latch
    /// automation gating) — surfaces call this from fader-touch
    /// sensors.
    pub async fn touch(&self) -> Result<()> {
        self.clients
            .automation
            .touch_param(self.context(), self.location())
            .await??;
        Ok(())
    }

    /// Release a touched parameter.
    pub async fn release(&self) -> Result<()> {
        self.clients
            .automation
            .release_param(self.context(), self.location())
            .await??;
        Ok(())
    }

    /// Write a value through the automation engine: updates the
    /// static value AND records an envelope point when the mode +
    /// touch state + transport allow.
    pub async fn write(&self, value: f64) -> Result<()> {
        self.clients
            .automation
            .write_param(self.context(), self.location(), value)
            .await??;
        Ok(())
    }

    /// Set envelope visibility
    pub async fn set_visible(&self, visible: bool) -> Result<()> {
        self.clients
            .automation
            .set_visible(self.context(), self.location(), visible)
            .await?;
        Ok(())
    }

    /// Show the envelope
    pub async fn show(&self) -> Result<()> {
        self.set_visible(true).await
    }

    /// Hide the envelope
    pub async fn hide(&self) -> Result<()> {
        self.set_visible(false).await
    }

    /// Set envelope armed state
    pub async fn set_armed(&self, armed: bool) -> Result<()> {
        self.clients
            .automation
            .set_armed(self.context(), self.location(), armed)
            .await?;
        Ok(())
    }

    /// Arm the envelope for recording
    pub async fn arm(&self) -> Result<()> {
        self.set_armed(true).await
    }

    /// Disarm the envelope
    pub async fn disarm(&self) -> Result<()> {
        self.set_armed(false).await
    }

    /// Set automation mode
    pub async fn set_automation_mode(&self, mode: AutomationMode) -> Result<()> {
        self.clients
            .automation
            .set_automation_mode(self.context(), self.location(), mode)
            .await?;
        Ok(())
    }

    // =========================================================================
    // Lane
    // =========================================================================

    /// Give this envelope a lane of its own, `height` px tall.
    ///
    /// The lane is where FX parameter automation becomes readable — a
    /// parameter curve overlaid on a waveform is not. Heights below the
    /// theme's `envcp_min_height` are clamped by the host.
    pub async fn show_in_lane(&self, height: u32) -> Result<()> {
        self.clients
            .automation
            .set_lane(self.context(), self.location(), LaneParams::own(height))
            .await??;
        Ok(())
    }

    /// Fold this envelope back over the track's own lane.
    pub async fn overlay(&self) -> Result<()> {
        self.clients
            .automation
            .set_lane(self.context(), self.location(), LaneParams::overlaid())
            .await??;
        Ok(())
    }

    // =========================================================================
    // Automation items
    // =========================================================================

    /// Every automation item on this envelope.
    pub async fn automation_items(&self) -> Result<Vec<AutomationItem>> {
        let items = self
            .clients
            .automation
            .automation_items(self.context(), self.location())
            .await?;
        Ok(items)
    }

    /// The points inside one automation item, in item-relative seconds.
    pub async fn automation_item_points(&self, index: u32) -> Result<Vec<EnvelopePoint>> {
        let points = self
            .clients
            .automation
            .automation_item_points(self.context(), self.location(), index)
            .await?;
        Ok(points)
    }

    /// Add an automation item over a range. Returns its index.
    pub async fn add_automation_item(
        &self,
        position: PositionInSeconds,
        length: Duration,
    ) -> Result<u32> {
        let index = self
            .clients
            .automation
            .add_automation_item(
                self.context(),
                self.location(),
                AddAutomationItemParams::new(position, length),
            )
            .await??;
        Ok(index)
    }

    /// Add another instance of an existing pool — edits propagate
    /// between instances, which is what pooling is for.
    pub async fn add_pooled_automation_item(
        &self,
        pool_id: i32,
        position: PositionInSeconds,
        length: Duration,
    ) -> Result<u32> {
        let index = self
            .clients
            .automation
            .add_automation_item(
                self.context(),
                self.location(),
                AddAutomationItemParams::pooled(pool_id, position, length),
            )
            .await??;
        Ok(index)
    }

    /// Move / resize / re-time one automation item.
    pub async fn set_automation_item(&self, params: SetAutomationItemParams) -> Result<()> {
        self.clients
            .automation
            .set_automation_item(self.context(), self.location(), params)
            .await??;
        Ok(())
    }

    /// Replace an automation item's curve (every instance of its pool).
    pub async fn set_automation_item_points(
        &self,
        index: u32,
        points: Vec<EnvelopePoint>,
    ) -> Result<()> {
        self.clients
            .automation
            .set_automation_item_points(
                self.context(),
                self.location(),
                daw_proto::automation::SetAutomationItemPointsParams { index, points },
            )
            .await??;
        Ok(())
    }

    /// Remove an automation item; the envelope's own points survive.
    pub async fn delete_automation_item(&self, index: u32) -> Result<()> {
        self.clients
            .automation
            .delete_automation_item(self.context(), self.location(), index)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Points
    // =========================================================================

    /// Get all points
    pub async fn points(&self) -> Result<Vec<EnvelopePoint>> {
        let points = self
            .clients
            .automation
            .points(self.context(), self.location())
            .await?;
        Ok(points)
    }

    /// Get points in a time range
    pub async fn points_in_range(
        &self,
        start: PositionInSeconds,
        end: PositionInSeconds,
    ) -> Result<Vec<EnvelopePoint>> {
        let points = self
            .clients
            .automation
            .points_in_range(
                self.context(),
                self.location(),
                TimeRangeParams::new(start, end),
            )
            .await?;
        Ok(points)
    }

    /// Get interpolated value at a time
    pub async fn value_at(&self, time: PositionInSeconds) -> Result<f64> {
        let value = self
            .clients
            .automation
            .value_at(self.context(), self.location(), time)
            .await?;
        Ok(value)
    }

    /// Add a point
    pub async fn add_point(
        &self,
        time: PositionInSeconds,
        value: f64,
        shape: EnvelopeShape,
    ) -> Result<u32> {
        let index = self
            .clients
            .automation
            .add_point(
                self.context(),
                self.location(),
                AddPointParams::new(time, value, shape),
            )
            .await?;
        Ok(index)
    }

    /// Add a point with linear shape
    pub async fn add_point_linear(&self, time: PositionInSeconds, value: f64) -> Result<u32> {
        self.add_point(time, value, EnvelopeShape::Linear).await
    }

    /// Delete a point
    pub async fn delete_point(&self, index: u32) -> Result<()> {
        self.clients
            .automation
            .delete_point(self.context(), self.location(), index)
            .await?;
        Ok(())
    }

    /// Set/update a point
    pub async fn set_point(
        &self,
        index: u32,
        time: PositionInSeconds,
        value: f64,
        shape: EnvelopeShape,
    ) -> Result<()> {
        self.clients
            .automation
            .set_point(
                self.context(),
                self.location(),
                SetPointParams {
                    index,
                    time,
                    value,
                    shape,
                },
            )
            .await?;
        Ok(())
    }

    /// Delete all points in a time range
    pub async fn delete_points_in_range(
        &self,
        start: PositionInSeconds,
        end: PositionInSeconds,
    ) -> Result<()> {
        self.clients
            .automation
            .delete_points_in_range(
                self.context(),
                self.location(),
                TimeRangeParams::new(start, end),
            )
            .await?;
        Ok(())
    }

    /// Clear all points
    pub async fn clear(&self) -> Result<()> {
        // Delete from 0 to a very large time
        self.delete_points_in_range(
            PositionInSeconds::ZERO,
            PositionInSeconds::from_seconds(86400.0 * 365.0), // ~1 year
        )
        .await
    }
}

impl std::fmt::Debug for EnvelopeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvelopeHandle")
            .field("track_guid", &self.track_guid)
            .field("envelope_ref", &self.envelope_ref)
            .field("project_id", &self.project_id)
            .finish()
    }
}
