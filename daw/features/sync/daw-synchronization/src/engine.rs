//! Sync engine — orchestrates local event collection, remote forwarding, and event application.
//!
//! The engine subscribes to all local DAW change streams, wraps them in [`SyncEvent`]
//! envelopes, and forwards them to connected peers. It also receives remote events
//! and applies them via the [`apply`] module, with echo suppression.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::{SyncConfig, SyncDomain, SyncEvent, SyncPeer, SyncSession, SyncStatus};
use daw::rpc::Daw;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::apply;
use crate::drift::DriftCorrector;
use crate::subscriptions::{self, ProjectSubscriptions};
use crate::suppression::SuppressionSet;

/// The sync engine. Owns the session state, event routing, and suppression logic.
pub struct SynchronizationEngine {
    /// The local DAW connection used for subscribing and mutating.
    daw: Daw,
    /// This peer's identity within the sync session.
    session: SyncSession,
    /// Current configuration (what domains to sync).
    config: SyncConfig,
    /// Monotonically increasing sequence number for outgoing events.
    sequence: Arc<AtomicU64>,
    /// Broadcast channel for distributing sync events to subscribers.
    event_tx: broadcast::Sender<SyncEvent>,
    /// Echo suppression set.
    suppression: Arc<tokio::sync::Mutex<SuppressionSet>>,
    /// Current sync status.
    status: tokio::sync::Mutex<SyncStatus>,
    /// Connected peers.
    peers: tokio::sync::Mutex<Vec<SyncPeer>>,
    /// Active per-project subscriptions.
    project_subs: Arc<tokio::sync::Mutex<Vec<ProjectSubscriptions>>>,
    /// Cancellation token for the project watcher task.
    project_watcher: tokio::sync::Mutex<Option<tokio_util::sync::CancellationToken>>,
    /// Set of connected peer IDs (including self). Master = lexicographically lowest.
    connected_peer_ids: tokio::sync::Mutex<Vec<String>>,
    /// Whether this peer is currently the transport master.
    is_master: Arc<AtomicBool>,
    /// Whether peer sync is enabled. When false, remote events are ignored
    /// and heartbeats are not sent. Toggled via the "Toggle Peer Sync" action.
    sync_enabled: Arc<AtomicBool>,
    /// Whether Ableton Link is handling continuous sync (tempo + phase).
    /// When true, the heartbeat and drift corrector are disabled — Link handles it.
    link_active: Arc<AtomicBool>,
    /// Drift corrector for follower transport sync (used when Link is not active).
    drift_corrector: tokio::sync::Mutex<DriftCorrector>,
}

impl SynchronizationEngine {
    /// Create a new sync engine.
    ///
    /// The engine is created in `Disconnected` state. Call [`start`] to
    /// subscribe to all DAW streams and begin syncing.
    pub fn new(daw: Daw, session: SyncSession, config: SyncConfig) -> Self {
        let (event_tx, _) = broadcast::channel(4096);
        let local_peer_id = session.peer_id.clone();

        Self {
            daw,
            session,
            config,
            sequence: Arc::new(AtomicU64::new(0)),
            event_tx,
            suppression: Arc::new(tokio::sync::Mutex::new(SuppressionSet::new())),
            status: tokio::sync::Mutex::new(SyncStatus::Disconnected),
            peers: tokio::sync::Mutex::new(Vec::new()),
            project_subs: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            project_watcher: tokio::sync::Mutex::new(None),
            connected_peer_ids: tokio::sync::Mutex::new(vec![local_peer_id]),
            is_master: Arc::new(AtomicBool::new(true)), // master until peers join
            sync_enabled: Arc::new(AtomicBool::new(true)), // enabled by default
            link_active: Arc::new(AtomicBool::new(false)),
            drift_corrector: tokio::sync::Mutex::new(DriftCorrector::new()),
        }
    }

