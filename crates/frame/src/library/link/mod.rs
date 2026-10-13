//! The surface library across a WebSocket.
//!
//! - [`serve_ws`]: serve a store's router (`FsLibrary::into_router()`) as a
//!   vox endpoint — what `frame-library-server` runs.
//! - [`connect`] / [`connect_stream`]: the async clients. A vox link
//!   carries one service, so the call side and the stream side each open
//!   their own socket.
//! - [`Blocking`]: the call side for synchronous hosts (the MCP stdio loop,
//!   the desktop player's key handlers).
//! - [`Follow`]: a background thread that subscribes to one surface's
//!   updates and the index changes, reconnecting with backoff, and hands
//!   them to a UI thread that drains them each frame without blocking.

use std::sync::mpsc;
use std::time::Duration;

pub use crate::library::proto::*;
use crate::surface::proto::Catalog;
use architect::Backoff;
use architect::vox::{self, SelfRef};
use tokio::sync::mpsc as tmpsc;

/// Connect to a library's call side (`list`, `get`, `publish`, `catalogs`).
///
/// # Errors
/// When the socket can't be opened or the vox handshake fails.
pub async fn connect(url: &str) -> Result<SurfaceStoreClient, String> {
    let link = vox_websocket::WsLink::connect(url)
        .await
        .map_err(|e| format!("ws connect {url}: {e:?}"))?;
    vox_core::initiator_on(link)
        .establish()
        .await
        .map_err(|e| format!("vox handshake with {url}: {e:?}"))
}

/// Connect to a library's stream side (`updates`, `changes`).
///
/// # Errors
/// When the socket can't be opened or the vox handshake fails.
pub async fn connect_stream(url: &str) -> Result<SurfaceStoreStreamClient, String> {
    let link = vox_websocket::WsLink::connect(url)
        .await
        .map_err(|e| format!("ws connect {url}: {e:?}"))?;
    vox_core::initiator_on(link)
        .establish()
        .await
        .map_err(|e| format!("vox handshake with {url}: {e:?}"))
}

/// Serve `router` as a WebSocket vox endpoint on `listener`. Runs until the
/// listener fails; connections live on the current tokio runtime.
pub async fn serve_ws(listener: tokio::net::TcpListener, router: architect::LayerRouter) {
    while let Ok((tcp, _)) = listener.accept().await {
        let router = router.clone();
        tokio::spawn(async move {
            let Ok(link) = vox_websocket::WsLink::server(tcp).await else {
                return;
            };
            if let Ok(connection) = vox_core::acceptor_on(link)
                .on_lane(router.acceptor())
                .establish_connection()
                .await
            {
                let _connection = connection;
                std::future::pending::<()>().await;
            }
        });
    }
}

