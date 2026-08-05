---
name: architect-actions
description: "Use when adding a new REAPER-facing action module, or refactoring an existing `*_actions.rs` in crates/session (or elsewhere) off the legacy action-id-enum + action_for_id/dispatch pattern onto architect's declarative action system. Covers #[architect::actions], ActionBackend/ScopedActionBackend, the daw crate's Tracks/Items/Projects + TracksExt, and the concrete-wrapper-struct-with-Deref shape used by TrackManager. Also the reference for `daw::service::TracksExt` conventions (TrackRef vs Track, generic plumbing vs domain logic) even outside action modules."
---

# architect + daw action modules

How to build (or refactor) a REAPER action module the way
`crates/session/session/src/track_manager.rs` does — paired with its
contract in `crates/session/proto/src/track_manager.rs`. Those files are
the canonical example — read them alongside this skill, not instead of
them.

## The old pattern (what you're refactoring away from)

The action modules under `crates/session/session/src/` used to look like
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

Deleting the legacy path also means deleting the module's entries in the
`actions_proto::define_actions!` block (`session/src/lib.rs`) and its arm
in `daw_module.rs`'s dispatch chain. The `#[architect::actions]` macro
emits the *same* `FTS_*` command ids, so leaving both in place registers
every command twice. Pin the generated ids with a test in the proto
module (`<Trait>Actions::all()`) so the exact REAPER command-name strings
keybindings and toolbars depend on can't drift.

## The target shape

Three pieces, using `track_manager` as the reference:

1. **A plain declaration-only trait**, annotated with `#[architect::actions]`
   — this is the macro surface, nothing else. It lives in the domain's
   `-proto` crate (`crates/session/proto/src/`), never beside the impl:
   traits are protocol, and the macro emits the `ActionMeta` consts +
   `register_<name>_actions` beside the trait, where any host — the
   REAPER extension, a CLI, a remote client — can see them without
   pulling in the implementation.

   ```rust
   #[architect::actions(namespace = "TRACK_MANAGER")]
   pub trait TrackManagerActions {
       #[action(undo, description = "Add the next dynamic-template channel to the selected track scope")]
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
   - **`undo`** (bare flag) marks a *mutating* action. The backend then
     brackets the handler in a host undo block labelled from the same
     metadata that names it in the action list — never hand-roll
     `begin_undo_block`/`end_undo_block` in an action body. Read-only
     actions leave it off so they don't litter the undo history with
     empty points.
   - **Method signature constraint (hard, macro-enforced):** `fn(&self)`
     — no other parameters — returning either nothing or
     `Result<(), E: std::fmt::Display>` (`DawResult<()>` qualifies,
     `eyre::Result<()>` would too). This is a real limitation in
     `libs/architect/macros/architect-action-derive/src/lib.rs`, not a
     style choice — REAPER named commands take no arguments. If your
     logic needs parameters, give it a private helper method with
     parameters and have the `#[action]` method call it with fixed
     arguments (e.g. `add_layer` calling `add_named_scope("DBL")`).

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
     `Deref` — use `Tracks::get(&self.daw, ...)` for those (needed
     around the `Tracks::get`/`Projects::get` name collision below).
   - One implementor of the trait, but the *type* is generic — the same
     `TrackManagerActions for TrackManager<D>` impl runs against
     `daw::reaper::Reaper` in production and
     `daw_standalone::sync::Standalone` in tests. Don't write two
     versions.

3. **No registration wrapper — call the macro-generated function.** The
   macro emits `register_<trait_snake_sans_Actions_suffix>_actions`
   (`TrackManagerActions` → `register_track_manager_actions`). Don't
   write a per-module `register_actions` shim around it; register at the
   entry point (`apps/extensions/reaper-fts-extensions/src/lib.rs`),
   composing the scope nesting there:

   ```rust
   session_proto::track_manager::register_track_manager_actions(
       &architect::action::ScopedActionBackend::new(daw_reaper::Reaper, "SESSION", "Session"),
       std::sync::Arc::new(session::track_manager::TrackManager::new(daw_reaper::Reaper)),
   );
   ```

   `ScopedActionBackend::new(inner, scope_id, scope_category)` prepends
   `scope_id` to every action's `id` and overrides `category` to
   `scope_category` — this is how "Track Manager" (the trait's own
   identity) ends up nested under "Session" without the trait ever
   saying so, and it belongs at the registration site because *who a
   module lives under* is a fact about the registrar, not the module.
   Stack another wrap for a further level (an eventual "FTS" outer
   scope) without touching the leaf trait at all.

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

## Undo blocks — `#[action(undo)]`, never by hand

Tag the action, write only the logic:

```rust
#[action(undo, description = "...")]
fn add_channel(&self) -> DawResult<()>;
```

The backend brackets the handler in a host undo block labelled
`"{category} {group} - {display_name}"` (e.g. `"Session Track Manager -
Add Channel"`), so the whole action is one atomic undo point and `?` can
propagate freely without skipping an end-block. Never call
`begin_undo_block`/`end_undo_block` inside an action body, and don't
build a "run with undo" wrapper method or an `ActionHistoryService` —
that plumbing lives once, in `impl ActionBackend for Reaper`
(`features/reaper/daw-reaper/src/action_registry.rs`).

