//! Wire contract for the TONE3000 tone library.
//!
//! The engine holds the session and does the fetching; a GUI drives it over
//! this surface. Wasm-clean, because the browser remote is one of the GUIs.
//!
//! # The shape, and why it is this shape
//!
//! Signing in is a three-step conversation rather than one call, because the
//! middle step does not happen in our process at all: the authorization page
//! is a web application, and neither a Blitz plugin editor nor an embedded
//! view can host it. So:
//!
//! 1. [`Tone3000::begin_sign_in`] — the engine mints PKCE and returns a URL.
//! 2. The GUI opens that URL in the **system browser** and waits.
//! 3. [`Tone3000::complete_sign_in`] — the GUI hands back the callback it
//!    received, and the engine redeems it.
//!
//! The GUI never sees a token, a verifier or the `state` nonce. It is a
//! courier for a URL in one direction and a callback in the other.

use facet::Facet;

/// Whether the engine currently holds a session.
#[derive(Facet, Clone, Debug, Default, PartialEq)]
pub struct SignInStatus {
    pub signed_in: bool,
    /// The signed-in account, when known. Display only.
    pub username: String,
}

/// An authorization to open in the system browser.
#[derive(Facet, Clone, Debug, Default)]
pub struct AuthRequest {
    /// Open this externally — it cannot be rendered in-process.
    pub authorize_url: String,
    /// Correlates the later [`Tone3000::complete_sign_in`] with this request.
    /// Opaque to the GUI; the secrets it stands for stay on the engine.
    pub request_id: String,
}

/// One model belonging to a tone.
#[derive(Facet, Clone, Debug, Default)]
pub struct ToneModel {
    pub id: String,
    pub name: String,
    /// `standard`, `lite`, `feather`, `nano`, `custom` — the CPU/quality tier.
    pub size: String,
    /// Present so a UI can warn before fetching a model this build will not
    /// run; the engine plays A1 and A2 alike today.
    pub architecture: String,
}

/// A tone the user picked, with everything attribution needs.
///
/// Creator and licence are carried on the wire rather than looked up later
/// because the API terms forbid stripping them, and because a UI has to be
/// able to show them next to the tone at the moment of download.
#[derive(Facet, Clone, Debug, Default)]
pub struct PickedTone {
    pub id: String,
    pub name: String,
    pub creator: String,
    pub creator_url: String,
    pub tone_url: String,
    pub license: String,
    pub models: Vec<ToneModel>,
}

/// Progress of one model download.
#[derive(Facet, Clone, Debug, Default)]
pub struct DownloadProgress {
    pub model_id: String,
    pub model_name: String,
    /// 0..=100. Servers do not always send a length, so this is advisory;
    /// `done` is the authority on completion.
    pub percent: u32,
    pub done: bool,
    /// Non-empty when the download failed; `done` is then also true.
    pub error: String,
    /// SHA-256 of the placed file once `done` and `error` is empty — the key
    /// the NAM catalog indexes by, so a UI can jump straight to the entry.
    pub hash: String,
}

pub mod tone3000 {
    //! `Tone3000` → `Tone3000Client` / `Tone3000Service`.
    use super::{AuthRequest, DownloadProgress, PickedTone, SignInStatus};

    #[architect::rpc]
    pub trait Tone3000 {
        /// Whether a session is stored, and for whom.
        fn status(&self) -> SignInStatus;

        /// Mint PKCE and build the authorize URL. The caller opens it in the
        /// system browser; nothing is exchanged until `complete_sign_in`.
        ///
        /// `prompt_select_tone` asks TONE3000 to show its own tone picker as
        /// part of the flow, which is how a user browses the full library
        /// without us re-implementing search — and without spending the
        /// search quota the free tier does not grant.
        fn begin_sign_in(&self, prompt_select_tone: bool) -> AuthRequest;

        /// Redeem the callback the browser produced. `callback_url` is the
        /// full redirect URI including its query; the engine checks `state`
        /// against the pending request and exchanges the code.
        fn complete_sign_in(&self, request_id: String, callback_url: String) -> SignInStatus;

        /// Forget the stored session.
        fn sign_out(&self);

        /// The tone a `prompt_select_tone` flow ended on, if any.
        fn picked_tone(&self, request_id: String) -> PickedTone;

        /// Fetch one model into the local NAM library. Progress arrives on
        /// [`Tone3000::downloads`]; this returns as soon as the work is
        /// queued, so a slow transfer never blocks the caller.
        fn download_model(&self, tone_id: String, model_id: String);

        /// Progress for every download this engine is running.
        #[subscribe]
        fn downloads(&self) -> DownloadProgress;
    }
}

pub use tone3000::prelude::*;
