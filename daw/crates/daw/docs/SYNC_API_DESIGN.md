# DAW Control Synchronous API Design

## Problem

The current `daw-control` API is fully async:
```rust
fx.param(idx).set(value).await?
```

This doesn't work in real-time audio processing loops where we can't use async/await. We need plugins and time-sensitive code to make synchronous DAW calls.

## Design Approach

### Option A: Blocking tokio runtime (Simple, Correct)

```rust
// In plugin initialization (once per plugin lifecycle)
let daw_sync = DawSync::new(connection_handle)?;

// In audio processing loop (real-time safe - non-blocking sender)
daw_sync.queue_set_param(track_idx, fx_idx, param_idx, value)?;
```

**How it works:**
1. Create a background tokio runtime in `DawSync::new()`
2. Async calls run on that runtime in a background thread
3. Audio loop sends requests via a bounded MPSC channel (non-blocking sender)
4. Background thread processes requests asynchronously
5. Responses optionally written to shared state (for verification)

**Pros:**
- No async in real-time code
- No blocking in real-time code (non-blocking sender)
- Clean separation of concerns
- DAW-agnostic (works with any DAW service)
- Backward compatible (daw-control stays async)

**Cons:**
- Extra thread overhead (minimal for bounded channels)
- Requests are queued, not immediately executed

### Option B: Sync wrapper with block_on (Not recommended for real-time)

```rust
let daw_sync = DawSync::blocking(connection_handle)?;
daw_sync.set_param(track_idx, fx_idx, param_idx, value)?; // blocks!
```

**Pros:**
- Simpler API
- Immediate execution

**Cons:**
- Blocks the audio thread (potential glitches, dropout)
- Not safe for real-time code
- Only suitable for non-time-critical contexts

## Recommended: Option A (Channel-Based)

### API Design

```rust
/// Thread-safe synchronous DAW interface for real-time contexts
pub struct DawSync {
    runtime: tokio::runtime::Runtime,
    clients: Arc<DawClients>,
    request_tx: mpsc::UnboundedSender<DawRequest>,
}

impl DawSync {
    /// Create new sync DAW interface with background tokio runtime
    pub fn new(handle: roam::ConnectionHandle) -> Result<Self> {
        let runtime = tokio::runtime::Runtime::new()?;
        let clients = Arc::new(DawClients::new(handle));
        let (request_tx, request_rx) = mpsc::unbounded_channel();

        // Spawn background task to process requests
        let clients_clone = clients.clone();
        runtime.spawn(async move {
            while let Some(request) = request_rx.recv().await {
                // Process DAW requests
                match request {
                    DawRequest::SetFxParam { track, fx, param, value } => {
                        let _ = clients_clone
                            .fx
                            .set_parameter(/* ... */)
                            .await;
                    }
                    // ... other request types
                }
            }
        });

        Ok(Self {
            runtime,
            clients,
            request_tx,
        })
    }

    /// Queue FX parameter change (non-blocking, real-time safe)
    pub fn queue_set_param(
        &self,
        track_idx: u32,
        fx_idx: u32,
        param_idx: u32,
        value: f32,
    ) -> Result<()> {
        self.request_tx.send(DawRequest::SetFxParam {
            track: track_idx,
            fx: fx_idx,
            param: param_idx,
            value,
        })?;
        Ok(())
    }

    // More methods: set_param, get_param, etc.
}

enum DawRequest {
    SetFxParam { track: u32, fx: u32, param: u32, value: f32 },
    GetFxParam { track: u32, fx: u32, param: u32 },
    SetFxName { track: u32, fx: u32, name: String },
    // ... more operations
}
```

### Usage in fts-macros Plugin

```rust
pub struct FtsMacros {
    params: Arc<MacroParams>,
    mapping_bank: Arc<MacroMappingBank>,
    resolution_cache: ResolutionCache,
    daw_sync: Option<DawSync>,  // Initialize from plugin init hook
}

fn process(&mut self, buffer: &mut Buffer, ...) -> ProcessStatus {
    self.resolution_cache.clear();

    let macro_values = [...];
    for (macro_idx, value) in macro_values.iter().enumerate() {
        let mappings = self.mapping_bank.get_mappings_for_param(macro_idx as u8);

        for mapping in mappings {
            match (
                self.resolution_cache.resolve_track_cached(&mapping.target_track),
                self.resolution_cache.resolve_fx_cached(0, &mapping.target_fx),
            ) {
                (Ok(track), Ok(fx), Ok(())) => {
                    let transformed = mapping.mode.apply(*value);

                    // Real-time safe: non-blocking queue
                    if let Some(daw) = &self.daw_sync {
                        let _ = daw.queue_set_param(
                            track,
                            fx,
                            mapping.target_param_index,
                            transformed,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    ProcessStatus::Normal
}
```

## Implementation Plan

### Phase 1: Core DawSync (New crate: `daw-control-sync`)

1. Create `crates/daw-control-sync/src/lib.rs`
2. Implement `DawSync` struct with bounded/unbounded channel
3. Implement request types: SetParam, GetParam, etc.
4. Background task processes requests

### Phase 2: Integration with daw-control

1. Add re-exports to `daw-control` for convenience
2. Or keep separate to avoid adding deps (tokio, mpsc)

### Phase 3: Plugin Integration

1. fts-macros adds daw-control-sync dependency
2. Implement plugin initialization hook to create DawSync
3. Update process() loop to queue parameter changes
4. Test with multi-track synchronized control

## Advantages for DAW Agnosticism

This design enables:

```rust
// Works with ANY DAW service (REAPER, Logic, Ableton, etc.)
// Plugin is 100% DAW-agnostic
pub fn apply_macro_mapping(
    daw: &DawSync,
    macro_value: f32,
    mapping: &MacroMapping,
) -> Result<()> {
    let transformed = mapping.mode.apply(macro_value);
    daw.queue_set_param(
        mapping.target_track,
        mapping.target_fx,
        mapping.target_param_index,
        transformed,
    )
}
```

## Testing Strategy

1. **Unit tests**: DawRequest serialization, channel behavior
2. **Integration tests**: Mock DAW service + DawSync
3. **Real tests**: fts-macros with actual REAPER + DAW service
4. **Benchmarks**: Measure queue latency, throughput

## Future Enhancements

- Response channels for getting parameter values
- Batched requests for efficiency
- Priority queues for time-critical changes
- Metrics/monitoring of queue depth
- Graceful shutdown on connection loss