Leave `undo` off read-only actions so they don't add empty undo points.

Selection save/restore is a separate concern and is **not** provided —
if an action needs to preserve selection across a mutation, do it
explicitly with `TracksExt::select`/`add_to_selection` at that call
site, don't build a shared wrapper on spec.

## `daw::service::TracksExt` — where generic plumbing goes

Selection, lookup, and tree-navigation helpers that aren't specific to
any one feature's business logic belong in
`crates/daw/proto/src/track/ext.rs` (`daw::service::TracksExt`,
blanket-impl'd for any `D: Tracks + Items + Projects`), not reimplemented
per action module:

**Snapshot + navigate** (prefer this for anything touching more than one
track — the per-call helpers each re-fetch the whole track list, so
walking a tree with them is quadratic and, over RPC, a round-trip per
node):

- `track_tree()` → `TrackTree`, one fetch of the whole list. Then
  `.get(guid)`, `.at_index(i)`, `.children_of(guid)`, `.parent_of(t)`,
  `.subtree_end_index(guid)`, `.shape_of_children(guid)` all run in
  memory. It's a *snapshot* — re-take it after mutating rather than
  reusing a stale one to compute indices.

**Selection:**

- `selected_scope()` — the one selected track, or a "no track is
  selected" error.
- `select(guid)` / `add_to_selection(guid)` — `select` clears and
  selects exactly one; `add_to_selection` extends whatever's already
  selected. There's no `select_only` — "select" already means exclusive
  selection.

**Lookup:**

- `get_track(guid)` / `track_at_index(i)` / `find_track(name)` —
  `find_track` errors on zero *or* multiple matches rather than
  silently picking one.
- `children_of(guid)`, `subtree_end_index(guid)` — single-shot
  convenience wrappers over `track_tree()`.

**Mutation** (all implicitly on `ProjectContext::Current`, so no
`project.clone()` threading):

- `insert_track(name)` / `insert_track_at(name, index)` — (not
  `add_top_level`: a track is just a track; "top level" is what happens
  when you don't give it a parent/index).
- `set_depth(guid, depth)` — folder-depth change (`1` opens, `0` plain,
  negative closes that many levels).
- `append_child(parent_guid, name)` / `append_shape(parent_guid, shape)`
  — create a track, or a whole nested `TrackShape` subtree, as the last
  children of a folder. These handle the fiddly part: `prepare_append`
  computes the insertion index from a pre-mutation snapshot and fixes up
  whichever track currently terminates the parent's subtree so the
  newcomer takes over closing the folder.
- `insert_shape_at(shape, index)` — escape hatch for when you already
  know the position and the tree is mid-restructure (not well-formed
  enough for a subtree-end walk).
- `move_items(from_guid, to_guid)`.

**`TrackShape`** is the nested "subtree to create" type
(`TrackShape::leaf(name)` / `with_children(name, kids)`), the
counterpart to the DAW's flat depth-change encoding.
`TrackTree::shape_of_children(guid)` reads an existing subtree back out
as one, so "give the new channel the same mics the old one has" is a
read-then-append rather than hand-rolled recursion.

The dividing line: if the logic only needs `Tracks`/`Items`/`Projects`
primitives and isn't specific to what your feature is building, it
belongs on `TracksExt`/`TrackTree`, where every other feature gets it
for free. Only what's genuinely domain-specific stays on your wrapper —
in `TrackManager`'s case that's "which dynamic-template dimension does
this track name read as" and "what shape does each action build", and
nothing else.

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

1. Delete the action-id enum, `action_for_id`, `dispatch()`, any
   `*ActionsImpl` bridge struct, the per-module `register_actions`
   wrapper, and any no-op `init(ctx)` (drop its call in
   `daw_module.rs`'s `init` too).
2. Turn the forwarding `#[architect::actions]` trait's one-line methods
   into declarations, and put the real logic in the wrapper's `impl` of
   that trait — one method per action, no shadow "logic" method behind
   it. Tag mutating ones `#[action(undo, ...)]`.
3. Introduce (or reuse) a concrete wrapper struct + `Deref` if the
   module doesn't have one yet.
4. Move any generic selection/lookup/tree-building plumbing onto
   `TracksExt`/`TrackTree`; leave only domain logic on the wrapper.
5. Update the callsite in
   `apps/extensions/reaper-fts-extensions/src/lib.rs` to call the
   macro-generated `register_<name>_actions` directly, wrapping the
   backend in `ScopedActionBackend` for the nesting.
6. Remove the module's `define_actions!` entries from
   `session_actions` in `lib.rs` and its arm from `daw_module.rs`'s
   dispatch chain if they exist — that's the legacy path this replaces;
   leaving both registers the same command twice under two different ids.
7. Add headless tests against `Standalone` before considering the
   refactor done — cover *every* branch, not just the happy path. The
   two branches that had no test in `track_manager` both turned
   out to be broken; the tests found it.
