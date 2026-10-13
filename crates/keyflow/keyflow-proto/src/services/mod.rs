//! RPC contracts for Keyflow.
//!
//! Each submodule owns a sync trait decorated with
//! `#[architect::rpc]`. The macro derives an async vox client + a
//! `serve` function from the sync trait — backends impl the plural sync
//! trait directly (zero-cost in-process call sites), and remote
//! callers reach the same surface via the auto-emitted `<svc>::Client`
//! over vox. See `architect/DESIGN.md`.
//!
//! Per-module aliases (`<svc>::Dispatcher`, `<svc>::descriptor`,
//! `<svc>::serve`, `<svc>::layer`, `<svc>::Service`) mirror the
//! pattern used in `daw::service::*` — consumers compose mounting via
//! `router.mount(chart::serve(backend))` or `chart::layer(backend)`.

pub mod chart;
pub mod chart_parse;
pub mod guide;
pub mod parser;

// Trait + payload re-exports at the `services::*` root for ergonomic imports.
// Traits use plural nouns by convention: singular structs (`Chart`), plural
// RPC contracts (`Charts`).
pub use chart::Charts;
pub use chart_parse::ChartParsers;
pub use guide::{GuideRequest, Guides};
pub use parser::{ParseRequest, ParseResponse, Parsers};

// Vox-emitted client aliases — these are the asynchronous caller-side
// proxies. Dispatchers and descriptors stay per-module since they're
// only touched at the mounting site, not by general callers.
#[cfg(feature = "vox")]
pub use chart::ChartsClient;
#[cfg(feature = "vox")]
pub use chart_parse::ChartParsersClient;
#[cfg(feature = "vox")]
pub use guide::GuidesClient;
#[cfg(feature = "vox")]
pub use parser::ParsersClient;
