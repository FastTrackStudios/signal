---
name: architect-actions
description: "Use when adding a new REAPER-facing action module, or refactoring an existing `*_actions.rs` in crates/session (or elsewhere) off the legacy action-id-enum + action_for_id/dispatch pattern onto architect's declarative action system. Covers #[architect::actions], ActionBackend/ScopedActionBackend, the daw crate's Tracks/Items/Projects + TracksExt, and the concrete-wrapper-struct-with-Deref shape used by TrackManager. Also the reference for `daw::service::TracksExt` conventions (TrackRef vs Track, generic plumbing vs domain logic) even outside action modules."
---

# architect + daw action modules

How to build (or refactor) a REAPER action module the way
`crates/session/session/src/track_manager_actions.rs` does. That file is
the canonical example — read it alongside this skill, not instead of it.

## The old pattern (what you're refactoring away from)

Most `*_actions.rs` files in `crates/session/session/src/` still look like
this — a hand-written id enum, a `action_for_id(&str) -> Option<Enum>`
string-matcher, a `dispatch(action)` free function, **and** a separate
`#[architect::actions]` trait whose methods are one-line forwards to
`dispatch()`. Two systems doing the same job:

```rust
pub enum FooAction { DoThing }
pub fn action_for_id(id: &str) -> Option<FooAction> { /* string match */ }
pub fn dispatch(action: FooAction) { /* match -> real logic, log errors */ }

#[architect::actions(namespace = "FTS_FOO")]
pub trait FooActions {
    #[action(description = "...", category = "Session", group = "Foo")]
    fn foo_do_thing(&self);
}
impl FooActions for FooActionsImpl {
    fn foo_do_thing(&self) { dispatch(FooAction::DoThing); }
}
```

Don't add to this pattern. New modules — and modules you're touching for
other reasons — should move to the shape below instead.

## The target shape

Three pieces, using `track_manager_actions.rs` as the reference:

1. **A plain declaration-only trait**, annotated with `#[architect::actions]`
   — this is the macro surface, nothing else:

   ```rust
   #[architect::actions(namespace = "TRACK_MANAGER", category = "Session")]
   pub trait TrackManagerActions {
       #[action(description = "Add the next dynamic-template channel to the selected track scope")]
       fn add_channel(&self) -> DawResult<()>;
       // ...
   }
   ```

   - **`namespace`** is the trait's own identity only — never its parent
     scope. `TrackManagerActions` says `"TRACK_MANAGER"`, not
     `"FTS_SESSION_TRACK_MANAGER"`. The trait must not know it's nested
     under Session or FTS; that's composed at registration time (see
     below), not declared here.
   - **`category`** (trait-level, optional) is the one thing that *can't*
     be derived — it's a fact about who's registering this trait (e.g.
     `"Session"`), not about the trait's own name. Give it once at the
     trait, not per method.
   - **`group`** defaults to the trait's own name in Title Case
     (`TrackManagerActions` → strips the `Actions` suffix → `"Track
     Manager"`) unless a method overrides it. Don't repeat
     `group = "..."` on every `#[action(...)]` — that's exactly the
     duplication the trait-name default exists to avoid.
   - **Method signature constraint (hard, macro-enforced):** `fn(&self)`
     — no other parameters — returning either nothing or
     `Result<(), E: std::fmt::Display>` (`DawResult<()>` qualifies,
     `eyre::Result<()>` would too). This is a real limitation in
     `libs/architect/macros/architect-action-derive/src/lib.rs`, not a
     style choice — REAPER named commands take no arguments. If your
     logic needs parameters, give it a private helper method with
     parameters and have the `#[action]` method call it with fixed
     arguments (e.g. `add_layer` calling `add_named_scope("DBL", true)`).

2. **One concrete struct implementing it**, generic over the DAW backend:

   ```rust
   pub struct TrackManager<D> { daw: D }
   impl<D> TrackManager<D> {
       pub fn new(daw: D) -> Self { Self { daw } }
   }
   impl<D> std::ops::Deref for TrackManager<D> {
       type Target = D;
       fn deref(&self) -> &D { &self.daw }
   }
   impl<D: Tracks + Items + Projects> TrackManagerActions for TrackManager<D> {
       fn add_channel(&self) -> DawResult<()> { /* real logic here, not a second method */ }
   }
   ```

   - **Never call action methods on the raw backend directly**
     (`daw_reaper::Reaper.add_channel()`, or a blanket `impl<D: Tracks +
     ...> TrackManagerActions for D {}`). `add_channel` is this module's
     business logic layered *on top of* `Tracks`/`Items`/`Projects`, not
     a capability the DAW backend itself has. If it reads like the raw
     backend has your action as a method, that's the wrong layer — wrap
     it in a struct.
   - The `Deref` impl is what keeps call sites readable: without it every
     line of business logic becomes `self.daw.set_folder_depth(...)`,
     `self.daw.all(...)`, `self.daw.selected(...)` — noisy repetition
     ("looks like a Tron").  With it, `self.set_folder_depth(...)` just
     works — Rust tries `TrackManager<D>`'s own inherent methods first
     (so `self.append_child(...)`, your own domain logic, still
     resolves to itself), and only reaches through `Deref` to `D` for
     names `TrackManager` doesn't define. One caveat: UFCS-style
     disambiguated calls (`Tracks::get(self, ...)`) do *not* go through
     `Deref` — use `Tracks::get(&self.daw, ...)` for those (see
     `prepare_append` in `track_manager_actions.rs` for the one place
     this comes up, working around the `Tracks::get`/`Projects::get`
     name collision below).
   - One implementor of the trait, but the *type* is generic — the same
     `TrackManagerActions for TrackManager<D>` impl runs against
     `daw::reaper::Reaper` in production and
     `daw_standalone::sync::Standalone` in tests. Don't write two
     versions.

