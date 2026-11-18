//! The engine-side `Tone3000` service.
//!
//! Holds the OAuth session, answers browse/search/detail, fetches artwork, and
//! runs model downloads into the local NAM library with progress on a
//! [`PubSub`] hub. Mount [`Tone3000Backend::router`] on any vox transport —
//! the engine's WebSocket router, its iroh endpoint, and the in-process
//! `LocalServer` a plugin editor uses all serve it unchanged.
//!
//! # Why every network method returns a payload rather than a `Result`
//!
//! Composite returns do not survive the browser client's schema-compat pass
//! (the story is written up in `packs-proto::pack_plan`), and the browser
//! remote is a first-class GUI here. So failures ride an `error` field. That
//! is not only a workaround: a list screen has to distinguish "nothing
//! matched" from "the request never happened", and a `Result` that the UI
//! flattens into an empty list loses exactly that.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use crate::tone3000::{Tone3000, Tone3000StreamSource};
use crate::{
    AuthRequest, DownloadProgress, ImageData, PickedTone, SignInStatus, TonePage, ToneQuery,
    ToneShelf,
};
use architect::{HasDispatcher, Layer, PubSub, Services, layers};
use futures_util::StreamExt as _;
use tone3000 as api;

use crate::client::config::Config;
use crate::client::map;
use crate::client::session::{AuthStart, Session, SessionError};
use crate::client::store::{TokenStore, Tokens};

/// Rows a browse screen asks for when it does not say.
const DEFAULT_PAGE_SIZE: u32 = 24;
/// The API's ceiling for `/tones/search`; asking for more is an error there.
const MAX_PAGE_SIZE: u32 = 25;
/// A shelf is ten tones upstream. Kept here so paging maths never invents a
/// page the endpoint does not have.
const SHELF_SIZE: u32 = 10;
/// Refuse an image larger than this rather than hold it in memory and push it
/// down a vox link. Tone photographs are tens of kilobytes; anything at this
/// size is not a photograph.
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
/// How many fetched tones to remember. Enough to cover a browse session, so
/// that downloading from a tone the user is looking at costs no extra calls;
/// small enough that a long session cannot grow without bound.
const TONE_MEMO: usize = 64;

/// The TONE3000 backend handle. Cheap to clone; all state is shared.
#[derive(Clone, HasDispatcher)]
pub struct Tone3000Backend {
    inner: Arc<Inner>,
}

struct Inner {
    cfg: Config,
    session: Session,
    /// Authorizations handed to a GUI and not yet redeemed.
    pending: Mutex<HashMap<String, AuthStart>>,
    /// `request_id` → the tone a `select_tone` flow came back with.
    picked: Mutex<HashMap<String, String>>,
    /// Tones already fetched this session, by id.
    ///
    /// A download has to record the creator and licence with the file, and
    /// re-fetching the tone to learn them costs three more API calls —
    /// detail plus one per architecture — against a 100-per-minute budget,
    /// *after* the bytes are already on disk. The UI has almost always just
    /// displayed the tone, so this is nearly always a hit. Provenance does
    /// not change, so there is nothing to go stale.
    tones: Mutex<HashMap<String, PickedTone>>,
    /// Image URLs the catalog gave us. The engine fetches nothing else on a
    /// GUI's say-so — a remote naming an arbitrary address would otherwise
    /// have the engine make requests on its behalf, from inside the LAN.
    known_images: Mutex<HashSet<String>>,
    /// Built lazily from the stored session; dropped on sign-out.
    client: tokio::sync::Mutex<Option<api::Client>>,
    /// The FastTrackStudio account, when the engine has one.
    ///
    /// Preferred over a local TONE3000 session, because the issuer holds
    /// the refresh token and rotates it in one place: link once at
    /// auth.fasttrackstudio.app/account, and every machine signed in to
    /// that account can download captures without its own authorization.
    /// A local session stays the fallback — for someone with no
    /// FastTrackStudio account, and for when the issuer is unreachable.
    account: Option<Arc<crate::account::Account>>,
    downloads: PubSub<DownloadProgress>,
    http: reqwest::Client,
}

