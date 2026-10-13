//! The engine as a Remote target: a daw-standalone engine served over the
//! WebSocket and over iroh, dialed with `remote::connect_engine_daw_from`,
//! driven through the daw facade the way a Session UI attached to it would.
//!
//! The iroh half binds loopback-only endpoints with relays and address
//! lookup disabled (`presets::Minimal`) and dials by full address, so it
//! needs no network.

use std::time::Duration;

use architect::iroh_link::{self, VOX_ALPN, iroh};
use daw_standalone::project_loader::load_rpp_text;
use daw_standalone::sync::Standalone;
use session::{
    SetlistServiceClient, SetlistServiceImpl, serve_setlist_service,
    setlist_service_service_descriptor,
};

use super::{EngineArgs, OpenTarget, daw_facade_router, serve_iroh, serve_ws};
use crate::remote::{EngineAddr, connect_engine_daw_from};

/// A three-track song with two markers, 120 bpm.
const FIXTURE_RPP: &str = r#"<REAPER_PROJECT 0.1 "7.0/test" 1700000000
  SAMPLERATE 48000 0 0
  TEMPO 120 4 4 0
  MARKER 1 0 Intro 0 0 1 B {00000000-0000-0000-0000-00000000B001} 0
  MARKER 2 4 Verse 0 0 1 B {00000000-0000-0000-0000-00000000B002} 0
  <TRACK {BBBBBBBB-0001-0000-0000-000000000000}
    NAME "Drums"
    TRACKID {BBBBBBBB-0001-0000-0000-000000000000}
    NCHAN 2
  >
  <TRACK {BBBBBBBB-0002-0000-0000-000000000000}
    NAME "Bass"
    TRACKID {BBBBBBBB-0002-0000-0000-000000000000}
    NCHAN 2
  >
  <TRACK {BBBBBBBB-0003-0000-0000-000000000000}
    NAME "Keys"
    TRACKID {BBBBBBBB-0003-0000-0000-000000000000}
    NCHAN 2
  >
>
"#;

/// vox's debug-build encode recurses deeply; tokio's default 2 MiB worker
/// stacks overflow on it (see `session_engine::bootstrap_blocking`).
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("test runtime")
}

/// A Standalone holding the fixture as its current project, and the router
/// an engine in Live Mode serves over it: the daw facade + the setlist.
fn fixture_engine() -> (Standalone, daw::LayerRouter) {
    let standalone = Standalone::new();
    let loaded = load_rpp_text(&standalone, "Fixture", "/fixture/Fixture.rpp", FIXTURE_RPP)
        .expect("fixture parses");
    assert_eq!(loaded.track_count, 3, "fixture tracks");
    standalone.set_current_project(&loaded.project_guid);
    let setlist = SetlistServiceImpl::with_daw(standalone.clone());
    let router = daw_facade_router(&standalone).with(
        setlist_service_service_descriptor(),
        serve_setlist_service(setlist),
    );
    (standalone, router)
}

/// Drive the facade the way a Session UI attached as a Remote does.
async fn exercise(engine: &crate::remote::EngineDaw) {
    let daw = &engine.daw;

    let projects = daw.projects().await.expect("list projects");
    assert_eq!(projects.len(), 1, "one open project");

    let project = daw.current_project().await.expect("current project");
    let info = project.info().await.expect("project info");
    assert_eq!(info.name, "Fixture");

    let tracks = project.tracks().all().await.expect("tracks");
    let names: Vec<&str> = tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Drums", "Bass", "Keys"]);

    let markers = project.markers().all().await.expect("markers");
    assert_eq!(markers.len(), 2, "markers: {markers:?}");

    // Play: the engine has no audio device here, so its soft clock drives
    // the playhead — which must move, as seen from the far side.
    let transport = project.transport();
    transport.set_position(0.0).await.expect("locate");
    transport.play().await.expect("play");
    assert!(transport.is_playing().await.expect("play state"));
    let mut position = 0.0;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        position = transport.get_position().await.expect("position");
        if position > 0.2 {
            break;
        }
    }
    assert!(position > 0.2, "playhead did not advance: {position}");
    transport.stop().await.expect("stop");
    assert!(!transport.is_playing().await.expect("play state"));

    // The setlist rides the same connection: same caller, other service.
    let setlist = SetlistServiceClient::new(daw.caller().clone());
    setlist
        .songs()
        .await
        .expect("the setlist service answers on the same connection");
}