/// A router served on its own thread (tests, and hosts that embed a
/// library next to a synchronous UI). Dropping it stops the server and
/// drops every connection, like a crashed server.
pub struct Served {
    pub url: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Served {
    /// Serve `router` on `listener` (e.g. `127.0.0.1:0`).
    ///
    /// # Errors
    /// When the listener can't be used or the runtime can't start.
    pub fn start(
        router: architect::LayerRouter,
        listener: std::net::TcpListener,
    ) -> Result<Self, String> {
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let url = format!(
            "ws://{}/vox",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("tokio: {e}"))?;
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("library-serve".into())
            .spawn(move || {
                rt.block_on(async move {
                    let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                        return;
                    };
                    tokio::select! {
                        () = serve_ws(listener, router) => {}
                        _ = stopped => {}
                    }
                });
                rt.shutdown_background();
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            url,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// The `host:port` it listens on.
    #[must_use]
    pub fn addr(&self) -> &str {
        self.url
            .trim_start_matches("ws://")
            .trim_end_matches("/vox")
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Move an owned value out of a received `SelfRef`.
fn owned<T: 'static>(item: SelfRef<T>) -> T {
    let mut out = None;
    let _ = item.map(|v| out = Some(v));
    out.expect("SelfRef::map runs its closure")
}

/// The call side of a library, for synchronous code: owns a small tokio
/// runtime and (re)opens the link on demand.
pub struct Blocking {
    rt: tokio::runtime::Runtime,
    url: String,
    client: Option<SurfaceStoreClient>,
}

impl Blocking {
    /// # Errors
    /// When the runtime can't start.
    pub fn new(url: &str) -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("tokio: {e}"))?;
        Ok(Self {
            rt,
            url: url.to_owned(),
            client: None,
        })
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Run `f` against the client. A cached link may have died with a
    /// server restart: a transport failure gets one retry on a fresh link.
    fn call<R, F, Fut>(&mut self, f: F) -> Result<R, String>
    where
        F: Fn(SurfaceStoreClient) -> Fut,
        Fut: Future<Output = Result<R, Call>>,
    {
        let url = self.url.clone();
        let cached = self.client.take();
        let (client, result) = self.rt.block_on(async move {
            if let Some(c) = cached {
                match f(c.clone()).await {
                    Err(Call::Transport(_)) => {}
                    other => return (Some(c), other),
                }
            }
            match connect(&url).await {
                Ok(c) => {
                    let r = f(c.clone()).await;
                    (Some(c), r)
                }
                Err(e) => (None, Err(Call::Transport(e))),
            }
        });
        self.client = client;
        result.map_err(|e| match e {
            Call::Transport(e) | Call::Refused(e) => e,
        })
    }

    /// # Errors
    /// The library is unreachable.
    pub fn list(&mut self, query: &ListQuery) -> Result<Vec<SurfaceMeta>, String> {
        self.call(|c| {
            let q = query.clone();
            async move { c.list(q).await.map_err(Call::transport) }
        })
    }

    /// # Errors
    /// The library is unreachable.
    pub fn preview(&mut self, id: &str) -> Result<Vec<u8>, String> {
        self.call(|c| {
            let id = id.to_owned();
            async move { c.preview(id).await.map_err(Call::transport) }
        })
    }

    /// # Errors
    /// Unreachable, or no such surface.
    pub fn get(&mut self, id: &str, version: Option<u64>) -> Result<Surface, String> {
        self.call(|c| {
            let id = id.to_owned();
            async move { c.get(id, version).await.map_err(Call::refused) }
        })
    }

    /// # Errors
    /// Unreachable, or the library refused the content.
    pub fn publish(&mut self, content: &Content, meta: &PublishMeta) -> Result<Published, String> {
        self.call(|c| {
            let (content, meta) = (content.clone(), meta.clone());
            async move { c.publish(content, meta).await.map_err(Call::refused) }
        })
    }

    /// # Errors
    /// The library is unreachable.
    pub fn catalogs(&mut self) -> Result<Vec<Catalog>, String> {
        self.call(|c| async move { c.catalogs().await.map_err(Call::transport) })
    }
}

/// How a blocking call failed: the link (retry on a fresh one) or the
/// library (don't).
enum Call {
    Transport(String),
    Refused(String),
}

impl Call {
    fn transport(e: impl std::fmt::Debug) -> Self {
        Self::Transport(format!("{e:?}"))
    }

    /// A refusal carries the library's error; anything else is the link.
    fn refused(e: impl std::fmt::Debug) -> Self {
        let text = format!("{e:?}");
        if ["NotFound", "BadContent", "BadId", "Io("]
            .iter()
            .any(|k| text.contains(k))
        {
            Self::Refused(text)
        } else {
            Self::Transport(text)
        }
    }
}

/// What a [`Follow`] hands the UI thread.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// (Re)connected to the library.
    Connected,
    /// The followed surface changed (the first one after following is its
    /// current snapshot).
    Update(SurfaceUpdate),
    /// The library's index changed.
    Index(IndexChange),
    /// The link dropped (or never came up); reconnecting.
    Lost(String),
}

/// Follows one surface's updates (and the index) on a background thread.
pub struct Follow {
    url: String,
    cmds: tmpsc::UnboundedSender<Option<SurfaceId>>,
    events: mpsc::Receiver<Event>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Follow {
    /// Start following `id` (or only the index) on `url`. Returns at once.
    ///
    /// # Errors
    /// When the background runtime can't start.
    pub fn start(url: &str, id: Option<SurfaceId>) -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = tmpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::channel();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("tokio: {e}"))?;
        let u = url.to_owned();
        let thread = std::thread::Builder::new()
            .name("library-follow".into())
            .spawn(move || rt.block_on(supervise(u, id, cmd_rx, event_tx)))
            .map_err(|e| format!("library thread: {e}"))?;
        Ok(Self {
            url: url.to_owned(),
            cmds: cmd_tx,
            events: event_rx,
            thread: Some(thread),
        })
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Follow another surface (`None` = only the index).
    pub fn follow(&self, id: Option<SurfaceId>) {
        let _ = self.cmds.send(id);
    }

    /// Every event since the last drain, without blocking.
    #[must_use]
    pub fn drain(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// The next event, waiting up to `timeout`.
    #[must_use]
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.events.recv_timeout(timeout).ok()
    }
}

impl Drop for Follow {
    fn drop(&mut self) {
        // Closing the command channel stops the thread.
        let (dead, _) = tmpsc::unbounded_channel();
        self.cmds = dead;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

enum Ended {
    UiGone,
    Lost { connected: bool, reason: String },
}

async fn supervise(
    url: String,
    mut id: Option<SurfaceId>,
    mut cmds: tmpsc::UnboundedReceiver<Option<SurfaceId>>,
    events: mpsc::Sender<Event>,
) {
    let mut backoff = Backoff::default();
    let mut reported = false;
    loop {
        match session(&url, &mut id, &mut cmds, &events).await {
            Ended::UiGone => return,
            Ended::Lost { connected, reason } => {
                if connected {
                    backoff.reset();
                    reported = false;
                }
                if !reported {
                    if events.send(Event::Lost(reason)).is_err() {
                        return;
                    }
                    reported = true;
                }
            }
        }
        let sleep = tokio::time::sleep(backoff.next_delay());
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                () = &mut sleep => break,
                cmd = cmds.recv() => match cmd {
                    None => return,
                    Some(new) => id = new,
                },
            }
        }
    }
}

async fn session(
    url: &str,
    id: &mut Option<SurfaceId>,
    cmds: &mut tmpsc::UnboundedReceiver<Option<SurfaceId>>,
    events: &mpsc::Sender<Event>,
) -> Ended {
    let stream = match connect_stream(url).await {
        Ok(s) => s,
        Err(reason) => {
            return Ended::Lost {
                connected: false,
                reason,
            };
        }
    };
    if events.send(Event::Connected).is_err() {
        return Ended::UiGone;
    }
    let lost = |reason: String| Ended::Lost {
        connected: true,
        reason,
    };
    loop {
        let (tx, mut rx) = vox::channel::<SurfaceUpdate>();
        let (ctx, mut crx) = vox::channel::<IndexChange>();
        let following = id.clone();
        let has_target = following.is_some();
        let sub = async {
            match following {
                Some(target) => format!("{:?}", stream.updates(target, tx).await),
                None => {
                    drop(tx);
                    std::future::pending::<String>().await
                }
            }
        };
        let idx = stream.changes(ctx);
        tokio::pin!(sub);
        tokio::pin!(idx);
        loop {
            tokio::select! {
                r = &mut sub => return lost(format!("updates subscription ended: {r}")),
                r = &mut idx => return lost(format!("changes subscription ended: {r:?}")),
                item = rx.recv(), if has_target => match item {
                    Ok(Some(u)) => if events.send(Event::Update(owned(u))).is_err() { return Ended::UiGone },
                    other => return lost(format!("updates stream ended: {:?}", other.err())),
                },
                item = crx.recv() => match item {
                    Ok(Some(c)) => if events.send(Event::Index(owned(c))).is_err() { return Ended::UiGone },
                    other => return lost(format!("changes stream ended: {:?}", other.err())),
                },
                cmd = cmds.recv() => match cmd {
                    None => return Ended::UiGone,
                    Some(new) => {
                        *id = new;
                        break;
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests;