impl Tone3000Backend {
    /// Build a backend over `cfg`. Does no I/O and never fails: an
    /// unconfigured or signed-out engine is an ordinary state that the
    /// service reports, not a construction error.
    #[must_use]
    pub fn new(cfg: Config) -> Self {
        let session = Session::new(
            TokenStore::new(cfg.token_path.clone()),
            cfg.library_root.clone(),
        );
        tracing::info!(
            configured = cfg.is_configured(),
            library = %cfg.library_root.display(),
            "tone3000: backend ready"
        );
        Self {
            inner: Arc::new(Inner {
                cfg,
                session,
                pending: Mutex::new(HashMap::new()),
                picked: Mutex::new(HashMap::new()),
                tones: Mutex::new(HashMap::new()),
                known_images: Mutex::new(HashSet::new()),
                client: tokio::sync::Mutex::new(None),
                account: None,
                downloads: architect::rig::events_hub(),
                http: reqwest::Client::new(),
            }),
        }
    }

    /// Read the account's linked TONE3000 token instead of holding a
    /// local session, when one is available.
    #[must_use]
    pub fn with_account(mut self, account: Arc<crate::account::Account>) -> Self {
        // `Arc::get_mut` is sound here: this is a builder, called before
        // the backend has been cloned into a router.
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.account = Some(account);
        } else {
            tracing::error!("tone3000: with_account called after the backend was shared");
        }
        self
    }

    /// The composed service router — mount on any vox transport.
    #[must_use]
    pub fn router(&self) -> architect::LayerRouter {
        self.clone().into_router()
    }

    /// The session, for callers that need the library paths (the engine's
    /// callback route, tests).
    #[must_use]
    pub fn session(&self) -> &Session {
        &self.inner.session
    }

    /// The redirect URI this backend expects its callbacks on.
    #[must_use]
    pub fn redirect_uri(&self) -> &str {
        &self.inner.cfg.redirect_uri
    }

    /// An authenticated client, built once from the stored session.
    async fn client(&self) -> Result<api::Client, SessionError> {
        if !self.inner.cfg.is_configured() {
            return Err(SessionError::NotConfigured);
        }
        let mut guard = self.inner.client.lock().await;
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }

        // The brokered path first. It is not cached in `client`: the issuer
        // hands out a short-lived token and is the only thing that knows
        // when it was last refreshed, so a client held here would go stale
        // invisibly. One request to a service the engine already talks to
        // is the cheaper mistake.
        if let Some(brokered) = self.brokered_client().await {
            return Ok(brokered);
        }

        let tokens = self.inner.session.stored_tokens()?;
        let store = self.inner.session.token_store().clone();
        let username = tokens.username.clone();
        let mut builder = api::Client::builder(&self.inner.cfg.publishable_key)
            .access_token(tokens.access_token)
            .refresh_token(tokens.refresh_token)
            .expires_at(u64::try_from(tokens.expires_at).unwrap_or(0))
            // The refresh token ROTATES on every use: a refresh we do not
            // persist kills the session at the next call.
            .auto_refresh(true)
            .on_tokens_changed(move |t| persist(&store, t, &username));
        if let Some(base) = &self.inner.cfg.base_url {
            builder = builder.base_url(base.clone());
        }
        let client = builder.build();
        *guard = Some(client.clone());
        Ok(client)
    }

    /// A client carrying the account's linked token, if there is one.
    ///
    /// Returns `None` rather than an error whenever the brokered route is
    /// not available — no account, not linked, issuer unreachable — because
    /// every one of those is a reason to fall back to a local session, not
    /// a reason to fail the call.
    async fn brokered_client(&self) -> Option<api::Client> {
        let token = self.brokered_token().await?;
        let mut builder = api::Client::builder(&self.inner.cfg.publishable_key).access_token(token);
        if let Some(base) = &self.inner.cfg.base_url {
            builder = builder.base_url(base.clone());
        }
        Some(builder.build())
    }

    /// The account's linked TONE3000 token, if the brokered route is up.
    async fn brokered_token(&self) -> Option<String> {
        let account = self.inner.account.as_ref()?;
        match account
            .linked_token(crate::account::TONE3000_PROVIDER)
            .await
        {
            Ok(linked) => {
                tracing::debug!(
                    login = linked.login.as_deref().unwrap_or_default(),
                    "tone3000: using the account's linked token"
                );
                Some(linked.access_token)
            }
            Err(e) => {
                tracing::debug!(%e, "tone3000: no brokered token — falling back to a local session");
                None
            }
        }
    }

    /// A client with no session — only good for redeeming an authorization.
    fn anonymous(&self) -> api::Client {
        let mut builder = api::Client::builder(&self.inner.cfg.publishable_key);
        if let Some(base) = &self.inner.cfg.base_url {
            builder = builder.base_url(base.clone());
        }
        builder.build()
    }

    /// Forget the cached client so the next call rebuilds it from disk.
    async fn invalidate_client(&self) {
        *self.inner.client.lock().await = None;
    }

    /// Remember the image URLs a payload carried, so [`Tone3000::image`] will
    /// serve them later.
    fn remember_images<'a>(&self, urls: impl IntoIterator<Item = &'a String>) {
        let mut seen = self
            .inner
            .known_images
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for url in urls {
            if !url.is_empty() {
                seen.insert(url.clone());
            }
        }
    }

    /// One tone plus every model it has, across the architectures it reports.
    ///
    /// The models endpoint serves ONE architecture per call and defaults to
    /// v1, so a single call silently drops half of a tone that has both. The
    /// counts on the detail payload say which calls are worth making.
    async fn tone_detail(&self, tone_id: &str) -> PickedTone {
        let Ok(id) = tone_id.parse::<u64>() else {
            return map::failed_tone(format!("not a tone id: {tone_id}"));
        };
        let client = match self.client().await {
            Ok(c) => c,
            Err(e) => return map::failed_tone(e),
        };
        let tone = match client.tone(api::ToneId(id)).await {
            Ok(t) => t,
            Err(e) => return map::failed_tone(e),
        };

        let mut models = Vec::new();
        for (count, arch) in [
            (tone.a1_models_count, api::ArchitectureVersion::V1),
            (tone.a2_models_count, api::ArchitectureVersion::V2),
            (tone.custom_models_count, api::ArchitectureVersion::Custom),
        ] {
            if count == 0 {
                continue;
            }
            match client.models(tone.id).architecture(arch).await {
                Ok(page) => models.extend(page.data),
                // A missing architecture is not a missing tone: show what did
                // come back rather than failing the whole screen.
                Err(e) => tracing::warn!(tone = tone_id, ?e, "tone3000: models page failed"),
            }
        }

        // An IR tone (or any non-NAM format) counts no NAM architecture at
        // all, so the loop above asks for nothing and the tone listed zero
        // models. Ask once without the filter.
        if tone.a1_models_count + tone.a2_models_count + tone.custom_models_count == 0 {
            match client.models(tone.id).await {
                Ok(page) => models.extend(page.data),
                Err(e) => tracing::warn!(tone = tone_id, ?e, "tone3000: models page failed"),
            }
        }

        let picked = map::picked(&tone, &models);
        self.remember_images(&picked.images);
        self.remember_tone(&picked);
        picked
    }

    /// Remember a fetched tone for [`Self::cached_tone`].
    fn remember_tone(&self, tone: &PickedTone) {
        let mut memo = self
            .inner
            .tones
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // A plain cap rather than an LRU: the value of this memo is entirely
        // in the last few seconds of browsing, and an eviction policy would
        // be more machinery than the thing it manages.
        if memo.len() >= TONE_MEMO {
            memo.clear();
        }
        memo.insert(tone.id.clone(), tone.clone());
    }

    /// A tone already fetched this session.
    fn cached_tone(&self, tone_id: &str) -> Option<PickedTone> {
        self.inner
            .tones
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(tone_id)
            .cloned()
    }

    /// Fetch and place one model, publishing progress as it goes.
    async fn run_download(self, tone_id: String, model_id: String) {
        let mut progress = DownloadProgress {
            model_id: model_id.clone(),
            ..DownloadProgress::default()
        };
        self.inner.downloads.publish(progress.clone());

        match self.fetch_model(&tone_id, &model_id, &mut progress).await {
            Ok(()) => {}
            Err(e) => {
                tracing::warn!(tone = tone_id, model = model_id, error = %e, "tone3000: download failed");
                progress.error = e.to_string();
            }
        }
        progress.percent = if progress.error.is_empty() {
            100
        } else {
            progress.percent
        };
        progress.done = true;
        self.inner.downloads.publish(progress);
    }

    /// The download proper. Split out so the progress bookkeeping above has
    /// one place to turn an error into a published event.
    async fn fetch_model(
        &self,
        tone_id: &str,
        model_id: &str,
        progress: &mut DownloadProgress,
    ) -> Result<(), SessionError> {
        let id = model_id
            .parse::<u64>()
            .map_err(|_| SessionError::Api(format!("not a model id: {model_id}")))?;
        let client = self.client().await?;
        let model = client
            .model(api::ModelId(id))
            .await
            .map_err(|e| SessionError::Api(e.to_string()))?;

        progress.model_name.clone_from(&model.name);
        self.inner.downloads.publish(progress.clone());

        // The file is fetched with the same credential the API call used:
        // the account's linked token when there is one — a machine signed in
        // only through its account has no local session, and reading one
        // here failed every download there with "not signed in". Otherwise
        // the local token, read AFTER the call above, which is what
        // refreshes it if it was stale.
        let bearer = match self.brokered_token().await {
            Some(token) => token,
            None => self.inner.session.stored_tokens()?.access_token,
        };
        let response = self
            .inner
            .http
            .get(&model.model_url)
            .bearer_auth(&bearer)
            .send()
            .await
            .map_err(|e| SessionError::Api(e.to_string()))?;
        if !response.status().is_success() {
            return Err(SessionError::Api(format!(
                "model download returned {}",
                response.status()
            )));
        }

        let total = response.content_length();
        let mut bytes: Vec<u8> = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| SessionError::Api(e.to_string()))?;
            bytes.extend_from_slice(&chunk);
            if let Some(percent) = percent_of(bytes.len(), total) {
                if percent != progress.percent {
                    progress.percent = percent;
                    self.inner.downloads.publish(progress.clone());
                }
            }
        }

        let filename = self.filename_for(tone_id, &model, &bytes);
        let outcome = self.inner.session.place_model(tone_id, &filename, &bytes)?;
        progress.hash.clone_from(&outcome.hash);
        progress.path = outcome.path.to_string_lossy().to_string();
        tracing::info!(
            tone = tone_id,
            model = model_id,
            path = %outcome.path.display(),
            written = outcome.written,
            "tone3000: model placed"
        );

        // Attribution is recorded in the same breath as the file. A download
        // that lands without its creator and licence is one the terms do not
        // permit us to keep, and "record it later" never happens.
        //
        // Almost always a memo hit — the user is downloading from a tone they
        // are looking at — so this costs nothing. The fetch is the fallback
        // for a download driven from an id alone.
        let tone = match self.cached_tone(tone_id) {
            Some(tone) => tone,
            None => self.tone_detail(tone_id).await,
        };
        // The cover travels with the model. A downloaded capture has to stay
        // presentable with no network and no session, so the picture is a file
        // in the tone's own folder rather than a URL to re-fetch or an entry in
        // a cache that does not move with the library.
        let artwork = self.place_artwork(tone_id, &tone).await;
        self.record(&outcome.path, &outcome.hash, &tone, model_id, artwork);
        Ok(())
    }

    /// Fetch the tone's first photograph into its library folder, returning the
    /// path relative to the library root.
    ///
    /// Best-effort by design: a tone with no photographs is ordinary, and a
    /// cover that will not download is not a reason to fail a model that
    /// already landed. Either way the capture is usable; it just draws plainer.
    async fn place_artwork(&self, tone_id: &str, tone: &PickedTone) -> Option<String> {
        let url = tone.images.iter().find(|u| !u.trim().is_empty())?;
        let image = match self.cached_image(url).await {
            Ok(image) if !image.bytes.is_empty() => image,
            Ok(_) => return None,
            Err(e) => {
                tracing::debug!(tone = tone_id, %e, "tone3000: no cover placed");
                return None;
            }
        };
        let filename = format!("cover.{}", extension_for_mime(&image.mime));
        let outcome = match self
            .inner
            .session
            .place_model(tone_id, &filename, &image.bytes)
        {
            Ok(outcome) => outcome,
            Err(e) => {
                tracing::debug!(tone = tone_id, %e, "tone3000: cover could not be written");
                return None;
            }
        };
        outcome
            .path
            .strip_prefix(&self.inner.cfg.library_root)
            .ok()
            .map(|rel| rel.to_string_lossy().to_string())
    }

    /// The name a downloaded model should take in the library.
    ///
    /// The creator's name for the model, not the catalog's storage name. The
    /// URL's last segment is an opaque id — a real download lands as
    /// `ix2qbhsyea.nam` — and a library of those is unusable anywhere except
    /// inside this app: a file dialog, a backup, a folder someone opens in
    /// five years. The extension still comes from the URL, which is the one
    /// thing it is reliably good for.
    ///
    /// Two models in one tone may share a name, and the second must not
    /// silently overwrite the first — the catalog indexes by content hash, so
    /// an overwritten file leaves an entry pointing at somebody else's bytes.
    /// A clash therefore takes the model id as a disambiguator; identical
    /// bytes at the same name are not a clash, they are the same file.
    fn filename_for(&self, tone_id: &str, model: &api::Model, bytes: &[u8]) -> String {
        let ext = extension_of(&model.model_url);
        let stem = if model.name.trim().is_empty() {
            format!("model-{}", model.id)
        } else {
            model.name.trim().to_string()
        };
        let preferred = format!("{stem}.{ext}");

        let path = self.inner.session.destination(tone_id, &preferred);
        let occupied_by_something_else =
            std::fs::read(&path).is_ok_and(|existing| digest(&existing) != digest(bytes));
        if occupied_by_something_else {
            format!("{stem} ({}).{ext}", model.id)
        } else {
            preferred
        }
    }

    /// Index the placed file in the NAM catalog, provenance and all.
    fn record(
        &self,
        path: &std::path::Path,
        hash: &str,
        tone: &PickedTone,
        model_id: &str,
        artwork_path: Option<String>,
    ) {
        let root = &self.inner.cfg.library_root;
        let entry = match signal_nam::scan_one(path, root) {
            Ok(Some(mut entry)) => {
                entry.provenance = Some(signal_nam::nam_file::Provenance {
                    source: "tone3000".to_string(),
                    tone_id: non_empty(&tone.id),
                    model_id: Some(model_id.to_string()),
                    tone_url: non_empty(&tone.tone_url),
                    creator: non_empty(&tone.creator),
                    creator_url: non_empty(&tone.creator_url),
                    license: non_empty(&tone.license),
                    tone_name: non_empty(&tone.name),
                    description: non_empty(&tone.description),
                    gear: non_empty(&tone.gear),
                    makes: tone.makes.clone(),
                    tags: tone.tags.clone(),
                    artwork_path,
                });
                entry
            }
            Ok(None) => {
                tracing::warn!(path = %path.display(), "tone3000: placed file is not a library kind");
                return;
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), %e, "tone3000: could not index placed file");
                return;
            }
        };

        let catalog_path = &self.inner.cfg.catalog_path;
        let mut catalog = signal_nam::NamCatalog::load(catalog_path).unwrap_or_default();
        signal_nam::merge_into_catalog(&mut catalog, HashMap::from([(hash.to_string(), entry)]));
        if let Err(e) = catalog.save(catalog_path) {
            tracing::warn!(path = %catalog_path.display(), %e, "tone3000: catalog save failed");
        }
    }

    /// Serve one image from the cache, fetching it once if it is not there.
    async fn cached_image(&self, url: &str) -> Result<ImageData, SessionError> {
        let known = self
            .inner
            .known_images
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(url);
        if !known {
            return Err(SessionError::Api(
                "image was not one this engine was given".to_string(),
            ));
        }

        let path = self.inner.cfg.image_cache.join(digest(url.as_bytes()));
        if let Ok(bytes) = std::fs::read(&path) {
            let mime = mime_of(&bytes).to_string();
            return Ok(ImageData {
                bytes,
                mime,
                error: String::new(),
            });
        }

        let response = self
            .inner
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| SessionError::Api(e.to_string()))?;
        if !response.status().is_success() {
            return Err(SessionError::Api(format!(
                "image fetch returned {}",
                response.status()
            )));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_IMAGE_BYTES)
        {
            return Err(SessionError::Api("image is implausibly large".to_string()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| SessionError::Api(e.to_string()))?
            .to_vec();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Best-effort: a picture that cannot be cached is still a picture we
        // can show now.
        if let Err(e) = std::fs::write(&path, &bytes) {
            tracing::debug!(%e, "tone3000: image not cached");
        }
        let mime = mime_of(&bytes).to_string();
        Ok(ImageData {
            bytes,
            mime,
            error: String::new(),
        })
    }
}

