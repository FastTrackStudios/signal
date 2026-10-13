//! The async bridge between a [`Runtime`](crate::Runtime) and an engine.
//!
//! [`Engine`] is what the runtime needs from an engine. It's implemented for
//! [`Local`] (any in-process [`SurfaceEngine`] backend — the mock, or a rig
//! embedded in the same process) and for the vox [`SurfaceEngineClient`]
//! (an engine across a WebSocket / iroh link), so the same surface runs
//! embedded in a plugin, in the desktop app, or remotely on an iPad.

use surface_proto::engine::SurfaceEngine;
#[cfg(feature = "vox")]
use surface_proto::engine::SurfaceEngineClient;
use surface_proto::{Catalog, FeedInfo, FeedRequest, ParamValue, TextValue};

use crate::runtime::{EngineCall, Runtime, Surface};

/// Why talking to an engine failed (refused, or the link dropped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError(pub String);

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EngineError {}

/// An engine as the runtime drives it.
#[allow(async_fn_in_trait)]
pub trait Engine {
    async fn catalog(&self) -> Result<Catalog, EngineError>;
    async fn values(&self) -> Result<Vec<ParamValue>, EngineError>;
    async fn call(&self, call: EngineCall) -> Result<(), EngineError>;
    /// Ask for feeds shaped for the surface (a plot's bins, a history's
    /// length). An engine that can't reshape keeps its catalog's shapes.
    async fn request_feeds(&self, requests: Vec<FeedRequest>) -> Result<Vec<FeedInfo>, EngineError> {
        let _ = requests;
        Ok(Vec::new())
    }
    /// Text params' values (names). An engine without text keeps none.
    async fn texts(&self) -> Result<Vec<TextValue>, EngineError> {
        Ok(Vec::new())
    }
}

/// An in-process backend.
pub struct Local<'a, E>(pub &'a E);

impl<E: SurfaceEngine> Engine for Local<'_, E> {
    async fn catalog(&self) -> Result<Catalog, EngineError> {
        Ok(self.0.catalog())
    }

    async fn values(&self) -> Result<Vec<ParamValue>, EngineError> {
        Ok(self.0.values())
    }

    async fn call(&self, call: EngineCall) -> Result<(), EngineError> {
        let r = match call {
            EngineCall::SetParam { addr, value, phase } => self.0.set_param(addr, value, phase),
            EngineCall::Invoke { action } => self.0.invoke(action),
            EngineCall::Select { collection, index } => self.0.select(collection, index),
            EngineCall::SetText { addr, text } => self.0.set_text(addr, text),
        };
        r.map_err(|e| EngineError(e.to_string()))
    }

    async fn request_feeds(&self, requests: Vec<FeedRequest>) -> Result<Vec<FeedInfo>, EngineError> {
        self.0.request_feeds(requests).map_err(|e| EngineError(e.to_string()))
    }

    async fn texts(&self) -> Result<Vec<TextValue>, EngineError> {
        Ok(self.0.texts())
    }
}

#[cfg(feature = "vox")]
impl Engine for SurfaceEngineClient {
    async fn catalog(&self) -> Result<Catalog, EngineError> {
        SurfaceEngineClient::catalog(self).await.map_err(|e| EngineError(format!("{e:?}")))
    }

    async fn values(&self) -> Result<Vec<ParamValue>, EngineError> {
        SurfaceEngineClient::values(self).await.map_err(|e| EngineError(format!("{e:?}")))
    }

    async fn call(&self, call: EngineCall) -> Result<(), EngineError> {
        let r = match call {
            EngineCall::SetParam { addr, value, phase } => self.set_param(addr, value, phase).await,
            EngineCall::Invoke { action } => self.invoke(action).await,
            EngineCall::Select { collection, index } => self.select(collection, index).await,
            EngineCall::SetText { addr, text } => self.set_text(addr, text).await,
        };
        r.map_err(|e| EngineError(format!("{e:?}")))
    }

    async fn request_feeds(&self, requests: Vec<FeedRequest>) -> Result<Vec<FeedInfo>, EngineError> {
        SurfaceEngineClient::request_feeds(self, requests).await.map_err(|e| EngineError(format!("{e:?}")))
    }

    async fn texts(&self) -> Result<Vec<TextValue>, EngineError> {
        SurfaceEngineClient::texts(self).await.map_err(|e| EngineError(format!("{e:?}")))
    }
}

/// Connect to an engine served over a WebSocket (e.g. signal's
/// `ws://127.0.0.1:4040/vox`), optionally picking a rig by instance scope
/// (`"guitar"`), and return its `SurfaceEngine` client.
///
/// # Errors
/// When the socket can't be opened or the vox handshake fails.
#[cfg(feature = "remote")]
pub async fn connect_ws(url: &str, scope: Option<&str>) -> Result<SurfaceEngineClient, EngineError> {
    let link = vox_websocket::WsLink::connect(url).await.map_err(|e| EngineError(format!("ws connect {url}: {e:?}")))?;
    let client: SurfaceEngineClient =
        vox_core::initiator_on(link).establish().await.map_err(|e| EngineError(format!("vox handshake: {e:?}")))?;
    Ok(match scope {
        Some(s) => architect::scope_client!(client, s.to_string()),
        None => client,
    })
}

