//! Service surface for `Standalone` — declared as [`architect::Services`].
//!
//! Mirrors `daw-reaper`'s [`Reaper`](daw_reaper::Reaper) services bundle so an
//! in-process `Daw` client can speak to Standalone exactly like it does to
//! REAPER. Only domains with real (non-stub) impls are listed; `midi`, `fx`,
//! `live_midi`, `automation`, `batch` are intentionally omitted until Standalone
//! grows real implementations (calls into those domains via the RPC client
//! will fail with a routing error — same as any missing service).
//!
//! ```ignore
//! use architect::Services;
//! use daw_standalone::Standalone;
//!
//! let router = Standalone::new().into_router();
//! ```

use architect::{Layer, Services, layers};
use daw_proto::{
    action_registry, audio_engine, automation, batch, dawfile_service, event_bus, ext_state, fx,
    fx_chains, fx_params, health, input, item, live_midi, marker, midi, peak, plugin_loader,
    project, region, resource, routing, screenset, song_files, stretch_marker, take, tempo_map,
    toolbar, track, transport, transport_sync, window_geometry,
};

use crate::sync::Standalone;

impl Services for Standalone {
    fn layers() -> impl Layer<Standalone> {
        layers![
            transport::Service,
            project::Service,
            marker::Service,
            // Real since batch execution routes through
            // daw_proto::batch::run (one RPC, N ops, FromStep chaining).
            batch::Service,
            region::Service,
            tempo_map::Service,
            audio_engine::Service,
            // Partial services are listed here so the in-proc Daw client can
            // route implemented methods while each service owns its fallback
            // behavior for unsupported operations.
            midi::Service,
            fx::Service,
            fx_chains::Service,
            fx_params::Service,
            live_midi::Service,
            automation::Service,
            track::Service,
            routing::Service,
            ext_state::Service,
            health::Service,
            item::Service,
            take::Service,
            action_registry::Service,
            input::Service,
            toolbar::Service,
            resource::Service,
            screenset::Service,
            dawfile_service::Service,
            window_geometry::Service,
            peak::Service,
            plugin_loader::Service,
            transport_sync::Service,
            // A peer streaming the song in: its folder's files by range.
            song_files::Service,
            // A take's stretch markers: what an edited slice plays through,
            // read and rebuilt by the project doc's bridge.
            stretch_marker::Service,
            // `#[subscribe]` stream siblings — served from the PubSub
            // hubs on `Standalone` (see each domain's StreamSource
            // impl). The event-bus base service is empty post-port;
            // only its stream sibling is mounted.
            transport::StreamService,
            track::StreamService,
            marker::StreamService,
            region::StreamService,
            tempo_map::StreamService,
            event_bus::StreamService,
            peak::StreamService,
            transport_sync::StreamService,
        ]
    }
}
