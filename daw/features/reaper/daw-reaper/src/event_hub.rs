//! Central event hub for daw-reaper streaming.
//!
//! Owns one [`broadcast::Sender`] per streaming domain. All
//! change-detection / poll sites push into the relevant sender;
//! the streaming-service impls subscribe a `Receiver` per client
//! and forward into the vox `Tx`.
//!
//! See `docs/streaming-design.md` (central hub pattern, occasional
//! vs continuous split). Mirrors helgobox's `ProtoSenders`
//! shape: one struct, one sender per stream, all wired through the
//! same hub instance.
//!
//! # Phase 1 scope
//!
//! Only transport streams are wired today (state + position). The
//! struct is shaped to grow — adding a new domain stream is one
//! field + one `subscribe_*` method + one `publish_*` method.

use std::sync::OnceLock;

use daw_proto::MeterFrame;
use daw_proto::StampedPosition;
use daw_proto::event_bus::DawEvent;
use daw_proto::marker::MarkerStreamEvent;
use daw_proto::project::ProjectStreamEvent;
use daw_proto::region::RegionStreamEvent;
use daw_proto::tempo_map::TempoMapStreamEvent;
use daw_proto::track::TrackStreamEvent;
use daw_proto::transport::{PositionTick, TransportEvent, TransportStreamEvent};
use tokio::sync::broadcast;

// Buffer sizes. Matched to helgobox's defaults for occasional
// channels; continuous channels run smaller because subscribers
// can drop intermediate samples safely.
const OCCASIONAL_BUFFER: usize = 128;
const CONTINUOUS_BUFFER: usize = 16;

/// The hub. One instance per process — fetched via [`hub()`].
///
/// Clone-cheap (each field is a `broadcast::Sender` or an
/// `architect::PubSub`, both effectively `Arc`s internally), but
/// consumers should reach for [`hub()`] rather than copying fields
/// around.
///
/// Two fan-out layers coexist:
///
/// - **broadcast senders** — in-process consumers (sync engine,
///   diagnostics, transport streaming pumps).
/// - **`architect::PubSub` hubs** — the wire surface for the
///   `#[subscribe]` stream services (tracks / markers / regions /
///   tempo map) plus the cross-domain [`DawEvent`] bus. Each
///   `publish_*` feeds both, so poll sites keep one publish call.
#[derive(Clone)]
pub struct DawEventHub {
    // ── Occasional ───────────────────────────────────────────────
    /// Transport state transitions (play/stop/record/tempo/loop).
    transport_state_tx: broadcast::Sender<TransportEvent>,
    /// Marker add/remove/modify events.
    markers_tx: broadcast::Sender<MarkerStreamEvent>,
    /// Region add/remove/modify events.
    regions_tx: broadcast::Sender<RegionStreamEvent>,
    /// Track add/remove/modify events.
    tracks_tx: broadcast::Sender<TrackStreamEvent>,
    /// Tempo map point add/remove/modify events.
    tempo_map_tx: broadcast::Sender<TempoMapStreamEvent>,
    /// Project lifecycle (open/close/active-tab switch).
    projects_tx: broadcast::Sender<ProjectStreamEvent>,

    // ── Continuous ───────────────────────────────────────────────
    /// Position ticks. Pushed at ~30Hz from the REAPER main loop.
    /// Drop-old semantics on backpressure.
    position_tx: broadcast::Sender<PositionTick>,

    // ── `#[subscribe]` stream hubs (wire subscribers) ────────────
    markers_hub: architect::PubSub<MarkerStreamEvent>,
    regions_hub: architect::PubSub<RegionStreamEvent>,
    tracks_hub: architect::PubSub<TrackStreamEvent>,
    tempo_map_hub: architect::PubSub<TempoMapStreamEvent>,
    /// Transport-domain `#[subscribe]` stream hub (`TransportStreamSource`).
    /// Fed by the 30 Hz REAPER poll (state changes + position ticks).
    transport_hub: architect::PubSub<TransportStreamEvent>,
    /// Live meter-frame hub (`PeaksStreamSource`). Fed by the ~30 Hz
    /// REAPER meter poll (`peak::poll_and_broadcast_meters`); one
    /// [`MeterFrame`] per tick for the active project. Continuous
    /// data — NOT bridged onto the cross-domain bus (same policy as
    /// daw-standalone's meter pump).
    meters_hub: architect::PubSub<MeterFrame>,
    /// Cross-domain bus — every domain's publish also lands here,
    /// wrapped in [`DawEvent`]. Subscribers filter client-side.
    bus_hub: architect::PubSub<DawEvent>,
    /// Stamped-position hub (`TransportSyncStreamSource`). Fed by the
    /// sync-position pump in [`crate::transport_sync`] from
    /// daw-audio-sync's per-buffer snapshots. Continuous — NOT bridged
    /// onto the cross-domain bus.
    sync_positions_hub: architect::PubSub<StampedPosition>,
}