impl Tone3000 for Tone3000Backend {
    async fn status(&self) -> SignInStatus {
        // A local session wins: it is this engine's own, it needs no
        // network to confirm, and someone who signed in here directly
        // expects to see that rather than an account they also happen to
        // have linked.
        match self.inner.session.token_store().load() {
            Ok(Some(tokens)) => {
                return SignInStatus {
                    signed_in: true,
                    username: tokens.username,
                    error: String::new(),
                    via: "tone3000".to_string(),
                };
            }
            Ok(None) => {}
            Err(e) => {
                return SignInStatus {
                    error: e.to_string(),
                    ..SignInStatus::default()
                };
            }
        }

        // Otherwise the account, if it has TONE3000 linked.
        if let Some(account) = self.inner.account.as_ref() {
            if let Ok(linked) = account
                .linked_token(crate::account::TONE3000_PROVIDER)
                .await
            {
                return SignInStatus {
                    signed_in: true,
                    username: linked.login.unwrap_or_default(),
                    error: String::new(),
                    via: "account".to_string(),
                };
            }
        }
        SignInStatus::default()
    }

    fn begin_sign_in(&self, prompt_select_tone: bool) -> AuthRequest {
        if !self.inner.cfg.is_configured() {
            // An empty URL is the only way to say "cannot start" on this
            // return type; a GUI that opens it gets nothing, so it must check.
            tracing::warn!("tone3000: no publishable key — sign-in cannot start");
            return AuthRequest::default();
        }
        let pkce = api::generate_pkce();
        // The `state` nonce and the request id are both 32 bytes of OS
        // randomness, which is precisely what the PKCE generator makes. Reused
        // rather than pulling in a second RNG for the same job.
        let state = api::generate_pkce().verifier;
        let request_id = api::generate_pkce().verifier;

        let prompt = if prompt_select_tone {
            api::Prompt::SelectTone
        } else {
            api::Prompt::Standard
        };
        let url = api::authorize_url(
            &self.inner.cfg.publishable_key,
            &self.inner.cfg.redirect_uri,
            &pkce.challenge,
            &state,
            prompt,
            api::AuthorizeOptions {
                // Let the user audition a tone inside the picker before
                // committing to a download.
                preview: true,
                ..api::AuthorizeOptions::default()
            },
        );

        let start = AuthStart::new(url.to_string(), pkce.verifier, state);
        self.inner
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(request_id.clone(), start.clone());

        AuthRequest {
            authorize_url: start.authorize_url,
            request_id,
        }
    }