3. **A `register_actions(&backend, daw)` function** that composes the
   namespace nesting and constructs the wrapper:

   ```rust
   pub fn register_actions<B, D>(backend: &B, daw: D)
   where
       B: architect::action::ActionBackend + Clone,
       D: Tracks + Items + Projects + Send + Sync + 'static,
   {
       let session = architect::action::ScopedActionBackend::new(backend.clone(), "SESSION", "Session");
       register_track_manager_actions_actions(&session, std::sync::Arc::new(TrackManager::new(daw)));
   }
   ```

   `register_<snake_trait_name>_actions` is macro-generated (from the
   trait name — mind the doubled `_actions` when the trait itself ends in
   `Actions`: `TrackManagerActions` → `register_track_manager_actions_actions`).
   `ScopedActionBackend::new(inner, scope_id, scope_category)` prepends
   `scope_id` to every action's `id` and overrides `category` to
   `scope_category` — this is how "Track Manager" (the trait's own
   identity) ends up nested under "Session" without the trait ever
   saying so. Stack another `ScopedActionBackend` wrap for a further
   level (e.g. an eventual "FTS" outer scope) without touching the leaf
   trait at all.

## Error handling — don't hand-roll it

`architect::action::ActionBackend::register`'s handler type is
`Arc<dyn Fn() -> Result<(), String> + Send + Sync>`. The macro wraps
`()`-returning methods as always-`Ok`, and `Result<(), E: Display>`
methods via `.map_err(|e| e.to_string())` automatically. On the REAPER
side, `daw-reaper`'s `impl ActionBackend for Reaper` already pops a
message box on `Err` (`show_action_error` in
`features/reaper/daw-reaper/src/action_registry.rs`) — this is universal
infrastructure, not something your action module implements. Just return
`DawResult<()>` (or any `Result<(), impl Display>`) and let a real `?`
propagate; don't swallow errors into `tracing::error!` the way the old
`dispatch()` pattern did (that's silent-to-the-user and is what this
whole system replaces).

## Undo blocks — call the primitive directly, don't build a service

`Projects::begin_undo_block`/`end_undo_block` are already on `D` (in
scope via `Deref`). Bracket a mutating action's body directly — no shared
"run with undo" wrapper method, no `ActionHistoryService`:

```rust
fn add_channel(&self) -> DawResult<()> {
    let project = ProjectContext::Current;
    let label = "Session Track Manager - Add Channel";
    self.begin_undo_block(project.clone(), label);
    let result = (|| -> DawResult<()> {
        // ... real logic, `?` freely ...
    })();
    self.end_undo_block(project, label, None);
    result
}
```

The inner `(|| -> DawResult<()> { ... })()` is a local IIFE, not a
generic abstraction — it exists purely so `?`/early-`return` inside the
body can't skip `end_undo_block`. Every mutating action repeats this
~4-line shape; that's fine, it's cheaper to read inline than to chase
through a wrapper. Non-mutating actions (pure queries, or ones that log
and return) don't need it at all.

Selection save/restore is a separate concern from undo blocks and is
**not** a responsibility this pattern gives you for free — if a specific
action needs to preserve selection across a mutation, do it explicitly
with `TracksExt::select`/`add_to_selection` at that call site, don't
build a shared "preserve selection" wrapper on spec.

## `daw::service::TracksExt` — where generic plumbing goes