/// Connect to the stream side of an engine served over a WebSocket — the
/// `param_changes` / `telemetry` subscriptions. A vox link carries one
/// service, so this opens its own socket next to [`connect_ws`]'s.
///
/// # Errors
/// When the socket can't be opened or the vox handshake fails.
#[cfg(feature = "remote")]
pub async fn connect_ws_stream(url: &str, scope: Option<&str>) -> Result<SurfaceEngineStreamClient, EngineError> {
    let link = vox_websocket::WsLink::connect(url).await.map_err(|e| EngineError(format!("ws connect {url}: {e:?}")))?;
    let client: SurfaceEngineStreamClient =
        vox_core::initiator_on(link).establish().await.map_err(|e| EngineError(format!("vox handshake: {e:?}")))?;
    Ok(match scope {
        Some(s) => architect::scope_client!(client, s.to_string()),
        None => client,
    })
}

/// Serve `router` (e.g. `Memory::demo().into_router()`, optionally
/// `merge_router_scoped`) as a WebSocket vox endpoint on `listener` —
/// the server half of [`connect_ws`] / [`connect_ws_stream`], for tests and
/// for serving an in-process engine to remote surfaces. Runs until the
/// listener fails; connections live on the current tokio runtime.
#[cfg(feature = "remote")]
pub async fn serve_ws(listener: tokio::net::TcpListener, router: architect::LayerRouter) {
    while let Ok((tcp, _)) = listener.accept().await {
        let router = router.clone();
        tokio::spawn(async move {
            let Ok(link) = vox_websocket::WsLink::server(tcp).await else { return };
            if let Ok(connection) =
                vox_core::acceptor_on(link).on_lane(router.acceptor()).establish_connection().await
            {
                let _connection = connection;
                std::future::pending::<()>().await;
            }
        });
    }
}

#[cfg(feature = "remote")]
use surface_proto::StreamFrame;
#[cfg(feature = "remote")]
use surface_proto::engine::SurfaceEngineStreamClient;

/// What an engine pushes to a surface: telemetry frames and parameter
/// changes (from other surfaces, MIDI, automation, or echoes of our own).
#[cfg(feature = "remote")]
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Frame(StreamFrame),
    Param(ParamValue),
}

#[cfg(feature = "remote")]
impl Incoming {
    /// Feed one into a runtime at `t` ms.
    pub fn apply(self, rt: &mut Runtime, t: f64) {
        match self {
            Self::Frame(frame) => rt.apply_frame(frame, t),
            Self::Param(value) => rt.apply_values([value]),
        }
    }
}

/// Bridges an engine's streams to a non-async thread: [`Telemetry::pump`]
/// subscribes to `telemetry` and `param_changes` and forwards every item
/// into a `std::sync::mpsc` channel, which a UI thread drains each frame
/// (`try_iter`) without ever blocking.
///
/// `T` is whatever the receiving side's channel carries — [`Incoming`] by
/// default, or a host event enum with `From<Incoming>` so connection
/// status and engine data arrive on one channel.
#[cfg(feature = "remote")]
pub struct Telemetry<T = Incoming> {
    tx: std::sync::mpsc::Sender<T>,
}

#[cfg(feature = "remote")]
impl<T> Clone for Telemetry<T> {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}

/// Why [`Telemetry::pump`] returned.
#[cfg(feature = "remote")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PumpEnd {
    /// The receiving side hung up (the UI is gone): stop for good.
    Closed,
    /// A subscription ended — the link dropped or the engine went away.
    Lost(EngineError),
}

#[cfg(feature = "remote")]
impl Telemetry<Incoming> {
    /// A pump and the receiver a UI thread drains.
    #[must_use]
    pub fn channel() -> (Self, std::sync::mpsc::Receiver<Incoming>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Self { tx }, rx)
    }
}

