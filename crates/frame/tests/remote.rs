//! The mock engine served over a real WebSocket, driven the way a remote
//! surface drives signal: a `SurfaceEngineClient` for calls, a
//! `SurfaceEngineStreamClient` on its own link for telemetry and param
//! changes, pumped by `sync::Telemetry` into a plain channel.
#![cfg(feature = "remote")]

use std::sync::mpsc::Receiver;
use std::time::Duration;

use architect::{LayerRouter, Services as _};
use frame::surface::memory::Memory;
use frame::surface::proto::{EditPhase, EntryId, ParamAddr};
use frame::surface::runtime::sync::{self, Engine, Incoming, PumpEnd, Telemetry};
use frame::surface::runtime::{EngineCall, Surface};

/// An engine served over a WebSocket on its own thread and runtime, so
/// [`Server::kill`] can drop every socket at once, like a crashed process.
struct Server {
    url: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn start(router: LayerRouter) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("ws://{}/vox", listener.local_addr().unwrap());
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                tokio::select! {
                    () = sync::serve_ws(listener, router) => {}
                    _ = stopped => {}
                }
            });
            rt.shutdown_background();
        });
        Self {
            url,
            stop: Some(stop),
            thread: Some(thread),
        }
    }

    fn kill(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Wait (ticking the engine) until the channel yields something matching.
async fn wait_for(
    engine: &Memory,
    rx: &Receiver<Incoming>,
    want: impl Fn(&Incoming) -> bool,
) -> Incoming {
    for i in 0..300 {
        engine.tick(f64::from(i) * 0.01);
        tokio::time::sleep(Duration::from_millis(10)).await;
        if let Some(hit) = rx.try_iter().find(|m| want(m)) {
            return hit;
        }
    }
    panic!("nothing matching arrived within 3 s");
}

#[tokio::test]
async fn telemetry_and_param_changes_arrive_over_a_websocket() {
    let engine = Memory::demo();
    let mut server = Server::start(
        LayerRouter::new().merge_router_scoped("guitar", engine.clone().into_router()),
    );
    let url = server.url.clone();

    let client = sync::connect_ws(&url, Some("guitar")).await.unwrap();
    let stream = sync::connect_ws_stream(&url, Some("guitar")).await.unwrap();
    assert_eq!(client.catalog().await.unwrap().id, "demo.guitar");

    let (telemetry, rx) = Telemetry::channel();
    let pump = tokio::spawn(async move { telemetry.pump(&stream).await });

    // Frames published by the engine arrive as `Incoming::Frame`.
    let got = wait_for(
        &engine,
        &rx,
        |m| matches!(m, Incoming::Frame(f) if f.id == EntryId::new("out_level")),
    )
    .await;
    let Incoming::Frame(frame) = got else {
        unreachable!()
    };
    assert_eq!(frame.values.len(), 2, "stereo out_level");

    // A call through the call client echoes back as `Incoming::Param`.
    let addr = ParamAddr::new("delay/mix");
    Engine::call(
        &client,
        EngineCall::SetParam {
            addr: addr.clone(),
            value: 0.5,
            phase: EditPhase::End,
        },
    )
    .await
    .unwrap();
    let got = wait_for(
        &engine,
        &rx,
        |m| matches!(m, Incoming::Param(p) if p.addr == addr),
    )
    .await;
    assert_eq!(
        got,
        Incoming::Param(frame::surface::proto::ParamValue {
            addr: addr.clone(),
            value: 0.5
        })
    );

    // …and feeds a runtime built from the remote engine.
    let surface = Surface::from_scene(frame::document::proto::Scene::new("S"));
    let mut rt = sync::connect(&client, surface).await.unwrap();
    assert!(
        rt.values()
            .iter()
            .any(|v| v.addr == addr && (v.value - 0.5).abs() < 1e-9)
    );
    Incoming::Frame(frame.clone()).apply(&mut rt, 0.0);

    // The server going away ends the pump with `Lost`, not a hang.
    server.kill();
    let end = tokio::time::timeout(Duration::from_secs(5), pump)
        .await
        .expect("pump noticed the drop")
        .unwrap();
    assert!(matches!(end, PumpEnd::Lost(_)), "{end:?}");
}

#[tokio::test]
async fn pump_stops_when_the_receiver_is_gone() {
    let engine = Memory::demo();
    let server = Server::start(engine.clone().into_router());
    let stream = sync::connect_ws_stream(&server.url, None).await.unwrap();
    let (telemetry, rx) = Telemetry::channel();
    let pump = tokio::spawn(async move { telemetry.pump(&stream).await });
    wait_for(&engine, &rx, |m| matches!(m, Incoming::Frame(_))).await;
    drop(rx);
    let mut end = None;
    for i in 0..300 {
        engine.tick(f64::from(i));
        tokio::time::sleep(Duration::from_millis(10)).await;
        if pump.is_finished() {
            end = Some(pump.await.unwrap());
            break;
        }
    }
    assert_eq!(end, Some(PumpEnd::Closed));
    drop(server);
}

#[tokio::test]
async fn connecting_to_nothing_fails_fast() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/vox", listener.local_addr().unwrap());
    drop(listener);
    assert!(sync::connect_ws(&url, None).await.is_err());
    assert!(sync::connect_ws_stream(&url, None).await.is_err());
}