    /// Start the sync engine — subscribe to all open projects and watch for new ones.
    ///
    /// This subscribes to every enabled domain stream for each currently open project,
    /// and starts a project watcher that auto-subscribes new projects as they open.
    pub async fn start(&self) -> Result<(), daw::rpc::Error> {
        *self.status.lock().await = SyncStatus::Connecting;

        // Subscribe to all currently open projects
        let projects = self.daw.projects().await?;
        for project in &projects {
            let info = project.info().await?;
            match subscriptions::subscribe_project(
                &self.daw,
                project,
                info.guid.clone(),
                self.session.peer_id.clone(),
                self.sequence.clone(),
                self.event_tx.clone(),
                Arc::clone(&self.suppression),
                &self.config,
            )
            .await
            {
                Ok(sub) => {
                    self.project_subs.lock().await.push(sub);
                }
                Err(e) => {
                    warn!("Failed to subscribe to project {}: {e}", info.guid);
                }
            }
        }

        // Start project watcher for auto-subscribing new projects
        let watcher_cancel = subscriptions::watch_projects(
            self.daw.clone(),
            self.session.peer_id.clone(),
            self.sequence.clone(),
            self.event_tx.clone(),
            Arc::clone(&self.suppression),
            self.config.clone(),
            self.project_subs.clone(),
        );
        *self.project_watcher.lock().await = Some(watcher_cancel);

        // Start the suppression GC ticker
        let suppression = Arc::clone(&self.suppression);
        tokio::task::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                suppression.lock().await.gc();
            }
        });

        *self.status.lock().await = SyncStatus::Connected;
        info!(
            "Sync engine started — subscribed to {} project(s)",
            projects.len()
        );

        Ok(())
    }

    /// Stop the sync engine — cancel all subscriptions.
    pub async fn stop(&self) {
        // Cancel project watcher
        if let Some(cancel) = self.project_watcher.lock().await.take() {
            cancel.cancel();
        }

        // Cancel all project subscriptions
        let mut subs = self.project_subs.lock().await;
        for sub in subs.drain(..) {
            sub.cancel();
        }

        *self.status.lock().await = SyncStatus::Disconnected;
        info!("Sync engine stopped");
    }

    /// Get the current sync status.
    pub async fn status(&self) -> SyncStatus {
        self.status.lock().await.clone()
    }

    /// Get the list of connected peers.
    pub async fn peers(&self) -> Vec<SyncPeer> {
        self.peers.lock().await.clone()
    }

    /// Get the local peer ID.
    pub fn peer_id(&self) -> &str {
        &self.session.peer_id
    }

    /// Update the sync configuration.
    pub async fn update_config(&self, config: SyncConfig) {
        info!("Sync config updated");
        let _ = config; // TODO: restart subscriptions with new config
    }

    /// Get the next sequence number for an outgoing event.
    fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::Relaxed)
    }

    /// Wrap a domain event as a SyncEvent with this peer's identity.
    pub fn wrap_event(&self, project_guid: String, domain: SyncDomain) -> SyncEvent {
        SyncEvent {
            origin_peer: self.session.peer_id.clone(),
            sequence: self.next_sequence(),
            project_guid,
            domain,
            created_at_ms: SyncEvent::now_ms(),
        }
    }

    /// Broadcast a locally-detected change to all subscribers (remote peers).
    ///
    /// Returns `false` if the event was suppressed (echo from a remote apply).
    pub async fn broadcast_local(&self, event: SyncEvent) -> bool {
        // Check if this event matches a recent suppression entry
        let suppressed = {
            let suppression = self.suppression.lock().await;
            is_event_suppressed(&suppression, &event)
        };

        if suppressed {
            debug!(
                "Suppressed echo for {:?} (origin: {})",
                std::mem::discriminant(&event.domain),
                event.origin_peer
            );
            return false;
        }

        // Not suppressed — broadcast to subscribers
        match self.event_tx.send(event) {
            Ok(n) => {
                debug!("Broadcast sync event to {n} subscribers");
                true
            }
            Err(_) => {
                // No active subscribers — that's fine, events are fire-and-forget
                true
            }
        }
    }

    /// Apply a remote sync event to the local DAW.
    ///
    /// Skips events from self (origin_peer == local peer_id).
    /// For transport events, only accepts events from the master peer to prevent
    /// echo loops and oscillation (followers echoing each other's state transitions).
    pub async fn apply_remote(&self, event: &SyncEvent) {
        // Skip everything when peer sync is disabled
        if !self.sync_enabled.load(Ordering::Relaxed) {
            return;
        }

        // Skip our own events
        if event.origin_peer == self.session.peer_id {
            return;
        }

        // Check if this domain is enabled in our config
        if !is_domain_enabled(&self.config, &event.domain) {
            debug!("Skipping event for disabled domain");
            return;
        }

        // For transport events, only accept from the master peer.
        // Without this, followers echo each other's transport state changes
        // (stop→play transitions bounce between peers causing oscillation).
        if matches!(&event.domain, SyncDomain::Transport(_)) {
            let peers = self.connected_peer_ids.lock().await;
            let master_id = peers.iter().min().cloned();
            drop(peers);
            if let Some(master) = master_id
                && event.origin_peer != master
            {
                debug!(
                    "Ignoring transport event from non-master {} (master={})",
                    event.origin_peer, master
                );
                return;
            }
        }

        // Apply with suppression and drift correction
        let mut suppression = self.suppression.lock().await;
        let mut drift = self.drift_corrector.lock().await;
        let link_active = self.link_active.load(Ordering::Relaxed);
        let is_master = self.is_master();
        apply::apply_remote_event(
            &self.daw,
            &event.project_guid,
            &event.domain,
            &mut suppression,
            &mut drift,
            link_active,
            is_master,
            event.created_at_ms,
        )
        .await;
    }

    /// Subscribe to outgoing sync events (for forwarding to remote peers).
    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.event_tx.subscribe()
    }

    /// Get the DAW connection (for full-state snapshot requests).
    pub fn daw(&self) -> &Daw {
        &self.daw
    }

    /// Get a reference to the suppression set (for outbound echo filtering).
    pub fn suppression(&self) -> &Arc<tokio::sync::Mutex<SuppressionSet>> {
        &self.suppression
    }

    /// Notify the engine that a mesh peer connected.
    ///
    /// Recalculates master status (lowest peer_id = master).
    /// Returns `true` if this peer was demoted from master to follower.
    pub async fn add_peer(&self, peer_id: String) -> bool {
        let was_master = self.is_master.load(Ordering::Relaxed);
        let mut peers = self.connected_peer_ids.lock().await;
        if !peers.contains(&peer_id) {
            peers.push(peer_id);
            peers.sort();
        }
        let master = peers.first().map(|s| s.as_str()) == Some(&self.session.peer_id);
        self.is_master.store(master, Ordering::Relaxed);
        info!(
            "Peer set updated ({} peers), master={}",
            peers.len(),
            if master {
                "self"
            } else {
                peers.first().map(|s| s.as_str()).unwrap_or("?")
            }
        );
        let demoted = was_master && !master;
        if demoted {
            info!("Demoted to follower — resetting drift corrector");
            self.drift_corrector.lock().await.reset();
        }
        demoted
    }

    /// Notify the engine that a mesh peer disconnected.
    ///
    /// Returns `true` if this peer was promoted from follower to master
    /// (i.e., was not master before but is now). This lets callers handle
    /// failover (e.g., resetting playrate to 1.0 during live playback).
    pub async fn remove_peer(&self, peer_id: &str) -> bool {
        let was_master = self.is_master.load(Ordering::Relaxed);
        let mut peers = self.connected_peer_ids.lock().await;
        peers.retain(|p| p != peer_id);
        let master = peers.first().map(|s| s.as_str()) == Some(&self.session.peer_id);
        self.is_master.store(master, Ordering::Relaxed);
        info!(
            "Peer set updated ({} peers), master={}",
            peers.len(),
            if master {
                "self"
            } else {
                peers.first().map(|s| s.as_str()).unwrap_or("?")
            }
        );
        let promoted = !was_master && master;
        if promoted {
            info!("Promoted to master (failover)");
        }
        promoted
    }

    /// Reset the drift corrector and return access to it for failover handling.
    ///
    /// Called when a follower is promoted to master during playback — the drift
    /// corrector's stale history and any non-1.0 playrate must be cleared so
    /// the new master plays at normal speed.
    pub async fn reset_drift_corrector(&self) {
        self.drift_corrector.lock().await.reset();
    }

    /// Get an atomic flag indicating whether this peer is the transport master.
    ///
    /// The heartbeat task reads this without async to decide whether to send position updates.
    pub fn is_master_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.is_master)
    }

    /// Check if this peer is currently the transport master.
    pub fn is_master(&self) -> bool {
        self.is_master.load(Ordering::Relaxed)
    }

    /// Set whether Ableton Link is handling continuous sync.
    ///
    /// When active, the heartbeat task stops sending position updates and
    /// the drift corrector in apply_transport is bypassed — Link handles
    /// tempo and phase alignment instead.
    pub fn set_link_active(&self, active: bool) {
        self.link_active.store(active, Ordering::Relaxed);
        info!("Link active: {active}");
    }

    /// Get an atomic flag indicating whether Link is active.
    pub fn link_active_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.link_active)
    }

    /// Check if Link is currently handling sync.
    pub fn is_link_active(&self) -> bool {
        self.link_active.load(Ordering::Relaxed)
    }

    /// Get an atomic flag indicating whether peer sync is enabled.
    pub fn sync_enabled_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.sync_enabled)
    }

    /// Set whether peer sync is enabled.
    pub fn set_sync_enabled(&self, enabled: bool) {
        self.sync_enabled.store(enabled, Ordering::Relaxed);
        info!("Peer sync enabled: {enabled}");
    }

    /// Check if peer sync is currently enabled.
    pub fn is_sync_enabled(&self) -> bool {
        self.sync_enabled.load(Ordering::Relaxed)
    }

    /// Number of peers in the connected set (including self).
    pub async fn peer_count(&self) -> usize {
        self.connected_peer_ids.lock().await.len()
    }

    /// Get the sequence counter (for the heartbeat task to generate events).
    pub fn sequence(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.sequence)
    }

    /// Get the broadcast sender (for the heartbeat task to emit events).
    pub fn event_sender(&self) -> broadcast::Sender<SyncEvent> {
        self.event_tx.clone()
    }
}

