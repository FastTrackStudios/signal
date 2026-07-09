# Crate Facade Pattern

This document describes the architectural pattern used in FastTrackStudio where
a **facade crate** is the only public API surface for a group of internal crates.
External consumers (apps, other domains) depend **only** on the facade — never
on internal implementation crates directly.

## Why

- **Single import**: Apps write `use signal::ProfileId` instead of guessing
  whether that lives in `signal-proto`, `signal-storage`, or `signal-live`.
- **Enforced boundary**: Internal crates can be refactored, split, merged, or
  renamed without breaking any app code — only the facade's re-exports change.
- **No accidental coupling**: An app can't reach into `signal-live::engine` to
  use an internal type that wasn't meant to be public.

## Structure

A domain (e.g. `signal`) is organized as a folder of crates:

```
crates/signal/
  signal-proto/       # Domain types, traits, IDs (leaf — no deps on siblings)
  signal-storage/     # Persistence layer
  signal-live/        # Runtime service impls
  signal-controller/  # User-facing controller API
  signal-import/      # Vendor preset import
  signal-daw-bridge/  # DAW FX tree inference
  nam-manager/        # NAM file catalog
  signal-ui/          # Dioxus UI components (special — stays public)
  signal/             # <-- THE FACADE. Only this is in [workspace.dependencies]
```

## Rules

### 1. Only the facade appears in `[workspace.dependencies]`

In the root `Cargo.toml`:

```toml
[workspace.dependencies]
signal = { path = "crates/signal/signal" }
signal-ui = { path = "crates/signal/signal-ui", default-features = false }
# NO signal-proto, signal-live, signal-storage, etc.
```

This means app crates **physically cannot** write `signal-proto.workspace = true`.

### 2. Internal crates reference each other via `path =`

Inside the signal folder, crates depend on siblings using relative paths:

```toml
# crates/signal/signal-import/Cargo.toml
[dependencies]
signal-proto = { path = "../signal-proto" }
```

This works because they're in the same directory. They don't need workspace entries.

### 3. The facade re-exports everything apps need

The facade crate (`signal/src/lib.rs`) has:

```rust
// Glob re-export of the domain types crate
pub use signal_proto::*;

// Explicit re-exports from internal crates
pub use signal_controller::{SignalController, ops, active_context};
pub use signal_live::{macro_registry, MacroRecorder, MacroRecord};
pub use signal_live::engine::{DawPatchApplier, RigSceneApplier, MorphEngine, ...};

// Companion crates as modules (for deeper access when needed)
pub use signal_import;
pub use signal_daw_bridge;
pub use nam_manager;
```

### 4. Apps import everything from the facade

```rust
// YES - always use the facade
use signal::{ProfileId, BlockType, SignalController};
use signal::signal_import::fabfilter::FabFilterImporter;
use signal::nam_manager::NamPack;

// NO - never depend on internal crates
use signal_proto::ProfileId;      // won't compile — not in workspace deps
use signal_live::macro_registry;  // won't compile
```

### 5. UI crates may stay public

UI component crates (`signal-ui`, `session-ui`, `keyflow-ui`) are an exception.
They contain Dioxus components with feature flags (`web`/`desktop`) and are
legitimately needed as direct dependencies by app crates. Keep them in
`[workspace.dependencies]`.

### 6. When adding a new public type

If an internal crate adds a new `pub` type that apps need:

1. Add the re-export in the facade's `lib.rs`
2. Apps use it via `signal::NewType`
3. Never add the internal crate back to workspace deps

### 7. Feature-gate heavy subsystems

When a facade spans subsystems with very different dependency weights (e.g.
WASM-compatible core vs platform-specific implementations), use **Cargo
features** to keep the base lightweight:

```toml
# crates/daw/Cargo.toml
[dependencies]
daw-proto = { path = "../daw-proto" }          # always included
daw-control = { path = "../daw-control" }      # always included
daw-reaper = { path = "../daw-reaper", optional = true }
daw-control-sync = { path = "../daw-control-sync", optional = true }
daw-standalone = { path = "../daw-standalone", optional = true }
dawfile-reaper = { path = "../dawfile-reaper", optional = true }

[features]
reaper = ["dep:daw-reaper"]
sync = ["dep:daw-control-sync"]
standalone = ["dep:daw-standalone"]
file = ["dep:dawfile-reaper"]
```

The facade's `lib.rs` then conditionally exposes modules:

