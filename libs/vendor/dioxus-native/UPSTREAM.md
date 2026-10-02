# dioxus-native (vendored)

From DioxusLabs/dioxus `packages/native` at rev
`f717a8e184a522d078b70bb4b4d62a5f9a99ddfc` — the rev the workspace pins
every other dioxus crate to. Patched in through the root Cargo.toml's
`[patch."https://github.com/DioxusLabs/dioxus"]`.

The one change (`src/dioxus_renderer.rs`): the iOS simulator takes the
`vello-hybrid` renderer even when `vello` is enabled. Full Vello needs
indirect compute dispatch, which the simulator's Metal lacks (a wgpu
validation panic at the first frame); real iPhones and desktops keep Vello.

`Cargo.toml` is the upstream one with the workspace-inherited dependencies
written out as `cargo metadata` resolved them (dioxus siblings as git deps
at the same rev). When the pin moves, re-vendor from the new rev and carry
the renderer change across.