Selection, lookup, and tree-navigation helpers that aren't specific to
any one feature's business logic belong in
`crates/daw/proto/src/track/ext.rs` (`daw::service::TracksExt`,
blanket-impl'd for any `D: Tracks + Items + Projects`), not reimplemented
per action module:

- `selected_scope()` — the one selected track, or a "no track is
  selected" error.
- `select(guid)` / `add_to_selection(guid)` — `select` clears and
  selects exactly one; `add_to_selection` extends whatever's already
  selected. There's no `select_only` — "select" already means exclusive
  selection.
- `insert_track(name)` — add a plain top-level track. (Not
  `add_top_level` — a track is just a track; "top level" is what
  happens when you don't give it a parent/index.)
- `find_track(name)` — errors on zero or multiple matches, doesn't
  silently pick one.
- `get_track(guid)` — one track by guid via `TrackRef::Guid`, or an
  "invalid object" error if it no longer exists. Prefer this over
  `self.all(...).iter().find(...)` when you only need one track.
- `children_of(guid)` — direct children of a track, in mixer order.
  Prefer this over fetching `all()` and filtering by `parent_guid`
  yourself.
- `subtree_end_index(guid)` — the insertion index just past a track's
  subtree, for "append as last child" operations.
- `move_items(from_guid, to_guid)`.

The dividing line: if the logic only needs `Tracks`/`Items`/`Projects`
primitives and isn't specific to what your feature is building (channel
trees, arrangement variants, whatever), it belongs on `TracksExt`, where
every other feature gets it for free too. If it's specific business
logic (like `TrackManager::append_child`'s folder-depth bookkeeping, or
`child_shapes`' dynamic-template-aware recursion), it stays as an
inherent method on your wrapper struct.

## `TrackRef` vs `Track` — don't collapse them

Every `Tracks`/`Items` RPC method takes a `TrackRef` (`Guid`/`Index`/
`Master`) to *identify* a target, never a `&Track`. Keep it that way:

- `TrackRef` is a cheap identifier, safe to pass across the RPC boundary
  and safe to hold across multiple calls — the backend re-resolves it
  fresh each time.
- `Track` is a data *snapshot* from a query. Using one as a "reference"
  to act on later risks staleness (indices shift, folder_depth changes)
  between fetch and use.

Pattern: fetch a `Track` (or `Vec<Track>`) to *read* fields off it, then
target further calls with `TrackRef::Guid(that_track.guid.clone())`.

## Known collision — don't add `Tracks: Projects` as a supertrait

It looks natural (tracks live in a project) but `Projects::get(&self,
project_id: &str)` and `Tracks::get(&self, project, track)` share the
name `get` — adding the supertrait bound puts both in scope and makes
every `.get()` call ambiguous (`E0034`), breaking the `#[architect::rpc]`
macro codegen. Fixing this for real means renaming one `get` across
every implementor in the tree — a separate, deliberate change, not
something to attempt inside a `*_actions.rs` refactor.

## Testing

No REAPER, no `ActionBackend` — construct the wrapper directly over
`daw_standalone::sync::Standalone` and call trait methods as plain
method calls:

```rust
let daw = Standalone::new();
daw.seed_project(ProjectInfo { guid: "demo-proj".into(), ..Default::default() });
let tm = TrackManager::new(daw.clone());

let electric_gtr = daw.insert_track("Electric GTR").unwrap();
daw.select(&electric_gtr).unwrap();
tm.add_multi_mic().expect("first Add Multi-Mic");
```

Assert on the resulting track tree with `daw_proto`'s existing
`TrackHierarchy`/`TrackStructureBuilder`/`assert_tracks_equal` (the same
helpers `dynamic-template`'s guitar-layer tests already use) rather than
inventing a one-off comparison type — see
`crates/session/session/tests/track_manager_actions.rs` for a full
example, including the `Track` → `TrackNode`/`FolderDepthChange`
conversion (`Track.folder_depth` is already `FolderDepthChange`'s raw
`i32` representation, so it's a direct field-for-field map, not a
reshape).

## Refactor checklist

When moving an existing `*_actions.rs` onto this pattern:

1. Delete the action-id enum, `action_for_id`, and `dispatch()`.
2. Turn the forwarding `#[architect::actions]` trait's one-line methods
   into the real logic (or keep them thin and call a private helper on
   the wrapper struct for anything needing non-`&self` parameters).
3. Introduce (or reuse) a concrete wrapper struct + `Deref` if the
   module doesn't have one yet.
4. Move any selection/lookup/undo plumbing that's genuinely generic onto
   `TracksExt`; leave feature-specific logic on the wrapper.
5. Update the `register_actions` callsite in
   `apps/extensions/reaper-fts-extensions/src/lib.rs` to the two-arg
   `register_actions(&daw_reaper::Reaper, daw_reaper::Reaper)` form and
   wire the `ScopedActionBackend` nesting.
6. Remove the module's `TRACK_MANAGER_*`-style entries from
   `session_actions`'s `define_actions!` block in `lib.rs` if they exist
   — that's the legacy path this replaces; leaving both registers the
   same command twice under two different ids.
7. Add a headless test against `Standalone` before considering the
   refactor done.
