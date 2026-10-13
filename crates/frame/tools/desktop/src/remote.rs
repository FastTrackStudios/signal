//! A live engine across a WebSocket (`--engine`), driven from the UI
//! thread without ever blocking it.
//!
//! A background thread runs a tokio runtime that owns the vox clients (a
//! call client and, on its own link, a stream client). The UI thread
//! queues [`EngineCall`]s ([`Remote::send`]) and drains [`Event`]s each
//! frame ([`Remote::drain`]). The link is supervised: when it drops, the
//! UI gets [`Event::Disconnected`] once, the thread reconnects under an
//! exponential [`Backoff`], and on success [`Event::Connected`] carries a
//! fresh catalog and values to resync from.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use architect::Backoff;
use frame::surface::proto::{Catalog, ParamValue};
use frame::surface::runtime::EngineCall;
use frame::surface::runtime::sync::{self, Engine, EngineError, Incoming, PumpEnd, Telemetry};
use tokio::sync::mpsc as tmpsc;

/// What the UI thread receives.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// (Re)connected: the engine's catalog and every current value.
    Connected {
        catalog: Catalog,
        values: Vec<ParamValue>,
    },
    /// A telemetry frame or a parameter change.
    Engine(Incoming),
    /// The link dropped (or never came up); reconnecting.
    Disconnected(String),
}

impl From<Incoming> for Event {
    fn from(i: Incoming) -> Self {
        Self::Engine(i)
    }
}

enum Cmd {
    Calls(Vec<EngineCall>),
    /// Read every value, ordered after the calls queued before it.
    Values(mpsc::Sender<Result<Vec<ParamValue>, EngineError>>),
}

pub struct Remote {
    url: String,
    cmds: tmpsc::UnboundedSender<Cmd>,
    events: mpsc::Receiver<Event>,
    /// The latest connection's catalog and values (for `sync::connect`).
    snapshot: Option<(Catalog, Vec<ParamValue>)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Remote {
    /// Start connecting to `url` (instance `scope`, e.g. `guitar`) in the
    /// background. Returns immediately.
    ///
    /// # Errors
    /// When the background runtime can't start.
    pub fn start(url: &str, scope: Option<&str>) -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = tmpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::channel();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("tokio: {e}"))?;
        let (u, s) = (url.to_owned(), scope.map(str::to_owned));
        let thread = std::thread::Builder::new()
            .name("frame-engine".into())
            .spawn(move || rt.block_on(supervise(u, s, cmd_rx, event_tx)))
            .map_err(|e| format!("engine thread: {e}"))?;
        Ok(Self {
            url: url.to_owned(),
            cmds: cmd_tx,
            events: event_rx,
            snapshot: None,
            thread: Some(thread),
        })
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Block until the first connection, up to `timeout`.
    ///
    /// # Errors
    /// The last connection error when `timeout` passes first.
    pub fn wait_connected(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut last = format!("no answer from {}", self.url);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(Event::Connected { catalog, values }) => {
                    self.snapshot = Some((catalog, values));
                    return Ok(());
                }
                Ok(Event::Disconnected(e)) => last = e,
                Ok(Event::Engine(_)) => {}
                Err(_) => return Err(format!("can't reach engine at {}: {last}", self.url)),
            }
        }
    }

    /// Queue calls; never blocks.
    pub fn send(&self, calls: Vec<EngineCall>) {
        if !calls.is_empty() {
            let _ = self.cmds.send(Cmd::Calls(calls));
        }
    }

    /// Every event since the last drain, without blocking. `Connected`
    /// also refreshes the snapshot [`sync::connect`] builds from.
    pub fn drain(&mut self) -> Vec<Event> {
        let events: Vec<Event> = self.events.try_iter().collect();
        for e in &events {
            if let Event::Connected { catalog, values } = e {
                self.snapshot = Some((catalog.clone(), values.clone()));
            }
        }
        events
    }

    /// Read every value from the engine, after all queued calls
    /// (blocks up to `timeout`; for scripted checks, not the frame loop).
    ///
    /// # Errors
    /// Disconnected, or no answer within `timeout`.
    pub fn fetch_values(&self, timeout: Duration) -> Result<Vec<ParamValue>, String> {
        let (tx, rx) = mpsc::channel();
        self.cmds
            .send(Cmd::Values(tx))
            .map_err(|_| "engine thread gone".to_string())?;
        rx.recv_timeout(timeout)
            .map_err(|_| "engine didn't answer".to_string())?
            .map_err(|e| e.0)
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        // Closing the command channel stops the thread.
        let (dead, _) = tmpsc::unbounded_channel();
        self.cmds = dead;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The UI side as an [`Engine`]: reads come from the connection snapshot,
/// calls are queued. So `sync::connect` / `sync::send` work unchanged.
impl Engine for Remote {
    async fn catalog(&self) -> Result<Catalog, EngineError> {
        self.snapshot
            .as_ref()
            .map(|s| s.0.clone())
            .ok_or_else(|| EngineError("not connected".into()))
    }

    async fn values(&self) -> Result<Vec<ParamValue>, EngineError> {
        self.snapshot
            .as_ref()
            .map(|s| s.1.clone())
            .ok_or_else(|| EngineError("not connected".into()))
    }

    async fn call(&self, call: EngineCall) -> Result<(), EngineError> {
        self.cmds
            .send(Cmd::Calls(vec![call]))
            .map_err(|_| EngineError("engine thread gone".into()))
    }
}

/// How one connection ended.
enum Ended {
    /// The UI dropped its end: stop for good.
    UiGone,
    /// Lost (or never made) the link.
    Lost { connected: bool, reason: String },
}

async fn supervise(
    url: String,
    scope: Option<String>,
    mut cmds: tmpsc::UnboundedReceiver<Cmd>,
    events: mpsc::Sender<Event>,
) {
    let mut backoff = Backoff::default();
    // One `Disconnected` per outage, not one per retry.
    let mut reported = false;
    loop {
        match session(&url, scope.as_deref(), &mut cmds, &events).await {
            Ended::UiGone => return,
            Ended::Lost { connected, reason } => {
                if connected {
                    backoff.reset();
                    reported = false;
                }
                if !reported {
                    let what = if connected {
                        "engine disconnected"
                    } else {
                        "engine unreachable"
                    };
                    eprintln!("frame: {what} ({reason}); retrying");
                    if events.send(Event::Disconnected(reason)).is_err() {
                        return;
                    }
                    reported = true;
                }
            }
        }
        // Wait out the backoff; calls made meanwhile can't reach the
        // engine and are dropped (the reconnect resyncs every value).
        let sleep = tokio::time::sleep(backoff.next_delay());
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                () = &mut sleep => break,
                cmd = cmds.recv() => match cmd {
                    None => return,
                    Some(Cmd::Values(reply)) => { let _ = reply.send(Err(EngineError("engine disconnected".into()))); }
                    Some(Cmd::Calls(_)) => {}
                },
            }
        }
    }
}

