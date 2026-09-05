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
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct SignInStatus {
    pub signed_in: bool,
    /// The signed-in account, when known. Display only.
    pub username: String,
    /// Why a sign-in did not happen, when one was attempted. Empty on the
    /// ordinary "not signed in" state — never having signed in is not a
    /// failure, and a UI must not present it as one.
    pub error: String,
    /// How the engine is authorized: `account` when the token is brokered
    /// by the FastTrackStudio issuer for a linked account, `tone3000` when
    /// this engine holds its own session, empty when neither.
    ///
    /// A UI needs the difference. "Sign in to TONE3000" is the wrong thing
    /// to offer someone whose account is already linked, and "Sign out"
    /// here cannot end a session this engine does not own.
    pub via: String,
}

/// An authorization to open in the system browser.
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthRequest {
    /// Open this externally — it cannot be rendered in-process.
    pub authorize_url: String,
    /// Correlates the later [`Tone3000::complete_sign_in`] with this request.
    /// Opaque to the GUI; the secrets it stands for stay on the engine.
    pub request_id: String,
}

/// One model belonging to a tone.
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
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
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct PickedTone {
    pub id: String,
    pub name: String,
    pub creator: String,
    pub creator_url: String,
    pub tone_url: String,
    pub license: String,
    pub models: Vec<ToneModel>,
    /// The creator's write-up: rig, capture chain, intended use.
    pub description: String,
    /// `amp`, `amp-cab`, `pedal`, `cab`, … — what was captured.
    pub gear: String,
    /// The real gear captured, e.g. `["Marshall Plexi"]`.
    pub makes: Vec<String>,
    /// Creator-applied labels.
    pub tags: Vec<String>,
    /// Photographs of the rig, as URLs on the catalog's CDN. Not fetchable
    /// by every GUI — see [`Tone3000::image`], which is how a picture
    /// actually reaches a surface.
    pub images: Vec<String>,
    /// Non-empty when the tone could not be fetched; every other field is
    /// then meaningless. Errors ride the payload rather than a `Result`
    /// because a composite return does not survive the browser client's
    /// schema-compat pass (the story is in `packs-proto::pack_plan`).
    pub error: String,
}

/// A tone as a list shows it — the search/browse row.
///
/// Deliberately flatter than [`PickedTone`]: search results and tone detail
/// are different payloads upstream (licence, sizes and links are detail-only),
/// so a row promises only what a row can actually be given.
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct ToneSummary {
    pub id: String,
    pub title: String,
    pub creator: String,
    pub gear: String,
    /// `nam`, `ir`, `aida-x`, … — how the capture is encoded.
    pub format: String,
    pub makes: Vec<String>,
    pub tags: Vec<String>,
    /// First image, if the creator uploaded one. Fetch it with
    /// [`Tone3000::image`].
    pub image: String,
    /// Total across every architecture when this row came from search, and
    /// the architecture-1 count alone when it came from detail — the API
    /// means two different things by it. A row shows it as "about this
    /// many"; [`Tone3000::tone`] is the authority.
    pub models_count: u32,
    pub downloads_count: u32,
    pub favorites_count: u32,
    pub tone_url: String,
}

/// What a list screen is asking for.
///
/// One struct rather than eight parameters: `#[architect::rpc]` allows four
/// per method (a Facet constraint), and a filter set grows.
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct ToneQuery {
    /// Free text. Empty browses instead of searching — upstream they are the
    /// same call.
    pub text: String,
    /// `amp`, `amp-cab`, `pedal`, `outboard`, `cab`, `space`,
    /// `experimental`. Empty = every category.
    pub gears: Vec<String>,
    /// `nam` or `ir`. Empty = both. IRs are filtered here, not by gear.
    pub format: String,
    /// `best-match`, `newest`, `oldest`, `trending`, `downloads-all-time`.
    pub sort: String,
    /// 1-based. 0 is read as 1.
    pub page: u32,
    /// Capped at 25 by the API; 0 takes the default.
    pub page_size: u32,
}