/// Check if a domain is enabled in the sync config.
fn is_domain_enabled(config: &SyncConfig, domain: &SyncDomain) -> bool {
    match domain {
        SyncDomain::Transport(_) => config.transport,
        SyncDomain::Track(_) => config.tracks,
        SyncDomain::Fx(_) => config.fx,
        SyncDomain::Item(_) => config.items,
        SyncDomain::Take(_) => config.items, // Takes follow items config
        SyncDomain::Routing(_) => config.routing,
        SyncDomain::TempoMap(_) => config.tempo_map,
        SyncDomain::Marker(_) => config.markers,
        SyncDomain::Region(_) => config.regions,
        SyncDomain::Project(_) => true, // Project events always enabled
    }
}

/// Check if a sync event should be suppressed based on the suppression set.
pub fn is_event_suppressed(suppression: &SuppressionSet, event: &SyncEvent) -> bool {
    use crate::suppression::SuppressionKey;
    use daw::service::{FxEvent, ItemEvent, RoutingEvent, TakeEvent, TrackEvent};

    match &event.domain {
        SyncDomain::Transport(_) => {
            suppression.is_suppressed(&SuppressionKey::transport(&event.project_guid))
        }
        SyncDomain::Track(te) => match te {
            // For f64-valued fields prefer value-aware matching so a
            // genuine local change to a *different* value still broadcasts
            // even if a recent apply suppressed the key.
            TrackEvent::VolumeChanged { guid, volume } => {
                suppression.is_suppressed_value(&SuppressionKey::track(guid, "volume"), *volume)
            }
            TrackEvent::PanChanged { guid, pan } => {
                suppression.is_suppressed_value(&SuppressionKey::track(guid, "pan"), *pan)
            }
            // Boolean / discrete fields fall back to key-only matching.
            TrackEvent::AutomationModeChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "automode"))
            }
            TrackEvent::InputMonitorChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "recmon"))
            }
            TrackEvent::PhaseInvertedChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "phase"))
            }
            TrackEvent::MuteChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "muted"))
            }
            TrackEvent::SoloChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "soloed"))
            }
            TrackEvent::ArmChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "armed"))
            }
            TrackEvent::Renamed { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "name"))
            }
            TrackEvent::ColorChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "color"))
            }
            TrackEvent::SelectionChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "selected"))
            }
            TrackEvent::TcpVisibilityChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "tcp_visible"))
            }
            TrackEvent::MixerVisibilityChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "mixer_visible"))
            }
            TrackEvent::Added(track) => {
                suppression.is_suppressed(&SuppressionKey::track(&track.guid, "added"))
            }
            TrackEvent::Removed(guid) => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "removed"))
            }
            TrackEvent::Moved { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "moved"))
            }
            TrackEvent::ParentSendChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "parentsend"))
            }
            // Never suppressed, because it is never applied — see the
            // matching arm in `apply.rs`. The FX domain carries the change
            // that caused it.
            TrackEvent::FxCountChanged { .. } | TrackEvent::RouteCountsChanged { .. } => false,
            // Never suppressed for the same reason: neither is ever
            // applied to a peer, so neither can echo back off one. See
            // the matching arms in `apply.rs`.
            TrackEvent::RecordInputChanged { .. } | TrackEvent::GroupingChanged { .. } => false,
            // Same again: a panel height is this desk's, and a lane
            // layout describes takes only this machine has.
            TrackEvent::HeightChanged { .. } | TrackEvent::LanesChanged { .. } => false,
            // This one IS applied, so it CAN echo, and has to be
            // suppressed like every other shared field.
            TrackEvent::FolderDepthChanged { guid, .. } => {
                suppression.is_suppressed(&SuppressionKey::track(guid, "folderdepth"))
            }
        },
        SyncDomain::Fx(fe) => {
            // Apply side (apply.rs) suppresses chain-level FX events with
            // `SuppressionKey::fx_param(chain_key, fx_guid, 0)` and
            // parameter changes with the real param_index. Match both
            // shapes so locally-detected events from `Effects::list`
            // diffs don't echo back to peers.
            let (context, fx_guid, param_index) = match fe {
                FxEvent::ParameterChanged {
                    context,
                    fx_guid,
                    param_index,
                    ..
                } => (context, fx_guid.as_str(), *param_index),
                FxEvent::Added { context, fx } => (context, fx.guid.as_str(), 0),
                FxEvent::Removed { context, fx_guid }
                | FxEvent::EnabledChanged {
                    context, fx_guid, ..
                }
                | FxEvent::Moved {
                    context, fx_guid, ..
                }
                | FxEvent::PresetChanged {
                    context, fx_guid, ..
                }
                | FxEvent::WindowChanged {
                    context, fx_guid, ..
                } => (context, fx_guid.as_str(), 0),
                _ => return false,
            };
            let context_key = format!("{context:?}");
            suppression.is_suppressed(&SuppressionKey::fx_param(
                &context_key,
                fx_guid,
                param_index,
            ))
        }
        SyncDomain::Routing(re) => {
            // Apply side uses `SuppressionKey::routing(track, "{field}:{index}")`
            // — match exactly.
            let (track, field) = match re {
                RoutingEvent::VolumeChanged {
                    source_track_guid,
                    route_index,
                    ..
                } => (source_track_guid.as_str(), format!("volume:{route_index}")),
                RoutingEvent::PanChanged {
                    source_track_guid,
                    route_index,
                    ..
                } => (source_track_guid.as_str(), format!("pan:{route_index}")),
                RoutingEvent::MuteChanged {
                    source_track_guid,
                    route_index,
                    ..
                } => (source_track_guid.as_str(), format!("mute:{route_index}")),
                RoutingEvent::RouteCreated {
                    source_track_guid,
                    route,
                    ..
                } => (
                    source_track_guid.as_str(),
                    format!("created:{}", route.index),
                ),
                RoutingEvent::RouteDeleted {
                    source_track_guid,
                    route_index,
                    ..
                } => (source_track_guid.as_str(), format!("deleted:{route_index}")),
                RoutingEvent::ParentSendChanged { track_guid, .. } => {
                    (track_guid.as_str(), "parent_send".to_string())
                }
            };
            suppression.is_suppressed(&SuppressionKey::routing(track, &field))
        }
        SyncDomain::Take(te) => {
            // Apply side uses `SuppressionKey::item(item_guid, "take_{field}")`.
            let (item_guid, field) = match te {
                TakeEvent::Created { item_guid, .. } => (item_guid.as_str(), "take_created"),
                TakeEvent::Deleted { item_guid, .. } => (item_guid.as_str(), "take_deleted"),
                TakeEvent::NameChanged { item_guid, .. } => (item_guid.as_str(), "take_name"),
                TakeEvent::PitchChanged { item_guid, .. } => (item_guid.as_str(), "take_pitch"),
                TakeEvent::PlayRateChanged { item_guid, .. } => {
                    (item_guid.as_str(), "take_play_rate")
                }
                TakeEvent::VolumeChanged { item_guid, .. } => (item_guid.as_str(), "take_volume"),
                TakeEvent::SourceChanged { item_guid, .. } => (item_guid.as_str(), "take_source"),
                TakeEvent::Changed { item_guid, .. } => (item_guid.as_str(), "take_changed"),
            };
            suppression.is_suppressed(&SuppressionKey::item(item_guid, field))
        }
        SyncDomain::Item(ie) => {
            let (guid, field) = match ie {
                ItemEvent::Created { item, .. } => (item.guid.as_str(), "created"),
                ItemEvent::Deleted { item_guid, .. } => (item_guid.as_str(), "deleted"),
                ItemEvent::PositionChanged { item_guid, .. } => (item_guid.as_str(), "position"),
                ItemEvent::LengthChanged { item_guid, .. } => (item_guid.as_str(), "length"),
                ItemEvent::MovedToTrack { item_guid, .. } => (item_guid.as_str(), "moved_to_track"),
                ItemEvent::MuteChanged { item_guid, .. } => (item_guid.as_str(), "muted"),
                ItemEvent::SelectionChanged { item_guid, .. } => (item_guid.as_str(), "selected"),
                ItemEvent::VolumeChanged { item_guid, .. } => (item_guid.as_str(), "volume"),
                ItemEvent::ActiveTakeChanged { item_guid, .. } => {
                    (item_guid.as_str(), "active_take")
                }
                ItemEvent::Changed { item_guid, .. } => (item_guid.as_str(), "changed"),
            };
            suppression.is_suppressed(&SuppressionKey::item(guid, field))
        }
        SyncDomain::TempoMap(_) => {
            suppression.is_suppressed(&SuppressionKey::tempo_map(&event.project_guid))
        }
        SyncDomain::Marker(me) => {
            use daw::service::MarkerEvent;
            let id = match me {
                MarkerEvent::Added(m) => m.id,
                MarkerEvent::Removed(id) => Some(*id),
                MarkerEvent::Changed(m) => m.id,
                MarkerEvent::MarkersChanged(_) => None,
                // The number this peer knew is the one its suppression
                // was filed under, so that is the one to ask about.
                MarkerEvent::Renumbered { from, .. } => Some(*from),
            };
            id.is_some_and(|id| {
                suppression.is_suppressed(&SuppressionKey::marker(&event.project_guid, id))
            })
        }
        SyncDomain::Region(re) => {
            use daw::service::RegionEvent;
            let id = match re {
                RegionEvent::Added(r) => r.id,
                RegionEvent::Removed(id) => Some(*id),
                RegionEvent::Changed(r) => r.id,
                RegionEvent::RegionsChanged(_) => None,
                RegionEvent::Renumbered { from, .. } => Some(*from),
            };
            id.is_some_and(|id| {
                suppression.is_suppressed(&SuppressionKey::region(&event.project_guid, id))
            })
        }
        _ => false,
    }
}