async fn session(
    url: &str,
    scope: Option<&str>,
    cmds: &mut tmpsc::UnboundedReceiver<Cmd>,
    events: &mpsc::Sender<Event>,
) -> Ended {
    let lost = |e: EngineError| Ended::Lost {
        connected: false,
        reason: e.0,
    };
    let client = match sync::connect_ws(url, scope).await {
        Ok(c) => c,
        Err(e) => return lost(e),
    };
    let stream = match sync::connect_ws_stream(url, scope).await {
        Ok(c) => c,
        Err(e) => return lost(e),
    };
    let (catalog, values) = match (
        Engine::catalog(&client).await,
        Engine::values(&client).await,
    ) {
        (Ok(c), Ok(v)) => (c, v),
        (Err(e), _) | (_, Err(e)) => return lost(e),
    };
    eprintln!(
        "frame: connected to engine `{}` at {url} ({} params)",
        catalog.id,
        catalog.params.len()
    );
    if events.send(Event::Connected { catalog, values }).is_err() {
        return Ended::UiGone;
    }

    let telemetry = Telemetry::new(events.clone());
    let calls = async {
        while let Some(cmd) = cmds.recv().await {
            match cmd {
                Cmd::Calls(calls) => {
                    // One by one: a refused call (unknown param) mustn't
                    // swallow the rest. Link loss shows up as the pump ending.
                    for call in calls {
                        if let Err(e) = Engine::call(&client, call).await {
                            eprintln!("frame: engine refused: {e}");
                        }
                    }
                }
                Cmd::Values(reply) => {
                    let _ = reply.send(Engine::values(&client).await);
                }
            }
        }
    };
    tokio::select! {
        end = telemetry.pump(&stream) => match end {
            PumpEnd::Closed => Ended::UiGone,
            PumpEnd::Lost(e) => Ended::Lost { connected: true, reason: e.0 },
        },
        () = calls => Ended::UiGone,
        () = client.caller.closed() => Ended::Lost { connected: true, reason: "call link closed".into() },
    }
}

