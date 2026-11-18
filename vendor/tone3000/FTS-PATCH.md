# tone3000 — FTS patch

Upstream: [OpenSauce/tone3000-rs](https://github.com/OpenSauce/tone3000-rs),
crates.io `tone3000` 0.1.0 (MIT, `LICENSE` kept as published). Vendored
from the published crate. `tests/` and `examples/` are not copied; the
unit tests in `src/` are, with upstream's dev-dependencies minus nam-rs,
which only the README's doc examples need (`doctest = false`).

## The patch

`Cargo.toml` only: `reqwest` 0.13 with `rustls` becomes `reqwest` 0.12
with `rustls-tls`. reqwest 0.13's `rustls` feature has exactly one crypto
backend, aws-lc-rs, and its `aws-lc-sys` build script compiles AWS-LC with
cmake — 186 s of a 375 s clean `cargo check --workspace`, on the critical
path, and tone3000 was the only crate in the graph that asked for it.
reqwest 0.12's `rustls-tls` is ring-backed and already in the graph
(architect, sentry and our own crates use it). `form` and `query` are
features in 0.13 and always on in 0.12, so they go; no source changes.

## Dropping it

When upstream lets a consumer choose the TLS backend (a `rustls-ring`
feature, or `rustls-no-provider`), take the crates.io release and delete
this directory and the `[patch.crates-io]` entry in the root `Cargo.toml`.
