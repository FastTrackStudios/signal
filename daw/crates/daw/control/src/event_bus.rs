//! Cross-domain event bus handle.
//!
//! Single-subscribe surface that multiplexes every per-domain stream
//! (tracks, markers, regions, tempo map, FX, transport, projects)
//! onto one channel. The server side is an argless architect
//! `#[subscribe]` stream that carries *everything*; the [`BusFilter`]
//! is applied **client-side** here, so disabled domains are dropped
//! before they reach the consumer's `Rx`. Use this when a consumer
//! wants several domains at once (OSC/MIDI bridges, inspectors, web
//! UIs). Use the per-domain `Project::tracks()` / `markers()` / etc.
//! when only one domain is interesting.

use std::sync::Arc;

use crate::{DawClients, Result};
use daw_proto::event_bus::{BusFilter, DawEvent};
use daw_proto::transport::TransportEvent;

/// Handle for subscribing to the cross-domain event bus. Cheap to
/// clone — wraps a shared `Arc<DawClients>`.
#[derive(Clone)]
pub struct Events {
    clients: Arc<DawClients>,
}

impl Events {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    /// Subscribe with the given filter. Returns an
    /// [`crate::EventStream`] that yields `DawEvent`s for every
    /// enabled domain until the subscription ends (drop it to
    /// unsubscribe). The filter is applied client-side — the wire
    /// carries every domain.
    pub async fn subscribe(&self, filter: BusFilter) -> Result<crate::EventStream<DawEvent>> {
        let stream = self.clients.event_bus_stream.clone();
        let (raw_tx, raw_rx) = vox::channel();
        let enabled = filter.any();
        let mut events = crate::EventStream::spawn(
            async move {
                if enabled {
                    let _ = stream.events(raw_tx).await;
                }
                // `!enabled`: never subscribe — raw_tx drops here and
                // the stream reads as ended (old server behavior for
                // an all-off filter).
            },
            raw_rx,
            Box::new(move |ev| admits(&filter, ev)),
        );
        // Wait until the hub actually has the sink.
        //
        // `EventStream::spawn` parks the call on a task and returns, so
        // without this `subscribe` returns before the server has
        // attached — and `PubSub` has no replay, so everything
        // published in that gap went to nobody. A consumer that
        // subscribed and immediately caused an event could wait forever
        // for it, which is what made
        // `daw-csi::in_process fader_gesture_reaches_engine_and_echoes_on_bus`
        // red and had to be worked around by polling the hub's
        // subscriber count — correct for an in-process test holding the
        // backend, and no help at all to a remote client.
        //
        // The backend puts `Attached` at the FRONT of the new
        // subscriber's mailbox as part of the attach itself, so by the
        // time it arrives nothing can have been missed. Swallowed here:
        // `admits` also rejects it, so it can never reach a consumer by
        // either route.
        if enabled {
            // A channel that ends while we wait is a backend going away,
            // not a failure to subscribe: the stream reads as ended,
            // which is what the caller would have seen anyway.
            let _ = events
                .await_marker(|ev| matches!(ev, DawEvent::Attached))
                .await;
        }
        Ok(events)
    }

    /// Convenience: subscribe with `BusFilter::all()`. Use this when
    /// the consumer wants every event the bridge can emit — typical
    /// for OSC/MIDI translation layers.
    pub async fn subscribe_all(&self) -> Result<crate::EventStream<DawEvent>> {
        self.subscribe(BusFilter::all()).await
    }
}

/// Client-side [`BusFilter`] application — one event, keep or drop.
/// `Project` events pass whenever the domain is enabled regardless of
/// `project_guid` (subscribers need them to know when to swap their
/// scoped guid — same rule the server applied pre-port).
fn admits(filter: &BusFilter, ev: &DawEvent) -> bool {
    match ev {
        DawEvent::Track(e) => filter.tracks && !filter.project_rejects(&e.project_guid),
        DawEvent::Fx(e) => filter.fx && !filter.project_rejects(&e.project_guid),
        DawEvent::Marker(e) => filter.markers && !filter.project_rejects(&e.project_guid),
        DawEvent::Region(e) => filter.regions && !filter.project_rejects(&e.project_guid),
        DawEvent::TempoMap(e) => filter.tempo_map && !filter.project_rejects(&e.project_guid),
        DawEvent::TransportState(e) => {
            filter.transport_state && !filter.project_rejects(transport_guid(e))
        }
        DawEvent::TransportPosition(t) => {
            filter.transport_position && !filter.project_rejects(&t.project_guid)
        }
        // Never a consumer's business: `subscribe` consumes it.
        DawEvent::Attached => false,
        DawEvent::Project(_) => filter.projects,
        DawEvent::Item(e) => filter.items && !filter.project_rejects(e.project_guid()),
        DawEvent::Take(e) => filter.takes && !filter.project_rejects(e.project_guid()),
        DawEvent::Routing(e) => filter.routing && !filter.project_rejects(routing_guid(e)),
    }
}

/// Extract the project guid a [`RoutingEvent`] applies to.
fn routing_guid(e: &daw_proto::routing::RoutingEvent) -> &str {
    use daw_proto::routing::RoutingEvent::*;
    match e {
        RouteCreated { project_guid, .. }
        | RouteDeleted { project_guid, .. }
        | VolumeChanged { project_guid, .. }
        | PanChanged { project_guid, .. }
        | MuteChanged { project_guid, .. }
        | ParentSendChanged { project_guid, .. } => project_guid,
    }
}

/// Every `TransportEvent` variant carries its project guid — extract
/// it for filter checks.
fn transport_guid(e: &TransportEvent) -> &str {
    match e {
        TransportEvent::PlayStateChanged { project_guid, .. }
        | TransportEvent::RecordModeChanged { project_guid, .. }
        | TransportEvent::LoopingChanged { project_guid, .. }
        | TransportEvent::MetronomeChanged { project_guid, .. }
        | TransportEvent::LoopRegionChanged { project_guid, .. }
        | TransportEvent::TimeSelectionChanged { project_guid, .. }
        | TransportEvent::TempoChanged { project_guid, .. }
        | TransportEvent::PlayrateChanged { project_guid, .. }
        | TransportEvent::Snapshot { project_guid, .. } => project_guid,
    }
}

impl std::fmt::Debug for Events {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Events").finish_non_exhaustive()
    }
}