#[cfg(test)]
pub mod testing {
    use architect::{LayerRouter, Services as _};
    use frame::surface::memory::Memory;
    use frame::surface::runtime::sync;

    /// The mock engine served over a WebSocket on its own thread; `kill`
    /// drops every socket at once, like a crashed engine.
    pub struct Server {
        pub url: String,
        pub engine: Memory,
        stop: Option<tokio::sync::oneshot::Sender<()>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Server {
        pub fn start(engine: Memory, scope: Option<&str>) -> Self {
            Self::start_on(
                engine,
                scope,
                std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
            )
        }

        fn start_on(engine: Memory, scope: Option<&str>, listener: std::net::TcpListener) -> Self {
            listener.set_nonblocking(true).unwrap();
            let url = format!("ws://{}/vox", listener.local_addr().unwrap());
            let router = match scope {
                Some(s) => LayerRouter::new().merge_router_scoped(s, engine.clone().into_router()),
                None => engine.clone().into_router(),
            };
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
                engine,
                stop: Some(stop),
                thread: Some(thread),
            }
        }

        /// Serve again on the same port (after `kill`).
        pub fn restart(&mut self, scope: Option<&str>) {
            let addr = self
                .url
                .trim_start_matches("ws://")
                .trim_end_matches("/vox")
                .to_owned();
            let listener = std::net::TcpListener::bind(addr).unwrap();
            *self = Self::start_on(self.engine.clone(), scope, listener);
        }

        pub fn kill(&mut self) {
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
}

#[cfg(test)]
mod tests {
    use super::testing::Server;
    use super::*;
    use frame::surface::memory::Memory;
    use frame::surface::proto::{EditPhase, ParamAddr};

    /// Drain until `want` matches, ticking the served engine.
    fn wait_for(remote: &mut Remote, engine: &Memory, want: impl Fn(&Event) -> bool) -> Event {
        for i in 0..500 {
            engine.tick(f64::from(i) * 0.01);
            if let Some(e) = remote.drain().into_iter().find(|e| want(e)) {
                return e;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no matching event within 5 s");
    }

    #[test]
    fn connects_streams_sends_and_reconnects() {
        let mut server = Server::start(Memory::demo(), Some("guitar"));
        let mut remote = Remote::start(&server.url, Some("guitar")).unwrap();
        remote.wait_connected(Duration::from_secs(5)).unwrap();
        let catalog = pollster::block_on(Engine::catalog(&remote)).unwrap();
        assert_eq!(catalog.id, "demo.guitar");

        wait_for(&mut remote, &server.engine, |e| {
            matches!(e, Event::Engine(Incoming::Frame(_)))
        });

        let addr = ParamAddr::new("delay/mix");
        remote.send(vec![EngineCall::SetParam {
            addr: addr.clone(),
            value: 0.5,
            phase: EditPhase::End,
        }]);
        let values = remote.fetch_values(Duration::from_secs(5)).unwrap();
        assert!(
            values
                .iter()
                .any(|v| v.addr == addr && (v.value - 0.5).abs() < 1e-9),
            "calls land before the read"
        );
        assert_eq!(server.engine.value(&addr), Some(0.5));
        wait_for(
            &mut remote,
            &server.engine,
            |e| matches!(e, Event::Engine(Incoming::Param(p)) if p.addr == addr),
        );

        server.kill();
        wait_for(&mut remote, &server.engine, |e| {
            matches!(e, Event::Disconnected(_))
        });
        assert!(
            remote.fetch_values(Duration::from_secs(5)).is_err(),
            "reads fail while disconnected"
        );

        server.restart(Some("guitar"));
        let e = wait_for(&mut remote, &server.engine, |e| {
            matches!(e, Event::Connected { .. })
        });
        let Event::Connected { values, .. } = e else {
            unreachable!()
        };
        assert!(
            values
                .iter()
                .any(|v| v.addr == addr && (v.value - 0.5).abs() < 1e-9),
            "resynced"
        );
    }

    #[test]
    fn unreachable_engine_times_out_with_the_reason() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}/vox", listener.local_addr().unwrap());
        drop(listener);
        let mut remote = Remote::start(&url, None).unwrap();
        let err = remote
            .wait_connected(Duration::from_millis(600))
            .unwrap_err();
        assert!(err.contains("can't reach engine"), "{err}");
    }
}