/// Which bounded list to serve — the free tier's alternative to search.
///
/// The API's terms grant OAuth plus *bounded* list endpoints for free, and
/// rate-limit `/tones/search` separately. These four cost nothing extra, so a
/// UI can open on real content before the user has typed anything.
#[derive(Facet, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum ToneShelf {
    /// The catalog's current top ten.
    #[default]
    Trending,
    /// The ten most recently published.
    Latest,
    /// Tones this account favourited.
    Favorited,
    /// Tones this account published.
    Created,
}

/// One page of tones.
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct TonePage {
    pub tones: Vec<ToneSummary>,
    pub page: u32,
    pub total_pages: u32,
    pub total: u32,
    /// Non-empty when the listing failed — an expired session, a rate limit,
    /// no network. The UI shows it rather than an empty shelf, which would
    /// otherwise be indistinguishable from "nothing matched".
    pub error: String,
}

/// An image, as bytes, because a URL is not portable across our surfaces.
///
/// Every GUI is a remote, and no two of them can fetch a picture the same
/// way: the browser remote is served cross-origin-isolated (COEP blocks the
/// catalog's CDN outright), a Blitz plugin editor has no browser cache or
/// cookie jar behind it, and an embedded engine has no HTTP server at all. So
/// the engine fetches and caches, and the picture travels the vox link that
/// already works everywhere. A UI renders it as a `data:` URI.
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageData {
    /// Encoded image bytes, exactly as the catalog served them — no
    /// re-encoding, so nothing is lost or silently transcoded.
    pub bytes: Vec<u8>,
    /// `image/jpeg`, `image/png`, … for the `data:` URI's media type.
    pub mime: String,
    /// Non-empty when the fetch failed; `bytes` is then empty.
    pub error: String,
}

/// Progress of one model download.
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct DownloadProgress {
    pub model_id: String,
    pub model_name: String,
    /// 0..=100. Servers do not always send a length, so this is advisory;
    /// `done` is the authority on completion.
    pub percent: u32,
    pub done: bool,
    /// Non-empty when the download failed; `done` is then also true.
    pub error: String,
    /// Absolute path of the placed file, on the machine running the engine.
    /// That is the right frame of reference even for a remote GUI on another
    /// device: the engine is what will load the model, so this is the value
    /// that goes into a preset.
    pub path: String,
    /// SHA-256 of the placed file once `done` and `error` is empty — the key
    /// the NAM catalog indexes by, so a UI can jump straight to the entry.
    pub hash: String,
}

pub mod tone3000 {
    //! `Tone3000` → `Tone3000Client` / `Tone3000Service`.
    use super::{
        AuthRequest, DownloadProgress, ImageData, PickedTone, SignInStatus, TonePage, ToneQuery,
        ToneShelf,
    };

    #[architect::rpc]
    pub trait Tone3000 {
        /// Whether a session is stored, and for whom.
        ///
        /// Async because the answer can depend on the account broker: a
        /// linked account is a working session this engine holds nothing
        /// for.
        async fn status(&self) -> SignInStatus;

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
        async fn complete_sign_in(&self, request_id: String, callback_url: String)
        -> SignInStatus;

        /// Forget the stored session.
        fn sign_out(&self);

        /// The tone a `prompt_select_tone` flow ended on, if any.
        async fn picked_tone(&self, request_id: String) -> PickedTone;

        /// Search or browse the public library.
        ///
        /// Empty [`ToneQuery::text`] browses; text searches. Upstream this is
        /// one rate-limited endpoint either way, which is why
        /// [`Tone3000::shelf`] exists for the screens that only need
        /// something to show.
        async fn search(&self, query: ToneQuery) -> TonePage;

        /// One of the bounded lists — the cheap way to fill a screen.
        ///
        /// `page` is honoured for the account's own shelves (favourited,
        /// created); trending and latest are ten tones with no paging.
        async fn shelf(&self, shelf: ToneShelf, page: u32) -> TonePage;

        /// One tone in full, with its models — the detail screen.
        ///
        /// Two upstream calls (detail, then models per architecture the tone
        /// reports), because a tone's detail response does not embed them.
        async fn tone(&self, tone_id: String) -> PickedTone;

        /// Fetch one image by the URL a tone carried, cached on the engine.
        ///
        /// The URL is only honoured if it is one the catalog gave us — the
        /// engine will not fetch an arbitrary address on a GUI's say-so.
        async fn image(&self, url: String) -> ImageData;

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
