# Vendored: facet/vox Swift runtime

- **Upstream**: https://github.com/facet-rs/facet (the facet monorepo)
- **Pinned at**: tag `vox-v0.10.0-rc.5`, commit
  `715745d49f4500b585319ca0f1d6d39fbd4fd5f7` (matches the workspace's
  crates.io pin `vox = 0.10.0-rc.5`, root `Cargo.toml`) — vendored
  2026-07-16.
- **Subtrees taken**: `phon/swift/{cblake3,phon-schema,phon-ir,phon-engine,phon}`
  and `vox/swift/vox-runtime` (Sources + Tests + `wireMessageSchemas.bin`).
- **Dropped**: `phon-jit` / `cphon-jit-stencils` / `VoxRuntimeJIT` (macOS-only
  JIT; the interpreter covers control-surface traffic), benches and fixture
  packages, the nested `vox-runtime/Package.swift` (superseded by the root
  `Package.swift` here, which also widens platforms to iOS 18 / watchOS 11).
- **Why vendored**: the Swift runtime is not published as a standalone
  package, and the wire must stay in lock-step with the pinned vox crates —
  same rule as `libs/vendor/phon` on the Rust side.
- **Consumers**: `apps/fasttrackstudio/watchos` (the watch remote; today it
  uses the HTTP+SSE bridge, this runtime is the native vox path), future
  iOS/macOS Swift clients.

To bump: re-run the sparse checkout at the tag matching the new crates.io
pin, recopy the subtrees, and keep this file's pin in sync.