impl DawEventHub {
    fn new() -> Self {
        Self {
            transport_state_tx: broadcast::channel(OCCASIONAL_BUFFER).0,
            markers_tx: broadcast::channel(OCCASIONAL_BUFFER).0,
            regions_tx: broadcast::channel(OCCASIONAL_BUFFER).0,
            tracks_tx: broadcast::channel(OCCASIONAL_BUFFER).0,
            tempo_map_tx: broadcast::channel(OCCASIONAL_BUFFER).0,
            projects_tx: broadcast::channel(OCCASIONAL_BUFFER).0,
            position_tx: broadcast::channel(CONTINUOUS_BUFFER).0,
            markers_hub: architect::PubSub::sliding(OCCASIONAL_BUFFER),
            regions_hub: architect::PubSub::sliding(OCCASIONAL_BUFFER),
            tracks_hub: architect::PubSub::sliding(OCCASIONAL_BUFFER),
            tempo_map_hub: architect::PubSub::sliding(OCCASIONAL_BUFFER),
            transport_hub: architect::PubSub::sliding(CONTINUOUS_BUFFER),
            meters_hub: architect::PubSub::sliding(CONTINUOUS_BUFFER),
            bus_hub: architect::PubSub::sliding(OCCASIONAL_BUFFER),
            sync_positions_hub: architect::PubSub::sliding(64),
        }
    }

    // ── Stream hubs (mounted via `*::StreamService`) ─────────────

    pub fn markers_hub(&self) -> &architect::PubSub<MarkerStreamEvent> {
        &self.markers_hub
    }

    pub fn regions_hub(&self) -> &architect::PubSub<RegionStreamEvent> {
        &self.regions_hub
    }

    pub fn tracks_hub(&self) -> &architect::PubSub<TrackStreamEvent> {
        &self.tracks_hub
    }

    pub fn tempo_map_hub(&self) -> &architect::PubSub<TempoMapStreamEvent> {
        &self.tempo_map_hub
    }

    pub fn bus_hub(&self) -> &architect::PubSub<DawEvent> {
        &self.bus_hub
    }

    pub fn transport_hub(&self) -> &architect::PubSub<TransportStreamEvent> {
        &self.transport_hub
    }

    pub fn meters_hub(&self) -> &architect::PubSub<MeterFrame> {
        &self.meters_hub
    }

    pub fn sync_positions_hub(&self) -> &architect::PubSub<StampedPosition> {
        &self.sync_positions_hub
    }

    // ── Meters ───────────────────────────────────────────────────

    /// Publish one live meter frame. Continuous / high-rate — goes to
    /// the `#[subscribe]` meters hub only, never the cross-domain bus.
    pub fn publish_meter_frame(&self, frame: MeterFrame) {
        self.meters_hub.publish(frame);
    }

    /// Wire subscribers on the meters stream. The meter poll skips all
    /// work (no peak reads, no publish) while this is zero.
    pub fn meters_subscriber_count(&self) -> usize {
        self.meters_hub.subscriber_count()
    }

    // ── Transport state ──────────────────────────────────────────

    pub fn subscribe_transport_state(&self) -> broadcast::Receiver<TransportEvent> {
        self.transport_state_tx.subscribe()
    }

    pub fn publish_transport_state(&self, event: TransportEvent) {
        self.transport_hub
            .publish(TransportStreamEvent::State(event.clone()));
        self.bus_hub
            .publish(DawEvent::TransportState(event.clone()));
        let _ = self.transport_state_tx.send(event);
    }

    pub fn transport_state_subscriber_count(&self) -> usize {
        self.transport_state_tx.receiver_count()
            + self.transport_hub.subscriber_count()
            + self.bus_hub.subscriber_count()
    }

    // ── Position ─────────────────────────────────────────────────

    pub fn subscribe_position(&self) -> broadcast::Receiver<PositionTick> {
        self.position_tx.subscribe()
    }

    pub fn publish_position(&self, tick: PositionTick) {
        self.transport_hub
            .publish(TransportStreamEvent::Position(tick.clone()));
        self.bus_hub
            .publish(DawEvent::TransportPosition(tick.clone()));
        let _ = self.position_tx.send(tick);
    }

    pub fn position_subscriber_count(&self) -> usize {
        self.position_tx.receiver_count()
            + self.transport_hub.subscriber_count()
            + self.bus_hub.subscriber_count()
    }

    // ── Markers ──────────────────────────────────────────────────

    pub fn subscribe_markers(&self) -> broadcast::Receiver<MarkerStreamEvent> {
        self.markers_tx.subscribe()
    }

    pub fn publish_marker(&self, event: MarkerStreamEvent) {
        self.bus_hub.publish(DawEvent::Marker(event.clone()));
        self.markers_hub.publish(event.clone());
        let _ = self.markers_tx.send(event);
    }

    pub fn markers_subscriber_count(&self) -> usize {
        self.markers_tx.receiver_count()
            + self.markers_hub.subscriber_count()
            + self.bus_hub.subscriber_count()
    }

    // ── Regions ──────────────────────────────────────────────────

    pub fn subscribe_regions(&self) -> broadcast::Receiver<RegionStreamEvent> {
        self.regions_tx.subscribe()
    }

