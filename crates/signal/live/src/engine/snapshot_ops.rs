//! Snapshot operations — bridges live DAW capture with SQLite persistence.
//!
//! Coordinates [`DawBridge`] (live capture/apply) and [`DawSnapshotRepo`]
//! (storage) to provide four high-level operations:
//!
//! - [`capture_and_save_snapshot`] — capture live params → serialize → SQLite
//! - [`recall_snapshot`] — load from SQLite → deserialize → apply to DAW
//! - [`capture_and_save_preset`] — capture params + state chunks → SQLite
//! - [`recall_preset`] — load full preset → apply chunks + params to DAW

use base64::{engine::general_purpose::STANDARD, Engine as _};
use signal_storage::daw_snapshot_repo::{
    DawSnapshotRepo, StoredChunkSnapshot, StoredParamSnapshot,
};
use uuid::Uuid;

use super::daw_bridge::{DawBridge, DawSceneSnapshot, DawStateChunk};
use super::morph::DawParameterSnapshot;

// region: --- Error

/// Errors from snapshot operations.
#[derive(Debug)]
pub enum SnapshotError {
    /// JSON serialization/deserialization failure.
    Serde(String),
    /// Base64 encoding/decoding failure.
    Base64(String),
    /// Storage layer failure.
    Storage(signal_storage::StorageError),
    /// No snapshot found for the given owner.
    NotFound(String),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serde(msg) => write!(f, "serde error: {msg}"),
            Self::Base64(msg) => write!(f, "base64 error: {msg}"),
            Self::Storage(e) => write!(f, "storage error: {e}"),
            Self::NotFound(id) => write!(f, "no snapshot for owner: {id}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

impl From<signal_storage::StorageError> for SnapshotError {
    fn from(e: signal_storage::StorageError) -> Self {
        Self::Storage(e)
    }
}

// endregion: --- Error

// region: --- Conversions

fn params_to_json(params: &DawParameterSnapshot) -> Result<String, SnapshotError> {
    serde_json::to_string(params).map_err(|e| SnapshotError::Serde(e.to_string()))
}

fn json_to_params(json: &str) -> Result<DawParameterSnapshot, SnapshotError> {
    serde_json::from_str(json).map_err(|e| SnapshotError::Serde(e.to_string()))
}

fn chunk_to_stored(chunk: &DawStateChunk, owner_id: &str) -> StoredChunkSnapshot {
    StoredChunkSnapshot {
        id: Uuid::new_v4().to_string(),
        owner_id: owner_id.to_string(),
        fx_id: chunk.fx_id.clone(),
        plugin_name: chunk.plugin_name.clone(),
        chunk_data_b64: STANDARD.encode(&chunk.chunk_data),
        created_at: epoch_timestamp(),
    }
}

fn stored_to_chunk(stored: &StoredChunkSnapshot) -> Result<DawStateChunk, SnapshotError> {
    let chunk_data = STANDARD
        .decode(&stored.chunk_data_b64)
        .map_err(|e| SnapshotError::Base64(e.to_string()))?;
    Ok(DawStateChunk {
        fx_id: stored.fx_id.clone(),
        plugin_name: stored.plugin_name.clone(),
        block_type: signal_proto::BlockType::Custom,
        chunk_data,
    })
}

/// Simple epoch-second timestamp. Callers needing human-readable ISO 8601
/// can swap this for a chrono-based implementation later.
fn epoch_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

// endregion: --- Conversions

// region: --- Operations

/// Capture live DAW parameters and save to SQLite.
///
/// Returns the captured snapshot for immediate use (e.g. morphing between scenes).
pub async fn capture_and_save_snapshot(
    bridge: &dyn DawBridge,
    repo: &dyn DawSnapshotRepo,
    track_id: &str,
    owner_id: &str,
    name: &str,
) -> Result<DawSceneSnapshot, SnapshotError> {
    let params = bridge.capture_parameters(track_id);
    let params_json = params_to_json(&params)?;

    let stored = StoredParamSnapshot {
        id: Uuid::new_v4().to_string(),
        owner_id: owner_id.to_string(),
        name: name.to_string(),
        params_json,
        created_at: epoch_timestamp(),
    };
    repo.save_param_snapshot(&stored).await?;

    Ok(DawSceneSnapshot::params_only(params))
}

/// Load the most recent stored snapshot for an owner and apply it to the DAW.
///
/// Returns the deserialized snapshot for follow-up operations.
pub async fn recall_snapshot(
    bridge: &dyn DawBridge,
    repo: &dyn DawSnapshotRepo,
    track_id: &str,
    owner_id: &str,
) -> Result<DawSceneSnapshot, SnapshotError> {
    let snapshots = repo.list_param_snapshots(owner_id).await?;
    let stored = snapshots
        .last()
        .ok_or_else(|| SnapshotError::NotFound(owner_id.to_string()))?;

    let params = json_to_params(&stored.params_json)?;
    bridge.apply_parameters(track_id, &params);

    Ok(DawSceneSnapshot::params_only(params))
}

/// Capture full plugin state (params + binary chunks) and save to SQLite.
///
/// This is the heavyweight save: captures exact binary plugin state for
/// complete preset recall. Previous chunks for this owner are replaced.
pub async fn capture_and_save_preset(
    bridge: &dyn DawBridge,
    repo: &dyn DawSnapshotRepo,
    track_id: &str,
    owner_id: &str,
    name: &str,
) -> Result<DawSceneSnapshot, SnapshotError> {
    let params = bridge.capture_parameters(track_id);
    let chunks = bridge.capture_state_chunks(track_id);

    // Save params
    let params_json = params_to_json(&params)?;
    let stored_params = StoredParamSnapshot {
        id: Uuid::new_v4().to_string(),
        owner_id: owner_id.to_string(),
        name: name.to_string(),
        params_json,
        created_at: epoch_timestamp(),
    };
    repo.save_param_snapshot(&stored_params).await?;

    // Replace previous chunks for this owner, then save new ones
    repo.delete_chunk_snapshots_by_owner(owner_id).await?;
    for chunk in &chunks {
        let stored = chunk_to_stored(chunk, owner_id);
        repo.save_chunk_snapshot(&stored).await?;
    }

    Ok(DawSceneSnapshot::new(params, chunks))
}

/// Load a full preset (params + binary chunks) and apply to the DAW.
///
/// Applies state chunks first (coarse binary state), then parameter values
/// (fine-tuned on top) for the most accurate state restoration.
pub async fn recall_preset(
    bridge: &dyn DawBridge,
    repo: &dyn DawSnapshotRepo,
    track_id: &str,
    owner_id: &str,
) -> Result<DawSceneSnapshot, SnapshotError> {
    let param_snapshots = repo.list_param_snapshots(owner_id).await?;
    let stored_params = param_snapshots
        .last()
        .ok_or_else(|| SnapshotError::NotFound(owner_id.to_string()))?;
    let params = json_to_params(&stored_params.params_json)?;

    let stored_chunks = repo.list_chunk_snapshots(owner_id).await?;
    let mut chunks = Vec::with_capacity(stored_chunks.len());
    for sc in &stored_chunks {
        chunks.push(stored_to_chunk(sc)?);
    }

    // Apply chunks first (full binary state), then params (fine-tune)
    bridge.apply_state_chunks(track_id, &chunks);
    bridge.apply_parameters(track_id, &params);

    Ok(DawSceneSnapshot::new(params, chunks))
}

// endregion: --- Operations

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::daw_bridge::MockDawBridge;
    use crate::engine::morph::DawParamValue;
    use signal_storage::daw_snapshot_repo::DawSnapshotRepoLive;
    use signal_storage::Database;

    async fn test_repo() -> DawSnapshotRepoLive {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let repo = DawSnapshotRepoLive::new(db);
        repo.init_schema().await.unwrap();
        repo
    }

    fn test_bridge_with_params() -> MockDawBridge {
        let bridge = MockDawBridge::new();
        let snap = DawParameterSnapshot::new(vec![
            DawParamValue {
                fx_id: "fx-1".into(),
                param_index: 0,
                param_name: "Gain".into(),
                value: 0.75,
            },
            DawParamValue {
                fx_id: "fx-1".into(),
                param_index: 1,
                param_name: "Volume".into(),
                value: 0.5,
            },
        ]);
        bridge.set_snapshot("track-1", snap);
        bridge
    }

    #[tokio::test]
    async fn capture_and_recall_snapshot() {
        let repo = test_repo().await;
        let bridge = test_bridge_with_params();

        // Capture
        let captured = capture_and_save_snapshot(&bridge, &repo, "track-1", "rig-1", "Clean")
            .await
            .unwrap();
        assert_eq!(captured.params.params.len(), 2);
        assert!(captured.chunks.is_empty());

        // Verify persisted
        let stored = repo.list_param_snapshots("rig-1").await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].name, "Clean");

