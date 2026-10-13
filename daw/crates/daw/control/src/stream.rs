//! Client-side plumbing for architect `#[subscribe]` streams.
//!
//! A `#[subscribe]` stream is argless on the wire — the server sends
//! every event and the client filters. [`EventStream`] wraps the raw
//! `vox::Rx` from the stream call with an admit predicate (project
//! guid, `BusFilter`, …) and owns the in-flight subscribe call: the
//! call future never completes while the subscription lives (vox
//! scopes channels to their request), so it's parked on a spawned
//! task that is released when the `EventStream` is dropped —
//! cancelling the call is the unsubscribe.

use core::future::Future;

// Target-conditional future bound. The in-flight `events(tx)` subscribe call
// parked below captures vox connection senders, which are `Send` on native
// (multi-thread tokio) but `!Send` on wasm32 (single-threaded vox runtime,
// `Rc<RefCell<…>>`). On native `MaybeSend: Send` (blanket for every `T: Send`),
// so the bound is byte-for-byte `Send` and the spawned task stays `Send` for
// `architect::platform::spawn`; on wasm32 the bound vanishes and the spawn
// maps to `spawn_local` (no `Send` required). Native codegen is unchanged.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> MaybeSend for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSend for T {}

/// A live, client-filtered event subscription. Yields events exactly
/// like `vox::Rx::recv` (`Ok(Some(SelfRef<T>))` per event, `Ok(None)`
/// / `Err` when the stream ends); events rejected by the filter are
/// skipped transparently. Dropping the stream unsubscribes.
pub struct EventStream<T: 'static> {
    rx: vox::Rx<T>,
    admit: Box<dyn Fn(&T) -> bool + Send + Sync>,
    /// Dropping this releases the spawned task holding the subscribe
    /// call in flight, cancelling the server-side subscription.
    _stop: futures::channel::oneshot::Sender<()>,
}

impl<T> EventStream<T>
where
    T: facet::Facet<'static> + for<'a> vox_types::Reborrow<Ref<'a> = T> + Send + 'static,
{
    /// Park `call` (the in-flight `events(tx)` future) on a task and
    /// wrap `rx` with `admit`.
    pub(crate) fn spawn(
        call: impl Future<Output = ()> + MaybeSend + 'static,
        rx: vox::Rx<T>,
        admit: Box<dyn Fn(&T) -> bool + Send + Sync>,
    ) -> Self {
        let (stop_tx, stop_rx) = futures::channel::oneshot::channel::<()>();
        architect::platform::spawn(async move {
            futures::pin_mut!(call);
            // Ends when the call ends (connection gone) or the
            // EventStream is dropped (stop_rx resolves) — either way
            // the call future is dropped, which is the unsubscribe.
            let _ = futures::future::select(call, stop_rx).await;
        });
        Self {
            rx,
            admit,
            _stop: stop_tx,
        }
    }

    /// Block until the server has attached this stream's sink.
    ///
    /// Reads frames off the RAW channel, bypassing `admit`, until one
    /// satisfies `is_attached` — the backend's intro marker, which the
    /// attach itself puts at the front of the mailbox. Anything read on
    /// the way is dropped: nothing can legitimately precede the marker,
    /// so a frame that does is from a previous life of the channel.
    ///
    /// Returns `Ok(())` if the stream ends first, which is not an error:
    /// a subscription to a backend that is going away has nothing to
    /// wait for.
    pub(crate) async fn await_marker(
        &mut self,
        is_marker: impl Fn(&T) -> bool,
    ) -> Result<(), vox::RxError> {
        loop {
            match self.rx.recv().await? {
                None => return Ok(()),
                Some(ev) => {
                    if is_marker(ev.get()) {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Next admitted event. Same contract as `vox::Rx::recv`.
    pub async fn recv(&mut self) -> Result<Option<vox::SelfRef<T>>, vox::RxError> {
        loop {
            match self.rx.recv().await? {
                None => return Ok(None),
                Some(ev) => {
                    if (self.admit)(ev.get()) {
                        return Ok(Some(ev));
                    }
                }
            }
        }
    }
}

impl<T> std::fmt::Debug for EventStream<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventStream").finish_non_exhaustive()
    }
}
