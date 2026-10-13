# Service Composition

How `daw-reaper`'s service surface is declared, mounted, and consumed.
Companion to the architect crate's `layer` module docs.

## The pieces

- **Trait** — declared in `daw-proto` with `#[architect::rpc]`. One
  trait per service slot (Markers, Transport, Fx, …). The derive
  emits a sync trait, an async mirror, a `<T>Client`, a `serve`
  mount verb, and a `Service` composition token.
- **Backend impl** — in `daw-reaper` (and `daw-standalone` for the
  in-memory test backend). Each backend struct impls every service
  trait. Reaper is currently `pub struct Reaper;` (stateless,
  REAPER FFI is global); dependencies-as-fields can be added later
  without touching the bundle.
- **Bundle** — `impl Services for Reaper` in `daw-reaper/src/services.rs`.
  One `layers![…::Service]` list of every service the backend
  exposes. Single source of truth.
- **Facade re-export** — `daw::{Services, Layer, layers!, …}` so
  apps depending only on `daw` can compose without an `architect`
  direct dep.

## Mounting Reaper

The full surface is one call:

```rust
use daw::Services as _;
let router = daw::reaper::Reaper.into_router();
```

`into_router()` is sugar for `Self::layers().provide(self)` —
builds the canonical bundle, binds Reaper into every service slot,
returns a `LayerRouter` that's also a `vox::Handler<DriverReplySink>`.

## Overrides and bolt-ons

The bridge mount adds a service whose backend isn't Reaper —
the dock host lives on the Dioxus side:

```rust
// crates/daw-bridge/src/lib.rs
use daw::{Layer, Services as _};

let daw_handler = daw::reaper::Reaper::layers()
    .merge(daw_proto::dock_host::layer(dock_host_backend))  // pre-mounted, different backend
    .provide(daw::reaper::Reaper);
```

`LayerRouter` resolves duplicate method IDs by **last-merge wins**,
so the same pattern overrides a service for tests:

```rust
let router = Reaper::layers()
    .merge(fx_chains_mock::mock())   // override fx_chains with an in-memory stub
    .provide(Reaper);
```

## Deployment shapes

The same `LayerRouter` runs four ways. Pick at the call site.

### 1. Direct sync (zero overhead)

The trait is plain Rust. Call directly on the backend:

```rust
use daw_proto::Markers;
let id = Markers::add(&reaper, "intro", 0.0)?;
```

No router, no dispatcher, no future. This is what `daw-extension-runtime`
uses on REAPER's main thread when it can block.

### 2. In-process async (same process, dispatcher-marshaled)

The UI runs on a separate thread and can't block the main DAW
loop. The router accepts calls, marshals them through Reaper's
main-thread dispatcher, returns futures:

```rust
let router = Reaper.into_router();
// Pair with vox::Driver + in-memory transport; clients use
// MarkersClient::new(driver.caller()).
```

### 3. Cross-process via Unix socket (bridge → external tools)

`daw-bridge` mounts the same router on a Unix socket so the daw
CLI, tests, and FTS extensions running outside the REAPER process
share the same client types:

```rust
// crates/daw-bridge/src/lib.rs
let acceptor = DawConnectionAcceptor::new(daw_handler);
start_unix_socket_server(acceptor.clone());
```

`daw-control` then exposes those clients as `daw::rpc::{Daw,
Project, TrackHandle, …}` — the async surface the UI and any
out-of-tree consumer touches.

### 4. HTTP / WebSocket via axum (future)

When we ship a web client, the same router plugs into axum:

```rust
let router = Reaper.into_router();
let app = axum::Router::new()
    .route("/rpc", architect::axum_ws::handler(router))
    .layer(/* cors, auth */);
```

The wasm-compiled `<T>Client` types work over the websocket without
code changes.

## Adding a service

Four touch points:

1. **`daw-proto/src/foo.rs`** — declare the trait:
   ```rust
   #[architect::rpc]
   pub trait Foo {
       fn do_thing(&self, ...) -> Result<..., DawError>;
   }
   ```
2. **`daw-proto/src/lib.rs`** — `pub mod foo;`
3. **`daw-reaper/src/foo.rs`** — implement against REAPER:
   ```rust
   impl Foo for Reaper { fn do_thing(&self, ...) { ... } }
   ```
4. **`daw-reaper/src/services.rs`** — one line in the `layers!`
   list:
   ```rust
   layers![/* ... */ foo::Service, /* ... */]
   ```

Mock backends (daw-standalone, daw-reaper-mock) follow the same
pattern with their own impls. The bundle list is per-backend, so a
mock can omit services it doesn't simulate.

## Dependency injection (when it lands)

`Reaper` is currently stateless. When a service needs a non-FFI
dependency — AI client, plugin host, audio backend — the dep
becomes a struct field:

```rust
pub struct Reaper {
    pub ai_client: Arc<dyn AiClient>,
    pub plugin_host: Arc<dyn PluginHost>,
    // ...
}

impl AiAssistant for Reaper {
    fn ask(&self, prompt: &str) -> Result<String> {
        self.ai_client.complete(prompt)  // dep through self
    }
}
```

The bundle declaration doesn't change. Tests construct `Reaper`
with mock fields. Production wires real impls (a `Reaper::builder()`
helper would land at the same time if more than one dep emerges).

This is the Rust equivalent of Effect's
`Service.DefaultWithoutDependencies.pipe(Layer.provide(MockDep))` —
same trait, different runtime deps, no separate Mock service
class.

## Reference

- Composition primitives: `architect::layer` module docs (in the
  architect repo).
- Runnable walkthrough: `architect/examples/layered-services/`.
- Production wiring: `crates/daw-bridge/src/lib.rs::register_daw_dispatcher`.
- Async client surface: `crates/daw-control/src/lib.rs::DawClients`.