        // Clear bridge state to prove recall restores from DB
        bridge.set_snapshot("track-1", DawParameterSnapshot::default());

        // Recall
        let recalled = recall_snapshot(&bridge, &repo, "track-1", "rig-1")
            .await
            .unwrap();
        assert_eq!(recalled.params.params.len(), 2);
        assert!((recalled.params.params[0].value - 0.75).abs() < f64::EPSILON);

        // Verify bridge received the restored params
        let applied = bridge.capture_parameters("track-1");
        assert_eq!(applied.params.len(), 2);
    }

    #[tokio::test]
    async fn capture_and_recall_preset_with_chunks() {
        let repo = test_repo().await;
        let bridge = test_bridge_with_params();

        // Capture (MockDawBridge returns empty chunks, but we test the plumbing)
        let captured = capture_and_save_preset(&bridge, &repo, "track-1", "rig-1", "Full Preset")
            .await
            .unwrap();
        assert_eq!(captured.params.params.len(), 2);

        // Verify params persisted
        let stored = repo.list_param_snapshots("rig-1").await.unwrap();
        assert_eq!(stored.len(), 1);

        // Recall
        let recalled = recall_preset(&bridge, &repo, "track-1", "rig-1")
            .await
            .unwrap();
        assert_eq!(recalled.params.params.len(), 2);
    }

    #[tokio::test]
    async fn recall_not_found() {
        let repo = test_repo().await;
        let bridge = MockDawBridge::new();

        let err = recall_snapshot(&bridge, &repo, "track-1", "nonexistent")
            .await
            .unwrap_err();
        assert!(matches!(err, SnapshotError::NotFound(_)));
    }

    #[tokio::test]
    async fn chunk_base64_round_trip() {
        let original = DawStateChunk {
            fx_id: "fx-1".into(),
            plugin_name: "Helix Native".into(),
            block_type: signal_proto::BlockType::Custom,
            chunk_data: vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03],
        };
        let stored = chunk_to_stored(&original, "owner-1");
        let restored = stored_to_chunk(&stored).unwrap();

        assert_eq!(restored.fx_id, "fx-1");
        assert_eq!(restored.plugin_name, "Helix Native");
        assert_eq!(
            restored.chunk_data,
            vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03]
        );
    }

    #[tokio::test]
    async fn params_json_round_trip() {
        let original = DawParameterSnapshot::new(vec![DawParamValue {
            fx_id: "fx-1".into(),
            param_index: 42,
            param_name: "Reverb Mix".into(),
            value: 0.333,
        }]);

        let json = params_to_json(&original).unwrap();
        let restored = json_to_params(&json).unwrap();

        assert_eq!(restored.params.len(), 1);
        assert_eq!(restored.params[0].param_index, 42);
        assert!((restored.params[0].value - 0.333).abs() < f64::EPSILON);
    }
}

// endregion: --- Tests