    async fn complete_sign_in(&self, request_id: String, callback_url: String) -> SignInStatus {
        let start = self
            .inner
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&request_id);
        let Some(start) = start else {
            return failed_sign_in("no authorization is pending for this request");
        };

        match self.redeem(&start, &request_id, &callback_url).await {
            Ok(status) => status,
            Err(e) => {
                tracing::warn!(error = %e, "tone3000: sign-in failed");
                failed_sign_in(e)
            }
        }
    }

    fn sign_out(&self) {
        if let Err(e) = self.inner.session.sign_out() {
            tracing::warn!(%e, "tone3000: sign-out could not clear the session");
        }
        self.inner
            .picked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        // The cached client still holds the tokens in memory; drop it, or the
        // next call would keep using a session the user just ended.
        let this = self.clone();
        architect::platform::spawn(async move { this.invalidate_client().await });
    }

    async fn picked_tone(&self, request_id: String) -> PickedTone {
        let tone_id = self
            .inner
            .picked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&request_id)
            .cloned();
        match tone_id {
            Some(id) => self.tone_detail(&id).await,
            None => PickedTone::default(),
        }
    }

    async fn search(&self, query: ToneQuery) -> TonePage {
        let client = match self.client().await {
            Ok(c) => c,
            Err(e) => return map::failed_page(e),
        };

        let mut search = client.tones();
        if !query.text.is_empty() {
            search = search.query(&query.text);
        }
        for gear in query.gears.iter().filter_map(|g| parse_enum(g)) {
            search = search.gear(gear);
        }
        if let Some(format) = parse_enum(&query.format) {
            search = search.format(format);
        }
        if let Some(arch) = parse_enum(&query.architecture) {
            search = search.architecture(arch);
        }
        search = search
            .sort(sort_of(&query.sort, query.text.is_empty()))
            .page(query.page.max(1))
            .page_size(page_size_of(query.page_size));

        match search.await {
            Ok(page) => {
                let mapped = map::page(&page);
                self.remember_images(mapped.tones.iter().map(|t| &t.image));
                mapped
            }
            Err(e) => map::failed_page(e),
        }
    }

    async fn shelf(&self, shelf: ToneShelf, page: u32) -> TonePage {
        let client = match self.client().await {
            Ok(c) => c,
            Err(e) => return map::failed_page(e),
        };
        let page_no = page.max(1);

        // Trending and latest have dedicated endpoints upstream that the
        // client crate does not expose yet, so they are served here as the
        // same content through the search endpoint. That means they cost
        // search quota, which is exactly what a shelf is supposed to avoid —
        // the fix belongs upstream (`tone3000` crate), not in a second HTTP
        // path here.
        let result = match shelf {
            ToneShelf::Trending => {
                client
                    .tones()
                    .sort(api::ToneSort::Trending)
                    .page_size(SHELF_SIZE)
                    .await
            }
            ToneShelf::Latest => {
                client
                    .tones()
                    .sort(api::ToneSort::Newest)
                    .page_size(SHELF_SIZE)
                    .await
            }
            ToneShelf::Favorited => client.favorited().page(page_no).await,
            ToneShelf::Created => client.created().page(page_no).await,
        };

        match result {
            Ok(page) => {
                let mapped = map::page(&page);
                self.remember_images(mapped.tones.iter().map(|t| &t.image));
                mapped
            }
            Err(e) => map::failed_page(e),
        }
    }

    async fn tone(&self, tone_id: String) -> PickedTone {
        self.tone_detail(&tone_id).await
    }

    async fn image(&self, url: String) -> ImageData {
        match self.cached_image(&url).await {
            Ok(image) => image,
            Err(e) => ImageData {
                error: e.to_string(),
                ..ImageData::default()
            },
        }
    }

    fn download_model(&self, tone_id: String, model_id: String) {
        // Returns as soon as the work is queued: a slow transfer must not
        // hold a vox call open, and progress has its own stream.
        let this = self.clone();
        architect::platform::spawn(async move { this.run_download(tone_id, model_id).await });
    }
}

