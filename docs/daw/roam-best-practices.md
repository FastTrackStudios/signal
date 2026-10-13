# Roam & Moire Best Practices

Case study from [bearcove/ship](https://github.com/bearcove/ship) and
[bearcove/moire](https://github.com/bearcove/moire). All code examples are from
production use. Clone both to `/tmp/ship` and `/tmp/moire` for reference.

## Versions

```toml
roam = "7.3.0"
facet = "0.44.1"
moire = "1.0"
edition = "2024"
```

---

## 1. Service Trait Definitions

### Basic pattern

```rust
#[roam::service]
pub trait Ship {
    // Simple query
    async fn list_projects(&self) -> Vec<ProjectInfo>;

    // Void return
    async fn accept(&self, session: SessionId);

    // Response enum (NOT Result<T,E>)
    async fn create_session(&self, req: CreateSessionRequest) -> CreateSessionResponse;

    // Server-push streaming
    async fn subscribe_events(&self, session: SessionId, output: Tx<SubscribeMessage>);

    // Bidirectional streaming
    async fn transcribe_audio(&self, audio_in: Rx<Vec<u8>>, segments_out: Tx<TranscribeMessage>);
}
```

### Rules

1. **No `Result<T, E>`** — roam doesn't support it. Use response enums:

   ```rust
   #[derive(Facet)]
   pub enum CreateSessionResponse {
       Created { session: SessionInfo },
       Failed { message: String },
   }
   ```

2. **Max 4 params** — Facet tuple constraint. Group with request structs:

   ```rust
   #[derive(Facet)]
   pub struct CaptainAssignExtras {
       pub files: Vec<AssignFileRef>,
       pub plan: Vec<PlanStepInput>,
   }

   async fn captain_assign(&self, title: String, desc: String, keep: bool, extras: CaptainAssignExtras);
   ```

3. **`Tx<T>` / `Rx<T>` for streaming** — channels that flow over the RPC wire.

4. **Newtype IDs** with `#[facet(transparent)]`:

   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
   #[facet(transparent)]
   pub struct SessionId(pub String);
   ```

5. **All types implement `Facet`** — roam's serialization trait (replaces serde for RPC).

### Multiple service traits

Ship uses separate traits for different client types:

```rust
#[roam::service]
pub trait CaptainMcp {
    async fn captain_assign(&self, ...) -> McpToolCallResponse;
}

#[roam::service]
pub trait MateMcp {
    async fn run_command(&self, command: String, cwd: Option<String>) -> McpToolCallResponse;
}
```

One struct implements all:

```rust
impl Ship for ShipImpl { ... }
impl CaptainMcp for CaptainMcpSessionService { ... }
impl MateMcp for MateMcpSessionService { ... }
```

### What `#[roam::service]` generates

For `#[roam::service] pub trait Ship`:
- `ShipDispatcher<H>` — server-side handler (routes method IDs)
- `ShipClient` — type-safe RPC caller
- `ship_service_descriptor()` — method routing metadata

---

## 2. Service Implementations

### State management: Arc all the way down

```rust
#[derive(Clone)]  // Clone is cheap — all fields are Arc
pub struct ShipImpl {
    registry: Arc<tokio::sync::Mutex<ProjectRegistry>>,
    sessions: Arc<Mutex<HashMap<SessionId, ActiveSession>>>,
    agent_driver: Arc<AcpAgentDriver>,
    store: Arc<JsonSessionStore>,
    global_events_tx: broadcast::Sender<GlobalEvent>,
}
```

**Why Clone?** Dispatchers take ownership of the impl. `Clone` must be cheap
(all `Arc` internally) so the dispatcher can clone it per-connection.

### Early lock release

```rust
// GOOD: release lock before async work
async fn list_branches(&self, project: ProjectName) -> Vec<String> {
    let project_path = {
        let registry = self.registry.lock().await;
        registry.get(&project.0).map(|p| p.path.clone())
    };  // lock released here

    let Some(path) = project_path else { return Vec::new() };
    git_branch_list(path).await  // async work after lock
}

// BAD: lock held across await
async fn list_branches(&self, project: ProjectName) -> Vec<String> {
    let registry = self.registry.lock().await;
    let path = registry.get(&project.0).map(|p| p.path.clone());
    git_branch_list(path).await  // lock still held!
}
```

### Session-scoped service wrappers

For services that need per-session context:

```rust
#[derive(Clone)]
struct CaptainMcpSessionService {
    ship: ShipImpl,
    session_id: SessionId,
}

impl CaptainMcp for CaptainMcpSessionService {
    async fn captain_assign(&self, title: String, ...) -> McpToolCallResponse {
        match self.ship.captain_tool_assign(&self.session_id, title, ...).await {
            Ok(text) => McpToolCallResponse { text, is_error: false, diffs: vec![] },
            Err(text) => McpToolCallResponse { text, is_error: true, diffs: vec![] },
        }
    }
}
```

---

## 3. Streaming

### Event subscription (server → client)

```rust
async fn subscribe_events(&self, session: SessionId, output: Tx<SubscribeMessage>) {
    // Get data while holding lock (scoped)
    let session_data = {
        let sessions = self.sessions.lock().expect("poisoned");
        sessions.get(&session).map(|s| {
            (s.events_tx.subscribe(), s.replay_events())
        })
    };  // lock released

    let Some((receiver, replay)) = session_data else {
        let _ = output.close(Default::default()).await;
        return;
    };

    // Replay history
    for event in replay {
        let _ = output.send(SubscribeMessage::Event(event)).await;
    }

    // Stream live events in background
    tokio::spawn(async move {
        let mut rx = receiver;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if output.send(SubscribeMessage::Event(event)).await.is_err() {
                        break;  // Client disconnected
                    }
                }
                Err(_) => break,
            }
        }
        let _ = output.close(Default::default()).await;
    });
}
```

### Bidirectional (client → server → client)

```rust
async fn transcribe_audio(&self, mut audio_in: Rx<Vec<u8>>, segments_out: Tx<TranscribeMessage>) {
    tokio::spawn(async move {
        loop {
            match audio_in.recv().await {
                Ok(Some(chunk)) => {
                    if let Some(segment) = process_audio(&chunk) {
                        if segments_out.send(TranscribeMessage::Segment(segment)).await.is_err() {
                            break;
                        }
                    }
                }
                Ok(None) => break,   // Stream ended
                Err(_) => break,     // Receiver dropped
            }
        }
        let _ = segments_out.close(Default::default()).await;
    });
}
```

---

## 4. Dispatcher & Connection Setup

### Single-service acceptor

```rust
use ship_service::{ShipClient, ShipDispatcher};

let link = roam_websocket::WsLink::new(ws_stream);
let (caller, _session) = roam::acceptor(link)
    .establish::<ShipClient>(ShipDispatcher::new(ship_impl.clone()))
    .await?;
```

### Multi-service composition (RoutedHandler)

Our pattern — composing multiple services over one Unix socket:

```rust
let handler = RoutedHandler::new()
    .with(transport_service_descriptor(), TransportServiceDispatcher::new(transport))
    .with(project_service_descriptor(), ProjectServiceDispatcher::new(project))
    .with(profile_service_descriptor(), ProfileServiceDispatcher::new(signal_svc.clone()))
    .with(rig_service_descriptor(), RigServiceDispatcher::new(signal_svc.clone()));

start_unix_socket_server(handler);
```

### Dynamic per-connection routing

Ship routes different services based on connection metadata:

```rust
impl ConnectionAcceptor for ShipMcpConnectionAcceptor {
    fn accept(&self, _conn_id: ConnectionId, peer: &ConnectionSettings) -> Result<AcceptedConnection, Metadata> {
        let role = peer.peer_properties.get("ship-service");

        match role {
            Some("captain-mcp") => {
                let svc = CaptainMcpSessionService { ship: self.ship.clone(), session_id };
                Ok(AcceptedConnection {
                    setup: Box::new(move |conn| {
                        conn.establish::<NoopCaller>(CaptainMcpDispatcher::new(svc))
                    }),
                    ..
                })
            }
            Some("mate-mcp") => { /* similar for mate */ }
            _ => Err(Metadata::default()),
        }
    }
}
```

---

## 5. Error Handling

### Response enums (primary pattern)

```rust
#[derive(Facet)]
pub enum CreateSessionResponse {
    Created { session: SessionInfo },
    Failed { message: String },
}

#[derive(Facet)]
pub struct McpToolCallResponse {
    pub text: String,
    pub is_error: bool,
    pub diffs: Vec<McpDiffContent>,
}
```

### Internal errors → structured responses

```rust
async fn create_session(&self, req: CreateSessionRequest) -> CreateSessionResponse {
    match self.inner_create(req).await {
        Ok(session) => CreateSessionResponse::Created { session },
        Err(e) => CreateSessionResponse::Failed { message: format!("{e:#}") },
    }
}
```

---

## 6. Moire: Instrumented Async Runtime

Moire wraps Tokio primitives with diagnostic instrumentation. Every spawn,
lock, channel, and RPC call becomes a named entity in a live graph.

### When to use moire vs tokio directly

**Always use moire** for spawning, channels, locks, and timers. When the
`diagnostics` feature is off (default in production), all moire calls are
zero-overhead pass-throughs to tokio. When enabled, you get a live web
dashboard showing task relationships, lock contention, and causal chains.

### Task spawning

```rust
use moire::task::{spawn, FutureExt};

// Named task (visible in dashboard)
spawn(async { /* work */ }).named("handler.process_request");

// Blocking work
moire::task::spawn_blocking(|| expensive_computation()).named("compute.fft");
```

**`spawn` signature**: Same as `tokio::spawn` — requires `Send + 'static`.

### JoinSet (multi-task coordination)

```rust
use moire::task::JoinSet;

let mut set = JoinSet::named("workers");
for id in 0..10 {
    set.spawn(async move { process(id).await });
}

while let Some(result) = set.join_next().await {
    handle(result?);
}
```

### Channels

Mirror tokio's API but with required names:

```rust
use moire::sync::mpsc;
use moire::sync::broadcast;
use moire::sync::oneshot;
use moire::sync::watch;

// MPSC (bounded)
let (tx, mut rx) = mpsc::channel("events.work_queue", 64);

// Broadcast (one-to-many)
let (tx, _) = broadcast::channel("events.state_change", 16);
let rx = tx.subscribe();

// Oneshot (request/response)
let (tx, rx) = oneshot::channel("request.auth_token");

// Watch (state broadcast)
let (tx, rx) = watch::channel("state.connection_status", Status::Disconnected);
```

### Synchronization primitives

```rust
use moire::sync::{Mutex, SyncMutex, RwLock, Semaphore, Notify};

// Async mutex (tokio-backed, safe to hold across .await)
let lock = Mutex::new("state.sessions", HashMap::new());
let guard = lock.lock().await;

// Sync mutex (parking_lot, for very short critical sections only)
let lock = SyncMutex::new("config.cache", config);
let guard = lock.lock();  // blocking, no .await

// Async RwLock
let lock = RwLock::new("data.user_profiles", profiles);
let read = lock.read().await;
let write = lock.write().await;

// Semaphore
let sem = Semaphore::new("limit.concurrent_requests", 10);
let permit = sem.acquire().await?;

// Notify
let notify = Notify::new("signal.shutdown");
notify.notify_one();
```

### Timers

```rust
use moire::time::{sleep, interval, timeout};
use std::time::Duration;

sleep(Duration::from_secs(1)).await;

let mut tick = interval(Duration::from_millis(100));
tick.tick().await;

timeout(Duration::from_secs(5), long_operation()).await?;
```

### Processes

```rust
use moire::process::Command;

let status = Command::new("git")
    .args(["fetch", "--all"])
    .current_dir("/path/to/repo")
    .status()
    .await?;
```

### RPC instrumentation (roam integration)

Roam uses moire internally to track RPC request/response pairs:

```rust
// Client side (automatic in roam)
let req = moire::rpc::rpc_request("Profile.Activate", args_json);

// Server side (automatic in roam)
let resp = moire::rpc::rpc_response_for("Profile.Activate", &req.entity_ref());
```

This creates causal chains visible in the dashboard: request → wire → handler → response.

### Dashboard

```bash
# Enable diagnostics at build time
cargo build --features moire/diagnostics

# Run with dashboard connection
MOIRE_DASHBOARD=127.0.0.1:9119 ./your-binary

# Open dashboard at http://127.0.0.1:9130
```

### Naming conventions

```rust
// Hierarchical: category.specific
moire::task::spawn(work).named("handler.http_request");
mpsc::channel("queue.work_items", 64);
Mutex::new("state.connection_pool", pool);
Notify::new("signal.shutdown");
```

---

## 7. Applying to FastTrackStudio

### Current state

- DAW services use roam over Unix sockets (extension → fts-control)
- `RoutedHandler` composes 14 DAW + 2 session services
- Signal services defined with `#[roam::service]` but not yet dispatched

### Signal service exposure

```rust
// In register_daw_dispatcher():
let svc = signal_bridge::controller().service().clone();

let handler = session_mgr.create_handler()
    .with(block_service_descriptor(), BlockServiceDispatcher::new(svc.clone()))
    .with(profile_service_descriptor(), ProfileServiceDispatcher::new(svc.clone()))
    .with(rig_service_descriptor(), RigServiceDispatcher::new(svc.clone()))
    // ... all 11 signal services
```

### Migration checklist

- [ ] Use `moire::task::spawn` instead of `tokio::spawn` everywhere
- [ ] Use `moire::sync::Mutex` instead of `tokio::sync::Mutex`
- [ ] Name all spawned tasks, channels, and locks
- [ ] Use response enums (not `Result`) in `#[roam::service]` traits
- [ ] `#[derive(Clone)]` on service impls with all `Arc` fields
- [ ] Early lock release before `.await` points
- [ ] Add `moire/diagnostics` feature flag for dev builds

### Lock safety rule

**Never hold `std::sync::RwLock` or `std::sync::Mutex` across `.await`.**

Use `tokio::sync::Mutex` (via `moire::sync::Mutex`) for async code, or
clone data out of `std::sync` locks before the `.await`:

```rust
// GOOD: clone out, then await
let applier = self.daw_applier.read().expect("poisoned").clone();
if let Some(applier) = applier {
    applier.apply_graph(&graph, name).await;
}

// BAD: guard held across await (not Send, will fail to compile in spawned tasks)
if let Some(applier) = self.daw_applier.read().expect("poisoned").clone() {
    applier.apply_graph(&graph, name).await;  // guard still alive!
}
```

---

## 8. Anti-Patterns (from ship's LESSONS.md)

- **Terminal scraping** — fragile. Use structured events.
- **Bare strings as IDs** — use newtypes (`ProfileId`, `RigId`)
- **Polling** — use event streams and subscriptions
- **Mixing formatting with delivery** — backend constructs, transport sends
- **Filesystem as database** — use typed storage
- **Result types in service traits** — roam doesn't support them

## 9. Key Files to Study

```
/tmp/ship/crates/ship-service/src/lib.rs     # Service trait definitions
/tmp/ship/crates/ship-types/src/lib.rs       # Type definitions (Facet)
/tmp/ship/crates/ship-server/src/ship_impl.rs # Service implementations
/tmp/ship/crates/ship-server/src/main.rs     # Server setup & dispatch
/tmp/ship/LESSONS.md                          # Architecture principles

/tmp/moire/crates/moire/src/lib.rs           # Moire API surface
/tmp/moire/crates/moire-tokio/src/enabled/   # Instrumented implementations
/tmp/moire/crates/moire-examples/            # Example scenarios
```
