# Service-call ergonomics: Direct views, scope views, domain handles

**Status:** shipped 2026-07-28 (`985e3ab03`, `d8e5ec871`, `b31047ef1`) — this
note documents the design so it can be re-evaluated once the pattern has
been lived with. See "Open questions" for what to judge it against.

## The problem

`#[architect::rpc]` traits are the seam of every domain: apps talk to
`daw`/`signal`/`session` backends through plain trait methods. Wire-wise
that's right, but *calling* them in-process had degenerated into a dialect
(observed throughout the signal rig refactor, #65/#66):

```rust
// 1. UFCS everywhere — Standalone implements ~20 traits, and four of them
//    declare `set_volume` (Tracks, Routing, Item, Take):
<Standalone as Tracks>::set_volume(&daw, ProjectContext::Current,
    TrackRef::guid(&guid), 0.9)?;

// 2. Context/ref ritual — ProjectContext::Current + TrackRef::guid(...) on
//    every single call.

// 3. Multi-call dances with invariants the caller must remember:
let idx = <Standalone as FxChains>::add(&daw, ctx.clone(), "slot")?;   // add…
let fx  = <Standalone as FxChains>::get(&daw, ctx, idx)…;              // …then fetch guid
Routing::add_send(…); Routing::set_send_mode(…, PostFx);               // bus-mic routing
Routing::set_parent_send_enabled(…, false);                            // (3 calls, order matters)
// folder hierarchies: hand-computed REAPER depth math (-1 vs -2 closes)
```

The async **client** face never had this problem: the rpc macro generates
`<Trait>Client` (namespaced, no collisions) and — with `context = T` — a
`ScopedClient` (`client.scoped(ctx)` binds the ambient context). The local
face simply had no equivalent.

## The layers (as shipped)

```
#[architect::rpc] trait          ← the wire/seam contract (unchanged)
  ├─ generated <Trait>Direct     ← local twin of the generated client
  ├─ generated scope views       ← opt-in: scopes(name: Type, ...)
  └─ hand-written domain handles ← only cross-trait fusions remain
```

### 1. `<Trait>Direct` — always generated

A `Copy` wrapper over `&B` whose methods are **inherent**, plus a blanket
accessor trait:

```rust
use daw::service::RoutingDirectExt as _;
daw.routing_direct().set_volume(ctx, location, vol)?;   // no UFCS, no trait import
```

- Sync methods only (the in-process face); async methods keep their trait
  form. Subscribe declarations are skipped.
- With ambient `context = T`, the view mirrors the trait's sync face
  (`ctx: &T` second parameter).
- Zero cost: every method is `#[inline]` pure delegation.

### 2. Scope views — `scopes(name: Type, ...)`

Declared once on the trait, in nesting order:

```rust
#[architect::rpc(ops(...), scopes(project: ProjectContext, track: TrackRef))]
pub trait Tracks { … }
```

Generates one view per level (`TracksProjectScope`, `TracksTrackScope`).
**Assignment rule:** a method belongs to level N when its leading parameter
types are token-equal to the first N scope types; that level elides them and
the view passes its stored values:

```rust
let track = daw.tracks_direct().project(ctx).track(TrackRef::guid(g));
track.set_volume(0.9)?;      // (project, track) ride along
track.set_muted(true)?;
track.get();                  // all params were scope params
```

Views are `Clone`, expose bound values (`slot.region()`), and re-narrow
(`region.clone().key(8)`). `scopes` and `context = ...` are mutually
exclusive for now (scopes elide declared params; context injects an
undeclared one).

Adopted on: `Tracks` + `Routing` (`project`, `track`), `FxChains`
(`chain: FxChainContext`).

### 3. Domain handles — `daw_proto::handle`

`ProjectHandle` / `TrackHandle` now **Deref onto the generated scopes** (nine
delegation methods deleted). What stays hand-written is exactly what a
macro can't infer:

| kept | why |
|---|---|
| `project.track(guid)` / `add_track(name)` | guid-string keying + handle construction |
| `track.add_fx_slot(label)` | fuses FxChains add + guid-fetch (constant-guid invariant) |
| `track.send_to(bus).post_fx().replace_master_send().apply()` | 3-call routing ritual, `#[must_use]` so a half-built send can't drop |
| `project.tree()` → `folder`/`track`/`end`/`finish` | REAPER folder-depth bookkeeping (an unclosed folder = corrupted layout) |
| `track.fx()` | cross-trait hop (Tracks scope → FxChains scope) |
| `arm_audio_input(ch)` | sugar over `RecordInput::Audio` |

Naming rule that fell out: handles no longer rename trait methods
(`set_muted`, not `mute`) — Deref surfaces the trait's own vocabulary, and
one vocabulary beats two.

## Design decisions & trade-offs (re-evaluate these)

1. **Type-matching for scope assignment** (leading param types token-equal
   to scope types) rather than per-arg attributes (`#[scope] project: …`).
   - + Zero annotation burden on methods; existing traits adopt with one
     attribute.
   - − Token equality is textual: `ProjectContext` vs
     `crate::ProjectContext` in the same trait would mis-classify (didn't
     occur in practice — traits spell types consistently).
   - − A method whose first param *coincidentally* has the scope's type
     gets scoped whether it makes sense or not (`Routing::add_send`'s
     `source: TrackRef` scoped as "the track" — happened to be right).
2. **Sync-only views.** Async methods would need
   `-> impl Future + use<...>` capture plumbing; local rig code is sync, so
   this was deferred, not designed around.
3. **Deref for handle→scope** rather than re-delegation. One vocabulary,
   no drift; the cost is that handle docs show fewer methods than exist
   (Deref targets are one click away in rustdoc).
4. **Scopes are local-face only.** The vox client keeps its separate
   `ScopedClient` (single ambient context). Two mechanisms exist where one
   might.
5. **`LayerRouter` scoping is unrelated** (`merge_router_scoped` /
   `svc-scope` metadata is *instance* scoping for the wire; `scopes(...)`
   is *parameter* scoping for local calls). The name collision is
   unfortunate — candidates for a rename if it confuses.

## Open questions — "is there an even better way?"

Things to evaluate after living with it (revisit ~a month in, or when the
next domain adopts it):

- **Unify local scopes with the client's `ScopedClient`?** If `scopes(...)`
  also generated the async-client chain (`client.project(ctx).track(tr)…`),
  remote and local call sites would read identically. Requires async view
  methods (decision 2) and touching the vox-face emitters.
- **Should `context = T` be re-expressed as `scopes`?** A single-level
  scope over an *injected* param is nearly `context`; folding them would
  delete a concept. Blocker: context changes the emitted trait, scopes
  don't.
- **Cross-trait scope bundles.** The remaining hand-written handles exist
  because scopes are per-trait. A `#[architect::facade]` over several
  traits (generating one TrackHandle-like type from Tracks+Routing+FxChains
  declarations) would generate them too — but is a facade declaration
  really cheaper than the ~150 lines of `daw_proto::handle`? Evaluate by
  counting how many other domains end up wanting a hand-written handle.
- **Per-arg override attribute.** If type-matching ever mis-classifies in
  practice, add `#[scope(skip)]` / `#[scope(n)]` per-arg escapes rather
  than switching wholesale to annotations.
- **daw adoption of `context = ProjectContext`** would delete the ctx param
  from ~200 call sites but changes every daw service's wire shape — its own
  migration if ever.
- **Does the generated surface bloat compiles?** Each rpc trait now emits
  1–3 extra structs. Not measured; if build times regress, gate views
  behind an opt-out (or opt-in) flag.

## Where things live

- Macro: `libs/architect/macros/architect-rpc-derive/src/lib.rs`
  (`emit_direct_view`, `emit_scope_views`).
- Tests: `libs/architect/architect/tests/scoped_router.rs` (direct view,
  scope chain end-to-end; also the unrelated instance-scoped router).
- Domain handles: `crates/daw/proto/src/handle.rs`.
- Showcase call sites: `crates/signal/rig-host/src/lib.rs`,
  `signal_sampler::keys_rig::build_lane_tracks` (TrackTree),
  drums `apply_kit_mixer` (scoped mixer application), `route_to_bus`
  (SendBuilder).
