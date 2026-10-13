//! REAPER service surface — declared as [`architect::Services`].
//!
//! `impl Services for Reaper` says "Reaper provides this canonical
//! bundle." Same pattern as every per-service trait impl (`impl
//! Transport for Reaper`, `impl Markers for Reaper`, …), now lifted
//! one level to "impl `Services` for Reaper exposes the whole
//! surface."
//!
//! ```ignore
//! use architect::Services;
//! use daw_reaper::Reaper;
//!
//! // Default mount:
//! let router = Reaper.into_router();
//!
//! // With overrides — last merge wins on duplicate method_id:
//! let router = Reaper::layers()
//!     .merge(fx_chains_mock::mock())            // overrides default fx_chains
//!     .merge(dock_host::layer(dock_host))       // bolt-on
//!     .provide(Reaper);
//!
//! // Sub-bundles:
//! let timeline = architect::layers![transport::Service, marker::Service, region::Service];
//! let routing  = architect::layers![project::Service, routing::Service, track::Service];
//! let router   = timeline.merge(routing).provide(Reaper);
//! ```
//!
//! Adding a service is one line in the [`architect::layers!`] list.
//! No bounds, no where clause. Other backends (Pro Tools, Logic, mock)
//! each implement [`architect::Services`] with their own bundle.

use architect::{Layer, Services, layers};
use daw_proto::{
    action_registry, audio_engine, automation, batch, dawfile_service, diagnostics, event_bus,
    ext_state, fx, fx_chains, fx_params, health, input, item, live_midi, marker, midi, peak,
    plugin_loader, project, region, resource, routing, screenset, song_files, take, tempo_map,
    toolbar, track, transport, transport_sync, window_geometry, window_manager,
};

use crate::Reaper;

impl Services for Reaper {
    fn layers() -> impl Layer<Reaper> {
        layers![
            // Sync-method services (architect::rpc)
            transport::Service,
            project::Service,
            marker::Service,
            region::Service,
            tempo_map::Service,
            audio_engine::Service,
            midi::Service,
            fx::Service,
            fx_chains::Service,
            fx_params::Service,
            track::Service,
            routing::Service,
            live_midi::Service,
            ext_state::Service,
            health::Service,
            item::Service,
            take::Service,
            action_registry::Service,
            input::Service,
            toolbar::Service,
            screenset::Service,
            // Resource paths + colour-theme loading. Mounted late: the trait
            // existed and compiled for a long time with no consumer, so
            // nothing noticed it was never in this list — an unmounted
            // service answers with a schema mismatch, not "no such method".
            resource::Service,
            dawfile_service::Service,
            window_geometry::Service,
            window_manager::Service,
            plugin_loader::Service,
            automation::Service,
            batch::Service,
            diagnostics::Service,
            peak::Service,
            transport_sync::Service,
            // A song's folder, for a peer streaming it in.
            song_files::Service,
            // `#[subscribe]` stream siblings — served from the central
            // event hub's PubSub hubs (see `event_hub.rs` + each
            // domain's StreamSource impl). The event-bus base service
            // is empty post-port; only its stream sibling is mounted.
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
