Source: crates.io `anyrender_vello_hybrid` 0.8.0 (dioxuslabs/anyrender), the
published tarball with the registry's own bookkeeping files dropped; the
licence files are the anyrender repo's.
License: MIT OR Apache-2.0 — theirs, not ours, and it stays that way.

Vendored for one change to `window_renderer.rs`:

- **`set_size` resizes the surface whenever the surface is the wrong
  size**, not only when the scene changes size. Upstream skips the whole
  body when the scene already matches, and it updates the scene even
  while the renderer is `Pending`, when there is no surface yet. So a
  resize that arrives between `resume` and `complete_resume` sets the
  scene to the new size, and the `set_size` that `complete_resume` makes
  with the same numbers then does nothing: the surface stays at the size
  `resume` was given.

  Found on the iPad simulator, the one place this renderer runs (full
  Vello needs indirect dispatch, which the simulator's Metal lacks — see
  libs/vendor/dioxus-native). The app starts in portrait and turns to
  landscape before the renderer is ready; the surface stayed 1640x2360
  and iOS showed it squeezed into the 2360x1640 layer, the page cut off
  at the right and a black band along the bottom. Full Vello's
  `set_size` resizes unconditionally and never had the bug.

  Still present in upstream `main` as of 2026-09-25; drop this copy once
  a release fixes it.
