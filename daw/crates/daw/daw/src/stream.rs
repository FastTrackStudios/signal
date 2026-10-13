//! Streaming ergonomics for vox `Rx<T>` channels.
//!
//! Service `subscribe()` methods on the daw facade hand back a `vox::Rx<T>`,
//! whose `recv()` returns `Result<Option<SelfRef<T>>, RxError>`. The `SelfRef`
//! wrapper is needed when `T` borrows from a backing buffer, but every event
//! type the daw facade exposes (`MarkerEvent`, `RegionEvent`, `ItemEvent`,
//! `TrackEvent`, `FxEvent`, `ActionEvent`, etc.) has `Ref<'a> = Self` — there's
//! no actual self-reference, so the `SelfRef` is pure ceremony.
//!
//! [`RxExt`] adds a `next_owned()` method that clones the inner value out of
//! the `SelfRef` and folds the channel-closed case into a plain `None`. The
//! shape mirrors `tokio::sync::broadcast::Receiver::recv` — a clean
//! `while let Some(event) = rx.next_owned().await? { … }` loop.

use std::error::Error;
use std::fmt;

use vox::Rx;

/// Error type returned by [`RxExt::next_owned`].
///
/// Distinguishes the channel being reset (recoverable: subscribe again) from
/// any other vox-side failure. [`std::error::Error`] is implemented so the
/// type composes with `eyre`/`anyhow`.
#[derive(Debug)]
pub enum StreamError {
    /// The vox channel sent a `Reset` frame — typically because the host
    /// service was reinitialized. Caller should resubscribe.
    Reset,
    /// Other vox channel failure.
    Other(String),
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reset => write!(f, "stream reset by remote — resubscribe"),
            Self::Other(s) => write!(f, "stream error: {s}"),
        }
    }
}

impl Error for StreamError {}

/// Ergonomic helpers for service event streams.
pub trait RxExt<T> {
    /// Receive the next event as an owned value.
    ///
    /// - `Ok(Some(event))` — a new event arrived.
    /// - `Ok(None)` — the producer closed the channel; the receiver should
    ///   exit its loop.
    /// - `Err(StreamError::Reset)` — the channel was reset; the caller
    ///   should resubscribe.
    /// - `Err(StreamError::Other(_))` — unrecoverable transport error.
    ///
    /// The value is moved out of the underlying `vox::SelfRef<T>` via
    /// `SelfRef::map` — no `Clone` bound required on `T`.
    #[allow(async_fn_in_trait)] // matches vox::Rx's signature
    async fn next_owned(&mut self) -> Result<Option<T>, StreamError>
    where
        T: 'static;
}

impl<T> RxExt<T> for Rx<T>
where
    T: facet::Facet<'static>,
{
    async fn next_owned(&mut self) -> Result<Option<T>, StreamError>
    where
        T: 'static,
    {
        match self.recv().await {
            // `SelfRef::map` is the only way to get the inner T out without a
            // `Deref` impl (vox removed Deref to avoid leaking the fake
            // 'static lifetime). We use it to capture the owned T into an
            // Option, then discard the resulting `SelfRef<()>`.
            Ok(Some(selfref)) => {
                let mut taken: Option<T> = None;
                let _ = selfref.map(|value| {
                    taken = Some(value);
                });
                Ok(Some(
                    taken.expect("vox::SelfRef::map runs the closure exactly once"),
                ))
            }
            Ok(None) => Ok(None),
            Err(vox::RxError::Reset) => Err(StreamError::Reset),
            Err(other) => Err(StreamError::Other(format!("{other:?}"))),
        }
    }
}