impl Tone3000Backend {
    /// Redeem a callback that arrived without a request id.
    ///
    /// The browser is redirected to a fixed URI and knows nothing about the
    /// request that started the flow, so the engine's HTTP route cannot name
    /// one. The `state` nonce identifies it instead — which is the job it
    /// already has, since it is the value that must match for the callback to
    /// be trusted at all.
    ///
    /// Returns the status and the request id it belonged to, so a caller can
    /// still ask [`Tone3000::picked_tone`] what the user chose.
    pub async fn complete_from_callback(&self, callback_url: &str) -> (SignInStatus, String) {
        let Ok(url) = url::Url::parse(callback_url) else {
            return (failed_sign_in("callback is not a URL"), String::new());
        };
        let echoed = url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default();

        let found = {
            let pending = self
                .inner
                .pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            pending
                .iter()
                .find(|(_, start)| start.verify_state(&echoed).is_ok())
                .map(|(id, _)| id.clone())
        };
        let Some(request_id) = found else {
            return (
                failed_sign_in("no authorization is pending for this callback"),
                String::new(),
            );
        };

        let status = self
            .complete_sign_in(request_id.clone(), callback_url.to_string())
            .await;
        (status, request_id)
    }

    /// Redeem a callback: check `state`, exchange the code, persist the
    /// session, and remember any tone the picker came back with.
    async fn redeem(
        &self,
        start: &AuthStart,
        request_id: &str,
        callback_url: &str,
    ) -> Result<SignInStatus, SessionError> {
        let url = url::Url::parse(callback_url)
            .map_err(|e| SessionError::Api(format!("callback is not a URL: {e}")))?;
        let params: HashMap<String, String> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        if let Some(error) = params.get("error") {
            let description = params
                .get("error_description")
                .map_or_else(String::new, |d| format!(": {d}"));
            return Err(SessionError::Api(format!("{error}{description}")));
        }

        start.verify_state(params.get("state").map_or("", String::as_str))?;

        // Closing TONE3000's own picker comes back as `canceled=true` with no
        // code. That is a person changing their mind, not a failure, and it
        // must not be reported as one — an error banner for "I clicked the X"
        // teaches people that the feature is broken.
        if params.get("canceled").is_some_and(|v| v == "true") {
            tracing::info!("tone3000: the user closed the picker");
            return Err(SessionError::Canceled);
        }

        let code = params.get("code").ok_or(SessionError::MissingCode)?;

        let client = self.anonymous();
        let tokens = client
            .exchange_code(code, start.verifier(), &self.inner.cfg.redirect_uri)
            .await
            .map_err(|e| SessionError::Api(e.to_string()))?;

        // Who signed in — asked once, here, and cached with the session so
        // that `status` never needs the network.
        let username = match client.user().await {
            Ok(user) => user.username,
            Err(e) => {
                tracing::debug!(%e, "tone3000: signed in but could not read the account name");
                String::new()
            }
        };
        persist(self.inner.session.token_store(), &tokens, &username);
        self.invalidate_client().await;

        // A `select_tone` flow comes back with the tone the user picked.
        if let Some(tone_id) = params.get("tone_id") {
            self.inner
                .picked
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(request_id.to_string(), tone_id.clone());
        }

        tracing::info!(user = username, "tone3000: signed in");
        Ok(SignInStatus {
            signed_in: true,
            username,
            error: String::new(),
            via: "tone3000".to_string(),
        })
    }
}

