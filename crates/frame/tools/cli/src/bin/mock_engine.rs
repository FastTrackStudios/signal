//! Serve the in-memory demo engine over a WebSocket, shaped like signal's
//! endpoint, so `frame --engine` / `frame-mcp --engine` can be tried
//! without a rig.
//!
//! ```text
//! mock_engine [--port 4040] [--rig guitar|none]
//! ```
//! Serves `ws://127.0.0.1:PORT/vox`, the engine mounted under instance
//! scope `--rig` (default `guitar`), publishing synthetic telemetry at 30 Hz.

use std::time::{Duration, Instant};

use architect::{LayerRouter, Services as _};
use surface_memory::Memory;
use surface_runtime::sync::serve_ws;

fn main() {
    let mut port = 4040u16;
    let mut rig = Some("guitar".to_string());
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => port = args.next().and_then(|p| p.parse().ok()).unwrap_or(port),
            "--rig" => rig = args.next().filter(|r| r != "none"),
            other => {
                eprintln!("mock_engine: unknown argument {other}\nusage: mock_engine [--port 4040] [--rig guitar|none]");
                std::process::exit(2);
            }
        }
    }
    let engine = Memory::demo();
    let router = match &rig {
        Some(r) => LayerRouter::new().merge_router_scoped(r, engine.clone().into_router()),
        None => engine.clone().into_router(),
    };
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async move {
        let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("mock_engine: bind 127.0.0.1:{port}: {e}");
                std::process::exit(1);
            }
        };
        eprintln!(
            "mock_engine: serving the demo engine at ws://127.0.0.1:{port}/vox{}",
            rig.as_ref().map_or(String::new(), |r| format!(" (rig {r})"))
        );
        let start = Instant::now();
        let ticks = async {
            let mut every = tokio::time::interval(Duration::from_millis(33));
            loop {
                every.tick().await;
                engine.tick(start.elapsed().as_secs_f64());
            }
        };
        tokio::select! {
            () = serve_ws(listener, router) => {}
            () = ticks => {}
        }
    });
}
