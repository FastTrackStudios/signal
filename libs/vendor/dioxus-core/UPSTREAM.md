# dioxus-core (vendored)

From DioxusLabs/dioxus `packages/core` at rev
`f717a8e184a522d078b70bb4b4d62a5f9a99ddfc` — the rev the workspace pins
every other dioxus crate to. Patched in through the root Cargo.toml's
`[patch."https://github.com/DioxusLabs/dioxus"]`.

The one change (`src/events.rs`, `Callback::__point_to`): when a component
re-renders, `rsx!`'s generated `memoize` points its old event handler at the
new one. Upstream unwraps that, and it fails whenever the old handler's
owner — the scope that created the closure — has already dropped while the
component still held it (an element built in one scope and rendered from
another: a popup's render function, a face remounted mid-update). The
result was `called Result::unwrap() on an Err value: Dropped(...)` on the
main thread, which on iOS aborts the app. The vendored copy logs a warning
and takes the new handler instead.

`Cargo.toml` is the upstream one with the workspace-inherited dependencies
written out (dioxus siblings as git deps at the same rev). When the pin
moves, re-vendor from the new rev and carry the change across — or drop
this copy once upstream stops unwrapping there.
