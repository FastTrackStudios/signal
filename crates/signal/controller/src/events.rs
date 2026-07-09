//! Signal event types for reactive UI updates.
//!
//! `SignalEvent` is emitted when state changes occur in the signal domain.
//! The UI subscribes to these events via `SignalController::subscribe()`.

use signal_proto::module_type::ModuleType;
use signal_proto::rig::RigSceneId;
use signal_proto::{BlockType, ModulePresetId, ModuleSnapshotId, PresetId, SnapshotId};

/// Events emitted by the signal controller for reactive UI updates.
#[derive(Debug, Clone)]
pub enum SignalEvent {
    /// A block's parameter state changed.
    BlockChanged { block_type: BlockType },
    /// A preset was loaded.
    PresetLoaded {
        block_type: BlockType,
        preset_id: PresetId,
        snapshot_id: SnapshotId,
    },
    /// A module preset was loaded.
    ModulePresetLoaded {
        module_preset_id: ModulePresetId,
        snapshot_id: ModuleSnapshotId,
    },
    /// A scene transition completed.
    SceneChanged { scene_id: Option<RigSceneId> },
    /// A snapshot was applied to a slot.
    SnapshotApplied {
        module_type: ModuleType,
        snapshot_id: ModuleSnapshotId,
    },
    /// A slot was disabled.
    SlotDisabled { module_type: ModuleType },
    /// A slot was enabled.
    SlotEnabled { module_type: ModuleType },
    /// Navigated to a different song.
    SongChanged { song_id: String },
    /// Navigated to a different section.
    SectionChanged { section_id: String },
    /// Morph position changed.
    MorphPositionChanged { position: f32 },
    /// A collection (preset, module, layer, etc.) was saved.
    CollectionSaved {
        entity_type: &'static str,
        id: String,
    },
    /// A collection was deleted.
    CollectionDeleted {
        entity_type: &'static str,
        id: String,
    },
    /// A profile patch was activated (resolved + optionally applied to DAW).
    PatchActivated {
        profile_id: String,
        patch_id: String,
        applied_to_daw: bool,
    },
    /// A block preset was activated directly (loaded + optionally applied to DAW).
    PresetActivated {
        block_type: BlockType,
        preset_id: String,
        snapshot_id: String,
        applied_to_daw: bool,
    },
}

/// Subscription handle for signal events.
///
/// Uses `tokio::sync::broadcast` for multi-consumer event delivery.
pub struct EventBus {
    sender: tokio::sync::broadcast::Sender<SignalEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(capacity);
        Self { sender }
    }

    /// Emit an event to all subscribers.
    pub fn emit(&self, event: SignalEvent) {
        // Ignore send errors (no active subscribers).
        let _ = self.sender.send(event);
    }

    /// Subscribe to events. Returns a receiver that gets all future events.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SignalEvent> {
        self.sender.subscribe()
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn event_bus_subscribe_and_receive() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.emit(SignalEvent::BlockChanged {
            block_type: BlockType::Amp,
        });

        let event = rx.recv().await.unwrap();
        assert!(matches!(event, SignalEvent::BlockChanged { .. }));
    }

    #[tokio::test]
    async fn event_bus_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.emit(SignalEvent::MorphPositionChanged { position: 0.5 });

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert!(matches!(e1, SignalEvent::MorphPositionChanged { .. }));
        assert!(matches!(e2, SignalEvent::MorphPositionChanged { .. }));
    }

    #[test]
    fn subscriber_count() {
        let bus = EventBus::new(16);
        assert_eq!(bus.subscriber_count(), 0);

        let _rx = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);
    }
}