impl Tone3000StreamSource for Tone3000Backend {
    fn downloads_hub(&self) -> &PubSub<DownloadProgress> {
        &self.inner.downloads
    }
}

impl Services for Tone3000Backend {
    fn layers() -> impl Layer<Self> {
        layers![crate::tone3000::Service, crate::tone3000::StreamService]
    }
}

/// Write a token set through to disk, keeping the account name.
///
/// The refresh token is only replaced when the response carried one — the
/// token endpoint omits it on some responses, and treating "absent" as "empty"
/// would end the session at the next refresh.
fn persist(store: &TokenStore, tokens: &api::Tokens, username: &str) {
    let previous = store.load().ok().flatten();
    let refresh_token = tokens
        .refresh_token
        .clone()
        .or_else(|| previous.as_ref().map(|p| p.refresh_token.clone()))
        .unwrap_or_default();
    let expires_at = now_unix().saturating_add(
        tokens
            .expires_in
            .and_then(|s| i64::try_from(s).ok())
            .unwrap_or(0),
    );
    let username = if username.is_empty() {
        previous.map(|p| p.username).unwrap_or_default()
    } else {
        username.to_string()
    };

    if let Err(e) = store.save(&Tokens {
        access_token: tokens.access_token.clone(),
        refresh_token,
        expires_at,
        username,
    }) {
        tracing::error!(%e, "tone3000: could not persist the session");
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

fn failed_sign_in(error: impl std::fmt::Display) -> SignInStatus {
    SignInStatus {
        error: error.to_string(),
        ..SignInStatus::default()
    }
}

fn non_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// Parse a wire string into one of the catalog's open enums.
///
/// Goes through the enum's own `Deserialize` so an unrecognised value lands in
/// its `Other` arm and is sent on verbatim, rather than being dropped here for
/// not matching a list this build happens to know.
fn parse_enum<T: serde::de::DeserializeOwned>(s: &str) -> Option<T> {
    if s.is_empty() {
        return None;
    }
    serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
}

/// The sort a query asked for. `best-match` is meaningless without search
/// text, so a browse with no explicit sort gets the steady one instead.
fn sort_of(requested: &str, browsing: bool) -> api::ToneSort {
    match requested {
        "newest" => api::ToneSort::Newest,
        "oldest" => api::ToneSort::Oldest,
        "trending" => api::ToneSort::Trending,
        "downloads-all-time" => api::ToneSort::DownloadsAllTime,
        "best-match" => api::ToneSort::BestMatch,
        _ if browsing => api::ToneSort::DownloadsAllTime,
        _ => api::ToneSort::BestMatch,
    }
}

/// The page size to ask for: the caller's, defaulted when unset and capped at
/// what the endpoint accepts. Asking for more than the cap is an error there,
/// and asking for zero is how a caller says "you choose".
fn page_size_of(requested: u32) -> u32 {
    if requested == 0 {
        DEFAULT_PAGE_SIZE
    } else {
        requested.clamp(1, MAX_PAGE_SIZE)
    }
}

/// Whole-percent progress, or `None` when the server did not say how big the
/// file is — which is common enough that a UI must handle it.
fn percent_of(done: usize, total: Option<u64>) -> Option<u32> {
    let total = total.filter(|t| *t > 0)?;
    let done = u64::try_from(done).ok()?;
    u32::try_from(done.saturating_mul(100).checked_div(total)?.min(100)).ok()
}

/// The extension the catalog serves a model under.
///
/// Taken from the URL because it is the one thing the URL is reliably good
/// for: `.nam` for a capture, `.wav` for an impulse response.
fn extension_of(model_url: &str) -> String {
    url::Url::parse(model_url)
        .ok()
        .and_then(|u| {
            let last = u.path_segments()?.next_back()?.to_string();
            let ext = last.rsplit_once('.')?.1.to_string();
            (!ext.is_empty() && ext.len() <= 8).then_some(ext)
        })
        .unwrap_or_else(|| "nam".to_string())
}

/// The media type of an encoded image, read from its own first bytes.
///
/// Sniffed rather than taken from the `Content-Type` header (or a sidecar
/// file) so the cache is a directory of plain image files with nothing to keep
/// in sync, and a cache entry written by an older build still types correctly.
/// File extension for a media type, so a placed cover is openable by anything
/// that opens pictures — a file manager, a backup, a person in five years.
/// Paired with [`mime_of`], which is what produced the type.
const fn extension_for_mime(mime: &str) -> &'static str {
    match mime.as_bytes() {
        b"image/png" => "png",
        b"image/gif" => "gif",
        b"image/webp" => "webp",
        // JPEG for anything unrecognised: the catalog serves it overwhelmingly,
        // and a wrong extension on a readable file beats no file.
        _ => "jpg",
    }
}

fn mime_of(bytes: &[u8]) -> &'static str {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'G', b'I', b'F', ..] => "image/gif",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "image/webp",
        _ => "application/octet-stream",
    }
}

/// Hex SHA-256 — the cache key for an image URL.
fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