```rust
pub use daw_control::*;                  // Core API — always available

pub mod service { pub use daw_proto::*; } // Protocol types — always available

#[cfg(feature = "reaper")]
pub mod reaper { pub use daw_reaper::*; }

#[cfg(feature = "sync")]
pub mod sync { pub use daw_control_sync::*; }
```

Consumers opt in: `daw = { workspace = true, features = ["reaper", "sync"] }`.

### 8. Hide implementation details from the public API

A facade is only as clean as what it re-exports. Watch for these leaks:

**Service clients**: If an internal crate has a control layer (high-level
handles) over a protocol layer (raw RPC clients), only the handles should be
public. Raw clients must be `pub(crate)`:

```rust
// In the control crate (e.g. daw-control/src/lib.rs):

// WRONG — leaks raw RPC client through the facade
pub use daw_proto::AudioEngineServiceClient;

// RIGHT — consumers use the high-level handle
pub(crate) use daw_proto::AudioEngineServiceClient;
pub use self::audio_engine::AudioEngine;  // wraps the client
```

**Why this matters**: If raw clients are public, consumers can bypass your
handle abstractions and call RPC methods directly. This creates an implicit
API contract you must maintain forever, even if you refactor the protocol.

**Return types**: When a handle method returns a type from an internal crate,
that type must be re-exported through the facade. Check that all types in
public method signatures are themselves public:

```rust
// AudioEngine::get_state() returns AudioEngineState from daw-proto.
// So daw-control must re-export it:
pub use daw_proto::{AudioEngineState, AudioLatency, AudioInputInfo};
```

**Implementation crates**: Feature-gated modules like `daw::reaper` expose
implementation details by design (the REAPER extension needs them). This is
acceptable — the feature gate acts as an opt-in boundary. But the default
(featureless) facade should expose only the abstract API.

### 9. Consistent module naming

Choose module names that reflect the consumer's mental model, not the internal
crate name:

| Internal crate | Facade module | Rationale |
|---------------|--------------|-----------|
| `daw-control` | `daw::` (root) | The primary API surface |
| `daw-proto` | `daw::service` | Protocol/service types |
| `daw-control-sync` | `daw::sync` | Blocking API variant |
| `daw-reaper` | `daw::reaper` | Implementation-specific |
| `daw-standalone` | `daw::standalone` | Testing/mock |
| `dawfile-reaper` | `daw::file` | File format operations |

Avoid leaking internal crate names into the public namespace. Write
`daw::file::parse_rpp_file()`, not `daw::dawfile_reaper::parse_rpp_file()`.

## Applying to Other Domains

This pattern should be applied to every domain group in the workspace:

| Domain | Facade | Internal crates | UI (public) |
|--------|--------|-----------------|-------------|
| signal | `signal` | signal-proto, signal-live, signal-storage, signal-controller, signal-import, signal-daw-bridge, nam-manager | signal-ui |
| session | `session` | session-proto | session-ui |
| keyflow | `keyflow` | keyflow-proto, keyflow-midi, keyflow-engraver | keyflow-ui |
| daw | `daw` | daw-proto, daw-control, daw-control-sync, daw-reaper, daw-standalone, dawfile-reaper | daw-ui |
| input | `input` | input-proto, input-reaper | — |

For each domain:

1. Ensure a facade crate exists (e.g. `crates/session/session/`)
2. Move all re-exports into the facade
3. Remove internal crates from `[workspace.dependencies]`
4. Fix internal crates to use `path = ` for sibling deps
5. Update all app imports from `domain_internal::X` to `domain::X`
6. Compile-check all affected crates

## Checklist for Applying

```
[ ] Identify all internal crates in the domain folder
[ ] Ensure facade crate exists with proper re-exports
[ ] Remove internal crates from root Cargo.toml [workspace.dependencies]
[ ] Fix internal crate Cargo.toml: workspace = true → path = "../sibling"
[ ] Fix all app crate Cargo.toml: remove internal deps, add facade
[ ] Fix all app source: use domain_internal:: → use domain::
[ ] Add missing re-exports to facade as compilation errors reveal them
[ ] Feature-gate heavy/optional subsystems (rule 7)
[ ] Audit public API: hide service clients, raw types (rule 8)
[ ] Verify module names match consumer mental model (rule 9)
[ ] cargo check all affected crates (including cross-workspace consumers)
[ ] Keep UI crates public in [workspace.dependencies]
```