#[test]
fn daw_facade_over_websocket() {
    runtime().block_on(async {
        let (_standalone, router) = fixture_engine();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let server = tokio::spawn(serve_ws(listener, router));

        // A bare ws://host:port gets its /vox path.
        let addr = EngineAddr::parse(&format!("ws://127.0.0.1:{port}")).expect("ws addr");
        let engine = connect_engine_daw_from(None, &addr)
            .await
            .expect("connect over ws");
        exercise(&engine).await;

        drop(engine);
        server.abort();
    });
}

async fn bind_loopback() -> iroh::Endpoint {
    iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .alpns(vec![VOX_ALPN.to_vec()])
        .bind_addr("127.0.0.1:0")
        .expect("bind_addr")
        .bind()
        .await
        .expect("bind endpoint")
}

#[test]
fn daw_facade_over_iroh() {
    runtime().block_on(async {
        let (_standalone, router) = fixture_engine();
        let server = bind_loopback().await;
        let client = bind_loopback().await;
        let server_addr = iroh::EndpointAddr::from_parts(
            server.id(),
            server
                .bound_sockets()
                .into_iter()
                .map(iroh::TransportAddr::Ip),
        );
        let serving = server.clone();
        let serve = tokio::spawn(async move { serve_iroh(&serving, router).await });

        let engine = connect_engine_daw_from(Some(&client), &EngineAddr::Iroh(server_addr))
            .await
            .expect("connect over iroh");
        exercise(&engine).await;

        drop(engine);
        server.close().await;
        serve.await.expect("iroh serve task");
    });
}

#[test]
fn engine_addr_parses_what_people_paste() {
    let key = iroh::SecretKey::generate();
    let id = key.public();

    let connect = EngineAddr::connect_string(&id);
    assert!(connect.starts_with("fts-engine:"));
    for text in [connect.clone(), id.to_string(), format!("  {connect}\n")] {
        match EngineAddr::parse(&text) {
            Ok(EngineAddr::Iroh(addr)) => assert_eq!(addr.id, id),
            other => panic!("{text:?} → {other:?}"),
        }
    }

    match EngineAddr::parse("ws://studio.local:4040/vox") {
        Ok(EngineAddr::Ws(url)) => assert_eq!(url, "ws://studio.local:4040/vox"),
        other => panic!("{other:?}"),
    }
    match EngineAddr::parse("wss://studio.example") {
        Ok(EngineAddr::Ws(url)) => assert_eq!(url, "wss://studio.example/vox"),
        other => panic!("{other:?}"),
    }
    assert!(EngineAddr::parse("fts-engine:not-an-id").is_err());
    assert!(EngineAddr::parse("http://studio.local").is_err());
}

#[test]
fn engine_identity_is_stable_across_restarts() {
    let dir = std::env::temp_dir().join(format!("session-engine-key-{}", std::process::id()));
    let path = dir.join("session-engine-iroh.key");
    let first = iroh_link::load_or_create_secret_key(&path).expect("create key");
    let again = iroh_link::load_or_create_secret_key(&path).expect("reload key");
    assert_eq!(
        first.public(),
        again.public(),
        "same key file, same endpoint id"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn engine_args() {
    let args = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
    assert_eq!(
        EngineArgs::parse(&args("session-desktop --engine")),
        EngineArgs::default()
    );
    assert_eq!(
        EngineArgs::parse(&args(
            "session-desktop --engine --port 5050 --project /s/Song.RPP --iroh-key /k"
        )),
        EngineArgs {
            port: Some(5050),
            open: Some(OpenTarget::Project("/s/Song.RPP".into())),
            no_iroh: false,
            iroh_key: Some("/k".into()),
        }
    );
    assert_eq!(
        EngineArgs::parse(&args(
            "session-desktop --engine --setlist /l/Sunday.md --no-iroh"
        )),
        EngineArgs {
            port: None,
            open: Some(OpenTarget::Setlist("/l/Sunday.md".into())),
            no_iroh: true,
            iroh_key: None,
        }
    );
}
