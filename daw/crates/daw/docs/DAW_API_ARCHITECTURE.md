# DAW API Architecture: Native Sync Core + Vox RPC Facade

## Decision

The `daw` crate should treat in-process REAPER extension code as native synchronous code first.
The async Vox API should be an exposure layer for UI, remote clients, background tasks, and tests.

Target namespace convention:

```rust
// Native synchronous API.
// Direct REAPER/reaper-rs calls, valid only on REAPER's main thread.
daw::

// Async Vox/RPC API.
// Used by UI, services, remotes, streams, and cross-thread callers.
daw::rpc
```

Plain `daw::` should mean "native DAW access". If code says `daw::rpc`, it is allowed to cross Vox,
Tokio, sockets, task queues, and main-thread dispatch boundaries.

## Problem

The current public API makes extension actions look like ordinary DAW operations, but many calls go
through the async Vox client path:

```rust
let project = daw::get()?.current_project().await?;
let position = project.transport().get_position().await?;
let selection = project.transport().get_time_selection().await?;
let regions = project.regions().all().await?;
```

In REAPER, each small async operation can schedule a separate main-thread hop. This is correct for
RPC clients, but it is too slow for hot extension actions. A workflow such as "insert section region"
can become many serial operations and land in the hundreds of milliseconds.

In-process extension actions should instead execute like a direct `reaper-rs` transaction:

```rust
let project = daw::current_project()?;
let position = project.transport().position()?;
let selection = project.transport().time_selection()?;
let regions = project.regions().all()?;
```

No `.await`, no Vox boundary, no per-operation main-thread scheduling.

## Core Model

There should be one native implementation with two front doors:

```text
                 daw-proto
        shared types and operation contracts
                      |
             native REAPER implementation
       direct reaper-rs calls on the main thread
              /                         \
       daw:: native API              daw::rpc API
       sync extension code           async Vox clients
```

`daw-proto` owns shared domain types and service contracts:

- `ProjectInfo`
- `Region`
- `Marker`
- `Transport`
- `MusicalPosition`
- request/response structs
- service operation contracts where useful

`daw::` owns native synchronous handles and methods. These are only valid on REAPER's main thread.

`daw::rpc` owns async Vox clients, dispatchers, streams, and remote/service-facing APIs.

## Main-Thread Rule

All direct `daw::` calls must run on REAPER's main thread.

That is not a limitation for normal extension actions: REAPER invokes those actions on the main
thread. It is also the desired behavior for hot paths.

If a workflow needs background computation, it should compute off-thread and then submit one
main-thread transaction to apply DAW changes.

## Module Service Model

Feature modules such as `session`, `dynamic-template`, and `keyflow` should implement their behavior
as native sync services first.

Example:

```rust
pub struct SessionRuntime;

impl SessionRuntime {
    pub fn insert_keyflow_region(&self, kind: SectionKind) -> Result<()> {
        let project = daw::current_project()?;
        let position = project.transport().position()?;
        let selection = project.transport().time_selection()?;
        let regions = project.regions().all()?;

        // infer bounds, carve overlaps, insert region, color, rename, move cursor
        Ok(())
    }
}
```

REAPER actions call that runtime directly:

```rust
SESSION_RUNTIME.insert_keyflow_region(SectionKind::Chorus)?;
```

The Vox endpoint is only an adapter:

```rust
async fn insert_keyflow_region(&self, req: InsertRegionRequest) -> Result<()> {
    main_thread::query(move || {
        SESSION_RUNTIME.insert_keyflow_region(req.kind)
    }).await?
}
```

This keeps hot extension code synchronous while still exposing the same capability over Vox.

## Service Traits

The existing async service traits remain useful for RPC and test backends. They should not be the only
contract for hot in-process code.

Use parallel contracts where necessary:

```rust
// Native sync contract.
pub trait RegionOps {
    fn get_regions(&self, project: ProjectContext) -> Vec<Region>;
    fn add_region(&self, project: ProjectContext, start: f64, end: f64, name: String) -> u32;
}

// Async RPC contract.
pub trait RegionService {
    async fn get_regions(&self, project: ProjectContext) -> Vec<Region>;
    async fn add_region(&self, project: ProjectContext, start: f64, end: f64, name: String) -> u32;
}
```

The REAPER async service should call the sync implementation inside one main-thread dispatch when
possible. It should not duplicate business logic or schedule one main-thread hop per tiny operation.

## RPC Endpoint Granularity

Generic DAW services still make sense:

- `RegionService::add_region`
- `MarkerService::add_marker`
- `TransportService::get_position`

But workflows that must feel instantaneous should be exposed as higher-level service methods:

- `SessionService::insert_keyflow_region`
- `TemplateService::apply_track_template`
- `RoutingService::apply_route_plan`

One RPC request should map to one native main-thread transaction where possible.

Avoid hot workflows shaped like this:

```rust
get_position().await;
get_time_selection().await;
regions().all().await;
tempo_map().time_to_musical().await;
tempo_map().musical_to_time().await;
regions().add().await;
regions().set_color().await;
```

Prefer:

```rust
session.insert_keyflow_region(kind).await?;
```

where the endpoint forwards to sync native code in one main-thread transaction.

## Relationship To Existing Sync Code

`daw-reaper/src/sync_api.rs` already contains `DawMainThread`, which is the right foundation:

- direct synchronous REAPER calls
- no Vox
- no Tokio
- no `main_thread::query`
- `!Send + !Sync`

This should be promoted into the public native `daw::` API and expanded into normal handles:

```rust
daw::Daw
daw::Project
daw::Transport
daw::Regions
daw::Markers
daw::TempoMap
```

The older `DawSync`/`daw-control-sync` model is a blocking or queued wrapper around async clients.
That model can still be useful for plugin/audio-control contexts, but it is not the native extension
main-thread API.

## Migration Plan

1. Add `daw::rpc` and re-export the current async `daw-control` API there.
2. Keep `daw::service` for shared protocol/domain types during the transition.
3. Promote and expand `daw-reaper::DawMainThread` as the root native `daw::` API when the `reaper`
   feature is enabled.
4. Add sync native handles for transport, regions, markers, tempo map, ruler lanes, and undo blocks.
5. Refactor REAPER async service implementations to call native sync operations inside one
   `main_thread::query` where possible.
6. Update hot extension actions, starting with session/keyflow region insertion, to call the sync
   native runtime directly.
7. Move UI, remote, stream, and background consumers to `daw::rpc`.
8. Add tests that verify hot actions do not call the async RPC path.

## Performance Goal

For in-process REAPER actions, latency should be comparable to direct `reaper-rs` calls.

The target for normal marker/region insertion workflows is "feels instantaneous", not one REAPER
timer tick per DAW primitive.

