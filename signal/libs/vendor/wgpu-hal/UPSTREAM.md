Source: crates.io `wgpu-hal` 29.0.4 (gfx-rs/wgpu), the published tarball
with the registry's own bookkeeping files dropped; the `resolver` key is
removed (the workspace root's applies). License: MIT OR Apache-2.0 — theirs.

Vendored for one change to `src/metal/adapter.rs`:

- **The iOS Simulator has indirect draw/dispatch.** It advertises only the
  Apple2 GPU family, which Metal's feature tables list without indirect
  draws and dispatches, so wgpu turned `INDIRECT_EXECUTION` off — and full
  Vello, which needs indirect compute dispatch, panicked at its first frame
  there. The simulator's Metal is serviced by the Mac's own GPU, which
  supports them (Dawn enables base vertex/instance draws on the simulator
  for the same reason). With it on, the simulator draws with full Vello on
  the GPU — the phone's renderer — instead of vello-hybrid's CPU path.

Drop this when upstream treats the simulator by its host GPU, or when the
simulator reports a family that includes indirect dispatch.
