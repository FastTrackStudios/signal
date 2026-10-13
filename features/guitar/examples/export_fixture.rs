//! Dump what a remote reads from the rig — the performance model, the
//! library (profiles, songs, setlists), the compositions (presets, Core,
//! Time, block presets), the patches and the live chain — as one JSON file,
//! for prototyping a UI on the real data without a running engine.
//!
//! ```sh
//! cargo run --release -p signal-guitar --example export_fixture -- out.json
//! ```

use architect::rig::RigBackend as _;
use architect::{LocalServer, Scope};
use facet::Facet;
use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::rig::RigClient;
use signal_guitar::proto::{CompositionModel, LibraryModel, PatchInfo, PerformanceModel, PresetInfo};
use signal_proto::live_node::LiveNode;

#[derive(Facet)]
struct Fixture {
    perf: PerformanceModel,
    library: LibraryModel,
    compositions: CompositionModel,
    patches: Vec<PatchInfo>,
    presets: Vec<PresetInfo>,
    nodes: Vec<LiveNode>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    // SAFETY: single-threaded, before the rig or any thread is created.
    unsafe {
        if std::env::var_os("SIGNAL_RIG_SILENT").is_none() {
            std::env::set_var("SIGNAL_RIG_SILENT", "1");
        }
    }
    let out = std::env::args().nth(1).unwrap_or_else(|| "fixture.json".to_string());
    let backend = GuitarRigBackend::new();
    backend.open_blocking();
    let server = LocalServer::serve(backend.router(), Scope::new());
    let rig: RigClient = server.establish().await.expect("rig client");
    let fixture = Fixture {
        perf: rig.perf().await.expect("perf"),
        library: rig.library().await.expect("library"),
        compositions: rig.compositions().await.expect("compositions"),
        patches: rig.patches().await.expect("patches"),
        presets: rig.presets().await.expect("presets"),
        nodes: rig.nodes().await.expect("nodes"),
    };
    std::fs::write(&out, facet_json::to_string(&fixture).expect("serialize fixture")).expect("write fixture");
    tracing::warn!(path = %out, "fixture written");
}
