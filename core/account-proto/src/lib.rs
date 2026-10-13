//! Wire contract for the `FastTrackStudio` account (`auth.fasttrackstudio.app`).
//!
//! One sign-in, shared by every third party the account has linked —
//! TONE3000 today. The shape mirrors `signal-tone3000-proto`'s own sign-in
//! trio, for the same reason: the authorization page is a web application,
//! and neither a Blitz plugin editor nor an embedded view can host it.
//!
//! 1. [`AccountAuth::begin_sign_in`] — the engine mints PKCE and returns a
//!    URL.
//! 2. The GUI opens that URL in the **system browser** and waits.
//! 3. The engine's own HTTP server (not this RPC — see
//!    `apps/desktop/src/engine_tone3000.rs::extend_account`) catches the
//!    redirect and redeems it; the GUI finds out by calling
//!    [`AccountAuth::status`] again.
//!
//! The GUI never sees a token, a verifier or the `state` nonce.

use facet::Facet;

/// Whether the engine holds a `FastTrackStudio` session, and whose.
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountStatus {
    pub signed_in: bool,
    /// Display only.
    pub email: String,
    /// Why a sign-in did not happen, when one was attempted. Empty on the
    /// ordinary "not signed in" state — never having signed in is not a
    /// failure, and a UI must not present it as one.
    pub error: String,
}

/// An authorization to open in the system browser.
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthRequest {
    /// Open this externally — it cannot be rendered in-process.
    pub authorize_url: String,
    /// Opaque to the GUI; correlates the engine's own redirect handling with
    /// this attempt. Not used to redeem the callback over this RPC — the
    /// engine's HTTP server does that directly, by `state` — but returned so
    /// a GUI can tell one sign-in attempt from the next.
    pub request_id: String,
}

pub mod account {
    //! `AccountAuth` → `AccountAuthClient` / `AccountAuthService` /
    //! `account_auth_serve`.
    use super::{AccountStatus, AuthRequest};

    #[architect::rpc]
    pub trait AccountAuth {
        /// Whether the engine holds a `FastTrackStudio` session. Answered
        /// from disk — never needs the network, so the UI does not go blank
        /// when the issuer is briefly unreachable.
        fn status(&self) -> AccountStatus;
        /// Mint PKCE and hand back a URL for the GUI to open in the system
        /// browser. An empty `authorize_url` means this build has no issuer
        /// configured.
        fn begin_sign_in(&self) -> AuthRequest;
        /// Forget the session.
        fn sign_out(&self);
    }
}
