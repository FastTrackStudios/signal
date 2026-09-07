//! Signing in to the issuer, and keeping the session.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use auth_client::oidc::{self, Pkce};
use auth_client::{FileTokenStore, StoredSession, TokenStore as _};
use rand::RngCore as _;

/// Where the account lives and who we are to it.
#[derive(Debug, Clone)]
pub struct AccountConfig {
    /// `https://auth.fasttrackstudio.app`.
    pub issuer: String,
    /// The OIDC client id — the app's leftmost domain label, `signal`.
    pub client_id: String,
    /// Must be registered on that client, and must be a route the engine
    /// actually serves.
    pub redirect_uri: String,
    /// The stored session, `0600`.
    pub session_path: PathBuf,
}

/// The issuer this product signs in to.
pub const DEFAULT_ISSUER: &str = "https://auth.fasttrackstudio.app";
/// This app's OIDC client id, as registered on that issuer.
pub const DEFAULT_CLIENT_ID: &str = "signal";
/// The engine's own callback route.
pub const CALLBACK_PATH: &str = "/account/callback";

/// What is asked for at sign-in.
///
/// `offline_access` is what makes the session last a week instead of an
/// hour, and `tone3000` is what authorizes reading the account's linked
/// TONE3000 token. A scope the client is not registered for is a 403 at
/// `/oauth2/authorize` before any login page is drawn, so this list and the
/// issuer's registration for `signal` have to agree.
pub const SCOPE: &str = "openid email profile offline_access tone3000";

impl AccountConfig {
    /// Read the environment, falling back to the deployed issuer.
    #[must_use]
    pub fn from_env(config_dir: &std::path::Path) -> Self {
        let issuer = std::env::var("SIGNAL_AUTH_ISSUER")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_ISSUER.to_string());
        let redirect_uri = std::env::var("SIGNAL_AUTH_REDIRECT_URI")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("http://localhost:4040{CALLBACK_PATH}"));
        Self {
            issuer,
            client_id: std::env::var("SIGNAL_AUTH_CLIENT_ID")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string()),
            redirect_uri,
            session_path: config_dir.join("account/session.json"),
        }
    }
}

/// Whether the engine holds a `FastTrackStudio` session, and whose.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountStatus {
    pub signed_in: bool,
    /// Display only.
    pub email: String,
    /// The issuer's user id — the principal everything else is keyed on.
    pub user_id: String,
    /// Why a sign-in did not happen, when one was attempted.
    pub error: String,
}