    pub fn publish_region(&self, event: RegionStreamEvent) {
        self.bus_hub.publish(DawEvent::Region(event.clone()));
        self.regions_hub.publish(event.clone());
        let _ = self.regions_tx.send(event);
    }

    pub fn regions_subscriber_count(&self) -> usize {
        self.regions_tx.receiver_count()
            + self.regions_hub.subscriber_count()
            + self.bus_hub.subscriber_count()
    }

    // ── Tracks ───────────────────────────────────────────────────

    pub fn subscribe_tracks(&self) -> broadcast::Receiver<TrackStreamEvent> {
        self.tracks_tx.subscribe()
    }

    pub fn publish_track(&self, event: TrackStreamEvent) {
        self.bus_hub.publish(DawEvent::Track(event.clone()));
        self.tracks_hub.publish(event.clone());
        let _ = self.tracks_tx.send(event);
    }

    /// Put an FX event on the bus.
    ///
    /// FX has no per-domain stream service of its own — the in-process
    /// `broadcast` in `fx_stream` is how extensions consume it — so this
    /// only feeds the cross-domain bus. Without it the events were built
    /// correctly and went nowhere, which is what made `EventBus` deliver
    /// nine of its eleven domains under REAPER while the standalone backend
    /// delivered all eleven.
    pub fn publish_fx(&self, event: daw_proto::fx::FxStreamEvent) {
        self.bus_hub.publish(DawEvent::Fx(event));
    }

    /// Is anyone listening for FX events on the bus?
    ///
    /// The poll sites gate on their own broadcaster's receiver count; this
    /// is the other half of that question, so a bus subscriber alone is
    /// enough to keep the poller running.
    pub fn fx_subscriber_count(&self) -> usize {
        self.bus_hub.subscriber_count()
    }

    /// Put a routing event on the bus. Same shape, same reason.
    pub fn publish_routing(&self, event: daw_proto::RoutingEvent) {
        self.bus_hub.publish(DawEvent::Routing(event));
    }

    pub fn routing_subscriber_count(&self) -> usize {
        self.bus_hub.subscriber_count()
    }

    /// Put an item event on the cross-domain bus.
    ///
    /// Items had a broadcast channel of their own and nothing else, so
    /// `DawEvent::Item` was a variant nobody ever published — a client
    /// on the bus saw tracks move and never saw an item move, which
    /// reads as the item stream being broken rather than absent.
    pub fn publish_item(&self, event: daw_proto::item::ItemEvent) {
        self.bus_hub.publish(DawEvent::Item(event));
    }

    pub fn items_subscriber_count(&self) -> usize {
        self.bus_hub.subscriber_count()
    }

    /// Put a take event on the bus. Same shape, same reason.
    pub fn publish_take(&self, event: daw_proto::item::TakeEvent) {
        self.bus_hub.publish(DawEvent::Take(event));
    }

    pub fn takes_subscriber_count(&self) -> usize {
        self.bus_hub.subscriber_count()
    }

    pub fn tracks_subscriber_count(&self) -> usize {
        self.tracks_tx.receiver_count()
            + self.tracks_hub.subscriber_count()
            + self.bus_hub.subscriber_count()
    }

    // ── Tempo map ────────────────────────────────────────────────

    pub fn subscribe_tempo_map(&self) -> broadcast::Receiver<TempoMapStreamEvent> {
        self.tempo_map_tx.subscribe()
    }

    pub fn publish_tempo_map(&self, event: TempoMapStreamEvent) {
        self.bus_hub.publish(DawEvent::TempoMap(event.clone()));
        self.tempo_map_hub.publish(event.clone());
        let _ = self.tempo_map_tx.send(event);
    }

    pub fn tempo_map_subscriber_count(&self) -> usize {
        self.tempo_map_tx.receiver_count()
            + self.tempo_map_hub.subscriber_count()
            + self.bus_hub.subscriber_count()
    }

    // ── Projects ─────────────────────────────────────────────────

    pub fn subscribe_projects(&self) -> broadcast::Receiver<ProjectStreamEvent> {
        self.projects_tx.subscribe()
    }

    pub fn publish_project(&self, event: ProjectStreamEvent) {
        self.bus_hub.publish(DawEvent::Project(event.clone()));
        let _ = self.projects_tx.send(event);
    }

    pub fn projects_subscriber_count(&self) -> usize {
        self.projects_tx.receiver_count() + self.bus_hub.subscriber_count()
    }
}

// ── Global accessor ──────────────────────────────────────────────

static HUB: OnceLock<DawEventHub> = OnceLock::new();

/// Get the process-wide hub. First call initializes it. Subsequent
/// calls return the same instance.
///
/// Safe to call from any thread; the hub is read-only after init
/// and broadcast::Sender is `Send + Sync`.
pub fn hub() -> &'static DawEventHub {
    HUB.get_or_init(DawEventHub::new)
}

/// Explicit initialization hook. Optional — `hub()` lazy-inits.
/// Provided so bootstrap code can ensure the hub exists before any
/// subscriber binds, parallel to the existing
/// `init_item_broadcaster` / `init_tempo_map_broadcaster` shape.
pub fn init_event_hub() {
    let _ = hub();
}