#[cfg(feature = "remote")]
impl<T: From<Incoming> + Send + 'static> Telemetry<T> {
    /// Forward into an existing channel.
    #[must_use]
    pub fn new(tx: std::sync::mpsc::Sender<T>) -> Self {
        Self { tx }
    }

    /// Subscribe to both streams of `client` and forward until either
    /// subscription ends or the receiver is dropped. Spawn it on the
    /// runtime that owns the link (`tokio::spawn(async move { t.pump(&c).await })`).
    pub async fn pump(&self, client: &SurfaceEngineStreamClient) -> PumpEnd {
        use architect::vox;
        let (frame_tx, mut frame_rx) = vox::channel::<StreamFrame>();
        let (param_tx, mut param_rx) = vox::channel::<ParamValue>();
        let frames_call = client.telemetry(frame_tx);
        let params_call = client.param_changes(param_tx);
        let frames = async {
            while let Ok(Some(item)) = frame_rx.recv().await {
                if self.tx.send(T::from(Incoming::Frame(owned(item)))).is_err() {
                    return PumpEnd::Closed;
                }
            }
            PumpEnd::Lost(EngineError("telemetry stream ended".into()))
        };
        let params = async {
            while let Ok(Some(item)) = param_rx.recv().await {
                if self.tx.send(T::from(Incoming::Param(owned(item)))).is_err() {
                    return PumpEnd::Closed;
                }
            }
            PumpEnd::Lost(EngineError("param_changes stream ended".into()))
        };
        tokio::select! {
            end = frames => end,
            end = params => end,
            r = frames_call => PumpEnd::Lost(EngineError(format!("telemetry subscription ended: {r:?}"))),
            r = params_call => PumpEnd::Lost(EngineError(format!("param_changes subscription ended: {r:?}"))),
        }
    }
}

/// Move an owned (non-borrowing) value out of a received `SelfRef`.
#[cfg(feature = "remote")]
fn owned<T: 'static>(item: architect::vox::SelfRef<T>) -> T {
    let mut out = None;
    let _ = item.map(|v| out = Some(v));
    out.expect("SelfRef::map runs its closure")
}

/// Fetch the catalog and current values and build a runtime for `surface`.
///
/// # Errors
/// When the engine can't be reached.
pub async fn connect(engine: &impl Engine, surface: Surface) -> Result<Runtime, EngineError> {
    let catalog = engine.catalog().await?;
    let values = engine.values().await?;
    let mut rt = Runtime::new(surface, catalog);
    rt.apply_values(values);
    // Best effort: an engine that predates text params has none.
    if let Ok(texts) = engine.texts().await {
        rt.apply_texts(texts);
    }
    // Best effort: an engine that refuses keeps its catalog's shapes, and
    // plots draw whatever arrives.
    let requests = rt.feed_requests();
    if !requests.is_empty() {
        let _ = engine.request_feeds(requests).await;
    }
    Ok(rt)
}

/// Send calls in order. Stops at the first error.
///
/// # Errors
/// The first failing call's error.
pub async fn send(engine: &impl Engine, calls: Vec<EngineCall>) -> Result<(), EngineError> {
    for call in calls {
        engine.call(call).await?;
    }
    Ok(())
}

#[cfg(all(test, feature = "remote"))]
mod tests {
    use super::*;
    use surface_proto::{EntryId, ParamAddr};

    fn frame() -> StreamFrame {
        StreamFrame { id: EntryId::new("out_level"), values: vec![-12.0, -18.0] }
    }

    #[test]
    fn channel_carries_incoming_in_order() {
        let (t, rx) = Telemetry::channel();
        let t2 = t.clone();
        t.tx.send(Incoming::Frame(frame())).unwrap();
        t2.tx.send(Incoming::Param(ParamValue { addr: ParamAddr::new("a"), value: 0.25 })).unwrap();
        let got: Vec<Incoming> = rx.try_iter().collect();
        assert_eq!(got.len(), 2);
        assert!(matches!(&got[0], Incoming::Frame(f) if f.values == [-12.0, -18.0]));
        assert!(matches!(&got[1], Incoming::Param(p) if p.value == 0.25));
        assert!(rx.try_iter().next().is_none(), "drained without blocking");
    }

    #[derive(Debug, PartialEq)]
    enum HostEvent {
        Engine(Incoming),
    }

    impl From<Incoming> for HostEvent {
        fn from(i: Incoming) -> Self {
            Self::Engine(i)
        }
    }

    #[test]
    fn forwards_into_a_host_event_channel() {
        let (tx, rx) = std::sync::mpsc::channel::<HostEvent>();
        let t = Telemetry::new(tx);
        t.tx.send(HostEvent::from(Incoming::Frame(frame()))).unwrap();
        assert_eq!(rx.try_recv().unwrap(), HostEvent::Engine(Incoming::Frame(frame())));
    }

    #[test]
    fn incoming_applies_to_a_runtime() {
        let catalog = Catalog {
            id: "t".into(),
            version: 1,
            params: vec![surface_proto::ParamInfo::new("a", "A", surface_proto::Range::default())],
            actions: vec![],
            streams: vec![],
            collections: vec![],
        };
        let surface = Surface::from_scene(document_proto::Scene::new("S"));
        let mut rt = Runtime::new(surface, catalog);
        Incoming::Param(ParamValue { addr: ParamAddr::new("a"), value: 0.75 }).apply(&mut rt, 0.0);
        Incoming::Frame(frame()).apply(&mut rt, 0.0);
        assert_eq!(rt.values(), vec![ParamValue { addr: ParamAddr::new("a"), value: 0.75 }]);
    }
}