/// An authorization in flight. The GUI is handed the URL and nothing else.
#[derive(Debug, Clone)]
pub struct AuthStart {
    pub authorize_url: String,
    pub request_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("not signed in to FastTrackStudio")]
    NotSignedIn,
    #[error("no authorization is pending for this request")]
    NoPendingRequest,
    /// A mismatched `state` is what a CSRF attempt looks like, and is
    /// indistinguishable from one, so it is never retried.
    #[error("authorization state did not match the request that started it")]
    StateMismatch,
    #[error("the authorization callback carried no code")]
    MissingCode,
    #[error("the issuer refused: {0}")]
    Issuer(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The engine's `FastTrackStudio` session.
pub struct Account {
    cfg: AccountConfig,
    store: FileTokenStore,
    /// Authorizations handed out and not yet redeemed, by request id. The
    /// verifier stays here rather than travelling with the GUI.
    pending: Mutex<HashMap<String, Pkce>>,
    http: reqwest::Client,
}

impl Account {
    #[must_use]
    pub fn new(cfg: AccountConfig) -> Self {
        let store = FileTokenStore::new(cfg.session_path.clone());
        Self {
            cfg,
            store,
            pending: Mutex::new(HashMap::new()),
            http: reqwest::Client::new(),
        }
    }

    #[must_use]
    pub const fn config(&self) -> &AccountConfig {
        &self.cfg
    }

    /// Whether a session is stored, and whose. Answered from disk — a
    /// status call must not need the network, or the UI goes blank
    /// whenever the issuer is briefly unreachable.
    #[must_use]
    pub fn status(&self) -> AccountStatus {
        match self.store.load() {
            Ok(Some(session)) => AccountStatus {
                signed_in: true,
                email: session.email.unwrap_or_default(),
                user_id: session.user_id.unwrap_or_default(),
                error: String::new(),
            },
            Ok(None) => AccountStatus::default(),
            Err(e) => AccountStatus {
                error: e.to_string(),
                ..AccountStatus::default()
            },
        }
    }

    /// The stored access token, if any.
    ///
    /// # Errors
    ///
    /// [`AccountError::NotSignedIn`] when there is no session.
    pub fn access_token(&self) -> Result<String, AccountError> {
        self.store
            .load()
            .ok()
            .flatten()
            .map(|s| s.token)
            .ok_or(AccountError::NotSignedIn)
    }

    /// Mint PKCE and build the authorize URL for a GUI to open.
    #[must_use]
    pub fn begin_sign_in(&self) -> AuthStart {
        // The entropy is supplied rather than generated inside the shared
        // helper, deliberately: the right source is platform-specific, and
        // making it an argument means an app cannot reach for a weak one
        // without noticing.
        let mut verifier = [0u8; 32];
        let mut state = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut verifier);
        rand::thread_rng().fill_bytes(&mut state);
        let pkce = Pkce::from_entropy(verifier, state);

        let url = oidc::authorize_url(
            &self.cfg.issuer,
            &self.cfg.client_id,
            &self.cfg.redirect_uri,
            &pkce,
            SCOPE,
        );

        // The request id is the state: it is already unguessable, already
        // unique to this attempt, and already what the callback carries
        // back — a second identifier would only be a second thing to keep
        // in step.
        let request_id = pkce.state().to_string();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(request_id.clone(), pkce);

        AuthStart {
            authorize_url: url,
            request_id,
        }
    }

    /// Redeem a callback. Finds its flow by the `state` it echoes, so the
    /// engine's HTTP route needs no correlation of its own.
    ///
    /// # Errors
    ///
    /// See [`AccountError`]; every arm leaves the stored session untouched.
    pub async fn complete_sign_in(
        &self,
        callback_url: &str,
    ) -> Result<AccountStatus, AccountError> {
        let url = url::Url::parse(callback_url)
            .map_err(|e| AccountError::Issuer(format!("callback is not a URL: {e}")))?;
        let params: HashMap<String, String> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        if let Some(error) = params.get("error") {
            let detail = params
                .get("error_description")
                .map_or_else(String::new, |d| format!(": {d}"));
            return Err(AccountError::Issuer(format!("{error}{detail}")));
        }

        let state = params.get("state").ok_or(AccountError::StateMismatch)?;
        let pkce = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(state)
            .ok_or(AccountError::NoPendingRequest)?;
        let code = params.get("code").ok_or(AccountError::MissingCode)?;

        let body =
            oidc::token_request_body(&self.cfg.client_id, &self.cfg.redirect_uri, code, &pkce);
        let response = self
            .http
            .post(format!(
                "{}/oauth2/token",
                self.cfg.issuer.trim_end_matches('/')
            ))
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await
            .map_err(|e| AccountError::Issuer(e.to_string()))?;
        let text = response
            .text()
            .await
            .map_err(|e| AccountError::Issuer(e.to_string()))?;
        let token =
            oidc::access_token_from(&text).map_err(|e| AccountError::Issuer(e.to_string()))?;

        // Who signed in, asked once and cached, so `status` never needs the
        // network. A userinfo failure does not fail the sign-in: the token
        // is good, and a missing display name is not a broken session.
        let user = self.fetch_user(&token).await;
        let mut session = StoredSession::new(token);
        if let Some(user) = user {
            session = session.with_user_id(user.sub);
            if let Some(email) = user.email {
                session = session.with_email(email);
            }
        }
        self.store
            .save(&session)
            .map_err(|e| AccountError::Issuer(e.to_string()))?;

        tracing::info!(
            user = session.user_id,
            "account: signed in to FastTrackStudio"
        );
        Ok(self.status())
    }

    /// Forget the session.
    ///
    /// # Errors
    ///
    /// Returns the store's error if the file cannot be removed.
    pub fn sign_out(&self) -> Result<(), AccountError> {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.store
            .clear()
            .map_err(|e| AccountError::Issuer(e.to_string()))
    }

    async fn fetch_user(&self, token: &str) -> Option<oidc::UserInfo> {
        let response = self
            .http
            .get(format!(
                "{}/oauth2/userinfo",
                self.cfg.issuer.trim_end_matches('/')
            ))
            .bearer_auth(token)
            .send()
            .await
            .ok()?;
        let text = response.text().await.ok()?;
        oidc::user_from(&text).ok()
    }

    /// The HTTP client and issuer, for [`crate::linked`].
    pub(crate) const fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub(crate) fn issuer(&self) -> &str {
        self.cfg.issuer.trim_end_matches('/')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(dir: &std::path::Path) -> Account {
        Account::new(AccountConfig {
            issuer: "https://auth.example.com".into(),
            client_id: "signal".into(),
            redirect_uri: "http://localhost:4040/account/callback".into(),
            session_path: dir.join("session.json"),
        })
    }

    #[test]
    fn a_fresh_engine_is_signed_out_without_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let status = account(dir.path()).status();
        assert!(!status.signed_in);
        assert!(
            status.error.is_empty(),
            "never having signed in is not a failure"
        );
    }

    #[test]
    fn the_authorize_url_asks_for_the_scopes_the_brokered_token_needs() {
        let dir = tempfile::tempdir().unwrap();
        let start = account(dir.path()).begin_sign_in();
        let url = url::Url::parse(&start.authorize_url).unwrap();
        let q: HashMap<_, _> = url.query_pairs().into_owned().collect();

        assert_eq!(url.path(), "/oauth2/authorize");
        assert_eq!(q.get("client_id").map(String::as_str), Some("signal"));
        assert_eq!(
            q.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        let scope = q.get("scope").expect("a scope");
        // Without `tone3000` the issuer will not hand over the linked
        // token; without `offline_access` the session lasts an hour.
        assert!(scope.contains("tone3000"), "{scope}");
        assert!(scope.contains("offline_access"), "{scope}");
        assert!(
            !q.contains_key("code_verifier"),
            "the verifier never travels in a URL"
        );
        assert_eq!(
            q.get("state").map(String::as_str),
            Some(start.request_id.as_str())
        );
    }

    /// Two sign-ins must not share a challenge, or one callback could
    /// redeem the other's code.
    #[test]
    fn every_sign_in_gets_its_own_challenge() {
        let dir = tempfile::tempdir().unwrap();
        let account = account(dir.path());
        let (a, b) = (account.begin_sign_in(), account.begin_sign_in());
        assert_ne!(a.request_id, b.request_id);
        assert_ne!(a.authorize_url, b.authorize_url);
    }

    #[tokio::test]
    async fn a_callback_for_no_pending_request_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let err = account(dir.path())
            .complete_sign_in("http://localhost:4040/account/callback?code=c&state=never-issued")
            .await
            .unwrap_err();
        assert!(matches!(err, AccountError::NoPendingRequest), "{err}");
    }

    #[tokio::test]
    async fn the_issuers_own_refusal_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let account = account(dir.path());
        let start = account.begin_sign_in();
        let err = account
            .complete_sign_in(&format!(
                "http://localhost:4040/account/callback?error=access_denied&error_description=nope&state={}",
                start.request_id
            ))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("access_denied"), "{err}");
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[tokio::test]
    async fn a_pending_request_is_good_once() {
        let dir = tempfile::tempdir().unwrap();
        let account = account(dir.path());
        let start = account.begin_sign_in();
        let url = format!(
            "http://localhost:4040/account/callback?state={}",
            start.request_id
        );
        // No code, so it fails — but it has consumed the pending flow.
        let first = account.complete_sign_in(&url).await.unwrap_err();
        let second = account.complete_sign_in(&url).await.unwrap_err();
        assert!(matches!(first, AccountError::MissingCode), "{first}");
        assert!(matches!(second, AccountError::NoPendingRequest), "{second}");
    }

    #[test]
    fn access_token_needs_a_session() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            account(dir.path()).access_token(),
            Err(AccountError::NotSignedIn)
        ));
    }
}
